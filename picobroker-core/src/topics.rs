//! Topic subscription registry
//!
//! Manages topic subscriptions

use crate::error::BrokerError;
use crate::protocol::heapless::{HeaplessString, HeaplessVec};
use crate::protocol::ProtocolError;

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
    type Error = ProtocolError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let topic_str = HeaplessString::try_from(value).map_err(|_| {
            ProtocolError::TopicNameLengthExceeded {
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

/// Represents a single topic with its subscribers
///
/// This is the primary data structure in the topic-centric organization.
/// Each topic maintains a list of sessions subscribed to it.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TopicEntry<const MAX_TOPIC_NAME_LENGTH: usize, const MAX_SUBSCRIBERS_PER_TOPIC: usize> {
    topic_name: TopicName<MAX_TOPIC_NAME_LENGTH>,
    subscribers: HeaplessVec<u128, MAX_SUBSCRIBERS_PER_TOPIC>,
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
    /// Does nothing if the session is already subscribed (prevents duplicates).
    pub fn add_subscriber(&mut self, session_id: u128) -> Result<(), BrokerError> {
        // Check for duplicates first
        if self.subscribers.contains(&session_id) {
            return Ok(()); // Already subscribed
        }
        let current = self.subscribers.len();
        self.subscribers
            .push(session_id)
            .map_err(|_| BrokerError::MaxSubscribersPerTopicReached {
                current,
                max: MAX_SUBSCRIBERS_PER_TOPIC,
            })
    }

    /// Remove a subscriber from this topic
    ///
    /// Returns true if the subscriber was removed, false if not found.
    pub fn remove_subscriber(&mut self, session_id: u128) -> bool {
        if let Some(pos) = self.subscribers.iter().position(|x| *x == session_id) {
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
/// and session connections/disconnections are rare.
///
/// # Design
///
/// The registry is organized by topics, not by sessions. Each topic
/// maintains its own list of subscribers (session IDs). This provides
/// significantly better performance for the common publish operation
/// at the cost of slightly more complex session disconnect handling.
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
    /// Subscribe a session to a topic filter
    ///
    /// If the topic doesn't exist, it will be created. If it exists,
    /// the session will be added to the subscriber list (unless already
    /// subscribed, in which case this is a no-op).
    ///
    /// # Errors
    ///
    /// Returns `Error::MaxTopicsReached` if MAX_TOPICS is exceeded.
    /// Returns `Error::MaxSubscribersPerTopicReached` if the topic has
    /// reached MAX_SUBSCRIBERS_PER_TOPIC.
    pub fn subscribe(
        &mut self,
        session_id: u128,
        filter: TopicName<MAX_TOPIC_NAME_LENGTH>,
    ) -> Result<(), BrokerError> {
        let topic_name = filter;

        // Find existing topic or create new
        if let Some(topic_entry) = self.topics.iter_mut().find(|t| t.topic_name == topic_name) {
            topic_entry.add_subscriber(session_id)
        } else {
            // Check if we've reached the max topics limit
            let current = self.topics.len();
            if current >= MAX_TOPICS {
                return Err(BrokerError::MaxTopicsReached {
                    current,
                    max: MAX_TOPICS,
                });
            }

            // Create new topic entry
            let mut new_entry = TopicEntry::new(topic_name);
            new_entry.add_subscriber(session_id)?;
            self.topics
                .push(new_entry)
                .map_err(|_| BrokerError::BufferFull)
        }
    }

    /// Get all subscribers for a topic
    ///
    /// Returns an empty vector if the topic doesn't exist or has no subscribers.
    /// This is optimized for the publish path - O(topics) single pass.
    ///
    /// # Wildcard Support
    ///
    /// This method supports MQTT wildcard topic filters:
    /// - `+` (single-level): Matches exactly one topic level
    /// - `#` (multi-level): Matches zero or more remaining levels
    ///
    /// All subscription filters are checked, including wildcards.
    pub fn get_subscribers(
        &self,
        topic: &TopicName<MAX_TOPIC_NAME_LENGTH>,
    ) -> HeaplessVec<u128, MAX_SUBSCRIBERS_PER_TOPIC> {
        let mut subscribers = HeaplessVec::new();
        let published_topic = topic.as_str();

        // Check ALL topic entries (wildcard filters require scanning all)
        for entry in &self.topics {
            let filter = entry.topic_name.as_str();

            // Check if published topic matches this subscription filter
            if topic_matches(published_topic, filter) {
                // Add all subscribers, preventing duplicates
                for sub_id in &entry.subscribers {
                    if !subscribers.iter().any(|s| s == sub_id) {
                        let _ = subscribers.push(*sub_id);
                    }
                }
            }
        }

        subscribers
    }

    /// Unsubscribe a session from a specific topic
    ///
    /// Returns true if the subscription was removed, false if not found.
    /// Automatically cleans up empty topics.
    pub fn unsubscribe(
        &mut self,
        session_id: u128,
        filter: &TopicName<MAX_TOPIC_NAME_LENGTH>,
    ) -> bool {
        let topic_name = filter;

        // Find the topic index first
        let topic_idx = self.topics.iter().position(|e| e.topic_name == *topic_name);

        if let Some(idx) = topic_idx {
            let removed = self.topics[idx].remove_subscriber(session_id);

            // Auto-cleanup empty topics
            if self.topics[idx].is_empty() {
                self.topics.remove(idx);
            }

            removed
        } else {
            false
        }
    }

    /// Unsubscribe a session from ALL topics
    ///
    /// This is called when a session disconnects. Returns the number of
    /// topics from which the session was removed.
    ///
    /// Automatically cleans up empty topics.
    pub fn unregister_all(&mut self, session_id: u128) -> usize {
        let mut removed_count = 0;

        // Remove session from all topics
        for entry in &mut self.topics {
            if entry.remove_subscriber(session_id) {
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

/// Wildcard topic matching logic
///
/// Heapless, on-the-fly tokenization for minimal memory usage
///
/// # Wildcard Semantics
///
/// - `+` (single-level): Matches exactly one topic level
/// - `#` (multi-level): Matches zero or more remaining levels (only at end)
fn topic_matches(published: &str, filter: &str) -> bool {
    let mut pub_levels = LevelIterator::new(published);
    let mut sub_levels = LevelIterator::new(filter);

    loop {
        let pub_level = pub_levels.next();
        let sub_level = sub_levels.next();

        // Multi-level wildcard matches everything remaining
        // Only works as wildcard if it's the last level in filter
        if let Some(level) = sub_level {
            if level == "#" {
                // Check if this is the last level (no more levels after #)
                if sub_levels.next().is_none() {
                    return true;
                }
                // # not at end - treat as literal (won't match anything except literal "#" in published topic)
            }
        } else {
            // Filter exhausted, publish must also be exhausted
            return pub_level.is_none();
        }

        // Published topic exhausted but filter continues
        if pub_level.is_none() {
            return false;
        }

        // Single-level wildcard matches any single level (only in filter)
        if sub_level == Some("+") {
            continue;
        }

        // Exact match required for this level
        if pub_level != sub_level {
            return false;
        }
    }
}

/// Iterator over topic levels (split by /)
///
/// Zero-allocation iterator that yields &str slices representing
/// individual levels of a topic hierarchy.
struct LevelIterator<'a> {
    remaining: &'a str,
    pending_empty: bool,
}

impl<'a> LevelIterator<'a> {
    fn new(topic: &'a str) -> Self {
        Self {
            remaining: topic,
            pending_empty: false,
        }
    }
}

impl<'a> Iterator for LevelIterator<'a> {
    type Item = &'a str;

    fn next(&mut self) -> Option<Self::Item> {
        // If we have a pending empty level from a trailing slash, return it
        if self.pending_empty {
            self.pending_empty = false;
            return Some("");
        }

        if self.remaining.is_empty() {
            return None;
        }

        match self.remaining.find('/') {
            Some(pos) => {
                let level = &self.remaining[..pos];
                self.remaining = &self.remaining[pos + 1..];
                // If remaining is now empty, we had a trailing slash
                if self.remaining.is_empty() {
                    self.pending_empty = true;
                }
                Some(level)
            }
            None => {
                let level = self.remaining;
                self.remaining = "";
                Some(level)
            }
        }
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_exact_match_subscription() {
        let mut registry = TopicRegistry::<50, 10, 5>::default();

        let session1 = 1;
        let filter = TopicName::try_from("sensors/temp").unwrap();

        registry.subscribe(session1, filter).unwrap();

        // Exact match should return subscriber
        let topic = TopicName::try_from("sensors/temp").unwrap();
        let subscribers = registry.get_subscribers(&topic);
        assert_eq!(subscribers.len(), 1);
        assert!(subscribers.contains(&session1));

        // Different topic should not match
        let topic2 = TopicName::try_from("sensors/humidity").unwrap();
        let subscribers2 = registry.get_subscribers(&topic2);
        assert_eq!(subscribers2.len(), 0);
    }

    #[test]
    fn test_plus_wildcard_subscription() {
        let mut registry = TopicRegistry::<50, 10, 5>::default();

        let session1 = 1;
        let filter = TopicName::try_from("sensors/+/temp").unwrap();
        registry.subscribe(session1, filter).unwrap();

        // Should match various room sensors
        let test_cases = [
            "sensors/room1/temp",
            "sensors/room2/temp",
            "sensors/abc/temp",
            "sensors/123/temp",
        ];

        for topic_str in test_cases {
            let topic = TopicName::try_from(topic_str).unwrap();
            let subscribers = registry.get_subscribers(&topic);
            assert_eq!(subscribers.len(), 1, "Should match {}", topic_str);
            assert!(subscribers.contains(&session1));
        }

        // Should not match extra levels
        let topic = TopicName::try_from("sensors/room1/temp/extra").unwrap();
        let subscribers = registry.get_subscribers(&topic);
        assert_eq!(subscribers.len(), 0);

        // Should not match different structure
        let topic = TopicName::try_from("sensors/temp").unwrap();
        let subscribers = registry.get_subscribers(&topic);
        assert_eq!(subscribers.len(), 0);
    }

    #[test]
    fn test_hash_wildcard_subscription() {
        let mut registry = TopicRegistry::<50, 10, 5>::default();

        let session1 = 1;
        let filter = TopicName::try_from("sensors/#").unwrap();
        registry.subscribe(session1, filter).unwrap();

        // Should match all sensors topics
        let test_cases = [
            "sensors",
            "sensors/temp",
            "sensors/temp/room1",
            "sensors/a/b/c/d/e/f",
        ];

        for topic_str in test_cases {
            let topic = TopicName::try_from(topic_str).unwrap();
            let subscribers = registry.get_subscribers(&topic);
            assert_eq!(subscribers.len(), 1, "Should match {}", topic_str);
            assert!(subscribers.contains(&session1));
        }

        // Should not match non-sensors topics
        let topic = TopicName::try_from("other/temp").unwrap();
        let subscribers = registry.get_subscribers(&topic);
        assert_eq!(subscribers.len(), 0);
    }

    #[test]
    fn test_multiple_wildcard_subscriptions() {
        let mut registry = TopicRegistry::<50, 10, 5>::default();

        let session1 = 1;
        let session2 = 2;
        let session3 = 3;

        // Three different subscriptions
        registry
            .subscribe(session1, TopicName::try_from("sensors/temp").unwrap())
            .unwrap();
        registry
            .subscribe(session2, TopicName::try_from("sensors/+/temp").unwrap())
            .unwrap();
        registry
            .subscribe(session3, TopicName::try_from("sensors/#").unwrap())
            .unwrap();

        // Publish to "sensors/room1/temp" should match session2 and session3
        let topic = TopicName::try_from("sensors/room1/temp").unwrap();
        let subscribers = registry.get_subscribers(&topic);
        assert_eq!(subscribers.len(), 2);
        assert!(subscribers.contains(&session2));
        assert!(subscribers.contains(&session3));
        assert!(!subscribers.contains(&session1));

        // Publish to "sensors/temp" should match session1 (exact) and session3 (# wildcard)
        // But NOT session2 because sensors/+/temp expects 3 levels while sensors/temp has only 2
        let topic2 = TopicName::try_from("sensors/temp").unwrap();
        let subscribers2 = registry.get_subscribers(&topic2);
        assert_eq!(subscribers2.len(), 2);
        assert!(subscribers2.contains(&session1));
        assert!(subscribers2.contains(&session3));
        assert!(!subscribers2.contains(&session2));

        // Publish to "sensors/humidity" should only match session3
        let topic3 = TopicName::try_from("sensors/humidity").unwrap();
        let subscribers3 = registry.get_subscribers(&topic3);
        assert_eq!(subscribers3.len(), 1);
        assert!(subscribers3.contains(&session3));
    }

    #[test]
    fn test_duplicate_subscribers_via_wildcards() {
        let mut registry = TopicRegistry::<50, 10, 5>::default();

        let session1 = 1;

        // Same session subscribes via multiple wildcards
        registry
            .subscribe(session1, TopicName::try_from("sensors/temp").unwrap())
            .unwrap();
        registry
            .subscribe(session1, TopicName::try_from("sensors/+").unwrap())
            .unwrap();
        registry
            .subscribe(session1, TopicName::try_from("#").unwrap())
            .unwrap();

        // Should only receive message once (no duplicates)
        let topic = TopicName::try_from("sensors/temp").unwrap();
        let subscribers = registry.get_subscribers(&topic);
        assert_eq!(subscribers.len(), 1);
        assert!(subscribers.contains(&session1));
    }

    #[test]
    fn test_invalid_wildcard_placement_literal_treatment() {
        let mut registry = TopicRegistry::<50, 10, 5>::default();

        let session1 = 1;

        // # not at end (per requirements: treat as literal, won't match)
        registry
            .subscribe(session1, TopicName::try_from("sensors/#/temp").unwrap())
            .unwrap();

        // Should not match anything
        let topic = TopicName::try_from("sensors/temp").unwrap();
        let subscribers = registry.get_subscribers(&topic);
        assert_eq!(subscribers.len(), 0);

        let topic2 = TopicName::try_from("sensors/anything/temp").unwrap();
        let subscribers2 = registry.get_subscribers(&topic2);
        assert_eq!(subscribers2.len(), 0);
    }
}

#[cfg(test)]
mod wildcard_tests {
    use super::*;

    // ===== EXACT MATCHES (No wildcards) =====

    #[test]
    fn test_exact_match_single_level() {
        assert!(topic_matches("temp", "temp"));
        assert!(!topic_matches("temp", "humidity"));
    }

    #[test]
    fn test_exact_match_multi_level() {
        assert!(topic_matches("sensors/temp", "sensors/temp"));
        assert!(!topic_matches("sensors/temp", "sensors/humidity"));
    }

    #[test]
    fn test_exact_match_different_levels() {
        assert!(!topic_matches("sensors/temp", "sensors"));
        assert!(!topic_matches("sensors", "sensors/temp"));
    }

    // ===== SINGLE-LEVEL WILDCARD (+) =====

    #[test]
    fn test_plus_wildcard_single_level() {
        assert!(topic_matches("temp", "+"));
        assert!(topic_matches("any", "+"));
    }

    #[test]
    fn test_plus_wildcard_middle() {
        assert!(topic_matches("sensors/room1/temp", "sensors/+/temp"));
        assert!(topic_matches("sensors/xyz/temp", "sensors/+/temp"));
        assert!(!topic_matches(
            "sensors/room1/temp/humidity",
            "sensors/+/temp"
        ));
    }

    #[test]
    fn test_plus_wildcard_first_level() {
        assert!(topic_matches("abc/temp", "+/temp"));
        assert!(!topic_matches("xyz/abc/temp", "+/temp"));
    }

    #[test]
    fn test_plus_wildcard_last_level() {
        assert!(topic_matches("sensors/value", "sensors/+"));
        assert!(!topic_matches("sensors", "sensors/+"));
    }

    #[test]
    fn test_multiple_plus_wildcards() {
        assert!(topic_matches("a/b/c", "+/+/c"));
        assert!(topic_matches("x/y/z", "+/+/+"));
        assert!(!topic_matches("a/b", "+/+/+"));
    }

    #[test]
    fn test_plus_wildcard_literal_treatment() {
        // Per requirements: invalid wildcard placement = literal (won't match)
        assert!(!topic_matches("sensors/room1/temp", "sensors/+/abc/temp"));

        // + wildcard in filter matches any level, including literal + in published topic
        assert!(topic_matches("sensors/+/temp", "sensors/+/temp"));

        // Filter with embedded + (not a whole level) is treated as literal
        assert!(!topic_matches("sensors/abc/temp", "sensor+/+/temp"));
    }

    // ===== MULTI-LEVEL WILDCARD (#) =====

    #[test]
    fn test_hash_wildcard_match_all() {
        assert!(topic_matches("anything", "#"));
        assert!(topic_matches("any/thing", "#"));
        assert!(topic_matches("any/thing/at/any/level", "#"));
    }

    #[test]
    fn test_hash_wildcard_suffix() {
        assert!(topic_matches("sensors", "sensors/#"));
        assert!(topic_matches("sensors/temp", "sensors/#"));
        assert!(topic_matches("sensors/temp/room1", "sensors/#"));
        assert!(topic_matches("sensors/a/b/c/d", "sensors/#"));
        assert!(!topic_matches("sensors", "sensors/temp/#"));
    }

    #[test]
    fn test_hash_wildcard_alone() {
        assert!(topic_matches("anything", "#"));
        assert!(topic_matches("a/b/c", "#"));
    }

    #[test]
    fn test_hash_wildcard_match_zero_levels() {
        assert!(topic_matches("sensors", "sensors/#"));
    }

    #[test]
    fn test_hash_wildcard_literal_treatment() {
        // Per requirements: # not at end = literal (won't match anything)
        assert!(!topic_matches("sensors/temp", "sensors/#/temp"));
        assert!(!topic_matches("sensors", "sensors/#/temp"));
    }

    // ===== COMBINED WILDCARDS =====

    #[test]
    fn test_plus_and_hash_combined() {
        assert!(topic_matches("sensors/temp", "sensors/+/#"));
        assert!(topic_matches("sensors/temp/room1", "sensors/+/#"));
        assert!(topic_matches("sensors/room1/temp", "sensors/+/temp/#"));
    }

    // ===== EDGE CASES =====

    #[test]
    fn test_empty_topic_levels() {
        assert!(topic_matches("sensors//temp", "sensors//temp")); // Empty level in middle
        assert!(topic_matches("sensors/", "sensors/")); // Trailing slash
        assert!(topic_matches("/temp", "/temp")); // Leading slash
    }

    #[test]
    fn test_unicode_topics() {
        assert!(topic_matches("ñáé", "ñáé"));
        assert!(topic_matches("sensor/温度", "sensor/温度"));
        assert!(topic_matches("sensor/温度", "sensor/+"));
    }

    #[test]
    fn test_case_sensitive() {
        assert!(topic_matches("Sensors/Temp", "Sensors/Temp"));
        assert!(!topic_matches("sensors/temp", "Sensors/Temp"));
        assert!(!topic_matches("Sensors/Temp", "sensors/temp"));
    }

    #[test]
    fn test_long_topics() {
        let long_topic = "a/b/c/d/e/f/g/h/i/j/k/l/m/n/o/p/q/r/s/t/u/v/w/x/y/z";
        assert!(topic_matches(long_topic, "+/+/+/+/+/+/+/+/+/+/+/+/+/+/#"));
    }

    #[test]
    fn test_empty_topic() {
        assert!(!topic_matches("", "sensors"));
        assert!(!topic_matches("sensors", ""));
        assert!(topic_matches("", ""));
        assert!(topic_matches("", "#"));
    }

    // ===== LEVEL ITERATOR TESTS =====

    #[test]
    fn test_level_iterator_basic() {
        let mut iter = LevelIterator::new("sensors/temp/room1");
        assert_eq!(iter.next(), Some("sensors"));
        assert_eq!(iter.next(), Some("temp"));
        assert_eq!(iter.next(), Some("room1"));
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_level_iterator_single_level() {
        let mut iter = LevelIterator::new("sensors");
        assert_eq!(iter.next(), Some("sensors"));
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_level_iterator_empty_levels() {
        let mut iter = LevelIterator::new("sensors//temp");
        assert_eq!(iter.next(), Some("sensors"));
        assert_eq!(iter.next(), Some("")); // Empty level
        assert_eq!(iter.next(), Some("temp"));
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_level_iterator_trailing_slash() {
        let mut iter = LevelIterator::new("sensors/");
        assert_eq!(iter.next(), Some("sensors"));
        assert_eq!(iter.next(), Some("")); // Empty trailing level
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_level_iterator_leading_slash() {
        let mut iter = LevelIterator::new("/temp");
        assert_eq!(iter.next(), Some("")); // Empty leading level
        assert_eq!(iter.next(), Some("temp"));
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_level_iterator_empty_string() {
        let mut iter = LevelIterator::new("");
        assert_eq!(iter.next(), None);
    }
}
