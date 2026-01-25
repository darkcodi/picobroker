//! # PicoBroker Tokio
//!
//! Tokio runtime support for PicoBroker.
//!
//! This crate provides async networking and time implementations for the
//! standard library using Tokio. It re-exports all types from `picobroker-core`
//! for convenience.

// Re-export core for convenience
pub use picobroker_core::*;

use std::sync::Arc;
use log::{debug, error, info, warn};
use tokio::net::TcpStream as TokioTcpStreamInner;

const DEFAULT_MAX_TOPIC_NAME_LENGTH: usize = 32;
const DEFAULT_MAX_PAYLOAD_SIZE: usize = 128;
const DEFAULT_QUEUE_SIZE: usize = 8;
const DEFAULT_MAX_CLIENTS: usize = 4;
const DEFAULT_MAX_TOPICS: usize = 16;
const DEFAULT_MAX_SUBSCRIBERS_PER_TOPIC: usize = 4;

pub type DefaultTokioPicoBrokerServer = TokioPicoBrokerServer<
    DEFAULT_MAX_TOPIC_NAME_LENGTH,
    DEFAULT_MAX_PAYLOAD_SIZE,
    DEFAULT_QUEUE_SIZE,
    DEFAULT_MAX_CLIENTS,
    DEFAULT_MAX_TOPICS,
    DEFAULT_MAX_SUBSCRIBERS_PER_TOPIC,
    >;

pub type TokioPicoBrokerServer<
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const QUEUE_SIZE: usize,
    const MAX_CLIENTS: usize,
    const MAX_TOPICS: usize,
    const MAX_SUBSCRIBERS_PER_TOPIC: usize,
    > = PicoBrokerServer<
    StdTimeSource,
    TokioTcpListener,
    TokioTaskSpawner,
    StdLogger,
    TokioDelay,
    MAX_TOPIC_NAME_LENGTH,
    MAX_PAYLOAD_SIZE,
    QUEUE_SIZE,
    MAX_CLIENTS,
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
    fn read<'a>(&'a mut self, buf: &'a mut [u8]) -> impl core::future::Future<Output = Result<usize, NetworkError>> + Send + 'a {
        async move {
            use tokio::io::AsyncReadExt;
            self.inner.read(buf).await.map_err(|_| NetworkError::IoError)
        }
    }

    fn write<'a>(&'a mut self, buf: &'a [u8]) -> impl core::future::Future<Output = Result<usize, NetworkError>> + Send + 'a {
        async move {
            use tokio::io::AsyncWriteExt;
            self.inner.write(buf).await.map_err(|_| NetworkError::IoError)
        }
    }

    fn close<'a>(&'a mut self) -> impl core::future::Future<Output = Result<(), NetworkError>> + Send + 'a {
        async move {
            use tokio::io::AsyncWriteExt;
            self.inner.shutdown().await.map_err(|_| NetworkError::IoError)
        }
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
    pub async fn bind(addr: &str) -> Result<Self, NetworkError> {
        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| {
                NetworkError::BindError
            })?;

        Ok(TokioTcpListener { inner: listener })
    }
}

impl TcpListener for TokioTcpListener {
    type Stream = TokioTcpStream;

    fn try_accept<'a>(&'a mut self) -> impl core::future::Future<Output = Result<(Self::Stream, SocketAddr), NetworkError>> + Send + 'a {
        async move {
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
                Ok(Err(_)) => Err(NetworkError::IoError),
                Err(_) => Err(NetworkError::NoPendingConnection), // Timeout - no connection pending
            }
        }
    }
}

/// Standard library time source
#[derive(Debug, Clone, Copy)]
pub struct StdTimeSource;

impl TimeSource for StdTimeSource {
    fn now_secs(&self) -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }
}

/// Tokio-based task spawner
///
/// This implementation uses Tokio's runtime to spawn tasks.
#[derive(Debug, Clone)]
pub struct TokioTaskSpawner {
    _runtime: Arc<tokio::runtime::Handle>,
}

impl TokioTaskSpawner {
    /// Create a new TokioTaskSpawner from the current Tokio runtime handle
    pub fn from_current_handle() -> Self {
        Self {
            _runtime: Arc::new(tokio::runtime::Handle::current()),
        }
    }

    /// Create a new TokioTaskSpawner from a specific Tokio runtime handle
    pub fn from_handle(handle: tokio::runtime::Handle) -> Self {
        Self {
            _runtime: Arc::new(handle),
        }
    }
}

impl TaskSpawner for TokioTaskSpawner {
    fn spawn<F, O>(&self, future: F) -> Result<(), TaskSpawnError>
    where
        F: core::future::Future<Output = O> + Send + 'static,
        O: Send + 'static,
    {
        self._runtime.spawn(future);
        Ok(())
    }
}

impl Default for TokioTaskSpawner {
    fn default() -> Self {
        Self::from_current_handle()
    }
}

#[derive(Clone)]
pub struct StdLogger;

impl Logger for StdLogger {
    fn debug(&self, message: &str) {
        debug!("{}", message);
    }

    fn info(&self, message: &str) {
        info!("{}", message);
    }

    fn warn(&self, message: &str) {
        warn!("{}", message);
    }

    fn error(&self, message: &str) {
        error!("{}", message);
    }
}

/// Tokio-based delay implementation
#[derive(Debug, Clone, Copy)]
pub struct TokioDelay;

impl Delay for TokioDelay {
    fn sleep_ms<'a>(&'a self, millis: u64) -> impl core::future::Future<Output = ()> + Send + 'a {
        async move {
            use tokio::time::{sleep, Duration};
            sleep(Duration::from_millis(millis)).await;
        }
    }
}
