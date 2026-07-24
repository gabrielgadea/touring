//! Centralized moka cache policies used across the hook runtime.
//!
//! This module is the single source of truth for cache shapes consumed by:
//! - `FileKnowledgeDB::query_extended` (LEFT JOIN across 6 specialized tables)
//! - `TantivyIndex::{search, fuzzy_search, suggest, search_by_crate}` (BM25 hits)
//! - `job_registry` terminal-state cache (bounded by payload size)
//!
//! The public API exposes builder functions so construction remains an
//! implementation detail — call-sites stay decoupled from moka version
//! upgrades, feature flag changes, and eviction tuning.
//!
//! # Why moka?
//!
//! moka implements W-TinyLFU admission + LRU eviction with weigher closures,
//! matching the workloads seen in Touring (Zipfian access on file paths and
//! queries). Compared to `DashMap` it adds automatic capacity governance and
//! reduces tail latency under hot-path contention.
//!
//! See `docs/analyses/2026-04-14-crates-inventory-ranking.md` for the ranking
//! justification (moka is P1 #6 with Context7 score 8.4).

use moka::sync::Cache;
use std::sync::Arc;
use std::time::Duration;

/// Maximum number of enriched knowledge rows cached in-memory.
///
/// Tuned for typical workspaces (hundreds of files actively edited in a
/// session). Oversubscription is fine — eviction is O(1) amortized.
pub const KNOWLEDGE_EXTENDED_CAPACITY: u64 = 4_096;

/// TTL for enriched knowledge rows. After this window the row is evicted
/// even if accessed, forcing a fresh LEFT JOIN.
pub const KNOWLEDGE_EXTENDED_TTL_SECS: u64 = 120;

/// Idle window for enriched knowledge rows.
pub const KNOWLEDGE_EXTENDED_TTI_SECS: u64 = 60;

/// Maximum number of BM25 query results cached.
///
/// Cache keys include `{op}:{query}:{top_k}` so duplicate queries are
/// absorbed without re-running BM25 scoring.
pub const TANTIVY_QUERY_CAPACITY: u64 = 1_024;

/// TTL for Tantivy query results. Short enough that new indexed symbols
/// are reflected quickly without relying solely on `commit()` invalidation.
pub const TANTIVY_QUERY_TTL_SECS: u64 = 30;

/// Idle window for Tantivy query results.
pub const TANTIVY_QUERY_TTI_SECS: u64 = 15;

/// Maximum bytes retained in the terminal-job cache.
///
/// Jobs produce variable-size stdout payloads; capping by total bytes keeps
/// memory pressure predictable. 32 MiB comfortably absorbs ~5k ordinary
/// jobs or ~100 verbose ones.
pub const TERMINAL_JOB_MAX_BYTES: u64 = 32 * 1024 * 1024;

/// TTL for terminal jobs. Long enough that clients polling slowly still
/// see their result, short enough that stale artifacts are reaped.
pub const TERMINAL_JOB_TTL_SECS: u64 = 30 * 60;

/// Idle window for terminal jobs.
pub const TERMINAL_JOB_TTI_SECS: u64 = 15 * 60;

/// Build the default cache used by `FileKnowledgeDB::query_extended`.
///
/// Value type is `Arc<T>` so clones are O(1) (reference count bump). Keys
/// are normalized file paths as UTF-8 strings.
pub fn build_knowledge_extended_cache<T>() -> Cache<String, Arc<T>>
where
    T: Send + Sync + 'static,
{
    Cache::builder()
        .max_capacity(KNOWLEDGE_EXTENDED_CAPACITY)
        .time_to_live(Duration::from_secs(KNOWLEDGE_EXTENDED_TTL_SECS))
        .time_to_idle(Duration::from_secs(KNOWLEDGE_EXTENDED_TTI_SECS))
        .eviction_policy(moka::policy::EvictionPolicy::lru())
        .build()
}

/// Build the default Tantivy query cache.
///
/// `T` is typically `Vec<SearchHit>`; wrap in `Arc` so read-path clones are
/// free. The weigher charges each entry by the number of hits (approximates
/// memory usage without walking the full struct).
pub fn build_tantivy_query_cache<T>() -> Cache<String, Arc<Vec<T>>>
where
    T: Send + Sync + 'static,
{
    Cache::builder()
        .max_capacity(TANTIVY_QUERY_CAPACITY)
        .time_to_live(Duration::from_secs(TANTIVY_QUERY_TTL_SECS))
        .time_to_idle(Duration::from_secs(TANTIVY_QUERY_TTI_SECS))
        .weigher(|_k: &String, v: &Arc<Vec<T>>| -> u32 {
            v.len().saturating_add(1).min(u32::MAX as usize) as u32
        })
        .eviction_policy(moka::policy::EvictionPolicy::lru())
        .build()
}

/// Build the terminal-job cache (byte-capped via weigher).
///
/// Values are `Arc<TerminalJobRecord>` — payloads carry stdout/stderr and
/// timestamps; the weigher computes the archived byte size.
pub fn build_terminal_job_cache<T, F>(weigher: F) -> Cache<String, Arc<T>>
where
    T: Send + Sync + 'static,
    F: Fn(&Arc<T>) -> u32 + Send + Sync + 'static,
{
    Cache::builder()
        .max_capacity(TERMINAL_JOB_MAX_BYTES)
        .time_to_live(Duration::from_secs(TERMINAL_JOB_TTL_SECS))
        .time_to_idle(Duration::from_secs(TERMINAL_JOB_TTI_SECS))
        .weigher(move |_k: &String, v: &Arc<T>| -> u32 { weigher(v) })
        .eviction_policy(moka::policy::EvictionPolicy::lru())
        .build()
}

/// Uniform snapshot for observability (JSON-friendly).
#[derive(Debug, Clone, serde::Serialize)]
pub struct MokaCacheStats {
    /// Current number of live entries in the cache.
    pub entry_count: u64,
    /// Total weighted size of live entries, per the cache's weigher.
    pub weighted_size: u64,
}

/// Sample a cache's runtime metrics in a single call.
pub fn sample_stats<K, V>(cache: &Cache<K, V>) -> MokaCacheStats
where
    K: Eq + std::hash::Hash + Send + Sync + 'static,
    V: Clone + Send + Sync + 'static,
{
    cache.run_pending_tasks();
    MokaCacheStats {
        entry_count: cache.entry_count(),
        weighted_size: cache.weighted_size(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn knowledge_cache_insert_and_read() {
        let cache: Cache<String, Arc<String>> = build_knowledge_extended_cache();
        cache.insert("a".into(), Arc::new("one".into()));
        assert_eq!(cache.get("a").as_deref().map(|s| s.as_str()), Some("one"));
        assert_eq!(sample_stats(&cache).entry_count, 1);
    }
    #[test]
    fn tantivy_cache_weighs_by_hit_count() {
        let cache: Cache<String, Arc<Vec<u8>>> = build_tantivy_query_cache();
        cache.insert("q1".into(), Arc::new(vec![0_u8; 16]));
        cache.insert("q2".into(), Arc::new(vec![0_u8; 4]));
        let stats = sample_stats(&cache);
        assert_eq!(stats.entry_count, 2);
        assert_eq!(stats.weighted_size, (16 + 1) + (4 + 1));
    }
    #[test]
    fn terminal_job_cache_honors_weigher() {
        #[derive(Debug)]
        struct Record {
            bytes: usize,
        }
        let weigher = |rec: &Arc<Record>| -> u32 { rec.bytes.min(u32::MAX as usize) as u32 };
        let cache: Cache<String, Arc<Record>> = build_terminal_job_cache(weigher);
        cache.insert("job1".into(), Arc::new(Record { bytes: 128 }));
        cache.insert("job2".into(), Arc::new(Record { bytes: 256 }));
        let stats = sample_stats(&cache);
        assert_eq!(stats.entry_count, 2);
        assert_eq!(stats.weighted_size, 128 + 256);
    }
    #[test]
    fn stats_is_serializable() {
        let s = MokaCacheStats {
            entry_count: 7,
            weighted_size: 42,
        };
        let json = serde_json::to_string(&s).expect("serialize");
        assert!(json.contains("\"entry_count\":7"));
        assert!(json.contains("\"weighted_size\":42"));
    }
}
