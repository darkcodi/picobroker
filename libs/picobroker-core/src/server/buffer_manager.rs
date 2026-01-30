use crate::protocol::heapless::HeaplessVec;
use crate::protocol::packets::{Packet, PacketEncoder};
use crate::protocol::utils::read_variable_length;
use crate::protocol::ProtocolError;
use log::{error, info};

#[derive(Debug, Default, Clone)]
pub struct SessionReadBuffer<const MAX_PAYLOAD_SIZE: usize> {
    pub session_id: u128,
    buffer: HeaplessVec<u8, MAX_PAYLOAD_SIZE>,
    len: usize,
}

impl<const MAX_PAYLOAD_SIZE: usize> SessionReadBuffer<MAX_PAYLOAD_SIZE> {
    pub fn new(session_id: u128) -> Self {
        Self {
            session_id,
            buffer: HeaplessVec::new(),
            len: 0,
        }
    }

    pub fn append(&mut self, data: &[u8]) {
        for &byte in data {
            if self.buffer.push(byte).is_ok() {
                self.len += 1;
            } else {
                break;
            }
        }
    }

    pub fn as_slice(&self) -> &[u8] {
        self.buffer.as_slice()
    }

    pub fn len(&self) -> usize {
        self.len
    }

    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    pub fn remove(&mut self, index: usize) {
        if index < self.len {
            self.buffer.remove(index);
            self.len -= 1;
        }
    }

    pub fn push(&mut self, byte: u8) -> Result<(), crate::protocol::heapless::PushError> {
        self.buffer.push(byte)?;
        self.len += 1;
        Ok(())
    }

    pub fn clear(&mut self) {
        self.buffer.clear();
        self.len = 0;
    }
}

pub struct BufferManager<const MAX_PAYLOAD_SIZE: usize, const MAX_SESSIONS: usize> {
    buffers: HeaplessVec<SessionReadBuffer<MAX_PAYLOAD_SIZE>, MAX_SESSIONS>,
}

impl<const MAX_PAYLOAD_SIZE: usize, const MAX_SESSIONS: usize>
    BufferManager<MAX_PAYLOAD_SIZE, MAX_SESSIONS>
{
    pub fn new() -> Self {
        Self {
            buffers: HeaplessVec::new(),
        }
    }

    pub fn get_buffer_mut(&mut self, session_id: u128) -> &mut SessionReadBuffer<MAX_PAYLOAD_SIZE> {
        let idx = self.buffers.iter().position(|b| b.session_id == session_id);
        if let Some(idx) = idx {
            return &mut self.buffers[idx];
        }

        let buf = SessionReadBuffer::new(session_id);
        let _ = self.buffers.push(buf);

        let new_idx = self.buffers.len() - 1;
        &mut self.buffers[new_idx]
    }

    pub fn get_remaining_space(&mut self, session_id: u128) -> usize {
        let buffer = self.get_buffer_mut(session_id);
        MAX_PAYLOAD_SIZE - buffer.len()
    }

    pub fn remove_session(&mut self, session_id: u128) -> bool {
        let idx = self.buffers.iter().position(|b| b.session_id == session_id);
        if let Some(idx) = idx {
            self.buffers.remove(idx);
            true
        } else {
            false
        }
    }

    pub fn try_decode_packet<const MAX_TOPIC_NAME_LENGTH: usize>(
        &mut self,
        session_id: u128,
    ) -> Result<Option<Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>>, ProtocolError> {
        let buffer = self.get_buffer_mut(session_id);
        Self::decode_from_buffer(buffer)
    }

    fn decode_from_buffer<const MAX_TOPIC_NAME_LENGTH: usize>(
        rx_buffer: &mut SessionReadBuffer<MAX_PAYLOAD_SIZE>,
    ) -> Result<Option<Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>>, ProtocolError> {
        if rx_buffer.len() < 2 {
            return Ok(None);
        }

        let remaining_length_result = read_variable_length(&rx_buffer.as_slice()[1..]);
        let (remaining_length, var_int_len) = match remaining_length_result {
            Ok((len, bytes)) => (len, bytes),
            Err(ProtocolError::IncompletePacket { .. }) => {
                return Ok(None);
            }
            Err(e) => {
                error!("Invalid remaining length encoding: {}", e);
                return Err(e);
            }
        };

        let total_packet_size = 1 + var_int_len + remaining_length;

        if total_packet_size > rx_buffer.len() {
            return Ok(None);
        }

        let packet_slice = &rx_buffer.as_slice()[..total_packet_size];
        let packet_result = Packet::decode(packet_slice);

        let packet = match packet_result {
            Ok(p) => p,
            Err(e) => {
                error!("Failed to decode packet: {}", e);
                return Err(e);
            }
        };

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
