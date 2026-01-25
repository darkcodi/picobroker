use crate::{BrokerError, Delay, Logger, NetworkError, Packet, PacketEncoder, PicoBroker, TcpListener, TcpStream, TimeSource};
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
            if let Ok((mut stream, socket_addr)) = self.listener.try_accept().await {
                self.logger.info(format_heapless!(128; "Received new connection from {}", socket_addr).as_str());

                const KEEP_ALIVE_SECS: u16 = 60; // Default non-configurable keep-alive for new clients
                let current_time = self.time_source.now_secs();
                let maybe_session_id = self.broker.register_new_client(socket_addr, KEEP_ALIVE_SECS, current_time);
                match maybe_session_id {
                    Ok(session_id) => {
                        self.logger.info(format_heapless!(128; "Registered new client with session ID {}", session_id).as_str());

                    },
                    Err(e) => {
                        self.logger.error(format_heapless!(128; "Failed to register new client: {}", e).as_str());
                        self.logger.error("Closing connection");
                        let _ = stream.close().await;
                    }
                };
            }

            // // 2. Process client messages (round-robin through all sessions)
            // self.process_client_messages().await?;
            //
            // // 3. Check for expired clients (keep-alive timeout)
            // self.process_expired_clients();

            // 4. Small yield to prevent busy-waiting
            self.delay.sleep_ms(10).await;
        }
    }

    async fn try_read_packet_from_stream<S: TcpStream>(&self, stream: &mut S) -> Result<Option<Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>>, BrokerError> {
        let mut header_buffer = [0u8; MAX_PAYLOAD_SIZE];

        match stream.try_read(&mut header_buffer).await {
            Ok(bytes_read) => {
                if bytes_read > 0 {
                    let slice = &header_buffer[..bytes_read];
                    let maybe_packet = Packet::decode(slice);
                    match maybe_packet {
                        Ok(packet) => Ok(Some(packet)),
                        Err(e) => {
                            self.logger.error(format_heapless!(128; "Failed to decode packet: {}", e).as_str());
                            Err(BrokerError::PacketEncodingError { error: e })
                        }
                    }
                } else {
                    self.logger.info("Connection closed by peer");
                    Err(BrokerError::NetworkError { error: NetworkError::ConnectionClosed })
                }
            }
            Err(e) => {
                self.logger.error(format_heapless!(128; "Failed to read from stream: {}", e).as_str());
                Err(BrokerError::NetworkError { error: e })
            }
        }
    }
}
