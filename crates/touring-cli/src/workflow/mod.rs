//! P8 — Workflow Intelligence Layer.
//!
//! This module provides the six components of the Workflow Intelligence Layer
//! (CEG Pln2 Phase P8):
//!
//! - [`mod@baseline`]    — forensic antipattern baseline mined from 3,058 CC sessions.
//! - [`stage`]       — deterministic workflow-stage inference from `ActionSignature`.
//! - [`antipattern`] — combination-antipattern detector wired into X1/X2.
//! - [`convert`]     — antipattern->elite-tool conversion advisor (P8.4).
//! - [`advise`]      — workflow next-step advisor via 10 combination patterns (P8.5).
//! - [`glob_diag`]   — Glob 26%-error-rate root-cause taxonomy + validator (P8.6).
//!
//! ## Architecture
//!
//! ```text
//! +-----------------------------------------------------------------+
//! |             Workflow Intelligence Layer (P8)                     |
//! |                                                                  |
//! |  baseline    --> ANTIPATTERN_BASELINE (const data)               |
//! |  stage       --> detect_stage(sig, state)                        |
//! |  antipattern --> detect_antipattern(sig, state)                  |
//! |                  +-> fold into X2 StaticReport                   |
//! |  convert     --> conversion_for(AntipatternKind)                 |
//! |                  +-> wired into X7 DECISION canonical_fix        |
//! |  advise      --> advise_next_step(WorkflowStage)                 |
//! |                  +-> injected into cli_suggester additionalContext|
//! |  glob_diag   --> validate_glob_pattern(pattern)                  |
//! |                  +-> surfaced pre-call in X7/cli_suggester hints  |
//! +-----------------------------------------------------------------+
//! ```
//!
//! P8.7 (workflow intelligence wired into cli_suggester + gateway X7 DECISION)
//! is delivered in this wave. Three wires landed:
//! - [`WorkflowEnrichment`] carrier populates `EnrichmentData::workflow_stage_hint`
//!   for every `PreToolUse` invocation via `cli_suggester::enrich`.
//! - X7 DECISION `build_canonical_fix` surfaces the elite-tool conversion hint
//!   when X2 STATIC raises a `Warn` carrying a `workflow[` antipattern finding.
//! - Exit-0 fail-open invariant holds on all new code paths.

pub mod advise;
pub mod convert;
pub mod glob_diag;
// S-13 (2026-06-06): the leaf-safe core (baseline/stage/antipattern) moved to the
// `touring-hooks-shared` leaf crate (breaks the gateway↔workflow cycle). Re-exported
// here so `crate::workflow::{baseline,stage,antipattern}::*` (used by advise, convert,
// cli_suggester) and the symbol re-exports below keep resolving unchanged.
pub use touring_hooks_shared::workflow::{antipattern, baseline, stage};

// -- Convenience re-exports ---------------------------------------------------

pub use advise::{WorkflowAdvice, WorkflowPattern, advise_next_step};
pub use antipattern::{
    CombinationAntipattern, antipattern_finding, antipattern_severity, detect_antipattern,
};
pub use baseline::{
    ANTIPATTERN_BASELINE, AntipatternEntry, AntipatternKind, GoodPatternEntry, GoodPatternKind,
    WorkflowBaseline, baseline,
};
pub use convert::{ConversionAdvice, conversion_for};
pub use glob_diag::{
    GlobErrorCategory, GlobErrorEntry, GlobErrorTaxonomy, GlobValidationResult,
    validate_glob_pattern,
};
pub use stage::{WorkflowStage, WorkflowState, detect_stage};

// ── P8.7 WorkflowEnrichment carrier ─────────────────────────────────────────

/// Carrier struct produced by the P8.7 wiring in `cli_suggester::enrich`.
///
/// Bridges the workflow layer (stage detection, antipattern detection, glob
/// validation) into the `EnrichmentData` that flows into `additionalContext`
/// for every `PreToolUse` hook invocation.
///
/// All fields are optional — every absent field degrades gracefully (fail-open).
#[derive(Debug, Clone, Default)]
pub struct WorkflowEnrichment {
    /// Current workflow stage label (e.g. `"locate"`, `"mutate"`).
    pub stage_label: Option<String>,
    /// Next-step hint from `advise_next_step` (one-line command).
    pub next_step_hint: Option<String>,
    /// Antipattern conversion hint for Bash Glob calls, derived from
    /// `validate_glob_pattern` when the pattern has a known error class.
    pub glob_hint: Option<String>,
    /// Elite-tool conversion hint from `conversion_for` when an antipattern
    /// was detected (Bash grep/cat/find etc.).  Advisory `Warn` only.
    pub antipattern_hint: Option<String>,
}

impl WorkflowEnrichment {
    /// Render to a single `Option<String>` suitable for `workflow_stage_hint`.
    ///
    /// Returns `None` when no field is populated (nothing to inject).
    /// Joins non-empty sections with ` | `.
    pub fn render(&self) -> Option<String> {
        let mut parts: Vec<String> = Vec::new();
        if let Some(ref s) = self.stage_label {
            parts.push(format!("stage={s}"));
        }
        if let Some(ref h) = self.next_step_hint {
            parts.push(format!("next={h}"));
        }
        if let Some(ref h) = self.glob_hint {
            parts.push(format!("glob_warn={h}"));
        }
        if let Some(ref h) = self.antipattern_hint {
            parts.push(format!("antipattern={h}"));
        }
        if parts.is_empty() {
            None
        } else {
            Some(parts.join(" | "))
        }
    }
}
