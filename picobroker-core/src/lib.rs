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

mod broker;
mod broker_error;
mod client;
mod protocol;
mod topics;
mod traits;
mod server;

pub use broker::PicoBroker;
pub use server::PicoBrokerServer;
pub use broker_error::BrokerError;
pub use client::ClientId;
pub use traits::{TcpListener, TcpStream, SocketAddr, NetworkError};
pub use protocol::ConnAckPacket;
pub use protocol::ConnectPacket;
pub use protocol::ConnectReturnCode;
pub use protocol::DisconnectPacket;
pub use protocol::Packet;
pub use protocol::PacketEncoder;
pub use protocol::PacketEncodingError;
pub use protocol::PacketType;
pub use protocol::PingReqPacket;
pub use protocol::PingRespPacket;
pub use protocol::PubAckPacket;
pub use protocol::PubCompPacket;
pub use protocol::PubRecPacket;
pub use protocol::PubRelPacket;
pub use protocol::PublishPacket;
pub use protocol::QoS;
pub use protocol::SubAckPacket;
pub use protocol::SubscribePacket;
pub use protocol::UnsubAckPacket;
pub use protocol::UnsubscribePacket;
pub use protocol::{
    read_string, read_variable_length, variable_length_length, write_string, write_variable_length,
};
pub use traits::{TimeSource, Delay};
pub use topics::{TopicEntry, TopicName, TopicRegistry, TopicSubscription};
pub use protocol::HeaplessString;
