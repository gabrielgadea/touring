//! S2 (2026-09-01) — `SecretsSignalLayer`: the F2.4 hardcoded-secret detector
//! evaluated on the **proposed** content of a Write/Edit, before it reaches
//! the disk.
//!
//! The 50-dim P0 gate (F2.4) scores a file that already exists; a brand-new
//! file, or the `new_string` of an edit, was invisible to it until
//! `SignalContext` v2 carried the proposal. This layer runs the very same
//! detector (`touring_quality::verifications::f2_4_secrets::scan_text`), so
//! the hook and the gate cannot disagree. It never echoes the offending
//! value — only the file and the line — because the hook output lands in the
//! transcript.

use crate::shared::signal_pipeline::{SignalContext, SignalLayer};

/// F2.4 over the proposed content. Silent without a proposal, when the text
/// is clean, or when it carries the `touring-quality:allow-secrets` pragma.
pub struct SecretsSignalLayer;

impl SignalLayer for SecretsSignalLayer {
    fn name(&self) -> &'static str {
        "secrets_f2_4"
    }

    fn enrich(&self, ctx: &SignalContext<'_>) -> Vec<(f32, String)> {
        if ctx.proposed.is_none() {
            return Vec::new();
        }
        let text = ctx.analysable_text();
        if text.trim().is_empty() {
            return Vec::new();
        }
        let scan = touring_quality::verifications::f2_4_secrets::scan_text(text);
        if scan.allowlisted || !scan.strong {
            return Vec::new();
        }
        let at = scan
            .first_line
            .map(|line| format!(" (L{line})"))
            .unwrap_or_default();
        // 1.0: a P0 — it must outrank every other signal on the page.
        vec![(
            1.0,
            format!(
                "[secret] P0 F2.4: hardcoded secret in the proposed content of {}{at} — remove it \
                 before writing (environment variable / keyring); a sample fixture must carry the \
                 `touring-quality:allow-secrets` pragma",
                ctx.file_path
            ),
        )]
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::shared::signal_pipeline::ProposedChange;

    fn conn_string() -> String {
        // Assembled at runtime so this source file never carries the pattern.
        format!(
            "postgres://admin:{}@db.example.com:5432/app",
            ["s3cr3t", "P4ssw0rd"].concat()
        )
    }

    #[test]
    fn flags_a_secret_in_a_proposed_write_with_file_and_line_but_never_the_value() {
        let content = format!("fn main() {{\n    let url = \"{}\";\n}}\n", conn_string());
        let ctx = SignalContext::new("src/config.rs", "")
            .with_hook("pre_write")
            .with_tool_name("Write")
            .with_proposed(ProposedChange::Write { content: &content });
        let signals = SecretsSignalLayer.enrich(&ctx);
        assert_eq!(signals.len(), 1, "{signals:?}");
        let (score, text) = &signals[0];
        assert!((*score - 1.0).abs() < f32::EPSILON);
        assert!(text.contains("[secret] P0 F2.4") && text.contains("src/config.rs") && text.contains("(L2)"), "{text}");
        assert!(!text.contains("P4ssw0rd"), "the value must never be echoed: {text}");
    }

    #[test]
    fn flags_a_secret_in_a_proposed_edit_new_string() {
        let new_string = format!("let dsn = \"{}\";", conn_string());
        let ctx = SignalContext::new("src/db.rs", "")
            .with_hook("pre_edit")
            .with_tool_name("Edit")
            .with_proposed(ProposedChange::Edit {
                old_string: "let dsn = env(\"DSN\");",
                new_string: &new_string,
            });
        assert_eq!(SecretsSignalLayer.enrich(&ctx).len(), 1);
    }

    #[test]
    fn silent_for_clean_code_for_the_pragma_and_without_a_proposal() {
        let clean = SignalContext::new("src/lib.rs", "")
            .with_proposed(ProposedChange::Write {
                content: "pub fn add(a: i32, b: i32) -> i32 { a + b }\n",
            });
        assert!(SecretsSignalLayer.enrich(&clean).is_empty());
        let fixture = format!("// touring-quality:allow-secrets\nlet u = \"{}\";\n", conn_string());
        let allow = SignalContext::new("tests/fixtures/sample.rs", "")
            .with_proposed(ProposedChange::Write { content: &fixture });
        assert!(SecretsSignalLayer.enrich(&allow).is_empty());
        let on_disk_only = format!("let u = \"{}\";\n", conn_string());
        let read = SignalContext::new("src/x.rs", &on_disk_only).with_hook("pre_read");
        assert!(SecretsSignalLayer.enrich(&read).is_empty(), "no proposal ⇒ not this layer's job");
    }
}
