use crate::protocol::packets::{
    PacketEncoder, PacketFixedSize, PacketFlagsConst, PacketHeader, PacketTypeConst,
};
use crate::protocol::packet_error::PacketEncodingError;
use crate::protocol::packet_type::PacketType;
use crate::protocol::utils::read_variable_length;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PingRespPacket;

impl PacketFixedSize for PingRespPacket {
    const PACKET_SIZE: usize = 2;
}

impl PacketTypeConst for PingRespPacket {
    const PACKET_TYPE: PacketType = PacketType::PingResp;
}

impl PacketFlagsConst for PingRespPacket {
    const PACKET_FLAGS: u8 = 0b0000;
}

impl PacketEncoder for PingRespPacket {
    fn encode(&self, buffer: &mut [u8]) -> Result<usize, PacketEncodingError> {
        Self::validate_buffer_size(buffer.len())?;
        buffer[0] = self.header_first_byte();
        buffer[1] = 0u8; // Remaining Length is 0
        Ok(2)
    }

    fn decode(bytes: &[u8]) -> Result<Self, PacketEncodingError> {
        Self::validate_buffer_size(bytes.len())?;
        Self::validate_packet_type(bytes[0])?;
        let (remaining_length, _) = read_variable_length(&bytes[1..])?;
        Self::validate_remaining_length(remaining_length)?;
        Ok(Self)
    }
}

impl core::fmt::Display for PingRespPacket {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "PingRespPacket {{ }}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::size_of;

    const MAX_PAYLOAD_SIZE: usize = 128;

    #[test]
    fn test_pingresp_packet_struct_size() {
        assert_eq!(size_of::<PingRespPacket>(), 0);
    }

    #[test]
    fn test_pingresp_packet_roundtrip() {
        assert_eq!(roundtrip_test(&[0xD0, 0x00]), PingRespPacket);
    }

    fn roundtrip_test(bytes: &[u8]) -> PingRespPacket {
        let result = PingRespPacket::decode(bytes);
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
