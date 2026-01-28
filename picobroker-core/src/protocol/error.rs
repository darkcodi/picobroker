#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolError {
    /// Buffer too small for packet
    BufferTooSmall { buffer_size: usize },
    /// Client identifier is empty in CONNECT, which is not allowed in persistent session
    ClientIdEmpty,
    /// Client identifier length exceeded maximum allowed length
    ClientIdLengthExceeded {
        max_length: usize,
        actual_length: usize,
    },
    /// Incomplete packet (not enough data)
    IncompletePacket { available: usize },
    /// Invalid connect flags in CONNECT
    InvalidConnectFlags { flags: u8 },
    /// Invalid connect return code in CONNACK
    InvalidConnectReturnCode { return_code: u8 },
    /// Packet length does not match expected length
    InvalidPacketLength { expected: usize, actual: usize },
    /// Invalid packet type
    InvalidPacketType { packet_type: u8 },
    /// Invalid protocol name in CONNECT
    InvalidProtocolName,
    /// Invalid session present flag in CONNACK
    InvalidSessionPresentFlag { flag: u8 },
    /// Invalid UTF-8 string
    InvalidUtf8String,
    /// Missing Packet Identifier where one is required
    MissingPacketId,
    /// Password length exceeded maximum allowed length
    PasswordLengthExceeded {
        max_length: usize,
        actual_length: usize,
    },
    /// Payload size exceeded maximum allowed size
    PayloadTooLarge { max_size: usize, actual_size: usize },
    /// Invalid QoS level
    InvalidQosLevel { level: u8 },
    /// Topic name is empty
    TopicEmpty,
    /// Topic name length exceeded maximum allowed length
    TopicNameLengthExceeded {
        max_length: usize,
        actual_length: usize,
    },
    /// Unsupported protocol level in CONNECT
    UnsupportedProtocolLevel { level: u8 },
    /// Username length exceeded maximum allowed length
    UsernameLengthExceeded {
        max_length: usize,
        actual_length: usize,
    },
}

impl core::fmt::Display for ProtocolError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ProtocolError::BufferTooSmall { buffer_size } => {
                write!(f, "Buffer too small for packet: size {}", buffer_size)
            }
            ProtocolError::ClientIdEmpty => write!(f, "Client ID is empty in CONNECT packet"),
            ProtocolError::ClientIdLengthExceeded {
                max_length,
                actual_length,
            } => {
                write!(
                    f,
                    "Client ID length exceeded: length {}, max {}",
                    actual_length, max_length
                )
            }
            ProtocolError::IncompletePacket { available } => {
                write!(f, "Incomplete packet: available {}", available)
            }
            ProtocolError::InvalidConnectFlags { flags } => {
                write!(f, "Invalid connect flags in CONNECT packet: {}", flags)
            }
            ProtocolError::InvalidConnectReturnCode { return_code } => {
                write!(f, "Invalid connect return code in CONNACK: {}", return_code)
            }
            ProtocolError::InvalidPacketLength { expected, actual } => {
                write!(
                    f,
                    "Invalid packet length: expected {}, got {}",
                    expected, actual
                )
            }
            ProtocolError::InvalidPacketType { packet_type } => {
                write!(f, "Invalid packet type: {}", packet_type)
            }
            ProtocolError::InvalidProtocolName => {
                write!(f, "Invalid protocol name in CONNECT packet")
            }
            ProtocolError::InvalidSessionPresentFlag { flag } => {
                write!(
                    f,
                    "Invalid session present flag in CONNACK packet: {}",
                    flag
                )
            }
            ProtocolError::InvalidUtf8String => write!(f, "Invalid UTF-8 string"),
            ProtocolError::MissingPacketId => {
                write!(f, "Missing Packet Identifier where one is required")
            }
            ProtocolError::PasswordLengthExceeded {
                max_length,
                actual_length,
            } => {
                write!(
                    f,
                    "Password length exceeded: length {}, max {}",
                    actual_length, max_length
                )
            }
            ProtocolError::PayloadTooLarge {
                max_size,
                actual_size,
            } => {
                write!(
                    f,
                    "Payload too large: size {}, max {}",
                    actual_size, max_size
                )
            }
            ProtocolError::InvalidQosLevel { level } => {
                write!(f, "Invalid QoS level: {}", level)
            }
            ProtocolError::TopicEmpty => write!(f, "Topic name is empty"),
            ProtocolError::TopicNameLengthExceeded {
                max_length,
                actual_length,
            } => {
                write!(
                    f,
                    "Topic name length exceeded: length {}, max {}",
                    actual_length, max_length
                )
            }
            ProtocolError::UnsupportedProtocolLevel { level } => {
                write!(f, "Unsupported protocol level in CONNECT packet: {}", level)
            }
            ProtocolError::UsernameLengthExceeded {
                max_length,
                actual_length,
            } => {
                write!(
                    f,
                    "Username length exceeded: length {}, max {}",
                    actual_length, max_length
                )
            }
        }
    }
}

impl core::error::Error for ProtocolError {}
