use crate::protocol::packets::PacketEncoder;
use crate::{Error, PacketType};

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

impl From<u8> for ConnectReturnCode {
    fn from(code: u8) -> Self {
        match code {
            0 => ConnectReturnCode::Accepted,
            1 => ConnectReturnCode::UnacceptableProtocolVersion,
            2 => ConnectReturnCode::IdentifierRejected,
            3 => ConnectReturnCode::ServerUnavailable,
            4 => ConnectReturnCode::BadUserNameOrPassword,
            5 => ConnectReturnCode::NotAuthorized,
            _ => ConnectReturnCode::ServerUnavailable, // Default case for unknown codes
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnAck {
    pub session_present: bool,
    pub return_code: ConnectReturnCode,
}

impl<'a> PacketEncoder<'a> for ConnAck {
    fn packet_type(&self) -> PacketType {
        PacketType::ConnAck
    }

    fn fixed_flags(&'a self) -> u8 {
        0b0000
    }

    fn encode(&self, buffer: &mut [u8]) -> Result<usize, Error> {
        if buffer.len() < 2 {
            return Err(Error::BufferTooSmall);
        }
        buffer[0] = if self.session_present { 0x01 } else { 0x00 };
        buffer[1] = self.return_code as u8;
        Ok(2)
    }

    fn decode(payload: &[u8], _header: u8) -> Result<Self, Error> {
        if payload.len() < 2 {
            return Err(Error::IncompletePacket);
        }
        let session_present = (payload[0] & 0x01) != 0;
        let return_code = ConnectReturnCode::from(payload[1]);
        Ok(Self {
            session_present,
            return_code,
        })
    }
}
