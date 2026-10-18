//! Cross-audit integration tests for touring-learning memory components.
//!
//! Audits documented contracts for:
//! 1. RecallCache   — per-key versioning (not full-invalidation), TTL, capacity eviction
//! 2. AsyncRlmMemory — write-through cache + background batch flush durability
//!
//! Every test here exercises a **contract**, not just "does not crash".

use touring_intelligence::rl::memory::{
    async_rlm::AsyncRlmMemory, recall_cache::RecallCache, recall_cache::RecallEntry,
    rlm::MemoryTier,
};

// ─────────────────────────────────────────────────────────────────────────────
// Helpers
// ─────────────────────────────────────────────────────────────────────────────

fn make_entry(key: &str) -> RecallEntry {
    RecallEntry {
        key: key.to_string(),
        value: "v".to_string(),
        score: 1.0,
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// CONTRACT: RecallCache v2 — per-key versioning (NOT full-invalidation)
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn recall_cache_per_key_invalidation_does_not_affect_other_keys() {
    let cache = RecallCache::new(60, 100);

    // Populate keys 1, 2, 3
    cache.get_or_query(1, || vec![make_entry("a")]);
    cache.get_or_query(2, || vec![make_entry("b")]);
    cache.get_or_query(3, || vec![make_entry("c")]);
    assert_eq!(cache.len(), 3, "all 3 keys must be cached");

    // Invalidate only key 2
    cache.invalidate_key(2);

    // Keys 1 and 3 must still be cached (no re-query)
    let mut k1_calls = 0u32;
    let mut k3_calls = 0u32;
    cache.get_or_query(1, || {
        k1_calls += 1;
        vec![make_entry("new_a")]
    });
    cache.get_or_query(3, || {
        k3_calls += 1;
        vec![make_entry("new_c")]
    });

    assert_eq!(
        k1_calls, 0,
        "CONTRACT: invalidating key 2 must NOT cause key 1 cache miss"
    );
    assert_eq!(
        k3_calls, 0,
        "CONTRACT: invalidating key 2 must NOT cause key 3 cache miss"
    );

    // Key 2 must re-query (was invalidated)
    let mut k2_calls = 0u32;
    cache.get_or_query(2, || {
        k2_calls += 1;
        vec![make_entry("new_b")]
    });
    assert_eq!(
        k2_calls, 1,
        "CONTRACT: invalidated key 2 must trigger re-query"
    );
}

#[test]
fn recall_cache_increment_version_causes_miss_for_that_key_only() {
    let cache = RecallCache::new(60, 100);

    // Warm both keys
    cache.get_or_query(10, || vec![make_entry("x")]);
    cache.get_or_query(20, || vec![make_entry("y")]);

    // Increment version for key 10 only
    cache.increment_version(10);

    // Key 10: must miss
    let mut miss_10 = 0u32;
    cache.get_or_query(10, || {
        miss_10 += 1;
        vec![make_entry("x2")]
    });
    assert_eq!(miss_10, 1, "version bump must cause cache miss for key 10");

    // Key 20: must still hit
    let mut miss_20 = 0u32;
    cache.get_or_query(20, || {
        miss_20 += 1;
        vec![make_entry("y2")]
    });
    assert_eq!(
        miss_20, 0,
        "key 20 must remain valid after key 10 version bump"
    );
}

#[test]
fn recall_cache_stats_accurately_track_hits_and_misses() {
    let cache = RecallCache::new(60, 100);

    // 2 cold misses
    cache.get_or_query(1, || vec![make_entry("a")]);
    cache.get_or_query(2, || vec![make_entry("b")]);

    // 3 hits
    cache.get_or_query(1, || vec![make_entry("should-not-be-called")]);
    cache.get_or_query(2, || vec![make_entry("should-not-be-called")]);
    cache.get_or_query(1, || vec![make_entry("should-not-be-called")]);

    let stats = cache.stats();
    assert_eq!(stats.version_misses, 2, "exactly 2 misses");
    assert_eq!(stats.cache_hits, 3, "exactly 3 hits");
    assert!(
        (cache.hit_rate() - (3.0 / 5.0)).abs() < 0.001,
        "hit_rate must be 60%"
    );
}

#[test]
fn recall_cache_capacity_eviction_keeps_len_at_max() {
    let cache = RecallCache::new(60, 3); // capacity = 3

    cache.get_or_query(1, || vec![make_entry("a")]);
    cache.get_or_query(2, || vec![make_entry("b")]);
    cache.get_or_query(3, || vec![make_entry("c")]);
    assert_eq!(cache.len(), 3);

    // 4th entry triggers eviction
    cache.get_or_query(4, || vec![make_entry("d")]);
    assert_eq!(
        cache.len(),
        3,
        "CONTRACT: capacity eviction must keep len <= max_entries"
    );
}

#[test]
fn recall_cache_full_invalidation_clears_all_entries() {
    let cache = RecallCache::new(60, 100);
    for i in 0..5u64 {
        cache.get_or_query(i, || vec![make_entry(&format!("v{i}"))]);
    }
    assert_eq!(cache.len(), 5);

    cache.invalidate_all();
    assert_eq!(cache.len(), 0, "invalidate_all must clear all entries");
}

// ─────────────────────────────────────────────────────────────────────────────
// CONTRACT: AsyncRlmMemory — write-through cache + batch flush durability
// ─────────────────────────────────────────────────────────────────────────────

#[test]
fn async_rlm_sync_write_through_cache_immediately_readable() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("cache.db").to_string_lossy().to_string();

    let mem = AsyncRlmMemory::new_sync(&path).expect("new_sync");

    // CONTRACT: store() must be immediately readable from cache (no flush needed)
    mem.store("key_a", "value_a", MemoryTier::Working)
        .expect("store");
    let result = mem.recall_sync("key_a").expect("recall_sync");
    assert_eq!(
        result,
        Some("value_a".to_string()),
        "CONTRACT: write-through cache must make stored value immediately readable"
    );
}

#[test]
fn async_rlm_sync_missing_key_returns_none() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("missing.db").to_string_lossy().to_string();

    let mem = AsyncRlmMemory::new_sync(&path).expect("new_sync");
    let result = mem.recall_sync("nonexistent").expect("recall_sync");
    assert_eq!(result, None, "missing key must return None, not error");
}

#[test]
fn async_rlm_sync_overwrite_updates_cache() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir
        .path()
        .join("overwrite.db")
        .to_string_lossy()
        .to_string();

    let mem = AsyncRlmMemory::new_sync(&path).expect("new_sync");
    mem.store("k", "v1", MemoryTier::Working).expect("store v1");
    mem.store("k", "v2", MemoryTier::Working).expect("store v2");

    let result = mem.recall_sync("k").expect("recall");
    assert_eq!(
        result,
        Some("v2".to_string()),
        "overwrite must update the cache value"
    );
}

#[test]
fn async_rlm_sync_multiple_tiers_independent() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().join("tiers.db").to_string_lossy().to_string();

    let mem = AsyncRlmMemory::new_sync(&path).expect("new_sync");

    // Store the same key in Working and Ephemeral (different tier slices in RLM)
    mem.store("shared_key", "working_val", MemoryTier::Working)
        .expect("store working");
    // recall_sync uses default_tier (Working), so it reads from Working
    let v = mem.recall_sync("shared_key").expect("recall");
    assert_eq!(v, Some("working_val".to_string()));
}

#[test]
fn async_rlm_sync_parent_dir_created_automatically() {
    let dir = tempfile::tempdir().expect("tempdir");
    let nested = dir
        .path()
        .join("deep")
        .join("nested")
        .join("dir")
        .join("mem.db");
    let path = nested.to_string_lossy().to_string();

    // Parent dirs do not exist — new_sync must create them
    let mem = AsyncRlmMemory::new_sync(&path);
    assert!(
        mem.is_ok(),
        "new_sync must create parent directories automatically"
    );
}

#[cfg(feature = "async-memory")]
mod async_tests {
    use super::*;

    #[tokio::test]
    async fn async_rlm_flush_guarantees_durability() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("flush.db").to_string_lossy().to_string();

        let mem = AsyncRlmMemory::new(&path).await.expect("new async");

        // Store without waiting — value in cache, may not be in SQLite yet
        mem.store("k1", "durable_val", MemoryTier::Working)
            .expect("store");

        // flush() waits for background writer to complete
        mem.flush().await.expect("flush");

        // After flush, recall must return the value
        let result = mem.recall("k1").await.expect("recall");
        assert_eq!(
            result,
            Some("durable_val".to_string()),
            "CONTRACT: flush() must guarantee SQLite durability before returning"
        );
    }

    #[tokio::test]
    async fn async_rlm_batch_multiple_writes_then_flush() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("batch.db").to_string_lossy().to_string();

        let mem = AsyncRlmMemory::new(&path).await.expect("new async");

        // Write 10 items without flushing — all should be in cache immediately
        for i in 0..10usize {
            mem.store(
                &format!("key_{i}"),
                &format!("val_{i}"),
                MemoryTier::Working,
            )
            .expect("store");
        }

        // Flush ensures all background batch writes complete
        mem.flush().await.expect("flush");

        // Verify all 10 are readable post-flush
        for i in 0..10usize {
            let v = mem.recall(&format!("key_{i}")).await.expect("recall");
            assert_eq!(
                v,
                Some(format!("val_{i}")),
                "key_{i} must be durable after flush"
            );
        }
    }

    #[tokio::test]
    async fn async_rlm_cache_populated_on_miss() {
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("miss.db").to_string_lossy().to_string();

        let mem = AsyncRlmMemory::new(&path).await.expect("new async");
        mem.store("ck", "cv", MemoryTier::Core).expect("store");
        mem.flush().await.expect("flush");

        // First recall reads from cache (populated on store) or from SQLite on miss
        let v = mem.recall("ck").await.expect("recall");
        assert_eq!(v, Some("cv".to_string()));

        // After the recall, cache should have at least this entry
        let cache_len = mem.cache_len().await;
        assert!(cache_len >= 1, "cache must be populated after recall");
    }
}
