//! Stage **X7.5 QUALITY-SIGNAL** of the Code Execution Gateway.
//!
//! Carries the 50-dimension composite score from `touring-quality` into the
//! X7 DECISION fusion so a `touring_elite_check` against a proposed code
//! change modulates the final safety verdict (Plan v3 §W4, Q3 — add quality
//! signal at `W_QUALITY = 0.20`).
//!
//! ## Multiplicative penalty model
//!
//! The existing X7 composite has 5 additive weights summing to `1.0`
//! (`W_STATIC` + `W_VGP` + `W_PREDICT` + `W_SANDBOX` + `W_GATE`). To avoid
//! rebalancing all five and breaking the in-flight signal contract, the
//! quality signal enters as a **multiplicative penalty** in `[0.80, 1.00]`:
//!
//! ```text
//! composite_with_quality = composite * quality_penalty_factor
//!   where quality_penalty_factor = 1.0 - W_QUALITY * (1.0 - quality_subscore)
//! ```
//!
//! At `quality_subscore = 1.0` (perfect) → factor = `1.0` (no penalty).
//! At `quality_subscore = 0.0` (worst)   → factor = `0.80` (20 % penalty).
//! When the report is `None` (synthetic / not yet scored) → factor = `1.0`
//! (neutral — nothing to penalize).
//!
//! ## Fail-open
//!
//! Mirrors the harness-extension policy: the quality signal is **optional**
//! and the CEG must continue to gate Edit/Write even when `touring-quality`
//! is not wired into the call site (e.g. fast-path Pure-skip, fast-path Edit,
//! remote bash). `None` → neutral `1.0`, never blocks.

use serde::{Deserialize, Serialize};

/// The condensed quality signal attached to `Evidence`.
///
/// Constructed by the optional `touring_elite_check`-equivalent integration
/// (W6 T1: `touring-cortex::handlers::quality::PostQualityGate`); consumed
/// by `decision::composite_score` via the X7.5 step.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct QualitySignalReport {
    /// Composite 50-dim score in `0.0..=1.0`. `1.0` = perfect, `0.0` = worst.
    pub composite: f32,
    /// Optional tier label (`"Diamond"` / `"Platinum"` / `"Gold"` /
    /// `"Silver"` / `"Bronze"` / `"Unranked"`). Logged for audit; the
    /// decision uses only `composite`.
    pub tier: Option<String>,
    /// Number of 50-dim BLOCK violations (P0) in the scored target. Drives
    /// the optional telemetry hook; the decision still uses `composite`.
    pub blocker_count: u32,
}

impl QualitySignalReport {
    /// Quality score in `0.0..=1.0` (clamp), used as the X7.5 sub-score.
    #[must_use]
    pub fn score(&self) -> f64 {
        // Clamp into the canonical 0..=1 range; Defensive against callers
        // that produced a NaN / negative / > 1 score.
        let c = if self.composite.is_finite() {
            self.composite as f64
        } else {
            0.0
        };
        c.clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn perfect_score_is_one() {
        let r = QualitySignalReport {
            composite: 1.0,
            tier: Some("Diamond".into()),
            blocker_count: 0,
        };
        assert!((r.score() - 1.0).abs() < 1e-6);
    }

    #[test]
    fn worst_score_is_zero() {
        let r = QualitySignalReport {
            composite: 0.0,
            tier: Some("Unranked".into()),
            blocker_count: 6,
        };
        assert!((r.score() - 0.0).abs() < 1e-6);
    }

    #[test]
    fn nan_composite_is_clamped_to_zero() {
        let r = QualitySignalReport {
            composite: f32::NAN,
            tier: None,
            blocker_count: 0,
        };
        assert_eq!(r.score(), 0.0);
    }

    #[test]
    fn out_of_range_clamped() {
        let r = QualitySignalReport {
            composite: 1.5,
            tier: None,
            blocker_count: 0,
        };
        assert_eq!(r.score(), 1.0);
    }
}
