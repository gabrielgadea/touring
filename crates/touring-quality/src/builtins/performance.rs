//! Performance gate — stub (advisory, fail-open until `cargo bench` baseline exists).

use crate::change::Change;
use crate::gate::{Gate, GateId, GateOutcome, GateSeverity};

/// Advisory, fail-open gate for performance regressions until a `cargo bench` baseline exists.
pub struct PerformanceGate;

impl Gate for PerformanceGate {
    fn id(&self) -> GateId {
        GateId::Performance
    }
    fn severity(&self) -> GateSeverity {
        GateSeverity::Advisory
    }
    fn check(&self, _: &Change) -> GateOutcome {
        GateOutcome::advisory(GateId::Performance, GateSeverity::Advisory, "performance stub — no P99 baseline; run `cargo bench` to populate docs/baselines/benchmarks.json")
            .with_payload(serde_json::json!({
                "baseline_path": "docs/baselines/benchmarks.json",
                "advisory_until": "first cargo bench run",
            }))
    }
}
