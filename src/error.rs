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
    /// Maximum number of clients reached
    MaxClientsReached,
    /// Maximum number of unique topics reached
    MaxTopicsReached,
    /// Maximum subscribers per topic reached
    MaxSubscribersReached,
    /// Topic registry is full
    TopicRegistryFull,
    /// Buffer too small for operation
    BufferTooSmall,
    /// Malformed UTF-8 string
    MalformedString,
    /// Client keep-alive timeout
    KeepAliveTimeout,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::Io => write!(f, "I/O error"),
            Error::InvalidPacket => write!(f, "Invalid MQTT packet"),
            Error::UnsupportedPacketType => write!(f, "Unsupported packet type"),
            Error::MaxClientsReached => write!(f, "Maximum clients reached"),
            Error::MaxTopicsReached => write!(f, "Maximum topics reached"),
            Error::MaxSubscribersReached => write!(f, "Maximum subscribers per topic"),
            Error::TopicRegistryFull => write!(f, "Topic registry full"),
            Error::BufferTooSmall => write!(f, "Buffer too small"),
            Error::MalformedString => write!(f, "Malformed UTF-8 string"),
            Error::KeepAliveTimeout => write!(f, "Keep-alive timeout"),
        }
    }
}

impl defmt::Format for Error {
    fn format(&self, f: defmt::Formatter) {
        match self {
            Error::Io => defmt::write!(f, "I/O error"),
            Error::InvalidPacket => defmt::write!(f, "Invalid MQTT packet"),
            Error::UnsupportedPacketType => defmt::write!(f, "Unsupported packet type"),
            Error::MaxClientsReached => defmt::write!(f, "Maximum clients reached"),
            Error::MaxTopicsReached => defmt::write!(f, "Maximum topics reached"),
            Error::MaxSubscribersReached => defmt::write!(f, "Maximum subscribers per topic"),
            Error::TopicRegistryFull => defmt::write!(f, "Topic registry full"),
            Error::BufferTooSmall => defmt::write!(f, "Buffer too small"),
            Error::MalformedString => defmt::write!(f, "Malformed UTF-8 string"),
            Error::KeepAliveTimeout => defmt::write!(f, "Keep-alive timeout"),
        }
    }
}

impl core::error::Error for Error {}

pub type Result<T> = core::result::Result<T, Error>;
