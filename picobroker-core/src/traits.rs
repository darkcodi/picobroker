#[allow(async_fn_in_trait)]
pub trait TcpStream {
    async fn try_read(&mut self, buf: &mut [u8]) -> Result<usize, NetworkError>;

    async fn write(&mut self, buf: &[u8]) -> Result<usize, NetworkError>;

    async fn flush(&mut self) -> Result<(), NetworkError>;

    async fn close(&mut self) -> Result<(), NetworkError>;
}

#[allow(async_fn_in_trait)]
pub trait TcpListener {
    type Stream: TcpStream;

    async fn try_accept(&mut self) -> Result<(Self::Stream, SocketAddr), NetworkError>;
}

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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum NetworkError {
    ConnectionClosed,

    ReadFailed,

    ReadWouldBlock,

    ReadTimedOut,

    ReadInterrupted,

    WriteFailed,

    WriteWouldBlock,

    WriteTimedOut,

    WriteInterrupted,

    FlushFailed,

    AcceptFailed,

    AcceptWouldBlock,

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

pub trait TimeSource {
    fn now_nano_secs(&self) -> u128;
}

#[allow(async_fn_in_trait)]
pub trait Delay {
    async fn sleep_ms(&self, millis: u64);
}
