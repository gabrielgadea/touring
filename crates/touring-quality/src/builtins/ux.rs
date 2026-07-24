//! UX gate — stub (`clap_complete` covers Bash/Zsh/Fish/PowerShell/Elvish).

use crate::change::Change;
use crate::gate::{Gate, GateId, GateOutcome, GateSeverity};

/// Warning gate asserting CLI UX coverage (`clap_complete` shells + `--help` audit).
pub struct UxGate;

impl Gate for UxGate {
    fn id(&self) -> GateId {
        GateId::Ux
    }
    fn severity(&self) -> GateSeverity {
        GateSeverity::Warn
    }
    fn check(&self, _: &Change) -> GateOutcome {
        GateOutcome::external(GateId::Ux, GateSeverity::Warn)
            .with_payload(serde_json::json!({
                "external": "clap_complete shells (Bash/Zsh/Fish/PowerShell/Elvish) + --help coverage via docs/ux_audit.py",
            }))
    }
}
