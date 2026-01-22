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
mod client;
mod error;
mod network;
mod protocol;
mod queue;
mod server;
mod task;
mod time;
mod topics;

pub use broker::PicoBroker;
pub use client::{Client, ClientId, ClientName, ClientRegistry};
pub use error::{Error, Result};
pub use queue::{MessageQueue, MessageReceiver, MessageSender, QueuedMessage};
pub use server::{ClientHandler, MqttServer, QueuedPublish, SenderRegistry};
pub use task::{SpawnError, TaskSpawner};
pub use network::{SocketAddr, TcpListener, TcpStream};
pub use protocol::ConnAck;
pub use protocol::Connect;
pub use protocol::Disconnect;
pub use protocol::Packet;
pub use protocol::PacketEncoder;
pub use protocol::PacketType;
pub use protocol::PingReq;
pub use protocol::PingResp;
pub use protocol::PubAck;
pub use protocol::PubComp;
pub use protocol::PubRec;
pub use protocol::PubRel;
pub use protocol::Publish;
pub use protocol::QoS;
pub use protocol::SubAck;
pub use protocol::Subscribe;
pub use protocol::UnsubAck;
pub use protocol::Unsubscribe;
pub use protocol::{
    read_string, read_variable_length, variable_length_length, write_string, write_variable_length,
};
pub use time::{DummyTimeSource, TimeSource};
pub use topics::{TopicEntry, TopicName, TopicRegistry, TopicSubscription};
