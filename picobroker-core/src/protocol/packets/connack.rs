use crate::protocol::packets::{PacketEncoder, PacketFixedSize, PacketFlagsConst, PacketHeader, PacketTypeConst};
use crate::{read_variable_length, PacketEncodingError, PacketType};

#[repr(u8)]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConnectReturnCode {
    Accepted = 0,
    UnacceptableProtocolVersion = 1,
    IdentifierRejected = 2,
    ServerUnavailable = 3,
    BadUserNameOrPassword = 4,
    NotAuthorized = 5,
}

impl TryFrom<u8> for ConnectReturnCode {
    type Error = PacketEncodingError;

    fn try_from(code: u8) -> Result<Self, Self::Error> {
        match code {
            0 => Ok(ConnectReturnCode::Accepted),
            1 => Ok(ConnectReturnCode::UnacceptableProtocolVersion),
            2 => Ok(ConnectReturnCode::IdentifierRejected),
            3 => Ok(ConnectReturnCode::ServerUnavailable),
            4 => Ok(ConnectReturnCode::BadUserNameOrPassword),
            5 => Ok(ConnectReturnCode::NotAuthorized),
            _ => Err(PacketEncodingError::MalformedPacket),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnAckPacket {
    pub session_present: bool,
    pub return_code: ConnectReturnCode,
}

impl PacketFixedSize for ConnAckPacket {
    const PACKET_SIZE: usize = 4;
}

impl PacketTypeConst for ConnAckPacket {
    const PACKET_TYPE: PacketType = PacketType::ConnAck;
}

impl PacketFlagsConst for ConnAckPacket {
    const PACKET_FLAGS: u8 = 0b0000;
}

impl PacketEncoder for ConnAckPacket {
    fn encode(&self, buffer: &mut [u8]) -> Result<usize, PacketEncodingError> {
        Self::validate_buffer_size(buffer.len())?;
        buffer[0] = self.header_first_byte();
        buffer[1] = 2u8;
        buffer[2] = if self.session_present { 0b0000_0001 } else { 0b0000_0000 };
        buffer[3] = self.return_code as u8;
        Ok(4)
    }

    fn decode(bytes: &[u8]) -> Result<Self, PacketEncodingError> {
        Self::validate_buffer_size(bytes.len())?;
        Self::validate_packet_type(bytes[0])?;
        let (remaining_length, _) = read_variable_length(&bytes[1..])?;
        Self::validate_remaining_length(remaining_length)?;
        let session_present_byte = bytes[2];
        let session_present = match session_present_byte {
            0b0000_0000 => false,
            0b0000_0001 => true,
            _ => return Err(PacketEncodingError::MalformedPacket),
        };
        let return_code = ConnectReturnCode::try_from(bytes[3])?;
        Ok(ConnAckPacket {
            session_present,
            return_code,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::ConnAckPacket;

    #[test]
    fn test_disconnect_packet_struct_size() {
        assert_eq!(size_of::<ConnAckPacket>(), 2);
    }
}
