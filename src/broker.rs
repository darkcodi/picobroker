//! MQTT broker main event loop
//!
//! Orchestrates client connections, message routing, and keep-alive tracking

use crate::client::{Action, Client, ClientState};
use crate::error::{Error, Result};
use crate::packet::encode_publish_qos0;
use crate::topics::TopicRegistry;
use embassy_net::tcp::TcpSocket;
use embassy_time::{Duration, Instant};
use heapless::Vec;

// Import Write trait for write_all method
use embedded_io_async::Write;

/// Main MQTT broker instance
///
/// Generic over:
/// - 'a: Lifetime
/// - MAX_CLIENTS: Maximum concurrent clients
pub struct Broker<'a, const MAX_CLIENTS: usize> {
    pub clients: Vec<Client<'a>, MAX_CLIENTS>,
    pub topic_registry: TopicRegistry<MAX_CLIENTS>,
    last_activity: Vec<Option<Instant>, MAX_CLIENTS>,
}

impl<'a, const MAX_CLIENTS: usize> Broker<'a, MAX_CLIENTS> {
    pub fn new() -> Self {
        Self {
            clients: Vec::new(),
            topic_registry: TopicRegistry::new(),
            last_activity: Vec::new(),
        }
    }

    /// Find a free client slot
    fn find_free_slot(&self) -> Option<usize> {
        self.clients
            .iter()
            .position(|c| c.state == ClientState::Disconnected)
    }

    /// Add a new client connection
    pub fn add_client(&mut self, socket: TcpSocket<'a>) -> Result<usize> {
        // Try to find a disconnected slot to reuse first
        if let Some(slot) = self.find_free_slot() {
            self.clients[slot] = Client::new(socket);

            defmt::info!("Client added at slot {} (reused)", slot);

            return Ok(slot);
        }

        // No free slots, try to add at the end
        if self.clients.len() < MAX_CLIENTS {
            let idx = self.clients.len();
            let _ = self.clients.push(Client::new(socket));
            let _ = self.last_activity.push(None);

            defmt::info!("Client added at slot {} (new)", idx);

            return Ok(idx);
        }

        // At maximum capacity
        Err(Error::MaxClientsReached)
    }

    /// Remove a client and clean up subscriptions
    pub fn remove_client(&mut self, idx: usize) {
        if idx >= self.clients.len() {
            return;
        }

        defmt::info!("Removing client at slot {}", idx);

        // Unregister from all topics
        self.topic_registry.unregister_all(idx);

        // Mark as disconnected
        self.clients[idx].state = ClientState::Disconnected;
        self.last_activity[idx] = None;
    }

    /// Update last activity time for a client
    pub fn update_activity(&mut self, idx: usize) {
        if idx < self.last_activity.len() {
            self.last_activity[idx] = Some(Instant::now());
        }
    }

    /// Check keep-alive timeouts and disconnect stale clients
    pub async fn check_timeouts(&mut self) {
        let now = Instant::now();

        for i in 0..self.clients.len() {
            if let ClientState::Connected = self.clients[i].state {
                if let Some(last_activity) = self.last_activity[i] {
                    let timeout = Duration::from_secs(self.clients[i].keep_alive as u64 * 2);

                    if now.duration_since(last_activity) > timeout {
                        defmt::warn!(
                            "Client {} keep-alive timeout (last seen: {}s ago)",
                            i,
                            now.duration_since(last_activity).as_secs()
                        );

                        self.remove_client(i);
                        let _ = self.clients[i].close().await;
                    }
                }
            }
        }
    }

    /// Dispatch a published message to all subscribers
    pub async fn dispatch_publish(&mut self, from_idx: usize, topic: &str, payload: &[u8]) {
        let subscribers = self.topic_registry.get_subscribers(topic);

        defmt::debug!(
            "Dispatching to topic {} ({} subscribers)",
            topic,
            subscribers.len()
        );

        // Collect indices of clients that failed to send
        let mut failed_clients = heapless::Vec::<usize, MAX_CLIENTS>::new();

        for &sub_idx in subscribers.iter() {
            if sub_idx != from_idx {
                // Don't echo back to publisher
                if sub_idx < self.clients.len() {
                    let client = &mut self.clients[sub_idx];
                    if client.is_connected() {
                        let packet = encode_publish_qos0(topic, payload);

                        if let Err(e) = client.socket.write_all(&packet).await {
                            defmt::error!("Failed to publish to client {}: {:?}", sub_idx, e);

                            // Mark for removal
                            let _ = failed_clients.push(sub_idx);
                        }
                    }
                }
            }
        }

        // Remove failed clients
        for idx in failed_clients.iter() {
            self.remove_client(*idx);
            if *idx < self.clients.len() {
                let _ = self.clients[*idx].close().await;
            }
        }
    }

    /// Poll a single client for activity
    pub async fn poll_client(&mut self, idx: usize, read_buf: &mut [u8]) {
        if idx >= self.clients.len() {
            return;
        }

        if !self.clients[idx].is_connected() {
            return;
        }

        match self.clients[idx].read_packet(read_buf).await {
            Ok(Some(packet)) => {
                // Update activity timestamp
                self.update_activity(idx);

                // Handle the packet
                match self.clients[idx].handle_packet(packet) {
                    Ok(Action::None) => {}

                    Ok(Action::SendConnack) => {
                        if let Err(e) = self.clients[idx].send_connack().await {
                            defmt::error!("Failed to send CONNACK: {:?}", e);
                            self.remove_client(idx);
                            let _ = self.clients[idx].close().await;
                        }
                    }

                    Ok(Action::SendSuback { packet_id, count }) => {
                        if let Err(e) = self.clients[idx].send_suback(packet_id, count).await {
                            defmt::error!("Failed to send SUBACK: {:?}", e);
                            self.remove_client(idx);
                            let _ = self.clients[idx].close().await;
                        }
                    }

                    Ok(Action::PublishToTopic { topic, payload }) => {
                        // Handle subscriptions
                        for sub in self.clients[idx].subscriptions.iter() {
                            if let Err(e) = self
                                .topic_registry
                                .subscribe(sub.topic.as_str(), idx)
                            {
                                defmt::error!("Failed to subscribe to {}: {:?}", sub.topic, e);
                            }
                        }

                        // Dispatch to subscribers
                        self.dispatch_publish(idx, topic, payload).await;
                    }

                    Ok(Action::Close) => {
                        self.remove_client(idx);
                        let _ = self.clients[idx].close().await;
                    }

                    Err(e) => {
                        defmt::error!("Error handling packet: {:?}", e);
                        self.remove_client(idx);
                        let _ = self.clients[idx].close().await;
                    }
                }
            }

            Ok(None) => {
                // No data available, continue
            }

            Err(Error::Io) => {
                // Connection error
                defmt::debug!("Client {} connection error", idx);
                self.remove_client(idx);
                let _ = self.clients[idx].close().await;
            }

            Err(e) => {
                defmt::error!("Error reading from client {}: {:?}", idx, e);
                self.remove_client(idx);
                let _ = self.clients[idx].close().await;
            }
        }
    }

    /// Get the number of connected clients
    pub fn connected_count(&self) -> usize {
        self.clients
            .iter()
            .filter(|c| c.state == ClientState::Connected)
            .count()
    }
}

impl<'a, const MAX_CLIENTS: usize> Default for Broker<'a, MAX_CLIENTS> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_broker_creation() {
        let broker: Broker<4> = Broker::new();
        assert_eq!(broker.connected_count(), 0);
    }

    #[test]
    fn test_topic_registry() {
        let mut broker: Broker<4> = Broker::new();

        broker.topic_registry.subscribe("test/topic", 0).unwrap();
        broker.topic_registry.subscribe("test/topic", 1).unwrap();

        let subs = broker.topic_registry.get_subscribers("test/topic");
        assert_eq!(subs.len(), 2);
    }
}
