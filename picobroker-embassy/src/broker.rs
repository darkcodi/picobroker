//! Embassy async broker extensions

use crate::time::EmbassyTimeSource;
use picobroker_core::PicoBroker;

/// Embassy broker with async methods
///
/// Type alias for PicoBroker using EmbassyTimeSource
pub type EmbassyPicoBroker<
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_CLIENTS: usize,
    const MAX_TOPICS: usize,
    const MAX_SUBSCRIBERS_PER_TOPIC: usize,
> = PicoBroker<
    EmbassyTimeSource,
    MAX_TOPIC_NAME_LENGTH,
    MAX_CLIENTS,
    MAX_TOPICS,
    MAX_SUBSCRIBERS_PER_TOPIC,
>;
