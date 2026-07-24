//! Stage **X2 STATIC** of the Code Execution Gateway. Phase **P3.3** of CEG
//! Pln2 (`docs/2026-05-17-ceg-pln2-plan.md`).
//!
//! X2 is the gateway's static-analysis gate. It inspects the code body — never
//! running it — for two classes of concern:
//!
//! 1. **Structural / destructive-command risk** — via the shared
//!    [`validate_command`] validator, whose rule set is the catalogue of
//!    destructive shell patterns (`rm -rf`, `find -delete`, `git push --force`,
//!    `git reset --hard`, `chmod -R 777`). It is *structural*: a destructive
//!    pattern inside a quoted string or a `#` comment does not fire.
//! 2. **Per-language risk patterns** — for the polyglot core five (Rust,
//!    Python, JavaScript, TypeScript, Go) the body is scanned with the
//!    `AstGrepRiskSignalLayer` engine ([`scan_source_cached`] +
//!    [`pattern_set_for`]) for language-specific risk patterns.
//!
//! The result — a [`StaticReport`] with a worst-case [`StaticSeverity`] — is
//! attached to the [`Evidence`](super::Evidence) ledger by
//! [`Execution::<Classified>::static_analyze`].

use super::typestate::{Analyzed, Classified, Execution};
use crate::gateway::sandbox_executor::SandboxLanguage;
use touring_hooks_shared::ast_grep_signal::{DEFAULT_BUDGET, format_matches, scan_source_cached};
use touring_hooks_shared::bash_ast_validator::{Verdict, validate_command};
use touring_hooks_shared::risk_patterns::pattern_set_for;
// S-13 (2026-06-06): the antipattern detector + WorkflowState live in the
// `touring-hooks-shared` leaf crate now — the gateway names them from the leaf
// instead of `crate::workflow`, which breaks the gateway → workflow forward edge
// (the last child→parent dependency blocking CEG extraction).
use serde::{Deserialize, Serialize};
use touring_code::polyglot::Lang;
use touring_hooks_shared::workflow::antipattern::{
    antipattern_finding, antipattern_severity, detect_antipattern,
};
use touring_hooks_shared::workflow::stage::WorkflowState;

// S-13 (2026-06-06): `StaticSeverity` (the X2 severity vocabulary) relocated to
// the `touring-hooks-shared` leaf crate so the gateway X2 stage and the workflow
// antipattern detector share it without a cycle. Re-exported here so every
// `crate::gateway::static_stage::StaticSeverity` call site (fast_path, vgp_stage,
// decision, staging_registry, this module) is unchanged.
pub use touring_hooks_shared::severity::StaticSeverity;

/// The X2 STATIC analysis result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StaticReport {
    /// The worst severity found across all checks.
    pub severity: StaticSeverity,
    /// Structural / risk findings — one human-readable line each.
    pub findings: Vec<String>,
    /// The per-language ast-grep risk summary (e.g. `"unwrap=3, panic=1"`),
    /// when the body's language is one of the polyglot core five and at least
    /// one risk pattern matched.
    pub risk_summary: Option<String>,
}

impl StaticReport {
    /// Run the X2 STATIC analysis over a code body.
    ///
    /// `source` is the verbatim code/command text; `language` is the language
    /// detected at X1 (`None` when it could not be inferred — only the
    /// structural check then runs).
    ///
    /// This is the backwards-compatible variant that uses an empty
    /// [`WorkflowState`] (no antipattern context). Prefer
    /// [`analyze_with_workflow`](Self::analyze_with_workflow) when a
    /// [`WorkflowState`] and `ActionSignature` are available.
    #[must_use]
    pub fn analyze(source: &str, language: Option<SandboxLanguage>) -> Self {
        use touring_hooks_shared::action_signature::{ActionSignature, ContextQualifier};
        let sig = ActionSignature {
            tool_class: String::new(),
            intent_class: String::new(),
            context_qualifier: ContextQualifier::Plain,
        };
        Self::analyze_with_workflow(source, language, &sig, &WorkflowState::default())
    }
    /// Run the X2 STATIC analysis with full workflow context.
    ///
    /// Extends [`analyze`](Self::analyze) with a P8.3 combination-antipattern
    /// check.  Detection is **advisory only**: the worst antipattern outcome is
    /// [`StaticSeverity::Warn`], never [`StaticSeverity::Block`] (R13/R14).
    ///
    /// `sig`   — the `ActionSignature` for the current tool call (X1 output).
    /// `state` — the [`WorkflowState`] sliding window maintained by the caller.
    #[must_use]
    pub fn analyze_with_workflow(
        source: &str,
        language: Option<SandboxLanguage>,
        sig: &touring_hooks_shared::action_signature::ActionSignature,
        state: &WorkflowState,
    ) -> Self {
        let mut findings: Vec<String> = Vec::new();
        let mut severity = StaticSeverity::Clear;
        match validate_command(source) {
            Verdict::Allow => {}
            Verdict::Warn { reason } => {
                severity = severity.max(StaticSeverity::Warn);
                findings.push(format!("structural: {reason}"));
            }
            Verdict::Block { reason } => {
                severity = severity.max(StaticSeverity::Block);
                findings.push(format!("structural: {reason}"));
            }
        }
        let risk_summary = language
            .and_then(sandbox_lang_to_polyglot)
            .and_then(pattern_set_for)
            .and_then(|pset| format_matches(&scan_source_cached(source, pset, DEFAULT_BUDGET)));
        if let Some(summary) = &risk_summary {
            severity = severity.max(StaticSeverity::Warn);
            findings.push(format!("risk: {summary}"));
        }
        let ap = detect_antipattern(sig, state);
        if let Some(sev) = antipattern_severity(ap.as_ref()) {
            severity = severity.max(sev);
        }
        if let Some(finding) = antipattern_finding(ap.as_ref()) {
            findings.push(finding);
        }
        Self {
            severity,
            findings,
            risk_summary,
        }
    }
}

/// Map a [`SandboxLanguage`] to the `touring_code::polyglot` [`Lang`] used by the
/// risk pattern sets.
///
/// Returns `None` for languages outside the polyglot core five —
/// `validate_command` covers the shell risk surface, and the other scripting
/// languages have no ast-grep risk set. The match is exhaustive so a new
/// `SandboxLanguage` variant forces a decision here rather than silently
/// falling through.
fn sandbox_lang_to_polyglot(lang: SandboxLanguage) -> Option<Lang> {
    match lang {
        SandboxLanguage::Rust => Some(Lang::Rust),
        SandboxLanguage::Python => Some(Lang::Python),
        SandboxLanguage::JavaScript => Some(Lang::JavaScript),
        SandboxLanguage::TypeScript => Some(Lang::TypeScript),
        SandboxLanguage::Go => Some(Lang::Go),
        SandboxLanguage::Ruby
        | SandboxLanguage::Php
        | SandboxLanguage::Perl
        | SandboxLanguage::R
        | SandboxLanguage::Elixir
        | SandboxLanguage::Shell => None,
    }
}

impl Execution<Classified> {
    /// **X2 STATIC** — analyse the code body for structural and per-language
    /// risk, attach the [`StaticReport`] to the evidence ledger, and advance
    /// to [`Analyzed`].
    ///
    /// The body analysed is `raw().payload`, so this transition is sound even
    /// for an execution that reached [`Classified`] via bare
    /// [`advance`](Execution::advance): a missing X1 classification only means
    /// the language-aware risk scan is skipped.
    pub fn static_analyze(mut self) -> Execution<Analyzed> {
        let language = self
            .evidence()
            .classification
            .as_ref()
            .and_then(|c| c.code_body.language);
        let report = StaticReport::analyze(self.raw().payload.as_str(), language);
        self.evidence_mut().static_report = Some(report);
        self.advance()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway::capture_tool_call;
    #[test]
    fn static_severity_is_ordered() {
        assert!(StaticSeverity::Clear < StaticSeverity::Warn);
        assert!(StaticSeverity::Warn < StaticSeverity::Block);
    }
    #[test]
    fn analyze_clean_command_is_clear() {
        let report = StaticReport::analyze("ls -la", Some(SandboxLanguage::Shell));
        assert_eq!(report.severity, StaticSeverity::Clear);
        assert!(report.findings.is_empty());
    }
    #[test]
    fn analyze_recursive_force_delete_is_block() {
        let report = StaticReport::analyze("rm -rf /tmp/x", Some(SandboxLanguage::Shell));
        assert_eq!(report.severity, StaticSeverity::Block);
        assert!(report.findings.iter().any(|f| f.starts_with("structural:")));
    }
    #[test]
    fn analyze_force_push_is_warn() {
        let report =
            StaticReport::analyze("git push --force origin main", Some(SandboxLanguage::Shell));
        assert_eq!(report.severity, StaticSeverity::Warn);
    }
    #[test]
    fn analyze_destructive_pattern_in_string_literal_is_not_flagged() {
        let report = StaticReport::analyze("echo \"rm -rf /\"", Some(SandboxLanguage::Shell));
        assert_eq!(report.severity, StaticSeverity::Clear);
    }
    #[test]
    fn analyze_rust_body_runs_the_risk_scan() {
        let rust = "fn f(o: Option<i32>) { let _ = o.unwrap(); unsafe {} panic!(); }";
        let report = StaticReport::analyze(rust, Some(SandboxLanguage::Rust));
        assert!(
            report.risk_summary.is_some(),
            "rust risk scan should produce a summary: {report:?}"
        );
        assert!(report.severity >= StaticSeverity::Warn);
    }
    #[test]
    fn analyze_unsupported_language_skips_the_risk_scan() {
        let report = StaticReport::analyze("puts 'hi'", Some(SandboxLanguage::Ruby));
        assert_eq!(report.risk_summary, None);
    }
    #[test]
    fn analyze_without_language_skips_the_risk_scan() {
        let report = StaticReport::analyze("some plain text", None);
        assert_eq!(report.risk_summary, None);
    }
    #[test]
    fn static_report_serde_roundtrip() {
        let report = StaticReport::analyze("rm -rf /x", Some(SandboxLanguage::Shell));
        let json = serde_json::to_string(&report).expect("serialize");
        let back: StaticReport = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(report, back);
    }
    #[test]
    fn static_analyze_transition_attaches_report_and_advances() {
        let analyzed = capture_tool_call("Bash", "rm -rf /tmp/x", None)
            .expect("Bash is code-bearing")
            .classify()
            .static_analyze();
        assert_eq!(analyzed.ordinal(), 2);
        assert_eq!(analyzed.stage(), "X2-STATIC");
        let report = analyzed
            .evidence()
            .static_report
            .as_ref()
            .expect("static_analyze must attach a StaticReport");
        assert_eq!(report.severity, StaticSeverity::Block);
    }
}
