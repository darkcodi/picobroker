use log::{error, info};
use crate::{BrokerError, ClientId, ConnAckPacket, Delay, NetworkError, Packet, PacketEncodingError, PacketEncoder, PingRespPacket, PicoBroker, PubAckPacket, QoS, read_variable_length, SubAckPacket, TcpListener, TcpStream, TimeSource, TopicName, TopicSubscription};
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
    streams: HeaplessVec<(ClientId, Option<TL::Stream>), MAX_CLIENTS>,
    rx_buffers: HeaplessVec<ClientReadBuffer<MAX_PAYLOAD_SIZE>, MAX_CLIENTS>,
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
            streams: HeaplessVec::new(),
            rx_buffers: HeaplessVec::new(),
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
            if let Ok((mut stream, socket_addr)) = self.listener.try_accept().await {
                info!("Received new connection from {}", socket_addr);

                const KEEP_ALIVE_SECS: u16 = 60; // Default non-configurable keep-alive for new clients
                let current_time = self.time_source.now_secs();

                // Proactively cleanup any dead/zombie sessions before accepting new connection
                self.broker.cleanup_zombie_sessions();

                let time = self.time_source.now_secs();
                let client_id = ClientId::generate(time);
                match self.broker.register_client(client_id.clone(), KEEP_ALIVE_SECS, current_time) {
                    Ok(_) => {
                        info!("Registered new client with client id {}", client_id);
                        self.set_stream(client_id, stream);
                    },
                    Err(e) => {
                        error!("Failed to register new client: {}", e);
                        error!("Closing connection");
                        let _ = stream.close().await;
                    }
                };
            }

            // 2. Clean up disconnected sessions before attempting to read
            self.broker.cleanup_zombie_sessions();

            // 3. Process client messages (round-robin through all sessions)
            self.read_client_messages().await?;

            // 4. Process messages from clients and route them via the broker
            self.process_client_messages().await?;

            // 5. Write messages to clients
            self.write_client_messages().await?;

            // 6. Check for expired clients (keep-alive timeout)
            self.broker.cleanup_expired_sessions(self.time_source.now_secs());

            // 7. Small yield to prevent busy-waiting
            self.delay.sleep_ms(10).await;
        }
    }

    fn get_stream_mut(&mut self, client_id: ClientId) -> Option<&mut TL::Stream> {
        for (cid, stream_option) in self.streams.iter_mut() {
            if *cid == client_id {
                if let Some(stream) = stream_option {
                    return Some(stream);
                }
            }
        }
        None
    }

    fn set_stream(&mut self, client_id: ClientId, stream: TL::Stream) {
        for (cid, stream_option) in self.streams.iter_mut() {
            if *cid == client_id {
                *stream_option = Some(stream);
                return;
            }
        }
        let _ = self.streams.push((client_id, Some(stream)));
    }

    fn get_or_create_rx_buffer(&mut self, client_id: ClientId) -> &mut ClientReadBuffer<MAX_PAYLOAD_SIZE> {
        // Check if buffer exists
        let idx = self.rx_buffers.iter().position(|b| b.client_id == client_id);
        if let Some(idx) = idx {
            return &mut self.rx_buffers[idx];
        }

        // Create new buffer
        let buf = ClientReadBuffer::new(client_id.clone());
        let _ = self.rx_buffers.push(buf);

        // Return the newly added buffer (it's now at the end)
        let new_idx = self.rx_buffers.len() - 1;
        &mut self.rx_buffers[new_idx]
    }

    fn remove_rx_buffer(&mut self, client_id: &ClientId) {
        let idx = self.rx_buffers.iter().position(|b| b.client_id == *client_id);
        if let Some(idx) = idx {
            self.rx_buffers.remove(idx);
        }
    }

    fn remove_stream(&mut self, client_id: &ClientId) {
        // Find the stream and set it to None instead of removing
        for (cid, stream_option) in self.streams.iter_mut() {
            if *cid == *client_id {
                *stream_option = None;
                return;
            }
        }
    }

    async fn read_client_messages(&mut self) -> Result<(), BrokerError> {
        let mut sessions_to_remove: [Option<ClientId>; 16] = [const { None }; 16];
        let mut remove_count = 0usize;

        // First pass: collect client IDs to process
        let mut client_ids: [Option<ClientId>; 16] = [const { None }; 16];
        let mut client_count = 0usize;

        for session_option in self.broker.sessions().iter() {
            if let Some(session) = session_option {
                if client_count < client_ids.len() {
                    client_ids[client_count] = Some(session.client_id.clone());
                    client_count += 1;
                }
            }
        }

        // Second pass: process each client
        for i in 0..client_count {
            if let Some(client_id) = &client_ids[i] {
                // Step 1: Read from stream into a local buffer
                let (bytes_read, should_disconnect) = {
                    // Get buffer to check remaining space
                    let buffer_idx = self.rx_buffers.iter().position(|b| b.client_id == *client_id);
                    let remaining_space = if let Some(idx) = buffer_idx {
                        MAX_PAYLOAD_SIZE - self.rx_buffers[idx].len()
                    } else {
                        MAX_PAYLOAD_SIZE
                    };

                    if remaining_space == 0 {
                        (None, false)
                    } else {
                        let mut read_buf = [0u8; MAX_PAYLOAD_SIZE];

                        // Get stream and read
                        let stream_result = self.get_stream_mut(client_id.clone());
                        let stream_exists = stream_result.is_some();

                        let read_result = if stream_exists {
                            let stream = stream_result.unwrap();
                            stream.try_read(&mut read_buf[..remaining_space]).await
                        } else {
                            // No stream found - return would block error
                            Err(NetworkError::WouldBlock)
                        };

                        match read_result {
                            Ok(0) => {
                                info!("Connection closed by peer (0 bytes read)");
                                (None, true)
                            }
                            Ok(n) => (Some((&read_buf[..n]).to_vec()), false),
                            Err(e) => {
                                let is_non_fatal = matches!(e,
                                    NetworkError::WouldBlock |
                                    NetworkError::TimedOut |
                                    NetworkError::Interrupted |
                                    NetworkError::InProgress
                                );

                                if is_non_fatal {
                                    (None, false)
                                } else {
                                    error!("Fatal error reading from client {}: {}", client_id, e);
                                    (None, true)
                                }
                            }
                        }
                    }
                };

                // Handle disconnection
                if should_disconnect {
                    self.broker.mark_client_disconnected(client_id.clone());
                }

                // Step 2: Append to buffer
                if let Some(data) = bytes_read {
                    let rx_buffer = self.get_or_create_rx_buffer(client_id.clone());
                    rx_buffer.append(&data);
                }

                // Step 3: Try to decode packets
                let buffer_idx = self.rx_buffers.iter().position(|b| b.client_id == *client_id);
                if let Some(idx) = buffer_idx {
                    let has_data = !self.rx_buffers[idx].is_empty();

                    if has_data {
                        let packet_result = Self::try_decode_from_buffer(&mut self.rx_buffers[idx]);

                        match packet_result {
                            Ok(Some(packet)) => {
                                info!("Received packet from client {}: {:?}", client_id, packet);
                                // Find session mutably to update it
                                if let Some(session) = self.broker.find_session(client_id) {
                                    session.update_activity(self.time_source.now_secs());
                                    let _ = session.queue_rx_packet(packet);
                                }
                            }
                            Ok(None) => {
                                // No complete packet available, continue
                            }
                            Err(e) => {
                                let is_fatal = match e {
                                    BrokerError::NetworkError { error: NetworkError::ConnectionClosed } => true,
                                    BrokerError::NetworkError { error: NetworkError::IoError } => true,
                                    _ => false,
                                };

                                if is_fatal {
                                    error!("Fatal error reading packet from client {}: {}", client_id, e);
                                    if let Some(session) = self.broker.find_session(client_id) {
                                        session.state = ClientState::Disconnected;
                                    }

                                    if remove_count < sessions_to_remove.len() {
                                        sessions_to_remove[remove_count] = Some(client_id.clone());
                                        remove_count += 1;
                                    }
                                } else {
                                    info!("Non-fatal error reading from client {}: {}", client_id, e);
                                }
                            }
                        }
                    }
                }
            }
        }

        // Remove sessions that had fatal errors
        for i in 0..remove_count {
            if let Some(client_id) = &sessions_to_remove[i] {
                let _ = self.broker.remove_client(client_id);
                self.remove_rx_buffer(client_id);
                self.remove_stream(client_id);
            }
        }

        Ok(())
    }

    /// Try to decode a packet from a RX buffer
    fn try_decode_from_buffer(rx_buffer: &mut ClientReadBuffer<MAX_PAYLOAD_SIZE>) -> Result<Option<Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>>, BrokerError> {
        // Step 1: Check if we have enough data
        if rx_buffer.len() < 2 {
            return Ok(None);
        }

        // Step 2: Decode Remaining Length
        let remaining_length_result = read_variable_length(&rx_buffer.as_slice()[1..]);
        let (remaining_length, var_int_len) = match remaining_length_result {
            Ok((len, bytes)) => (len, bytes),
            Err(PacketEncodingError::IncompletePacket { .. }) => {
                return Ok(None);
            }
            Err(e) => {
                error!("Invalid remaining length encoding: {}", e);
                return Err(BrokerError::PacketEncodingError { error: e });
            }
        };

        // Step 3: Calculate total packet size
        let total_packet_size = 1 + var_int_len + remaining_length;

        // Step 4: Check if we have complete packet
        if total_packet_size > rx_buffer.len() {
            return Ok(None);
        }

        // Step 5: Decode packet
        let packet_slice = &rx_buffer.as_slice()[..total_packet_size];
        let packet_result = Packet::decode(packet_slice);

        let packet = match packet_result {
            Ok(p) => p,
            Err(e) => {
                error!("Failed to decode packet: {}", e);
                return Err(BrokerError::PacketEncodingError { error: e });
            }
        };

        // Step 6: Remove decoded packet from buffer
        for _ in 0..total_packet_size {
            if !rx_buffer.is_empty() {
                rx_buffer.remove(0);
            }
        }

        info!("Decoded packet, {} bytes remaining in buffer", rx_buffer.len());

        Ok(Some(packet))
    }

    async fn process_client_messages(&mut self) -> Result<(), BrokerError> {
        // Process messages from clients and route them via the broker
        // Collect client IDs and packets to route, then process in a second pass
        let mut messages_to_route: [Option<(ClientId, Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>)>; QUEUE_SIZE] = [const { None }; QUEUE_SIZE];
        let mut route_count = 0usize;

        // Collect subscribe requests to process later (use tuples to avoid generic parameter issues)
        let mut subscribe_client_ids: [Option<ClientId>; 16] = [const { None }; 16];
        let mut subscribe_topic_filters: [Option<TopicName<MAX_TOPIC_NAME_LENGTH>>; 16] = [const { None }; 16];
        let mut subscribe_packet_ids: [Option<u16>; 16] = [const { None }; 16];
        let mut subscribe_qos: [Option<QoS>; 16] = [const { None }; 16];
        let mut subscribe_count = 0usize;

        for session_option in self.broker.sessions_mut().iter_mut() {
            if let Some(session) = session_option {
                while let Some(packet) = session.dequeue_rx_packet() {
                    info!("Processing packet from client {}: {:?}", session.client_id, packet);
                    match packet {
                        Packet::Connect(connect) => {
                            // Handle CONNECT packet
                            info!("Client {} connected with ID {:?}, keep-alive: {} seconds",
                                  session.client_id, connect.client_id, connect.keep_alive);

                            let old_client_id = session.client_id.clone();

                            // Update client_id if provided
                            if !connect.client_id.is_empty() {
                                let new_client_id = connect.client_id.clone();

                                // Update session
                                session.client_id = new_client_id.clone();

                                // Update streams map key
                                for (cid, _stream_option) in self.streams.iter_mut() {
                                    if *cid == old_client_id {
                                        *cid = new_client_id.clone();
                                        break;
                                    }
                                }

                                // Update rx_buffers map key
                                for buf in self.rx_buffers.iter_mut() {
                                    if buf.client_id == old_client_id {
                                        buf.client_id = new_client_id.clone();
                                        break;
                                    }
                                }

                                info!("Updated client ID from {} to {}", old_client_id, new_client_id);
                            }
                            // If client_id is empty, keep the server-generated ID (no changes needed)

                            session.state = ClientState::Connected;

                            // Extract and use keep_alive from CONNECT packet
                            session.keep_alive_secs = connect.keep_alive;
                            info!("Updated keep-alive for client {} to {} seconds",
                                  session.client_id, connect.keep_alive);

                            // Send CONNACK response
                            let connack = Packet::ConnAck(ConnAckPacket::default());
                            let _ = session.queue_tx_packet(connack);
                        }
                        Packet::ConnAck(_) => {}
                        Packet::Publish(publish) => {
                            // Handle PUBLISH packet
                            info!("Client {} published to topic {}: {:?}", session.client_id, publish.topic_name, publish.payload);

                            // Collect for routing after loop ends (avoid borrowing broker while sessions is borrowed)
                            if route_count < messages_to_route.len() {
                                messages_to_route[route_count] = Some((session.client_id.clone(), Packet::Publish(publish)));
                                route_count += 1;
                            }

                            // Send PUBACK if QoS 1
                            let publish_ref = if let Packet::Publish(ref p) = messages_to_route[route_count - 1].as_ref().unwrap().1 {
                                p
                            } else {
                                unreachable!()
                            };
                            if publish_ref.qos > QoS::AtMostOnce && publish_ref.packet_id.is_some() {
                                let mut puback = PubAckPacket::default();
                                puback.packet_id = publish_ref.packet_id.unwrap();
                                let _ = session.queue_tx_packet(Packet::PubAck(puback));
                            }
                        }
                        Packet::PubAck(_) => {}
                        Packet::PubRec(_) => {}
                        Packet::PubRel(_) => {}
                        Packet::PubComp(_) => {}
                        Packet::Subscribe(subscribe) => {
                            // Handle SUBSCRIBE packet - collect for later processing
                            info!("Client {} subscribing to topic {} with QoS {:?}",
                                  session.client_id, subscribe.topic_filter, subscribe.requested_qos);

                            // Collect subscribe request for processing after loop
                            if subscribe_count < subscribe_client_ids.len() {
                                subscribe_client_ids[subscribe_count] = Some(session.client_id.clone());
                                subscribe_topic_filters[subscribe_count] = Some(subscribe.topic_filter.clone());
                                subscribe_packet_ids[subscribe_count] = Some(subscribe.packet_id);
                                subscribe_qos[subscribe_count] = Some(subscribe.requested_qos);
                                subscribe_count += 1;
                            }
                        }
                        Packet::SubAck(_) => {}
                        Packet::Unsubscribe(_) => {}
                        Packet::UnsubAck(_) => {}
                        Packet::PingReq(_) => {
                            // Handle PINGREQ packet - respond with PINGRESP
                            info!("Client {} sent PINGREQ, sending PINGRESP", session.client_id);

                            // Create and queue PINGRESP packet
                            let pingresp = Packet::PingResp(PingRespPacket::default());
                            match session.queue_tx_packet(pingresp) {
                                Ok(()) => {
                                    info!("Queued PINGRESP for client {}", session.client_id);
                                }
                                Err(e) => {
                                    error!("Failed to queue PINGRESP for client {}: {}", session.client_id, e);
                                }
                            }
                        }
                        Packet::PingResp(_) => {}
                        Packet::Disconnect(_) => {
                            // Handle DISCONNECT packet
                            info!("Client {} disconnecting", session.client_id);
                            session.state = ClientState::Disconnected;
                            // Mark session for removal by adding it to removal list
                            // We'll handle this in the next iteration
                        }
                    }
                }
            }
        }

        // Route collected messages to subscribers (now sessions is no longer borrowed)
        for i in 0..route_count {
            if let Some((_publisher_id, packet)) = &messages_to_route[i] {
                if let Packet::Publish(publish) = packet {
                    // Get subscribers for this topic
                    let subscribers = self.broker.get_topic_subscribers(&publish.topic_name);

                    // Route to each subscriber
                    for subscriber_id in subscribers {
                        if let Some(target_session) = self.broker.find_session(&subscriber_id) {
                            let _ = target_session.queue_tx_packet(packet.clone());
                            info!("Routed message to subscriber {:?}", subscriber_id);
                        }
                    }
                }
            }
        }

        // Process collected subscribe requests (now sessions is no longer borrowed)
        for i in 0..subscribe_count {
            if let (Some(client_id), Some(topic_filter), Some(packet_id), Some(requested_qos)) = (
                &subscribe_client_ids[i],
                &subscribe_topic_filters[i],
                subscribe_packet_ids[i],
                subscribe_qos[i],
            ) {
                let topic_subscription = TopicSubscription::Exact(topic_filter.clone());
                match self.broker.subscribe_client(client_id.clone(), topic_subscription) {
                    Ok(()) => {
                        info!("Client {} successfully subscribed to {}",
                              client_id, topic_filter);

                        // Send SUBACK
                        if let Some(session) = self.broker.find_session(client_id) {
                            let suback = SubAckPacket {
                                packet_id,
                                granted_qos: requested_qos,
                            };
                            let _ = session.queue_tx_packet(Packet::SubAck(suback));
                        }
                    }
                    Err(e) => {
                        error!("Failed to subscribe client {} to topic {}: {}",
                               client_id, topic_filter, e);

                        // Send SUBACK with failure QoS
                        if let Some(session) = self.broker.find_session(client_id) {
                            let suback = SubAckPacket {
                                packet_id,
                                granted_qos: QoS::from_u8(0x80).unwrap_or(QoS::AtMostOnce),
                            };
                            let _ = session.queue_tx_packet(Packet::SubAck(suback));
                        }
                    }
                }
            }
        }

        Ok(())
    }

    async fn write_client_messages(&mut self) -> Result<(), BrokerError> {
        let mut sessions_to_remove: [Option<ClientId>; 16] = [const { None }; 16];
        let mut remove_count = 0usize;

        // Collect all packets to send with their client IDs
        let mut packets_to_send: [Option<(ClientId, Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>)>; 32] = [const { None }; 32];
        let mut packet_count = 0usize;

        // First collect client IDs
        let mut client_ids: [Option<ClientId>; 16] = [const { None }; 16];
        let mut client_count = 0usize;

        for session_option in self.broker.sessions().iter() {
            if let Some(session) = session_option {
                if client_count < client_ids.len() {
                    client_ids[client_count] = Some(session.client_id.clone());
                    client_count += 1;
                }
            }
        }

        // Then collect packets from each client
        for i in 0..client_count {
            if let Some(client_id) = &client_ids[i] {
                if let Some(session) = self.broker.find_session(client_id) {
                    while let Some(packet) = session.dequeue_tx_packet() {
                        if packet_count < packets_to_send.len() {
                            packets_to_send[packet_count] = Some((client_id.clone(), packet));
                            packet_count += 1;
                        }
                    }
                }
            }
        }

        // Send each packet
        for i in 0..packet_count {
            if let Some((client_id, packet)) = &packets_to_send[i] {
                info!("Sending packet to client {}: {:?}", client_id, packet);
                let mut buffer = [0u8; MAX_PAYLOAD_SIZE];
                let packet_size_result = packet.encode(&mut buffer);
                let packet_size = match packet_size_result {
                    Ok(encoded) => encoded,
                    Err(e) => {
                        error!("Error encoding packet for client {}: {}", client_id, e);
                        continue; // Skip sending this packet
                    }
                };
                let encoded_packet = &buffer[..packet_size];
                info!("Encoded packet: {} bytes", packet_size);

                // Get stream for this client
                let stream_exists = self.get_stream_mut(client_id.clone()).is_some();

                if !stream_exists {
                    error!("No stream found for client {}", client_id);
                    if let Some(session) = self.broker.find_session(client_id) {
                        session.state = ClientState::Disconnected;
                    }
                    if remove_count < sessions_to_remove.len() {
                        sessions_to_remove[remove_count] = Some(client_id.clone());
                        remove_count += 1;
                    }
                    continue;
                }

                // Write ALL bytes (handle partial writes)
                let mut total_written = 0usize;
                let mut should_disconnect = false;

                while total_written < packet_size && !should_disconnect {
                    let stream = self.get_stream_mut(client_id.clone());
                    if stream.is_none() {
                        error!("No stream found for client {}", client_id);
                        should_disconnect = true;
                        break;
                    }

                    let stream = stream.unwrap();

                    match stream.write(&encoded_packet[total_written..]).await {
                        Ok(0) => {
                            // Wrote 0 bytes = connection closed
                            error!("Write returned 0 bytes, connection closed");
                            should_disconnect = true;
                            break; // Exit write loop
                        }
                        Ok(bytes_written) => {
                            total_written += bytes_written;
                            info!("Wrote {} bytes (total: {})", bytes_written, total_written);

                            if total_written >= packet_size {
                                // All bytes written, now flush
                                break;
                            }
                        }
                        Err(e) => {
                            let is_fatal = matches!(e,
                                NetworkError::ConnectionClosed | NetworkError::IoError
                            );

                            if is_fatal {
                                error!("Fatal error writing to client {}: {}", client_id, e);
                                should_disconnect = true;
                            } else {
                                info!("Non-fatal write error, retrying: {}", e);
                                break;
                            }
                        }
                    }
                }

                if should_disconnect {
                    if let Some(session) = self.broker.find_session(client_id) {
                        session.state = ClientState::Disconnected;
                    }
                    if remove_count < sessions_to_remove.len() {
                        sessions_to_remove[remove_count] = Some(client_id.clone());
                        remove_count += 1;
                    }
                    continue;
                }

                // Only flush if write succeeded
                if total_written >= packet_size && !should_disconnect {
                    let stream = self.get_stream_mut(client_id.clone());
                    if let Some(stream) = stream {
                        match stream.flush().await {
                            Ok(()) => {
                                info!("Flushed stream for client {}", client_id);
                            }
                            Err(e) => {
                                error!("Error flushing stream: {}", e);
                                if let Some(session) = self.broker.find_session(client_id) {
                                    session.state = ClientState::Disconnected;
                                }
                                if remove_count < sessions_to_remove.len() {
                                    sessions_to_remove[remove_count] = Some(client_id.clone());
                                    remove_count += 1;
                                }
                            }
                        }
                    }
                }
            }
        }

        // Remove sessions that had fatal errors
        for i in 0..remove_count {
            if let Some(client_id) = &sessions_to_remove[i] {
                let _ = self.broker.remove_client(client_id);
                self.remove_rx_buffer(client_id);
                self.remove_stream(client_id);
            }
        }

        Ok(())
    }
}

/// Client read buffer
#[derive(Debug, Default, Clone)]
pub struct ClientReadBuffer<const MAX_PAYLOAD_SIZE: usize> {
    pub client_id: ClientId,
    buffer: HeaplessVec<u8, MAX_PAYLOAD_SIZE>,
    len: usize,
}

impl<const MAX_PAYLOAD_SIZE: usize> ClientReadBuffer<MAX_PAYLOAD_SIZE> {
    /// Create a new read buffer for a client
    pub fn new(client_id: ClientId) -> Self {
        Self {
            client_id,
            buffer: HeaplessVec::new(),
            len: 0,
        }
    }

    /// Append data to the buffer
    pub fn append(&mut self, data: &[u8]) {
        for &byte in data {
            if self.buffer.push(byte).is_ok() {
                self.len += 1;
            } else {
                break; // Buffer full
            }
        }
    }

    /// Get buffer as a slice
    pub fn as_slice(&self) -> &[u8] {
        self.buffer.as_slice()
    }

    /// Get buffer length
    pub fn len(&self) -> usize {
        self.len
    }

    /// Check if buffer is empty
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Remove element at index
    pub fn remove(&mut self, index: usize) {
        if index < self.len {
            self.buffer.remove(index);
            self.len -= 1;
        }
    }

    /// Push a single byte to the buffer
    pub fn push(&mut self, byte: u8) -> Result<(), ()> {
        match self.buffer.push(byte) {
            Ok(()) => {
                self.len += 1;
                Ok(())
            }
            Err(e) => Err(e)
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
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const QUEUE_SIZE: usize,
> {
    /// Client identifier
    pub client_id: ClientId,

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

impl<const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize, const QUEUE_SIZE: usize> ClientSession<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_SIZE>
{
    /// Create a new client session
    pub fn new(client_id: ClientId, keep_alive_secs: u16, current_time: u64) -> Self {
        Self {
            client_id,
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
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const QUEUE_SIZE: usize,
    const MAX_CLIENTS: usize,
    const MAX_TOPICS: usize,
    const MAX_SUBSCRIBERS_PER_TOPIC: usize,
> {
    /// Sessions with communication queues
    pub sessions: HeaplessVec<
        Option<ClientSession<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_SIZE>>,
        MAX_CLIENTS
    >,
}

impl<const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize, const QUEUE_SIZE: usize, const MAX_CLIENTS: usize, const MAX_TOPICS: usize, const MAX_SUBSCRIBERS_PER_TOPIC: usize>
ClientRegistry<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_SIZE, MAX_CLIENTS, MAX_TOPICS, MAX_SUBSCRIBERS_PER_TOPIC>
{
    /// Create a new client registry
    pub fn new() -> Self {
        Self {
            sessions: HeaplessVec::new(),
        }
    }

    /// Mark a client as disconnected
    pub fn mark_disconnected(&mut self, client_id: ClientId) {
        if let Some(session) = self.find_session_by_client_id(&client_id) {
            session.state = ClientState::Disconnected;
        }
    }

    /// Register a new client session
    pub fn register_new_client(&mut self, client_id: ClientId, keep_alive_secs: u16, current_time: u64) -> Result<(), BrokerError> {
        if self.sessions.len() >= MAX_CLIENTS {
            return Err(BrokerError::MaxClientsReached { max_clients: MAX_CLIENTS });
        }

        if self.find_session_by_client_id(&client_id).is_some() {
            return Err(BrokerError::ClientAlreadyConnected);
        }

        let session = ClientSession::new(client_id, keep_alive_secs, current_time);
        self.sessions.push(Some(session)).map_err(|_| BrokerError::MaxClientsReached { max_clients: MAX_CLIENTS })?;
        Ok(())
    }

    /// Find a mutable session reference by ClientId
    ///
    /// Returns None if session not found or client_id is not set.
    /// Uses linear search which is acceptable since MAX_CLIENTS is typically small (4-16).
    pub fn find_session_by_client_id(
        &mut self,
        client_id: &ClientId
    ) -> Option<&mut ClientSession<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_SIZE>> {
        self.sessions.iter_mut().find(|s_opt| {
            s_opt.as_ref()
                .map(|s| &s.client_id == client_id)
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
        client_id: &ClientId,
        topics: &mut crate::topics::TopicRegistry<MAX_TOPIC_NAME_LENGTH, MAX_TOPICS, MAX_SUBSCRIBERS_PER_TOPIC>
    ) -> Result<(), BrokerError> {
        // Find the session index
        let session_idx = self.sessions.iter().position(|s| {
            s.as_ref().map(|sess| &sess.client_id == client_id).unwrap_or(false)
        });

        if let Some(idx) = session_idx {
            // Extract the client_id before removing
            let client_id = self.sessions[idx]
                .as_ref()
                .map(|s| s.client_id.clone());

            // Cleanup subscriptions if client_id exists
            if let Some(id) = &client_id {
                topics.unregister_client(id.clone());
                info!("Cleaned up subscriptions for client {:?}", id);
            }

            // Remove from vector - replace with None and then shift
            // We can't use remove() because Option<ClientSession> doesn't implement Clone
            self.sessions[idx] = None;

            // Shift remaining elements to fill the gap
            for i in idx..self.sessions.len() - 1 {
                self.sessions[i] = self.sessions[i + 1].take();
            }

            // Remove the last element which is now a duplicate
            let _ = self.sessions.pop();

            Ok(())
        } else {
            error!("Session {} not found for removal", client_id);
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
        topics: &mut crate::topics::TopicRegistry<MAX_TOPIC_NAME_LENGTH, MAX_TOPICS, MAX_SUBSCRIBERS_PER_TOPIC>,
        current_time: u64
    ) {
        // Use a simple array to collect expired session IDs
        let mut expired_ids = [const { None }; 16]; // Stack-allocated, reasonable max
        let mut expired_count = 0usize;

        // Collect expired session IDs
        for session in &self.sessions {
            if let Some(sess) = session {
                if sess.is_expired(current_time) {
                    expired_ids[expired_count] = Some(sess.client_id.clone());
                    expired_count += 1;
                }
            }
        }

        // Remove each expired session
        for i in 0..expired_count {
            if let Some(session_id) = &expired_ids[i] {
                info!("Removing expired session {}", session_id);
                let _ = self.remove_session(session_id, topics);
            }
        }
    }

    /// Cleanup zombie sessions (connections that have been closed but not yet removed)
    ///
    /// This is called before accepting new connections to ensure zombie slots are freed.
    /// It checks for dead connections by attempting to detect socket errors.
    pub fn cleanup_zombie_sessions(
        &mut self,
        topics: &mut crate::topics::TopicRegistry<MAX_TOPIC_NAME_LENGTH, MAX_TOPICS, MAX_SUBSCRIBERS_PER_TOPIC>
    ) {
        let mut zombie_ids = [const { None }; MAX_CLIENTS];
        let mut zombie_count = 0usize;

        // Check each session for dead connections
        for session in &self.sessions {
            if let Some(sess) = session {
                // Check if session state is Disconnected
                if sess.state == ClientState::Disconnected {
                    if zombie_count < zombie_ids.len() {
                        zombie_ids[zombie_count] = Some(sess.client_id.clone());
                        zombie_count += 1;
                    }
                }
            }
        }

        // Remove each zombie session
        for i in 0..zombie_count {
            if let Some(client_id) = &zombie_ids[i] {
                info!("Removing zombie session {}", client_id);
                let _ = self.remove_session(client_id, topics);
            }
        }
    }
}