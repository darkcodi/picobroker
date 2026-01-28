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
    /// of session messages between connection attempts.
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

    /// Read operation failed
    ReadFailed,
    /// Read operation would block
    ReadWouldBlock,
    /// Read operation timed out
    ReadTimedOut,
    /// Read operation interrupted
    ReadInterrupted,

    /// Write operation failed
    WriteFailed,
    /// Write operation would block
    WriteWouldBlock,
    /// Write operation timed out
    WriteTimedOut,
    /// Write operation interrupted
    WriteInterrupted,

    /// Flush operation failed
    FlushFailed,

    /// Accept connection failed
    AcceptFailed,
    /// No pending connection to accept
    AcceptWouldBlock,

    /// Close operation failed
    CloseFailed,
}

impl core::fmt::Display for NetworkError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            NetworkError::ConnectionClosed => write!(f, "Connection closed"),
            NetworkError::ReadFailed => write!(f, "Read operation failed"),
            NetworkError::ReadWouldBlock => write!(f, "Read operation would block"),
            NetworkError::ReadTimedOut => write!(f, "Read operation timed out"),
            NetworkError::ReadInterrupted => write!(f, "Read operation interrupted"),
            NetworkError::WriteFailed => write!(f, "Write operation failed"),
            NetworkError::WriteWouldBlock => write!(f, "Write operation would block"),
            NetworkError::WriteTimedOut => write!(f, "Write operation timed out"),
            NetworkError::WriteInterrupted => write!(f, "Write operation interrupted"),
            NetworkError::FlushFailed => write!(f, "Flush operation failed"),
            NetworkError::AcceptFailed => write!(f, "Accept connection failed"),
            NetworkError::AcceptWouldBlock => write!(f, "No pending connection to accept"),
            NetworkError::CloseFailed => write!(f, "Close operation failed"),
        }
    }
}

impl core::error::Error for NetworkError {}

/// Time source trait
///
/// Abstracts time operations for both std and embedded platforms
pub trait TimeSource {
    /// Get current time in nanoseconds since Unix epoch
    fn now_nano_secs(&self) -> u128;
}

/// Delay trait for abstracting sleep/delay functionality
#[allow(async_fn_in_trait)]
pub trait Delay {
    /// Async sleep for the specified duration in milliseconds
    async fn sleep_ms(&self, millis: u64);
}
