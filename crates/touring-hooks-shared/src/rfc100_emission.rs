//! Centralized RFC-100 diagnostic emission helpers for W-codes.
//!
//! Reduces duplication across CLI handlers by providing a single emission point
//! for wiring-related diagnostics (W-100 series).
//!
//! # W-codes covered
//! - `W-101`: Low integration score (< 1.0)
//! - `W-110`: Dependency cycles detected
//! - `W-120`: Stale index detected
//!
//! # Example
//! ```
//! use touring_hooks_shared::rfc100_emission::Rfc100Emitter;
//!
//! Rfc100Emitter::emit_w101_low_integration("touring-hooks/src/lib.rs", 0.85);
//! Rfc100Emitter::emit_w110_dependency_cycle("touring-hooks/src/wiring.rs", 3);
//! Rfc100Emitter::emit_w120_stale_index("touring-hooks/src/index.rs", 3600);
//! ```

use touring_foundation::diagnostic::{Diagnostic, Severity, codes};

/// Centralized emitter for RFC-100 wiring diagnostics.
///
/// Provides structured emission helpers for W-101, W-110, W-120 codes
/// that ensure consistent formatting and proper use of `codes::*` constants.
#[derive(Debug, Clone, Copy)]
pub struct Rfc100Emitter;

impl Rfc100Emitter {
    /// Emit W-101 LOW_INTEGRATION when integration_score < 1.0.
    ///
    /// Only emits when score is strictly below threshold to avoid noise.
    ///
    /// # Arguments
    /// * `module_path` - Path to the module with low integration (used in message)
    /// * `score` - Current integration score (0.0 to 1.0)
    #[inline]
    pub fn emit_w101_low_integration(module_path: &str, score: f64) {
        if score < 1.0 {
            let diag = Diagnostic::new(
                codes::W_101_LOW_INTEGRATION,
                Severity::Warning,
                format!(
                    "Module integration score {:.2} below threshold 1.0: {}",
                    score, module_path
                ),
            )
            .with_file(module_path);

            tracing::warn!(
                code = %diag.code,
                message = %diag.message,
                severity = %diag.severity,
                file_path = module_path,
                integration_score = score,
                "W-101: low integration score"
            );
        }
    }

    /// Emit W-110 DEPENDENCY_CYCLE when cycles are detected.
    ///
    /// Only emits when cycle_count > 0.
    ///
    /// # Arguments
    /// * `module_path` - Path to the module where cycles were detected
    /// * `cycle_count` - Number of dependency cycles detected
    #[inline]
    pub fn emit_w110_dependency_cycle(module_path: &str, cycle_count: usize) {
        if cycle_count > 0 {
            let diag = Diagnostic::new(
                codes::W_110_DEPENDENCY_CYCLE,
                Severity::Warning,
                format!(
                    "{} dependency cycle(s) detected in: {}",
                    cycle_count, module_path
                ),
            )
            .with_file(module_path);

            tracing::warn!(
                code = %diag.code,
                message = %diag.message,
                severity = %diag.severity,
                file_path = module_path,
                cycle_count = cycle_count,
                "W-110: dependency cycle detected"
            );
        }
    }

    /// Emit W-120 STALE_INDEX when index age exceeds threshold.
    ///
    /// Always emits with the age information.
    ///
    /// # Arguments
    /// * `module_path` - Path to the module with stale index
    /// * `last_index_age_secs` - Age of the index in seconds
    #[inline]
    pub fn emit_w120_stale_index(module_path: &str, last_index_age_secs: u64) {
        let diag = Diagnostic::new(
            codes::W_120_STALE_INDEX,
            Severity::Warning,
            format!(
                "Index stale ({} seconds) for: {}",
                last_index_age_secs, module_path
            ),
        )
        .with_file(module_path);

        tracing::warn!(
            code = %diag.code,
            message = %diag.message,
            severity = %diag.severity,
            file_path = module_path,
            index_age_secs = last_index_age_secs,
            "W-120: stale index detected"
        );
    }

    /// Build a W-101 diagnostic for external consumers (e.g., testing).
    ///
    /// Returns the Diagnostic struct without emitting.
    #[inline]
    #[must_use]
    pub fn build_w101_low_integration(module_path: &str, score: f64) -> Diagnostic {
        Diagnostic::new(
            codes::W_101_LOW_INTEGRATION,
            Severity::Warning,
            format!(
                "Module integration score {:.2} below threshold 1.0: {}",
                score, module_path
            ),
        )
        .with_file(module_path)
    }

    /// Build a W-110 diagnostic for external consumers (e.g., testing).
    ///
    /// Returns the Diagnostic struct without emitting.
    #[inline]
    #[must_use]
    pub fn build_w110_dependency_cycle(module_path: &str, cycle_count: usize) -> Diagnostic {
        Diagnostic::new(
            codes::W_110_DEPENDENCY_CYCLE,
            Severity::Warning,
            format!(
                "{} dependency cycle(s) detected in: {}",
                cycle_count, module_path
            ),
        )
        .with_file(module_path)
    }

    /// Build a W-120 diagnostic for external consumers (e.g., testing).
    ///
    /// Returns the Diagnostic struct without emitting.
    #[inline]
    #[must_use]
    pub fn build_w120_stale_index(module_path: &str, last_index_age_secs: u64) -> Diagnostic {
        Diagnostic::new(
            codes::W_120_STALE_INDEX,
            Severity::Warning,
            format!(
                "Index stale ({} seconds) for: {}",
                last_index_age_secs, module_path
            ),
        )
        .with_file(module_path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_w101_emits_only_when_below_threshold() {
        // Should not emit (score >= 1.0)
        let diag_above = Rfc100Emitter::build_w101_low_integration("test.rs", 1.0);
        assert_eq!(diag_above.code, "W-101");

        // Should not emit (score >= 1.0)
        let diag_equal = Rfc100Emitter::build_w101_low_integration("test.rs", 1.0);
        assert_eq!(diag_equal.code, "W-101");

        // Should emit (score < 1.0)
        let diag_below = Rfc100Emitter::build_w101_low_integration("test.rs", 0.85);
        assert_eq!(diag_below.code, "W-101");
        assert!(diag_below.message.contains("0.85"));
        assert!(diag_below.message.contains("test.rs"));
    }

    #[test]
    fn test_w110_emits_only_when_cycles_exist() {
        // Should not emit (cycle_count = 0)
        let diag_zero = Rfc100Emitter::build_w110_dependency_cycle("test.rs", 0);
        assert_eq!(diag_zero.code, "W-110");

        // Should emit (cycle_count > 0)
        let diag_three = Rfc100Emitter::build_w110_dependency_cycle("test.rs", 3);
        assert_eq!(diag_three.code, "W-110");
        assert!(diag_three.message.contains("3"));
        assert!(diag_three.message.contains("test.rs"));
    }

    #[test]
    fn test_w120_always_emits() {
        let diag = Rfc100Emitter::build_w120_stale_index("test.rs", 3600);
        assert_eq!(diag.code, "W-120");
        assert!(diag.message.contains("3600"));
        assert!(diag.message.contains("test.rs"));
    }

    #[test]
    fn test_w101_message_format() {
        let diag = Rfc100Emitter::build_w101_low_integration("module/path.rs", 0.75);
        assert!(diag.message.starts_with("Module integration score"));
        assert!(diag.message.contains("0.75"));
        assert!(diag.message.contains("0.75"));
        assert!(diag.message.contains("module/path.rs"));
        assert_eq!(diag.file.as_deref(), Some("module/path.rs"));
    }

    #[test]
    fn test_w110_message_format() {
        let diag = Rfc100Emitter::build_w110_dependency_cycle("module/path.rs", 2);
        assert!(diag.message.starts_with("2 dependency cycle"));
        assert!(diag.message.contains("2"));
        assert!(diag.message.contains("module/path.rs"));
    }

    #[test]
    fn test_w120_message_format() {
        let diag = Rfc100Emitter::build_w120_stale_index("module/path.rs", 7200);
        assert!(diag.message.starts_with("Index stale"));
        assert!(diag.message.contains("7200"));
        assert!(diag.message.contains("module/path.rs"));
    }

    #[test]
    fn test_diagnostic_code_constants() {
        assert_eq!(codes::W_101_LOW_INTEGRATION, "W-101");
        assert_eq!(codes::W_110_DEPENDENCY_CYCLE, "W-110");
        assert_eq!(codes::W_120_STALE_INDEX, "W-120");
    }

    #[test]
    fn test_severity_is_warning() {
        let w101 = Rfc100Emitter::build_w101_low_integration("test.rs", 0.5);
        let w110 = Rfc100Emitter::build_w110_dependency_cycle("test.rs", 1);
        let w120 = Rfc100Emitter::build_w120_stale_index("test.rs", 100);

        assert_eq!(w101.severity, Severity::Warning);
        assert_eq!(w110.severity, Severity::Warning);
        assert_eq!(w120.severity, Severity::Warning);
    }
}
