//! Craftsmanship gate — stub (advisory, TDG + `cognitive_score` threshold).

use crate::change::Change;
use crate::gate::{Gate, GateId, GateOutcome, GateSeverity};

/// Advisory gate checking craftsmanship via TDG grade and `cognitive_score` thresholds.
pub struct CraftsmanshipGate;

impl Gate for CraftsmanshipGate {
    fn id(&self) -> GateId {
        GateId::Craftsmanship
    }
    fn severity(&self) -> GateSeverity {
        GateSeverity::Warn
    }
    fn check(&self, _: &Change) -> GateOutcome {
        GateOutcome::external(GateId::Craftsmanship, GateSeverity::Warn)
            .with_payload(serde_json::json!({
                "external": "touring ast tdg (grade >= B) + touring ast meta (cognitive_score <= 0.7) on files > 500 LOC",
            }))
    }
}
