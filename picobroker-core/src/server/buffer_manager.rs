//! Buffer manager for client read buffers
//!
//! Manages read buffers for all connected clients, including buffer creation,
//! overflow handling, and packet decoding from buffers.

use crate::client::ClientId;
use crate::protocol::heapless::HeaplessVec;
use crate::protocol::packet_error::PacketEncodingError;
use crate::protocol::packets::{Packet, PacketEncoder};
use crate::protocol::utils::read_variable_length;
use log::{error, info};

/// Client read buffer
///
/// Stores incoming data for a client until complete packets can be decoded.
#[derive(Debug, Default, Clone)]
pub struct ClientReadBuffer<const MAX_PAYLOAD_SIZE: usize> {
    pub client_id: ClientId,
    buffer: HeaplessVec<u8, MAX_PAYLOAD_SIZE>,
    len: usize,
}

impl<const MAX_PAYLOAD_SIZE: usize> ClientReadBuffer<MAX_PAYLOAD_SIZE> {
    /// Create a new read buffer for a client
    pub fn new(client_id: ClientId) -> Self {
        Self {
            client_id,
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

/// Manages read buffers for all clients
pub struct BufferManager<const MAX_PAYLOAD_SIZE: usize, const MAX_CLIENTS: usize> {
    buffers: HeaplessVec<ClientReadBuffer<MAX_PAYLOAD_SIZE>, MAX_CLIENTS>,
}

impl<const MAX_PAYLOAD_SIZE: usize, const MAX_CLIENTS: usize>
    BufferManager<MAX_PAYLOAD_SIZE, MAX_CLIENTS>
{
    /// Create a new buffer manager
    pub fn new() -> Self {
        Self {
            buffers: HeaplessVec::new(),
        }
    }

    /// Get or create a buffer for a client
    pub fn get_buffer_mut(
        &mut self,
        client_id: &ClientId,
    ) -> &mut ClientReadBuffer<MAX_PAYLOAD_SIZE> {
        // Check if buffer exists
        let idx = self.buffers.iter().position(|b| &b.client_id == client_id);
        if let Some(idx) = idx {
            return &mut self.buffers[idx];
        }

        // Create new buffer
        let buf = ClientReadBuffer::new(client_id.clone());
        let _ = self.buffers.push(buf);

        // Return the newly added buffer (it's now at the end)
        let new_idx = self.buffers.len() - 1;
        &mut self.buffers[new_idx]
    }

    /// Get remaining space in a client's buffer
    pub fn get_remaining_space(&mut self, client_id: &ClientId) -> usize {
        let buffer = self.get_buffer_mut(client_id);
        MAX_PAYLOAD_SIZE - buffer.len()
    }

    /// Remove a client's buffer
    pub fn remove_client(&mut self, client_id: &ClientId) -> bool {
        let idx = self.buffers.iter().position(|b| b.client_id == *client_id);
        if let Some(idx) = idx {
            self.buffers.remove(idx);
            true
        }
        else {
            false
        }
    }

    /// Try to decode a packet from a client's buffer
    ///
    /// Returns Ok(Some(packet)) if a complete packet was decoded,
    /// Ok(None) if more data is needed, or Err if the packet is invalid.
    pub fn try_decode_packet<const MAX_TOPIC_NAME_LENGTH: usize>(
        &mut self,
        client_id: &ClientId,
    ) -> Result<Option<Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>>, PacketEncodingError> {
        let buffer = self.get_buffer_mut(client_id);
        Self::decode_from_buffer(buffer)
    }

    /// Try to decode a packet from a read buffer
    ///
    /// This is a standalone function that decodes a packet from a buffer,
    /// removing the decoded bytes from the buffer if successful.
    fn decode_from_buffer<const MAX_TOPIC_NAME_LENGTH: usize>(
        rx_buffer: &mut ClientReadBuffer<MAX_PAYLOAD_SIZE>,
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

impl<const MAX_PAYLOAD_SIZE: usize, const MAX_CLIENTS: usize> Default
    for BufferManager<MAX_PAYLOAD_SIZE, MAX_CLIENTS>
{
    fn default() -> Self {
        Self::new()
    }
}
