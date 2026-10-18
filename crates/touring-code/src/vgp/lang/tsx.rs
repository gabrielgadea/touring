//! Tsx adapter (VTSX) — STUB: registered, answers `unknown` until its body is written.
//!
//! This file is the ONLY thing the VTSX phase edits: replace [`Tsx::verificar`] with the
//! real checks (build findings with [`Finding::candidato`](crate::vgp::Finding::candidato) and an
//! [`Alvo`](crate::vgp::politica::Alvo); never set a severity, `check` decides it) and delete the
//! stub test below. The registry line in `super::REGISTRO` already exists.
//!
//! Parse with `super::parse("tsx", ...)` (`LANGUAGE_TSX`); adds the `componente` and `prop` classes on top of the TypeScript ones.

use super::{Adaptador, Verificacao};
use crate::vgp::CheckInput;

/// The `tsx` adapter registered in `super::REGISTRO`.
pub struct Tsx;

impl Adaptador for Tsx {
    fn lang(&self) -> &'static str {
        "tsx"
    }

    fn verificar(&self, _input: &CheckInput) -> Verificacao {
        Verificacao::nao_implementado("tsx")
    }
}

#[cfg(test)]
mod tests {
    use crate::vgp::{CheckInput, check};
    use std::collections::HashMap;

    #[test]
    fn stub_responde_unknown_com_motivo_nunca_allow() {
        let out = check(&CheckInput {
            path: "/tmp/a.tsx".into(),
            content: "qualquer coisa".into(),
            project_root: "/tmp".into(),
            disk: None,
            overlays: HashMap::new(),
            lang: "tsx".into(),
        });
        assert_eq!(out.verdict, "unknown");
        assert!(out.findings.is_empty());
        assert_eq!(out.lang, "tsx");
        assert_eq!(
            out.motivo.as_deref(),
            Some("adaptador tsx ainda não implementado")
        );
    }
}
