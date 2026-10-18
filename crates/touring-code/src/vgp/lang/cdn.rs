//! Transversal check `cdn` (VCDN) — STUB: registered in `super::TRANSVERSAIS`, finds nothing yet.
//!
//! CDN version and file existence (`cdn`, `cdn_pin`) for the src and href attributes of an html document and for a JS or TS import of an https URL. It lives apart from the per-language adapters because it crosses them, which also lets
//! the VCDN phase run in parallel with the html, ts and tsx adapters without touching their
//! files. Replace [`Cdn::verificar`] (findings via
//! [`Finding::candidato`](crate::vgp::Finding::candidato); a `cdn` with a confirmed 404 is
//! [`Alvo::Externo`](crate::vgp::politica::Alvo::Externo), a pin or a CSS class is advisory by
//! policy) and delete the stub test. An empty answer is NOT an `allow` claim: the adapter's
//! verdict stands.

use super::Transversal;
use crate::vgp::{CheckInput, Finding};

/// The `cdn` transversal check.
pub struct Cdn;

impl Transversal for Cdn {
    fn nome(&self) -> &'static str {
        "cdn"
    }

    fn langs(&self) -> &'static [&'static str] {
        &["html", "javascript", "typescript", "tsx"]
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
        assert!(Cdn.verificar(&input).is_empty());
        assert!(!Cdn.langs().is_empty());
    }
}
