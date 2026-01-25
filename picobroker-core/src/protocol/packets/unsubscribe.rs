use crate::protocol::packets::{PacketEncoder, PacketFlagsConst, PacketTypeConst};
use crate::protocol::utils::{read_string, write_string};
use crate::{PacketEncodingError, PacketType, TopicName};
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
            return Err(PacketEncodingError::BufferTooSmall {
                buffer_size: buffer.len(),
            });
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
            return Err(PacketEncodingError::IncompletePacket {
                buffer_size: bytes.len(),
            });
        }
        let packet_id = u16::from_be_bytes([bytes[offset], bytes[offset + 1]]);

        // MQTT 3.1.1 spec: Packet Identifier MUST be non-zero
        if packet_id == 0 {
            return Err(PacketEncodingError::MissingPacketId);
        }

        offset += 2;
        let topic_filter = read_string(bytes, &mut offset)?;
        if topic_filter.is_empty() {
            return Err(PacketEncodingError::TopicEmpty);
        }
        let topic_name = HeaplessString::try_from(topic_filter)
            .map(|s| TopicName::new(s))
            .map_err(|_| {
                PacketEncodingError::TopicNameLengthExceeded {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::packet_error::PacketEncodingError;

    const MAX_TOPIC_NAME_LENGTH: usize = 30;

    // ===== HELPER FUNCTIONS =====

    fn roundtrip_test(bytes: &[u8]) -> UnsubscribePacket<MAX_TOPIC_NAME_LENGTH> {
        let result = UnsubscribePacket::<MAX_TOPIC_NAME_LENGTH>::decode(&bytes);
        assert!(result.is_ok(), "Failed to decode packet: {:?}", result.err());
        let packet = result.unwrap();
        let mut buffer = [0u8; 128];
        let encode_result = packet.encode(&mut buffer);
        assert!(encode_result.is_ok(), "Failed to encode packet: {:?}", encode_result.err());
        let encoded_size = encode_result.unwrap();
        assert_eq!(encoded_size, bytes.len(), "Encoded size mismatch");
        assert_eq!(&buffer[..encoded_size], bytes, "Encoded bytes mismatch");
        packet
    }

    fn decode_test(bytes: &[u8]) -> Result<UnsubscribePacket<MAX_TOPIC_NAME_LENGTH>, PacketEncodingError> {
        UnsubscribePacket::<MAX_TOPIC_NAME_LENGTH>::decode(bytes)
    }

    // ===== STRUCT SIZE TEST =====

    #[test]
    fn test_unsubscribe_packet_struct_size() {
        // Should be 34 bytes: 2 for packet_id + 32 for topic_filter (HeaplessString with MAX_TOPIC_NAME_LENGTH=30)
        // HeaplessString<30> is 32 bytes (30 bytes array + 2 bytes for length)
        assert_eq!(core::mem::size_of::<UnsubscribePacket<MAX_TOPIC_NAME_LENGTH>>(), 34);
    }

    // ===== PACKET BYTES SIZE TEST =====

    #[test]
    fn test_unsubscribe_packet_bytes_size_minimal() {
        // Minimal packet: packet_id (2 bytes) + topic length (2 bytes) + "a" (1 byte) = 5 bytes
        let packet: UnsubscribePacket<MAX_TOPIC_NAME_LENGTH> = UnsubscribePacket {
            packet_id: 1,
            topic_filter: TopicName::new(HeaplessString::try_from("a").unwrap()),
        };
        let mut buffer = [0u8; 128];
        let size = packet.encode(&mut buffer).unwrap();
        assert_eq!(size, 5);
    }

    #[test]
    fn test_unsubscribe_packet_bytes_size_normal() {
        // Normal packet: packet_id (2 bytes) + topic length (2 bytes) + "sensors/temp" (12 bytes) = 16 bytes
        let packet: UnsubscribePacket<MAX_TOPIC_NAME_LENGTH> = UnsubscribePacket {
            packet_id: 1234,
            topic_filter: TopicName::new(HeaplessString::try_from("sensors/temp").unwrap()),
        };
        let mut buffer = [0u8; 128];
        let size = packet.encode(&mut buffer).unwrap();
        assert_eq!(size, 16);
    }

    // ===== PACKET ID FIELD TESTS =====

    #[test]
    fn test_packet_id_zero_fails() {
        // MQTT 3.1.1 spec: Packet Identifier MUST be non-zero
        let bytes = [0x00, 0x00, 0x00, 0x01, 0x61];
        let result = decode_test(&bytes);
        assert!(matches!(result, Err(PacketEncodingError::MissingPacketId)));
    }

    #[test]
    fn test_packet_id_one() {
        let bytes = [0x00, 0x01, 0x00, 0x01, 0x61];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 1);
    }

    #[test]
    fn test_packet_id_normal_values() {
        // Test packet_id = 100
        let bytes = [0x00, 0x64, 0x00, 0x0C, 0x73, 0x65, 0x6E, 0x73, 0x6F, 0x72, 0x73, 0x2F, 0x74, 0x65, 0x6D, 0x70];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 100);

        // Test packet_id = 1000
        let bytes = [0x03, 0xE8, 0x00, 0x01, 0x61];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 1000);

        // Test packet_id = 12345
        let bytes = [0x30, 0x39, 0x00, 0x01, 0x61];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 12345);
    }

    #[test]
    fn test_packet_id_max() {
        let bytes = [0xFF, 0xFF, 0x00, 0x01, 0x61];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 65535);
    }

    #[test]
    fn test_packet_id_byte_order() {
        // Test big-endian encoding: packet_id 0x1234 should be encoded as [0x12, 0x34]
        let bytes = [0x12, 0x34, 0x00, 0x01, 0x61];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 0x1234);

        // Verify encoding preserves big-endian byte order
        let mut buffer = [0u8; 128];
        let _encoded = packet.encode(&mut buffer).unwrap();
        assert_eq!(buffer[0], 0x12);
        assert_eq!(buffer[1], 0x34);
    }

    // ===== TOPIC FILTER FIELD TESTS =====

    #[test]
    fn test_topic_filter_simple() {
        let bytes = [0x00, 0x01, 0x00, 0x0C, 0x73, 0x65, 0x6E, 0x73, 0x6F, 0x72, 0x73, 0x2F, 0x74, 0x65, 0x6D, 0x70];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.as_str(), "sensors/temp");
    }

    #[test]
    fn test_topic_filter_multi_level() {
        let bytes = [0x00, 0x01, 0x00, 0x15, 0x68, 0x6F, 0x6D, 0x65, 0x2F, 0x6C, 0x69, 0x76, 0x69, 0x6E, 0x67, 0x72, 0x6F, 0x6F, 0x6D, 0x2F, 0x6C, 0x69, 0x67, 0x68, 0x74];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.as_str(), "home/livingroom/light");
    }

    #[test]
    fn test_topic_filter_with_wildcards() {
        // Single-level wildcard
        let bytes = [0x00, 0x01, 0x00, 0x0E, 0x73, 0x65, 0x6E, 0x73, 0x6F, 0x72, 0x73, 0x2F, 0x2B, 0x2F, 0x74, 0x65, 0x6D, 0x70];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.as_str(), "sensors/+/temp");

        // Multi-level wildcard
        let bytes = [0x00, 0x01, 0x00, 0x09, 0x73, 0x65, 0x6E, 0x73, 0x6F, 0x72, 0x73, 0x2F, 0x23];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.as_str(), "sensors/#");
    }

    #[test]
    fn test_topic_filter_empty_fails() {
        let bytes = [0x00, 0x01, 0x00, 0x00];
        let result = decode_test(&bytes);
        assert!(matches!(result, Err(PacketEncodingError::TopicEmpty)));
    }

    #[test]
    fn test_topic_filter_single_char() {
        // Single character "a"
        let bytes = [0x00, 0x01, 0x00, 0x01, 0x61];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.as_str(), "a");

        // Single wildcard "#"
        let bytes = [0x00, 0x01, 0x00, 0x01, 0x23];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.as_str(), "#");

        // Single single-level wildcard "+"
        let bytes = [0x00, 0x01, 0x00, 0x01, 0x2B];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.as_str(), "+");
    }

    #[test]
    fn test_topic_filter_unicode() {
        // Test with simple Unicode: "ñáé" (Spanish characters with accents)
        // UTF-8 encoding: ñ = 0xC3 0xB1, á = 0xC3 0xA1, é = 0xC3 0xA9
        let bytes = [0x00, 0x01, 0x00, 0x06, 0xC3, 0xB1, 0xC3, 0xA1, 0xC3, 0xA9];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.as_str(), "ñáé");
    }

    #[test]
    fn test_topic_filter_special_mqtt_chars() {
        let bytes = [0x00, 0x01, 0x00, 0x06, 0x61, 0x2F, 0x62, 0x2B, 0x2F, 0x63];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.as_str(), "a/b+/c");
    }

    #[test]
    fn test_topic_filter_exactly_max_length() {
        let topic = "123456789012345678901234567890"; // Exactly 30 characters
        let mut bytes = [0u8; 34];
        bytes[0] = 0x00;
        bytes[1] = 0x01;
        bytes[2] = 0x00;
        bytes[3] = 30;
        bytes[4..34].copy_from_slice(topic.as_bytes());
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.len(), 30);
    }

    #[test]
    fn test_topic_filter_too_long_fails() {
        let topic = "1234567890123456789012345678901"; // 31 characters, exceeds MAX_TOPIC_NAME_LENGTH
        let mut bytes = [0u8; 35];
        bytes[0] = 0x00;
        bytes[1] = 0x01;
        bytes[2] = 0x00;
        bytes[3] = 31;
        bytes[4..35].copy_from_slice(topic.as_bytes());
        let result = decode_test(&bytes);
        assert!(matches!(result, Err(PacketEncodingError::TopicNameLengthExceeded { .. })));
    }

    // ===== ERROR CONDITION TESTS: BUFFER SIZE =====

    #[test]
    fn test_encode_buffer_too_small_0_bytes() {
        let packet: UnsubscribePacket<MAX_TOPIC_NAME_LENGTH> = UnsubscribePacket {
            packet_id: 1,
            topic_filter: TopicName::new(HeaplessString::try_from("a").unwrap()),
        };
        let mut buffer = [0u8; 0];
        let result = packet.encode(&mut buffer);
        assert!(matches!(result, Err(PacketEncodingError::BufferTooSmall { .. })));
    }

    #[test]
    fn test_encode_buffer_too_small_1_byte() {
        let packet: UnsubscribePacket<MAX_TOPIC_NAME_LENGTH> = UnsubscribePacket {
            packet_id: 1,
            topic_filter: TopicName::new(HeaplessString::try_from("a").unwrap()),
        };
        let mut buffer = [0u8; 1];
        let result = packet.encode(&mut buffer);
        assert!(matches!(result, Err(PacketEncodingError::BufferTooSmall { .. })));
    }

    #[test]
    fn test_encode_buffer_too_small_2_bytes() {
        let packet: UnsubscribePacket<MAX_TOPIC_NAME_LENGTH> = UnsubscribePacket {
            packet_id: 1,
            topic_filter: TopicName::new(HeaplessString::try_from("a").unwrap()),
        };
        let mut buffer = [0u8; 2];
        let result = packet.encode(&mut buffer);
        assert!(matches!(result, Err(PacketEncodingError::BufferTooSmall { .. })));
    }

    #[test]
    fn test_encode_buffer_too_small_3_bytes() {
        let packet: UnsubscribePacket<MAX_TOPIC_NAME_LENGTH> = UnsubscribePacket {
            packet_id: 1,
            topic_filter: TopicName::new(HeaplessString::try_from("a").unwrap()),
        };
        let mut buffer = [0u8; 3];
        let result = packet.encode(&mut buffer);
        assert!(matches!(result, Err(PacketEncodingError::BufferTooSmall { .. })));
    }

    #[test]
    fn test_encode_buffer_too_small_4_bytes() {
        let packet: UnsubscribePacket<MAX_TOPIC_NAME_LENGTH> = UnsubscribePacket {
            packet_id: 1,
            topic_filter: TopicName::new(HeaplessString::try_from("a").unwrap()),
        };
        let mut buffer = [0u8; 4];
        let result = packet.encode(&mut buffer);
        assert!(matches!(result, Err(PacketEncodingError::BufferTooSmall { .. })));
    }

    #[test]
    fn test_encode_buffer_exactly_5_bytes() {
        let packet: UnsubscribePacket<MAX_TOPIC_NAME_LENGTH> = UnsubscribePacket {
            packet_id: 1,
            topic_filter: TopicName::new(HeaplessString::try_from("a").unwrap()),
        };
        let mut buffer = [0u8; 5];
        let result = packet.encode(&mut buffer);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 5);
    }

    // ===== ERROR CONDITION TESTS: INCOMPLETE PACKET =====

    #[test]
    fn test_decode_incomplete_packet_0_bytes() {
        let bytes = [];
        let result = decode_test(&bytes);
        assert!(matches!(result, Err(PacketEncodingError::IncompletePacket { .. })));
    }

    #[test]
    fn test_decode_incomplete_packet_1_byte() {
        let bytes = [0x00];
        let result = decode_test(&bytes);
        assert!(matches!(result, Err(PacketEncodingError::IncompletePacket { .. })));
    }

    #[test]
    fn test_decode_incomplete_packet_2_bytes() {
        let bytes = [0x00, 0x01];
        let result = decode_test(&bytes);
        assert!(matches!(result, Err(PacketEncodingError::IncompletePacket { .. })));
    }

    #[test]
    fn test_decode_incomplete_packet_3_bytes() {
        let bytes = [0x00, 0x01, 0x00];
        let result = decode_test(&bytes);
        assert!(matches!(result, Err(PacketEncodingError::IncompletePacket { .. })));
    }

    #[test]
    fn test_decode_incomplete_packet_4_bytes() {
        // Packet_id + length prefix but no topic data
        let bytes = [0x00, 0x01, 0x00, 0x01];
        let result = decode_test(&bytes);
        assert!(matches!(result, Err(PacketEncodingError::IncompletePacket { .. })));
    }

    // ===== COMPLETE PACKET ROUNDTRIP TESTS =====

    #[test]
    fn test_roundtrip_minimal_packet() {
        // Minimal packet: packet_id=1, topic="a"
        let bytes = [0x00, 0x01, 0x00, 0x01, 0x61];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 1);
        assert_eq!(packet.topic_filter.as_str(), "a");
    }

    #[test]
    fn test_roundtrip_normal_packet() {
        // Normal packet: packet_id=1234, topic="sensors/temp"
        let bytes = [0x12, 0x34, 0x00, 0x0C, 0x73, 0x65, 0x6E, 0x73, 0x6F, 0x72, 0x73, 0x2F, 0x74, 0x65, 0x6D, 0x70];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 0x1234);
        assert_eq!(packet.topic_filter.as_str(), "sensors/temp");
    }

    #[test]
    fn test_roundtrip_max_packet_id() {
        // Max packet_id: packet_id=65535, topic="sensors/temp"
        let bytes = [0xFF, 0xFF, 0x00, 0x0C, 0x73, 0x65, 0x6E, 0x73, 0x6F, 0x72, 0x73, 0x2F, 0x74, 0x65, 0x6D, 0x70];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 65535);
        assert_eq!(packet.topic_filter.as_str(), "sensors/temp");
    }

    #[test]
    fn test_roundtrip_unicode_packet() {
        // Unicode packet: packet_id=100, topic="ñáé"
        let bytes = [0x00, 0x64, 0x00, 0x06, 0xC3, 0xB1, 0xC3, 0xA1, 0xC3, 0xA9];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 100);
        assert_eq!(packet.topic_filter.as_str(), "ñáé");
    }

    #[test]
    fn test_roundtrip_wildcard_packet() {
        // Wildcard packet: packet_id=500, topic="sensors/#"
        let bytes = [0x01, 0xF4, 0x00, 0x09, 0x73, 0x65, 0x6E, 0x73, 0x6F, 0x72, 0x73, 0x2F, 0x23];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 500);
        assert_eq!(packet.topic_filter.as_str(), "sensors/#");
    }

    // ===== PROTOCOL COMPLIANCE TESTS =====

    #[test]
    fn test_packet_type_constant() {
        // Verify PACKET_TYPE is correctly set to Unsubscribe (0xA2)
        assert_eq!(UnsubscribePacket::<MAX_TOPIC_NAME_LENGTH>::PACKET_TYPE, PacketType::Unsubscribe);
    }

    #[test]
    fn test_packet_flags_constant() {
        // Verify PACKET_FLAGS is correctly set to 0b0010
        assert_eq!(UnsubscribePacket::<MAX_TOPIC_NAME_LENGTH>::PACKET_FLAGS, 0b0010);
    }
}
