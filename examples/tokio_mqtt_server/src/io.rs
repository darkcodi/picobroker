use bytes::{Buf, Bytes, BytesMut};
use picobroker::protocol::packets::{Packet, PacketEncoder};
use picobroker::protocol::ProtocolError;
use tokio::io::AsyncReadExt;
use tokio::net::TcpStream;

/// Parse MQTT variable length integer from buffer
pub fn parse_remaining_length(buffer: &BytesMut) -> Result<(usize, usize), ProtocolError> {
    let mut idx = 1;
    let mut multiplier = 1usize;
    let mut value = 0usize;

    loop {
        if idx >= buffer.len() {
            return Err(ProtocolError::IncompletePacket {
                available: buffer.len(),
            });
        }
        let byte = buffer[idx] as usize;
        idx += 1;
        value += (byte & 0x7F) * multiplier;

        if (byte & 0x80) == 0 {
            break;
        }

        multiplier *= 128;
        if multiplier > 128 * 128 * 128 {
            return Err(ProtocolError::InvalidPacketLength {
                expected: 4,
                actual: 5,
            });
        }
    }

    Ok((value, idx - 1))
}

/// Read a complete MQTT packet from the stream
pub async fn read_packet<const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize>(
    socket: &mut TcpStream,
    buffer: &mut BytesMut,
) -> Result<Option<Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>>, ProtocolError> {
    // Ensure we have at least 2 bytes (packet type + min remaining length)
    while buffer.len() < 2 {
        buffer.reserve(2);
        let n = socket.read_buf(buffer).await.unwrap_or(0);
        if n == 0 {
            return Ok(None); // EOF
        }
    }

    // Parse remaining length
    let (remaining_len, var_len_bytes) = parse_remaining_length(buffer)?;
    let total_len = 1 + var_len_bytes + remaining_len;

    // Ensure we have the full packet
    while buffer.len() < total_len {
        buffer.reserve(total_len - buffer.len());
        let n = socket.read_buf(buffer).await.unwrap_or(0);
        if n == 0 {
            return Err(ProtocolError::IncompletePacket {
                available: buffer.len(),
            });
        }
    }

    // Decode the packet
    let packet = Packet::decode(&buffer[..total_len])?;

    // Advance buffer (keep any excess for next packet)
    buffer.advance(total_len);

    Ok(Some(packet))
}

/// Encode a packet into Bytes for zero-copy transmission
pub fn encode_frame<const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize>(
    packet: &Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>,
) -> Result<Bytes, std::io::Error> {
    // Allocate a buffer on heap.
    // We need space for packet type (1) + remaining length (up to 4) + payload
    let buffer_size = 5 + MAX_PAYLOAD_SIZE + MAX_TOPIC_NAME_LENGTH;
    let mut buffer = vec![0u8; buffer_size];

    // Encode the packet using the PacketEncoder trait
    let size = packet.encode(&mut buffer).map_err(|_| {
        std::io::Error::new(std::io::ErrorKind::InvalidData, "Failed to encode packet")
    })?;

    // Convert to Bytes for zero-copy sharing
    Ok(BytesMut::from(&buffer[..size]).freeze())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_remaining_length_single_byte() {
        let mut buffer = BytesMut::new();
        buffer.extend_from_slice(&[0x10, 0x05]); // Packet type + remaining length 5
        let (len, bytes) = parse_remaining_length(&buffer).unwrap();
        assert_eq!(len, 5);
        assert_eq!(bytes, 1);
    }

    #[test]
    fn test_parse_remaining_length_multi_byte() {
        let mut buffer = BytesMut::new();
        buffer.extend_from_slice(&[0x10, 0x80, 0x01]); // Remaining length 128
        let (len, bytes) = parse_remaining_length(&buffer).unwrap();
        assert_eq!(len, 128);
        assert_eq!(bytes, 2);
    }
}
