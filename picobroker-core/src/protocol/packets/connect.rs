use crate::protocol::packets::{PacketEncoder, PacketFlagsConst, PacketHeader, PacketTypeConst};
use crate::protocol::qos::QoS;
use crate::protocol::utils::{read_binary, read_string, write_binary, write_string};
use crate::{read_variable_length, write_variable_length, ClientName, Error, PacketEncodingError, PacketType, TopicName};

pub const MQTT_PROTOCOL_NAME: &str = "MQTT";
pub const MQTT_3_1_1_PROTOCOL_LEVEL: u8 = 4; // MQTT 3.1.1
pub const _MQTT_5_0_PROTOCOL_LEVEL: u8 = 5; // MQTT 5.0

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct ConnectFlags(u8);

impl ConnectFlags {
    pub const RESERVED:  Self = Self(0b_0000_0001);
    pub const CLEAN_SESSION: Self = Self(0b_0000_0010);
    pub const WILL_FLAG:  Self = Self(0b_0000_0100);
    pub const WILL_QOS_1:  Self = Self(0b_0000_1000);
    pub const WILL_QOS_2:  Self = Self(0b_0001_0000);
    pub const WILL_RETAIN:  Self = Self(0b_0010_0000);
    pub const PASSWORD:  Self = Self(0b_0100_0000);
    pub const USERNAME:  Self = Self(0b_1000_0000);

    pub const fn empty() -> Self { Self(0) }
    pub const fn bits(self) -> u8 { self.0 }
    pub const fn contains(self, other: Self) -> bool { (self.0 & other.0) == other.0 }
    pub fn insert(&mut self, other: Self) { self.0 |= other.0; }
    pub fn remove(&mut self, other: Self) { self.0 &= !other.0; }
    pub fn toggle(&mut self, other: Self) { self.0 ^= other.0; }
}

/// Fixed Header
///   byte 1:  0x10                      (type=1, flags=0000)
///   bytes :  Remaining Length (var-int)
///
/// Variable Header
///   Protocol Name      ("MQTT" as UTF-8 string)
///   Protocol Level     (0x04 for MQTT 3.1.1)
///   Connect Flags      (bitfield)
///   Keep Alive         (2 bytes)
///
/// Payload (order matters, some fields optional)
///   Client Identifier  (UTF-8 string)
///   Will Topic         (UTF-8 string)   [if Will Flag = 1]
///   Will Payload       (binary data)    [if Will Flag = 1]
///   User Name          (UTF-8 string)   [if User Name Flag = 1]
///   Password           (binary data)    [if Password Flag = 1]
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectPacket<const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize> {
    pub protocol_name: &'static str,
    pub protocol_level: u8,
    pub connect_flags: ConnectFlags,
    pub keep_alive: u16,
    pub client_id: ClientName,
    pub will_topic: Option<TopicName<MAX_TOPIC_NAME_LENGTH>>,
    pub will_payload: Option<heapless::Vec<u8, MAX_PAYLOAD_SIZE>>,
    pub username: Option<heapless::String<MAX_TOPIC_NAME_LENGTH>>,
    pub password: Option<heapless::Vec<u8, MAX_TOPIC_NAME_LENGTH>>,
}

impl<const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize> PacketTypeConst for ConnectPacket<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE> {
    const PACKET_TYPE: PacketType = PacketType::Connect;
}

impl<const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize> PacketFlagsConst for ConnectPacket<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE> {
    const PACKET_FLAGS: u8 = 0b0000;
}

impl<const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize> PacketEncoder for ConnectPacket<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE> {
    fn encode(&self, buffer: &mut [u8]) -> Result<usize, PacketEncodingError> {
        // calculate remaining length
        let mut remaining_length = 0;
        remaining_length += 2 + self.protocol_name.len(); // Protocol Name
        remaining_length += 1; // Protocol Level
        remaining_length += 1; // Connect Flags
        remaining_length += 2; // Keep Alive
        remaining_length += 2 + self.client_id.len(); // Client ID
        if let Some(will_topic) = &self.will_topic {
            remaining_length += 2 + will_topic.as_str().len(); // Will Topic
        }
        if let Some(will_payload) = &self.will_payload {
            remaining_length += 2 + will_payload.len(); // Will Payload
        }
        if let Some(username) = &self.username {
            remaining_length += 2 + username.len(); // Username
        }
        if let Some(password) = &self.password {
            remaining_length += 2 + password.len(); // Password
        }

        // write fixed header
        let mut offset = 0;
        buffer[offset] = self.header_first_byte();
        offset += 1;
        let int_len = write_variable_length(remaining_length, &mut buffer[offset..])?;
        offset += int_len;

        // write variable header
        write_string(&self.protocol_name, buffer, &mut offset)?;
        buffer[offset] = self.protocol_level;
        offset += 1;
        buffer[offset] = self.connect_flags.bits();
        offset += 1;
        buffer[offset..offset + 2].copy_from_slice(&self.keep_alive.to_be_bytes());
        offset += 2;

        // write payload
        write_string(self.client_id.as_str(), buffer, &mut offset)?;
        if let Some(will_topic) = &self.will_topic {
            write_string(will_topic.as_str(), buffer, &mut offset)?;
        }
        if let Some(will_payload) = &self.will_payload {
            write_binary(will_payload.as_slice(), buffer, &mut offset)?;
        }
        if let Some(username) = &self.username {
            write_string(username.as_str(), buffer, &mut offset)?;
        }
        if let Some(password) = &self.password {
            write_binary(password.as_slice(), buffer, &mut offset)?;
        }

        Ok(offset)
    }

    fn decode(bytes: &[u8]) -> Result<Self, PacketEncodingError> {
        Self::validate_packet_type(bytes[0])?;
        let (remaining_length, int_length) = read_variable_length(&bytes[1..])?;

        // validate protocol name
        let mut offset = 1 + int_length;
        if offset >= bytes.len() {
            return Err(PacketEncodingError::IncompletePacket.into());
        }
        let protocol_name = read_string(bytes, &mut offset)?;
        if protocol_name != MQTT_PROTOCOL_NAME {
            return Err(PacketEncodingError::InvalidProtocolName);
        }

        // validate protocol level
        if offset >= bytes.len() {
            return Err(PacketEncodingError::IncompletePacket.into());
        }
        let protocol_level = bytes[offset];
        offset += 1;
        if protocol_level != MQTT_3_1_1_PROTOCOL_LEVEL {
            return Err(PacketEncodingError::UnsupportedProtocolLevel {
                level: protocol_level,
            });
        }

        // validate connect flags
        if offset >= bytes.len() {
            return Err(PacketEncodingError::IncompletePacket.into());
        }
        let connect_flags = bytes[offset];
        offset += 1;
        let connect_flags = ConnectFlags(connect_flags);
        // validate reserved bits per MQTT 3.1.1 spec: Bit 0 (reserved) must be 0
        if connect_flags.contains(ConnectFlags::RESERVED) {
            return Err(Error::InvalidConnectFlags.into());
        }
        let clean_session = connect_flags.contains(ConnectFlags::CLEAN_SESSION);
        let will_flag = connect_flags.contains(ConnectFlags::WILL_FLAG);
        let will_qos = if connect_flags.contains(ConnectFlags::WILL_QOS_2) {
            QoS::ExactlyOnce
        } else if connect_flags.contains(ConnectFlags::WILL_QOS_1) {
            QoS::AtLeastOnce
        } else {
            QoS::AtMostOnce
        };
        let will_retain = connect_flags.contains(ConnectFlags::WILL_RETAIN);
        let username_flag = connect_flags.contains(ConnectFlags::USERNAME);
        let password_flag = connect_flags.contains(ConnectFlags::PASSWORD);

        // extract keep alive
        let keep_alive = u16::from_be_bytes([bytes[offset], bytes[offset + 1]]);
        offset += 2;

        // extract client id
        let client_id = read_string(bytes, &mut offset)?;
        if client_id.is_empty() && !clean_session {
            return Err(Error::InvalidClientIdLength { length: 0 }.into());
        }
        if client_id.len() > MAX_TOPIC_NAME_LENGTH {
            return Err(Error::ClientIdLengthExceeded {
                max_length: MAX_TOPIC_NAME_LENGTH,
                actual_length: client_id.len(),
            }.into());
        }
        let client_id = heapless::String::try_from(client_id)
            .map(|s| ClientName::from(s))
            .map_err(|_| {
                Error::ClientIdLengthExceeded {
                    max_length: MAX_TOPIC_NAME_LENGTH,
                    actual_length: client_id.len(),
                }
        })?;

        // extract optional fields (will topic, will payload, username, password) as needed
        let mut will_topic: Option<TopicName<MAX_TOPIC_NAME_LENGTH>> = None;
        let mut will_payload: Option<heapless::Vec<u8, MAX_PAYLOAD_SIZE>> = None;
        if will_flag {
            // extract will topic
            let will_topic_str = read_string(bytes, &mut offset)?;
            if will_topic_str.is_empty() {
                return Err(Error::EmptyTopic.into());
            }
            if will_topic_str.len() > MAX_TOPIC_NAME_LENGTH {
                return Err(Error::ClientIdLengthExceeded {
                    max_length: MAX_TOPIC_NAME_LENGTH,
                    actual_length: will_topic_str.len(),
                }.into());
            }
            will_topic = Some(heapless::String::<MAX_TOPIC_NAME_LENGTH>::try_from(will_topic_str)
                .map(|s| TopicName::new(s))
                .map_err(|_| {
                    Error::ClientIdLengthExceeded {
                        max_length: MAX_TOPIC_NAME_LENGTH,
                        actual_length: will_topic_str.len(),
                    }
            })?);

            // extract will payload
            let will_payload_bytes = read_binary(bytes, &mut offset)?;
            let mut will_payload_vec = heapless::Vec::<u8, MAX_PAYLOAD_SIZE>::new();
            if will_payload_bytes.len() > MAX_PAYLOAD_SIZE {
                return Err(Error::PacketTooLarge {
                    max_size: MAX_PAYLOAD_SIZE,
                    actual_size: will_payload_bytes.len(),
                }.into());
            }
            // Note: MQTT 3.1.1 spec does not limit will payload size, but we enforce MAX_PAYLOAD_SIZE here
            will_payload_vec.extend_from_slice(will_payload_bytes).map_err(|_| {
                Error::PacketTooLarge {
                    max_size: MAX_PAYLOAD_SIZE,
                    actual_size: will_payload_bytes.len(),
                }
            })?;
            will_payload = Some(will_payload_vec);
        }

        // extract username
        let mut username: Option<heapless::String<MAX_TOPIC_NAME_LENGTH>> = None;
        if username_flag {
            let username_str = read_string(bytes, &mut offset)?;
            if username_str.len() > MAX_TOPIC_NAME_LENGTH {
                return Err(Error::ClientIdLengthExceeded {
                    max_length: MAX_TOPIC_NAME_LENGTH,
                    actual_length: username_str.len(),
                }.into());
            }
            username = Some(heapless::String::<MAX_TOPIC_NAME_LENGTH>::try_from(username_str)
                .map_err(|_| {
                    Error::ClientIdLengthExceeded {
                        max_length: MAX_TOPIC_NAME_LENGTH,
                        actual_length: username_str.len(),
                    }
                })?);
        }

        // extract password
        let mut password: Option<heapless::Vec<u8, MAX_TOPIC_NAME_LENGTH>> = None;
        if password_flag {
            let password_bytes = read_binary(bytes, &mut offset)?;
            let mut password_vec = heapless::Vec::<u8, MAX_TOPIC_NAME_LENGTH>::new();
            if password_bytes.len() > MAX_TOPIC_NAME_LENGTH {
                return Err(Error::PacketTooLarge {
                    max_size: MAX_TOPIC_NAME_LENGTH,
                    actual_size: password_bytes.len(),
                }.into());
            }
            password_vec.extend_from_slice(password_bytes).map_err(|_| {
                Error::PacketTooLarge {
                    max_size: MAX_TOPIC_NAME_LENGTH,
                    actual_size: password_bytes.len(),
                }
            })?;
            password = Some(password_vec);
        }

        // Validate that the remaining length matches the actual bytes read
        let actual_payload_size = offset - 1 - int_length;
        if actual_payload_size != remaining_length as usize {
            return Err(PacketEncodingError::InvalidPacketLength {
                expected: remaining_length as usize,
                actual: actual_payload_size,
            });
        }

        Ok(Self {
            protocol_name: MQTT_PROTOCOL_NAME,
            protocol_level,
            connect_flags,
            keep_alive,
            client_id,
            will_topic,
            will_payload,
            username,
            password,
        })
    }
}
