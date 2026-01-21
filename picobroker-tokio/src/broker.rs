//! Tokio async broker extensions

use crate::network::TcpStream;
use crate::time::StdTimeSource;
use picobroker_core::{
    PicoBroker, Result, TopicName, TopicSubscription, Packet, QoS,
};
use picobroker_core::client::ClientId;
use picobroker_core::protocol::packets::*;

/// Tokio broker with async methods
///
/// Type alias for PicoBroker using StdTimeSource
pub type TokioPicoBroker<
    const MAX_CLIENT_NAME_LENGTH: usize,
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_CLIENTS: usize,
    const MAX_TOPICS: usize,
    const MAX_SUBSCRIBERS_PER_TOPIC: usize,
> = PicoBroker<
    StdTimeSource,
    MAX_CLIENT_NAME_LENGTH,
    MAX_TOPIC_NAME_LENGTH,
    MAX_CLIENTS,
    MAX_TOPICS,
    MAX_SUBSCRIBERS_PER_TOPIC,
>;

/// Extension trait for Tokio async broker methods
pub trait TokioBrokerExt<
    const MAX_CLIENT_NAME_LENGTH: usize,
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_CLIENTS: usize,
    const MAX_TOPICS: usize,
    const MAX_SUBSCRIBERS_PER_TOPIC: usize,
>
{
    /// Create a new Tokio broker
    fn new_tokio() -> Self;

    /// Handle PUBLISH packet
    async fn handle_publish<S>(
        &self,
        client_id: usize,
        publish: &Publish<'_>,
        stream: &mut S,
    ) -> Result<()>
    where
        S: TcpStream;

    /// Handle SUBSCRIBE packet
    async fn handle_subscribe<S>(
        &mut self,
        client_id: usize,
        subscribe: &Subscribe<'_>,
        stream: &mut S,
    ) -> Result<()>
    where
        S: TcpStream;

    /// Handle PINGREQ packet
    async fn handle_ping<S>(&self, stream: &mut S) -> Result<()>
    where
        S: TcpStream;
}

impl<const MAX_CLIENT_NAME_LENGTH: usize, const MAX_TOPIC_NAME_LENGTH: usize, const MAX_CLIENTS: usize, const MAX_TOPICS: usize, const MAX_SUBSCRIBERS_PER_TOPIC: usize>
    TokioBrokerExt<MAX_CLIENT_NAME_LENGTH, MAX_TOPIC_NAME_LENGTH, MAX_CLIENTS, MAX_TOPICS, MAX_SUBSCRIBERS_PER_TOPIC>
    for TokioPicoBroker<MAX_CLIENT_NAME_LENGTH, MAX_TOPIC_NAME_LENGTH, MAX_CLIENTS, MAX_TOPICS, MAX_SUBSCRIBERS_PER_TOPIC>
{
    fn new_tokio() -> Self {
        Self::new(StdTimeSource)
    }

    async fn handle_publish<S>(
        &self,
        _client_id: usize,
        publish: &Publish<'_>,
        _stream: &mut S,
    ) -> Result<()>
    where
        S: TcpStream,
    {
        let topic_name = TopicName::try_from(publish.topic_name)?;
        let subscriber_ids = self.topics().get_subscribers(&topic_name);

        // Route to all subscribers (fire-and-forget for QoS 0)
        // Note: In the single-threaded model, we can't directly access other client streams
        // This is a placeholder for the full implementation
        let _ = subscriber_ids;
        let _ = _stream;
        todo!("Route PUBLISH to subscribers: {:?}", subscriber_ids);
    }

    async fn handle_subscribe<S>(
        &mut self,
        client_id: usize,
        subscribe: &Subscribe<'_>,
        stream: &mut S,
    ) -> Result<()>
    where
        S: TcpStream,
    {
        let topic_filter = TopicSubscription::try_from(subscribe.topic_filter)?;
        self.topics_mut().subscribe(ClientId::new(client_id), topic_filter)?;

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

    async fn handle_ping<S>(&self, stream: &mut S) -> Result<()>
    where
        S: TcpStream,
    {
        let pingresp = Packet::PingResp(PingResp::default());
        let mut buffer = [0u8; 256];
        let len = pingresp.encode(&mut buffer)?;
        stream.write(&buffer[..len]).await?;
        Ok(())
    }
}
