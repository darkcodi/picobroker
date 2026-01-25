//! Simple std tokio MQTT broker server example
//!
//! This example demonstrates how to run a basic MQTT broker using tokio.
//! You can test it with any MQTT client (e.g., mosquitto_pub/sub).

use picobroker_tokio::{DefaultTokioPicoBrokerServer, StdLogger, StdTimeSource, TokioDelay, TokioTcpListener};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("debug")).init();

    let time_source = StdTimeSource;
    let listener = TokioTcpListener::bind("0.0.0.0:1883").await?;
    let logger = StdLogger;
    let delay = TokioDelay;

    let mut server = DefaultTokioPicoBrokerServer::new(time_source, listener, logger, delay);
    server.run().await?;
    Ok(())
}
