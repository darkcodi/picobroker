use crate::protocol::packets::{PacketEncoder, PacketFixedSize, PacketFlagsConst, PacketHeader, PacketTypeConst};
use crate::{read_variable_length, PacketEncodingError, PacketType};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PingReqPacket;

impl PacketFixedSize for PingReqPacket {
    const PACKET_SIZE: usize = 2;
}

impl PacketTypeConst for PingReqPacket {
    const PACKET_TYPE: PacketType = PacketType::PingReq;
}

impl PacketFlagsConst for PingReqPacket {
    const PACKET_FLAGS: u8 = 0b0000;
}

impl PacketEncoder for PingReqPacket {
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

#[cfg(test)]
mod tests {
    use crate::PingReqPacket;

    #[test]
    fn test_disconnect_packet_struct_size() {
        assert_eq!(size_of::<PingReqPacket>(), 0);
    }
}

