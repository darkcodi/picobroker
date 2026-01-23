use crate::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacketEncodingError {
    /// Buffer too small for packet
    BufferTooSmall,
    /// Incomplete packet (not enough data)
    IncompletePacket,
    /// Packet length does not match expected length
    InvalidPacketLength { expected: usize, actual: usize },
    /// Invalid packet type
    InvalidPacketType { packet_type: u8 },
    /// Invalid protocol name in CONNECT
    InvalidProtocolName,
    /// Packet is malformed
    MalformedPacket,
    /// Unsupported protocol level in CONNECT
    UnsupportedProtocolLevel { level: u8 },
    Other,
}

impl core::fmt::Display for PacketEncodingError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PacketEncodingError::BufferTooSmall => write!(f, "Buffer too small for packet"),
            PacketEncodingError::IncompletePacket => write!(f, "Incomplete packet"),
            PacketEncodingError::InvalidPacketLength { expected, actual } => {
                write!(f, "Invalid packet length: expected {}, got {}", expected, actual)
            },
            PacketEncodingError::InvalidPacketType { packet_type } => {
                write!(f, "Invalid packet type: {}", packet_type)
            },
            PacketEncodingError::InvalidProtocolName => write!(f, "Invalid protocol name in CONNECT packet"),
            PacketEncodingError::MalformedPacket => write!(f, "Malformed packet"),
            PacketEncodingError::UnsupportedProtocolLevel { level } => {
                write!(f, "Unsupported protocol level in CONNECT packet: {}", level)
            },
            PacketEncodingError::Other => write!(f, "An unspecified packet encoding error occurred"),
        }
    }
}

impl From<Error> for PacketEncodingError {
    fn from(_error: Error) -> Self {
        PacketEncodingError::Other
    }
}