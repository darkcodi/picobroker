//! Networking types - pure data structures only

#[allow(async_fn_in_trait)]

/// Socket address (IPv4)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SocketAddr {
    pub ip: [u8; 4],
    pub port: u16,
}

/// TCP stream trait for Tokio
pub trait TcpStream {
    /// Read data from the stream into the buffer
    async fn read(&mut self, buf: &mut [u8]) -> crate::Result<usize>;

    /// Write data from the buffer to the stream
    async fn write(&mut self, buf: &[u8]) -> crate::Result<usize>;

    /// Close the stream
    async fn close(&mut self) -> crate::Result<()>;
}

/// TCP listener trait for Tokio
pub trait TcpListener {
    /// The stream type produced by this listener
    type Stream: TcpStream;

    /// Accept a new connection
    async fn accept(&mut self) -> crate::Result<(Self::Stream, SocketAddr)>;
}
