//! MQTT packet parsing and encoding
//!
//! Implements MQTT 3.1.1 packet formats with QoS 0 only

use crate::error::{Error, Result};

/// Fixed header of an MQTT packet
#[derive(Debug, Clone, Copy)]
pub struct FixedHeader {
    /// Packet type (bits 7-4 of first byte)
    pub packet_type: u8,
    /// Flags (bits 3-0 of first byte)
    pub flags: u8,
    /// Remaining length (variable length integer)
    pub remaining_length: usize,
}

/// MQTT CONNECT packet
#[derive(Debug)]
pub struct ConnectPacket<'a> {
    pub protocol_name: &'a [u8],
    pub protocol_level: u8,
    pub client_id: &'a str,
    pub keep_alive: u16,
}

/// MQTT PUBLISH packet (QoS 0 only)
#[derive(Debug)]
pub struct PublishPacket<'a> {
    pub topic_name: &'a str,
    pub payload: &'a [u8],
}

/// MQTT SUBSCRIBE packet
#[derive(Debug)]
pub struct SubscribePacket<'a> {
    pub packet_id: u16,
    pub topics: heapless::Vec<&'a str, 16>,
}

/// MQTT packet types
#[derive(Debug)]
pub enum Packet<'a> {
    Connect(ConnectPacket<'a>),
    Publish(PublishPacket<'a>),
    Subscribe(SubscribePacket<'a>),
    Disconnect,
}

/// Decode variable length integer from MQTT packet format
///
/// Returns (value, number_of_bytes_consumed)
pub fn decode_remaining_length(bytes: &[u8]) -> Result<(usize, usize)> {
    let mut multiplier = 1;
    let mut value = 0usize;
    let mut index = 0;

    loop {
        if index >= bytes.len() {
            return Err(Error::InvalidPacket);
        }

        let encoded_byte = bytes[index];
        value += (encoded_byte & 0x7F) as usize * multiplier;

        if encoded_byte & 0x80 == 0 {
            return Ok((value, index + 1));
        }

        multiplier *= 128;
        if multiplier > 128 * 128 * 128 {
            return Err(Error::InvalidPacket);
        }

        index += 1;
        if index > 3 {
            return Err(Error::InvalidPacket);
        }
    }
}

/// Parse fixed header from packet bytes
///
/// Returns Ok((header, bytes_consumed)) if successful
pub fn parse_fixed_header(bytes: &[u8]) -> Result<(FixedHeader, usize)> {
    if bytes.is_empty() {
        return Err(Error::InvalidPacket);
    }

    let first_byte = bytes[0];
    let packet_type = (first_byte & 0xF0) >> 4;
    let flags = first_byte & 0x0F;

    let (remaining_length, len_bytes) = decode_remaining_length(&bytes[1..])?;

    Ok((
        FixedHeader {
            packet_type,
            flags,
            remaining_length,
        },
        1 + len_bytes,
    ))
}

/// Parse a 2-byte length-prefixed UTF-8 string
///
/// Returns (string, bytes_consumed)
fn parse_length_prefixed_string(bytes: &[u8]) -> Result<(&str, usize)> {
    if bytes.len() < 2 {
        return Err(Error::InvalidPacket);
    }

    let len = u16::from_be_bytes([bytes[0], bytes[1]]) as usize;

    if bytes.len() < 2 + len {
        return Err(Error::InvalidPacket);
    }

    let string_bytes = &bytes[2..2 + len];
    let string = core::str::from_utf8(string_bytes).map_err(|_| Error::MalformedString)?;

    Ok((string, 2 + len))
}

/// Parse CONNECT packet
pub fn parse_connect(payload: &[u8]) -> Result<ConnectPacket<'_>> {
    let mut offset = 0;

    // Protocol name: "MQTT" (should be 0x00 0x04 0x4D 0x51 0x54 0x54)
    if payload.len() < 6 {
        return Err(Error::InvalidPacket);
    }

    let protocol_name_len = u16::from_be_bytes([payload[0], payload[1]]) as usize;
    if protocol_name_len != 4 || payload.len() < 2 + protocol_name_len {
        return Err(Error::InvalidPacket);
    }

    let protocol_name = &payload[2..2 + protocol_name_len];
    offset += 2 + protocol_name_len;

    // Protocol level (should be 4 for MQTT 3.1.1)
    if offset >= payload.len() {
        return Err(Error::InvalidPacket);
    }
    let protocol_level = payload[offset];
    offset += 1;

    // Connect flags
    if offset >= payload.len() {
        return Err(Error::InvalidPacket);
    }
    let _connect_flags = payload[offset];
    offset += 1;

    // Keep alive
    if offset + 2 > payload.len() {
        return Err(Error::InvalidPacket);
    }
    let keep_alive = u16::from_be_bytes([payload[offset], payload[offset + 1]]);
    offset += 2;

    // Client ID
    let (client_id, _consumed) = parse_length_prefixed_string(&payload[offset..])?;

    Ok(ConnectPacket {
        protocol_name,
        protocol_level,
        client_id,
        keep_alive,
    })
}

/// Parse PUBLISH packet (QoS 0 only)
pub fn parse_publish(payload: &[u8]) -> Result<PublishPacket<'_>> {
    let (topic_name, consumed) = parse_length_prefixed_string(payload)?;

    let payload_start = consumed;
    let payload_data = &payload[payload_start..];

    Ok(PublishPacket {
        topic_name,
        payload: payload_data,
    })
}

/// Parse SUBSCRIBE packet
pub fn parse_subscribe(payload: &[u8]) -> Result<SubscribePacket<'_>> {
    if payload.len() < 2 {
        return Err(Error::InvalidPacket);
    }

    // Packet identifier (2 bytes)
    let packet_id = u16::from_be_bytes([payload[0], payload[1]]);
    let mut offset = 2;

    let mut topics = heapless::Vec::new();

    while offset < payload.len() {
        let (topic, consumed) = parse_length_prefixed_string(&payload[offset..])?;
        topics.push(topic).map_err(|_| Error::MaxTopicsReached)?;
        offset += consumed;

        // Requested QoS (1 byte) - skip since we only support QoS 0
        if offset < payload.len() {
            offset += 1;
        }
    }

    Ok(SubscribePacket { packet_id, topics })
}

/// Parse DISCONNECT packet (no payload)
pub fn parse_disconnect(payload: &[u8]) -> Result<()> {
    if payload.is_empty() {
        Ok(())
    } else {
        Err(Error::InvalidPacket)
    }
}

/// Parse an MQTT packet from bytes
pub fn parse_packet(bytes: &[u8]) -> Result<(FixedHeader, Packet<'_>)> {
    let (header, header_len) = parse_fixed_header(bytes)?;

    let payload_start = header_len;
    let payload_end = payload_start + header.remaining_length;

    if payload_end > bytes.len() {
        return Err(Error::InvalidPacket);
    }

    let payload = &bytes[payload_start..payload_end];

    let packet = match header.packet_type {
        1 => Packet::Connect(parse_connect(payload)?),
        3 => Packet::Publish(parse_publish(payload)?),
        8 => Packet::Subscribe(parse_subscribe(payload)?),
        14 => {
            parse_disconnect(payload)?;
            Packet::Disconnect
        }
        _ => return Err(Error::UnsupportedPacketType),
    };

    Ok((header, packet))
}

// ============== Packet Encoding ==============

/// Encode a value as variable length integer
///
/// Returns the encoded bytes
fn encode_variable_length(mut value: usize) -> heapless::Vec<u8, 4> {
    let mut encoded = heapless::Vec::new();

    loop {
        let mut byte = (value % 128) as u8;
        value /= 128;

        if value > 0 {
            byte |= 0x80;
        }

        let _ = encoded.push(byte);
        if value == 0 {
            break;
        }
    }

    encoded
}

/// Encode CONNACK packet (always success in minimal implementation)
///
/// Returns: [0x20, 0x02, 0x00, 0x00]
pub fn encode_connack() -> [u8; 4] {
    [0x20, 0x02, 0x00, 0x00]
}

/// Encode PUBLISH packet (QoS 0)
///
/// Returns heapless::Vec with the encoded packet
pub fn encode_publish_qos0(topic: &str, payload: &[u8]) -> heapless::Vec<u8, 512> {
    let mut packet = heapless::Vec::new();

    // Fixed header: packet type 3 (PUBLISH), flags 0 (QoS 0, no retain, no dup)
    let _ = packet.push(0x30);

    // Calculate remaining length
    let topic_bytes = topic.as_bytes();
    let remaining_length = 2 + topic_bytes.len() + payload.len();

    // Encode remaining length
    let rem_len_encoded = encode_variable_length(remaining_length);
    for byte in rem_len_encoded {
        let _ = packet.push(byte);
    }

    // Topic name length (big endian)
    let topic_len = (topic_bytes.len() as u16).to_be_bytes();
    let _ = packet.push(topic_len[0]);
    let _ = packet.push(topic_len[1]);

    // Topic name
    for &byte in topic_bytes {
        let _ = packet.push(byte);
    }

    // Payload
    for &byte in payload {
        let _ = packet.push(byte);
    }

    packet
}

/// Encode SUBACK packet
///
/// Returns heapless::Vec with the encoded packet
pub fn encode_suback(packet_id: u16, topic_count: u8) -> heapless::Vec<u8, 256> {
    let mut packet = heapless::Vec::new();

    // Fixed header: packet type 9 (SUBACK)
    let _ = packet.push(0x90);

    // Remaining length: 2 (packet ID) + topic_count (granted QoS bytes)
    let remaining_length = 2 + topic_count as usize;
    let rem_len_encoded = encode_variable_length(remaining_length);
    for byte in rem_len_encoded {
        let _ = packet.push(byte);
    }

    // Packet identifier (big endian)
    let pid_bytes = packet_id.to_be_bytes();
    let _ = packet.push(pid_bytes[0]);
    let _ = packet.push(pid_bytes[1]);

    // Granted QoS for each topic (always 0 for QoS 0)
    for _ in 0..topic_count {
        let _ = packet.push(0x00);
    }

    packet
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_decode_remaining_length() {
        // Single byte
        let result = decode_remaining_length(&[0x05]);
        assert_eq!(result, Ok((5, 1)));

        // Two bytes
        let result = decode_remaining_length(&[0x81, 0x02]);
        assert_eq!(result, Ok((1 + 2 * 128, 2)));
    }

    #[test]
    fn test_parse_length_prefixed_string() {
        let data = [0x00, 0x05, b'H', b'e', b'l', b'l', b'o'];
        let (s, len) = parse_length_prefixed_string(&data).unwrap();
        assert_eq!(s, "Hello");
        assert_eq!(len, 7);
    }

    #[test]
    fn test_encode_connack() {
        let connack = encode_connack();
        assert_eq!(connack, [0x20, 0x02, 0x00, 0x00]);
    }

    #[test]
    fn test_encode_publish_qos0() {
        let publish = encode_publish_qos0("test/topic", b"hello");
        assert_eq!(publish[0], 0x30); // PUBLISH packet type
                                      // Check topic is in the packet
        let packet_str = core::str::from_utf8(&publish).unwrap();
        assert!(packet_str.contains("test/topic"));
    }

    #[test]
    fn test_encode_variable_length() {
        let encoded = encode_variable_length(5);
        assert_eq!(encoded.as_slice(), &[5]);

        let encoded = encode_variable_length(129);
        assert_eq!(encoded.len(), 2);
        assert_eq!(encoded[0] & 0x80, 0x80); // Continuation bit set
    }

    #[test]
    fn test_encode_suback() {
        let suback = encode_suback(1234, 2);
        assert_eq!(suback[0], 0x90); // SUBACK packet type
        assert_eq!(suback[2], 1234u16.to_be_bytes()[0]); // Packet ID high byte
        assert_eq!(suback[3], 1234u16.to_be_bytes()[1]); // Packet ID low byte
        assert_eq!(suback[4], 0x00); // Granted QoS 0
        assert_eq!(suback[5], 0x00); // Granted QoS 0
    }
}
