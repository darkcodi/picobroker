//! MQTT broker implementation (core logic)
//!
//! Manages client connections, message routing, and keep-alive monitoring

use crate::topics::TopicRegistry;
use crate::server::{ClientRegistry, ClientSession};
use crate::{ClientId, BrokerError, TopicName, TopicSubscription};
use crate::protocol::HeaplessVec;

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
    client_registry: ClientRegistry<
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
    > PicoBroker<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_SIZE, MAX_CLIENTS, MAX_TOPICS, MAX_SUBSCRIBERS_PER_TOPIC>
{
    /// Create a new MQTT broker with the given time source
    pub fn new() -> Self {
        Self {
            topics: TopicRegistry::new(),
            client_registry: ClientRegistry::new(),
        }
    }

    // ===== Client Lifecycle Operations =====

    /// Register a new client session
    pub fn register_client(
        &mut self,
        client_id: ClientId,
        keep_alive_secs: u16,
        current_time: u64,
    ) -> Result<(), BrokerError> {
        self.client_registry.register_new_client(client_id, keep_alive_secs, current_time)
    }

    /// Mark a client as disconnected
    pub fn mark_client_disconnected(&mut self, client_id: ClientId) {
        self.client_registry.mark_disconnected(client_id);
    }

    /// Remove a client session and cleanup subscriptions
    pub fn remove_client(&mut self, client_id: &ClientId) -> Result<(), BrokerError> {
        self.client_registry.remove_session(client_id, &mut self.topics)
    }

    /// Cleanup zombie sessions (connected but without active connection)
    pub fn cleanup_zombie_sessions(&mut self) {
        self.client_registry.cleanup_zombie_sessions(&mut self.topics);
    }

    /// Cleanup expired sessions (keep-alive timeout)
    pub fn cleanup_expired_sessions(&mut self, current_time: u64) {
        self.client_registry.cleanup_expired_sessions(&mut self.topics, current_time);
    }

    // ===== Topic Operations =====

    /// Subscribe a client to a topic
    pub fn subscribe_client(
        &mut self,
        client_id: ClientId,
        subscription: TopicSubscription<MAX_TOPIC_NAME_LENGTH>,
    ) -> Result<(), BrokerError> {
        // Verify client exists
        if self.client_registry.find_session_by_client_id(&client_id).is_none() {
            return Err(BrokerError::ClientNotFound);
        }
        self.topics.subscribe(client_id, subscription)
    }

    /// Get subscribers for a topic
    pub fn get_topic_subscribers(
        &self,
        topic: &TopicName<MAX_TOPIC_NAME_LENGTH>,
    ) -> HeaplessVec<ClientId, MAX_SUBSCRIBERS_PER_TOPIC> {
        self.topics.get_subscribers(topic)
    }

    // ===== New High-Level API =====

    // ===== Query Methods (Immutable) =====

    /// Get all active client IDs into a stack-allocated array
    ///
    /// Returns the number of active clients (capped at output array size)
    pub fn get_active_client_ids(&self, output: &mut [Option<ClientId>]) -> usize {
        self.client_registry.get_active_client_ids(output)
    }

    /// Get the number of active (connected/connecting) sessions
    pub fn active_session_count(&self) -> usize {
        self.client_registry.active_session_count()
    }

    /// Check if a client session exists
    pub fn has_session(&self, client_id: &ClientId) -> bool {
        self.client_registry.has_session(client_id)
    }

    // ===== State Update Methods (Mutable) =====

    /// Update a session's activity timestamp
    pub fn update_session_activity(
        &mut self,
        client_id: &ClientId,
        current_time: u64,
    ) -> Result<(), BrokerError> {
        if let Some(session) = self.client_registry.find_session_by_client_id(client_id) {
            session.update_activity(current_time);
            Ok(())
        } else {
            Err(BrokerError::ClientNotFound)
        }
    }

    /// Set a session's connection state
    pub fn set_session_state(
        &mut self,
        client_id: &ClientId,
        state: crate::server::ClientState,
    ) -> Result<(), BrokerError> {
        if let Some(session) = self.client_registry.find_session_by_client_id(client_id) {
            session.state = state;
            Ok(())
        } else {
            Err(BrokerError::ClientNotFound)
        }
    }

    /// Update a session's client_id (for CONNECT packet handling)
    ///
    /// This updates both the session's client_id
    /// Note: The caller is responsible for updating any external mappings
    pub fn update_session_client_id(
        &mut self,
        old_id: &ClientId,
        new_id: ClientId,
    ) -> Result<(), BrokerError> {
        if let Some(session) = self.client_registry.find_session_by_client_id(old_id) {
            session.client_id = new_id;
            Ok(())
        } else {
            Err(BrokerError::ClientNotFound)
        }
    }

    /// Update a session's keep-alive interval
    pub fn update_session_keep_alive(
        &mut self,
        client_id: &ClientId,
        keep_alive_secs: u16,
    ) -> Result<(), BrokerError> {
        if let Some(session) = self.client_registry.find_session_by_client_id(client_id) {
            session.keep_alive_secs = keep_alive_secs;
            Ok(())
        } else {
            Err(BrokerError::ClientNotFound)
        }
    }

    // ===== Queue Methods (Mutable) =====

    /// Queue a packet to a session's RX queue (network -> session)
    pub fn queue_rx_packet(
        &mut self,
        client_id: &ClientId,
        packet: crate::Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>,
    ) -> Result<(), BrokerError> {
        if let Some(session) = self.client_registry.find_session_by_client_id(client_id) {
            session.queue_rx_packet(packet)
        } else {
            Err(BrokerError::ClientNotFound)
        }
    }

    /// Queue a packet to a session's TX queue (broker -> client)
    pub fn queue_tx_packet(
        &mut self,
        client_id: &ClientId,
        packet: crate::Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>,
    ) -> Result<(), BrokerError> {
        if let Some(session) = self.client_registry.find_session_by_client_id(client_id) {
            session.queue_tx_packet(packet)
        } else {
            Err(BrokerError::ClientNotFound)
        }
    }

    /// Dequeue a packet from a session's RX queue
    pub fn dequeue_rx_packet(
        &mut self,
        client_id: &ClientId,
    ) -> Option<crate::Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>> {
        self.client_registry.find_session_by_client_id(client_id)?.dequeue_rx_packet()
    }

    /// Dequeue a packet from a session's TX queue
    pub fn dequeue_tx_packet(
        &mut self,
        client_id: &ClientId,
    ) -> Option<crate::Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>> {
        self.client_registry.find_session_by_client_id(client_id)?.dequeue_tx_packet()
    }

    // ===== High-Level Message Operations (Mutable) =====

    /// Publish a message from a client to all topic subscribers
    ///
    /// Returns the number of subscribers the message was routed to.
    /// Also queues PUBACK to the publisher if QoS > 0.
    ///
    /// This encapsulates the entire PUBLISH handling logic.
    pub fn publish_message(
        &mut self,
        publisher_id: &ClientId,
        publish: crate::PublishPacket<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>,
    ) -> Result<usize, BrokerError> {
        // Get subscribers for this topic
        let subscribers = self.topics.get_subscribers(&publish.topic_name);

        // Route to each subscriber
        let mut routed_count = 0usize;
        for subscriber_id in &subscribers {
            if let Some(session) = self.client_registry.find_session_by_client_id(subscriber_id) {
                let packet = crate::Packet::Publish(publish.clone());
                if session.queue_tx_packet(packet).is_ok() {
                    routed_count += 1;
                }
            }
        }

        // Send PUBACK if QoS > 0
        if publish.qos > crate::QoS::AtMostOnce && publish.packet_id.is_some() {
            let mut puback = crate::PubAckPacket::default();
            puback.packet_id = publish.packet_id.unwrap();

            if let Some(session) = self.client_registry.find_session_by_client_id(publisher_id) {
                let _ = session.queue_tx_packet(crate::Packet::PubAck(puback));
            }
        }

        Ok(routed_count)
    }

    /// Subscribe a client to a topic and send SUBACK
    ///
    /// Returns Ok if subscription succeeded (SUBACK queued).
    /// Returns Err if subscription failed (failure SUBACK queued).
    ///
    /// This encapsulates the entire SUBSCRIBE handling logic.
    pub fn subscribe_client_to_topic(
        &mut self,
        client_id: &ClientId,
        topic_filter: TopicName<MAX_TOPIC_NAME_LENGTH>,
        packet_id: u16,
        requested_qos: crate::QoS,
    ) -> Result<(), BrokerError> {
        let subscription = TopicSubscription::Exact(topic_filter);

        match self.topics.subscribe(client_id.clone(), subscription) {
            Ok(()) => {
                // Success - send SUBACK with granted QoS
                if let Some(session) = self.client_registry.find_session_by_client_id(client_id) {
                    let suback = crate::SubAckPacket {
                        packet_id,
                        granted_qos: requested_qos,
                    };
                    let _ = session.queue_tx_packet(crate::Packet::SubAck(suback));
                }
                Ok(())
            }
            Err(e) => {
                // Failure - send SUBACK with failure QoS
                if let Some(session) = self.client_registry.find_session_by_client_id(client_id) {
                    let suback = crate::SubAckPacket {
                        packet_id,
                        granted_qos: crate::QoS::from_u8(0x80).unwrap_or(crate::QoS::AtMostOnce),
                    };
                    let _ = session.queue_tx_packet(crate::Packet::SubAck(suback));
                }
                Err(e)
            }
        }
    }

    // ===== High-Level Packet Handlers (Mutable) =====

    /// Handle a CONNECT packet for a session
    ///
    /// Updates client_id, keep_alive, state, and queues CONNACK.
    ///
    /// This encapsulates CONNECT packet handling logic.
    pub fn handle_connect(
        &mut self,
        client_id: &ClientId,
        connect: &crate::ConnectPacket<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>,
    ) -> Result<(), BrokerError> {
        if let Some(session) = self.client_registry.find_session_by_client_id(client_id) {
            // Update client_id if provided
            if !connect.client_id.is_empty() {
                session.client_id = connect.client_id.clone();
            }

            // Update keep_alive
            session.keep_alive_secs = connect.keep_alive;

            // Update state
            session.state = crate::server::ClientState::Connected;

            // Queue CONNACK
            let connack = crate::Packet::ConnAck(crate::ConnAckPacket::default());
            let _ = session.queue_tx_packet(connack);

            Ok(())
        } else {
            Err(BrokerError::ClientNotFound)
        }
    }

    /// Handle a PINGREQ packet for a session
    ///
    /// Queues PINGRESP response.
    ///
    /// This encapsulates PINGREQ handling logic.
    pub fn handle_pingreq(
        &mut self,
        client_id: &ClientId,
    ) -> Result<(), BrokerError> {
        if let Some(session) = self.client_registry.find_session_by_client_id(client_id) {
            let pingresp = crate::Packet::PingResp(crate::PingRespPacket::default());
            session.queue_tx_packet(pingresp)
        } else {
            Err(BrokerError::ClientNotFound)
        }
    }

    /// Handle a DISCONNECT packet for a session
    ///
    /// Marks session as disconnected (will be cleaned up later).
    ///
    /// This encapsulates DISCONNECT handling logic.
    pub fn handle_disconnect(
        &mut self,
        client_id: &ClientId,
    ) -> Result<(), BrokerError> {
        self.set_session_state(client_id, crate::server::ClientState::Disconnected)
    }

    // ===== Master Processing Method (Mutable) =====

    /// Process all packets from all sessions (round-robin)
    ///
    /// For each packet received:
    /// - For CONNECT: calls handle_connect()
    /// - For PUBLISH: calls publish_message()
    /// - For SUBSCRIBE: calls subscribe_client_to_topic()
    /// - For PINGREQ: calls handle_pingreq()
    /// - For DISCONNECT: calls handle_disconnect()
    ///
    /// Returns the number of packets processed.
    pub fn process_all_client_packets(&mut self) -> Result<usize, BrokerError> {
        use crate::Packet;

        // Collect client IDs (avoid holding mutable reference during iteration)
        let mut client_ids = [const { None }; 16];
        let client_count = self.get_active_client_ids(&mut client_ids);

        let mut total_processed = 0usize;

        // Process each client's packets
        for i in 0..client_count {
            if let Some(client_id) = &client_ids[i] {
                // Process all packets from this client
                while let Some(packet) = self.dequeue_rx_packet(client_id) {
                    match packet {
                        Packet::Connect(connect) => {
                            self.handle_connect(client_id, &connect)?;
                        }
                        Packet::ConnAck(_) => {
                            // Client should not send CONNACK
                            log::info!("Unexpected CONNACK from client {}", client_id);
                        }
                        Packet::Publish(publish) => {
                            self.publish_message(client_id, publish)?;
                        }
                        Packet::PubAck(_) => {
                            // QoS 2 handling - not implemented yet
                        }
                        Packet::PubRec(_) => {}
                        Packet::PubRel(_) => {}
                        Packet::PubComp(_) => {}
                        Packet::Subscribe(subscribe) => {
                            self.subscribe_client_to_topic(
                                client_id,
                                subscribe.topic_filter,
                                subscribe.packet_id,
                                subscribe.requested_qos,
                            )?;
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
                            // Client response to our PINGREQ - activity already updated
                        }
                        Packet::Disconnect(_) => {
                            self.handle_disconnect(client_id)?;
                        }
                    }

                    total_processed += 1;
                }
            }
        }

        Ok(total_processed)
    }
}
