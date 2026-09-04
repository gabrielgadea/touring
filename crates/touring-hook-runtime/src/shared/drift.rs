//! Shared temporal drift detection for hook handlers.
//!
//! Complementação-hooks H14: enrich() signal layer that detects drift in
//! the workspace between the last session checkpoint and the current state.
//! Emits scored signals consumed by session_start handlers.
//!
//! Latency budget: <5ms p95 (Rust direct, no subprocess).
//!
//! MVP: file-count delta between sessions. Future iterations can extend
//! to schema-version drift, KPI regression, etc.

use touring_hooks_shared::signal_layer::{SignalContext, SignalLayer};

/// Drift tier reported to the LLM.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriftTier {
    /// No drift detected between sessions.
    None,
    /// Low drift — a few files changed since last session.
    Low,
    /// Medium drift — significant changes warrant attention.
    Medium,
    /// High drift — substantial structural changes occurred.
    High,
}

/// Drift report returned by [`detect_drift`].
#[derive(Debug, Clone)]
pub struct DriftReport {
    /// Tier classification.
    pub tier: DriftTier,
    /// Number of files changed since the last session checkpoint.
    pub files_changed: usize,
    /// Number of files added since the last session checkpoint.
    pub files_added: usize,
    /// Number of files removed since the last session checkpoint.
    pub files_removed: usize,
}

/// Detect drift between two file-count snapshots.
///
/// MVP signature — accepts explicit counts to keep the detector pure and
/// trivially testable. The caller (s `session_hooks.rs`) is responsible for
/// sourcing the snapshot via `touring-cli::evolution drift` and passing the
/// numbers here.
pub fn detect_drift(
    prev_files_changed: usize,
    prev_files_added: usize,
    prev_files_removed: usize,
    curr_files_changed: usize,
    curr_files_added: usize,
    curr_files_removed: usize,
) -> DriftReport {
    let delta_changed = curr_files_changed.saturating_sub(prev_files_changed);
    let delta_added = curr_files_added.saturating_sub(prev_files_added);
    let delta_removed = curr_files_removed.saturating_sub(prev_files_removed);

    let tier = match (delta_changed, delta_added, delta_removed) {
        (0, 0, 0) => DriftTier::None,
        (d, _, _) if d <= 5 => DriftTier::Low,
        (d, _, _) if d <= 20 => DriftTier::Medium,
        _ => DriftTier::High,
    };

    DriftReport {
        tier,
        files_changed: delta_changed,
        files_added: delta_added,
        files_removed: delta_removed,
    }
}

/// SignalLayer that emits a single temporal-drift signal.
///
/// Layer name: `temporal_drift`. Score weight: 0.0 (None) | 0.2 (Low) |
/// 0.5 (Medium) | 1.0 (High).
pub struct TemporalDriftLayer;

impl SignalLayer for TemporalDriftLayer {
    fn name(&self) -> &'static str {
        "temporal_drift"
    }

    fn enrich(&self, ctx: &SignalContext<'_>) -> Vec<(f32, String)> {
        // MVP: parse two comma-separated numbers from `ctx.source` (format:
        // "prev_changed,curr_changed"). Real wiring will source snapshots
        // from `touring-cli evolution drift --json` before constructing the
        // SignalContext — for now, the layer demonstrates the trait wiring
        // without depending on the daemon.
        let report = parse_snapshot_from_source(ctx.source);
        let score = match report.tier {
            DriftTier::None => 0.0,
            DriftTier::Low => 0.2,
            DriftTier::Medium => 0.5,
            DriftTier::High => 1.0,
        };
        if score == 0.0 {
            return Vec::new();
        }
        let tier_str = match report.tier {
            DriftTier::None => "none",
            DriftTier::Low => "low",
            DriftTier::Medium => "medium",
            DriftTier::High => "high",
        };
        vec![(
            score,
            format!(
                "[temporal_drift] tier={tier_str} changed={} added={} removed={}",
                report.files_changed, report.files_added, report.files_removed
            ),
        )]
    }

    fn should_run(&self, _cila_level: usize) -> bool {
        // Drift layer always runs on session-start regardless of CILA.
        true
    }
}

/// Parse "prev_changed,prev_added,prev_removed,curr_changed,curr_added,curr_removed"
/// from a comma-separated string. Missing fields default to 0.
fn parse_snapshot_from_source(source: &str) -> DriftReport {
    let nums: Vec<usize> = source
        .split(',')
        .filter_map(|s| s.trim().parse::<usize>().ok())
        .collect();
    if nums.len() < 6 {
        return DriftReport {
            tier: DriftTier::None,
            files_changed: 0,
            files_added: 0,
            files_removed: 0,
        };
    }
    detect_drift(
        nums[0], nums[1], nums[2], nums[3], nums[4], nums[5],
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_drift_returns_none_tier() {
        let r = detect_drift(0, 0, 0, 0, 0, 0);
        assert_eq!(r.tier, DriftTier::None);
    }

    #[test]
    fn low_drift_under_5_changes() {
        let r = detect_drift(0, 0, 0, 3, 1, 0);
        assert_eq!(r.tier, DriftTier::Low);
    }

    #[test]
    fn medium_drift_under_20() {
        let r = detect_drift(0, 0, 0, 15, 5, 1);
        assert_eq!(r.tier, DriftTier::Medium);
    }

    #[test]
    fn high_drift_over_20() {
        let r = detect_drift(0, 0, 0, 50, 10, 2);
        assert_eq!(r.tier, DriftTier::High);
    }

    #[test]
    fn parses_snapshot_from_source() {
        let r = parse_snapshot_from_source("0,0,0,3,1,0");
        assert_eq!(r.tier, DriftTier::Low);
        assert_eq!(r.files_changed, 3);
        assert_eq!(r.files_added, 1);
    }

    #[test]
    fn malformed_source_returns_none() {
        let r = parse_snapshot_from_source("not_a_number");
        assert_eq!(r.tier, DriftTier::None);
    }

    #[test]
    fn signal_layer_emits_high_drift() {
        let ctx = SignalContext::new("session", "0,0,0,50,10,2");
        let signals = TemporalDriftLayer.enrich(&ctx);
        assert!(!signals.is_empty());
        assert!(signals.iter().any(|(s, _)| *s == 1.0));
    }

    #[test]
    fn signal_layer_skips_no_drift() {
        let ctx = SignalContext::new("session", "0,0,0,0,0,0");
        let signals = TemporalDriftLayer.enrich(&ctx);
        assert!(signals.is_empty());
    }
}