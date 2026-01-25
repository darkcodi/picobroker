//! MQTT broker implementation (core logic)
//!
//! Manages client connections, message routing, and keep-alive monitoring

use crate::client::{ClientRegistry};
use crate::{BrokerError, SocketAddr, TcpStream};
use crate::topics::TopicRegistry;

/// MQTT broker (core logic)
///
/// Main broker structure managing clients and topic subscriptions.
/// This is the core that is platform-agnostic and no_std compatible.
///
/// # Generic Parameters
///
/// - `MAX_TOPIC_NAME_LENGTH`: Maximum length of topic names
/// - `MAX_PAYLOAD_SIZE`: Maximum payload size for packets
/// - `QUEUE_SIZE`: Queue size for client -> broker messages and broker -> client messages
/// - `MAX_CLIENTS`: Maximum number of concurrent clients
/// - `MAX_TOPICS`: Maximum number of distinct topics
/// - `MAX_SUBSCRIBERS_PER_TOPIC`: Maximum subscribers per topic
/// - `S`: TCP stream type
#[derive(Debug)]
pub struct PicoBroker<
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const QUEUE_SIZE: usize,
    const MAX_CLIENTS: usize,
    const MAX_TOPICS: usize,
    const MAX_SUBSCRIBERS_PER_TOPIC: usize,
    S: TcpStream,
> {
    pub clients: ClientRegistry<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_SIZE, MAX_CLIENTS, S>,
    pub topics: TopicRegistry<MAX_TOPIC_NAME_LENGTH, MAX_TOPICS, MAX_SUBSCRIBERS_PER_TOPIC>,
}

impl<
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const QUEUE_SIZE: usize,
    const MAX_CLIENTS: usize,
    const MAX_TOPICS: usize,
    const MAX_SUBSCRIBERS_PER_TOPIC: usize,
    S: TcpStream,
    > PicoBroker<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE, QUEUE_SIZE, MAX_CLIENTS, MAX_TOPICS, MAX_SUBSCRIBERS_PER_TOPIC, S>
{
    /// Create a new MQTT broker with the given time source
    pub fn new() -> Self {
        Self {
            clients: ClientRegistry::new(),
            topics: TopicRegistry::new(),
        }
    }

    /// Register a new client with the broker
    pub fn register_new_client(&mut self, socket_addr: SocketAddr, keep_alive_secs: u16, current_time: u64) -> Result<usize, BrokerError> {
        let session_id = self.clients.register_new_client(socket_addr, keep_alive_secs, current_time)?;
        Ok(session_id)
    }
}
