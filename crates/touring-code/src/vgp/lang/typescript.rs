//! TypeScript adapter (VTS) — STUB: registered, answers `unknown` until its body is written.
//!
//! This file is the ONLY thing the VTS phase edits: replace [`TypeScript::verificar`] with the
//! real checks (build findings with [`Finding::candidato`](crate::vgp::Finding::candidato) and an
//! [`Alvo`](crate::vgp::politica::Alvo); never set a severity, `check` decides it) and delete the
//! stub test below. The registry line in `super::REGISTRO` already exists.
//!
//! Parse with `super::parse("typescript", ...)` (`LANGUAGE_TYPESCRIPT`), NOT `ast::Lang::TypeScript` (TSX for everything, which breaks the `<T>expr` cast).

use super::{Adaptador, Verificacao};
use crate::vgp::CheckInput;

/// The `typescript` adapter registered in `super::REGISTRO`.
pub struct TypeScript;

impl Adaptador for TypeScript {
    fn lang(&self) -> &'static str {
        "typescript"
    }

    fn verificar(&self, _input: &CheckInput) -> Verificacao {
        Verificacao::nao_implementado("typescript")
    }
}

#[cfg(test)]
mod tests {
    use crate::vgp::{CheckInput, check};
    use std::collections::HashMap;

    #[test]
    fn stub_responde_unknown_com_motivo_nunca_allow() {
        let out = check(&CheckInput {
            path: "/tmp/a.ts".into(),
            content: "qualquer coisa".into(),
            project_root: "/tmp".into(),
            disk: None,
            overlays: HashMap::new(),
            lang: "typescript".into(),
        });
        assert_eq!(out.verdict, "unknown");
        assert!(out.findings.is_empty());
        assert_eq!(out.lang, "typescript");
        assert_eq!(
            out.motivo.as_deref(),
            Some("adaptador typescript ainda não implementado")
        );
    }
}
