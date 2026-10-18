//! JavaScript adapter (VTS) — STUB: registered, answers `unknown` until its body is written.
//!
//! This file is the ONLY thing the VTS phase edits: replace [`JavaScript::verificar`] with the
//! real checks (build findings with [`Finding::candidato`](crate::vgp::Finding::candidato) and an
//! [`Alvo`](crate::vgp::politica::Alvo); never set a severity, `check` decides it) and delete the
//! stub test below. The registry line in `super::REGISTRO` already exists.
//!
//! The `javascript` grammar includes JSX; the inline script bodies of an html file are re-injected here by the html adapter.

use super::{Adaptador, Verificacao};
use crate::vgp::CheckInput;

/// The `javascript` adapter registered in `super::REGISTRO`.
pub struct JavaScript;

impl Adaptador for JavaScript {
    fn lang(&self) -> &'static str {
        "javascript"
    }

    fn verificar(&self, _input: &CheckInput) -> Verificacao {
        Verificacao::nao_implementado("javascript")
    }
}

#[cfg(test)]
mod tests {
    use crate::vgp::{CheckInput, check};
    use std::collections::HashMap;

    #[test]
    fn stub_responde_unknown_com_motivo_nunca_allow() {
        let out = check(&CheckInput {
            path: "/tmp/a.js".into(),
            content: "qualquer coisa".into(),
            project_root: "/tmp".into(),
            disk: None,
            overlays: HashMap::new(),
            lang: "javascript".into(),
        });
        assert_eq!(out.verdict, "unknown");
        assert!(out.findings.is_empty());
        assert_eq!(out.lang, "javascript");
        assert_eq!(
            out.motivo.as_deref(),
            Some("adaptador javascript ainda não implementado")
        );
    }
}
