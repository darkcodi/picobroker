//! Error types for PicoBroker
//!
//! no_std compatible error handling

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Error {
    /// Client name length exceeded maximum allowed length
    ClientNameLengthExceeded {
        max_length: usize,
        actual_length: usize,
    },
    /// Topic name length exceeded maximum allowed length
    TopicNameLengthExceeded {
        max_length: usize,
        actual_length: usize,
    },
    /// Maximum number of subscriptions reached for a client
    MaxSubscriptionsReached { max_subscriptions: usize },
    /// Maximum number of clients reached
    MaxClientsReached { max_clients: usize },
    /// Maximum number of topics reached
    MaxTopicsReached { max_topics: usize },
    /// Maximum number of subscribers per topic reached
    MaxSubscribersPerTopicReached { max_subscribers: usize },
    /// Invalid QoS level in PUBLISH packet
    InvalidPublishQoS { invalid_qos: u8 },
    /// Invalid QoS level in SUBACK packet
    InvalidSubAckQoS { invalid_qos: u8 },
    /// Invalid fixed header flags for a packet type
    InvalidFixedHeaderFlags { expected: u8, actual: u8 },
    /// Malformed MQTT packet
    MalformedPacket,
    /// Invalid packet type
    InvalidPacketType { packet_type: u8 },
    /// Invalid variable length integer encoding
    InvalidLengthEncoding,
    /// Buffer too small for packet
    BufferTooSmall,
    /// Invalid UTF-8 in string field
    InvalidUtf8,
    /// Incomplete packet (not enough data)
    IncompletePacket,
    /// Invalid protocol name in CONNECT
    InvalidProtocolName,
    /// Unsupported protocol level in CONNECT
    UnsupportedProtocolLevel { level: u8 },
    /// Invalid protocol level in CONNECT
    InvalidProtocolLevel { level: u8 },
    /// Connect flags invalid
    InvalidConnectFlags,
    /// Topic name cannot be empty
    EmptyTopic,
    /// Invalid client ID length
    InvalidClientIdLength { length: u16 },
    /// Client ID length exceeded maximum allowed length
    ClientIdLengthExceeded { max_length: usize, actual_length: usize },
    /// Packet size exceeded maximum allowed size
    PacketTooLarge { max_size: usize, actual_size: usize },
    /// Network I/O error
    IoError,
    /// Keep-alive timeout expired
    KeepAliveTimeout,
    /// Client already connected with this ID
    ClientAlreadyConnected,
    /// Client not found in registry
    ClientNotFound,
    /// Connection accept failed
    AcceptError,
    /// Failed to bind to address
    BindError,
    /// Message queue is full (QoS 0: message dropped)
    QueueFull,
}

impl core::fmt::Display for Error {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Error::ClientNameLengthExceeded {
                max_length,
                actual_length,
            } => {
                write!(
                    f,
                    "Client name length exceeded: max {}, actual {}",
                    max_length, actual_length
                )
            }
            Error::TopicNameLengthExceeded {
                max_length,
                actual_length,
            } => {
                write!(
                    f,
                    "Topic name length exceeded: max {}, actual {}",
                    max_length, actual_length
                )
            }
            Error::MaxSubscriptionsReached {
                max_subscriptions: max_filters,
            } => {
                write!(
                    f,
                    "Maximum number of subscriptions reached for client: max {}",
                    max_filters
                )
            }
            Error::MaxClientsReached { max_clients } => {
                write!(f, "Maximum number of clients reached: max {}", max_clients)
            }
            Error::MaxTopicsReached { max_topics } => {
                write!(f, "Maximum number of topics reached: max {}", max_topics)
            }
            Error::MaxSubscribersPerTopicReached { max_subscribers } => {
                write!(
                    f,
                    "Maximum number of subscribers per topic reached: max {}",
                    max_subscribers
                )
            }
            Error::InvalidPublishQoS { invalid_qos } => {
                write!(f, "Invalid QoS level in PUBLISH packet: {}", invalid_qos)
            }
            Error::InvalidSubAckQoS { invalid_qos } => {
                write!(f, "Invalid QoS level in SUBACK packet: {}", invalid_qos)
            }
            Error::InvalidFixedHeaderFlags { expected, actual } => {
                write!(
                    f,
                    "Invalid fixed header flags: expected {:04b}, actual {:04b}",
                    expected, actual
                )
            }
            Error::MalformedPacket => write!(f, "Malformed MQTT packet"),
            Error::InvalidPacketType { packet_type } => {
                write!(f, "Invalid packet type: {}", packet_type)
            }
            Error::InvalidLengthEncoding => write!(f, "Invalid variable length integer encoding"),
            Error::BufferTooSmall => write!(f, "Buffer too small for packet"),
            Error::InvalidUtf8 => write!(f, "Invalid UTF-8 in string field"),
            Error::IncompletePacket => write!(f, "Incomplete packet (not enough data)"),
            Error::InvalidProtocolName => write!(f, "Invalid protocol name in CONNECT"),
            Error::UnsupportedProtocolLevel { level } => {
                write!(f, "Unsupported protocol level in CONNECT: {}", level)
            }
            Error::InvalidProtocolLevel { level } => {
                write!(f, "Invalid protocol level in CONNECT: {}", level)
            }
            Error::InvalidConnectFlags => write!(f, "Invalid connect flags"),
            Error::EmptyTopic => write!(f, "Topic name cannot be empty"),
            Error::InvalidClientIdLength { length } => {
                write!(f, "Invalid client ID length: {}", length)
            }
            Error::ClientIdLengthExceeded {
                max_length,
                actual_length,
            } => {
                write!(
                    f,
                    "Client ID length exceeded: max {}, actual {}",
                    max_length, actual_length
                )
            }
            Error::PacketTooLarge {
                max_size,
                actual_size,
            } => {
                write!(
                    f,
                    "Packet too large: max {} bytes, actual {} bytes",
                    max_size, actual_size
                )
            }
            Error::IoError => write!(f, "Network I/O error"),
            Error::KeepAliveTimeout => write!(f, "Keep-alive timeout expired"),
            Error::ClientAlreadyConnected => write!(f, "Client already connected with this ID"),
            Error::ClientNotFound => write!(f, "Client not found in registry"),
            Error::AcceptError => write!(f, "Connection accept failed"),
            Error::BindError => write!(f, "Failed to bind to address"),
            Error::QueueFull => write!(f, "Message queue is full"),
        }
    }
}

impl core::error::Error for Error {}

pub type Result<T> = core::result::Result<T, Error>;
