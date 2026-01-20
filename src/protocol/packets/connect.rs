use crate::protocol::qos::QoS;
use crate::protocol::utils::{read_string, write_string};
use crate::Error;

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
