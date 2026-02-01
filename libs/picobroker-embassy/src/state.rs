//! Server state management for Embassy-based MQTT broker.

use embassy_sync::channel::Channel;
use embassy_sync::mutex::Mutex;
use embassy_sync::blocking_mutex::raw::RawMutex;
use picobroker_core::broker::PicoBroker;

// =============================================================================
// Type Aliases
// =============================================================================

/// Broker mutex type for Embassy (no-std, static allocation)
/// The raw mutex type is generic for portability across platforms.
pub type BrokerMutex<
    'mtx,
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const QUEUE_SIZE: usize,
    const MAX_SESSIONS: usize,
    const MAX_TOPICS: usize,
    const MAX_SUBSCRIBERS_PER_TOPIC: usize,
    M,
> = Mutex<
    M,
    PicoBroker<
        MAX_TOPIC_NAME_LENGTH,
        MAX_PAYLOAD_SIZE,
        QUEUE_SIZE,
        MAX_SESSIONS,
        MAX_TOPICS,
        MAX_SUBSCRIBERS_PER_TOPIC,
    >,
>;

// =============================================================================
// Notification System for Waking Idle Connections
// =============================================================================
//
// CRITICAL: This notification system is essential for proper message routing.
// Without it, idle clients (those not sending data) would never receive messages
// published by other clients because socket.read() blocks indefinitely.
//
// Each connection handler has a notification channel. When the broker queues
// packets for a session, we signal that session's channel to wake it up so it
// can flush the pending packets.

/// Static array of notification channels (one per potential connection)
pub struct NotificationRegistry<const MAX_SESSIONS: usize, M: RawMutex> {
    channels: [Channel<M, (), 1>; MAX_SESSIONS],
    /// Map from channel index to session_id (so we know which session to notify)
    /// Using simple array instead of HeaplessVec for const-initializability
    session_ids: Mutex<M, [u128; MAX_SESSIONS]>,
}

impl<const MAX_SESSIONS: usize, M: RawMutex> NotificationRegistry<MAX_SESSIONS, M>
where
    [Channel<M, (), 1>; MAX_SESSIONS]: Sized,
{
    pub const fn new(channels: [Channel<M, (), 1>; MAX_SESSIONS]) -> Self {
        Self {
            channels,
            session_ids: Mutex::new([0u128; MAX_SESSIONS]),
        }
    }

    /// Register a session's notification channel at the given index
    pub async fn register_session(&self, channel_idx: usize, session_id: u128) {
        let mut ids = self.session_ids.lock().await;
        if channel_idx < MAX_SESSIONS {
            ids[channel_idx] = session_id;
        }
    }

    /// Unregister a session
    pub async fn unregister_session(&self, channel_idx: usize) {
        let mut ids = self.session_ids.lock().await;
        if channel_idx < MAX_SESSIONS {
            ids[channel_idx] = 0;
        }
    }

    /// Notify a specific session by session_id
    pub async fn notify_session(&self, session_id: u128) {
        let ids = self.session_ids.lock().await;
        // Find which channel index has this session_id
        for (idx, &id) in ids.iter().enumerate() {
            if id == session_id {
                // Use try_send - if channel is full, we don't need to send another signal
                // (the connection will wake up and check anyway)
                let _ = self.channels[idx].sender().try_send(());
                break;
            }
        }
    }

    /// Get the receiver for a specific channel index
    pub fn receiver(&self, channel_idx: usize) -> &Channel<M, (), 1> {
        &self.channels[channel_idx]
    }
}

// =============================================================================
// Session ID Generator
// =============================================================================

/// Session ID generator using timestamp and counter
pub struct SessionIdGen(u64);

impl Default for SessionIdGen {
    fn default() -> Self {
        Self::new()
    }
}

impl SessionIdGen {
    pub const fn new() -> Self {
        Self(0)
    }

    pub fn generate(&mut self) -> u128 {
        let timestamp = embassy_time::Instant::now().as_ticks() as u128;
        let counter = self.0;
        self.0 = self.0.wrapping_add(1);
        (timestamp << 32) | (counter as u128)
    }
}

// =============================================================================
// Time Helper
// =============================================================================

/// Get current time in nanoseconds
pub fn current_time_nanos() -> u128 {
    embassy_time::Instant::now().as_ticks() as u128
}
