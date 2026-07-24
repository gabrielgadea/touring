//! P8.4 — Antipattern-to-elite-tool conversion advisor.
//!
//! For every [`AntipatternKind`] detected by the P8.3 detector, this module
//! emits a [`ConversionAdvice`] that names the elite alternative, together
//! with a ready-to-paste command example and a `Verdict::Warn` carrying the
//! human-readable message.
//!
//! ## Design constraints (R13/R14)
//!
//! All advice is **advisory only** — the worst outcome is `Verdict::Warn`.
//! This module never emits `Verdict::Deny`. The goal is to guide agents toward
//! elite tool use, not to block execution.
//!
//! ## Wire-in to X7 DECISION
//!
//! [`conversion_for`] is called from the X7 path inside
//! [`crate::gateway::decision`] when the static report carries a workflow
//! antipattern finding. The returned [`ConversionAdvice`] is attached to
//! [`crate::gateway::decision::GateDecision::canonical_fix`] if no harder
//! reason has already been set (deny-wins discipline is preserved — a hard
//! block from X2 or a denied capability from X6 always takes precedence).

use crate::gateway::decision::Verdict;
use crate::workflow::baseline::AntipatternKind;
use serde::{Deserialize, Serialize};

// ── ConversionAdvice ──────────────────────────────────────────────────────────

/// Advisory conversion for a detected antipattern.
///
/// Carries the elite-tool alternative, an example command, and the advisory
/// verdict (`Warn` — never `Deny`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConversionAdvice {
    /// The antipattern that triggered this advice.
    pub antipattern: AntipatternKind,
    /// Name of the elite tool or command pattern to use instead.
    pub elite_tool: &'static str,
    /// One-line human-readable explanation of why the antipattern is costly.
    pub rationale: &'static str,
    /// A short, ready-to-paste command example.
    pub example: &'static str,
    /// The advisory verdict — always [`Verdict::Warn`] (R13/R14).
    pub verdict: Verdict,
}

impl ConversionAdvice {
    /// `true` if this advice should be surfaced to the user.
    ///
    /// Currently always `true` — every antipattern warrants a hint. Reserved
    /// for future suppression based on confidence or session-level toggles.
    pub fn should_surface(&self) -> bool {
        true
    }

    /// Format the advice as a single human-readable hint line suitable for
    /// injection into the `canonical_fix` field of a [`GateDecision`].
    ///
    /// [`GateDecision`]: crate::gateway::decision::GateDecision
    pub fn as_hint(&self) -> String {
        format!(
            "workflow[{}] → use {} instead. {} Example: `{}`",
            self.antipattern.label(),
            self.elite_tool,
            self.rationale,
            self.example,
        )
    }
}

// ── conversion_for ────────────────────────────────────────────────────────────

/// Return the [`ConversionAdvice`] for the given [`AntipatternKind`].
///
/// Never returns `Verdict::Deny` — advice is always advisory (`Warn`).
/// Never panics.
pub fn conversion_for(kind: AntipatternKind) -> ConversionAdvice {
    let (elite_tool, rationale, example) = conversion_meta(kind);
    ConversionAdvice {
        antipattern: kind,
        elite_tool,
        rationale,
        example,
        verdict: Verdict::Warn,
    }
}

/// Static (elite_tool, rationale, example) metadata per antipattern.
///
/// Every mapping is grounded in the forensic baseline:
/// - `BashGrepRaw`     → 35,975 instances; `Grep` tool / `touring tantivy search`
/// - `BashCatHeadTail` → 44,487 instances; `Read` with `offset`/`limit`
/// - `BashFind`        →  3,494 instances; `Glob` tool
/// - `EditWithoutRead` →  2,273 instances; `Read` + `touring ast meta` first
/// - `ReadWithoutLocate`→ 46,307 instances; locate first with `Grep`/`Glob`/`touring index find`
fn conversion_meta(kind: AntipatternKind) -> (&'static str, &'static str, &'static str) {
    match kind {
        AntipatternKind::BashGrepRaw => (
            "Grep tool or `touring tantivy search`",
            "The Grep tool is symbol-aware and avoids shell spawning overhead; \
             `touring tantivy search` returns BM25-ranked results in <10 ms.",
            "touring tantivy search \"<symbol>\"  # or: Grep tool with pattern",
        ),
        AntipatternKind::BashCatHeadTail => (
            "Read tool (offset/limit)",
            "The Read tool provides line-numbered, token-budgeted range reads \
             without shell spawning; use `offset` + `limit` for large files.",
            "Read(<path>, offset=40, limit=60)  # reads lines 40-100",
        ),
        AntipatternKind::BashFind => (
            "Glob tool or `touring index files`",
            "The Glob tool resolves patterns against the workspace index; \
             `touring index files \"<pattern>\"` is symbol-aware and runs in <10 ms.",
            "Glob(\"crates/**/*.rs\")  # or: touring index files \"*.rs\"",
        ),
        AntipatternKind::BashEcho => (
            "`Write tool` or Write tool",
            "`echo >` skips the touring-native tooling 12-stage gate (VGP, TDG, wiring audit). \
             Use `Write tool` for code files, Write tool for config.",
            "Write tool --path foo.rs --kind RustModule --intent \"<desc>\"",
        ),
        AntipatternKind::EditWithoutRead => (
            "Read + `touring ast meta` before Edit",
            "Blind edits miss blast_radius, quality_score, and cognitive_score context. \
             A prior Read establishes the edit window and prevents accidental overwrites.",
            "touring ast meta <file> --depth summary -j && Read(<file>)  # then Edit",
        ),
        AntipatternKind::ReadWithoutLocate => (
            "`touring index find` or Grep/Glob before Read",
            "Blind reads skip symbol discovery. Locating first gives the exact line range \
             and avoids reading the wrong file.",
            "touring index find \"<symbol>\" -j  # then Read(<file>, offset=<line>)",
        ),
        AntipatternKind::BashAwk => (
            "`touring ast grep --rewrite`",
            "`awk` in-place transforms bypass AST-aware edits and wiring validation. \
             `Edit tool` applies the change through the full 9-stage gate.",
            "touring ast grep --rewrite --path <file> --pattern \"old\" --replacement \"new\"",
        ),
        AntipatternKind::BashSedInplace => (
            "`touring ast grep --rewrite`",
            "`sed -i` bypasses AST-aware edits and wiring validation. \
             `Edit tool` applies the change atomically with rollback support.",
            "touring ast grep --rewrite --path <file> --pattern \"old\" --replacement \"new\"",
        ),
        AntipatternKind::ClaudeCliInBash => (
            "Touring CLI + deterministic scripts",
            "Nesting `claude -p` / `run_loop` inside Bash can exhaust API credits and \
             creates LLM-in-LLM loops. Use `touring` CLI for all intelligence queries.",
            "touring tantivy search \"<query>\"  # deterministic, <10 ms",
        ),
        AntipatternKind::BashPcre2Default => (
            "default linear-regex engine (no -P/--pcre2 flag)",
            "PCRE2 reintroduces backtracking and potential ReDoS; \
             the default linear engine covers the vast majority of patterns.",
            "grep '<pattern>' <file>  # omit -P; add only when lookaround/backref is required",
        ),
        AntipatternKind::SerialReadNoFanout => (
            "parallel fan-out Read block",
            "Serial Reads round-trip one file at a time; a parallel fan-out block \
             (Glob + concurrent Reads) achieves N× throughput at the same latency.",
            "Glob(\"crates/**/*.rs\")  # then Read each result in a parallel batch",
        ),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
#[cfg(test)]
mod tests {
    use super::*;

    // ── Verdict is always Warn (R13/R14) ─────────────────────────────────────

    #[test]
    fn conversion_verdict_is_always_warn_never_deny() {
        for kind in AntipatternKind::all() {
            let advice = conversion_for(*kind);
            assert_eq!(
                advice.verdict,
                Verdict::Warn,
                "P8.4 R13/R14: antipattern {:?} must never produce Deny, only Warn",
                kind
            );
        }
    }

    // ── All antipatterns covered ──────────────────────────────────────────────

    #[test]
    fn all_antipatterns_have_non_empty_elite_tool() {
        for kind in AntipatternKind::all() {
            let advice = conversion_for(*kind);
            assert!(
                !advice.elite_tool.is_empty(),
                "antipattern {:?} must name an elite tool",
                kind
            );
        }
    }

    #[test]
    fn all_antipatterns_have_non_empty_rationale() {
        for kind in AntipatternKind::all() {
            let advice = conversion_for(*kind);
            assert!(
                !advice.rationale.is_empty(),
                "antipattern {:?} must have a rationale",
                kind
            );
        }
    }

    #[test]
    fn all_antipatterns_have_non_empty_example() {
        for kind in AntipatternKind::all() {
            let advice = conversion_for(*kind);
            assert!(
                !advice.example.is_empty(),
                "antipattern {:?} must have a command example",
                kind
            );
        }
    }

    // ── Specific mappings (BashGrepRaw, BashCatHeadTail, BashFind) ───────────

    #[test]
    fn bash_grep_raw_maps_to_grep_tool_or_tantivy() {
        let advice = conversion_for(AntipatternKind::BashGrepRaw);
        assert!(
            advice.elite_tool.contains("Grep") || advice.elite_tool.contains("tantivy"),
            "BashGrepRaw elite_tool should name Grep or tantivy: {}",
            advice.elite_tool
        );
    }

    #[test]
    fn bash_cat_head_tail_maps_to_read_tool() {
        let advice = conversion_for(AntipatternKind::BashCatHeadTail);
        assert!(
            advice.elite_tool.contains("Read"),
            "BashCatHeadTail elite_tool should name the Read tool: {}",
            advice.elite_tool
        );
    }

    #[test]
    fn bash_find_maps_to_glob_or_touring_index_files() {
        let advice = conversion_for(AntipatternKind::BashFind);
        assert!(
            advice.elite_tool.contains("Glob") || advice.elite_tool.contains("touring"),
            "BashFind elite_tool should name Glob or touring index files: {}",
            advice.elite_tool
        );
    }

    // ── as_hint format ────────────────────────────────────────────────────────

    #[test]
    fn as_hint_contains_antipattern_label() {
        let advice = conversion_for(AntipatternKind::BashGrepRaw);
        let hint = advice.as_hint();
        assert!(
            hint.contains("bash-grep-raw"),
            "hint must name the antipattern label: {hint}"
        );
    }

    #[test]
    fn as_hint_contains_elite_tool_name() {
        let advice = conversion_for(AntipatternKind::BashCatHeadTail);
        let hint = advice.as_hint();
        assert!(
            hint.contains("Read"),
            "hint must name the elite tool: {hint}"
        );
    }

    #[test]
    fn as_hint_contains_example_snippet() {
        let advice = conversion_for(AntipatternKind::BashFind);
        let hint = advice.as_hint();
        assert!(
            hint.contains("Glob") || hint.contains("touring"),
            "hint must contain the example command: {hint}"
        );
    }

    // ── should_surface ────────────────────────────────────────────────────────

    #[test]
    fn should_surface_always_true() {
        for kind in AntipatternKind::all() {
            assert!(
                conversion_for(*kind).should_surface(),
                "all advices should surface by default"
            );
        }
    }

    // ── Serde roundtrip ───────────────────────────────────────────────────────

    #[test]
    fn advice_serde_roundtrip() {
        // ConversionAdvice has &'static str fields; deserializing back into the
        // struct requires 'static lifetime which serde_json::from_str cannot
        // provide.  Instead we validate that the serialized JSON is valid and
        // contains the expected keys/values using serde_json::Value.
        let advice = conversion_for(AntipatternKind::BashGrepRaw);
        let json = serde_json::to_string(&advice).expect("serialize");
        let value: serde_json::Value = serde_json::from_str(&json).expect("deserialize as Value");
        assert!(
            value.get("antipattern").is_some(),
            "must have antipattern field"
        );
        assert!(
            value.get("elite_tool").is_some(),
            "must have elite_tool field"
        );
        assert!(
            value.get("rationale").is_some(),
            "must have rationale field"
        );
        assert!(value.get("example").is_some(), "must have example field");
        assert!(value.get("verdict").is_some(), "must have verdict field");
        // Verify the roundtrip preserves the antipattern label
        let ap_str = value["antipattern"].as_str().unwrap_or("");
        assert!(
            !ap_str.is_empty(),
            "antipattern field must be non-empty string"
        );
    }
}
