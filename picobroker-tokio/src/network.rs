//! Tokio networking implementation

use picobroker_core::{TcpListener, TcpStream};
use picobroker_core::{BrokerError, SocketAddr};
use tokio::net::TcpStream as TokioTcpStreamInner;

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
    fn read<'a, 'b>(
        &'a mut self,
        buf: &'b mut [u8],
    ) -> impl core::future::Future<Output = Result<usize, BrokerError>> + 'a
    where
        'b: 'a,
    {
        use tokio::io::AsyncReadExt;
        async move { self.inner.read(buf).await.map_err(|_| BrokerError::IoError) }
    }

    fn write<'a, 'b>(
        &'a mut self,
        buf: &'b [u8],
    ) -> impl core::future::Future<Output = Result<usize, BrokerError>> + 'a
    where
        'b: 'a,
    {
        use tokio::io::AsyncWriteExt;
        async move { self.inner.write(buf).await.map_err(|_| BrokerError::IoError) }
    }

    fn close<'a>(
        &'a mut self,
    ) -> impl core::future::Future<Output = Result<(), BrokerError>> + 'a {
        use tokio::io::AsyncWriteExt;
        async move { self.inner.shutdown().await.map_err(|_| BrokerError::IoError) }
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
            .map_err(|_| BrokerError::BindError)?;

        Ok(TokioTcpListener { inner: listener })
    }
}

impl TcpListener for TokioTcpListener {
    type Stream = TokioTcpStream;

    fn accept<'a>(
        &'a mut self,
    ) -> impl core::future::Future<Output = Result<(Self::Stream, SocketAddr), BrokerError>> + 'a {
        async move {
            let (stream, addr) = self.inner.accept().await.map_err(|_| BrokerError::AcceptConnectionError)?;

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
    }
}
