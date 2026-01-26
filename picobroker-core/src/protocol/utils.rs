use core::fmt::Write;
use crate::{HeaplessString, PacketEncodingError};
use crate::protocol::HeaplessVec;

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

pub fn read_variable_length(bytes: &[u8]) -> Result<(usize, usize), PacketEncodingError> {
    // MQTT spec limits variable length to 268,435,455 (0x0FFFFFFF)
    const MAX_VARIABLE_LENGTH: usize = 268_435_455;

    let mut multiplier = 1;
    let mut value = 0usize;
    let mut bytes_read = 0usize;

    loop {
        if bytes_read >= bytes.len() {
            return Err(PacketEncodingError::IncompletePacket {
                buffer_size: bytes.len(),
            });
        }
        let byte = bytes[bytes_read] as usize;
        bytes_read += 1;
        value += (byte & 0x7F) * multiplier;

        // Check if value exceeds MQTT spec maximum BEFORE processing continuation
        if value > MAX_VARIABLE_LENGTH {
            return Err(PacketEncodingError::InvalidPacketLength {
                expected: MAX_VARIABLE_LENGTH,
                actual: value,
            });
        }

        multiplier *= 128;

        // Check multiplier to prevent more than 4 bytes (spec limit)
        if multiplier > 128 * 128 * 128 * 128 {
            return Err(PacketEncodingError::InvalidPacketLength {
                expected: MAX_VARIABLE_LENGTH,
                actual: value,
            });
        }

        if (byte & 0x80) == 0 {
            break;
        }
    }

    Ok((value, bytes_read))
}

pub fn write_variable_length(
    value: usize,
    buffer: &mut [u8],
) -> Result<usize, PacketEncodingError> {
    // MQTT spec limits variable length to 268,435,455 (0x0FFFFFFF)
    const MAX_VARIABLE_LENGTH: usize = 268_435_455;

    if value > MAX_VARIABLE_LENGTH {
        return Err(PacketEncodingError::InvalidPacketLength {
            expected: MAX_VARIABLE_LENGTH,
            actual: value,
        });
    }

    let mut encoded = value;
    let mut bytes_written = 0;

    loop {
        if bytes_written >= buffer.len() {
            return Err(PacketEncodingError::BufferTooSmall {
                buffer_size: buffer.len(),
            });
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

pub fn read_string<'a>(
    bytes: &'a [u8],
    offset: &'_ mut usize,
) -> Result<&'a str, PacketEncodingError> {
    if *offset + 2 > bytes.len() {
        return Err(PacketEncodingError::IncompletePacket {
            buffer_size: bytes.len(),
        });
    }
    let len = u16::from_be_bytes([bytes[*offset], bytes[*offset + 1]]) as usize;
    *offset += 2;
    if *offset + len > bytes.len() {
        return Err(PacketEncodingError::IncompletePacket {
            buffer_size: bytes.len(),
        });
    }
    let str_bytes = &bytes[*offset..*offset + len];
    *offset += len;
    let str_slice =
        core::str::from_utf8(str_bytes).map_err(|_| PacketEncodingError::InvalidUtf8String)?;
    Ok(str_slice)
}

pub fn write_string(
    s: &str,
    buffer: &mut [u8],
    offset: &mut usize,
) -> Result<(), PacketEncodingError> {
    let bytes = s.as_bytes();
    let len = bytes.len();
    if *offset + 2 + len > buffer.len() {
        return Err(PacketEncodingError::BufferTooSmall {
            buffer_size: buffer.len(),
        });
    }
    let len_bytes = (len as u16).to_be_bytes();
    buffer[*offset] = len_bytes[0];
    buffer[*offset + 1] = len_bytes[1];
    *offset += 2;
    buffer[*offset..*offset + len].copy_from_slice(bytes);
    *offset += len;
    Ok(())
}

pub fn read_binary<'a>(
    bytes: &'a [u8],
    offset: &'_ mut usize,
) -> Result<&'a [u8], PacketEncodingError> {
    if *offset + 2 > bytes.len() {
        return Err(PacketEncodingError::IncompletePacket {
            buffer_size: bytes.len(),
        });
    }
    let len = u16::from_be_bytes([bytes[*offset], bytes[*offset + 1]]) as usize;
    *offset += 2;
    if *offset + len > bytes.len() {
        return Err(PacketEncodingError::IncompletePacket {
            buffer_size: bytes.len(),
        });
    }
    let bin_bytes = &bytes[*offset..*offset + len];
    *offset += len;
    Ok(bin_bytes)
}

pub fn write_binary(
    data: &[u8],
    buffer: &mut [u8],
    offset: &mut usize,
) -> Result<(), PacketEncodingError> {
    let len = data.len();
    if *offset + 2 + len > buffer.len() {
        return Err(PacketEncodingError::BufferTooSmall {
            buffer_size: buffer.len(),
        });
    }
    let len_bytes = (len as u16).to_be_bytes();
    buffer[*offset] = len_bytes[0];
    buffer[*offset + 1] = len_bytes[1];
    *offset += 2;
    buffer[*offset..*offset + len].copy_from_slice(data);
    *offset += len;
    Ok(())
}

pub fn hex_to_bytes<const N: usize>(hex: &str) -> HeaplessVec<u8, N> {
    let mut result = HeaplessVec::new();
    for s in hex.split_whitespace() {
        if let Ok(b) = u8::from_str_radix(s, 16) {
            let _ = result.push(b);
        }
    }
    result
}

pub fn bytes_to_hex<const N: usize>(bytes: &[u8]) -> HeaplessString<N> {
    let mut result = HeaplessString::new();
    for (i, byte) in bytes.iter().enumerate() {
        if i > 0 {
            let _ = result.push(' ');
        }
        let _ = core::write!(result, "{:02X}", byte);
    }
    result
}