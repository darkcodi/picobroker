//! Topic subscription registry
//!
//! Manages topic subscriptions with exact matching (no wildcards)

use crate::error::{Error, Result};

/// Topic filter (exact string matching, no wildcards)
#[derive(Debug, Clone)]
pub struct TopicFilter {
    pub topic: heapless::String<64>,
}

impl TopicFilter {
    pub fn new(topic: &str) -> Result<Self> {
        Ok(TopicFilter {
            topic: heapless::String::try_from(topic)
                .map_err(|_| Error::MalformedString)?,
        })
    }
}

/// Topic subscription registry
///
/// Maps topics to lists of subscribed client indices.
/// Uses exact string matching (no wildcards).
pub struct TopicRegistry<const MAX_CLIENTS: usize> {
    // Each entry: (topic_name, [client_indices])
    // Max 32 unique topics system-wide
    entries: heapless::Vec<
        (heapless::String<64>, heapless::Vec<usize, MAX_CLIENTS>),
        32,
    >,
}

impl<const MAX_CLIENTS: usize> TopicRegistry<MAX_CLIENTS> {
    pub const fn new() -> Self {
        Self {
            entries: heapless::Vec::new(),
        }
    }

    /// Subscribe a client to a topic
    pub fn subscribe(&mut self, topic: &str, client_idx: usize) -> Result<()> {
        // Find existing topic or create new entry
        match self
            .entries
            .iter()
            .position(|(t, _)| t.as_str() == topic)
        {
            Some(pos) => {
                // Add client to existing topic
                let (_topic, clients) = &mut self.entries[pos];
                if !clients.contains(&client_idx) {
                    clients.push(client_idx).map_err(|_| Error::MaxSubscribersReached)?;
                }
            }
            None => {
                // Create new topic entry
                if self.entries.len() >= 32 {
                    return Err(Error::MaxTopicsReached);
                }
                let mut clients = heapless::Vec::new();
                clients.push(client_idx).map_err(|_| Error::MaxSubscribersReached)?;
                self.entries
                    .push((
                        heapless::String::try_from(topic).map_err(|_| Error::MalformedString)?,
                        clients,
                    ))
                    .map_err(|_| Error::TopicRegistryFull)?;
            }
        }
        Ok(())
    }

    /// Unsubscribe a client from a topic
    pub fn unsubscribe(&mut self, topic: &str, client_idx: usize) -> Result<()> {
        if let Some(pos) = self.entries.iter().position(|(t, _)| t.as_str() == topic) {
            let (_topic, clients) = &mut self.entries[pos];
            if let Some(idx) = clients.iter().position(|&c| c == client_idx) {
                clients.remove(idx);
            }
        }
        Ok(())
    }

    /// Get all subscribers for a topic
    pub fn get_subscribers(&self, topic: &str) -> heapless::Vec<usize, MAX_CLIENTS> {
        self.entries
            .iter()
            .find(|(t, _)| t.as_str() == topic)
            .map(|(_, clients)| clients.clone())
            .unwrap_or_default()
    }

    /// Unsubscribe a client from all topics
    pub fn unregister_all(&mut self, client_idx: usize) {
        for (_topic, clients) in &mut self.entries {
            if let Some(pos) = clients.iter().position(|&c| c == client_idx) {
                clients.remove(pos);
            }
        }
    }

    /// Get number of active topics
    pub fn topic_count(&self) -> usize {
        self.entries.len()
    }

    /// Clear all subscriptions
    pub fn clear(&mut self) {
        self.entries.clear();
    }
}

impl<const MAX_CLIENTS: usize> Default for TopicRegistry<MAX_CLIENTS> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subscribe_and_get_subscribers() {
        let mut registry = TopicRegistry::<4>::new();

        registry.subscribe("sensors/temp", 0).unwrap();
        registry.subscribe("sensors/temp", 1).unwrap();

        let subscribers = registry.get_subscribers("sensors/temp");
        assert_eq!(subscribers.len(), 2);
        assert!(subscribers.contains(&0));
        assert!(subscribers.contains(&1));
    }

    #[test]
    fn test_unsubscribe() {
        let mut registry = TopicRegistry::<4>::new();

        registry.subscribe("test/topic", 0).unwrap();
        registry.subscribe("test/topic", 1).unwrap();
        registry.unsubscribe("test/topic", 0).unwrap();

        let subscribers = registry.get_subscribers("test/topic");
        assert_eq!(subscribers.len(), 1);
        assert!(subscribers.contains(&1));
        assert!(!subscribers.contains(&0));
    }

    #[test]
    fn test_unregister_all() {
        let mut registry = TopicRegistry::<4>::new();

        registry.subscribe("topic1", 0).unwrap();
        registry.subscribe("topic2", 0).unwrap();
        registry.subscribe("topic1", 1).unwrap();

        registry.unregister_all(0);

        assert_eq!(registry.get_subscribers("topic1").len(), 1);
        assert!(!registry.get_subscribers("topic1").contains(&0));
        assert!(registry.get_subscribers("topic1").contains(&1));
        assert_eq!(registry.get_subscribers("topic2").len(), 0);
    }

    #[test]
    fn test_exact_matching() {
        let mut registry = TopicRegistry::<4>::new();

        registry.subscribe("sensors/temp", 0).unwrap();

        // Exact match
        assert_eq!(registry.get_subscribers("sensors/temp").len(), 1);

        // No wildcards - different topics
        assert_eq!(registry.get_subscribers("sensors/humidity").len(), 0);
        assert_eq!(registry.get_subscribers("sensors").len(), 0);
        assert_eq!(registry.get_subscribers("sensor/temp").len(), 0);
    }

    #[test]
    fn test_max_subscribers() {
        let mut registry = TopicRegistry::<2>::new();

        registry.subscribe("test", 0).unwrap();
        registry.subscribe("test", 1).unwrap();

        // Should fail - max 2 subscribers
        let result = registry.subscribe("test", 2);
        assert!(matches!(result, Err(Error::MaxSubscribersReached)));
    }

    #[test]
    fn test_multiple_topics() {
        let mut registry = TopicRegistry::<4>::new();

        registry.subscribe("topic1", 0).unwrap();
        registry.subscribe("topic2", 1).unwrap();
        registry.subscribe("topic3", 0).unwrap();

        assert_eq!(registry.topic_count(), 3);
        assert!(registry.get_subscribers("topic1").contains(&0));
        assert!(registry.get_subscribers("topic2").contains(&1));
        assert!(registry.get_subscribers("topic3").contains(&0));
    }
}
