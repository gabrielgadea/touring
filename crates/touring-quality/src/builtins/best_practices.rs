//! Best practices gate — stub (external CI: `clippy::pedantic` + cargo semver-checks).

use crate::change::Change;
use crate::gate::{Gate, GateId, GateOutcome, GateSeverity};

/// Gate delegating best-practices enforcement to external CI (`clippy::pedantic` + `cargo semver-checks`).
pub struct BestPracticesGate;

impl Gate for BestPracticesGate {
    fn id(&self) -> GateId {
        GateId::BestPractices
    }
    fn severity(&self) -> GateSeverity {
        GateSeverity::Warn
    }
    fn check(&self, _: &Change) -> GateOutcome {
        GateOutcome::external(GateId::BestPractices, GateSeverity::Warn).with_payload(
            serde_json::json!({
                "external": "clippy::pedantic lints + cargo semver-checks",
                "ci_step": "ci.yml:gates (advisory)",
            }),
        )
    }
}
