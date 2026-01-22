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

use crate::protocol::utils::{read_variable_length, write_variable_length};
use crate::protocol::PacketType;
use crate::Error;

pub trait PacketEncoder: Sized {
    fn packet_type(&self) -> PacketType;
    fn fixed_flags(&self) -> u8;
    fn header_first_byte(&self) -> u8 {
        (self.packet_type() as u8) << 4 | (self.fixed_flags() & 0x0F)
    }
    fn encode(&self, buffer: &mut [u8]) -> Result<usize, Error>;
    fn decode(bytes: &[u8], header: u8) -> Result<Self, Error>;
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

impl<const MAX_CLIENT_NAME_LENGTH: usize, const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize> Packet<MAX_CLIENT_NAME_LENGTH, MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE> {
    pub fn packet_type(&self) -> PacketType {
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

    pub fn header_first_byte(&self) -> u8 {
        match self {
            Packet::Connect(packet) => packet.header_first_byte(),
            Packet::ConnAck(packet) => packet.header_first_byte(),
            Packet::Publish(packet) => packet.header_first_byte(),
            Packet::PubAck(packet) => packet.header_first_byte(),
            Packet::PubRec(packet) => packet.header_first_byte(),
            Packet::PubRel(packet) => packet.header_first_byte(),
            Packet::PubComp(packet) => packet.header_first_byte(),
            Packet::Subscribe(packet) => packet.header_first_byte(),
            Packet::SubAck(packet) => packet.header_first_byte(),
            Packet::Unsubscribe(packet) => packet.header_first_byte(),
            Packet::UnsubAck(packet) => packet.header_first_byte(),
            Packet::PingReq(packet) => packet.header_first_byte(),
            Packet::PingResp(packet) => packet.header_first_byte(),
            Packet::Disconnect(packet) => packet.header_first_byte(),
        }
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.is_empty() {
            return Err(Error::IncompletePacket);
        }
        let header = bytes[0];
        let packet_type = PacketType::from_u8(header).ok_or(Error::InvalidPacketType {
            packet_type: header,
        })?;
        // Self::validate_flags(packet_type, header)?;

        let (remaining_length, len_bytes) = read_variable_length(&bytes[1..])?;
        let payload_start = 1 + len_bytes;
        if bytes.len() < payload_start + remaining_length {
            return Err(Error::IncompletePacket);
        }

        // let total_packet_size = payload_start + remaining_length;
        // if total_packet_size > MAX_PACKET_SIZE {
        //     return Err(Error::PacketTooLarge {
        //         max_size: MAX_PACKET_SIZE,
        //         actual_size: total_packet_size,
        //     });
        // }

        // validate payload length does not exceed maximums payload size
        if remaining_length > MAX_PAYLOAD_SIZE {
            return Err(Error::PacketTooLarge {
                max_size: MAX_PAYLOAD_SIZE,
                actual_size: remaining_length,
            });
        }

        let payload = &bytes[payload_start..payload_start + remaining_length];

        match packet_type {
            PacketType::Connect => {
                let connect = ConnectPacket::decode(payload, header)?;
                Ok(Packet::Connect(connect))
            }
            PacketType::ConnAck => {
                let connack = ConnAckPacket::decode(payload, header)?;
                Ok(Packet::ConnAck(connack))
            }
            PacketType::Publish => {
                let publish = PublishPacket::decode(payload, header)?;
                Ok(Packet::Publish(publish))
            }
            PacketType::PubAck => {
                let puback = PubAckPacket::decode(payload, header)?;
                Ok(Packet::PubAck(puback))
            }
            PacketType::PubRec => {
                let pubrec = PubRecPacket::decode(payload, header)?;
                Ok(Packet::PubRec(pubrec))
            }
            PacketType::PubRel => {
                let pubrel = PubRelPacket::decode(payload, header)?;
                Ok(Packet::PubRel(pubrel))
            }
            PacketType::PubComp => {
                let pubcomp = PubCompPacket::decode(payload, header)?;
                Ok(Packet::PubComp(pubcomp))
            }
            PacketType::Subscribe => {
                let subscribe = SubscribePacket::decode(payload, header)?;
                Ok(Packet::Subscribe(subscribe))
            }
            PacketType::SubAck => {
                let suback = SubAckPacket::decode(payload, header)?;
                Ok(Packet::SubAck(suback))
            }
            PacketType::Unsubscribe => {
                let unsubscribe = UnsubscribePacket::decode(payload, header)?;
                Ok(Packet::Unsubscribe(unsubscribe))
            }
            PacketType::UnsubAck => {
                let unsuback = UnsubAckPacket::decode(payload, header)?;
                Ok(Packet::UnsubAck(unsuback))
            }
            PacketType::PingReq => {
                let pingreq = PingReqPacket::decode(payload, header)?;
                Ok(Packet::PingReq(pingreq))
            }
            PacketType::PingResp => {
                let pingresp = PingRespPacket::decode(payload, header)?;
                Ok(Packet::PingResp(pingresp))
            }
            PacketType::Disconnect => {
                let disconnect = DisconnectPacket::decode(payload, header)?;
                Ok(Packet::Disconnect(disconnect))
            }
            _ => Err(Error::InvalidPacketType {
                packet_type: header,
            }),
        }
    }

    pub fn encode(&self, buffer: &mut [u8]) -> Result<usize, Error> {
        if buffer.is_empty() {
            return Err(Error::BufferTooSmall);
        }

        let packet_type = self.packet_type();
        buffer[0] = self.header_first_byte();

        let mut payload_buffer = [0u8; MAX_PAYLOAD_SIZE];
        let payload_len = match self {
            Packet::Connect(connect) => connect.encode(&mut payload_buffer)?,
            Packet::ConnAck(connack) => connack.encode(&mut payload_buffer)?,
            Packet::Publish(publish) => publish.encode(&mut payload_buffer)?,
            Packet::PubAck(puback) => puback.encode(&mut payload_buffer)?,
            Packet::PubRec(pubrec) => pubrec.encode(&mut payload_buffer)?,
            Packet::PubRel(pubrel) => pubrel.encode(&mut payload_buffer)?,
            Packet::PubComp(pubcomp) => pubcomp.encode(&mut payload_buffer)?,
            Packet::Subscribe(subscribe) => subscribe.encode(&mut payload_buffer)?,
            Packet::SubAck(suback) => suback.encode(&mut payload_buffer)?,
            Packet::Unsubscribe(unsubscribe) => unsubscribe.encode(&mut payload_buffer)?,
            Packet::UnsubAck(unsuback) => unsuback.encode(&mut payload_buffer)?,
            Packet::PingReq(pingreq) => pingreq.encode(&mut payload_buffer)?,
            Packet::PingResp(pingresp) => pingresp.encode(&mut payload_buffer)?,
            Packet::Disconnect(disconnect) => disconnect.encode(&mut payload_buffer)?,
        };

        let mut length_buffer = [0u8; 4];
        let len_bytes = write_variable_length(payload_len, &mut length_buffer)?;

        let total_len = 1 + len_bytes + payload_len;
        if buffer.len() < total_len {
            return Err(Error::BufferTooSmall);
        }

        buffer[1..1 + len_bytes].copy_from_slice(&length_buffer[..len_bytes]);
        if payload_len > 0 {
            buffer[1 + len_bytes..total_len].copy_from_slice(&payload_buffer[..payload_len]);
        }

        Ok(total_len)
    }
}
