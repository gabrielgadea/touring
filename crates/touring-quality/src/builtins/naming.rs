//! Naming gate — stub (clippy lints cover C-CASE/C-CONV; type-driven is advisory).

use crate::change::Change;
use crate::gate::{Gate, GateId, GateOutcome, GateSeverity};

/// Blocking gate enforcing API-guideline naming conventions (C-CASE/C-CONV via clippy defaults).
pub struct NamingGate;

impl Gate for NamingGate {
    fn id(&self) -> GateId {
        GateId::Naming
    }
    fn severity(&self) -> GateSeverity {
        GateSeverity::Block
    }
    fn check(&self, _: &Change) -> GateOutcome {
        GateOutcome::external(GateId::Naming, GateSeverity::Block)
            .with_payload(serde_json::json!({
                "external": "rust-api-guidelines C-CASE/C-CONV via clippy default + advisory: type-driven (no bools) + acronym casing (URL/Url)",
            }))
    }
}
