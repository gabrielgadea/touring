//! Testing gate — stub (external CI step: `cargo test --workspace --lib`).

use crate::change::Change;
use crate::gate::{Gate, GateId, GateOutcome, GateSeverity};

/// Blocking gate delegating test execution to external CI (`cargo test --workspace --lib`).
pub struct TestingGate;

impl Gate for TestingGate {
    fn id(&self) -> GateId {
        GateId::Testing
    }
    fn severity(&self) -> GateSeverity {
        GateSeverity::Block
    }
    fn check(&self, _: &Change) -> GateOutcome {
        GateOutcome::external(GateId::Testing, GateSeverity::Block).with_payload(
            serde_json::json!({
                "external": "cargo test --workspace --lib + cargo mutants --in-diff",
                "ci_job": "ci.yml:test",
            }),
        )
    }
}
