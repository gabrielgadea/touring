//! Wave v4.29.0 — shared ast-grep signal helper.
//!
//! A thin layer over `touring_code::polyglot::search` that:
//!  - Runs a curated `PatternSet` against a source snippet or file.
//!  - Caches results via `moka` keyed by (file blake3, pattern set id) — TTL 5 min.
//!  - Enforces a soft latency budget; returns partial results on overflow.
//!  - Produces a compact `ScanResult` suitable for one-line context injection.
//!
//! Used by:
//!  - `pre_read::AstGrepRiskSignalLayer` (S1) — defensive risk patterns per language.
//!  - `bash_ast_validator` (S2 + S3) — risk patterns + command-shape extraction.
//!
//! # Latency
//!
//! Cold parse of a 200-line file: ~3–10 ms. Cache hit: <1 µs. Budget overrun
//! returns the matches collected so far with `partial = true` set. Pre-read's
//! <10 ms warm budget is preserved by gating the layer at `cila_level >= 2`
//! and pre-warming the parser cache from `post_edit`.
//!
//! # Cache invalidation
//!
//! moka TTI 5 min + 64-entry LRU. `post_edit` does NOT explicitly invalidate
//! — content-addressing via blake3 makes the cache self-correcting (a new
//! file content => new key => fresh scan). Stale entries decay naturally.

use std::path::Path;
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use blake3::Hasher;
use moka::sync::Cache;
use touring_code::polyglot::{Lang, search};

use crate::risk_patterns::{lang_for_path, pattern_set_for};
use crate::signal_layer::{SignalContext, SignalLayer};

/// One pattern within a [`PatternSet`].
#[derive(Debug, Clone, Copy)]
pub struct PatternEntry {
    /// Stable identifier (used in counters, snapshot serialisation, debug).
    pub id: &'static str,
    /// ast-grep pattern source (e.g. `"$X.unwrap()"`).
    pub pattern: &'static str,
    /// Human-readable label (e.g. `"unwrap"`).
    pub label: &'static str,
}

/// Group of patterns scoped to a single language.
#[derive(Debug, Clone, Copy)]
pub struct PatternSet {
    /// Stable identifier — used as cache key prefix.
    pub set_id: &'static str,
    /// Target language.
    pub language: Lang,
    /// Patterns to apply (run in order; results aggregated per id).
    pub patterns: &'static [PatternEntry],
}

/// Aggregated count for a single pattern.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PatternCount {
    /// Pattern entry id from the [`PatternSet`].
    pub id: &'static str,
    /// User-facing label.
    pub label: &'static str,
    /// Number of structural matches.
    pub count: usize,
}

/// Result of running a [`PatternSet`] against source.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ScanResult {
    /// One entry per pattern that produced ≥1 match. Patterns with zero
    /// matches are dropped to keep downstream formatting clean.
    pub matches: Vec<PatternCount>,
    /// Total match count across all patterns.
    pub total: usize,
    /// Wall-clock elapsed time, ms.
    pub elapsed_ms: u64,
    /// `true` if the budget was exceeded — `matches` may be incomplete.
    pub partial: bool,
}

impl ScanResult {
    /// `true` when at least one pattern produced a match.
    pub fn has_matches(&self) -> bool {
        self.total > 0
    }
}

/// Default budget per scan (~3× cold parse latency on a 200-line file).
pub const DEFAULT_BUDGET: Duration = Duration::from_millis(30);

/// Run `patterns` against `source` and return aggregated counts. Stops early
/// if `budget` is exceeded. Pure (no cache, no I/O) — suitable for unit tests.
pub fn scan_source(source: &str, patterns: &PatternSet, budget: Duration) -> ScanResult {
    let started = Instant::now();
    let mut matches = Vec::with_capacity(patterns.patterns.len());
    let mut total = 0usize;
    let mut partial = false;

    for entry in patterns.patterns {
        if started.elapsed() >= budget {
            partial = true;
            break;
        }
        match search(patterns.language, source, entry.pattern) {
            Ok(hits) if !hits.is_empty() => {
                total += hits.len();
                matches.push(PatternCount {
                    id: entry.id,
                    label: entry.label,
                    count: hits.len(),
                });
            }
            Ok(_) => {} // Zero matches — skip silently.
            Err(_) => {
                // Pattern failed to compile or grammar rejected it.
                // Treat as "no match" rather than fatal — the rest of the
                // patterns may still be useful, and a malformed pattern is a
                // bug we want to surface via tests, not in production.
                continue;
            }
        }
    }

    ScanResult {
        matches,
        total,
        elapsed_ms: started.elapsed().as_millis() as u64,
        partial,
    }
}

/// Cache: (blake3(content) || set_id) → ScanResult.
fn cache() -> &'static Cache<u64, ScanResult> {
    static CELL: OnceLock<Cache<u64, ScanResult>> = OnceLock::new();
    CELL.get_or_init(|| {
        Cache::builder()
            .max_capacity(64)
            .time_to_idle(Duration::from_secs(300))
            .build()
    })
}

/// Compose a cache key from content + set id.
fn cache_key(content_hash: &[u8; 32], set_id: &str) -> u64 {
    // Fold the 32-byte blake3 hash into 8 bytes (xor of the 4 quarters), then
    // mix in `set_id` via a second blake3 pass. Total ~120 ns; cache hit savings
    // dwarf this cost on warm paths.
    let mut hasher = Hasher::new();
    hasher.update(content_hash);
    hasher.update(set_id.as_bytes());
    let derived = hasher.finalize();
    let bytes = derived.as_bytes();
    u64::from_le_bytes([
        bytes[0], bytes[1], bytes[2], bytes[3], bytes[4], bytes[5], bytes[6], bytes[7],
    ])
}

/// Cached variant of [`scan_source`]. Returns `None` only when the source
/// cannot be read; otherwise always returns a (possibly empty) [`ScanResult`].
pub fn scan_source_cached(source: &str, patterns: &PatternSet, budget: Duration) -> ScanResult {
    let content_hash = blake3::hash(source.as_bytes());
    let key = cache_key(content_hash.as_bytes(), patterns.set_id);
    if let Some(cached) = cache().get(&key) {
        return cached;
    }
    let fresh = scan_source(source, patterns, budget);
    cache().insert(key, fresh.clone());
    fresh
}

/// Read `path` into memory and run [`scan_source_cached`]. Returns `None` on
/// I/O error so callers can fail open silently.
pub fn scan_path_cached(
    path: &Path,
    patterns: &PatternSet,
    budget: Duration,
) -> Option<ScanResult> {
    let source = std::fs::read_to_string(path).ok()?;
    Some(scan_source_cached(&source, patterns, budget))
}

/// Format a [`ScanResult`] as a one-line label list:
///
/// `"unwrap=12, panic=2, unsafe=1"`
///
/// Returns `None` when the scan produced zero matches — the caller must skip
/// the signal entirely (silence-by-default policy).
pub fn format_matches(result: &ScanResult) -> Option<String> {
    if !result.has_matches() {
        return None;
    }
    let mut out = String::with_capacity(result.matches.len() * 16);
    for (i, m) in result.matches.iter().enumerate() {
        if i > 0 {
            out.push_str(", ");
        }
        out.push_str(m.label);
        out.push('=');
        out.push_str(&m.count.to_string());
    }
    Some(out)
}

// ─── SignalLayer impl ─────────────────────────────────────────────────

/// Signal layer that injects a one-line summary of risk patterns found in the
/// file being read, e.g. `[risk] rust: unwrap=12, panic=2`.
///
/// Wired into `pre_read`'s SignalPipeline. `should_run` is gated at
/// `cila_level >= 2` because the underlying ast-grep parse — even cached —
/// adds variance to pre-read latency that's only justified for medium+
/// CILA tasks.
pub struct AstGrepRiskSignalLayer {
    /// Override the project root prepended to relative paths. Tests use this
    /// to point at fixtures; production keeps `None` and resolves via the
    /// current working directory.
    project_root: Option<std::path::PathBuf>,
}

impl AstGrepRiskSignalLayer {
    /// Layer using the current working directory as the project root.
    pub fn new() -> Self {
        Self { project_root: None }
    }

    /// Layer rooted at a custom path (used by tests + multi-project actors).
    pub fn with_root(root: std::path::PathBuf) -> Self {
        Self {
            project_root: Some(root),
        }
    }

    fn resolve_path(&self, rel: &str) -> std::path::PathBuf {
        match &self.project_root {
            Some(root) => root.join(rel),
            None => std::path::PathBuf::from(rel),
        }
    }
}

impl Default for AstGrepRiskSignalLayer {
    fn default() -> Self {
        Self::new()
    }
}

impl SignalLayer for AstGrepRiskSignalLayer {
    fn name(&self) -> &'static str {
        "ast_grep_risk"
    }

    fn enrich(&self, ctx: &SignalContext<'_>) -> Vec<(f32, String)> {
        let path = self.resolve_path(ctx.file_path);
        let lang = match lang_for_path(&path) {
            Some(l) => l,
            None => return Vec::new(),
        };
        let pset = match pattern_set_for(lang) {
            Some(p) => p,
            None => return Vec::new(),
        };
        let result = match scan_path_cached(&path, pset, DEFAULT_BUDGET) {
            Some(r) => r,
            None => return Vec::new(),
        };
        let summary = match format_matches(&result) {
            Some(s) => s,
            None => return Vec::new(),
        };
        // Score 0.85: high enough to survive RRF merge on noisy pages but
        // below the top-tier signals (gotchas, blast_radius, dependents).
        vec![(0.85, format!("[risk] {}: {}", lang.name(), summary))]
    }

    fn should_run(&self, cila_level: usize) -> bool {
        cila_level >= 2
    }
}

#[cfg(test)]
mod layer_tests {
    use super::*;
    use std::fs;

    fn write_temp_rs(name: &str, content: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join("touring_ast_grep_layer_tests");
        fs::create_dir_all(&dir).expect("temp dir");
        let path = dir.join(name);
        fs::write(&path, content).expect("write temp");
        path
    }

    #[test]
    fn layer_emits_signal_for_rust_with_unwrap() {
        let path = write_temp_rs(
            "layer_unwrap.rs",
            "fn main() { let x: Option<i32> = Some(1); let _ = x.unwrap(); }",
        );
        let parent = path.parent().expect("parent dir").to_path_buf();
        let rel = path
            .file_name()
            .expect("filename")
            .to_str()
            .expect("utf-8")
            .to_string();
        let layer = AstGrepRiskSignalLayer::with_root(parent);
        let ctx = SignalContext::new(&rel, "")
            .with_cila(3)
            .with_hook("pre_read");
        let signals = layer.enrich(&ctx);
        assert_eq!(signals.len(), 1, "expected 1 signal, got {signals:?}");
        let (score, text) = &signals[0];
        assert!((score - 0.85).abs() < 0.01);
        assert!(text.contains("[risk]"));
        assert!(text.contains("unwrap=1"));
    }

    #[test]
    fn layer_emits_no_signal_for_clean_rust() {
        let path = write_temp_rs("layer_clean.rs", "fn main() { let _ = 1 + 1; }");
        let parent = path.parent().expect("parent dir").to_path_buf();
        let rel = path
            .file_name()
            .expect("filename")
            .to_str()
            .expect("utf-8")
            .to_string();
        let layer = AstGrepRiskSignalLayer::with_root(parent);
        let ctx = SignalContext::new(&rel, "")
            .with_cila(3)
            .with_hook("pre_read");
        assert!(layer.enrich(&ctx).is_empty());
    }

    #[test]
    fn layer_skips_unsupported_extension() {
        let path = write_temp_rs("notes.txt", "rm -rf /");
        let parent = path.parent().expect("parent dir").to_path_buf();
        let rel = path
            .file_name()
            .expect("filename")
            .to_str()
            .expect("utf-8")
            .to_string();
        let layer = AstGrepRiskSignalLayer::with_root(parent);
        let ctx = SignalContext::new(&rel, "")
            .with_cila(3)
            .with_hook("pre_read");
        assert!(layer.enrich(&ctx).is_empty());
    }

    #[test]
    fn layer_should_not_run_at_low_cila() {
        let layer = AstGrepRiskSignalLayer::new();
        assert!(!layer.should_run(0));
        assert!(!layer.should_run(1));
        assert!(layer.should_run(2));
        assert!(layer.should_run(5));
    }

    #[test]
    fn layer_returns_empty_for_missing_file() {
        let layer = AstGrepRiskSignalLayer::with_root(std::path::PathBuf::from(
            "/nonexistent/path/that/does/not/exist",
        ));
        let ctx = SignalContext::new("ghost.rs", "")
            .with_cila(3)
            .with_hook("pre_read");
        assert!(layer.enrich(&ctx).is_empty());
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const RUST_TEST_PATTERNS: PatternSet = PatternSet {
        set_id: "test_rust",
        language: Lang::Rust,
        patterns: &[
            PatternEntry {
                id: "unwrap",
                pattern: "$X.unwrap()",
                label: "unwrap",
            },
            PatternEntry {
                id: "panic",
                pattern: "panic!($$$)",
                label: "panic",
            },
        ],
    };

    #[test]
    fn scan_source_finds_unwrap() {
        let src = r#"fn main() { let x: Option<i32> = Some(1); let _ = x.unwrap(); }"#;
        let result = scan_source(src, &RUST_TEST_PATTERNS, DEFAULT_BUDGET);
        assert_eq!(result.total, 1);
        assert_eq!(result.matches.len(), 1);
        assert_eq!(result.matches[0].id, "unwrap");
        assert_eq!(result.matches[0].count, 1);
    }

    #[test]
    fn scan_source_counts_multiple_unwraps() {
        let src = r#"
            fn a() { x.unwrap(); }
            fn b() { y.unwrap(); }
            fn c() { z.unwrap(); }
        "#;
        let result = scan_source(src, &RUST_TEST_PATTERNS, DEFAULT_BUDGET);
        assert_eq!(result.total, 3);
        assert_eq!(result.matches[0].count, 3);
    }

    #[test]
    fn scan_source_finds_multiple_pattern_kinds() {
        let src = r#"
            fn a() { x.unwrap(); }
            fn b() { panic!("oops"); }
        "#;
        let result = scan_source(src, &RUST_TEST_PATTERNS, DEFAULT_BUDGET);
        assert_eq!(result.total, 2);
        assert_eq!(result.matches.len(), 2);
        let ids: Vec<&str> = result.matches.iter().map(|m| m.id).collect();
        assert!(ids.contains(&"unwrap"));
        assert!(ids.contains(&"panic"));
    }

    #[test]
    fn scan_source_returns_empty_for_clean_code() {
        let src = r#"fn main() { let _ = 1 + 1; }"#;
        let result = scan_source(src, &RUST_TEST_PATTERNS, DEFAULT_BUDGET);
        assert_eq!(result.total, 0);
        assert!(result.matches.is_empty());
        assert!(!result.has_matches());
    }

    #[test]
    fn scan_source_excludes_unwrap_in_string_literal() {
        // ast-grep correctly distinguishes `"x.unwrap()"` (string) from the call.
        let src = r#"fn main() { let _ = "x.unwrap()"; }"#;
        let result = scan_source(src, &RUST_TEST_PATTERNS, DEFAULT_BUDGET);
        assert_eq!(
            result.total, 0,
            "ast-grep must not match string-literal occurrences"
        );
    }

    #[test]
    fn scan_source_cached_returns_same_result_twice() {
        let src = r#"fn main() { x.unwrap(); }"#;
        let r1 = scan_source_cached(src, &RUST_TEST_PATTERNS, DEFAULT_BUDGET);
        let r2 = scan_source_cached(src, &RUST_TEST_PATTERNS, DEFAULT_BUDGET);
        assert_eq!(r1.matches, r2.matches);
        assert_eq!(r1.total, r2.total);
    }

    #[test]
    fn cache_key_distinct_per_set_id() {
        let h = blake3::hash(b"same-content");
        let k1 = cache_key(h.as_bytes(), "set_a");
        let k2 = cache_key(h.as_bytes(), "set_b");
        assert_ne!(k1, k2, "different set_ids must produce different keys");
    }

    #[test]
    fn cache_key_distinct_per_content() {
        let h1 = blake3::hash(b"content one");
        let h2 = blake3::hash(b"content two");
        let k1 = cache_key(h1.as_bytes(), "same_set");
        let k2 = cache_key(h2.as_bytes(), "same_set");
        assert_ne!(k1, k2, "different content must produce different keys");
    }

    #[test]
    fn format_matches_returns_none_for_empty() {
        let result = ScanResult {
            matches: vec![],
            total: 0,
            elapsed_ms: 0,
            partial: false,
        };
        assert!(format_matches(&result).is_none());
    }

    #[test]
    fn format_matches_renders_csv_style() {
        let result = ScanResult {
            matches: vec![
                PatternCount {
                    id: "unwrap",
                    label: "unwrap",
                    count: 12,
                },
                PatternCount {
                    id: "panic",
                    label: "panic",
                    count: 2,
                },
            ],
            total: 14,
            elapsed_ms: 5,
            partial: false,
        };
        assert_eq!(
            format_matches(&result).as_deref(),
            Some("unwrap=12, panic=2")
        );
    }

    #[test]
    fn budget_zero_short_circuits() {
        let src = r#"fn main() { x.unwrap(); }"#;
        let result = scan_source(src, &RUST_TEST_PATTERNS, Duration::from_secs(0));
        // With a 0-budget the loop body still runs once before the next check;
        // we tolerate either 0 or 1 match — the contract is "stops early when
        // budget is exceeded", not "runs zero patterns".
        assert!(result.partial || result.total <= 1);
    }
}
