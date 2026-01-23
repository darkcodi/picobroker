mod connack;
mod connect;
mod disconnect;
mod pingreq;
mod pingresp;
mod puback;
mod pubcomp;
mod publish;
mod pubrec;
mod pubrel;
mod suback;
mod subscribe;
mod unsuback;
mod unsubscribe;

use crate::{variable_length_length, PacketEncodingError, PacketType};
pub use crate::protocol::packets::connack::ConnAckPacket;
pub use crate::protocol::packets::connect::ConnectPacket;
pub use crate::protocol::packets::disconnect::DisconnectPacket;
pub use crate::protocol::packets::pingreq::PingReqPacket;
pub use crate::protocol::packets::pingresp::PingRespPacket;
pub use crate::protocol::packets::puback::PubAckPacket;
pub use crate::protocol::packets::pubcomp::PubCompPacket;
pub use crate::protocol::packets::publish::PublishPacket;
pub use crate::protocol::packets::pubrec::PubRecPacket;
pub use crate::protocol::packets::pubrel::PubRelPacket;
pub use crate::protocol::packets::suback::SubAckPacket;
pub use crate::protocol::packets::subscribe::SubscribePacket;
pub use crate::protocol::packets::unsuback::UnsubAckPacket;
pub use crate::protocol::packets::unsubscribe::UnsubscribePacket;

pub trait PacketTypeConst {
    const PACKET_TYPE: PacketType;

    fn validate_packet_type(header_first_byte: u8) -> Result<(), PacketEncodingError> {
        let type_byte = header_first_byte >> 4;
        let packet_type = PacketType::from(type_byte);
        if packet_type != Self::PACKET_TYPE {
            return Err(PacketEncodingError::InvalidPacketType { packet_type: type_byte });
        }
        Ok(())
    }
}

pub trait PacketTypeDynamic {
    fn packet_type(&self) -> PacketType;
}

impl<T: PacketTypeConst> PacketTypeDynamic for T {
    fn packet_type(&self) -> PacketType {
        Self::PACKET_TYPE
    }
}

pub trait PacketFlagsConst {
    const PACKET_FLAGS: u8;
}

pub trait PacketFlagsDynamic {
    fn flags(&self) -> u8;
}

impl <T: PacketFlagsConst> PacketFlagsDynamic for T {
    fn flags(&self) -> u8 {
        Self::PACKET_FLAGS
    }
}

trait PacketFixedSize {
    const PACKET_SIZE: usize;

    fn validate_buffer_size(buffer_size: usize) -> Result<(), PacketEncodingError> {
        if buffer_size < Self::PACKET_SIZE {
            return Err(PacketEncodingError::BufferTooSmall);
        }
        Ok(())
    }

    fn validate_remaining_length(remaining_length: usize) -> Result<(), PacketEncodingError> {
        let int_length = variable_length_length(remaining_length);
        let expected_remaining_length = Self::PACKET_SIZE - int_length - 1;
        if remaining_length != expected_remaining_length {
            return Err(PacketEncodingError::InvalidPacketLength {
                actual: remaining_length + int_length + 1,
                expected: Self::PACKET_SIZE,
            });
        }
        Ok(())
    }
}

pub trait PacketHeader {
    fn header_first_byte(&self) -> u8;
}

impl<T: PacketTypeDynamic + PacketFlagsDynamic> PacketHeader for T {
    fn header_first_byte(&self) -> u8 {
        (self.packet_type() as u8) << 4 | (self.flags() & 0x0F)
    }
}

pub trait PacketEncoder: Sized {
    fn encode(&self, buffer: &mut [u8]) -> Result<usize, PacketEncodingError>;
    fn decode(bytes: &[u8]) -> Result<Self, PacketEncodingError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Packet<const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize> {
    Connect(ConnectPacket<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>),
    ConnAck(ConnAckPacket),
    Publish(PublishPacket<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>),
    PubAck(PubAckPacket),
    PubRec(PubRecPacket),
    PubRel(PubRelPacket),
    PubComp(PubCompPacket),
    Subscribe(SubscribePacket<MAX_TOPIC_NAME_LENGTH>),
    SubAck(SubAckPacket),
    Unsubscribe(UnsubscribePacket<MAX_TOPIC_NAME_LENGTH>),
    UnsubAck(UnsubAckPacket),
    PingReq(PingReqPacket),
    PingResp(PingRespPacket),
    Disconnect(DisconnectPacket),
}

impl<const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize> PacketTypeDynamic for Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE> {
    fn packet_type(&self) -> PacketType {
        match self {
            Packet::Connect(_) => PacketType::Connect,
            Packet::ConnAck(_) => PacketType::ConnAck,
            Packet::Publish(_) => PacketType::Publish,
            Packet::PubAck(_) => PacketType::PubAck,
            Packet::PubRec(_) => PacketType::PubRec,
            Packet::PubRel(_) => PacketType::PubRel,
            Packet::PubComp(_) => PacketType::PubComp,
            Packet::Subscribe(_) => PacketType::Subscribe,
            Packet::SubAck(_) => PacketType::SubAck,
            Packet::Unsubscribe(_) => PacketType::Unsubscribe,
            Packet::UnsubAck(_) => PacketType::UnsubAck,
            Packet::PingReq(_) => PacketType::PingReq,
            Packet::PingResp(_) => PacketType::PingResp,
            Packet::Disconnect(_) => PacketType::Disconnect,
        }
    }
}

impl<const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize> PacketFlagsDynamic for Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE> {
    fn flags(&self) -> u8 {
        match self {
            Packet::Connect(packet) => packet.flags(),
            Packet::ConnAck(packet) => packet.flags(),
            Packet::Publish(packet) => packet.flags(),
            Packet::PubAck(packet) => packet.flags(),
            Packet::PubRec(packet) => packet.flags(),
            Packet::PubRel(packet) => packet.flags(),
            Packet::PubComp(packet) => packet.flags(),
            Packet::Subscribe(packet) => packet.flags(),
            Packet::SubAck(packet) => packet.flags(),
            Packet::Unsubscribe(packet) => packet.flags(),
            Packet::UnsubAck(packet) => packet.flags(),
            Packet::PingReq(packet) => packet.flags(),
            Packet::PingResp(packet) => packet.flags(),
            Packet::Disconnect(packet) => packet.flags(),
        }
    }
}

impl<const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize> PacketEncoder for Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE> {
    fn encode(&self, buffer: &mut [u8]) -> Result<usize, PacketEncodingError> {
        match self {
            Packet::Connect(packet) => packet.encode(buffer),
            Packet::ConnAck(packet) => packet.encode(buffer),
            Packet::Publish(packet) => packet.encode(buffer),
            Packet::PubAck(packet) => packet.encode(buffer),
            Packet::PubRec(packet) => packet.encode(buffer),
            Packet::PubRel(packet) => packet.encode(buffer),
            Packet::PubComp(packet) => packet.encode(buffer),
            Packet::Subscribe(packet) => packet.encode(buffer),
            Packet::SubAck(packet) => packet.encode(buffer),
            Packet::Unsubscribe(packet) => packet.encode(buffer),
            Packet::UnsubAck(packet) => packet.encode(buffer),
            Packet::PingReq(packet) => packet.encode(buffer),
            Packet::PingResp(packet) => packet.encode(buffer),
            Packet::Disconnect(packet) => packet.encode(buffer),
        }
    }

    fn decode(bytes: &[u8]) -> Result<Self, PacketEncodingError> {
        let type_byte = bytes[0] >> 4;
        let packet_type = PacketType::from(type_byte);
        match packet_type {
            PacketType::Reserved => Err(PacketEncodingError::InvalidPacketType {
                packet_type: type_byte,
            }),
            PacketType::Connect => Ok(Packet::Connect(ConnectPacket::decode(bytes)?)),
            PacketType::ConnAck => Ok(Packet::ConnAck(ConnAckPacket::decode(bytes)?)),
            PacketType::Publish => Ok(Packet::Publish(PublishPacket::decode(bytes)?)),
            PacketType::PubAck => Ok(Packet::PubAck(PubAckPacket::decode(bytes)?)),
            PacketType::PubRec => Ok(Packet::PubRec(PubRecPacket::decode(bytes)?)),
            PacketType::PubRel => Ok(Packet::PubRel(PubRelPacket::decode(bytes)?)),
            PacketType::PubComp => Ok(Packet::PubComp(PubCompPacket::decode(bytes)?)),
            PacketType::Subscribe => Ok(Packet::Subscribe(SubscribePacket::decode(bytes)?)),
            PacketType::SubAck => Ok(Packet::SubAck(SubAckPacket::decode(bytes)?)),
            PacketType::Unsubscribe => Ok(Packet::Unsubscribe(UnsubscribePacket::decode(bytes)?)),
            PacketType::UnsubAck => Ok(Packet::UnsubAck(UnsubAckPacket::decode(bytes)?)),
            PacketType::PingReq => Ok(Packet::PingReq(PingReqPacket::decode(bytes)?)),
            PacketType::PingResp => Ok(Packet::PingResp(PingRespPacket::decode(bytes)?)),
            PacketType::Disconnect => Ok(Packet::Disconnect(DisconnectPacket::decode(bytes)?)),
            PacketType::Reserved2 => Err(PacketEncodingError::InvalidPacketType {
                packet_type: type_byte,
            }),
        }
    }
}

#[cfg(test)]
mod tests {
    use core::any::TypeId;
    use crate::protocol::packets::connack::ConnectReturnCode;
    use crate::protocol::packets::connect::ConnectFlags;
    use crate::protocol::packet_error::PacketEncodingError;
    use super::*;

    const MAX_TOPIC_NAME_LENGTH: usize = 30;
    const MAX_PAYLOAD_SIZE: usize = 128;
    type DefaultPacket = Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>;

    #[test]
    fn test_connack_packet_roundtrip() {
        assert_eq!(roundtrip_test::<ConnAckPacket>(&[0x20, 0x02, 0x00, 0x00]).session_present, false);
        assert_eq!(roundtrip_test::<ConnAckPacket>(&[0x20, 0x02, 0x01, 0x00]).session_present, true);
        assert_eq!(roundtrip_test::<ConnAckPacket>(&[0x20, 0x02, 0x00, 0x00]).return_code, ConnectReturnCode::Accepted);
        assert_eq!(roundtrip_test::<ConnAckPacket>(&[0x20, 0x02, 0x00, 0x01]).return_code, ConnectReturnCode::UnacceptableProtocolVersion);
        assert_eq!(roundtrip_test::<ConnAckPacket>(&[0x20, 0x02, 0x00, 0x02]).return_code, ConnectReturnCode::IdentifierRejected);
        assert_eq!(roundtrip_test::<ConnAckPacket>(&[0x20, 0x02, 0x00, 0x03]).return_code, ConnectReturnCode::ServerUnavailable);
        assert_eq!(roundtrip_test::<ConnAckPacket>(&[0x20, 0x02, 0x00, 0x04]).return_code, ConnectReturnCode::BadUserNameOrPassword);
        assert_eq!(roundtrip_test::<ConnAckPacket>(&[0x20, 0x02, 0x00, 0x05]).return_code, ConnectReturnCode::NotAuthorized);
    }

    #[test]
    fn test_connect_packet_roundtrip() {
        let connect_bytes: [u8; 17] = [
            0x10, 0x0F, // Fixed header (remaining length = 15)
            0x00, 0x04, // Protocol Name Length
            0x4D, 0x51, 0x54, 0x54, // Protocol Name "MQTT"
            0x04, // Protocol Level
            0b0000_0010, // Connect Flags (Clean Session)
            0x00, 0x3C, // Keep Alive (60 seconds)
            0x00, 0x03, // Client ID Length
            0x61, 0x62, 0x63, // Client ID "abc"
        ];
        let packet = roundtrip_test::<ConnectPacket<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>>(&connect_bytes);
        assert_eq!(packet.client_id.as_str(), "abc");
        assert!(packet.connect_flags.contains(ConnectFlags::CLEAN_SESSION));
        assert_eq!(packet.keep_alive, 60);
    }

    #[test]
    fn test_connect_packet_invalid_remaining_length() {
        // Test with incorrect remaining length (should be 0x0F but we use 0x0C)
        let connect_bytes: [u8; 17] = [
            0x10, 0x0C, // Fixed header (WRONG remaining length = 12 instead of 15)
            0x00, 0x04, // Protocol Name Length
            0x4D, 0x51, 0x54, 0x54, // Protocol Name "MQTT"
            0x04, // Protocol Level
            0b0000_0010, // Connect Flags (Clean Session)
            0x00, 0x3C, // Keep Alive (60 seconds)
            0x00, 0x03, // Client ID Length
            0x61, 0x62, 0x63, // Client ID "abc"
        ];
        let result = ConnectPacket::<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>::decode(&connect_bytes);
        assert!(result.is_err(), "Should fail with invalid remaining length");
        match result {
            Err(PacketEncodingError::InvalidPacketLength { expected, actual }) => {
                assert_eq!(expected, 12);
                assert_eq!(actual, 15);
            },
            _ => panic!("Expected InvalidPacketLength error"),
        }
    }

    #[test]
    fn test_disconnect_packet_roundtrip() {
        assert_eq!(roundtrip_test::<DisconnectPacket>(&[0xE0, 0x00]), DisconnectPacket);
    }

    #[test]
    fn test_pingreq_packet_roundtrip() {
        assert_eq!(roundtrip_test::<PingReqPacket>(&[0xC0, 0x00]), PingReqPacket);
    }

    #[test]
    fn test_pingresp_packet_roundtrip() {
        assert_eq!(roundtrip_test::<PingRespPacket>(&[0xD0, 0x00]), PingRespPacket);
    }

    fn roundtrip_test<P: PacketEncoder + PartialEq + core::fmt::Debug + 'static>(bytes: &[u8]) -> P {
        let result = DefaultPacket::decode(&bytes);
        assert!(result.is_ok(), "Failed to decode packet");
        let packet = result.unwrap();
        let casted_packet = extract_packet_unsafe::<P>(packet);
        let mut buffer = [0u8; MAX_PAYLOAD_SIZE];
        let encode_result = casted_packet.encode(&mut buffer);
        assert!(encode_result.is_ok(), "Failed to encode packet");
        let encoded_size = encode_result.unwrap();
        assert_eq!(encoded_size, bytes.len(), "Encoded size mismatch");
        assert_eq!(&buffer[..encoded_size], bytes, "Encoded bytes mismatch");
        casted_packet
    }

    fn extract_packet_unsafe<P: PacketEncoder + Sized + 'static>(packet: DefaultPacket) -> P {
        // Get the expected TypeId once
        let expected_type_id = TypeId::of::<P>();

        // Use unsafe to manually handle the conversion after TypeId check
        match packet {
            DefaultPacket::Connect(packet) if TypeId::of::<ConnectPacket<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>>() == expected_type_id => {
                unsafe {
                    // SAFETY: We've verified the TypeId matches, so the types are compatible
                    let ptr = &packet as *const ConnectPacket<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE> as *const P;
                    ptr.read()
                }
            },
            DefaultPacket::ConnAck(packet) if TypeId::of::<ConnAckPacket>() == expected_type_id => {
                unsafe {
                    // SAFETY: We've verified the TypeId matches, so the types are compatible
                    let ptr = &packet as *const ConnAckPacket as *const P;
                    ptr.read()
                }
            },
            DefaultPacket::Publish(packet) if TypeId::of::<PublishPacket<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>>() == expected_type_id => {
                unsafe {
                    // SAFETY: We've verified the TypeId matches, so the types are compatible
                    let ptr = &packet as *const PublishPacket<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE> as *const P;
                    ptr.read()
                }
            },
            DefaultPacket::PubAck(packet) if TypeId::of::<PubAckPacket>() == expected_type_id => {
                unsafe {
                    // SAFETY: We've verified the TypeId matches, so the types are compatible
                    let ptr = &packet as *const PubAckPacket as *const P;
                    ptr.read()
                }
            },
            DefaultPacket::PubRec(packet) if TypeId::of::<PubRecPacket>() == expected_type_id => {
                unsafe {
                    // SAFETY: We've verified the TypeId matches, so the types are compatible
                    let ptr = &packet as *const PubRecPacket as *const P;
                    ptr.read()
                }
            },
            DefaultPacket::PubRel(packet) if TypeId::of::<PubRelPacket>() == expected_type_id => {
                unsafe {
                    // SAFETY: We've verified the TypeId matches, so the types are compatible
                    let ptr = &packet as *const PubRelPacket as *const P;
                    ptr.read()
                }
            },
            DefaultPacket::PubComp(packet) if TypeId::of::<PubCompPacket>() == expected_type_id => {
                unsafe {
                    // SAFETY: We've verified the TypeId matches, so the types are compatible
                    let ptr = &packet as *const PubCompPacket as *const P;
                    ptr.read()
                }
            },
            DefaultPacket::Subscribe(packet) if TypeId::of::<SubscribePacket<MAX_TOPIC_NAME_LENGTH>>() == expected_type_id => {
                unsafe {
                    // SAFETY: We've verified the TypeId matches, so the types are compatible
                    let ptr = &packet as *const SubscribePacket<MAX_TOPIC_NAME_LENGTH> as *const P;
                    ptr.read()
                }
            },
            DefaultPacket::SubAck(packet) if TypeId::of::<SubAckPacket>() == expected_type_id => {
                unsafe {
                    // SAFETY: We've verified the TypeId matches, so the types are compatible
                    let ptr = &packet as *const SubAckPacket as *const P;
                    ptr.read()
                }
            },
            DefaultPacket::Unsubscribe(packet) if TypeId::of::<UnsubscribePacket<MAX_TOPIC_NAME_LENGTH>>() == expected_type_id => {
                unsafe {
                    // SAFETY: We've verified the TypeId matches, so the types are compatible
                    let ptr = &packet as *const UnsubscribePacket<MAX_TOPIC_NAME_LENGTH> as *const P;
                    ptr.read()
                }
            },
            DefaultPacket::UnsubAck(packet) if TypeId::of::<UnsubAckPacket>() == expected_type_id => {
                unsafe {
                    // SAFETY: We've verified the TypeId matches, so the types are compatible
                    let ptr = &packet as *const UnsubAckPacket as *const P;
                    ptr.read()
                }
            },
            DefaultPacket::PingReq(packet) if TypeId::of::<PingReqPacket>() == expected_type_id => {
                unsafe {
                    // SAFETY: We've verified the TypeId matches, so the types are compatible
                    let ptr = &packet as *const PingReqPacket as *const P;
                    ptr.read()
                }
            },
            DefaultPacket::PingResp(packet) if TypeId::of::<PingRespPacket>() == expected_type_id => {
                unsafe {
                    // SAFETY: We've verified the TypeId matches, so the types are compatible
                    let ptr = &packet as *const PingRespPacket as *const P;
                    ptr.read()
                }
            },
            DefaultPacket::Disconnect(packet) if TypeId::of::<DisconnectPacket>() == expected_type_id => {
                unsafe {
                    // SAFETY: We've verified the TypeId matches, so the types are compatible
                    let ptr = &packet as *const DisconnectPacket as *const P;
                    ptr.read()
                }
            },
            _ => panic!("Packet type mismatch or type ID check failed"),
        }
    }
}

