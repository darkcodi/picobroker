//! Topic subscription registry
//!
//! Manages topic subscriptions

use crate::broker_error::BrokerError;
use crate::client::ClientId;
use crate::protocol::heapless::{HeaplessString, HeaplessVec};
use crate::protocol::packet_error::PacketEncodingError;

/// Topic name
/// Represents an MQTT topic name with a maximum length.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TopicName<const MAX_TOPIC_NAME_LENGTH: usize>(HeaplessString<MAX_TOPIC_NAME_LENGTH>);

impl<const MAX_TOPIC_NAME_LENGTH: usize> TopicName<MAX_TOPIC_NAME_LENGTH> {
    pub const fn new(name: HeaplessString<MAX_TOPIC_NAME_LENGTH>) -> Self {
        TopicName(name)
    }
}

impl<const MAX_TOPIC_NAME_LENGTH: usize> From<HeaplessString<MAX_TOPIC_NAME_LENGTH>>
    for TopicName<MAX_TOPIC_NAME_LENGTH>
{
    fn from(name: HeaplessString<MAX_TOPIC_NAME_LENGTH>) -> Self {
        TopicName(name)
    }
}

impl<const MAX_TOPIC_NAME_LENGTH: usize> TryFrom<&str> for TopicName<MAX_TOPIC_NAME_LENGTH> {
    type Error = PacketEncodingError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let topic_str = HeaplessString::try_from(value).map_err(|_| {
            PacketEncodingError::TopicNameLengthExceeded {
                max_length: MAX_TOPIC_NAME_LENGTH,
                actual_length: value.len(),
            }
        })?;
        Ok(TopicName(topic_str))
    }
}

impl<const MAX_TOPIC_NAME_LENGTH: usize> core::ops::Deref for TopicName<MAX_TOPIC_NAME_LENGTH> {
    type Target = HeaplessString<MAX_TOPIC_NAME_LENGTH>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<const MAX_TOPIC_NAME_LENGTH: usize> core::ops::DerefMut for TopicName<MAX_TOPIC_NAME_LENGTH> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<const MAX_TOPIC_NAME_LENGTH: usize> core::fmt::Display for TopicName<MAX_TOPIC_NAME_LENGTH> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Topic subscription filter
/// Currently supports only exact topic matching.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum TopicSubscription<const MAX_TOPIC_NAME_LENGTH: usize> {
    #[default]
    Empty,
    Exact(TopicName<MAX_TOPIC_NAME_LENGTH>),
}

impl<const MAX_TOPIC_NAME_LENGTH: usize> TopicSubscription<MAX_TOPIC_NAME_LENGTH> {
    pub fn empty() -> Self {
        TopicSubscription::Empty
    }

    pub fn exact(topic: TopicName<MAX_TOPIC_NAME_LENGTH>) -> Self {
        TopicSubscription::Exact(topic)
    }

    pub fn matches(&self, topic: &str) -> bool {
        match self {
            TopicSubscription::Empty => false,
            TopicSubscription::Exact(t) => t.as_str() == topic,
        }
    }
}

impl<const MAX_TOPIC_NAME_LENGTH: usize> TryFrom<&str>
    for TopicSubscription<MAX_TOPIC_NAME_LENGTH>
{
    type Error = PacketEncodingError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let topic_name = TopicName::try_from(value)?;
        Ok(TopicSubscription::exact(topic_name))
    }
}

/// Represents a single topic with its subscribers
///
/// This is the primary data structure in the topic-centric organization.
/// Each topic maintains a list of clients subscribed to it.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TopicEntry<const MAX_TOPIC_NAME_LENGTH: usize, const MAX_SUBSCRIBERS_PER_TOPIC: usize> {
    topic_name: TopicName<MAX_TOPIC_NAME_LENGTH>,
    subscribers: HeaplessVec<ClientId, MAX_SUBSCRIBERS_PER_TOPIC>,
}

impl<const MAX_TOPIC_NAME_LENGTH: usize, const MAX_SUBSCRIBERS_PER_TOPIC: usize>
    TopicEntry<MAX_TOPIC_NAME_LENGTH, MAX_SUBSCRIBERS_PER_TOPIC>
{
    /// Create a new topic entry
    pub fn new(topic_name: TopicName<MAX_TOPIC_NAME_LENGTH>) -> Self {
        Self {
            topic_name,
            subscribers: HeaplessVec::new(),
        }
    }

    /// Add a subscriber to this topic
    ///
    /// Returns an error if MAX_SUBSCRIBERS_PER_TOPIC is reached.
    /// Does nothing if the client is already subscribed (prevents duplicates).
    pub fn add_subscriber(&mut self, id: ClientId) -> Result<(), BrokerError> {
        // Check for duplicates first
        if self.subscribers.iter().any(|c| c == &id) {
            return Ok(()); // Already subscribed
        }
        self.subscribers
            .push(id)
            .map_err(|_| BrokerError::MaxSubscribersPerTopicReached {
                max_subscribers: MAX_SUBSCRIBERS_PER_TOPIC,
            })
    }

    /// Remove a subscriber from this topic
    ///
    /// Returns true if the subscriber was removed, false if not found.
    pub fn remove_subscriber(&mut self, id: &ClientId) -> bool {
        if let Some(pos) = self.subscribers.iter().position(|c| c == id) {
            self.subscribers.remove(pos);
            true
        } else {
            false
        }
    }

    /// Check if this topic has no subscribers
    pub fn is_empty(&self) -> bool {
        self.subscribers.is_empty()
    }

    /// Get the topic name
    pub fn topic_name(&self) -> &TopicName<MAX_TOPIC_NAME_LENGTH> {
        &self.topic_name
    }

    /// Get the number of subscribers
    pub fn subscriber_count(&self) -> usize {
        self.subscribers.len()
    }
}

/// Topic registry
///
/// Manages topic subscriptions using a topic-centric organization.
/// This is optimized for MQTT workloads where publishes are frequent
/// and client connections/disconnections are rare.
///
/// # Performance
///
/// - **get_subscribers()**: O(topics) - optimized for the publish path
/// - **subscribe()**: O(topics) - find or create topic entry
/// - **unregister_client()**: O(topics * subscribers) - acceptable for rare disconnects
///
/// # Design
///
/// The registry is organized by topics, not by clients. Each topic
/// maintains its own list of subscribers (client IDs). This provides
/// significantly better performance for the common publish operation
/// at the cost of slightly more complex client disconnect handling.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TopicRegistry<
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_TOPICS: usize,
    const MAX_SUBSCRIBERS_PER_TOPIC: usize,
> {
    topics: HeaplessVec<TopicEntry<MAX_TOPIC_NAME_LENGTH, MAX_SUBSCRIBERS_PER_TOPIC>, MAX_TOPICS>,
}

impl<
        const MAX_TOPIC_NAME_LENGTH: usize,
        const MAX_TOPICS: usize,
        const MAX_SUBSCRIBERS_PER_TOPIC: usize,
    > TopicRegistry<MAX_TOPIC_NAME_LENGTH, MAX_TOPICS, MAX_SUBSCRIBERS_PER_TOPIC>
{
    /// Create a new topic registry
    pub fn new() -> Self {
        Self {
            topics: HeaplessVec::new(),
        }
    }

    /// Subscribe a client to a topic filter
    ///
    /// If the topic doesn't exist, it will be created. If it exists,
    /// the client will be added to the subscriber list (unless already
    /// subscribed, in which case this is a no-op).
    ///
    /// # Errors
    ///
    /// Returns `Error::MaxTopicsReached` if MAX_TOPICS is exceeded.
    /// Returns `Error::MaxSubscribersPerTopicReached` if the topic has
    /// reached MAX_SUBSCRIBERS_PER_TOPIC.
    pub fn subscribe(
        &mut self,
        id: ClientId,
        filter: TopicSubscription<MAX_TOPIC_NAME_LENGTH>,
    ) -> Result<(), BrokerError> {
        let topic_name = match filter {
            TopicSubscription::Exact(name) => name,
            TopicSubscription::Empty => {
                return Ok(()); // Empty filter, nothing to subscribe to
            }
        };

        // Find existing topic or create new
        if let Some(topic_entry) = self.topics.iter_mut().find(|t| t.topic_name == topic_name) {
            topic_entry.add_subscriber(id)
        } else {
            // Check if we've reached the max topics limit
            if self.topics.len() >= MAX_TOPICS {
                return Err(BrokerError::MaxTopicsReached {
                    max_topics: MAX_TOPICS,
                });
            }

            // Create new topic entry
            let mut new_entry = TopicEntry::new(topic_name);
            new_entry.add_subscriber(id)?;
            self.topics
                .push(new_entry)
                .map_err(|_| BrokerError::MaxTopicsReached {
                    max_topics: MAX_TOPICS,
                })
        }
    }

    /// Get all subscribers for a topic
    ///
    /// Returns an empty vector if the topic doesn't exist or has no subscribers.
    /// This is optimized for the publish path - O(topics) single pass.
    pub fn get_subscribers(
        &self,
        topic: &TopicName<MAX_TOPIC_NAME_LENGTH>,
    ) -> HeaplessVec<ClientId, MAX_SUBSCRIBERS_PER_TOPIC> {
        self.topics
            .iter()
            .find(|entry| &entry.topic_name == topic)
            .map(|entry| entry.subscribers.clone())
            .unwrap_or_default()
    }

    /// Unsubscribe a client from a specific topic
    ///
    /// Returns true if the subscription was removed, false if not found.
    /// Automatically cleans up empty topics.
    pub fn unsubscribe(
        &mut self,
        id: ClientId,
        filter: &TopicSubscription<MAX_TOPIC_NAME_LENGTH>,
    ) -> bool {
        let topic_name = match filter {
            TopicSubscription::Exact(name) => name,
            TopicSubscription::Empty => return false, // Empty filter, nothing to unsubscribe
        };

        // Find the topic index first
        let topic_idx = self.topics.iter().position(|e| e.topic_name == *topic_name);

        if let Some(idx) = topic_idx {
            let removed = self.topics[idx].remove_subscriber(&id);

            // Auto-cleanup empty topics
            if self.topics[idx].is_empty() {
                self.topics.remove(idx);
            }

            removed
        } else {
            false
        }
    }

    /// Unsubscribe a client from ALL topics
    ///
    /// This is called when a client disconnects. Returns the number of
    /// topics from which the client was removed.
    ///
    /// Automatically cleans up empty topics.
    pub fn unregister_client(&mut self, id: ClientId) -> usize {
        let mut removed_count = 0;

        // Remove client from all topics
        for entry in &mut self.topics {
            if entry.remove_subscriber(&id) {
                removed_count += 1;
            }
        }

        // Clean up empty topics
        self.topics.retain(|entry| !entry.is_empty());

        removed_count
    }

    /// Get the number of active topics
    pub fn topic_count(&self) -> usize {
        self.topics.len()
    }

    /// Get the total number of subscriptions across all topics
    ///
    /// This is useful for monitoring and debugging.
    pub fn subscription_count(&self) -> usize {
        self.topics.iter().map(|t| t.subscribers.len()).sum()
    }

    /// Clear all subscriptions
    pub fn clear_all(&mut self) {
        self.topics.clear();
    }
}
