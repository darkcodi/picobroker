//! # PicoBroker Tokio
//!
//! Tokio runtime support for PicoBroker.
//!
//! This crate provides async networking and time implementations for the
//! standard library using Tokio. It re-exports all types from `picobroker-core`
//! for convenience.

// Re-export core for convenience
pub use picobroker_core::*;

use picobroker_core::server::PicoBrokerServer;
use picobroker_core::traits::{
    Delay, NetworkError, SocketAddr, TcpListener, TcpStream, TimeSource,
};
use tokio::net::TcpStream as TokioTcpStreamInner;

const DEFAULT_MAX_TOPIC_NAME_LENGTH: usize = 32;
const DEFAULT_MAX_PAYLOAD_SIZE: usize = 128;
const DEFAULT_QUEUE_SIZE: usize = 8;
const DEFAULT_MAX_SESSIONS: usize = 4;
const DEFAULT_MAX_TOPICS: usize = 16;
const DEFAULT_MAX_SUBSCRIBERS_PER_TOPIC: usize = 4;

pub type DefaultTokioPicoBrokerServer = TokioPicoBrokerServer<
    DEFAULT_MAX_TOPIC_NAME_LENGTH,
    DEFAULT_MAX_PAYLOAD_SIZE,
    DEFAULT_QUEUE_SIZE,
    DEFAULT_MAX_SESSIONS,
    DEFAULT_MAX_TOPICS,
    DEFAULT_MAX_SUBSCRIBERS_PER_TOPIC,
>;

pub type TokioPicoBrokerServer<
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const QUEUE_SIZE: usize,
    const MAX_SESSIONS: usize,
    const MAX_TOPICS: usize,
    const MAX_SUBSCRIBERS_PER_TOPIC: usize,
> = PicoBrokerServer<
    StdTimeSource,
    TokioTcpListener,
    TokioDelay,
    MAX_TOPIC_NAME_LENGTH,
    MAX_PAYLOAD_SIZE,
    QUEUE_SIZE,
    MAX_SESSIONS,
    MAX_TOPICS,
    MAX_SUBSCRIBERS_PER_TOPIC,
>;

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
    async fn try_read(&mut self, buf: &mut [u8]) -> Result<usize, NetworkError> {
        use std::io::ErrorKind;

        // tokio's try_read is synchronous but non-blocking
        self.inner.try_read(buf).map_err(|e| match e.kind() {
            ErrorKind::WouldBlock => NetworkError::ReadWouldBlock,
            ErrorKind::TimedOut => NetworkError::ReadTimedOut,
            ErrorKind::Interrupted => NetworkError::ReadInterrupted,
            _ => NetworkError::ReadFailed,
        })
    }

    async fn write(&mut self, buf: &[u8]) -> Result<usize, NetworkError> {
        use tokio::io::AsyncWriteExt;
        self.inner
            .write(buf)
            .await
            .map_err(|_| NetworkError::WriteFailed)
    }

    async fn flush(&mut self) -> Result<(), NetworkError> {
        use tokio::io::AsyncWriteExt;
        self.inner
            .flush()
            .await
            .map_err(|_| NetworkError::FlushFailed)
    }

    async fn close(&mut self) -> Result<(), NetworkError> {
        use tokio::io::AsyncWriteExt;
        self.inner
            .shutdown()
            .await
            .map_err(|_| NetworkError::CloseFailed)
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
    pub async fn bind(addr: &str) -> Result<Self, std::io::Error> {
        let listener = tokio::net::TcpListener::bind(addr).await?;
        Ok(TokioTcpListener { inner: listener })
    }
}

impl TcpListener for TokioTcpListener {
    type Stream = TokioTcpStream;

    async fn try_accept(&mut self) -> Result<(Self::Stream, SocketAddr), NetworkError> {
        use tokio::time::{timeout, Duration};

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
            Ok(Err(_)) => Err(NetworkError::AcceptFailed),
            Err(_) => Err(NetworkError::AcceptWouldBlock), // Timeout - no connection pending
        }
    }
}

/// Standard library time source
#[derive(Debug, Clone, Copy)]
pub struct StdTimeSource;

impl TimeSource for StdTimeSource {
    fn now_nano_secs(&self) -> u128 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    }
}

/// Tokio-based delay implementation
#[derive(Debug, Clone, Copy)]
pub struct TokioDelay;

impl Delay for TokioDelay {
    async fn sleep_ms(&self, millis: u64) {
        use tokio::time::{sleep, Duration};
        sleep(Duration::from_millis(millis)).await;
    }
}
