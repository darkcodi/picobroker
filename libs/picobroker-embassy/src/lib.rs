#![no_std]

pub use picobroker_core::*;

use picobroker_core::server::PicoBrokerServer;
use picobroker_core::traits::*;

use embassy_net::tcp::TcpSocket;
use embassy_net::Stack;
use embassy_futures::select::{select, Either};
use embassy_time::{Duration, Timer};

// Constants
const DEFAULT_MAX_TOPIC_NAME_LENGTH: usize = 32;
const DEFAULT_MAX_PAYLOAD_SIZE: usize = 128;
const DEFAULT_QUEUE_SIZE: usize = 8;
const DEFAULT_MAX_SESSIONS: usize = 4;
const DEFAULT_MAX_TOPICS: usize = 16;
const DEFAULT_MAX_SUBSCRIBERS_PER_TOPIC: usize = 4;
const DEFAULT_BUFFER_SIZE: usize = 2048;

pub struct EmbassyTcpStream<'a> {
    inner: TcpSocket<'a>,
}

impl<'a> EmbassyTcpStream<'a> {
    pub fn from_tcp_socket(socket: TcpSocket<'a>) -> Self {
        Self { inner: socket }
    }

    pub fn inner(&self) -> &TcpSocket<'a> {
        &self.inner
    }

    pub fn inner_mut(&mut self) -> &mut TcpSocket<'a> {
        &mut self.inner
    }
}

impl<'a> TcpStream for EmbassyTcpStream<'a> {
    async fn try_read(&mut self, buf: &mut [u8]) -> Result<usize, NetworkError> {
        // Embassy read() blocks, must check can_recv() first for non-blocking behavior
        if !self.inner.can_recv() {
            return Err(NetworkError::ReadWouldBlock);
        }

        self.inner.read(buf).await
            .map_err(|_| NetworkError::ReadFailed)
    }

    async fn write(&mut self, buf: &[u8]) -> Result<usize, NetworkError> {
        self.inner.write(buf).await
            .map_err(|_| NetworkError::WriteFailed)
    }

    async fn flush(&mut self) -> Result<(), NetworkError> {
        // Embassy auto-flushes, no-op
        Ok(())
    }

    async fn close(&mut self) -> Result<(), NetworkError> {
        self.inner.close();
        Ok(())
    }
}

pub struct EmbassyTcpListener<'a> {
    stack: Stack<'a>,
    current_listening_socket: Option<TcpSocket<'a>>,
    rx_buffer: [u8; DEFAULT_BUFFER_SIZE],
    tx_buffer: [u8; DEFAULT_BUFFER_SIZE],
    port: u16,
}

impl<'a> EmbassyTcpListener<'a> {
    pub fn new(stack: Stack<'a>) -> Self {
        Self {
            stack,
            current_listening_socket: None,
            rx_buffer: [0; DEFAULT_BUFFER_SIZE],
            tx_buffer: [0; DEFAULT_BUFFER_SIZE],
            port: 0,
        }
    }

    pub async fn bind(&mut self, port: u16) -> Result<(), NetworkError> {
        self.port = port;

        // Create first listening socket
        // SAFETY: We extend the lifetime of the buffer borrow to match the socket lifetime 'a.
        // This is safe because the listener owns the buffers and keeps them alive for 'a.
        let rx_slice: &mut [u8] = &mut self.rx_buffer[..];
        let tx_slice: &mut [u8] = &mut self.tx_buffer[..];
        let rx_buffer: &'a mut [u8] = unsafe { core::mem::transmute(rx_slice) };
        let tx_buffer: &'a mut [u8] = unsafe { core::mem::transmute(tx_slice) };

        let mut socket = TcpSocket::new(
            self.stack.clone(),
            rx_buffer,
            tx_buffer,
        );

        socket.accept(port).await
            .map_err(|_| NetworkError::AcceptFailed)?;

        self.current_listening_socket = Some(socket);
        Ok(())
    }

    async fn recreate_listening_socket(&mut self) -> Result<(), NetworkError> {
        // SAFETY: We extend the lifetime of the buffer borrow to match the socket lifetime 'a.
        // This is safe because the listener owns the buffers and keeps them alive for 'a.
        let rx_slice: &mut [u8] = &mut self.rx_buffer[..];
        let tx_slice: &mut [u8] = &mut self.tx_buffer[..];
        let rx_buffer: &'a mut [u8] = unsafe { core::mem::transmute(rx_slice) };
        let tx_buffer: &'a mut [u8] = unsafe { core::mem::transmute(tx_slice) };

        let mut socket = TcpSocket::new(
            self.stack.clone(),
            rx_buffer,
            tx_buffer,
        );

        socket.accept(self.port).await
            .map_err(|_| NetworkError::AcceptFailed)?;

        self.current_listening_socket = Some(socket);
        Ok(())
    }
}

impl<'a> TcpListener for EmbassyTcpListener<'a> {
    type Stream = EmbassyTcpStream<'a>;

    async fn try_accept(&mut self) -> Result<(Self::Stream, SocketAddr), NetworkError> {
        // Ensure we have a listening socket
        if self.current_listening_socket.is_none() {
            self.recreate_listening_socket().await?;
        }

        let socket = self.current_listening_socket.as_mut().unwrap();

        // Race accept() against zero-duration timer for non-blocking behavior
        match select(socket.accept(self.port), Timer::after(Duration::from_secs(0))).await {
            Either::First(Ok(())) => {
                // Connection accepted! Socket is now ESTABLISHED
                let stream_socket = self.current_listening_socket.take().unwrap();

                // Get peer address before wrapping
                let endpoint = stream_socket.remote_endpoint()
                    .ok_or(NetworkError::AcceptFailed)?;

                let socket_addr = SocketAddr {
                    ip: match endpoint.addr {
                        embassy_net::IpAddress::Ipv4(ipv4) => ipv4.octets(),
                    },
                    port: endpoint.port,
                };

                let stream = EmbassyTcpStream::from_tcp_socket(stream_socket);

                // Don't recreate listening socket yet - will be recreated on next try_accept() call
                Ok((stream, socket_addr))
            }
            Either::First(Err(_)) => Err(NetworkError::AcceptFailed),
            Either::Second(_) => {
                // Timer expired - no pending connection
                // Socket remains in LISTEN state, ready for next call
                Err(NetworkError::AcceptWouldBlock)
            }
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub struct EmbassyTimeSource;

impl TimeSource for EmbassyTimeSource {
    fn now_nano_secs(&self) -> u128 {
        embassy_time::Instant::now().as_micros() as u128 * 1000
    }
}

#[derive(Debug, Clone, Copy)]
pub struct EmbassyDelay;

impl Delay for EmbassyDelay {
    async fn sleep_ms(&self, millis: u64) {
        embassy_time::Timer::after(embassy_time::Duration::from_millis(millis)).await;
    }
}

pub type DefaultEmbassyPicoBrokerServer<'a> = EmbassyPicoBrokerServer<
    'a,
    DEFAULT_MAX_TOPIC_NAME_LENGTH,
    DEFAULT_MAX_PAYLOAD_SIZE,
    DEFAULT_QUEUE_SIZE,
    DEFAULT_MAX_SESSIONS,
    DEFAULT_MAX_TOPICS,
    DEFAULT_MAX_SUBSCRIBERS_PER_TOPIC,
>;

pub type EmbassyPicoBrokerServer<
    'a,
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const QUEUE_SIZE: usize,
    const MAX_SESSIONS: usize,
    const MAX_TOPICS: usize,
    const MAX_SUBSCRIBERS_PER_TOPIC: usize,
> = PicoBrokerServer<
    EmbassyTimeSource,
    EmbassyTcpListener<'a>,
    EmbassyDelay,
    MAX_TOPIC_NAME_LENGTH,
    MAX_PAYLOAD_SIZE,
    QUEUE_SIZE,
    MAX_SESSIONS,
    MAX_TOPICS,
    MAX_SUBSCRIBERS_PER_TOPIC,
>;
