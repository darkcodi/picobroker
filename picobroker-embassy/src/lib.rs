//! # PicoBroker Embassy
//!
//! Embassy runtime support for PicoBroker.
//!
//! This crate provides async networking and time implementations for embedded
//! systems using the Embassy framework. It re-exports all types from
//! `picobroker-core` for convenience.

#![no_std]

pub mod broker;
pub mod network;
pub mod time;

// Re-export core for convenience
pub use picobroker_core::*;

// Embassy-specific types
pub use broker::EmbassyPicoBroker;
pub use network::{EmbassyTcpListener, EmbassyTcpStream};
pub use time::EmbassyTimeSource;

/// Default broker type with common configuration
pub type DefaultPicoBroker = EmbassyPicoBroker<30, 4, 4, 4, 10, 10, 256>;
