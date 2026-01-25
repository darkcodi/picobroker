use crate::protocol::packets::{PacketEncoder, PacketFlagsConst, PacketTypeConst};
use crate::protocol::qos::QoS;
use crate::{PacketEncodingError, PacketType};

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
            return Err(PacketEncodingError::BufferTooSmall {
                buffer_size: buffer.len(),
            });
        }
        let pid_bytes = self.packet_id.to_be_bytes();
        buffer[0] = pid_bytes[0];
        buffer[1] = pid_bytes[1];
        buffer[2] = self.granted_qos as u8;
        Ok(3)
    }

    fn decode(bytes: &[u8]) -> Result<Self, PacketEncodingError> {
        if bytes.len() < 3 {
            return Err(PacketEncodingError::IncompletePacket {
                buffer_size: bytes.len(),
            });
        }
        let packet_id = u16::from_be_bytes([bytes[0], bytes[1]]);
        let qos_value = bytes[2];
        let granted_qos = QoS::from_u8(qos_value)?;
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
        let result = SubAckPacket::decode(bytes);
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

    #[allow(dead_code)]
    fn decode_test(bytes: &[u8]) -> Result<SubAckPacket, PacketEncodingError> {
        SubAckPacket::decode(bytes)
    }

    // ===== STRUCT SIZE TEST =====

    #[test]
    fn test_suback_packet_struct_size() {
        // Should be 4 bytes: 2 for packet_id + 2 for alignment padding
        assert_eq!(core::mem::size_of::<SubAckPacket>(), 4);
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
    fn test_packet_id_max() {
        let bytes = [0xFF, 0xFF, 0x01];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 65535);
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
}
