use crate::protocol::packets::PacketEncoder;
use crate::protocol::qos::QoS;
use crate::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubAck<'a> {
    pub packet_id: u16,
    pub granted_qos: QoS,
    _phantom: core::marker::PhantomData<&'a ()>,
}

impl<'a> PacketEncoder<'a> for SubAck<'a> {
    fn encode(&'a self, buffer: &mut [u8]) -> Result<usize, Error> {
        if buffer.len() < 3 {
            return Err(Error::BufferTooSmall);
        }
        let pid_bytes = self.packet_id.to_be_bytes();
        buffer[0] = pid_bytes[0];
        buffer[1] = pid_bytes[1];
        buffer[2] = self.granted_qos as u8;
        Ok(3)
    }

    fn decode(payload: &'a [u8], _header: u8) -> Result<Self, Error> {
        if payload.len() < 3 {
            return Err(Error::IncompletePacket);
        }
        let packet_id = u16::from_be_bytes([payload[0], payload[1]]);
        let qos_value = payload[2];
        let granted_qos = QoS::from_u8(qos_value).ok_or(Error::InvalidSubAckQoS {
            invalid_qos: qos_value,
        })?;
        Ok(Self {
            packet_id,
            granted_qos,
            _phantom: Default::default(),
        })
    }
}
