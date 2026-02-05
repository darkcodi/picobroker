//! Packet I/O functions for Embassy-based MQTT broker.

use embassy_net::tcp::TcpSocket;
use picobroker::protocol::packets::{Packet, PacketEncoder};
use picobroker::protocol::ProtocolError;

/// Parse variable length integer from buffer (MQTT remaining length format)
pub fn parse_remaining_length(buffer: &[u8]) -> Result<(usize, usize), ProtocolError> {
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

/// Read complete MQTT packet from socket
pub async fn read_packet<'a, const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize>(
    socket: &mut TcpSocket<'a>,
    buffer: &mut [u8],
    buffer_len: &mut usize,
) -> Result<Option<Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>>, ProtocolError> {
    // Ensure at least 2 bytes (packet type + at least 1 byte of remaining length)
    while *buffer_len < 2 {
        match socket.read(&mut buffer[*buffer_len..]).await {
            Ok(0) => return Ok(None), // EOF - client closed connection
            Ok(n) => *buffer_len += n,
            Err(_) => return Err(ProtocolError::IncompletePacket { available: *buffer_len }),
        }
    }

    // Parse remaining length
    let (remaining_len, var_len_bytes) = parse_remaining_length(&buffer[..*buffer_len])?;
    let total_len = 1 + var_len_bytes + remaining_len;

    // Ensure full packet is available
    while *buffer_len < total_len {
        match socket.read(&mut buffer[*buffer_len..]).await {
            Ok(0) => return Err(ProtocolError::IncompletePacket { available: *buffer_len }),
            Ok(n) => *buffer_len += n,
            Err(_) => return Err(ProtocolError::IncompletePacket { available: *buffer_len }),
        }
    }

    // Decode packet
    let packet = Packet::decode(&buffer[..total_len])?;

    // Shift remaining data (manual buffer management, no BytesMut)
    let remaining = *buffer_len - total_len;
    buffer.copy_within(total_len.., 0);
    *buffer_len = remaining;

    Ok(Some(packet))
}

/// Write packet to socket
pub async fn write_packet<'a, const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize>(
    socket: &mut TcpSocket<'a>,
    packet: &Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>,
    buffer: &mut [u8],
) -> Result<(), ProtocolError> {
    let size = packet
        .encode(buffer)
        .map_err(|_| ProtocolError::BufferTooSmall { buffer_size: 0 })?;

    // Embassy TcpSocket doesn't have write_all, so write in a loop
    let mut written = 0;
    while written < size {
        match socket.write(&buffer[written..size]).await {
            Ok(0) => return Err(ProtocolError::IncompletePacket { available: written }),
            Ok(n) => written += n,
            Err(_) => return Err(ProtocolError::BufferTooSmall { buffer_size: 0 }),
        }
    }

    Ok(())
}
