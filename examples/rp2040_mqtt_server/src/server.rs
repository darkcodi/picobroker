//! MQTT server implementation for Embassy runtime.
//!
//! Due to Embassy's task system limitations, this library provides the building blocks
//! for an MQTT server, but task spawning must be done in user code.
//! See the example for how to properly spawn tasks.

use crate::state::{NotificationRegistry, SessionIdGen};
use embassy_sync::blocking_mutex::raw::RawMutex;
use embassy_sync::mutex::Mutex;
use picobroker::broker::PicoBroker;

// =============================================================================
// Configuration
// =============================================================================

/// Runtime configuration for the MQTT server
#[derive(Debug, Clone)]
pub struct MqttServerConfig {
    /// Default keep-alive in seconds (default: 60)
    pub default_keep_alive_secs: u16,
    /// Cleanup interval in seconds (default: 5)
    pub cleanup_interval_secs: u64,
}

impl Default for MqttServerConfig {
    fn default() -> Self {
        Self {
            default_keep_alive_secs: 60,
            cleanup_interval_secs: 5,
        }
    }
}

// =============================================================================
// MqttServer
// =============================================================================

/// An Embassy-based MQTT broker server
///
/// This struct holds the references to the broker state but does not spawn tasks.
/// Task spawning must be done in user code due to Embassy's limitations.
///
/// # Type Parameters
/// * `M` - The raw mutex type (must implement `RawMutex + 'static`, e.g., `ThreadModeRawMutex`)
/// * `MAX_TOPIC_NAME_LENGTH` - Maximum topic name length in bytes
/// * `MAX_PAYLOAD_SIZE` - Maximum publish payload size in bytes
/// * `QUEUE_SIZE` - Per-session packet queue size
/// * `MAX_SESSIONS` - Maximum concurrent sessions
/// * `MAX_TOPICS` - Maximum unique topics
/// * `MAX_SUBSCRIBERS_PER_TOPIC` - Maximum subscribers per topic
pub struct MqttServer<
    M: RawMutex + 'static,
    const MAX_TOPIC_NAME_LENGTH: usize = 64,
    const MAX_PAYLOAD_SIZE: usize = 256,
    const QUEUE_SIZE: usize = 8,
    const MAX_SESSIONS: usize = 4,
    const MAX_TOPICS: usize = 32,
    const MAX_SUBSCRIBERS_PER_TOPIC: usize = 8,
> {
    pub broker: &'static Mutex<
        M,
        PicoBroker<
            MAX_TOPIC_NAME_LENGTH,
            MAX_PAYLOAD_SIZE,
            QUEUE_SIZE,
            MAX_SESSIONS,
            MAX_TOPICS,
            MAX_SUBSCRIBERS_PER_TOPIC,
        >,
    >,
    pub notification_registry: &'static NotificationRegistry<MAX_SESSIONS, M>,
    pub session_id_gen: &'static Mutex<M, SessionIdGen>,
    pub config: MqttServerConfig,
}

impl<
        M: RawMutex + 'static,
        const MAX_TOPIC_NAME_LENGTH: usize,
        const MAX_PAYLOAD_SIZE: usize,
        const QUEUE_SIZE: usize,
        const MAX_SESSIONS: usize,
        const MAX_TOPICS: usize,
        const MAX_SUBSCRIBERS_PER_TOPIC: usize,
    >
    MqttServer<
        M,
        MAX_TOPIC_NAME_LENGTH,
        MAX_PAYLOAD_SIZE,
        QUEUE_SIZE,
        MAX_SESSIONS,
        MAX_TOPICS,
        MAX_SUBSCRIBERS_PER_TOPIC,
    >
{
    /// Create a new server with the given components
    pub fn new(
        broker: &'static Mutex<
            M,
            PicoBroker<
                MAX_TOPIC_NAME_LENGTH,
                MAX_PAYLOAD_SIZE,
                QUEUE_SIZE,
                MAX_SESSIONS,
                MAX_TOPICS,
                MAX_SUBSCRIBERS_PER_TOPIC,
            >,
        >,
        notification_registry: &'static NotificationRegistry<MAX_SESSIONS, M>,
        session_id_gen: &'static Mutex<M, SessionIdGen>,
    ) -> Self {
        Self {
            broker,
            notification_registry,
            session_id_gen,
            config: MqttServerConfig::default(),
        }
    }

    /// Create with custom configuration
    pub fn with_config(
        broker: &'static Mutex<
            M,
            PicoBroker<
                MAX_TOPIC_NAME_LENGTH,
                MAX_PAYLOAD_SIZE,
                QUEUE_SIZE,
                MAX_SESSIONS,
                MAX_TOPICS,
                MAX_SUBSCRIBERS_PER_TOPIC,
            >,
        >,
        notification_registry: &'static NotificationRegistry<MAX_SESSIONS, M>,
        session_id_gen: &'static Mutex<M, SessionIdGen>,
        config: MqttServerConfig,
    ) -> Self {
        Self {
            broker,
            notification_registry,
            session_id_gen,
            config,
        }
    }
}
