//! Networking abstraction layer
//!
//! Provides platform-agnostic TCP traits for both std and Embassy platforms

use crate::error::Result;

/// Socket address
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SocketAddr {
    pub ip: [u8; 4],
    pub port: u16,
}

/// TCP stream trait
///
/// Abstracts TCP read/write operations for both std and Embassy platforms
#[async_trait::async_trait]
pub trait TcpStream {
    /// Read data from the stream into the buffer
    ///
    /// Returns the number of bytes read
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize>;

    /// Write data from the buffer to the stream
    ///
    /// Returns the number of bytes written
    async fn write(&mut self, buf: &[u8]) -> Result<usize>;

    /// Close the stream
    async fn close(&mut self) -> Result<()>;
}

/// TCP listener trait
///
/// Abstracts TCP accept operations for both std and Embassy platforms
#[async_trait::async_trait]
pub trait TcpListener {
    /// The stream type produced by this listener
    type Stream: TcpStream;

    /// Accept a new connection
    ///
    /// Returns a tuple of (stream, remote_address)
    async fn accept(&mut self) -> Result<(Self::Stream, SocketAddr)>;
}

// Platform-specific modules
#[cfg(feature = "std")]
pub mod std;

#[cfg(feature = "embassy")]
pub mod embassy;
