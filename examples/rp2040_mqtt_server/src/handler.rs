//! Connection handling logic for Embassy-based MQTT broker.

use crate::io::{read_packet, write_packet};
use crate::state::{current_time_nanos, NotificationRegistry};
use embassy_futures::select::{select3, Either3};
use embassy_net::tcp::TcpSocket;
use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::channel::Channel;
use embassy_sync::mutex::Mutex;
use embassy_time::{Duration, Instant};
use picobroker::protocol::heapless::HeaplessVec;
use picobroker::protocol::packets::Packet;
use picobroker::protocol::ProtocolError;

/// Per-connection handler configuration
pub struct HandlerConfig {
    /// Default keep-alive in seconds
    pub default_keep_alive_secs: u16,
}

impl Default for HandlerConfig {
    fn default() -> Self {
        Self {
            default_keep_alive_secs: 60,
        }
    }
}

/// Handle a single MQTT connection
///
/// This function manages the complete lifecycle of a client connection:
/// - Reading packets from the client
/// - Processing them through the broker
/// - Sending outbound packets (both responses and notifications)
/// - Managing keep-alive timeout
///
/// The select3 pattern ensures idle clients wake up when messages are published
/// to topics they're subscribed to.
pub async fn handle_connection<
    'a,
    M: RawMutex,
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const QUEUE_SIZE: usize,
    const MAX_SESSIONS: usize,
    const MAX_TOPICS: usize,
    const MAX_SUBSCRIBERS_PER_TOPIC: usize,
    const RX_BUFFER_SIZE: usize,
>(
    socket: &mut TcpSocket<'a>,
    session_id: u128,
    _socket_idx: usize,
    broker: &Mutex<
        M,
        picobroker::broker::PicoBroker<
            MAX_TOPIC_NAME_LENGTH,
            MAX_PAYLOAD_SIZE,
            QUEUE_SIZE,
            MAX_SESSIONS,
            MAX_TOPICS,
            MAX_SUBSCRIBERS_PER_TOPIC,
        >,
    >,
    notification_registry: &NotificationRegistry<MAX_SESSIONS, M>,
    notify_receiver: &Channel<M, (), 1>,
    config: &HandlerConfig,
) -> Result<(), ProtocolError> {
    // Static buffers for packet processing (local to this connection)
    let mut rx_buffer = [0u8; RX_BUFFER_SIZE];
    let mut tx_buffer = [0u8; RX_BUFFER_SIZE];
    let mut buffer_len = 0usize;
    let mut last_activity = Instant::now();
    let keep_alive_timeout = Duration::from_secs(config.default_keep_alive_secs as u64 * 3 / 2);

    defmt::debug!("Session {} entering main loop", session_id);

    // Main connection loop
    loop {
        // Calculate timeout duration
        let elapsed = last_activity.elapsed();
        let time_until_timeout = if elapsed > keep_alive_timeout {
            Duration::from_secs(0)
        } else {
            keep_alive_timeout - elapsed
        };

        // Check if timeout occurred before trying to read
        if elapsed >= keep_alive_timeout {
            defmt::info!("Session {} keep-alive timeout", session_id);
            break;
        }

        // CRITICAL: Use select3 to wait for THREE possible events:
        // 1. Incoming data from client (socket.read)
        // 2. Notification from broker that outbound packets are ready (notify_receiver.receive())
        // 3. Keep-alive timeout
        //
        // Without the notification channel, idle clients would never receive messages
        // because socket.read() blocks indefinitely.
        let select_result = select3(
            socket.read(&mut rx_buffer[buffer_len..]),
            notify_receiver.receive(),
            embassy_time::Timer::after(time_until_timeout),
        )
        .await;

        match select_result {
            Either3::First(result) => {
                // Data received from socket
                match result {
                    Ok(0) => {
                        // EOF - client closed connection
                        defmt::info!("Session {} client closed connection", session_id);
                        break;
                    }
                    Ok(n) => {
                        buffer_len += n;

                        // Try to read a complete packet
                        match read_packet::<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>(
                            socket,
                            &mut rx_buffer,
                            &mut buffer_len,
                        )
                        .await
                        {
                            Ok(Some(packet)) => {
                                last_activity = Instant::now();

                                defmt::debug!(
                                    "Session {} received packet type={}",
                                    session_id,
                                    packet_type_name(&packet)
                                );

                                // Process with broker
                                let mut broker_lock = broker.lock().await;
                                let current_time = current_time_nanos();

                                let result = broker_lock
                                    .queue_packet_received_from_client(
                                        session_id,
                                        packet,
                                        current_time,
                                    )
                                    .and_then(|_| broker_lock.process_all_session_packets());

                                // CRITICAL: Get sessions with pending packets BEFORE dropping lock
                                // This is how we notify OTHER sessions that they have messages waiting
                                let sessions_to_notify = if result.is_ok() {
                                    broker_lock.get_sessions_with_pending_packets()
                                } else {
                                    HeaplessVec::new()
                                };

                                let tx_packets = match result {
                                    Ok(()) => {
                                        let mut packets = heapless::Vec::<
                                            Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>,
                                            8,
                                        >::new(
                                        );
                                        while let Ok(Some(pkt)) =
                                            broker_lock.dequeue_packet_to_send_to_client(session_id)
                                        {
                                            let _ = packets.push(pkt);
                                        }
                                        packets
                                    }
                                    Err(_) => {
                                        defmt::warn!(
                                            "Error processing packet for session {}",
                                            session_id
                                        );
                                        let _ = broker_lock.mark_session_disconnected(session_id);
                                        drop(broker_lock);
                                        break;
                                    }
                                };
                                drop(broker_lock);

                                // CRITICAL: Notify all sessions that have pending packets
                                // This wakes up idle clients so they can receive their messages
                                for session_id_to_notify in sessions_to_notify {
                                    notification_registry
                                        .notify_session(session_id_to_notify)
                                        .await;
                                }

                                // Send packets to client
                                for pkt in tx_packets {
                                    if (write_packet::<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>(
                                        socket,
                                        &pkt,
                                        &mut tx_buffer,
                                    )
                                    .await)
                                        .is_err()
                                    {
                                        defmt::error!("Write error for session {}", session_id);
                                        break;
                                    }
                                }
                            }
                            Ok(None) => {
                                // Incomplete packet, continue reading
                                continue;
                            }
                            Err(_) => {
                                defmt::warn!("Protocol error for session {}", session_id);
                                break;
                            }
                        }
                    }
                    Err(_) => {
                        defmt::warn!("Socket read error for session {}", session_id);
                        break;
                    }
                }
            }
            Either3::Second(_) => {
                // Notification received - broker has queued packets for this session
                defmt::debug!(
                    "Session {} received notification, flushing packets",
                    session_id
                );

                // Dequeue and send all pending packets
                let mut broker_lock = broker.lock().await;
                let mut packets =
                    heapless::Vec::<Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>, 8>::new();
                while let Ok(Some(pkt)) = broker_lock.dequeue_packet_to_send_to_client(session_id) {
                    let _ = packets.push(pkt);
                }
                drop(broker_lock);

                for pkt in packets {
                    if (write_packet::<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>(
                        socket,
                        &pkt,
                        &mut tx_buffer,
                    )
                    .await)
                        .is_err()
                    {
                        defmt::error!("Write error for session {}", session_id);
                        break;
                    }
                }
                // Loop back to wait for more events
            }
            Either3::Third(_) => {
                // Timer fired - keep-alive timeout
                defmt::info!("Session {} keep-alive timeout", session_id);
                break;
            }
        }
    }

    Ok(())
}

/// Get a short name for a packet type (for logging)
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
