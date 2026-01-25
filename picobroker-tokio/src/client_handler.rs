//! Client handler task for Tokio runtime
//!
//! This module provides the client handler implementation specific to Tokio,
//! using std::sync::Arc and std::sync::Mutex for session sharing.
//!
//! Note: We use std::sync::Mutex instead of tokio::sync::Mutex because:
//! 1. The lock is held very briefly (just to enqueue/dequeue)
//! 2. It avoids needing the tokio "sync" feature
//! 3. It's compatible with no-std design (session is in core, Mutex is in runtime layer)

use picobroker_core::{
    TcpStream,
    ClientId,
    Packet,
    PacketEncoder,
    ClientSession,
    ClientToBrokerMessage,
    BrokerToClientMessage,
};
use std::sync::{Arc, Mutex};
use log::{info, trace};

/// Client handler task for Tokio runtime
///
/// This function runs as a separate task for each connected client.
/// It handles full-duplex I/O:
/// - Reading packets from the network and sending them to the broker
/// - Receiving packets from the broker and writing them to the network
///
/// # Parameters
/// - `stream`: The TCP stream for communicating with the client
/// - `client_id`: The client's identifier (currently unused, for future tracking)
/// - `session`: Arc-wrapped Mutex to the client session for shared access
///
/// # Behavior
/// The task runs in a loop, continuously:
/// 1. Reading packets from the network
/// 2. Enqueueing them to the client_to_broker queue
/// 3. Dequeueing messages from broker_to_client
/// 4. Writing them to the network
///
/// The task exits when:
/// - The client disconnects
/// - A network error occurs
/// - The broker sends a Disconnect message
pub async fn client_handler_task<S, const CB_Q: usize, const BC_Q: usize, const MAX_TOPIC: usize, const MAX_PAYLOAD: usize, const MAX_PACKET: usize>(
    mut stream: S,
    client_id: ClientId,
    session: Arc<Mutex<ClientSession<CB_Q, BC_Q, MAX_TOPIC, MAX_PAYLOAD>>>,
) where
    S: TcpStream,
{
    info!("Client handler task started for {}", client_id);

    // Stack-allocated buffer for packet I/O (no heap allocation)
    let mut buffer = [0u8; MAX_PACKET];

    loop {
        // === READ FROM NETWORK ===
        // Try to read packet header
        match stream.read(&mut buffer[0..1]).await {
            Ok(0) => {
                // EOF - client disconnected gracefully
                info!("Client {} disconnected (EOF)", client_id);
                let _ = session.lock().unwrap().client_to_broker.enqueue(ClientToBrokerMessage::Disconnected);
                break;
            }
            Ok(_) => {
                trace!("Client {} sent packet header", client_id);

                // Decode remaining length (MQTT variable-length encoding)
                let mut remaining_length = 0;
                let mut multiplier = 1;
                let mut offset = 1;

                loop {
                    if offset >= MAX_PACKET {
                        // Packet too large
                        info!("Client {} packet too large", client_id);
                        let _ = session.lock().unwrap().client_to_broker.enqueue(ClientToBrokerMessage::IoError);
                        break;
                    }

                    match stream.read(&mut buffer[offset..offset+1]).await {
                        Ok(0) => {
                            // Unexpected EOF
                            let _ = session.lock().unwrap().client_to_broker.enqueue(ClientToBrokerMessage::IoError);
                            break;
                        }
                        Ok(_) => {
                            let byte = buffer[offset];
                            remaining_length += ((byte & 0x7F) as usize) * multiplier;
                            offset += 1;

                            if byte & 0x80 == 0 {
                                // Done decoding
                                break;
                            }

                            multiplier *= 128;
                            if multiplier > 128 * 128 * 128 {
                                // Malformed remaining length
                                let _ = session.lock().unwrap().client_to_broker.enqueue(ClientToBrokerMessage::IoError);
                                break;
                            }
                        }
                        Err(_) => {
                            let _ = session.lock().unwrap().client_to_broker.enqueue(ClientToBrokerMessage::IoError);
                            break;
                        }
                    }
                }

                let total_packet_size = 1 + offset + remaining_length;

                if total_packet_size > MAX_PACKET {
                    // Packet too large
                    info!("Client {} packet too large", client_id);
                    let _ = session.lock().unwrap().client_to_broker.enqueue(ClientToBrokerMessage::IoError);
                    continue;
                }

                // Read remaining packet data
                let data_start = 1 + offset;
                let mut bytes_read = 0;
                while bytes_read < remaining_length {
                    match stream.read(&mut buffer[data_start + bytes_read..data_start + remaining_length]).await {
                        Ok(0) => {
                            // Unexpected EOF
                            info!("Client {} unexpected EOF during packet read", client_id);
                            let _ = session.lock().unwrap().client_to_broker.enqueue(ClientToBrokerMessage::IoError);
                            break;
                        }
                        Ok(n) => {
                            bytes_read += n;
                        }
                        Err(_) => {
                            info!("Client {} network error during packet read", client_id);
                            let _ = session.lock().unwrap().client_to_broker.enqueue(ClientToBrokerMessage::IoError);
                            break;
                        }
                    }
                }

                // Decode packet using PacketEncoder trait
                let packet_result = Packet::decode(&buffer[..total_packet_size]);

                match packet_result {
                    Ok(packet) => {
                        trace!("Client {} decoded packet successfully", client_id);
                        // Enqueue packet to broker
                        let msg = ClientToBrokerMessage::Packet(packet);
                        let _ = session.lock().unwrap().client_to_broker.enqueue(msg);
                    }
                    Err(_) => {
                        // Protocol error
                        info!("Client {} protocol error decoding packet", client_id);
                        let _ = session.lock().unwrap().client_to_broker.enqueue(ClientToBrokerMessage::IoError);
                        break;
                    }
                }
            }
            Err(_) => {
                // Network error
                info!("Client {} network error reading packet header", client_id);
                let _ = session.lock().unwrap().client_to_broker.enqueue(ClientToBrokerMessage::IoError);
                break;
            }
        }

        // === WRITE TO NETWORK ===
        // Drain broker_to_client queue
        let mut should_disconnect = false;
        let mut messages_to_send = Vec::new(); // Use a temporary Vec to collect messages

        // Collect all messages while holding the lock
        {
            let mut session_guard = session.lock().unwrap();
            while let Some(msg) = session_guard.broker_to_client.dequeue() {
                messages_to_send.push(msg);
            }
        }

        // Write messages to network (without holding the lock)
        for msg in messages_to_send {
            match msg {
                BrokerToClientMessage::SendPacket(packet) => {
                    trace!("Client {} sending packet", client_id);
                    if let Ok(len) = packet.encode(&mut buffer) {
                        if stream.write(&buffer[..len]).await.is_err() {
                            // Write error, abort
                            info!("Client {} write error", client_id);
                            should_disconnect = true;
                            break;
                        }
                    }
                }
                BrokerToClientMessage::Disconnect => {
                    info!("Client {} received disconnect message", client_id);
                    let _ = stream.close().await;
                    should_disconnect = true;
                    break;
                }
            }
        }

        if should_disconnect {
            break;
        }
    }

    // Task terminating - notify broker if not already notified
    info!("Client handler task terminating for {}", client_id);
    let _ = session.lock().unwrap().client_to_broker.enqueue(ClientToBrokerMessage::Disconnected);
}
