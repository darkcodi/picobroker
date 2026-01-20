//! Topic subscription registry
//!
//! Manages topic subscriptions

use crate::client::ClientName;
use crate::error::{Error, Result};

const DEFAULT_TOPIC_LENGTH: usize = 30;
const DEFAULT_SUBSCRIPTIONS: usize = 8;

/// Topic name
/// Represents an MQTT topic name with a maximum length.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TopicName<const MAX_TOPIC_LENGTH: usize = DEFAULT_TOPIC_LENGTH>(heapless::String<MAX_TOPIC_LENGTH>);

impl<const MAX_TOPIC_LENGTH: usize> TopicName<MAX_TOPIC_LENGTH> {
    pub fn new(name: heapless::String<MAX_TOPIC_LENGTH>) -> Self {
        TopicName(name)
    }
}

impl<const MAX_TOPIC_LENGTH: usize> From<heapless::String<MAX_TOPIC_LENGTH>> for TopicName<MAX_TOPIC_LENGTH> {
    fn from(name: heapless::String<MAX_TOPIC_LENGTH>) -> Self {
        TopicName(name)
    }
}

impl<const MAX_TOPIC_LENGTH: usize> TryFrom<&str> for TopicName<MAX_TOPIC_LENGTH> {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self> {
        let topic_str = heapless::String::try_from(value)
            .map_err(|_| Error::TopicLengthExceeded {
                max_length: MAX_TOPIC_LENGTH,
                actual_length: value.len(),
            })?;
        Ok(TopicName(topic_str))
    }
}

impl<const MAX_TOPIC_LENGTH: usize> core::ops::Deref for TopicName<MAX_TOPIC_LENGTH> {
    type Target = heapless::String<MAX_TOPIC_LENGTH>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<const MAX_TOPIC_LENGTH: usize> core::ops::DerefMut for TopicName<MAX_TOPIC_LENGTH> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<const MAX_TOPIC_LENGTH: usize> core::fmt::Display for TopicName<MAX_TOPIC_LENGTH> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl<const MAX_TOPIC_LENGTH: usize> defmt::Format for TopicName<MAX_TOPIC_LENGTH> {
    fn format(&self, f: defmt::Formatter) {
        defmt::write!(f, "{}", self.0.as_str());
    }
}

/// Topic subscription filter
/// Currently supports only exact topic matching.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Default)]
pub enum TopicSubscription {
    #[default]
    Empty,
    Exact(TopicName),
}

impl TopicSubscription {
    pub fn empty() -> Self {
        TopicSubscription::Empty
    }

    pub fn exact(topic: TopicName) -> Self {
        TopicSubscription::Exact(topic)
    }

    pub fn matches(&self, topic: &str) -> bool {
        match self {
            TopicSubscription::Empty => false,
            TopicSubscription::Exact(t) => t.as_str() == topic,
        }
    }
}

impl TryFrom<&str> for TopicSubscription {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self> {
        let topic_name = TopicName::try_from(value)?;
        Ok(TopicSubscription::exact(topic_name))
    }
}

/// Client subscriptions
/// Keeps track of a client's topic subscriptions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientSubscriptions<const MAX_SUBSCRIPTIONS: usize = DEFAULT_SUBSCRIPTIONS> {
    pub client_name: ClientName,
    pub subscriptions: heapless::Vec<TopicSubscription, MAX_SUBSCRIPTIONS>,
}

impl<const MAX_SUBSCRIPTIONS: usize> ClientSubscriptions<MAX_SUBSCRIPTIONS> {
    pub fn new(client_name: ClientName) -> Self {
        Self {
            client_name,
            subscriptions: heapless::Vec::new(),
        }
    }

    pub fn add_filter(&mut self, filter: TopicSubscription) -> Result<()> {
        self.subscriptions
            .push(filter)
            .map_err(|_| Error::MaxSubscriptionsReached {
                max_subscriptions: MAX_SUBSCRIPTIONS,
            })
    }

    pub fn remove_filter(&mut self, filter: &TopicSubscription) -> bool {
        if let Some(pos) = self.subscriptions.iter().position(|f| f == filter) {
            self.subscriptions.remove(pos);
            return true;
        }
        false
    }

    pub fn clear_all_filters(&mut self) {
        self.subscriptions.clear();
    }
}

/// Topic registry
/// Manages topic subscriptions for multiple clients.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TopicRegistry<const MAX_CLIENTS: usize> {
    clients: heapless::Vec<ClientSubscriptions, MAX_CLIENTS>,
}

impl<const MAX_CLIENTS: usize> TopicRegistry<MAX_CLIENTS> {
    /// Create a new topic registry
    pub const fn new() -> Self {
        Self {
            clients: heapless::Vec::new(),
        }
    }

    /// Get all subscribers for a topic
    pub fn get_subscribers(&self, topic: TopicName) -> heapless::Vec<&ClientName, MAX_CLIENTS> {
        let mut subscribers = heapless::Vec::<&ClientName, MAX_CLIENTS>::new();

        for client_subs in &self.clients {
            for filter in &client_subs.subscriptions {
                if filter.matches(topic.as_str()) {
                    subscribers.push(&client_subs.client_name).ok();
                    break;
                }
            }
        }

        subscribers
    }

    /// Subscribe a client to a topic
    pub fn subscribe(&mut self, client: ClientName, sub: TopicSubscription) -> Result<()> {
        // Find existing client subscriptions or create new
        if let Some(client_subs) = self.clients.iter_mut().find(|cs| cs.client_name == client) {
            client_subs.add_filter(sub)
        } else {
            let mut new_client_subs = ClientSubscriptions::new(client);
            new_client_subs.add_filter(sub)?;
            self.clients
                .push(new_client_subs)
                .map_err(|_| Error::MaxClientsReached {
                    max_clients: MAX_CLIENTS,
                })
        }
    }

    /// Unsubscribe a client from a topic
    pub fn unsubscribe(&mut self, client: ClientName, sub: TopicSubscription) -> bool {
        if let Some(client_subs) = self.clients.iter_mut().find(|cs| cs.client_name == client) {
            client_subs.remove_filter(&sub)
        } else {
            false
        }
    }

    /// Unsubscribe a client from all topics
    pub fn unregister_client(&mut self, client: ClientName) {
        if let Some(pos) = self.clients.iter().position(|cs| cs.client_name == client) {
            self.clients.remove(pos);
        }
    }

    /// Get number of active topics
    pub fn clients_count(&self) -> usize {
        self.clients.len()
    }

    /// Clear all subscriptions
    pub fn clear_all(&mut self) {
        self.clients.clear();
    }
}
