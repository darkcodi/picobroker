//! Tokio-specific task spawner implementation

use picobroker_core::SpawnError;
use picobroker_core::TaskSpawner;
use std::sync::Arc;

/// Tokio-based task spawner
///
/// This implementation uses Tokio's runtime to spawn tasks.
#[derive(Debug, Clone)]
pub struct TokioTaskSpawner {
    _runtime: Arc<tokio::runtime::Handle>,
}

impl TokioTaskSpawner {
    /// Create a new TokioTaskSpawner from the current Tokio runtime handle
    pub fn from_current_handle() -> Self {
        Self {
            _runtime: Arc::new(tokio::runtime::Handle::current()),
        }
    }

    /// Create a new TokioTaskSpawner from a specific Tokio runtime handle
    pub fn from_handle(handle: tokio::runtime::Handle) -> Self {
        Self {
            _runtime: Arc::new(handle),
        }
    }
}

impl TaskSpawner for TokioTaskSpawner {
    fn spawn<F, O>(&self, future: F) -> Result<(), SpawnError>
    where
        F: core::future::Future<Output = O> + Send + 'static,
        O: Send + 'static,
    {
        self._runtime.spawn(future);
        Ok(())
    }
}

impl Default for TokioTaskSpawner {
    fn default() -> Self {
        Self::from_current_handle()
    }
}
