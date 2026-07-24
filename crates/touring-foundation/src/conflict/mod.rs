//! Multi-tier conflict detection for the Touring workspace.
//!
//! Provides three detector tiers with progressively deeper analysis:
//! - **AstDiff** (< 100ms SLA) — syntactic diff via AST comparison
//! - **Semantic** (< 1s SLA) — semantic conflict detection with type awareness
//! - **GraphImpact** (< 5s SLA) — call-graph impact analysis
//!
//! # Architecture
//!
//! [`ConflictTier`] selects the detector tier. Each tier implements
//! [`ConflictDetector`] and is wrapped with SLA tracking via the governor pattern.
//!
//! # Example
//!
//! ```ignore
//! use touring_foundation::conflict::{detect_conflict, ConflictTier};
//!
//! let report = detect_conflict("fn foo() {}", "fn foo() {}", ConflictTier::AstDiff);
//! assert!(!report.has_conflict);
//! ```

pub mod ast_diff;
pub mod graph_impact;
pub mod semantic;
pub mod sla;

pub use ast_diff::AstDiffDetector as AstDiff;
pub use graph_impact::GraphImpactDetector as GraphImpact;
pub use semantic::SemanticConflictDetector as Semantic;

use crate::conflict::sla::{SlaSpec, SlaViolation};
use serde::{Deserialize, Serialize};

/// Conflict detection tier — determines analysis depth and latency budget.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConflictTier {
    /// AST-level syntactic diff. SLA: < 100ms.
    AstDiff = 1,
    /// Semantic conflict detection with type inference. SLA: < 1s.
    Semantic = 2,
    /// Call-graph impact analysis. SLA: < 5s.
    GraphImpact = 3,
}

impl ConflictTier {
    /// Returns the SLA spec for this tier.
    pub fn sla_spec(self) -> SlaSpec {
        match self {
            ConflictTier::AstDiff => SlaSpec {
                p99_ms: 100,
                tier: self,
            },
            ConflictTier::Semantic => SlaSpec {
                p99_ms: 1_000,
                tier: self,
            },
            ConflictTier::GraphImpact => SlaSpec {
                p99_ms: 5_000,
                tier: self,
            },
        }
    }

    /// Parse from CLI integer (1, 2, or 3).
    pub fn from_u8(v: u8) -> Option<Self> {
        match v {
            1 => Some(ConflictTier::AstDiff),
            2 => Some(ConflictTier::Semantic),
            3 => Some(ConflictTier::GraphImpact),
            _ => None,
        }
    }
}

/// Core trait implemented by each conflict detector tier.
pub trait ConflictDetector: Send + Sync {
    /// Detect conflicts between two source snippets.
    fn detect(&self, a: &str, b: &str) -> ConflictReport;

    /// Human-readable name of this detector.
    fn name(&self) -> &'static str;
}

/// Result of a conflict detection run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictReport {
    /// Whether any conflict was detected.
    pub has_conflict: bool,
    /// Conflict tier that produced this report.
    pub tier: ConflictTier,
    /// Latency of the detection in milliseconds.
    pub latency_ms: u64,
    /// SLA spec that was applied.
    pub sla: SlaSpec,
    /// SLA violations, if any.
    pub sla_violations: Vec<SlaViolation>,
    /// Human-readable conflict description.
    pub description: String,
    /// Conflict locations (file/line if available).
    pub locations: Vec<ConflictLocation>,
    /// Severity: 0.0 = none, 1.0 = maximum.
    pub severity: f32,
}

/// Location of a detected conflict.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConflictLocation {
    /// Source identifier (e.g., file path or "input_a").
    pub source: String,
    /// Start line (1-indexed).
    pub line_start: usize,
    /// End line (1-indexed).
    pub line_end: usize,
}

/// Detect conflicts between two source snippets at the given tier.
pub fn detect_conflict(a: &str, b: &str, tier: ConflictTier) -> ConflictReport {
    match tier {
        ConflictTier::AstDiff => ast_diff::AstDiffDetector.detect(a, b),
        ConflictTier::Semantic => semantic::SemanticConflictDetector.detect(a, b),
        ConflictTier::GraphImpact => graph_impact::GraphImpactDetector.detect(a, b),
    }
}

/// Shared AST-diff detector instance (stateless, thread-safe).
/// Use as a process-wide singleton — no allocation cost.
pub static AST_DIFF_DETECTOR: ast_diff::AstDiffDetector = ast_diff::AstDiffDetector;
/// Shared semantic-conflict detector instance (stateless,
/// thread-safe). Process-wide singleton.
pub static SEMANTIC_DETECTOR: semantic::SemanticConflictDetector =
    semantic::SemanticConflictDetector;
/// Shared graph-impact detector instance (stateless,
/// thread-safe). Process-wide singleton.
pub static GRAPH_IMPACT_DETECTOR: graph_impact::GraphImpactDetector =
    graph_impact::GraphImpactDetector;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_conflict_tier_sla() {
        assert_eq!(ConflictTier::AstDiff.sla_spec().p99_ms, 100);
        assert_eq!(ConflictTier::Semantic.sla_spec().p99_ms, 1_000);
        assert_eq!(ConflictTier::GraphImpact.sla_spec().p99_ms, 5_000);
    }

    #[test]
    fn test_conflict_tier_from_u8() {
        assert_eq!(ConflictTier::from_u8(1), Some(ConflictTier::AstDiff));
        assert_eq!(ConflictTier::from_u8(2), Some(ConflictTier::Semantic));
        assert_eq!(ConflictTier::from_u8(3), Some(ConflictTier::GraphImpact));
        assert_eq!(ConflictTier::from_u8(99), None);
    }

    #[test]
    fn test_no_conflict_identical() {
        let r = detect_conflict("fn foo() {}", "fn foo() {}", ConflictTier::AstDiff);
        assert!(!r.has_conflict);
        assert_eq!(r.tier, ConflictTier::AstDiff);
        assert!(r.latency_ms < 100);
    }

    #[test]
    fn test_conflict_detected() {
        let r = detect_conflict("fn foo() {}", "fn bar() {}", ConflictTier::AstDiff);
        assert!(r.has_conflict);
        assert!(r.severity > 0.0);
    }
}
