use crate::protocol::packets::{PacketEncoder, PacketFlagsConst, PacketTypeConst};
use crate::protocol::qos::QoS;
use crate::{Error, PacketEncodingError, PacketType};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubAckPacket {
    pub packet_id: u16,
    pub granted_qos: QoS,
}

impl PacketTypeConst for SubAckPacket {
    const PACKET_TYPE: PacketType = PacketType::SubAck;
}

impl PacketFlagsConst for SubAckPacket {
    const PACKET_FLAGS: u8 = 0b0000;
}

impl PacketEncoder for SubAckPacket {
    fn encode(&self, buffer: &mut [u8]) -> Result<usize, PacketEncodingError> {
        if buffer.len() < 3 {
            return Err(Error::BufferTooSmall.into());
        }
        let pid_bytes = self.packet_id.to_be_bytes();
        buffer[0] = pid_bytes[0];
        buffer[1] = pid_bytes[1];
        buffer[2] = self.granted_qos as u8;
        Ok(3)
    }

    fn decode(bytes: &[u8]) -> Result<Self, PacketEncodingError> {
        if bytes.len() < 3 {
            return Err(Error::IncompletePacket.into());
        }
        let packet_id = u16::from_be_bytes([bytes[0], bytes[1]]);
        let qos_value = bytes[2];
        let granted_qos = QoS::from_u8(qos_value).ok_or(Error::InvalidSubAckQoS {
            invalid_qos: qos_value,
        })?;
        Ok(Self {
            packet_id,
            granted_qos,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::packet_error::PacketEncodingError;

    const MAX_PAYLOAD_SIZE: usize = 128;

    // ===== HELPER FUNCTIONS =====

    fn roundtrip_test(bytes: &[u8]) -> SubAckPacket {
        let result = SubAckPacket::decode(&bytes);
        assert!(result.is_ok(), "Failed to decode packet: {:?}", result.err());
        let packet = result.unwrap();
        let mut buffer = [0u8; MAX_PAYLOAD_SIZE];
        let encode_result = packet.encode(&mut buffer);
        assert!(encode_result.is_ok(), "Failed to encode packet: {:?}", encode_result.err());
        let encoded_size = encode_result.unwrap();
        assert_eq!(encoded_size, bytes.len(), "Encoded size mismatch");
        assert_eq!(&buffer[..encoded_size], bytes, "Encoded bytes mismatch");
        packet
    }

    fn decode_test(bytes: &[u8]) -> Result<SubAckPacket, PacketEncodingError> {
        SubAckPacket::decode(bytes)
    }

    // ===== STRUCT SIZE TEST =====

    #[test]
    fn test_suback_packet_struct_size() {
        // Should be 4 bytes: 2 for packet_id + 2 for alignment padding
        assert_eq!(core::mem::size_of::<SubAckPacket>(), 4);
    }

    // ===== PACKET BYTES SIZE TEST =====

    #[test]
    fn test_suback_packet_bytes_size() {
        // SubAck packet is always exactly 3 bytes
        let packet = SubAckPacket {
            packet_id: 1,
            granted_qos: QoS::AtMostOnce,
        };
        let mut buffer = [0u8; MAX_PAYLOAD_SIZE];
        let size = packet.encode(&mut buffer).unwrap();
        assert_eq!(size, 3);
    }

    // ===== PACKET ID FIELD TESTS =====

    #[test]
    fn test_packet_id_zero() {
        let bytes = [0x00, 0x00, 0x00];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 0);
    }

    #[test]
    fn test_packet_id_one() {
        let bytes = [0x00, 0x01, 0x00];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 1);
    }

    #[test]
    fn test_packet_id_normal_values() {
        // Test packet_id = 100
        let bytes = [0x00, 0x64, 0x01];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 100);

        // Test packet_id = 1000
        let bytes = [0x03, 0xE8, 0x02];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 1000);

        // Test packet_id = 12345
        let bytes = [0x30, 0x39, 0x00];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 12345);
    }

    #[test]
    fn test_packet_id_max() {
        let bytes = [0xFF, 0xFF, 0x01];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 65535);
    }

    #[test]
    fn test_packet_id_byte_order() {
        // Test big-endian encoding: packet_id 0x1234 should be encoded as [0x12, 0x34]
        let bytes = [0x12, 0x34, 0x00];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 0x1234);

        // Verify encoding preserves big-endian byte order
        let mut buffer = [0u8; MAX_PAYLOAD_SIZE];
        let _encoded = packet.encode(&mut buffer).unwrap();
        assert_eq!(buffer[0], 0x12);
        assert_eq!(buffer[1], 0x34);
    }

    // ===== GRANTED QoS FIELD TESTS =====

    #[test]
    fn test_granted_qos_0() {
        let bytes = [0x00, 0x01, 0x00];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.granted_qos, QoS::AtMostOnce);
    }

    #[test]
    fn test_granted_qos_1() {
        let bytes = [0x00, 0x01, 0x01];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.granted_qos, QoS::AtLeastOnce);
    }

    #[test]
    fn test_granted_qos_2() {
        let bytes = [0x00, 0x01, 0x02];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.granted_qos, QoS::ExactlyOnce);
    }

    #[test]
    fn test_qos_encode_decode_roundtrip() {
        // Test all valid QoS values roundtrip correctly
        for qos_value in 0..=2 {
            let bytes = [0x12, 0x34, qos_value];
            let packet = roundtrip_test(&bytes);

            let expected_qos = match qos_value {
                0 => QoS::AtMostOnce,
                1 => QoS::AtLeastOnce,
                2 => QoS::ExactlyOnce,
                _ => unreachable!(),
            };
            assert_eq!(packet.granted_qos, expected_qos);
        }
    }

    #[test]
    fn test_qos_byte_representation() {
        // Verify QoS encodes as correct u8 values
        let packet_q0 = SubAckPacket {
            packet_id: 1,
            granted_qos: QoS::AtMostOnce,
        };
        let mut buffer = [0u8; 3];
        packet_q0.encode(&mut buffer).unwrap();
        assert_eq!(buffer[2], 0);

        let packet_q1 = SubAckPacket {
            packet_id: 1,
            granted_qos: QoS::AtLeastOnce,
        };
        packet_q1.encode(&mut buffer).unwrap();
        assert_eq!(buffer[2], 1);

        let packet_q2 = SubAckPacket {
            packet_id: 1,
            granted_qos: QoS::ExactlyOnce,
        };
        packet_q2.encode(&mut buffer).unwrap();
        assert_eq!(buffer[2], 2);
    }

    // ===== ERROR CONDITION TESTS: BUFFER SIZE =====

    #[test]
    fn test_encode_buffer_too_small_0_bytes() {
        let packet = SubAckPacket {
            packet_id: 1,
            granted_qos: QoS::AtMostOnce,
        };
        let mut buffer = [0u8; 0];
        let result = packet.encode(&mut buffer);
        assert!(matches!(result, Err(PacketEncodingError::Other)));
    }

    #[test]
    fn test_encode_buffer_too_small_1_byte() {
        let packet = SubAckPacket {
            packet_id: 1,
            granted_qos: QoS::AtMostOnce,
        };
        let mut buffer = [0u8; 1];
        let result = packet.encode(&mut buffer);
        assert!(matches!(result, Err(PacketEncodingError::Other)));
    }

    #[test]
    fn test_encode_buffer_too_small_2_bytes() {
        let packet = SubAckPacket {
            packet_id: 1,
            granted_qos: QoS::AtMostOnce,
        };
        let mut buffer = [0u8; 2];
        let result = packet.encode(&mut buffer);
        assert!(matches!(result, Err(PacketEncodingError::Other)));
    }

    #[test]
    fn test_encode_buffer_exactly_3_bytes() {
        let packet = SubAckPacket {
            packet_id: 1,
            granted_qos: QoS::AtMostOnce,
        };
        let mut buffer = [0u8; 3];
        let result = packet.encode(&mut buffer);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 3);
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

    // ===== ERROR CONDITION TESTS: INVALID QoS =====

    #[test]
    fn test_decode_invalid_qos_3() {
        // QoS value 3 is the first invalid value
        let bytes = [0x00, 0x01, 0x03];
        let result = decode_test(&bytes);
        assert!(matches!(result, Err(PacketEncodingError::Other)));
    }

    #[test]
    fn test_decode_invalid_qos_4() {
        let bytes = [0x00, 0x01, 0x04];
        let result = decode_test(&bytes);
        assert!(matches!(result, Err(PacketEncodingError::Other)));
    }

    #[test]
    fn test_decode_invalid_qos_255() {
        // Maximum u8 value
        let bytes = [0x00, 0x01, 0xFF];
        let result = decode_test(&bytes);
        assert!(matches!(result, Err(PacketEncodingError::Other)));
    }

    // ===== COMPLETE PACKET ROUNDTRIP TESTS =====

    #[test]
    fn test_roundtrip_qos0_packet_id_1() {
        let bytes = [0x00, 0x01, 0x00];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 1);
        assert_eq!(packet.granted_qos, QoS::AtMostOnce);
    }

    #[test]
    fn test_roundtrip_qos1_packet_id_1234() {
        let bytes = [0x12, 0x34, 0x01];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 0x1234);
        assert_eq!(packet.granted_qos, QoS::AtLeastOnce);
    }

    #[test]
    fn test_roundtrip_qos2_packet_id_65535() {
        let bytes = [0xFF, 0xFF, 0x02];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 65535);
        assert_eq!(packet.granted_qos, QoS::ExactlyOnce);
    }

    // ===== PROTOCOL COMPLIANCE TESTS =====

    #[test]
    fn test_packet_type_constant() {
        // Verify PACKET_TYPE is correctly set to SubAck (0x90)
        assert_eq!(SubAckPacket::PACKET_TYPE, PacketType::SubAck);
    }

    #[test]
    fn test_packet_flags_constant() {
        // Verify PACKET_FLAGS is correctly set to 0b0000
        assert_eq!(SubAckPacket::PACKET_FLAGS, 0b0000);
    }
}
