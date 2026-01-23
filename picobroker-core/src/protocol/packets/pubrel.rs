use crate::protocol::packets::{PacketEncoder, PacketFixedSize, PacketFlagsConst, PacketHeader, PacketTypeConst};
use crate::{read_variable_length, PacketEncodingError, PacketType};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PubRelPacket {
    pub packet_id: u16,
}

impl PacketFixedSize for PubRelPacket {
    const PACKET_SIZE: usize = 4;
}

impl PacketTypeConst for PubRelPacket {
    const PACKET_TYPE: PacketType = PacketType::PubRel;
}

impl PacketFlagsConst for PubRelPacket {
    const PACKET_FLAGS: u8 = 0b0010;
}

impl PacketEncoder for PubRelPacket {
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
