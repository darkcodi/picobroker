//! Message types for client-broker communication

use crate::Packet;
use crate::client::ClientId;

/// Messages sent from client task to broker
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ClientToBrokerMessage<const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize> {
    /// Client has successfully connected
    Connected(ClientId),
    /// Packet received from client
    Packet(Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>),
    /// Client has disconnected
    Disconnected,
    /// I/O error occurred
    IoError,
}

/// Messages sent from broker to client task
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BrokerToClientMessage<const MAX_TOPIC_NAME_LENGTH: usize, const MAX_PAYLOAD_SIZE: usize> {
    /// Send a packet to the client
    SendPacket(Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>),
    /// Disconnect the client
    Disconnect,
}
