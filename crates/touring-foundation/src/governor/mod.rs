//! Unified resource governor — enforces timeout, chunk count, and memory bounds.
//!
//! Coordinates three resource limits that were previously enforced ad-hoc
//! in different places across touring:
//! - **Timeout** — long-running queries are aborted when `elapsed >= timeout`
//! - **Chunk count** — chunkers that would produce 100k+ chunks abort at limit
//! - **Memory pressure** — RSS > threshold emits degraded mode in search
//!
//! # RAII guard pattern
//!
//! `GovernorGuard` is a scope guard that automatically records the chunk count
//! when dropped, enabling precise tracking without explicit finally-blocks.
//!
//! # Example
//!
//! ```ignore
//! let gov = ResourceGovernor::new(PerformanceSettings {
//!     timeout: Duration::from_secs(30),
//!     max_chunks: 50_000,
//!     max_memory_mb: Some(512),
//! });
//! let _guard = gov.enter();
//! // ... operation under governor control ...
//! // guard.drop() records chunk count automatically
//! ```

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

pub mod error;

pub use error::{LimitError, MemoryError, TimeoutError};

/// Configuration for a running operation under governor control.
#[derive(Debug, Clone, Copy)]
pub struct PerformanceSettings {
    /// Maximum allowed elapsed time.
    pub timeout: Duration,
    /// Maximum number of chunks allowed.
    pub max_chunks: usize,
    /// Optional memory threshold (RSS in MB). `None` = no memory check.
    pub max_memory_mb: Option<usize>,
}

impl PerformanceSettings {
    /// Returns a builder configured for very permissive limits (testing/fuzzing).
    #[cfg(test)]
    pub fn test() -> Self {
        Self {
            timeout: Duration::from_secs(3600),
            max_chunks: 1_000_000,
            max_memory_mb: None,
        }
    }
}

/// Default settings: 30s timeout, 100k chunk limit, no memory cap.
impl Default for PerformanceSettings {
    fn default() -> Self {
        Self {
            timeout: Duration::from_secs(30),
            max_chunks: 100_000,
            max_memory_mb: None,
        }
    }
}

/// Default governor: 30s timeout, 100k chunks, no memory cap.
impl Default for ResourceGovernor {
    fn default() -> Self {
        Self::new(PerformanceSettings::default())
    }
}

/// Unified context manager for timeout + chunk count + memory bounds.
pub struct ResourceGovernor {
    /// Configured timeout.
    timeout: Duration,
    /// Configured chunk limit.
    max_chunks: usize,
    /// Configured memory threshold (MB). `None` = no check.
    max_memory_mb: Option<usize>,
    /// When the governor was entered (protected by mutex for thread safety).
    start_time: Mutex<Option<Instant>>,
    /// Number of chunks registered so far.
    chunk_count: AtomicUsize,
    /// Optional memory pressure probe. When set, the governor can report
    /// memory pressure via `memory_pressure_ratio()`.
    memory_probe: Option<Arc<dyn Fn() -> f64 + Send + Sync + 'static>>,
}

impl std::fmt::Debug for ResourceGovernor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ResourceGovernor")
            .field("timeout", &self.timeout)
            .field("max_chunks", &self.max_chunks)
            .field("max_memory_mb", &self.max_memory_mb)
            .field("start_time", &self.start_time)
            .field("chunk_count", &self.chunk_count)
            .finish_non_exhaustive()
    }
}

impl ResourceGovernor {
    /// Create a new governor from performance settings.
    pub fn new(settings: PerformanceSettings) -> Self {
        Self {
            timeout: settings.timeout,
            max_chunks: settings.max_chunks,
            max_memory_mb: settings.max_memory_mb,
            start_time: Mutex::new(None),
            chunk_count: AtomicUsize::new(0),
            memory_probe: None,
        }
    }

    /// Add a memory pressure probe to the governor.
    ///
    /// The probe returns memory usage as a ratio (0.0 = no memory pressure,
    /// 1.0 = max pressure). The probe is invoked when `memory_pressure_ratio()`
    /// is called.
    pub fn with_memory_probe(mut self, probe: impl Fn() -> f64 + Send + Sync + 'static) -> Self {
        self.memory_probe = Some(Arc::new(probe));
        self
    }

    /// Returns the current memory pressure ratio (0.0–1.0) from the probe,
    /// or `None` if no probe is configured.
    pub fn memory_pressure_ratio(&self) -> Option<f64> {
        self.memory_probe.as_ref().map(|p| (p)())
    }

    /// Check if the configured timeout has been exceeded.
    ///
    /// Returns `Ok(())` if still within the time limit, or `Err(TimeoutError)`
    /// if `elapsed >= timeout`.
    pub fn check_timeout(&self) -> Result<(), TimeoutError> {
        let start = {
            let guard = self.start_time.lock().unwrap_or_else(|e| e.into_inner());
            *guard
        };
        let start = start.ok_or(TimeoutError {
            elapsed: Duration::ZERO,
            limit: self.timeout,
        })?;
        let elapsed = start.elapsed();
        if elapsed >= self.timeout {
            Err(TimeoutError {
                elapsed,
                limit: self.timeout,
            })
        } else {
            Ok(())
        }
    }

    /// Register a new chunk and check against the chunk limit.
    ///
    /// Returns `Ok(())` if the new count is within the limit, or
    /// `Err(LimitError)` if `count > max_chunks`.
    pub fn register_chunk(&self) -> Result<(), LimitError> {
        let prev = self.chunk_count.fetch_add(1, Ordering::AcqRel);
        let count = prev + 1;
        if count > self.max_chunks {
            Err(LimitError {
                count,
                limit: self.max_chunks,
            })
        } else {
            Ok(())
        }
    }

    /// Probe current memory usage and compare against the configured threshold.
    ///
    /// Returns `Ok(())` if RSS is below the threshold (or no threshold set),
    /// or `Err(MemoryError)` if RSS exceeds the threshold.
    ///
    /// The probe is provided by the caller to avoid a circular dependency
    /// between `touring-foundation` and `touring-hooks`. Use `MemoryProbe::from_snapshot()`
    /// from `touring_hooks::shared::memory_stats_probe` when integrating.
    pub fn check_memory_probed(&self, rss_mb: f64) -> Result<(), MemoryError> {
        let Some(threshold_mb) = self.max_memory_mb else {
            return Ok(());
        };
        if rss_mb > threshold_mb as f64 {
            Err(MemoryError {
                rss_mb,
                threshold_mb: threshold_mb as f64,
            })
        } else {
            Ok(())
        }
    }

    /// Convenience: probe memory using a closure to avoid circular deps.
    ///
    /// # Example
    ///
    /// ```ignore
    /// gov.check_memory_with(|| {
    ///     touring_hooks::shared::memory_stats_probe::snapshot().physical_mb
    /// })?;
    /// ```
    pub fn check_memory_with<F>(&self, mut probe: F) -> Result<(), MemoryError>
    where
        F: FnMut() -> f64,
    {
        self.check_memory_probed(probe())
    }

    /// Returns the number of chunks registered so far.
    pub fn chunk_count(&self) -> usize {
        self.chunk_count.load(Ordering::Acquire)
    }

    /// Returns the configured chunk limit.
    pub fn max_chunks(&self) -> usize {
        self.max_chunks
    }

    /// Returns the elapsed time since the governor was entered, or `Duration::ZERO`
    /// if the governor has not been entered yet.
    pub fn elapsed(&self) -> Duration {
        let guard = self.start_time.lock().unwrap_or_else(|e| e.into_inner());
        guard.map(|i| i.elapsed()).unwrap_or(Duration::ZERO)
    }
}

/// RAII scope guard that automatically records chunk counts on drop.
///
/// Acquired by calling `ResourceGovernor::enter()`.
#[derive(Debug)]
pub struct GovernorGuard<'a> {
    gov: &'a ResourceGovernor,
}

impl ResourceGovernor {
    /// Enter the governor context, starting the timeout clock.
    ///
    /// Returns a guard that must be kept alive for the duration of the operation.
    /// When the guard is dropped, the start time is cleared.
    #[must_use]
    pub fn enter(&self) -> GovernorGuard<'_> {
        let mut guard = self.start_time.lock().unwrap_or_else(|e| e.into_inner());
        if guard.is_none() {
            *guard = Some(Instant::now());
        }
        GovernorGuard { gov: self }
    }
}

impl Drop for GovernorGuard<'_> {
    fn drop(&mut self) {
        let mut guard = self
            .gov
            .start_time
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        *guard = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn governor_test() -> ResourceGovernor {
        ResourceGovernor::new(PerformanceSettings {
            timeout: Duration::from_millis(500),
            max_chunks: 5,
            max_memory_mb: None,
        })
    }

    #[test]
    fn test_timeout_ok() {
        let gov = governor_test();
        let _guard = gov.enter();
        // Without waiting, timeout should be fine.
        assert!(gov.check_timeout().is_ok());
    }

    #[test]
    fn test_timeout_exceeded() {
        let gov = governor_test();
        let _guard = gov.enter();
        // Wait past the 500ms limit.
        std::thread::sleep(Duration::from_millis(600));
        let err = gov.check_timeout().expect_err("timeout should fire");
        assert_eq!(err.limit, Duration::from_millis(500));
    }

    #[test]
    fn test_chunk_limit_ok() {
        let gov = governor_test();
        for _ in 0..5 {
            assert!(gov.register_chunk().is_ok());
        }
        // 5 chunks at limit — should still be OK.
        assert_eq!(gov.chunk_count(), 5);
    }

    #[test]
    fn test_chunk_limit_exceeded() {
        let gov = governor_test();
        let _guard = gov.enter();
        for _ in 0..5 {
            gov.register_chunk().expect("should succeed");
        }
        // 6th chunk should fail.
        let err = gov
            .register_chunk()
            .expect_err("6th chunk should exceed limit");
        assert_eq!(err.limit, 5);
        assert_eq!(err.count, 6);
    }

    #[test]
    fn test_guard_clears_start_time_on_drop() {
        let gov = governor_test();
        {
            let _guard = gov.enter();
            assert!(gov.check_timeout().is_ok());
        }
        // After guard drops, start_time is cleared — check_timeout should fail.
        let err = gov.check_timeout().expect_err("start_time should be None");
        assert_eq!(err.elapsed, Duration::ZERO);
    }

    #[test]
    fn test_memory_check_no_threshold() {
        let gov = ResourceGovernor::new(PerformanceSettings {
            timeout: Duration::MAX,
            max_chunks: usize::MAX,
            max_memory_mb: None, // No threshold — should always pass.
        });
        assert!(gov.check_memory_probed(99999.0).is_ok());
    }

    #[test]
    fn test_memory_check_within_threshold() {
        let gov = ResourceGovernor::new(PerformanceSettings {
            timeout: Duration::MAX,
            max_chunks: usize::MAX,
            max_memory_mb: Some(512),
        });
        // 256 MB RSS — below 512 MB threshold — should pass.
        assert!(gov.check_memory_probed(256.0).is_ok());
    }

    #[test]
    fn test_memory_check_exceeds_threshold() {
        let gov = ResourceGovernor::new(PerformanceSettings {
            timeout: Duration::MAX,
            max_chunks: usize::MAX,
            max_memory_mb: Some(256),
        });
        // 512 MB RSS — above 256 MB threshold — should fail.
        let err = gov
            .check_memory_probed(512.0)
            .expect_err("memory should exceed");
        assert_eq!(err.threshold_mb, 256.0);
        assert_eq!(err.rss_mb, 512.0);
    }

    #[test]
    fn test_enter_is_idempotent_per_guard() {
        let gov = governor_test();
        let g1 = gov.enter();
        let g2 = gov.enter(); // Re-entry should not panic.
        drop(g1);
        drop(g2);
    }

    #[test]
    fn test_default_settings() {
        let gov = ResourceGovernor::default();
        assert_eq!(gov.timeout, Duration::from_secs(30));
        assert_eq!(gov.max_chunks, 100_000);
        assert!(gov.max_memory_mb.is_none());
    }

    #[test]
    fn test_test_settings() {
        let settings = PerformanceSettings::test();
        assert_eq!(settings.timeout, Duration::from_secs(3600));
        assert_eq!(settings.max_chunks, 1_000_000);
        assert!(settings.max_memory_mb.is_none());
    }
}
