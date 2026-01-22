//! Embassy async broker extensions

use picobroker_core::{TcpStream, ClientId, Packet, PicoBroker, QoS, Result, TopicName, TopicSubscription};
use picobroker_core::{Publish, Subscribe, SubAck, PingResp};
use crate::time::EmbassyTimeSource;

/// Embassy broker with async methods
///
/// Type alias for PicoBroker using EmbassyTimeSource
pub type EmbassyPicoBroker<
    const MAX_CLIENT_NAME_LENGTH: usize,
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_CLIENTS: usize,
    const MAX_TOPICS: usize,
    const MAX_SUBSCRIBERS_PER_TOPIC: usize,
> = PicoBroker<
    EmbassyTimeSource,
    MAX_CLIENT_NAME_LENGTH,
    MAX_TOPIC_NAME_LENGTH,
    MAX_CLIENTS,
    MAX_TOPICS,
    MAX_SUBSCRIBERS_PER_TOPIC,
>;

