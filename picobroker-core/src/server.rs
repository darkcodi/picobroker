use crate::broker::PicoBroker;
use crate::broker_error::BrokerError;
use crate::client::ClientId;
use crate::protocol::heapless::HeaplessVec;
use crate::protocol::packet_error::PacketEncodingError;
use crate::protocol::packets::{Packet, PacketEncoder};
use crate::protocol::utils::read_variable_length;
use crate::traits::{Delay, NetworkError, TcpListener, TcpStream, TimeSource};
use log::{error, info, warn};

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
    broker: PicoBroker<
        MAX_TOPIC_NAME_LENGTH,
        MAX_PAYLOAD_SIZE,
        QUEUE_SIZE,
        MAX_CLIENTS,
        MAX_TOPICS,
        MAX_SUBSCRIBERS_PER_TOPIC,
    >,
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
    >
    PicoBrokerServer<
        TS,
        TL,
        D,
        MAX_TOPIC_NAME_LENGTH,
        MAX_PAYLOAD_SIZE,
        QUEUE_SIZE,
        MAX_CLIENTS,
        MAX_TOPICS,
        MAX_SUBSCRIBERS_PER_TOPIC,
    >
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
    /// The server loop runs indefinitely, handling all client connections and message routing.
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

                // Proactively cleanup any dead/zombie sessions before accepting new connection
                self.remove_disconnected_clients();

                let time = self.time_source.now_secs();
                let client_id = ClientId::generate(time);
                const KEEP_ALIVE_SECS: u16 = 60; // Default non-configurable keep-alive for new clients
                let current_time = self.time_source.now_secs();
                match self
                    .broker
                    .register_client(client_id.clone(), KEEP_ALIVE_SECS, current_time)
                {
                    Ok(_) => {
                        info!("Registered new client with client id {}", client_id);
                        self.set_stream(client_id, stream);
                    }
                    Err(e) => {
                        error!("Failed to register new client: {}", e);
                        error!("Closing connection");
                        let _ = stream.close().await;
                    }
                };
            }

            // 2. Process client messages (round-robin through all sessions)
            self.remove_disconnected_clients();
            self.read_client_messages().await?;

            // 3. Process messages from clients and route them via the broker
            self.remove_disconnected_clients();
            let packets_processed = self.broker.process_all_client_packets()?;
            if packets_processed > 0 {
                info!("Processed {} packets from clients", packets_processed);
            }

            // 4. Write messages to clients
            self.remove_disconnected_clients();
            self.write_client_messages().await?;

            // 5. Small yield to prevent busy-waiting
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

    fn get_or_create_rx_buffer(
        &mut self,
        client_id: &ClientId,
    ) -> &mut ClientReadBuffer<MAX_PAYLOAD_SIZE> {
        // Check if buffer exists
        let idx = self
            .rx_buffers
            .iter()
            .position(|b| &b.client_id == client_id);
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

    fn get_rx_buffer_remaining_space(&mut self, client_id: &ClientId) -> usize {
        let rx_buffer = self.get_or_create_rx_buffer(client_id);
        MAX_PAYLOAD_SIZE - rx_buffer.len()
    }

    fn remove_disconnected_clients(&mut self) {
        for client_id in self.broker.get_disconnected_clients().iter().flatten() {
            self.remove_client(client_id);
        }

        let current_time = self.time_source.now_secs();
        for client_id in self
            .broker
            .get_expired_clients(current_time)
            .iter()
            .flatten()
        {
            self.remove_client(client_id);
        }
    }

    fn remove_client(&mut self, client_id: &ClientId) {
        let _ = self.broker.remove_client(client_id);

        // Remove RX buffer
        let idx = self
            .rx_buffers
            .iter()
            .position(|b| b.client_id == *client_id);
        if let Some(idx) = idx {
            self.rx_buffers.remove(idx);
        }

        // Find the stream and set it to None instead of removing
        for (cid, stream_option) in self.streams.iter_mut() {
            if *cid == *client_id {
                *stream_option = None;
                return;
            }
        }
    }

    async fn read_client_messages(&mut self) -> Result<(), BrokerError> {
        for client_id in self.broker.get_all_clients().iter().flatten() {
            let mut remaining_space = self.get_rx_buffer_remaining_space(client_id);
            if remaining_space == 0 {
                warn!(
                    "RX buffer overflow for client {}, clearing buffer",
                    client_id
                );
                let rx_buffer = self.get_or_create_rx_buffer(client_id);
                rx_buffer.clear();
                remaining_space = self.get_rx_buffer_remaining_space(client_id);
            }

            let mut read_buf = [0u8; MAX_PAYLOAD_SIZE];

            // Get stream and read
            let stream = match self.get_stream_mut(client_id.clone()) {
                Some(s) => s,
                None => {
                    error!("No stream found for client {}", client_id);
                    let _ = self.broker.mark_client_disconnected(client_id);
                    continue;
                }
            };
            let read_result = stream.try_read(&mut read_buf[..remaining_space]).await;
            match read_result {
                Ok(0) => {
                    info!("Connection closed by peer (0 bytes read)");
                    let _ = self.broker.mark_client_disconnected(client_id);
                    continue;
                }
                Ok(n) => {
                    let data = &read_buf[..n];
                    info!("Read {} bytes from client {}", n, client_id);
                    let rx_buffer = self.get_or_create_rx_buffer(client_id);
                    rx_buffer.append(data);

                    let packet_result = Self::try_decode_from_buffer(rx_buffer);

                    match packet_result {
                        Ok(Some(packet)) => {
                            info!("Received packet from client {}: {:?}", client_id, packet);
                            // Use new broker API to update activity and queue packet
                            let current_time = self.time_source.now_secs();
                            let _ = self.broker.queue_packet_received_from_client(
                                client_id,
                                packet,
                                current_time,
                            );
                        }
                        Ok(None) => {
                            // No complete packet available, continue
                        }
                        Err(e) => {
                            error!("Error decoding packet from client {}: {}", client_id, e);
                        }
                    }
                }
                Err(e) if matches!(e, NetworkError::ConnectionClosed | NetworkError::IoError) => {
                    error!("Fatal error reading from client {}: {}", client_id, e);
                    let _ = self.broker.mark_client_disconnected(client_id);
                    break;
                }
                Err(e) if matches!(e, NetworkError::TimedOut | NetworkError::Interrupted) => {
                    warn!("Read timeout/interrupted for client {}: {}", client_id, e);
                    break;
                }
                Err(_) => {
                    // no data available or non-fatal error
                }
            }
        }

        Ok(())
    }

    /// Try to decode a packet from a RX buffer
    fn try_decode_from_buffer(
        rx_buffer: &mut ClientReadBuffer<MAX_PAYLOAD_SIZE>,
    ) -> Result<Option<Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>>, PacketEncodingError> {
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
                return Err(e);
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
                return Err(e);
            }
        };

        // Step 6: Remove decoded packet from buffer
        for _ in 0..total_packet_size {
            if !rx_buffer.is_empty() {
                rx_buffer.remove(0);
            }
        }

        info!(
            "Decoded packet, {} bytes remaining in buffer",
            rx_buffer.len()
        );

        Ok(Some(packet))
    }

    async fn write_client_messages(&mut self) -> Result<(), BrokerError> {
        for client_id in self.broker.get_all_clients().iter().flatten() {
            while let Ok(Some(packet)) = self.broker.dequeue_packet_to_send_to_client(client_id) {
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

                let stream = match self.get_stream_mut(client_id.clone()) {
                    Some(s) => s,
                    None => {
                        error!("No stream found for client {}", client_id);
                        let _ = self.broker.mark_client_disconnected(client_id);
                        break;
                    }
                };

                // Write ALL bytes (handle partial writes)
                let mut total_written = 0usize;

                while total_written < packet_size {
                    match stream.write(&encoded_packet[total_written..]).await {
                        Ok(0) => {
                            error!("Write returned 0 bytes, connection closed");
                            let _ = self.broker.mark_client_disconnected(client_id);
                            break;
                        }
                        Ok(bytes_written) => {
                            total_written += bytes_written;
                            info!("Wrote {} bytes (total: {})", bytes_written, total_written);

                            if total_written >= packet_size {
                                match stream.flush().await {
                                    Ok(()) => {
                                        info!("Flushed stream for client {}", client_id);
                                    }
                                    Err(e) => {
                                        error!("Error flushing stream: {}", e);
                                        let _ = self.broker.mark_client_disconnected(client_id);
                                    }
                                }
                                break;
                            }
                        }
                        Err(e)
                            if matches!(
                                e,
                                NetworkError::ConnectionClosed | NetworkError::IoError
                            ) =>
                        {
                            error!("Fatal error writing to client {}: {}", client_id, e);
                            let _ = self.broker.mark_client_disconnected(client_id);
                            break;
                        }
                        Err(e)
                            if matches!(e, NetworkError::TimedOut | NetworkError::Interrupted) =>
                        {
                            warn!("Write timeout/interrupted for client {}: {}", client_id, e);
                            break;
                        }
                        Err(e) => {
                            info!("Non-fatal write error, retrying: {}", e);
                        }
                    }
                }
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
    pub fn push(&mut self, byte: u8) -> Result<(), crate::protocol::heapless::PushError> {
        self.buffer.push(byte)?;
        self.len += 1;
        Ok(())
    }

    /// Clear the buffer
    pub fn clear(&mut self) {
        self.buffer.clear();
        self.len = 0;
    }
}
