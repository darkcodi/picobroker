//! Embassy time implementation

use picobroker_core::time::TimeSource;

/// Embassy time source
#[derive(Debug, Clone, Copy)]
pub struct EmbassyTimeSource;

impl TimeSource for EmbassyTimeSource {
    fn now_secs(&self) -> u64 {
        embassy_time::Instant::now().as_secs()
    }
}
