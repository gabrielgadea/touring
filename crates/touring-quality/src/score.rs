//! Composite `EliteScore` aggregator and 6-tier mapping.
//!
//! The 17-gate composite score is the **single number** that release
//! readiness pivots on. It is the **only signal** the CEG X7 BLOCK-by-default
//! gate consults, the **only number** `touring elite badge` emits, and the
//! **only number** other LLMs receive via the `touring_elite_check` MCP tool.
//!
//! ## Formula
//!
//! ```text
//! composite = Σ (weight_i × score_i) / Σ weight_i
//! ```
//!
//! where `score_i` is the per-gate score (0.0-1.0) and `weight_i` is the
//! effective weight (default from `GateId::default_weight`, overridable
//! via `HarnessConfig::weights`).
//!
//! ## Tiers
//!
//! | Tier | Range | Glyph | Release-ready? |
//! |------|-------|-------|----------------|
//! | `Diamond`  | 0.95-1.00 | 💎 | ✓ Yes — Premium |
//! | `Platinum` | 0.90-0.94 | ★  | ✓ Yes — Elite de Mercado |
//! | `Gold`     | 0.80-0.89 | ✓  | ✓ Yes — production-grade |
//! | `Silver`   | 0.70-0.79 | ⚠  | ⚠ Human review required |
//! | `Bronze`   | 0.60-0.69 | ⚠  | ✗ Refactor before merging |
//! | `Unranked` | < 0.60   | 🚫 | ✗ BLOCK |

use serde::{Deserialize, Serialize};

use crate::gate::{GateId, GateOutcome, GateSeverity, GateStatus};

/// The 6 elite tiers.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum EliteTier {
    /// Composite in 0.95-1.00 — Premium, release-ready.
    Diamond,
    /// Composite in 0.90-0.94 — Elite de Mercado, release-ready.
    Platinum,
    /// Composite in 0.80-0.89 — production-grade, release-ready.
    Gold,
    /// Composite in 0.70-0.79 — human review required before release.
    Silver,
    /// Composite in 0.60-0.69 — refactor before merging.
    Bronze,
    /// Composite below 0.60 — BLOCK by default.
    Unranked,
}

impl EliteTier {
    /// Glyph used in badges.
    #[must_use]
    pub const fn glyph(self) -> &'static str {
        match self {
            Self::Diamond => "💎",
            Self::Platinum => "★",
            Self::Gold => "✓",
            Self::Silver | Self::Bronze => "⚠",
            Self::Unranked => "🚫",
        }
    }

    /// Human label.
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Diamond => "DIAMOND",
            Self::Platinum => "PLATINUM",
            Self::Gold => "GOLD",
            Self::Silver => "SILVER",
            Self::Bronze => "BRONZE",
            Self::Unranked => "UNRANKED",
        }
    }

    /// Whether releases are auto-allowed at this tier.
    ///
    /// - `Diamond` / `Platinum` / `Gold` → release-ready (auto).
    /// - `Silver` / `Bronze` / `Unranked` → BLOCK by default.
    #[must_use]
    pub const fn is_release_ready(self) -> bool {
        matches!(self, Self::Diamond | Self::Platinum | Self::Gold)
    }
}

impl std::fmt::Display for EliteTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} {}", self.glyph(), self.label())
    }
}

/// Map a composite score to its tier.
#[must_use]
pub fn tier_for(score: f32) -> EliteTier {
    if score >= 0.95 {
        EliteTier::Diamond
    } else if score >= 0.90 {
        EliteTier::Platinum
    } else if score >= 0.80 {
        EliteTier::Gold
    } else if score >= 0.70 {
        EliteTier::Silver
    } else if score >= 0.60 {
        EliteTier::Bronze
    } else {
        EliteTier::Unranked
    }
}

/// Composite score with full breakdown.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EliteScore {
    /// Weighted average in [0.0, 1.0].
    pub composite: f32,
    /// Tier derived from `composite`.
    pub tier: EliteTier,
    /// Per-gate outcomes (one per gate run, in canonical order).
    pub gates: Vec<GateOutcome>,
    /// Sum of effective weights.
    pub total_weight: f32,
    /// Sum of (`weight_i` × `score_i`).
    pub weighted_sum: f32,
}

impl EliteScore {
    /// Compute composite from a list of gate outcomes.
    #[must_use]
    pub fn from_gates(gates: Vec<GateOutcome>) -> Self {
        let mut weighted_sum = 0.0_f32;
        let mut total_weight = 0.0_f32;
        for g in &gates {
            weighted_sum += g.weight * g.score;
            total_weight += g.weight;
        }
        let composite = if total_weight > 0.0 {
            weighted_sum / total_weight
        } else {
            0.0
        };
        let tier = tier_for(composite);
        Self {
            composite,
            tier,
            gates,
            total_weight,
            weighted_sum,
        }
    }

    /// All BLOCK-severity gates that failed.
    #[must_use]
    pub fn block_reasons(&self) -> Vec<&GateOutcome> {
        self.gates
            .iter()
            .filter(|g| g.severity == GateSeverity::Block && g.status == GateStatus::Fail)
            .collect()
    }

    /// All gates that warned (status=Warn regardless of severity).
    #[must_use]
    pub fn warn_reasons(&self) -> Vec<&GateOutcome> {
        self.gates
            .iter()
            .filter(|g| g.status == GateStatus::Warn)
            .collect()
    }

    /// Whether the score is at the Gold tier or above (release-ready).
    #[must_use]
    pub fn is_release_ready(&self) -> bool {
        self.tier.is_release_ready()
    }

    /// ASCII badge: `💎 DIAMOND (composite=0.97)`.
    #[must_use]
    pub fn badge(&self) -> String {
        format!(
            "{} {} (composite={:.4})",
            self.tier.glyph(),
            self.tier.label(),
            self.composite
        )
    }

    /// Whether a specific gate is in the breakdown.
    #[must_use]
    pub fn has_gate(&self, id: GateId) -> bool {
        self.gates.iter().any(|g| g.gate_id == id)
    }

    /// Get the outcome of a specific gate (if present).
    #[must_use]
    pub fn outcome(&self, id: GateId) -> Option<&GateOutcome> {
        self.gates.iter().find(|g| g.gate_id == id)
    }
}

impl std::fmt::Display for EliteScore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(
            f,
            "EliteScore composite={:.4} tier={}",
            self.composite, self.tier
        )?;
        for g in &self.gates {
            let status = match g.status {
                GateStatus::Pass => "✓",
                GateStatus::Warn => "⚠",
                GateStatus::Advisory => "○",
                GateStatus::Fail => "✗",
                GateStatus::External => "·",
                GateStatus::Missing => "?",
            };
            writeln!(
                f,
                "  {} {:<20} score={:.2} weight={:.2} {}",
                status, g.gate_id, g.score, g.weight, g.message
            )?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gate::{GateOutcome, GateSeverity};

    #[test]
    fn tier_for_canonical_thresholds() {
        assert_eq!(tier_for(0.99), EliteTier::Diamond);
        assert_eq!(tier_for(0.95), EliteTier::Diamond);
        assert_eq!(tier_for(0.949), EliteTier::Platinum);
        assert_eq!(tier_for(0.90), EliteTier::Platinum);
        assert_eq!(tier_for(0.899), EliteTier::Gold);
        assert_eq!(tier_for(0.80), EliteTier::Gold);
        assert_eq!(tier_for(0.799), EliteTier::Silver);
        assert_eq!(tier_for(0.70), EliteTier::Silver);
        assert_eq!(tier_for(0.699), EliteTier::Bronze);
        assert_eq!(tier_for(0.60), EliteTier::Bronze);
        assert_eq!(tier_for(0.599), EliteTier::Unranked);
        assert_eq!(tier_for(0.0), EliteTier::Unranked);
    }

    #[test]
    fn release_ready_only_top3() {
        assert!(EliteTier::Diamond.is_release_ready());
        assert!(EliteTier::Platinum.is_release_ready());
        assert!(EliteTier::Gold.is_release_ready());
        assert!(!EliteTier::Silver.is_release_ready());
        assert!(!EliteTier::Bronze.is_release_ready());
        assert!(!EliteTier::Unranked.is_release_ready());
    }

    #[test]
    fn from_gates_simple() {
        let gates = vec![
            GateOutcome::pass(GateId::Architecture, GateSeverity::Block),
            GateOutcome::pass(GateId::Security, GateSeverity::Block),
            GateOutcome::fail(GateId::Performance, GateSeverity::Advisory, "no bench"),
        ];
        let s = EliteScore::from_gates(gates);
        // weight: arch=1.0, sec=1.5, perf=0.7; total=3.2
        // sum: 1.0*1.0 + 1.5*1.0 + 0.7*0.0 = 2.5
        // composite = 2.5/3.2 = 0.781
        assert!((s.composite - 2.5 / 3.2).abs() < 1e-4);
        assert_eq!(s.tier, EliteTier::Silver);
        assert_eq!(s.gates.len(), 3);
    }

    #[test]
    fn badge_format() {
        let s = EliteScore {
            composite: 0.97,
            tier: EliteTier::Diamond,
            gates: vec![],
            total_weight: 1.0,
            weighted_sum: 0.97,
        };
        assert_eq!(s.badge(), "💎 DIAMOND (composite=0.9700)");
    }

    #[test]
    fn block_reasons_filters_correctly() {
        let gates = vec![
            GateOutcome::pass(GateId::Architecture, GateSeverity::Block),
            GateOutcome::fail(GateId::Security, GateSeverity::Block, "advisory"),
            GateOutcome::warn(GateId::Testing, GateSeverity::Block, "low coverage"),
            GateOutcome::fail(GateId::Performance, GateSeverity::Advisory, "no bench"),
        ];
        let s = EliteScore::from_gates(gates);
        assert_eq!(s.block_reasons().len(), 1); // only Security (Block+Fail)
        assert_eq!(s.block_reasons()[0].gate_id, GateId::Security);
        assert_eq!(s.warn_reasons().len(), 1); // only Testing (Warn)
    }
}
