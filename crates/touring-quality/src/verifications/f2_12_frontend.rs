//! F2.12 — Frontend Performance verifier (D25).
//!
//! **Real engine (default `workspace-integration`)**: delegates to
//! [`touring_analysis::quality::analyze_frontend`] — a polyglot CWV detector
//! across 6 dimensions:
//!
//! | Detector | Signal | Lang |
//! |----------|--------|------|
//! | `render-blocking-script` | `<script>` without `defer`/`async`/`type="module"` | HTML, TSX, JSX, Vue |
//! | `blocking-stylesheet-in-body` | `<link rel="stylesheet">` outside `<head>` | TSX, JSX, Vue |
//! | `unbuffered-layout-shift` | `<img>` without `width=`/`height=` (CLS) | HTML, TSX, JSX, Vue |
//! | `lazy-without-fetchpriority` | `<img loading="lazy">` without `fetchpriority="high"` (LCP contradiction) | HTML, TSX, JSX, Vue |
//! | `sync-heavy-handler` | `addEventListener`/`onclick=` multi-line body with no `await`/`Promise` (INP) | JS, TS, JSX, TSX |
//! | `wasm-no-opt-flag` | `.wasm` literal with no `wasm-opt` invocation nearby | Rust, JS, TS |
//! | `dynamic-import-many` | `import(` count > 5 without code-split hint | JS, TS, JSX, TSX |
//!
//! It is **disjoint** from F2.1 OWASP (security — `innerHTML` injection is a
//! security signal, not a perf signal; the F2.12 detector keys on render
//! patterns), F2.9 caching (no overlap), and F2.10 I/O (which is about
//! blocking I/O in async — F2.12 is about browser-load latency). The CWV
//! signal (`<script defer>`, `<img width>`, `fetchpriority`) is unique to
//! this verifier.
//!
//! This replaces a stub that scored `innerHTML` density as an anti-metric —
//! `innerHTML` is a *security* smell (F2.1 OWASP), not a perf one, and the
//! stub treated every `innerHTML` as equally bad regardless of context (the
//! canonical `el.innerHTML = ''` clear vs. `el.innerHTML = user_input` are
//! both penalized equally).
//!
//! **Standalone fallback (`--no-default-features`)**: a labelled
//! `<script>` / `<img` density heuristic.
//!
//! **Scope**: per-file; rolls up as `AggKind::WeightedLoc`. ADVISORY-tier.

use crate::verifications::Verification;
use crate::{DimId, DimScore};
use anyhow::Result;
use std::path::Path;

/// F2.12 verifier — Frontend Performance.
#[allow(non_camel_case_types)]
pub struct F2_12_Frontend;

impl Verification for F2_12_Frontend {
    fn id(&self) -> DimId {
        DimId::F2_12
    }
    fn check(&self, target: &Path) -> Result<DimScore> {
        let (value, evidence) = analyze_frontend_dim(target)?;
        Ok(crate::verifications::finish(
            self.id(),
            value,
            evidence,
            target,
        ))
    }
}

// ── Real engine: frontend-perf CWV detection ───────────────────────────────
#[cfg(feature = "workspace-integration")]
fn analyze_frontend_dim(target: &Path) -> Result<(f32, String)> {
    use touring_analysis::quality::{analyze_frontend, score_frontend};
    if is_detector_own_source(target) {
        return Ok((
            1.0,
            "F2.12: detector own source (CWV needle vocabulary embedded as data) — score=1.000"
                .to_string(),
        ));
    }
    let raw = crate::verifications::read_target_source(target)?;
    let lang = crate::verifications::lang_from_ext(target);
    let r = analyze_frontend(&raw, lang);
    let value = score_frontend(&r);
    let top = r
        .findings
        .first()
        .map(|(m, c)| format!("; top: {m} ({c}x)"))
        .unwrap_or_default();
    let evidence = format!(
        "F2.12: {} CWV anti-pattern(s) over {} lines ({lang}) — score={value:.3} \
         (touring-analysis analyze_frontend: render-blocking-script / blocking-stylesheet / unbuffered-layout-shift / lazy-without-fetchpriority / sync-heavy-handler / wasm-no-opt / dynamic-import-many){top}",
        r.violations, r.total_lines
    );
    Ok((value, evidence))
}

/// The engine and this verifier embed the CWV needle vocabulary as detection
/// data, so scoring their own source is a self-match false positive. Mirrors
/// `f2_8_memory::is_detector_own_source`.
#[cfg(feature = "workspace-integration")]
fn is_detector_own_source(target: &Path) -> bool {
    crate::verifications::is_detector_own_source(target)
}

// ── Standalone fallback: CWV-density heuristic ───────────────────────────────
#[cfg(not(feature = "workspace-integration"))]
fn analyze_frontend_dim(target: &Path) -> Result<(f32, String)> {
    let raw = crate::verifications::read_target_source(target)?;
    let lines = raw.lines().count().max(1) as f32;
    let smells = raw.matches("<script").count()
        + raw.matches("<img").count()
        + raw.matches("addEventListener").count()
        + raw.matches(".wasm").count();
    let value = (1.0 - (smells as f32 / lines) * 6.0).clamp(0.0, 1.0);
    let evidence = format!(
        "{smells} <script/<img/addEventListener/.wasm smell(s) over {} lines (heuristic; build \
         --features workspace-integration for full CWV analysis)",
        lines as usize
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
    fn test_frontend_returns_valid_score() {
        let f = write_temp_ext("<html><body>hello</body></html>\n", ".html");
        let s = F2_12_Frontend.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "score out of range: {}",
            s.value
        );
    }
    #[test]
    fn test_frontend_empty_file() {
        let f = write_temp("");
        let s = F2_12_Frontend.check(f.path()).expect("check");
        assert!((0.0..=1.0).contains(&s.value));
    }
    /// Render-blocking `<script>` is the top CWV regression per Lighthouse.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_render_blocking_scores_lower() {
        let bad = write_temp_ext(
            "<html><body><script>work();</script><img src=\"hero.jpg\"></body></html>\n",
            ".html",
        );
        let good = write_temp_ext(
            "<html><body><script defer>work();</script><img src=\"hero.jpg\" width=\"100\" height=\"100\"></body></html>\n",
            ".html",
        );
        let sb = F2_12_Frontend.check(bad.path()).expect("check");
        let sg = F2_12_Frontend.check(good.path()).expect("check");
        assert!(
            sb.value < sg.value,
            "blocking ({}) < clean ({})",
            sb.value,
            sg.value
        );
    }
    /// `<img>` without `width`/`height` is the canonical CLS smell.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_img_no_dims_flagged() {
        let bad = write_temp_ext(
            "<html><body><img src=\"hero.jpg\"></body></html>\n",
            ".html",
        );
        let good = write_temp_ext(
            "<html><body><img src=\"hero.jpg\" width=\"100\" height=\"100\"></body></html>\n",
            ".html",
        );
        let sb = F2_12_Frontend.check(bad.path()).expect("check");
        let sg = F2_12_Frontend.check(good.path()).expect("check");
        assert!(
            sb.value < sg.value,
            "no-dims ({}) < with-dims ({})",
            sb.value,
            sg.value
        );
    }
    /// Pure Rust file (no .wasm) is clean — the frontend verifier does not
    /// penalise generic Rust.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_pure_rust_clean() {
        let f = write_temp_ext("fn add(a: i32, b: i32) -> i32 { a + b }\n", ".rs");
        let s = F2_12_Frontend.check(f.path()).expect("check");
        assert!(s.value > 0.9, "pure-Rust file is clean, got {}", s.value);
    }
    /// The engine's own source (which embeds every needle as data) must be
    /// allowlisted, not self-matched.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_detector_own_source_allowlisted() {
        use std::path::Path;
        assert!(is_detector_own_source(Path::new(
            "/x/touring-analysis/src/quality/frontend.rs"
        )));
        assert!(is_detector_own_source(Path::new(
            "/x/crates/foo/tests/bar.rs"
        )));
        assert!(!is_detector_own_source(Path::new(
            "/x/crates/touring-web/src/index.html"
        )));
    }
}
