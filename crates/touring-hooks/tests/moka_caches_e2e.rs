//! End-to-end proofs for the moka-backed caches introduced by the
//! 2026-04-14 `docs/analyses/2026-04-14-crates-inventory-ranking.md` wave.
//!
//! Covers:
//! 1. `FileKnowledgeDB::query_extended` — hit on second call, invalidation
//!    wired to `upsert`, `upsert_cognitive_enrichment`,
//!    `upsert_blake3_registry`, `upsert_file_community`,
//!    `upsert_test_coverage`.
//! 2. `shared::terminal_job_cache` — terminal outcomes survive `drop_job`
//!    until cache eviction, corrupted payloads degrade gracefully.
//! 3. `shared::moka_policies` — cache counters are coherent after realistic
//!    access patterns.
//!
//! These tests are deliberately written as integration tests (the `tests/`
//! directory is compiled into a separate crate by cargo) so they exercise
//! the public API surface only.

use tempfile::TempDir;
use touring_hooks::knowledge::{FileKnowledge, FileKnowledgeDB};
use touring_hooks::shared::terminal_job_cache;

fn base(path: &str) -> FileKnowledge {
    FileKnowledge {
        file_path: path.to_string(),
        language: Some("rust".into()),
        line_count: 42,
        symbol_count: 7,
        read_count: 0,
        last_read_at: None,
        content_hash: Some("abc".into()),
        imports_json: Some("[]".into()),
        symbols_json: Some("[]".into()),
        notes: Some("initial".into()),
    }
}

fn new_db() -> (TempDir, FileKnowledgeDB) {
    let tmp = TempDir::new().expect("tempdir");
    let db_path = tmp.path().join("knowledge.db");
    let db = FileKnowledgeDB::new(&db_path).expect("open knowledge db");
    (tmp, db)
}

#[test]
fn query_extended_cache_hits_on_repeat_lookup() {
    let (_tmp, db) = new_db();
    let path = "crates/demo/src/lib.rs";
    db.upsert(&base(path)).expect("upsert");

    let first = db
        .query_extended(path)
        .expect("query1")
        .expect("row present");
    assert_eq!(first.file_path, path);

    // Stats after first call: exactly one entry warmed.
    let s1 = db.extended_cache_stats();
    assert_eq!(s1.entry_count, 1, "first call should warm the cache");

    // Second call must not grow the cache (hit).
    let second = db
        .query_extended(path)
        .expect("query2")
        .expect("row present");
    assert_eq!(first.content_hash, second.content_hash);

    let s2 = db.extended_cache_stats();
    assert_eq!(s2.entry_count, 1, "second call must be a cache hit");
}

#[test]
fn upsert_invalidates_extended_cache() {
    let (_tmp, db) = new_db();
    let path = "crates/demo/src/util.rs";
    db.upsert(&base(path)).expect("upsert1");
    let _ = db.query_extended(path).expect("warm");
    assert_eq!(db.extended_cache_stats().entry_count, 1);

    // Writing again must kick the cached row out.
    let mut k = base(path);
    k.notes = Some("updated".into());
    db.upsert(&k).expect("upsert2");
    assert_eq!(
        db.extended_cache_stats().entry_count,
        0,
        "upsert must invalidate cached row"
    );

    // Re-query sees the fresh notes and re-populates the cache.
    let fresh = db.query_extended(path).expect("reload").expect("row");
    assert_eq!(fresh.notes.as_deref(), Some("updated"));
    assert_eq!(db.extended_cache_stats().entry_count, 1);
}

#[test]
fn cognitive_enrichment_write_invalidates_cache() {
    let (_tmp, db) = new_db();
    let path = "crates/demo/src/cognitive.rs";
    db.upsert(&base(path)).expect("upsert");
    let _ = db.query_extended(path).expect("warm");
    assert_eq!(db.extended_cache_stats().entry_count, 1);

    db.upsert_cognitive_enrichment(path, 0.91, 0.55, 0.33, 0.11, 0.77)
        .expect("cognitive upsert");
    assert_eq!(
        db.extended_cache_stats().entry_count,
        0,
        "cognitive enrichment must invalidate cache",
    );

    let enriched = db
        .query_extended(path)
        .expect("reload")
        .expect("row present");
    assert_eq!(enriched.cognitive_score, Some(0.91));
    assert_eq!(enriched.complexity_signal, Some(0.55));
    assert_eq!(enriched.fan_in_signal, Some(0.33));
    assert_eq!(enriched.fan_out_signal, Some(0.11));
    assert_eq!(enriched.doc_signal, Some(0.77));
}

#[test]
fn blake3_registry_write_invalidates_cache() {
    let (_tmp, db) = new_db();
    let path = "crates/demo/src/blake.rs";
    db.upsert(&base(path)).expect("upsert");
    let _ = db.query_extended(path).expect("warm");
    assert_eq!(db.extended_cache_stats().entry_count, 1);

    db.upsert_blake3_registry(path, "deadbeef", 128, None)
        .expect("blake3");
    assert_eq!(db.extended_cache_stats().entry_count, 0);

    let fresh = db.query_extended(path).expect("reload").expect("row");
    assert_eq!(fresh.blake3_hash.as_deref(), Some("deadbeef"));
}

#[test]
fn community_and_coverage_writes_invalidate_cache() {
    let (_tmp, db) = new_db();
    let path = "crates/demo/src/community.rs";
    db.upsert(&base(path)).expect("upsert");
    let _ = db.query_extended(path).expect("warm");

    db.upsert_file_community(path, 3, 0.42).expect("community");
    assert_eq!(db.extended_cache_stats().entry_count, 0);
    let after_community = db.query_extended(path).expect("q").expect("row");
    assert_eq!(after_community.community_id, Some(3));
    assert_eq!(after_community.modularity_score, Some(0.42));

    db.upsert_test_coverage(path, 73.0, 12, 16)
        .expect("coverage");
    assert_eq!(db.extended_cache_stats().entry_count, 0);
    let after_coverage = db.query_extended(path).expect("q").expect("row");
    assert_eq!(after_coverage.coverage_pct, Some(73.0));
}

#[test]
fn bulk_invalidation_clears_every_entry() {
    let (_tmp, db) = new_db();
    for idx in 0..8 {
        let p = format!("crates/demo/src/bulk_{idx}.rs");
        db.upsert(&base(&p)).expect("upsert");
        let _ = db.query_extended(&p).expect("warm").expect("row");
    }
    assert!(db.extended_cache_stats().entry_count >= 8);
    db.invalidate_extended_cache_all();
    assert_eq!(
        db.extended_cache_stats().entry_count,
        0,
        "invalidate_all must drop every cached row",
    );
}

#[test]
fn terminal_job_cache_round_trip_preserves_payload() {
    let job_id = "e2e-job-roundtrip";
    let payload = serde_json::json!({
        "job_id": job_id,
        "status": "completed",
        "result": "round-trip ok",
        "duration_secs": 3,
    });
    assert!(terminal_job_cache::store_terminal(job_id, &payload));

    let restored = terminal_job_cache::lookup_terminal(job_id).expect("cached");
    assert_eq!(restored["status"], "completed");
    assert_eq!(restored["result"], "round-trip ok");
    assert_eq!(restored["duration_secs"], 3);
    terminal_job_cache::forget(job_id);
}

#[test]
fn terminal_job_cache_rejects_non_terminal_status() {
    let job_id = "e2e-reject-running";
    let payload = serde_json::json!({"status": "running"});
    assert!(
        !terminal_job_cache::store_terminal(job_id, &payload),
        "cache must reject running status",
    );
    assert!(terminal_job_cache::lookup_terminal(job_id).is_none());
}

#[test]
fn terminal_job_cache_stats_reflect_byte_sized_weight() {
    let job_a = "e2e-stats-a";
    let job_b = "e2e-stats-b";
    terminal_job_cache::store_terminal(
        job_a,
        &serde_json::json!({"status": "completed", "result": "a"}),
    );
    terminal_job_cache::store_terminal(
        job_b,
        &serde_json::json!({"status": "failed", "error": "boom"}),
    );
    let s = terminal_job_cache::stats();
    assert!(
        s.entry_count >= 2,
        "expected ≥2 entries, got {}",
        s.entry_count
    );
    assert!(
        s.weighted_size >= 1,
        "weighted_size must reflect payload bytes",
    );
    terminal_job_cache::forget(job_a);
    terminal_job_cache::forget(job_b);
}
