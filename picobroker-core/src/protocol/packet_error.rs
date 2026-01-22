use crate::Error;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PacketEncodingError {
    /// Invalid packet type
    InvalidPacketType { packet_type: u8 },
    Other,
}

impl core::fmt::Display for PacketEncodingError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            PacketEncodingError::InvalidPacketType { packet_type } => {
                write!(f, "Invalid packet type: {}", packet_type)
            },
            PacketEncodingError::Other => {
                write!(f, "An unspecified packet encoding error occurred")
            },
        }
    }
}

impl From<Error> for PacketEncodingError {
    fn from(_error: Error) -> Self {
        PacketEncodingError::Other
    }
}