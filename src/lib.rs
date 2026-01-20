//! # PicoBroker - Minimal MQTT 3.1.1 Broker for Embedded Systems
//!
//! An extremely minimal MQTT 3.1.1 broker designed for embedded no_std environments
//! using the Embassy async framework.
//!
//! ## Features
//!
//! - **no_std** compatible - Fully embedded, no standard library
//! - **MQTT 3.1.1** compliant - Protocol compliant with QoS 0
//! - **Heapless** - All stack/static allocation, no heap usage
//! - **Generic networking** - Works with any Embassy network stack
//! - **Configurable** - Compile-time configuration via const generics
//! - **Tiny footprint** - ~8-12 KB RAM, ~30-40 KB flash for 4 clients
//!
//! ## Limitations
//!
//! - QoS 0 only (fire and forget)
//! - No topic wildcards (+, #)
//! - No retained messages
//! - No authentication/authorization
//! - No TLS
//!
//! ## Example
//!
//! ```rust,ignore
//! use picobroker::Broker;
//! use embassy_net::tcp::TcpSocket;
//! use embassy_time::Duration;
//!
//! #[embassy_executor::main]
//! async fn main(spawner: Spawner) {
//!     // Create broker with max 4 clients
//!     let mut broker: Broker<4> = Broker::new();
//!
//!     // Main broker loop
//!     loop {
//!         // Accept connections, poll clients, etc.
//!         broker.poll_client(0, &mut read_buf).await;
//!
//!         // Check timeouts periodically
//!         broker.check_timeouts().await;
//!
//!         // Small yield
//!         Timer::after(Duration::from_millis(10)).await;
//!     }
//! }
//! ```
//!
//! ## Configuration
//!
//! The broker is configured via const generics:
//!
//! - `MAX_CLIENTS`: Maximum concurrent clients (e.g., 4)
//!
//! ## Memory Footprint
//!
//! For `MAX_CLIENTS = 4`:
//! - **RAM**: ~8-12 KB (mostly socket buffers managed by network stack)
//! - **Flash**: ~30-40 KB (code + static data)
//! - **Heap**: 0 bytes (fully heapless)

#![no_std]

pub mod broker;
pub mod client;
pub mod error;
pub mod packet;
pub mod topics;

// Re-export commonly used types
pub use broker::Broker;
pub use client::{Action, Client, ClientState};
pub use error::{Error, Result};
pub use packet::{ConnectPacket, Packet, PublishPacket, SubscribePacket};
pub use topics::{TopicFilter, TopicRegistry};

/// Common broker configurations
pub mod prelude {
    use super::Broker;

    /// Small configuration: 2 clients
    pub type SmallBroker<'a> = Broker<'a, 2>;

    /// Medium configuration: 4 clients
    pub type MediumBroker<'a> = Broker<'a, 4>;

    /// Large configuration: 8 clients
    pub type LargeBroker<'a> = Broker<'a, 8>;
}
