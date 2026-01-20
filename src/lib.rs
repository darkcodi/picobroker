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

#![no_std]

pub mod broker;
pub mod client;
pub mod error;
pub mod protocol;
pub mod topics;

pub use error::{Error, Result};
pub use topics::{TopicRegistry, TopicSubscription};
