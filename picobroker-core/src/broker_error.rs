//! Error types for PicoBroker
//!
//! no_std compatible error handling

use crate::protocol::packet_error::PacketEncodingError;
use crate::traits::NetworkError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrokerError {
    /// Session with the given ID is already exists
    SessionAlreadyExists,
    /// Session with the given ID was not found
    SessionNotFound,
    /// Session's message queue is full
    SessionQueueFull { queue_size: usize },
    /// Maximum number of sessions reached
    MaxSessionsReached { max_sessions: usize },
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
            BrokerError::SessionAlreadyExists => {
                write!(f, "Session with the given ID is already exists")
            }
            BrokerError::SessionNotFound => {
                write!(f, "Session with the given ID was not found")
            }
            BrokerError::SessionQueueFull { queue_size } => {
                write!(f, "Session's message queue is full (size: {})", queue_size)
            }
            BrokerError::MaxSessionsReached { max_sessions } => {
                write!(f, "Maximum number of sessions reached: {}", max_sessions)
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
