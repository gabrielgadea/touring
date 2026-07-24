//! E2E tests for the moka-backed query-embedding cache in `ContextualReranker`.
//!
//! These tests exercise the cache through its public API (`cache_query_embedding`,
//! `lookup_query_embedding`, `clear_query_cache`, `stats`) and verify that:
//!
//! 1. Embeddings round-trip losslessly through `Arc<Vec<f32>>`.
//! 2. Hits/misses counters are accurate under mixed read patterns.
//! 3. `stats().cached_bytes` reflects f32-aware weigher output.
//! 4. `clear_query_cache` removes everything synchronously.
//! 5. The cache is usable via `&self` (the moka migration amplified
//!    thread-safety by dropping the `&mut self` requirement).

use std::sync::Arc;

use touring_intelligence::ann::reranker::ContextualReranker;

#[test]
fn e2e_insert_then_lookup_returns_arc() {
    let reranker = ContextualReranker::new();
    reranker.cache_query_embedding("q1", vec![0.1_f32, 0.2, 0.3, 0.4]);

    let got = reranker.lookup_query_embedding("q1").expect("hit");
    assert_eq!(got.len(), 4);
    assert!((got[0] - 0.1).abs() < 1e-6);
    assert!((got[3] - 0.4).abs() < 1e-6);
}

#[test]
fn e2e_miss_increments_miss_counter() {
    let reranker = ContextualReranker::new();
    assert!(reranker.lookup_query_embedding("nonexistent").is_none());

    let stats = reranker.stats();
    assert_eq!(stats.query_cache_hits, 0);
    assert_eq!(stats.query_cache_misses, 1);
}

#[test]
fn e2e_hit_increments_hit_counter() {
    let reranker = ContextualReranker::new();
    reranker.cache_query_embedding("q", vec![1.0, 2.0, 3.0]);

    // Three lookups: all hits.
    for _ in 0..3 {
        assert!(reranker.lookup_query_embedding("q").is_some());
    }

    let stats = reranker.stats();
    assert_eq!(stats.query_cache_hits, 3);
    assert_eq!(stats.query_cache_misses, 0);
}

#[test]
fn e2e_cached_bytes_reflects_f32_weigher() {
    let reranker = ContextualReranker::new();
    // 100 × f32 = 400 bytes per entry.
    reranker.cache_query_embedding("a", vec![0.0_f32; 100]);
    reranker.cache_query_embedding("b", vec![0.0_f32; 100]);

    let stats = reranker.stats();
    assert_eq!(stats.cached_queries, 2);
    assert_eq!(stats.cached_bytes, 800, "2 × 100 × sizeof(f32) = 800 bytes");
}

#[test]
fn e2e_clear_query_cache_removes_all_entries() {
    let reranker = ContextualReranker::new();
    for i in 0..10 {
        reranker.cache_query_embedding(&format!("q{i}"), vec![i as f32; 8]);
    }
    assert_eq!(reranker.stats().cached_queries, 10);

    reranker.clear_query_cache();
    let stats = reranker.stats();
    assert_eq!(stats.cached_queries, 0, "clear must drop every entry");
    assert_eq!(stats.cached_bytes, 0);
}

#[test]
fn e2e_repeated_insert_same_key_overwrites_without_growth() {
    let reranker = ContextualReranker::new();
    for i in 0..100 {
        reranker.cache_query_embedding("key", vec![i as f32; 4]);
    }
    let stats = reranker.stats();
    assert_eq!(stats.cached_queries, 1, "same key — one slot");
    assert_eq!(stats.cached_bytes, 16, "4 × sizeof(f32) = 16");
}

#[test]
fn e2e_cache_api_works_through_immutable_reference() {
    // Proves that the API is now reachable from `&ContextualReranker` —
    // callers no longer need `&mut` just to cache a query embedding.
    let reranker = ContextualReranker::new();
    fn use_cache(r: &ContextualReranker) {
        r.cache_query_embedding("immut", vec![0.5_f32; 4]);
        let _ = r.lookup_query_embedding("immut");
        r.clear_query_cache();
    }
    use_cache(&reranker);
    assert_eq!(reranker.stats().cached_queries, 0);
}

#[test]
fn e2e_arc_values_share_underlying_storage() {
    let reranker = ContextualReranker::new();
    reranker.cache_query_embedding("q", vec![7.0_f32; 4]);

    let a: Arc<Vec<f32>> = reranker.lookup_query_embedding("q").expect("hit");
    let b: Arc<Vec<f32>> = reranker.lookup_query_embedding("q").expect("hit");

    // Both handles should point at the same allocation — proof that hits
    // return an `Arc` clone rather than an expensive Vec deep copy.
    assert!(Arc::ptr_eq(&a, &b), "moka returns clones of the same Arc");
}
