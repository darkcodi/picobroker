//! Error types for PicoBroker
//!
//! no_std compatible error handling

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrokerError {
    /// Client with the given ID is already connected
    ClientAlreadyConnected,
    /// Maximum number of clients reached
    MaxClientsReached { max_clients: usize },
    /// Maximum number of subscribers per topic reached
    MaxSubscribersPerTopicReached { max_subscribers: usize },
    /// Maximum number of topics reached
    MaxTopicsReached { max_topics: usize },
}

impl core::fmt::Display for BrokerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BrokerError::ClientAlreadyConnected => {
                write!(f, "Client with the given ID is already connected")
            }
            BrokerError::MaxClientsReached { max_clients } => {
                write!(f, "Maximum number of clients reached: {}", max_clients)
            }
            BrokerError::MaxSubscribersPerTopicReached { max_subscribers } => {
                write!(
                    f,
                    "Maximum number of subscribers per topic reached: {}",
                    max_subscribers
                )
            }
            BrokerError::MaxTopicsReached { max_topics } => {
                write!(f, "Maximum number of topics reached: {}", max_topics)
            }
        }
    }
}

impl core::error::Error for BrokerError {}
