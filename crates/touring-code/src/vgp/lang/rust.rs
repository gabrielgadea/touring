//! Rust adapter (VRS) — STUB: registered, answers `unknown` until its body is written.
//!
//! This file is the ONLY thing the VRS phase edits: replace [`Rust::verificar`] with the
//! real checks (build findings with [`Finding::candidato`](crate::vgp::Finding::candidato) and an
//! [`Alvo`](crate::vgp::politica::Alvo); never set a severity, `check` decides it) and delete the
//! stub test below. The registry line in `super::REGISTRO` already exists.
//!
//! Rust has a natural corpus (rustc errors); the classes are `nome`, `import`, `atributo` and `kwarg` (struct-literal field).

use super::{Adaptador, Verificacao};
use crate::vgp::CheckInput;

/// The `rust` adapter registered in `super::REGISTRO`.
pub struct Rust;

impl Adaptador for Rust {
    fn lang(&self) -> &'static str {
        "rust"
    }

    fn verificar(&self, _input: &CheckInput) -> Verificacao {
        Verificacao::nao_implementado("rust")
    }
}

#[cfg(test)]
mod tests {
    use crate::vgp::{CheckInput, check};
    use std::collections::HashMap;

    #[test]
    fn stub_responde_unknown_com_motivo_nunca_allow() {
        let out = check(&CheckInput {
            path: "/tmp/a.rs".into(),
            content: "qualquer coisa".into(),
            project_root: "/tmp".into(),
            disk: None,
            overlays: HashMap::new(),
            lang: "rust".into(),
        });
        assert_eq!(out.verdict, "unknown");
        assert!(out.findings.is_empty());
        assert_eq!(out.lang, "rust");
        assert_eq!(
            out.motivo.as_deref(),
            Some("adaptador rust ainda não implementado")
        );
    }
}
