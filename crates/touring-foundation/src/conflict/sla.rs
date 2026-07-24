//! SLA specification and violation tracking for conflict detection tiers.
//!
//! Follows the governor pattern from `touring-foundation/governor`.

use crate::conflict::ConflictTier;
use serde::{Deserialize, Serialize};
use std::time::{Duration, Instant};

/// SLA specification for a conflict detection tier.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SlaSpec {
    /// Target p99 latency in milliseconds.
    pub p99_ms: u64,
    /// The tier this spec applies to.
    pub tier: ConflictTier,
}

impl SlaSpec {
    /// Create a new SLA spec.
    pub fn new(p99_ms: u64, tier: ConflictTier) -> Self {
        Self { p99_ms, tier }
    }

    /// Check if actual latency violates this SLA.
    pub fn is_violated(&self, actual_ms: u64) -> bool {
        actual_ms > self.p99_ms
    }

    /// Return violations given actual latency.
    pub fn check_violation(&self, actual_ms: u64) -> Vec<SlaViolation> {
        if self.is_violated(actual_ms) {
            vec![SlaViolation {
                tier: self.tier,
                sla_ms: self.p99_ms,
                actual_ms,
                overage_ms: actual_ms.saturating_sub(self.p99_ms),
            }]
        } else {
            vec![]
        }
    }

    /// Duration equivalent of p99_ms.
    pub fn duration(&self) -> Duration {
        Duration::from_millis(self.p99_ms)
    }
}

/// An SLA violation record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SlaViolation {
    /// Tier whose SLA was violated (AstDiff / Semantic / GraphImpact).
    pub tier: ConflictTier,
    /// Configured p99 budget that was breached, in milliseconds.
    pub sla_ms: u64,
    /// Observed latency in milliseconds.
    pub actual_ms: u64,
    /// How much the observation exceeded the budget
    /// (`actual - sla`, saturating at zero). Useful for SLO
    /// dashboards.
    pub overage_ms: u64,
}

impl SlaViolation {
    /// Human-readable description.
    pub fn description(&self) -> String {
        format!(
            "SLA violation: {} tier exceeded {}ms (actual: {}ms, +{}ms over)",
            match self.tier {
                ConflictTier::AstDiff => "AstDiff",
                ConflictTier::Semantic => "Semantic",
                ConflictTier::GraphImpact => "GraphImpact",
            },
            self.sla_ms,
            self.actual_ms,
            self.overage_ms
        )
    }
}

/// SLA tracker that records start time and checks violations.
#[derive(Debug)]
pub struct SlaTracker {
    start: Instant,
    #[allow(dead_code)]
    tier: ConflictTier,
}

impl SlaTracker {
    /// Start tracking for the given tier.
    pub fn start(tier: ConflictTier) -> Self {
        Self {
            start: Instant::now(),
            tier,
        }
    }

    /// Finish tracking and return measured latency in ms plus violations.
    pub fn finish(&self, sla: SlaSpec) -> (u64, Vec<SlaViolation>) {
        let elapsed = self.start.elapsed().as_millis() as u64;
        (elapsed, sla.check_violation(elapsed))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sla_violation_detected() {
        let sla = SlaSpec::new(100, ConflictTier::AstDiff);
        assert!(sla.is_violated(150));
        assert!(!sla.is_violated(50));
    }

    #[test]
    fn test_sla_violation_record() {
        let sla = SlaSpec::new(100, ConflictTier::AstDiff);
        let violations = sla.check_violation(200);
        assert_eq!(violations.len(), 1);
        assert_eq!(violations[0].overage_ms, 100);
    }

    #[test]
    fn test_sla_tracker() {
        let tracker = SlaTracker::start(ConflictTier::AstDiff);
        std::thread::sleep(Duration::from_millis(10));
        let sla = SlaSpec::new(100, ConflictTier::AstDiff);
        let (ms, violations) = tracker.finish(sla);
        assert!(ms >= 10);
        assert!(violations.is_empty()); // 10ms is well under 100ms
    }

    #[test]
    fn test_sla_violation_description() {
        let sla = SlaSpec::new(100, ConflictTier::AstDiff);
        let violations = sla.check_violation(200);
        let desc = violations[0].description();
        assert!(desc.contains("AstDiff"));
        assert!(desc.contains("200ms"));
    }
}
