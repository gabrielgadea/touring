//! P8.5 — Workflow next-step advisor.
//!
//! Given the current [`WorkflowStage`], emits the elite [`WorkflowAdvice`]
//! that names the best next step drawn from the 10 combination patterns
//! mined from the forensic baseline:
//!
//! | # | Pattern | Description |
//! |---|---------|-------------|
//! | 1 | Funnel | Narrow scope before reading: search → filter → read |
//! | 2 | Pipeline | Sequential stage advancement: locate → comprehend → plan → mutate → validate |
//! | 3 | Map-First | Understand the full map before editing any single point |
//! | 4 | Probe-Confirm | Make a small probe, confirm outcome, then expand |
//! | 5 | Mirror | Apply the same change symmetrically to all callers |
//! | 6 | Speculate | Shadow-validate before applying: `touring shadow validate` |
//! | 7 | Subagent-Isolation | Delegate heavy reads to a subagent to protect context budget |
//! | 8 | Verify-After | Always validate after mutating: `cargo check` / `touring e2e` |
//! | 9 | Fan-out | Discover all consumers before touching a pub symbol |
//! | 10 | Checkpoint | Persist state before a risky multi-step operation |
//!
//! ## Advisory only (R13/R14)
//!
//! All advice is purely informational — no action is blocked. The advisor
//! surfaces the next-step hint in the `additionalContext` field of the
//! `cli_suggester` enrichment, not in the hard-block gate.

use crate::workflow::baseline::WorkflowBaseline;
use crate::workflow::stage::WorkflowStage;
use serde::{Deserialize, Serialize};

// ── WorkflowAdvice ────────────────────────────────────────────────────────────

/// The 10 elite workflow combination patterns.
///
/// Each variant represents a best-practice workflow pattern drawn from the
/// forensic baseline of 575,821 real tool-calls across 3,058 sessions.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WorkflowPattern {
    /// Narrow scope before reading: search → filter → read. Prevents blind
    /// reads (46,307 instances in the baseline).
    Funnel,
    /// Sequential stage advancement: locate → comprehend → plan → mutate →
    /// validate. The canonical path through the pipeline.
    Pipeline,
    /// Understand the full map (`touring ast blast`, `touring ast overview`)
    /// before editing any single point.
    MapFirst,
    /// Make a small probe, confirm outcome, then expand. Reduces blast-radius
    /// surprises.
    ProbeConfirm,
    /// Apply the same change symmetrically to all callers (cross-caller
    /// compare). Prevents asymmetric bugs.
    Mirror,
    /// Shadow-validate before applying: `touring shadow validate`. Catches
    /// issues before the change lands.
    Speculate,
    /// Delegate heavy reads / scans to a subagent to protect the context
    /// budget of the orchestrator.
    SubagentIsolation,
    /// Always validate after mutating: `cargo check` / `cargo test` /
    /// `touring e2e`. The most-missed pattern (10,541 edit-then-bash good
    /// patterns vs 24,462 total edits).
    VerifyAfter,
    /// Discover all consumers (`touring wiring impact`) before touching a
    /// pub symbol.
    FanOut,
    /// Persist state before a risky multi-step operation:
    /// `touring memory store --tier semantic`.
    Checkpoint,
}

impl WorkflowPattern {
    /// Short machine-readable label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Funnel => "funnel",
            Self::Pipeline => "pipeline",
            Self::MapFirst => "map-first",
            Self::ProbeConfirm => "probe-confirm",
            Self::Mirror => "mirror",
            Self::Speculate => "speculate",
            Self::SubagentIsolation => "subagent-isolation",
            Self::VerifyAfter => "verify-after",
            Self::FanOut => "fan-out",
            Self::Checkpoint => "checkpoint",
        }
    }

    /// All 10 patterns, in definition order.
    pub fn all() -> &'static [WorkflowPattern] {
        &[
            Self::Funnel,
            Self::Pipeline,
            Self::MapFirst,
            Self::ProbeConfirm,
            Self::Mirror,
            Self::Speculate,
            Self::SubagentIsolation,
            Self::VerifyAfter,
            Self::FanOut,
            Self::Checkpoint,
        ]
    }
}

/// Advisory next-step recommendation for a given workflow stage.
///
/// Contains the recommended [`WorkflowPattern`], the concrete elite command
/// to issue next, and an optional baseline metric that grounds the advice in
/// real data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorkflowAdvice {
    /// Current stage this advice is for.
    pub current_stage: WorkflowStage,
    /// The elite pattern recommended for this stage.
    pub pattern: WorkflowPattern,
    /// The concrete next command or action to take.
    pub next_step: &'static str,
    /// One-sentence rationale grounded in the forensic baseline or best
    /// practices.
    pub rationale: &'static str,
    /// Optional baseline metric that grounds the advice (e.g., antipattern
    /// frequency from [`WorkflowBaseline`]).
    pub baseline_count: Option<u64>,
}

impl WorkflowAdvice {
    /// Format the advice as a single hint line for the `additionalContext`
    /// enrichment field.
    pub fn as_hint(&self) -> String {
        let baseline_note = self
            .baseline_count
            .map(|c| format!(" (baseline: {} instances)", c))
            .unwrap_or_default();
        format!(
            "workflow[{}→{}]: {}{}  Next: `{}`",
            self.current_stage.label(),
            self.pattern.label(),
            self.rationale,
            baseline_note,
            self.next_step,
        )
    }
}

// ── advise_next_step ──────────────────────────────────────────────────────────

/// Emit the elite next-step [`WorkflowAdvice`] for the current
/// [`WorkflowStage`].
///
/// The mapping is derived from the forensic baseline and the 10 combination
/// patterns. Advisory only — never blocks execution.
///
/// | Stage | Pattern | Rationale |
/// |---|---|---|
/// | `Locate` | `Funnel` | Narrow scope via search before reading |
/// | `Comprehend` | `MapFirst` | Build the full map before touching anything |
/// | `Plan` | `Speculate` | Shadow-validate the plan before committing |
/// | `Mutate` | `VerifyAfter` | Always validate after every edit |
/// | `Validate` | `Pipeline` | Advance to the next pipeline stage |
/// | `Recover` | `ProbeConfirm` | Small probe, confirm, then expand the fix |
pub fn advise_next_step(
    stage: WorkflowStage,
    baseline: Option<&WorkflowBaseline>,
) -> WorkflowAdvice {
    match stage {
        WorkflowStage::Locate => WorkflowAdvice {
            current_stage: stage,
            pattern: WorkflowPattern::Funnel,
            next_step: "touring index find \"<symbol>\" -j  # then Read(<file>, offset=<line>)",
            rationale: "Funnel: narrow scope with a search before reading — avoids blind reads",
            baseline_count: baseline.map(|b| {
                b.antipattern_count(crate::workflow::baseline::AntipatternKind::ReadWithoutLocate)
            }),
        },
        WorkflowStage::Comprehend => WorkflowAdvice {
            current_stage: stage,
            pattern: WorkflowPattern::MapFirst,
            next_step: "touring ast blast <file> && touring wiring impact <symbol> --depth 2",
            rationale: "Map-First: understand blast radius and wiring before editing",
            baseline_count: baseline.map(|b| {
                b.antipattern_count(crate::workflow::baseline::AntipatternKind::EditWithoutRead)
            }),
        },
        WorkflowStage::Plan => WorkflowAdvice {
            current_stage: stage,
            pattern: WorkflowPattern::Speculate,
            next_step: "touring shadow validate  # score >= 0.8 before applying",
            rationale: "Speculate: shadow-validate the planned change before it lands",
            baseline_count: None,
        },
        WorkflowStage::Mutate => WorkflowAdvice {
            current_stage: stage,
            pattern: WorkflowPattern::VerifyAfter,
            next_step: "cargo check -p <crate>  # then cargo test -p <crate>",
            rationale: "Verify-After: compile-verify every mutation before moving on",
            baseline_count: None,
        },
        WorkflowStage::Validate => WorkflowAdvice {
            current_stage: stage,
            pattern: WorkflowPattern::Pipeline,
            next_step: "touring wiring orphans -j  # REGRA #0: no new orphan pub symbols",
            rationale: "Pipeline: advance to orphan-check — the next stage in the quality pipeline",
            baseline_count: None,
        },
        WorkflowStage::Recover => WorkflowAdvice {
            current_stage: stage,
            pattern: WorkflowPattern::ProbeConfirm,
            next_step: "cargo check -p <crate> 2>&1 | grep \"^error\" | head -5",
            rationale: "Probe-Confirm: small diagnostic probe, confirm root cause, then expand fix",
            baseline_count: None,
        },
    }
}

// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::baseline::baseline;

    // ── All stages produce advice ─────────────────────────────────────────────

    #[test]
    fn every_stage_produces_advice() {
        for stage in WorkflowStage::all() {
            let advice = advise_next_step(*stage, None);
            assert_eq!(advice.current_stage, *stage);
        }
    }

    #[test]
    fn every_stage_advice_has_non_empty_next_step() {
        for stage in WorkflowStage::all() {
            let advice = advise_next_step(*stage, None);
            assert!(
                !advice.next_step.is_empty(),
                "stage {:?} must have a next_step",
                stage
            );
        }
    }

    #[test]
    fn every_stage_advice_has_non_empty_rationale() {
        for stage in WorkflowStage::all() {
            let advice = advise_next_step(*stage, None);
            assert!(
                !advice.rationale.is_empty(),
                "stage {:?} must have a rationale",
                stage
            );
        }
    }

    // ── Specific stage → pattern mappings ────────────────────────────────────

    #[test]
    fn locate_maps_to_funnel_pattern() {
        let advice = advise_next_step(WorkflowStage::Locate, None);
        assert_eq!(advice.pattern, WorkflowPattern::Funnel);
    }

    #[test]
    fn comprehend_maps_to_map_first_pattern() {
        let advice = advise_next_step(WorkflowStage::Comprehend, None);
        assert_eq!(advice.pattern, WorkflowPattern::MapFirst);
    }

    #[test]
    fn plan_maps_to_speculate_pattern() {
        let advice = advise_next_step(WorkflowStage::Plan, None);
        assert_eq!(advice.pattern, WorkflowPattern::Speculate);
    }

    #[test]
    fn mutate_maps_to_verify_after_pattern() {
        let advice = advise_next_step(WorkflowStage::Mutate, None);
        assert_eq!(advice.pattern, WorkflowPattern::VerifyAfter);
    }

    #[test]
    fn validate_maps_to_pipeline_pattern() {
        let advice = advise_next_step(WorkflowStage::Validate, None);
        assert_eq!(advice.pattern, WorkflowPattern::Pipeline);
    }

    #[test]
    fn recover_maps_to_probe_confirm_pattern() {
        let advice = advise_next_step(WorkflowStage::Recover, None);
        assert_eq!(advice.pattern, WorkflowPattern::ProbeConfirm);
    }

    // ── Baseline integration ──────────────────────────────────────────────────

    #[test]
    fn locate_advice_carries_baseline_count_from_read_without_locate() {
        let b = baseline();
        let advice = advise_next_step(WorkflowStage::Locate, Some(b));
        assert!(
            advice.baseline_count.is_some(),
            "Locate advice should carry a baseline count"
        );
        // The baseline has 46,307 ReadWithoutLocate instances.
        assert_eq!(advice.baseline_count, Some(46307));
    }

    #[test]
    fn comprehend_advice_carries_baseline_count_from_edit_without_read() {
        let b = baseline();
        let advice = advise_next_step(WorkflowStage::Comprehend, Some(b));
        assert!(
            advice.baseline_count.is_some(),
            "Comprehend advice should carry a baseline count"
        );
        assert_eq!(advice.baseline_count, Some(2273));
    }

    // ── as_hint ───────────────────────────────────────────────────────────────

    #[test]
    fn as_hint_contains_stage_label() {
        let advice = advise_next_step(WorkflowStage::Mutate, None);
        let hint = advice.as_hint();
        assert!(hint.contains("mutate"), "hint must name the stage: {hint}");
    }

    #[test]
    fn as_hint_contains_pattern_label() {
        let advice = advise_next_step(WorkflowStage::Mutate, None);
        let hint = advice.as_hint();
        assert!(
            hint.contains("verify-after"),
            "hint must name the pattern: {hint}"
        );
    }

    // ── WorkflowPattern ───────────────────────────────────────────────────────

    #[test]
    fn all_ten_patterns_defined() {
        assert_eq!(WorkflowPattern::all().len(), 10);
    }

    #[test]
    fn pattern_labels_are_unique() {
        let labels: Vec<_> = WorkflowPattern::all().iter().map(|p| p.label()).collect();
        let unique: std::collections::HashSet<_> = labels.iter().collect();
        assert_eq!(labels.len(), unique.len(), "pattern labels must be unique");
    }

    // ── Serde roundtrip ───────────────────────────────────────────────────────

    #[test]
    fn advice_serde_roundtrip() {
        // WorkflowAdvice has &'static str fields; deserializing back into the
        // struct requires 'static lifetime which serde_json::from_str cannot
        // provide.  Instead we validate that the serialized JSON is valid and
        // contains the expected keys/values using serde_json::Value.
        let advice = advise_next_step(WorkflowStage::Mutate, None);
        let json = serde_json::to_string(&advice).expect("serialize");
        let value: serde_json::Value = serde_json::from_str(&json).expect("deserialize as Value");
        assert!(
            value.get("current_stage").is_some(),
            "must have current_stage field"
        );
        assert!(value.get("pattern").is_some(), "must have pattern field");
        assert!(
            value.get("next_step").is_some(),
            "must have next_step field"
        );
        assert!(
            value.get("rationale").is_some(),
            "must have rationale field"
        );
        // Verify next_step is a non-empty string
        let next_step = value["next_step"].as_str().unwrap_or("");
        assert!(!next_step.is_empty(), "next_step must be non-empty string");
        // Verify pattern matches WorkflowPattern::VerifyAfter (Mutate stage)
        let pattern_str = value["pattern"].as_str().unwrap_or("");
        assert_eq!(
            pattern_str, "VerifyAfter",
            "Mutate stage must map to VerifyAfter pattern"
        );
    }
}
