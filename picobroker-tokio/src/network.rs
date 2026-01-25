//! Tokio networking implementation

use picobroker_core::{BrokerError, SocketAddr};
use picobroker_core::{TcpListener, TcpStream};
use tokio::net::TcpStream as TokioTcpStreamInner;
use log::{info, trace, warn};

/// Tokio TCP stream wrapper
///
/// Wraps tokio's TCP stream for use with the TcpStream trait
pub struct TokioTcpStream {
    inner: TokioTcpStreamInner,
}

impl TokioTcpStream {
    /// Create a new TokioTcpStream from a tokio TcpStream
    pub fn from_tcp_stream(stream: TokioTcpStreamInner) -> Self {
        TokioTcpStream { inner: stream }
    }

    /// Get the inner tokio TcpStream
    pub fn inner(&self) -> &TokioTcpStreamInner {
        &self.inner
    }

    /// Get mutable reference to the inner tokio TcpStream
    pub fn inner_mut(&mut self) -> &mut TokioTcpStreamInner {
        &mut self.inner
    }
}

impl TcpStream for TokioTcpStream {
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, BrokerError> {
        use tokio::io::AsyncReadExt;
        self.inner.read(buf).await.map_err(|e| {
            trace!("TCP read error: {}", e);
            BrokerError::IoError
        })
    }

    async fn write(&mut self, buf: &[u8]) -> Result<usize, BrokerError> {
        use tokio::io::AsyncWriteExt;
        self.inner
            .write(buf)
            .await
            .map_err(|e| {
                trace!("TCP write error: {}", e);
                BrokerError::IoError
            })
    }

    async fn close(&mut self) -> Result<(), BrokerError> {
        use tokio::io::AsyncWriteExt;
        self.inner
            .shutdown()
            .await
            .map_err(|e| {
                info!("TCP shutdown error: {}", e);
                BrokerError::IoError
            })
    }
}

/// Tokio TCP listener wrapper
///
/// Wraps tokio's TCP listener for use with the TcpListener trait
pub struct TokioTcpListener {
    inner: tokio::net::TcpListener,
}

impl TokioTcpListener {
    /// Bind to the specified address
    pub async fn bind(addr: &str) -> Result<Self, BrokerError> {
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| {
                warn!("Failed to bind to {}: {}", addr, e);
                BrokerError::BindError
            })?;

        info!("Successfully bound to {}", addr);
        Ok(TokioTcpListener { inner: listener })
    }
}

impl TcpListener for TokioTcpListener {
    type Stream = TokioTcpStream;

    async fn accept(&mut self) -> Result<(Self::Stream, SocketAddr), BrokerError> {
        let (stream, addr) = self
            .inner
            .accept()
            .await
            .map_err(|_| BrokerError::AcceptConnectionError)?;

        let socket_addr = SocketAddr {
            ip: match addr.ip() {
                std::net::IpAddr::V4(ipv4) => ipv4.octets(),
                std::net::IpAddr::V6(_) => [0, 0, 0, 0], // Ignore IPv6 for embedded
            },
            port: addr.port(),
        };

        let tokio_stream = TokioTcpStream::from_tcp_stream(stream);

        Ok((tokio_stream, socket_addr))
    }

    async fn try_accept(&mut self) -> Result<(Self::Stream, SocketAddr), BrokerError> {
        use tokio::time::{Duration, timeout};

        // Try to accept with zero timeout - returns immediately if no connection is pending
        match timeout(Duration::ZERO, self.inner.accept()).await {
            Ok(Ok((stream, addr))) => {
                let socket_addr = SocketAddr {
                    ip: match addr.ip() {
                        std::net::IpAddr::V4(ipv4) => ipv4.octets(),
                        std::net::IpAddr::V6(_) => [0, 0, 0, 0], // Ignore IPv6 for embedded
                    },
                    port: addr.port(),
                };

                let tokio_stream = TokioTcpStream::from_tcp_stream(stream);
                Ok((tokio_stream, socket_addr))
            }
            Ok(Err(_)) => Err(BrokerError::AcceptConnectionError),
            Err(_) => Err(BrokerError::AcceptConnectionError), // Timeout - no connection pending
        }
    }
}
