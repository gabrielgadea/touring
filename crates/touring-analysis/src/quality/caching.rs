//! Polyglot cache-discipline analysis (D22 / F2.9): unbounded cache growth and
//! missing single-flight (cache-stampede risk).
//!
//! | Smell | Signal | Lang |
//! |-------|--------|------|
//! | Unbounded cache | a `Cache::builder()` → `.build()` chain with no `max_capacity(`/`time_to_live(`/`time_to_idle(` | Rust (moka) |
//! | Unbounded LRU | `new LRUCache(` / `new QuickLRU(` whose options lack `max`/`ttl`/a numeric capacity | JS/TS |
//! | Stampede (no single-flight) | a *cache-named* receiver read (`.get(`) **and** written (`.insert(`/`.put(`) in a file that never uses `.get_with(`/`.try_get_with(`/`.or_insert_with(`/`.entry(` | Rust |
//!
//! **Disjoint** from F2.8 memory (which owns `unbounded_channel(`/`unbounded(`/
//! `maxsize=None`) by keying on the *cache builder chain* + cache-named
//! get/insert + single-flight absence — none of which the memory engine inspects.
//! A bound-less `Cache::builder().build()` contains no `unbounded(` literal, so the
//! two engines never double-count the same construct. Python's unbounded
//! `@lru_cache(maxsize=None)` stays with F2.8 by prior claim.
//!
//! Score is `1 - density·SCALE` (SCALE 6.0, ADVISORY-tier), where density is
//! `weighted_violations / total_lines`. Comments / `#[cfg(test)]` are excluded via
//! `super::code_regions`.
//!
//! **Sources (context7, `/anthropics/moka`, High reputation, bench 96):**
//! `get_with`/`try_get_with` coalesce concurrent initialisations so only one
//! closure runs for a key — the canonical anti-stampede primitive; a
//! `Cache::builder()` left without `max_capacity` is unbounded (no size eviction).

use memchr::memmem;

use super::code_regions::{non_executable_regions, offset_suppressed};

/// Density→score scale (shared with the other ADVISORY-tier engines).
const SCALE: f32 = 6.0;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Lang {
    Rust,
    JsTs,
    Other,
}

fn canonical_lang(lang: &str) -> Lang {
    match lang {
        "rust" | "rs" => Lang::Rust,
        "typescript" | "ts" | "tsx" | "javascript" | "js" | "jsx" | "mjs" | "cjs" => Lang::JsTs,
        _ => Lang::Other,
    }
}

/// Cache-discipline findings for one file.
pub type CachingReport = crate::quality::SmellReport;

#[inline]
fn is_ident(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

/// `true` if the `.method(` whose `.` is at `dot` is called on a *cache-named*
/// receiver — i.e. the identifier immediately before the `.` contains "cache"
/// (case-insensitive). `self.query_cache.get(` → receiver `query_cache` → true;
/// `map.get(` → `map` → false. Non-allocating.
fn receiver_is_cache(bytes: &[u8], dot: usize) -> bool {
    let mut start = dot;
    while start > 0 && is_ident(bytes[start - 1]) {
        start -= 1;
    }
    if start == dot {
        return false;
    }
    bytes[start..dot]
        .windows(5)
        .any(|w| w.eq_ignore_ascii_case(b"cache"))
}

/// Moka cache builders (`Cache::builder()`, `SegmentedCache::builder()`, …) that
/// reach `.build()` without ever setting `max_capacity`/`time_to_live`/
/// `time_to_idle` — an unbounded, never-expiring cache (grows forever). The chain
/// terminator is the nearest `.build(` after the builder; the span between holds
/// the fluent config calls, so a split chain still works as long as the bound is
/// textually before `.build()`.
fn unbounded_cache_builder(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let mut count = 0;
    for off in memmem::find_iter(bytes, b"Cache::builder(") {
        if offset_suppressed(off, regions) {
            continue;
        }
        let rest = &bytes[off..];
        let Some(rel_build) = memmem::find(rest, b".build(") else {
            continue;
        };
        let span = &rest[..rel_build];
        let bounded = memmem::find(span, b"max_capacity(").is_some()
            || memmem::find(span, b"time_to_live(").is_some()
            || memmem::find(span, b"time_to_idle(").is_some();
        if !bounded {
            count += 1;
        }
    }
    count
}

/// Cache-stampede risk: a file that reads a cache-named receiver (`.get(`) **and**
/// writes one (`.insert(`/`.put(`) but never uses a single-flight primitive
/// (`get_with`/`try_get_with`/`or_insert_with`/`entry`). Count = the number of
/// stampede-prone cache writes. moka's `get_with` coalesces concurrent
/// initialisations; a manual get-then-insert lets every racing caller recompute.
fn stampede_risk(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let has_single_flight = memmem::find(bytes, b".get_with(").is_some()
        || memmem::find(bytes, b".try_get_with(").is_some()
        || memmem::find(bytes, b".or_insert_with(").is_some()
        || memmem::find(bytes, b".entry(").is_some();
    if has_single_flight {
        return 0;
    }
    let reads_cache = memmem::find_iter(bytes, b".get(")
        .any(|off| !offset_suppressed(off, regions) && receiver_is_cache(bytes, off));
    if !reads_cache {
        return 0;
    }
    let mut count = 0;
    for needle in [b".insert(".as_slice(), b".put(".as_slice()] {
        for off in memmem::find_iter(bytes, needle) {
            if !offset_suppressed(off, regions) && receiver_is_cache(bytes, off) {
                count += 1;
            }
        }
    }
    count
}

/// JS/TS `new LRUCache(` / `new QuickLRU(` whose constructor args contain no
/// `max`/`ttl` key and no numeric (positional) capacity — an unbounded LRU.
fn ts_unbounded_lru(bytes: &[u8], regions: &[(usize, usize)]) -> usize {
    let mut count = 0;
    for ctor in [b"new LRUCache(".as_slice(), b"new QuickLRU(".as_slice()] {
        for off in memmem::find_iter(bytes, ctor) {
            if offset_suppressed(off, regions) {
                continue;
            }
            let arg_start = off + ctor.len();
            let mut depth = 1i32;
            let mut j = arg_start;
            let mut close = None;
            while j < bytes.len() {
                match bytes[j] {
                    b'(' => depth += 1,
                    b')' => {
                        depth -= 1;
                        if depth == 0 {
                            close = Some(j);
                            break;
                        }
                    }
                    _ => {}
                }
                j += 1;
            }
            let Some(close) = close else { continue };
            let args = &bytes[arg_start..close];
            let bounded = memmem::find(args, b"max").is_some()
                || memmem::find(args, b"ttl").is_some()
                || args.iter().any(u8::is_ascii_digit);
            if !bounded {
                count += 1;
            }
        }
    }
    count
}

/// Analyze cache-discipline smells in `source`.
pub fn analyze_caching(source: &str, lang: &str) -> CachingReport {
    let bytes = source.as_bytes();
    let regions = non_executable_regions(source, lang);
    let mut report = CachingReport {
        total_lines: source.lines().count().max(1),
        ..Default::default()
    };
    match canonical_lang(lang) {
        Lang::Rust => {
            report.push(
                "unbounded cache (Cache::builder without max_capacity/time_to_live)",
                unbounded_cache_builder(bytes, &regions),
                1.0,
            );
            report.push(
                "cache stampede risk (manual get/insert without get_with single-flight)",
                stampede_risk(bytes, &regions),
                0.8,
            );
        }
        Lang::JsTs => {
            report.push(
                "unbounded LRU (new LRUCache/QuickLRU without max/ttl)",
                ts_unbounded_lru(bytes, &regions),
                1.0,
            );
        }
        Lang::Other => {}
    }
    report.findings.sort_by_key(|f| std::cmp::Reverse(f.1));
    report
}

/// Score a [`CachingReport`] as `1 - density·SCALE`, clamped to `[0,1]`.
/// Delegates to [`super::score_utils::density_score`] for the `max(20)` floor
/// so short files don't saturate (F2.13 lesson).
pub fn score_caching(report: &CachingReport) -> f32 {
    super::score_utils::density_score(report.weighted_total, report.total_lines, SCALE)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn clean_bounded_cache_high() {
        let src = "fn build() {\n    let cache = Cache::builder().max_capacity(1000).time_to_live(d).build();\n    let v = cache.get_with(k, || compute());\n}\n";
        let r = analyze_caching(src, "rust");
        assert_eq!(
            r.violations, 0,
            "bounded + single-flight is clean: {:?}",
            r.findings
        );
        assert!(score_caching(&r) > 0.95);
    }
    #[test]
    fn unbounded_builder_flagged() {
        let r = analyze_caching("fn f() { let c = Cache::builder().build(); }\n", "rust");
        assert_eq!(
            r.violations, 1,
            "bound-less builder is unbounded: {:?}",
            r.findings
        );
    }
    #[test]
    fn builder_with_ttl_not_flagged() {
        let r = analyze_caching(
            "fn f() { let c = Cache::builder().time_to_idle(d).build(); }\n",
            "rust",
        );
        assert_eq!(
            r.findings
                .iter()
                .filter(|(m, _)| m.contains("unbounded cache"))
                .count(),
            0,
            "ttl bounds the cache: {:?}",
            r.findings
        );
    }
    #[test]
    fn stampede_manual_get_insert_flagged() {
        let src = "fn f() {\n    if let Some(v) = cache.get(&k) { return v; }\n    cache.insert(k, compute());\n}\n";
        let r = analyze_caching(src, "rust");
        assert!(
            r.findings.iter().any(|(m, _)| m.contains("stampede")),
            "manual get/insert is stampede-prone: {:?}",
            r.findings
        );
    }
    #[test]
    fn single_flight_not_flagged() {
        let r = analyze_caching(
            "fn f() { let v = cache.get_with(k, || compute()); }\n",
            "rust",
        );
        assert_eq!(
            r.findings
                .iter()
                .filter(|(m, _)| m.contains("stampede"))
                .count(),
            0,
            "get_with is the safe single-flight API: {:?}",
            r.findings
        );
    }
    #[test]
    fn non_cache_map_not_flagged() {
        let r = analyze_caching(
            "fn f() { let v = map.get(&k); map.insert(k, v); }\n",
            "rust",
        );
        assert_eq!(
            r.violations, 0,
            "non-cache map is not flagged: {:?}",
            r.findings
        );
    }
    #[test]
    fn ts_unbounded_lru_flagged() {
        let r = analyze_caching("const c = new LRUCache({ dispose: fn });\n", "typescript");
        assert_eq!(
            r.violations, 1,
            "LRU without max/ttl is unbounded: {:?}",
            r.findings
        );
    }
    #[test]
    fn ts_bounded_lru_not_flagged() {
        let r1 = analyze_caching("const c = new LRUCache({ max: 500 });\n", "typescript");
        let r2 = analyze_caching("const c = new LRUCache(500);\n", "typescript");
        assert_eq!(
            r1.violations, 0,
            "max-bounded LRU is clean: {:?}",
            r1.findings
        );
        assert_eq!(
            r2.violations, 0,
            "positional-capacity LRU is clean: {:?}",
            r2.findings
        );
    }
    #[test]
    fn comment_excluded() {
        let r = analyze_caching("// let c = Cache::builder().build();\nfn f() {}\n", "rust");
        assert_eq!(
            r.violations, 0,
            "commented builder is excluded: {:?}",
            r.findings
        );
    }
    #[test]
    fn score_monotonic_unbounded_below_clean() {
        let bad = analyze_caching(
            "fn f() {\n    let a = Cache::builder().build();\n    let b = Cache::builder().build();\n}\n",
            "rust",
        );
        let good = analyze_caching(
            "fn f() {\n    let a = Cache::builder().max_capacity(10).build();\n    let b = Cache::builder().max_capacity(20).build();\n}\n",
            "rust",
        );
        assert!(
            score_caching(&bad) < score_caching(&good),
            "unbounded must score below bounded"
        );
    }
    /// Regression test for the F2.13 saturation fix (`max(20)` floor in
    /// [`super::score_utils::density_score`]): a 5-line file with 3
    /// weighted findings must NOT score 0.0 (the prior `total_lines.max(1)`
    /// saturated to 0 here). The smell is the smell, regardless of LOC.
    #[test]
    fn score_short_file_does_not_saturate() {
        let r = analyze_caching(
            "fn f() {\n    let a = Cache::builder().build();\n    let b = Cache::builder().build();\n    let c = Cache::builder().build();\n}\n",
            "rust",
        );
        let s = score_caching(&r);
        assert!(
            s > 0.0,
            "5-line file with 3 unbounded builders must not score 0.0: {s} (would saturate without max(20) floor)"
        );
        // Expected: 1 - (weighted_total / 20) * SCALE. With 3 unbounded
        // builders, weighted_total ≈ 3.0 (one 1.0-weighted + two 1.0).
        // Score = 1 - (3/20)*6 = 0.1.
        assert!(s < 0.5, "many findings should still pull score down: {s}");
    }
}
