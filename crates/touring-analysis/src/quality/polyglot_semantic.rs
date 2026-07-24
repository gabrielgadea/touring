//! Polyglot quality signals derived from `touring_code::ast::polyglot_semantic`.
//!
//! Bridges the deep tree-sitter semantic analysis (`PolyglotSemanticReport`)
//! into touring-analysis quality scores — the cross-language analog of
//! [`crate::quality::rust_semantic::RustQualitySignals`]. Only activates for
//! Python / TypeScript / JavaScript; other languages return `None`.
//!
//! ## Signals exposed
//!
//! - **Dynamic-escape density** — `eval`/`exec`/`getattr` (Python), `any` (TS);
//!   the cross-language analog of Rust's `unsafe` count.
//! - **Async surface** — async function count.
//! - **Generic abstraction** — type-parameter count.
//! - **Annotation coverage** — fraction of typed parameters.
//! - **Semantic complexity score** — `[0.0, 1.0]` composite.
//!
//! `health_score()` uses the SAME penalty shape as `RustQualitySignals` so the
//! `generate` VGP semantic gate (`min_semantic_score`) applies one consistent
//! bar across languages.

use serde::{Deserialize, Serialize};

use touring_code::ast::languages::Lang;
use touring_code::ast::polyglot_semantic::PolyglotSemanticReport;

/// Quality signals for a Python / TypeScript / JavaScript source file.
///
/// The cross-language analog of
/// [`crate::quality::rust_semantic::RustQualitySignals`].
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PolyglotQualitySignals {
    /// `"python"`, `"typescript"`, or `"javascript"`.
    pub language: String,
    /// Count of dynamic / type-safety escapes (`eval`/`exec`/`getattr`, `any`).
    /// The cross-language analog of `RustQualitySignals::unsafe_count`.
    pub dynamic_escape_count: usize,
    /// Count of `async` functions.
    pub async_count: usize,
    /// Count of generic type parameters.
    pub type_param_count: usize,
    /// Count of decorators.
    pub decorator_count: usize,
    /// Fraction of parameters carrying a type annotation, in `[0.0, 1.0]`.
    pub annotation_coverage: f32,
    /// Composite semantic complexity in `[0.0, 1.0]` (higher = more complex).
    pub semantic_complexity: f32,
    /// True when the file has no advanced semantic features.
    pub is_simple: bool,
    /// Heuristic: "needs-review" when a dynamic escape is present or
    /// `semantic_complexity > 0.6`.
    pub needs_review: bool,
}

impl PolyglotQualitySignals {
    /// Parse `source` under `lang` and derive quality signals.
    ///
    /// Returns `None` for unsupported languages (Rust/Go/Java/…) or parse
    /// failure — mirroring `RustQualitySignals::from_source`.
    #[must_use]
    pub fn from_source(lang: Lang, source: &str) -> Option<Self> {
        let report = PolyglotSemanticReport::from_source(lang, source).ok()?;
        Some(Self::from_report(&report))
    }

    /// Build signals from an already-parsed report.
    #[must_use]
    pub fn from_report(report: &PolyglotSemanticReport) -> Self {
        let semantic_complexity = report.semantic_complexity();
        Self {
            language: report.language.clone(),
            dynamic_escape_count: report.dynamic_escapes,
            async_count: report.async_fns,
            type_param_count: report.type_params,
            decorator_count: report.decorators,
            annotation_coverage: report.annotation_coverage(),
            semantic_complexity,
            is_simple: report.is_simple(),
            needs_review: report.dynamic_escapes > 0 || semantic_complexity > 0.6,
        }
    }

    /// Overall semantic health in `[0.0, 1.0]` (higher = healthier).
    ///
    /// Uses the SAME penalty shape as `RustQualitySignals::health_score`: start
    /// at `1.0`, penalize each dynamic escape 5% (capped 30%) — the analog of
    /// the unsafe penalty — then subtract `semantic_complexity * 0.30`.
    #[must_use]
    pub fn health_score(&self) -> f32 {
        let mut score = 1.0_f32;
        score -= (self.dynamic_escape_count as f32 * 0.05).min(0.30);
        score -= self.semantic_complexity * 0.30;
        score.clamp(0.0, 1.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn python_signals_flag_dynamic_escapes() {
        let src = "def run(x):\n    return eval(x)\n";
        let s = PolyglotQualitySignals::from_source(Lang::Python, src).unwrap();
        assert_eq!(s.language, "python");
        assert_eq!(s.dynamic_escape_count, 1);
        assert!(s.needs_review, "eval present → needs review");
        assert!(s.health_score() < 1.0);
    }

    #[test]
    fn clean_typed_python_is_healthy() {
        let src = "def add(a: int, b: int) -> int:\n    return a + b\n";
        let s = PolyglotQualitySignals::from_source(Lang::Python, src).unwrap();
        assert_eq!(s.dynamic_escape_count, 0);
        assert!((s.annotation_coverage - 1.0).abs() < 1e-6);
        assert!(s.health_score() > 0.9);
    }

    #[test]
    fn typescript_any_lowers_health() {
        let dirty = "function f(x: any): any { return x; }";
        let clean = "function g(x: number): number { return x + 1; }";
        let ds = PolyglotQualitySignals::from_source(Lang::TypeScript, dirty).unwrap();
        let cs = PolyglotQualitySignals::from_source(Lang::TypeScript, clean).unwrap();
        assert!(ds.dynamic_escape_count >= 1);
        assert_eq!(cs.dynamic_escape_count, 0);
        assert!(ds.health_score() < cs.health_score());
    }

    #[test]
    fn rust_returns_none() {
        assert!(PolyglotQualitySignals::from_source(Lang::Rust, "fn main() {}").is_none());
    }
}
