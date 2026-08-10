//! F3.9 — API Documentation verifier (D35).
//!
//! **Real engine (default `workspace-integration`)**: delegates to
//! [`touring_analysis::quality::analyze_api_doc`] — a polyglot detector
//! of the canonical "API undocumented / OpenAPI missing / rustdoc Examples
//! missing" smell:
//!
//! | Detector | Signal | Lang |
//! |----------|--------|------|
//! | `no-doctest-on-pub` | file has ≥3 `pub fn`/`pub struct`/`pub enum`/`pub trait` but ZERO `# Examples` / `# Panics` / `# Errors` rustdoc section | Rust |
//! | `no-openapi-spec` | HTTP framework (reqwest/actix/axum/express/@hapi/fastapi/flask/django) in use without `openapi`/`swagger`/`utoipa`/`async-openapi`/`oasgen` reference | all |
//! | `no-utoipa-derive` | Rust handler (`fn handler` / `async fn` / `-> Response`) without `#[utoipa::path]` or `#[derive(ToSchema)]` AND no OpenAPI ref | Rust |
//! | `no-error-schema` | openapi/swagger/utoipa code present but no `Response` / `Error` / `4xx` / `5xx` schema mention (errors undocumented) | all |
//! | `no-md-api-heading` | Markdown doc (>200 bytes) without `# API` / `# Endpoints` / `# Reference` H2 heading | Markdown |
//!
//! Disjoint from F3.8 inline doc (F3.8 keys on `///` proximity on `pub` items;
//! F3.9 keys on **`# Examples` doctest** presence + OpenAPI schema coverage),
//! F3.10 arch doc (F3.10 keys on MADR + Mermaid; F3.9 keys on API-surface
//! doc), and F3.12 doc accuracy (F3.12 keys on doc drift; F3.9 keys on
//! doc *presence* for the API surface).
//!
//! **Sources (context7, `/openapitools/openapi-generator`, High reputation,
//! bench 90; rustdoc)**: OpenAPI 3.x is the gold standard for REST API
//! documentation — generates client SDKs, server stubs, mock and interactive
//! docs from a single spec. For Rust crates, the analog is
//! `#[utoipa::path]` derive (utoipa crate) which auto-generates OpenAPI from
//! handler signatures; doctest `# Examples` ensures the example actually
//! compiles.
//!
//! **Standalone fallback (`--no-default-features`)**: an `openapi`/`swagger`
//! /`/// API` density heuristic.
//!
//! **Scope**: per-file; rolls up as `AggKind::WeightedLoc`. ADVISORY-tier.

use crate::DimId;
use crate::verifications::Verification;
use anyhow::Result;
use std::path::Path;

/// F3.9 verifier — API Documentation.
#[allow(non_camel_case_types)]
pub struct F3_9_ApiDoc;

impl Verification for F3_9_ApiDoc {
    fn id(&self) -> DimId {
        DimId::F3_9
    }
    fn measure(&self, target: &Path) -> Result<(f32, String)> {
        analyze_api_doc_dim(target)
    }
}

// ── Real engine: API-documentation detector ─────────────────────────────────
#[cfg(feature = "workspace-integration")]
fn analyze_api_doc_dim(target: &Path) -> Result<(f32, String)> {
    use touring_analysis::quality::{analyze_api_doc, score_api_doc};
    if is_detector_own_source(target) {
        return Ok((
            1.0,
            "F3.9: detector own source (api_doc needle vocabulary embedded as data) — score=1.000"
                .to_string(),
        ));
    }
    let raw = crate::verifications::read_target_source(target)?;
    let lang = crate::verifications::lang_from_ext(target);
    let r = analyze_api_doc(&raw, lang);
    let value = score_api_doc(&r);
    let top = crate::verifications::top_finding(&r.findings);
    let evidence = format!(
        "F3.9: {} API-doc gap(s) over {} lines ({lang}) — score={value:.3} \
         (touring-analysis analyze_api_doc: no-doctest-on-pub / no-openapi-spec / \
         no-utoipa-derive / no-error-schema / no-md-api-heading){top}",
        r.violations, r.total_lines
    );
    Ok((value, evidence))
}

/// The engine and this verifier embed the api-doc needle vocabulary as
/// detection data, so scoring their own source is a self-match false positive.
#[cfg(feature = "workspace-integration")]
fn is_detector_own_source(target: &Path) -> bool {
    crate::verifications::is_detector_own_source(target)
}

// ── Standalone fallback: openapi/swagger density heuristic ───────────────────
#[cfg(not(feature = "workspace-integration"))]
fn analyze_api_doc_dim(target: &Path) -> Result<(f32, String)> {
    let raw = crate::verifications::read_target_source(target)?;
    let lines = raw.lines().count().max(1) as f32;
    let api = raw.matches("openapi").count()
        + raw.matches("swagger").count()
        + raw.matches("/// API").count();
    let value = if api > 0 { 1.0 } else { 0.2 };
    let evidence = format!(
        "{api} openapi/swagger/doc-API reference(s) over {lines:.0} lines \
         (heuristic; build --features workspace-integration for full API-doc analysis)"
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
    fn test_api_doc_returns_valid_score() {
        let f = write_temp_ext("fn example() {}\n", ".rs");
        let s = F3_9_ApiDoc.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "score out of range: {}",
            s.value
        );
    }
    #[test]
    fn test_api_doc_empty_file() {
        let f = write_temp("");
        let s = F3_9_ApiDoc.check(f.path()).expect("check");
        assert!((0.0..=1.0).contains(&s.value));
    }
    /// `pub fn` items WITH `# Examples` doctest section score higher than
    /// items WITHOUT (surface documented).
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_documented_pub_scores_higher_than_undocumented() {
        let bad = write_temp_ext(
            "/// Just docs.\npub fn a() -> i32 { 1 }\n\
             /// Just docs.\npub fn b() -> i32 { 2 }\n\
             /// Just docs.\npub fn c() -> i32 { 3 }\n\
             /// Just docs.\npub fn d() -> i32 { 4 }\n",
            ".rs",
        );
        let good = write_temp_ext(
            "/// Computes 2+2.\n///\n/// # Examples\n/// ```\n/// assert_eq!(answer(), 4);\n/// ```\npub fn answer() -> i32 { 4 }\n\
             /// # Examples\n/// ```\n/// assert_eq!(2+2, 4);\n/// ```\npub fn add(a: i32, b: i32) -> i32 { a + b }\n",
            ".rs",
        );
        let sb = F3_9_ApiDoc.check(bad.path()).expect("check");
        let sg = F3_9_ApiDoc.check(good.path()).expect("check");
        assert!(
            sg.value > sb.value,
            "documented file ({}) must score above undocumented ({})",
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
            "/x/touring-analysis/src/quality/api_doc.rs"
        )));
        assert!(is_detector_own_source(Path::new(
            "/x/crates/foo/tests/bar.rs"
        )));
        assert!(!is_detector_own_source(Path::new(
            "/x/crates/touring-server/src/main.rs"
        )));
    }
}
