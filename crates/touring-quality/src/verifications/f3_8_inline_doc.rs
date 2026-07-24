//! F3.8 — Inline Documentation verifier (D34).
//!
//! **Real engine (default `workspace-integration`)**: delegates to
//! [`touring_analysis::quality::analyze_inline_doc`] — a polyglot detector of
//! the canonical "missing doc comment on `pub` item" smell that `rustdoc`
//! lints via `#[warn(missing_docs)]`:
//!
//! | Detector | Signal | Lang |
//! |----------|--------|------|
//! | `pub-undocumented` | a `pub fn`/`pub struct`/`pub enum`/`pub trait`/... whose preceding non-blank line is **not** `///` / `//!` | Rust |
//! | `pub-missing-examples` | a documented `pub fn` without a `# Examples` section in its doc block | Rust |
//! | `jsdoc-undocumented` | an `export function` / `export class` / `export interface` / `export const` whose preceding non-blank line is **not** `/** */` | JS/TS |
//! | `python-docstring-missing` | a module-level `def` / `class` (no leading `_`) whose next line is not a triple-quoted docstring | Python |
//!
//! Disjoint from F3.9 API doc (which keys on `cargo doc --no-deps` / OpenAPI
//! specs — F3.8 keys on **per-pub-item** `///` proximity) and F3.10 arch doc
//! (which keys on `docs/adr/` + ` ```mermaid ` — F3.8 keys on source code).
//!
//! **Sources (context7, `/rust-lang/rust`, High reputation)**: `#![deny(missing_docs)]`
//! is the gold-standard enforcement (`rustdoc/src/write-documentation/what-to-include.md`).
//!
//! **Standalone fallback (`--no-default-features`)**: a labelled
//! `/// count / fn count` density heuristic.
//!
//! **Scope**: per-file; rolls up as `AggKind::WeightedLoc`. ADVISORY-tier.

use crate::verifications::Verification;
use crate::{DimId, DimScore};
use anyhow::Result;
use std::path::Path;

/// F3.8 verifier — Inline Documentation.
#[allow(non_camel_case_types)]
pub struct F3_8_InlineDoc;

impl Verification for F3_8_InlineDoc {
    fn id(&self) -> DimId {
        DimId::F3_8
    }
    fn check(&self, target: &Path) -> Result<DimScore> {
        let (value, evidence) = analyze_inline_doc_dim(target)?;
        Ok(crate::verifications::finish(
            self.id(),
            value,
            evidence,
            target,
        ))
    }
}

// ── Real engine: inline-documentation detector ─────────────────────────────
#[cfg(feature = "workspace-integration")]
fn analyze_inline_doc_dim(target: &Path) -> Result<(f32, String)> {
    use touring_analysis::quality::{analyze_inline_doc, score_inline_doc};
    if is_detector_own_source(target) {
        return Ok((
            1.0,
            "F3.8: detector own source (inline_doc needle vocabulary embedded as data) — score=1.000"
                .to_string(),
        ));
    }
    let raw = crate::verifications::read_target_source(target)?;
    let lang = crate::verifications::lang_from_ext(target);
    let r = analyze_inline_doc(&raw, lang);
    let value = score_inline_doc(&r);
    let top = r
        .findings
        .first()
        .map(|(m, c)| format!("; top: {m} ({c}x)"))
        .unwrap_or_default();
    let evidence = format!(
        "F3.8: {} inline-documentation gap(s) over {} lines ({lang}) — score={value:.3} \
         (touring-analysis analyze_inline_doc: pub-undocumented / pub-missing-examples / \
         jsdoc-undocumented / python-docstring-missing){top}",
        r.violations, r.total_lines
    );
    Ok((value, evidence))
}

/// The engine and this verifier embed the doc-detection needle vocabulary as
/// detection data, so scoring their own source is a self-match false
/// positive. Mirrors `f2_8_memory::is_detector_own_source`.
#[cfg(feature = "workspace-integration")]
fn is_detector_own_source(target: &Path) -> bool {
    crate::verifications::is_detector_own_source(target)
}

// ── Standalone fallback: ///-density heuristic ──────────────────────────────
#[cfg(not(feature = "workspace-integration"))]
fn analyze_inline_doc_dim(target: &Path) -> Result<(f32, String)> {
    let raw = crate::verifications::read_target_source(target)?;
    let lines = raw.lines().count().max(1) as f32;
    let doc_lines = raw.matches("///").count() + raw.matches("//!").count();
    let pub_fns = raw.matches("pub fn ").count()
        + raw.matches("pub async fn ").count()
        + raw.matches("pub struct ").count();
    let ratio = if pub_fns > 0 {
        (doc_lines as f32 / pub_fns as f32).min(1.0)
    } else {
        1.0
    };
    let value = ratio;
    let evidence = format!(
        "{doc_lines} doc-comment lines / {pub_fns} pub items over {lines:.0} lines \
         (heuristic; build --features workspace-integration for full inline-doc analysis)"
    );
    Ok((value, evidence))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    fn write_temp_ext(content: &str, suffix: &str) -> NamedTempFile {
        let mut f = tempfile::Builder::new()
            .suffix(suffix)
            .tempfile()
            .expect("create temp");
        f.write_all(content.as_bytes()).expect("write");
        f
    }

    fn write_temp(content: &str) -> NamedTempFile {
        let mut f = NamedTempFile::new().expect("create temp");
        f.write_all(content.as_bytes()).expect("write");
        f
    }

    #[test]
    fn test_inline_doc_returns_valid_score() {
        let f = write_temp_ext("fn example() {}\n", ".rs");
        let s = F3_8_InlineDoc.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "score out of range: {}",
            s.value
        );
    }

    #[test]
    fn test_inline_doc_empty_file() {
        let f = write_temp("");
        let s = F3_8_InlineDoc.check(f.path()).expect("check");
        assert!((0.0..=1.0).contains(&s.value));
    }

    /// Documented pub fn scores higher than undocumented.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_documented_scores_higher_than_undocumented() {
        let bad = write_temp_ext("pub fn a() {}\npub fn b() {}\n", ".rs");
        let good = write_temp_ext("/// a.\npub fn a() {}\n/// b.\npub fn b() {}\n", ".rs");
        let sb = F3_8_InlineDoc.check(bad.path()).expect("check");
        let sg = F3_8_InlineDoc.check(good.path()).expect("check");
        assert!(
            sg.value > sb.value,
            "documented ({}) must score above undocumented ({})",
            sg.value,
            sb.value
        );
    }

    /// The engine's own source (which embeds every needle as data) must be
    /// allowlisted, not self-matched.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_detector_own_source_allowlisted() {
        use std::path::Path;
        assert!(is_detector_own_source(Path::new(
            "/x/touring-analysis/src/quality/inline_doc.rs"
        )));
        assert!(is_detector_own_source(Path::new(
            "/x/crates/foo/tests/bar.rs"
        )));
        assert!(!is_detector_own_source(Path::new(
            "/x/crates/touring-server/src/main.rs"
        )));
    }
}
