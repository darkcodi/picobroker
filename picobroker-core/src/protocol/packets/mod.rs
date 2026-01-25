pub mod connack;
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

pub use crate::protocol::packets::connack::ConnAckPacket;
pub use crate::protocol::packets::connack::ConnectReturnCode;
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
use crate::{variable_length_length, PacketEncodingError, PacketType};

pub trait PacketTypeConst {
    const PACKET_TYPE: PacketType;

    fn validate_packet_type(header_first_byte: u8) -> Result<(), PacketEncodingError> {
        let type_byte = header_first_byte >> 4;
        let packet_type = PacketType::from(type_byte);
        if packet_type != Self::PACKET_TYPE {
            return Err(PacketEncodingError::InvalidPacketType {
                packet_type: type_byte,
            });
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

impl<T: PacketFlagsConst> PacketFlagsDynamic for T {
    fn flags(&self) -> u8 {
        Self::PACKET_FLAGS
    }
}

trait PacketFixedSize {
    const PACKET_SIZE: usize;

    fn validate_buffer_size(buffer_size: usize) -> Result<(), PacketEncodingError> {
        if buffer_size < Self::PACKET_SIZE {
            return Err(PacketEncodingError::BufferTooSmall { buffer_size });
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

impl<const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize> PacketTypeDynamic
    for Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>
{
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

impl<const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize> PacketFlagsDynamic
    for Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>
{
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

impl<const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize> PacketEncoder
    for Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>
{
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
