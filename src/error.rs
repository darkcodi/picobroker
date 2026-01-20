//! Error types for PicoBroker
//!
//! no_std compatible error handling with defmt support

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// I/O error from network stack
    Io,
    /// Invalid MQTT packet format
    InvalidPacket,
    /// Unsupported or unknown packet type
    UnsupportedPacketType,
    /// Buffer too small for operation
    BufferTooSmall,
    /// Malformed UTF-8 string
    MalformedString,
    /// Client keep-alive timeout
    KeepAliveTimeout,
    /// Topic length exceeded maximum allowed length
    TopicLengthExceeded { max_length: usize, actual_length: usize },
    /// Maximum number of subscriptions reached for a client
    MaxSubscriptionsReached { max_subscriptions: usize },
    /// Maximum number of clients reached
    MaxClientsReached { max_clients: usize },
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::Io => write!(f, "I/O error"),
            Error::InvalidPacket => write!(f, "Invalid MQTT packet"),
            Error::UnsupportedPacketType => write!(f, "Unsupported packet type"),
            Error::BufferTooSmall => write!(f, "Buffer too small"),
            Error::MalformedString => write!(f, "Malformed UTF-8 string"),
            Error::KeepAliveTimeout => write!(f, "Keep-alive timeout"),
            Error::TopicLengthExceeded { max_length, actual_length } => {
                write!(f, "Topic length exceeded: max {}, actual {}", max_length, actual_length)
            },
            Error::MaxSubscriptionsReached { max_subscriptions: max_filters } => {
                write!(f, "Maximum number of subscriptions reached for client: max {}", max_filters)
            },
            Error::MaxClientsReached { max_clients } => {
                write!(f, "Maximum number of clients reached: max {}", max_clients)
            },
        }
    }
}

impl defmt::Format for Error {
    fn format(&self, f: defmt::Formatter) {
        match self {
            Error::Io => defmt::write!(f, "I/O error"),
            Error::InvalidPacket => defmt::write!(f, "Invalid MQTT packet"),
            Error::UnsupportedPacketType => defmt::write!(f, "Unsupported packet type"),
            Error::BufferTooSmall => defmt::write!(f, "Buffer too small"),
            Error::MalformedString => defmt::write!(f, "Malformed UTF-8 string"),
            Error::KeepAliveTimeout => defmt::write!(f, "Keep-alive timeout"),
            Error::TopicLengthExceeded { max_length, actual_length } => {
                defmt::write!(f, "Topic length exceeded: max {}, actual {}", max_length, actual_length)
            },
            Error::MaxSubscriptionsReached { max_subscriptions: max_filters } => {
                defmt::write!(f, "Maximum number of subscriptions reached for client: max {}", max_filters)
            },
            Error::MaxClientsReached { max_clients } => {
                defmt::write!(f, "Maximum number of clients reached: max {}", max_clients)
            }
        }
    }
}

impl core::error::Error for Error {}

pub type Result<T> = core::result::Result<T, Error>;
