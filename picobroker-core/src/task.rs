//! Task spawning abstraction for runtime-agnostic async task spawning

/// Error that can occur when spawning a task
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnError {
    /// No memory available to spawn task
    NoMemory,
    /// Task spawner is busy
    Busy,
}

impl core::fmt::Display for SpawnError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            SpawnError::NoMemory => write!(f, "No memory available to spawn task"),
            SpawnError::Busy => write!(f, "Task spawner is busy"),
        }
    }
}

/// Trait for spawning async tasks across different runtimes
///
/// This trait abstracts task spawning, allowing picobroker-core to spawn
/// concurrent client handler tasks without knowing the specific async runtime.
pub trait TaskSpawner {
    /// Spawn a new task that runs the given future to completion
    fn spawn<F, O>(&self, future: F) -> Result<(), SpawnError>
    where
        F: core::future::Future<Output = O> + Send + 'static,
        O: Send + 'static;
}
