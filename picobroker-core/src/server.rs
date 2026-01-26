use log::{error, info};
use crate::{bytes_to_hex, BrokerError, ClientId, ConnAckPacket, Delay, NetworkError, Packet, PacketEncoder, PacketType, PicoBroker, PubAckPacket, QoS, SocketAddr, TcpListener, TcpStream, TimeSource};
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
    delay: D,
    broker: PicoBroker<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_SIZE, MAX_CLIENTS, MAX_TOPICS, MAX_SUBSCRIBERS_PER_TOPIC>,
    client_registry: ClientRegistry<
        TL::Stream,
        MAX_TOPIC_NAME_LENGTH,
        MAX_PAYLOAD_SIZE,
        QUEUE_SIZE,
        MAX_CLIENTS,
        MAX_TOPICS,
        MAX_SUBSCRIBERS_PER_TOPIC,
    >,
}

impl<
    TS: TimeSource,
    TL: TcpListener,
    D: Delay,
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const QUEUE_SIZE: usize,
    const MAX_CLIENTS: usize,
    const MAX_TOPICS: usize,
    const MAX_SUBSCRIBERS_PER_TOPIC: usize,
> PicoBrokerServer<TS, TL, D, MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_SIZE, MAX_CLIENTS, MAX_TOPICS, MAX_SUBSCRIBERS_PER_TOPIC>
{
    /// Create a new MQTT broker server
    pub fn new(time_source: TS, listener: TL, delay: D) -> Self {
        Self {
            time_source,
            listener,
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
    {
        info!("Starting server main loop");
        loop {
            // 1. Try to accept a connection (non-blocking)
            // Note: This returns immediately if no connection is pending
            if let Ok((stream, socket_addr)) = self.listener.try_accept().await {
                info!("Received new connection from {}", socket_addr);

                const KEEP_ALIVE_SECS: u16 = 60; // Default non-configurable keep-alive for new clients
                let current_time = self.time_source.now_secs();
                let maybe_session_id = self.client_registry.register_new_client(stream, socket_addr, KEEP_ALIVE_SECS, current_time);
                match maybe_session_id {
                    Ok(session_id) => {
                        info!("Registered new client with session ID {}", session_id);

                    },
                    Err(e) => {
                        error!("Failed to register new client: {}", e);
                        error!("Closing connection");
                    }
                };
            }

            // 2. Process client messages (round-robin through all sessions)
            self.read_client_messages().await?;

            // 3. Process messages from clients and route them via the broker
            self.process_client_messages().await?;

            // 4. Write messages to clients
            self.write_client_messages().await?;

            // 5. Check for expired clients (keep-alive timeout)
            self.client_registry.cleanup_expired_sessions(&mut self.broker, self.time_source.now_secs());

            // 6. Small yield to prevent busy-waiting
            self.delay.sleep_ms(10).await;
        }
    }

    async fn read_client_messages(&mut self) -> Result<(), BrokerError> {
        let mut sessions_to_remove: [Option<usize>; 16] = [None; 16];
        let mut remove_count = 0usize;

        for session_option in self.client_registry.sessions.iter_mut() {
            if let Some(session) = session_option {
                match Self::try_read_packet_from_stream(&mut session.stream).await {
                    Ok(Some(packet)) => {
                        info!("Received packet from client {}: {:?}", session.session_id, packet);
                        // Here you would process the packet (e.g., handle CONNECT, PUBLISH, SUBSCRIBE, etc.)
                        session.update_activity(self.time_source.now_secs());
                        let _ = session.queue_rx_packet(packet);
                    }
                    Ok(None) => {
                        // No packet available, continue
                    }
                    Err(e) => {
                        // All errors here are fatal (WouldBlock is already handled in try_read_packet_from_stream)
                        error!("Error reading packet from client {}: {}", session.session_id, e);

                        // Remove session on any fatal error
                        if remove_count < sessions_to_remove.len() {
                            sessions_to_remove[remove_count] = Some(session.session_id);
                            remove_count += 1;
                        }
                    }
                }
            }
        }

        // Remove sessions that had fatal errors
        for i in 0..remove_count {
            if let Some(session_id) = sessions_to_remove[i] {
                let _ = self.client_registry.remove_session(session_id, &mut self.broker);
            }
        }

        Ok(())
    }

    async fn process_client_messages(&mut self) -> Result<(), BrokerError> {
        // Process messages from clients and route them via the broker
        // Collect client IDs and packets to route, then process in a second pass
        let mut messages_to_route: [Option<(ClientId, Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>)>; QUEUE_SIZE] = [const { None }; QUEUE_SIZE];
        let mut route_count = 0usize;

        for session_option in self.client_registry.sessions.iter_mut() {
            if let Some(session) = session_option {
                while let Some(packet) = session.dequeue_rx_packet() {
                    info!("Processing packet from client {}: {:?}", session.session_id, packet);
                    match packet {
                        Packet::Connect(connect) => {
                            // Handle CONNECT packet
                            info!("Client {} connected with ID {:?}", session.session_id, connect.client_id);
                            session.client_id = Some(connect.client_id.clone());
                            session.state = ClientState::Connected;
                            // Send CONNACK response
                            let connack = Packet::ConnAck(ConnAckPacket::default());
                            let _ = session.queue_tx_packet(connack);
                        }
                        Packet::ConnAck(_) => {}
                        Packet::Publish(publish) => {
                            // Handle PUBLISH packet
                            info!("Client {} published to topic {}: {:?}", session.session_id, publish.topic_name, publish.payload);

                            // Get subscribers for this topic
                            let subscribers = self.broker.topics.get_subscribers(&publish.topic_name);

                            // Collect routing tasks for later processing
                            for client_id in subscribers {
                                if route_count < messages_to_route.len() {
                                    messages_to_route[route_count] = Some((client_id, Packet::Publish(publish.clone())));
                                    route_count += 1;
                                }
                            }

                            // Send PUBACK if QoS 1
                            if publish.qos > QoS::AtMostOnce && publish.packet_id.is_some() {
                                let mut puback = PubAckPacket::default();
                                puback.packet_id = publish.packet_id.unwrap();
                                let _ = session.queue_tx_packet(Packet::PubAck(puback));
                            }
                        }
                        Packet::PubAck(_) => {}
                        Packet::PubRec(_) => {}
                        Packet::PubRel(_) => {}
                        Packet::PubComp(_) => {}
                        Packet::Subscribe(_) => {}
                        Packet::SubAck(_) => {}
                        Packet::Unsubscribe(_) => {}
                        Packet::UnsubAck(_) => {}
                        Packet::PingReq(_) => {}
                        Packet::PingResp(_) => {}
                        Packet::Disconnect(_) => {}
                    }
                }
            }
        }

        // Second pass: route collected messages to subscribers
        for i in 0..route_count {
            if let Some((client_id, packet)) = &messages_to_route[i] {
                if let Some(target_session) = self.client_registry.find_session_by_client_id(client_id) {
                    let _ = target_session.queue_tx_packet(packet.clone());
                    info!("Routed message to subscriber {:?}", client_id);
                }
            }
        }

        Ok(())
    }

    async fn write_client_messages(&mut self) -> Result<(), BrokerError> {
        let mut sessions_to_remove: [Option<usize>; 16] = [None; 16];
        let mut remove_count = 0usize;

        for session_option in self.client_registry.sessions.iter_mut() {
            if let Some(session) = session_option {
                while let Some(packet) = session.dequeue_tx_packet() {
                    info!("Sending packet to client {}: {:?}", session.session_id, packet);
                    let mut buffer = [0u8; MAX_PAYLOAD_SIZE];
                    let packet_size_result = packet.encode(&mut buffer);
                    let packet_size = match packet_size_result {
                        Ok(encoded) => encoded,
                        Err(e) => {
                            error!("Error encoding packet for client {}: {}", session.session_id, e);
                            continue; // Skip sending this packet
                        }
                    };
                    let encoded_packet = &buffer[..packet_size];
                    match session.stream.write(&encoded_packet).await {
                        Ok(bytes_written) => {
                            info!("Sent {} bytes to client {}", bytes_written, session.session_id);
                        }
                        Err(e) => {
                            error!("Error writing packet to client {}: {}", session.session_id, e);

                            // Fatal error - mark for removal
                            if remove_count < sessions_to_remove.len() {
                                sessions_to_remove[remove_count] = Some(session.session_id);
                                remove_count += 1;
                            }
                            break; // Stop processing this session
                        }
                    }
                }
            }
        }

        // Remove sessions that had fatal errors
        for i in 0..remove_count {
            if let Some(session_id) = sessions_to_remove[i] {
                let _ = self.client_registry.remove_session(session_id, &mut self.broker);
            }
        }

        Ok(())
    }

    async fn try_read_packet_from_stream<S: TcpStream>(stream: &mut S) -> Result<Option<Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>>, BrokerError> {
        let mut header_buffer = [0u8; MAX_PAYLOAD_SIZE];

        match stream.try_read(&mut header_buffer).await {
            Ok(bytes_read) => {
                if bytes_read > 0 {
                    let slice = &header_buffer[..bytes_read];
                    let hex_repr = bytes_to_hex::<256>(slice);
                    info!("Trying to decode packet: {}", &hex_repr);
                    let packet_type = PacketType::from(slice[0]);
                    info!("Detected packet type: {:?}", packet_type);
                    let maybe_packet = Packet::decode(slice);
                    match maybe_packet {
                        Ok(packet) => Ok(Some(packet)),
                        Err(e) => {
                            error!("Failed to decode packet: {}", e);
                            Err(BrokerError::PacketEncodingError { error: e })
                        }
                    }
                } else {
                    info!("Connection closed by peer");
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

    /// Receive queue (client -> broker)
    pub rx_queue: HeaplessVec<
        Option<Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>>,
        QUEUE_SIZE
    >,

    /// Transmit queue (broker -> client)
    pub tx_queue: HeaplessVec<
        Option<Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>>,
        QUEUE_SIZE
    >,
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
            rx_queue: HeaplessVec::new(),
            tx_queue: HeaplessVec::new(),
        }
    }

    /// Create a new client session with an explicit session ID
    pub fn new_with_id(stream: S, socket_addr: SocketAddr, keep_alive_secs: u16, current_time: u64, session_id: usize) -> Self {
        Self {
            stream,
            session_id,
            socket_addr,
            client_id: None,
            state: ClientState::Connecting,
            keep_alive_secs,
            last_activity: current_time,
            rx_queue: HeaplessVec::new(),
            tx_queue: HeaplessVec::new(),
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

    /// Queue a packet for transmission to the client
    pub fn queue_tx_packet(&mut self, packet: Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>) -> Result<(), BrokerError> {
        self.tx_queue.push(Some(packet)).map_err(|_| BrokerError::ClientQueueFull { queue_size: QUEUE_SIZE })
    }

    /// Dequeue a packet to send to the client
    pub fn dequeue_tx_packet(&mut self) -> Option<Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>> {
        self.tx_queue.dequeue_front().flatten()
    }

    /// Queue a received packet from the client
    pub fn queue_rx_packet(&mut self, packet: Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>) -> Result<(), BrokerError> {
        self.rx_queue.push(Some(packet)).map_err(|_| BrokerError::ClientQueueFull { queue_size: QUEUE_SIZE })
    }

    /// Dequeue a received packet from the client
    pub fn dequeue_rx_packet(&mut self) -> Option<Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>> {
        self.rx_queue.dequeue_front().flatten()
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
    const MAX_TOPICS: usize,
    const MAX_SUBSCRIBERS_PER_TOPIC: usize,
> {
    /// Sessions with communication queues
    sessions: HeaplessVec<
        Option<ClientSession<S, MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_SIZE>>,
        MAX_CLIENTS
    >,
    /// Counter for generating unique session IDs
    next_session_id: usize,
}

impl<S: TcpStream, const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize, const QUEUE_SIZE: usize, const MAX_CLIENTS: usize, const MAX_TOPICS: usize, const MAX_SUBSCRIBERS_PER_TOPIC: usize>
ClientRegistry<S, MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_SIZE, MAX_CLIENTS, MAX_TOPICS, MAX_SUBSCRIBERS_PER_TOPIC>
{
    /// Create a new client registry
    pub fn new() -> Self {
        Self {
            sessions: HeaplessVec::new(),
            next_session_id: 0,
        }
    }

    /// Register a new client session
    pub fn register_new_client(&mut self, stream: S, socket_addr: SocketAddr, keep_alive_secs: u16, current_time: u64) -> Result<usize, BrokerError> {
        if self.sessions.len() >= MAX_CLIENTS {
            return Err(BrokerError::MaxClientsReached { max_clients: MAX_CLIENTS });
        }

        // Use counter instead of port for unique session IDs
        let session_id = self.next_session_id;
        self.next_session_id = self.next_session_id.wrapping_add(1);

        let session = ClientSession::new_with_id(stream, socket_addr, keep_alive_secs, current_time, session_id);
        self.sessions.push(Some(session)).map_err(|_| BrokerError::MaxClientsReached { max_clients: MAX_CLIENTS })?;
        Ok(session_id)
    }

    /// Find a mutable session reference by ClientId
    ///
    /// Returns None if session not found or client_id is not set.
    /// Uses linear search which is acceptable since MAX_CLIENTS is typically small (4-16).
    fn find_session_by_client_id(
        &mut self,
        client_id: &ClientId
    ) -> Option<&mut ClientSession<S, MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_SIZE>> {
        self.sessions.iter_mut().find(|s_opt| {
            s_opt.as_ref()
                .map(|s| s.client_id.as_ref() == Some(client_id))
                .unwrap_or(false)
        })
        .and_then(|s_opt| s_opt.as_mut())
    }

    /// Remove a session and cleanup its subscriptions
    ///
    /// This will:
    /// 1. Remove the session from the registry
    /// 2. Unsubscribe the client from all topics
    /// 3. Close the network connection
    pub fn remove_session(
        &mut self,
        session_id: usize,
        broker: &mut PicoBroker<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_SIZE,
                                MAX_CLIENTS, MAX_TOPICS, MAX_SUBSCRIBERS_PER_TOPIC>
    ) -> Result<(), BrokerError> {
        // Find the session index
        let session_idx = self.sessions.iter().position(|s| {
            s.as_ref().map(|sess| sess.session_id == session_id).unwrap_or(false)
        });

        if let Some(idx) = session_idx {
            // Extract the client_id before removing
            let client_id = self.sessions[idx]
                .as_ref()
                .and_then(|s| s.client_id.clone());

            // Cleanup subscriptions if client_id exists
            if let Some(id) = &client_id {
                broker.topics.unregister_client(id.clone());
                info!("Cleaned up subscriptions for client {:?}", id);
            }

            // Remove from vector - replace with None and then shift
            // We can't use remove() because Option<ClientSession> doesn't implement Clone
            self.sessions[idx] = None;

            // Shift remaining elements to fill the gap
            for i in idx..self.sessions.len() - 1 {
                self.sessions[i] = self.sessions[i + 1].take();
            }

            Ok(())
        } else {
            error!("Session {} not found for removal", session_id);
            Err(BrokerError::NetworkError {
                error: NetworkError::ConnectionClosed
            })
        }
    }

    /// Cleanup expired sessions based on keep-alive timeout
    ///
    /// Removes all sessions where time since last activity exceeds 1.5x keep-alive.
    pub fn cleanup_expired_sessions(
        &mut self,
        broker: &mut PicoBroker<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_SIZE,
                                MAX_CLIENTS, MAX_TOPICS, MAX_SUBSCRIBERS_PER_TOPIC>,
        current_time: u64
    ) {
        // Use a simple array to collect expired session IDs
        let mut expired_ids = [None; 16]; // Stack-allocated, reasonable max
        let mut expired_count = 0usize;

        // Collect expired session IDs
        for session in &self.sessions {
            if let Some(sess) = session {
                if sess.is_expired(current_time) && expired_count < expired_ids.len() {
                    expired_ids[expired_count] = Some(sess.session_id);
                    expired_count += 1;
                }
            }
        }

        // Remove each expired session
        for i in 0..expired_count {
            if let Some(session_id) = expired_ids[i] {
                info!("Removing expired session {}", session_id);
                let _ = self.remove_session(session_id, broker);
            }
        }
    }
}