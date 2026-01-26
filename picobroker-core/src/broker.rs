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

    // ===== Client Registry Access =====

    /// Get all sessions (immutable)
    pub fn sessions(&self) -> &HeaplessVec<Option<ClientSession<
        MAX_TOPIC_NAME_LENGTH,
        MAX_PAYLOAD_SIZE,
        QUEUE_SIZE
    >>, MAX_CLIENTS> {
        &self.client_registry.sessions
    }

    /// Get all sessions (mutable)
    pub fn sessions_mut(&mut self) -> &mut HeaplessVec<Option<ClientSession<
        MAX_TOPIC_NAME_LENGTH,
        MAX_PAYLOAD_SIZE,
        QUEUE_SIZE
    >>, MAX_CLIENTS> {
        &mut self.client_registry.sessions
    }

    /// Find a session by client ID (mutable)
    pub fn find_session(&mut self, client_id: &ClientId) -> Option<&mut ClientSession<
        MAX_TOPIC_NAME_LENGTH,
        MAX_PAYLOAD_SIZE,
        QUEUE_SIZE
    >> {
        self.client_registry.find_session_by_client_id(client_id)
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

    /// Get a reference to the topics registry
    pub fn topics(&self) -> &TopicRegistry<MAX_TOPIC_NAME_LENGTH, MAX_TOPICS, MAX_SUBSCRIBERS_PER_TOPIC> {
        &self.topics
    }

    /// Get a mutable reference to the topics registry
    pub fn topics_mut(&mut self) -> &mut TopicRegistry<MAX_TOPIC_NAME_LENGTH, MAX_TOPICS, MAX_SUBSCRIBERS_PER_TOPIC> {
        &mut self.topics
    }

    /// Get a reference to the client registry
    pub fn client_registry(&self) -> &ClientRegistry<
        MAX_TOPIC_NAME_LENGTH,
        MAX_PAYLOAD_SIZE,
        QUEUE_SIZE,
        MAX_CLIENTS,
        MAX_TOPICS,
        MAX_SUBSCRIBERS_PER_TOPIC,
    > {
        &self.client_registry
    }

    /// Get a mutable reference to the client registry
    pub fn client_registry_mut(&mut self) -> &mut ClientRegistry<
        MAX_TOPIC_NAME_LENGTH,
        MAX_PAYLOAD_SIZE,
        QUEUE_SIZE,
        MAX_CLIENTS,
        MAX_TOPICS,
        MAX_SUBSCRIBERS_PER_TOPIC,
    > {
        &mut self.client_registry
    }
}
