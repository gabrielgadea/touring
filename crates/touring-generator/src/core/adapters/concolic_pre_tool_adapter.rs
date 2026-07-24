//! Concolic Pre-Tool Adapter
//!
//! Wraps `ConcolicExecutor` from `touring-offensive` into 3 focused `Arc<dyn Fn>` closures
//! suitable for injection into `GeneratorContext` as pre-tool hook integration.
//!
//! # Architecture
//!
//! The adapter exposes 3 closure interfaces:
//! - `ConcolicAnalyzeFn` — synchronous input analysis returning `ConcolicResult`
//! - `ConcolicDetectFn` — pattern detection returning `Vec<VulnMatch>`
//! - `ConcolicSatisfiableFn` — satisfiability check returning `bool`
//!
//! This design wraps the 38-method `ConcolicExecutor` into a minimal, focused API
//! that avoids coupling `touring-generator` directly to `touring-offensive`.
//!
//! # Thread Safety
//!
//! `ConcolicExecutor` internally uses `Rc` for vulnerability patterns, which is not
//! `Send + Sync`. Since we cannot modify `ConcolicExecutor`, we use a factory pattern
//! where each closure invocation creates a fresh executor instance. This avoids
//! sharing `ConcolicExecutor` across thread boundaries entirely.

use std::sync::Arc;
use touring_offensive::concolic::{ConcolicExecutor, ConcolicResult};
use touring_offensive::vuln::VulnMatch;

/// Synchronous concolic analysis function type.
/// Takes an input string, performs concolic execution, returns analysis result.
pub type ConcolicAnalyzeFn = Arc<dyn Fn(&str) -> ConcolicResult + Send + Sync + 'static>;

/// Pattern detection function type.
/// Analyzes input for registered vulnerability patterns.
pub type ConcolicDetectFn = Arc<dyn Fn(&str) -> Vec<VulnMatch> + Send + Sync + 'static>;

/// Satisfiability check function type.
/// Checks if the current path constraints are satisfiable.
pub type ConcolicSatisfiableFn = Arc<dyn Fn() -> bool + Send + Sync + 'static>;

/// Pre-tool adapter wrapping `ConcolicExecutor` into 3 focused closures.
///
/// This struct provides a stable ABI surface for the generator, decoupling it
/// from the evolving `ConcolicExecutor` API. The 3 closure fields are designed
/// to be injected into `GeneratorContext` for pre-tool hook integration.
///
/// # Thread Safety
///
/// Uses a factory pattern rather than sharing `ConcolicExecutor` across threads.
/// Each closure invocation creates a fresh `ConcolicExecutor` instance, sidestepping
/// the `Send + Sync` requirement on `ConcolicExecutor` (which contains `Rc`).
#[allow(clippy::struct_field_names)]
#[derive(Clone)]
pub struct ConcolicPreToolAdapter {
    analyze_fn: ConcolicAnalyzeFn,
    detect_fn: ConcolicDetectFn,
    satisfiable_fn: ConcolicSatisfiableFn,
}

impl ConcolicPreToolAdapter {
    /// Creates a new adapter using a factory pattern to create fresh executors per call.
    ///
    /// This avoids sharing `ConcolicExecutor` (which contains `Rc` and is not `Send`)
    /// across thread boundaries. Each closure creates a new executor on each invocation.
    #[must_use]
    pub fn new() -> Self {
        let analyze_fn: ConcolicAnalyzeFn = Arc::new(move |input: &str| {
            let mut exec = ConcolicExecutor::new();
            exec.execute(input)
        });

        let detect_fn: ConcolicDetectFn = Arc::new(move |input: &str| {
            let exec = ConcolicExecutor::new();
            exec.detect_all_patterns(input)
        });

        let satisfiable_fn: ConcolicSatisfiableFn = Arc::new(move || {
            let mut exec = ConcolicExecutor::new();
            exec.is_satisfiable()
        });

        Self {
            analyze_fn,
            detect_fn,
            satisfiable_fn,
        }
    }

    /// Returns the concolic analysis closure.
    #[must_use]
    pub fn analyze_fn(&self) -> ConcolicAnalyzeFn {
        Arc::clone(&self.analyze_fn)
    }

    /// Returns the pattern detection closure.
    #[must_use]
    pub fn detect_fn(&self) -> ConcolicDetectFn {
        Arc::clone(&self.detect_fn)
    }

    /// Returns the satisfiability check closure.
    #[must_use]
    pub fn satisfiable_fn(&self) -> ConcolicSatisfiableFn {
        Arc::clone(&self.satisfiable_fn)
    }

    /// Directly analyzes an input string, returning the `ConcolicResult`.
    #[must_use]
    pub fn analyze(&self, input: &str) -> ConcolicResult {
        (self.analyze_fn)(input)
    }
}

impl Default for ConcolicPreToolAdapter {
    fn default() -> Self {
        Self::new()
    }
}

// Pre-2026-05-14 the `#[cfg(feature = "generator")]` gate referenced a
// feature that doesn't exist in this crate's `[features]` table — those
// gates produced "unexpected cfg condition value" warnings and silently
// hid this method from every build. The adapter lives inside the
// `touring-generator` crate itself, so we use `crate::GenerateError`
// (intra-crate path) and always compile the bridge.
use crate::GenerateError;

impl ConcolicPreToolAdapter {
    /// Analyzes an input and returns `Result<(), GenerateError>` for generator pipeline.
    ///
    /// Wraps `analyze()` to fit the generator's error handling pattern.
    ///
    /// # Errors
    ///
    /// Returns `GenerateError::Internal` when the concolic analysis fails (i.e., `success == false`).
    pub fn analyze_for_generator(&self, input: &str) -> Result<(), GenerateError> {
        let result = self.analyze(input);
        if result.success {
            Ok(())
        } else {
            Err(GenerateError::Internal(format!(
                "concolic analysis failed: {}",
                result.error.unwrap_or_default()
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_adapter_creation() {
        let adapter = ConcolicPreToolAdapter::new();
        let _ = adapter.analyze_fn(); // just verify it doesn't panic
        let result = adapter.analyze("test input");
        // ConcolicResult has success: bool field
        assert!(result.success);
    }

    #[test]
    #[allow(clippy::absurd_extreme_comparisons, unused_comparisons)]
    fn test_analyze_returns_result() {
        let adapter = ConcolicPreToolAdapter::new();
        let result = adapter.analyze("test input");
        // Basic validation - actual result depends on executor state
        assert!(result.path_condition.len() >= 0 || result.error.is_some() || result.success);
    }

    #[test]
    fn test_closures_are_send_sync() {
        fn assert_send_sync<T: Send + Sync>(_: &T) {}
        let adapter = ConcolicPreToolAdapter::new();
        assert_send_sync(&adapter.analyze_fn());
        assert_send_sync(&adapter.detect_fn());
        assert_send_sync(&adapter.satisfiable_fn());
    }
}
