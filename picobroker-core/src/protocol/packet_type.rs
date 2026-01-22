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

impl PacketType {
    pub const fn from_u8(byte: u8) -> Option<Self> {
        match byte >> 4 {
            0 => Some(PacketType::Reserved),
            1 => Some(PacketType::Connect),
            2 => Some(PacketType::ConnAck),
            3 => Some(PacketType::Publish),
            4 => Some(PacketType::PubAck),
            5 => Some(PacketType::PubRec),
            6 => Some(PacketType::PubRel),
            7 => Some(PacketType::PubComp),
            8 => Some(PacketType::Subscribe),
            9 => Some(PacketType::SubAck),
            10 => Some(PacketType::Unsubscribe),
            11 => Some(PacketType::UnsubAck),
            12 => Some(PacketType::PingReq),
            13 => Some(PacketType::PingResp),
            14 => Some(PacketType::Disconnect),
            15 => Some(PacketType::Reserved2),
            _ => None,
        }
    }
}
