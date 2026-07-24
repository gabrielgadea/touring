//! Parallel harness runner — the 17-gate engine.
//!
//! `run_harness(change, gates, config)` runs every gate in parallel via
//! `rayon`, then aggregates per-gate outcomes into an [`EliteScore`].
//!
//! The runner is **fail-open** by default (per-gate `check()` that returns
//! an Err or panics is caught and recorded as `GateStatus::Advisory`,
//! score 0.5). Set `HarnessConfig::fail_open = false` to BLOCK on any
//! internal gate error.

use std::collections::HashMap;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::time::Instant;

use rayon::prelude::*;

use crate::change::Change;
use crate::gate::{Gate, GateId, GateOutcome, GateStatus};
use crate::score::EliteScore;

/// Runtime configuration for the harness.
#[derive(Debug, Clone)]
pub struct HarnessConfig {
    /// Per-gate weight overrides. Missing keys fall back to `GateId::default_weight`.
    pub weights: HashMap<GateId, f32>,
    /// Whether to run gates in parallel via rayon (default: true).
    pub parallel: bool,
    /// Whether to catch panics in gates and treat them as `Advisory` (default: true).
    pub fail_open: bool,
}

impl Default for HarnessConfig {
    fn default() -> Self {
        let mut weights = HashMap::new();
        for id in GateId::ALL {
            weights.insert(*id, id.default_weight());
        }
        Self {
            weights,
            parallel: true,
            fail_open: true,
        }
    }
}

impl HarnessConfig {
    /// Override the weight for a specific gate.
    #[must_use]
    pub fn with_weight(mut self, gate_id: GateId, weight: f32) -> Self {
        self.weights.insert(gate_id, weight);
        self
    }

    /// Disable parallel execution (for testing).
    #[must_use]
    pub fn sequential(mut self) -> Self {
        self.parallel = false;
        self
    }

    /// Set fail-open behavior.
    #[must_use]
    pub fn with_fail_open(mut self, fail_open: bool) -> Self {
        self.fail_open = fail_open;
        self
    }
}

/// Run every gate in `gates` against `change` and aggregate into an
/// `EliteScore`.
///
/// Gates are executed in parallel by default; the `parallel` flag on
/// `HarnessConfig` switches to sequential. Each gate's `check()` is
/// wrapped in `catch_unwind` (if `fail_open = true`) to prevent one
/// panicking gate from killing the whole harness.
#[must_use]
pub fn run_harness(change: &Change, gates: &[Box<dyn Gate>], config: &HarnessConfig) -> EliteScore {
    let run_one = |gate: &dyn Gate| -> GateOutcome {
        let id = gate.id();
        let severity = gate.severity();
        let weight = config
            .weights
            .get(&id)
            .copied()
            .unwrap_or_else(|| id.default_weight());
        let start = Instant::now();
        let result = if config.fail_open {
            catch_unwind(AssertUnwindSafe(|| gate.check(change)))
        } else {
            Ok(gate.check(change))
        };
        let elapsed_ms = u64::try_from(start.elapsed().as_millis()).unwrap_or(u64::MAX);
        match result {
            Ok(mut outcome) => {
                outcome.weight = weight;
                outcome.elapsed_ms = elapsed_ms;
                outcome
            }
            Err(_) => GateOutcome {
                gate_id: id,
                severity,
                status: GateStatus::Advisory,
                score: 0.5,
                weight,
                message: format!("gate {} panicked (fail-open)", id.slug()),
                payload: serde_json::Value::Null,
                evidence: Vec::new(),
                elapsed_ms,
            },
        }
    };

    let outcomes = if config.parallel {
        gates
            .par_iter()
            .map(|gate| run_one(gate.as_ref()))
            .collect()
    } else {
        gates.iter().map(|gate| run_one(gate.as_ref())).collect()
    };

    EliteScore::from_gates(outcomes)
}

/// Convenience: compute the harness result AND return block-status.
///
/// Returns `true` if the change should be `BLOCKed` (tier < Gold OR any
/// BLOCK-severity gate failed).
#[must_use]
pub fn should_block(
    change: &Change,
    gates: &[Box<dyn Gate>],
    config: &HarnessConfig,
) -> (EliteScore, bool) {
    let score = run_harness(change, gates, config);
    let block = !score.tier.is_release_ready() || !score.block_reasons().is_empty();
    (score, block)
}

/// Test-only convenience extension: `true` when the score's tier is not
/// release-ready (mirrors the block half of [`should_block`]).
#[cfg(test)]
trait ShouldBlockViaLib {
    /// Returns `true` when [`EliteScore::tier`] is below release-ready.
    fn should_block_via_lib(&self) -> bool;
}
#[cfg(test)]
impl ShouldBlockViaLib for crate::score::EliteScore {
    fn should_block_via_lib(&self) -> bool {
        !self.tier.is_release_ready()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::change::ProposedFile;
    use crate::gate::{Gate, GateOutcome, GateSeverity};

    struct AlwaysPass;
    impl Gate for AlwaysPass {
        fn id(&self) -> GateId {
            GateId::CodeQuality
        }
        fn severity(&self) -> GateSeverity {
            GateSeverity::Block
        }
        fn check(&self, _: &Change) -> GateOutcome {
            GateOutcome::pass(GateId::CodeQuality, GateSeverity::Block)
        }
    }

    struct AlwaysFail;
    impl Gate for AlwaysFail {
        fn id(&self) -> GateId {
            GateId::Security
        }
        fn severity(&self) -> GateSeverity {
            GateSeverity::Block
        }
        fn check(&self, _: &Change) -> GateOutcome {
            GateOutcome::fail(GateId::Security, GateSeverity::Block, "denied")
        }
    }

    #[test]
    fn simple_pass() {
        let change = Change::new().with_file(ProposedFile::create("src/lib.rs", "// new"));
        let gates: Vec<Box<dyn Gate>> = vec![Box::new(AlwaysPass)];
        let s = run_harness(&change, &gates, &HarnessConfig::default());
        assert_eq!(s.tier, crate::score::EliteTier::Diamond);
        assert!(!s.should_block_via_lib());
    }

    #[test]
    fn simple_block() {
        let change = Change::new().with_file(ProposedFile::create("src/lib.rs", "// new"));
        let gates: Vec<Box<dyn Gate>> = vec![Box::new(AlwaysPass), Box::new(AlwaysFail)];
        let s = run_harness(&change, &gates, &HarnessConfig::default());
        assert!(!s.tier.is_release_ready());
        assert_eq!(s.block_reasons().len(), 1);
    }

    #[test]
    fn panicking_gate_is_advisory_when_fail_open() {
        struct Panic;
        impl Gate for Panic {
            fn id(&self) -> GateId {
                GateId::Performance
            }
            fn severity(&self) -> GateSeverity {
                GateSeverity::Advisory
            }
            fn check(&self, _: &Change) -> GateOutcome {
                panic!("intentional")
            }
        }
        let change = Change::new();
        let gates: Vec<Box<dyn Gate>> = vec![Box::new(Panic)];
        let s = run_harness(&change, &gates, &HarnessConfig::default());
        assert_eq!(s.gates.len(), 1);
        assert_eq!(s.gates[0].status, GateStatus::Advisory);
    }
}
