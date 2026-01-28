use crate::protocol::packet_type::PacketType;
use crate::protocol::packets::{PacketEncoder, PacketFlagsConst, PacketHeader, PacketTypeConst};
use crate::protocol::qos::QoS;
use crate::protocol::utils::read_variable_length;
use crate::protocol::ProtocolError;

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
    fn encode(&self, buffer: &mut [u8]) -> Result<usize, ProtocolError> {
        if buffer.len() < 5 {
            return Err(ProtocolError::BufferTooSmall {
                buffer_size: buffer.len(),
            });
        }
        buffer[0] = self.header_first_byte();
        buffer[1] = 3u8; // Remaining length: packet_id (2 bytes) + granted_qos (1 byte)
        let pid_bytes = self.packet_id.to_be_bytes();
        buffer[2] = pid_bytes[0];
        buffer[3] = pid_bytes[1];
        buffer[4] = self.granted_qos as u8;
        Ok(5)
    }

    fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        if bytes.len() < 5 {
            return Err(ProtocolError::IncompletePacket {
                available: bytes.len(),
            });
        }
        Self::validate_packet_type(bytes[0])?;
        let (remaining_length, _) = read_variable_length(&bytes[1..])?;
        if remaining_length != 3 {
            return Err(ProtocolError::InvalidPacketLength {
                expected: 3,
                actual: remaining_length,
            });
        }
        let packet_id = u16::from_be_bytes([bytes[2], bytes[3]]);
        let qos_value = bytes[4];
        let granted_qos = QoS::from_u8(qos_value)?;
        Ok(Self {
            packet_id,
            granted_qos,
        })
    }
}

impl core::fmt::Display for SubAckPacket {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "SubAckPacket {{ packet_id: {}, granted_qos: {:?} }}",
            self.packet_id, self.granted_qos
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::protocol::ProtocolError;

    const MAX_PAYLOAD_SIZE: usize = 128;

    // ===== HELPER FUNCTIONS =====

    fn roundtrip_test(bytes: &[u8]) -> SubAckPacket {
        let result = SubAckPacket::decode(bytes);
        assert!(
            result.is_ok(),
            "Failed to decode packet: {:?}",
            result.err()
        );
        let packet = result.unwrap();
        let mut buffer = [0u8; MAX_PAYLOAD_SIZE];
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
    fn decode_test(bytes: &[u8]) -> Result<SubAckPacket, ProtocolError> {
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
        let bytes = [0x90, 0x03, 0x00, 0x00, 0x00];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 0);
    }

    #[test]
    fn test_packet_id_one() {
        let bytes = [0x90, 0x03, 0x00, 0x01, 0x00];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 1);
    }

    #[test]
    fn test_packet_id_max() {
        let bytes = [0x90, 0x03, 0xFF, 0xFF, 0x01];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 65535);
    }

    // ===== GRANTED QoS FIELD TESTS =====

    #[test]
    fn test_granted_qos_0() {
        let bytes = [0x90, 0x03, 0x00, 0x01, 0x00];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.granted_qos, QoS::AtMostOnce);
    }

    #[test]
    fn test_granted_qos_1() {
        let bytes = [0x90, 0x03, 0x00, 0x01, 0x01];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.granted_qos, QoS::AtLeastOnce);
    }

    #[test]
    fn test_granted_qos_2() {
        let bytes = [0x90, 0x03, 0x00, 0x01, 0x02];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.granted_qos, QoS::ExactlyOnce);
    }

    // ===== COMPLETE PACKET ROUNDTRIP TESTS =====

    #[test]
    fn test_roundtrip_qos0_packet_id_1() {
        let bytes = [0x90, 0x03, 0x00, 0x01, 0x00];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 1);
        assert_eq!(packet.granted_qos, QoS::AtMostOnce);
    }

    #[test]
    fn test_roundtrip_qos1_packet_id_1234() {
        let bytes = [0x90, 0x03, 0x12, 0x34, 0x01];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 0x1234);
        assert_eq!(packet.granted_qos, QoS::AtLeastOnce);
    }

    #[test]
    fn test_roundtrip_qos2_packet_id_65535() {
        let bytes = [0x90, 0x03, 0xFF, 0xFF, 0x02];
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.packet_id, 65535);
        assert_eq!(packet.granted_qos, QoS::ExactlyOnce);
    }

    // ===== PROTOCOL COMPLIANCE TESTS =====
}
