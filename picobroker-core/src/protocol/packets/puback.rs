use crate::protocol::packets::PacketEncoder;
use crate::{Error, PacketType};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PubAckPacket {
    pub packet_id: u16,
}

impl PacketEncoder for PubAckPacket {
    fn packet_type(&self) -> PacketType {
        PacketType::PubAck
    }

    fn fixed_flags(&self) -> u8 {
        0b0000
    }

    fn encode(&self, buffer: &mut [u8]) -> Result<usize, Error> {
        if buffer.len() < 2 {
            return Err(Error::BufferTooSmall);
        }
        let pid_bytes = self.packet_id.to_be_bytes();
        buffer[0] = pid_bytes[0];
        buffer[1] = pid_bytes[1];
        Ok(2)
    }

    fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() < 2 {
            return Err(Error::IncompletePacket);
        }
        let packet_id = u16::from_be_bytes([bytes[0], bytes[1]]);

        // MQTT 3.1.1 spec: Packet Identifier MUST be non-zero
        if packet_id == 0 {
            return Err(Error::MalformedPacket);
        }

        Ok(Self {
            packet_id,
        })
    }
}
