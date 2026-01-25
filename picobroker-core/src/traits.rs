/// TCP stream trait for Tokio
#[allow(async_fn_in_trait)]
pub trait TcpStream {
    /// Read data from the stream into the buffer
    async fn read(&mut self, buf: &mut [u8]) -> Result<usize, NetworkError>;

    /// Write data from the buffer to the stream
    async fn write(&mut self, buf: &[u8]) -> Result<usize, NetworkError>;

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
        write!(f, "{}.{}.{}.{}:{}", self.ip[0], self.ip[1], self.ip[2], self.ip[3], self.port)
    }
}

/// Network error enumeration
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkError {
    /// Failed to bind to the specified address
    BindError,
    /// No pending connection to accept
    NoPendingConnection,
    /// I/O error occurred
    IoError,
}

impl core::fmt::Display for NetworkError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            NetworkError::BindError => write!(f, "Failed to bind to the specified address"),
            NetworkError::NoPendingConnection => write!(f, "No pending connection to accept"),
            NetworkError::IoError => write!(f, "I/O error occurred"),
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

/// Trait for spawning async tasks across different runtimes
///
/// This trait abstracts task spawning, allowing picobroker-core to spawn
/// concurrent client handler tasks without knowing the specific async runtime.
pub trait TaskSpawner {
    /// Spawn a new task that runs the given future to completion
    fn spawn<F, O>(&self, future: F) -> Result<(), TaskSpawnError>
    where
        F: core::future::Future<Output = O> + Send + 'static,
        O: Send + 'static;
}

/// Error that can occur when spawning a task
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskSpawnError {
    /// No memory available to spawn task
    NoMemory,
    /// Task spawner is busy
    Busy,
}

impl core::fmt::Display for TaskSpawnError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            TaskSpawnError::NoMemory => write!(f, "No memory available to spawn task"),
            TaskSpawnError::Busy => write!(f, "Task spawner is busy"),
        }
    }
}

impl core::error::Error for TaskSpawnError {}

/// Logging trait for abstracting logging functionality
pub trait Logger: Clone {
    /// Log a debug message
    fn debug(&self, message: &str);

    /// Log an informational message
    fn info(&self, message: &str);

    /// Log a warning message
    fn warn(&self, message: &str);

    /// Log an error message
    fn error(&self, message: &str);
}

/// Delay trait for abstracting sleep/delay functionality
#[allow(async_fn_in_trait)]
pub trait Delay {
    /// Async sleep for the specified duration in milliseconds
    async fn sleep_ms(&self, millis: u64);
}
