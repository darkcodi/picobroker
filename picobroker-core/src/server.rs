use crate::{BrokerError, Delay, Logger, Packet, PicoBroker, SocketAddr, TaskSpawner, TcpListener, TcpStream, TimeSource};
use crate::format_heapless;

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
/// - `SP`: Task spawner implementation
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
    SP: TaskSpawner,
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
    spawner: SP,
    logger: L,
    delay: D,
    broker: PicoBroker<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_SIZE, MAX_CLIENTS, MAX_TOPICS, MAX_SUBSCRIBERS_PER_TOPIC>,
}

impl<
    TS: TimeSource,
    TL: TcpListener,
    SP: TaskSpawner,
    L: Logger,
    D: Delay,
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const QUEUE_SIZE: usize,
    const MAX_CLIENTS: usize,
    const MAX_TOPICS: usize,
    const MAX_SUBSCRIBERS_PER_TOPIC: usize,
> PicoBrokerServer<TS, TL, SP, L, D, MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_SIZE, MAX_CLIENTS, MAX_TOPICS, MAX_SUBSCRIBERS_PER_TOPIC>
{
    /// Create a new MQTT broker server
    pub fn new(time_source: TS, listener: TL, spawner: SP, logger: L, delay: D) -> Self {
        Self {
            time_source,
            listener,
            spawner,
            logger,
            delay,
            broker: PicoBroker::new(),
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
            if let Ok((mut stream, socket_addr)) = self.listener.try_accept().await {
                self.logger.info(format_heapless!(128; "Received new connection from {}", socket_addr).as_str());

                const KEEP_ALIVE_SECS: u16 = 60;
                let current_time = self.time_source.now_secs();

                match self.broker.register_new_client(socket_addr, KEEP_ALIVE_SECS, current_time) {
                    Ok(session_id) => {
                        self.logger.info(format_heapless!(128; "Registered new client with session ID {}", session_id).as_str());

                        // Spawn client handler task
                        let logger = self.logger.clone();
                        match self.spawner.spawn(async move {
                            Self::client_handler_task(session_id, stream, logger).await;
                        }) {
                            Ok(_) => {
                                self.logger.info(format_heapless!(128; "Spawned client handler task for client {}", session_id).as_str());
                            }
                            Err(e) => {
                                self.logger.error(format_heapless!(128; "Failed to spawn client handler task: {}", e).as_str());
                            }
                        }
                    }
                    Err(e) => {
                        self.logger.error(format_heapless!(128; "Failed to register new client: {}", e).as_str());
                        let _ = stream.close().await;
                    }
                }
            }

            // 2. Small yield to prevent busy-waiting
            self.delay.sleep_ms(10).await;
        }
    }

    async fn client_handler_task<S>(session_id: usize, mut stream: S, logger: L)
    where
        S: TcpStream,
    {
        logger.info(format_heapless!(128; "Client handler task started for session {}", session_id).as_str());

        let mut buffer = [0u8; MAX_PAYLOAD_SIZE];
        let mut should_disconnect = false;

        loop {
            // === READ FROM NETWORK ===
            match stream.read(&mut buffer[0..1]).await {
                Ok(0) => {
                    // EOF - client disconnected gracefully
                    logger.info(format_heapless!(128; "Client {} disconnected (EOF)", session_id).as_str());
                    should_disconnect = true;
                }
                Ok(_) => {
                    // TODO: Parse and handle packet
                    logger.debug(format_heapless!(128; "Client {} received data", session_id).as_str());
                }
                Err(_) => {
                    // Network error
                    logger.info(format_heapless!(128; "Client {} network error", session_id).as_str());
                    should_disconnect = true;
                }
            }

            if should_disconnect {
                break;
            }
        }

        let _ = stream.close().await;
        logger.info(format_heapless!(128; "Client handler task terminating for session {}", session_id).as_str());
    }
}

enum ClientToBrokerMessage<const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize> {
    ReceivedPacket(Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>),
    Disconnected,
}

impl<const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize> core::fmt::Display for ClientToBrokerMessage<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            ClientToBrokerMessage::ReceivedPacket(packet) => write!(f, "ReceivedPacket: {:?}", packet),
            ClientToBrokerMessage::Disconnected => write!(f, "Disconnected"),
        }
    }
}

enum BrokerToClientMessage<const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize> {
    SendPacket(Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>),
    Disconnect,
}

impl<const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize> core::fmt::Display for BrokerToClientMessage<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            BrokerToClientMessage::SendPacket(packet) => write!(f, "SendPacket: {:?}", packet),
            BrokerToClientMessage::Disconnect => write!(f, "Disconnect"),
        }
    }
}
