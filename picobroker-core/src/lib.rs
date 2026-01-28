//! # PicoBroker Core
//!
//! Pure `no_std` MQTT 3.1.1 broker core library.
//!
//! This library contains the core broker logic, protocol implementation,
//! and data structures. It is platform-agnostic and has no async runtime
//! dependencies.
//!
//! ## Features
//!
//! - **no_std** compatible - Fully embedded, no standard library
//! - **MQTT 3.1.1** compliant - Protocol compliant with QoS 0
//! - **Heapless** - All stack/static allocation, no heap usage
//! - **Generic networking** - Works with any TCP implementation
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
pub mod broker_error;
pub mod client;
pub mod protocol;
pub mod server;
pub mod topics;
pub mod traits;
pub mod session;
