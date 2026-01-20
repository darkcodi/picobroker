//! Error types for PicoBroker
//!
//! no_std compatible error handling with defmt support

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// Topic length exceeded maximum allowed length
    TopicLengthExceeded { max_length: usize, actual_length: usize },
    /// Maximum number of subscriptions reached for a client
    MaxSubscriptionsReached { max_subscriptions: usize },
    /// Maximum number of clients reached
    MaxClientsReached { max_clients: usize },
    /// Invalid QoS level in PUBLISH packet
    InvalidPublishQoS { invalid_qos: u8 },
    /// Invalid fixed header flags for a packet type
    InvalidFixedHeaderFlags { expected: u8, actual: u8 },
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::TopicLengthExceeded { max_length, actual_length } => {
                write!(f, "Topic length exceeded: max {}, actual {}", max_length, actual_length)
            },
            Error::MaxSubscriptionsReached { max_subscriptions: max_filters } => {
                write!(f, "Maximum number of subscriptions reached for client: max {}", max_filters)
            },
            Error::MaxClientsReached { max_clients } => {
                write!(f, "Maximum number of clients reached: max {}", max_clients)
            },
            Error::InvalidPublishQoS { invalid_qos} => {
                write!(f, "Invalid QoS level in PUBLISH packet: {}", invalid_qos)
            },
            Error::InvalidFixedHeaderFlags { expected, actual } => {
                write!(f, "Invalid fixed header flags: expected {:04b}, actual {:04b}", expected, actual)
            },
        }
    }
}

impl defmt::Format for Error {
    fn format(&self, f: defmt::Formatter) {
        match self {
            Error::TopicLengthExceeded { max_length, actual_length } => {
                defmt::write!(f, "Topic length exceeded: max {}, actual {}", max_length, actual_length)
            },
            Error::MaxSubscriptionsReached { max_subscriptions: max_filters } => {
                defmt::write!(f, "Maximum number of subscriptions reached for client: max {}", max_filters)
            },
            Error::MaxClientsReached { max_clients } => {
                defmt::write!(f, "Maximum number of clients reached: max {}", max_clients)
            },
            Error::InvalidPublishQoS { invalid_qos} => {
                defmt::write!(f, "Invalid QoS level in PUBLISH packet: {}", invalid_qos)
            },
            Error::InvalidFixedHeaderFlags { expected, actual } => {
                defmt::write!(f, "Invalid fixed header flags: expected {:04b}, actual {:04b}", expected, actual)
            },
        }
    }
}

impl core::error::Error for Error {}

pub type Result<T> = core::result::Result<T, Error>;
