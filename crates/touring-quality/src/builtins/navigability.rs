//! Navigability gate — stub (cognitive metrics + LSP coverage).

use crate::change::Change;
use crate::gate::{Gate, GateId, GateOutcome, GateSeverity};

/// Blocking gate asserting code navigability via cognitive metrics and LSP coverage.
pub struct NavigabilityGate;

impl Gate for NavigabilityGate {
    fn id(&self) -> GateId {
        GateId::Navigability
    }
    fn severity(&self) -> GateSeverity {
        GateSeverity::Block
    }
    fn check(&self, _: &Change) -> GateOutcome {
        GateOutcome::external(GateId::Navigability, GateSeverity::Block)
            .with_payload(serde_json::json!({
                "external": "touring cognitive metrics (has_graph=true) + touring ast find coverage 100% + LSP clean",
            }))
    }
}
