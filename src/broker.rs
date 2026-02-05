use crate::error::BrokerError;
use crate::protocol::heapless::HeaplessVec;
use crate::protocol::packets::{
    ConnAckPacket, ConnectPacket, Packet, PingRespPacket, PubAckPacket, PublishPacket,
    SubAckPacket, SubscribePacket,
};
use crate::protocol::qos::QoS;
use crate::session::{ExpirationInfo, SessionRegistry};
use crate::topics::TopicRegistry;

#[derive(Debug)]
pub struct PicoBroker<
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const QUEUE_SIZE: usize,
    const MAX_SESSIONS: usize,
    const MAX_TOPICS: usize,
    const MAX_SUBSCRIBERS_PER_TOPIC: usize,
> {
    topics: TopicRegistry<MAX_TOPIC_NAME_LENGTH, MAX_TOPICS, MAX_SUBSCRIBERS_PER_TOPIC>,
    sessions: SessionRegistry<
        MAX_TOPIC_NAME_LENGTH,
        MAX_PAYLOAD_SIZE,
        QUEUE_SIZE,
        MAX_SESSIONS,
        MAX_TOPICS,
        MAX_SUBSCRIBERS_PER_TOPIC,
    >,
}

impl<
        const MAX_TOPIC_NAME_LENGTH: usize,
        const MAX_PAYLOAD_SIZE: usize,
        const QUEUE_SIZE: usize,
        const MAX_SESSIONS: usize,
        const MAX_TOPICS: usize,
        const MAX_SUBSCRIBERS_PER_TOPIC: usize,
    > Default
    for PicoBroker<
        MAX_TOPIC_NAME_LENGTH,
        MAX_PAYLOAD_SIZE,
        QUEUE_SIZE,
        MAX_SESSIONS,
        MAX_TOPICS,
        MAX_SUBSCRIBERS_PER_TOPIC,
    >
{
    fn default() -> Self {
        Self {
            topics: TopicRegistry::default(),
            sessions: SessionRegistry::default(),
        }
    }
}

impl<
        const MAX_TOPIC_NAME_LENGTH: usize,
        const MAX_PAYLOAD_SIZE: usize,
        const QUEUE_SIZE: usize,
        const MAX_SESSIONS: usize,
        const MAX_TOPICS: usize,
        const MAX_SUBSCRIBERS_PER_TOPIC: usize,
    >
    PicoBroker<
        MAX_TOPIC_NAME_LENGTH,
        MAX_PAYLOAD_SIZE,
        QUEUE_SIZE,
        MAX_SESSIONS,
        MAX_TOPICS,
        MAX_SUBSCRIBERS_PER_TOPIC,
    >
{
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register_new_session(
        &mut self,
        session_id: u128,
        keep_alive_secs: u16,
        current_time: u128,
    ) -> Result<(), BrokerError> {
        self.sessions
            .register_new_session(session_id, keep_alive_secs, current_time)
    }

    pub fn mark_session_disconnected(&mut self, session_id: u128) -> Result<(), BrokerError> {
        self.sessions.mark_disconnected(session_id)
    }

    pub fn remove_session(&mut self, session_id: u128) -> bool {
        self.topics.unregister_all(session_id);
        self.sessions.remove_session(session_id)
    }

    pub fn get_all_sessions(&self) -> [Option<u128>; MAX_SESSIONS] {
        self.sessions.get_all_sessions()
    }

    pub fn get_expired_sessions(&mut self, current_time: u128) -> [Option<ExpirationInfo>; MAX_SESSIONS] {
        self.sessions.get_expired_sessions(current_time)
    }

    pub fn get_disconnected_sessions(&mut self) -> [Option<u128>; MAX_SESSIONS] {
        self.sessions.get_disconnected_sessions()
    }

    pub fn queue_packet_received_from_client(
        &mut self,
        session_id: u128,
        packet: Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>,
        current_time: u128,
    ) -> Result<(), BrokerError> {
        self.sessions
            .update_session_activity(session_id, current_time)?;
        self.sessions.queue_rx_packet(session_id, packet)
    }

    pub fn dequeue_packet_to_send_to_client(
        &mut self,
        session_id: u128,
    ) -> Result<Option<Packet<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>>, BrokerError> {
        self.sessions.dequeue_tx_packet(session_id)
    }

    pub fn process_all_session_packets(&mut self) -> Result<(), BrokerError> {
        let session_ids = self.get_all_sessions();

        for session_id in session_ids.into_iter().flatten() {
            while let Some(packet) = self.sessions.dequeue_rx_packet(session_id)? {
                match packet {
                    Packet::Connect(connect) => {
                        self.handle_connect(session_id, &connect)?;
                    }
                    Packet::ConnAck(_) => {}
                    Packet::Publish(publish) => {
                        self.handle_publish(session_id, &publish)?;
                    }
                    Packet::PubAck(_) => {}
                    Packet::PubRec(_) => {}
                    Packet::PubRel(_) => {}
                    Packet::PubComp(_) => {}
                    Packet::Subscribe(subscribe) => {
                        self.handle_subscribe(session_id, &subscribe)?;
                    }
                    Packet::SubAck(_) => {}
                    Packet::Unsubscribe(_) => {}
                    Packet::UnsubAck(_) => {}
                    Packet::PingReq(_) => {
                        self.handle_pingreq(session_id)?;
                    }
                    Packet::PingResp(_) => {}
                    Packet::Disconnect(_) => {
                        self.handle_disconnect(session_id)?;
                    }
                }
            }
        }

        Ok(())
    }

    fn handle_connect(
        &mut self,
        session_id: u128,
        connect: &ConnectPacket<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>,
    ) -> Result<(), BrokerError> {
        if !connect.client_id.is_empty() {
            self.sessions
                .update_client_id(session_id, connect.client_id.clone())?;
        }

        self.sessions
            .update_keep_alive(session_id, connect.keep_alive)?;

        self.sessions.mark_connected(session_id)?;

        let connack = Packet::ConnAck(ConnAckPacket::default());
        self.sessions.queue_tx_packet(session_id, connack)?;

        Ok(())
    }

    fn handle_publish(
        &mut self,
        session_id: u128,
        publish: &PublishPacket<MAX_TOPIC_NAME_LENGTH, MAX_PAYLOAD_SIZE>,
    ) -> Result<(), BrokerError> {
        let subscribers = self.topics.get_subscribers(&publish.topic_name);

        for subscriber_id in subscribers {
            let packet = Packet::Publish(publish.clone());
            self.sessions.queue_tx_packet(subscriber_id, packet)?;
        }

        if publish.qos > QoS::AtMostOnce && publish.packet_id.is_some() {
            let puback = PubAckPacket {
                packet_id: publish.packet_id.unwrap(),
            };

            let puback_packet = Packet::PubAck(puback);
            self.sessions.queue_tx_packet(session_id, puback_packet)?;
        }

        Ok(())
    }

    fn handle_subscribe(
        &mut self,
        session_id: u128,
        subscribe: &SubscribePacket<MAX_TOPIC_NAME_LENGTH>,
    ) -> Result<(), BrokerError> {
        self.topics
            .subscribe(session_id, subscribe.topic_filter.clone())?;

        let granted_qos = subscribe.requested_qos;

        let suback = SubAckPacket {
            packet_id: subscribe.packet_id,
            granted_qos,
        };

        self.sessions
            .queue_tx_packet(session_id, Packet::SubAck(suback))
    }

    fn handle_pingreq(&mut self, session_id: u128) -> Result<(), BrokerError> {
        self.sessions
            .queue_tx_packet(session_id, Packet::PingResp(PingRespPacket))
    }

    fn handle_disconnect(&mut self, session_id: u128) -> Result<(), BrokerError> {
        self.mark_session_disconnected(session_id)
    }

    pub fn get_sessions_with_pending_packets(&self) -> HeaplessVec<u128, MAX_SESSIONS> {
        let mut pending = HeaplessVec::new();
        let session_ids = self.get_all_sessions();

        for session_id in session_ids.into_iter().flatten() {
            // Check if session has pending outbound packets
            if self.sessions.has_pending_tx_packets(session_id) {
                let _ = pending.push(session_id);
            }
        }

        pending
    }
}
