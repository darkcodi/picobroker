use crate::protocol::packets::PacketEncoder;
use crate::{Error, PacketType};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DisconnectPacket;

impl<'a> PacketEncoder<'a> for DisconnectPacket {
    fn packet_type(&self) -> PacketType {
        PacketType::Disconnect
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default() {
        let disconnect = DisconnectPacket::default();
        assert_eq!(disconnect, DisconnectPacket);
    }
}
