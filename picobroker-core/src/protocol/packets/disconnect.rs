use crate::protocol::packets::PacketEncoder;
use crate::{PacketEncodingError, PacketType};

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DisconnectPacket;

impl PacketEncoder for DisconnectPacket {
    fn packet_type(&self) -> PacketType {
        PacketType::Disconnect
    }

    fn fixed_flags(&self) -> u8 {
        0b0000
    }

    fn encode(&self, buffer: &mut [u8]) -> Result<usize, PacketEncodingError> {
        let header_byte = self.header_first_byte();
        let remaining_length = 0u8;
        if buffer.len() < 2 {
            return Err(PacketEncodingError::BufferTooSmall);
        }
        buffer[0] = header_byte;
        buffer[1] = remaining_length;
        Ok(2)
    }

    fn decode(bytes: &[u8]) -> Result<Self, PacketEncodingError> {
        if bytes.len() < 2 {
            return Err(PacketEncodingError::BufferTooSmall);
        }
        let type_byte = bytes[0] >> 4;
        let packet_type = PacketType::from(type_byte);
        if packet_type != PacketType::Disconnect {
            return Err(PacketEncodingError::InvalidPacketType { packet_type: type_byte });
        }
        let remaining_length = bytes[1];
        if remaining_length != 0 {
            return Err(PacketEncodingError::Other);
        }
        Ok(DisconnectPacket)
    }
}

#[cfg(test)]
mod tests {
    use crate::DisconnectPacket;

    #[test]
    fn test_disconnect_packet_struct_size() {
        assert_eq!(size_of::<DisconnectPacket>(), 0);
    }
}
