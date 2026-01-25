use crate::protocol::packets::{PacketEncoder, PacketFlagsConst, PacketTypeConst};
use crate::protocol::utils::{read_string, write_string};
use crate::{Error, PacketEncodingError, PacketType, QoS, TopicName};
use crate::protocol::HeaplessString;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscribePacket<const MAX_TOPIC_NAME_LENGTH: usize> {
    pub packet_id: u16,
    pub topic_filter: TopicName<MAX_TOPIC_NAME_LENGTH>,
    pub requested_qos: QoS,
}

impl<const MAX_TOPIC_NAME_LENGTH: usize> PacketTypeConst for SubscribePacket<MAX_TOPIC_NAME_LENGTH> {
    const PACKET_TYPE: PacketType = PacketType::Subscribe;
}

impl<const MAX_TOPIC_NAME_LENGTH: usize> PacketFlagsConst for SubscribePacket<MAX_TOPIC_NAME_LENGTH> {
    const PACKET_FLAGS: u8 = 0b0010;
}

impl<const MAX_TOPIC_NAME_LENGTH: usize> PacketEncoder for SubscribePacket<MAX_TOPIC_NAME_LENGTH> {
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
        if offset >= buffer.len() {
            return Err(Error::BufferTooSmall.into());
        }
        buffer[offset] = 0;
        offset += 1;
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
        if offset + 1 > bytes.len() {
            return Err(Error::IncompletePacket.into());
        }
        let topic_name = HeaplessString::try_from(topic_filter)
            .map(|s| TopicName::new(s))
            .map_err(|_| {
                Error::TopicNameLengthExceeded {
                    max_length: MAX_TOPIC_NAME_LENGTH,
                    actual_length: topic_filter.len(),
                }
            })?;
        // Read requested QoS byte (currently unused but must be read to validate packet structure)
        let requested_qos = bytes[offset];
        let requested_qos = match requested_qos {
            0 => QoS::AtMostOnce,
            1 => QoS::AtLeastOnce,
            2 => QoS::ExactlyOnce,
            _ => return Err(Error::MalformedPacket.into()),
        };
        Ok(Self {
            packet_id,
            topic_filter: topic_name,
            requested_qos,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::packet_error::PacketEncodingError;

    const MAX_TOPIC_NAME_LENGTH: usize = 30;

    // ===== HELPER FUNCTIONS =====

    fn roundtrip_test(bytes: &[u8]) -> SubscribePacket<MAX_TOPIC_NAME_LENGTH> {
        let result = SubscribePacket::<MAX_TOPIC_NAME_LENGTH>::decode(&bytes);
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

    fn decode_test(bytes: &[u8]) -> Result<SubscribePacket<MAX_TOPIC_NAME_LENGTH>, PacketEncodingError> {
        SubscribePacket::<MAX_TOPIC_NAME_LENGTH>::decode(bytes)
    }

    // ===== STRUCT SIZE TEST =====

    #[test]
    fn test_subscribe_packet_struct_size() {
        // Should be 35 bytes: 2 for packet_id + 32 for topic_filter (HeaplessString with MAX_TOPIC_NAME_LENGTH=30) + 1 for requested_qos
        // HeaplessString<30> is 32 bytes (30 bytes array + 2 bytes for length)
        assert_eq!(core::mem::size_of::<SubscribePacket<MAX_TOPIC_NAME_LENGTH>>(), 35);
    }

    // ===== PACKET BYTES SIZE TEST =====

    #[test]
    fn test_subscribe_packet_bytes_size_minimal() {
        // Minimal packet: packet_id (2 bytes) + topic length (2 bytes) + "a" (1 byte) + qos (1 byte) = 6 bytes
        let packet: SubscribePacket<MAX_TOPIC_NAME_LENGTH> = SubscribePacket {
            packet_id: 1,
            topic_filter: TopicName::new(HeaplessString::try_from("a").unwrap()),
            requested_qos: QoS::AtMostOnce,
        };
        let mut buffer = [0u8; 128];
        let size = packet.encode(&mut buffer).unwrap();
        assert_eq!(size, 6);
    }

    #[test]
    fn test_subscribe_packet_bytes_size_normal() {
        // Normal packet: packet_id (2 bytes) + topic length (2 bytes) + "sensors/temp" (12 bytes) + qos (1 byte) = 17 bytes
        let packet: SubscribePacket<MAX_TOPIC_NAME_LENGTH> = SubscribePacket {
            packet_id: 1234,
            topic_filter: TopicName::new(HeaplessString::try_from("sensors/temp").unwrap()),
            requested_qos: QoS::AtLeastOnce,
        };
        let mut buffer = [0u8; 128];
        let size = packet.encode(&mut buffer).unwrap();
        assert_eq!(size, 17);
    }

    // ===== PACKET ID FIELD TESTS =====

    #[test]
    fn test_packet_id_zero_fails() {
        // MQTT 3.1.1 spec: Packet Identifier MUST be non-zero
        let bytes = [0x00, 0x00, 0x00, 0x01, 0x61, 0x00];
        let result = decode_test(&bytes);
        assert!(matches!(result, Err(PacketEncodingError::Other)));
    }

    #[test]
    fn test_packet_id_one() {
        let bytes = [0x00, 0x01, 0x00, 0x01, 0x61, 0x00];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 1);
    }

    #[test]
    fn test_packet_id_normal_values() {
        // Test packet_id = 100
        let bytes = [0x00, 0x64, 0x00, 0x0C, 0x73, 0x65, 0x6E, 0x73, 0x6F, 0x72, 0x73, 0x2F, 0x74, 0x65, 0x6D, 0x70, 0x01];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 100);

        // Test packet_id = 1000
        let bytes = [0x03, 0xE8, 0x00, 0x01, 0x61, 0x02];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 1000);

        // Test packet_id = 12345
        let bytes = [0x30, 0x39, 0x00, 0x01, 0x61, 0x00];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 12345);
    }

    #[test]
    fn test_packet_id_max() {
        let bytes = [0xFF, 0xFF, 0x00, 0x01, 0x61, 0x01];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 65535);
    }

    #[test]
    fn test_packet_id_byte_order() {
        // Test big-endian encoding: packet_id 0x1234 should be encoded as [0x12, 0x34]
        let bytes = [0x12, 0x34, 0x00, 0x01, 0x61, 0x00];
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
        let bytes = [0x00, 0x01, 0x00, 0x0C, 0x73, 0x65, 0x6E, 0x73, 0x6F, 0x72, 0x73, 0x2F, 0x74, 0x65, 0x6D, 0x70, 0x00];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.as_str(), "sensors/temp");
    }

    #[test]
    fn test_topic_filter_multi_level() {
        let bytes = [0x00, 0x01, 0x00, 0x15, 0x68, 0x6F, 0x6D, 0x65, 0x2F, 0x6C, 0x69, 0x76, 0x69, 0x6E, 0x67, 0x72, 0x6F, 0x6F, 0x6D, 0x2F, 0x6C, 0x69, 0x67, 0x68, 0x74, 0x01];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.as_str(), "home/livingroom/light");
    }

    #[test]
    fn test_topic_filter_with_wildcards() {
        // Single-level wildcard
        let bytes = [0x00, 0x01, 0x00, 0x0E, 0x73, 0x65, 0x6E, 0x73, 0x6F, 0x72, 0x73, 0x2F, 0x2B, 0x2F, 0x74, 0x65, 0x6D, 0x70, 0x00];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.as_str(), "sensors/+/temp");

        // Multi-level wildcard
        let bytes = [0x00, 0x01, 0x00, 0x09, 0x73, 0x65, 0x6E, 0x73, 0x6F, 0x72, 0x73, 0x2F, 0x23, 0x01];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.as_str(), "sensors/#");
    }

    #[test]
    fn test_topic_filter_empty_fails() {
        let bytes = [0x00, 0x01, 0x00, 0x00, 0x00];
        let result = decode_test(&bytes);
        assert!(matches!(result, Err(PacketEncodingError::Other)));
    }

    #[test]
    fn test_topic_filter_single_char() {
        // Single character "a"
        let bytes = [0x00, 0x01, 0x00, 0x01, 0x61, 0x00];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.as_str(), "a");

        // Single wildcard "#"
        let bytes = [0x00, 0x01, 0x00, 0x01, 0x23, 0x02];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.as_str(), "#");

        // Single single-level wildcard "+"
        let bytes = [0x00, 0x01, 0x00, 0x01, 0x2B, 0x01];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.as_str(), "+");
    }

    #[test]
    fn test_topic_filter_unicode() {
        // Test with simple Unicode: "ñáé" (Spanish characters with accents)
        // UTF-8 encoding: ñ = 0xC3 0xB1, á = 0xC3 0xA1, é = 0xC3 0xA9
        let bytes = [0x00, 0x01, 0x00, 0x06, 0xC3, 0xB1, 0xC3, 0xA1, 0xC3, 0xA9, 0x00];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.as_str(), "ñáé");
    }

    #[test]
    fn test_topic_filter_special_mqtt_chars() {
        let bytes = [0x00, 0x01, 0x00, 0x06, 0x61, 0x2F, 0x62, 0x2B, 0x2F, 0x63, 0x02];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.as_str(), "a/b+/c");
    }

    #[test]
    fn test_topic_filter_exactly_max_length() {
        let topic = "123456789012345678901234567890"; // Exactly 30 characters
        let mut bytes = [0u8; 35];
        bytes[0] = 0x00;
        bytes[1] = 0x01;
        bytes[2] = 0x00;
        bytes[3] = 30;
        bytes[4..34].copy_from_slice(topic.as_bytes());
        bytes[34] = 0x00; // QoS 0
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.len(), 30);
    }

    #[test]
    fn test_topic_filter_too_long_fails() {
        let topic = "1234567890123456789012345678901"; // 31 characters, exceeds MAX_TOPIC_NAME_LENGTH
        let mut bytes = [0u8; 36];
        bytes[0] = 0x00;
        bytes[1] = 0x01;
        bytes[2] = 0x00;
        bytes[3] = 31;
        bytes[4..35].copy_from_slice(topic.as_bytes());
        bytes[35] = 0x00; // QoS 0
        let result = decode_test(&bytes);
        assert!(matches!(result, Err(PacketEncodingError::Other)));
    }

    // ===== REQUESTED QoS FIELD TESTS =====

    #[test]
    fn test_requested_qos_0() {
        let bytes = [0x00, 0x01, 0x00, 0x01, 0x61, 0x00];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.requested_qos, QoS::AtMostOnce);
    }

    #[test]
    fn test_requested_qos_1() {
        let bytes = [0x00, 0x01, 0x00, 0x01, 0x61, 0x01];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.requested_qos, QoS::AtLeastOnce);
    }

    #[test]
    fn test_requested_qos_2() {
        let bytes = [0x00, 0x01, 0x00, 0x01, 0x61, 0x02];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.requested_qos, QoS::ExactlyOnce);
    }

    #[test]
    fn test_qos_encode_decode_roundtrip() {
        // Test all valid QoS values roundtrip correctly
        for qos_value in 0..=2 {
            let bytes = [0x12, 0x34, 0x00, 0x01, 0x61, qos_value];
            let packet = roundtrip_test(&bytes);

            let expected_qos = match qos_value {
                0 => QoS::AtMostOnce,
                1 => QoS::AtLeastOnce,
                2 => QoS::ExactlyOnce,
                _ => unreachable!(),
            };
            assert_eq!(packet.requested_qos, expected_qos);
        }
    }

    #[test]
    fn test_qos_byte_representation() {
        // Verify QoS encodes as correct u8 values
        let packet_q0: SubscribePacket<MAX_TOPIC_NAME_LENGTH> = SubscribePacket {
            packet_id: 1,
            topic_filter: TopicName::new(HeaplessString::try_from("a").unwrap()),
            requested_qos: QoS::AtMostOnce,
        };
        let mut buffer = [0u8; 128];
        let size = packet_q0.encode(&mut buffer).unwrap();
        assert_eq!(buffer[size - 1], 0);

        let packet_q1: SubscribePacket<MAX_TOPIC_NAME_LENGTH> = SubscribePacket {
            packet_id: 1,
            topic_filter: TopicName::new(HeaplessString::try_from("a").unwrap()),
            requested_qos: QoS::AtLeastOnce,
        };
        let size = packet_q1.encode(&mut buffer).unwrap();
        assert_eq!(buffer[size - 1], 1);

        let packet_q2: SubscribePacket<MAX_TOPIC_NAME_LENGTH> = SubscribePacket {
            packet_id: 1,
            topic_filter: TopicName::new(HeaplessString::try_from("a").unwrap()),
            requested_qos: QoS::ExactlyOnce,
        };
        let size = packet_q2.encode(&mut buffer).unwrap();
        assert_eq!(buffer[size - 1], 2);
    }

    // ===== ERROR CONDITION TESTS: BUFFER SIZE =====

    #[test]
    fn test_encode_buffer_too_small_0_bytes() {
        let packet: SubscribePacket<MAX_TOPIC_NAME_LENGTH> = SubscribePacket {
            packet_id: 1,
            topic_filter: TopicName::new(HeaplessString::try_from("a").unwrap()),
            requested_qos: QoS::AtMostOnce,
        };
        let mut buffer = [0u8; 0];
        let result = packet.encode(&mut buffer);
        assert!(matches!(result, Err(PacketEncodingError::Other)));
    }

    #[test]
    fn test_encode_buffer_too_small_1_byte() {
        let packet: SubscribePacket<MAX_TOPIC_NAME_LENGTH> = SubscribePacket {
            packet_id: 1,
            topic_filter: TopicName::new(HeaplessString::try_from("a").unwrap()),
            requested_qos: QoS::AtMostOnce,
        };
        let mut buffer = [0u8; 1];
        let result = packet.encode(&mut buffer);
        assert!(matches!(result, Err(PacketEncodingError::Other)));
    }

    #[test]
    fn test_encode_buffer_too_small_2_bytes() {
        let packet: SubscribePacket<MAX_TOPIC_NAME_LENGTH> = SubscribePacket {
            packet_id: 1,
            topic_filter: TopicName::new(HeaplessString::try_from("a").unwrap()),
            requested_qos: QoS::AtMostOnce,
        };
        let mut buffer = [0u8; 2];
        let result = packet.encode(&mut buffer);
        assert!(matches!(result, Err(PacketEncodingError::Other)));
    }

    #[test]
    fn test_encode_buffer_too_small_3_bytes() {
        let packet: SubscribePacket<MAX_TOPIC_NAME_LENGTH> = SubscribePacket {
            packet_id: 1,
            topic_filter: TopicName::new(HeaplessString::try_from("a").unwrap()),
            requested_qos: QoS::AtMostOnce,
        };
        let mut buffer = [0u8; 3];
        let result = packet.encode(&mut buffer);
        assert!(matches!(result, Err(PacketEncodingError::Other)));
    }

    #[test]
    fn test_encode_buffer_too_small_4_bytes() {
        let packet: SubscribePacket<MAX_TOPIC_NAME_LENGTH> = SubscribePacket {
            packet_id: 1,
            topic_filter: TopicName::new(HeaplessString::try_from("a").unwrap()),
            requested_qos: QoS::AtMostOnce,
        };
        let mut buffer = [0u8; 4];
        let result = packet.encode(&mut buffer);
        assert!(matches!(result, Err(PacketEncodingError::Other)));
    }

    #[test]
    fn test_encode_buffer_too_small_5_bytes() {
        let packet: SubscribePacket<MAX_TOPIC_NAME_LENGTH> = SubscribePacket {
            packet_id: 1,
            topic_filter: TopicName::new(HeaplessString::try_from("a").unwrap()),
            requested_qos: QoS::AtMostOnce,
        };
        let mut buffer = [0u8; 5];
        let result = packet.encode(&mut buffer);
        assert!(matches!(result, Err(PacketEncodingError::Other)));
    }

    #[test]
    fn test_encode_buffer_exactly_6_bytes() {
        let packet: SubscribePacket<MAX_TOPIC_NAME_LENGTH> = SubscribePacket {
            packet_id: 1,
            topic_filter: TopicName::new(HeaplessString::try_from("a").unwrap()),
            requested_qos: QoS::AtMostOnce,
        };
        let mut buffer = [0u8; 6];
        let result = packet.encode(&mut buffer);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 6);
    }

    // ===== ERROR CONDITION TESTS: INCOMPLETE PACKET =====

    #[test]
    fn test_decode_incomplete_packet_0_bytes() {
        let bytes = [];
        let result = decode_test(&bytes);
        assert!(matches!(result, Err(PacketEncodingError::Other)));
    }

    #[test]
    fn test_decode_incomplete_packet_1_byte() {
        let bytes = [0x00];
        let result = decode_test(&bytes);
        assert!(matches!(result, Err(PacketEncodingError::Other)));
    }

    #[test]
    fn test_decode_incomplete_packet_2_bytes() {
        let bytes = [0x00, 0x01];
        let result = decode_test(&bytes);
        assert!(matches!(result, Err(PacketEncodingError::Other)));
    }

    #[test]
    fn test_decode_incomplete_packet_3_bytes() {
        let bytes = [0x00, 0x01, 0x00];
        let result = decode_test(&bytes);
        assert!(matches!(result, Err(PacketEncodingError::Other)));
    }

    #[test]
    fn test_decode_incomplete_packet_4_bytes() {
        let bytes = [0x00, 0x01, 0x00, 0x01];
        let result = decode_test(&bytes);
        assert!(matches!(result, Err(PacketEncodingError::Other)));
    }

    #[test]
    fn test_decode_incomplete_packet_5_bytes() {
        // Packet_id + length prefix + topic data but missing QoS byte
        let bytes = [0x00, 0x01, 0x00, 0x01, 0x61];
        let result = decode_test(&bytes);
        assert!(matches!(result, Err(PacketEncodingError::Other)));
    }

    // ===== ERROR CONDITION TESTS: INVALID QoS =====

    #[test]
    fn test_decode_invalid_qos_3() {
        // QoS value 3 is the first invalid value
        let bytes = [0x00, 0x01, 0x00, 0x01, 0x61, 0x03];
        let result = decode_test(&bytes);
        assert!(matches!(result, Err(PacketEncodingError::Other)));
    }

    #[test]
    fn test_decode_invalid_qos_4() {
        let bytes = [0x00, 0x01, 0x00, 0x01, 0x61, 0x04];
        let result = decode_test(&bytes);
        assert!(matches!(result, Err(PacketEncodingError::Other)));
    }

    #[test]
    fn test_decode_invalid_qos_255() {
        // Maximum u8 value
        let bytes = [0x00, 0x01, 0x00, 0x01, 0x61, 0xFF];
        let result = decode_test(&bytes);
        assert!(matches!(result, Err(PacketEncodingError::Other)));
    }

    // ===== COMPLETE PACKET ROUNDTRIP TESTS =====

    #[test]
    fn test_roundtrip_minimal_packet_qos0() {
        // Minimal packet: packet_id=1, topic="a", qos=0
        let bytes = [0x00, 0x01, 0x00, 0x01, 0x61, 0x00];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 1);
        assert_eq!(packet.topic_filter.as_str(), "a");
        assert_eq!(packet.requested_qos, QoS::AtMostOnce);
    }

    #[test]
    fn test_roundtrip_normal_packet_qos1() {
        // Normal packet: packet_id=1234, topic="sensors/temp", qos=1
        let bytes = [0x12, 0x34, 0x00, 0x0C, 0x73, 0x65, 0x6E, 0x73, 0x6F, 0x72, 0x73, 0x2F, 0x74, 0x65, 0x6D, 0x70, 0x01];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 0x1234);
        assert_eq!(packet.topic_filter.as_str(), "sensors/temp");
        assert_eq!(packet.requested_qos, QoS::AtLeastOnce);
    }

    #[test]
    fn test_roundtrip_max_packet_id_qos2() {
        // Max packet_id: packet_id=65535, topic="sensors/temp", qos=2
        let bytes = [0xFF, 0xFF, 0x00, 0x0C, 0x73, 0x65, 0x6E, 0x73, 0x6F, 0x72, 0x73, 0x2F, 0x74, 0x65, 0x6D, 0x70, 0x02];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 65535);
        assert_eq!(packet.topic_filter.as_str(), "sensors/temp");
        assert_eq!(packet.requested_qos, QoS::ExactlyOnce);
    }

    #[test]
    fn test_roundtrip_unicode_packet() {
        // Unicode packet: packet_id=100, topic="ñáé", qos=0
        let bytes = [0x00, 0x64, 0x00, 0x06, 0xC3, 0xB1, 0xC3, 0xA1, 0xC3, 0xA9, 0x00];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 100);
        assert_eq!(packet.topic_filter.as_str(), "ñáé");
        assert_eq!(packet.requested_qos, QoS::AtMostOnce);
    }

    #[test]
    fn test_roundtrip_wildcard_packet_qos2() {
        // Wildcard packet: packet_id=500, topic="sensors/#", qos=2
        let bytes = [0x01, 0xF4, 0x00, 0x09, 0x73, 0x65, 0x6E, 0x73, 0x6F, 0x72, 0x73, 0x2F, 0x23, 0x02];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 500);
        assert_eq!(packet.topic_filter.as_str(), "sensors/#");
        assert_eq!(packet.requested_qos, QoS::ExactlyOnce);
    }

    #[test]
    fn test_roundtrip_complex_topic_qos1() {
        // Complex topic with single-level wildcard: packet_id=1000, topic="sensors/+/temp", qos=1
        let bytes = [0x03, 0xE8, 0x00, 0x0E, 0x73, 0x65, 0x6E, 0x73, 0x6F, 0x72, 0x73, 0x2F, 0x2B, 0x2F, 0x74, 0x65, 0x6D, 0x70, 0x01];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 1000);
        assert_eq!(packet.topic_filter.as_str(), "sensors/+/temp");
        assert_eq!(packet.requested_qos, QoS::AtLeastOnce);
    }

    // ===== PROTOCOL COMPLIANCE TESTS =====

    #[test]
    fn test_packet_type_constant() {
        // Verify PACKET_TYPE is correctly set to Subscribe (0x82)
        assert_eq!(SubscribePacket::<MAX_TOPIC_NAME_LENGTH>::PACKET_TYPE, PacketType::Subscribe);
    }

    #[test]
    fn test_packet_flags_constant() {
        // Verify PACKET_FLAGS is correctly set to 0b0010
        assert_eq!(SubscribePacket::<MAX_TOPIC_NAME_LENGTH>::PACKET_FLAGS, 0b0010);
    }
}
