//! H90: DriftAuditHandler
//!
//! Detects statistical drift (KS-test) in edit success rates using
//! `touring_simd::DriftDetector`. Complements the CUSUM drift detection
//! in touring-learning with a distribution-level KS-test from touring-simd.
//!
//! On PostToolUse: reads recent edit history, splits into baseline and current
//! windows, computes KS statistic on success rate, and records drift events
//! via `persistence.drift_record` when significant drift is detected.

use touring_simd::DriftDetector;
use touring_simd::statistics::has_significant_drift;
// DriftDetection trait needed for ks_statistic method
use touring_simd::DriftDetection;

use crate::context::CortexContext;
use crate::handler::Handler;
use crate::pipeline::Pipeline;
use crate::types::{HandlerResult, HookEvent};

/// Minimum context budget (in chars) required before running drift audit.
const MIN_BUDGET: usize = 20;

/// Minimum number of edits required to form meaningful baseline + current windows.
const MIN_EDITS: usize = 20;

/// Total number of edits to fetch from knowledge DB.
const EDIT_WINDOW: usize = 100;

/// Baseline window: edits 60..100 (the 40 oldest in the fetched window).
const BASELINE_START: usize = 60;

/// Current window: edits 0..40 (the 40 most recent in the fetched window).
const CURRENT_END: usize = 40;

/// KS statistic threshold for declaring significant drift.
const KS_THRESHOLD: f64 = 0.05;

/// H90: Drift audit handler — KS-test drift detection via touring-simd.
///
/// Architecture:
/// ```text
/// PostToolUse:
///   knowledge.recent_edits_all(100) → [EditEvent]
///   split: baseline (60..100), current (0..40)
///   metric: error_pattern.is_none() → 1.0 (success), 0.0 (error)
///   DriftDetector::ks_statistic(baseline, current) → ks_stat
///   has_significant_drift(ks_stat, 0.05) → bool
///   if drift: persistence.drift_record("edit_success_drift", ks_stat)
/// ```
#[derive(Default)]
pub struct DriftAuditHandler;

impl DriftAuditHandler {
    /// Creates a new drift-audit handler.
    pub fn new() -> Self {
        Self
    }
}

impl Handler for DriftAuditHandler {
    fn name(&self) -> &str {
        "H90_drift_audit"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PostToolUse]
    }

    fn priority(&self) -> u8 {
        190 // After H88 rules (185), before ranking (250)
    }

    fn dependency_tier(&self) -> u8 {
        1 // T1: needs knowledge DB to read edit history
    }

    fn timeout_ms(&self) -> u64 {
        40
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        // Skip if context budget is too low.
        if ctx.context_budget_remaining < MIN_BUDGET {
            return HandlerResult::skip(self.name());
        }

        // Fetch recent edits from knowledge DB.
        let edits = match ctx.knowledge.recent_edits_all(EDIT_WINDOW) {
            Ok(e) => e,
            Err(_) => return HandlerResult::skip(self.name()),
        };

        // Need at least MIN_EDITS to form meaningful windows.
        if edits.len() < MIN_EDITS {
            return HandlerResult::skip(self.name());
        }

        // Build current window (most recent edits, indices 0..CURRENT_END).
        let current_end = edits.len().min(CURRENT_END);
        let current_sample: Vec<f64> = edits
            .get(..current_end)
            .unwrap_or(&[])
            .iter()
            .map(|e| if e.error_pattern.is_none() { 1.0 } else { 0.0 })
            .collect();

        // Build baseline window (older edits, indices BASELINE_START..len).
        let baseline_start = edits.len().min(BASELINE_START);
        let baseline_sample: Vec<f64> = edits
            .get(baseline_start..)
            .unwrap_or(&[])
            .iter()
            .map(|e| if e.error_pattern.is_none() { 1.0 } else { 0.0 })
            .collect();

        // Need non-empty samples for KS test.
        if baseline_sample.is_empty() || current_sample.is_empty() {
            return HandlerResult::skip(self.name());
        }

        // Compute KS statistic between baseline and current windows.
        let detector = DriftDetector::new();
        let ks_stat = detector.ks_statistic(&baseline_sample, &current_sample);

        // Check if drift is significant.
        if has_significant_drift(ks_stat, KS_THRESHOLD)
            && let Some(ref persistence) = ctx.persistence
        {
            let _ = persistence.drift_record("edit_success_drift", ks_stat);
        }

        // Never inject context — drift audit is observational only.
        HandlerResult::skip(self.name())
    }
}

/// Register H90 DriftAuditHandler.
pub fn register(pipeline: &mut Pipeline) {
    pipeline.register(Box::new(DriftAuditHandler::new()));
}

#[cfg(test)]
mod tests {
    use super::*;
    use touring_simd::DriftDetection;

    // ── trait property tests ──────────────────────────────────────────────

    #[test]
    fn test_handler_name() {
        let h = DriftAuditHandler::new();
        assert_eq!(h.name(), "H90_drift_audit");
    }

    #[test]
    fn test_handler_events() {
        let h = DriftAuditHandler::new();
        let events = h.events();
        assert_eq!(events, &[HookEvent::PostToolUse]);
    }

    #[test]
    fn test_handler_priority_190() {
        let h = DriftAuditHandler::new();
        assert_eq!(h.priority(), 190);
        assert!(h.priority() > 185, "must run after H88 rules (185)");
    }

    #[test]
    fn test_handler_not_critical() {
        let h = DriftAuditHandler::new();
        assert!(!h.is_critical(), "drift audit is not critical");
    }

    #[test]
    fn test_handler_timeout_fast() {
        let h = DriftAuditHandler::new();
        assert!(h.timeout_ms() <= 50, "must be fast (<= 50ms)");
        assert_eq!(h.timeout_ms(), 40);
    }

    #[test]
    fn test_handler_dependency_tier_1() {
        let h = DriftAuditHandler::new();
        assert_eq!(h.dependency_tier(), 1, "T1: needs knowledge DB");
    }

    #[test]
    fn test_handler_not_async() {
        let h = DriftAuditHandler::new();
        assert!(!h.is_async(), "drift audit is synchronous");
    }

    // ── DriftDetector behavioral tests ───────────────────────────────────

    #[test]
    fn test_drift_detector_works() {
        // Two very different distributions should produce high KS stat.
        let baseline: Vec<f64> = vec![1.0; 40]; // all successes
        let current: Vec<f64> = vec![0.0; 40]; // all failures
        let detector = DriftDetector::new();
        let ks_stat = detector.ks_statistic(&baseline, &current);
        assert!(
            ks_stat > 0.1,
            "KS stat for completely different samples should be > 0.1, got {ks_stat}"
        );
        assert!(
            has_significant_drift(ks_stat, 0.05),
            "drift should be significant"
        );
    }

    #[test]
    fn test_drift_detector_identical() {
        // Identical samples should produce KS stat = 0.0 or very small.
        let sample: Vec<f64> = vec![1.0, 0.0, 1.0, 1.0, 0.0, 1.0, 0.0, 1.0, 1.0, 1.0];
        let detector = DriftDetector::new();
        let ks_stat = detector.ks_statistic(&sample, &sample);
        assert!(
            ks_stat < 0.01,
            "KS stat for identical samples should be ~0.0, got {ks_stat}"
        );
        assert!(
            !has_significant_drift(ks_stat, 0.05),
            "no drift expected for identical samples"
        );
    }
}
