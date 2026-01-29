use crate::error::BrokerError;
use crate::protocol::heapless::{HeaplessString, HeaplessVec};
use crate::protocol::ProtocolError;

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

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct TopicEntry<const MAX_TOPIC_NAME_LENGTH: usize, const MAX_SUBSCRIBERS_PER_TOPIC: usize> {
    topic_name: TopicName<MAX_TOPIC_NAME_LENGTH>,
    subscribers: HeaplessVec<u128, MAX_SUBSCRIBERS_PER_TOPIC>,
}

impl<const MAX_TOPIC_NAME_LENGTH: usize, const MAX_SUBSCRIBERS_PER_TOPIC: usize>
    TopicEntry<MAX_TOPIC_NAME_LENGTH, MAX_SUBSCRIBERS_PER_TOPIC>
{
    pub fn new(topic_name: TopicName<MAX_TOPIC_NAME_LENGTH>) -> Self {
        Self {
            topic_name,
            subscribers: HeaplessVec::new(),
        }
    }

    pub fn add_subscriber(&mut self, session_id: u128) -> Result<(), BrokerError> {
        if self.subscribers.contains(&session_id) {
            return Ok(());
        }
        let current = self.subscribers.len();
        self.subscribers
            .push(session_id)
            .map_err(|_| BrokerError::MaxSubscribersPerTopicReached {
                current,
                max: MAX_SUBSCRIBERS_PER_TOPIC,
            })
    }

    pub fn remove_subscriber(&mut self, session_id: u128) -> bool {
        if let Some(pos) = self.subscribers.iter().position(|x| *x == session_id) {
            self.subscribers.remove(pos);
            true
        } else {
            false
        }
    }

    pub fn is_empty(&self) -> bool {
        self.subscribers.is_empty()
    }

    pub fn topic_name(&self) -> &TopicName<MAX_TOPIC_NAME_LENGTH> {
        &self.topic_name
    }

    pub fn subscriber_count(&self) -> usize {
        self.subscribers.len()
    }
}

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
    pub fn subscribe(
        &mut self,
        session_id: u128,
        filter: TopicName<MAX_TOPIC_NAME_LENGTH>,
    ) -> Result<(), BrokerError> {
        let topic_name = filter;

        if let Some(topic_entry) = self.topics.iter_mut().find(|t| t.topic_name == topic_name) {
            topic_entry.add_subscriber(session_id)
        } else {
            let current = self.topics.len();
            if current >= MAX_TOPICS {
                return Err(BrokerError::MaxTopicsReached {
                    current,
                    max: MAX_TOPICS,
                });
            }

            let mut new_entry = TopicEntry::new(topic_name);
            new_entry.add_subscriber(session_id)?;
            self.topics
                .push(new_entry)
                .map_err(|_| BrokerError::BufferFull)
        }
    }

    pub fn get_subscribers(
        &self,
        topic: &TopicName<MAX_TOPIC_NAME_LENGTH>,
    ) -> HeaplessVec<u128, MAX_SUBSCRIBERS_PER_TOPIC> {
        let mut subscribers = HeaplessVec::new();
        let published_topic = topic.as_str();

        for entry in &self.topics {
            let filter = entry.topic_name.as_str();

            if topic_matches(published_topic, filter) {
                for sub_id in &entry.subscribers {
                    if !subscribers.iter().any(|s| s == sub_id) {
                        let _ = subscribers.push(*sub_id);
                    }
                }
            }
        }

        subscribers
    }

    pub fn unsubscribe(
        &mut self,
        session_id: u128,
        filter: &TopicName<MAX_TOPIC_NAME_LENGTH>,
    ) -> bool {
        let topic_name = filter;

        let topic_idx = self.topics.iter().position(|e| e.topic_name == *topic_name);

        if let Some(idx) = topic_idx {
            let removed = self.topics[idx].remove_subscriber(session_id);

            if self.topics[idx].is_empty() {
                self.topics.remove(idx);
            }

            removed
        } else {
            false
        }
    }

    pub fn unregister_all(&mut self, session_id: u128) -> usize {
        let mut removed_count = 0;

        for entry in &mut self.topics {
            if entry.remove_subscriber(session_id) {
                removed_count += 1;
            }
        }

        self.topics.retain(|entry| !entry.is_empty());

        removed_count
    }

    pub fn topic_count(&self) -> usize {
        self.topics.len()
    }

    pub fn subscription_count(&self) -> usize {
        self.topics.iter().map(|t| t.subscribers.len()).sum()
    }

    pub fn clear_all(&mut self) {
        self.topics.clear();
    }
}

fn topic_matches(published: &str, filter: &str) -> bool {
    let mut pub_levels = LevelIterator::new(published);
    let mut sub_levels = LevelIterator::new(filter);

    loop {
        let pub_level = pub_levels.next();
        let sub_level = sub_levels.next();

        if let Some(level) = sub_level {
            if level == "#" {
                if sub_levels.next().is_none() {
                    return true;
                }
            }
        } else {
            return pub_level.is_none();
        }

        if pub_level.is_none() {
            return false;
        }

        if sub_level == Some("+") {
            continue;
        }

        if pub_level != sub_level {
            return false;
        }
    }
}

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

        let topic = TopicName::try_from("sensors/temp").unwrap();
        let subscribers = registry.get_subscribers(&topic);
        assert_eq!(subscribers.len(), 1);
        assert!(subscribers.contains(&session1));

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

        let topic = TopicName::try_from("sensors/room1/temp/extra").unwrap();
        let subscribers = registry.get_subscribers(&topic);
        assert_eq!(subscribers.len(), 0);

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

        registry
            .subscribe(session1, TopicName::try_from("sensors/temp").unwrap())
            .unwrap();
        registry
            .subscribe(session2, TopicName::try_from("sensors/+/temp").unwrap())
            .unwrap();
        registry
            .subscribe(session3, TopicName::try_from("sensors/#").unwrap())
            .unwrap();

        let topic = TopicName::try_from("sensors/room1/temp").unwrap();
        let subscribers = registry.get_subscribers(&topic);
        assert_eq!(subscribers.len(), 2);
        assert!(subscribers.contains(&session2));
        assert!(subscribers.contains(&session3));
        assert!(!subscribers.contains(&session1));

        let topic2 = TopicName::try_from("sensors/temp").unwrap();
        let subscribers2 = registry.get_subscribers(&topic2);
        assert_eq!(subscribers2.len(), 2);
        assert!(subscribers2.contains(&session1));
        assert!(subscribers2.contains(&session3));
        assert!(!subscribers2.contains(&session2));

        let topic3 = TopicName::try_from("sensors/humidity").unwrap();
        let subscribers3 = registry.get_subscribers(&topic3);
        assert_eq!(subscribers3.len(), 1);
        assert!(subscribers3.contains(&session3));
    }

    #[test]
    fn test_duplicate_subscribers_via_wildcards() {
        let mut registry = TopicRegistry::<50, 10, 5>::default();

        let session1 = 1;

        registry
            .subscribe(session1, TopicName::try_from("sensors/temp").unwrap())
            .unwrap();
        registry
            .subscribe(session1, TopicName::try_from("sensors/+").unwrap())
            .unwrap();
        registry
            .subscribe(session1, TopicName::try_from("#").unwrap())
            .unwrap();

        let topic = TopicName::try_from("sensors/temp").unwrap();
        let subscribers = registry.get_subscribers(&topic);
        assert_eq!(subscribers.len(), 1);
        assert!(subscribers.contains(&session1));
    }

    #[test]
    fn test_invalid_wildcard_placement_literal_treatment() {
        let mut registry = TopicRegistry::<50, 10, 5>::default();

        let session1 = 1;

        registry
            .subscribe(session1, TopicName::try_from("sensors/#/temp").unwrap())
            .unwrap();

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
        assert!(!topic_matches("sensors/room1/temp", "sensors/+/abc/temp"));

        assert!(topic_matches("sensors/+/temp", "sensors/+/temp"));

        assert!(!topic_matches("sensors/abc/temp", "sensor+/+/temp"));
    }

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
        assert!(!topic_matches("sensors/temp", "sensors/#/temp"));
        assert!(!topic_matches("sensors", "sensors/#/temp"));
    }

    #[test]
    fn test_plus_and_hash_combined() {
        assert!(topic_matches("sensors/temp", "sensors/+/#"));
        assert!(topic_matches("sensors/temp/room1", "sensors/+/#"));
        assert!(topic_matches("sensors/room1/temp", "sensors/+/temp/#"));
    }

    #[test]
    fn test_empty_topic_levels() {
        assert!(topic_matches("sensors//temp", "sensors//temp"));
        assert!(topic_matches("sensors/", "sensors/"));
        assert!(topic_matches("/temp", "/temp"));
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
        assert_eq!(iter.next(), Some(""));
        assert_eq!(iter.next(), Some("temp"));
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_level_iterator_trailing_slash() {
        let mut iter = LevelIterator::new("sensors/");
        assert_eq!(iter.next(), Some("sensors"));
        assert_eq!(iter.next(), Some(""));
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_level_iterator_leading_slash() {
        let mut iter = LevelIterator::new("/temp");
        assert_eq!(iter.next(), Some(""));
        assert_eq!(iter.next(), Some("temp"));
        assert_eq!(iter.next(), None);
    }

    #[test]
    fn test_level_iterator_empty_string() {
        let mut iter = LevelIterator::new("");
        assert_eq!(iter.next(), None);
    }
}
