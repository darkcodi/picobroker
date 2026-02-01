#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd, Default)]
pub enum PacketType {
    #[default]
    Reserved = 0,

    Connect = 1,

    ConnAck = 2,

    Publish = 3,

    PubAck = 4,

    PubRec = 5,

    PubRel = 6,

    PubComp = 7,

    Subscribe = 8,

    SubAck = 9,

    Unsubscribe = 10,

    UnsubAck = 11,

    PingReq = 12,

    PingResp = 13,

    Disconnect = 14,

    Reserved2 = 15,
}

impl From<u8> for PacketType {
    fn from(byte: u8) -> Self {
        match byte {
            0 => PacketType::Reserved,
            1 => PacketType::Connect,
            2 => PacketType::ConnAck,
            3 => PacketType::Publish,
            4 => PacketType::PubAck,
            5 => PacketType::PubRec,
            6 => PacketType::PubRel,
            7 => PacketType::PubComp,
            8 => PacketType::Subscribe,
            9 => PacketType::SubAck,
            10 => PacketType::Unsubscribe,
            11 => PacketType::UnsubAck,
            12 => PacketType::PingReq,
            13 => PacketType::PingResp,
            14 => PacketType::Disconnect,
            15 => PacketType::Reserved2,
            big_byte => PacketType::from(big_byte >> 4),
        }
    }
}
