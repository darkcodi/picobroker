#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd, Default)]
pub enum PacketType {
    /// Reserved
    /// Direction: Forbidden
    #[default]
    Reserved = 0,

    /// Client request to connect to Server
    /// Direction: Client to Server
    Connect = 1,

    /// Connect acknowledgment
    /// Direction: Server to Client
    ConnAck = 2,

    /// Publish message
    /// Direction: Client to Server or Server to Client
    Publish = 3,

    /// Publish acknowledgment
    /// Direction: Client to Server or Server to Client
    PubAck = 4,

    /// Publish received (assured delivery part 1)
    /// Direction: Client to Server or Server to Client
    PubRec = 5,

    /// Publish release (assured delivery part 2)
    /// Direction: Client to Server or Server to Client
    PubRel = 6,

    /// Publish complete (assured delivery part 3)
    /// Direction: Client to Server or Server to Client
    PubComp = 7,

    /// Client subscribe request
    /// Direction: Client to Server
    Subscribe = 8,

    /// Subscribe acknowledgment
    /// Direction: Server to Client
    SubAck = 9,

    /// Client unsubscribe request
    /// Direction: Client to Server
    Unsubscribe = 10,

    /// Unsubscribe acknowledgment
    /// Direction: Server to Client
    UnsubAck = 11,

    /// Ping request
    /// Direction: Client to Server
    PingReq = 12,

    /// Ping response
    /// Direction: Server to Client
    PingResp = 13,

    /// Client disconnect request
    /// Direction: Client to Server
    Disconnect = 14,

    /// Reserved
    /// Direction: Forbidden
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
