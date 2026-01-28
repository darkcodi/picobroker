//! Buffer manager for session read buffers
//!
//! Manages read buffers for all connected clients, including buffer creation,
//! overflow handling, and packet decoding from buffers.

use crate::protocol::heapless::HeaplessVec;
use crate::protocol::packet_error::PacketEncodingError;
use crate::protocol::packets::{Packet, PacketEncoder};
use crate::protocol::utils::read_variable_length;
use log::{error, info};

/// Session read buffer
///
/// Stores incoming data for a session until complete packets can be decoded.
#[derive(Debug, Default, Clone)]
pub struct SessionReadBuffer<const MAX_PAYLOAD_SIZE: usize> {
    pub session_id: u128,
    buffer: HeaplessVec<u8, MAX_PAYLOAD_SIZE>,
    len: usize,
}

impl<const MAX_PAYLOAD_SIZE: usize> SessionReadBuffer<MAX_PAYLOAD_SIZE> {
    /// Create a new read buffer for a session
    pub fn new(session_id: u128) -> Self {
        Self {
            session_id,
            buffer: HeaplessVec::new(),
            len: 0,
        }
    }

    /// Append data to the buffer
    pub fn append(&mut self, data: &[u8]) {
        for &byte in data {
            if self.buffer.push(byte).is_ok() {
                self.len += 1;
            } else {
                break; // Buffer full
            }
        }
    }

    /// Get buffer as a slice
    pub fn as_slice(&self) -> &[u8] {
        self.buffer.as_slice()
    }

    /// Get buffer length
    pub fn len(&self) -> usize {
        self.len
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Remove element at index
    pub fn remove(&mut self, index: usize) {
        if index < self.len {
            self.buffer.remove(index);
            self.len -= 1;
        }
    }

    /// Push a single byte to the buffer
    pub fn push(&mut self, byte: u8) -> Result<(), crate::protocol::heapless::PushError> {
        self.buffer.push(byte)?;
        self.len += 1;
        Ok(())
    }

    /// Clear the buffer
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.len = 0;
    }
}

/// Manages read buffers for all sessions
pub struct BufferManager<const MAX_PAYLOAD_SIZE: usize, const MAX_SESSIONS: usize> {
    buffers: HeaplessVec<SessionReadBuffer<MAX_PAYLOAD_SIZE>, MAX_SESSIONS>,
}

impl<const MAX_PAYLOAD_SIZE: usize, const MAX_SESSIONS: usize>
    BufferManager<MAX_PAYLOAD_SIZE, MAX_SESSIONS>
{
    /// Create a new buffer manager
    pub fn new() -> Self {
        Self {
            buffers: HeaplessVec::new(),
        }
    }

    /// Get or create a buffer for a session
    pub fn get_buffer_mut(
        &mut self,
        session_id: u128,
    ) -> &mut SessionReadBuffer<MAX_PAYLOAD_SIZE> {
        // Check if buffer exists
        let idx = self.buffers.iter().position(|b| b.session_id == session_id);
        if let Some(idx) = idx {
            return &mut self.buffers[idx];
        }

        // Create new buffer
        let buf = SessionReadBuffer::new(session_id);
        let _ = self.buffers.push(buf);

        // Return the newly added buffer (it's now at the end)
        let new_idx = self.buffers.len() - 1;
        &mut self.buffers[new_idx]
    }

    /// Get remaining space in a session's buffer
    pub fn get_remaining_space(&mut self, session_id: u128) -> usize {
        let buffer = self.get_buffer_mut(session_id);
        MAX_PAYLOAD_SIZE - buffer.len()
    }

    /// Remove a session's buffer
    pub fn remove_session(&mut self, session_id: u128) -> bool {
        let idx = self.buffers.iter().position(|b| b.session_id == session_id);
        if let Some(idx) = idx {
            self.buffers.remove(idx);
            true
        }
        else {
            false
        }
    }

    /// Try to decode a packet from a session's buffer
    ///
    /// Returns Ok(Some(packet)) if a complete packet was decoded,
    /// Ok(None) if more data is needed, or Err if the packet is invalid.
    pub fn try_decode_packet<const MAX_TOPIC_NAME_LENGTH: usize>(
        &mut self,
        session_id: u128,
    ) -> Result<Option<Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>>, PacketEncodingError> {
        let buffer = self.get_buffer_mut(session_id);
        Self::decode_from_buffer(buffer)
    }

    /// Try to decode a packet from a read buffer
    ///
    /// This is a standalone function that decodes a packet from a buffer,
    /// removing the decoded bytes from the buffer if successful.
    fn decode_from_buffer<const MAX_TOPIC_NAME_LENGTH: usize>(
        rx_buffer: &mut SessionReadBuffer<MAX_PAYLOAD_SIZE>,
    ) -> Result<Option<Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>>, PacketEncodingError> {
        // Step 1: Check if we have enough data (minimum packet size is 2 bytes)
        if rx_buffer.len() < 2 {
            return Ok(None);
        }

        // Step 2: Decode Remaining Length
        let remaining_length_result = read_variable_length(&rx_buffer.as_slice()[1..]);
        let (remaining_length, var_int_len) = match remaining_length_result {
            Ok((len, bytes)) => (len, bytes),
            Err(PacketEncodingError::IncompletePacket { .. }) => {
                return Ok(None);
            }
            Err(e) => {
                error!("Invalid remaining length encoding: {}", e);
                return Err(e);
            }
        };

        // Step 3: Calculate total packet size
        let total_packet_size = 1 + var_int_len + remaining_length;

        // Step 4: Check if we have complete packet
        if total_packet_size > rx_buffer.len() {
            return Ok(None);
        }

        // Step 5: Decode packet
        let packet_slice = &rx_buffer.as_slice()[..total_packet_size];
        let packet_result = Packet::decode(packet_slice);

        let packet = match packet_result {
            Ok(p) => p,
            Err(e) => {
                error!("Failed to decode packet: {}", e);
                return Err(e);
            }
        };

        // Step 6: Remove decoded packet from buffer
        for _ in 0..total_packet_size {
            if !rx_buffer.is_empty() {
                rx_buffer.remove(0);
            }
        }

        info!(
            "Decoded packet, {} bytes remaining in buffer",
            rx_buffer.len()
        );

        Ok(Some(packet))
    }
}

impl<const MAX_PAYLOAD_SIZE: usize, const MAX_SESSIONS: usize> Default
    for BufferManager<MAX_PAYLOAD_SIZE, MAX_SESSIONS>
{
    fn default() -> Self {
        Self::new()
    }
}
