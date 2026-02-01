//! # picobroker-embassy
//!
//! An Embassy-based MQTT broker server library for embedded systems.
//!
//! This library provides building blocks for an MQTT 3.1.1 broker that runs on Embassy.
//! It is built on top of `picobroker-core` for protocol handling and broker logic.
//!
//! ## Example
//!
//! ```no_run
//! use picobroker_embassy::{MqttServer, handle_connection, HandlerConfig};
//! use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
//! use static_cell::StaticCell;
//! use embassy_sync::mutex::Mutex;
//!
//! #[embassy_executor::main]
//! async fn main(spawner: embassy_executor::Spawner) {
//!     // ... (WiFi and network stack setup) ...
//!
//!     // Static storage (required by Embassy)
//!     static BROKER_CELL: StaticCell<Mutex<CriticalSectionRawMutex, ...>> = StaticCell::new();
//!     static NOTIF_CELL: StaticCell<NotificationRegistry<4, CriticalSectionRawMutex>> = StaticCell::new();
//!     static SESSION_ID_GEN_CELL: StaticCell<Mutex<CriticalSectionRawMutex, ...>> = StaticCell::new();
//!
//!     let server = picobroker_embassy::MqttServer::<CriticalSectionRawMutex, 64, 256, 8, 4, 32, 8>::new(
//!         &BROKER_CELL, &NOTIF_CELL, &SESSION_ID_GEN_CELL
//!     );
//!
//!     // Spawn tasks in user code (Embassy requires task functions to be non-generic)
//!     for idx in 0..4 {
//!         spawner.spawn(accept_task(...)).ok();
//!     }
//! }
//! ```

#![no_std]

// Re-export core types for convenience
pub use picobroker_core::{
    broker::PicoBroker,
    protocol::packets::Packet,
    protocol::ProtocolError,
};

// Public API
pub mod server;
pub use server::{MqttServer, MqttServerConfig};

// Modules (public - users need these to build their tasks)
pub mod handler;
pub use handler::{handle_connection, HandlerConfig};

// Modules (private)
mod io;
mod state;

// Re-export state types for users who need to declare StaticCells
pub use state::{BrokerMutex, NotificationRegistry, SessionIdGen, current_time_nanos};

// =============================================================================
// Convenience Type Aliases
// =============================================================================
//
// These aliases take a mutex type parameter (must implement `RawMutex`).
//
// Example:
// ```
// use embassy_sync::blocking_mutex::raw::CriticalSectionRawMutex;
// type MyMqttServer = DefaultMqttServer<CriticalSectionRawMutex>;
// ```

/// Default MQTT server type with specified mutex.
/// Usage: `DefaultMqttServer<CriticalSectionRawMutex>`
pub type DefaultMqttServer<M> = MqttServer<M, 64, 256, 8, 4, 32, 8>;

/// Small MQTT server for embedded/constrained environments with specified mutex.
/// Usage: `SmallMqttServer<CriticalSectionRawMutex>`
pub type SmallMqttServer<M> = MqttServer<M, 32, 128, 4, 2, 16, 4>;

/// Large MQTT server for higher capacity with specified mutex.
/// Usage: `LargeMqttServer<CriticalSectionRawMutex>`
pub type LargeMqttServer<M> = MqttServer<M, 128, 1024, 16, 16, 128, 32>;
