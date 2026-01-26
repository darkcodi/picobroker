use crate::protocol::heapless::HeaplessString;
use crate::protocol::packet_error::PacketEncodingError;
use crate::protocol::packets::{PacketEncoder, PacketFlagsConst, PacketTypeConst};
use crate::protocol::utils::{read_string, write_string};
use crate::protocol::packet_type::PacketType;
use crate::protocol::qos::QoS;
use crate::topics::TopicName;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscribePacket<const MAX_TOPIC_NAME_LENGTH: usize> {
    pub packet_id: u16,
    pub topic_filter: TopicName<MAX_TOPIC_NAME_LENGTH>,
    pub requested_qos: QoS,
}

impl<const MAX_TOPIC_NAME_LENGTH: usize> PacketTypeConst
    for SubscribePacket<MAX_TOPIC_NAME_LENGTH>
{
    const PACKET_TYPE: PacketType = PacketType::Subscribe;
}

impl<const MAX_TOPIC_NAME_LENGTH: usize> PacketFlagsConst
    for SubscribePacket<MAX_TOPIC_NAME_LENGTH>
{
    const PACKET_FLAGS: u8 = 0b0010;
}

impl<const MAX_TOPIC_NAME_LENGTH: usize> PacketEncoder for SubscribePacket<MAX_TOPIC_NAME_LENGTH> {
    fn encode(&self, buffer: &mut [u8]) -> Result<usize, PacketEncodingError> {
        let mut offset = 0;

        // 1. Calculate remaining length
        //    = 2 (packet ID) + 2 (topic length) + topic.len() + 1 (QoS)
        let remaining_length = 2 + 2 + self.topic_filter.len() + 1;

        // 2. Write header byte (type + flags)
        if offset >= buffer.len() {
            return Err(PacketEncodingError::BufferTooSmall {
                buffer_size: buffer.len(),
            });
        }
        buffer[offset] = (Self::PACKET_TYPE as u8) << 4 | Self::PACKET_FLAGS;
        offset += 1;

        // 3. Write variable length encoding
        let var_len_bytes =
            crate::protocol::utils::write_variable_length(remaining_length, &mut buffer[offset..])?;
        offset += var_len_bytes;

        // 4. Write packet ID
        if offset + 2 > buffer.len() {
            return Err(PacketEncodingError::BufferTooSmall {
                buffer_size: buffer.len(),
            });
        }
        let pid_bytes = self.packet_id.to_be_bytes();
        buffer[offset] = pid_bytes[0];
        buffer[offset + 1] = pid_bytes[1];
        offset += 2;

        // 5. Write topic filter
        write_string(self.topic_filter.as_str(), buffer, &mut offset)?;

        // 6. Write requested QoS
        if offset >= buffer.len() {
            return Err(PacketEncodingError::BufferTooSmall {
                buffer_size: buffer.len(),
            });
        }
        buffer[offset] = self.requested_qos as u8;
        offset += 1;

        Ok(offset)
    }

    fn decode(bytes: &[u8]) -> Result<Self, PacketEncodingError> {
        let mut offset = 0;

        // 1. Validate packet type
        if offset >= bytes.len() {
            return Err(PacketEncodingError::IncompletePacket {
                buffer_size: bytes.len(),
            });
        }
        let header_byte = bytes[offset];
        offset += 1;
        let packet_type = (header_byte >> 4) & 0x0F;
        if packet_type != Self::PACKET_TYPE as u8 {
            return Err(PacketEncodingError::InvalidPacketType { packet_type });
        }

        // 2. Read remaining length
        let (remaining_length, var_len_bytes) =
            crate::protocol::utils::read_variable_length(&bytes[offset..])?;
        offset += var_len_bytes;

        // Track remaining bytes for validation
        let start_offset = offset;

        // 3. Read packet ID
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

        // 4. Read topic filter
        let topic_filter = read_string(bytes, &mut offset)?;
        if topic_filter.is_empty() {
            return Err(PacketEncodingError::TopicEmpty);
        }
        let topic_name = HeaplessString::try_from(topic_filter)
            .map(TopicName::new)
            .map_err(|_| PacketEncodingError::TopicNameLengthExceeded {
                max_length: MAX_TOPIC_NAME_LENGTH,
                actual_length: topic_filter.len(),
            })?;

        // 5. Read requested QoS byte
        if offset + 1 > bytes.len() {
            return Err(PacketEncodingError::IncompletePacket {
                buffer_size: bytes.len(),
            });
        }
        let requested_qos_byte = bytes[offset];
        let requested_qos = QoS::from_u8(requested_qos_byte)?;
        offset += 1;

        // 6. Validate remaining length matched actual data
        let actual_consumed = offset - start_offset;
        if actual_consumed != remaining_length {
            return Err(PacketEncodingError::InvalidPacketLength {
                expected: remaining_length,
                actual: actual_consumed,
            });
        }

        Ok(Self {
            packet_id,
            topic_filter: topic_name,
            requested_qos,
        })
    }
}

impl<const MAX_TOPIC_NAME_LENGTH: usize> core::fmt::Display for SubscribePacket<MAX_TOPIC_NAME_LENGTH> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "SubscribePacket {{ packet_id: {}, topic_filter: {}, requested_qos: {:?} }}",
            self.packet_id, self.topic_filter, self.requested_qos
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::packet_error::PacketEncodingError;
    use crate::protocol::qos::QoS;

    const MAX_TOPIC_NAME_LENGTH: usize = 30;

    // ===== HELPER FUNCTIONS =====

    fn roundtrip_test(bytes: &[u8]) -> SubscribePacket<MAX_TOPIC_NAME_LENGTH> {
        let result = SubscribePacket::<MAX_TOPIC_NAME_LENGTH>::decode(bytes);
        assert!(
            result.is_ok(),
            "Failed to decode packet: {:?}",
            result.err()
        );
        let packet = result.unwrap();
        let mut buffer = [0u8; 128];
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

    #[allow(dead_code)]
    fn decode_test(
        bytes: &[u8],
    ) -> Result<SubscribePacket<MAX_TOPIC_NAME_LENGTH>, PacketEncodingError> {
        SubscribePacket::<MAX_TOPIC_NAME_LENGTH>::decode(bytes)
    }

    // ===== STRUCT SIZE TEST =====

    #[test]
    fn test_subscribe_packet_struct_size() {
        assert_eq!(
            core::mem::size_of::<SubscribePacket<MAX_TOPIC_NAME_LENGTH>>(),
            34
        );
    }

    // ===== PACKET ID FIELD TESTS =====

    #[test]
    fn test_packet_id_one() {
        let bytes = [0x82, 0x06, 0x00, 0x01, 0x00, 0x01, 0x61, 0x00];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 1);
    }

    #[test]
    fn test_packet_id_max() {
        let bytes = [0x82, 0x06, 0xFF, 0xFF, 0x00, 0x01, 0x61, 0x01];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 65535);
    }

    // ===== TOPIC FILTER FIELD TESTS =====

    #[test]
    fn test_topic_filter_simple() {
        let bytes = [
            0x82, 0x11, 0x00, 0x01, 0x00, 0x0C, 0x73, 0x65, 0x6E, 0x73, 0x6F, 0x72, 0x73, 0x2F, 0x74,
            0x65, 0x6D, 0x70, 0x00,
        ];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.as_str(), "sensors/temp");
    }

    #[test]
    fn test_topic_filter_multi_level() {
        let bytes = [
            0x82, 0x1A, 0x00, 0x01, 0x00, 0x15, 0x68, 0x6F, 0x6D, 0x65, 0x2F, 0x6C, 0x69, 0x76,
            0x69, 0x6E, 0x67, 0x72, 0x6F, 0x6F, 0x6D, 0x2F, 0x6C, 0x69, 0x67, 0x68, 0x74, 0x01,
        ];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.as_str(), "home/livingroom/light");
    }

    #[test]
    fn test_topic_filter_with_wildcards() {
        // Single-level wildcard
        let bytes = [
            0x82, 0x13, 0x00, 0x01, 0x00, 0x0E, 0x73, 0x65, 0x6E, 0x73, 0x6F, 0x72, 0x73, 0x2F,
            0x2B, 0x2F, 0x74, 0x65, 0x6D, 0x70, 0x00,
        ];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.as_str(), "sensors/+/temp");

        // Multi-level wildcard
        let bytes = [
            0x82, 0x0E, 0x00, 0x01, 0x00, 0x09, 0x73, 0x65, 0x6E, 0x73, 0x6F, 0x72, 0x73, 0x2F,
            0x23, 0x01,
        ];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.as_str(), "sensors/#");
    }

    #[test]
    fn test_topic_filter_single_char() {
        // Single character "a"
        let bytes = [0x82, 0x06, 0x00, 0x01, 0x00, 0x01, 0x61, 0x00];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.as_str(), "a");

        // Single wildcard "#"
        let bytes = [0x82, 0x06, 0x00, 0x01, 0x00, 0x01, 0x23, 0x02];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.as_str(), "#");

        // Single single-level wildcard "+"
        let bytes = [0x82, 0x06, 0x00, 0x01, 0x00, 0x01, 0x2B, 0x01];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.as_str(), "+");
    }

    #[test]
    fn test_topic_filter_unicode() {
        // Test with simple Unicode: "ñáé" (Spanish characters with accents)
        // UTF-8 encoding: ñ = 0xC3 0xB1, á = 0xC3 0xA1, é = 0xC3 0xA9
        let bytes = [
            0x82, 0x0B, 0x00, 0x01, 0x00, 0x06, 0xC3, 0xB1, 0xC3, 0xA1, 0xC3, 0xA9, 0x00,
        ];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.as_str(), "ñáé");
    }

    #[test]
    fn test_topic_filter_special_mqtt_chars() {
        let bytes = [
            0x82, 0x0B, 0x00, 0x01, 0x00, 0x06, 0x61, 0x2F, 0x62, 0x2B, 0x2F, 0x63, 0x02,
        ];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.as_str(), "a/b+/c");
    }

    #[test]
    fn test_topic_filter_exactly_max_length() {
        let topic = "123456789012345678901234567890"; // Exactly 30 characters
        let mut bytes = [0u8; 37];
        bytes[0] = 0x82; // Header byte
        bytes[1] = 0x23; // Remaining length = 35
        bytes[2] = 0x00;
        bytes[3] = 0x01;
        bytes[4] = 0x00;
        bytes[5] = 30;
        bytes[6..36].copy_from_slice(topic.as_bytes());
        bytes[36] = 0x00; // QoS 0
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.len(), 30);
    }

    // ===== REQUESTED QoS FIELD TESTS =====

    #[test]
    fn test_requested_qos_0() {
        let bytes = [0x82, 0x06, 0x00, 0x01, 0x00, 0x01, 0x61, 0x00];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.requested_qos, QoS::AtMostOnce);
    }

    #[test]
    fn test_requested_qos_1() {
        let bytes = [0x82, 0x06, 0x00, 0x01, 0x00, 0x01, 0x61, 0x01];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.requested_qos, QoS::AtLeastOnce);
    }

    #[test]
    fn test_requested_qos_2() {
        let bytes = [0x82, 0x06, 0x00, 0x01, 0x00, 0x01, 0x61, 0x02];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.requested_qos, QoS::ExactlyOnce);
    }

    // ===== COMPLETE PACKET ROUNDTRIP TESTS =====

    #[test]
    fn test_roundtrip_minimal_packet_qos0() {
        // Minimal packet: packet_id=1, topic="a", qos=0
        let bytes = [0x82, 0x06, 0x00, 0x01, 0x00, 0x01, 0x61, 0x00];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 1);
        assert_eq!(packet.topic_filter.as_str(), "a");
        assert_eq!(packet.requested_qos, QoS::AtMostOnce);
    }

    #[test]
    fn test_roundtrip_normal_packet_qos1() {
        // Normal packet: packet_id=1234, topic="sensors/temp", qos=1
        let bytes = [
            0x82, 0x11, 0x12, 0x34, 0x00, 0x0C, 0x73, 0x65, 0x6E, 0x73, 0x6F, 0x72, 0x73, 0x2F,
            0x74, 0x65, 0x6D, 0x70, 0x01,
        ];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 0x1234);
        assert_eq!(packet.topic_filter.as_str(), "sensors/temp");
        assert_eq!(packet.requested_qos, QoS::AtLeastOnce);
    }

    #[test]
    fn test_roundtrip_max_packet_id_qos2() {
        // Max packet_id: packet_id=65535, topic="sensors/temp", qos=2
        let bytes = [
            0x82, 0x11, 0xFF, 0xFF, 0x00, 0x0C, 0x73, 0x65, 0x6E, 0x73, 0x6F, 0x72, 0x73, 0x2F,
            0x74, 0x65, 0x6D, 0x70, 0x02,
        ];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 65535);
        assert_eq!(packet.topic_filter.as_str(), "sensors/temp");
        assert_eq!(packet.requested_qos, QoS::ExactlyOnce);
    }

    #[test]
    fn test_roundtrip_unicode_packet() {
        // Unicode packet: packet_id=100, topic="ñáé", qos=0
        let bytes = [
            0x82, 0x0B, 0x00, 0x64, 0x00, 0x06, 0xC3, 0xB1, 0xC3, 0xA1, 0xC3, 0xA9, 0x00,
        ];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 100);
        assert_eq!(packet.topic_filter.as_str(), "ñáé");
        assert_eq!(packet.requested_qos, QoS::AtMostOnce);
    }

    #[test]
    fn test_roundtrip_wildcard_packet_qos2() {
        // Wildcard packet: packet_id=500, topic="sensors/#", qos=2
        let bytes = [
            0x82, 0x0E, 0x01, 0xF4, 0x00, 0x09, 0x73, 0x65, 0x6E, 0x73, 0x6F, 0x72, 0x73, 0x2F,
            0x23, 0x02,
        ];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 500);
        assert_eq!(packet.topic_filter.as_str(), "sensors/#");
        assert_eq!(packet.requested_qos, QoS::ExactlyOnce);
    }

    #[test]
    fn test_roundtrip_complex_topic_qos1() {
        // Complex topic with single-level wildcard: packet_id=1000, topic="sensors/+/temp", qos=1
        let bytes = [
            0x82, 0x13, 0x03, 0xE8, 0x00, 0x0E, 0x73, 0x65, 0x6E, 0x73, 0x6F, 0x72, 0x73, 0x2F,
            0x2B, 0x2F, 0x74, 0x65, 0x6D, 0x70, 0x01,
        ];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 1000);
        assert_eq!(packet.topic_filter.as_str(), "sensors/+/temp");
        assert_eq!(packet.requested_qos, QoS::AtLeastOnce);
    }

    // ===== PROTOCOL COMPLIANCE TESTS =====

    #[test]
    fn test_example_a() {
        let bytes = [
            0x82, 0x0F, 0x00, 0x01, 0x00, 0x0A, 0x74, 0x65, 0x73, 0x74, 0x2F, 0x74, 0x6F, 0x70, 0x69, 0x63, 0x00,
        ];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 1);
        assert_eq!(packet.topic_filter.as_str(), "test/topic");
        assert_eq!(packet.requested_qos, QoS::AtMostOnce);
    }
}
