use crate::protocol::packets::PacketEncoder;
use crate::{Error, PacketEncodingError, PacketType};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PubRelPacket {
    pub packet_id: u16,
}

impl PacketEncoder for PubRelPacket {
    fn packet_type(&self) -> PacketType {
        PacketType::PubRel
    }

    fn fixed_flags(&self) -> u8 {
        0b0010
    }

    fn encode(&self, buffer: &mut [u8]) -> Result<usize, PacketEncodingError> {
        if buffer.len() < 2 {
            return Err(Error::BufferTooSmall.into());
        }
        let bytes = self.packet_id.to_be_bytes();
        buffer[0] = bytes[0];
        buffer[1] = bytes[1];
        Ok(2)
    }

    fn decode(bytes: &[u8]) -> Result<Self, PacketEncodingError> {
        if bytes.len() < 2 {
            return Err(Error::IncompletePacket.into());
        }
        let packet_id = u16::from_be_bytes([bytes[0], bytes[1]]);

        // MQTT 3.1.1 spec: Packet Identifier MUST be non-zero
        if packet_id == 0 {
            return Err(Error::MalformedPacket.into());
        }

        Ok(Self {
            packet_id,
        })
    }
}
