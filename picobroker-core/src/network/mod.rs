//! Networking types - pure data structures only

/// Socket address (IPv4)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SocketAddr {
    pub ip: [u8; 4],
    pub port: u16,
}

/// TCP stream trait for Tokio
pub trait TcpStream {
    /// Read data from the stream into the buffer
    fn read<'a, 'b>(
        &'a mut self,
        buf: &'b mut [u8],
    ) -> impl core::future::Future<Output = crate::Result<usize>> + 'a
    where
        'b: 'a;

    /// Write data from the buffer to the stream
    fn write<'a, 'b>(
        &'a mut self,
        buf: &'b [u8],
    ) -> impl core::future::Future<Output = crate::Result<usize>> + 'a
    where
        'b: 'a;

    /// Close the stream
    fn close<'a>(
        &'a mut self,
    ) -> impl core::future::Future<Output = crate::Result<()>> + 'a;
}

/// TCP listener trait for Tokio
pub trait TcpListener {
    /// The stream type produced by this listener
    type Stream: TcpStream;

    /// Accept a new connection
    fn accept<'a>(
        &'a mut self,
    ) -> impl core::future::Future<Output = crate::Result<(Self::Stream, SocketAddr)>> + 'a;
}
