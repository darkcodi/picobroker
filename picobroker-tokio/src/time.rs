//! Std time implementation

use picobroker_core::TimeSource;

/// Standard library time source
#[derive(Debug, Clone, Copy)]
pub struct StdTimeSource;

impl TimeSource for StdTimeSource {
    fn now_secs(&self) -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs()
    }
}
