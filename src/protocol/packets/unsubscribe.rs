use crate::protocol::utils::{read_string, write_string};
use crate::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unsubscribe<'a> {
    pub packet_id: u16,
    pub topic_filter: &'a str,
}

impl<'a> Unsubscribe<'a> {
    pub fn new(packet_id: u16) -> Self {
        Self {
            packet_id,
            topic_filter: "",
        }
    }

    pub fn decode(bytes: &'a [u8]) -> Result<Self, Error> {
        let mut offset = 0;
        if offset + 2 > bytes.len() {
            return Err(Error::IncompletePacket);
        }
        let packet_id = u16::from_be_bytes([bytes[offset], bytes[offset + 1]]);

        // MQTT 3.1.1 spec: Packet Identifier MUST be non-zero
        if packet_id == 0 {
            return Err(Error::MalformedPacket);
        }

        offset += 2;
        let topic_filter = read_string(bytes, &mut offset)?;
        if topic_filter.is_empty() {
            return Err(Error::EmptyTopic);
        }
        Ok(Self {
            packet_id,
            topic_filter,
        })
    }

    pub fn encode(&self, buffer: &mut [u8]) -> Result<usize, Error> {
        let mut offset = 0;
        if offset + 2 > buffer.len() {
            return Err(Error::BufferTooSmall);
        }
        let pid_bytes = self.packet_id.to_be_bytes();
        buffer[offset] = pid_bytes[0];
        buffer[offset + 1] = pid_bytes[1];
        offset += 2;
        write_string(self.topic_filter, buffer, &mut offset)?;
        Ok(offset)
    }
}
