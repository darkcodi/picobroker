//! MQTT broker implementation
//!
//! Manages client connections, message routing, and keep-alive monitoring

use crate::client::{ClientId, ClientName, ClientRegistry};
use crate::error::Result;
use crate::network::TcpStream;
use crate::protocol::packets::Packet;
use crate::protocol::packets::*;
use crate::protocol::qos::QoS;
use crate::topics::{TopicName, TopicRegistry, TopicSubscription};

pub type DefaultPicoBroker = PicoBroker<30, 30, 4, 4, 4>;

/// MQTT broker
///
/// Main broker structure managing clients and topic subscriptions.
///
/// # Generic Parameters
///
/// - `MAX_CLIENT_NAME_LENGTH`: Maximum length of client identifiers
/// - `MAX_TOPIC_NAME_LENGTH`: Maximum length of topic names
/// - `MAX_CLIENTS`: Maximum number of concurrent clients
/// - `MAX_TOPICS`: Maximum number of distinct topics
/// - `MAX_SUBSCRIBERS_PER_TOPIC`: Maximum subscribers per topic
#[derive(Debug, Default)]
pub struct PicoBroker<
    const MAX_CLIENT_NAME_LENGTH: usize,
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_CLIENTS: usize,
    const MAX_TOPICS: usize,
    const MAX_SUBSCRIBERS_PER_TOPIC: usize,
> {
    clients: ClientRegistry<MAX_CLIENT_NAME_LENGTH, MAX_CLIENTS>,
    topics: TopicRegistry<MAX_TOPIC_NAME_LENGTH, MAX_TOPICS, MAX_SUBSCRIBERS_PER_TOPIC>,
}

impl<const MAX_CLIENT_NAME_LENGTH: usize, const MAX_TOPIC_NAME_LENGTH: usize, const MAX_CLIENTS: usize, const MAX_TOPICS: usize, const MAX_SUBSCRIBERS_PER_TOPIC: usize>
    PicoBroker<MAX_CLIENT_NAME_LENGTH, MAX_TOPIC_NAME_LENGTH, MAX_CLIENTS, MAX_TOPICS, MAX_SUBSCRIBERS_PER_TOPIC>
{
    /// Create a new MQTT broker
    pub const fn new() -> Self {
        Self {
            clients: ClientRegistry::new(),
            topics: TopicRegistry::new(),
        }
    }

    /// Register a new client
    ///
    /// Returns the client ID that should be used for subsequent operations.
    pub fn register_client(&mut self, name: ClientName<MAX_CLIENT_NAME_LENGTH>, keep_alive: u16) -> Result<ClientId> {
        let current_time = Self::get_current_time();
        let client_id = self.clients.register(name.clone(), keep_alive, current_time)?;
        Ok(client_id)
    }

    /// Unregister a client
    pub fn unregister_client(&mut self, name: ClientName<MAX_CLIENT_NAME_LENGTH>) {
        let client_id = self.clients.unregister(&name);
        if let Some(client_id) = client_id {
            self.topics.unregister_client(client_id);
        }
    }

    /// Disconnect a client
    pub fn disconnect_client(&mut self, name: ClientName<MAX_CLIENT_NAME_LENGTH>) {
        self.unregister_client(name);
    }

    /// Update client activity
    pub fn update_client_activity(&mut self, name: &ClientName<MAX_CLIENT_NAME_LENGTH>) {
        let current_time = Self::get_current_time();
        self.clients.update_activity(name, current_time);
    }

    /// Check if client is connected
    pub fn is_client_connected(&self, name: &ClientName<MAX_CLIENT_NAME_LENGTH>) -> bool {
        self.clients.is_connected(name)
    }

    /// Process expired clients
    pub fn process_expired_clients(&mut self) {
        let current_time = Self::get_current_time();
        let expired = self.clients.get_expired_clients(current_time);

        for client_name in expired {
            self.disconnect_client(client_name);
        }
    }

    /// Handle PUBLISH packet
    pub async fn handle_publish<S>(
        &self,
        _client_id: usize,
        publish: &Publish<'_>,
        _stream: &mut S,
    ) -> Result<()>
    where
        S: TcpStream,
    {
        let topic_name = TopicName::try_from(publish.topic_name)?;
        let subscriber_ids = self.topics.get_subscribers(&topic_name);

        // Route to all subscribers (fire-and-forget for QoS 0)
        // Note: In the single-threaded model, we can't directly access other client streams
        // This is a placeholder for the full implementation
        let _ = subscriber_ids;
        let _ = _stream;
        todo!("Route PUBLISH to subscribers: {:?}", subscriber_ids);
    }

    /// Handle SUBSCRIBE packet
    pub async fn handle_subscribe<S>(
        &mut self,
        client_id: usize,
        subscribe: &Subscribe<'_>,
        stream: &mut S,
    ) -> Result<()>
    where
        S: TcpStream,
    {
        let topic_filter = TopicSubscription::try_from(subscribe.topic_filter)?;
        self.topics.subscribe(ClientId::new(client_id), topic_filter)?;

        let suback = SubAck {
            packet_id: subscribe.packet_id,
            granted_qos: QoS::AtMostOnce,
            _phantom: Default::default(),
        };

        let mut buffer = [0u8; 256];
        let packet = Packet::SubAck(suback);
        let len = packet.encode(&mut buffer)?;
        stream.write(&buffer[..len]).await?;

        Ok(())
    }

    /// Handle PINGREQ packet
    pub async fn handle_ping<S>(&self, stream: &mut S) -> Result<()>
    where
        S: TcpStream,
    {
        let pingresp = Packet::PingResp(PingResp::default());
        let mut buffer = [0u8; 256];
        let len = pingresp.encode(&mut buffer)?;
        stream.write(&buffer[..len]).await?;
        Ok(())
    }

    /// Get current time in seconds
    fn get_current_time() -> u64 {
        #[cfg(feature = "std")]
        {
            use std::time::{SystemTime, UNIX_EPOCH};
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_secs()
        }

        #[cfg(not(feature = "std"))]
        {
            // TODO: Implement Embassy time
            0
        }
    }
}
