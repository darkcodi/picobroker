use crate::protocol::packets::PacketEncoder;
use crate::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PubRel<'a> {
    pub packet_id: u16,
    _phantom: core::marker::PhantomData<&'a ()>,
}

impl<'a> PacketEncoder<'a> for PubRel<'a> {
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
        if packet_id == 0 {
            return Err(Error::MalformedPacket);
        }
        Ok(Self {
            packet_id,
            _phantom: Default::default(),
        })
    }
}
