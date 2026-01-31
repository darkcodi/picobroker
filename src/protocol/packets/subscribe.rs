use crate::protocol::heapless::HeaplessString;
use crate::protocol::packet_type::PacketType;
use crate::protocol::packets::{PacketEncoder, PacketFlagsConst, PacketTypeConst};
use crate::protocol::qos::QoS;
use crate::protocol::utils::{read_string, write_string};
use crate::protocol::ProtocolError;
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
    fn encode(&self, buffer: &mut [u8]) -> Result<usize, ProtocolError> {
        let mut offset = 0;

        let remaining_length = 2 + 2 + self.topic_filter.len() + 1;

        if offset >= buffer.len() {
            return Err(ProtocolError::BufferTooSmall {
                buffer_size: buffer.len(),
            });
        }
        buffer[offset] = (Self::PACKET_TYPE as u8) << 4 | Self::PACKET_FLAGS;
        offset += 1;

        let var_len_bytes =
            crate::protocol::utils::write_variable_length(remaining_length, &mut buffer[offset..])?;
        offset += var_len_bytes;

        if offset + 2 > buffer.len() {
            return Err(ProtocolError::BufferTooSmall {
                buffer_size: buffer.len(),
            });
        }
        let pid_bytes = self.packet_id.to_be_bytes();
        buffer[offset] = pid_bytes[0];
        buffer[offset + 1] = pid_bytes[1];
        offset += 2;

        write_string(self.topic_filter.as_str(), buffer, &mut offset)?;

        if offset >= buffer.len() {
            return Err(ProtocolError::BufferTooSmall {
                buffer_size: buffer.len(),
            });
        }
        buffer[offset] = self.requested_qos as u8;
        offset += 1;

        Ok(offset)
    }

    fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        let mut offset = 0;

        if offset >= bytes.len() {
            return Err(ProtocolError::IncompletePacket {
                available: bytes.len(),
            });
        }
        let header_byte = bytes[offset];
        offset += 1;
        let packet_type = (header_byte >> 4) & 0x0F;
        if packet_type != Self::PACKET_TYPE as u8 {
            return Err(ProtocolError::InvalidPacketType { packet_type });
        }

        let (remaining_length, var_len_bytes) =
            crate::protocol::utils::read_variable_length(&bytes[offset..])?;
        offset += var_len_bytes;

        let start_offset = offset;

        if offset + 2 > bytes.len() {
            return Err(ProtocolError::IncompletePacket {
                available: bytes.len(),
            });
        }
        let packet_id = u16::from_be_bytes([bytes[offset], bytes[offset + 1]]);

        if packet_id == 0 {
            return Err(ProtocolError::MissingPacketId);
        }

        offset += 2;

        let topic_filter = read_string(bytes, &mut offset)?;
        if topic_filter.is_empty() {
            return Err(ProtocolError::TopicEmpty);
        }
        let topic_name = HeaplessString::try_from(topic_filter)
            .map(TopicName::new)
            .map_err(|_| ProtocolError::TopicNameLengthExceeded {
                max_length: MAX_TOPIC_NAME_LENGTH,
                actual_length: topic_filter.len(),
            })?;

        if offset + 1 > bytes.len() {
            return Err(ProtocolError::IncompletePacket {
                available: bytes.len(),
            });
        }
        let requested_qos_byte = bytes[offset];
        let requested_qos = QoS::from_u8(requested_qos_byte)?;
        offset += 1;

        let actual_consumed = offset - start_offset;
        if actual_consumed != remaining_length {
            return Err(ProtocolError::InvalidPacketLength {
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

impl<const MAX_TOPIC_NAME_LENGTH: usize> core::fmt::Display
    for SubscribePacket<MAX_TOPIC_NAME_LENGTH>
{
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
    use crate::protocol::qos::QoS;
    use crate::protocol::ProtocolError;

    const MAX_TOPIC_NAME_LENGTH: usize = 30;

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
    fn decode_test(bytes: &[u8]) -> Result<SubscribePacket<MAX_TOPIC_NAME_LENGTH>, ProtocolError> {
        SubscribePacket::<MAX_TOPIC_NAME_LENGTH>::decode(bytes)
    }

    #[test]
    fn test_subscribe_packet_struct_size() {
        assert_eq!(
            core::mem::size_of::<SubscribePacket<MAX_TOPIC_NAME_LENGTH>>(),
            34
        );
    }

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

    #[test]
    fn test_topic_filter_simple() {
        let bytes = [
            0x82, 0x11, 0x00, 0x01, 0x00, 0x0C, 0x73, 0x65, 0x6E, 0x73, 0x6F, 0x72, 0x73, 0x2F,
            0x74, 0x65, 0x6D, 0x70, 0x00,
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
        let bytes = [
            0x82, 0x13, 0x00, 0x01, 0x00, 0x0E, 0x73, 0x65, 0x6E, 0x73, 0x6F, 0x72, 0x73, 0x2F,
            0x2B, 0x2F, 0x74, 0x65, 0x6D, 0x70, 0x00,
        ];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.as_str(), "sensors/+/temp");

        let bytes = [
            0x82, 0x0E, 0x00, 0x01, 0x00, 0x09, 0x73, 0x65, 0x6E, 0x73, 0x6F, 0x72, 0x73, 0x2F,
            0x23, 0x01,
        ];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.as_str(), "sensors/#");
    }

    #[test]
    fn test_topic_filter_single_char() {
        let bytes = [0x82, 0x06, 0x00, 0x01, 0x00, 0x01, 0x61, 0x00];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.as_str(), "a");

        let bytes = [0x82, 0x06, 0x00, 0x01, 0x00, 0x01, 0x23, 0x02];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.as_str(), "#");

        let bytes = [0x82, 0x06, 0x00, 0x01, 0x00, 0x01, 0x2B, 0x01];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.as_str(), "+");
    }

    #[test]
    fn test_topic_filter_unicode() {
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
        let topic = "123456789012345678901234567890";
        let mut bytes = [0u8; 37];
        bytes[0] = 0x82;
        bytes[1] = 0x23;
        bytes[2] = 0x00;
        bytes[3] = 0x01;
        bytes[4] = 0x00;
        bytes[5] = 30;
        bytes[6..36].copy_from_slice(topic.as_bytes());
        bytes[36] = 0x00;
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.len(), 30);
    }

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

    #[test]
    fn test_roundtrip_minimal_packet_qos0() {
        let bytes = [0x82, 0x06, 0x00, 0x01, 0x00, 0x01, 0x61, 0x00];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 1);
        assert_eq!(packet.topic_filter.as_str(), "a");
        assert_eq!(packet.requested_qos, QoS::AtMostOnce);
    }

    #[test]
    fn test_roundtrip_normal_packet_qos1() {
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
        let bytes = [
            0x82, 0x13, 0x03, 0xE8, 0x00, 0x0E, 0x73, 0x65, 0x6E, 0x73, 0x6F, 0x72, 0x73, 0x2F,
            0x2B, 0x2F, 0x74, 0x65, 0x6D, 0x70, 0x01,
        ];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 1000);
        assert_eq!(packet.topic_filter.as_str(), "sensors/+/temp");
        assert_eq!(packet.requested_qos, QoS::AtLeastOnce);
    }

    #[test]
    fn test_example_a() {
        let bytes = [
            0x82, 0x0F, 0x00, 0x01, 0x00, 0x0A, 0x74, 0x65, 0x73, 0x74, 0x2F, 0x74, 0x6F, 0x70,
            0x69, 0x63, 0x00,
        ];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 1);
        assert_eq!(packet.topic_filter.as_str(), "test/topic");
        assert_eq!(packet.requested_qos, QoS::AtMostOnce);
    }
}
