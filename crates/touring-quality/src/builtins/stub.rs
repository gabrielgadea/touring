//! Stub gates — minimal implementations of 1 gate (`CodeQuality`).
//! All other stub gates (testing, naming, navigability, etc.) live in their own
//! minimal files under `builtins/`; this file holds the generic `Stub` builder.

use crate::change::Change;
use crate::gate::{Gate, GateId, GateOutcome, GateSeverity};

/// Generic stub gate. Real implementations live in sibling modules.
pub struct Stub {
    /// Identifier of the gate this stub stands in for.
    pub id: GateId,
    /// Severity applied to the stub's external outcome.
    pub severity: GateSeverity,
    /// Human-readable note explaining how the real check is enforced.
    pub message: &'static str,
}

impl Stub {
    /// Builds the `CodeQuality` stub, deferring to clippy and rustfmt in CI.
    #[must_use]
    pub fn code_quality() -> Self {
        Self {
            id: GateId::CodeQuality,
            severity: GateSeverity::Block,
            message: "code_quality stub — covered by `cargo clippy -D warnings` + `cargo fmt --check` in CI",
        }
    }
}

impl Gate for Stub {
    fn id(&self) -> GateId {
        self.id
    }
    fn severity(&self) -> GateSeverity {
        self.severity
    }
    fn check(&self, _change: &Change) -> GateOutcome {
        let mut o = GateOutcome::external(self.id, self.severity);
        o.message = self.message.into();
        o
    }
}
