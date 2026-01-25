use crate::{BrokerError, PacketEncodingError};
use crate::protocol::{HeaplessString, HeaplessVec};

const MAX_CLIENT_ID_LENGTH: usize = 23;

/// Client identifier
/// The Server MUST allow ClientIds which are between 1 and 23 UTF-8 encoded bytes in length, and that contain only the characters
/// "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ"
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClientId(HeaplessString<MAX_CLIENT_ID_LENGTH>);

impl From<HeaplessString<MAX_CLIENT_ID_LENGTH>> for ClientId
{
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
        let client_id_str =
            HeaplessString::try_from(value).map_err(|_| PacketEncodingError::ClientIdLengthExceeded {
                max_length: MAX_CLIENT_ID_LENGTH,
                actual_length: value.len(),
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

impl core::ops::DerefMut for ClientId
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl core::fmt::Display for ClientId
{
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

#[allow(dead_code)]
pub struct ClientChannel {
    pub packets_to_send: heapless::spsc::Queue<u8, 23>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Client {
    pub id: ClientId,
    pub keep_alive_secs: u16,
    pub last_activity: u64,
}

impl Client {
    pub fn new(
        id: ClientId,
        keep_alive_secs: u16,
        current_time: u64,
    ) -> Self {
        Self {
            id,
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
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ClientRegistry<const MAX_CLIENTS: usize> {
    clients: HeaplessVec<Option<Client>, MAX_CLIENTS>,
}

impl<const MAX_CLIENTS: usize> ClientRegistry<MAX_CLIENTS>
{
    /// Create a new client registry
    pub fn new() -> Self {
        Self {
            clients: HeaplessVec::new(),
        }
    }

    /// Register a new client
    pub fn register(
        &mut self,
        id: ClientId,
        keep_alive: u16,
        current_time: u64,
    ) -> Result<(), BrokerError> {
        // Check if client already exists
        if self.find_index(&id).is_some() {
            return Err(BrokerError::ClientAlreadyConnected);
        }

        // Find empty slot or add new
        if let Some(slot) = self.clients.iter().position(|c| c.is_none()) {
            self.clients[slot] = Some(Client::new(id, keep_alive, current_time));
            Ok(())
        } else {
            // Add to end
            let slot = self.clients.len();
            if slot >= MAX_CLIENTS {
                return Err(BrokerError::MaxClientsReached {
                    max_clients: MAX_CLIENTS,
                });
            }
            self.clients
                .push(Some(Client::new(id, keep_alive, current_time)))
                .map_err(|_| BrokerError::MaxClientsReached {
                    max_clients: MAX_CLIENTS,
                })?;
            Ok(())
        }
    }

    /// Unregister a client
    pub fn unregister(&mut self, id: &ClientId) {
        if let Some(index) = self.find_index(id) {
            self.clients[index] = None;
        }
    }

    /// Update client activity timestamp
    pub fn update_activity(
        &mut self,
        id: &ClientId,
        current_time: u64,
    ) -> bool {
        if let Some(index) = self.find_index(id) {
            if let Some(client) = self.clients.get_mut(index) {
                if let Some(client) = client {
                    client.update_activity(current_time);
                    return true;
                }
            }
        }
        false
    }

    /// Get list of expired clients
    pub fn get_expired_clients(
        &self,
        current_time: u64,
    ) -> HeaplessVec<ClientId, MAX_CLIENTS> {
        let mut expired = HeaplessVec::new();
        for client in &self.clients {
            if let Some(client) = client {
                if client.is_expired(current_time) {
                    let _ = expired.push(client.id.clone());
                }
            }
        }
        expired
    }

    /// Find client index by id
    pub fn find_index(&self, id: &ClientId) -> Option<usize> {
        self.clients.iter().position(|c| {
            if let Some(client) = c {
                &client.id == id
            } else {
                false
            }
        })
    }

    /// Check if client is connected
    pub fn is_connected(&self, id: &ClientId) -> bool {
        self.find_index(id).is_some()
    }

    /// Get number of connected clients
    pub fn count(&self) -> usize {
        self.clients.iter().filter(|c| c.is_some()).count()
    }
}
