use crate::protocol::heapless::PushError;
use crate::protocol::ProtocolError;
use crate::traits::NetworkError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BrokerError {
    SessionAlreadyExists { session_id: u128 },

    SessionNotFound { session_id: u128 },

    SessionQueueFull { session_id: u128, queue_size: usize },

    SessionDisconnected { session_id: u128 },

    MaxSessionsReached { current: usize, max: usize },

    MaxSubscribersPerTopicReached { current: usize, max: usize },

    MaxTopicsReached { current: usize, max: usize },

    Network(NetworkError),

    Protocol(ProtocolError),

    BufferFull,
}

impl core::fmt::Display for BrokerError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BrokerError::SessionAlreadyExists { session_id } => {
                write!(f, "Session {} already exists", session_id)
            }
            BrokerError::SessionNotFound { session_id } => {
                write!(f, "Session {} not found", session_id)
            }
            BrokerError::SessionQueueFull {
                session_id,
                queue_size,
            } => {
                write!(
                    f,
                    "Session {} queue full (size: {})",
                    session_id, queue_size
                )
            }
            BrokerError::SessionDisconnected { session_id } => {
                write!(f, "Session {} disconnected", session_id)
            }
            BrokerError::MaxSessionsReached { current, max } => {
                write!(f, "Maximum sessions reached: {}/{}", current, max)
            }
            BrokerError::MaxSubscribersPerTopicReached { current, max } => {
                write!(
                    f,
                    "Maximum subscribers per topic reached: {}/{}",
                    current, max
                )
            }
            BrokerError::MaxTopicsReached { current, max } => {
                write!(f, "Maximum topics reached: {}/{}", current, max)
            }
            BrokerError::Network(error) => {
                write!(f, "Network error: {}", error)
            }
            BrokerError::Protocol(error) => {
                write!(f, "Protocol error: {}", error)
            }
            BrokerError::BufferFull => {
                write!(f, "Buffer full")
            }
        }
    }
}

impl core::error::Error for BrokerError {}

impl From<NetworkError> for BrokerError {
    fn from(error: NetworkError) -> Self {
        BrokerError::Network(error)
    }
}

impl From<ProtocolError> for BrokerError {
    fn from(error: ProtocolError) -> Self {
        BrokerError::Protocol(error)
    }
}

impl From<PushError> for BrokerError {
    fn from(_error: PushError) -> Self {
        BrokerError::BufferFull
    }
}
