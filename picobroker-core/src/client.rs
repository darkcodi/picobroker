use crate::error::{Error, Result};

const MAX_CLIENT_NAME_LENGTH: usize = 23;

/// Client name
/// The Server MUST allow ClientIds which are between 1 and 23 UTF-8 encoded bytes in length, and that contain only the characters
/// "0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ"
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClientName(heapless::String<MAX_CLIENT_NAME_LENGTH>);

impl From<heapless::String<MAX_CLIENT_NAME_LENGTH>> for ClientName
{
    fn from(name: heapless::String<MAX_CLIENT_NAME_LENGTH>) -> Self {
        ClientName(name)
    }
}

impl TryFrom<&str> for ClientName {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self> {
        let client_name_str =
            heapless::String::try_from(value).map_err(|_| Error::ClientNameLengthExceeded {
                max_length: MAX_CLIENT_NAME_LENGTH,
                actual_length: value.len(),
            })?;
        Ok(ClientName(client_name_str))
    }
}

impl core::ops::Deref for ClientName {
    type Target = heapless::String<MAX_CLIENT_NAME_LENGTH>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl core::ops::DerefMut for ClientName
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl core::fmt::Display for ClientName
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Opaque client identifier
///
/// Decouples the topic registry from client name implementation details.
/// This allows the topic registry to work with simple integer IDs instead
/// of carrying the MAX_CLIENT_NAME_LENGTH parameter.
#[derive(Debug, Clone, Default, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct ClientId(usize);

impl ClientId {
    /// Create a new ClientId
    pub const fn new(id: usize) -> Self {
        Self(id)
    }

    /// Get the underlying ID value
    pub fn get(&self) -> usize {
        self.0
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
    pub name: ClientName,
    pub keep_alive_secs: u16,
    pub last_activity: u64,
}

impl Client {
    pub fn new(
        id: ClientId,
        name: ClientName,
        keep_alive_secs: u16,
        current_time: u64,
    ) -> Self {
        Self {
            id,
            name,
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
    clients: heapless::Vec<Option<Client>, MAX_CLIENTS>,
}

impl<const MAX_CLIENTS: usize> ClientRegistry<MAX_CLIENTS>
{
    /// Create a new client registry
    pub const fn new() -> Self {
        Self {
            clients: heapless::Vec::new(),
        }
    }

    /// Register a new client
    pub fn register(
        &mut self,
        name: ClientName,
        keep_alive: u16,
        current_time: u64,
    ) -> Result<ClientId> {
        // Check if client already exists
        if self.find_index(&name).is_some() {
            return Err(Error::ClientAlreadyConnected);
        }

        // Find empty slot or add new
        if let Some(slot) = self.clients.iter().position(|c| c.is_none()) {
            let id = ClientId::new(slot);
            self.clients[slot] = Some(Client::new(id, name, keep_alive, current_time));
            Ok(id)
        } else {
            // Add to end
            let slot = self.clients.len();
            if slot >= MAX_CLIENTS {
                return Err(Error::MaxClientsReached {
                    max_clients: MAX_CLIENTS,
                });
            }
            let id = ClientId::new(slot);
            self.clients
                .push(Some(Client::new(id, name, keep_alive, current_time)))
                .map_err(|_| Error::MaxClientsReached {
                    max_clients: MAX_CLIENTS,
                })?;
            Ok(id)
        }
    }

    /// Unregister a client
    pub fn unregister(&mut self, name: &ClientName) -> Option<ClientId> {
        if let Some(index) = self.find_index(name) {
            let client_id = self.clients[index].as_ref().map(|c| c.id);
            self.clients[index] = None;
            client_id
        } else {
            None
        }
    }

    /// Update client activity timestamp
    pub fn update_activity(
        &mut self,
        name: &ClientName,
        current_time: u64,
    ) -> bool {
        if let Some(index) = self.find_index(name) {
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
    ) -> heapless::Vec<ClientName, MAX_CLIENTS> {
        let mut expired = heapless::Vec::new();
        for client in &self.clients {
            if let Some(client) = client {
                if client.is_expired(current_time) {
                    let _ = expired.push(client.name.clone());
                }
            }
        }
        expired
    }

    /// Find client index by name
    pub fn find_index(&self, name: &ClientName) -> Option<usize> {
        self.clients.iter().position(|c| {
            if let Some(client) = c {
                &client.name == name
            } else {
                false
            }
        })
    }

    /// Check if client is connected
    pub fn is_connected(&self, name: &ClientName) -> bool {
        self.find_index(name).is_some()
    }

    /// Get number of connected clients
    pub fn count(&self) -> usize {
        self.clients.iter().filter(|c| c.is_some()).count()
    }
}
