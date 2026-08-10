//! F3.12 — Documentation Accuracy verifier (D38).
//!
//! **Real engine (default `workspace-integration`)**: delegates to
//! [`touring_analysis::quality::analyze_doc_accuracy`] — a polyglot detector
//! of the canonical "doc drift / no executable examples" smell:
//!
//! | Detector | Signal | Lang |
//! |----------|--------|------|
//! | `no-doctest-enforcement` | no `#![deny(missing_docs)]` / `#![warn(missing_docs)]` at crate root in a file with `///` doc | Rust |
//! | `no-doctest-examples` | `///` doc comments present but no inline ` ``` ` (doctest) fence | Rust |
//! | `drift-marker-in-doc` | `TODO` / `FIXME` / `XXX` / `???` inside a `///` or `//!` doc comment | any |
//! | `no-code-blocks-in-md` | `.md` with no ` ```rust ` / ` ```toml ` / ` ```bash ` code block (>200 bytes) | `.md` |
//! | `drift-marker-in-md` | `TODO` / `FIXME` / `XXX` / `???` in Markdown body | `.md` |
//!
//! Disjoint from F3.8 inline doc (which keys on `///` proximity on
//! `pub` items — F3.12 keys on **whether the doc is actually executable /
//! drift-free**); F3.10 arch doc (MADR + Mermaid — F3.12 keys on drift
//! markers); F3.13 changelog (Keep-a-Changelog — F3.12 keys on placeholder
//! links in docs).
//!
//! **Sources (context7, `/vale-cli/vale`, Medium reputation)**: Vale is
//! the "prose-as-code" linter for documentation. It catches drift signals
//! and style/terminology consistency. The inline ` ``` ` doctest check
//! mirrors `cargo test --doc`: a doc-comment without an inline example
//! cannot fail at build time, so its claim is unverified.
//!
//! **Standalone fallback (`--no-default-features`)**: a labelled
//! ` ``` ` / `#![deny(missing_docs)]` density heuristic.
//!
//! **Scope**: per-file; rolls up as `AggKind::WeightedLoc`. ADVISORY-tier.

use crate::DimId;
use crate::verifications::Verification;
use anyhow::Result;
use std::path::Path;

/// F3.12 verifier — Documentation Accuracy.
#[allow(non_camel_case_types)]
pub struct F3_12_DocAccuracy;

impl Verification for F3_12_DocAccuracy {
    fn id(&self) -> DimId {
        DimId::F3_12
    }
    fn measure(&self, target: &Path) -> Result<(f32, String)> {
        analyze_doc_accuracy_dim(target)
    }
}

// ── Real engine: doc-accuracy detector ──────────────────────────────────────
#[cfg(feature = "workspace-integration")]
fn analyze_doc_accuracy_dim(target: &Path) -> Result<(f32, String)> {
    use touring_analysis::quality::{analyze_doc_accuracy, score_doc_accuracy};
    if is_detector_own_source(target) {
        return Ok((
            1.0,
            "F3.12: detector own source (doc_accuracy needle vocabulary embedded as data) — score=1.000"
                .to_string(),
        ));
    }
    let raw = crate::verifications::read_target_source(target)?;
    let lang = crate::verifications::lang_from_ext(target);
    let r = analyze_doc_accuracy(&raw, lang);
    let value = score_doc_accuracy(&r);
    let top = crate::verifications::top_finding(&r.findings);
    let evidence = format!(
        "F3.12: {} doc-accuracy gap(s) over {} lines ({lang}) — score={value:.3} \
         (touring-analysis analyze_doc_accuracy: no-doctest-enforcement / no-doctest-examples / \
         drift-marker-in-doc / no-code-blocks-in-md / drift-marker-in-md){top}",
        r.violations, r.total_lines
    );
    Ok((value, evidence))
}

/// The engine and this verifier embed the doc-accuracy needle vocabulary as
/// detection data, so scoring their own source is a self-match false
/// positive. Mirrors `f2_8_memory::is_detector_own_source`.
#[cfg(feature = "workspace-integration")]
fn is_detector_own_source(target: &Path) -> bool {
    crate::verifications::is_detector_own_source(target)
}

// ── Standalone fallback: doctest-density heuristic ──────────────────────────
#[cfg(not(feature = "workspace-integration"))]
fn analyze_doc_accuracy_dim(target: &Path) -> Result<(f32, String)> {
    let raw = crate::verifications::read_target_source(target)?;
    let lines = raw.lines().count().max(1) as f32;
    let doc_lines = raw.matches("///").count() + raw.matches("//!").count();
    let doctest_fences = raw.matches("```").count();
    let deny = raw.contains("#![deny(missing_docs)]") || raw.contains("#![warn(missing_docs)]");
    let doc_with_fence = doc_lines > 0 && doctest_fences > 0;
    let value = if doc_with_fence && deny {
        1.0
    } else if doc_with_fence || deny {
        0.5
    } else {
        0.0
    };
    let evidence = format!(
        "{doc_lines} doc-comment lines / {doctest_fences} ``` fences / lint={deny} over {lines:.0} lines \
         (heuristic; build --features workspace-integration for full doc-accuracy analysis)"
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
    fn test_doc_accuracy_returns_valid_score() {
        let f = write_temp_ext("fn example() {}\n", ".rs");
        let s = F3_12_DocAccuracy.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "score out of range: {}",
            s.value
        );
    }

    #[test]
    fn test_doc_accuracy_empty_file() {
        let f = write_temp("");
        let s = F3_12_DocAccuracy.check(f.path()).expect("check");
        assert!((0.0..=1.0).contains(&s.value));
    }

    /// Deny lint + doctest fences score higher than no lint, no fences.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_doctest_scores_higher_than_no_doctest() {
        let good = write_temp_ext(
            "#![deny(missing_docs)]\n/// Doc.\n/// ```\n/// assert!(true);\n/// ```\npub fn x() {}\n",
            ".rs",
        );
        let bad = write_temp_ext(
            "/// Just docs, no example, no lint.\npub fn y() {}\n",
            ".rs",
        );
        let sg = F3_12_DocAccuracy.check(good.path()).expect("check");
        let sb = F3_12_DocAccuracy.check(bad.path()).expect("check");
        assert!(
            sg.value > sb.value,
            "with deny + doctest ({}) must score above without ({})",
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
            "/x/touring-analysis/src/quality/doc_accuracy.rs"
        )));
        assert!(is_detector_own_source(Path::new(
            "/x/crates/foo/tests/bar.rs"
        )));
        assert!(!is_detector_own_source(Path::new(
            "/x/crates/touring-server/src/main.rs"
        )));
    }
}
