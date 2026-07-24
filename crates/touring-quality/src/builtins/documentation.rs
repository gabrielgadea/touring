//! Documentation gate — stub (external CI: rustdoc -D warnings + `missing_docs`).

use crate::change::Change;
use crate::gate::{Gate, GateId, GateOutcome, GateSeverity};

/// Blocking gate delegating documentation enforcement to external CI (`rustdoc -D warnings` + `missing_docs`).
pub struct DocumentationGate;

impl Gate for DocumentationGate {
    fn id(&self) -> GateId {
        GateId::Documentation
    }
    fn severity(&self) -> GateSeverity {
        GateSeverity::Block
    }
    fn check(&self, _: &Change) -> GateOutcome {
        GateOutcome::external(GateId::Documentation, GateSeverity::Block).with_payload(
            serde_json::json!({
                "external": "RUSTDOCFLAGS=-D warnings cargo doc + missing-docs gate",
                "drift_check": "docs/sync_metrics.py --check",
            }),
        )
    }
}
