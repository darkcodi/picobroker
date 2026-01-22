use crate::protocol::packets::PacketEncoder;
use crate::{Error, PacketType};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PubComp {
    pub packet_id: u16,
}

impl<'a> PacketEncoder<'a> for PubComp {
    fn packet_type(&self) -> PacketType {
        PacketType::PubComp
    }

    fn fixed_flags(&'a self) -> u8 {
        0b0000
    }

    fn encode(&'a self, buffer: &mut [u8]) -> Result<usize, Error> {
        if buffer.len() < 2 {
            return Err(Error::BufferTooSmall);
        }
        let bytes = self.packet_id.to_be_bytes();
        buffer[0] = bytes[0];
        buffer[1] = bytes[1];
        Ok(2)
    }

    fn decode(payload: &'a [u8], _header: u8) -> Result<Self, Error> {
        if payload.len() < 2 {
            return Err(Error::IncompletePacket);
        }
        let packet_id = u16::from_be_bytes([payload[0], payload[1]]);

        // MQTT 3.1.1 spec: Packet Identifier MUST be non-zero
        if packet_id == 0 {
            return Err(Error::MalformedPacket);
        }

        Ok(Self {
            packet_id,
        })
    }
}
