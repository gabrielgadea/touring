//! F2.13 — Scalability verifier (D26).
//!
//! **Real engine (default `workspace-integration`)**: delegates to
//! [`touring_analysis::quality::analyze_scalability`] — a polyglot detector
//! of the canonical horizontal/vertical scaling smells:
//!
//! | Detector | Signal | Lang |
//! |----------|--------|------|
//! | `unbounded-state-in-process` | `Arc<Mutex<HashMap<…>>>` / `Arc<RwLock<HashMap<…>>>` / `Arc<Mutex<Vec<…>>>` | Rust |
//! | `unbounded-channel` | `unbounded_channel(` without a bounded `tokio::sync::mpsc::channel(` sibling in the same file | Rust |
//! | `hardcoded-thread-pool-small` | `rayon::ThreadPoolBuilder::new().num_threads(N)` with `N ≤ 8` literal | Rust |
//! | `missing-timeout-external` | `reqwest::Client::new(` / `reqwest::get(` without `.timeout(` in same file | Rust |
//! | `hot-loop-no-yield` | `loop {}` / `while` in `async fn` body without `tokio::task::yield_now()` | Rust |
//! | `go-goroutine-no-cap` | `go func()` with no `sync.WaitGroup` / bounded `chan` in same file | Go |
//!
//! It is **disjoint** from F2.8 memory (which keys on the *individual*
//! `unbounded_channel(` literal — F2.13 keys on the *file-level* comparison:
//! unbounded *and* no bounded sibling), F2.10 I/O (which keys on
//! `reqwest::blocking` — F2.13 keys on `reqwest::get` without `.timeout`),
//! and F2.11 concurrency (which keys on `std::sync::Mutex::lock(` in
//! `async fn` — F2.13 keys on `Arc<Mutex<HashMap>>` as a *state shape*
//! anti-pattern that prevents horizontal replication).
//!
//! This replaces a stub that scored `(`.capacity(` + `.limit(` +
//! `Vec::with_capacity`)` density / 20 as an anti-metric (more
//! capacity-calls = lower score; idiomatic Rust pre-sizes frequently).
//!
//! **Standalone fallback (`--no-default-features`)**: a labelled
//! `Arc<Mutex<HashMap` / `unbounded_channel` density heuristic.
//!
//! **Scope**: per-file; rolls up as `AggKind::WeightedLoc`. ADVISORY-tier.

use crate::DimId;
use crate::verifications::Verification;
use anyhow::Result;
use std::path::Path;

/// F2.13 verifier — Scalability.
#[allow(non_camel_case_types)]
pub struct F2_13_Scalability;

impl Verification for F2_13_Scalability {
    fn id(&self) -> DimId {
        DimId::F2_13
    }
    fn measure(&self, target: &Path) -> Result<(f32, String)> {
        analyze_scalability_dim(target)
    }
}

// ── Real engine: scalability anti-pattern detection ─────────────────────────
#[cfg(feature = "workspace-integration")]
fn analyze_scalability_dim(target: &Path) -> Result<(f32, String)> {
    use touring_analysis::quality::{analyze_scalability, score_scalability};
    if is_detector_own_source(target) {
        return Ok((
            1.0,
            "F2.13: detector own source (scalability needle vocabulary embedded as data) — score=1.000"
                .to_string(),
        ));
    }
    let raw = crate::verifications::read_target_source(target)?;
    let lang = crate::verifications::lang_from_ext(target);
    let r = analyze_scalability(&raw, lang);
    let value = score_scalability(&r);
    let top = crate::verifications::top_finding(&r.findings);
    let evidence = format!(
        "F2.13: {} scalability anti-pattern(s) over {} lines ({lang}) — score={value:.3} \
         (touring-analysis analyze_scalability: unbounded-state-in-process / unbounded-channel / hardcoded-thread-pool-small / missing-timeout-external / hot-loop-no-yield / go-goroutine-no-cap){top}",
        r.violations, r.total_lines
    );
    Ok((value, evidence))
}

/// The engine and this verifier embed the scalability needle vocabulary as
/// detection data, so scoring their own source is a self-match false
/// positive. Mirrors `f2_8_memory::is_detector_own_source`.
#[cfg(feature = "workspace-integration")]
fn is_detector_own_source(target: &Path) -> bool {
    crate::verifications::is_detector_own_source(target)
}

// ── Standalone fallback: scalability-density heuristic ───────────────────────
#[cfg(not(feature = "workspace-integration"))]
fn analyze_scalability_dim(target: &Path) -> Result<(f32, String)> {
    let raw = crate::verifications::read_target_source(target)?;
    let lines = raw.lines().count().max(1) as f32;
    let smells = raw.matches("Arc<Mutex<HashMap").count()
        + raw.matches("unbounded_channel(").count()
        + raw.matches("reqwest::Client::new(").count();
    let value = (1.0 - (smells as f32 / lines) * 6.0).clamp(0.0, 1.0);
    let evidence = format!(
        "{smells} Arc<Mutex<HashMap / unbounded_channel / reqwest::Client smell(s) over {} lines \
         (heuristic; build --features workspace-integration for full scalability analysis)",
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
    fn test_scalability_returns_valid_score() {
        let f = write_temp_ext("fn example() {}\n", ".rs");
        let s = F2_13_Scalability.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "score out of range: {}",
            s.value
        );
    }
    #[test]
    fn test_scalability_empty_file() {
        let f = write_temp("");
        let s = F2_13_Scalability.check(f.path()).expect("check");
        assert!((0.0..=1.0).contains(&s.value));
    }
    /// Unbounded channel (without bounded sibling) scores below the
    /// bounded-channel version. The detector fires for the *file-level*
    /// presence of `unbounded_channel(` *and* the absence of a bounded
    /// `tokio::sync::mpsc::channel(` sibling — so the `bad` file is flagged
    /// for both smells, the `good` file for neither.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_unbounded_channel_scores_lower() {
        let bad = write_temp_ext(
            "fn go() {\n    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();\n    let _ = tx.send(x);\n}\n",
            ".rs",
        );
        let good = write_temp_ext(
            "fn go() {\n    let (tx, rx) = tokio::sync::mpsc::channel(64);\n    let _ = tx.send(x).await;\n}\n",
            ".rs",
        );
        let sb = F2_13_Scalability.check(bad.path()).expect("check");
        let sg = F2_13_Scalability.check(good.path()).expect("check");
        assert!(
            sb.value < sg.value,
            "unbounded-channel file ({}) must score below bounded-sibling file ({})",
            sb.value,
            sg.value
        );
    }
    /// The engine's own source (which embeds every needle as data) must be
    /// allowlisted, not self-matched.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_detector_own_source_allowlisted() {
        use std::path::Path;
        assert!(is_detector_own_source(Path::new(
            "/x/touring-analysis/src/quality/scalability.rs"
        )));
        assert!(is_detector_own_source(Path::new(
            "/x/crates/foo/tests/bar.rs"
        )));
        assert!(!is_detector_own_source(Path::new(
            "/x/crates/touring-server/src/main.rs"
        )));
    }
}
