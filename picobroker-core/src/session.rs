use log::error;
use crate::error::BrokerError;
use crate::client::ClientId;
use crate::protocol::heapless::HeaplessVec;
use crate::protocol::packets::Packet;

/// Session state machine
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum SessionState {
    #[default]
    Connecting,
    Connected,
    Disconnected,
}

/// Session with dual queues
///
/// Manages the state and communication queues for a connected session
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Session<
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const QUEUE_SIZE: usize,
> {
    /// Unique session identifier
    pub session_id: u128,

    /// Client identifier
    pub client_id: Option<ClientId>,

    /// Current session state
    pub state: SessionState,

    /// Keep-alive interval in seconds
    pub keep_alive_secs: u16,

    /// Timestamp of last activity (nanoseconds since epoch)
    pub last_activity: u128,

    /// Receive queue (client -> broker)
    pub rx_queue: HeaplessVec<Option<Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>>, QUEUE_SIZE>,

    /// Transmit queue (broker -> session)
    pub tx_queue: HeaplessVec<Option<Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>>, QUEUE_SIZE>,
}

impl<
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const QUEUE_SIZE: usize,
> Session<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_SIZE>
{
    /// Create a new session
    pub fn new(session_id: u128, keep_alive_secs: u16, current_time: u128) -> Self {
        Self {
            session_id,
            client_id: None,
            state: SessionState::Connecting,
            keep_alive_secs,
            last_activity: current_time,
            rx_queue: HeaplessVec::new(),
            tx_queue: HeaplessVec::new(),
        }
    }

    /// Check if session's keep-alive has expired
    ///
    /// Returns true if the time since last activity exceeds 1.5x the keep-alive value
    pub fn is_expired(&self, current_time: u128) -> bool {
        let timeout_secs = (self.keep_alive_secs as u128) * 3 / 2;
        let elapsed = current_time.saturating_sub(self.last_activity);
        elapsed > timeout_secs
    }

    /// Update the last activity timestamp
    pub fn update_activity(&mut self, current_time: u128) {
        self.last_activity = current_time;
    }

    /// Queue a packet for transmission to the session
    pub fn queue_tx_packet(
        &mut self,
        packet: Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>,
    ) -> Result<(), BrokerError> {
        self.tx_queue
            .push(Some(packet))
            .map_err(|_| BrokerError::BufferFull)
    }

    /// Dequeue a packet to send to the session
    pub fn dequeue_tx_packet(&mut self) -> Option<Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>> {
        self.tx_queue.dequeue_front().flatten()
    }

    /// Queue a received packet from the session
    pub fn queue_rx_packet(
        &mut self,
        packet: Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>,
    ) -> Result<(), BrokerError> {
        self.rx_queue
            .push(Some(packet))
            .map_err(|_| BrokerError::BufferFull)
    }

    /// Dequeue a received packet from the session
    pub fn dequeue_rx_packet(&mut self) -> Option<Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>> {
        self.rx_queue.dequeue_front().flatten()
    }
}

/// Session registry
///
/// Manages all sessions
#[derive(Debug, PartialEq, Eq)]
pub struct SessionRegistry<
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const QUEUE_SIZE: usize,
    const MAX_SESSIONS: usize,
    const MAX_TOPICS: usize,
    const MAX_SUBSCRIBERS_PER_TOPIC: usize,
> {
    /// Vector of sessions
    sessions: HeaplessVec<
        Option<Session<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_SIZE>>,
        MAX_SESSIONS,
    >,
}

impl<
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const QUEUE_SIZE: usize,
    const MAX_SESSIONS: usize,
    const MAX_TOPICS: usize,
    const MAX_SUBSCRIBERS_PER_TOPIC: usize,
> Default
for SessionRegistry<
    MAX_TOPIC_NAME_LENGTH,
    MAX_PAYLOAD_SIZE,
    QUEUE_SIZE,
    MAX_SESSIONS,
    MAX_TOPICS,
    MAX_SUBSCRIBERS_PER_TOPIC,
>
{
    fn default() -> Self {
        Self {
            sessions: HeaplessVec::new(),
        }
    }
}

impl<
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const QUEUE_SIZE: usize,
    const MAX_SESSIONS: usize,
    const MAX_TOPICS: usize,
    const MAX_SUBSCRIBERS_PER_TOPIC: usize,
>
SessionRegistry<
    MAX_TOPIC_NAME_LENGTH,
    MAX_PAYLOAD_SIZE,
    QUEUE_SIZE,
    MAX_SESSIONS,
    MAX_TOPICS,
    MAX_SUBSCRIBERS_PER_TOPIC,
>
{
    /// Get all session IDs
    pub fn get_all_sessions(&self) -> [Option<u128>; MAX_SESSIONS] {
        let mut session_ids = [const { None }; MAX_SESSIONS];
        let mut count = 0usize;

        for sess in (&self.sessions).into_iter().flatten() {
            if count < session_ids.len() {
                session_ids[count] = Some(sess.session_id.clone());
                count += 1;
            }
        }

        session_ids
    }

    /// Get expired session IDs based on current time
    pub fn get_expired_sessions(&self, current_time: u128) -> [Option<u128>; MAX_SESSIONS] {
        let mut session_ids = [const { None }; MAX_SESSIONS];
        let mut count = 0usize;

        for session in (&self.sessions).into_iter().flatten() {
            if session.is_expired(current_time) && count < session_ids.len() {
                session_ids[count] = Some(session.session_id.clone());
                count += 1;
            }
        }

        session_ids
    }

    /// Get disconnected session IDs
    pub fn get_disconnected_sessions(&self) -> [Option<u128>; MAX_SESSIONS] {
        let mut session_ids = [const { None }; MAX_SESSIONS];
        let mut count = 0usize;

        for session in (&self.sessions).into_iter().flatten() {
            if session.state == SessionState::Disconnected && count < session_ids.len() {
                session_ids[count] = Some(session.session_id.clone());
                count += 1;
            }
        }

        session_ids
    }

    /// Mark a session as connected
    pub fn mark_connected(&mut self, session_id: u128) -> Result<(), BrokerError> {
        if let Some(session) = self.find_session(session_id) {
            session.state = SessionState::Connected;
            Ok(())
        } else {
            Err(BrokerError::SessionNotFound { session_id })
        }
    }

    /// Mark a session as disconnected
    pub fn mark_disconnected(&mut self, session_id: u128) -> Result<(), BrokerError> {
        if let Some(session) = self.find_session(session_id) {
            session.state = SessionState::Disconnected;
            Ok(())
        } else {
            Err(BrokerError::SessionNotFound { session_id })
        }
    }

    /// Register a new session
    pub fn register_new_session(
        &mut self,
        session_id: u128,
        keep_alive_secs: u16,
        current_time: u128,
    ) -> Result<(), BrokerError> {
        let current = self.sessions.len();
        if current >= MAX_SESSIONS {
            return Err(BrokerError::MaxSessionsReached {
                current,
                max: MAX_SESSIONS,
            });
        }

        if self.find_session(session_id).is_some() {
            return Err(BrokerError::SessionAlreadyExists { session_id });
        }

        let session = Session::new(session_id, keep_alive_secs, current_time);
        self.sessions
            .push(Some(session))
            .map_err(|_| BrokerError::BufferFull)?;
        Ok(())
    }

    /// Remove a session by ID
    pub fn remove_session(&mut self, session_id: u128) -> bool {
        let session_idx = self.sessions.iter().position(|s| {
            s.as_ref()
                .map(|session| session.session_id == session_id)
                .unwrap_or(false)
        });

        if let Some(idx) = session_idx {
            self.sessions.remove(idx);
            true
        } else {
            error!("Session {} not found for removal", session_id);
            false
        }
    }

    /// Update the last activity timestamp for a session
    pub fn update_session_activity(
        &mut self,
        session_id: u128,
        current_time: u128,
    ) -> Result<(), BrokerError> {
        if let Some(session) = self.find_session(session_id) {
            session.update_activity(current_time);
            Ok(())
        } else {
            Err(BrokerError::SessionNotFound { session_id })
        }
    }

    /// Update the client ID for an existing session
    pub fn update_client_id(
        &mut self,
        session_id: u128,
        client_id: ClientId,
    ) -> Result<(), BrokerError> {
        if let Some(session) = self.find_session(session_id) {
            session.client_id = Some(client_id);
            Ok(())
        } else {
            Err(BrokerError::SessionNotFound { session_id })
        }
    }

    /// Update the keep-alive interval for a session
    pub fn update_keep_alive(
        &mut self,
        session_id: u128,
        keep_alive_secs: u16,
    ) -> Result<(), BrokerError> {
        if let Some(session) = self.find_session(session_id) {
            session.keep_alive_secs = keep_alive_secs;
            Ok(())
        } else {
            Err(BrokerError::SessionNotFound { session_id })
        }
    }

    /// Queue a packet received from the client
    pub fn queue_rx_packet(
        &mut self,
        session_id: u128,
        packet: Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>,
    ) -> Result<(), BrokerError> {
        if let Some(session) = self.find_session(session_id) {
            session.queue_rx_packet(packet).map_err(|_| BrokerError::SessionQueueFull {
                session_id,
                queue_size: QUEUE_SIZE,
            })
        } else {
            Err(BrokerError::SessionNotFound { session_id })
        }
    }

    /// Queue a packet to send to the client
    pub fn queue_tx_packet(
        &mut self,
        session_id: u128,
        packet: Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>,
    ) -> Result<(), BrokerError> {
        if let Some(session) = self.find_session(session_id) {
            session.queue_tx_packet(packet).map_err(|_| BrokerError::SessionQueueFull {
                session_id,
                queue_size: QUEUE_SIZE,
            })
        } else {
            Err(BrokerError::SessionNotFound { session_id })
        }
    }

    /// Dequeue a packet received from the client
    pub fn dequeue_rx_packet(
        &mut self,
        session_id: u128,
    ) -> Result<Option<Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>>, BrokerError> {
        if let Some(session) = self.find_session(session_id) {
            Ok(session.dequeue_rx_packet())
        } else {
            Err(BrokerError::SessionNotFound { session_id })
        }
    }

    /// Dequeue a packet to send to the session
    pub fn dequeue_tx_packet(
        &mut self,
        session_id: u128,
    ) -> Result<Option<Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>>, BrokerError> {
        if let Some(session) = self.find_session(session_id) {
            Ok(session.dequeue_tx_packet())
        } else {
            Err(BrokerError::SessionNotFound { session_id })
        }
    }

    fn find_session(
        &mut self,
        session_id: u128,
    ) -> Option<&mut Session<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_SIZE>> {
        self.sessions
            .iter_mut()
            .find(|s_opt| {
                s_opt
                    .as_ref()
                    .map(|s| s.session_id == session_id)
                    .unwrap_or(false)
            })
            .and_then(|s_opt| s_opt.as_mut())
    }
}
