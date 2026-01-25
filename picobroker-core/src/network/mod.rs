//! Networking types - pure data structures only

use crate::BrokerError;

/// Socket address (IPv4)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SocketAddr {
    pub ip: [u8; 4],
    pub port: u16,
}

/// TCP stream trait for Tokio
#[allow(async_fn_in_trait)]
pub trait TcpStream {
    /// Read data from the stream into the buffer
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, BrokerError>;

    /// Write data from the buffer to the stream
    async fn write(&mut self, buf: &[u8]) -> Result<usize, BrokerError>;

    /// Close the stream
    async fn close(&mut self) -> Result<(), BrokerError>;
}

/// TCP listener trait for Tokio
#[allow(async_fn_in_trait)]
pub trait TcpListener {
    /// The stream type produced by this listener
    type Stream: TcpStream;

    /// Accept a new connection
    async fn accept(&mut self) -> Result<(Self::Stream, SocketAddr), BrokerError>;

    /// Try to accept a new connection without blocking
    ///
    /// Returns immediately with an error if no connection is pending.
    /// This is used in the server main loop to allow periodic processing
    /// of client messages between connection attempts.
    async fn try_accept(&mut self) -> Result<(Self::Stream, SocketAddr), BrokerError>;
}
