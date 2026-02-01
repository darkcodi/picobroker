use bytes::{Buf, BytesMut};
use log::{debug, error, info, trace, warn};
use picobroker_core::broker::PicoBroker;
use picobroker_core::protocol::packets::{Packet, PacketEncoder};
use picobroker_core::protocol::ProtocolError;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration, Instant};

// =============================================================================
// Configuration
// =============================================================================

const MAX_TOPIC_NAME_LENGTH: usize = 64;
const MAX_PAYLOAD_SIZE: usize = 256;
const QUEUE_SIZE: usize = 8;
const MAX_SESSIONS: usize = 4;
const MAX_TOPICS: usize = 32;
const MAX_SUBSCRIBERS_PER_TOPIC: usize = 8;

// Channel capacities
const FRAME_CHANNEL_CAPACITY: usize = 32;

// Default keep-alive (seconds)
const DEFAULT_KEEP_ALIVE: u16 = 60;

// =============================================================================
// Type Aliases
// =============================================================================

type SharedBroker = Arc<tokio::sync::Mutex<PicoBroker<
    MAX_TOPIC_NAME_LENGTH,
    MAX_PAYLOAD_SIZE,
    QUEUE_SIZE,
    MAX_SESSIONS,
    MAX_TOPICS,
    MAX_SUBSCRIBERS_PER_TOPIC,
>>>;

// =============================================================================
// Server State
// =============================================================================

struct ServerState {
    connections: dashmap::DashMap<u128, ConnectionHandle>,
    session_id_gen: AtomicU64,
    notification_senders: dashmap::DashMap<u128, tokio::sync::mpsc::Sender<()>>,
}

#[allow(dead_code)]
struct ConnectionHandle {
    session_id: u128,
    peer_addr: String,
}

impl ServerState {
    fn new() -> Self {
        Self {
            connections: dashmap::DashMap::new(),
            session_id_gen: AtomicU64::new(0),
            notification_senders: dashmap::DashMap::new(),
        }
    }

    fn register_notification(&self, session_id: u128, sender: tokio::sync::mpsc::Sender<()>) {
        self.notification_senders.insert(session_id, sender);
    }

    fn remove_notification(&self, session_id: u128) {
        self.notification_senders.remove(&session_id);
    }

    async fn notify_session(&self, session_id: u128) {
        if let Some(sender) = self.notification_senders.get(&session_id) {
            let _ = sender.send(()).await;
        }
    }

    fn generate_session_id(&self) -> u128 {
        let base = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs() as u128;
        let counter = self.session_id_gen.fetch_add(1, Ordering::Relaxed) as u128;
        (base << 32) | (counter as u128)
    }
}

fn current_time_nanos() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos()
}

// =============================================================================
// Packet I/O
// =============================================================================

/// Parse variable length integer from BytesMut
fn parse_remaining_length(buffer: &BytesMut) -> Result<(usize, usize), ProtocolError> {
    let mut idx = 1;
    let mut multiplier = 1usize;
    let mut value = 0usize;

    loop {
        if idx >= buffer.len() {
            return Err(ProtocolError::IncompletePacket {
                available: buffer.len(),
            });
        }
        let byte = buffer[idx] as usize;
        idx += 1;
        value += (byte & 0x7F) * multiplier;

        if (byte & 0x80) == 0 {
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

    Ok((value, idx - 1))
}

/// Read a complete MQTT packet from the stream using BytesMut
async fn read_packet(
    socket: &mut TcpStream,
    buffer: &mut BytesMut,
) -> Result<Option<Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>>, ProtocolError> {
    // Ensure we have at least 2 bytes (packet type + min remaining length)
    while buffer.len() < 2 {
        buffer.reserve(2);
        let n = socket.read_buf(buffer).await.unwrap_or(0);
        if n == 0 {
            return Ok(None); // EOF
        }
    }

    // Parse remaining length
    let (remaining_len, var_len_bytes) = parse_remaining_length(buffer)?;
    let total_len = 1 + var_len_bytes + remaining_len;

    // Ensure we have the full packet
    while buffer.len() < total_len {
        buffer.reserve(total_len - buffer.len());
        let n = socket.read_buf(buffer).await.unwrap_or(0);
        if n == 0 {
            return Err(ProtocolError::IncompletePacket {
                available: buffer.len(),
            });
        }
    }

    // Decode the packet
    let packet = Packet::decode(&buffer[..total_len])?;

    // Advance buffer (keep any excess for next packet)
    buffer.advance(total_len);

    Ok(Some(packet))
}

/// Encode a packet into Bytes for zero-copy transmission
fn encode_frame(
    packet: &Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>,
) -> Result<bytes::Bytes, std::io::Error> {
    // Allocate a buffer large enough for any packet
    let mut buffer = [0u8; MAX_PAYLOAD_SIZE + 32];

    // Encode the packet using the PacketEncoder trait
    let size = packet
        .encode(&mut buffer)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "Failed to encode packet"))?;

    // Convert to Bytes for zero-copy sharing
    Ok(bytes::BytesMut::from(&buffer[..size]).freeze())
}

// =============================================================================
// Connection Handler
// =============================================================================

async fn handle_connection(
    mut socket: TcpStream,
    peer_addr: String,
    state: Arc<ServerState>,
    broker: SharedBroker,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Generate session ID
    let session_id = state.generate_session_id();

    info!("New session {}", session_id);

    // NEW: Create notification channel for this session
    let (notify_tx, mut notify_rx) = mpsc::channel(8);

    // Register notification sender with ServerState
    state.register_notification(session_id, notify_tx);

    // Register session with broker
    {
        let mut broker_guard = broker.lock().await;
        debug!("Registering session {} with keep-alive={}s", session_id, DEFAULT_KEEP_ALIVE);
        if let Err(_) = broker_guard.register_new_session(session_id, DEFAULT_KEEP_ALIVE, current_time_nanos()) {
            error!("Failed to register session for {}", peer_addr);
            return Err("Session registration failed".into());
        }
        debug!("Session {} registered successfully", session_id);
    }

    info!("Registered session {} for {}", session_id, peer_addr);

    // Store connection handle
    state.connections.insert(
        session_id,
        ConnectionHandle {
            session_id,
            peer_addr: peer_addr.clone(),
        },
    );
    info!("Session {} connection handle stored for {}", session_id, peer_addr);

    // Channels for frame transmission
    let (frame_tx, mut frame_rx) = mpsc::channel(FRAME_CHANNEL_CAPACITY);

    // Read buffer with initial capacity
    let mut read_buffer = BytesMut::with_capacity(4096);

    // Keep-alive tracking
    let mut last_activity = Instant::now();
    let keep_alive_timeout = Duration::from_secs(DEFAULT_KEEP_ALIVE as u64 * 3 / 2);

    info!("Session {} entering main connection loop", session_id);

    // Main connection loop with tokio::select!
    loop {
        // Calculate time until keep-alive timeout
        let time_until_timeout = keep_alive_timeout.saturating_sub(last_activity.elapsed());

        tokio::select! {
            // Read from socket
            read_result = read_packet(&mut socket, &mut read_buffer) => {
                match read_result {
                    Ok(Some(packet)) => {
                        last_activity = Instant::now();

                        let (packet_type, details) = match &packet {
                            Packet::Connect(c) => ("CONNECT", format!("client_id={}", c.client_id)),
                            Packet::ConnAck(_) => ("CONNACK", String::new()),
                            Packet::Publish(p) => (
                                "PUBLISH",
                                format!(
                                    "topic={}, qos={}, retain={}, payload_len={}",
                                    p.topic_name,
                                    p.qos as u8,
                                    p.retain,
                                    p.payload.len()
                                ),
                            ),
                            Packet::PubAck(p) => ("PUBACK", format!("packet_id={}", p.packet_id)),
                            Packet::PubRec(p) => ("PUBREC", format!("packet_id={}", p.packet_id)),
                            Packet::PubRel(p) => ("PUBREL", format!("packet_id={}", p.packet_id)),
                            Packet::PubComp(p) => ("PUBCOMP", format!("packet_id={}", p.packet_id)),
                            Packet::Subscribe(s) => ("SUBSCRIBE", format!("packet_id={}, topics={}", s.packet_id, s.topic_filter.len())),
                            Packet::SubAck(_) => ("SUBACK", String::new()),
                            Packet::Unsubscribe(u) => ("UNSUBSCRIBE", format!("packet_id={}, topics={}", u.packet_id, u.topic_filter.len())),
                            Packet::UnsubAck(_) => ("UNSUBACK", String::new()),
                            Packet::PingReq(_) => ("PINGREQ", String::new()),
                            Packet::PingResp(_) => ("PINGRESP", String::new()),
                            Packet::Disconnect(_) => ("DISCONNECT", String::new()),
                        };
                        if details.is_empty() {
                            debug!("Session {}: Received {}", session_id, packet_type);
                        } else {
                            debug!("Session {}: Received {} ({})", session_id, packet_type, details);
                        }

                        // Process packet with broker
                        let mut broker_guard = broker.lock().await;
                        let current_time = current_time_nanos();

                        let result = broker_guard
                            .queue_packet_received_from_client(session_id, packet, current_time)
                            .and_then(|_| broker_guard.process_all_session_packets());

                        // NEW: Get sessions with pending packets BEFORE dropping lock
                        let sessions_to_notify: Vec<u128> = if result.is_ok() {
                            broker_guard.get_sessions_with_pending_packets().to_vec()
                        } else {
                            Vec::new()
                        };

                        let tx_packets = match result {
                            Ok(()) => {
                                let mut packets = Vec::new();
                                while let Ok(Some(pkt)) =
                                    broker_guard.dequeue_packet_to_send_to_client(session_id)
                                {
                                    let pkt_type = match &pkt {
                                        Packet::ConnAck(_) => "CONNACK",
                                        Packet::Publish(_) => "PUBLISH",
                                        Packet::PubAck(_) => "PUBACK",
                                        Packet::PubRec(_) => "PUBREC",
                                        Packet::PubRel(_) => "PUBREL",
                                        Packet::PubComp(_) => "PUBCOMP",
                                        Packet::SubAck(_) => "SUBACK",
                                        Packet::UnsubAck(_) => "UNSUBACK",
                                        Packet::PingResp(_) => "PINGRESP",
                                        _ => "OTHER",
                                    };
                                    debug!("Dequeuing {} for session {}", pkt_type, session_id);
                                    packets.push(pkt);
                                }
                                if !packets.is_empty() {
                                    debug!("Sending {} packets to session {}", packets.len(), session_id);
                                }
                                packets
                            }
                            Err(e) => {
                                warn!("Error processing packet for session {}: {:?}", session_id, e);
                                let _ = broker_guard.mark_session_disconnected(session_id);
                                Vec::new()
                            }
                        };
                        drop(broker_guard);

                        // NEW: Send notifications to sessions with pending packets
                        for sid in sessions_to_notify {
                            state.notify_session(sid).await;
                        }

                        // Send packets to client
                        for pkt in tx_packets {
                            let pkt_type = match &pkt {
                                Packet::ConnAck(c) => Some(format!("CONNACK (rc={})", c.return_code as u8)),
                                Packet::Publish(p) => Some(format!("PUBLISH (topic={})", p.topic_name)),
                                Packet::PubAck(p) => Some(format!("PUBACK (id={})", p.packet_id)),
                                Packet::PubRec(p) => Some(format!("PUBREC (id={})", p.packet_id)),
                                Packet::PubRel(p) => Some(format!("PUBREL (id={}", p.packet_id)),
                                Packet::PubComp(p) => Some(format!("PUBCOMP (id={})", p.packet_id)),
                                Packet::SubAck(_) => Some("SUBACK".to_string()),
                                Packet::UnsubAck(_) => Some("UNSUBACK".to_string()),
                                Packet::PingResp(_) => Some("PINGRESP".to_string()),
                                _ => None,
                            };
                            match encode_frame(&pkt) {
                                Ok(frame) => {
                                    if let Some(pt) = pkt_type {
                                        debug!("Session {}: Sending {}", session_id, pt);
                                    }
                                    if frame_tx.send(frame).await.is_err() {
                                        warn!("Frame channel closed for session {}", session_id);
                                        break;
                                    }
                                }
                                Err(e) => {
                                    error!("Failed to encode packet: {}", e);
                                }
                            }
                        }
                    }
                    Ok(None) => {
                        // Client closed connection
                        info!("Session {} client closed connection", session_id);
                        break;
                    }
                    Err(e) => {
                        warn!("Session {}: Protocol error: {}", session_id, e);
                        break;
                    }
                }
            }

            // NEW: Notification from broker that outbound packets are ready
            Some(()) = notify_rx.recv() => {
                trace!("Session {}: Received outbound notification", session_id);

                // Dequeue and send packets
                let mut broker_guard = broker.lock().await;
                let mut packets = Vec::new();
                while let Ok(Some(pkt)) = broker_guard.dequeue_packet_to_send_to_client(session_id) {
                    let pkt_type = match &pkt {
                        Packet::ConnAck(_) => "CONNACK",
                        Packet::Publish(_) => "PUBLISH",
                        Packet::PubAck(_) => "PUBACK",
                        Packet::PubRec(_) => "PUBREC",
                        Packet::PubRel(_) => "PUBREL",
                        Packet::PubComp(_) => "PUBCOMP",
                        Packet::SubAck(_) => "SUBACK",
                        Packet::UnsubAck(_) => "UNSUBACK",
                        Packet::PingResp(_) => "PINGRESP",
                        _ => "OTHER",
                    };
                    debug!("Dequeuing {} for session {}", pkt_type, session_id);
                    packets.push(pkt);
                }
                drop(broker_guard);

                if !packets.is_empty() {
                    debug!("Sending {} packets to session {}", packets.len(), session_id);
                }

                // Send packets to client
                for pkt in packets {
                    let pkt_type = match &pkt {
                        Packet::ConnAck(c) => Some(format!("CONNACK (rc={})", c.return_code as u8)),
                        Packet::Publish(p) => Some(format!("PUBLISH (topic={})", p.topic_name)),
                        Packet::PubAck(p) => Some(format!("PUBACK (id={})", p.packet_id)),
                        Packet::PubRec(p) => Some(format!("PUBREC (id={})", p.packet_id)),
                        Packet::PubRel(p) => Some(format!("PUBREL (id={})", p.packet_id)),
                        Packet::PubComp(p) => Some(format!("PUBCOMP (id={})", p.packet_id)),
                        Packet::SubAck(_) => Some("SUBACK".to_string()),
                        Packet::UnsubAck(_) => Some("UNSUBACK".to_string()),
                        Packet::PingResp(_) => Some("PINGRESP".to_string()),
                        _ => None,
                    };
                    match encode_frame(&pkt) {
                        Ok(frame) => {
                            if let Some(pt) = pkt_type {
                                debug!("Session {}: Sending {}", session_id, pt);
                            }
                            if frame_tx.send(frame).await.is_err() {
                                warn!("Frame channel closed for session {}", session_id);
                                break;
                            }
                        }
                        Err(e) => {
                            error!("Failed to encode packet: {}", e);
                        }
                    }
                }
            }

            // Write to socket
            Some(frame) = frame_rx.recv() => {
                trace!("Session {}: Writing {} bytes to socket", session_id, frame.len());
                if let Err(e) = socket.write_all(&frame).await {
                    error!("Session {}: Write error: {}", session_id, e);
                    break;
                }
                if let Err(e) = socket.flush().await {
                    error!("Session {}: Flush error: {}", session_id, e);
                    break;
                }
            }

            // Keep-alive timeout
            _ = sleep(time_until_timeout) => {
                if last_activity.elapsed() >= keep_alive_timeout {
                    info!("Session {} keep-alive timeout", session_id);
                    break;
                }
            }
        }
    }

    // Cleanup
    info!("Session {} connection closing for {}", session_id, peer_addr);
    let was_present = state.connections.remove(&session_id).is_some();
    if was_present {
        debug!("Session {} removed from server state", session_id);
    }

    // NEW: Remove notification sender
    state.remove_notification(session_id);

    {
        let mut broker_guard = broker.lock().await;
        debug!("Removing session {}", session_id);
        broker_guard.remove_session(session_id);
    }

    Ok(())
}

// =============================================================================
// Cleanup Task
// =============================================================================

async fn cleanup_task(state: Arc<ServerState>, broker: SharedBroker) {
    info!("Cleanup task started");
    let mut interval = sleep(Duration::from_secs(5));
    loop {
        interval.await;
        interval = sleep(Duration::from_secs(5));

        let current_time = current_time_nanos();
        trace!("Cleanup scan running at time {}", current_time);

        let mut broker_guard = broker.lock().await;

        // Check expired sessions
        let expired: Vec<u128> = broker_guard
            .get_expired_sessions(current_time)
            .into_iter()
            .flatten()
            .collect();
        for session_id in expired {
            info!("Cleaning up expired session {}", session_id);
            state.connections.remove(&session_id);
            broker_guard.remove_session(session_id);
        }

        // Check disconnected sessions
        let disconnected: Vec<u128> = broker_guard
            .get_disconnected_sessions()
            .into_iter()
            .flatten()
            .collect();
        for session_id in disconnected {
            info!("Cleaning up disconnected session {}", session_id);
            state.connections.remove(&session_id);
            broker_guard.remove_session(session_id);
        }
    }
}

// =============================================================================
// Main Entry Point
// =============================================================================

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .init();

    let bind_addr = "0.0.0.0:1883";
    info!("Starting picobroker tokio server on {}", bind_addr);
    info!("Configuration: {} max sessions", MAX_SESSIONS);

    let listener = TcpListener::bind(bind_addr).await?;
    info!("Server listening on {}", bind_addr);

    // Create server state
    let state = Arc::new(ServerState::new());

    // Create shared broker
    let broker: SharedBroker = Arc::new(tokio::sync::Mutex::new(PicoBroker::new()));

    // Spawn cleanup task
    let cleanup_state = state.clone();
    let cleanup_broker = broker.clone();
    tokio::spawn(async move {
        cleanup_task(cleanup_state, cleanup_broker).await;
    });
    info!("Cleanup task spawned");

    // Accept connections loop
    info!("Accepting incoming connections...");
    loop {
        match listener.accept().await {
            Ok((socket, addr)) => {
                let peer_addr = addr.to_string();
                info!("New connection from {}", peer_addr);
                debug!("Active connections: {}", state.connections.len());

                let handler_state = state.clone();
                let handler_broker = broker.clone();

                tokio::spawn(async move {
                    debug!("Handler task spawned for {}", peer_addr);
                    if let Err(e) = handle_connection(socket, peer_addr, handler_state, handler_broker).await {
                        error!("Client handler error: {}", e);
                    }
                });
            }
            Err(e) => {
                error!("Error accepting connection: {}", e);
            }
        }
    }
}
