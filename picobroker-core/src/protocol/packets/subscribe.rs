use crate::protocol::packets::PacketEncoder;
use crate::protocol::utils::{read_string, write_string};
use crate::{Error, PacketType, QoS, TopicName};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscribePacket<const MAX_TOPIC_NAME_LENGTH: usize> {
    pub packet_id: u16,
    pub topic_filter: TopicName<MAX_TOPIC_NAME_LENGTH>,
    pub requested_qos: QoS,
}

impl<const MAX_TOPIC_NAME_LENGTH: usize> PacketEncoder for SubscribePacket<MAX_TOPIC_NAME_LENGTH> {
    fn packet_type(&self) -> PacketType {
        PacketType::Subscribe
    }

    fn fixed_flags(&self) -> u8 {
        0b0010
    }

    fn encode(&self, buffer: &mut [u8]) -> Result<usize, Error> {
        let mut offset = 0;
        if offset + 2 > buffer.len() {
            return Err(Error::BufferTooSmall);
        }
        let pid_bytes = self.packet_id.to_be_bytes();
        buffer[offset] = pid_bytes[0];
        buffer[offset + 1] = pid_bytes[1];
        offset += 2;
        write_string(self.topic_filter.as_str(), buffer, &mut offset)?;
        if offset >= buffer.len() {
            return Err(Error::BufferTooSmall);
        }
        buffer[offset] = 0;
        offset += 1;
        Ok(offset)
    }

    fn decode(payload: &[u8], _header: u8) -> Result<Self, Error> {
        let mut offset = 0;
        if offset + 2 > payload.len() {
            return Err(Error::IncompletePacket);
        }
        let packet_id = u16::from_be_bytes([payload[offset], payload[offset + 1]]);

        // MQTT 3.1.1 spec: Packet Identifier MUST be non-zero
        if packet_id == 0 {
            return Err(Error::MalformedPacket);
        }

        offset += 2;
        let topic_filter = read_string(payload, &mut offset)?;
        if topic_filter.is_empty() {
            return Err(Error::EmptyTopic);
        }
        if offset + 1 > payload.len() {
            return Err(Error::IncompletePacket);
        }
        let topic_name = heapless::String::try_from(topic_filter)
            .map(|s| TopicName::new(s))
            .map_err(|_| {
                Error::TopicNameLengthExceeded {
                    max_length: MAX_TOPIC_NAME_LENGTH,
                    actual_length: topic_filter.len(),
                }
            })?;
        // Read requested QoS byte (currently unused but must be read to validate packet structure)
        let requested_qos = payload[offset];
        let requested_qos = match requested_qos {
            0 => QoS::AtMostOnce,
            1 => QoS::AtLeastOnce,
            2 => QoS::ExactlyOnce,
            _ => return Err(Error::MalformedPacket),
        };
        Ok(Self {
            packet_id,
            topic_filter: topic_name,
            requested_qos,
        })
    }
}
