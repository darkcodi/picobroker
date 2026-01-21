//! Topic subscription registry
//!
//! Manages topic subscriptions

use crate::client::ClientName;
use crate::error::{Error, Result};

/// Topic name
/// Represents an MQTT topic name with a maximum length.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct TopicName<const MAX_TOPIC_NAME_LENGTH: usize>(
    heapless::String<MAX_TOPIC_NAME_LENGTH>,
);

impl<const MAX_TOPIC_NAME_LENGTH: usize> TopicName<MAX_TOPIC_NAME_LENGTH> {
    pub fn new(name: heapless::String<MAX_TOPIC_NAME_LENGTH>) -> Self {
        TopicName(name)
    }
}

impl<const MAX_TOPIC_NAME_LENGTH: usize> From<heapless::String<MAX_TOPIC_NAME_LENGTH>>
    for TopicName<MAX_TOPIC_NAME_LENGTH>
{
    fn from(name: heapless::String<MAX_TOPIC_NAME_LENGTH>) -> Self {
        TopicName(name)
    }
}

impl<const MAX_TOPIC_NAME_LENGTH: usize> TryFrom<&str> for TopicName<MAX_TOPIC_NAME_LENGTH> {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self> {
        let topic_str =
            heapless::String::try_from(value).map_err(|_| Error::TopicLengthExceeded {
                max_length: MAX_TOPIC_NAME_LENGTH,
                actual_length: value.len(),
            })?;
        Ok(TopicName(topic_str))
    }
}

impl<const MAX_TOPIC_NAME_LENGTH: usize> core::ops::Deref for TopicName<MAX_TOPIC_NAME_LENGTH> {
    type Target = heapless::String<MAX_TOPIC_NAME_LENGTH>;

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

impl<const MAX_TOPIC_NAME_LENGTH: usize> TryFrom<&str> for TopicSubscription<MAX_TOPIC_NAME_LENGTH> {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self> {
        let topic_name = TopicName::try_from(value)?;
        Ok(TopicSubscription::exact(topic_name))
    }
}

/// Client subscriptions
/// Keeps track of a client's topic subscriptions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientSubscriptions<const MAX_CLIENT_NAME_LENGTH: usize, const MAX_TOPIC_NAME_LENGTH: usize, const MAX_SUBSCRIPTIONS_PER_CLIENT: usize> {
    pub client_name: ClientName<MAX_CLIENT_NAME_LENGTH>,
    pub subscriptions: heapless::Vec<TopicSubscription<MAX_TOPIC_NAME_LENGTH>, MAX_SUBSCRIPTIONS_PER_CLIENT>,
}

impl<const MAX_CLIENT_NAME_LENGTH: usize, const MAX_TOPIC_NAME_LENGTH: usize, const MAX_SUBSCRIPTIONS_PER_CLIENT: usize> ClientSubscriptions<MAX_CLIENT_NAME_LENGTH, MAX_TOPIC_NAME_LENGTH, MAX_SUBSCRIPTIONS_PER_CLIENT> {
    pub fn new(client_name: ClientName<MAX_CLIENT_NAME_LENGTH>) -> Self {
        Self {
            client_name,
            subscriptions: heapless::Vec::new(),
        }
    }

    pub fn add_filter(&mut self, filter: TopicSubscription<MAX_TOPIC_NAME_LENGTH>) -> Result<()> {
        self.subscriptions
            .push(filter)
            .map_err(|_| Error::MaxSubscriptionsReached {
                max_subscriptions: MAX_SUBSCRIPTIONS_PER_CLIENT,
            })
    }

    pub fn remove_filter(&mut self, filter: &TopicSubscription<MAX_TOPIC_NAME_LENGTH>) -> bool {
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
pub struct TopicRegistry<const MAX_CLIENT_NAME_LENGTH: usize, const MAX_TOPIC_NAME_LENGTH: usize, const MAX_SUBSCRIPTIONS_PER_CLIENT: usize, const MAX_CLIENTS: usize> {
    clients: heapless::Vec<ClientSubscriptions<MAX_CLIENT_NAME_LENGTH, MAX_TOPIC_NAME_LENGTH, MAX_SUBSCRIPTIONS_PER_CLIENT>, MAX_CLIENTS>,
}

impl<const MAX_CLIENT_NAME_LENGTH: usize, const MAX_TOPIC_NAME_LENGTH: usize, const MAX_SUBSCRIPTIONS_PER_CLIENT: usize, const MAX_CLIENTS: usize> TopicRegistry<MAX_CLIENT_NAME_LENGTH, MAX_TOPIC_NAME_LENGTH, MAX_SUBSCRIPTIONS_PER_CLIENT, MAX_CLIENTS> {
    /// Create a new topic registry
    pub const fn new() -> Self {
        Self {
            clients: heapless::Vec::new(),
        }
    }

    /// Get all subscribers for a topic
    pub fn get_subscribers(&self, topic: TopicName<MAX_TOPIC_NAME_LENGTH>) -> heapless::Vec<&ClientName<MAX_CLIENT_NAME_LENGTH>, MAX_CLIENTS> {
        let mut subscribers = heapless::Vec::<&ClientName<MAX_CLIENT_NAME_LENGTH>, MAX_CLIENTS>::new();

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
    pub fn subscribe(&mut self, client: ClientName<MAX_CLIENT_NAME_LENGTH>, sub: TopicSubscription<MAX_TOPIC_NAME_LENGTH>) -> Result<()> {
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
    pub fn unsubscribe(&mut self, client: ClientName<MAX_CLIENT_NAME_LENGTH>, sub: TopicSubscription<MAX_TOPIC_NAME_LENGTH>) -> bool {
        if let Some(client_subs) = self.clients.iter_mut().find(|cs| cs.client_name == client) {
            client_subs.remove_filter(&sub)
        } else {
            false
        }
    }

    /// Unsubscribe a client from all topics
    pub fn unregister_client(&mut self, client: ClientName<MAX_CLIENT_NAME_LENGTH>) {
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
