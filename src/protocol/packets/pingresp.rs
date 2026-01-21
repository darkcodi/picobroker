use crate::protocol::packets::PacketEncoder;
use crate::Error;

#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct PingResp<'a> {
    pub _phantom: core::marker::PhantomData<&'a ()>,
}

impl<'a> PacketEncoder<'a> for PingResp<'a> {
    fn encode(&'a self, _buffer: &mut [u8]) -> Result<usize, Error> {
        Ok(0)
    }

    fn decode(_payload: &'a [u8], _header: u8) -> Result<Self, Error> {
        Ok(Self::default())
    }
}
