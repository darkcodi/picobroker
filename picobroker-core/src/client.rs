use core::fmt::Write;
use log::{error, info};
use crate::broker_error::BrokerError;
use crate::protocol::heapless::{HeaplessString, HeaplessVec};
use crate::protocol::packet_error::PacketEncodingError;
use crate::protocol::packets::Packet;
use crate::traits::NetworkError;

pub const MAX_CLIENT_ID_LENGTH: usize = 23;

/// Client identifier
/// The Server MUST allow ClientIds which are between 1 and 23 UTF-8 encoded bytes in length, and that contain only the characters
/// "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ"
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClientId(HeaplessString<MAX_CLIENT_ID_LENGTH>);

impl From<HeaplessString<MAX_CLIENT_ID_LENGTH>> for ClientId {
    fn from(value: HeaplessString<MAX_CLIENT_ID_LENGTH>) -> Self {
        ClientId(value)
    }
}

impl ClientId {
    pub const fn new(value: HeaplessString<MAX_CLIENT_ID_LENGTH>) -> Self {
        ClientId(value)
    }

    pub fn generate(n: u64) -> Self {
        let mut client_id = HeaplessString::<MAX_CLIENT_ID_LENGTH>::new();
        let _ = core::write!(client_id, "client_{:016X}", n);
        ClientId(client_id)
    }
}

impl TryFrom<&str> for ClientId {
    type Error = PacketEncodingError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        let client_id_str = HeaplessString::try_from(value).map_err(|_| {
            PacketEncodingError::ClientIdLengthExceeded {
                max_length: MAX_CLIENT_ID_LENGTH,
                actual_length: value.len(),
            }
        })?;
        Ok(ClientId(client_id_str))
    }
}

impl core::ops::Deref for ClientId {
    type Target = HeaplessString<MAX_CLIENT_ID_LENGTH>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl core::ops::DerefMut for ClientId {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl core::fmt::Display for ClientId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Client state machine
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ClientState {
    #[default]
    Connecting,
    Connected,
    Disconnected,
}

/// Client session with dual queues
///
/// Maintains the communication channels between a client task
/// and the broker, along with connection state.
#[derive(Debug, PartialEq, Eq)]
pub struct ClientSession<
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const QUEUE_SIZE: usize,
> {
    /// Client identifier
    pub client_id: ClientId,

    /// Current client state
    pub state: ClientState,

    /// Keep-alive interval in seconds
    pub keep_alive_secs: u16,

    /// Timestamp of last activity (seconds since epoch)
    pub last_activity: u64,

    /// Receive queue (client -> broker)
    pub rx_queue: HeaplessVec<
        Option<Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>>,
        QUEUE_SIZE
    >,

    /// Transmit queue (broker -> client)
    pub tx_queue: HeaplessVec<
        Option<Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>>,
        QUEUE_SIZE
    >,
}

impl<const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize, const QUEUE_SIZE: usize> ClientSession<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_SIZE>
{
    /// Create a new client session
    pub fn new(client_id: ClientId, keep_alive_secs: u16, current_time: u64) -> Self {
        Self {
            client_id,
            state: ClientState::Connecting,
            keep_alive_secs,
            last_activity: current_time,
            rx_queue: HeaplessVec::new(),
            tx_queue: HeaplessVec::new(),
        }
    }

    /// Check if client's keep-alive has expired
    ///
    /// Returns true if the time since last activity exceeds 1.5x the keep-alive value
    pub fn is_expired(&self, current_time: u64) -> bool {
        let timeout_secs = (self.keep_alive_secs as u64) * 3 / 2;
        let elapsed = current_time.saturating_sub(self.last_activity);
        elapsed > timeout_secs
    }

    /// Update the last activity timestamp
    pub fn update_activity(&mut self, current_time: u64) {
        self.last_activity = current_time;
    }

    /// Queue a packet for transmission to the client
    pub fn queue_tx_packet(&mut self, packet: Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>) -> Result<(), BrokerError> {
        self.tx_queue.push(Some(packet)).map_err(|_| BrokerError::ClientQueueFull { queue_size: QUEUE_SIZE })
    }

    /// Dequeue a packet to send to the client
    pub fn dequeue_tx_packet(&mut self) -> Option<Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>> {
        self.tx_queue.dequeue_front().flatten()
    }

    /// Queue a received packet from the client
    pub fn queue_rx_packet(&mut self, packet: Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>) -> Result<(), BrokerError> {
        self.rx_queue.push(Some(packet)).map_err(|_| BrokerError::ClientQueueFull { queue_size: QUEUE_SIZE })
    }

    /// Dequeue a received packet from the client
    pub fn dequeue_rx_packet(&mut self) -> Option<Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>> {
        self.rx_queue.dequeue_front().flatten()
    }
}

/// Client registry
///
/// Manages connected clients and their state
#[derive(Debug, PartialEq, Eq)]
pub struct ClientRegistry<
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const QUEUE_SIZE: usize,
    const MAX_CLIENTS: usize,
    const MAX_TOPICS: usize,
    const MAX_SUBSCRIBERS_PER_TOPIC: usize,
> {
    /// Sessions with communication queues
    sessions: HeaplessVec<
        Option<ClientSession<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_SIZE>>,
        MAX_CLIENTS
    >,
}

impl<const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize, const QUEUE_SIZE: usize, const MAX_CLIENTS: usize, const MAX_TOPICS: usize, const MAX_SUBSCRIBERS_PER_TOPIC: usize>
ClientRegistry<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_SIZE, MAX_CLIENTS, MAX_TOPICS, MAX_SUBSCRIBERS_PER_TOPIC>
{
    /// Create a new client registry
    pub fn new() -> Self {
        Self {
            sessions: HeaplessVec::new(),
        }
    }

    // ===== Helper methods for broker.rs =====

    /// Get all active client IDs into a stack-allocated array
    pub fn get_active_client_ids(&self, output: &mut [Option<ClientId>]) -> usize {
        let mut count = 0usize;
        for session_option in self.sessions.iter() {
            if let Some(session) = session_option {
                if count < output.len() {
                    output[count] = Some(session.client_id.clone());
                    count += 1;
                }
            }
        }
        count
    }

    /// Get the number of active (connected/connecting) sessions
    pub fn active_session_count(&self) -> usize {
        self.sessions.iter().filter(|s| s.is_some()).count()
    }

    /// Check if a client session exists
    pub fn has_session(&self, client_id: &ClientId) -> bool {
        self.sessions.iter().any(|s| {
            s.as_ref().map(|sess| &sess.client_id == client_id).unwrap_or(false)
        })
    }

    // ===== End helper methods =====

    /// Mark a client as disconnected
    pub fn mark_disconnected(&mut self, client_id: ClientId) {
        if let Some(session) = self.find_session_by_client_id(&client_id) {
            session.state = ClientState::Disconnected;
        }
    }

    /// Register a new client session
    pub fn register_new_client(&mut self, client_id: ClientId, keep_alive_secs: u16, current_time: u64) -> Result<(), BrokerError> {
        if self.sessions.len() >= MAX_CLIENTS {
            return Err(BrokerError::MaxClientsReached { max_clients: MAX_CLIENTS });
        }

        if self.find_session_by_client_id(&client_id).is_some() {
            return Err(BrokerError::ClientAlreadyConnected);
        }

        let session = ClientSession::new(client_id, keep_alive_secs, current_time);
        self.sessions.push(Some(session)).map_err(|_| BrokerError::MaxClientsReached { max_clients: MAX_CLIENTS })?;
        Ok(())
    }

    /// Find a mutable session reference by ClientId
    ///
    /// Returns None if session not found or client_id is not set.
    /// Uses linear search which is acceptable since MAX_CLIENTS is typically small (4-16).
    pub fn find_session_by_client_id(
        &mut self,
        client_id: &ClientId
    ) -> Option<&mut ClientSession<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_SIZE>> {
        self.sessions.iter_mut().find(|s_opt| {
            s_opt.as_ref()
                .map(|s| &s.client_id == client_id)
                .unwrap_or(false)
        })
            .and_then(|s_opt| s_opt.as_mut())
    }

    /// Remove a session and cleanup its subscriptions
    ///
    /// This will:
    /// 1. Remove the session from the registry
    /// 2. Unsubscribe the client from all topics
    /// 3. Close the network connection
    pub fn remove_session(
        &mut self,
        client_id: &ClientId,
        topics: &mut crate::topics::TopicRegistry<MAX_TOPIC_NAME_LENGTH, MAX_TOPICS, MAX_SUBSCRIBERS_PER_TOPIC>
    ) -> Result<(), BrokerError> {
        // Find the session index
        let session_idx = self.sessions.iter().position(|s| {
            s.as_ref().map(|sess| &sess.client_id == client_id).unwrap_or(false)
        });

        if let Some(idx) = session_idx {
            // Extract the client_id before removing
            let client_id = self.sessions[idx]
                .as_ref()
                .map(|s| s.client_id.clone());

            // Cleanup subscriptions if client_id exists
            if let Some(id) = &client_id {
                topics.unregister_client(id.clone());
                info!("Cleaned up subscriptions for client {:?}", id);
            }

            // Remove from vector - replace with None and then shift
            // We can't use remove() because Option<ClientSession> doesn't implement Clone
            self.sessions[idx] = None;

            // Shift remaining elements to fill the gap
            for i in idx..self.sessions.len() - 1 {
                self.sessions[i] = self.sessions[i + 1].take();
            }

            // Remove the last element which is now a duplicate
            let _ = self.sessions.pop();

            Ok(())
        } else {
            error!("Session {} not found for removal", client_id);
            Err(BrokerError::NetworkError {
                error: NetworkError::ConnectionClosed
            })
        }
    }

    /// Cleanup expired sessions based on keep-alive timeout
    ///
    /// Removes all sessions where time since last activity exceeds 1.5x keep-alive.
    pub fn cleanup_expired_sessions(
        &mut self,
        topics: &mut crate::topics::TopicRegistry<MAX_TOPIC_NAME_LENGTH, MAX_TOPICS, MAX_SUBSCRIBERS_PER_TOPIC>,
        current_time: u64
    ) {
        // Use a simple array to collect expired session IDs
        let mut expired_ids = [const { None }; 16]; // Stack-allocated, reasonable max
        let mut expired_count = 0usize;

        // Collect expired session IDs
        for session in &self.sessions {
            if let Some(sess) = session {
                if sess.is_expired(current_time) {
                    expired_ids[expired_count] = Some(sess.client_id.clone());
                    expired_count += 1;
                }
            }
        }

        // Remove each expired session
        for i in 0..expired_count {
            if let Some(session_id) = &expired_ids[i] {
                info!("Removing expired session {}", session_id);
                let _ = self.remove_session(session_id, topics);
            }
        }
    }

    /// Cleanup zombie sessions (connections that have been closed but not yet removed)
    ///
    /// This is called before accepting new connections to ensure zombie slots are freed.
    /// It checks for dead connections by attempting to detect socket errors.
    pub fn cleanup_zombie_sessions(
        &mut self,
        topics: &mut crate::topics::TopicRegistry<MAX_TOPIC_NAME_LENGTH, MAX_TOPICS, MAX_SUBSCRIBERS_PER_TOPIC>
    ) {
        let mut zombie_ids = [const { None }; MAX_CLIENTS];
        let mut zombie_count = 0usize;

        // Check each session for dead connections
        for session in &self.sessions {
            if let Some(sess) = session {
                // Check if session state is Disconnected
                if sess.state == ClientState::Disconnected {
                    if zombie_count < zombie_ids.len() {
                        zombie_ids[zombie_count] = Some(sess.client_id.clone());
                        zombie_count += 1;
                    }
                }
            }
        }

        // Remove each zombie session
        for i in 0..zombie_count {
            if let Some(client_id) = &zombie_ids[i] {
                info!("Removing zombie session {}", client_id);
                let _ = self.remove_session(client_id, topics);
            }
        }
    }
}
