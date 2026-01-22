use crate::protocol::packet_type::PacketType;
use crate::protocol::packets::PacketEncoder;
use crate::protocol::qos::QoS;
use crate::protocol::utils::{read_string, write_string};
use crate::{Error, TopicName};

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
struct PublishFlags {
    pub dup: bool,
    pub qos: QoS,
    pub retain: bool,
}

impl PublishFlags {
    pub const fn publish_header_byte(self) -> u8 {
        ((PacketType::Publish as u8) << 4) | (self.to_nibble() & 0x0F)
    }

    pub const fn to_nibble(self) -> u8 {
        let dup = if self.dup { 1u8 } else { 0u8 };
        let retain = if self.retain { 1u8 } else { 0u8 };
        (dup << 3) | ((self.qos as u8) << 1) | retain
    }

    pub fn from_nibble(nibble: u8) -> Option<Self> {
        let qos = QoS::from_u8((nibble >> 1) & 0b11)?;
        Some(PublishFlags {
            dup: (nibble & 0b1000) != 0,
            qos,
            retain: (nibble & 0b0001) != 0,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Publish<const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize> {
    pub topic_name: TopicName<MAX_TOPIC_NAME_LENGTH>,
    pub packet_id: Option<u16>,
    pub payload: heapless::Vec<u8, MAX_PAYLOAD_SIZE>,
    pub qos: QoS,
    pub dup: bool,
    pub retain: bool,
}

impl<'a, const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize> PacketEncoder<'a> for Publish<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE> {
    fn encode(&'a self, buffer: &mut [u8]) -> Result<usize, Error> {
        let mut offset = 0;
        write_string(self.topic_name.as_str(), buffer, &mut offset)?;

        match self.qos {
            QoS::AtMostOnce => { /* packet_id not required */ }
            QoS::AtLeastOnce | QoS::ExactlyOnce => {
                let packet_id = self.packet_id.ok_or(Error::MalformedPacket)?;
                if offset + 2 > buffer.len() {
                    return Err(Error::BufferTooSmall);
                }
                let pid_bytes = packet_id.to_be_bytes();
                buffer[offset] = pid_bytes[0];
                buffer[offset + 1] = pid_bytes[1];
                offset += 2;
            }
        }

        if offset + self.payload.len() > buffer.len() {
            return Err(Error::BufferTooSmall);
        }
        buffer[offset..offset + self.payload.len()].copy_from_slice(&self.payload);
        offset += self.payload.len();

        Ok(offset)
    }

    fn decode(bytes: &'a [u8], header: u8) -> Result<Self, Error> {
        let mut offset = 0;
        let topic_name = read_string(bytes, &mut offset)?;
        if topic_name.is_empty() {
            return Err(Error::EmptyTopic);
        }
        let topic_name = heapless::String::try_from(topic_name)
            .map(|s| TopicName::new(s))
            .map_err(|_| {
                Error::TopicNameLengthExceeded {
                    max_length: MAX_TOPIC_NAME_LENGTH,
                    actual_length: topic_name.len(),
                }
            })?;

        // Extract flags from the header byte
        let flags =
            PublishFlags::from_nibble(header & 0x0F).ok_or(Error::InvalidFixedHeaderFlags {
                actual: header & 0x0F,
                expected: 0,
            })?;

        let packet_id = if flags.qos != QoS::AtMostOnce {
            if offset + 2 > bytes.len() {
                return Err(Error::IncompletePacket);
            }
            let pid = u16::from_be_bytes([bytes[offset], bytes[offset + 1]]);

            // MQTT 3.1.1 spec: Packet Identifier MUST be non-zero
            if pid == 0 {
                return Err(Error::MalformedPacket);
            }

            offset += 2;
            Some(pid)
        } else {
            None
        };

        let payload_len = bytes.len() - offset;
        let mut payload = heapless::Vec::new();
        payload
            .extend_from_slice(&bytes[offset..offset + payload_len])
            .map_err(|_| Error::BufferTooSmall)?;

        Ok(Self {
            topic_name,
            packet_id,
            payload,
            qos: flags.qos,
            dup: flags.dup,
            retain: flags.retain,
        })
    }
}

impl<const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize> Publish<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE> {
    pub fn header_byte(&self) -> u8 {
        PublishFlags {
            dup: self.dup,
            qos: self.qos,
            retain: self.retain,
        }
        .publish_header_byte()
    }
}
