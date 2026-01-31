use log::{debug, error, info, warn};
use picobroker_core::broker::PicoBroker;
use picobroker_core::protocol::packets::{Packet, PacketEncoder};
use picobroker_core::protocol::utils;
use picobroker_core::protocol::ProtocolError;
use std::collections::HashMap;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::sync::Mutex;
use tokio::task::JoinSet;
use tokio::time::{sleep, Duration};

// Broker configuration - sensible defaults for an example
const MAX_TOPIC_NAME_LENGTH: usize = 64;
const MAX_PAYLOAD_SIZE: usize = 256;
const QUEUE_SIZE: usize = 8;
const MAX_SESSIONS: usize = 16;
const MAX_TOPICS: usize = 32;
const MAX_SUBSCRIBERS_PER_TOPIC: usize = 8;

// Type alias for the broker
type Broker = PicoBroker<
    MAX_TOPIC_NAME_LENGTH,
    MAX_PAYLOAD_SIZE,
    QUEUE_SIZE,
    MAX_SESSIONS,
    MAX_TOPICS,
    MAX_SUBSCRIBERS_PER_TOPIC,
>;

/// Shared state for the broker and client connections
struct ServerState {
    broker: Broker,
    clients: HashMap<u128, ClientState>,
}

struct ClientState {
    _peer_addr: String,
}

impl ServerState {
    fn new() -> Self {
        Self {
            broker: PicoBroker::new(),
            clients: HashMap::new(),
        }
    }

    fn current_time_nanos(&self) -> u128 {
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    }

    fn cleanup_sessions(&mut self) {
        let current_time = self.current_time_nanos();

        // Clean up expired sessions
        for session_id in self
            .broker
            .get_expired_sessions(current_time)
            .into_iter()
            .flatten()
        {
            info!("Cleaning up expired session {}", session_id);
            self.remove_session(session_id);
        }

        // Clean up disconnected sessions
        for session_id in self
            .broker
            .get_disconnected_sessions()
            .into_iter()
            .flatten()
        {
            info!("Cleaning up disconnected session {}", session_id);
            self.remove_session(session_id);
        }
    }

    fn remove_session(&mut self, session_id: u128) {
        if self.broker.remove_session(session_id) {
            info!("Removed session {}", session_id);
        }
        self.clients.remove(&session_id);
    }
}

/// Decode MQTT variable-length integer from a stream
async fn decode_variable_length<R: AsyncReadExt + Unpin>(
    reader: &mut R,
) -> Result<usize, ProtocolError> {
    let mut value: usize = 0;
    let mut multiplier: usize = 1;
    let mut encoded_byte;

    loop {
        let mut buf = [0u8; 1];
        reader
            .read_exact(&mut buf)
            .await
            .map_err(|_| ProtocolError::IncompletePacket { available: 0 })?;
        encoded_byte = buf[0];

        value += ((encoded_byte & 0x7F) as usize) * multiplier;

        if (encoded_byte & 0x80) == 0 {
            break;
        }

        multiplier *= 128;

        if multiplier > 128 * 128 * 128 {
            return Err(ProtocolError::InvalidPacketLength {
                expected: 4,
                actual: 5,
            });
        }
    }

    Ok(value)
}

/// Read a complete MQTT packet from the stream
async fn read_mqtt_packet(
    socket: &mut TcpStream,
    buffer: &mut Vec<u8>,
) -> Result<Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>, ProtocolError> {
    // Read first byte (packet type + flags)
    let mut first_byte = [0u8; 1];
    socket
        .read_exact(&mut first_byte)
        .await
        .map_err(|_| ProtocolError::IncompletePacket { available: 0 })?;

    buffer.clear();
    buffer.push(first_byte[0]);

    // Decode remaining length
    let remaining_length = decode_variable_length(socket).await?;

    // Encode the remaining length into the buffer (for Packet::decode)
    let mut length_bytes = [0u8; 4];
    let length_len = utils::write_variable_length(remaining_length, &mut length_bytes)?;
    buffer.extend_from_slice(&length_bytes[..length_len]);

    // Read remaining payload
    if remaining_length > 0 {
        buffer.resize(buffer.len() + remaining_length, 0);
        socket
            .read_exact(&mut buffer[length_len + 1..])
            .await
            .map_err(|_| ProtocolError::IncompletePacket { available: 0 })?;
    }

    // Decode the packet
    Packet::decode(buffer)
}

/// Encode and write an MQTT packet to the stream
async fn write_mqtt_packet(
    socket: &mut TcpStream,
    packet: &Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>,
) -> Result<(), std::io::Error> {
    let mut buffer = [0u8; MAX_PAYLOAD_SIZE + 16];
    let size = packet
        .encode(&mut buffer)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "Failed to encode packet"))?;
    socket.write_all(&buffer[..size]).await?;
    socket.flush().await?;
    Ok(())
}

/// Handle a single client connection
async fn handle_client(
    mut socket: TcpStream,
    peer_addr: String,
    state: Arc<Mutex<ServerState>>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let session_id = {
        let mut state_lock = state.lock().await;
        let current_time = state_lock.current_time_nanos();

        // Generate a unique session ID from timestamp and a simple hash of address
        let addr_hash = peer_addr
            .split(':')
            .next()
            .and_then(|s| s.parse::<u32>().ok())
            .unwrap_or(0) as u128;
        let session_id = current_time ^ addr_hash;

        // Register session with broker (60 second keep-alive default)
        match state_lock
            .broker
            .register_new_session(session_id, 60, current_time)
        {
            Ok(_) => {
                info!("Registered new session {} for {}", session_id, peer_addr);
                state_lock.clients.insert(
                    session_id,
                    ClientState {
                        _peer_addr: peer_addr.clone(),
                    },
                );
                session_id
            }
            Err(e) => {
                error!("Failed to register session for {}: {}", peer_addr, e);
                return Err(Box::new(e));
            }
        }
    };

    let mut read_buffer = Vec::new();

    loop {
        // Check if session was removed (e.g., by cleanup)
        {
            let state_lock = state.lock().await;
            if !state_lock.clients.contains_key(&session_id) {
                warn!("Session {} was removed, closing connection", session_id);
                return Ok(());
            }
        }

        // Try to read a packet with timeout
        let packet_result = tokio::time::timeout(
            Duration::from_secs(1),
            read_mqtt_packet(&mut socket, &mut read_buffer),
        )
        .await;

        match packet_result {
            Ok(Ok(packet)) => {
                // Log the packet type at debug level
                let packet_type = match &packet {
                    Packet::Connect(_) => "CONNECT",
                    Packet::ConnAck(_) => "CONNACK",
                    Packet::Publish(_) => "PUBLISH",
                    Packet::PubAck(_) => "PUBACK",
                    Packet::PubRec(_) => "PUBREC",
                    Packet::PubRel(_) => "PUBREL",
                    Packet::PubComp(_) => "PUBCOMP",
                    Packet::Subscribe(_) => "SUBSCRIBE",
                    Packet::SubAck(_) => "SUBACK",
                    Packet::Unsubscribe(_) => "UNSUBSCRIBE",
                    Packet::UnsubAck(_) => "UNSUBACK",
                    Packet::PingReq(_) => "PINGREQ",
                    Packet::PingResp(_) => "PINGRESP",
                    Packet::Disconnect(_) => "DISCONNECT",
                };
                debug!("Session {}: Received {}", session_id, packet_type);

                // Queue packet to broker and process
                let mut state_lock = state.lock().await;
                let current_time = state_lock.current_time_nanos();

                if let Err(e) = state_lock
                    .broker
                    .queue_packet_received_from_client(session_id, packet, current_time)
                {
                    error!(
                        "Session {}: Error queuing packet from client: {}",
                        session_id, e
                    );
                    // Mark as disconnected and break
                    let _ = state_lock.broker.mark_session_disconnected(session_id);
                    break;
                }

                if let Err(e) = state_lock.broker.process_all_session_packets() {
                    error!(
                        "Session {}: Error processing packets: {}",
                        session_id, e
                    );
                }

                // Send all queued packets to client
                while let Ok(Some(packet)) = state_lock
                    .broker
                    .dequeue_packet_to_send_to_client(session_id)
                {
                    if let Err(e) = write_mqtt_packet(&mut socket, &packet).await {
                        error!("Session {}: Error writing packet: {}", session_id, e);
                        let _ = state_lock.broker.mark_session_disconnected(session_id);
                        return Err(Box::new(e));
                    }
                }
            }
            Ok(Err(e)) => {
                // Protocol error - disconnect client
                warn!("Session {}: Protocol error: {}, disconnecting", session_id, e);
                let mut state_lock = state.lock().await;
                let _ = state_lock.broker.mark_session_disconnected(session_id);
                break;
            }
            Err(_) => {
                // Timeout - continue loop to check for session cleanup
            }
        }
    }

    info!("Session {} connection closed", session_id);
    Ok(())
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logger
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .init();

    let bind_addr = "0.0.0.0:1883";
    info!("Starting picobroker tokio server on {}", bind_addr);

    let listener = TcpListener::bind(bind_addr).await?;
    info!("Server listening on {}", bind_addr);

    let state = Arc::new(Mutex::new(ServerState::new()));
    let mut tasks: JoinSet<Result<(), Box<dyn std::error::Error + Send + Sync>>> = JoinSet::new();

    // Spawn cleanup task
    let state_clone = state.clone();
    tasks.spawn(async move {
        loop {
            sleep(Duration::from_secs(5)).await;
            let mut state_lock = state_clone.lock().await;
            state_lock.cleanup_sessions();
        }
    });

    // Accept connections loop
    loop {
        // Accept new connection
        match listener.accept().await {
            Ok((socket, addr)) => {
                let peer_addr = addr.to_string();
                info!("New connection from {}", peer_addr);

                let state = state.clone();
                tasks.spawn(async move {
                    if let Err(e) = handle_client(socket, peer_addr, state).await {
                        error!("Client handler error: {}", e);
                    }
                    Ok(())
                });
            }
            Err(e) => {
                error!("Error accepting connection: {}", e);
            }
        }

        // Clean up completed tasks
        while let Some(result) = tasks.try_join_next() {
            if let Err(e) = result {
                error!("Task error: {}", e);
            }
        }
    }
}
