//! MQTT broker implementation
//!
//! Manages client connections, message routing, and keep-alive monitoring

use crate::client::{ClientName, ClientRegistry};
use crate::error::Result;
use crate::network::TcpStream;
use crate::protocol::packets::Packet;
use crate::protocol::packets::*;
use crate::protocol::qos::QoS;
use crate::topics::{TopicName, TopicRegistry, TopicSubscription};

/// MQTT broker
///
/// Main broker structure managing clients and topic subscriptions
pub struct MqttBroker<
    const MAX_CLIENTS: usize = 4,
    const MAX_TOPICS: usize = 32,
    const MAX_SUBSCRIPTIONS: usize = 16,
> {
    clients: ClientRegistry<MAX_CLIENTS>,
    topics: TopicRegistry<MAX_CLIENTS>,
}

impl<const MAX_CLIENTS: usize, const MAX_TOPICS: usize, const MAX_SUBSCRIPTIONS: usize>
    MqttBroker<MAX_CLIENTS, MAX_TOPICS, MAX_SUBSCRIPTIONS>
{
    /// Create a new MQTT broker
    pub const fn new() -> Self {
        Self {
            clients: ClientRegistry::new(),
            topics: TopicRegistry::new(),
        }
    }

    /// Register a new client
    pub fn register_client(&mut self, name: ClientName, keep_alive: u16) -> Result<()> {
        let current_time = Self::get_current_time();
        self.clients.register(name, keep_alive, current_time)?;
        Ok(())
    }

    /// Unregister a client
    pub fn unregister_client(&mut self, name: &ClientName) {
        self.clients.unregister(name);
        self.topics.unregister_client(name.clone());
    }

    /// Disconnect a client
    pub fn disconnect_client(&mut self, name: &ClientName) {
        self.unregister_client(name);
    }

    /// Update client activity
    pub fn update_client_activity(&mut self, name: &ClientName) {
        let current_time = Self::get_current_time();
        self.clients.update_activity(name, current_time);
    }

    /// Check if client is connected
    pub fn is_client_connected(&self, name: &ClientName) -> bool {
        self.clients.is_connected(name)
    }

    /// Process expired clients
    pub fn process_expired_clients(&mut self) {
        let current_time = Self::get_current_time();
        let expired = self.clients.get_expired_clients(current_time);

        for client_name in expired {
            self.disconnect_client(&client_name);
        }
    }

    /// Handle PUBLISH packet
    pub async fn handle_publish<S>(
        &self,
        _client_name: &ClientName,
        publish: &Publish<'_>,
        _stream: &mut S,
    ) -> Result<()>
    where
        S: TcpStream,
    {
        let topic_name = TopicName::try_from(publish.topic_name)?;
        let subscribers = self.topics.get_subscribers(topic_name);

        // Route to all subscribers (fire-and-forget for QoS 0)
        // Note: In the single-threaded model, we can't directly access other client streams
        // This is a placeholder for the full implementation
        let _ = subscribers;
        let _ = _stream;

        Ok(())
    }

    /// Handle SUBSCRIBE packet
    pub async fn handle_subscribe<S>(
        &mut self,
        client_name: &ClientName,
        subscribe: &Subscribe<'_>,
        stream: &mut S,
    ) -> Result<()>
    where
        S: TcpStream,
    {
        let topic_filter = TopicSubscription::try_from(subscribe.topic_filter)?;
        self.topics.subscribe(client_name.clone(), topic_filter)?;

        let suback = SubAck {
            packet_id: subscribe.packet_id,
            granted_qos: QoS::AtMostOnce,
            _phantom: Default::default(),
        };

        let mut buffer = [0u8; 256];
        let packet = Packet::SubAck(suback);
        let len = packet.encode(&mut buffer)?;
        stream.write(&buffer[..len]).await?;

        Ok(())
    }

    /// Handle PINGREQ packet
    pub async fn handle_ping<S>(&self, stream: &mut S) -> Result<()>
    where
        S: TcpStream,
    {
        let pingresp = Packet::PingResp(PingResp::default());
        let mut buffer = [0u8; 256];
        let len = pingresp.encode(&mut buffer)?;
        stream.write(&buffer[..len]).await?;
        Ok(())
    }

    /// Get current time in seconds
    fn get_current_time() -> u64 {
        #[cfg(feature = "std")]
        {
            use std::time::{SystemTime, UNIX_EPOCH};
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
        }

        #[cfg(not(feature = "std"))]
        {
            // TODO: Implement Embassy time
            0
        }
    }
}

impl<const MAX_CLIENTS: usize, const MAX_TOPICS: usize, const MAX_SUBSCRIPTIONS: usize> Default
    for MqttBroker<MAX_CLIENTS, MAX_TOPICS, MAX_SUBSCRIPTIONS>
{
    fn default() -> Self {
        Self::new()
    }
}
