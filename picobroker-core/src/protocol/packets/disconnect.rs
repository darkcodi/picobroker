use crate::protocol::packets::PacketEncoder;
use crate::{Error, PacketType};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DisconnectPacket;

impl PacketEncoder for DisconnectPacket {
    fn packet_type(&self) -> PacketType {
        PacketType::Disconnect
    }

    fn fixed_flags(&self) -> u8 {
        0b0000
    }

    fn encode(&self, _buffer: &mut [u8]) -> Result<usize, Error> {
        Ok(0)
    }

    fn decode(_bytes: &[u8], _header: u8) -> Result<Self, Error> {
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
