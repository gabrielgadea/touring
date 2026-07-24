//! Extensibility gate — stub (advisory, fail-open).

use crate::change::Change;
use crate::gate::{Gate, GateId, GateOutcome, GateSeverity};

/// Advisory, fail-open gate delegating extensibility scanning to external CI.
pub struct ExtensibilityGate;

impl Gate for ExtensibilityGate {
    fn id(&self) -> GateId {
        GateId::Extensibility
    }
    fn severity(&self) -> GateSeverity {
        GateSeverity::Warn
    }
    fn check(&self, _: &Change) -> GateOutcome {
        GateOutcome::external(GateId::Extensibility, GateSeverity::Warn)
            .with_payload(serde_json::json!({
                "external": "docs/extensibility_scan.py --check (Rust-native equivalent: AST-grep for kitchen-sink string-dispatch matches)",
            }))
    }
}
