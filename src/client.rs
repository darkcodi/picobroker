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

impl<const MAX_CLIENT_NAME_LENGTH: usize> defmt::Format for ClientName<MAX_CLIENT_NAME_LENGTH> {
    fn format(&self, f: defmt::Formatter) {
        defmt::write!(f, "{}", self.0.as_str());
    }
}
