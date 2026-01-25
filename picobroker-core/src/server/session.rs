//! Client session management

use crate::client::ClientId;
use crate::server::message::{ClientToBrokerMessage, BrokerToClientMessage};

/// Client session with dual queues
///
/// Maintains the communication channels between a client task
/// and the broker, along with connection state.
#[derive(Debug, PartialEq, Eq)]
pub struct ClientSession<
    const CLIENT_TO_BROKER_QUEUE_SIZE: usize,
    const BROKER_TO_CLIENT_QUEUE_SIZE: usize,
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
> {
    /// Client identifier
    pub client_id: ClientId,

    /// Queue for messages from client to broker
    pub client_to_broker: heapless::spsc::Queue<
        ClientToBrokerMessage<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>,
        CLIENT_TO_BROKER_QUEUE_SIZE
    >,

    /// Queue for messages from broker to client
    pub broker_to_client: heapless::spsc::Queue<
        BrokerToClientMessage<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>,
        BROKER_TO_CLIENT_QUEUE_SIZE
    >,

    /// Whether the client is currently connected
    pub is_connected: bool,
}

impl<const CB_Q: usize, const BC_Q: usize, const MAX_TOPIC: usize, const MAX_PAYLOAD: usize>
    ClientSession<CB_Q, BC_Q, MAX_TOPIC, MAX_PAYLOAD>
{
    /// Create a new client session
    pub const fn new(client_id: ClientId) -> Self {
        Self {
            client_id,
            client_to_broker: heapless::spsc::Queue::new(),
            broker_to_client: heapless::spsc::Queue::new(),
            is_connected: false,
        }
    }

    /// Mark the session as connected
    pub fn connect(&mut self) {
        self.is_connected = true;
    }

    /// Mark the session as disconnected
    pub fn disconnect(&mut self) {
        self.is_connected = false;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Packet, PingReqPacket};

    #[test]
    fn test_session_creation() {
        let id = ClientId::try_from("test_client").unwrap();
        let session = ClientSession::<8, 16, 64, 256>::new(id.clone());

        assert_eq!(session.client_id, id);
        assert!(!session.is_connected);
    }

    #[test]
    fn test_session_connect_disconnect() {
        let id = ClientId::try_from("test_client").unwrap();
        let mut session = ClientSession::<8, 16, 64, 256>::new(id);

        assert!(!session.is_connected);
        session.connect();
        assert!(session.is_connected);
        session.disconnect();
        assert!(!session.is_connected);
    }

    #[test]
    fn test_client_to_broker_queue() {
        let id = ClientId::try_from("test_client").unwrap();
        let mut session = ClientSession::<8, 16, 64, 256>::new(id);

        let ping = Packet::PingReq(PingReqPacket);
        let msg = ClientToBrokerMessage::Packet(ping);

        // Enqueue should succeed
        session.client_to_broker.enqueue(msg).unwrap();

        // Dequeue should return the message
        let received = session.client_to_broker.dequeue().unwrap();
        matches!(received, ClientToBrokerMessage::Packet(_));
    }

    #[test]
    fn test_broker_to_client_queue() {
        let id = ClientId::try_from("test_client").unwrap();
        let mut session = ClientSession::<8, 16, 64, 256>::new(id);

        let ping = Packet::PingReq(PingReqPacket);
        let msg = BrokerToClientMessage::SendPacket(ping);

        // Enqueue should succeed
        session.broker_to_client.enqueue(msg).unwrap();

        // Dequeue should return the message
        let received = session.broker_to_client.dequeue().unwrap();
        matches!(received, BrokerToClientMessage::SendPacket(_));
    }

    #[test]
    fn test_queue_operations() {
        let id = ClientId::try_from("test_client").unwrap();
        let mut session = ClientSession::<4, 16, 64, 256>::new(id);

        // Enqueue should succeed
        let ping = Packet::PingReq(PingReqPacket);
        let msg = ClientToBrokerMessage::Packet(ping);
        assert!(session.client_to_broker.enqueue(msg).is_ok());

        // Dequeue should return the message
        let received = session.client_to_broker.dequeue();
        assert!(received.is_some());
        matches!(received.unwrap(), ClientToBrokerMessage::Packet(_));

        // Queue should be empty now
        assert!(session.client_to_broker.dequeue().is_none());
    }
}
