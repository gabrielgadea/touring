//! Html adapter (VHTML) — STUB: registered, answers `unknown` until its body is written.
//!
//! This file is the ONLY thing the VHTML phase edits: replace [`Html::verificar`] with the
//! real checks (build findings with [`Finding::candidato`](crate::vgp::Finding::candidato) and an
//! [`Alvo`](crate::vgp::politica::Alvo); never set a severity, `check` decides it) and delete the
//! stub test below. The registry line in `super::REGISTRO` already exists.
//!
//! Classes `recurso` and `ancora`; the script and style bodies are `raw_text` nodes the adapter re-parses with `javascript` and `css`.

use super::{Adaptador, Verificacao};
use crate::vgp::CheckInput;

/// The `html` adapter registered in `super::REGISTRO`.
pub struct Html;

impl Adaptador for Html {
    fn lang(&self) -> &'static str {
        "html"
    }

    fn verificar(&self, _input: &CheckInput) -> Verificacao {
        Verificacao::nao_implementado("html")
    }
}

#[cfg(test)]
mod tests {
    use crate::vgp::{CheckInput, check};
    use std::collections::HashMap;

    #[test]
    fn stub_responde_unknown_com_motivo_nunca_allow() {
        let out = check(&CheckInput {
            path: "/tmp/a.html".into(),
            content: "qualquer coisa".into(),
            project_root: "/tmp".into(),
            disk: None,
            overlays: HashMap::new(),
            lang: "html".into(),
        });
        assert_eq!(out.verdict, "unknown");
        assert!(out.findings.is_empty());
        assert_eq!(out.lang, "html");
        assert_eq!(
            out.motivo.as_deref(),
            Some("adaptador html ainda não implementado")
        );
    }
}
