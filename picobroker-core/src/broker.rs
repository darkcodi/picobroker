//! MQTT broker implementation (core sync logic)
//!
//! Manages client connections, message routing, and keep-alive monitoring

use crate::client::{ClientId, ClientRegistry};
use crate::time::TimeSource;
use crate::topics::TopicRegistry;
use crate::BrokerError;

/// MQTT broker (core sync logic)
///
/// Main broker structure managing clients and topic subscriptions.
/// This is the sync core that is platform-agnostic and no_std compatible.
///
/// # Generic Parameters
///
/// - `T`: Time source for tracking keep-alives
/// - `MAX_TOPIC_NAME_LENGTH`: Maximum length of topic names
/// - `MAX_CLIENTS`: Maximum number of concurrent clients
/// - `MAX_TOPICS`: Maximum number of distinct topics
/// - `MAX_SUBSCRIBERS_PER_TOPIC`: Maximum subscribers per topic
#[derive(Debug)]
pub struct PicoBroker<
    T: TimeSource,
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_CLIENTS: usize,
    const MAX_TOPICS: usize,
    const MAX_SUBSCRIBERS_PER_TOPIC: usize,
> {
    time_source: T,
    clients: ClientRegistry<MAX_CLIENTS>,
    topics: TopicRegistry<MAX_TOPIC_NAME_LENGTH, MAX_TOPICS, MAX_SUBSCRIBERS_PER_TOPIC>,
}

impl<
        T: TimeSource,
        const MAX_TOPIC_NAME_LENGTH: usize,
        const MAX_CLIENTS: usize,
        const MAX_TOPICS: usize,
        const MAX_SUBSCRIBERS_PER_TOPIC: usize,
    > PicoBroker<T, MAX_TOPIC_NAME_LENGTH, MAX_CLIENTS, MAX_TOPICS, MAX_SUBSCRIBERS_PER_TOPIC>
{
    /// Create a new MQTT broker with the given time source
    pub fn new(time_source: T) -> Self {
        Self {
            time_source,
            clients: ClientRegistry::new(),
            topics: TopicRegistry::new(),
        }
    }

    /// Register a new client
    ///
    /// Returns the client ID that should be used for subsequent operations.
    pub fn register_client(&mut self, id: ClientId, keep_alive: u16) -> Result<(), BrokerError> {
        let current_time = self.time_source.now_secs();
        self.clients.register(id, keep_alive, current_time)?;
        Ok(())
    }

    /// Unregister a client
    pub fn unregister_client(&mut self, id: ClientId) {
        self.clients.unregister(&id);
        self.topics.unregister_client(id);
    }

    /// Disconnect a client
    pub fn disconnect_client(&mut self, id: ClientId) {
        self.unregister_client(id);
    }

    /// Update client activity
    pub fn update_client_activity(&mut self, id: &ClientId) {
        let current_time = self.time_source.now_secs();
        self.clients.update_activity(id, current_time);
    }

    /// Check if client is connected
    pub fn is_client_connected(&self, id: &ClientId) -> bool {
        self.clients.is_connected(id)
    }

    /// Process expired clients
    pub fn process_expired_clients(&mut self) {
        let current_time = self.time_source.now_secs();
        let expired = self.clients.get_expired_clients(current_time);

        for client_name in expired {
            self.disconnect_client(client_name);
        }
    }

    /// Get reference to the topics registry
    pub fn topics(
        &self,
    ) -> &TopicRegistry<MAX_TOPIC_NAME_LENGTH, MAX_TOPICS, MAX_SUBSCRIBERS_PER_TOPIC> {
        &self.topics
    }

    /// Get mutable reference to the topics registry
    pub fn topics_mut(
        &mut self,
    ) -> &mut TopicRegistry<MAX_TOPIC_NAME_LENGTH, MAX_TOPICS, MAX_SUBSCRIBERS_PER_TOPIC> {
        &mut self.topics
    }

    /// Get reference to the clients registry
    pub fn clients(&self) -> &ClientRegistry<MAX_CLIENTS> {
        &self.clients
    }
}
