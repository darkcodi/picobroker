use crate::protocol::packet_type::PacketType;
use crate::protocol::packets::connack::ConnAck;
use crate::protocol::packets::connect::Connect;
use crate::protocol::packets::disconnect::Disconnect;
use crate::protocol::packets::pingreq::PingReq;
use crate::protocol::packets::pingresp::PingResp;
use crate::protocol::packets::puback::PubAck;
use crate::protocol::packets::publish::Publish;
use crate::protocol::packets::suback::SubAck;
use crate::protocol::packets::subscribe::Subscribe;
use crate::protocol::packets::unsuback::UnsubAck;
use crate::protocol::packets::unsubscribe::Unsubscribe;
use crate::protocol::utils::{read_variable_length, write_variable_length};
use crate::Error;

mod connack;
mod connect;
mod disconnect;
mod pingreq;
mod pingresp;
mod puback;
mod publish;
mod suback;
mod subscribe;
mod unsuback;
mod unsubscribe;

const DEFAULT_PACKET_SIZE: usize = 256;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Packet<'a> {
    Connect(Connect<'a>),
    ConnAck(ConnAck),
    Publish(Publish<'a>),
    PubAck(PubAck),
    PubRec(PubAck),
    PubRel(PubAck),
    PubComp(PubAck),
    Subscribe(Subscribe<'a>),
    SubAck(SubAck),
    Unsubscribe(Unsubscribe<'a>),
    UnsubAck(UnsubAck),
    PingReq(PingReq),
    PingResp(PingResp),
    Disconnect(Disconnect),
}

impl<'a> Packet<'a> {
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

    fn validate_flags(packet_type: PacketType, flags: u8) -> Result<(), Error> {
        let flags = flags & 0x0F;
        match packet_type {
            PacketType::Publish => {
                let qos = (flags >> 1) & 0b11;
                if qos == 0b11 {
                    return Err(Error::InvalidPublishQoS { invalid_qos: qos });
                }
                Ok(())
            }
            _ => {
                let expected = packet_type.fixed_flags();
                if flags == expected {
                    Ok(())
                } else {
                    Err(Error::InvalidFixedHeaderFlags {
                        actual: flags,
                        expected,
                    })
                }
            }
        }
    }

    pub fn decode(bytes: &'a [u8]) -> Result<Self, Error> {
        if bytes.is_empty() {
            return Err(Error::IncompletePacket);
        }
        let header = bytes[0];
        let packet_type = PacketType::from_u8(header).ok_or(Error::InvalidPacketType {
            packet_type: header,
        })?;
        Self::validate_flags(packet_type, header)?;

        let (remaining_length, len_bytes) = read_variable_length(&bytes[1..])?;
        let payload_start = 1 + len_bytes;
        if bytes.len() < payload_start + remaining_length {
            return Err(Error::IncompletePacket);
        }

        // Validate packet size against maximum (DEFAULT_PACKET_SIZE)
        const MAX_PACKET_SIZE: usize = DEFAULT_PACKET_SIZE;
        let total_packet_size = payload_start + remaining_length;
        if total_packet_size > MAX_PACKET_SIZE {
            return Err(Error::PacketTooLarge {
                max_size: MAX_PACKET_SIZE,
                actual_size: total_packet_size,
            });
        }

        let payload = &bytes[payload_start..payload_start + remaining_length];

        match packet_type {
            PacketType::Connect => {
                let connect = Connect::decode(payload)?;
                Ok(Packet::Connect(connect))
            }
            PacketType::ConnAck => {
                let connack = ConnAck::decode(payload)?;
                Ok(Packet::ConnAck(connack))
            }
            PacketType::Publish => {
                let publish = Publish::decode(payload, header)?;
                Ok(Packet::Publish(publish))
            }
            PacketType::PubAck => {
                let puback = PubAck::decode(payload)?;
                Ok(Packet::PubAck(puback))
            }
            PacketType::PubRec => {
                let pubrec = PubAck::decode(payload)?;
                Ok(Packet::PubRec(pubrec))
            }
            PacketType::PubRel => {
                let pubrel = PubAck::decode(payload)?;
                Ok(Packet::PubRel(pubrel))
            }
            PacketType::PubComp => {
                let pubcomp = PubAck::decode(payload)?;
                Ok(Packet::PubComp(pubcomp))
            }
            PacketType::Subscribe => {
                let subscribe = Subscribe::decode(payload)?;
                Ok(Packet::Subscribe(subscribe))
            }
            PacketType::SubAck => {
                let suback = SubAck::decode(payload)?;
                Ok(Packet::SubAck(suback))
            }
            PacketType::Unsubscribe => {
                let unsubscribe = Unsubscribe::decode(payload)?;
                Ok(Packet::Unsubscribe(unsubscribe))
            }
            PacketType::UnsubAck => {
                let unsuback = UnsubAck::decode(payload)?;
                Ok(Packet::UnsubAck(unsuback))
            }
            PacketType::PingReq => {
                PingReq::decode(payload)?;
                Ok(Packet::PingReq(PingReq))
            }
            PacketType::PingResp => {
                PingResp::decode(payload)?;
                Ok(Packet::PingResp(PingResp))
            }
            PacketType::Disconnect => {
                Disconnect::decode(payload)?;
                Ok(Packet::Disconnect(Disconnect))
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
        buffer[0] = match packet_type {
            PacketType::Publish => {
                if let Packet::Publish(publish) = self {
                    publish.header_byte()
                } else {
                    packet_type.header_byte()
                }
            }
            _ => packet_type.header_byte(),
        };

        let mut payload_buffer = [0u8; DEFAULT_PACKET_SIZE];
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
            Packet::PingReq(_) => 0,
            Packet::PingResp(_) => 0,
            Packet::Disconnect(_) => 0,
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
