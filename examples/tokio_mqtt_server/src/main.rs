// Modules (private)
mod server;
mod handler;
mod io;
mod state;

// Public exports
pub use server::{MqttServer, MqttServerConfig, SharedBroker};

// Convenience type aliases
/// Default MQTT server configuration (64, 256, 8, 4, 32, 8)
pub type DefaultMqttServer = MqttServer<64, 256, 8, 4, 32, 8>;

/// Small MQTT server for embedded/constrained environments (32, 128, 4, 2, 16, 4)
pub type SmallMqttServer = MqttServer<32, 128, 4, 2, 16, 4>;

/// Large MQTT server for higher capacity (128, 1024, 16, 16, 128, 32)
pub type LargeMqttServer = MqttServer<128, 1024, 16, 16, 128, 32>;


#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .init();

    const MAX_SESSIONS: usize = 4;
    const MAX_TOPICS: usize = 32;

    type MyMqttServer = MqttServer<64, 256, 8, MAX_SESSIONS, MAX_TOPICS, 8>;
    let server = MyMqttServer::new();

    log::info!("Starting picobroker tokio server on 0.0.0.0:1883");
    log::info!("Configuration: {} max sessions, {} max topics", MAX_SESSIONS, MAX_TOPICS);

    server.run("0.0.0.0:1883").await?;

    Ok(())
}
