use crate::protocol::packets::PacketEncoder;
use crate::{Error, PacketType};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PingReq;

impl<'a> PacketEncoder<'a> for PingReq {
    fn packet_type(&self) -> PacketType {
        PacketType::PingReq
    }

    fn fixed_flags(&'a self) -> u8 {
        0b0000
    }

    fn encode(&'a self, _buffer: &mut [u8]) -> Result<usize, Error> {
        Ok(0)
    }

    fn decode(_payload: &'a [u8], _header: u8) -> Result<Self, Error> {
        Ok(Self::default())
    }
}
