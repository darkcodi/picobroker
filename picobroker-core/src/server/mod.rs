//! MQTT broker server implementation
//!
//! This module provides the main server implementation with connection handling,
//! message processing, and client lifecycle management.

use crate::broker::PicoBroker;
use crate::broker_error::BrokerError;
use crate::client::ClientId;
use crate::protocol::packets::{Packet, PacketEncoder};
use crate::protocol::packet_error::PacketEncodingError;
use crate::traits::{Delay, NetworkError, TcpListener, TcpStream, TimeSource};
use log::{error, info, warn};

// Submodules
pub mod buffer_manager;
pub mod connection_manager;

// Re-export public types
pub use buffer_manager::{BufferManager, ClientReadBuffer};
pub use connection_manager::ConnectionManager;

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
    connection_manager: ConnectionManager<TL, MAX_CLIENTS>,
    buffer_manager: BufferManager<MAX_PAYLOAD_SIZE, MAX_CLIENTS>,
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
            connection_manager: ConnectionManager::new(),
            buffer_manager: BufferManager::new(),
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
            // Phase 1: Accept new connections
            self.accept_new_connection().await;

            // Phase 2: Read from all clients
            self.read_client_messages().await;

            // Phase 3: Process packets through broker
            self.process_client_messages().await;

            // Phase 4: Write to all clients
            self.write_client_messages().await;

            // Phase 5: Small yield to prevent busy-waiting
            self.delay.sleep_ms(10).await;
        }
    }

    /// Cleanup all disconnected and expired clients
    pub fn cleanup_disconnected(&mut self) {
        // Remove disconnected clients
        for client_id in self.broker.get_disconnected_clients().iter().flatten() {
            self.remove_client(client_id);
        }

        // Remove expired clients
        let current_time = self.time_source.now_secs();
        for client_id in self.broker.get_expired_clients(current_time).iter().flatten() {
            self.remove_client(client_id);
        }
    }

    pub fn remove_client(&mut self, client_id: &ClientId) {
        match self.broker.remove_client(client_id) {
            true => info!("Removed client {}", client_id),
            false => error!("No client found to remove with id {}", client_id),
        }
        match self.connection_manager.remove_client(client_id) {
            true => info!("Removed connection for client {}", client_id),
            false => error!("No connection found to remove for client {}", client_id),
        }
        match self.buffer_manager.remove_client(client_id) {
            true => info!("Removed buffer for client {}", client_id),
            false => error!("No buffer found to remove for client {}", client_id),
        }
    }

    /// Accept a new connection if one is pending
    async fn accept_new_connection(&mut self)
    where
        TL::Stream: Send + 'static,
    {
        self.cleanup_disconnected();
        if let Ok((mut stream, socket_addr)) = self.listener.try_accept().await {
            info!("Received new connection from {}", socket_addr);

            let time = self.time_source.now_secs();
            let client_id = ClientId::generate(time);
            const KEEP_ALIVE_SECS: u16 = 60; // Default keep-alive
            let current_time = self.time_source.now_secs();

            match self
                .broker
                .register_client(client_id.clone(), KEEP_ALIVE_SECS, current_time)
            {
                Ok(_) => {
                    info!("Registered new client with client id {}", client_id);
                    self.connection_manager.set_stream(client_id, stream);
                }
                Err(e) => {
                    error!("Failed to register new client: {}", e);
                    error!("Closing connection");
                    let _ = stream.close().await;
                }
            }
        }
    }

    /// Read messages from all connected clients
    async fn read_client_messages(&mut self)
    where
        TL::Stream: Send + 'static,
    {
        self.cleanup_disconnected();
        for client_id in self.broker.get_all_clients().iter().flatten() {
            if let Err(e) = self.read_single_client(client_id).await {
                match e {
                    ReadError::StreamNotFound => {
                        error!("Stream not found for client {}", client_id);
                    }
                    ReadError::ConnectionClosed => {
                        info!("Client {} disconnected", client_id);
                    }
                    ReadError::Fatal(e) => {
                        info!("Client {} forcefully disconnected due to network fatal error: {:?}", client_id, e);
                    }
                }
            }
        }
    }

    /// Read messages from a single client
    async fn read_single_client(&mut self, client_id: &ClientId) -> Result<(), ReadError>
    where
        TL::Stream: Send + 'static,
    {
        self.ensure_buffer_space(client_id)
            .map_err(|_| ReadError::StreamNotFound)?;

        let remaining_space = self.buffer_manager.get_remaining_space(client_id);
        let mut read_buf = [0u8; MAX_PAYLOAD_SIZE];

        // Get stream, read data, and release stream borrow before processing
        let bytes_read = {
            let stream = match self
                .connection_manager
                .get_stream_mut(client_id) {
                Some(s) => s,
                None => {
                    error!("No stream found for client {}", client_id);
                    let _ = self.broker.mark_client_disconnected(client_id);
                    return Err(ReadError::StreamNotFound);
                }
            };

            match stream.try_read(&mut read_buf[..remaining_space]).await {
                Ok(0) => {
                    info!("Connection closed by peer (0 bytes read)");
                    let _ = self.broker.mark_client_disconnected(client_id);
                    return Err(ReadError::ConnectionClosed);
                }
                Ok(n) => {
                    info!("Read {} bytes from client {}", n, client_id);
                    n
                }
                Err(e) if matches!(e, NetworkError::ConnectionClosed | NetworkError::IoError) => {
                    error!("Fatal error reading from client {}: {}", client_id, e);
                    let _ = self.broker.mark_client_disconnected(client_id);
                    return Err(ReadError::Fatal(e));
                }
                Err(_) => 0,
            }
        };

        if bytes_read > 0 {
            let buffer = self.buffer_manager.get_buffer_mut(client_id);
            buffer.append(&read_buf[..bytes_read]);
            self.process_received_data(client_id)
                .map_err(|_| ReadError::StreamNotFound)?;
        }

        Ok(())
    }

    /// Ensure the client's buffer has space for new data
    fn ensure_buffer_space(&mut self, client_id: &ClientId) -> Result<(), BrokerError> {
        let remaining = self.buffer_manager.get_remaining_space(client_id);
        if remaining == 0 {
            warn!(
                "RX buffer overflow for client {}, clearing buffer",
                client_id
            );
            let buffer = self.buffer_manager.get_buffer_mut(client_id);
            buffer.clear();
        }
        Ok(())
    }

    /// Process received data and try to decode packets
    fn process_received_data(&mut self, client_id: &ClientId) -> Result<(), BrokerError> {
        let current_time = self.time_source.now_secs();

        match self
            .buffer_manager
            .try_decode_packet::<MAX_TOPIC_NAME_LENGTH>(client_id)
        {
            Ok(Some(packet)) => {
                info!("Received packet from client {}: {:?}", client_id, packet);
                self.broker.queue_packet_received_from_client(
                    client_id,
                    packet,
                    current_time,
                )?;
            }
            Ok(None) => {
                // No complete packet yet, that's ok
            }
            Err(e) => {
                error!("Error decoding packet from client {}: {}", client_id, e);
            }
        }

        Ok(())
    }

    /// Process all client messages through the broker
    async fn process_client_messages(&mut self)
    where
        TL::Stream: Send + 'static,
    {
        self.cleanup_disconnected();
        match self.broker.process_all_client_packets() {
            Ok(_) => {}
            Err(e) => {
                warn!("Error processing client messages: {:?}", e);
            }
        }
    }

    /// Write messages to all connected clients
    async fn write_client_messages(&mut self)
    where
        TL::Stream: Send + 'static,
    {
        self.cleanup_disconnected();
        for client_id in self.broker.get_all_clients().iter().flatten() {
            if let Err(e) = self.write_client_queue(client_id).await {
                warn!("Error writing to client {}: {:?}", client_id, e);
            }
        }
    }

    /// Write all queued messages to a single client
    async fn write_client_queue(&mut self, client_id: &ClientId) -> Result<(), BrokerError>
    where
        TL::Stream: Send + 'static,
    {
        while let Some(packet) = self
            .broker
            .dequeue_packet_to_send_to_client(client_id)?
        {
            if let Err(e) = self.write_single_packet(client_id, packet).await {
                warn!("Error writing packet to client {}: {:?}", client_id, e);
                break;
            }
        }
        Ok(())
    }

    /// Write a single packet to a client
    async fn write_single_packet(
        &mut self,
        client_id: &ClientId,
        packet: Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>,
    ) -> Result<(), WriteError>
    where
        TL::Stream: Send + 'static,
    {
        info!("Sending packet to client {}: {:?}", client_id, packet);

        // Encode packet
        let mut buffer = [0u8; MAX_PAYLOAD_SIZE];
        let packet_size = packet.encode(&mut buffer).map_err(|e| {
            error!("Error encoding packet: {}", e);
            WriteError::EncodeError(e)
        })?;
        let encoded_packet = &buffer[..packet_size];
        info!("Encoded packet: {} bytes", packet_size);

        // Get stream and write all bytes (handle partial writes)
        let mut total_written = 0;

        while total_written < packet_size {
            let stream = self
                .connection_manager
                .get_stream_mut(client_id)
                .ok_or_else(|| {
                    error!("No stream found for client {}", client_id);
                    let _ = self.broker.mark_client_disconnected(client_id);
                    WriteError::StreamNotFound
                })?;

            match stream.write(&encoded_packet[total_written..]).await {
                Ok(0) => {
                    error!("Write returned 0 bytes, connection closed");
                    let _ = self.broker.mark_client_disconnected(client_id);
                    return Err(WriteError::ConnectionClosed);
                }
                Ok(bytes_written) => {
                    total_written += bytes_written;
                    info!("Wrote {} bytes (total: {})", bytes_written, total_written);

                    if total_written >= packet_size {
                        // Flush after writing all bytes
                        let stream = self
                            .connection_manager
                            .get_stream_mut(client_id)
                            .ok_or(WriteError::StreamNotFound)?;

                        stream.flush().await.map_err(|e| {
                            error!("Error flushing stream: {}", e);
                            let _ = self.broker.mark_client_disconnected(client_id);
                            WriteError::FlushError(e)
                        })?;
                        break;
                    }
                }
                Err(e) if matches!(e, NetworkError::ConnectionClosed | NetworkError::IoError) => {
                    error!("Fatal error writing to client {}: {}", client_id, e);
                    let _ = self.broker.mark_client_disconnected(client_id);
                    return Err(WriteError::Fatal(e));
                }
                Err(_) => {
                    // Retry for non-fatal errors
                    continue;
                }
            }
        }

        Ok(())
    }
}

/// Error type for read operations
#[derive(Debug)]
enum ReadError {
    StreamNotFound,
    ConnectionClosed,
    Fatal(NetworkError),
}

/// Error type for write operations
#[derive(Debug)]
enum WriteError {
    StreamNotFound,
    EncodeError(PacketEncodingError),
    FlushError(NetworkError),
    Fatal(NetworkError),
    ConnectionClosed,
}
