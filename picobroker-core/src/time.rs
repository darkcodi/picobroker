//! Time abstraction for different platforms

/// Time source trait
///
/// Abstracts time operations for both std and embedded platforms
pub trait TimeSource {
    /// Get current time in seconds since Unix epoch
    fn now_secs(&self) -> u64;
}

/// Default time source for no_std (returns 0)
#[derive(Debug, Clone, Copy)]
pub struct DummyTimeSource;

impl TimeSource for DummyTimeSource {
    fn now_secs(&self) -> u64 {
        0
    }
}
