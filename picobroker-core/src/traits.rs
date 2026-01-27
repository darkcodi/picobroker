/// TCP stream trait for Tokio
#[allow(async_fn_in_trait)]
pub trait TcpStream {
    /// Read data from the stream into the buffer
    async fn try_read(&mut self, buf: &mut [u8]) -> Result<usize, NetworkError>;

    /// Write data from the buffer to the stream
    async fn write(&mut self, buf: &[u8]) -> Result<usize, NetworkError>;

    /// Flush the stream to ensure all buffered data is transmitted
    async fn flush(&mut self) -> Result<(), NetworkError>;

    /// Close the stream
    async fn close(&mut self) -> Result<(), NetworkError>;
}

/// TCP listener trait for Tokio
#[allow(async_fn_in_trait)]
pub trait TcpListener {
    /// The stream type produced by this listener
    type Stream: TcpStream;

    /// Try to accept a new connection without blocking
    ///
    /// Returns immediately with an error if no connection is pending.
    /// This is used in the server main loop to allow periodic processing
    /// of client messages between connection attempts.
    async fn try_accept(&mut self) -> Result<(Self::Stream, SocketAddr), NetworkError>;
}

/// Socket address (IPv4)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SocketAddr {
    pub ip: [u8; 4],
    pub port: u16,
}

impl core::fmt::Display for SocketAddr {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(
            f,
            "{}.{}.{}.{}:{}",
            self.ip[0], self.ip[1], self.ip[2], self.ip[3], self.port
        )
    }
}

/// Network error enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkError {
    /// Connection closed
    ConnectionClosed,
    /// I/O error occurred
    IoError,
    /// No pending connection to accept
    NoPendingConnection,
    /// Operation would block
    WouldBlock,
    /// Operation timed out
    TimedOut,
    /// Operation interrupted
    Interrupted,
}

impl core::fmt::Display for NetworkError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            NetworkError::ConnectionClosed => write!(f, "Connection closed"),
            NetworkError::IoError => write!(f, "I/O error occurred"),
            NetworkError::NoPendingConnection => write!(f, "No pending connection to accept"),
            NetworkError::WouldBlock => write!(f, "Operation would block"),
            NetworkError::TimedOut => write!(f, "Operation timed out"),
            NetworkError::Interrupted => write!(f, "Operation interrupted"),
        }
    }
}

impl core::error::Error for NetworkError {}

/// Time source trait
///
/// Abstracts time operations for both std and embedded platforms
pub trait TimeSource {
    /// Get current time in seconds since Unix epoch
    fn now_secs(&self) -> u64;
}

/// Delay trait for abstracting sleep/delay functionality
#[allow(async_fn_in_trait)]
pub trait Delay {
    /// Async sleep for the specified duration in milliseconds
    async fn sleep_ms(&self, millis: u64);
}
