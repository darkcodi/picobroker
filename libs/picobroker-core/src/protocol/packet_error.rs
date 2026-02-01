#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProtocolError {
    
    BufferTooSmall { buffer_size: usize },
    
    ClientIdEmpty,
    
    ClientIdLengthExceeded {
        max_length: usize,
        actual_length: usize,
    },
    
    IncompletePacket { buffer_size: usize },
    
    InvalidConnectFlags { flags: u8 },
    
    InvalidConnectReturnCode { return_code: u8 },
    
    InvalidPacketLength { expected: usize, actual: usize },
    
    InvalidPacketType { packet_type: u8 },
    
    InvalidProtocolName,
    
    InvalidSessionPresentFlag { flag: u8 },
    
    InvalidUtf8String,
    
    MissingPacketId,
    
    PasswordLengthExceeded {
        max_length: usize,
        actual_length: usize,
    },
    
    PayloadTooLarge { max_size: usize, actual_size: usize },
    
    InvalidQosLevel { level: u8 },
    
    TopicEmpty,
    
    TopicNameLengthExceeded {
        max_length: usize,
        actual_length: usize,
    },
    
    UnsupportedProtocolLevel { level: u8 },
    
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
            ProtocolError::IncompletePacket { buffer_size } => {
                write!(f, "Incomplete packet: buffer size {}", buffer_size)
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
