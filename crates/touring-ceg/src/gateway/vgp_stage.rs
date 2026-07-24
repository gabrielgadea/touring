//! Stage **X3 VGP** of the Code Execution Gateway. Phase **P3.3** of CEG Pln2
//! (`docs/2026-05-17-ceg-pln2-plan.md`).
//!
//! X3 is the Verified Generation Protocol gate. It extracts the identifier-like
//! symbol references from the code body and checks each against the touring
//! symbol index: a symbol the index cannot resolve is an *unverified
//! reference* — a yellow flag for code about to run against the workspace.
//!
//! The index lookup is supplied as a closure, so the typestate transition
//! stays pure and testable. The production wiring (P3.7) passes a closure
//! backed by `touring index find`; a degraded caller with no index may pass a
//! soft-pass closure. The result — a [`VgpReport`] — is attached to the
//! [`Evidence`](super::Evidence) ledger by
//! [`Execution::<Analyzed>::vgp_verify`].

use super::typestate::{Analyzed, Execution, Verified};
use serde::{Deserialize, Serialize};
use std::collections::BTreeSet;

/// The X3 VGP verification result.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VgpReport {
    /// Symbol references the index resolved.
    pub verified: Vec<String>,
    /// Symbol references the index could not resolve.
    pub unresolved: Vec<String>,
}

impl VgpReport {
    /// `true` when every checked reference resolved — including the vacuous
    /// case of no references to check.
    #[must_use]
    pub fn all_resolved(&self) -> bool {
        self.unresolved.is_empty()
    }

    /// The total number of symbol references checked.
    #[must_use]
    pub fn checked(&self) -> usize {
        self.verified.len() + self.unresolved.len()
    }
}

/// Extract candidate identifier tokens from a code body for VGP verification.
///
/// Keeps `[A-Za-z_][A-Za-z0-9_]*` runs of length ≥ 4, de-duplicated in
/// first-seen order, minus a small stop-list of ubiquitous keywords that are
/// never index symbols. Purely lexical — no language parse — so it is sound
/// for any surface.
#[must_use]
pub fn extract_symbols(source: &str) -> Vec<String> {
    const STOP: &[&str] = &[
        "true", "false", "null", "none", "self", "this", "else", "then", "return", "import",
        "export", "const", "function", "print", "echo", "while", "break", "continue",
    ];
    let mut seen: BTreeSet<String> = BTreeSet::new();
    let mut out: Vec<String> = Vec::new();
    for run in source.split(|c: char| !(c.is_ascii_alphanumeric() || c == '_')) {
        if run.len() < 4 {
            continue;
        }
        let ident_start = run
            .chars()
            .next()
            .is_some_and(|c| c.is_ascii_alphabetic() || c == '_');
        if !ident_start {
            continue; // a numeric run — not a symbol
        }
        if STOP.contains(&run.to_ascii_lowercase().as_str()) {
            continue;
        }
        if seen.insert(run.to_owned()) {
            out.push(run.to_owned());
        }
    }
    out
}

impl Execution<Analyzed> {
    /// **X3 VGP** — verify the code body's symbol references against the
    /// symbol index, attach the [`VgpReport`] to the evidence ledger, and
    /// advance to [`Verified`].
    ///
    /// `symbol_exists` answers "is this symbol in the index?". The production
    /// wiring backs it with `touring index find`; tests pass a mock; a
    /// degraded caller with no index may pass `|_| true` for a soft pass.
    pub fn vgp_verify<F>(mut self, symbol_exists: F) -> Execution<Verified>
    where
        F: Fn(&str) -> bool,
    {
        let mut verified: Vec<String> = Vec::new();
        let mut unresolved: Vec<String> = Vec::new();
        for symbol in extract_symbols(self.raw().payload.as_str()) {
            if symbol_exists(&symbol) {
                verified.push(symbol);
            } else {
                unresolved.push(symbol);
            }
        }
        self.evidence_mut().vgp_report = Some(VgpReport {
            verified,
            unresolved,
        });
        self.advance()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway::{StaticSeverity, capture_tool_call};

    #[test]
    fn extract_symbols_keeps_identifier_runs() {
        let syms = extract_symbols("cargo test --release workspace");
        assert!(syms.contains(&"cargo".to_owned()));
        assert!(syms.contains(&"release".to_owned()));
        assert!(syms.contains(&"workspace".to_owned()));
    }

    #[test]
    fn extract_symbols_dedups_in_first_seen_order() {
        let syms = extract_symbols("touring touring index touring");
        assert_eq!(syms, vec!["touring".to_owned(), "index".to_owned()]);
    }

    #[test]
    fn extract_symbols_skips_short_and_numeric_runs() {
        let syms = extract_symbols("ls -la 12345 ab");
        assert!(
            syms.is_empty(),
            "short + numeric runs must be skipped: {syms:?}"
        );
    }

    #[test]
    fn extract_symbols_skips_the_stop_list() {
        let syms = extract_symbols("return false import module");
        assert_eq!(syms, vec!["module".to_owned()]);
    }

    #[test]
    fn vgp_report_all_resolved_and_checked() {
        let report = VgpReport {
            verified: vec!["alpha".into(), "bravo".into()],
            unresolved: vec![],
        };
        assert!(report.all_resolved());
        assert_eq!(report.checked(), 2);
    }

    #[test]
    fn vgp_report_unresolved_is_not_all_resolved() {
        let report = VgpReport {
            verified: vec!["alpha".into()],
            unresolved: vec!["ghost".into()],
        };
        assert!(!report.all_resolved());
        assert_eq!(report.checked(), 2);
    }

    #[test]
    fn vgp_report_serde_roundtrip() {
        let report = VgpReport {
            verified: vec!["known".into()],
            unresolved: vec!["unknown".into()],
        };
        let json = serde_json::to_string(&report).expect("serialize");
        let back: VgpReport = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(report, back);
    }

    #[test]
    fn vgp_verify_soft_pass_resolves_everything() {
        let verified = capture_tool_call("Bash", "cargo build workspace", None)
            .expect("Bash is code-bearing")
            .classify()
            .static_analyze()
            .vgp_verify(|_| true);
        let report = verified.evidence().vgp_report.as_ref().expect("vgp report");
        assert!(report.all_resolved());
        assert!(report.checked() >= 2);
    }

    #[test]
    fn vgp_verify_transition_advances_to_verified() {
        let verified = capture_tool_call("Bash", "cargo build", None)
            .expect("Bash is code-bearing")
            .classify()
            .static_analyze()
            .vgp_verify(|_| true);
        assert_eq!(verified.ordinal(), 3);
        assert_eq!(verified.stage(), "X3-VGP");
        assert_eq!(verified.evidence().stage_log.len(), 4);
    }

    // ── E2E: the X0 → X3 chain ────────────────────────────────────────────

    #[test]
    fn e2e_clean_bash_flows_to_verified() {
        let verified = capture_tool_call("Bash", "cargo test --release", None)
            .expect("admitted at X0")
            .classify()
            .static_analyze()
            .vgp_verify(|_| true);
        assert_eq!(verified.ordinal(), 3);
        let static_report = verified
            .evidence()
            .static_report
            .as_ref()
            .expect("static report");
        assert_eq!(static_report.severity, StaticSeverity::Clear);
        assert!(
            verified
                .evidence()
                .vgp_report
                .as_ref()
                .expect("vgp report")
                .all_resolved()
        );
    }

    #[test]
    fn e2e_destructive_bash_records_block_severity() {
        let verified = capture_tool_call("Bash", "rm -rf /tmp/build", None)
            .expect("admitted at X0")
            .classify()
            .static_analyze()
            .vgp_verify(|_| true);
        let static_report = verified
            .evidence()
            .static_report
            .as_ref()
            .expect("static report");
        assert_eq!(static_report.severity, StaticSeverity::Block);
    }

    #[test]
    fn e2e_vgp_splits_known_and_unknown_symbols() {
        // A verifier that knows only "cargo" and "build".
        let known = ["cargo", "build"];
        let verified = capture_tool_call("Bash", "cargo build phantomtarget", None)
            .expect("admitted at X0")
            .classify()
            .static_analyze()
            .vgp_verify(|s| known.contains(&s));
        let report = verified.evidence().vgp_report.as_ref().expect("vgp report");
        assert!(report.verified.contains(&"cargo".to_owned()));
        assert!(report.verified.contains(&"build".to_owned()));
        assert!(report.unresolved.contains(&"phantomtarget".to_owned()));
        assert!(!report.all_resolved());
    }
}
