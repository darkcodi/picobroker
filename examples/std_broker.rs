//! Minimal MQTT Broker Example
//!
//! This example demonstrates the basic structure of the picobroker MQTT broker.
//! Note: This is a minimal example showing the broker API.
//! Full server implementation with connection handling will be added in the future.

use picobroker::broker::{DefaultPicoBroker, PicoBroker};
use picobroker::client::ClientName;
use picobroker::network::std::StdTcpListener;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    println!("PicoBroker - Minimal MQTT 3.1.1 Broker");
    println!("======================================\n");

    // Create a new broker with default limits
    let mut broker = DefaultPicoBroker::new();
    println!("Broker created successfully!");

    // Example: Register a client (normally done during CONNECT handling)
    let client_name = ClientName::try_from("test_client")?;
    broker.register_client(client_name.clone(), 60)?; // 60 second keep-alive
    println!("Registered client: {}", client_name);

    // Check if client is connected
    if broker.is_client_connected(&client_name) {
        println!("Client '{}' is connected", client_name);
    }

    // Process expired clients (keep-alive monitoring)
    broker.process_expired_clients();

    println!();
    println!("Broker is ready for connection handling implementation.");
    println!();
    println!("TODO: Implement full connection accept loop with:");
    println!("  - TCP listener bind and accept");
    println!("  - CONNECT packet parsing");
    println!("  - Client handler spawning");
    println!("  - Message routing between clients");

    Ok(())
}
