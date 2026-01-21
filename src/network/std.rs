//! std platform networking implementation
//!
//! Uses tokio for async TCP operations

use crate::error::{Error, Result};
use crate::network::{SocketAddr, TcpListener, TcpStream};
use tokio::net::TcpStream as TokioTcpStream;

/// Std TCP stream wrapper
///
/// Wraps tokio's TCP stream for use with the TcpStream trait
pub struct StdTcpStream {
    inner: TokioTcpStream,
}

impl StdTcpStream {
    /// Create a new StdTcpStream from a tokio TcpStream
    pub fn from_tcp_stream(stream: TokioTcpStream) -> Self {
        StdTcpStream { inner: stream }
    }
}

#[async_trait::async_trait]
impl TcpStream for StdTcpStream {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize> {
        use tokio::io::AsyncReadExt;
        self.inner.read(buf).await.map_err(|_| Error::IoError)
    }

    async fn write(&mut self, buf: &[u8]) -> Result<usize> {
        use tokio::io::AsyncWriteExt;
        self.inner.write(buf).await.map_err(|_| Error::IoError)
    }

    async fn close(&mut self) -> Result<()> {
        use tokio::io::AsyncWriteExt;
        self.inner.shutdown().await.map_err(|_| Error::IoError)
    }
}

/// Std TCP listener wrapper
///
/// Wraps tokio's TCP listener for use with the TcpListener trait
pub struct StdTcpListener {
    inner: tokio::net::TcpListener,
}

impl StdTcpListener {
    /// Bind to the specified address
    pub async fn bind(addr: &str) -> Result<Self> {
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|_| Error::BindError)?;

        Ok(StdTcpListener { inner: listener })
    }
}

#[async_trait::async_trait]
impl TcpListener for StdTcpListener {
    type Stream = StdTcpStream;

    async fn accept(&mut self) -> Result<(Self::Stream, SocketAddr)> {
        let (stream, addr) = self.inner.accept().await.map_err(|_| Error::AcceptError)?;

        let socket_addr = SocketAddr {
            ip: match addr.ip() {
                std::net::IpAddr::V4(ipv4) => ipv4.octets(),
                std::net::IpAddr::V6(_) => [0, 0, 0, 0], // Ignore IPv6 for embedded
            },
            port: addr.port(),
        };

        let std_stream = StdTcpStream::from_tcp_stream(stream);

        Ok((std_stream, socket_addr))
    }
}
