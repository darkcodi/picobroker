use crate::protocol::heapless::HeaplessString;
use crate::protocol::packet_type::PacketType;
use crate::protocol::packets::{PacketEncoder, PacketFlagsConst, PacketTypeConst};
use crate::protocol::utils::{
    read_string, read_variable_length, write_string, write_variable_length,
};
use crate::protocol::ProtocolError;
use crate::topics::TopicName;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsubscribePacket<const MAX_TOPIC_NAME_LENGTH: usize> {
    pub packet_id: u16,
    pub topic_filter: TopicName<MAX_TOPIC_NAME_LENGTH>,
}

impl<const MAX_TOPIC_NAME_LENGTH: usize> PacketTypeConst
    for UnsubscribePacket<MAX_TOPIC_NAME_LENGTH>
{
    const PACKET_TYPE: PacketType = PacketType::Unsubscribe;
}

impl<const MAX_TOPIC_NAME_LENGTH: usize> PacketFlagsConst
    for UnsubscribePacket<MAX_TOPIC_NAME_LENGTH>
{
    const PACKET_FLAGS: u8 = 0b0010;
}

impl<const MAX_TOPIC_NAME_LENGTH: usize> PacketEncoder
    for UnsubscribePacket<MAX_TOPIC_NAME_LENGTH>
{
    fn encode(&self, buffer: &mut [u8]) -> Result<usize, ProtocolError> {
        let mut offset = 0;

        let remaining_length = 2 + 2 + self.topic_filter.len();

        if offset >= buffer.len() {
            return Err(ProtocolError::BufferTooSmall {
                buffer_size: buffer.len(),
            });
        }
        buffer[offset] = (Self::PACKET_TYPE as u8) << 4 | Self::PACKET_FLAGS;
        offset += 1;

        let var_len_bytes = write_variable_length(remaining_length, &mut buffer[offset..])?;
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

        let (remaining_length, var_len_bytes) = read_variable_length(&bytes[offset..])?;
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
        })
    }
}

impl<const MAX_TOPIC_NAME_LENGTH: usize> core::fmt::Display
    for UnsubscribePacket<MAX_TOPIC_NAME_LENGTH>
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "UnsubscribePacket {{ packet_id: {}, topic_filter: {} }}",
            self.packet_id, self.topic_filter
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::ProtocolError;

    const MAX_TOPIC_NAME_LENGTH: usize = 30;

    fn roundtrip_test(bytes: &[u8]) -> UnsubscribePacket<MAX_TOPIC_NAME_LENGTH> {
        let result = UnsubscribePacket::<MAX_TOPIC_NAME_LENGTH>::decode(bytes);
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
    ) -> Result<UnsubscribePacket<MAX_TOPIC_NAME_LENGTH>, ProtocolError> {
        UnsubscribePacket::<MAX_TOPIC_NAME_LENGTH>::decode(bytes)
    }

    #[test]
    fn test_unsubscribe_packet_struct_size() {
        assert_eq!(
            core::mem::size_of::<UnsubscribePacket<MAX_TOPIC_NAME_LENGTH>>(),
            34
        );
    }

    #[test]
    fn test_packet_id_one() {
        let bytes = [0xA2, 0x05, 0x00, 0x01, 0x00, 0x01, 0x61];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 1);
    }

    #[test]
    fn test_packet_id_max() {
        let bytes = [0xA2, 0x05, 0xFF, 0xFF, 0x00, 0x01, 0x61];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 65535);
    }

    #[test]
    fn test_topic_filter_simple() {
        let bytes = [
            0xA2, 0x10, 0x00, 0x01, 0x00, 0x0C, 0x73, 0x65, 0x6E, 0x73, 0x6F, 0x72, 0x73, 0x2F,
            0x74, 0x65, 0x6D, 0x70,
        ];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.as_str(), "sensors/temp");
    }

    #[test]
    fn test_topic_filter_multi_level() {
        let bytes = [
            0xA2, 0x19, 0x00, 0x01, 0x00, 0x15, 0x68, 0x6F, 0x6D, 0x65, 0x2F, 0x6C, 0x69, 0x76,
            0x69, 0x6E, 0x67, 0x72, 0x6F, 0x6F, 0x6D, 0x2F, 0x6C, 0x69, 0x67, 0x68, 0x74,
        ];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.as_str(), "home/livingroom/light");
    }

    #[test]
    fn test_topic_filter_with_wildcards() {
        let bytes = [
            0xA2, 0x12, 0x00, 0x01, 0x00, 0x0E, 0x73, 0x65, 0x6E, 0x73, 0x6F, 0x72, 0x73, 0x2F,
            0x2B, 0x2F, 0x74, 0x65, 0x6D, 0x70,
        ];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.as_str(), "sensors/+/temp");

        let bytes = [
            0xA2, 0x0D, 0x00, 0x01, 0x00, 0x09, 0x73, 0x65, 0x6E, 0x73, 0x6F, 0x72, 0x73, 0x2F,
            0x23,
        ];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.as_str(), "sensors/#");
    }

    #[test]
    fn test_topic_filter_single_char() {
        let bytes = [0xA2, 0x05, 0x00, 0x01, 0x00, 0x01, 0x61];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.as_str(), "a");

        let bytes = [0xA2, 0x05, 0x00, 0x01, 0x00, 0x01, 0x23];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.as_str(), "#");

        let bytes = [0xA2, 0x05, 0x00, 0x01, 0x00, 0x01, 0x2B];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.as_str(), "+");
    }

    #[test]
    fn test_topic_filter_unicode() {
        let bytes = [
            0xA2, 0x0A, 0x00, 0x01, 0x00, 0x06, 0xC3, 0xB1, 0xC3, 0xA1, 0xC3, 0xA9,
        ];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.as_str(), "ñáé");
    }

    #[test]
    fn test_topic_filter_special_mqtt_chars() {
        let bytes = [
            0xA2, 0x0A, 0x00, 0x01, 0x00, 0x06, 0x61, 0x2F, 0x62, 0x2B, 0x2F, 0x63,
        ];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.as_str(), "a/b+/c");
    }

    #[test]
    fn test_topic_filter_exactly_max_length() {
        let topic = "123456789012345678901234567890";
        let mut bytes = [0u8; 36];
        bytes[0] = 0xA2;
        bytes[1] = 0x22;
        bytes[2] = 0x00;
        bytes[3] = 0x01;
        bytes[4] = 0x00;
        bytes[5] = 30;
        bytes[6..36].copy_from_slice(topic.as_bytes());
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.topic_filter.len(), 30);
    }

    #[test]
    fn test_roundtrip_minimal_packet() {
        let bytes = [0xA2, 0x05, 0x00, 0x01, 0x00, 0x01, 0x61];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 1);
        assert_eq!(packet.topic_filter.as_str(), "a");
    }

    #[test]
    fn test_roundtrip_normal_packet() {
        let bytes = [
            0xA2, 0x10, 0x12, 0x34, 0x00, 0x0C, 0x73, 0x65, 0x6E, 0x73, 0x6F, 0x72, 0x73, 0x2F,
            0x74, 0x65, 0x6D, 0x70,
        ];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 0x1234);
        assert_eq!(packet.topic_filter.as_str(), "sensors/temp");
    }

    #[test]
    fn test_roundtrip_max_packet_id() {
        let bytes = [
            0xA2, 0x10, 0xFF, 0xFF, 0x00, 0x0C, 0x73, 0x65, 0x6E, 0x73, 0x6F, 0x72, 0x73, 0x2F,
            0x74, 0x65, 0x6D, 0x70,
        ];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 65535);
        assert_eq!(packet.topic_filter.as_str(), "sensors/temp");
    }

    #[test]
    fn test_roundtrip_unicode_packet() {
        let bytes = [
            0xA2, 0x0A, 0x00, 0x64, 0x00, 0x06, 0xC3, 0xB1, 0xC3, 0xA1, 0xC3, 0xA9,
        ];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 100);
        assert_eq!(packet.topic_filter.as_str(), "ñáé");
    }

    #[test]
    fn test_roundtrip_wildcard_packet() {
        let bytes = [
            0xA2, 0x0D, 0x01, 0xF4, 0x00, 0x09, 0x73, 0x65, 0x6E, 0x73, 0x6F, 0x72, 0x73, 0x2F,
            0x23,
        ];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 500);
        assert_eq!(packet.topic_filter.as_str(), "sensors/#");
    }
}
