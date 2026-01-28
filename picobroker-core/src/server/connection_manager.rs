//! Connection manager for TCP streams
//!
//! Manages the lifecycle of TCP stream connections for all sessions,
//! including stream storage, retrieval, and cleanup of disconnected sessions.

use crate::protocol::heapless::HeaplessVec;
use crate::traits::TcpListener;

/// Manages TCP stream connections for all sessions
pub struct ConnectionManager<TL, const MAX_SESSIONS: usize>
where
    TL: TcpListener,
{
    streams: HeaplessVec<(u128, Option<TL::Stream>), MAX_SESSIONS>,
}

impl<TL, const MAX_SESSIONS: usize> ConnectionManager<TL, MAX_SESSIONS>
where
    TL: TcpListener,
{
    /// Create a new connection manager
    pub fn new() -> Self {
        Self {
            streams: HeaplessVec::new(),
        }
    }

    /// Get mutable reference to session's stream
    pub fn get_stream_mut(&mut self, session_id: u128) -> Option<&mut TL::Stream> {
        for (cid, stream_option) in self.streams.iter_mut() {
            if *cid == session_id {
                return stream_option.as_mut();
            }
        }
        None
    }

    /// Set or replace a session's stream
    pub fn set_stream(&mut self, session_id: u128, stream: TL::Stream) {
        // First try to find and replace an existing entry
        for (cid, stream_option) in self.streams.iter_mut() {
            if *cid == session_id {
                *stream_option = Some(stream);
                return;
            }
        }

        // Try to find an empty slot (where stream is None) and reuse it
        for (cid, stream_option) in self.streams.iter_mut() {
            if stream_option.is_none() {
                *cid = session_id;
                *stream_option = Some(stream);
                return;
            }
        }

        // No empty slot found, try to push a new entry
        if self.streams.push((session_id, Some(stream))).is_err() {
            log::error!("Failed to add stream for session {}: connection manager full", session_id);
        }
    }

    /// Remove a session's stream by setting it to None
    pub fn remove_session(&mut self, session_id: u128) -> bool {
        for (cid, stream_option) in self.streams.iter_mut() {
            if *cid == session_id {
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

impl<TL, const MAX_SESSIONS: usize> Default for ConnectionManager<TL, MAX_SESSIONS>
where
    TL: TcpListener,
{
    fn default() -> Self {
        Self::new()
    }
}
