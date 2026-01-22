use crate::protocol::packets::PacketEncoder;
use crate::protocol::qos::QoS;
use crate::{Error, PacketEncodingError, PacketType};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubAckPacket {
    pub packet_id: u16,
    pub granted_qos: QoS,
}

impl PacketEncoder for SubAckPacket {
    fn packet_type(&self) -> PacketType {
        PacketType::SubAck
    }

    fn fixed_flags(&self) -> u8 {
        0b0000
    }

    fn encode(&self, buffer: &mut [u8]) -> Result<usize, PacketEncodingError> {
        if buffer.len() < 3 {
            return Err(Error::BufferTooSmall.into());
        }
        let pid_bytes = self.packet_id.to_be_bytes();
        buffer[0] = pid_bytes[0];
        buffer[1] = pid_bytes[1];
        buffer[2] = self.granted_qos as u8;
        Ok(3)
    }

    fn decode(bytes: &[u8]) -> Result<Self, PacketEncodingError> {
        if bytes.len() < 3 {
            return Err(Error::IncompletePacket.into());
        }
        let packet_id = u16::from_be_bytes([bytes[0], bytes[1]]);
        let qos_value = bytes[2];
        let granted_qos = QoS::from_u8(qos_value).ok_or(Error::InvalidSubAckQoS {
            invalid_qos: qos_value,
        })?;
        Ok(Self {
            packet_id,
            granted_qos,
        })
    }
}
