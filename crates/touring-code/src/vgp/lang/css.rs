//! Css adapter (VCSS) — STUB: registered, answers `unknown` until its body is written.
//!
//! This file is the ONLY thing the VCSS phase edits: replace [`Css::verificar`] with the
//! real checks (build findings with [`Finding::candidato`](crate::vgp::Finding::candidato) and an
//! [`Alvo`](crate::vgp::politica::Alvo); never set a severity, `check` decides it) and delete the
//! stub test below. The registry line in `super::REGISTRO` already exists.
//!
//! The sheet itself; the `classe_css` USAGE check (html and JSX against the loaded sheets) is the transversal `classe_css.rs`.

use super::{Adaptador, Verificacao};
use crate::vgp::CheckInput;

/// The `css` adapter registered in `super::REGISTRO`.
pub struct Css;

impl Adaptador for Css {
    fn lang(&self) -> &'static str {
        "css"
    }

    fn verificar(&self, _input: &CheckInput) -> Verificacao {
        Verificacao::nao_implementado("css")
    }
}

#[cfg(test)]
mod tests {
    use crate::vgp::{CheckInput, check};
    use std::collections::HashMap;

    #[test]
    fn stub_responde_unknown_com_motivo_nunca_allow() {
        let out = check(&CheckInput {
            path: "/tmp/a.css".into(),
            content: "qualquer coisa".into(),
            project_root: "/tmp".into(),
            disk: None,
            overlays: HashMap::new(),
            lang: "css".into(),
        });
        assert_eq!(out.verdict, "unknown");
        assert!(out.findings.is_empty());
        assert_eq!(out.lang, "css");
        assert_eq!(
            out.motivo.as_deref(),
            Some("adaptador css ainda não implementado")
        );
    }
}
