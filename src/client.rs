//! Client session management
//!
//! Handles individual MQTT client connections and state

use crate::error::{Error, Result};
use crate::packet::Packet;
use crate::topics::TopicFilter;
use embassy_net::tcp::TcpSocket;
use embedded_io_async::Write;

/// Client state machine
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ClientState {
    Connecting,
    Connected,
    Disconnected,
}

/// Actions returned from packet handling
#[derive(Debug)]
pub enum Action<'a> {
    None,
    SendConnack,
    SendSuback { packet_id: u16, count: u8 },
    PublishToTopic { topic: &'a str, payload: &'a [u8] },
    Close,
}

/// Connected MQTT client session
pub struct Client<'a> {
    pub socket: TcpSocket<'a>,
    pub client_id: Option<heapless::String<32>>,
    pub subscriptions: heapless::Vec<TopicFilter, 16>,
    pub state: ClientState,
    pub keep_alive: u16, // Keep-alive in seconds from CONNECT
}

impl<'a> Client<'a> {
    pub fn new(socket: TcpSocket<'a>) -> Self {
        Self {
            socket,
            client_id: None,
            subscriptions: heapless::Vec::new(),
            state: ClientState::Connecting,
            keep_alive: 60, // Default 60 seconds
        }
    }

    pub fn is_connected(&self) -> bool {
        self.state == ClientState::Connected
    }

    /// Read a packet from the socket
    pub async fn read_packet<'b>(&mut self, read_buf: &'b mut [u8]) -> Result<Option<Packet<'b>>> {
        // Read at least the fixed header (2 bytes minimum)
        let mut header_buf = [0u8; 5];

        // Read first byte
        match self.socket.read(&mut header_buf[0..1]).await {
            Ok(0) => return Ok(None), // Connection closed
            Ok(_) => {}
            Err(_) => return Err(Error::Io),
        }
        let mut n = 1; // Start with first byte already read

        // Read remaining length bytes (1-4 bytes)
        let mut bytes_read = 0;
        loop {
            if n >= header_buf.len() {
                return Err(Error::InvalidPacket);
            }

            match self.socket.read(&mut header_buf[n..n + 1]).await {
                Ok(0) => return Ok(None),
                Ok(_) => {
                    bytes_read += 1;
                    n += 1;
                }
                Err(_) => return Err(Error::Io),
            }

            // Check continuation bit
            if header_buf[n - 1] & 0x80 == 0 {
                break;
            }

            if bytes_read > 3 {
                return Err(Error::InvalidPacket);
            }
        }

        // Decode remaining length
        let (remaining_length, len_bytes) =
            crate::packet::decode_remaining_length(&header_buf[1..n])?;

        let total_packet_size = 1 + len_bytes + remaining_length;

        if total_packet_size > read_buf.len() {
            return Err(Error::BufferTooSmall);
        }

        // Copy header to read buffer
        read_buf[0..n].copy_from_slice(&header_buf[0..n]);

        // Read remaining payload
        if remaining_length > 0 {
            let payload_start = n;
            let mut total_read = 0;

            while total_read < remaining_length {
                match self
                    .socket
                    .read(&mut read_buf[payload_start + total_read..payload_start + remaining_length])
                    .await
                {
                    Ok(0) => return Ok(None),
                    Ok(read) => total_read += read,
                    Err(_) => return Err(Error::Io),
                }
            }
        }

        // Parse the packet
        let (_header, packet) = crate::packet::parse_packet(&read_buf[..total_packet_size])?;
        Ok(Some(packet))
    }

    /// Send CONNACK response
    pub async fn send_connack(&mut self) -> Result<()> {
        let connack = crate::packet::encode_connack();
        self.socket.write_all(&connack).await.map_err(|_| Error::Io)?;
        Ok(())
    }

    /// Send SUBACK response
    pub async fn send_suback(&mut self, packet_id: u16, count: u8) -> Result<()> {
        let suback = crate::packet::encode_suback(packet_id, count);
        self.socket.write_all(&suback.as_slice()).await.map_err(|_| Error::Io)?;
        Ok(())
    }

    /// Handle a received packet
    pub fn handle_packet<'b>(&mut self, packet: Packet<'b>) -> Result<Action<'b>> {
        match packet {
            Packet::Connect(connect) => {
                // Validate protocol
                if connect.protocol_level != 4 {
                    return Err(Error::InvalidPacket);
                }

                self.client_id = Some(
                    heapless::String::try_from(connect.client_id)
                        .unwrap_or_else(|_| heapless::String::new()),
                );
                self.keep_alive = connect.keep_alive;
                self.state = ClientState::Connected;

                defmt::info!(
                    "Client connected: {} (keep-alive: {}s)",
                    connect.client_id,
                    connect.keep_alive
                );

                Ok(Action::SendConnack)
            }

            Packet::Publish(publish) => {
                if !self.is_connected() {
                    return Err(Error::InvalidPacket);
                }

                defmt::debug!(
                    "PUBLISH from {}: topic={}, payload_len={}",
                    self.client_id.as_ref().map(|s| s.as_str()).unwrap_or("unknown"),
                    publish.topic_name,
                    publish.payload.len()
                );

                Ok(Action::PublishToTopic {
                    topic: publish.topic_name,
                    payload: publish.payload,
                })
            }

            Packet::Subscribe(subscribe) => {
                if !self.is_connected() {
                    return Err(Error::InvalidPacket);
                }

                for topic in subscribe.topics.iter() {
                    if let Ok(filter) = TopicFilter::new(topic) {
                        if self.subscriptions.push(filter).is_err() {
                            defmt::warn!("Max subscriptions reached for client");
                            break;
                        }
                    }
                }

                defmt::info!(
                    "Client {} subscribed to {} topics",
                    self.client_id.as_ref().map(|s| s.as_str()).unwrap_or("unknown"),
                    subscribe.topics.len()
                );

                Ok(Action::SendSuback {
                    packet_id: subscribe.packet_id,
                    count: subscribe.topics.len() as u8,
                })
            }

            Packet::Disconnect => {
                defmt::info!(
                    "Client disconnected: {}",
                    self.client_id.as_ref().map(|s| s.as_str()).unwrap_or("unknown")
                );

                self.state = ClientState::Disconnected;
                Ok(Action::Close)
            }
        }
    }

    /// Close the client connection
    pub async fn close(&mut self) {
        let _ = self.socket.close();
        self.state = ClientState::Disconnected;
    }
}
