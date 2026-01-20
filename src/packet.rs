use crate::Error;

/// Maximum packet size enforced by PICObroker
///
/// Per MQTT 3.1.1 spec, packets can be up to 256MB. However, this embedded
/// implementation enforces a strict 256-byte limit due to memory constraints.
/// Attempts to encode or decode packets larger than this will return
/// `Error::PacketTooLarge`.
const DEFAULT_PACKET_SIZE: usize = 256;
const DEFAULT_PAYLOAD_SIZE: usize = 128;

pub type PacketBuffer<const SIZE: usize = DEFAULT_PACKET_SIZE> = heapless::Vec<u8, SIZE>;
pub type Payload<const SIZE: usize = DEFAULT_PAYLOAD_SIZE> = heapless::Vec<u8, SIZE>;

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
    pub const fn fixed_flags(self) -> u8 {
        match self {
            PacketType::PubRel | PacketType::Subscribe | PacketType::Unsubscribe => 0b0010,
            PacketType::Publish => 0,
            _ => 0b0000,
        }
    }

    pub const fn header_byte(self) -> u8 {
        ((self as u8) << 4) | (self.fixed_flags() & 0x0F)
    }

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

#[repr(u8)]
#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub enum QoS {
    AtMostOnce = 0,
    AtLeastOnce = 1,
    ExactlyOnce = 2,
}

impl QoS {
    pub const fn from_u8(value: u8) -> Option<Self> {
        match value {
            0 => Some(QoS::AtMostOnce),
            1 => Some(QoS::AtLeastOnce),
            2 => Some(QoS::ExactlyOnce),
            _ => None,
        }
    }
}

#[derive(Debug, Copy, Clone, Eq, PartialEq)]
pub struct PublishFlags {
    pub dup: bool,
    pub qos: QoS,
    pub retain: bool,
}

impl PublishFlags {
    pub const fn to_nibble(self) -> u8 {
        let dup = if self.dup { 1u8 } else { 0u8 };
        let retain = if self.retain { 1u8 } else { 0u8 };
        (dup << 3) | ((self.qos as u8) << 1) | retain
    }

    pub fn from_nibble(nibble: u8) -> Option<Self> {
        let qos = QoS::from_u8((nibble >> 1) & 0b11)?;
        Some(PublishFlags {
            dup: (nibble & 0b1000) != 0,
            qos,
            retain: (nibble & 0b0001) != 0,
        })
    }
}

pub const fn publish_header_byte(flags: PublishFlags) -> u8 {
    ((PacketType::Publish as u8) << 4) | (flags.to_nibble() & 0x0F)
}

pub fn validate_flags(packet_type: PacketType, flags: u8) -> Result<(), Error> {
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

pub fn read_variable_length(bytes: &[u8]) -> Result<(usize, usize), Error> {
    // MQTT spec limits variable length to 268,435,455 (0x0FFFFFFF)
    const MAX_VARIABLE_LENGTH: usize = 268_435_455;

    let mut multiplier = 1;
    let mut value = 0usize;
    let mut bytes_read = 0usize;

    loop {
        if bytes_read >= bytes.len() {
            return Err(Error::IncompletePacket);
        }
        let byte = bytes[bytes_read] as usize;
        bytes_read += 1;
        value += (byte & 0x7F) * multiplier;

        // Check if value exceeds MQTT spec maximum BEFORE processing continuation
        if value > MAX_VARIABLE_LENGTH {
            return Err(Error::InvalidLengthEncoding);
        }

        multiplier *= 128;

        // Check multiplier to prevent more than 4 bytes (spec limit)
        if multiplier > 128 * 128 * 128 * 128 {
            return Err(Error::InvalidLengthEncoding);
        }

        if (byte & 0x80) == 0 {
            break;
        }
    }

    Ok((value, bytes_read))
}

pub const fn variable_length_length(value: usize) -> usize {
    if value < 128 {
        1
    } else if value < 16384 {
        2
    } else if value < 2097152 {
        3
    } else {
        4
    }
}

pub fn write_variable_length(value: usize, buffer: &mut [u8]) -> Result<usize, Error> {
    // MQTT spec limits variable length to 268,435,455 (0x0FFFFFFF)
    const MAX_VARIABLE_LENGTH: usize = 268_435_455;

    if value > MAX_VARIABLE_LENGTH {
        return Err(Error::InvalidLengthEncoding);
    }

    let mut encoded = value;
    let mut bytes_written = 0;

    loop {
        if bytes_written >= buffer.len() {
            return Err(Error::BufferTooSmall);
        }
        let mut byte = (encoded & 0x7F) as u8;
        encoded >>= 7;
        if encoded > 0 {
            byte |= 0x80;
        }
        buffer[bytes_written] = byte;
        bytes_written += 1;
        if encoded == 0 {
            break;
        }
    }

    Ok(bytes_written)
}

pub fn read_string<'a>(bytes: &'a [u8], offset: &'_ mut usize) -> Result<&'a str, Error> {
    if *offset + 2 > bytes.len() {
        return Err(Error::IncompletePacket);
    }
    let len = u16::from_be_bytes([bytes[*offset], bytes[*offset + 1]]) as usize;
    *offset += 2;
    if *offset + len > bytes.len() {
        return Err(Error::IncompletePacket);
    }
    let str_bytes = &bytes[*offset..*offset + len];
    *offset += len;
    let str_slice = core::str::from_utf8(str_bytes).map_err(|_| Error::InvalidUtf8)?;
    Ok(str_slice)
}

pub fn write_string(s: &str, buffer: &mut [u8], offset: &mut usize) -> Result<(), Error> {
    let bytes = s.as_bytes();
    let len = bytes.len();
    if *offset + 2 + len > buffer.len() {
        return Err(Error::BufferTooSmall);
    }
    let len_bytes = (len as u16).to_be_bytes();
    buffer[*offset] = len_bytes[0];
    buffer[*offset + 1] = len_bytes[1];
    *offset += 2;
    buffer[*offset..*offset + len].copy_from_slice(bytes);
    *offset += len;
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Connect<'a> {
    pub protocol_name: &'a str,
    pub protocol_level: u8,
    pub clean_session: bool,
    pub keep_alive: u16,
    pub client_id: &'a str,
    pub will_flag: bool,
    pub will_qos: QoS,
    pub will_retain: bool,
}

impl<'a> Connect<'a> {
    pub fn new() -> Self {
        Self {
            protocol_name: "MQTT",
            protocol_level: 4,
            clean_session: true,
            keep_alive: 0,
            client_id: "pico",
            will_flag: false,
            will_qos: QoS::AtMostOnce,
            will_retain: false,
        }
    }

    pub fn decode(bytes: &'a [u8]) -> Result<Self, Error> {
        let mut offset = 0;
        let protocol_name = read_string(bytes, &mut offset)?;
        if protocol_name != "MQTT" {
            return Err(Error::InvalidProtocolName);
        }
        if offset >= bytes.len() {
            return Err(Error::IncompletePacket);
        }
        let protocol_level = bytes[offset];
        offset += 1;
        if protocol_level != 4 {
            return Err(Error::InvalidProtocolLevel {
                level: protocol_level,
            });
        }
        if offset >= bytes.len() {
            return Err(Error::IncompletePacket);
        }
        let connect_flags = bytes[offset];
        offset += 1;

        // Validate reserved bits per MQTT 3.1.1 spec
        // Bit 0 (reserved) must be 0
        if connect_flags & 0x01 != 0 {
            return Err(Error::InvalidConnectFlags);
        }

        // Bits 6-7 (reserved) must be 0
        if connect_flags & 0xC0 != 0 {
            return Err(Error::InvalidConnectFlags);
        }

        // Extract will flag
        let will_flag = (connect_flags & 0x04) != 0;

        // If will flag is 0, bits 3-5 (Will QoS, Will Retain) must be 0
        if !will_flag && (connect_flags & 0x38 != 0) {
            return Err(Error::InvalidConnectFlags);
        }

        let clean_session = (connect_flags & 0x02) != 0;

        // Extract will QoS (bits 4-3)
        let will_qos = match (connect_flags >> 3) & 0b11 {
            0 => QoS::AtMostOnce,
            1 => QoS::AtLeastOnce,
            2 => QoS::ExactlyOnce,
            _ => return Err(Error::InvalidConnectFlags),
        };

        // Extract will retain (bit 5)
        let will_retain = (connect_flags & 0x20) != 0;
        if offset + 2 > bytes.len() {
            return Err(Error::IncompletePacket);
        }
        let keep_alive = u16::from_be_bytes([bytes[offset], bytes[offset + 1]]);
        offset += 2;
        let client_id = read_string(bytes, &mut offset)?;
        if client_id.is_empty() && !clean_session {
            return Err(Error::InvalidClientIdLength { length: 0 });
        }
        Ok(Self {
            protocol_name,
            protocol_level,
            clean_session,
            keep_alive,
            client_id,
            will_flag,
            will_qos,
            will_retain,
        })
    }

    pub fn encode(&self, buffer: &mut [u8]) -> Result<usize, Error> {
        let mut offset = 0;
        write_string(self.protocol_name, buffer, &mut offset)?;
        if offset >= buffer.len() {
            return Err(Error::BufferTooSmall);
        }
        buffer[offset] = self.protocol_level;
        offset += 1;

        // Build connect flags byte per MQTT 3.1.1 spec
        let mut connect_flags: u8 = 0;

        // Bit 0: Reserved, must be 0
        // Bit 1: Clean Session
        if self.clean_session {
            connect_flags |= 0x02;
        }

        // Bit 2: Will Flag
        if self.will_flag {
            connect_flags |= 0x04;
        }

        // Bits 3-4: Will QoS
        connect_flags |= (self.will_qos as u8) << 3;

        // Bit 5: Will Retain
        if self.will_retain {
            connect_flags |= 0x20;
        }

        // Bits 6-7: Reserved, must be 0

        if offset >= buffer.len() {
            return Err(Error::BufferTooSmall);
        }
        buffer[offset] = connect_flags;
        offset += 1;
        if offset + 2 > buffer.len() {
            return Err(Error::BufferTooSmall);
        }
        let keep_alive_bytes = self.keep_alive.to_be_bytes();
        buffer[offset] = keep_alive_bytes[0];
        buffer[offset + 1] = keep_alive_bytes[1];
        offset += 2;
        write_string(self.client_id, buffer, &mut offset)?;
        Ok(offset)
    }
}

impl<'a> Default for Connect<'a> {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ConnectReturnCode {
    pub code: u8,
}

impl ConnectReturnCode {
    pub const ACCEPTED: u8 = 0;
    pub const UNACCEPTABLE_PROTOCOL_VERSION: u8 = 1;
    pub const IDENTIFIER_REJECTED: u8 = 2;
    pub const SERVER_UNAVAILABLE: u8 = 3;
    pub const BAD_USER_NAME_OR_PASSWORD: u8 = 4;
    pub const NOT_AUTHORIZED: u8 = 5;

    pub const fn accepted() -> Self {
        Self {
            code: Self::ACCEPTED,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnAck {
    pub session_present: bool,
    pub return_code: ConnectReturnCode,
}

impl ConnAck {
    pub const fn new(session_present: bool, return_code: ConnectReturnCode) -> Self {
        Self {
            session_present,
            return_code,
        }
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() < 2 {
            return Err(Error::IncompletePacket);
        }
        let session_present = (bytes[0] & 0x01) != 0;
        let return_code = ConnectReturnCode { code: bytes[1] };
        Ok(Self {
            session_present,
            return_code,
        })
    }

    pub fn encode(&self, buffer: &mut [u8]) -> Result<usize, Error> {
        if buffer.len() < 2 {
            return Err(Error::BufferTooSmall);
        }
        buffer[0] = if self.session_present { 0x01 } else { 0x00 };
        buffer[1] = self.return_code.code;
        Ok(2)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Publish<'a> {
    pub topic_name: &'a str,
    pub packet_id: Option<u16>,
    pub payload: Payload,
    pub qos: QoS,
    pub dup: bool,
    pub retain: bool,
}

impl<'a> Default for Publish<'a> {
    fn default() -> Self {
        Self::new()
    }
}

impl<'a> Publish<'a> {
    pub fn new() -> Self {
        Self {
            topic_name: "",
            packet_id: None,
            payload: Payload::new(),
            qos: QoS::AtMostOnce,
            dup: false,
            retain: false,
        }
    }

    pub fn decode(bytes: &'a [u8], header_byte: u8) -> Result<Self, Error> {
        let mut offset = 0;
        let topic_name = read_string(bytes, &mut offset)?;
        if topic_name.is_empty() {
            return Err(Error::EmptyTopic);
        }

        // Extract flags from the header byte
        let flags = PublishFlags::from_nibble(header_byte & 0x0F).ok_or(
            Error::InvalidFixedHeaderFlags {
                actual: header_byte & 0x0F,
                expected: 0,
            },
        )?;

        let packet_id = if flags.qos != QoS::AtMostOnce {
            if offset + 2 > bytes.len() {
                return Err(Error::IncompletePacket);
            }
            let pid = u16::from_be_bytes([bytes[offset], bytes[offset + 1]]);

            // MQTT 3.1.1 spec: Packet Identifier MUST be non-zero
            if pid == 0 {
                return Err(Error::MalformedPacket);
            }

            offset += 2;
            Some(pid)
        } else {
            None
        };

        let payload_len = bytes.len() - offset;
        let mut payload = Payload::new();
        payload
            .extend_from_slice(&bytes[offset..offset + payload_len])
            .map_err(|_| Error::BufferTooSmall)?;

        Ok(Self {
            topic_name,
            packet_id,
            payload,
            qos: flags.qos,
            dup: flags.dup,
            retain: flags.retain,
        })
    }

    pub fn encode(&self, buffer: &mut [u8]) -> Result<usize, Error> {
        let mut offset = 0;
        write_string(self.topic_name, buffer, &mut offset)?;

        match self.qos {
            QoS::AtMostOnce => { /* packet_id not required */ }
            QoS::AtLeastOnce | QoS::ExactlyOnce => {
                let packet_id = self.packet_id.ok_or(Error::MalformedPacket)?;
                if offset + 2 > buffer.len() {
                    return Err(Error::BufferTooSmall);
                }
                let pid_bytes = packet_id.to_be_bytes();
                buffer[offset] = pid_bytes[0];
                buffer[offset + 1] = pid_bytes[1];
                offset += 2;
            }
        }

        if offset + self.payload.len() > buffer.len() {
            return Err(Error::BufferTooSmall);
        }
        buffer[offset..offset + self.payload.len()].copy_from_slice(&self.payload);
        offset += self.payload.len();

        Ok(offset)
    }

    pub fn header_byte(&self) -> u8 {
        publish_header_byte(PublishFlags {
            dup: self.dup,
            qos: self.qos,
            retain: self.retain,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PubAck {
    pub packet_id: u16,
}

impl PubAck {
    pub const fn new(packet_id: u16) -> Self {
        Self { packet_id }
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() < 2 {
            return Err(Error::IncompletePacket);
        }
        let packet_id = u16::from_be_bytes([bytes[0], bytes[1]]);

        // MQTT 3.1.1 spec: Packet Identifier MUST be non-zero
        if packet_id == 0 {
            return Err(Error::MalformedPacket);
        }

        Ok(Self { packet_id })
    }

    pub fn encode(&self, buffer: &mut [u8]) -> Result<usize, Error> {
        if buffer.len() < 2 {
            return Err(Error::BufferTooSmall);
        }
        let pid_bytes = self.packet_id.to_be_bytes();
        buffer[0] = pid_bytes[0];
        buffer[1] = pid_bytes[1];
        Ok(2)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Subscribe<'a> {
    pub packet_id: u16,
    pub topic_filter: &'a str,
}

impl<'a> Subscribe<'a> {
    pub fn new(packet_id: u16) -> Self {
        Self {
            packet_id,
            topic_filter: "",
        }
    }

    pub fn decode(bytes: &'a [u8]) -> Result<Self, Error> {
        let mut offset = 0;
        if offset + 2 > bytes.len() {
            return Err(Error::IncompletePacket);
        }
        let packet_id = u16::from_be_bytes([bytes[offset], bytes[offset + 1]]);

        // MQTT 3.1.1 spec: Packet Identifier MUST be non-zero
        if packet_id == 0 {
            return Err(Error::MalformedPacket);
        }

        offset += 2;
        let topic_filter = read_string(bytes, &mut offset)?;
        if topic_filter.is_empty() {
            return Err(Error::EmptyTopic);
        }
        if offset + 1 > bytes.len() {
            return Err(Error::IncompletePacket);
        }
        // Read requested QoS byte (currently unused but must be read to validate packet structure)
        let _requested_qos = bytes[offset];
        Ok(Self {
            packet_id,
            topic_filter,
        })
    }

    pub fn encode(&self, buffer: &mut [u8]) -> Result<usize, Error> {
        let mut offset = 0;
        if offset + 2 > buffer.len() {
            return Err(Error::BufferTooSmall);
        }
        let pid_bytes = self.packet_id.to_be_bytes();
        buffer[offset] = pid_bytes[0];
        buffer[offset + 1] = pid_bytes[1];
        offset += 2;
        write_string(self.topic_filter, buffer, &mut offset)?;
        if offset >= buffer.len() {
            return Err(Error::BufferTooSmall);
        }
        buffer[offset] = 0;
        offset += 1;
        Ok(offset)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubAck {
    pub packet_id: u16,
    pub granted_qos: QoS,
}

impl SubAck {
    pub const fn new(packet_id: u16, granted_qos: QoS) -> Self {
        Self {
            packet_id,
            granted_qos,
        }
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() < 3 {
            return Err(Error::IncompletePacket);
        }
        let packet_id = u16::from_be_bytes([bytes[0], bytes[1]]);
        let qos_value = bytes[2];
        let granted_qos = QoS::from_u8(qos_value).ok_or(Error::InvalidSubAckQoS {
            invalid_qos: qos_value,
        })?;
        Ok(Self {
            packet_id,
            granted_qos,
        })
    }

    pub fn encode(&self, buffer: &mut [u8]) -> Result<usize, Error> {
        if buffer.len() < 3 {
            return Err(Error::BufferTooSmall);
        }
        let pid_bytes = self.packet_id.to_be_bytes();
        buffer[0] = pid_bytes[0];
        buffer[1] = pid_bytes[1];
        buffer[2] = self.granted_qos as u8;
        Ok(3)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Unsubscribe<'a> {
    pub packet_id: u16,
    pub topic_filter: &'a str,
}

impl<'a> Unsubscribe<'a> {
    pub fn new(packet_id: u16) -> Self {
        Self {
            packet_id,
            topic_filter: "",
        }
    }

    pub fn decode(bytes: &'a [u8]) -> Result<Self, Error> {
        let mut offset = 0;
        if offset + 2 > bytes.len() {
            return Err(Error::IncompletePacket);
        }
        let packet_id = u16::from_be_bytes([bytes[offset], bytes[offset + 1]]);

        // MQTT 3.1.1 spec: Packet Identifier MUST be non-zero
        if packet_id == 0 {
            return Err(Error::MalformedPacket);
        }

        offset += 2;
        let topic_filter = read_string(bytes, &mut offset)?;
        if topic_filter.is_empty() {
            return Err(Error::EmptyTopic);
        }
        Ok(Self {
            packet_id,
            topic_filter,
        })
    }

    pub fn encode(&self, buffer: &mut [u8]) -> Result<usize, Error> {
        let mut offset = 0;
        if offset + 2 > buffer.len() {
            return Err(Error::BufferTooSmall);
        }
        let pid_bytes = self.packet_id.to_be_bytes();
        buffer[offset] = pid_bytes[0];
        buffer[offset + 1] = pid_bytes[1];
        offset += 2;
        write_string(self.topic_filter, buffer, &mut offset)?;
        Ok(offset)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnsubAck {
    pub packet_id: u16,
}

impl UnsubAck {
    pub const fn new(packet_id: u16) -> Self {
        Self { packet_id }
    }

    pub fn decode(bytes: &[u8]) -> Result<Self, Error> {
        if bytes.len() < 2 {
            return Err(Error::IncompletePacket);
        }
        let packet_id = u16::from_be_bytes([bytes[0], bytes[1]]);

        // MQTT 3.1.1 spec: Packet Identifier MUST be non-zero
        if packet_id == 0 {
            return Err(Error::MalformedPacket);
        }

        Ok(Self { packet_id })
    }

    pub fn encode(&self, buffer: &mut [u8]) -> Result<usize, Error> {
        if buffer.len() < 2 {
            return Err(Error::BufferTooSmall);
        }
        let pid_bytes = self.packet_id.to_be_bytes();
        buffer[0] = pid_bytes[0];
        buffer[1] = pid_bytes[1];
        Ok(2)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PingReq;

impl Default for PingReq {
    fn default() -> Self {
        Self::new()
    }
}

impl PingReq {
    pub const fn new() -> Self {
        Self
    }

    pub fn decode(_bytes: &[u8]) -> Result<Self, Error> {
        Ok(Self)
    }

    pub fn encode(&self, _buffer: &mut [u8]) -> Result<usize, Error> {
        Ok(0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PingResp;

impl Default for PingResp {
    fn default() -> Self {
        Self::new()
    }
}

impl PingResp {
    pub const fn new() -> Self {
        Self
    }

    pub fn decode(_bytes: &[u8]) -> Result<Self, Error> {
        Ok(Self)
    }

    pub fn encode(&self, _buffer: &mut [u8]) -> Result<usize, Error> {
        Ok(0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Disconnect;

impl Default for Disconnect {
    fn default() -> Self {
        Self::new()
    }
}

impl Disconnect {
    pub const fn new() -> Self {
        Self
    }

    pub fn decode(_bytes: &[u8]) -> Result<Self, Error> {
        Ok(Self)
    }

    pub fn encode(&self, _buffer: &mut [u8]) -> Result<usize, Error> {
        Ok(0)
    }
}

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

    pub fn decode(bytes: &'a [u8]) -> Result<Self, Error> {
        if bytes.is_empty() {
            return Err(Error::IncompletePacket);
        }
        let header = bytes[0];
        let packet_type = PacketType::from_u8(header).ok_or(Error::InvalidPacketType {
            packet_type: header,
        })?;
        validate_flags(packet_type, header)?;

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
