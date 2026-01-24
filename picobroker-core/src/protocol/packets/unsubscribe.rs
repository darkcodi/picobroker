use crate::protocol::packets::{PacketEncoder, PacketFlagsConst, PacketTypeConst};
use crate::protocol::utils::{read_string, write_string};
use crate::{Error, PacketEncodingError, PacketType, TopicName};
use crate::protocol::HeaplessString;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsubscribePacket<const MAX_TOPIC_NAME_LENGTH: usize> {
    pub packet_id: u16,
    pub topic_filter: TopicName<MAX_TOPIC_NAME_LENGTH>,
}

impl<const MAX_TOPIC_NAME_LENGTH: usize> PacketTypeConst for UnsubscribePacket<MAX_TOPIC_NAME_LENGTH> {
    const PACKET_TYPE: PacketType = PacketType::Unsubscribe;
}

impl<const MAX_TOPIC_NAME_LENGTH: usize> PacketFlagsConst for UnsubscribePacket<MAX_TOPIC_NAME_LENGTH> {
    const PACKET_FLAGS: u8 = 0b0010;
}

impl<const MAX_TOPIC_NAME_LENGTH: usize> PacketEncoder for UnsubscribePacket<MAX_TOPIC_NAME_LENGTH> {
    fn encode(&self, buffer: &mut [u8]) -> Result<usize, PacketEncodingError> {
        let mut offset = 0;
        if offset + 2 > buffer.len() {
            return Err(Error::BufferTooSmall.into());
        }
        let pid_bytes = self.packet_id.to_be_bytes();
        buffer[offset] = pid_bytes[0];
        buffer[offset + 1] = pid_bytes[1];
        offset += 2;
        write_string(self.topic_filter.as_str(), buffer, &mut offset)?;
        Ok(offset)
    }

    fn decode(bytes: &[u8]) -> Result<Self, PacketEncodingError> {
        let mut offset = 0;
        if offset + 2 > bytes.len() {
            return Err(Error::IncompletePacket.into());
        }
        let packet_id = u16::from_be_bytes([bytes[offset], bytes[offset + 1]]);

        // MQTT 3.1.1 spec: Packet Identifier MUST be non-zero
        if packet_id == 0 {
            return Err(Error::MalformedPacket.into());
        }

        offset += 2;
        let topic_filter = read_string(bytes, &mut offset)?;
        if topic_filter.is_empty() {
            return Err(Error::EmptyTopic.into());
        }
        let topic_name = HeaplessString::try_from(topic_filter)
            .map(|s| TopicName::new(s))
            .map_err(|_| {
                Error::TopicNameLengthExceeded {
                    max_length: MAX_TOPIC_NAME_LENGTH,
                    actual_length: topic_filter.len(),
                }
            })?;
        Ok(Self {
            packet_id,
            topic_filter: topic_name,
        })
    }
}
