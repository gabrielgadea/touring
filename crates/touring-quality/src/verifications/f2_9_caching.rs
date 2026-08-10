//! F2.9 — Caching verifier (D22).
//!
//! **Real engine (default `workspace-integration`)**: delegates to
//! [`touring_analysis::quality::analyze_caching`] — a detector of two
//! cache-discipline smells: **unbounded cache growth** (`Cache::builder()`
//! → `.build()` with no `max_capacity`/`time_to_live`/`time_to_idle` in Rust;
//! `new LRUCache(` without `max`/`ttl`/numeric capacity in JS/TS), and
//! **cache-stampede risk** (a file that reads a cache-named receiver via
//! `.get(` and writes one via `.insert(`/`.put(` but never uses a single-flight
//! primitive `get_with`/`try_get_with`/`or_insert_with`/`entry`). It is
//! **disjoint** from F2.8 memory (which keys on `unbounded_channel(`/
//! `unbounded(`/Python `maxsize=None`) — F2.9 keys on the *cache builder
//! chain* + cache-named get/insert + single-flight absence, none of which the
//! memory engine inspects. A bound-less `Cache::builder().build()` contains no
//! `unbounded(` literal, so the two engines never double-count. Python's
//! unbounded `@lru_cache(maxsize=None)` stays with F2.8 by prior claim.
//!
//! This replaces a stub that scored `HashMap`/`moka`/`.cache_get(` density
//! (anti-metric: more cache = better, regardless of bounds or stampede).
//! Comments and `#[cfg(test)]`/test regions are excluded via `code_regions`.
//!
//! **Standalone fallback (`--no-default-features`)**: a labelled
//! `Cache::builder(`/`LRUCache(` density heuristic.
//!
//! **Scope**: per-file; rolls up as `AggKind::WeightedLoc`. ADVISORY-tier.

use crate::DimId;
use crate::verifications::Verification;
use anyhow::Result;
use std::path::Path;

/// F2.9 verifier — Caching.
#[allow(non_camel_case_types)]
pub struct F2_9_Caching;

impl Verification for F2_9_Caching {
    fn id(&self) -> DimId {
        DimId::F2_9
    }
    fn measure(&self, target: &Path) -> Result<(f32, String)> {
        analyze_caching_dim(target)
    }
}

// ── Real engine: cache-discipline anti-pattern detection ────────────────────
#[cfg(feature = "workspace-integration")]
fn analyze_caching_dim(target: &Path) -> Result<(f32, String)> {
    use touring_analysis::quality::{analyze_caching, score_caching};
    if is_detector_own_source(target) {
        return Ok((
            1.0,
            "F2.9: detector own source (cache-discipline needle vocabulary embedded as data) — score=1.000"
                .to_string(),
        ));
    }
    let raw = crate::verifications::read_target_source(target)?;
    let lang = crate::verifications::lang_from_ext(target);
    let r = analyze_caching(&raw, lang);
    let value = score_caching(&r);
    let top = crate::verifications::top_finding(&r.findings);
    let evidence = format!(
        "F2.9: {} cache-discipline anti-pattern(s) over {} lines ({lang}) — score={value:.3} \
         (touring-analysis analyze_caching: unbounded cache / stampede){top}",
        r.violations, r.total_lines
    );
    Ok((value, evidence))
}

/// The engine and this verifier embed the cache-discipline needle vocabulary
/// (`Cache::builder(`, `LRUCache(`, `QuickLRU(`, `get_with(`, …) as detection
/// data, so scoring their own source is a self-match false positive. Mirrors
/// `f2_8_memory::is_detector_own_source` (test/bench dirs + the quality
/// engine + verifier dirs). Pure path logic.
#[cfg(feature = "workspace-integration")]
fn is_detector_own_source(target: &Path) -> bool {
    crate::verifications::is_detector_own_source(target)
}

// ── Standalone fallback: cache-density heuristic ─────────────────────────────
#[cfg(not(feature = "workspace-integration"))]
fn analyze_caching_dim(target: &Path) -> Result<(f32, String)> {
    let raw = crate::verifications::read_target_source(target)?;
    let lines = raw.lines().count().max(1) as f32;
    let smells = raw.matches("Cache::builder(").count()
        + raw.matches("LRUCache(").count()
        + raw.matches(".get_with(").count();
    let value = (1.0 - (smells as f32 / lines) * 6.0).clamp(0.0, 1.0);
    let evidence = format!(
        "{smells} cache-builder/insert smell(s) over {} lines (heuristic; build --features \
         workspace-integration for full cache-discipline analysis)",
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
    fn test_caching_returns_valid_score() {
        let f = write_temp_ext(
            "let c = Cache::builder().max_capacity(1000).build();\n",
            ".rs",
        );
        let s = F2_9_Caching.check(f.path()).expect("check");
        assert!(
            (0.0..=1.0).contains(&s.value),
            "score out of range: {}",
            s.value
        );
    }
    #[test]
    fn test_caching_empty_file() {
        let f = write_temp("");
        let s = F2_9_Caching.check(f.path()).expect("check");
        assert!((0.0..=1.0).contains(&s.value));
    }
    /// Bounded + single-flight → high score.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_bounded_single_flight_clean_high() {
        let src = "fn f() {\n    let c = Cache::builder().max_capacity(1000).time_to_live(d).build();\n    let v = c.get_with(k, || compute());\n}\n";
        let f = write_temp_ext(src, ".rs");
        let s = F2_9_Caching.check(f.path()).expect("check");
        assert!(
            s.value > 0.95,
            "bounded + get_with is clean, got {}",
            s.value
        );
    }
    /// **End-to-end vs stub**: an unbounded builder scores below a bounded one —
    /// the stub (`HashMap`/`moka` density) was blind to the bound.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_unbounded_scores_lower() {
        let bad = write_temp_ext("fn f() { let c = Cache::builder().build(); }\n", ".rs");
        let good = write_temp_ext(
            "fn f() { let c = Cache::builder().max_capacity(100).build(); }\n",
            ".rs",
        );
        let sb = F2_9_Caching.check(bad.path()).expect("check");
        let sg = F2_9_Caching.check(good.path()).expect("check");
        assert!(
            sb.value < sg.value,
            "unbounded ({}) < bounded ({})",
            sb.value,
            sg.value
        );
    }
    /// Polyglot: a JS/TS unbounded LRU is flagged; a bounded one is not.
    #[cfg(feature = "workspace-integration")]
    #[test]
    fn test_ts_unbounded_lru_polyglot() {
        let bad = write_temp_ext("const c = new LRUCache({ dispose: fn });\n", ".ts");
        let good = write_temp_ext("const c = new LRUCache({ max: 500 });\n", ".ts");
        let sb = F2_9_Caching.check(bad.path()).expect("check");
        let sg = F2_9_Caching.check(good.path()).expect("check");
        assert!(
            sb.value < sg.value,
            "unbounded LRU ({}) must score below bounded LRU ({})",
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
            "/x/touring-analysis/src/quality/caching.rs"
        )));
        assert!(is_detector_own_source(Path::new(
            "/x/crates/foo/tests/bar.rs"
        )));
        assert!(!is_detector_own_source(Path::new(
            "/x/crates/touring-storage/src/cache.rs"
        )));
    }
}
