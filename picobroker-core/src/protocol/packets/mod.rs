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

use crate::{PacketEncodingError, PacketType};
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

pub trait PacketEncoder: Sized {
    fn packet_type(&self) -> PacketType;
    fn fixed_flags(&self) -> u8;
    fn header_first_byte(&self) -> u8 {
        (self.packet_type() as u8) << 4 | (self.fixed_flags() & 0x0F)
    }
    fn encode(&self, buffer: &mut [u8]) -> Result<usize, PacketEncodingError>;
    fn decode(bytes: &[u8]) -> Result<Self, PacketEncodingError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Packet<const MAX_CLIENT_NAME_LENGTH: usize, const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize> {
    Connect(ConnectPacket<MAX_CLIENT_NAME_LENGTH>),
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

impl<const MAX_CLIENT_NAME_LENGTH: usize, const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize> PacketEncoder for Packet<MAX_CLIENT_NAME_LENGTH, MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE> {
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

    fn fixed_flags(&self) -> u8 {
        match self {
            Packet::Connect(packet) => packet.fixed_flags(),
            Packet::ConnAck(packet) => packet.fixed_flags(),
            Packet::Publish(packet) => packet.fixed_flags(),
            Packet::PubAck(packet) => packet.fixed_flags(),
            Packet::PubRec(packet) => packet.fixed_flags(),
            Packet::PubRel(packet) => packet.fixed_flags(),
            Packet::PubComp(packet) => packet.fixed_flags(),
            Packet::Subscribe(packet) => packet.fixed_flags(),
            Packet::SubAck(packet) => packet.fixed_flags(),
            Packet::Unsubscribe(packet) => packet.fixed_flags(),
            Packet::UnsubAck(packet) => packet.fixed_flags(),
            Packet::PingReq(packet) => packet.fixed_flags(),
            Packet::PingResp(packet) => packet.fixed_flags(),
            Packet::Disconnect(packet) => packet.fixed_flags(),
        }
    }

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
    use super::*;

    const MAX_CLIENT_NAME_LENGTH: usize = 30;
    const MAX_TOPIC_NAME_LENGTH: usize = 30;
    const MAX_PAYLOAD_SIZE: usize = 128;
    type DefaultPacket = Packet<MAX_CLIENT_NAME_LENGTH, MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>;

    #[test]
    fn test_packet_disconnect() {
        const DISCONNECT_PACKET_BYTES: [u8; 2] = [0xE0, 0x00];
        let result = DefaultPacket::decode(&DISCONNECT_PACKET_BYTES);
        assert!(result.is_ok(), "Failed to decode packet");
        let packet = result.unwrap();
        let disconnect_packet = match packet {
            DefaultPacket::Disconnect(pkt) => pkt,
            _ => panic!("Decoded packet is not Disconnect"),
        };
        assert_eq!(disconnect_packet, DisconnectPacket::default());
        let mut buffer = [0u8; MAX_PAYLOAD_SIZE];
        let encode_result = disconnect_packet.encode(&mut buffer);
        assert!(encode_result.is_ok(), "Failed to encode packet");
        let encoded_size = encode_result.unwrap();
        assert_eq!(encoded_size, DISCONNECT_PACKET_BYTES.len(), "Encoded size mismatch");
        assert_eq!(&buffer[..encoded_size], &DISCONNECT_PACKET_BYTES, "Encoded bytes mismatch");
    }
}

