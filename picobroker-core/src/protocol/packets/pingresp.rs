use crate::protocol::packets::PacketEncoder;
use crate::{PacketEncodingError, PacketType};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PingRespPacket;

impl PacketEncoder for PingRespPacket {
    fn packet_type(&self) -> PacketType {
        PacketType::PingResp
    }

    fn fixed_flags(&self) -> u8 {
        0b0000
    }

    fn encode(&self, _buffer: &mut [u8]) -> Result<usize, PacketEncodingError> {
        Ok(0)
    }

    fn decode(_bytes: &[u8]) -> Result<Self, PacketEncodingError> {
        Ok(Self::default())
    }
}
