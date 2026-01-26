use crate::protocol::packet_error::PacketEncodingError;
use crate::protocol::packet_type::PacketType;
use crate::protocol::packets::{
    PacketEncoder, PacketFixedSize, PacketFlagsConst, PacketHeader, PacketTypeConst,
};
use crate::protocol::utils::read_variable_length;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsubAckPacket {
    pub packet_id: u16,
}

impl PacketFixedSize for UnsubAckPacket {
    const PACKET_SIZE: usize = 4;
}

impl PacketTypeConst for UnsubAckPacket {
    const PACKET_TYPE: PacketType = PacketType::UnsubAck;
}

impl PacketFlagsConst for UnsubAckPacket {
    const PACKET_FLAGS: u8 = 0b0000;
}

impl PacketEncoder for UnsubAckPacket {
    fn encode(&self, buffer: &mut [u8]) -> Result<usize, PacketEncodingError> {
        Self::validate_buffer_size(buffer.len())?;
        buffer[0] = self.header_first_byte();
        buffer[1] = 2u8;
        let pid_bytes = self.packet_id.to_be_bytes();
        buffer[2] = pid_bytes[0];
        buffer[3] = pid_bytes[1];
        Ok(4)
    }

    fn decode(bytes: &[u8]) -> Result<Self, PacketEncodingError> {
        Self::validate_buffer_size(bytes.len())?;
        Self::validate_packet_type(bytes[0])?;
        let (remaining_length, _) = read_variable_length(&bytes[1..])?;
        Self::validate_remaining_length(remaining_length)?;
        let packet_id = u16::from_be_bytes([bytes[2], bytes[3]]);
        Ok(Self { packet_id })
    }
}

impl core::fmt::Display for UnsubAckPacket {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "UnsubAckPacket {{ packet_id: {} }}", self.packet_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MAX_PAYLOAD_SIZE: usize = 128;

    #[test]
    fn test_unsuback_packet_struct_size() {
        assert_eq!(size_of::<UnsubAckPacket>(), 2);
    }

    #[test]
    fn test_unsuback_packet_roundtrip() {
        let packet = roundtrip_test(&[0xB0, 0x02, 0x12, 0x34]);
        assert_eq!(packet.packet_id, 0x1234);
    }

    fn roundtrip_test(bytes: &[u8]) -> UnsubAckPacket {
        let result = UnsubAckPacket::decode(bytes);
        assert!(result.is_ok(), "Failed to decode packet");
        let packet = result.unwrap();
        let mut buffer = [0u8; MAX_PAYLOAD_SIZE];
        let encode_result = packet.encode(&mut buffer);
        assert!(encode_result.is_ok(), "Failed to encode packet");
        let encoded_size = encode_result.unwrap();
        assert_eq!(encoded_size, bytes.len(), "Encoded size mismatch");
        assert_eq!(&buffer[..encoded_size], bytes, "Encoded bytes mismatch");
        packet
    }
}
