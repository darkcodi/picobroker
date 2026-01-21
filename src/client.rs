use crate::error::{Error, Result};

const DEFAULT_CLIENT_NAME_LENGTH: usize = 32;

/// Client name
/// Represents an MQTT client name with a maximum length.
#[derive(Debug, Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ClientName<const MAX_CLIENT_NAME_LENGTH: usize = DEFAULT_CLIENT_NAME_LENGTH>(
    heapless::String<MAX_CLIENT_NAME_LENGTH>,
);

impl<const MAX_CLIENT_NAME_LENGTH: usize> ClientName<MAX_CLIENT_NAME_LENGTH> {
    pub fn new(name: heapless::String<MAX_CLIENT_NAME_LENGTH>) -> Self {
        ClientName(name)
    }
}

impl<const MAX_CLIENT_NAME_LENGTH: usize> From<heapless::String<MAX_CLIENT_NAME_LENGTH>>
    for ClientName<MAX_CLIENT_NAME_LENGTH>
{
    fn from(name: heapless::String<MAX_CLIENT_NAME_LENGTH>) -> Self {
        ClientName(name)
    }
}

impl<const MAX_CLIENT_NAME_LENGTH: usize> TryFrom<&str> for ClientName<MAX_CLIENT_NAME_LENGTH> {
    type Error = Error;

    fn try_from(value: &str) -> Result<Self> {
        let client_name_str =
            heapless::String::try_from(value).map_err(|_| Error::TopicLengthExceeded {
                max_length: MAX_CLIENT_NAME_LENGTH,
                actual_length: value.len(),
            })?;
        Ok(ClientName(client_name_str))
    }
}

impl<const MAX_CLIENT_NAME_LENGTH: usize> core::ops::Deref for ClientName<MAX_CLIENT_NAME_LENGTH> {
    type Target = heapless::String<MAX_CLIENT_NAME_LENGTH>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<const MAX_CLIENT_NAME_LENGTH: usize> core::ops::DerefMut
    for ClientName<MAX_CLIENT_NAME_LENGTH>
{
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<const MAX_CLIENT_NAME_LENGTH: usize> core::fmt::Display
    for ClientName<MAX_CLIENT_NAME_LENGTH>
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Client connection state
pub struct ClientState {
    pub name: ClientName,
    pub keep_alive_secs: u16,
    pub last_activity: u64,
}

impl ClientState {
    pub fn new(name: ClientName, keep_alive_secs: u16, current_time: u64) -> Self {
        Self {
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
pub struct ClientRegistry<const MAX_CLIENTS: usize> {
    clients: heapless::Vec<Option<ClientState>, MAX_CLIENTS>,
}

impl<const MAX_CLIENTS: usize> ClientRegistry<MAX_CLIENTS> {
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
    ) -> Result<usize> {
        // Check if client already exists
        if self.find_index(&name).is_some() {
            return Err(Error::ClientAlreadyConnected);
        }

        // Find empty slot or add new
        if let Some(slot) = self.clients.iter().position(|c| c.is_none()) {
            self.clients[slot] = Some(ClientState::new(name, keep_alive, current_time));
            Ok(slot)
        } else {
            // Add to end
            self.clients
                .push(Some(ClientState::new(name, keep_alive, current_time)))
                .map_err(|_| Error::MaxClientsReached {
                    max_clients: MAX_CLIENTS,
                })?;
            Ok(self.clients.len() - 1)
        }
    }

    /// Unregister a client
    pub fn unregister(&mut self, name: &ClientName) -> bool {
        if let Some(index) = self.find_index(name) {
            self.clients[index] = None;
            true
        } else {
            false
        }
    }

    /// Update client activity timestamp
    pub fn update_activity(&mut self, name: &ClientName, current_time: u64) -> bool {
        if let Some(index) = self.find_index(name) {
            if let Some(client) = self.clients.get_mut(index) {
                if let Some(state) = client {
                    state.update_activity(current_time);
                    return true;
                }
            }
        }
        false
    }

    /// Get list of expired clients
    pub fn get_expired_clients(&self, current_time: u64) -> heapless::Vec<ClientName, MAX_CLIENTS> {
        let mut expired = heapless::Vec::new();
        for client in &self.clients {
            if let Some(state) = client {
                if state.is_expired(current_time) {
                    let _ = expired.push(state.name.clone());
                }
            }
        }
        expired
    }

    /// Find client index by name
    pub fn find_index(&self, name: &ClientName) -> Option<usize> {
        self.clients.iter().position(|c| {
            if let Some(state) = c {
                &state.name == name
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

impl<const MAX_CLIENTS: usize> Default for ClientRegistry<MAX_CLIENTS> {
    fn default() -> Self {
        Self::new()
    }
}
