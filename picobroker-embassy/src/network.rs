//! Embassy networking implementation (stubs)

use async_trait::async_trait;
use picobroker_core::{Error, Result, SocketAddr};

/// TCP stream trait for Embassy
#[async_trait]
pub trait TcpStream {
    /// Read data from the stream into the buffer
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize>;

    /// Write data from the buffer to the stream
    async fn write(&mut self, buf: &[u8]) -> Result<usize>;

    /// Close the stream
    async fn close(&mut self) -> Result<()>;
}

/// TCP listener trait for Embassy
#[async_trait]
pub trait TcpListener {
    /// The stream type produced by this listener
    type Stream: TcpStream;

    /// Accept a new connection
    async fn accept(&mut self) -> Result<(Self::Stream, SocketAddr)>;
}

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

#[async_trait]
impl<'a> TcpStream for EmbassyTcpStream<'a> {
    async fn read(&mut self, _buf: &mut [u8]) -> Result<usize> {
        // TODO: Implement Embassy TCP read
        Err(Error::IoError)
    }

    async fn write(&mut self, _buf: &[u8]) -> Result<usize> {
        // TODO: Implement Embassy TCP write
        Err(Error::IoError)
    }

    async fn close(&mut self) -> Result<()> {
        // TODO: Implement Embassy TCP close
        Err(Error::IoError)
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

#[async_trait]
impl<'a> TcpListener for EmbassyTcpListener<'a> {
    type Stream = EmbassyTcpStream<'a>;

    async fn accept(&mut self) -> Result<(Self::Stream, SocketAddr)> {
        // TODO: Implement Embassy TCP accept
        Err(Error::AcceptError)
    }
}
