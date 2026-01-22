use crate::protocol::packets::{PacketEncoder, PacketFlagsConst, PacketTypeConst};
use crate::{PacketEncodingError, PacketType};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PingRespPacket;

impl PacketTypeConst for PingRespPacket {
    const PACKET_TYPE: PacketType = PacketType::PingResp;
}

impl PacketFlagsConst for PingRespPacket {
    const PACKET_FLAGS: u8 = 0b0000;
}

impl PacketEncoder for PingRespPacket {
    fn encode(&self, _buffer: &mut [u8]) -> Result<usize, PacketEncodingError> {
        Ok(0)
    }

    fn decode(_bytes: &[u8]) -> Result<Self, PacketEncodingError> {
        Ok(Self::default())
    }
}
