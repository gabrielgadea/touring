//! P8.2 — WorkflowStage enum + deterministic stage detection.
//!
//! Every tool invocation in a Claude Code session occurs in a *workflow stage*
//! — a coarse, deterministic phase of the edit-understand-validate loop.
//! Knowing the stage lets the gateway and the advisor inject stage-appropriate
//! hints rather than generic ones.
//!
//! ## Stages
//!
//! ```text
//! Locate     — finding where something is (search, glob, index find)
//! Comprehend — reading / understanding (read, ast overview, tantivy)
//! Plan       — thinking / decomposing (sequential-thinking, decompose)
//! Mutate     — changing code (edit, write, create)
//! Validate   — verifying changes (bash cargo, test, clippy, check)
//! Recover    — responding to an error (bash after error, rollback)
//! ```
//!
//! ## Detection
//!
//! [`detect_stage`] takes the current [`ActionSignature`] and a short window
//! of recent tool names.  It is *purely deterministic*: same inputs → same
//! output.  It never calls async code, never panics, and never blocks.

use crate::action_signature::ActionSignature;
use serde::{Deserialize, Serialize};

// ── Stage enum ────────────────────────────────────────────────────────────────

/// Coarse phase of the tool-use workflow.
///
/// Ordered from earliest (Locate) to latest (Recover) in a typical session.
/// A session does not have to traverse every stage.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum WorkflowStage {
    /// Locating where something is: `Grep`, `Glob`, `touring index find`,
    /// `touring tantivy search`.
    Locate,
    /// Reading / understanding without changing: `Read`, `touring ast overview`,
    /// `touring ast blast`.
    Comprehend,
    /// Planning, thinking, decomposing: `mcp__sequential-thinking__*`,
    /// `touring decompose`, `touring session`.
    Plan,
    /// Mutating code: `Edit`, `Write`, `Write tool`,
    /// `Edit tool`.
    Mutate,
    /// Validating mutations: `Bash cargo check`, `Bash cargo test`,
    /// `Bash cargo clippy`, `touring e2e`.
    Validate,
    /// Recovering from an error: Bash following a prior error, rollback.
    Recover,
}

impl WorkflowStage {
    /// Human-readable label, used in hints and serialisation.
    pub fn label(self) -> &'static str {
        match self {
            Self::Locate => "locate",
            Self::Comprehend => "comprehend",
            Self::Plan => "plan",
            Self::Mutate => "mutate",
            Self::Validate => "validate",
            Self::Recover => "recover",
        }
    }

    /// All variants, in definition order.
    pub fn all() -> &'static [WorkflowStage] {
        &[
            Self::Locate,
            Self::Comprehend,
            Self::Plan,
            Self::Mutate,
            Self::Validate,
            Self::Recover,
        ]
    }
}

// ── Sliding-window state ──────────────────────────────────────────────────────

/// Lightweight, rolling tool-use state fed to [`detect_stage`].
///
/// The caller maintains this state by pushing tool names (as returned by
/// Claude Code) after each tool invocation. The window is capped at
/// [`WorkflowState::WINDOW`] entries to bound memory.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct WorkflowState {
    /// The last N tool names, most-recent last.
    pub recent_tools: Vec<String>,
    /// Whether the most-recent Bash command produced an error (exit != 0).
    pub last_bash_errored: bool,
}

impl WorkflowState {
    /// Maximum number of recent tool names retained.
    pub const WINDOW: usize = 8;

    /// Create an empty state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Push a tool name into the window, evicting the oldest entry when full.
    ///
    /// `errored` should be `true` when the call produced a non-zero exit or a
    /// tool-use error — used by [`detect_stage`] to infer `Recover`.
    pub fn push(&mut self, tool_name: impl Into<String>, errored: bool) {
        if self.recent_tools.len() >= Self::WINDOW {
            self.recent_tools.remove(0);
        }
        self.recent_tools.push(tool_name.into());
        self.last_bash_errored = errored;
    }

    /// Whether the window contains a tool whose name starts with `prefix`.
    pub fn contains_prefix(&self, prefix: &str) -> bool {
        self.recent_tools.iter().any(|t| t.starts_with(prefix))
    }

    /// Whether the window contains a Validate-class Bash command.
    pub fn contains_validate_bash(&self) -> bool {
        // We cannot inspect the command text here — we rely on the caller
        // having tagged those Bash invocations with a distinct tool name
        // (e.g. "Bash(cargo check)") OR on the detect_stage caller passing
        // the ActionSignature whose intent_class == "cargo".
        //
        // This method is a convenience for tests that inject synthetic names.
        self.recent_tools
            .iter()
            .any(|t| t == "Bash(cargo)" || t == "Bash(validate)")
    }
}

// ── Deterministic stage detector ─────────────────────────────────────────────

/// Infer the [`WorkflowStage`] from the current [`ActionSignature`] and the
/// recent-tool window.
///
/// The mapping is purely deterministic and is derived from the tool_class /
/// intent_class dimensions of [`ActionSignature`]:
///
/// | tool_class | intent_class / qualifier | Stage |
/// |---|---|---|
/// | `search` | any | Locate |
/// | `read` | any | Comprehend |
/// | `bash` | `cargo` | Validate |
/// | `bash` | any | Recover (if last_bash_errored) else Validate-or-Locate |
/// | `edit` / `write` | any | Mutate |
/// | `task` / `mcp` | any | Plan |
/// | `other` | any | Comprehend (fallback) |
///
/// Window heuristics (applied after the primary mapping):
/// - If `last_bash_errored` is true and the current tool is `bash` → Recover.
/// - If the window's last 2 entries are both `Bash` and intent is not `cargo`
///   → Locate (iterating, not validating).
///
/// Never panics. Returns a valid [`WorkflowStage`] for all inputs.
pub fn detect_stage(sig: &ActionSignature, state: &WorkflowState) -> WorkflowStage {
    // Recover is the highest-priority override: a Bash following an error is
    // always recovery, regardless of intent.
    if state.last_bash_errored && sig.tool_class == "bash" {
        return WorkflowStage::Recover;
    }

    match sig.tool_class.as_str() {
        "search" => WorkflowStage::Locate,
        "read" => WorkflowStage::Comprehend,
        "edit" | "write" => WorkflowStage::Mutate,
        "task" | "mcp" => WorkflowStage::Plan,
        "bash" => {
            if sig.intent_class == "cargo" {
                return WorkflowStage::Validate;
            }
            // Touring CLI bash calls are Locate (discovery), not Validate.
            if sig.intent_class == "touring" {
                return WorkflowStage::Locate;
            }
            // A pure Bash→Bash→Bash chain with no cargo/edit seen → Locate.
            if consecutive_bash_tail(state, 2) && !state.contains_validate_bash() {
                return WorkflowStage::Locate;
            }
            // Default for Bash: treat as Validate (post-edit verification).
            WorkflowStage::Validate
        }
        _ => WorkflowStage::Comprehend,
    }
}

/// True when the last `n` entries in `state.recent_tools` are all `"Bash"`.
fn consecutive_bash_tail(state: &WorkflowState, n: usize) -> bool {
    if state.recent_tools.len() < n {
        return false;
    }
    state
        .recent_tools
        .iter()
        .rev()
        .take(n)
        .all(|t| t == "Bash" || t.starts_with("Bash("))
}

// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;
    use crate::action_signature::ContextQualifier;

    fn sig(tool: &str, intent: &str) -> ActionSignature {
        ActionSignature {
            tool_class: tool.into(),
            intent_class: intent.into(),
            context_qualifier: ContextQualifier::Plain,
        }
    }

    fn empty_state() -> WorkflowState {
        WorkflowState::new()
    }

    #[test]
    fn search_tool_maps_to_locate() {
        assert_eq!(
            detect_stage(&sig("search", "symbol"), &empty_state()),
            WorkflowStage::Locate
        );
    }

    #[test]
    fn read_tool_maps_to_comprehend() {
        assert_eq!(
            detect_stage(&sig("read", "rs"), &empty_state()),
            WorkflowStage::Comprehend
        );
    }

    #[test]
    fn edit_tool_maps_to_mutate() {
        assert_eq!(
            detect_stage(&sig("edit", "rs"), &empty_state()),
            WorkflowStage::Mutate
        );
    }

    #[test]
    fn write_tool_maps_to_mutate() {
        assert_eq!(
            detect_stage(&sig("write", "rs"), &empty_state()),
            WorkflowStage::Mutate
        );
    }

    #[test]
    fn bash_cargo_maps_to_validate() {
        assert_eq!(
            detect_stage(&sig("bash", "cargo"), &empty_state()),
            WorkflowStage::Validate
        );
    }

    #[test]
    fn bash_after_error_maps_to_recover() {
        let mut state = WorkflowState::new();
        state.push("Bash", true);
        assert_eq!(
            detect_stage(&sig("bash", "ls"), &state),
            WorkflowStage::Recover
        );
    }

    #[test]
    fn task_tool_maps_to_plan() {
        assert_eq!(
            detect_stage(&sig("task", "unknown"), &empty_state()),
            WorkflowStage::Plan
        );
    }

    #[test]
    fn mcp_tool_maps_to_plan() {
        assert_eq!(
            detect_stage(&sig("mcp", "sequential-thinking"), &empty_state()),
            WorkflowStage::Plan
        );
    }

    #[test]
    fn unknown_tool_class_maps_to_comprehend() {
        assert_eq!(
            detect_stage(&sig("other", "unknown"), &empty_state()),
            WorkflowStage::Comprehend
        );
    }

    #[test]
    fn bash_bash_chain_without_cargo_maps_to_locate() {
        let mut state = WorkflowState::new();
        state.push("Bash", false);
        state.push("Bash", false);
        // third Bash with non-cargo intent → Locate
        assert_eq!(
            detect_stage(&sig("bash", "ls"), &state),
            WorkflowStage::Locate
        );
    }

    #[test]
    fn touring_bash_maps_to_locate() {
        assert_eq!(
            detect_stage(&sig("bash", "touring"), &empty_state()),
            WorkflowStage::Locate
        );
    }

    #[test]
    fn stage_labels_are_unique() {
        let labels: Vec<_> = WorkflowStage::all().iter().map(|s| s.label()).collect();
        let unique: std::collections::HashSet<_> = labels.iter().collect();
        assert_eq!(labels.len(), unique.len());
    }

    #[test]
    fn workflow_state_window_caps_at_8() {
        let mut state = WorkflowState::new();
        for i in 0..12 {
            state.push(format!("Tool{i}"), false);
        }
        assert_eq!(state.recent_tools.len(), WorkflowState::WINDOW);
        assert_eq!(state.recent_tools[0], "Tool4");
    }
}
