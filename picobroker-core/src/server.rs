//! MQTT server implementation
//!
//! This module provides the main server loop that accepts client connections
//! and handles the MQTT protocol for concurrent clients.

use crate::client::{ClientName, ClientId};
use crate::error::{Error, Result};
use crate::network::{TcpListener, TcpStream};
use crate::protocol::{
    ConnAck, Packet, PingResp, QoS,
    SubAck, read_variable_length,
};
use crate::topics::{TopicName, TopicSubscription};
use crate::queue::{MessageSender, QueuedMessage};
use crate::{PicoBroker, TaskSpawner, TimeSource};
use heapless::Vec;

/// Maximum packet size (default from protocol module)
const DEFAULT_MAX_PACKET_SIZE: usize = 256;

/// Queued publish message for routing between clients
#[derive(Debug, Clone)]
pub struct QueuedPublish<const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize> {
    pub topic: TopicName<MAX_TOPIC_NAME_LENGTH>,
    pub payload: Vec<u8, MAX_PAYLOAD_SIZE>,
}

/// Registry for client message senders
///
/// Maps ClientId to MessageSender for routing PUBLISH messages to clients.
/// Each client has its own message queue, and this registry stores the sender
/// endpoints for those queues.
pub struct SenderRegistry<
    const MAX_CLIENTS: usize,
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const QUEUE_DEPTH: usize,
> {
    senders: Vec<Option<MessageSender<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_DEPTH>>, MAX_CLIENTS>,
}

impl<
    const MAX_CLIENTS: usize,
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const QUEUE_DEPTH: usize,
> SenderRegistry<MAX_CLIENTS, MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_DEPTH>
{
    /// Create a new sender registry
    pub fn new() -> Self {
        Self {
            senders: Vec::new(),
        }
    }

    /// Register a message sender for a client
    pub fn register(
        &mut self,
        client_id: ClientId,
        sender: MessageSender<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_DEPTH>,
    ) -> Result<()> {
        // Ensure vector has space
        while self.senders.len() <= client_id.get() {
            self.senders.push(None).map_err(|_| Error::MaxClientsReached {
                max_clients: MAX_CLIENTS,
            })?;
        }

        // Register sender
        self.senders[client_id.get()] = Some(sender);
        Ok(())
    }

    /// Get a message sender for a client
    pub fn get(
        &self,
        client_id: ClientId,
    ) -> Option<&MessageSender<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_DEPTH>> {
        self.senders.get(client_id.get())?.as_ref()
    }

    /// Remove a message sender for a client
    pub fn remove(&mut self, client_id: ClientId) {
        if let Some(slot) = self.senders.get_mut(client_id.get()) {
            *slot = None;
        }
    }
}

impl<
    const MAX_CLIENTS: usize,
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const QUEUE_DEPTH: usize,
> Default for SenderRegistry<MAX_CLIENTS, MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_DEPTH>
{
    fn default() -> Self {
        Self::new()
    }
}

/// MQTT server
///
/// Main server structure that accepts client connections and spawns
/// client handler tasks for concurrent connection processing.
pub struct MqttServer<
    'a,
    T: TimeSource,
    L: TcpListener<Stream = S>,
    S: TcpStream,
    TS: TaskSpawner,
    const MAX_CLIENT_NAME_LENGTH: usize = 30,
    const MAX_TOPIC_NAME_LENGTH: usize = 30,
    const MAX_CLIENTS: usize = 4,
    const MAX_TOPICS: usize = 4,
    const MAX_SUBSCRIBERS_PER_TOPIC: usize = 4,
    const MAX_PACKET_SIZE: usize = 256,
    const MAX_PUBLISH_QUEUE_DEPTH: usize = 32,
> {
    listener: L,
    broker: PicoBroker<
        T,
        MAX_CLIENT_NAME_LENGTH,
        MAX_TOPIC_NAME_LENGTH,
        MAX_CLIENTS,
        MAX_TOPICS,
        MAX_SUBSCRIBERS_PER_TOPIC,
    >,
    task_spawner: &'a TS,
    sender_registry: SenderRegistry<
        MAX_CLIENTS,
        MAX_TOPIC_NAME_LENGTH,
        MAX_PACKET_SIZE,
        MAX_PUBLISH_QUEUE_DEPTH,
    >,
}

impl<
        'a,
        T: TimeSource,
        L: TcpListener<Stream = S>,
        S: TcpStream,
        TS: TaskSpawner,
        const MAX_CLIENT_NAME_LENGTH: usize,
        const MAX_TOPIC_NAME_LENGTH: usize,
        const MAX_CLIENTS: usize,
        const MAX_TOPICS: usize,
        const MAX_SUBSCRIBERS_PER_TOPIC: usize,
        const MAX_PACKET_SIZE: usize,
        const MAX_PUBLISH_QUEUE_DEPTH: usize,
    >
    MqttServer<
        'a,
        T,
        L,
        S,
        TS,
        MAX_CLIENT_NAME_LENGTH,
        MAX_TOPIC_NAME_LENGTH,
        MAX_CLIENTS,
        MAX_TOPICS,
        MAX_SUBSCRIBERS_PER_TOPIC,
        MAX_PACKET_SIZE,
        MAX_PUBLISH_QUEUE_DEPTH,
    >
{
    /// Create a new MQTT server
    pub fn new(listener: L, time_source: T, task_spawner: &'a TS) -> Self {
        Self {
            listener,
            broker: PicoBroker::new(time_source),
            task_spawner,
            sender_registry: SenderRegistry::new(),
        }
    }

    /// Run the server (accept and handle connections)
    pub async fn run(&mut self) -> Result<()> {
        loop {
            // Accept new connection
            let (stream, _addr) = self.listener.accept().await?;

            // For now, we'll handle this synchronously
            // In a full implementation, we'd spawn a task here
            // but that requires the full client handler to be implemented
            let _ = stream;
        }
    }

    /// Route a PUBLISH message to all subscribers
    ///
    /// This method looks up all subscribers for the given topic and sends
    /// the message to each one using their registered message sender.
    /// Messages are sent with QoS 0 semantics (fire and forget - drop if queue is full).
    pub fn route_publish(
        &self,
        topic: &TopicName<MAX_TOPIC_NAME_LENGTH>,
        payload: &Vec<u8, MAX_PACKET_SIZE>,
        qos: QoS,
        retain: bool,
    ) {
        // Get subscribers for this topic
        let subscribers = self.broker.topics().get_subscribers(topic);

        // Send to each subscriber
        for client_id in subscribers {
            if let Some(sender) = self.sender_registry.get(client_id) {
                // Convert topic to heapless::String
                let topic_str = heapless::String::try_from(topic.as_str())
                    .unwrap_or_else(|_| {
                        // This should never happen since topic is already bounded
                        heapless::String::new()
                    });

                // Create queued message
                let message = QueuedMessage {
                    topic: topic_str,
                    payload: payload.clone(),
                    qos,
                    retain,
                };

                // Send with QoS 0 semantics (drop if full)
                sender.send_or_drop(message);
            }
        }
    }
}

/// Client handler - manages a single client connection
pub struct ClientHandler<
    'a,
    T: TimeSource,
    L: TcpListener<Stream = S>,
    S: TcpStream,
    TS: TaskSpawner,
    const MAX_CLIENT_NAME_LENGTH: usize,
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_CLIENTS: usize,
    const MAX_TOPICS: usize,
    const MAX_SUBSCRIBERS_PER_TOPIC: usize,
    const MAX_PACKET_SIZE: usize,
    const MAX_PUBLISH_QUEUE_DEPTH: usize,
> {
    server: &'a mut MqttServer<
        'a,
        T,
        L,
        S,
        TS,
        MAX_CLIENT_NAME_LENGTH,
        MAX_TOPIC_NAME_LENGTH,
        MAX_CLIENTS,
        MAX_TOPICS,
        MAX_SUBSCRIBERS_PER_TOPIC,
        MAX_PACKET_SIZE,
        MAX_PUBLISH_QUEUE_DEPTH,
    >,
    stream: S,
    client_id: Option<ClientId>,
    client_name: Option<ClientName<MAX_CLIENT_NAME_LENGTH>>,
    read_buffer: [u8; MAX_PACKET_SIZE],
    read_offset: usize,
}

impl<
        'a,
        T: TimeSource,
        L: TcpListener<Stream = S>,
        S: TcpStream,
        TS: TaskSpawner,
        const MAX_CLIENT_NAME_LENGTH: usize,
        const MAX_TOPIC_NAME_LENGTH: usize,
        const MAX_CLIENTS: usize,
        const MAX_TOPICS: usize,
        const MAX_SUBSCRIBERS_PER_TOPIC: usize,
        const MAX_PACKET_SIZE: usize,
        const MAX_PUBLISH_QUEUE_DEPTH: usize,
    >
    ClientHandler<
        'a,
        T,
        L,
        S,
        TS,
        MAX_CLIENT_NAME_LENGTH,
        MAX_TOPIC_NAME_LENGTH,
        MAX_CLIENTS,
        MAX_TOPICS,
        MAX_SUBSCRIBERS_PER_TOPIC,
        MAX_PACKET_SIZE,
        MAX_PUBLISH_QUEUE_DEPTH,
    >
{
    /// Create a new client handler
    pub fn new(
        server: &'a mut MqttServer<
            'a,
            T,
            L,
            S,
            TS,
            MAX_CLIENT_NAME_LENGTH,
            MAX_TOPIC_NAME_LENGTH,
            MAX_CLIENTS,
            MAX_TOPICS,
            MAX_SUBSCRIBERS_PER_TOPIC,
            MAX_PACKET_SIZE,
            MAX_PUBLISH_QUEUE_DEPTH,
        >,
        stream: S,
    ) -> Self {
        Self {
            server,
            stream,
            client_id: None,
            client_name: None,
            read_buffer: [0; MAX_PACKET_SIZE],
            read_offset: 0,
        }
    }

    /// Handle the client connection
    pub async fn handle(&mut self) -> Result<()> {
        // First, read and validate CONNECT packet
        self.handle_connect().await?;

        // Enter packet processing loop
        loop {
            // Read the complete packet
            let packet = self.read_packet().await?;

            match packet {
                Packet::Publish(publish) => {
                    // Extract owned data from the packet to break lifetime chain
                    let topic_str = publish.topic_name;
                    let topic = heapless::String::try_from(topic_str)
                        .map_err(|_| Error::TopicNameLengthExceeded {
                            max_length: MAX_TOPIC_NAME_LENGTH,
                            actual_length: topic_str.len(),
                        })?;
                    let qos = publish.qos;
                    let retain = publish.retain;

                    // Clone payload (since it's already a Vec with the right size)
                    let payload = publish.payload.clone();

                    // Now we can call handler with owned data
                    self.handle_publish_owned(topic, payload, qos, retain).await?;
                }
                Packet::Subscribe(subscribe) => {
                    // Extract owned data from the packet to break lifetime chain
                    let topic_filter_str = subscribe.topic_filter;
                    let topic_filter = heapless::String::try_from(topic_filter_str)
                        .map_err(|_| Error::TopicNameLengthExceeded {
                            max_length: MAX_TOPIC_NAME_LENGTH,
                            actual_length: topic_filter_str.len(),
                        })?;
                    let packet_id = subscribe.packet_id;

                    // Now we can call handler with owned data
                    self.handle_subscribe_owned(topic_filter, packet_id).await?;
                }
                Packet::Unsubscribe(unsubscribe) => {
                    let topic_filter_str = unsubscribe.topic_filter;
                    let topic_filter = heapless::String::try_from(topic_filter_str)
                        .map_err(|_| Error::TopicNameLengthExceeded {
                            max_length: MAX_TOPIC_NAME_LENGTH,
                            actual_length: topic_filter_str.len(),
                        })?;

                    self.handle_unsubscribe_owned(topic_filter).await?;
                }
                Packet::PingReq(_pingreq) => {
                    self.handle_pingreq_owned().await?;
                }
                Packet::Disconnect(_disconnect) => {
                    break;
                }
                _ => {
                    return Err(Error::InvalidPacketType { packet_type: 0 });
                }
            }
        }

        Ok(())
    }

    /// Read a complete packet from the stream
    async fn read_packet(&mut self) -> Result<Packet<'_>> {
        // Read fixed header (at least 2 bytes: packet type + remaining length)
        if self.read_offset < 2 {
            let n = self.stream.read(&mut self.read_buffer[self.read_offset..]).await?;
            self.read_offset += n;
            if self.read_offset < 2 {
                return Err(Error::IncompletePacket);
            }
        }

        // Parse remaining length
        let (remaining_length, len_bytes) = read_variable_length(&self.read_buffer[1..])?;
        let total_packet_size = 1 + len_bytes + remaining_length;

        // Check if we have the complete packet
        if self.read_offset < total_packet_size {
            let n = self.stream.read(&mut self.read_buffer[self.read_offset..total_packet_size]).await?;
            self.read_offset += n;
            if self.read_offset < total_packet_size {
                return Err(Error::IncompletePacket);
            }
        }

        // Decode the packet
        let packet = Packet::decode(&self.read_buffer[..total_packet_size])?;

        // Reset buffer for next packet
        self.read_offset = 0;

        Ok(packet)
    }

    /// Handle CONNECT packet
    async fn handle_connect(&mut self) -> Result<()> {
        let packet = self.read_packet().await?;

        let connect = match packet {
            Packet::Connect(connect) => connect,
            _ => return Err(Error::InvalidPacketType { packet_type: 0 }),
        };

        // Validate protocol name and level
        let protocol_str: &str = &(connect.protocol_name);
        if protocol_str != "MQTT" {
            return Err(Error::InvalidProtocolName);
        }
        if connect.protocol_level != 4 {
            return Err(Error::InvalidProtocolLevel { level: connect.protocol_level });
        }

        // Extract all needed data to break lifetime chain
        let client_id_str: &str = &(connect.client_id);
        let client_id_owned = heapless::String::try_from(client_id_str)
            .map_err(|_| Error::InvalidClientIdLength { length: client_id_str.len() as u16 })?;
        let keep_alive = connect.keep_alive;

        // Now packet is dropped, we can use self.broker
        let client_name = ClientName::new(client_id_owned);

        // Register client with broker
        let client_id = self.server.broker.register_client(
            client_name.clone(),
            keep_alive,
        )?;

        // Store client info
        self.client_id = Some(client_id);
        self.client_name = Some(client_name);

        // Send CONNACK
        let connack = ConnAck {
            session_present: false,
            return_code: 0u8.into(),
            _phantom: core::marker::PhantomData,
        };

        self.send_packet(Packet::ConnAck(connack)).await?;

        Ok(())
    }

    /// Handle SUBSCRIBE packet with owned data
    async fn handle_subscribe_owned(&mut self, topic_filter: heapless::String<MAX_TOPIC_NAME_LENGTH>, packet_id: u16) -> Result<()> {
        let client_id = self.client_id.ok_or(Error::ClientNotFound)?;
        let client_name = self.client_name.as_ref().ok_or(Error::ClientNotFound)?;

        // Update activity
        self.server.broker.update_client_activity(client_name);

        // Subscribe
        let topic_name = TopicName::new(topic_filter);
        let topic_subscription = TopicSubscription::exact(topic_name);
        self.server.broker.topics_mut().subscribe(client_id, topic_subscription)?;

        // Send SUBACK
        let suback = SubAck {
            packet_id,
            granted_qos: QoS::AtMostOnce,
            _phantom: core::marker::PhantomData,
        };

        self.send_packet(Packet::SubAck(suback)).await?;

        Ok(())
    }

    /// Handle UNSUBSCRIBE packet with owned data
    async fn handle_unsubscribe_owned(&mut self, topic_filter: heapless::String<MAX_TOPIC_NAME_LENGTH>) -> Result<()> {
        let client_id = self.client_id.ok_or(Error::ClientNotFound)?;
        let client_name = self.client_name.as_ref().ok_or(Error::ClientNotFound)?;

        // Update activity
        self.server.broker.update_client_activity(client_name);

        // Unsubscribe
        let topic_name = TopicName::new(topic_filter);
        let topic_subscription = TopicSubscription::exact(topic_name);
        let _ = self.server.broker.topics_mut().unsubscribe(client_id, &topic_subscription);

        Ok(())
    }

    /// Handle PINGREQ packet
    async fn handle_pingreq_owned(&mut self) -> Result<()> {
        let client_name = self.client_name.as_ref().ok_or(Error::ClientNotFound)?;

        // Update activity
        self.server.broker.update_client_activity(client_name);

        // Send PINGRESP
        let pingresp = PingResp {
            _phantom: core::marker::PhantomData,
        };
        self.send_packet(Packet::PingResp(pingresp)).await?;

        Ok(())
    }

    /// Handle PUBLISH packet with owned data
    async fn handle_publish_owned(
        &mut self,
        topic: heapless::String<MAX_TOPIC_NAME_LENGTH>,
        payload: heapless::Vec<u8, 128>,
        qos: QoS,
        retain: bool,
    ) -> Result<()> {
        let client_name = self.client_name.as_ref().ok_or(Error::ClientNotFound)?;

        // Update activity
        self.server.broker.update_client_activity(client_name);

        // Route the publish to all subscribers
        let topic_name = TopicName::new(topic);

        // Convert payload to the target size
        let mut target_payload = heapless::Vec::new();
        for byte in payload.iter() {
            if target_payload.push(*byte).is_err() {
                break; // Payload too large, truncate (QoS 0 semantics)
            }
        }

        self.server.route_publish(&topic_name, &target_payload, qos, retain);

        Ok(())
    }

    /// Send a packet to the client
    async fn send_packet(&mut self, packet: Packet<'_>) -> Result<()> {
        let mut write_buffer = [0u8; MAX_PACKET_SIZE];
        let len = packet.encode(&mut write_buffer)?;
        self.stream.write(&write_buffer[..len]).await?;
        Ok(())
    }

    /// Clean up client connection
    pub fn cleanup(&mut self) {
        if let Some(client_name) = self.client_name.take() {
            self.server.broker.disconnect_client(client_name);
        }
    }
}
