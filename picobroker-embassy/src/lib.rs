//! # PicoBroker Embassy
//!
//! Embassy runtime support for PicoBroker.
//!
//! This crate provides async networking and time implementations for embedded
//! systems using the Embassy framework. It re-exports all types from
//! `picobroker-core` for convenience.
//!
//! ## Usage
//!
//! ```toml
//! [dependencies]
//! picobroker-embassy = "0.1"
//! ```
//!
//! ```rust,ignore
//! #![no_std]
//! #![no_main]
//! use picobroker_embassy::*;
//!
//! #[embassy_executor::main(entry = "main")]
//! async fn main(sp: embassy_executor::Spawner) {
//!     let broker = EmbassyPicoBroker::<30, 30, 4, 4, 4>::new_embassy();
//!     // Use broker...
//! }
//! ```
//!
//! ## Status
//!
//! **NOTE**: Embassy networking implementations are currently stubs. The crate
//! will build but networking operations will return errors until the Embassy
//! integration is completed. Enable the `embassy` feature flag when ready:
//!
//! ```toml
//! picobroker-embassy = { version = "0.1", features = ["embassy"] }
//! ```

#![no_std]

pub mod broker;
pub mod network;
pub mod time;

// Re-export core for convenience
pub use picobroker_core::*;

// Embassy-specific types
pub use broker::{EmbassyPicoBroker};
pub use network::{EmbassyTcpListener, EmbassyTcpStream};
pub use time::EmbassyTimeSource;

/// Default broker type with common configuration
pub type DefaultPicoBroker = EmbassyPicoBroker<30, 30, 4, 4, 4>;
