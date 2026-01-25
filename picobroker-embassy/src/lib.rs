//! # PicoBroker Embassy
//!
//! Embassy runtime support for PicoBroker.
//!
//! This crate provides async networking and time implementations for embedded
//! systems using the Embassy framework. It re-exports all types from
//! `picobroker-core` for convenience.

#![no_std]

// Re-export core for convenience
pub use picobroker_core::*;

