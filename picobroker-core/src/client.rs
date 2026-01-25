use crate::protocol::{HeaplessString, HeaplessVec};
use crate::{BrokerError, Packet, PacketEncodingError, SocketAddr};

const MAX_CLIENT_ID_LENGTH: usize = 23;

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
    /// Unique session identifier
    pub session_id: usize,

    /// Client socket address
    pub socket_addr: SocketAddr,

    /// Client identifier
    pub client_id: Option<ClientId>,

    /// Current client state
    pub state: ClientState,

    /// Keep-alive interval in seconds
    pub keep_alive_secs: u16,

    /// Timestamp of last activity (seconds since epoch)
    pub last_activity: u64,
}

impl<const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize, const QUEUE_SIZE: usize> ClientSession<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_SIZE>
{
    /// Create a new client session
    pub fn new(socket_addr: SocketAddr, keep_alive_secs: u16, current_time: u64) -> Self {
        let session_id = socket_addr.port as usize; // Simple session ID based on port for demo purposes
        Self {
            session_id,
            socket_addr,
            client_id: None,
            state: ClientState::Connecting,
            keep_alive_secs,
            last_activity: current_time,
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
> {
    /// Sessions with communication queues
    sessions: HeaplessVec<
        Option<ClientSession<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_SIZE>>,
        MAX_CLIENTS
    >,
}

impl<const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize, const QUEUE_SIZE: usize, const MAX_CLIENTS: usize>
    ClientRegistry<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_SIZE, MAX_CLIENTS>
{
    /// Create a new client registry
    pub fn new() -> Self {
        Self {
            sessions: HeaplessVec::new(),
        }
    }

    /// Register a new client session
    pub fn register_new_client(&mut self, socket_addr: SocketAddr, keep_alive_secs: u16, current_time: u64) -> Result<usize, BrokerError> {
        if self.sessions.len() >= MAX_CLIENTS {
            return Err(BrokerError::MaxClientsReached { max_clients: MAX_CLIENTS });
        }
        let session = ClientSession::new(socket_addr, keep_alive_secs, current_time);
        let session_id = session.session_id;
        self.sessions.push(Some(session)).map_err(|_| BrokerError::MaxClientsReached { max_clients: MAX_CLIENTS })?;
        Ok(session_id)
    }
}
