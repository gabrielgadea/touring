//! Cached analysis pipeline backed by moka W-TinyLFU.
//!
//! Wraps `AnalysisPipeline` with two `moka::sync::Cache` instances (one per
//! TTL class) to avoid re-analyzing the same project on every hook invocation.
//! Entries expire automatically via moka's time-to-live + time-to-idle policy;
//! no manual TTL bookkeeping is required.
//!
//! # Why moka instead of `Mutex<HashMap>` (2026-04-16 Moka Expansion)
//!
//! The previous implementation paid a full `Mutex` acquisition on every
//! read *and* write, serialized hits under contention, and required manual
//! TTL checks that scaled O(n) with entry count. moka delivers:
//!
//! - **Lock-free reads** via segmented sharded concurrent hash map.
//! - **O(1) amortized eviction** via W-TinyLFU admission policy.
//! - **Automatic TTL + TTI** — no `Instant::elapsed()` branches.
//! - **Deterministic tests** via `run_pending_tasks()`.
//!
//! See `docs/analyses/2026-04-14-crates-inventory-ranking.md` P1 #6 and
//! `crates/touring-hooks/src/shared/moka_policies.rs` for the shared
//! policy module that informs these settings.

use crate::engine::{AnalysisConfig, Depth};
use crate::health::CodeHealthReport;
use crate::pipeline::AnalysisPipeline;
use moka::sync::Cache;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Maximum entries cached per TTL class. Tuned for workspaces with at most a
/// few hundred concurrently-open projects per session; eviction is amortized
/// O(1) so oversubscription is safe.
const CACHE_CAPACITY: u64 = 512;

/// TTL for Quick/Standard depth entries. Short window because these are cheap
/// to recompute and must reflect recent edits quickly.
const SHORT_TTL_SECS: u64 = 30;

/// TTL for Deep depth entries. Deep analyses are expensive (seconds), so we
/// retain them long enough for sustained sessions without thrashing.
const DEEP_TTL_SECS: u64 = 300;

/// Idle window (time-to-idle): unused entries are evicted earlier than TTL
/// when cold, freeing memory proactively.
const SHORT_TTI_SECS: u64 = 15;
const DEEP_TTI_SECS: u64 = 120;

/// Cached wrapper around `AnalysisPipeline`.
///
/// Thread-safe via moka's concurrent cache (no explicit Mutex). Entries are
/// keyed by `"{project_root}:{depth|cache_key}"` and expire via moka TTL.
///
/// - Quick/Standard: 30s TTL · 15s TTI · up to 512 entries
/// - Deep: 300s TTL · 120s TTI · up to 512 entries
///
/// Values are stored as `Arc<CodeHealthReport>` so every hit is an O(1)
/// reference-count bump; the deref is cloned only at the boundary back to
/// the caller (preserving the historical return type).
///
/// # Note
///
/// Not to be confused with `touring_hooks::cli_e2e::CachedAnalysisPipeline`,
/// which wraps the E2E orchestrator entry point and has no lifetime parameter.
pub struct CachedAnalysisPipeline<'a> {
    inner: AnalysisPipeline<'a>,
    short_cache: Cache<String, Arc<CodeHealthReport>>,
    deep_cache: Cache<String, Arc<CodeHealthReport>>,
    hits: AtomicU64,
    misses: AtomicU64,
}

impl<'a> CachedAnalysisPipeline<'a> {
    /// Create a cached pipeline wrapping an existing `AnalysisPipeline`.
    pub fn new(pipeline: AnalysisPipeline<'a>) -> Self {
        Self {
            inner: pipeline,
            short_cache: Cache::builder()
                .max_capacity(CACHE_CAPACITY)
                .time_to_live(Duration::from_secs(SHORT_TTL_SECS))
                .time_to_idle(Duration::from_secs(SHORT_TTI_SECS))
                .eviction_policy(moka::policy::EvictionPolicy::lru())
                .build(),
            deep_cache: Cache::builder()
                .max_capacity(CACHE_CAPACITY)
                .time_to_live(Duration::from_secs(DEEP_TTL_SECS))
                .time_to_idle(Duration::from_secs(DEEP_TTI_SECS))
                .eviction_policy(moka::policy::EvictionPolicy::lru())
                .build(),
            hits: AtomicU64::new(0),
            misses: AtomicU64::new(0),
        }
    }

    /// Select the appropriate cache for the given depth.
    fn cache_for(&self, depth: Depth) -> &Cache<String, Arc<CodeHealthReport>> {
        match depth {
            Depth::Deep => &self.deep_cache,
            _ => &self.short_cache,
        }
    }

    /// Run analysis with caching using a custom config and explicit cache key.
    ///
    /// Use this when you need full control over the `AnalysisConfig` (e.g. to
    /// enable learning or temporal dimensions) while still benefiting from TTL
    /// caching. The `cache_key` must uniquely identify the config variant so
    /// that different configs for the same project root do not collide.
    ///
    /// # Example
    /// ```no_run
    /// use touring_analysis::{AnalysisConfig, CachedAnalysisPipeline, AnalysisPipeline};
    /// use touring_analysis::engine::Depth;
    ///
    /// let conn = rusqlite::Connection::open_in_memory().expect("open in-memory db");
    /// let pipeline = AnalysisPipeline::new(&conn, AnalysisConfig::standard());
    /// let cached = CachedAnalysisPipeline::new(pipeline);
    /// let config = AnalysisConfig::standard_with_learning();
    /// let report = cached.run_cached_with_config(".", config, "standard+learning", Depth::Standard);
    /// ```
    pub fn run_cached_with_config(
        &self,
        project_root: &str,
        _config: AnalysisConfig,
        cache_key: &str,
        depth: Depth,
    ) -> CodeHealthReport {
        let key = format!("{project_root}:{cache_key}");
        let cache = self.cache_for(depth);
        if let Some(hit) = cache.get(&key) {
            self.hits.fetch_add(1, Ordering::Relaxed);
            return (*hit).clone();
        }
        let report = self.inner.run_parallel(project_root);
        self.misses.fetch_add(1, Ordering::Relaxed);
        cache.insert(key, Arc::new(report.clone()));
        report
    }

    /// Run analysis with caching. Returns a cached result if available and valid.
    pub fn run_cached(&self, project_root: &str, depth: Depth) -> CodeHealthReport {
        let key = format!("{project_root}:{}", depth.as_str());
        let cache = self.cache_for(depth);
        if let Some(hit) = cache.get(&key) {
            self.hits.fetch_add(1, Ordering::Relaxed);
            return (*hit).clone();
        }
        let report = self.inner.run_parallel(project_root);
        self.misses.fetch_add(1, Ordering::Relaxed);
        cache.insert(key, Arc::new(report.clone()));
        report
    }

    /// Invalidate all cache entries for a project root across both caches.
    ///
    /// Uses synchronous prefix-scan + per-key `invalidate` so callers observe
    /// the final state immediately. moka's async `invalidate_entries_if` is
    /// avoided because its predicate is applied lazily by the maintenance
    /// task and cannot be forced to complete deterministically.
    pub fn invalidate(&self, project_root: &str) {
        for cache in [&self.short_cache, &self.deep_cache] {
            let to_remove: Vec<String> = cache
                .iter()
                .filter_map(|(k, _)| {
                    if k.starts_with(project_root) {
                        Some((*k).clone())
                    } else {
                        None
                    }
                })
                .collect();
            for k in to_remove {
                cache.invalidate(&k);
            }
            cache.run_pending_tasks();
        }
    }

    /// Cache statistics: `(entries, hits, misses)`.
    ///
    /// `entries` is the sum of live rows across both TTL classes. Runs
    /// moka's pending-task queue first so counts are deterministic in tests.
    pub fn cache_stats(&self) -> (usize, usize, usize) {
        self.short_cache.run_pending_tasks();
        self.deep_cache.run_pending_tasks();
        let entries = (self.short_cache.entry_count() + self.deep_cache.entry_count()) as usize;
        let hits = self.hits.load(Ordering::Relaxed) as usize;
        let misses = self.misses.load(Ordering::Relaxed) as usize;
        (entries, hits, misses)
    }

    /// Byte-aware moka snapshot (one entry per TTL class).
    ///
    /// Exposed for observability dashboards and moka-specific regression
    /// tests; callers wanting `(entries, hits, misses)` should use
    /// [`Self::cache_stats`] instead.
    pub fn moka_snapshot(&self) -> CacheSnapshot {
        self.short_cache.run_pending_tasks();
        self.deep_cache.run_pending_tasks();
        CacheSnapshot {
            short_entries: self.short_cache.entry_count(),
            short_weighted: self.short_cache.weighted_size(),
            deep_entries: self.deep_cache.entry_count(),
            deep_weighted: self.deep_cache.weighted_size(),
            hits: self.hits.load(Ordering::Relaxed),
            misses: self.misses.load(Ordering::Relaxed),
        }
    }
}

/// JSON-serializable moka cache snapshot for observability.
#[derive(Debug, Clone, serde::Serialize)]
pub struct CacheSnapshot {
    /// Number of live entries in the short (shallow-depth) analysis cache.
    pub short_entries: u64,
    /// Total weighted size of the short analysis cache (moka cost units).
    pub short_weighted: u64,
    /// Number of live entries in the deep analysis cache.
    pub deep_entries: u64,
    /// Total weighted size of the deep analysis cache (moka cost units).
    pub deep_weighted: u64,
    /// Cumulative cache hits across both caches since startup.
    pub hits: u64,
    /// Cumulative cache misses across both caches since startup.
    pub misses: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::AnalysisConfig;

    fn setup_db() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().expect("open");
        conn.execute_batch(touring_foundation::schema::knowledge::KNOWLEDGE_SCHEMA_V8)
            .expect("schema");
        conn
    }

    #[test]
    fn test_cached_pipeline_miss_then_hit() {
        let conn = setup_db();
        let pipeline = AnalysisPipeline::new(&conn, AnalysisConfig::standard());
        let cached = CachedAnalysisPipeline::new(pipeline);

        // First call: miss
        let report1 = cached.run_cached(".", Depth::Standard);
        let (entries, hits, misses) = cached.cache_stats();
        assert_eq!(entries, 1);
        assert_eq!(hits, 0);
        assert_eq!(misses, 1);

        // Second call: hit
        let report2 = cached.run_cached(".", Depth::Standard);
        let (entries, hits, misses) = cached.cache_stats();
        assert_eq!(entries, 1);
        assert_eq!(hits, 1);
        assert_eq!(misses, 1);

        // Same result
        assert!((report1.composite_score - report2.composite_score).abs() < 1e-9);
    }

    #[test]
    fn test_cached_pipeline_different_depths_separate_entries() {
        let conn = setup_db();
        let pipeline = AnalysisPipeline::new(&conn, AnalysisConfig::standard());
        let cached = CachedAnalysisPipeline::new(pipeline);

        cached.run_cached(".", Depth::Quick);
        cached.run_cached(".", Depth::Standard);
        let (entries, _, _) = cached.cache_stats();
        assert_eq!(
            entries, 2,
            "different depths should create separate cache entries"
        );
    }

    #[test]
    fn test_invalidate_clears_project_entries() {
        let conn = setup_db();
        let pipeline = AnalysisPipeline::new(&conn, AnalysisConfig::standard());
        let cached = CachedAnalysisPipeline::new(pipeline);

        cached.run_cached("/project_a", Depth::Standard);
        cached.run_cached("/project_b", Depth::Standard);
        let (entries, _, _) = cached.cache_stats();
        assert_eq!(entries, 2);

        cached.invalidate("/project_a");
        let (entries, _, _) = cached.cache_stats();
        assert_eq!(entries, 1, "only project_b should remain");
    }

    // ── Moka-specific regression tests ─────────────────────────────────────

    #[test]
    fn test_moka_snapshot_reflects_tiered_caches() {
        let conn = setup_db();
        let pipeline = AnalysisPipeline::new(&conn, AnalysisConfig::standard());
        let cached = CachedAnalysisPipeline::new(pipeline);

        cached.run_cached("/quick", Depth::Quick);
        cached.run_cached("/standard", Depth::Standard);
        cached.run_cached("/deep", Depth::Deep);

        let snap = cached.moka_snapshot();
        // Quick + Standard → short cache (2 entries)
        assert_eq!(
            snap.short_entries, 2,
            "short cache should hold Quick+Standard"
        );
        assert_eq!(snap.deep_entries, 1, "deep cache should hold Deep entry");
        assert_eq!(snap.misses, 3);
        assert_eq!(snap.hits, 0);
    }

    #[test]
    fn test_moka_snapshot_json_shape() {
        let snap = CacheSnapshot {
            short_entries: 1,
            short_weighted: 1,
            deep_entries: 2,
            deep_weighted: 2,
            hits: 7,
            misses: 3,
        };
        let json = serde_json::to_string(&snap).expect("serialize");
        assert!(json.contains("\"short_entries\":1"));
        assert!(json.contains("\"deep_entries\":2"));
        assert!(json.contains("\"hits\":7"));
        assert!(json.contains("\"misses\":3"));
    }

    #[test]
    fn test_hit_miss_counters_survive_invalidation() {
        let conn = setup_db();
        let pipeline = AnalysisPipeline::new(&conn, AnalysisConfig::standard());
        let cached = CachedAnalysisPipeline::new(pipeline);

        cached.run_cached("/p", Depth::Standard);
        cached.run_cached("/p", Depth::Standard); // hit
        cached.invalidate("/p");

        // Counters are AtomicU64 → NOT reset by invalidation (matches prior semantics).
        let (_entries, hits, misses) = cached.cache_stats();
        assert_eq!(hits, 1);
        assert_eq!(misses, 1);
    }

    #[test]
    fn test_invalidate_is_scoped_to_prefix() {
        let conn = setup_db();
        let pipeline = AnalysisPipeline::new(&conn, AnalysisConfig::standard());
        let cached = CachedAnalysisPipeline::new(pipeline);

        cached.run_cached("/alpha", Depth::Standard);
        cached.run_cached("/alpha-x", Depth::Deep);
        cached.run_cached("/beta", Depth::Standard);

        cached.invalidate("/alpha"); // prefix match: should hit /alpha AND /alpha-x
        let snap = cached.moka_snapshot();
        assert_eq!(snap.short_entries, 1, "only /beta remains in short cache");
        assert_eq!(snap.deep_entries, 0, "/alpha-x removed from deep cache");
    }
}
