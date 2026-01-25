//! Simple std tokio MQTT broker server example
//!
//! This example demonstrates how to run a basic MQTT broker using tokio.
//! You can test it with any MQTT client (e.g., mosquitto_pub/sub).

use picobroker_tokio::TokioPicoBrokerServer;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Initialize logger
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info")).init();

    // Configure the broker with reasonable limits
    // These parameters control memory usage and maximum sizes
    // Note: Smaller values are used to avoid stack overflow
    const MAX_TOPIC_NAME_LENGTH: usize = 64;
    const MAX_PAYLOAD_SIZE: usize = 128;
    const CLIENT_TO_BROKER_QUEUE_SIZE: usize = 8;
    const BROKER_TO_CLIENT_QUEUE_SIZE: usize = 8;
    const MAX_CLIENTS: usize = 4;
    const MAX_TOPICS: usize = 16;
    const MAX_SUBSCRIBERS_PER_TOPIC: usize = 4;
    const MAX_PACKET_SIZE: usize = 256;

    println!("Starting PicoBroker MQTT server on 0.0.0.0:1883...");
    println!("Limits:");
    println!("  Max clients: {}", MAX_CLIENTS);
    println!("  Max topics: {}", MAX_TOPICS);
    println!("  Max payload: {} bytes", MAX_PAYLOAD_SIZE);
    println!();
    println!("You can test with:");
    println!("  mosquitto_sub -h localhost -t test/topic");
    println!("  mosquitto_pub -h localhost -t test/topic -m \"Hello, MQTT!\"");
    println!();

    // Create and run the server
    let mut server = TokioPicoBrokerServer::<
        MAX_TOPIC_NAME_LENGTH,
        MAX_PAYLOAD_SIZE,
        CLIENT_TO_BROKER_QUEUE_SIZE,
        BROKER_TO_CLIENT_QUEUE_SIZE,
        MAX_CLIENTS,
        MAX_TOPICS,
        MAX_SUBSCRIBERS_PER_TOPIC,
        MAX_PACKET_SIZE,
    >::new("0.0.0.0:1883").await?;

    server.run().await?;

    Ok(())
}
