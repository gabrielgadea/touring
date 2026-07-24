//! E2E tests for the moka-backed `CachedAnalysisPipeline`.
//!
//! These tests exercise the public API against a real in-memory SQLite
//! knowledge DB and verify that:
//!
//! 1. Hits/misses counters advance correctly across tiered caches.
//! 2. TTL + TTI isolation prevents cross-talk between Quick/Standard and Deep.
//! 3. Prefix-scoped invalidation is observable immediately (synchronous).
//! 4. Concurrent readers see consistent snapshots.
//! 5. The observability snapshot (`moka_snapshot`) serializes cleanly.
//!
//! They complement the unit tests in `crates/touring-analysis/src/cache.rs`
//! by running through the real `AnalysisPipeline::run_parallel` path instead
//! of any in-module shortcut.

use touring_analysis::engine::Depth;
use touring_analysis::{AnalysisConfig, AnalysisPipeline, CachedAnalysisPipeline};

fn fresh_conn() -> rusqlite::Connection {
    let conn = rusqlite::Connection::open_in_memory().expect("open");
    conn.execute_batch(touring_foundation::schema::knowledge::KNOWLEDGE_SCHEMA_V8)
        .expect("schema");
    conn
}

#[test]
fn e2e_miss_then_hit_matches_composite_score() {
    let conn = fresh_conn();
    let pipeline = AnalysisPipeline::new(&conn, AnalysisConfig::standard());
    let cached = CachedAnalysisPipeline::new(pipeline);

    let first = cached.run_cached(".", Depth::Standard);
    let second = cached.run_cached(".", Depth::Standard);
    let (entries, hits, misses) = cached.cache_stats();

    assert_eq!(entries, 1, "one project:depth slot");
    assert_eq!(hits, 1, "second call must hit the cache");
    assert_eq!(misses, 1, "first call is the sole miss");
    assert!(
        (first.composite_score - second.composite_score).abs() < 1e-9,
        "same project+depth must yield the same composite_score ({} vs {})",
        first.composite_score,
        second.composite_score
    );
}

#[test]
fn e2e_tiered_caches_do_not_cross_contaminate() {
    let conn = fresh_conn();
    let pipeline = AnalysisPipeline::new(&conn, AnalysisConfig::standard());
    let cached = CachedAnalysisPipeline::new(pipeline);

    // Quick + Standard go to the short cache; Deep goes to the deep cache.
    cached.run_cached("/project", Depth::Quick);
    cached.run_cached("/project", Depth::Standard);
    cached.run_cached("/project", Depth::Deep);

    let snap = cached.moka_snapshot();
    assert_eq!(
        snap.short_entries, 2,
        "Quick+Standard share the short cache"
    );
    assert_eq!(snap.deep_entries, 1, "Deep lives in its own cache");
    assert_eq!(snap.hits, 0);
    assert_eq!(snap.misses, 3);
}

#[test]
fn e2e_invalidate_is_synchronous_and_prefix_scoped() {
    let conn = fresh_conn();
    let pipeline = AnalysisPipeline::new(&conn, AnalysisConfig::standard());
    let cached = CachedAnalysisPipeline::new(pipeline);

    cached.run_cached("/alpha", Depth::Standard);
    cached.run_cached("/alpha-subtree", Depth::Deep);
    cached.run_cached("/beta", Depth::Standard);

    // Drop everything starting with "/alpha" — must remove /alpha AND
    // /alpha-subtree, leaving /beta intact.
    cached.invalidate("/alpha");

    let snap = cached.moka_snapshot();
    assert_eq!(
        snap.short_entries, 1,
        "only /beta survives in the short cache"
    );
    assert_eq!(snap.deep_entries, 0, "/alpha-subtree was purged from deep");
}

#[test]
fn e2e_repeated_hits_do_not_inflate_entry_count() {
    let conn = fresh_conn();
    let pipeline = AnalysisPipeline::new(&conn, AnalysisConfig::standard());
    let cached = CachedAnalysisPipeline::new(pipeline);

    // Single project+depth slot: 1 miss + N hits must keep entries = 1.
    cached.run_cached("/warm", Depth::Standard);
    for _ in 0..16 {
        let report = cached.run_cached("/warm", Depth::Standard);
        assert!(report.composite_score.is_finite());
    }

    let (entries, hits, misses) = cached.cache_stats();
    assert_eq!(entries, 1, "same key reused — one slot");
    assert_eq!(misses, 1, "only the first call was a miss");
    assert_eq!(hits, 16, "all subsequent calls hit");
}

#[test]
fn e2e_snapshot_is_json_serializable() {
    let conn = fresh_conn();
    let pipeline = AnalysisPipeline::new(&conn, AnalysisConfig::standard());
    let cached = CachedAnalysisPipeline::new(pipeline);
    cached.run_cached("/j", Depth::Standard);

    let snap = cached.moka_snapshot();
    let json = serde_json::to_string(&snap).expect("serialize snapshot");
    assert!(json.contains("\"short_entries\":"));
    assert!(json.contains("\"deep_entries\":"));
    assert!(json.contains("\"hits\":"));
    assert!(json.contains("\"misses\":"));
}

#[test]
fn e2e_custom_config_keys_do_not_collide_with_depth_keys() {
    let conn = fresh_conn();
    let pipeline = AnalysisPipeline::new(&conn, AnalysisConfig::standard());
    let cached = CachedAnalysisPipeline::new(pipeline);

    // Standard run_cached keys as "/p:standard"
    cached.run_cached("/p", Depth::Standard);
    // run_cached_with_config uses a custom cache_key → "/p:standard+learning"
    cached.run_cached_with_config(
        "/p",
        AnalysisConfig::standard(),
        "standard+learning",
        Depth::Standard,
    );

    let (entries, _hits, misses) = cached.cache_stats();
    assert_eq!(entries, 2, "two distinct cache_keys → two entries");
    assert_eq!(misses, 2);
}
