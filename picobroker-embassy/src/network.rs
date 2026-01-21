//! Embassy networking implementation (stubs)

use picobroker_core::{Error, Result, SocketAddr, TcpListener, TcpStream};

/// Embassy TCP stream wrapper
///
/// Stub implementation - TODO: Implement when Embassy dependencies are available
/// This will use embassy_net::tcp::TcpSocket
pub struct EmbassyTcpStream<'a> {
    _phantom: core::marker::PhantomData<&'a ()>,
}

impl<'a> EmbassyTcpStream<'a> {
    pub async fn from_socket(_socket: embassy_net::tcp::TcpSocket<'a>) -> Result<Self> {
        // TODO: Implement when Embassy dependencies are available
        Err(Error::IoError)
    }
}

impl<'a> TcpStream for EmbassyTcpStream<'a> {
    fn read<'life0, 'life1>(
        &'life0 mut self,
        _buf: &'life1 mut [u8],
    ) -> impl core::future::Future<Output = Result<usize>> + 'life0
    where
        'life1: 'life0,
    {
        async move {
            // TODO: Implement Embassy TCP read
            Err(Error::IoError)
        }
    }

    fn write<'life0, 'life1>(
        &'life0 mut self,
        _buf: &'life1 [u8],
    ) -> impl core::future::Future<Output = Result<usize>> + 'life0
    where
        'life1: 'life0,
    {
        async move {
            // TODO: Implement Embassy TCP write
            Err(Error::IoError)
        }
    }

    fn close<'life0>(
        &'life0 mut self,
    ) -> impl core::future::Future<Output = Result<()>> + 'life0 {
        async move {
            // TODO: Implement Embassy TCP close
            Err(Error::IoError)
        }
    }
}

/// Embassy TCP listener wrapper
///
/// Stub implementation - TODO: Implement when Embassy dependencies are available
pub struct EmbassyTcpListener<'a> {
    _phantom: core::marker::PhantomData<&'a ()>,
}

impl<'a> EmbassyTcpListener<'a> {
    /// TODO: Bind to address using Embassy when available
    pub async fn bind(_addr: &str) -> Result<Self> {
        // TODO: Implement when Embassy dependencies are available
        Err(Error::BindError)
    }
}

impl<'a> TcpListener for EmbassyTcpListener<'a> {
    type Stream = EmbassyTcpStream<'a>;

    fn accept<'life0>(
        &'life0 mut self,
    ) -> impl core::future::Future<Output = Result<(Self::Stream, SocketAddr)>> + 'life0 {
        async move {
            // TODO: Implement Embassy TCP accept
            Err(Error::AcceptError)
        }
    }
}
