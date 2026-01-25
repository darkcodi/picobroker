use crate::{BrokerError, Delay, Logger, NetworkError, PicoBroker, QoS, TcpListener, TcpStream, TimeSource};
use crate::format_heapless;
use crate::protocol::utils::read_variable_length;
use crate::{ConnAckPacket, ConnectReturnCode, Packet, PacketEncoder, PingRespPacket, SubAckPacket, UnsubAckPacket};
use crate::topics::{TopicName, TopicSubscription};
use crate::PacketEncodingError;

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
    broker: PicoBroker<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_SIZE, MAX_CLIENTS, MAX_TOPICS, MAX_SUBSCRIBERS_PER_TOPIC, TL::Stream>,
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
where
    TL::Stream: Send + 'static,
    L: Clone + Send + 'static,
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
    pub async fn run(&mut self) -> Result<(), BrokerError> {
        self.logger.info("Starting server main loop");
        loop {
            // 1. Try to accept a connection (non-blocking)
            // Note: This returns immediately if no connection is pending
            if let Ok((mut stream, socket_addr)) = self.listener.try_accept().await {
                self.logger.info(format_heapless!(128; "Received new connection from {}", socket_addr).as_str());

                const KEEP_ALIVE_SECS: u16 = 60; // Default non-configurable keep-alive for new clients
                let current_time = self.time_source.now_secs();
                self.logger.debug(format_heapless!(128; "Current time: {}, keep-alive: {}s", current_time, KEEP_ALIVE_SECS).as_str());

                let maybe_session_id = self.broker.register_new_client(socket_addr, KEEP_ALIVE_SECS, current_time);
                match maybe_session_id {
                    Ok(session_id) => {
                        self.logger.info(format_heapless!(128; "Registered new client with session ID {}", session_id).as_str());

                        // Store the stream in the session
                        // Find the session with matching ID and store the stream
                        let mut stream_stored = false;
                        for session in self.broker.clients.sessions.iter_mut() {
                            if let Some(ref mut client) = session {
                                if client.session_id == session_id {
                                    client.stream = Some(stream);
                                    stream_stored = true;
                                    self.logger.debug(format_heapless!(128; "Stream stored for session ID {}", session_id).as_str());
                                    break;
                                }
                            }
                        }
                        if !stream_stored {
                            self.logger.warn(format_heapless!(128; "Failed to store stream for session ID {}", session_id).as_str());
                        }
                    },
                    Err(e) => {
                        self.logger.error(format_heapless!(128; "Failed to register new client: {}", e).as_str());
                        self.logger.error("Closing connection");
                        let _ = stream.close().await;
                    }
                };
            } else {
            }

            // 2. Process client messages (round-robin through all sessions)
            self.process_client_messages().await?;

            // 3. Check for expired clients (keep-alive timeout)
            self.process_expired_clients().await;

            // 4. Small yield to prevent busy-waiting
            self.delay.sleep_ms(10).await;
        }
    }

    /// Process a single packet from a client
    fn process_packet(&mut self, session_index: usize, packet: Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>) -> Result<(), BrokerError> {
        self.logger.debug(format_heapless!(128; "Processing packet from client index {}", session_index).as_str());

        match packet {
            Packet::Connect(connect_pkt) => {
                self.logger.info(format_heapless!(128; "CONNECT from client {}", session_index).as_str());

                // Extract client info
                let client_id = connect_pkt.client_id.clone();
                let keep_alive = connect_pkt.keep_alive;
                self.logger.debug(format_heapless!(128; "Client ID: {}, keep-alive: {}s", client_id.as_str(), keep_alive).as_str());

                // Update session
                if let Some(session) = self.broker.clients.get_session_mut(session_index) {
                    session.client_id = Some(client_id.clone());
                    session.keep_alive_secs = keep_alive;
                    session.state = crate::client::ClientState::Connected;
                    self.logger.debug(format_heapless!(128; "Session {} updated to Connected state", session_index).as_str());
                } else {
                    self.logger.warn(format_heapless!(128; "Session {} not found for CONNECT", session_index).as_str());
                }

                // Send CONNACK
                let connack = ConnAckPacket {
                    session_present: false,
                    return_code: ConnectReturnCode::Accepted,
                };
                self.logger.debug(format_heapless!(128; "Sending CONNACK to client {}", session_index).as_str());
                self.send_packet_to_session(session_index, &Packet::ConnAck(connack))?;

                self.logger.info(format_heapless!(128; "Client {} connected as {}", session_index, client_id.as_str()).as_str());
            }
            Packet::Publish(publish_pkt) => {
                self.logger.debug(format_heapless!(128; "PUBLISH to {}", publish_pkt.topic_name.as_str()).as_str());
                self.logger.debug(format_heapless!(128; "QoS: {}, retain: {}, dup: {}, payload len: {}",
                    publish_pkt.qos as u8, publish_pkt.retain, publish_pkt.dup, publish_pkt.payload.len()).as_str());

                // Get subscribers for this topic
                let topic = TopicName::try_from(publish_pkt.topic_name.as_str())
                    .map_err(|_| BrokerError::PacketEncodingError {
                        error: PacketEncodingError::TopicNameLengthExceeded {
                            max_length: MAX_TOPIC_NAME_LENGTH,
                            actual_length: publish_pkt.topic_name.as_str().len(),
                        }
                    })?;

                let subscribers = self.broker.topics.get_subscribers(&topic);
                self.logger.debug(format_heapless!(128; "Found {} subscribers for topic", subscribers.len()).as_str());

                // Queue PUBLISH for each subscriber
                // Collect subscriber indices first to avoid borrow issues
                let mut subscriber_indices = [0usize; 16]; // Fixed size array
                let mut sub_count = 0;
                for (idx, session_opt) in self.broker.clients.sessions.iter().enumerate() {
                    if let Some(session) = session_opt {
                        if let Some(ref client_id) = session.client_id {
                            if subscribers.iter().any(|sub| sub == client_id) {
                                if sub_count < subscriber_indices.len() {
                                    subscriber_indices[sub_count] = idx;
                                    sub_count += 1;
                                }
                            }
                        }
                    }
                }

                self.logger.debug(format_heapless!(128; "Queueing PUBLISH for {} subscribers", sub_count).as_str());

                // Now queue publish packets
                for &idx in &subscriber_indices[..sub_count] {
                    self.send_packet_to_session(idx, &Packet::Publish(publish_pkt.clone()))?;
                }
            }
            Packet::Subscribe(subscribe_pkt) => {
                self.logger.info(format_heapless!(128; "SUBSCRIBE to {}", subscribe_pkt.topic_filter.as_str()).as_str());
                self.logger.debug(format_heapless!(128; "Packet ID: {}, QoS: {}", subscribe_pkt.packet_id, subscribe_pkt.requested_qos as u8).as_str());

                // Subscribe client to topic
                if let Some(session) = self.broker.clients.get_session(session_index) {
                    if let Some(ref client_id) = session.client_id {
                        let topic_filter = TopicSubscription::try_from(subscribe_pkt.topic_filter.as_str())
                            .map_err(|_| BrokerError::PacketEncodingError {
                                error: PacketEncodingError::TopicNameLengthExceeded {
                                    max_length: MAX_TOPIC_NAME_LENGTH,
                                    actual_length: subscribe_pkt.topic_filter.as_str().len(),
                                }
                            })?;

                        self.broker.topics.subscribe(client_id.clone(), topic_filter)?;
                        self.logger.debug(format_heapless!(128; "Client {} subscribed to {}", client_id.as_str(), subscribe_pkt.topic_filter.as_str()).as_str());

                        // Send SUBACK
                        let suback = SubAckPacket {
                            packet_id: subscribe_pkt.packet_id,
                            granted_qos: QoS::AtMostOnce,
                        };
                        self.logger.debug(format_heapless!(128; "Sending SUBACK to client {}", session_index).as_str());
                        self.send_packet_to_session(session_index, &Packet::SubAck(suback))?;
                    } else {
                        self.logger.warn(format_heapless!(128; "Session {} has no client ID for SUBSCRIBE", session_index).as_str());
                    }
                } else {
                    self.logger.warn(format_heapless!(128; "Session {} not found for SUBSCRIBE", session_index).as_str());
                }
            }
            Packet::Unsubscribe(unsubscribe_pkt) => {
                self.logger.info(format_heapless!(128; "UNSUBSCRIBE from {}", unsubscribe_pkt.topic_filter.as_str()).as_str());
                self.logger.debug(format_heapless!(128; "Packet ID: {}", unsubscribe_pkt.packet_id).as_str());

                // Unsubscribe client from topic
                if let Some(session) = self.broker.clients.get_session(session_index) {
                    if let Some(ref client_id) = session.client_id {
                        let topic_filter = TopicSubscription::try_from(unsubscribe_pkt.topic_filter.as_str())
                            .map_err(|_| BrokerError::PacketEncodingError {
                                error: PacketEncodingError::TopicNameLengthExceeded {
                                    max_length: MAX_TOPIC_NAME_LENGTH,
                                    actual_length: unsubscribe_pkt.topic_filter.as_str().len(),
                                }
                            })?;

                        self.broker.topics.unsubscribe(client_id.clone(), &topic_filter);
                        self.logger.debug(format_heapless!(128; "Client {} unsubscribed from {}", client_id.as_str(), unsubscribe_pkt.topic_filter.as_str()).as_str());

                        // Send UNSUBACK
                        let unsuback = UnsubAckPacket {
                            packet_id: unsubscribe_pkt.packet_id,
                        };
                        self.logger.debug(format_heapless!(128; "Sending UNSUBACK to client {}", session_index).as_str());
                        self.send_packet_to_session(session_index, &Packet::UnsubAck(unsuback))?;
                    } else {
                        self.logger.warn(format_heapless!(128; "Session {} has no client ID for UNSUBSCRIBE", session_index).as_str());
                    }
                } else {
                    self.logger.warn(format_heapless!(128; "Session {} not found for UNSUBSCRIBE", session_index).as_str());
                }
            }
            Packet::PingReq(_) => {
                self.logger.debug(format_heapless!(128; "PINGREQ from client {}", session_index).as_str());

                // Send PINGRESP
                let pingresp = PingRespPacket;
                self.logger.debug(format_heapless!(128; "Sending PINGRESP to client {}", session_index).as_str());
                self.send_packet_to_session(session_index, &Packet::PingResp(pingresp))?;
            }
            Packet::Disconnect(_) => {
                self.logger.info(format_heapless!(128; "DISCONNECT from client {}", session_index).as_str());

                // Mark session for cleanup
                if let Some(session) = self.broker.clients.get_session_mut(session_index) {
                    session.state = crate::client::ClientState::Disconnected;
                    self.logger.debug(format_heapless!(128; "Client {} marked as Disconnected", session_index).as_str());
                }
            }
            _ => {
                // Other packet types not handled yet
                self.logger.warn(format_heapless!(128; "Unhandled packet type from client {}", session_index).as_str());
            }
        }

        self.logger.debug(format_heapless!(128; "Finished processing packet from client {}", session_index).as_str());
        Ok(())
    }

    /// Send a packet to a client's outbound buffer
    fn send_packet_to_session(&mut self, session_index: usize, packet: &Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>) -> Result<(), BrokerError> {
        self.logger.debug(format_heapless!(128; "Sending packet to session {}, buffer size before: {}",
            session_index,
            self.broker.clients.get_session(session_index).map_or(0, |s| s.outbound_buffer.len())).as_str());

        let mut buffer = [0u8; MAX_PAYLOAD_SIZE];
        let encoded_size = packet.encode(&mut buffer)
            .map_err(|e| BrokerError::PacketEncodingError { error: e })?;

        self.logger.debug(format_heapless!(128; "Packet encoded size: {} bytes", encoded_size).as_str());

        if let Some(session) = self.broker.clients.get_session_mut(session_index) {
            let buffer_len_before = session.outbound_buffer.len();
            session.outbound_buffer.extend_from_slice(&buffer[..encoded_size])
                .map_err(|_| BrokerError::PacketEncodingError {
                    error: PacketEncodingError::BufferTooSmall { buffer_size: session.outbound_buffer.capacity() }
                })?;
            self.logger.debug(format_heapless!(128; "Buffer extended from {} to {} bytes",
                buffer_len_before, session.outbound_buffer.len()).as_str());
        } else {
            self.logger.warn(format_heapless!(128; "Session {} not found when sending packet", session_index).as_str());
        }

        Ok(())
    }

    /// Process client messages (read packets, process, flush outbound buffers)
    async fn process_client_messages(&mut self) -> Result<(), BrokerError> {
        let current_time = self.time_source.now_secs();

        // Iterate through all sessions
        let mut i = 0;
        let mut active_sessions = 0;
        while i < self.broker.clients.sessions.len() {
            let has_stream = self.broker.clients.sessions.get(i)
                .and_then(|s| s.as_ref())
                .map(|client| client.stream.is_some() && client.state != crate::client::ClientState::Disconnected)
                .unwrap_or(false);

            if !has_stream {
                self.logger.debug(format_heapless!(128; "Session {}: no stream or disconnected", i).as_str());
                i += 1;
                continue;
            }

            active_sessions += 1;
            self.logger.debug(format_heapless!(128; "Processing session {} (client ID: {:?})",
                i,
                self.broker.clients.sessions.get(i).and_then(|s| s.as_ref()).and_then(|c| c.client_id.as_ref())).as_str());

            // 1. Try to read data into buffer
            let read_result = {
                let client = self.broker.clients.sessions.get_mut(i).unwrap().as_mut().unwrap();
                let stream = client.stream.as_mut().unwrap();
                let read_space = &mut client.read_buffer.as_mut_slice()[client.read_buffer_len..];
                stream.try_read(read_space).await
            };

            match read_result {
                Ok(bytes_read) => {
                    if bytes_read > 0 {
                        self.logger.debug(format_heapless!(128; "Session {}: read {} bytes", i, bytes_read).as_str());
                        {
                            let client = self.broker.clients.sessions.get_mut(i).unwrap().as_mut().unwrap();
                            client.read_buffer_len += bytes_read;
                            self.logger.debug(format_heapless!(128; "Session {}: buffer now {} bytes", i, client.read_buffer_len).as_str());
                        }

                        // 2. Try to parse packet(s) from buffer
                        let mut packets_processed = 0;
                        loop {
                            let (packet, packet_size) = {
                                let client = self.broker.clients.sessions.get(i).unwrap().as_ref().unwrap();
                                let decode_result = Packet::decode(&client.read_buffer.as_slice()[..client.read_buffer_len]);

                                match decode_result {
                                    Ok(packet) => {
                                        let (remaining_len, var_int_len) = read_variable_length(&client.read_buffer[1..])
                                            .map_err(|e| BrokerError::PacketEncodingError { error: e })?;
                                        let packet_size = 1 + var_int_len + remaining_len;
                                        self.logger.debug(format_heapless!(128; "Session {}: decoded packet size {} bytes", i, packet_size).as_str());
                                        (Some(packet), packet_size)
                                    }
                                    Err(PacketEncodingError::IncompletePacket { .. }) => {
                                        self.logger.debug(format_heapless!(128; "Session {}: incomplete packet, waiting for more data", i).as_str());
                                        break;
                                    }
                                    Err(e) => {
                                        let client = self.broker.clients.sessions.get_mut(i).unwrap().as_mut().unwrap();
                                        self.logger.error(format_heapless!(128; "Packet decode error: {}", e).as_str());
                                        client.state = crate::client::ClientState::Disconnected;
                                        break;
                                    }
                                }
                            };

                            if let Some(packet) = packet {
                                {
                                    let client = self.broker.clients.sessions.get_mut(i).unwrap().as_mut().unwrap();
                                    client.update_activity(current_time);
                                }

                                self.logger.debug(format_heapless!(128; "Session {}: processing packet #{}", i, packets_processed + 1).as_str());

                                // Process packet
                                self.process_packet(i, packet)?;
                                packets_processed += 1;

                                // Remove processed packet from buffer
                                let client = self.broker.clients.sessions.get_mut(i).unwrap().as_mut().unwrap();
                                client.read_buffer.copy_within(packet_size..client.read_buffer_len, 0);
                                client.read_buffer_len -= packet_size;
                                self.logger.debug(format_heapless!(128; "Session {}: removed {} bytes from buffer, remaining: {}",
                                    i, packet_size, client.read_buffer_len).as_str());
                            } else {
                                break;
                            }
                        }

                        if packets_processed > 0 {
                            self.logger.debug(format_heapless!(128; "Session {}: processed {} packets", i, packets_processed).as_str());
                        }
                    } else if bytes_read == 0 {
                        // Connection closed by peer
                        let client = self.broker.clients.sessions.get_mut(i).unwrap().as_mut().unwrap();
                        // Only treat as closed if we've received at least one packet (client is Connected)
                        // If still in Connecting state, might just not have sent data yet
                        if client.state == crate::client::ClientState::Connected {
                            self.logger.info(format_heapless!(128; "Client {} closed connection", client.session_id).as_str());
                            client.state = crate::client::ClientState::Disconnected;
                        } else {
                            // Still in Connecting state - check if we've been waiting too long
                            let elapsed = current_time.saturating_sub(client.last_activity);
                            if elapsed > 5 {
                                // Give up after 5 seconds of no data in Connecting state
                                self.logger.warn(format_heapless!(128; "Session {}: timeout waiting for CONNECT packet ({}s)", i, elapsed).as_str());
                                client.state = crate::client::ClientState::Disconnected;
                            } else {
                                // Still connecting - no data yet but not necessarily closed
                                self.logger.debug(format_heapless!(128; "Session {}: no data yet (still connecting, {}s elapsed)", i, elapsed).as_str());
                            }
                        }
                    }
                }
                Err(NetworkError::WouldBlock) | Err(NetworkError::NoPendingConnection) => {
                    // No data available - continue to next client
                    self.logger.debug(format_heapless!(128; "Session {}: no data available (would block)", i).as_str());
                }
                Err(e) => {
                    // Connection error - mark for removal
                    let client = self.broker.clients.sessions.get_mut(i).unwrap().as_mut().unwrap();
                    self.logger.error(format_heapless!(128; "Read error: {}", e).as_str());
                    client.state = crate::client::ClientState::Disconnected;
                }
            }

            // 3. Flush outbound buffer
            {
                let client = self.broker.clients.sessions.get_mut(i).unwrap().as_mut().unwrap();
                if !client.outbound_buffer.is_empty() {
                    let buffer_len = client.outbound_buffer.len();
                    self.logger.debug(format_heapless!(128; "Session {}: flushing {} bytes from outbound buffer", i, buffer_len).as_str());
                    let stream = client.stream.as_mut().unwrap();
                    match stream.write(client.outbound_buffer.as_slice()).await {
                        Ok(bytes_written) => {
                            self.logger.debug(format_heapless!(128; "Session {}: wrote {} bytes", i, bytes_written).as_str());
                            // Remove written bytes from buffer
                            client.outbound_buffer.remove_prefix(bytes_written);
                            if !client.outbound_buffer.is_empty() {
                                self.logger.debug(format_heapless!(128; "Session {}: {} bytes remaining in buffer", i, client.outbound_buffer.len()).as_str());
                            }
                        }
                        Err(e) => {
                            // Write error - mark for removal
                            self.logger.error(format_heapless!(128; "Session {}: write error: {}", i, e).as_str());
                            client.state = crate::client::ClientState::Disconnected;
                        }
                    }
                }
            }

            i += 1;
        }

        if active_sessions > 0 {
            self.logger.debug(format_heapless!(128; "Processed {} active sessions", active_sessions).as_str());
        }

        Ok(())
    }

    /// Process expired clients (close streams, cleanup)
    async fn process_expired_clients(&mut self) {
        let current_time = self.time_source.now_secs();

        // Collect indices of clients to remove
        let mut to_remove = [0usize; 16]; // Fixed size array
        let mut remove_count = 0;

        // First pass: mark sessions for removal
        for i in 0..self.broker.clients.sessions.len() {
            if let Some(session) = self.broker.clients.sessions.get(i) {
                if let Some(ref client) = session {
                    let is_expired = client.is_expired(current_time);
                    let is_disconnected = client.state == crate::client::ClientState::Disconnected;

                    if is_expired || is_disconnected {
                        if remove_count < to_remove.len() {
                            to_remove[remove_count] = i;
                            remove_count += 1;
                        }

                        let reason = if is_expired { "expired" } else { "disconnected" };
                        self.logger.info(format_heapless!(128; "Removing {} client {} (index {})",
                            reason, client.session_id, i).as_str());

                        // Unsubscribe from all topics
                        if let Some(ref client_id) = client.client_id {
                            self.logger.debug(format_heapless!(128; "Unregistering client {} from all topics", client_id.as_str()).as_str());
                            self.broker.topics.unregister_client(client_id.clone());
                        }
                    }
                }
            }
        }

        if remove_count > 0 {
            self.logger.debug(format_heapless!(128; "Found {} clients to remove", remove_count).as_str());
        }

        // Second pass: close streams and set to None
        for &i in &to_remove[..remove_count] {
            if let Some(session) = self.broker.clients.get_session_mut(i) {
                if let Some(ref mut stream) = session.stream {
                    self.logger.debug(format_heapless!(128; "Closing stream for session index {}", i).as_str());
                    let _ = stream.close().await;
                }
                session.state = crate::client::ClientState::Disconnected;
            }
        }

        // Third pass: remove sessions marked as disconnected
        let before_count = self.broker.clients.sessions.len();
        let mut i = 0;
        while i < self.broker.clients.sessions.len() {
            let is_disconnected = self.broker.clients.sessions.get(i)
                .and_then(|s| s.as_ref())
                .map(|client| client.state == crate::client::ClientState::Disconnected)
                .unwrap_or(false);

            if is_disconnected {
                // Swap with last element and decrease length
                let last_idx = self.broker.clients.sessions.len() - 1;
                if i != last_idx {
                    self.logger.debug(format_heapless!(128; "Swapping session {} with session {}", i, last_idx).as_str());
                    // Use ptr::swap to avoid borrow checker issues
                    unsafe {
                        let ptr_i = self.broker.clients.sessions.data.as_mut_ptr().add(i);
                        let ptr_last = self.broker.clients.sessions.data.as_mut_ptr().add(last_idx);
                        core::ptr::swap(ptr_i, ptr_last);
                    }
                }
                self.broker.clients.sessions.length -= 1;
                self.logger.debug(format_heapless!(128; "Removed session {}, total sessions now: {}", i, self.broker.clients.sessions.len()).as_str());
            } else {
                i += 1;
            }
        }

        if self.broker.clients.sessions.len() != before_count {
            self.logger.debug(format_heapless!(128; "Client count changed from {} to {}", before_count, self.broker.clients.sessions.len()).as_str());
        }
    }
}
