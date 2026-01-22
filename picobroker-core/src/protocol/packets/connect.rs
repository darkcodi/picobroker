use crate::protocol::packets::{PacketEncoder, PacketFlagsConst, PacketTypeConst};
use crate::protocol::qos::QoS;
use crate::protocol::utils::{read_string, write_string};
use crate::{ClientName, Error, PacketEncodingError, PacketType};

pub const MQTT_PROTOCOL_NAME: &str = "MQTT";
pub const MQTT_3_1_1_PROTOCOL_LEVEL: u8 = 4; // MQTT 3.1.1
pub const MQTT_5_0_PROTOCOL_LEVEL: u8 = 5; // MQTT 5.0

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConnectPacket<const MAX_CLIENT_NAME_LENGTH: usize> {
    pub protocol_name: &'static str,
    pub protocol_level: u8,
    pub clean_session: bool,
    pub keep_alive: u16,
    pub client_id: ClientName<MAX_CLIENT_NAME_LENGTH>,
    pub will_flag: bool,
    pub will_qos: QoS,
    pub will_retain: bool,
}

impl<const MAX_CLIENT_NAME_LENGTH: usize> PacketTypeConst for ConnectPacket<MAX_CLIENT_NAME_LENGTH> {
    const PACKET_TYPE: PacketType = PacketType::Connect;
}

impl<const MAX_CLIENT_NAME_LENGTH: usize> PacketFlagsConst for ConnectPacket<MAX_CLIENT_NAME_LENGTH> {
    const PACKET_FLAGS: u8 = 0b0000;
}

impl<const MAX_CLIENT_NAME_LENGTH: usize> PacketEncoder for ConnectPacket<MAX_CLIENT_NAME_LENGTH> {
    fn encode(&self, buffer: &mut [u8]) -> Result<usize, PacketEncodingError> {
        let mut offset = 0;
        write_string(self.protocol_name, buffer, &mut offset)?;
        if offset >= buffer.len() {
            return Err(Error::BufferTooSmall.into());
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
            return Err(Error::BufferTooSmall.into());
        }
        buffer[offset] = connect_flags;
        offset += 1;
        if offset + 2 > buffer.len() {
            return Err(Error::BufferTooSmall.into());
        }
        let keep_alive_bytes = self.keep_alive.to_be_bytes();
        buffer[offset] = keep_alive_bytes[0];
        buffer[offset + 1] = keep_alive_bytes[1];
        offset += 2;
        write_string(self.client_id.as_str(), buffer, &mut offset)?;
        Ok(offset)
    }

    fn decode(bytes: &[u8]) -> Result<Self, PacketEncodingError> {
        let mut offset = 0;
        let protocol_name = read_string(bytes, &mut offset)?;
        if protocol_name != "MQTT" {
            return Err(Error::InvalidProtocolName.into());
        }
        let protocol_name: &'static str = MQTT_PROTOCOL_NAME;
        if offset >= bytes.len() {
            return Err(Error::IncompletePacket.into());
        }
        let protocol_level = bytes[offset];
        offset += 1;
        if protocol_level != MQTT_3_1_1_PROTOCOL_LEVEL {
            return if protocol_level == MQTT_5_0_PROTOCOL_LEVEL {
                Err(Error::UnsupportedProtocolLevel {
                    level: protocol_level,
                }.into())
            } else {
                Err(Error::InvalidProtocolLevel {
                    level: protocol_level,
                }.into())
            }
        }
        if offset >= bytes.len() {
            return Err(Error::IncompletePacket.into());
        }
        let connect_flags = bytes[offset];
        offset += 1;

        // Validate reserved bits per MQTT 3.1.1 spec
        // Bit 0 (reserved) must be 0
        if connect_flags & 0x01 != 0 {
            return Err(Error::InvalidConnectFlags.into());
        }

        // Bits 6-7 (reserved) must be 0
        if connect_flags & 0xC0 != 0 {
            return Err(Error::InvalidConnectFlags.into());
        }

        // Extract will flag
        let will_flag = (connect_flags & 0x04) != 0;

        // If will flag is 0, bits 3-5 (Will QoS, Will Retain) must be 0
        if !will_flag && (connect_flags & 0x38 != 0) {
            return Err(Error::InvalidConnectFlags.into());
        }

        let clean_session = (connect_flags & 0x02) != 0;

        // Extract will QoS (bits 4-3)
        let will_qos = match (connect_flags >> 3) & 0b11 {
            0 => QoS::AtMostOnce,
            1 => QoS::AtLeastOnce,
            2 => QoS::ExactlyOnce,
            _ => return Err(Error::InvalidConnectFlags.into()),
        };

        // Extract will retain (bit 5)
        let will_retain = (connect_flags & 0x20) != 0;
        if offset + 2 > bytes.len() {
            return Err(Error::IncompletePacket.into());
        }
        let keep_alive = u16::from_be_bytes([bytes[offset], bytes[offset + 1]]);
        offset += 2;
        let client_id = read_string(bytes, &mut offset)?;
        if client_id.is_empty() && !clean_session {
            return Err(Error::InvalidClientIdLength { length: 0 }.into());
        }
        if client_id.len() > MAX_CLIENT_NAME_LENGTH {
            return Err(Error::ClientIdLengthExceeded {
                max_length: MAX_CLIENT_NAME_LENGTH,
                actual_length: client_id.len(),
            }.into());
        }
        let client_id = heapless::String::try_from(client_id)
            .map(|s| ClientName::new(s))
            .map_err(|_| {
                Error::ClientIdLengthExceeded {
                    max_length: MAX_CLIENT_NAME_LENGTH,
                    actual_length: client_id.len(),
                }
        })?;
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
}
