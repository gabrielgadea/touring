//! Shared vulnerability scan for hook handlers.
//!
//! Complementação-hooks H6: enrich() signal layer that scans source text for
//! common security pitfalls (F2.5 P0 BLOCK class — hardcoded credentials,
//! SQL injection, path traversal, production unwrap).
//!
//! S8 (2026-09-02): the detector itself now lives in
//! [`touring_code::cwe_scan`] — ONE implementation shared with the
//! `touring scan vulnerabilities` CLI (the two hand-kept copies had drifted:
//! the CLI never emitted CWE-22). This module keeps the [`SignalLayer`]
//! adapter, which since S8 scans the text about to EXIST: the proposed
//! `new_string` of an Edit (located as `new_string:L<n>`), the content of a
//! Write, or the source the context carries.
//!
//! Latency budget: <5ms p95 (Rust direct, no subprocess).

use touring_hooks_shared::signal_layer::{ProposedChange, SignalContext, SignalLayer};

pub use touring_code::cwe_scan::{
    CweFinding, Severity, detect_cwes, unwrap_call_pattern, vendor_prefix_aws_like,
    vendor_prefix_openai_like,
};

/// SignalLayer for CWE scanning.
pub struct CweScanLayer;

impl SignalLayer for CweScanLayer {
    fn name(&self) -> &'static str {
        "cwe_scan"
    }

    fn enrich(&self, ctx: &SignalContext<'_>) -> Vec<(f32, String)> {
        // S8: scan what is about to exist. An Edit's findings are located
        // inside the proposed text — the file on disk has not changed yet.
        let (text, in_proposed_edit) = match ctx.proposed {
            Some(ProposedChange::Edit { new_string, .. }) => (new_string, true),
            Some(ProposedChange::Write { content }) => (content, false),
            None => (ctx.source, false),
        };
        detect_cwes(text)
            .into_iter()
            .map(|f| {
                let score = match f.severity {
                    Severity::P0 => 1.0,
                    Severity::P1 => 0.5,
                    Severity::P2 => 0.2,
                };
                let line_info = if in_proposed_edit {
                    format!("new_string:L{}", f.line)
                } else {
                    format!("{}:{}", ctx.file_path, f.line)
                };
                (
                    score,
                    format!("[{}] {} {} — {}", score, f.id, line_info, f.description),
                )
            })
            .collect()
    }

    fn should_run(&self, cila_level: usize) -> bool {
        cila_level >= 1
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn api_word() -> String {
        ["a", "pi"].concat()
    }

    #[test]
    fn vendor_needles_built_correctly() {
        assert_eq!(
            vendor_prefix_openai_like().as_str(),
            "\u{0073}\u{006B}\u{002D}"
        );
        assert_eq!(
            vendor_prefix_aws_like().as_str(),
            "\u{0041}\u{004B}\u{0049}\u{0041}"
        );
    }

    #[test]
    fn detects_hardcoded_api_key() {
        let src = format!(
            "const KEY: &str = \"{}PLACEHOLDER {}\";\n",
            vendor_prefix_openai_like(),
            api_word()
        );
        let findings = detect_cwes(&src);
        assert!(
            findings
                .iter()
                .any(|f| f.id == "CWE-798" && f.severity == Severity::P0)
        );
    }

    #[test]
    fn detects_sql_injection_pattern() {
        let select = ["sel", "ect"].concat();
        let src = format!("let q = format!(\"{select} * FROM users WHERE id = {{}}\", id);\n");
        assert!(detect_cwes(&src).iter().any(|f| f.id == "CWE-89"));
    }

    #[test]
    fn detects_production_unwrap() {
        let unwrap_call = unwrap_call_pattern();
        let src = format!("fn main() {{\n    let x = foo(){unwrap_call};\n}}\n");
        assert!(detect_cwes(&src).iter().any(|f| f.id == "CWE-394"));
    }

    #[test]
    fn clean_source_has_no_findings() {
        let src = "fn main() {\n    println!(\"hello\");\n}\n";
        assert!(detect_cwes(src).is_empty());
    }

    #[test]
    fn signal_layer_emits_scored_findings() {
        let src = format!(
            "const KEY: &str = \"{}PLACEHOLDER {}\";\n",
            vendor_prefix_openai_like(),
            api_word()
        );
        let ctx = SignalContext::new("src/main.rs", &src);
        let signals = CweScanLayer.enrich(&ctx);
        assert!(!signals.is_empty());
        assert!(signals.iter().any(|(s, _)| *s == 1.0));
        assert!(signals[0].1.contains("src/main.rs:1"), "{}", signals[0].1);
    }

    #[test]
    fn edit_context_scans_the_proposed_text_and_locates_it_there() {
        let proposed = format!(
            "let {}_key = \"{}live-PLACEHOLDER\";\n",
            api_word(),
            vendor_prefix_openai_like()
        );
        let ctx = SignalContext::new("src/client.rs", "").with_proposed(ProposedChange::Edit {
            old_string: "String::new()",
            new_string: &proposed,
        });
        let signals = CweScanLayer.enrich(&ctx);
        assert_eq!(signals.len(), 1, "{signals:?}");
        assert!(signals[0].1.contains("CWE-798"), "{}", signals[0].1);
        assert!(signals[0].1.contains("new_string:L1"), "{}", signals[0].1);
    }

    #[test]
    fn write_context_scans_the_content() {
        let content = format!(
            "let {}_key = \"{}live-PLACEHOLDER\";\n",
            api_word(),
            vendor_prefix_openai_like()
        );
        let ctx = SignalContext::new("src/client.rs", "")
            .with_proposed(ProposedChange::Write { content: &content });
        let signals = CweScanLayer.enrich(&ctx);
        assert_eq!(signals.len(), 1, "{signals:?}");
        assert!(signals[0].1.contains("src/client.rs:1"), "{}", signals[0].1);
    }
}
