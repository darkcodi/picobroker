use crate::protocol::heapless::{HeaplessString, HeaplessVec};
use crate::protocol::packet_type::PacketType;
use crate::protocol::packets::{PacketEncoder, PacketFlagsDynamic, PacketTypeConst};
use crate::protocol::qos::QoS;
use crate::protocol::ProtocolError;
use crate::topics::TopicName;

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

    pub fn from_nibble(nibble: u8) -> Result<Self, ProtocolError> {
        let qos = QoS::from_u8((nibble >> 1) & 0b11)?;
        Ok(PublishFlags {
            dup: (nibble & 0b1000) != 0,
            qos,
            retain: (nibble & 0b0001) != 0,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PublishPacket<const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize> {
    pub topic_name: TopicName<MAX_TOPIC_NAME_LENGTH>,
    pub packet_id: Option<u16>,
    pub payload: HeaplessVec<u8, MAX_PAYLOAD_SIZE>,
    pub qos: QoS,
    pub dup: bool,
    pub retain: bool,
}

impl<const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize> PacketTypeConst
    for PublishPacket<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>
{
    const PACKET_TYPE: PacketType = PacketType::Publish;
}

impl<const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize> PacketFlagsDynamic
    for PublishPacket<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>
{
    fn flags(&self) -> u8 {
        PublishFlags {
            dup: self.dup,
            qos: self.qos,
            retain: self.retain,
        }
        .publish_header_byte()
    }
}

impl<const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize> PacketEncoder
    for PublishPacket<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>
{
    fn encode(&self, buffer: &mut [u8]) -> Result<usize, ProtocolError> {
        let mut offset = 0;

        // 1. Calculate remaining length
        //    = 2 (topic length) + topic.len() + payload.len()
        //    + 2 (packet ID) if QoS > 0
        let mut remaining_length = 2 + self.topic_name.len();
        if self.qos != QoS::AtMostOnce {
            remaining_length += 2; // packet ID
        }
        remaining_length += self.payload.len();

        // 2. Write header byte (type + flags)
        if offset >= buffer.len() {
            return Err(ProtocolError::BufferTooSmall {
                buffer_size: buffer.len(),
            });
        }
        buffer[offset] = PublishFlags {
            dup: self.dup,
            qos: self.qos,
            retain: self.retain,
        }
        .publish_header_byte();
        offset += 1;

        // 3. Write variable length encoding
        let var_len_bytes =
            crate::protocol::utils::write_variable_length(remaining_length, &mut buffer[offset..])?;
        offset += var_len_bytes;

        // 4. Write topic name
        crate::protocol::utils::write_string(self.topic_name.as_str(), buffer, &mut offset)?;

        // 5. Write packet ID if QoS > 0
        if self.qos != QoS::AtMostOnce {
            if offset + 2 > buffer.len() {
                return Err(ProtocolError::BufferTooSmall {
                    buffer_size: buffer.len(),
                });
            }
            let pid = self.packet_id.ok_or(ProtocolError::MissingPacketId)?;
            let pid_bytes = pid.to_be_bytes();
            buffer[offset] = pid_bytes[0];
            buffer[offset + 1] = pid_bytes[1];
            offset += 2;
        }

        // 6. Write payload
        if offset + self.payload.len() > buffer.len() {
            return Err(ProtocolError::BufferTooSmall {
                buffer_size: buffer.len(),
            });
        }
        buffer[offset..offset + self.payload.len()].copy_from_slice(&self.payload);
        offset += self.payload.len();

        Ok(offset)
    }

    fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        let mut offset = 0;

        // 1. Validate packet type
        if offset >= bytes.len() {
            return Err(ProtocolError::IncompletePacket {
                available: bytes.len(),
            });
        }
        let header_byte = bytes[offset];
        offset += 1;
        let packet_type = (header_byte >> 4) & 0x0F;
        if packet_type != PacketType::Publish as u8 {
            return Err(ProtocolError::InvalidPacketType { packet_type });
        }

        // 2. Parse flags from header byte
        let flags_nibble = header_byte & 0x0F;
        let publish_flags = PublishFlags::from_nibble(flags_nibble)?;

        // 3. Read remaining length
        let (remaining_length, var_len_bytes) =
            crate::protocol::utils::read_variable_length(&bytes[offset..])?;
        offset += var_len_bytes;

        // Track remaining bytes for validation
        let start_offset = offset;

        // 4. Read topic name
        let topic_str = crate::protocol::utils::read_string(bytes, &mut offset)?;
        if topic_str.is_empty() {
            return Err(ProtocolError::TopicEmpty);
        }
        let topic_name = HeaplessString::<MAX_TOPIC_NAME_LENGTH>::try_from(topic_str)
            .map(TopicName::new)
            .map_err(|_| ProtocolError::TopicNameLengthExceeded {
                max_length: MAX_TOPIC_NAME_LENGTH,
                actual_length: topic_str.len(),
            })?;

        // 5. Read packet ID if QoS > 0
        let packet_id = if publish_flags.qos != QoS::AtMostOnce {
            if offset + 2 > bytes.len() {
                return Err(ProtocolError::IncompletePacket {
                    available: bytes.len(),
                });
            }
            let pid = u16::from_be_bytes([bytes[offset], bytes[offset + 1]]);
            if pid == 0 {
                return Err(ProtocolError::MissingPacketId);
            }
            offset += 2;
            Some(pid)
        } else {
            None
        };

        // 6. Read payload
        let payload_end = start_offset + remaining_length;
        if payload_end > bytes.len() {
            return Err(ProtocolError::IncompletePacket {
                available: bytes.len(),
            });
        }
        let payload_bytes = &bytes[offset..payload_end];
        let mut payload = HeaplessVec::<u8, MAX_PAYLOAD_SIZE>::new();
        payload
            .extend_from_slice(payload_bytes)
            .map_err(|_| ProtocolError::PayloadTooLarge {
                max_size: MAX_PAYLOAD_SIZE,
                actual_size: payload_bytes.len(),
            })?;

        // 7. Validate remaining length matched actual data
        let actual_consumed = offset - start_offset + payload_bytes.len();
        if actual_consumed != remaining_length {
            return Err(ProtocolError::InvalidPacketLength {
                expected: remaining_length,
                actual: actual_consumed,
            });
        }

        Ok(Self {
            topic_name,
            packet_id,
            payload,
            qos: publish_flags.qos,
            dup: publish_flags.dup,
            retain: publish_flags.retain,
        })
    }
}

impl<const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize> core::fmt::Display
    for PublishPacket<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "PublishPacket {{ topic_name: {}, packet_id: {:?}, qos: {:?}, dup: {}, retain: {}, payload: {} bytes }}",
            self.topic_name,
            self.packet_id,
            self.qos,
            self.dup,
            self.retain,
            self.payload.len()
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAX_TOPIC_NAME_LENGTH: usize = 30;
    const MAX_PAYLOAD_SIZE: usize = 100;

    // ===== HELPER FUNCTIONS =====

    fn roundtrip_test(bytes: &[u8]) -> PublishPacket<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE> {
        let result = PublishPacket::<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>::decode(bytes);
        assert!(
            result.is_ok(),
            "Failed to decode packet: {:?}",
            result.err()
        );
        let packet = result.unwrap();
        let mut buffer = [0u8; 256];
        let encode_result = packet.encode(&mut buffer);
        assert!(
            encode_result.is_ok(),
            "Failed to encode packet: {:?}",
            encode_result.err()
        );
        let encoded_size = encode_result.unwrap();
        assert_eq!(encoded_size, bytes.len(), "Encoded size mismatch");
        assert_eq!(&buffer[..encoded_size], bytes, "Encoded bytes mismatch");
        packet
    }

    // ===== BASIC QoS TESTS =====

    #[test]
    fn test_qos0_minimal_packet() {
        // QoS 0, topic="a", no payload
        let bytes = [0x30, 0x03, 0x00, 0x01, 0x61];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_name.as_str(), "a");
        assert_eq!(packet.qos, QoS::AtMostOnce);
        assert!(!packet.dup);
        assert!(!packet.retain);
        assert_eq!(packet.packet_id, None);
        assert_eq!(packet.payload.len(), 0);
    }

    #[test]
    fn test_qos0_with_payload() {
        // QoS 0, topic="sensor/temp" (11 bytes), payload="hello" (5 bytes)
        // remaining length = 2 (topic len) + 11 (topic) + 5 (payload) = 18
        let bytes: &[u8] = &[
            0x30, // QoS 0
            0x12, // remaining length = 18
            0x00, 0x0B, // topic length = 11
            // topic "sensor/temp"
            0x73, 0x65, 0x6E, 0x73, 0x6F, 0x72, 0x2F, 0x74, 0x65, 0x6D, 0x70,
            // payload "hello"
            0x68, 0x65, 0x6C, 0x6C, 0x6F,
        ];

        let packet = roundtrip_test(bytes);
        assert_eq!(packet.topic_name.as_str(), "sensor/temp");
        assert_eq!(packet.qos, QoS::AtMostOnce);
        assert_eq!(packet.payload.as_slice(), b"hello");
    }

    #[test]
    fn test_qos1_with_packet_id() {
        // QoS 1, topic="a", packet_id=1, no payload
        let bytes = [0x32, 0x05, 0x00, 0x01, 0x61, 0x00, 0x01];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_name.as_str(), "a");
        assert_eq!(packet.qos, QoS::AtLeastOnce);
        assert_eq!(packet.packet_id, Some(1));
        assert_eq!(packet.payload.len(), 0);
    }

    #[test]
    fn test_qos2_with_packet_id() {
        // QoS 2, topic="a", packet_id=65535, no payload
        let bytes = [0x34, 0x05, 0x00, 0x01, 0x61, 0xFF, 0xFF];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_name.as_str(), "a");
        assert_eq!(packet.qos, QoS::ExactlyOnce);
        assert_eq!(packet.packet_id, Some(65535));
    }

    // ===== HEADER FLAGS TESTS =====

    #[test]
    fn test_dup_flag() {
        // DUP=1, QoS=0
        let bytes = [0x38, 0x03, 0x00, 0x01, 0x61];
        let packet = roundtrip_test(&bytes);
        assert!(packet.dup);
    }

    #[test]
    fn test_retain_flag() {
        // RETAIN=1, QoS=0
        let bytes = [0x31, 0x03, 0x00, 0x01, 0x61];
        let packet = roundtrip_test(&bytes);
        assert!(packet.retain);
    }

    #[test]
    fn test_dup_and_retain_flags() {
        // DUP=1, RETAIN=1, QoS=1
        let bytes = [0x3B, 0x05, 0x00, 0x01, 0x61, 0x00, 0x01];
        let packet = roundtrip_test(&bytes);
        assert!(packet.dup);
        assert!(packet.retain);
        assert_eq!(packet.qos, QoS::AtLeastOnce);
    }

    // ===== TOPIC NAME TESTS =====

    #[test]
    fn test_topic_simple() {
        // topic "sensors/temp" (12 bytes), remaining length = 2 + 12 = 14
        let bytes = [
            0x30, 0x0E, 0x00, 0x0C, 0x73, 0x65, 0x6E, 0x73, 0x6F, 0x72, 0x73, 0x2F, 0x74, 0x65,
            0x6D, 0x70,
        ];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_name.as_str(), "sensors/temp");
    }

    #[test]
    fn test_topic_multi_level() {
        // topic "home/livingroom/light" (21 bytes), remaining length = 2 + 21 = 23
        let bytes: &[u8] = &[
            0x30, 0x17, // header: type + remaining length = 23
            0x00, 0x15, // topic length = 21
            // "home/livingroom/light"
            0x68, 0x6F, 0x6D, 0x65, 0x2F, 0x6C, 0x69, 0x76, 0x69, 0x6E, 0x67, 0x72, 0x6F, 0x6F,
            0x6D, 0x2F, 0x6C, 0x69, 0x67, 0x68, 0x74,
        ];
        let packet = roundtrip_test(bytes);
        assert_eq!(packet.topic_name.as_str(), "home/livingroom/light");
    }

    #[test]
    fn test_topic_unicode() {
        // UTF-8: ñ = 0xC3 0xB1, á = 0xC3 0xA1, é = 0xC3 0xA9
        let bytes = [
            0x30, 0x08, // header
            0x00, 0x06, // topic length = 6
            0xC3, 0xB1, 0xC3, 0xA1, 0xC3, 0xA9, // "ñáé"
        ];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_name.as_str(), "ñáé");
    }

    // ===== PACKET ID TESTS =====

    #[test]
    fn test_packet_id_one() {
        let bytes = [0x32, 0x05, 0x00, 0x01, 0x61, 0x00, 0x01];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, Some(1));
    }

    #[test]
    fn test_packet_id_max() {
        let bytes = [0x32, 0x05, 0x00, 0x01, 0x61, 0xFF, 0xFF];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, Some(65535));
    }

    // ===== PAYLOAD TESTS =====

    #[test]
    fn test_payload_empty() {
        let bytes = [0x30, 0x03, 0x00, 0x01, 0x61];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.payload.len(), 0);
    }

    #[test]
    fn test_payload_small() {
        let mut bytes = [0u8; 256];
        bytes[0] = 0x30; // QoS 0
        bytes[1] = 0x08; // remaining length
        bytes[2] = 0x00; // topic length MSB
        bytes[3] = 0x01; // topic length LSB
        bytes[4] = 0x61; // "a"
                         // payload "hello"
        bytes[5] = b'h';
        bytes[6] = b'e';
        bytes[7] = b'l';
        bytes[8] = b'l';
        bytes[9] = b'o';
        let bytes = &bytes[..10];

        let packet = roundtrip_test(bytes);
        assert_eq!(packet.payload.as_slice(), b"hello");
    }

    #[test]
    fn test_payload_binary() {
        // topic "a" (1 byte), payload 5 bytes, remaining length = 2 + 1 + 5 = 8
        let bytes: &[u8] = &[
            0x30, // QoS 0
            0x08, // remaining length = 8
            0x00, 0x01, // topic length = 1
            0x61, // topic "a"
            // binary payload: 0x00, 0x01, 0x02, 0xFF, 0xFE
            0x00, 0x01, 0x02, 0xFF, 0xFE,
        ];

        let packet = roundtrip_test(bytes);
        assert_eq!(packet.payload.as_slice(), &[0x00, 0x01, 0x02, 0xFF, 0xFE]);
    }

    // ===== ROUNDTRIP TESTS =====

    #[test]
    fn test_example_a() {
        let bytes: &[u8] = &[
            0x30, 0x08, //Fixed header: PUBLISH QoS0, RL=8
            0x00, 0x04, 0x74, 0x65, 0x73, 0x74, // Topic Name ("test")
            0x68, 0x69, // Payload ("hi")
        ];
        let packet = roundtrip_test(bytes);
        assert!(!packet.dup);
        assert_eq!(packet.qos, QoS::AtMostOnce);
        assert!(!packet.retain);
        assert_eq!(packet.topic_name.as_str(), "test");
        assert_eq!(packet.payload.as_slice(), b"hi");
    }

    #[test]
    fn test_example_b() {
        let bytes: &[u8] = &[
            0x32, 0x14, 0x00, 0x0C, 0x73, 0x65, 0x6E, 0x73, 0x6F, 0x72, 0x73, 0x2F, 0x74, 0x65,
            0x6D, 0x70, 0x00, 0x0A, 0x32, 0x32, 0x2E, 0x35,
        ];
        let packet = roundtrip_test(bytes);
        assert_eq!(packet.qos, QoS::AtLeastOnce);
        assert_eq!(packet.topic_name.as_str(), "sensors/temp");
        assert_eq!(packet.packet_id, Some(0x000A));
        assert_eq!(packet.payload.as_slice(), b"22.5");
    }

    #[test]
    fn test_example_c() {
        let bytes: &[u8] = &[0x35, 0x08, 0x00, 0x03, 0x61, 0x2F, 0x62, 0x12, 0x34, 0x58];
        let packet = roundtrip_test(bytes);
        assert_eq!(packet.qos, QoS::ExactlyOnce);
        assert!(packet.retain);
        assert_eq!(packet.topic_name.as_str(), "a/b");
        assert_eq!(packet.packet_id, Some(0x1234));
        assert_eq!(packet.payload.as_slice(), b"X");
    }

    #[test]
    fn test_example_d() {
        let bytes: &[u8] = &[0x3A, 0x05, 0x00, 0x01, 0x74, 0x00, 0x01];
        let packet = roundtrip_test(bytes);
        assert_eq!(packet.qos, QoS::AtLeastOnce);
        assert!(packet.dup);
        assert_eq!(packet.topic_name.as_str(), "t");
        assert_eq!(packet.packet_id, Some(0x0001));
        assert_eq!(packet.payload.as_slice(), b"");
    }
}
