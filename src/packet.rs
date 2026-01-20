use crate::Error;

/// MQTT Control Packet types (high nibble of byte 1)
#[repr(u8)]
#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Ord, PartialOrd, Default)]
pub enum PacketType {
    #[default]
    Reserved = 0, // Forbidden; Reserved
    Connect = 1, // Client to Server; Client request to connect to Server
    ConnAck = 2, // Server to Client; Connect acknowledgment
    Publish = 3, // Client to Server or Server to Client; Publish message
    PubAck = 4, // Client to Server or Server to Client; Publish acknowledgment
    PubRec = 5, // Client to Server or Server to Client; Publish received (assured delivery part 1)
    PubRel = 6, // Client to Server or Server to Client; Publish release (assured delivery part 2)
    PubComp = 7, // Client to Server or Server to Client; Publish complete (assured delivery part 3)
    Subscribe = 8, // Client to Server; Client subscribe request
    SubAck = 9, // Server to Client; Subscribe acknowledgment
    Unsubscribe = 10, // Client to Server; Client unsubscribe request
    UnsubAck = 11, // Server to Client; Unsubscribe acknowledgment
    PingReq = 12, // Client to Server; PING request
    PingResp = 13, // Server to Client; PING response
    Disconnect = 14, // Client to Server; Client is disconnecting
    Reserved2 = 15, // Forbidden; Reserved
}

impl PacketType {
    /// Fixed header flags for all packet types EXCEPT PUBLISH.
    /// (PUBLISH flags depend on DUP/QoS/RETAIN.)
    pub const fn fixed_flags(self) -> u8 {
        match self {
            PacketType::PubRel | PacketType::Subscribe | PacketType::Unsubscribe => 0b0010,
            PacketType::Publish => 0, // not used; use PublishFlags instead
            _ => 0b0000,
        }
    }

    /// Builds the first fixed-header byte for non-PUBLISH packets.
    pub const fn header_byte(self) -> u8 {
        ((self as u8) << 4) | (self.fixed_flags() & 0x0F)
    }
}

/// QoS is encoded in bits 2..1 of the PUBLISH flags nibble.
#[repr(u8)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum QoS {
    AtMostOnce  = 0,
    AtLeastOnce = 1,
    ExactlyOnce = 2,
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct PublishFlags {
    pub dup: bool,     // bit 3
    pub qos: QoS,      // bits 2..1
    pub retain: bool,  // bit 0
}

impl PublishFlags {
    pub const fn to_nibble(self) -> u8 {
        let dup = if self.dup { 1u8 } else { 0u8 };
        let retain = if self.retain { 1u8 } else { 0u8 };
        (dup << 3) | ((self.qos as u8) << 1) | retain
    }
}

/// Builds the first fixed-header byte for PUBLISH packets.
pub const fn publish_header_byte(flags: PublishFlags) -> u8 {
    ((PacketType::Publish as u8) << 4) | (flags.to_nibble() & 0x0F)
}

pub fn validate_flags(packet_type: PacketType, flags: u8) -> Result<(), Error> {
    let flags = flags & 0x0F;
    match packet_type {
        PacketType::Publish => {
            // QoS=3 is invalid in MQTT 3.1.1
            let qos = (flags >> 1) & 0b11;
            if qos == 0b11 { return Err(Error::InvalidPublishQoS { invalid_qos: qos }); }
            Ok(())
        }
        _ => {
            let expected = packet_type.fixed_flags();
            if flags == expected { Ok(()) }
            else { Err(Error::InvalidFixedHeaderFlags {
                actual: flags,
                expected,
            }) }
        }
    }
}
