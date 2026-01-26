//! Error types for PicoBroker
//!
//! no_std compatible error handling

use crate::protocol::packet_error::PacketEncodingError;
use crate::traits::NetworkError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrokerError {
    /// Client with the given ID is already connected
    ClientAlreadyConnected,
    /// Client with the given ID was not found
    ClientNotFound,
    /// Client's message queue is full
    ClientQueueFull { queue_size: usize },
    /// Maximum number of clients reached
    MaxClientsReached { max_clients: usize },
    /// Maximum number of subscribers per topic reached
    MaxSubscribersPerTopicReached { max_subscribers: usize },
    /// Maximum number of topics reached
    MaxTopicsReached { max_topics: usize },
    /// Network error occurred
    NetworkError { error: NetworkError },
    /// Packet encoding/decoding error occurred
    PacketEncodingError { error: PacketEncodingError },
}

impl core::fmt::Display for BrokerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BrokerError::ClientAlreadyConnected => {
                write!(f, "Client with the given ID is already connected")
            }
            BrokerError::ClientNotFound => {
                write!(f, "Client with the given ID was not found")
            }
            BrokerError::ClientQueueFull { queue_size } => {
                write!(f, "Client's message queue is full (size: {})", queue_size)
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
            BrokerError::NetworkError { error } => {
                write!(f, "Network error occurred: {}", error)
            }
            BrokerError::PacketEncodingError { error } => {
                write!(f, "Packet encoding/decoding error occurred: {}", error)
            }
        }
    }
}

impl core::error::Error for BrokerError {}

impl From<NetworkError> for BrokerError {
    fn from(error: NetworkError) -> Self {
        BrokerError::NetworkError { error }
    }
}
