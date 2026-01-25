use crate::PacketEncodingError;

#[repr(u8)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum QoS {
    AtMostOnce = 0,
    AtLeastOnce = 1,
    ExactlyOnce = 2,
}

impl QoS {
    pub const fn from_u8(value: u8) -> Result<Self, PacketEncodingError> {
        match value {
            0 => Ok(QoS::AtMostOnce),
            1 => Ok(QoS::AtLeastOnce),
            2 => Ok(QoS::ExactlyOnce),
            _ => Err(PacketEncodingError::InvalidQosLevel { level: value }),
        }
    }
}
