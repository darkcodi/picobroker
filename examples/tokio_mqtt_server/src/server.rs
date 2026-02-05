use crate::handler::{handle_connection, HandlerConfig};
use crate::state::ServerState;
use log::{debug, error, info};
use picobroker::broker::PicoBroker;
use std::sync::Arc;
use tokio::net::TcpListener;

/// Shared broker type alias
pub type SharedBroker<
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const QUEUE_SIZE: usize,
    const MAX_SESSIONS: usize,
    const MAX_TOPICS: usize,
    const MAX_SUBSCRIBERS_PER_TOPIC: usize,
> = Arc<
    tokio::sync::Mutex<
        PicoBroker<
            MAX_TOPIC_NAME_LENGTH,
            MAX_PAYLOAD_SIZE,
            QUEUE_SIZE,
            MAX_SESSIONS,
            MAX_TOPICS,
            MAX_SUBSCRIBERS_PER_TOPIC,
        >,
    >,
>;

/// Runtime configuration for the MQTT server
#[derive(Debug, Clone)]
pub struct MqttServerConfig {
    /// Default keep-alive in seconds (default: 60)
    pub default_keep_alive_secs: u16,
    /// Channel capacity for frame transmission (default: 32)
    pub frame_channel_capacity: usize,
    /// Cleanup interval in seconds (default: 5)
    pub cleanup_interval_secs: u64,
}

impl Default for MqttServerConfig {
    fn default() -> Self {
        Self {
            default_keep_alive_secs: 60,
            frame_channel_capacity: 32,
            cleanup_interval_secs: 5,
        }
    }
}

/// A Tokio-based MQTT broker server
///
/// # Type Parameters
/// * `MAX_TOPIC_NAME_LENGTH` - Maximum topic name length in bytes
/// * `MAX_PAYLOAD_SIZE` - Maximum publish payload size in bytes
/// * `QUEUE_SIZE` - Per-session packet queue size
/// * `MAX_SESSIONS` - Maximum concurrent sessions
/// * `MAX_TOPICS` - Maximum unique topics
/// * `MAX_SUBSCRIBERS_PER_TOPIC` - Maximum subscribers per topic
pub struct MqttServer<
    const MAX_TOPIC_NAME_LENGTH: usize = 64,
    const MAX_PAYLOAD_SIZE: usize = 256,
    const QUEUE_SIZE: usize = 8,
    const MAX_SESSIONS: usize = 4,
    const MAX_TOPICS: usize = 32,
    const MAX_SUBSCRIBERS_PER_TOPIC: usize = 8,
> {
    state: Arc<ServerState>,
    broker: SharedBroker<
        MAX_TOPIC_NAME_LENGTH,
        MAX_PAYLOAD_SIZE,
        QUEUE_SIZE,
        MAX_SESSIONS,
        MAX_TOPICS,
        MAX_SUBSCRIBERS_PER_TOPIC,
    >,
    config: MqttServerConfig,
}

impl<
        const MAX_TOPIC_NAME_LENGTH: usize,
        const MAX_PAYLOAD_SIZE: usize,
        const QUEUE_SIZE: usize,
        const MAX_SESSIONS: usize,
        const MAX_TOPICS: usize,
        const MAX_SUBSCRIBERS_PER_TOPIC: usize,
    >
    MqttServer<
        MAX_TOPIC_NAME_LENGTH,
        MAX_PAYLOAD_SIZE,
        QUEUE_SIZE,
        MAX_SESSIONS,
        MAX_TOPICS,
        MAX_SUBSCRIBERS_PER_TOPIC,
    >
{
    /// Create a new server with default configuration
    pub fn new() -> Self {
        Self::with_config(MqttServerConfig::default())
    }

    /// Create a new server with custom configuration
    pub fn with_config(config: MqttServerConfig) -> Self {
        Self {
            state: Arc::new(ServerState::new()),
            broker: Arc::new(tokio::sync::Mutex::new(PicoBroker::new())),
            config,
        }
    }

    /// Get a handle to the shared broker for external interaction
    pub fn broker(
        &self,
    ) -> &SharedBroker<
        MAX_TOPIC_NAME_LENGTH,
        MAX_PAYLOAD_SIZE,
        QUEUE_SIZE,
        MAX_SESSIONS,
        MAX_TOPICS,
        MAX_SUBSCRIBERS_PER_TOPIC,
    > {
        &self.broker
    }

    /// Get the number of active connections
    pub fn connection_count(&self) -> usize {
        self.state.connection_count()
    }

    /// Run the server, binding to the specified address
    pub async fn run(&self, bind_addr: &str) -> Result<(), Box<dyn std::error::Error>> {
        let listener = TcpListener::bind(bind_addr).await?;
        info!("Server listening on {}", bind_addr);
        debug!("Server configuration: keep_alive={}s, cleanup_interval={}s",
               self.config.default_keep_alive_secs, self.config.cleanup_interval_secs);

        // Spawn cleanup task
        let cleanup_state = self.state.clone();
        let cleanup_broker = self.broker.clone();
        let cleanup_interval_secs = self.config.cleanup_interval_secs;
        tokio::spawn(async move {
            cleanup_task(cleanup_state, cleanup_broker, cleanup_interval_secs).await;
        });
        debug!("Cleanup task spawned (interval: {}s)", cleanup_interval_secs);

        // Accept connections loop
        info!("Accepting incoming connections on {}...", bind_addr);
        loop {
            match listener.accept().await {
                Ok((socket, addr)) => {
                    let peer_addr = addr.to_string();
                    info!("New connection from {}", peer_addr);

                    let handler_state = self.state.clone();
                    let handler_broker = self.broker.clone();
                    let handler_config = HandlerConfig {
                        default_keep_alive_secs: self.config.default_keep_alive_secs,
                        frame_channel_capacity: self.config.frame_channel_capacity,
                    };

                    tokio::spawn(async move {
                        if let Err(e) = handle_connection(
                            socket,
                            peer_addr,
                            handler_state,
                            handler_broker,
                            &handler_config,
                        )
                        .await
                        {
                            error!("Client handler error: {}", e);
                        }
                    });
                }
                Err(e) => {
                    error!("Error accepting connection: {}", e);
                }
            }
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
    > Default
    for MqttServer<
        MAX_TOPIC_NAME_LENGTH,
        MAX_PAYLOAD_SIZE,
        QUEUE_SIZE,
        MAX_SESSIONS,
        MAX_TOPICS,
        MAX_SUBSCRIBERS_PER_TOPIC,
    >
{
    fn default() -> Self {
        Self::new()
    }
}

/// Background cleanup task for expired sessions
async fn cleanup_task<
    const MAX_TOPIC_NAME_LENGTH: usize,
    const MAX_PAYLOAD_SIZE: usize,
    const QUEUE_SIZE: usize,
    const MAX_SESSIONS: usize,
    const MAX_TOPICS: usize,
    const MAX_SUBSCRIBERS_PER_TOPIC: usize,
>(
    state: Arc<ServerState>,
    broker: SharedBroker<
        MAX_TOPIC_NAME_LENGTH,
        MAX_PAYLOAD_SIZE,
        QUEUE_SIZE,
        MAX_SESSIONS,
        MAX_TOPICS,
        MAX_SUBSCRIBERS_PER_TOPIC,
    >,
    interval_secs: u64,
) {
    use crate::state::current_time_nanos;
    use log::trace;
    use tokio::time::{sleep, Duration};

    info!("Cleanup task started (interval: {}s)", interval_secs);
    let mut interval = sleep(Duration::from_secs(interval_secs));
    loop {
        interval.await;
        interval = sleep(Duration::from_secs(interval_secs));

        let current_time = current_time_nanos();
        let active_connections = state.connection_count();
        trace!(
            "Cleanup scan running at time {}, active connections: {}",
            current_time,
            active_connections
        );

        let mut broker_guard = broker.lock().await;

        // Check expired sessions
        let expired: Vec<u128> = broker_guard
            .get_expired_sessions(current_time)
            .into_iter()
            .flatten()
            .map(|info| info.session_id)
            .collect();
        let expired_count = expired.len();
        if !expired.is_empty() {
            info!("Found {} expired session(s)", expired_count);
            for session_id in &expired {
                info!("Cleaning up expired session {}", session_id);
                state.connections.remove(session_id);
                broker_guard.remove_session(*session_id);
            }
        }

        // Check disconnected sessions
        let disconnected: Vec<u128> = broker_guard
            .get_disconnected_sessions()
            .into_iter()
            .flatten()
            .collect();
        let disconnected_count = disconnected.len();
        if !disconnected.is_empty() {
            info!("Found {} disconnected session(s)", disconnected_count);
            for session_id in &disconnected {
                info!("Cleaning up disconnected session {}", session_id);
                state.connections.remove(session_id);
                broker_guard.remove_session(*session_id);
            }
        }

        if expired.is_empty() && disconnected.is_empty() {
            trace!("Cleanup scan complete: no sessions to remove");
        } else {
            info!(
                "Cleanup scan complete: removed {} expired + {} disconnected sessions, remaining connections: {}",
                expired_count,
                disconnected_count,
                state.connection_count()
            );
        }
    }
}
