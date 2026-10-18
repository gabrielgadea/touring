//! Transversal check `classe_css` (VCSS) — STUB: registered in `super::TRANSVERSAIS`, finds nothing yet.
//!
//! CSS classes used in html `class` attributes and in JSX `className` that no loaded sheet defines (`classe_css`, with "voce quis dizer"). It lives apart from the per-language adapters because it crosses them, which also lets
//! the VCSS phase run in parallel with the html, ts and tsx adapters without touching their
//! files. Replace [`ClasseCss::verificar`] (findings via
//! [`Finding::candidato`](crate::vgp::Finding::candidato); a `cdn` with a confirmed 404 is
//! [`Alvo::Externo`](crate::vgp::politica::Alvo::Externo), a pin or a CSS class is advisory by
//! policy) and delete the stub test. An empty answer is NOT an `allow` claim: the adapter's
//! verdict stands.

use super::Transversal;
use crate::vgp::{CheckInput, Finding};

/// The `classe_css` transversal check.
pub struct ClasseCss;

impl Transversal for ClasseCss {
    fn nome(&self) -> &'static str {
        "classe_css"
    }

    fn langs(&self) -> &'static [&'static str] {
        &["html", "tsx", "javascript"]
    }

    fn verificar(&self, _input: &CheckInput) -> Vec<Finding> {
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn stub_nao_inventa_achado() {
        let input = CheckInput {
            path: "/tmp/a.html".into(),
            content: "src=\"https://cdn.exemplo/x.js\"".into(),
            project_root: "/tmp".into(),
            disk: None,
            overlays: HashMap::new(),
            lang: "html".into(),
        };
        assert!(ClasseCss.verificar(&input).is_empty());
        assert!(!ClasseCss.langs().is_empty());
    }
}
