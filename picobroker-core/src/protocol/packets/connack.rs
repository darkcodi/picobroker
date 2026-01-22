use crate::protocol::packets::PacketEncoder;
use crate::{PacketEncodingError, PacketType};

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

impl PacketEncoder for ConnAckPacket {
    fn packet_type(&self) -> PacketType {
        PacketType::ConnAck
    }

    fn fixed_flags(&self) -> u8 {
        0b0000
    }

    fn encode(&self, buffer: &mut [u8]) -> Result<usize, PacketEncodingError> {
        let header_byte = self.header_first_byte();
        let remaining_length = 2u8;
        if buffer.len() < 4 {
            return Err(PacketEncodingError::BufferTooSmall);
        }
        buffer[0] = header_byte;
        buffer[1] = remaining_length;
        buffer[2] = if self.session_present { 0b0000_0001 } else { 0b0000_0000 };
        buffer[3] = self.return_code as u8;
        Ok(4)
    }

    fn decode(bytes: &[u8]) -> Result<Self, PacketEncodingError> {
        if bytes.len() < 4 {
            return Err(PacketEncodingError::BufferTooSmall);
        }
        let type_byte = bytes[0] >> 4;
        let packet_type = PacketType::from(type_byte);
        if packet_type != PacketType::ConnAck {
            return Err(PacketEncodingError::InvalidPacketType { packet_type: type_byte });
        }
        let remaining_length = bytes[1];
        if remaining_length != 2 {
            return Err(PacketEncodingError::InvalidPacketLength { actual: 2 + remaining_length as usize, expected: 4 });
        }
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
