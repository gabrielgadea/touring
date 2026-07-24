//! P8.3 — CombinationAntipattern detector.
//!
//! Detects the 5 high-frequency antipatterns from the forensic baseline at
//! stage X1 CLASSIFY time, before any execution. Results fold into the X2
//! STATIC severity via [`antipattern_severity`].
//!
//! ## Design constraints
//!
//! - **Advisory only** (R13/R14): detection never produces a hard `Block`.
//!   The worst outcome is `StaticSeverity::Warn` appended to the findings
//!   list — the human can always override.
//! - **Deterministic**: given the same `ActionSignature` + `WorkflowState`,
//!   `detect_antipattern` always returns the same result.
//! - **Fail-open**: no panics, no unwraps in production paths.
//!
//! ## The 5 antipatterns
//!
//! | Variant | Forensic count | Description |
//! |---|---|---|
//! | `BashGrepRaw` | 35,975 | `grep`/`rg` via Bash instead of `touring index find` |
//! | `BashCatHeadTail` | 44,487 | `cat`/`head`/`tail` via Bash instead of `Read` tool |
//! | `BashFind` | 3,494 | `find` via Bash instead of `touring index files` |
//! | `EditWithoutRead` | 2,273 | `Edit` not preceded by a `Read` of the same target |
//! | `ReadWithoutLocate` | 46,307 | `Read` not preceded by a search in the current window |

use crate::action_signature::ActionSignature;
use crate::workflow::baseline::AntipatternKind;
use crate::workflow::stage::WorkflowState;
use serde::{Deserialize, Serialize};

// ── Detected antipattern ──────────────────────────────────────────────────────

/// A single detected combination antipattern, with an advisory hint.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CombinationAntipattern {
    /// Which antipattern was detected.
    pub kind: AntipatternKind,
    /// One-line human-readable description of what was detected.
    pub description: &'static str,
    /// Advisory hint suggesting the canonical alternative.
    pub hint: &'static str,
}

impl CombinationAntipattern {
    /// Build a `CombinationAntipattern` for a given `AntipatternKind`.
    fn for_kind(kind: AntipatternKind) -> Self {
        let (description, hint) = static_meta(kind);
        Self {
            kind,
            description,
            hint,
        }
    }
}

/// Static (description, hint) strings for each antipattern kind.
fn static_meta(kind: AntipatternKind) -> (&'static str, &'static str) {
    match kind {
        AntipatternKind::BashGrepRaw => (
            "grep/rg run as Bash instead of touring index find",
            "Use `touring index find <symbol>` or `touring tantivy search \"<query>\"` — <10ms, symbol-aware.",
        ),
        AntipatternKind::BashCatHeadTail => (
            "cat/head/tail run as Bash instead of Read tool",
            "Use the Read tool with `limit`/`offset` for range-controlled, token-budgeted reads.",
        ),
        AntipatternKind::BashFind => (
            "find run as Bash instead of touring index files",
            "Use `touring index files \"<pattern>\"` — symbol-aware enumeration in <10ms.",
        ),
        AntipatternKind::BashEcho => (
            "echo used to create/overwrite a file in Bash",
            "Use `Write tool` (new file) or the Write tool (config files).",
        ),
        AntipatternKind::EditWithoutRead => (
            "Edit not preceded by a Read of the same target in the current window",
            "Read the file first (`touring ast meta` + Read) to establish context before editing.",
        ),
        AntipatternKind::ReadWithoutLocate => (
            "Read not preceded by a search in the current tool window",
            "Locate before reading: `touring index find <symbol>` or `Grep`/`Glob` first.",
        ),
        AntipatternKind::BashAwk => (
            "awk used for in-place streaming transforms",
            "Use `touring ast grep --rewrite` for structured edits.",
        ),
        AntipatternKind::BashSedInplace => (
            "sed -i used for in-place edits",
            "Use `touring ast grep --rewrite` instead of sed -i.",
        ),
        AntipatternKind::ClaudeCliInBash => (
            "claude -p / run_loop invoked inside Bash",
            "Never nest LLM CLI calls — use Touring CLI + deterministic scripts instead.",
        ),
        AntipatternKind::BashPcre2Default => (
            "grep -P / rg -P / --pcre2 used, reintroducing backtracking into the search path",
            "Use the default linear-time regex engine; add -P/--pcre2 only when lookarounds or backreferences are strictly required.",
        ),
        AntipatternKind::SerialReadNoFanout => (
            "N consecutive Read calls without interleaved search/fan-out",
            "Use a parallel fan-out block (Glob + multiple Read calls in one batch) — same latency, N× throughput.",
        ),
    }
}

// ── Detector ──────────────────────────────────────────────────────────────────

/// Detect which (if any) of the 5 primary combination antipatterns apply to
/// the current tool invocation.
///
/// Returns `None` when no antipattern is detected — the common case.
/// Returns `Some(antipattern)` with the highest-priority match when one is
/// found.  Detection is deterministic and never panics.
///
/// ## Priority order (highest first)
///
/// 1. `BashGrepRaw`
/// 2. `BashCatHeadTail`
/// 3. `BashFind`
/// 4. `EditWithoutRead`
/// 5. `ReadWithoutLocate`
pub fn detect_antipattern(
    sig: &ActionSignature,
    state: &WorkflowState,
) -> Option<CombinationAntipattern> {
    // ── Bash-class antipatterns ───────────────────────────────────────────────
    if sig.tool_class == "bash" {
        // BashGrepRaw: intent_class == "grep" or "rg" (set by classify_intent_class)
        if sig.intent_class == "grep" || sig.intent_class == "rg" {
            return Some(CombinationAntipattern::for_kind(
                AntipatternKind::BashGrepRaw,
            ));
        }
        // BashCatHeadTail: intent_class == "cat", "head", or "tail"
        if matches!(sig.intent_class.as_str(), "cat" | "head" | "tail") {
            return Some(CombinationAntipattern::for_kind(
                AntipatternKind::BashCatHeadTail,
            ));
        }
        // BashFind: intent_class == "find"
        if sig.intent_class == "find" {
            return Some(CombinationAntipattern::for_kind(AntipatternKind::BashFind));
        }
    }

    // ── Edit-class antipatterns ───────────────────────────────────────────────
    if sig.tool_class == "edit" {
        // EditWithoutRead: no Read in the recent window
        if !state.contains_prefix("Read") {
            return Some(CombinationAntipattern::for_kind(
                AntipatternKind::EditWithoutRead,
            ));
        }
    }

    // ── Read-class antipatterns ───────────────────────────────────────────────
    if sig.tool_class == "read" {
        // SerialReadNoFanout: checked FIRST (more specific than ReadWithoutLocate).
        // Count how many consecutive Read entries sit at the tail of the window;
        // add 1 for the current invocation to get the total serial depth.
        let prior_consecutive_reads = state
            .recent_tools
            .iter()
            .rev()
            .take_while(|t| t.starts_with("Read"))
            .count();
        // prior_consecutive_reads + 1 (current) >= 2  ⟺  prior >= 1
        if prior_consecutive_reads >= 1 {
            return Some(CombinationAntipattern::for_kind(
                AntipatternKind::SerialReadNoFanout,
            ));
        }

        // ReadWithoutLocate: no search in the recent window
        let has_locate = state.contains_prefix("Grep")
            || state.contains_prefix("Glob")
            || state.contains_prefix("search")
            || state.recent_tools.iter().any(|t| t.contains("touring"));
        if !has_locate {
            return Some(CombinationAntipattern::for_kind(
                AntipatternKind::ReadWithoutLocate,
            ));
        }
    }

    // ── Bash PCRE2 antipattern ────────────────────────────────────────────────
    if sig.tool_class == "bash" {
        // BashPcre2Default: command body carries -P flag or --pcre2 flag.
        // We detect via the intent_class set by the bash classifier when it
        // sees grep/rg with a -P or --pcre2 argument.  The intent_class is
        // "grep-pcre2" or "rg-pcre2" for those specific invocations.
        if sig.intent_class == "grep-pcre2" || sig.intent_class == "rg-pcre2" {
            return Some(CombinationAntipattern::for_kind(
                AntipatternKind::BashPcre2Default,
            ));
        }
    }

    None
}

/// Map a detected antipattern to its advisory X2 STATIC severity addition.
///
/// All antipatterns are `Warn` — never `Block` (R13/R14 advisory constraint).
/// Returns `None` when `antipattern` is `None` (no-op path).
pub fn antipattern_severity(
    antipattern: Option<&CombinationAntipattern>,
) -> Option<crate::severity::StaticSeverity> {
    // S-13 (2026-06-06): StaticSeverity is now the shared leaf vocabulary
    // (`crate::severity`), not `gateway::static_stage` — this is what lets the
    // antipattern detector live in the leaf without a cycle back to the gateway.
    antipattern.map(|_| crate::severity::StaticSeverity::Warn)
}

/// Format a detected antipattern as a single X2 STATIC finding line.
///
/// Returns `None` when `antipattern` is `None`.
pub fn antipattern_finding(antipattern: Option<&CombinationAntipattern>) -> Option<String> {
    antipattern.map(|ap| {
        format!(
            "workflow[{}]: {} — {}",
            ap.kind.label(),
            ap.description,
            ap.hint
        )
    })
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

    fn state_with(tools: &[&str]) -> WorkflowState {
        let mut s = WorkflowState::new();
        for t in tools {
            s.push(*t, false);
        }
        s
    }

    // ── BashGrepRaw ──────────────────────────────────────────────────────────

    #[test]
    fn bash_grep_intent_detected() {
        let ap = detect_antipattern(&sig("bash", "grep"), &empty_state());
        assert_eq!(ap.map(|a| a.kind), Some(AntipatternKind::BashGrepRaw));
    }

    #[test]
    fn bash_rg_intent_detected() {
        let ap = detect_antipattern(&sig("bash", "rg"), &empty_state());
        assert_eq!(ap.map(|a| a.kind), Some(AntipatternKind::BashGrepRaw));
    }

    // ── BashCatHeadTail ──────────────────────────────────────────────────────

    #[test]
    fn bash_cat_intent_detected() {
        let ap = detect_antipattern(&sig("bash", "cat"), &empty_state());
        assert_eq!(ap.map(|a| a.kind), Some(AntipatternKind::BashCatHeadTail));
    }

    #[test]
    fn bash_head_intent_detected() {
        let ap = detect_antipattern(&sig("bash", "head"), &empty_state());
        assert_eq!(ap.map(|a| a.kind), Some(AntipatternKind::BashCatHeadTail));
    }

    #[test]
    fn bash_tail_intent_detected() {
        let ap = detect_antipattern(&sig("bash", "tail"), &empty_state());
        assert_eq!(ap.map(|a| a.kind), Some(AntipatternKind::BashCatHeadTail));
    }

    // ── BashFind ─────────────────────────────────────────────────────────────

    #[test]
    fn bash_find_intent_detected() {
        let ap = detect_antipattern(&sig("bash", "find"), &empty_state());
        assert_eq!(ap.map(|a| a.kind), Some(AntipatternKind::BashFind));
    }

    // ── EditWithoutRead ──────────────────────────────────────────────────────

    #[test]
    fn edit_without_read_detected() {
        let state = state_with(&["Bash", "Bash"]);
        let ap = detect_antipattern(&sig("edit", "rs"), &state);
        assert_eq!(ap.map(|a| a.kind), Some(AntipatternKind::EditWithoutRead));
    }

    #[test]
    fn edit_with_prior_read_not_detected() {
        let state = state_with(&["Read", "Bash"]);
        let ap = detect_antipattern(&sig("edit", "rs"), &state);
        assert!(ap.is_none());
    }

    // ── ReadWithoutLocate ────────────────────────────────────────────────────

    #[test]
    fn read_without_locate_detected() {
        let ap = detect_antipattern(&sig("read", "rs"), &empty_state());
        assert_eq!(ap.map(|a| a.kind), Some(AntipatternKind::ReadWithoutLocate));
    }

    #[test]
    fn read_after_grep_not_detected() {
        let state = state_with(&["Grep"]);
        let ap = detect_antipattern(&sig("read", "rs"), &state);
        assert!(ap.is_none());
    }

    #[test]
    fn read_after_glob_not_detected() {
        let state = state_with(&["Glob"]);
        let ap = detect_antipattern(&sig("read", "rs"), &state);
        assert!(ap.is_none());
    }

    #[test]
    fn read_after_touring_not_detected() {
        let state = state_with(&["touring-index-find"]);
        let ap = detect_antipattern(&sig("read", "rs"), &state);
        assert!(ap.is_none());
    }

    // ── No antipattern ───────────────────────────────────────────────────────

    #[test]
    fn cargo_bash_not_detected() {
        let ap = detect_antipattern(&sig("bash", "cargo"), &empty_state());
        assert!(ap.is_none(), "cargo check is not an antipattern");
    }

    #[test]
    fn search_tool_not_detected() {
        let ap = detect_antipattern(&sig("search", "symbol"), &empty_state());
        assert!(ap.is_none());
    }

    // ── Severity + finding format ────────────────────────────────────────────

    #[test]
    fn antipattern_severity_is_warn_not_block() {
        use crate::severity::StaticSeverity;
        let ap = detect_antipattern(&sig("bash", "grep"), &empty_state());
        let sev = antipattern_severity(ap.as_ref());
        assert_eq!(sev, Some(StaticSeverity::Warn));
    }

    #[test]
    fn antipattern_finding_contains_kind_label() {
        let ap = detect_antipattern(&sig("bash", "grep"), &empty_state());
        let finding = antipattern_finding(ap.as_ref()).unwrap();
        assert!(finding.contains("bash-grep-raw"), "finding: {finding}");
    }

    #[test]
    fn none_antipattern_returns_none_severity() {
        assert!(antipattern_severity(None).is_none());
    }

    // ── BashPcre2Default ─────────────────────────────────────────────────────

    #[test]
    fn bash_pcre2_grep_intent_detected() {
        let ap = detect_antipattern(&sig("bash", "grep-pcre2"), &empty_state());
        assert_eq!(ap.map(|a| a.kind), Some(AntipatternKind::BashPcre2Default));
    }

    #[test]
    fn bash_pcre2_rg_intent_detected() {
        let ap = detect_antipattern(&sig("bash", "rg-pcre2"), &empty_state());
        assert_eq!(ap.map(|a| a.kind), Some(AntipatternKind::BashPcre2Default));
    }

    #[test]
    fn bash_grep_without_pcre2_not_detected_as_pcre2() {
        // plain grep → BashGrepRaw, not BashPcre2Default
        let ap = detect_antipattern(&sig("bash", "grep"), &empty_state());
        assert_eq!(ap.map(|a| a.kind), Some(AntipatternKind::BashGrepRaw));
    }

    #[test]
    fn bash_rg_without_pcre2_not_detected_as_pcre2() {
        // plain rg → BashGrepRaw, not BashPcre2Default
        let ap = detect_antipattern(&sig("bash", "rg"), &empty_state());
        assert_eq!(ap.map(|a| a.kind), Some(AntipatternKind::BashGrepRaw));
    }

    // ── SerialReadNoFanout ───────────────────────────────────────────────────

    #[test]
    fn serial_read_two_consecutive_detected() {
        // Window already has one Read; current tool is also read → 2 consecutive.
        let state = state_with(&["Grep", "Read"]);
        let ap = detect_antipattern(&sig("read", "rs"), &state);
        assert_eq!(
            ap.map(|a| a.kind),
            Some(AntipatternKind::SerialReadNoFanout)
        );
    }

    #[test]
    fn serial_read_three_consecutive_detected() {
        let state = state_with(&["Read", "Read"]);
        let ap = detect_antipattern(&sig("read", "rs"), &state);
        assert_eq!(
            ap.map(|a| a.kind),
            Some(AntipatternKind::SerialReadNoFanout)
        );
    }

    #[test]
    fn single_read_after_grep_not_serial() {
        // Only one prior Read in window — not serial.
        let state = state_with(&["Grep"]);
        let ap = detect_antipattern(&sig("read", "rs"), &state);
        // No antipattern: has locate (Grep) and only 1 consecutive Read (itself).
        assert!(
            ap.is_none(),
            "single Read after Grep should not be flagged: {ap:?}"
        );
    }

    #[test]
    fn read_interleaved_with_grep_not_serial() {
        // Read → Grep → Read: the Grep breaks the consecutive chain.
        let state = state_with(&["Read", "Grep"]);
        let ap = detect_antipattern(&sig("read", "rs"), &state);
        assert!(
            ap.is_none(),
            "Read after Grep should not be flagged as serial: {ap:?}"
        );
    }
}
