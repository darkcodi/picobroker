//! MQTT broker implementation (core logic)
//!
//! Manages client connections, message routing, and keep-alive monitoring

use crate::broker_error::BrokerError;
use crate::client::{ClientId, ClientRegistry};
use crate::protocol::packets::{
    ConnAckPacket, ConnectPacket, Packet, PingRespPacket, PubAckPacket, PublishPacket,
    SubAckPacket, SubscribePacket,
};
use crate::protocol::qos::QoS;
use crate::topics::{TopicRegistry, TopicSubscription};

/// MQTT broker (core logic)
///
/// Main broker structure managing clients and topic subscriptions.
/// This is the core that is platform-agnostic and no_std compatible.
///
/// # Generic Parameters
///
/// - `MAX_TOPIC_NAME_LENGTH`: Maximum length of topic names
/// - `MAX_PAYLOAD_SIZE`: Maximum payload size for packets
/// - `QUEUE_SIZE`: Queue size for client -> broker messages and broker -> client messages
/// - `MAX_CLIENTS`: Maximum number of concurrent clients
/// - `MAX_TOPICS`: Maximum number of distinct topics
/// - `MAX_SUBSCRIBERS_PER_TOPIC`: Maximum subscribers per topic
#[derive(Debug)]
pub struct PicoBroker<
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const QUEUE_SIZE: usize,
    const MAX_CLIENTS: usize,
    const MAX_TOPICS: usize,
    const MAX_SUBSCRIBERS_PER_TOPIC: usize,
> {
    topics: TopicRegistry<MAX_TOPIC_NAME_LENGTH, MAX_TOPICS, MAX_SUBSCRIBERS_PER_TOPIC>,
    clients: ClientRegistry<
        MAX_TOPIC_NAME_LENGTH,
        MAX_PAYLOAD_SIZE,
        QUEUE_SIZE,
        MAX_CLIENTS,
        MAX_TOPICS,
        MAX_SUBSCRIBERS_PER_TOPIC,
    >,
}

impl<
        const MAX_TOPIC_NAME_LENGTH: usize,
        const MAX_PAYLOAD_SIZE: usize,
        const QUEUE_SIZE: usize,
        const MAX_CLIENTS: usize,
        const MAX_TOPICS: usize,
        const MAX_SUBSCRIBERS_PER_TOPIC: usize,
    > Default
    for PicoBroker<
        MAX_TOPIC_NAME_LENGTH,
        MAX_PAYLOAD_SIZE,
        QUEUE_SIZE,
        MAX_CLIENTS,
        MAX_TOPICS,
        MAX_SUBSCRIBERS_PER_TOPIC,
    >
{
    fn default() -> Self {
        Self {
            topics: TopicRegistry::default(),
            clients: ClientRegistry::default(),
        }
    }
}

impl<
        const MAX_TOPIC_NAME_LENGTH: usize,
        const MAX_PAYLOAD_SIZE: usize,
        const QUEUE_SIZE: usize,
        const MAX_CLIENTS: usize,
        const MAX_TOPICS: usize,
        const MAX_SUBSCRIBERS_PER_TOPIC: usize,
    >
    PicoBroker<
        MAX_TOPIC_NAME_LENGTH,
        MAX_PAYLOAD_SIZE,
        QUEUE_SIZE,
        MAX_CLIENTS,
        MAX_TOPICS,
        MAX_SUBSCRIBERS_PER_TOPIC,
    >
{
    /// Create a new MQTT broker with the given time source
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a new client session
    pub fn register_client(
        &mut self,
        client_id: ClientId,
        keep_alive_secs: u16,
        current_time: u64,
    ) -> Result<(), BrokerError> {
        self.clients
            .register_new_client(client_id, keep_alive_secs, current_time)
    }

    /// Mark a client as disconnected
    pub fn mark_client_disconnected(&mut self, client_id: &ClientId) -> Result<(), BrokerError> {
        self.clients.mark_disconnected(client_id)
    }

    /// Remove a client session and cleanup subscriptions
    pub fn remove_client(&mut self, client_id: &ClientId) -> Result<(), BrokerError> {
        self.clients.remove_client(client_id)?;
        self.topics.unregister_client(client_id);
        log::info!("Removing client {}", client_id);
        Ok(())
    }

    /// Get all client IDs
    pub fn get_all_clients(&self) -> [Option<ClientId>; MAX_CLIENTS] {
        self.clients.get_all_clients()
    }

    /// Get expired clients (keep-alive timeout)
    pub fn get_expired_clients(&mut self, current_time: u64) -> [Option<ClientId>; MAX_CLIENTS] {
        self.clients.get_expired_clients(current_time)
    }

    /// Get disconnected clients
    pub fn get_disconnected_clients(&mut self) -> [Option<ClientId>; MAX_CLIENTS] {
        self.clients.get_disconnected_clients()
    }

    /// Queue a packet to a session's RX queue (network -> session)
    pub fn queue_packet_received_from_client(
        &mut self,
        client_id: &ClientId,
        packet: Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>,
        current_time: u64,
    ) -> Result<(), BrokerError> {
        self.clients
            .update_client_activity(client_id, current_time)?;
        self.clients.queue_rx_packet(client_id, packet)
    }

    /// Dequeue a packet from a session's TX queue
    pub fn dequeue_packet_to_send_to_client(
        &mut self,
        client_id: &ClientId,
    ) -> Result<Option<Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>>, BrokerError> {
        self.clients.dequeue_tx_packet(client_id)
    }

    /// Process all packets from all sessions (round-robin)
    ///
    /// Returns the number of packets processed.
    pub fn process_all_client_packets(&mut self) -> Result<usize, BrokerError> {
        use Packet;

        // Collect client IDs (avoid holding mutable reference during iteration)
        let client_ids = self.get_all_clients();

        let mut total_processed = 0usize;

        // Process each client's packets
        for client_id in client_ids.iter().flatten() {
            // Process all packets from this client
            while let Some(packet) = self.clients.dequeue_rx_packet(client_id)? {
                // Handle packet based on type
                match packet {
                    Packet::Connect(connect) => {
                        self.handle_connect(client_id, &connect)?;
                    }
                    Packet::ConnAck(_) => {
                        // Client should not send CONNACK
                        log::info!("Unexpected CONNACK from client {}", client_id);
                    }
                    Packet::Publish(publish) => {
                        self.handle_publish(client_id, &publish)?;
                    }
                    Packet::PubAck(_) => {
                        // QoS 2 handling - not implemented yet
                    }
                    Packet::PubRec(_) => {}
                    Packet::PubRel(_) => {}
                    Packet::PubComp(_) => {}
                    Packet::Subscribe(subscribe) => {
                        self.handle_subscribe(client_id, &subscribe)?;
                    }
                    Packet::SubAck(_) => {
                        // Client should not send SUBACK
                        log::info!("Unexpected SUBACK from client {}", client_id);
                    }
                    Packet::Unsubscribe(_) => {}
                    Packet::UnsubAck(_) => {}
                    Packet::PingReq(_) => {
                        self.handle_pingreq(client_id)?;
                    }
                    Packet::PingResp(_) => {
                        log::info!("Received PINGRESP from client {}", client_id);
                    }
                    Packet::Disconnect(_) => {
                        self.handle_disconnect(client_id)?;
                    }
                }

                total_processed += 1;
            }
        }

        Ok(total_processed)
    }

    fn handle_connect(
        &mut self,
        client_id: &ClientId,
        connect: &ConnectPacket<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>,
    ) -> Result<(), BrokerError> {
        let mut client_id = client_id;

        // Update client_id if provided
        if !connect.client_id.is_empty() {
            self.clients
                .update_client_id(client_id, connect.client_id.clone())?;
            client_id = &connect.client_id;
        }

        // Update keep_alive
        self.clients
            .update_client_keep_alive(client_id, connect.keep_alive)?;

        // Update state
        self.clients.mark_connected(client_id)?;

        // Queue CONNACK
        let connack = Packet::ConnAck(ConnAckPacket::default());
        self.clients.queue_tx_packet(client_id, connack)?;

        Ok(())
    }

    fn handle_publish(
        &mut self,
        client_id: &ClientId,
        publish: &PublishPacket<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>,
    ) -> Result<(), BrokerError> {
        // Get subscribers for this topic
        let subscribers = self.topics.get_subscribers(&publish.topic_name);

        // Route to each subscriber
        for subscriber_id in &subscribers {
            let packet = Packet::Publish(publish.clone());
            self.clients.queue_tx_packet(subscriber_id, packet)?;
        }

        // Send PUBACK if QoS > 0
        if publish.qos > QoS::AtMostOnce && publish.packet_id.is_some() {
            let puback = PubAckPacket {
                packet_id: publish.packet_id.unwrap(),
            };

            let puback_packet = Packet::PubAck(puback);
            self.clients.queue_tx_packet(client_id, puback_packet)?;
        }

        Ok(())
    }

    fn handle_subscribe(
        &mut self,
        client_id: &ClientId,
        subscribe: &SubscribePacket<MAX_TOPIC_NAME_LENGTH>,
    ) -> Result<(), BrokerError> {
        let subscription = TopicSubscription::Exact(subscribe.topic_filter.clone());
        self.topics.subscribe(client_id.clone(), subscription)?;

        // For simplicity, grant requested QoS directly, but in real implementation, it should be min(requested, supported)
        let granted_qos = subscribe.requested_qos;

        let suback = SubAckPacket {
            packet_id: subscribe.packet_id,
            granted_qos,
        };

        self.clients
            .queue_tx_packet(client_id, Packet::SubAck(suback))
    }

    fn handle_pingreq(&mut self, client_id: &ClientId) -> Result<(), BrokerError> {
        self.clients
            .queue_tx_packet(client_id, Packet::PingResp(PingRespPacket))
    }

    fn handle_disconnect(&mut self, client_id: &ClientId) -> Result<(), BrokerError> {
        self.mark_client_disconnected(client_id)
    }
}
