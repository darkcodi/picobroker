//! # PicoBroker Tokio
//!
//! Tokio runtime support for PicoBroker.
//!
//! This crate provides async networking and time implementations for the
//! standard library using Tokio. It re-exports all types from `picobroker-core`
//! for convenience.
//!
//! ## Usage
//!
//! ```toml
//! [dependencies]
//! picobroker-tokio = "0.1"
//! ```
//!
//! ```rust
//! use picobroker_tokio::*;
//!
//! #[tokio::main]
//! async fn main() -> Result<()> {
//!     let broker = TokioPicoBroker::<30, 30, 4, 4, 4>::new_tokio();
//!     // Use broker...
//!     Ok(())
//! }
//! ```

pub mod network;
pub mod task;
pub mod time;

// Re-export core for convenience
pub use picobroker_core::*;

// Tokio-specific types
pub use network::{TokioTcpListener, TokioTcpStream};
pub use task::TokioTaskSpawner;
pub use time::StdTimeSource;

