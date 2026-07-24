//! Dependencies gate — stub (external CI: cargo-deny binding, SEC-03 2026-06-13).

use crate::change::Change;
use crate::gate::{Gate, GateId, GateOutcome, GateSeverity};

/// Blocking gate delegating supply-chain checks to external CI (`cargo-deny` via `deny.toml`).
pub struct DependenciesGate;

impl Gate for DependenciesGate {
    fn id(&self) -> GateId {
        GateId::Dependencies
    }
    fn severity(&self) -> GateSeverity {
        GateSeverity::Block
    }
    fn check(&self, _: &Change) -> GateOutcome {
        GateOutcome::external(GateId::Dependencies, GateSeverity::Block)
            .with_payload(serde_json::json!({
                "external": "deny.toml binding in ci.yml:supply-chain (advisories, bans, licenses, sources)",
                "skip_list": "7 ignored advisories with rationale",
            }))
    }
}
