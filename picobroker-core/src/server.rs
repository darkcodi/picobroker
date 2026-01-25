use crate::{BrokerError, ClientId, Delay, Logger, NetworkError, Packet, PacketEncoder, PicoBroker, SocketAddr, TcpListener, TcpStream, TimeSource};
use crate::format_heapless;
use crate::protocol::HeaplessVec;

/// MQTT Broker Server
///
/// Full server implementation with accept loop, client task spawning,
/// and message processing. This is the main server structure that
/// runs the MQTT broker.
///
/// # Generic Parameters
///
/// - `TS`: Time source for tracking keep-alives
/// - `TL`: TCP listener implementation
/// - `L`: Logger implementation
/// - `D`: Delay implementation
/// - `MAX_TOPIC_NAME_LENGTH`: Maximum length of topic names
/// - `MAX_PAYLOAD_SIZE`: Maximum payload size for packets
/// - `QUEUE_SIZE`: Queue size for client -> broker messages and broker -> client messages
/// - `MAX_CLIENTS`: Maximum number of concurrent clients
/// - `MAX_TOPICS`: Maximum number of distinct topics
/// - `MAX_SUBSCRIBERS_PER_TOPIC`: Maximum subscribers per topic
pub struct PicoBrokerServer<
    TS: TimeSource,
    TL: TcpListener,
    L: Logger,
    D: Delay,
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const QUEUE_SIZE: usize,
    const MAX_CLIENTS: usize,
    const MAX_TOPICS: usize,
    const MAX_SUBSCRIBERS_PER_TOPIC: usize,
> {
    time_source: TS,
    listener: TL,
    logger: L,
    delay: D,
    broker: PicoBroker<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_SIZE, MAX_CLIENTS, MAX_TOPICS, MAX_SUBSCRIBERS_PER_TOPIC>,
    client_registry: ClientRegistry<
        TL::Stream,
        MAX_TOPIC_NAME_LENGTH,
        MAX_PAYLOAD_SIZE,
        QUEUE_SIZE,
        MAX_CLIENTS,
    >,
}

impl<
    TS: TimeSource,
    TL: TcpListener,
    L: Logger,
    D: Delay,
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const QUEUE_SIZE: usize,
    const MAX_CLIENTS: usize,
    const MAX_TOPICS: usize,
    const MAX_SUBSCRIBERS_PER_TOPIC: usize,
> PicoBrokerServer<TS, TL, L, D, MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_SIZE, MAX_CLIENTS, MAX_TOPICS, MAX_SUBSCRIBERS_PER_TOPIC>
{
    /// Create a new MQTT broker server
    pub fn new(time_source: TS, listener: TL, logger: L, delay: D) -> Self {
        Self {
            time_source,
            listener,
            logger,
            delay,
            broker: PicoBroker::new(),
            client_registry: ClientRegistry::new(),
        }
    }

    /// Run the server main loop
    ///
    /// This method:
    /// 1. Accepts new connections (non-blocking)
    /// 2. Spawns a client handler task for each new connection
    /// 3. Processes client messages (round-robin)
    /// 4. Checks for expired clients
    ///
    /// The server loop runs indefinitely, handling all client connections
    /// and message routing.
    pub async fn run(&mut self) -> Result<(), BrokerError>
    where
        TL::Stream: Send + 'static,
        L: Clone + Send + 'static,
    {
        self.logger.info("Starting server main loop");
        loop {
            // 1. Try to accept a connection (non-blocking)
            // Note: This returns immediately if no connection is pending
            if let Ok((stream, socket_addr)) = self.listener.try_accept().await {
                self.logger.info(format_heapless!(128; "Received new connection from {}", socket_addr).as_str());

                const KEEP_ALIVE_SECS: u16 = 60; // Default non-configurable keep-alive for new clients
                let current_time = self.time_source.now_secs();
                let maybe_session_id = self.client_registry.register_new_client(stream, socket_addr, KEEP_ALIVE_SECS, current_time);
                match maybe_session_id {
                    Ok(session_id) => {
                        self.logger.info(format_heapless!(128; "Registered new client with session ID {}", session_id).as_str());

                    },
                    Err(e) => {
                        self.logger.error(format_heapless!(128; "Failed to register new client: {}", e).as_str());
                        self.logger.error("Closing connection");
                    }
                };
            }

            // 2. Process client messages (round-robin through all sessions)
            self.process_client_messages().await?;

            // // 3. Check for expired clients (keep-alive timeout)
            // self.process_expired_clients();

            // 4. Small yield to prevent busy-waiting
            self.delay.sleep_ms(10).await;
        }
    }

    async fn process_client_messages(&mut self) -> Result<(), BrokerError> {
        for session_option in self.client_registry.sessions.iter_mut() {
            if let Some(session) = session_option {
                match Self::try_read_packet_from_stream(&self.logger, &mut session.stream).await {
                    Ok(Some(packet)) => {
                        self.logger.info(format_heapless!(200; "Received packet from client {}: {:?}", session.session_id, packet).as_str());
                        // Here you would process the packet (e.g., handle CONNECT, PUBLISH, SUBSCRIBE, etc.)
                        match packet {
                            Packet::Connect(connect_packet) => {
                                self.logger.info(format_heapless!(128; "Client {} sent CONNECT with client ID: {}", session.session_id, connect_packet.client_id).as_str());
                                session.client_id = Some(ClientId::try_from(connect_packet.client_id.as_str()).unwrap_or_default());
                                session.state = ClientState::Connected;
                                // Send CONNACK response (not implemented here)
                            }
                            _ => {
                                self.logger.info(format_heapless!(128; "Processing other packet types not implemented yet").as_str());
                            },
                        }
                    }
                    Ok(None) => {
                        // No packet available, continue
                    }
                    Err(e) => {
                        // self.logger.error(format_heapless!(128; "Error reading packet from client {}: {}", session.session_id, e).as_str());
                        // Handle error (e.g., close connection, cleanup)
                    }
                }
            }
        }
        Ok(())
    }

    async fn try_read_packet_from_stream<S: TcpStream>(logger: &L, stream: &mut S) -> Result<Option<Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>>, BrokerError> {
        let mut header_buffer = [0u8; MAX_PAYLOAD_SIZE];

        match stream.try_read(&mut header_buffer).await {
            Ok(bytes_read) => {
                if bytes_read > 0 {
                    let slice = &header_buffer[..bytes_read];
                    let maybe_packet = Packet::decode(slice);
                    match maybe_packet {
                        Ok(packet) => Ok(Some(packet)),
                        Err(e) => {
                            logger.error(format_heapless!(128; "Failed to decode packet: {}", e).as_str());
                            Err(BrokerError::PacketEncodingError { error: e })
                        }
                    }
                } else {
                    logger.info("Connection closed by peer");
                    Err(BrokerError::NetworkError { error: NetworkError::ConnectionClosed })
                }
            }
            Err(e) => {
                if matches!(e, NetworkError::WouldBlock) {
                    // No data available right now
                    return Ok(None);
                }
                // logger.error(format_heapless!(128; "Failed to read from stream: {}", e).as_str());
                Err(BrokerError::NetworkError { error: e })
            }
        }
    }
}

/// Client state machine
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ClientState {
    #[default]
    Connecting,
    Connected,
    Disconnected,
}

/// Client session with dual queues
///
/// Maintains the communication channels between a client task
/// and the broker, along with connection state.
#[derive(Debug, PartialEq, Eq)]
pub struct ClientSession<
    S: TcpStream,
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const QUEUE_SIZE: usize,
> {
    /// Underlying TCP stream
    pub stream: S,

    /// Unique session identifier
    pub session_id: usize,

    /// Client socket address
    pub socket_addr: SocketAddr,

    /// Client identifier
    pub client_id: Option<ClientId>,

    /// Current client state
    pub state: ClientState,

    /// Keep-alive interval in seconds
    pub keep_alive_secs: u16,

    /// Timestamp of last activity (seconds since epoch)
    pub last_activity: u64,
}

impl<S: TcpStream, const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize, const QUEUE_SIZE: usize> ClientSession<S, MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_SIZE>
{
    /// Create a new client session
    pub fn new(stream: S, socket_addr: SocketAddr, keep_alive_secs: u16, current_time: u64) -> Self {
        let session_id = socket_addr.port as usize; // Simple session ID based on port for demo purposes
        Self {
            stream,
            session_id,
            socket_addr,
            client_id: None,
            state: ClientState::Connecting,
            keep_alive_secs,
            last_activity: current_time,
        }
    }

    /// Check if client's keep-alive has expired
    ///
    /// Returns true if the time since last activity exceeds 1.5x the keep-alive value
    pub fn is_expired(&self, current_time: u64) -> bool {
        let timeout_secs = (self.keep_alive_secs as u64) * 3 / 2;
        let elapsed = current_time.saturating_sub(self.last_activity);
        elapsed > timeout_secs
    }

    /// Update the last activity timestamp
    pub fn update_activity(&mut self, current_time: u64) {
        self.last_activity = current_time;
    }
}

/// Client registry
///
/// Manages connected clients and their state
#[derive(Debug, PartialEq, Eq)]
pub struct ClientRegistry<
    S: TcpStream,
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const QUEUE_SIZE: usize,
    const MAX_CLIENTS: usize,
> {
    /// Sessions with communication queues
    sessions: HeaplessVec<
        Option<ClientSession<S, MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_SIZE>>,
        MAX_CLIENTS
    >,
}

impl<S: TcpStream, const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize, const QUEUE_SIZE: usize, const MAX_CLIENTS: usize>
ClientRegistry<S, MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_SIZE, MAX_CLIENTS>
{
    /// Create a new client registry
    pub fn new() -> Self {
        Self {
            sessions: HeaplessVec::new(),
        }
    }

    /// Register a new client session
    pub fn register_new_client(&mut self, stream: S, socket_addr: SocketAddr, keep_alive_secs: u16, current_time: u64) -> Result<usize, BrokerError> {
        if self.sessions.len() >= MAX_CLIENTS {
            return Err(BrokerError::MaxClientsReached { max_clients: MAX_CLIENTS });
        }
        let session = ClientSession::new(stream, socket_addr, keep_alive_secs, current_time);
        let session_id = session.session_id;
        self.sessions.push(Some(session)).map_err(|_| BrokerError::MaxClientsReached { max_clients: MAX_CLIENTS })?;
        Ok(session_id)
    }
}