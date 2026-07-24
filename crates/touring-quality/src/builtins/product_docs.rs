//! Product docs gate — stub (`sync_metrics` + `changelog_synth` anchor).

use crate::change::Change;
use crate::gate::{Gate, GateId, GateOutcome, GateSeverity};

/// Blocking gate asserting product-doc freshness via `sync_metrics` and `changelog_synth` anchors.
pub struct ProductDocsGate;

impl Gate for ProductDocsGate {
    fn id(&self) -> GateId {
        GateId::ProductDocs
    }
    fn severity(&self) -> GateSeverity {
        GateSeverity::Block
    }
    fn check(&self, _: &Change) -> GateOutcome {
        GateOutcome::external(GateId::ProductDocs, GateSeverity::Block)
            .with_payload(serde_json::json!({
                "external": "docs/sync_metrics.py --check (ARCHITECTURE.md anti-drift) + docs/changelog_synth.py (102 TOON checkpoints)",
            }))
    }
}
