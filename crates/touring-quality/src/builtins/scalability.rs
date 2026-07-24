//! Scalability gate — stub (advisory, fail-open).

use crate::change::Change;
use crate::gate::{Gate, GateId, GateOutcome, GateSeverity};

/// Advisory, fail-open gate delegating scalability scanning to external CI.
pub struct ScalabilityGate;

impl Gate for ScalabilityGate {
    fn id(&self) -> GateId {
        GateId::Scalability
    }
    fn severity(&self) -> GateSeverity {
        GateSeverity::Warn
    }
    fn check(&self, _: &Change) -> GateOutcome {
        GateOutcome::external(GateId::Scalability, GateSeverity::Warn)
            .with_payload(serde_json::json!({
                "external": "docs/scalability_scan.py --check (Rust-native equivalent: AST-grep for `static mut` / non-Sync statics)",
            }))
    }
}
