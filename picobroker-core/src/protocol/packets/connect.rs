use crate::protocol::packets::{PacketEncoder, PacketFlagsConst, PacketHeader, PacketTypeConst};
use crate::protocol::qos::QoS;
use crate::protocol::utils::{read_binary, read_string, write_binary, write_string};
use crate::{read_variable_length, write_variable_length, ClientId, Error, PacketEncodingError, PacketType, TopicName};

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
    pub const ALL_FLAGS: Self = Self(0b_1111_1110);

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
    pub connect_flags: ConnectFlags,
    pub keep_alive: u16,
    pub client_id: ClientId,
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
        remaining_length += 2 + MQTT_PROTOCOL_NAME.len(); // Protocol Name
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
        write_string(MQTT_PROTOCOL_NAME, buffer, &mut offset)?;
        buffer[offset] = MQTT_3_1_1_PROTOCOL_LEVEL;
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
        if offset + 2 > bytes.len() {
            return Err(PacketEncodingError::IncompletePacket.into());
        }
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
            .map(|s| ClientId::from(s))
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

impl<const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize> ConnectPacket<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE> {
    pub const fn estimate_struct_size() -> usize {
        const BASE: usize = 128;

        const fn ceil_div(n: usize, d: usize) -> usize {
            if n == 0 { 0 } else { (n + d - 1) / d }
        }

        const fn round_up_to(n: usize, m: usize) -> usize {
            if n == 0 { 0 } else { ceil_div(n, m) * m }
        }

        // Payload contributes in 8-byte chunks.
        let payload_bytes = round_up_to(MAX_PAYLOAD_SIZE, 8);

        // Topic contributes 0 for 1..=8, +24 for 9..=16, +48 for 17..=24, etc.
        let topic_blocks = ceil_div(MAX_TOPIC_NAME_LENGTH, 8);
        let extra_topic_blocks = topic_blocks.saturating_sub(1);
        let topic_bytes = extra_topic_blocks * 24;

        BASE + topic_bytes + payload_bytes
    }
}

#[cfg(test)]
mod tests {
    use heapless::Vec;
    use super::*;
    use crate::protocol::packet_error::PacketEncodingError;

    const MAX_TOPIC_NAME_LENGTH: usize = 30;
    const MAX_PAYLOAD_SIZE: usize = 128;

    // ===== HELPER FUNCTIONS =====

    fn hex_to_bytes(hex: &str) -> Vec<u8, MAX_PAYLOAD_SIZE> {
        let mut result = Vec::new();
        for s in hex.split_whitespace() {
            if let Ok(b) = u8::from_str_radix(s, 16) {
                let _ = result.push(b);
            }
        }
        result
    }

    fn roundtrip_test(bytes: &[u8]) -> ConnectPacket<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE> {
        let result = ConnectPacket::<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>::decode(&bytes);
        if let Err(ref e) = result {
            panic!("Failed to decode packet: {:?}", e);
        }
        assert!(result.is_ok(), "Failed to decode packet");
        let packet = result.unwrap();
        let mut buffer = [0u8; 512];
        let encode_result = packet.encode(&mut buffer);
        assert!(encode_result.is_ok(), "Failed to encode packet");
        let encoded_size = encode_result.unwrap();
        assert_eq!(encoded_size, bytes.len(), "Encoded size mismatch");
        assert_eq!(&buffer[..encoded_size], bytes, "Encoded bytes mismatch");
        packet
    }

    fn decode_test(bytes: &[u8]) -> Result<ConnectPacket<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>, PacketEncodingError> {
        ConnectPacket::<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>::decode(bytes)
    }

    // ===== SIZE OF STRUCT TEST =====

    #[macro_export]
    macro_rules! assert_teq {
        ($a:expr, $b:expr, $c:expr $(,)?) => {{
            // Evaluate each expression once
            let __a = &$a;
            let __b = &$b;
            let __c = &$c;

            if !(*__a == *__b && *__b == *__c) {
                panic!(
                    "assert_teq! failed: values are not all equal\n  a = {:?}\n  b = {:?}\n  c = {:?}",
                    __a, __b, __c
                );
            }
        }};
    }

    #[macro_export]
    macro_rules! validate_size_of_struct {
        ($a:literal, $b:literal, $c:literal) => {{
            use ::core::mem::size_of;

            let actual = size_of::<$crate::ConnectPacket<$a, $b>>();
            let estimated = $crate::ConnectPacket::<$a, $b>::estimate_struct_size();

            assert_eq!(
                actual, estimated,
                "size_of != estimate for ConnectPacket<{}, {}> (actual {}, estimated {})",
                $a, $b, actual, estimated
            );

            assert_eq!(
                actual, $c,
                "unexpected size for ConnectPacket<{}, {}> (actual {}, expected {})",
                $a, $b, actual, $c
            );
        }};
    }

    // Size varies based on generic parameters; test some combinations
    #[test]
    fn test_connect_packet_struct_size() {
        // Sizes for MAX_TOPIC_NAME_LENGTH = 1 and varying MAX_PAYLOAD_SIZE
        validate_size_of_struct!(1, 1, 136);
        validate_size_of_struct!(1, 8, 136);
        validate_size_of_struct!(1, 9, 144);
        validate_size_of_struct!(1, 16, 144);
        validate_size_of_struct!(1, 17, 152);
        validate_size_of_struct!(1, 24, 152);
        validate_size_of_struct!(1, 25, 160);
        validate_size_of_struct!(1, 32, 160);
        validate_size_of_struct!(1, 33, 168);
        validate_size_of_struct!(1, 40, 168);
        validate_size_of_struct!(1, 41, 176);
        validate_size_of_struct!(1, 48, 176);
        validate_size_of_struct!(1, 49, 184);
        validate_size_of_struct!(1, 56, 184);
        validate_size_of_struct!(1, 57, 192);
        validate_size_of_struct!(1, 64, 192);
        validate_size_of_struct!(1, 65, 200);
        validate_size_of_struct!(1, 72, 200);
        validate_size_of_struct!(1, 73, 208);
        validate_size_of_struct!(1, 80, 208);
        validate_size_of_struct!(1, 81, 216);
        validate_size_of_struct!(1, 88, 216);
        validate_size_of_struct!(1, 89, 224);
        validate_size_of_struct!(1, 96, 224);
        validate_size_of_struct!(1, 97, 232);
        validate_size_of_struct!(1, 104, 232);
        validate_size_of_struct!(1, 105, 240);
        validate_size_of_struct!(1, 112, 240);
        validate_size_of_struct!(1, 113, 248);
        validate_size_of_struct!(1, 120, 248);
        validate_size_of_struct!(1, 121, 256);
        validate_size_of_struct!(1, 128, 256);

        // Sizes for MAX_PAYLOAD_SIZE = 1 and varying MAX_TOPIC_NAME_LENGTH
        validate_size_of_struct!(1, 1, 136);
        validate_size_of_struct!(8, 1, 136);
        validate_size_of_struct!(9, 1, 160);
        validate_size_of_struct!(16, 1, 160);
        validate_size_of_struct!(17, 1, 184);
        validate_size_of_struct!(24, 1, 184);
        validate_size_of_struct!(25, 1, 208);
        validate_size_of_struct!(32, 1, 208);
        validate_size_of_struct!(33, 1, 232);
        validate_size_of_struct!(40, 1, 232);
        validate_size_of_struct!(41, 1, 256);
        validate_size_of_struct!(48, 1, 256);
        validate_size_of_struct!(49, 1, 280);
        validate_size_of_struct!(56, 1, 280);
        validate_size_of_struct!(57, 1, 304);
        validate_size_of_struct!(64, 1, 304);
        validate_size_of_struct!(65, 1, 328);
        validate_size_of_struct!(72, 1, 328);
        validate_size_of_struct!(73, 1, 352);
        validate_size_of_struct!(80, 1, 352);
        validate_size_of_struct!(81, 1, 376);
        validate_size_of_struct!(88, 1, 376);
        validate_size_of_struct!(89, 1, 400);
        validate_size_of_struct!(96, 1, 400);
        validate_size_of_struct!(97, 1, 424);
        validate_size_of_struct!(104, 1, 424);
        validate_size_of_struct!(105, 1, 448);
        validate_size_of_struct!(112, 1, 448);
        validate_size_of_struct!(113, 1, 472);
        validate_size_of_struct!(120, 1, 472);
        validate_size_of_struct!(121, 1, 496);
        validate_size_of_struct!(128, 1, 496);

        // Sizes for varying MAX_TOPIC_NAME_LENGTH and MAX_PAYLOAD_SIZE
        validate_size_of_struct!(30, 128, 328);
        validate_size_of_struct!(32, 128, 328);
        validate_size_of_struct!(33, 128, 352);
        validate_size_of_struct!(30, 256, 456);
        validate_size_of_struct!(32, 256, 456);
        validate_size_of_struct!(33, 256, 480);
    }

    // ===== SIZE OF PACKET BYTES TEST =====

    fn max_string<const N: usize>(c: char) -> heapless::String<N> {
        let mut s = heapless::String::<N>::new();
        for _ in 0..N {
            s.push(c).unwrap();
        }
        s
    }

    fn max_vec<const N: usize>(c: u8) -> Vec<u8, N> {
        let mut v = Vec::<u8, N>::new();
        for _ in 0..N {
            v.push(c).unwrap();
        }
        v
    }

    fn max_packet<const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize>() -> ConnectPacket<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE> {
        ConnectPacket {
            connect_flags: ConnectFlags::ALL_FLAGS,
            keep_alive: 60,
            client_id: ClientId::from(max_string('c')),
            will_topic: Some(TopicName::new(max_string('t'))),
            will_payload: Some(max_vec(1)),
            username: Some(max_string('u')),
            password: Some(max_vec(2)),
        }
    }

    #[macro_export]
    macro_rules! validate_size_of_packet {
        ($a:literal, $b:literal, $c:literal) => {{

            const STRUCT_SIZE: usize = size_of::<$crate::ConnectPacket<$a, $b>>();
            let packet = max_packet::<$a, $b>();
            let mut buffer = [0u8; STRUCT_SIZE + 512];
            let packet_size = packet.encode(&mut buffer).unwrap();
            assert_eq!(
                packet_size, $c,
                "unexpected encoded size for ConnectPacket<{}, {}> (encoded {}, expected {})",
                $a, $b, packet_size, $c
            );
        }};
    }

    #[test]
    fn test_connect_packet_bytes_size() {
        // Sizes for MAX_TOPIC_NAME_LENGTH = 1 and varying MAX_PAYLOAD_SIZE
        validate_size_of_packet!(1, 1, 49);
        validate_size_of_packet!(1, 8, 56);
        validate_size_of_packet!(1, 9, 57);
        validate_size_of_packet!(1, 16, 64);
        validate_size_of_packet!(1, 17, 65);
        validate_size_of_packet!(1, 24, 72);
        validate_size_of_packet!(1, 25, 73);
        validate_size_of_packet!(1, 32, 80);
        validate_size_of_packet!(1, 33, 81);
        validate_size_of_packet!(1, 40, 88);
        validate_size_of_packet!(1, 41, 89);
        validate_size_of_packet!(1, 48, 96);
        validate_size_of_packet!(1, 49, 97);
        validate_size_of_packet!(1, 56, 104);
        validate_size_of_packet!(1, 57, 105);
        validate_size_of_packet!(1, 64, 112);
        validate_size_of_packet!(1, 65, 113);
        validate_size_of_packet!(1, 72, 120);
        validate_size_of_packet!(1, 73, 121);
        validate_size_of_packet!(1, 80, 128);
        validate_size_of_packet!(1, 81, 129);
        validate_size_of_packet!(1, 88, 137);
        validate_size_of_packet!(1, 89, 138);
        validate_size_of_packet!(1, 96, 145);
        validate_size_of_packet!(1, 97, 146);
        validate_size_of_packet!(1, 104, 153);
        validate_size_of_packet!(1, 105, 154);
        validate_size_of_packet!(1, 112, 161);
        validate_size_of_packet!(1, 113, 162);
        validate_size_of_packet!(1, 120, 169);
        validate_size_of_packet!(1, 121, 170);
        validate_size_of_packet!(1, 128, 177);

        // Sizes for MAX_PAYLOAD_SIZE = 1 and varying MAX_TOPIC_NAME_LENGTH
        validate_size_of_packet!(1, 1, 49);
        validate_size_of_packet!(8, 1, 70);
        validate_size_of_packet!(9, 1, 73);
        validate_size_of_packet!(16, 1, 94);
        validate_size_of_packet!(17, 1, 97);
        validate_size_of_packet!(24, 1, 118);
        validate_size_of_packet!(25, 1, 121);
        validate_size_of_packet!(32, 1, 143);
        validate_size_of_packet!(33, 1, 146);
        validate_size_of_packet!(40, 1, 167);
        validate_size_of_packet!(41, 1, 170);
        validate_size_of_packet!(48, 1, 191);
        validate_size_of_packet!(49, 1, 194);
        validate_size_of_packet!(56, 1, 215);
        validate_size_of_packet!(57, 1, 218);
        validate_size_of_packet!(64, 1, 239);
        validate_size_of_packet!(65, 1, 242);
        validate_size_of_packet!(72, 1, 263);
        validate_size_of_packet!(73, 1, 266);
        validate_size_of_packet!(80, 1, 287);
        validate_size_of_packet!(81, 1, 290);
        validate_size_of_packet!(88, 1, 311);
        validate_size_of_packet!(89, 1, 314);
        validate_size_of_packet!(96, 1, 335);
        validate_size_of_packet!(97, 1, 338);
        validate_size_of_packet!(104, 1, 359);
        validate_size_of_packet!(105, 1, 362);
        validate_size_of_packet!(112, 1, 383);
        validate_size_of_packet!(113, 1, 386);
        validate_size_of_packet!(120, 1, 407);
        validate_size_of_packet!(121, 1, 410);
        validate_size_of_packet!(128, 1, 431);

        // Sizes for varying MAX_TOPIC_NAME_LENGTH and MAX_PAYLOAD_SIZE
        validate_size_of_packet!(30, 128, 264);
        validate_size_of_packet!(32, 128, 270);
        validate_size_of_packet!(33, 128, 273);
        validate_size_of_packet!(30, 256, 392);
        validate_size_of_packet!(32, 256, 398);
        validate_size_of_packet!(33, 256, 401);
    }

    // ===== PROTOCOL NAME FIELD TESTS =====

    #[test]
    fn test_protocol_name_invalid_wrong_chars() {
        let bytes = hex_to_bytes("10 0F 00 04 4D 51 54 53 04 02 00 3C 00 03 61 62 63");
        let result = decode_test(&bytes);
        assert!(matches!(result, Err(PacketEncodingError::InvalidProtocolName)));
    }

    #[test]
    fn test_protocol_name_invalid_lowercase() {
        let bytes = hex_to_bytes("10 0F 00 04 6D 71 74 74 04 02 00 3C 00 03 61 62 63");
        let result = decode_test(&bytes);
        assert!(matches!(result, Err(PacketEncodingError::InvalidProtocolName)));
    }

    // ===== PROTOCOL LEVEL FIELD TESTS =====

    #[test]
    fn test_protocol_level_invalid_31() {
        let bytes = hex_to_bytes("10 0F 00 04 4D 51 54 54 03 02 00 3C 00 03 61 62 63");
        let result = decode_test(&bytes);
        assert!(matches!(result, Err(PacketEncodingError::UnsupportedProtocolLevel { level: 3 })));
    }

    #[test]
    fn test_protocol_level_invalid_50() {
        let bytes = hex_to_bytes("10 0F 00 04 4D 51 54 54 05 02 00 3C 00 03 61 62 63");
        let result = decode_test(&bytes);
        assert!(matches!(result, Err(PacketEncodingError::UnsupportedProtocolLevel { level: 5 })));
    }

    // ===== CONNECT FLAGS: CLEAN SESSION BIT =====

    #[test]
    fn test_connect_flag_clean_session_set() {
        let bytes = hex_to_bytes("10 0F 00 04 4D 51 54 54 04 02 00 3C 00 03 61 62 63");
        let packet = roundtrip_test(&bytes);
        assert!(packet.connect_flags.contains(ConnectFlags::CLEAN_SESSION));
    }

    #[test]
    fn test_connect_flag_clean_session_not_set() {
        let bytes = hex_to_bytes("10 0F 00 04 4D 51 54 54 04 00 00 3C 00 03 61 62 63");
        let packet = roundtrip_test(&bytes);
        assert!(!packet.connect_flags.contains(ConnectFlags::CLEAN_SESSION));
    }

    #[test]
    fn test_connect_flag_reserved_bit_set() {
        let bytes = hex_to_bytes("10 0F 00 04 4D 51 54 54 04 03 00 3C 00 03 61 62 63");
        let result = ConnectPacket::<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>::decode(&bytes);
        assert!(matches!(result, Err(PacketEncodingError::Other)));
    }

    // ===== CONNECT FLAGS: WILL FLAG =====

    #[test]
    fn test_connect_flag_will_flag_set() {
        let bytes = hex_to_bytes("10 20 00 04 4D 51 54 54 04 06 00 3C 00 03 61 62 63 00 06 77 69 6C 6C 74 70 00 07 77 69 6C 6C 6D 73 67");
        let packet = roundtrip_test(&bytes);
        assert!(packet.connect_flags.contains(ConnectFlags::WILL_FLAG));
    }

    #[test]
    fn test_connect_flag_will_flag_not_set() {
        let bytes = hex_to_bytes("10 0F 00 04 4D 51 54 54 04 02 00 3C 00 03 61 62 63");
        let packet = roundtrip_test(&bytes);
        assert!(!packet.connect_flags.contains(ConnectFlags::WILL_FLAG));
    }

    // ===== CONNECT FLAGS: WILL QoS =====

    #[test]
    fn test_connect_flag_will_qos_0() {
        let bytes = hex_to_bytes("10 20 00 04 4D 51 54 54 04 06 00 3C 00 03 61 62 63 00 06 77 69 6C 6C 74 70 00 07 77 69 6C 6C 6D 73 67");
        let packet = roundtrip_test(&bytes);
        assert!(!packet.connect_flags.contains(ConnectFlags::WILL_QOS_1));
        assert!(!packet.connect_flags.contains(ConnectFlags::WILL_QOS_2));
    }

    #[test]
    fn test_connect_flag_will_qos_1() {
        let bytes = hex_to_bytes("10 20 00 04 4D 51 54 54 04 0E 00 3C 00 03 61 62 63 00 06 77 69 6C 6C 74 70 00 07 77 69 6C 6C 6D 73 67");
        let packet = roundtrip_test(&bytes);
        assert!(packet.connect_flags.contains(ConnectFlags::WILL_QOS_1));
    }

    #[test]
    fn test_connect_flag_will_qos_2() {
        let bytes = hex_to_bytes("10 20 00 04 4D 51 54 54 04 16 00 3C 00 03 61 62 63 00 06 77 69 6C 6C 74 70 00 07 77 69 6C 6C 6D 73 67");
        let packet = roundtrip_test(&bytes);
        assert!(packet.connect_flags.contains(ConnectFlags::WILL_QOS_2));
    }

    // ===== CONNECT FLAGS: WILL RETAIN =====

    #[test]
    fn test_connect_flag_will_retain_set() {
        let bytes = hex_to_bytes("10 20 00 04 4D 51 54 54 04 26 00 3C 00 03 61 62 63 00 06 77 69 6C 6C 74 70 00 07 77 69 6C 6C 6D 73 67");
        let packet = roundtrip_test(&bytes);
        assert!(packet.connect_flags.contains(ConnectFlags::WILL_RETAIN));
    }

    #[test]
    fn test_connect_flag_will_retain_not_set() {
        let bytes = hex_to_bytes("10 0F 00 04 4D 51 54 54 04 02 00 3C 00 03 61 62 63");
        let packet = roundtrip_test(&bytes);
        assert!(!packet.connect_flags.contains(ConnectFlags::WILL_RETAIN));
    }

    // ===== CONNECT FLAGS: USERNAME FLAG =====

    #[test]
    fn test_connect_flag_username_set() {
        let bytes = hex_to_bytes("10 16 00 04 4D 51 54 54 04 82 00 3C 00 03 61 62 63 00 05 75 73 65 72 31");
        let packet = roundtrip_test(&bytes);
        assert!(packet.connect_flags.contains(ConnectFlags::USERNAME));
    }

    #[test]
    fn test_connect_flag_username_not_set() {
        let bytes = hex_to_bytes("10 0F 00 04 4D 51 54 54 04 02 00 3C 00 03 61 62 63");
        let packet = roundtrip_test(&bytes);
        assert!(!packet.connect_flags.contains(ConnectFlags::USERNAME));
    }

    // ===== CONNECT FLAGS: PASSWORD FLAG =====

    #[test]
    fn test_connect_flag_password_set() {
        let bytes = hex_to_bytes("10 1C 00 04 4D 51 54 54 04 C2 00 3C 00 03 61 62 63 00 05 75 73 65 72 31 00 04 70 61 73 73");
        let packet = roundtrip_test(&bytes);
        assert!(packet.connect_flags.contains(ConnectFlags::PASSWORD));
    }

    #[test]
    fn test_connect_flag_password_not_set() {
        let bytes = hex_to_bytes("10 0F 00 04 4D 51 54 54 04 02 00 3C 00 03 61 62 63");
        let packet = roundtrip_test(&bytes);
        assert!(!packet.connect_flags.contains(ConnectFlags::PASSWORD));
    }

    // ===== KEEP ALIVE FIELD TESTS =====

    #[test]
    fn test_keep_alive_value_60() {
        let bytes = hex_to_bytes("10 0F 00 04 4D 51 54 54 04 02 00 3C 00 03 61 62 63");
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.keep_alive, 60);
    }

    #[test]
    fn test_keep_alive_value_zero() {
        let bytes = hex_to_bytes("10 0F 00 04 4D 51 54 54 04 02 00 00 00 03 61 62 63");
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.keep_alive, 0);
    }

    #[test]
    fn test_keep_alive_value_max() {
        let bytes = hex_to_bytes("10 0F 00 04 4D 51 54 54 04 02 FF FF 00 03 61 62 63");
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.keep_alive, 65535);
    }

    // ===== CLIENT ID FIELD TESTS =====

    #[test]
    fn test_client_id_value_abc() {
        let bytes = hex_to_bytes("10 0F 00 04 4D 51 54 54 04 02 00 3C 00 03 61 62 63");
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.client_id.as_str(), "abc");
    }

    #[test]
    fn test_client_id_empty_with_clean_session() {
        let bytes = hex_to_bytes("10 0C 00 04 4D 51 54 54 04 02 00 3C 00 00");
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.client_id.as_str(), "");
    }

    #[test]
    fn test_client_id_empty_without_clean_session() {
        let bytes = hex_to_bytes("10 0C 00 04 4D 51 54 54 04 00 00 3C 00 00");
        let result = ConnectPacket::<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>::decode(&bytes);
        assert!(matches!(result, Err(PacketEncodingError::Other)));
    }

    #[test]
    fn test_client_id_too_long() {
        let bytes = hex_to_bytes("10 2C 00 04 4D 51 54 54 04 02 00 3C 00 20 61 61 61 61 61 61 61 61 61 61 61 61 61 61 61 61 61 61 61 61 61 61 61 61 61 61 61 61 61 61 61 61");
        let result = ConnectPacket::<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>::decode(&bytes);
        assert!(matches!(result, Err(PacketEncodingError::Other)));
    }

    #[test]
    fn test_client_id_exactly_max_length() {
        let bytes = hex_to_bytes("10 23 00 04 4D 51 54 54 04 02 00 3C 00 17 61 61 61 61 61 61 61 61 61 61 61 61 61 61 61 61 61 61 61 61 61 61 61");
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.client_id.len(), 23);
    }

    #[test]
    fn test_client_id_unicode() {
        let bytes = hex_to_bytes("10 15 00 04 4D 51 54 54 04 02 00 3C 00 09 E4 BD A0 E5 A5 BD E4 B8 96");
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.client_id.as_str(), "你好世");
    }

    // ===== WILL TOPIC FIELD TESTS =====

    #[test]
    fn test_will_topic_valid() {
        let bytes = hex_to_bytes("10 20 00 04 4D 51 54 54 04 06 00 3C 00 03 61 62 63 00 06 77 69 6C 6C 74 70 00 07 77 69 6C 6C 6D 73 67");
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.will_topic.as_ref().map(|t| t.as_str()), Some("willtp"));
    }

    #[test]
    fn test_will_topic_empty() {
        let bytes = hex_to_bytes("10 11 00 04 4D 51 54 54 04 06 00 3C 00 03 61 62 63 00 00");
        let result = ConnectPacket::<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>::decode(&bytes);
        assert!(matches!(result, Err(PacketEncodingError::Other)));
    }

    #[test]
    fn test_will_topic_too_long() {
        let bytes = hex_to_bytes("10 38 00 04 4D 51 54 54 04 06 00 3C 00 03 61 62 63 00 20 77 77 77 77 77 77 77 77 77 77 77 77 77 77 77 77 77 77 77 77 77 77 77 77 77 77 77 77 77 77 77 77 00 05 68 65 6C 6C 6F");
        let result = ConnectPacket::<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>::decode(&bytes);
        assert!(matches!(result, Err(PacketEncodingError::Other)));
    }

    // ===== WILL PAYLOAD FIELD TESTS =====

    #[test]
    fn test_will_payload_valid() {
        let bytes = hex_to_bytes("10 20 00 04 4D 51 54 54 04 06 00 3C 00 03 61 62 63 00 06 77 69 6C 6C 74 70 00 07 77 69 6C 6C 6D 73 67");
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.will_payload.as_ref().map(|p| p.as_slice()), Some(b"willmsg".as_ref()));
    }

    #[test]
    fn test_will_payload_empty() {
        let bytes = hex_to_bytes("10 19 00 04 4D 51 54 54 04 06 00 3C 00 03 61 62 63 00 06 77 69 6C 6C 74 70 00 00");
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.will_payload.as_ref().unwrap().len(), 0);
    }

    #[test]
    fn test_will_payload_too_large() {
        let bytes = hex_to_bytes("10 18 00 04 4D 51 54 54 04 06 00 3C 00 03 61 62 63 00 05 77 69 6C 6C 74 01 00 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78 78");
        let result = ConnectPacket::<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>::decode(&bytes);
        assert!(matches!(result, Err(PacketEncodingError::Other)));
    }

    // ===== USERNAME FIELD TESTS =====

    #[test]
    fn test_username_value_user1() {
        let bytes = hex_to_bytes("10 16 00 04 4D 51 54 54 04 82 00 3C 00 03 61 62 63 00 05 75 73 65 72 31");
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.username.as_ref().map(|s| s.as_str()), Some("user1"));
    }

    #[test]
    fn test_username_empty() {
        let bytes = hex_to_bytes("10 11 00 04 4D 51 54 54 04 82 00 3C 00 03 61 62 63 00 00");
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.username.as_ref().unwrap().len(), 0);
    }

    #[test]
    fn test_username_too_long() {
        let bytes = hex_to_bytes("10 31 00 04 4D 51 54 54 04 82 00 3C 00 03 61 62 63 00 20 75 75 75 75 75 75 75 75 75 75 75 75 75 75 75 75 75 75 75 75 75 75 75 75 75 75 75 75 75 75 75 75");
        let result = ConnectPacket::<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>::decode(&bytes);
        assert!(matches!(result, Err(PacketEncodingError::Other)));
    }

    #[test]
    fn test_username_exactly_max_length() {
        let bytes = hex_to_bytes("10 2F 00 04 4D 51 54 54 04 82 00 3C 00 03 61 62 63 00 1E 75 75 75 75 75 75 75 75 75 75 75 75 75 75 75 75 75 75 75 75 75 75 75 75 75 75 75 75 75 75");
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.username.as_ref().unwrap().len(), 30);
    }

    #[test]
    fn test_username_unicode() {
        let bytes = hex_to_bytes("10 17 00 04 4D 51 54 54 04 82 00 3C 00 03 61 62 63 00 06 C3 B1 C3 A1 C3 A9");
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.username.as_ref().unwrap().as_str(), "ñáé");
    }

    // ===== PASSWORD FIELD TESTS =====

    #[test]
    fn test_password_value_pass1() {
        let bytes = hex_to_bytes("10 1D 00 04 4D 51 54 54 04 C2 00 3C 00 03 61 62 63 00 05 75 73 65 72 31 00 05 70 61 73 73 31");
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.password.as_ref().map(|p| p.as_slice()), Some(b"pass1".as_ref()));
    }

    #[test]
    fn test_password_empty() {
        let bytes = hex_to_bytes("10 18 00 04 4D 51 54 54 04 C2 00 3C 00 03 61 62 63 00 05 75 73 65 72 31 00 00");
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.password.as_ref().unwrap().len(), 0);
    }

    #[test]
    fn test_password_too_long() {
        let bytes = hex_to_bytes("10 38 00 04 4D 51 54 54 04 C2 00 3C 00 03 61 62 63 00 05 75 73 65 72 31 00 20 70 70 70 70 70 70 70 70 70 70 70 70 70 70 70 70 70 70 70 70 70 70 70 70 70 70 70 70 70 70 70 70");
        let result = ConnectPacket::<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>::decode(&bytes);
        assert!(matches!(result, Err(PacketEncodingError::Other)));
    }

    #[test]
    fn test_password_exactly_max_length() {
        let bytes = hex_to_bytes("10 36 00 04 4D 51 54 54 04 C2 00 3C 00 03 61 62 63 00 05 75 73 65 72 31 00 1E 70 70 70 70 70 70 70 70 70 70 70 70 70 70 70 70 70 70 70 70 70 70 70 70 70 70 70 70 70 70");
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.password.as_ref().unwrap().len(), 30);
    }

    #[test]
    fn test_password_binary_data() {
        let bytes = hex_to_bytes("10 20 00 04 4D 51 54 54 04 C2 00 3C 00 03 61 62 63 00 05 75 73 65 72 31 00 08 00 01 FF FE 00 FF 01 02");
        let packet = roundtrip_test(&bytes);
        assert_eq!(packet.password.as_ref().unwrap().as_slice(), &[0x00, 0x01, 0xFF, 0xFE, 0x00, 0xFF, 0x01, 0x02]);
    }

    // ===== REMAINING LENGTH FIELD TESTS =====

    #[test]
    fn test_remaining_length_too_small() {
        let bytes = hex_to_bytes("10 0C 00 04 4D 51 54 54 04 02 00 3C 00 03 61 62 63");
        let result = ConnectPacket::<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>::decode(&bytes);
        assert!(matches!(result, Err(PacketEncodingError::InvalidPacketLength { expected: 12, actual: 15 })));
    }

    #[test]
    fn test_remaining_length_too_large() {
        let bytes = hex_to_bytes("10 20 00 04 4D 51 54 54 04 02 00 3C 00 03 61 62 63");
        let result = ConnectPacket::<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>::decode(&bytes);
        assert!(matches!(result, Err(PacketEncodingError::InvalidPacketLength { expected: 32, actual: 15 })));
    }
}
