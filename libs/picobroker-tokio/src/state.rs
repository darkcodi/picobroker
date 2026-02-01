use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use dashmap::DashMap;

/// Session ID generation counter and storage
#[derive(Debug)]
pub struct SessionIdGenerator(AtomicU64);

impl SessionIdGenerator {
    pub fn new() -> Self {
        Self(AtomicU64::new(0))
    }

    /// Generate a unique session ID using timestamp + counter
    pub fn generate(&self) -> u128 {
        let base = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as u128;
        let counter = self.0.fetch_add(1, Ordering::Relaxed) as u128;
        (base << 32) | (counter as u128)
    }
}

impl Default for SessionIdGenerator {
    fn default() -> Self {
        Self::new()
    }
}

/// Lightweight session metadata
#[derive(Debug)]
pub struct ConnectionHandle {
    pub session_id: u128,
    pub peer_addr: String,
}

/// Server state tracking connections and notifications
pub struct ServerState {
    pub connections: DashMap<u128, ConnectionHandle>,
    session_id_gen: SessionIdGenerator,
    notification_senders: DashMap<u128, tokio::sync::mpsc::Sender<()>>,
}

impl ServerState {
    pub fn new() -> Self {
        Self {
            connections: DashMap::new(),
            session_id_gen: SessionIdGenerator::new(),
            notification_senders: DashMap::new(),
        }
    }

    pub fn register_notification(&self, session_id: u128, sender: tokio::sync::mpsc::Sender<()>) {
        self.notification_senders.insert(session_id, sender);
    }

    pub fn remove_notification(&self, session_id: u128) {
        self.notification_senders.remove(&session_id);
    }

    pub async fn notify_session(&self, session_id: u128) {
        if let Some(sender) = self.notification_senders.get(&session_id) {
            let _ = sender.send(()).await;
        }
    }

    pub fn generate_session_id(&self) -> u128 {
        self.session_id_gen.generate()
    }

    pub fn connection_count(&self) -> usize {
        self.connections.len()
    }
}

impl Default for ServerState {
    fn default() -> Self {
        Self::new()
    }
}

/// Get current time as nanoseconds since UNIX epoch
pub fn current_time_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_id_generation() {
        let gen = SessionIdGenerator::new();
        let id1 = gen.generate();
        let id2 = gen.generate();
        assert_ne!(id1, id2);
    }

    #[test]
    fn test_server_state_default() {
        let state = ServerState::default();
        assert_eq!(state.connection_count(), 0);
    }
}
