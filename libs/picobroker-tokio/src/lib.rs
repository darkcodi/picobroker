//! # picobroker-tokio
//!
//! A Tokio-based MQTT broker server library.
//!
//! This library provides a ready-to-use MQTT 3.1.1 broker that runs on Tokio.
//! It is built on top of `picobroker-core` for protocol handling and broker logic.
//!
//! ## Example
//!
//! ```no_run
//! use picobroker_tokio::MqttServer;
//!
//! #[tokio::main]
//! async fn main() -> Result<(), Box<dyn std::error::Error>> {
//!     let server = MqttServer::new();
//!     server.run("0.0.0.0:1883").await?;
//!     Ok(())
//! }
//! ```

// Re-export core types for convenience
pub use picobroker_core::{
    broker::PicoBroker,
    protocol::packets::Packet,
    protocol::ProtocolError,
};

// Public API
pub mod server;
pub use server::{MqttServer, MqttServerConfig, SharedBroker};

// Modules (private)
mod handler;
mod io;
mod state;

// Convenience type aliases
/// Default MQTT server configuration (64, 256, 8, 4, 32, 8)
pub type DefaultMqttServer = MqttServer<64, 256, 8, 4, 32, 8>;

/// Small MQTT server for embedded/constrained environments (32, 128, 4, 2, 16, 4)
pub type SmallMqttServer = MqttServer<32, 128, 4, 2, 16, 4>;

/// Large MQTT server for higher capacity (128, 1024, 16, 16, 128, 32)
pub type LargeMqttServer = MqttServer<128, 1024, 16, 16, 128, 32>;
