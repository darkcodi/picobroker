use crate::Error;

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
