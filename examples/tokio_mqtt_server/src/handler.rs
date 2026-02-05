use crate::io::{encode_frame, read_packet};
use crate::state::{current_time_nanos, ServerState};
use bytes::BytesMut;
use log::{debug, error, info, trace, warn};
use picobroker::broker::PicoBroker;
use picobroker::protocol::packets::Packet;
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tokio::net::TcpStream;
use tokio::sync::mpsc;
use tokio::time::{sleep, Duration, Instant};

/// Server configuration for connection handling
#[derive(Debug, Clone)]
pub struct HandlerConfig {
    pub default_keep_alive_secs: u16,
    pub frame_channel_capacity: usize,
}

impl Default for HandlerConfig {
    fn default() -> Self {
        Self {
            default_keep_alive_secs: 60,
            frame_channel_capacity: 32,
        }
    }
}

/// Get human-readable packet type name
fn packet_type_name<const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize>(
    packet: &Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>,
) -> &'static str {
    match packet {
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
    }
}

/// Get detailed packet info for logging
fn packet_details<const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize>(
    packet: &Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>,
) -> String {
    match packet {
        Packet::Connect(c) => format!("client_id={}", c.client_id),
        Packet::ConnAck(c) => format!("rc={}", c.return_code as u8),
        Packet::Publish(p) => format!(
            "topic={}, qos={}, retain={}, payload_len={}",
            p.topic_name,
            p.qos as u8,
            p.retain,
            p.payload.len()
        ),
        Packet::PubAck(p) => format!("packet_id={}", p.packet_id),
        Packet::PubRec(p) => format!("packet_id={}", p.packet_id),
        Packet::PubRel(p) => format!("packet_id={}", p.packet_id),
        Packet::PubComp(p) => format!("packet_id={}", p.packet_id),
        Packet::Subscribe(s) => {
            format!("packet_id={}, topics={}", s.packet_id, s.topic_filter.len())
        }
        Packet::SubAck(_) => String::new(),
        Packet::Unsubscribe(u) => {
            format!("packet_id={}, topics={}", u.packet_id, u.topic_filter.len())
        }
        Packet::UnsubAck(_) => String::new(),
        Packet::PingReq(_) => String::new(),
        Packet::PingResp(_) => String::new(),
        Packet::Disconnect(_) => String::new(),
    }
}

/// Handle a single MQTT client connection
pub async fn handle_connection<
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const QUEUE_SIZE: usize,
    const MAX_SESSIONS: usize,
    const MAX_TOPICS: usize,
    const MAX_SUBSCRIBERS_PER_TOPIC: usize,
>(
    mut socket: TcpStream,
    peer_addr: String,
    state: Arc<ServerState>,
    broker: Arc<
        tokio::sync::Mutex<
            PicoBroker<
                MAX_TOPIC_NAME_LENGTH,
                MAX_PAYLOAD_SIZE,
                QUEUE_SIZE,
                MAX_SESSIONS,
                MAX_TOPICS,
                MAX_SUBSCRIBERS_PER_TOPIC,
            >,
        >,
    >,
    config: &HandlerConfig,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    // Generate session ID
    let session_id = state.generate_session_id();
    info!("New connection from {} -> assigned session {}", peer_addr, session_id);
    debug!("Session {} configuration: keep_alive={}s, frame_channel_capacity={}",
           session_id, config.default_keep_alive_secs, config.frame_channel_capacity);

    // Create notification channel for this session
    let (notify_tx, mut notify_rx) = mpsc::channel(8);
    trace!("Session {} notification channel created", session_id);

    // Register notification sender with ServerState
    state.register_notification(session_id, notify_tx);
    trace!("Session {} notification sender registered", session_id);

    // Register session with broker
    {
        let mut broker_guard = broker.lock().await;
        debug!(
            "Registering session {} with broker (keep-alive={}s)",
            session_id, config.default_keep_alive_secs
        );
        if broker_guard
            .register_new_session(
                session_id,
                config.default_keep_alive_secs,
                current_time_nanos(),
            )
            .is_err()
        {
            error!("Failed to register session {} for {}", session_id, peer_addr);
            return Err("Session registration failed".into());
        }
        debug!("Session {} registered with broker successfully", session_id);
    }

    // Store connection handle
    state.connections.insert(session_id, ());
    info!("Session {} for {} initialized successfully", session_id, peer_addr);

    // Channels for frame transmission
    let (frame_tx, mut frame_rx) = mpsc::channel(config.frame_channel_capacity);
    trace!("Session {} frame transmission channel created", session_id);

    // Read buffer with initial capacity
    let mut read_buffer = BytesMut::with_capacity(4096);
    trace!("Session {} read buffer initialized with capacity 4096", session_id);

    // Keep-alive tracking
    let mut last_activity = Instant::now();
    let keep_alive_timeout = Duration::from_secs(config.default_keep_alive_secs as u64 * 3 / 2);
    debug!(
        "Session {} keep-alive timeout set to {}s",
        session_id,
        keep_alive_timeout.as_secs()
    );

    info!("Session {} for {} entering main connection loop", session_id, peer_addr);

    // Main connection loop with tokio::select!
    loop {
        // Calculate time until keep-alive timeout
        let time_until_timeout = keep_alive_timeout.saturating_sub(last_activity.elapsed());

        tokio::select! {
            // Read from socket
            read_result = read_packet::<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>(
                &mut socket,
                &mut read_buffer
            ) => {
                match read_result {
                    Ok(Some(packet)) => {
                        last_activity = Instant::now();

                        let pkt_type = packet_type_name(&packet);
                        let details = packet_details(&packet);
                        if details.is_empty() {
                            debug!("Session {}: Received {}", session_id, pkt_type);
                        } else {
                            debug!("Session {}: Received {} ({})", session_id, pkt_type, details);
                        }

                        // Process packet with broker
                        let mut broker_guard = broker.lock().await;
                        let current_time = current_time_nanos();

                        let result = broker_guard
                            .queue_packet_received_from_client(session_id, packet, current_time)
                            .and_then(|_| broker_guard.process_all_session_packets());

                        // Get sessions with pending packets BEFORE dropping lock
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
                                    let pkt_type = packet_type_name(&pkt);
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

                        // Send notifications to sessions with pending packets
                        for sid in sessions_to_notify {
                            state.notify_session(sid).await;
                        }

                        // Send packets to client
                        for pkt in tx_packets {
                            let pkt_type = packet_type_name(&pkt);
                            let details = packet_details(&pkt);
                            match encode_frame(&pkt) {
                                Ok(frame) => {
                                    if !details.is_empty() {
                                        debug!("Session {}: Sending {} ({})", session_id, pkt_type, details);
                                    } else {
                                        debug!("Session {}: Sending {}", session_id, pkt_type);
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
                        info!("Session {} for {} closed connection (EOF)", session_id, peer_addr);
                        break;
                    }
                    Err(e) => {
                        warn!("Session {} for {}: Protocol error: {}", session_id, peer_addr, e);
                        break;
                    }
                }
            }

            // Notification from broker that outbound packets are ready
            Some(()) = notify_rx.recv() => {
                trace!("Session {}: Received outbound notification", session_id);

                // Dequeue and send packets
                let mut broker_guard = broker.lock().await;
                let mut packets = Vec::new();
                while let Ok(Some(pkt)) = broker_guard.dequeue_packet_to_send_to_client(session_id) {
                    let pkt_type = packet_type_name(&pkt);
                    debug!("Dequeuing {} for session {}", pkt_type, session_id);
                    packets.push(pkt);
                }
                drop(broker_guard);

                if !packets.is_empty() {
                    debug!("Sending {} packets to session {}", packets.len(), session_id);
                }

                // Send packets to client
                for pkt in packets {
                    let pkt_type = packet_type_name(&pkt);
                    let details = packet_details(&pkt);
                    match encode_frame(&pkt) {
                        Ok(frame) => {
                            if !details.is_empty() {
                                debug!("Session {}: Sending {} ({})", session_id, pkt_type, details);
                            } else {
                                debug!("Session {}: Sending {}", session_id, pkt_type);
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
                    info!(
                        "Session {} for {} keep-alive timeout (last activity: {:?})",
                        session_id,
                        peer_addr,
                        last_activity.elapsed()
                    );
                    break;
                }
            }
        }
    }

    // Cleanup
    info!(
        "Session {} for {} connection closing (cleaning up resources)",
        session_id, peer_addr
    );

    let was_present = state.connections.remove(&session_id).is_some();
    if was_present {
        trace!("Session {} removed from server connections map", session_id);
    } else {
        warn!("Session {} was not found in server connections map", session_id);
    }

    // Remove notification sender
    state.remove_notification(session_id);
    trace!("Session {} notification sender removed", session_id);

    {
        let mut broker_guard = broker.lock().await;
        debug!("Removing session {} from broker", session_id);
        broker_guard.remove_session(session_id);
        trace!("Session {} removed from broker", session_id);
    }

    info!("Session {} for {} cleanup complete", session_id, peer_addr);
    Ok(())
}
