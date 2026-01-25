//! Embassy networking implementation (stubs)

use picobroker_core::{BrokerError, SocketAddr, TcpListener, TcpStream};

/// Embassy TCP stream wrapper
///
/// Stub implementation - TODO: Implement when Embassy dependencies are available
/// This will use embassy_net::tcp::TcpSocket
pub struct EmbassyTcpStream<'a> {
    _phantom: core::marker::PhantomData<&'a ()>,
}

impl<'a> EmbassyTcpStream<'a> {
    pub async fn from_socket(
        _socket: embassy_net::tcp::TcpSocket<'a>,
    ) -> Result<Self, BrokerError> {
        // TODO: Implement when Embassy dependencies are available
        Err(BrokerError::IoError)
    }
}

impl<'a> TcpStream for EmbassyTcpStream<'a> {
    async fn read(&mut self, _buf: &mut [u8]) -> Result<usize, BrokerError> {
        // TODO: Implement Embassy TCP read
        Err(BrokerError::IoError)
    }

    async fn write(&mut self, _buf: &[u8]) -> Result<usize, BrokerError> {
        // TODO: Implement Embassy TCP write
        Err(BrokerError::IoError)
    }

    async fn close(&mut self) -> Result<(), BrokerError> {
        // TODO: Implement Embassy TCP close
        Err(BrokerError::IoError)
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
    pub async fn bind(_addr: &str) -> Result<Self, BrokerError> {
        // TODO: Implement when Embassy dependencies are available
        Err(BrokerError::BindError)
    }
}

impl<'a> TcpListener for EmbassyTcpListener<'a> {
    type Stream = EmbassyTcpStream<'a>;

    async fn accept(&mut self) -> Result<(Self::Stream, SocketAddr), BrokerError> {
        // TODO: Implement Embassy TCP accept
        Err(BrokerError::AcceptConnectionError)
    }

    async fn try_accept(&mut self) -> Result<(Self::Stream, SocketAddr), BrokerError> {
        // TODO: Implement non-blocking Embassy TCP accept
        Err(BrokerError::AcceptConnectionError)
    }
}
