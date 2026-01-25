//! MQTT broker implementation (core sync logic)
//!
//! Manages client connections, message routing, and keep-alive monitoring

use crate::client::{ClientId, ClientRegistry};
use crate::time::TimeSource;
use crate::topics::TopicRegistry;
use crate::BrokerError;
use crate::server::{ClientToBrokerMessage, BrokerToClientMessage, ClientSession};
use crate::{Packet, ConnectPacket, SubscribePacket, PublishPacket, ConnAckPacket, SubAckPacket, PingRespPacket, ConnectReturnCode};
use crate::network::TcpStream;
use crate::protocol::QoS;
use crate::topics::TopicSubscription;

/// MQTT broker (core sync logic)
///
/// Main broker structure managing clients and topic subscriptions.
/// This is the sync core that is platform-agnostic and no_std compatible.
///
/// # Generic Parameters
///
/// - `T`: Time source for tracking keep-alives
/// - `MAX_TOPIC_NAME_LENGTH`: Maximum length of topic names
/// - `MAX_CLIENTS`: Maximum number of concurrent clients
/// - `MAX_TOPICS`: Maximum number of distinct topics
/// - `MAX_SUBSCRIBERS_PER_TOPIC`: Maximum subscribers per topic
/// - `CLIENT_TO_BROKER_QUEUE_SIZE`: Queue size for client→broker messages
/// - `BROKER_TO_CLIENT_QUEUE_SIZE`: Queue size for broker→client messages
/// - `MAX_PAYLOAD_SIZE`: Maximum payload size for packets
#[derive(Debug)]
pub struct PicoBroker<
    T: TimeSource,
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_CLIENTS: usize,
    const MAX_TOPICS: usize,
    const MAX_SUBSCRIBERS_PER_TOPIC: usize,
    const CLIENT_TO_BROKER_QUEUE_SIZE: usize,
    const BROKER_TO_CLIENT_QUEUE_SIZE: usize,
    const MAX_PAYLOAD_SIZE: usize,
> {
    time_source: T,
    clients: ClientRegistry<MAX_CLIENTS, CLIENT_TO_BROKER_QUEUE_SIZE, BROKER_TO_CLIENT_QUEUE_SIZE, MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>,
    topics: TopicRegistry<MAX_TOPIC_NAME_LENGTH, MAX_TOPICS, MAX_SUBSCRIBERS_PER_TOPIC>,
}

impl<
        T: TimeSource,
        const MAX_TOPIC_NAME_LENGTH: usize,
        const MAX_CLIENTS: usize,
        const MAX_TOPICS: usize,
        const MAX_SUBSCRIBERS_PER_TOPIC: usize,
        const CLIENT_TO_BROKER_QUEUE_SIZE: usize,
        const BROKER_TO_CLIENT_QUEUE_SIZE: usize,
        const MAX_PAYLOAD_SIZE: usize,
    > PicoBroker<T, MAX_TOPIC_NAME_LENGTH, MAX_CLIENTS, MAX_TOPICS, MAX_SUBSCRIBERS_PER_TOPIC, CLIENT_TO_BROKER_QUEUE_SIZE, BROKER_TO_CLIENT_QUEUE_SIZE, MAX_PAYLOAD_SIZE>
{
    /// Create a new MQTT broker with the given time source
    pub fn new(time_source: T) -> Self {
        Self {
            time_source,
            clients: ClientRegistry::new(),
            topics: TopicRegistry::new(),
        }
    }

    /// Register a new client
    ///
    /// Returns the client ID that should be used for subsequent operations.
    pub fn register_client(&mut self, id: ClientId, keep_alive: u16) -> Result<(), BrokerError> {
        let current_time = self.time_source.now_secs();
        self.clients.register(id, keep_alive, current_time)?;
        Ok(())
    }

    /// Unregister a client
    pub fn unregister_client(&mut self, id: ClientId) {
        self.clients.unregister(&id);
        self.topics.unregister_client(id);
    }

    /// Disconnect a client
    pub fn disconnect_client(&mut self, id: ClientId) {
        self.unregister_client(id);
    }

    /// Update client activity
    pub fn update_client_activity(&mut self, id: &ClientId) {
        let current_time = self.time_source.now_secs();
        self.clients.update_activity(id, current_time);
    }

    /// Check if client is connected
    pub fn is_client_connected(&self, id: &ClientId) -> bool {
        self.clients.is_connected(id)
    }

    /// Process expired clients
    pub fn process_expired_clients(&mut self) {
        let current_time = self.time_source.now_secs();
        let expired = self.clients.get_expired_clients(current_time);

        for client_name in expired {
            self.disconnect_client(client_name);
        }
    }

    /// Get reference to the topics registry
    pub fn topics(
        &self,
    ) -> &TopicRegistry<MAX_TOPIC_NAME_LENGTH, MAX_TOPICS, MAX_SUBSCRIBERS_PER_TOPIC> {
        &self.topics
    }

    /// Get mutable reference to the topics registry
    pub fn topics_mut(
        &mut self,
    ) -> &mut TopicRegistry<MAX_TOPIC_NAME_LENGTH, MAX_TOPICS, MAX_SUBSCRIBERS_PER_TOPIC> {
        &mut self.topics
    }

    /// Get reference to the clients registry
    pub fn clients(&self) -> &ClientRegistry<MAX_CLIENTS, CLIENT_TO_BROKER_QUEUE_SIZE, BROKER_TO_CLIENT_QUEUE_SIZE, MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE> {
        &self.clients
    }
}

/// MQTT Broker Server
///
/// Full server implementation with accept loop, client task spawning,
/// and message processing. This is the main server structure that
/// runs the MQTT broker.
///
/// # Generic Parameters
///
/// - `T`: Time source for tracking keep-alives
/// - `L`: TCP listener implementation (TokioTcpListener, EmbassyTcpListener)
/// - `S`: Task spawner implementation (TokioTaskSpawner, EmbassyTaskSpawner)
/// - `MAX_TOPIC_NAME_LENGTH`: Maximum length of topic names
/// - `MAX_PAYLOAD_SIZE`: Maximum payload size for packets
/// - `CLIENT_TO_BROKER_QUEUE_SIZE`: Queue size for client→broker messages
/// - `BROKER_TO_CLIENT_QUEUE_SIZE`: Queue size for broker→client messages
/// - `MAX_CLIENTS`: Maximum number of concurrent clients
/// - `MAX_TOPICS`: Maximum number of distinct topics
/// - `MAX_SUBSCRIBERS_PER_TOPIC`: Maximum subscribers per topic
/// - `MAX_PACKET_SIZE`: Maximum size of a network packet (buffer size)
pub struct PicoBrokerServer<
    T: TimeSource,
    L: crate::network::TcpListener,
    S: crate::task::TaskSpawner,
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const CLIENT_TO_BROKER_QUEUE_SIZE: usize,
    const BROKER_TO_CLIENT_QUEUE_SIZE: usize,
    const MAX_CLIENTS: usize,
    const MAX_TOPICS: usize,
    const MAX_SUBSCRIBERS_PER_TOPIC: usize,
    const MAX_PACKET_SIZE: usize,
> {
    time_source: T,
    listener: L,
    spawner: S,
    clients: ClientRegistry<MAX_CLIENTS, CLIENT_TO_BROKER_QUEUE_SIZE, BROKER_TO_CLIENT_QUEUE_SIZE, MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>,
    topics: TopicRegistry<MAX_TOPIC_NAME_LENGTH, MAX_TOPICS, MAX_SUBSCRIBERS_PER_TOPIC>,
    io_buffer: [u8; MAX_PACKET_SIZE],
}

impl<
        T: TimeSource,
        L: crate::network::TcpListener,
        S: crate::task::TaskSpawner,
        const MAX_TOPIC_NAME_LENGTH: usize,
        const MAX_PAYLOAD_SIZE: usize,
        const CLIENT_TO_BROKER_QUEUE_SIZE: usize,
        const BROKER_TO_CLIENT_QUEUE_SIZE: usize,
        const MAX_CLIENTS: usize,
        const MAX_TOPICS: usize,
        const MAX_SUBSCRIBERS_PER_TOPIC: usize,
        const MAX_PACKET_SIZE: usize,
    > PicoBrokerServer<T, L, S, MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, CLIENT_TO_BROKER_QUEUE_SIZE, BROKER_TO_CLIENT_QUEUE_SIZE, MAX_CLIENTS, MAX_TOPICS, MAX_SUBSCRIBERS_PER_TOPIC, MAX_PACKET_SIZE>
{
    /// Create a new MQTT broker server
    pub fn new(time_source: T, listener: L, spawner: S) -> Self {
        Self {
            time_source,
            listener,
            spawner,
            clients: ClientRegistry::new(),
            topics: TopicRegistry::new(),
            io_buffer: [0u8; MAX_PACKET_SIZE],
        }
    }

    /// Run the server main loop
    ///
    /// This method:
    /// 1. Tries to accept new connections (non-blocking)
    /// 2. Processes client messages (round-robin)
    /// 3. Checks for expired clients
    ///
    /// The server loop runs indefinitely, handling all client connections
    /// and message routing.
    pub async fn run(&mut self) -> Result<(), BrokerError> {
        use TcpStream;

        loop {
            // 1. Try to accept a connection (non-blocking)
            // Note: This returns immediately if no connection is pending
            if let Ok((mut stream, _addr)) = self.listener.try_accept().await {
                // TODO: Spawn client handler task for this connection
                // For now, just close the stream
                let _ = stream.close().await;
            }

            // 2. Process client messages (round-robin through all sessions)
            self.process_client_messages().await?;

            // 3. Check for expired clients (keep-alive timeout)
            self.process_expired_clients();

            // 4. Small yield to prevent busy-waiting
            // In a real implementation, this would be an async sleep
            // For now, we just continue the loop
        }
    }

    /// Process messages from all client queues in round-robin fashion
    async fn process_client_messages(&mut self) -> Result<(), BrokerError> {
        // Collect all messages and client IDs first to avoid borrow checker issues
        let mut messages_to_process = heapless::Vec::<(ClientId, ClientToBrokerMessage<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>), MAX_CLIENTS>::new();

        // First pass: collect all messages from all sessions
        for session_slot in self.clients.sessions_mut().iter_mut() {
            if let Some(session) = session_slot {
                let client_id = session.client_id.clone();
                while let Some(msg) = session.client_to_broker.dequeue() {
                    let _ = messages_to_process.push((client_id.clone(), msg));
                }
            }
        }

        // Second pass: process all collected messages
        for (client_id, msg) in messages_to_process.iter() {
            match msg {
                ClientToBrokerMessage::Connected(_id) => {
                    // Mark session as connected
                    if let Some(session) = self.clients.get_session_mut(client_id) {
                        session.connect();
                    }
                }
                ClientToBrokerMessage::Packet(packet) => {
                    // Handle the packet
                    self.handle_packet(packet.clone(), client_id).await?;
                }
                ClientToBrokerMessage::Disconnected | ClientToBrokerMessage::IoError => {
                    // Client disconnected or had an error
                    if let Some(session) = self.clients.get_session_mut(client_id) {
                        let client_id = session.client_id.clone();
                        session.disconnect();
                        self.clients.unregister(&client_id);
                        self.topics.unregister_client(client_id);
                    }
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
        if let Some(session) = self.clients.get_session_mut(client_id) {
            let _ = session.broker_to_client.enqueue(BrokerToClientMessage::SendPacket(Packet::ConnAck(connack)));
        }

        Ok(())
    }

    /// Handle SUBSCRIBE packet
    async fn handle_subscribe(
        &mut self,
        sub: SubscribePacket<MAX_TOPIC_NAME_LENGTH>,
        client_id: &ClientId,
    ) -> Result<(), BrokerError> {
        // Subscribe to topic
        let filter = TopicSubscription::exact(sub.topic_filter.clone());
        self.topics.subscribe(client_id.clone(), filter)?;

        // Send SUBACK (grant QoS 0 for now)
        let suback = SubAckPacket {
            packet_id: sub.packet_id,
            granted_qos: QoS::AtMostOnce,
        };
        if let Some(session) = self.clients.get_session_mut(client_id) {
            let _ = session.broker_to_client.enqueue(BrokerToClientMessage::SendPacket(Packet::SubAck(suback)));
        }

        Ok(())
    }

    /// Handle PUBLISH packet (QoS 0)
    async fn handle_publish(
        &mut self,
        publ: PublishPacket<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>,
    ) -> Result<(), BrokerError> {
        // Find all subscribers for this topic
        let subscribers = self.topics.get_subscribers(&publ.topic_name);

        // Forward publish to each subscriber
        for subscriber_id in subscribers {
            if let Some(subscriber_session) = self.clients.get_session_mut(&subscriber_id) {
                if subscriber_session.is_connected {
                    // Clone packet for each subscriber (QoS 0, so no state tracking needed)
                    let packet = Packet::Publish(publ.clone());
                    let _ = subscriber_session.broker_to_client.enqueue(BrokerToClientMessage::SendPacket(packet));
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
        // Send PINGRESP
        let pingresp = PingRespPacket;
        if let Some(session) = self.clients.get_session_mut(client_id) {
            let _ = session.broker_to_client.enqueue(BrokerToClientMessage::SendPacket(Packet::PingResp(pingresp)));

            // Update activity timestamp for keep-alive
            let current_time = self.time_source.now_secs();
            self.clients.update_activity(client_id, current_time);
        }

        Ok(())
    }

    /// Handle client disconnect
    fn handle_disconnect(
        &mut self,
        session: &mut ClientSession<CLIENT_TO_BROKER_QUEUE_SIZE, BROKER_TO_CLIENT_QUEUE_SIZE, MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>,
    ) {
        let client_id = session.client_id.clone();
        session.disconnect();
        self.clients.unregister(&client_id);
        self.topics.unregister_client(client_id);
    }

    /// Process expired clients based on keep-alive timeout
    fn process_expired_clients(&mut self) {
        let current_time = self.time_source.now_secs();
        let expired = self.clients.get_expired_clients(current_time);

        for client_id in expired {
            // Disconnect the client (inline to avoid borrow checker issues)
            if let Some(session) = self.clients.get_session_mut(&client_id) {
                session.disconnect();
            }
            self.clients.unregister(&client_id);
            self.topics.unregister_client(client_id);
        }
    }
}
