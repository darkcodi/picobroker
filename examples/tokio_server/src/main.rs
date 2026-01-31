use bytes::{Buf, Bytes, BytesMut};
use dashmap::DashMap;
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
use tokio::sync::oneshot;
use tokio::time::{sleep, Duration, Instant};

// =============================================================================
// Configuration
// =============================================================================

const MAX_TOPIC_NAME_LENGTH: usize = 64;
const MAX_PAYLOAD_SIZE: usize = 256;
const QUEUE_SIZE: usize = 8;
const MAX_SESSIONS: usize = 16;
const MAX_TOPICS: usize = 32;
const MAX_SUBSCRIBERS_PER_TOPIC: usize = 8;

// Number of broker shards for parallelization
const NUM_SHARDS: usize = 4;
const SESSIONS_PER_SHARD: usize = MAX_SESSIONS / NUM_SHARDS;

// Channel capacities
const SHARD_CHANNEL_CAPACITY: usize = 256;
const FRAME_CHANNEL_CAPACITY: usize = 32;

// Default keep-alive (seconds)
const DEFAULT_KEEP_ALIVE: u16 = 60;

// =============================================================================
// Type Aliases
// =============================================================================

type ShardBroker = PicoBroker<
    MAX_TOPIC_NAME_LENGTH,
    MAX_PAYLOAD_SIZE,
    QUEUE_SIZE,
    SESSIONS_PER_SHARD,
    MAX_TOPICS,
    MAX_SUBSCRIBERS_PER_TOPIC,
>;

// =============================================================================
// Shard Work Types
// =============================================================================

#[derive(Debug)]
enum ShardWork {
    QueuePacket {
        session_id: u128,
        packet: Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>,
        response_tx: oneshot::Sender<ProcessResult>,
    },
    RegisterSession {
        session_id: u128,
        keep_alive_secs: u16,
        current_time: u128,
        response_tx: oneshot::Sender<Result<(), BrokerWorkError>>,
    },
    RemoveSession {
        session_id: u128,
        response_tx: oneshot::Sender<bool>,
    },
    GetExpiredSessions {
        current_time: u128,
        response_tx: oneshot::Sender<Vec<u128>>,
    },
    GetDisconnectedSessions {
        response_tx: oneshot::Sender<Vec<u128>>,
    },
}

#[derive(Debug)]
enum BrokerWorkError {
    SessionLimitReached,
}

#[derive(Debug)]
struct ProcessResult {
    tx_packets: Vec<Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>>,
    disconnected: bool,
}

// =============================================================================
// Broker Shard
// =============================================================================

struct BrokerShard {
    work_tx: mpsc::Sender<ShardWork>,
}

impl BrokerShard {
    fn new() -> (Self, mpsc::Receiver<ShardWork>) {
        let (work_tx, work_rx) = mpsc::channel(SHARD_CHANNEL_CAPACITY);
        (Self { work_tx }, work_rx)
    }

    async fn send_work(&self, work: ShardWork) -> Result<(), ShardError> {
        self.work_tx
            .send(work)
            .await
            .map_err(|_| ShardError::ChannelClosed)
    }
}

#[derive(Debug)]
enum ShardError {
    ChannelClosed,
}

impl std::fmt::Display for ShardError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ShardError::ChannelClosed => write!(f, "Shard channel closed"),
        }
    }
}

impl std::error::Error for ShardError {}

// =============================================================================
// Server State
// =============================================================================

struct ServerState {
    connections: DashMap<u128, ConnectionHandle>,
    session_id_gen: AtomicU64,
}

#[allow(dead_code)]
struct ConnectionHandle {
    session_id: u128,
    shard_index: usize,
    peer_addr: String,
}

impl ServerState {
    fn new() -> Self {
        Self {
            connections: DashMap::new(),
            session_id_gen: AtomicU64::new(0),
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

fn get_shard_index(session_id: u128) -> usize {
    (session_id as usize) % NUM_SHARDS
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
) -> Result<Bytes, std::io::Error> {
    // Allocate a buffer large enough for any packet
    let mut buffer = [0u8; MAX_PAYLOAD_SIZE + 32];

    // Encode the packet using the PacketEncoder trait
    let size = packet
        .encode(&mut buffer)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::InvalidData, "Failed to encode packet"))?;

    // Convert to Bytes for zero-copy sharing
    Ok(BytesMut::from(&buffer[..size]).freeze())
}

// =============================================================================
// Broker Shard Worker
// =============================================================================

async fn shard_worker(mut rx: mpsc::Receiver<ShardWork>, mut broker: ShardBroker) {
    loop {
        match rx.recv().await {
            Some(work) => match work {
                ShardWork::QueuePacket {
                    session_id,
                    packet,
                    response_tx,
                } => {
                    let current_time = current_time_nanos();

                    // Queue and process the packet
                    debug!("[Shard] Queueing packet for session {}", session_id);
                    let result = broker
                        .queue_packet_received_from_client(session_id, packet, current_time)
                        .and_then(|_| broker.process_all_session_packets());

                    // Collect outbound packets
                    let (tx_packets, disconnected) = match result {
                        Ok(()) => {
                            let mut packets = Vec::new();
                            while let Ok(Some(pkt)) =
                                broker.dequeue_packet_to_send_to_client(session_id)
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
                                debug!("[Shard] Dequeuing {} for session {}", pkt_type, session_id);
                                packets.push(pkt);
                            }
                            if !packets.is_empty() {
                                debug!("[Shard] Sending {} packets to session {}", packets.len(), session_id);
                            }
                            (packets, false)
                        }
                        Err(e) => {
                            warn!("[Shard] Error processing packet for session {}: {:?}", session_id, e);
                            let _ = broker.mark_session_disconnected(session_id);
                            (vec![], true)
                        }
                    };

                    let _ = response_tx.send(ProcessResult {
                        tx_packets,
                        disconnected,
                    });
                }

                ShardWork::RegisterSession {
                    session_id,
                    keep_alive_secs,
                    current_time,
                    response_tx,
                } => {
                    debug!("[Shard] Registering session {} with keep-alive={}s", session_id, keep_alive_secs);
                    let result = broker
                        .register_new_session(session_id, keep_alive_secs, current_time)
                        .map_err(|_| BrokerWorkError::SessionLimitReached);
                    match &result {
                        Ok(()) => debug!("[Shard] Session {} registered successfully", session_id),
                        Err(_) => warn!("[Shard] Failed to register session {}: limit reached", session_id),
                    }
                    let _ = response_tx.send(result);
                }

                ShardWork::RemoveSession {
                    session_id,
                    response_tx,
                } => {
                    debug!("[Shard] Removing session {}", session_id);
                    let removed = broker.remove_session(session_id);
                    if removed {
                        debug!("[Shard] Session {} removed successfully", session_id);
                    } else {
                        warn!("[Shard] Session {} not found for removal", session_id);
                    }
                    let _ = response_tx.send(removed);
                }

                ShardWork::GetExpiredSessions {
                    current_time,
                    response_tx,
                } => {
                    let expired: Vec<u128> = broker
                        .get_expired_sessions(current_time)
                        .into_iter()
                        .flatten()
                        .collect();
                    if !expired.is_empty() {
                        debug!("[Shard] Found {} expired sessions", expired.len());
                    }
                    let _ = response_tx.send(expired);
                }

                ShardWork::GetDisconnectedSessions { response_tx } => {
                    let disconnected: Vec<u128> = broker
                        .get_disconnected_sessions()
                        .into_iter()
                        .flatten()
                        .collect();
                    if !disconnected.is_empty() {
                        debug!("[Shard] Found {} disconnected sessions", disconnected.len());
                    }
                    let _ = response_tx.send(disconnected);
                }
            },
            None => {
                // Channel closed - worker should exit
                info!("Shard worker channel closed, exiting");
                break;
            }
        }
    }
}

// =============================================================================
// Connection Handler
// =============================================================================

async fn handle_connection(
    mut socket: TcpStream,
    peer_addr: String,
    state: Arc<ServerState>,
    shards: &[Arc<BrokerShard>],
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Generate session ID and register
    let session_id = state.generate_session_id();
    let shard_index = get_shard_index(session_id);
    let shard = &shards[shard_index];

    info!("New session {} assigned to shard {}", session_id, shard_index);

    // Register session with broker
    let (response_tx, response_rx) = oneshot::channel();
    shard
        .send_work(ShardWork::RegisterSession {
            session_id,
            keep_alive_secs: DEFAULT_KEEP_ALIVE,
            current_time: current_time_nanos(),
            response_tx,
        })
        .await?;

    match response_rx.await {
        Ok(Ok(())) => {
            info!(
                "Registered session {} for {} on shard {}",
                session_id, peer_addr, shard_index
            );
        }
        Ok(Err(_)) | Err(_) => {
            error!("Failed to register session for {}", peer_addr);
            return Err("Session registration failed".into());
        }
    }

    // Store connection handle
    state.connections.insert(
        session_id,
        ConnectionHandle {
            session_id,
            shard_index,
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

                        // Send to shard for processing
                        let (response_tx, response_rx) = oneshot::channel();
                        match shard.send_work(ShardWork::QueuePacket {
                            session_id,
                            packet,
                            response_tx,
                        }).await {
                            Ok(()) => {
                                // Wait for processing result
                                match response_rx.await {
                                    Ok(ProcessResult { tx_packets, disconnected }) => {
                                        for pkt in tx_packets {
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
                                        if disconnected {
                                            info!("Session {} marked as disconnected", session_id);
                                            break;
                                        }
                                    }
                                    Err(_) => {
                                        error!("Shard response channel closed for session {}", session_id);
                                        break;
                                    }
                                }
                            }
                            Err(_) => {
                                error!("Shard channel closed for session {}", session_id);
                                break;
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

    let (response_tx, _) = oneshot::channel();
    let _ = shard
        .send_work(ShardWork::RemoveSession {
            session_id,
            response_tx,
        })
        .await;

    Ok(())
}

// =============================================================================
// Cleanup Task
// =============================================================================

async fn cleanup_task(state: Arc<ServerState>, shards: Vec<Arc<BrokerShard>>) {
    info!("Cleanup task started");
    let mut interval = sleep(Duration::from_secs(5));
    loop {
        interval.await;
        interval = sleep(Duration::from_secs(5));

        let current_time = current_time_nanos();
        trace!("Cleanup scan running at time {}", current_time);

        // Check each shard for expired/disconnected sessions
        for (shard_idx, shard) in shards.iter().enumerate() {
            // Check expired sessions
            let (tx, rx) = oneshot::channel();
            if shard
                .send_work(ShardWork::GetExpiredSessions {
                    current_time,
                    response_tx: tx,
                })
                .await
                .is_ok()
            {
                if let Ok(expired) = rx.await {
                    for session_id in expired {
                        info!(
                            "Cleaning up expired session {} on shard {}",
                            session_id, shard_idx
                        );
                        state.connections.remove(&session_id);
                        let (remove_tx, _) = oneshot::channel();
                        let _ = shard
                            .send_work(ShardWork::RemoveSession {
                                session_id,
                                response_tx: remove_tx,
                            })
                            .await;
                    }
                }
            }

            // Check disconnected sessions
            let (tx, rx) = oneshot::channel();
            if shard
                .send_work(ShardWork::GetDisconnectedSessions { response_tx: tx })
                .await
                .is_ok()
            {
                if let Ok(disconnected) = rx.await {
                    for session_id in disconnected {
                        info!(
                            "Cleaning up disconnected session {} on shard {}",
                            session_id, shard_idx
                        );
                        state.connections.remove(&session_id);
                        let (remove_tx, _) = oneshot::channel();
                        let _ = shard
                            .send_work(ShardWork::RemoveSession {
                                session_id,
                                response_tx: remove_tx,
                            })
                            .await;
                    }
                }
            }
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
    info!("Configuration: {} shards, {} max sessions per shard", NUM_SHARDS, SESSIONS_PER_SHARD);

    let listener = TcpListener::bind(bind_addr).await?;
    info!("Server listening on {}", bind_addr);

    // Create server state
    let state = Arc::new(ServerState::new());

    // Create broker shards
    let mut shards = Vec::with_capacity(NUM_SHARDS);
    for shard_idx in 0..NUM_SHARDS {
        let broker = ShardBroker::new();
        let (shard, work_rx) = BrokerShard::new();
        let shard = Arc::new(shard);
        shards.push(shard.clone());

        // Spawn shard worker
        tokio::spawn(async move {
            info!("Shard {} worker starting", shard_idx);
            shard_worker(work_rx, broker).await;
            info!("Shard {} worker exited", shard_idx);
        });
    }

    // Spawn cleanup task
    let cleanup_state = state.clone();
    let cleanup_shards = shards.clone();
    tokio::spawn(async move {
        cleanup_task(cleanup_state, cleanup_shards).await;
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
                let handler_shards = shards.clone();

                tokio::spawn(async move {
                    debug!("Handler task spawned for {}", peer_addr);
                    if let Err(e) = handle_connection(socket, peer_addr, handler_state, &handler_shards).await {
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
