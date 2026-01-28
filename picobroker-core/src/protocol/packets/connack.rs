use crate::protocol::ProtocolError;
use crate::protocol::packet_type::PacketType;
use crate::protocol::packets::{
    PacketEncoder, PacketFixedSize, PacketFlagsConst, PacketHeader, PacketTypeConst,
};
use crate::protocol::utils::read_variable_length;

#[repr(u8)]
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub enum ConnectReturnCode {
    #[default]
    Accepted = 0,
    UnacceptableProtocolVersion = 1,
    IdentifierRejected = 2,
    ServerUnavailable = 3,
    BadUserNameOrPassword = 4,
    NotAuthorized = 5,
}

impl TryFrom<u8> for ConnectReturnCode {
    type Error = ProtocolError;

    fn try_from(code: u8) -> Result<Self, Self::Error> {
        match code {
            0 => Ok(ConnectReturnCode::Accepted),
            1 => Ok(ConnectReturnCode::UnacceptableProtocolVersion),
            2 => Ok(ConnectReturnCode::IdentifierRejected),
            3 => Ok(ConnectReturnCode::ServerUnavailable),
            4 => Ok(ConnectReturnCode::BadUserNameOrPassword),
            5 => Ok(ConnectReturnCode::NotAuthorized),
            _ => Err(ProtocolError::InvalidConnectReturnCode { return_code: code }),
        }
    }
}

#[derive(Debug, Default, Clone, PartialEq, Eq)]
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
    fn encode(&self, buffer: &mut [u8]) -> Result<usize, ProtocolError> {
        Self::validate_buffer_size(buffer.len())?;
        buffer[0] = self.header_first_byte();
        buffer[1] = 2u8;
        buffer[2] = if self.session_present {
            0b0000_0001
        } else {
            0b0000_0000
        };
        buffer[3] = self.return_code as u8;
        Ok(4)
    }

    fn decode(bytes: &[u8]) -> Result<Self, ProtocolError> {
        Self::validate_buffer_size(bytes.len())?;
        Self::validate_packet_type(bytes[0])?;
        let (remaining_length, _) = read_variable_length(&bytes[1..])?;
        Self::validate_remaining_length(remaining_length)?;
        let session_present_byte = bytes[2];
        let session_present = match session_present_byte {
            0b0000_0000 => false,
            0b0000_0001 => true,
            _ => {
                return Err(ProtocolError::InvalidSessionPresentFlag {
                    flag: session_present_byte,
                })
            }
        };
        let return_code = ConnectReturnCode::try_from(bytes[3])?;
        Ok(Self {
            session_present,
            return_code,
        })
    }
}

impl core::fmt::Display for ConnAckPacket {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "ConnAckPacket {{ session_present: {}, return_code: {:?} }}",
            self.session_present, self.return_code
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use core::mem::size_of;

    const MAX_PAYLOAD_SIZE: usize = 128;

    #[test]
    fn test_connack_packet_struct_size() {
        assert_eq!(size_of::<ConnAckPacket>(), 2);
    }

    #[test]
    fn test_connack_packet_roundtrip() {
        assert!(!roundtrip_test(&[0x20, 0x02, 0x00, 0x00]).session_present);
        assert!(roundtrip_test(&[0x20, 0x02, 0x01, 0x00]).session_present);
        assert_eq!(
            roundtrip_test(&[0x20, 0x02, 0x00, 0x00]).return_code,
            ConnectReturnCode::Accepted
        );
        assert_eq!(
            roundtrip_test(&[0x20, 0x02, 0x00, 0x01]).return_code,
            ConnectReturnCode::UnacceptableProtocolVersion
        );
        assert_eq!(
            roundtrip_test(&[0x20, 0x02, 0x00, 0x02]).return_code,
            ConnectReturnCode::IdentifierRejected
        );
        assert_eq!(
            roundtrip_test(&[0x20, 0x02, 0x00, 0x03]).return_code,
            ConnectReturnCode::ServerUnavailable
        );
        assert_eq!(
            roundtrip_test(&[0x20, 0x02, 0x00, 0x04]).return_code,
            ConnectReturnCode::BadUserNameOrPassword
        );
        assert_eq!(
            roundtrip_test(&[0x20, 0x02, 0x00, 0x05]).return_code,
            ConnectReturnCode::NotAuthorized
        );
    }

    fn roundtrip_test(bytes: &[u8]) -> ConnAckPacket {
        let result = ConnAckPacket::decode(bytes);
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
