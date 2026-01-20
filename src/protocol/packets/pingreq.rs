use crate::Error;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PingReq;

impl Default for PingReq {
    fn default() -> Self {
        Self::new()
    }
}

impl PingReq {
    pub const fn new() -> Self {
        Self
    }

    pub fn decode(_bytes: &[u8]) -> Result<Self, Error> {
        Ok(Self)
    }

    pub fn encode(&self, _buffer: &mut [u8]) -> Result<usize, Error> {
        Ok(0)
    }
}
