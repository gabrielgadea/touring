//! API documentation analysis (D35 / F3.9) — polyglot detector of the canonical
//! "API undocumented / OpenAPI missing / rustdoc Examples missing" smell. The
//! API is as good as its documentation for consumers. Endpoints without
//! examples, errors not documented, schema not described — friction and misuse.
//!
//! | Smell | Signal | Lang |
//! |-------|--------|------|
//! | `no-doctest-on-pub` | file has `pub fn` / `pub struct` but ZERO `# Examples` doctest section | Rust |
//! | `no-openapi-spec` | HTTP-handling file (reqwest/actix/axum/express) but no `openapi` / `swagger` / `utoipa` / `async-openapi` reference | all |
//! | `no-utoipa-derive` | Rust HTTP handler (`fn handler(...)` returning `Response`) but no `#[utoipa::path]` / `#[derive(ToSchema)]` | Rust |
//! | `no-api-doc-section` | Markdown file (README) but no `# API` / `# Endpoints` / `# Reference` heading | Markdown |
//! | `no-error-schema` | OpenAPI-adjacent code (`openapi`/`swagger`/`async-openapi`) but no `Response` / `Error` schema mention | all |
//!
//! **Disjoint** from F3.8 inline doc (F3.8 keys on `///` proximity on `pub`
//! items; F3.9 keys on **`# Examples` doctest** presence + OpenAPI schema
//! coverage); F3.10 arch doc (F3.10 keys on MADR + Mermaid; F3.9 keys on
//! API-surface doc); F3.12 doc accuracy (F3.12 keys on doc drift; F3.9 keys
//! on doc *presence* for the API surface).
//!
//! **Sources (context7, `/openapitools/openapi-generator`, High reputation;
//! `/redocly/redoc`; rustdoc)**: OpenAPI 3.x is the gold standard for
//! REST API documentation — generates client SDKs, server stubs, mock and
//! interactive docs from a single spec. For Rust crates, the analog is
//! `#[utoipa::path]` derive (utoipa crate) which auto-generates OpenAPI from
//! handler signatures; doctest `# Examples` ensures the example actually
//! compiles.
//!
//! Comments / `#[cfg(test)]` are excluded via `super::code_regions`.

use memchr::memmem;

use super::code_regions::{non_executable_regions, offset_suppressed};
use super::score_utils::{count_executable_including_test_bodies, density_score};

/// Density→score scale (ADVISORY-tier).
const SCALE: f32 = 6.0;

/// Rust public-item needles.
const PUB_FN: &[u8] = b"pub fn ";
const PUB_STRUCT: &[u8] = b"pub struct ";
const PUB_ENUM: &[u8] = b"pub enum ";
const PUB_TRAIT: &[u8] = b"pub trait ";

/// Rust doctest section marker (rustdoc convention).
const EXAMPLES_SECTION: &[u8] = b"# Examples";
const EXAMPLES_SECTION_ALT: &[u8] = b"# Example";
const PANICS_SECTION: &[u8] = b"# Panics";
const ERRORS_SECTION: &[u8] = b"# Errors";

/// OpenAPI / swagger reference needles.
const OPENAPI: &[u8] = b"openapi";
const SWAGGER: &[u8] = b"swagger";
const UTOIPA: &[u8] = b"utoipa";
const ASYNC_OPENAPI: &[u8] = b"async-openapi";
const OASGEN: &[u8] = b"oasgen";
const SPECTRAL: &[u8] = b"spectral";

/// utoipa derive / attribute needles.
const UTOIPA_PATH: &[u8] = b"utoipa::path";
const UTOIPA_TOSCHEMA: &[u8] = b"ToSchema";
const UTOIPA_INTO_PARAMS: &[u8] = b"IntoParams";
const UTOIPA_INTO_RESPONSES: &[u8] = b"IntoResponses";

/// HTTP framework code signals.
const CODE_REQWEST: &[u8] = b"reqwest";
const CODE_ACTIX: &[u8] = b"actix";
const CODE_AXUM: &[u8] = b"axum";
const CODE_EXPRESS: &[u8] = b"express";
const CODE_FASTAPI: &[u8] = b"fastapi";
const CODE_FLASK: &[u8] = b"flask";
const CODE_DJANGO: &[u8] = b"django";
const CODE_HAPI: &[u8] = b"@hapi";

/// Handler signature patterns.
const HANDLER_FN: &[u8] = b"fn handler";
const HANDLER_ASYNC: &[u8] = b"async fn";
const HANDLER_RETURN: &[u8] = b"-> Response";
const HANDLER_RESPONSE: &[u8] = b"Response";

/// Markdown API-doc heading needles.
const MD_API_HEADING: &[u8] = b"api";
const MD_ENDPOINTS_HEADING: &[u8] = b"endpoint";
const MD_REFERENCE_HEADING: &[u8] = b"reference";

/// OpenAPI schema keywords.
const SCHEMA_RESPONSE: &[u8] = b"Response";
const SCHEMA_ERROR: &[u8] = b"Error";
const SCHEMA_4XX: &[u8] = b"4xx";
const SCHEMA_5XX: &[u8] = b"5xx";

/// Findings of a single API-doc analysis pass.
pub type ApiDocReport = crate::quality::SmellReport;

/// Count occurrences of `needle` in `bytes` outside non-executable regions.
fn count_executable(bytes: &[u8], regions: &[(usize, usize)], needle: &[u8]) -> usize {
    memmem::find_iter(bytes, needle)
        .filter(|&off| !offset_suppressed(off, regions))
        .count()
}

/// Rust pub-item count.
fn count_pub_items(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    count_executable(bytes, regions, PUB_FN)
        + count_executable(bytes, regions, PUB_STRUCT)
        + count_executable(bytes, regions, PUB_ENUM)
        + count_executable(bytes, regions, PUB_TRAIT)
}

/// Doctest `# Examples` / `# Panics` / `# Errors` section count.
///
/// Uses line-walk because these markers live INSIDE `///` doc comments,
/// which `non_executable_regions` marks as non-executable — we WANT to
/// see them (they're the rustdoc contract).
fn count_doc_sections(bytes: &[u8]) -> usize {
    count_executable_including_test_bodies(bytes, EXAMPLES_SECTION)
        + count_executable_including_test_bodies(bytes, EXAMPLES_SECTION_ALT)
        + count_executable_including_test_bodies(bytes, PANICS_SECTION)
        + count_executable_including_test_bodies(bytes, ERRORS_SECTION)
}

/// OpenAPI / swagger reference count.
fn count_openapi_refs(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    count_executable(bytes, regions, OPENAPI)
        + count_executable(bytes, regions, SWAGGER)
        + count_executable(bytes, regions, UTOIPA)
        + count_executable(bytes, regions, ASYNC_OPENAPI)
        + count_executable(bytes, regions, OASGEN)
        + count_executable(bytes, regions, SPECTRAL)
}

/// utoipa derive / attribute count.
fn count_utoipa_derives(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    count_executable(bytes, regions, UTOIPA_PATH)
        + count_executable(bytes, regions, UTOIPA_TOSCHEMA)
        + count_executable(bytes, regions, UTOIPA_INTO_PARAMS)
        + count_executable(bytes, regions, UTOIPA_INTO_RESPONSES)
}

/// HTTP-framework code-signal count.
fn count_http_signals(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    count_executable(bytes, regions, CODE_REQWEST)
        + count_executable(bytes, regions, CODE_ACTIX)
        + count_executable(bytes, regions, CODE_AXUM)
        + count_executable(bytes, regions, CODE_EXPRESS)
        + count_executable(bytes, regions, CODE_FASTAPI)
        + count_executable(bytes, regions, CODE_FLASK)
        + count_executable(bytes, regions, CODE_DJANGO)
        + count_executable(bytes, regions, CODE_HAPI)
}

/// Handler signature count.
fn count_handlers(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    count_executable(bytes, regions, HANDLER_FN)
        + count_executable(bytes, regions, HANDLER_ASYNC)
        + count_executable(bytes, regions, HANDLER_RETURN)
        + count_executable(bytes, regions, HANDLER_RESPONSE)
}

/// Error/response schema keyword count.
fn count_schema_keywords(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    count_executable(bytes, regions, SCHEMA_RESPONSE)
        + count_executable(bytes, regions, SCHEMA_ERROR)
        + count_executable(bytes, regions, SCHEMA_4XX)
        + count_executable(bytes, regions, SCHEMA_5XX)
}

/// Markdown heading (case-insensitive). Bypasses `non_executable_regions`
/// because Markdown has no profile (the Rust fallback would treat `#` as a
/// comment, masking every heading).
fn markdown_has_h2(md_lower: &[u8], marker: &[u8]) -> bool {
    let mut line_start = 0usize;
    while line_start < md_lower.len() {
        let line_end = md_lower[line_start..]
            .iter()
            .position(|&b| b == b'\n')
            .map(|p| line_start + p)
            .unwrap_or(md_lower.len());
        let line = &md_lower[line_start..line_end];
        if line.starts_with(b"## ")
            && line.len() >= 3 + marker.len()
            && &line[3..3 + marker.len()] == marker
        {
            return true;
        }
        line_start = line_end + 1;
    }
    false
}

/// Rust branch — pub surface + HTTP framework + utoipa coverage.
fn emit_rust_findings(report: &mut ApiDocReport, bytes: &[u8], regions: &[(usize, usize)]) {
    let pub_items = count_pub_items(bytes, regions);
    let doc_sections = count_doc_sections(bytes);
    let openapi = count_openapi_refs(bytes, regions);
    let utoipa_derives = count_utoipa_derives(bytes, regions);
    let http = count_http_signals(bytes, regions);
    let handlers = count_handlers(bytes, regions);
    let schema = count_schema_keywords(bytes, regions);

    if pub_items >= 3 && doc_sections == 0 {
        report.push(
            "≥3 pub fn/struct/enum/trait without `# Examples` / `# Panics` / `# Errors` \
             rustdoc section — API surface undocumented",
            1,
            0.6,
        );
    }
    if http >= 1 && openapi == 0 {
        report.push(
            "HTTP framework (reqwest/actix/axum) without openapi/swagger/utoipa \
             /async-openapi reference — REST API has no machine-readable spec",
            1,
            0.7,
        );
    }
    if handlers >= 1 && utoipa_derives == 0 && openapi == 0 {
        report.push(
            "handler / async fn / -> Response without `#[utoipa::path]` derive or \
             OpenAPI spec — endpoint not annotated for API doc",
            1,
            0.5,
        );
    }
    if openapi >= 1 && schema == 0 {
        report.push(
            "openapi/swagger/utoipa reference without `Response`/`Error`/4xx/5xx \
             schema mention — error responses undocumented",
            1,
            0.5,
        );
    }
}

/// JS/TS branch — express + openapi.
fn emit_js_ts_findings(report: &mut ApiDocReport, bytes: &[u8], regions: &[(usize, usize)]) {
    let http = count_http_signals(bytes, regions);
    let openapi = count_openapi_refs(bytes, regions);
    if http >= 1 && openapi == 0 {
        report.push(
            "express/@hapi/fastapi in use without openapi/swagger/async-openapi \
             — REST endpoints have no machine-readable spec",
            1,
            0.7,
        );
    }
}

/// Python branch — FastAPI/Flask/Django + openapi.
fn emit_python_findings(report: &mut ApiDocReport, bytes: &[u8], regions: &[(usize, usize)]) {
    let http = count_http_signals(bytes, regions);
    let openapi = count_openapi_refs(bytes, regions);
    if http >= 1 && openapi == 0 {
        report.push(
            "FastAPI/Flask/Django in use without openapi/swagger \
             — endpoints not exposed as OpenAPI spec",
            1,
            0.7,
        );
    }
}

/// Markdown branch — API heading presence.
fn emit_markdown_findings(report: &mut ApiDocReport, source: &str) {
    let md_lower = source.to_ascii_lowercase().into_bytes();
    let has_api = markdown_has_h2(&md_lower, MD_API_HEADING)
        || markdown_has_h2(&md_lower, MD_ENDPOINTS_HEADING)
        || markdown_has_h2(&md_lower, MD_REFERENCE_HEADING);
    if source.len() > 200 && !has_api {
        report.push(
            "Markdown doc (>200 bytes) without `# API` / `# Endpoints` / \
             `# Reference` H2 heading — API surface not documented",
            1,
            0.5,
        );
    }
}

/// Analyze API documentation in `source` for the given language.
pub fn analyze_api_doc(source: &str, lang: &str) -> ApiDocReport {
    let bytes = source.as_bytes();
    let regions = non_executable_regions(source, lang);
    let mut report = ApiDocReport {
        total_lines: source.lines().count().max(1),
        ..Default::default()
    };
    match lang {
        "rust" | "rs" => emit_rust_findings(&mut report, bytes, &regions),
        "typescript" | "ts" | "tsx" | "javascript" | "js" | "jsx" | "mjs" | "cjs" => {
            emit_js_ts_findings(&mut report, bytes, &regions)
        }
        "python" | "py" => emit_python_findings(&mut report, bytes, &regions),
        "markdown" | "md" => emit_markdown_findings(&mut report, source),
        _ => {}
    }
    report.findings.sort_by_key(|f| std::cmp::Reverse(f.1));
    report
}

/// Score a [`ApiDocReport`] as `1 - density·SCALE`, clamped to `[0,1]`.
pub fn score_api_doc(report: &ApiDocReport) -> f32 {
    density_score(report.weighted_total, report.total_lines, SCALE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rust_with_doc_sections_clean() {
        let src = r#"
/// Computes 2+2.
///
/// # Examples
///
/// ```
/// assert_eq!(answer(), 4);
/// ```
///
/// # Panics
/// Never panics.
pub fn answer() -> i32 { 4 }

/// Foo struct.
pub struct Foo { x: i32 }

/// Bar enum.
pub enum Bar { A, B }
"#;
        let r = analyze_api_doc(src, "rust");
        assert_eq!(
            r.violations, 0,
            "doc sections + pub items is clean: {:?}",
            r.findings
        );
    }

    #[test]
    fn rust_pub_no_examples_flagged() {
        let src = r#"
/// Just docs, no Examples section.
pub fn a() -> i32 { 1 }
/// Just docs.
pub fn b() -> i32 { 2 }
/// Just docs.
pub fn c() -> i32 { 3 }
/// Just docs.
pub fn d() -> i32 { 4 }
"#;
        let r = analyze_api_doc(src, "rust");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("undocumented") || m.contains("API surface")),
            "pub items without Examples flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn rust_http_no_openapi_flagged() {
        let src = r#"
use actix_web;
use reqwest;

pub fn handler() -> actix_web::HttpResponse { actix_web::HttpResponse::Ok().finish() }
"#;
        let r = analyze_api_doc(src, "rust");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("no machine-readable spec")),
            "HTTP without openapi flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn rust_with_utoipa_clean() {
        let src = r#"
use utoipa;

#[utoipa::path(
    get,
    path = "/items",
    responses(
        (status = 200, description = "List items", body = [Item]),
        (status = 401, description = "Unauthorized"),
    ),
)]
pub async fn list_items() -> Response;
"#;
        let r = analyze_api_doc(src, "rust");
        assert!(
            !r.findings
                .iter()
                .any(|(m, _)| m.contains("no machine-readable spec")),
            "utoipa with 4xx schema is clean: {:?}",
            r.findings
        );
    }

    #[test]
    fn rust_handler_no_utoipa_flagged() {
        let src = r#"
pub async fn handler() -> Response;
"#;
        let r = analyze_api_doc(src, "rust");
        assert!(
            !r.findings.is_empty(),
            "handler without utoipa is flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn rust_no_pub_no_doc_issue() {
        let src = r#"
fn private() -> i32 { 1 }
fn internal() -> i32 { 2 }
"#;
        let r = analyze_api_doc(src, "rust");
        assert_eq!(r.violations, 0, "no pub surface: {:?}", r.findings);
    }

    #[test]
    fn js_with_openapi_clean() {
        let src = r#"
const swagger = require('swagger-ui-express');
const openapi = require('openapi-types');

app.use('/api-docs', swagger.serve, swagger.setup(openapi));
"#;
        let r = analyze_api_doc(src, "javascript");
        assert_eq!(
            r.violations, 0,
            "openapi + swagger is clean: {:?}",
            r.findings
        );
    }

    #[test]
    fn js_express_no_openapi_flagged() {
        let src = r#"
const express = require('express');
const app = express();
app.listen(3000);
"#;
        let r = analyze_api_doc(src, "javascript");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("no machine-readable spec")),
            "express without openapi flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn python_fastapi_no_openapi_flagged() {
        let src = r#"
from fastapi import FastAPI
app = FastAPI()
"#;
        let r = analyze_api_doc(src, "python");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("not exposed as OpenAPI")),
            "fastapi without openapi flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn markdown_with_api_heading_clean() {
        let src = "# Project\n\n## API\n\nEndpoints here.\n\n## Endpoints\n\n- GET /items\n";
        let r = analyze_api_doc(src, "markdown");
        assert_eq!(r.violations, 0, "md with API heading: {:?}", r.findings);
    }

    #[test]
    fn markdown_no_api_heading_flagged() {
        let src = "# Project\n\nThis is a long markdown document about my project. \
                  It has lots of content but no API section. We talk about installation, \
                  usage, contributing, and license but never document the API endpoints \
                  or the public Rust API surface. This file is well over the 200-byte \
                  threshold that the missing-API-section detector checks for.\n";
        let r = analyze_api_doc(src, "markdown");
        assert!(
            r.findings
                .iter()
                .any(|(m, _)| m.contains("API surface not documented")),
            "long md without API heading flagged: {:?}",
            r.findings
        );
    }

    #[test]
    fn comment_excluded() {
        let src = r#"
// pub fn a() -> i32 { 1 }
// pub fn b() -> i32 { 2 }
// pub fn c() -> i32 { 3 }
/// # Examples
/// ```
/// assert!(true);
/// ```
pub fn real() -> i32 { 1 }
"#;
        let r = analyze_api_doc(src, "rust");
        assert_eq!(
            r.violations, 0,
            "commented pub items excluded: {:?}",
            r.findings
        );
    }

    #[test]
    fn score_dirty_below_clean() {
        let bad = analyze_api_doc(
            r#"
use actix_web;
pub fn a() -> i32 { 1 }
pub fn b() -> i32 { 2 }
pub fn c() -> i32 { 3 }
pub fn d() -> i32 { 4 }
pub fn e() -> i32 { 5 }
pub fn handler() -> actix_web::HttpResponse { actix_web::HttpResponse::Ok().finish() }
"#,
            "rust",
        );
        let good = analyze_api_doc(
            r#"
/// Helper add.
///
/// # Examples
/// ```
/// assert_eq!(add(1, 2), 3);
/// ```
pub fn add(a: i32, b: i32) -> i32 { a + b }
"#,
            "rust",
        );
        assert!(
            score_api_doc(&bad) < score_api_doc(&good),
            "undocumented+HTTP file ({:.3}) must score below documented ({:.3})",
            score_api_doc(&bad),
            score_api_doc(&good)
        );
    }

    #[test]
    fn score_short_file_does_not_saturate() {
        let r = analyze_api_doc(
            r#"use actix_web;
pub fn a() -> i32 { 1 }
pub fn b() -> i32 { 2 }
pub fn c() -> i32 { 3 }
pub fn d() -> i32 { 4 }
"#,
            "rust",
        );
        let s = score_api_doc(&r);
        assert!(s > 0.0, "short undocumented file must not score 0.0: {s}");
    }
}
