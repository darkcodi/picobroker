//! Tokio-specific MQTT broker server implementation
//!
//! This module provides the full server implementation with Tokio task spawning,
//! using std::sync::Arc and std::sync::Mutex for session sharing between tasks.

use picobroker_core::{
    BrokerError, ClientId, ClientRegistry, TimeSource, TopicRegistry, TcpListener, TaskSpawner,
    ClientToBrokerMessage, BrokerToClientMessage, ClientSession,
    Packet, ConnectPacket, ConnectReturnCode, ConnAckPacket, SubscribePacket,
    SubAckPacket, PublishPacket, PingRespPacket, QoS, TopicSubscription,
};
use picobroker_core::heapless::Vec;
use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use crate::client_handler::client_handler_task;
use crate::network::{TokioTcpListener, TokioTcpStream};
use crate::time::StdTimeSource;
use crate::task::TokioTaskSpawner;
use log::{info, debug, warn, error};

/// Tokio MQTT broker server
///
/// Full server implementation with Tokio task spawning.
/// This is the main server structure that runs the MQTT broker with Tokio runtime.
///
/// # Generic Parameters
///
/// - `MAX_TOPIC_NAME_LENGTH`: Maximum length of topic names
/// - `MAX_PAYLOAD_SIZE`: Maximum payload size for packets
/// - `CLIENT_TO_BROKER_QUEUE_SIZE`: Queue size for client→broker messages
/// - `BROKER_TO_CLIENT_QUEUE_SIZE`: Queue size for broker→client messages
/// - `MAX_CLIENTS`: Maximum number of concurrent clients
/// - `MAX_TOPICS`: Maximum number of distinct topics
/// - `MAX_SUBSCRIBERS_PER_TOPIC`: Maximum subscribers per topic
/// - `MAX_PACKET_SIZE`: Maximum size of a network packet (buffer size)
pub struct TokioPicoBrokerServer<
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const CLIENT_TO_BROKER_QUEUE_SIZE: usize,
    const BROKER_TO_CLIENT_QUEUE_SIZE: usize,
    const MAX_CLIENTS: usize,
    const MAX_TOPICS: usize,
    const MAX_SUBSCRIBERS_PER_TOPIC: usize,
    const MAX_PACKET_SIZE: usize,
> {
    time_source: StdTimeSource,
    listener: TokioTcpListener,
    spawner: TokioTaskSpawner,
    clients: ClientRegistry<MAX_CLIENTS, CLIENT_TO_BROKER_QUEUE_SIZE, BROKER_TO_CLIENT_QUEUE_SIZE, MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>,
    topics: TopicRegistry<MAX_TOPIC_NAME_LENGTH, MAX_TOPICS, MAX_SUBSCRIBERS_PER_TOPIC>,
    /// Tokio-specific session tracking (Arc-wrapped for task sharing)
    tokio_sessions: HashMap<ClientId, Arc<Mutex<ClientSession<CLIENT_TO_BROKER_QUEUE_SIZE, BROKER_TO_CLIENT_QUEUE_SIZE, MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>>>>,
}

impl<
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const CLIENT_TO_BROKER_QUEUE_SIZE: usize,
    const BROKER_TO_CLIENT_QUEUE_SIZE: usize,
    const MAX_CLIENTS: usize,
    const MAX_TOPICS: usize,
    const MAX_SUBSCRIBERS_PER_TOPIC: usize,
    const MAX_PACKET_SIZE: usize,
> TokioPicoBrokerServer<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, CLIENT_TO_BROKER_QUEUE_SIZE, BROKER_TO_CLIENT_QUEUE_SIZE, MAX_CLIENTS, MAX_TOPICS, MAX_SUBSCRIBERS_PER_TOPIC, MAX_PACKET_SIZE>
{
    /// Create a new Tokio MQTT broker server
    ///
    /// # Parameters
    ///
    /// - `bind_addr`: Address to bind the listener to (e.g., "0.0.0.0:1883")
    pub async fn new(bind_addr: &str) -> Result<Self, BrokerError> {
        info!("Starting PicoBroker server on {}", bind_addr);
        let listener = TokioTcpListener::bind(bind_addr).await?;
        let time_source = StdTimeSource;
        let spawner = TokioTaskSpawner::default();
        let clients = ClientRegistry::new();
        let topics = TopicRegistry::new();
        let tokio_sessions = HashMap::new();

        info!("PicoBroker server initialized successfully");
        Ok(Self {
            time_source,
            listener,
            spawner,
            clients,
            topics,
            tokio_sessions,
        })
    }

    /// Run the server main loop
    ///
    /// This method:
    /// 1. Accepts new connections (non-blocking)
    /// 2. Spawns a client handler task for each new connection
    /// 3. Processes client messages (round-robin)
    /// 4. Checks for expired clients
    ///
    /// The server loop runs indefinitely, handling all client connections
    /// and message routing.
    pub async fn run(&mut self) -> Result<(), BrokerError> {
        info!("Starting server main loop");
        loop {
            // 1. Try to accept a connection (non-blocking)
            // Note: This returns immediately if no connection is pending
            if let Ok((stream, addr)) = self.listener.try_accept().await {
                // Generate a client ID from the socket address
                let ip_bytes = addr.ip;
                let ip_str = format!("{}", std::net::Ipv4Addr::from(ip_bytes));
                let client_id = match ClientId::try_from(ip_str.as_str()) {
                    Ok(id) => id,
                    Err(_) => {
                        warn!("Invalid client ID from IP: {}", ip_str);
                        continue; // Skip invalid client IDs
                    }
                };

                info!("New connection from {} (client_id: {})", ip_str, client_id);

                // Create a new client session with dual queues
                let session = Arc::new(Mutex::new(ClientSession::new(client_id.clone())));

                // Track the session in our HashMap
                self.tokio_sessions.insert(client_id.clone(), session.clone());

                // Clone values for the async block
                let client_id_for_task = client_id.clone();
                let session_clone = session.clone();

                // Send Connected message to broker (before spawning task to avoid lock issues)
                {
                    let mut session_guard = session.lock().unwrap();
                    let _ = session_guard.client_to_broker.enqueue(ClientToBrokerMessage::Connected(client_id.clone()));
                }

                // Spawn client handler task
                // Note: We move the stream and the Arc clone into the task
                match self.spawner.spawn(async move {
                    client_handler_task::<TokioTcpStream, CLIENT_TO_BROKER_QUEUE_SIZE, BROKER_TO_CLIENT_QUEUE_SIZE, MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, MAX_PACKET_SIZE>(
                        stream,
                        client_id_for_task,
                        session_clone,
                    ).await;
                }) {
                    Ok(_) => {
                        info!("Spawned client handler task for {}", client_id);
                    },
                    Err(_) => {
                        error!("Failed to spawn task for client {}", client_id);
                        continue; // Failed to spawn task, skip this client
                    }
                }
            }

            // 2. Process client messages (round-robin through all sessions)
            self.process_client_messages().await?;

            // 3. Check for expired clients (keep-alive timeout)
            self.process_expired_clients();

            // 4. Small yield to prevent busy-waiting
            tokio::time::sleep(tokio::time::Duration::from_millis(10)).await;
        }
    }

    /// Process messages from all client queues in round-robin fashion
    async fn process_client_messages(&mut self) -> Result<(), BrokerError> {
        // Collect all messages and client IDs first to avoid borrow checker issues
        let mut messages_to_process = Vec::<(ClientId, ClientToBrokerMessage<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>), MAX_CLIENTS>::new();

        // First pass: collect all messages from all sessions
        for (client_id, session_arc) in self.tokio_sessions.iter() {
            let mut session = session_arc.lock().unwrap();
            while let Some(msg) = session.client_to_broker.dequeue() {
                let _ = messages_to_process.push((client_id.clone(), msg));
            }
        }

        // Second pass: process all collected messages
        for (client_id, msg) in messages_to_process.iter() {
            match msg {
                ClientToBrokerMessage::Connected(_id) => {
                    // Mark session as connected
                    if let Some(session_arc) = self.tokio_sessions.get(client_id) {
                        session_arc.lock().unwrap().connect();
                    }
                    info!("Client {} marked as connected", client_id);
                }
                ClientToBrokerMessage::Packet(packet) => {
                    // Handle the packet
                    info!("Received packet from {}", client_id);
                    self.handle_packet(packet.clone(), client_id).await?;
                }
                ClientToBrokerMessage::Disconnected | ClientToBrokerMessage::IoError => {
                    // Client disconnected or had an error
                    info!("Client {} disconnected", client_id);
                    self.tokio_sessions.remove(client_id);
                    self.clients.unregister(client_id);
                    self.topics.unregister_client(client_id.clone());
                }
            }
        }

        Ok(())
    }

    /// Handle an incoming packet from a client
    async fn handle_packet(
        &mut self,
        packet: Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>,
        client_id: &ClientId,
    ) -> Result<(), BrokerError> {
        match packet {
            Packet::Connect(conn) => self.handle_connect(conn, client_id).await,
            Packet::Subscribe(sub) => self.handle_subscribe(sub, client_id).await,
            Packet::Publish(publ) => self.handle_publish(publ).await,
            Packet::PingReq(_) => self.handle_pingreq(client_id).await,
            Packet::Disconnect(_) => {
                // Client disconnect
                if let Some(session) = self.clients.get_session_mut(client_id) {
                    let client_id = session.client_id.clone();
                    session.disconnect();
                    self.clients.unregister(&client_id);
                    self.topics.unregister_client(client_id);
                }
                Ok(())
            }
            _ => {
                // Unsupported packet type - ignore for now
                Ok(())
            }
        }
    }

    /// Handle CONNECT packet
    async fn handle_connect(
        &mut self,
        conn: ConnectPacket<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>,
        client_id: &ClientId,
    ) -> Result<(), BrokerError> {
        info!("Client {} CONNECT with keep_alive={}s", client_id, conn.keep_alive);

        // TODO: Validate protocol name and version
        // For now, just accept the connection

        // Register client with keep-alive
        let current_time = self.time_source.now_secs();
        self.clients.register(client_id.clone(), conn.keep_alive, current_time)?;

        // Send CONNACK (connection accepted)
        let connack = ConnAckPacket {
            session_present: false,
            return_code: ConnectReturnCode::Accepted,
        };
        if let Some(session_arc) = self.tokio_sessions.get(client_id) {
            let mut session = session_arc.lock().unwrap();
            let _ = session.broker_to_client.enqueue(BrokerToClientMessage::SendPacket(Packet::ConnAck(connack)));
        }

        info!("Sent CONNACK to {}", client_id);
        Ok(())
    }

    /// Handle SUBSCRIBE packet
    async fn handle_subscribe(
        &mut self,
        sub: SubscribePacket<MAX_TOPIC_NAME_LENGTH>,
        client_id: &ClientId,
    ) -> Result<(), BrokerError> {
        info!("Client {} SUBSCRIBE to '{}'", client_id, sub.topic_filter);

        // Subscribe to topic
        let filter = TopicSubscription::exact(sub.topic_filter.clone());
        self.topics.subscribe(client_id.clone(), filter)?;

        // Send SUBACK (grant QoS 0 for now)
        let suback = SubAckPacket {
            packet_id: sub.packet_id,
            granted_qos: QoS::AtMostOnce,
        };
        if let Some(session_arc) = self.tokio_sessions.get(client_id) {
            let mut session = session_arc.lock().unwrap();
            let _ = session.broker_to_client.enqueue(BrokerToClientMessage::SendPacket(Packet::SubAck(suback)));
        }

        info!("Sent SUBACK to {}", client_id);
        Ok(())
    }

    /// Handle PUBLISH packet (QoS 0)
    async fn handle_publish(
        &mut self,
        publ: PublishPacket<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>,
    ) -> Result<(), BrokerError> {
        // Find all subscribers for this topic
        let subscribers = self.topics.get_subscribers(&publ.topic_name);

        info!("PUBLISH to '{}': {} bytes, {} subscribers",
              publ.topic_name, publ.payload.len(), subscribers.len());

        // Forward publish to each subscriber
        for subscriber_id in subscribers {
            if let Some(session_arc) = self.tokio_sessions.get(&subscriber_id) {
                let mut session = session_arc.lock().unwrap();
                if session.is_connected {
                    // Clone packet for each subscriber (QoS 0, so no state tracking needed)
                    let packet = Packet::Publish(publ.clone());
                    let _ = session.broker_to_client.enqueue(BrokerToClientMessage::SendPacket(packet));
                    info!("Forwarded PUBLISH to {}", subscriber_id);
                }
            }
        }

        Ok(())
    }

    /// Handle PINGREQ packet
    async fn handle_pingreq(
        &mut self,
        client_id: &ClientId,
    ) -> Result<(), BrokerError> {
        info!("Client {} PINGREQ", client_id);

        // Send PINGRESP
        let pingresp = PingRespPacket;
        if let Some(session_arc) = self.tokio_sessions.get(client_id) {
            let mut session = session_arc.lock().unwrap();
            let _ = session.broker_to_client.enqueue(BrokerToClientMessage::SendPacket(Packet::PingResp(pingresp)));

            // Update activity timestamp for keep-alive
            let current_time = self.time_source.now_secs();
            self.clients.update_activity(client_id, current_time);
        }

        Ok(())
    }

    /// Process expired clients based on keep-alive timeout
    fn process_expired_clients(&mut self) {
        let current_time = self.time_source.now_secs();
        let expired = self.clients.get_expired_clients(current_time);

        for client_id in expired {
            warn!("Client {} expired (keep-alive timeout)", client_id);
            // Disconnect the session
            if let Some(session_arc) = self.tokio_sessions.get(&client_id) {
                session_arc.lock().unwrap().disconnect();
            }
            // Remove from tokio sessions
            self.tokio_sessions.remove(&client_id);
            // Unregister from clients and topics
            self.clients.unregister(&client_id);
            self.topics.unregister_client(client_id);
        }
    }
}
