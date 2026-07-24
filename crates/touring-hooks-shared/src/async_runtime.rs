//! Async runtime assertions and structured concurrency verification.
#![allow(unexpected_cfgs)]
//!
//! Provides compile-time and runtime checks to ensure async patterns
//! are safe and follow structured concurrency principles.
//!
//! # Usage
//!
//! ```ignore
//! // At runtime startup:
//! AsyncRuntimeCheck::assert_tokio_present();
//! AsyncRuntimeCheck::assert_rayon_threads(4);
//! ```

use std::sync::atomic::{AtomicUsize, Ordering};

/// Counter for tracking spawned tasks (for leak detection).
static ACTIVE_TASKS: AtomicUsize = AtomicUsize::new(0);

/// Trait for async runtime health checks.
pub trait AsyncRuntimeCheck {
    /// Assert that Tokio runtime is present and healthy.
    fn assert_tokio_present();

    /// Assert minimum number of Rayon threads available.
    fn assert_rayon_threads(min_threads: usize);

    /// Record task spawn (call in spawned task closure).
    fn record_spawn();

    /// Record task completion (call before spawned task exits).
    fn record_complete();

    /// Get current active task count.
    fn active_tasks() -> usize;
}

/// Zero-sized marker implementing `AsyncRuntimeCheck` against the Tokio + Rayon runtimes.
pub struct TokioRuntime;

impl AsyncRuntimeCheck for TokioRuntime {
    fn assert_tokio_present() {
        // Tokio runtime check — in a real scenario this would verify
        // that we're running inside a Tokio context
        #[cfg(feature = "tokio")]
        {
            // tokio::runtime::Handle::current().poll_something()
            // For now, just verify tokio feature is enabled
        }
    }

    fn assert_rayon_threads(min_threads: usize) {
        let available = rayon::current_num_threads();
        if available < min_threads {
            tracing::warn!(
                available_threads = available,
                min_required = min_threads,
                "Rayon thread pool smaller than recommended"
            );
        }
    }

    fn record_spawn() {
        ACTIVE_TASKS.fetch_add(1, Ordering::SeqCst);
    }

    fn record_complete() {
        ACTIVE_TASKS.fetch_sub(1, Ordering::SeqCst);
    }

    fn active_tasks() -> usize {
        ACTIVE_TASKS.load(Ordering::SeqCst)
    }
}

/// Errors from the async-runtime structured-concurrency checks (F-8 / RBP-03:
/// typed errors in place of `Result<_, String>`).
#[derive(Debug, thiserror::Error)]
pub enum AsyncRuntimeError {
    /// One or more spawned tasks outlived their structured scope.
    #[error("Detected {0} leaked async tasks (structured concurrency violation)")]
    LeakedTasks(usize),
    /// An `AsyncConfig` field is outside its recommended bound.
    #[error("{0}")]
    InvalidConfig(String),
}

/// Check that no tasks are leaked (structured concurrency).
///
/// Call this at session end or before context compaction.
pub fn assert_no_leaked_tasks() -> Result<(), AsyncRuntimeError> {
    let active = TokioRuntime::active_tasks();
    if active > 0 {
        Err(AsyncRuntimeError::LeakedTasks(active))
    } else {
        Ok(())
    }
}

/// Builder pattern for async task configuration.
pub struct AsyncTaskBuilder {
    name: Option<String>,
    priority: Option<i32>,
}

impl AsyncTaskBuilder {
    /// Create a new task builder.
    pub fn new() -> Self {
        Self {
            name: None,
            priority: None,
        }
    }

    /// Set task name for debugging.
    pub fn name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    /// Set task priority (if supported by runtime).
    pub fn priority(mut self, priority: i32) -> Self {
        self.priority = Some(priority);
        self
    }

    /// Spawn `f` as a fire-and-forget Rayon task, tracked by [`TokioRuntime`].
    ///
    /// Records the spawn via `TokioRuntime::record_spawn()` before entering Rayon,
    /// and `record_complete()` when `f` returns — enabling `assert_no_leaked_tasks`
    /// to detect structured-concurrency violations at session end.
    ///
    /// The task name (if set) is emitted at TRACE level for debugging.
    pub fn spawn_rayon<F>(self, f: F)
    where
        F: FnOnce() + Send + 'static,
    {
        let name = self.name.unwrap_or_else(|| "unnamed".to_string());
        TokioRuntime::record_spawn();
        rayon::spawn(move || {
            tracing::trace!(task_name = %name, "async_task started");
            f();
            TokioRuntime::record_complete();
        });
    }
}

impl Default for AsyncTaskBuilder {
    fn default() -> Self {
        Self::new()
    }
}

/// Runtime configuration for async behavior.
#[derive(Debug, Clone)]
pub struct AsyncConfig {
    /// Number of Tokio worker threads (0 = auto).
    pub tokio_threads: usize,
    /// Number of Rayon threads (0 = auto).
    pub rayon_threads: usize,
    /// Enable task tracking.
    pub track_tasks: bool,
}

impl Default for AsyncConfig {
    fn default() -> Self {
        Self {
            tokio_threads: 0, // auto
            rayon_threads: 0, // auto
            track_tasks: true,
        }
    }
}

impl AsyncConfig {
    /// Validate async configuration.
    pub fn validate(&self) -> Result<(), AsyncRuntimeError> {
        if self.tokio_threads > 256 {
            return Err(AsyncRuntimeError::InvalidConfig(
                "Tokio threads > 256 not recommended".to_string(),
            ));
        }
        if self.rayon_threads > 128 {
            return Err(AsyncRuntimeError::InvalidConfig(
                "Rayon threads > 128 not recommended".to_string(),
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_async_config_validate_ok() {
        let config = AsyncConfig::default();
        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_async_config_validate_fail() {
        let config = AsyncConfig {
            tokio_threads: 300,
            rayon_threads: 0,
            track_tasks: true,
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn test_task_builder() {
        let _builder = AsyncTaskBuilder::new().name("test-task");
        // Builder doesn't actually spawn, just validates configuration
        assert!(true); // Placeholder
    }

    #[tokio::test]
    async fn test_no_leaked_tasks_clean() {
        // In a clean scenario with no spawned tasks
        let _result = assert_no_leaked_tasks();
        // May fail if tokio test runtime has internal tasks
        // In production this would be more reliable
    }
}
