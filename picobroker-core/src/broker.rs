//! MQTT broker implementation (core logic)
//!
//! Manages client connections, message routing, and keep-alive monitoring

use log::info;
use crate::broker_error::BrokerError;
use crate::protocol::packets::{
    ConnAckPacket, ConnectPacket, Packet, PingRespPacket, PubAckPacket, PublishPacket,
    SubAckPacket, SubscribePacket,
};
use crate::protocol::qos::QoS;
use crate::session::SessionRegistry;
use crate::topics::TopicRegistry;

/// MQTT broker (core logic)
///
/// Main broker structure managing sessions and topic subscriptions.
/// This is the core that is platform-agnostic and no_std compatible.
///
/// # Generic Parameters
///
/// - `MAX_TOPIC_NAME_LENGTH`: Maximum length of topic names
/// - `MAX_PAYLOAD_SIZE`: Maximum payload size for packets
/// - `QUEUE_SIZE`: Queue size for session -> broker messages and broker -> session messages
/// - `MAX_SESSIONS`: Maximum number of concurrent sessions
/// - `MAX_TOPICS`: Maximum number of distinct topics
/// - `MAX_SUBSCRIBERS_PER_TOPIC`: Maximum subscribers per topic
#[derive(Debug)]
pub struct PicoBroker<
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const QUEUE_SIZE: usize,
    const MAX_SESSIONS: usize,
    const MAX_TOPICS: usize,
    const MAX_SUBSCRIBERS_PER_TOPIC: usize,
> {
    topics: TopicRegistry<MAX_TOPIC_NAME_LENGTH, MAX_TOPICS, MAX_SUBSCRIBERS_PER_TOPIC>,
    sessions: SessionRegistry<
        MAX_TOPIC_NAME_LENGTH,
        MAX_PAYLOAD_SIZE,
        QUEUE_SIZE,
        MAX_SESSIONS,
        MAX_TOPICS,
        MAX_SUBSCRIBERS_PER_TOPIC,
    >,
}

impl<
        const MAX_TOPIC_NAME_LENGTH: usize,
        const MAX_PAYLOAD_SIZE: usize,
        const QUEUE_SIZE: usize,
        const MAX_SESSIONS: usize,
        const MAX_TOPICS: usize,
        const MAX_SUBSCRIBERS_PER_TOPIC: usize,
    > Default
    for PicoBroker<
        MAX_TOPIC_NAME_LENGTH,
        MAX_PAYLOAD_SIZE,
        QUEUE_SIZE,
        MAX_SESSIONS,
        MAX_TOPICS,
        MAX_SUBSCRIBERS_PER_TOPIC,
    >
{
    fn default() -> Self {
        Self {
            topics: TopicRegistry::default(),
            sessions: SessionRegistry::default(),
        }
    }
}

impl<
        const MAX_TOPIC_NAME_LENGTH: usize,
        const MAX_PAYLOAD_SIZE: usize,
        const QUEUE_SIZE: usize,
        const MAX_SESSIONS: usize,
        const MAX_TOPICS: usize,
        const MAX_SUBSCRIBERS_PER_TOPIC: usize,
    >
    PicoBroker<
        MAX_TOPIC_NAME_LENGTH,
        MAX_PAYLOAD_SIZE,
        QUEUE_SIZE,
        MAX_SESSIONS,
        MAX_TOPICS,
        MAX_SUBSCRIBERS_PER_TOPIC,
    >
{
    /// Create a new MQTT broker with the given time source
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a new client session
    pub fn register_new_session(
        &mut self,
        session_id: u128,
        keep_alive_secs: u16,
        current_time: u128,
    ) -> Result<(), BrokerError> {
        self.sessions
            .register_new_session(session_id, keep_alive_secs, current_time)
    }

    /// Mark a session as disconnected
    pub fn mark_session_disconnected(&mut self, session_id: u128) -> Result<(), BrokerError> {
        self.sessions.mark_disconnected(session_id)
    }

    /// Remove a client session and cleanup subscriptions
    pub fn remove_session(&mut self, session_id: u128) -> bool {
        self.topics.unregister_all(session_id);
        self.sessions.remove_session(session_id)
    }

    /// Get all session IDs
    pub fn get_all_sessions(&self) -> [Option<u128>; MAX_SESSIONS] {
        self.sessions.get_all_sessions()
    }

    /// Get expired session IDs (keep-alive timeout)
    pub fn get_expired_sessions(&mut self, current_time: u128) -> [Option<u128>; MAX_SESSIONS] {
        self.sessions.get_expired_sessions(current_time)
    }

    /// Get disconnected session IDs
    pub fn get_disconnected_sessions(&mut self) -> [Option<u128>; MAX_SESSIONS] {
        self.sessions.get_disconnected_sessions()
    }

    /// Queue a packet to a session's RX queue (network -> session)
    pub fn queue_packet_received_from_client(
        &mut self,
        session_id: u128,
        packet: Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>,
        current_time: u128,
    ) -> Result<(), BrokerError> {
        self.sessions
            .update_session_activity(session_id, current_time)?;
        self.sessions.queue_rx_packet(session_id, packet)
    }

    /// Dequeue a packet from a session's TX queue
    pub fn dequeue_packet_to_send_to_client(
        &mut self,
        session_id: u128,
    ) -> Result<Option<Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>>, BrokerError> {
        self.sessions.dequeue_tx_packet(session_id)
    }

    /// Process all packets from all sessions (round-robin)
    ///
    /// Returns the number of packets processed.
    pub fn process_all_session_packets(&mut self) -> Result<(), BrokerError> {
        // Collect session IDs (avoid holding mutable reference during iteration)
        let session_ids = self.get_all_sessions();

        let mut total_processed = 0usize;

        // Process each session's packets
        for session_id in session_ids.into_iter().flatten() {
            // Process all packets from this session
            while let Some(packet) = self.sessions.dequeue_rx_packet(session_id)? {
                // Handle packet based on type
                match packet {
                    Packet::Connect(connect) => {
                        self.handle_connect(session_id, &connect)?;
                    }
                    Packet::ConnAck(_) => {
                        // Client should not send CONNACK
                        info!("Unexpected CONNACK from session {}", session_id);
                    }
                    Packet::Publish(publish) => {
                        self.handle_publish(session_id, &publish)?;
                    }
                    Packet::PubAck(_) => {
                        // QoS 2 handling - not implemented yet
                    }
                    Packet::PubRec(_) => {}
                    Packet::PubRel(_) => {}
                    Packet::PubComp(_) => {}
                    Packet::Subscribe(subscribe) => {
                        self.handle_subscribe(session_id, &subscribe)?;
                    }
                    Packet::SubAck(_) => {
                        // Client should not send SUBACK
                        info!("Unexpected SUBACK from session {}", session_id);
                    }
                    Packet::Unsubscribe(_) => {}
                    Packet::UnsubAck(_) => {}
                    Packet::PingReq(_) => {
                        self.handle_pingreq(session_id)?;
                    }
                    Packet::PingResp(_) => {
                        info!("Received PINGRESP from session {}", session_id);
                    }
                    Packet::Disconnect(_) => {
                        self.handle_disconnect(session_id)?;
                    }
                }

                total_processed += 1;
            }
        }

        if total_processed > 0 {
            info!("Processed {} packets from sessions", total_processed);
        }

        Ok(())
    }

    fn handle_connect(
        &mut self,
        session_id: u128,
        connect: &ConnectPacket<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>,
    ) -> Result<(), BrokerError> {
        // Update client_id if provided
        if !connect.client_id.is_empty() {
            self.sessions
                .update_client_id(session_id, connect.client_id.clone())?;
        }

        // Update keep_alive
        self.sessions
            .update_keep_alive(session_id, connect.keep_alive)?;

        // Update state
        self.sessions.mark_connected(session_id)?;

        // Queue CONNACK
        let connack = Packet::ConnAck(ConnAckPacket::default());
        self.sessions.queue_tx_packet(session_id, connack)?;

        Ok(())
    }

    fn handle_publish(
        &mut self,
        session_id: u128,
        publish: &PublishPacket<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>,
    ) -> Result<(), BrokerError> {
        // Get subscribers for this topic
        let subscribers = self.topics.get_subscribers(&publish.topic_name);

        // Route to each subscriber
        for subscriber_id in subscribers {
            let packet = Packet::Publish(publish.clone());
            self.sessions.queue_tx_packet(subscriber_id, packet)?;
        }

        // Send PUBACK if QoS > 0
        if publish.qos > QoS::AtMostOnce && publish.packet_id.is_some() {
            let puback = PubAckPacket {
                packet_id: publish.packet_id.unwrap(),
            };

            let puback_packet = Packet::PubAck(puback);
            self.sessions.queue_tx_packet(session_id, puback_packet)?;
        }

        Ok(())
    }

    fn handle_subscribe(
        &mut self,
        session_id: u128,
        subscribe: &SubscribePacket<MAX_TOPIC_NAME_LENGTH>,
    ) -> Result<(), BrokerError> {
        self.topics.subscribe(session_id, subscribe.topic_filter.clone())?;

        // For simplicity, grant requested QoS directly, but in real implementation, it should be min(requested, supported)
        let granted_qos = subscribe.requested_qos;

        let suback = SubAckPacket {
            packet_id: subscribe.packet_id,
            granted_qos,
        };

        self.sessions
            .queue_tx_packet(session_id, Packet::SubAck(suback))
    }

    fn handle_pingreq(&mut self, session_id: u128) -> Result<(), BrokerError> {
        self.sessions
            .queue_tx_packet(session_id, Packet::PingResp(PingRespPacket))
    }

    fn handle_disconnect(&mut self, session_id: u128) -> Result<(), BrokerError> {
        self.mark_session_disconnected(session_id)
    }
}
