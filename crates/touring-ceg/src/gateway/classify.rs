//! Stage **X1 CLASSIFY** of the Code Execution Gateway. Phase **P3.2** of CEG
//! Pln2 (`docs/2026-05-17-ceg-pln2-plan.md`).
//!
//! X1 takes a captured call and extracts the three facts the downstream stages
//! need: its execution [`surface`](Classification::surface), its
//! [`language`](CodeBody::language) and its verbatim
//! [`source`](CodeBody::source). The result — a [`Classification`] — is
//! attached to the [`Evidence`](super::Evidence) ledger by
//! [`Execution::<Captured>::classify`].

use super::capture::ExecSurface;
use super::typestate::{Captured, Classified, Execution, RawInvocation};
use crate::gateway::sandbox_executor::SandboxLanguage;
use serde::{Deserialize, Serialize};
use touring_hooks_shared::bash_ast_validator::command_shape;

/// The extracted, language-tagged code body of a captured call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeBody {
    /// The code or command text to be gated, verbatim.
    pub source: String,
    /// The detected sandbox language, when one could be determined. `None`
    /// when the language could not be inferred from the payload alone.
    pub language: Option<SandboxLanguage>,
    /// A stable cluster key for bash commands — the normalized
    /// `head subcommand` shape from [`command_shape`]. `None` for non-bash
    /// surfaces and for un-shapeable input (e.g. an empty command).
    pub shape: Option<String>,
}

/// The **X1 CLASSIFY** result: the [`ExecSurface`] plus the extracted
/// [`CodeBody`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Classification {
    /// Which execution surface the call uses.
    pub surface: ExecSurface,
    /// The extracted, language-tagged code body.
    pub code_body: CodeBody,
}

impl Classification {
    /// Derive the classification of a captured [`RawInvocation`] — the X1
    /// extraction logic: detect the surface, infer the language, and (for bash
    /// surfaces) compute the cluster shape.
    #[must_use]
    pub fn derive(raw: &RawInvocation) -> Self {
        let surface = ExecSurface::detect(&raw.tool);
        let language = detect_language(surface, &raw.payload);
        let shape = match surface {
            ExecSurface::BashCommand => command_shape(&raw.payload),
            _ => None,
        };
        Self {
            surface,
            code_body: CodeBody {
                source: raw.payload.clone(),
                language,
                shape,
            },
        }
    }
}

/// Determine the sandbox language of a captured payload.
///
/// A `Bash` command is unconditionally [`SandboxLanguage::Shell`] — that is
/// definitional, not a guess. For every other surface the payload *is* the
/// code, so the language is sniffed from it ([`sniff_language`]); a future
/// refinement will thread the caller-declared language of a `ctx_execute`
/// call through capture instead of relying on the sniff.
fn detect_language(surface: ExecSurface, payload: &str) -> Option<SandboxLanguage> {
    match surface {
        ExecSurface::BashCommand => Some(SandboxLanguage::Shell),
        _ => sniff_language(payload),
    }
}

/// Best-effort language detection from a code body.
///
/// Recognises a `#!` shebang line and a few unambiguous syntactic signals.
/// Deliberately conservative — it returns `None` rather than guess, so that a
/// wrong language is never recorded in the evidence ledger.
#[must_use]
pub fn sniff_language(source: &str) -> Option<SandboxLanguage> {
    let head = source.trim_start();

    // 1. Shebang line — the strongest signal.
    if let Some(rest) = head.strip_prefix("#!") {
        let shebang = rest.lines().next().unwrap_or_default();
        if shebang.contains("python") {
            return Some(SandboxLanguage::Python);
        }
        if shebang.contains("node") || shebang.contains("bun") {
            return Some(SandboxLanguage::JavaScript);
        }
        if shebang.contains("ruby") {
            return Some(SandboxLanguage::Ruby);
        }
        if shebang.contains("perl") {
            return Some(SandboxLanguage::Perl);
        }
        if shebang.contains("bash") || shebang.contains("/sh") {
            return Some(SandboxLanguage::Shell);
        }
    }

    // 2. Unambiguous syntactic markers.
    if head.contains("console.log(") || head.contains("=> {") {
        return Some(SandboxLanguage::JavaScript);
    }
    if head.contains("fn main(") {
        return Some(SandboxLanguage::Rust);
    }
    if head.contains("def ") && head.contains(':') {
        return Some(SandboxLanguage::Python);
    }

    None
}

impl Execution<Captured> {
    /// **X1 CLASSIFY** — extract the surface, language and code body, attach
    /// the [`Classification`] to the evidence ledger, and advance to
    /// [`Classified`].
    ///
    /// This is the evidence-carrying form of the X0→X1 transition; bare
    /// [`advance`](Execution::advance) performs the same typestate move
    /// without recording a classification.
    pub fn classify(mut self) -> Execution<Classified> {
        let classification = Classification::derive(self.raw());
        self.evidence_mut().classification = Some(classification);
        self.advance()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gateway::capture_tool_call;

    #[test]
    fn classify_bash_has_shell_language_and_shape() {
        let cls = Classification::derive(&RawInvocation::new("Bash", "cargo test --release"));
        assert_eq!(cls.surface, ExecSurface::BashCommand);
        assert_eq!(cls.code_body.language, Some(SandboxLanguage::Shell));
        assert_eq!(cls.code_body.shape.as_deref(), Some("cargo test"));
    }

    #[test]
    fn classify_ctx_execute_surface_has_no_bash_shape() {
        let cls = Classification::derive(&RawInvocation::new("ctx_execute", "print('hi')"));
        assert_eq!(cls.surface, ExecSurface::CtxExecute);
        assert_eq!(cls.code_body.shape, None);
    }

    #[test]
    fn code_body_preserves_source_verbatim() {
        let cls = Classification::derive(&RawInvocation::new("Bash", "echo  spaced   out"));
        assert_eq!(cls.code_body.source, "echo  spaced   out");
    }

    #[test]
    fn sniff_language_python_shebang() {
        assert_eq!(
            sniff_language("#!/usr/bin/env python3\nprint(1)"),
            Some(SandboxLanguage::Python)
        );
    }

    #[test]
    fn sniff_language_node_shebang() {
        assert_eq!(
            sniff_language("#!/usr/bin/env node\nconsole.log(1)"),
            Some(SandboxLanguage::JavaScript)
        );
    }

    #[test]
    fn sniff_language_bash_shebang() {
        assert_eq!(
            sniff_language("#!/bin/bash\necho hi"),
            Some(SandboxLanguage::Shell)
        );
    }

    #[test]
    fn sniff_language_syntactic_markers() {
        assert_eq!(sniff_language("fn main() {}"), Some(SandboxLanguage::Rust));
        assert_eq!(
            sniff_language("console.log('x')"),
            Some(SandboxLanguage::JavaScript)
        );
        assert_eq!(
            sniff_language("def greet():\n    pass"),
            Some(SandboxLanguage::Python)
        );
    }

    #[test]
    fn sniff_language_unknown_is_none() {
        assert_eq!(sniff_language("???not code???"), None);
        assert_eq!(sniff_language(""), None);
    }

    #[test]
    fn classify_transition_attaches_evidence_and_advances() {
        let captured =
            capture_tool_call("Bash", "git push origin main", None).expect("Bash is code-bearing");
        assert_eq!(captured.ordinal(), 0);
        let classified = captured.classify();
        assert_eq!(classified.ordinal(), 1);
        assert_eq!(classified.stage(), "X1-CLASSIFY");
        assert_eq!(classified.evidence().stage_log.len(), 2);
        let cls = classified
            .evidence()
            .classification
            .as_ref()
            .expect("classify() must attach a Classification");
        assert_eq!(cls.surface, ExecSurface::BashCommand);
        assert_eq!(cls.code_body.shape.as_deref(), Some("git push"));
    }

    #[test]
    fn classification_serde_roundtrip() {
        let cls = Classification::derive(&RawInvocation::new("Bash", "cargo build"));
        let json = serde_json::to_string(&cls).expect("serialize");
        let back: Classification = serde_json::from_str(&json).expect("deserialize");
        assert_eq!(cls, back);
    }

    // ── E2E: the X0 → X1 chain ────────────────────────────────────────────

    #[test]
    fn e2e_capture_then_classify_bash() {
        let classified = capture_tool_call("Bash", "cargo test --release", None)
            .expect("Bash admitted at X0")
            .classify();
        let cls = classified
            .evidence()
            .classification
            .as_ref()
            .expect("classified");
        assert_eq!(cls.surface, ExecSurface::BashCommand);
        assert_eq!(cls.code_body.language, Some(SandboxLanguage::Shell));
        assert_eq!(cls.code_body.shape.as_deref(), Some("cargo test"));
        assert_eq!(classified.ordinal(), 1);
    }

    #[test]
    fn e2e_capture_then_classify_ctx_execute_python() {
        let py = "#!/usr/bin/env python3\nprint('compute')";
        let classified = capture_tool_call("ctx_execute", py, Some("compute".to_owned()))
            .expect("ctx_execute admitted at X0")
            .classify();
        let cls = classified
            .evidence()
            .classification
            .as_ref()
            .expect("classified");
        assert_eq!(cls.surface, ExecSurface::CtxExecute);
        assert_eq!(cls.code_body.language, Some(SandboxLanguage::Python));
        assert_eq!(cls.code_body.shape, None);
        assert_eq!(classified.raw().intent.as_deref(), Some("compute"));
    }
}
