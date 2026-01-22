use crate::protocol::packets::PacketEncoder;
use crate::{Error, PacketType};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PingReqPacket;

impl PacketEncoder for PingReqPacket {
    fn packet_type(&self) -> PacketType {
        PacketType::PingReq
    }

    fn fixed_flags(&self) -> u8 {
        0b0000
    }

    fn encode(&self, _buffer: &mut [u8]) -> Result<usize, Error> {
        Ok(0)
    }

    fn decode(_payload: &[u8], _header: u8) -> Result<Self, Error> {
        Ok(Self::default())
    }
}
