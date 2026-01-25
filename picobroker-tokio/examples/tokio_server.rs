//! Simple std tokio MQTT broker server example
//!
//! This example demonstrates how to run a basic MQTT broker using tokio.
//! You can test it with any MQTT client (e.g., mosquitto_pub/sub).

use picobroker_tokio::{DefaultTokioPicoBrokerServer, StdLogger, StdTimeSource, TokioDelay, TokioTaskSpawner, TokioTcpListener};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logger
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    let time_source = StdTimeSource;
    let listener = TokioTcpListener::bind("0.0.0.0:1883").await?;
    let spawner = TokioTaskSpawner::default();
    let logger = StdLogger;
    let delay = TokioDelay;

    let mut server = DefaultTokioPicoBrokerServer::new(time_source, listener, spawner, logger, delay);
    server.run().await?;
    Ok(())
}
