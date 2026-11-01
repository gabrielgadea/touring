//! Code-mode presentation — the scope-level declaration of HOW tools reach the
//! model, shared by every enforcement point.
//!
//! This is the `presentAs` of the DeepSeek harness
//! (`packages/core/agent-tool-presentation`) adapted to what we control. Two
//! surfaces enforce it and they MUST agree, so the type and the parser live
//! here rather than in either one:
//!
//! * the `PreToolUse` hook (`touring-cli::cli_suggester`) — collapses the
//!   inspection classes measured to fan out;
//! * the MCP handshake (`touring-server::server::apply_curation`) — shrinks the
//!   advertised tool surface to the search+execute façade.
//!
//! Duplicating the parser in both would recreate the failure this workspace has
//! already paid for twice: a rule whose sites drift apart (five call sites
//! writing the same edge, 2026-08-23) and a declaration that promises what the
//! executor does not apply (finding D8, 2026-08-25). One definition, two
//! consumers.

use std::path::Path;

/// How tools are presented to the model in THIS scope.
///
/// The collapse lives in the executor, never in the announcement: the DeepSeek
/// postmortem of 2026-08-07 is explicit that *"schema omission is not
/// enforcement when a direct caller can bypass it; denial must be tested
/// through the executor"*.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodeModePresentation {
    /// No collapse and no induction: atomic inspection passes silently.
    Native,
    /// Collapse by class: inspection measured to fan out is DENIED carrying the
    /// derived route; mutation, build and single-call classes pass. The MCP
    /// handshake narrows to the search+execute façade.
    Code,
    /// The default: both surfaces coexist, with induction. Forcing every scope
    /// into `Code` is the refusal DeepSeek documented — *"forcing every edit
    /// through a program taxes the common case"*.
    Both,
}

impl CodeModePresentation {
    /// Parse the declared value. `None` for anything we do not recognise, so an
    /// unknown value falls through to the caller's default instead of silently
    /// picking a collapse nobody asked for.
    pub fn parse(value: &str) -> Option<Self> {
        match value.trim().trim_matches(|c| c == '"' || c == '\'') {
            "native" => Some(Self::Native),
            "code" => Some(Self::Code),
            "both" => Some(Self::Both),
            _ => None,
        }
    }

    /// Stable label for telemetry and messages.
    pub fn label(self) -> &'static str {
        match self {
            Self::Native => "native",
            Self::Code => "code",
            Self::Both => "both",
        }
    }
}

/// Read `[code_mode] mode` from `<root>/.touring/touring.toml`.
///
/// A line scan with section state, **not** a TOML parser — neither crate on the
/// consuming side depends on `toml`, and pulling the whole dependency for one
/// key would be disproportionate. It understands exactly the form we write:
///
/// ```toml
/// [code_mode]
/// mode = "code"
/// ```
///
/// Any other shape (nesting, inline table, unquoted value) returns `None` and
/// falls back to today's behaviour — failing to the status quo, never to a
/// collapse nobody asked for.
pub fn project_presentation(project_root: &Path) -> Option<CodeModePresentation> {
    let texto = std::fs::read_to_string(project_root.join(".touring/touring.toml")).ok()?;
    parse_declaration(&texto)
}

/// The parser behind [`project_presentation`], split out so it is testable
/// without touching the filesystem.
fn parse_declaration(texto: &str) -> Option<CodeModePresentation> {
    let mut na_secao = false;
    for linha in texto.lines() {
        let l = linha.trim();
        if l.starts_with('[') {
            na_secao = l == "[code_mode]";
            continue;
        }
        if !na_secao {
            continue;
        }
        let (chave, valor) = l.split_once('=')?;
        if chave.trim() != "mode" {
            continue;
        }
        return CodeModePresentation::parse(valor);
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_each_declared_mode() {
        for (decl, esperado) in [
            ("native", CodeModePresentation::Native),
            ("code", CodeModePresentation::Code),
            ("both", CodeModePresentation::Both),
        ] {
            let texto = format!("[code_mode]\nmode = \"{decl}\"\n");
            assert_eq!(parse_declaration(&texto), Some(esperado), "decl={decl}");
        }
    }

    #[test]
    fn unknown_value_falls_through_to_none() {
        assert_eq!(parse_declaration("[code_mode]\nmode = \"turbo\"\n"), None);
    }

    #[test]
    fn only_the_code_mode_section_decides() {
        let texto = "[outra]\nmode = \"code\"\n";
        assert_eq!(parse_declaration(texto), None, "só [code_mode] decide");
    }

    #[test]
    fn absent_file_is_none_not_a_collapse() {
        let tmp = std::env::temp_dir().join("touring-code-mode-absent-probe");
        assert_eq!(project_presentation(&tmp), None);
    }

    #[test]
    fn label_round_trips_through_parse() {
        for m in [
            CodeModePresentation::Native,
            CodeModePresentation::Code,
            CodeModePresentation::Both,
        ] {
            assert_eq!(CodeModePresentation::parse(m.label()), Some(m));
        }
    }
}
