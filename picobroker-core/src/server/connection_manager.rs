//! Connection manager for TCP streams
//!
//! Manages the lifecycle of TCP stream connections for all clients,
//! including stream storage, retrieval, and cleanup of disconnected clients.

use crate::client::ClientId;
use crate::protocol::heapless::HeaplessVec;
use crate::traits::TcpListener;

/// Manages TCP stream connections for all clients
pub struct ConnectionManager<TL, const MAX_CLIENTS: usize>
where
    TL: TcpListener,
{
    streams: HeaplessVec<(ClientId, Option<TL::Stream>), MAX_CLIENTS>,
}

impl<TL, const MAX_CLIENTS: usize> ConnectionManager<TL, MAX_CLIENTS>
where
    TL: TcpListener,
{
    /// Create a new connection manager
    pub fn new() -> Self {
        Self {
            streams: HeaplessVec::new(),
        }
    }

    /// Get mutable reference to client's stream
    pub fn get_stream_mut(&mut self, client_id: &ClientId) -> Option<&mut TL::Stream> {
        for (cid, stream_option) in self.streams.iter_mut() {
            if *cid == *client_id {
                return stream_option.as_mut();
            }
        }
        None
    }

    /// Set or replace a client's stream
    pub fn set_stream(&mut self, client_id: ClientId, stream: TL::Stream) {
        for (cid, stream_option) in self.streams.iter_mut() {
            if *cid == client_id {
                *stream_option = Some(stream);
                return;
            }
        }
        let _ = self.streams.push((client_id, Some(stream)));
    }

    /// Remove a client's stream (set to None instead of removing to maintain heapless vec stability)
    pub fn remove_client(&mut self, client_id: &ClientId) -> bool {
        for (cid, stream_option) in self.streams.iter_mut() {
            if *cid == *client_id {
                *stream_option = None;
                return true;
            }
        }
        false
    }

    /// Get the number of active streams
    pub fn active_stream_count(&self) -> usize {
        self.streams.iter().filter(|(_, s)| s.is_some()).count()
    }
}

impl<TL, const MAX_CLIENTS: usize> Default for ConnectionManager<TL, MAX_CLIENTS>
where
    TL: TcpListener,
{
    fn default() -> Self {
        Self::new()
    }
}
