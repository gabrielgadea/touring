//! Focus Cache — LRU-16 memoization for graph context lookups by focus file.
//!
//! Avoids recomputing graph neighborhoods when switching between files.
//! Uses an LRU cache with capacity 16, so alternating between up to 16 files
//! remains fully cached (vs. single-entry which missed on every file switch).

use std::num::NonZeroUsize;
use std::sync::RwLock;

/// Default LRU capacity — 16 entries covers typical multi-file editing sessions.
const DEFAULT_CAPACITY: usize = 16;

/// Internal state for the LRU focus cache.
struct FocusCacheInner {
    cache: lru::LruCache<String, String>,
    hit_count: u64,
    miss_count: u64,
}

impl std::fmt::Debug for FocusCacheInner {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("FocusCacheInner")
            .field("len", &self.cache.len())
            .field("hit_count", &self.hit_count)
            .field("miss_count", &self.miss_count)
            .finish()
    }
}

/// Cache for graph context lookups by focus file.
///
/// Uses an LRU eviction policy with capacity 16.  Thread-safe via `RwLock`.
/// Read-only stat queries use `read()`, mutations use `write()`.
/// When the cache is full, the least-recently-used entry is evicted.
#[derive(Debug)]
pub struct FocusCache {
    inner: RwLock<FocusCacheInner>,
}

impl FocusCache {
    /// Create a new empty focus cache with the default capacity (16).
    pub fn new() -> Self {
        Self {
            inner: RwLock::new(FocusCacheInner {
                cache: lru::LruCache::new(
                    // SAFETY: DEFAULT_CAPACITY is a compile-time constant > 0
                    NonZeroUsize::new(DEFAULT_CAPACITY).expect("capacity must be > 0"),
                ),
                hit_count: 0,
                miss_count: 0,
            }),
        }
    }

    /// Get cached context if present, otherwise compute and cache.
    ///
    /// Uses a double-checked locking pattern to avoid holding the lock
    /// during potentially slow `compute_fn` execution:
    ///
    /// 1. Acquire write lock, check cache -- if hit, return immediately.
    /// 2. If miss, record the miss, release the lock.
    /// 3. Compute the value outside the lock (other readers are not blocked).
    /// 4. Re-acquire the write lock and check again (another thread may have
    ///    inserted the same key while we were computing).
    /// 5. Insert only if still absent; otherwise return the winner's value.
    pub fn get_or_compute<F>(&self, focus_key: &str, compute_fn: F) -> String
    where
        F: FnOnce() -> String,
    {
        // Phase 1: Quick check under write lock (LRU::get is &mut self — updates MRU).
        {
            let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());
            if let Some(cached) = inner.cache.get(focus_key) {
                let value = cached.clone();
                inner.hit_count += 1;
                return value;
            }
            // Record miss before releasing the lock.
            inner.miss_count += 1;
        }
        // Lock released here — other threads can read/write the cache.

        // Phase 2: Compute outside the lock (potentially slow).
        let context = compute_fn();

        // Phase 3: Re-acquire write lock, double-check before inserting.
        {
            let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());
            if let Some(existing) = inner.cache.get(focus_key) {
                // Another thread computed and inserted while we were working.
                // Return their value (it is at least as fresh as ours).
                return existing.clone();
            }
            inner.cache.put(focus_key.to_string(), context.clone());
        }

        context
    }

    /// Get hit rate as a ratio (0.0 to 1.0). Returns 0.0 if no queries yet.
    pub fn hit_rate(&self) -> f64 {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        let total = inner.hit_count + inner.miss_count;
        if total == 0 {
            0.0
        } else {
            inner.hit_count as f64 / total as f64
        }
    }

    /// Get the total number of cache hits.
    pub fn hit_count(&self) -> u64 {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        inner.hit_count
    }

    /// Get the total number of cache misses.
    pub fn miss_count(&self) -> u64 {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        inner.miss_count
    }

    /// Clear all cached entries (invalidation needed).
    /// Preserves hit/miss stats across clears.
    pub fn clear(&self) {
        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());
        inner.cache.clear();
        // Preserve hit/miss stats across clears
    }

    /// Return true if the cache currently holds at least one value.
    pub fn is_cached(&self) -> bool {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        !inner.cache.is_empty()
    }

    /// Return the maximum capacity of the LRU cache.
    pub fn capacity(&self) -> usize {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        inner.cache.cap().get()
    }

    /// Return the current number of cached entries.
    pub fn len(&self) -> usize {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        inner.cache.len()
    }

    /// Pre-populate a cache entry without a compute function.
    ///
    /// Used by predictive prefetch to warm the cache before actual reads.
    /// Only inserts if the key is not already cached (does not replace fresher data).
    /// Does not affect hit/miss counters — this is proactive warming.
    pub fn prefetch(&self, key: &str, context: String) {
        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());
        // Only insert if not already cached (don't replace fresher data)
        if inner.cache.peek(key).is_none() {
            inner.cache.put(key.to_string(), context);
            // Don't count as miss — this is proactive
        }
    }

    /// Return true if no entries are cached.
    pub fn is_empty(&self) -> bool {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        inner.cache.is_empty()
    }

    /// Return the keys currently in the cache (most-recent first), for debugging.
    pub fn keys(&self) -> Vec<String> {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        inner.cache.iter().map(|(k, _)| k.clone()).collect()
    }
}

impl Default for FocusCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::sync::atomic::{AtomicUsize, Ordering};

    #[test]
    fn test_miss_calls_compute_fn() {
        let cache = FocusCache::new();
        let call_count = Arc::new(AtomicUsize::new(0));
        let cc = call_count.clone();

        let result = cache.get_or_compute("file_a.rs", move || {
            cc.fetch_add(1, Ordering::SeqCst);
            "context_for_a".to_string()
        });

        assert_eq!(result, "context_for_a");
        assert_eq!(call_count.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn test_hit_skips_compute_fn() {
        let cache = FocusCache::new();
        let call_count = Arc::new(AtomicUsize::new(0));

        // First call — miss
        let cc = call_count.clone();
        cache.get_or_compute("file_a.rs", move || {
            cc.fetch_add(1, Ordering::SeqCst);
            "context_a".to_string()
        });

        // Second call with same key — hit
        let cc = call_count.clone();
        let result = cache.get_or_compute("file_a.rs", move || {
            cc.fetch_add(1, Ordering::SeqCst);
            "should_not_be_called".to_string()
        });

        assert_eq!(
            call_count.load(Ordering::SeqCst),
            1,
            "compute_fn called on cache hit"
        );
        assert_eq!(result, "context_a");
    }

    #[test]
    fn test_different_key_recomputes() {
        let cache = FocusCache::new();

        let r1 = cache.get_or_compute("file_a.rs", || "ctx_a".to_string());
        let r2 = cache.get_or_compute("file_b.rs", || "ctx_b".to_string());

        assert_eq!(r1, "ctx_a");
        assert_eq!(r2, "ctx_b");
        assert_eq!(cache.miss_count(), 2);
    }

    #[test]
    fn test_hit_rate_calculation() {
        let cache = FocusCache::new();
        assert_eq!(cache.hit_rate(), 0.0); // no queries yet

        cache.get_or_compute("a", || "x".to_string()); // miss
        cache.get_or_compute("a", || "x".to_string()); // hit
        cache.get_or_compute("a", || "x".to_string()); // hit
        cache.get_or_compute("a", || "x".to_string()); // hit

        // 3 hits, 1 miss = 0.75
        assert!((cache.hit_rate() - 0.75).abs() < 1e-9);
        assert_eq!(cache.hit_count(), 3);
        assert_eq!(cache.miss_count(), 1);
    }

    #[test]
    fn test_clear_invalidates_cache() {
        let cache = FocusCache::new();
        cache.get_or_compute("file.rs", || "ctx".to_string());
        assert!(cache.is_cached());

        cache.clear();
        assert!(!cache.is_cached());

        // After clear, same key should be a miss
        let call_count = Arc::new(AtomicUsize::new(0));
        let cc = call_count.clone();
        cache.get_or_compute("file.rs", move || {
            cc.fetch_add(1, Ordering::SeqCst);
            "new_ctx".to_string()
        });
        assert_eq!(
            call_count.load(Ordering::SeqCst),
            1,
            "should recompute after clear"
        );
    }

    #[test]
    fn test_clear_preserves_stats() {
        let cache = FocusCache::new();
        cache.get_or_compute("a", || "x".to_string()); // miss
        cache.get_or_compute("a", || "x".to_string()); // hit
        cache.clear();
        // Stats should persist across clear
        assert_eq!(cache.hit_count(), 1);
        assert_eq!(cache.miss_count(), 1);
    }

    #[test]
    fn test_empty_key_is_always_miss() {
        let cache = FocusCache::new();
        // Empty cache means no cached value
        assert!(!cache.is_cached());
        let r = cache.get_or_compute("any_file.rs", || "result".to_string());
        assert_eq!(r, "result");
    }

    #[test]
    fn test_single_entry_eviction() {
        let cache = FocusCache::new();
        cache.get_or_compute("file_1.rs", || "ctx_1".to_string());
        cache.get_or_compute("file_2.rs", || "ctx_2".to_string());

        // With LRU-16, going back to file_1 should be a HIT (not a miss like single-entry)
        let call_count = Arc::new(AtomicUsize::new(0));
        let cc = call_count.clone();
        let r = cache.get_or_compute("file_1.rs", move || {
            cc.fetch_add(1, Ordering::SeqCst);
            "ctx_1_recomputed".to_string()
        });
        assert_eq!(
            call_count.load(Ordering::SeqCst),
            0,
            "LRU-16 should cache both files"
        );
        assert_eq!(r, "ctx_1");
    }

    #[test]
    fn test_default_trait() {
        let cache = FocusCache::default();
        assert!(!cache.is_cached());
        assert_eq!(cache.hit_rate(), 0.0);
    }

    // ── New LRU-specific tests ──────────────────────────────────────────

    #[test]
    fn test_lru_multi_file_alternation() {
        let cache = FocusCache::new();
        let call_count = Arc::new(AtomicUsize::new(0));

        // Insert 3 files (3 misses)
        for key in &["alpha.rs", "beta.rs", "gamma.rs"] {
            let cc = call_count.clone();
            cache.get_or_compute(key, move || {
                cc.fetch_add(1, Ordering::SeqCst);
                format!("ctx_{key}")
            });
        }
        assert_eq!(call_count.load(Ordering::SeqCst), 3);

        // Alternate between the 3 files — all should be hits
        for key in &["alpha.rs", "gamma.rs", "beta.rs", "alpha.rs", "gamma.rs"] {
            let cc = call_count.clone();
            let r = cache.get_or_compute(key, move || {
                cc.fetch_add(1, Ordering::SeqCst);
                "should_not_compute".to_string()
            });
            assert_eq!(r, format!("ctx_{key}"));
        }
        // No additional computes — all were hits
        assert_eq!(call_count.load(Ordering::SeqCst), 3);
        assert_eq!(cache.hit_count(), 5);
        assert_eq!(cache.miss_count(), 3);
    }

    #[test]
    fn test_lru_eviction_at_capacity() {
        let cache = FocusCache::new();
        let cap = cache.capacity(); // 16

        // Fill cache to capacity (16 misses)
        for i in 0..cap {
            cache.get_or_compute(&format!("file_{i}.rs"), || format!("ctx_{i}"));
        }
        assert_eq!(cache.len(), cap);
        assert_eq!(cache.miss_count(), cap as u64);

        // Insert one more — should evict the oldest (file_0.rs)
        cache.get_or_compute("file_overflow.rs", || "ctx_overflow".to_string());
        assert_eq!(cache.len(), cap); // still at capacity

        // file_0.rs should be evicted — accessing it is a miss
        let call_count = Arc::new(AtomicUsize::new(0));
        let cc = call_count.clone();
        cache.get_or_compute("file_0.rs", move || {
            cc.fetch_add(1, Ordering::SeqCst);
            "ctx_0_recomputed".to_string()
        });
        assert_eq!(
            call_count.load(Ordering::SeqCst),
            1,
            "file_0 should have been evicted"
        );

        // file_15.rs (most recent before overflow) should still be cached.
        // Note: file_1.rs was evicted when we re-inserted file_0.rs above.
        let call_count2 = Arc::new(AtomicUsize::new(0));
        let cc2 = call_count2.clone();
        let r = cache.get_or_compute("file_15.rs", move || {
            cc2.fetch_add(1, Ordering::SeqCst);
            "should_not_compute".to_string()
        });
        assert_eq!(
            call_count2.load(Ordering::SeqCst),
            0,
            "file_15 should still be cached"
        );
        assert_eq!(r, "ctx_15");
    }

    #[test]
    fn test_lru_capacity_and_len() {
        let cache = FocusCache::new();
        assert_eq!(cache.capacity(), DEFAULT_CAPACITY);
        assert_eq!(cache.len(), 0);

        cache.get_or_compute("a.rs", || "a".to_string());
        assert_eq!(cache.len(), 1);

        cache.get_or_compute("b.rs", || "b".to_string());
        assert_eq!(cache.len(), 2);

        // Hitting an existing key does not change len
        cache.get_or_compute("a.rs", || "unreachable".to_string());
        assert_eq!(cache.len(), 2);

        cache.clear();
        assert_eq!(cache.len(), 0);
        assert_eq!(cache.capacity(), DEFAULT_CAPACITY); // capacity unchanged
    }

    #[test]
    fn test_lru_keys_returns_cached_keys() {
        let cache = FocusCache::new();

        cache.get_or_compute("one.rs", || "1".to_string());
        cache.get_or_compute("two.rs", || "2".to_string());
        cache.get_or_compute("three.rs", || "3".to_string());

        let keys = cache.keys();
        assert_eq!(keys.len(), 3);
        // keys() returns most-recent first (LRU iteration order)
        assert!(keys.contains(&"one.rs".to_string()));
        assert!(keys.contains(&"two.rs".to_string()));
        assert!(keys.contains(&"three.rs".to_string()));

        // After accessing "one.rs", it becomes most-recent
        cache.get_or_compute("one.rs", || "unreachable".to_string());
        let keys_after = cache.keys();
        assert_eq!(keys_after.len(), 3);
        // "one.rs" should be first (most-recent) in the LRU iteration
        assert_eq!(keys_after[0], "one.rs");
    }

    // ── Prefetch tests ──────────────────────────────────────────────────

    #[test]
    fn test_prefetch_populates_cache() {
        let cache = FocusCache::new();

        // Prefetch a file
        cache.prefetch("predicted.rs", "prefetched context".to_string());

        // get_or_compute should find it without calling compute_fn
        let call_count = Arc::new(AtomicUsize::new(0));
        let cc = call_count.clone();
        let result = cache.get_or_compute("predicted.rs", move || {
            cc.fetch_add(1, Ordering::SeqCst);
            "should_not_be_called".to_string()
        });

        assert_eq!(
            call_count.load(Ordering::SeqCst),
            0,
            "compute_fn should not be called after prefetch"
        );
        assert_eq!(result, "prefetched context");
        assert_eq!(cache.hit_count(), 1);
    }

    #[test]
    fn test_prefetch_no_overwrite() {
        let cache = FocusCache::new();

        // Insert via normal path
        cache.get_or_compute("file.rs", || "fresh context".to_string());

        // Prefetch with stale data — should NOT overwrite
        cache.prefetch("file.rs", "stale prefetched data".to_string());

        // Verify the original (fresher) data is still there
        let call_count = Arc::new(AtomicUsize::new(0));
        let cc = call_count.clone();
        let result = cache.get_or_compute("file.rs", move || {
            cc.fetch_add(1, Ordering::SeqCst);
            "should_not_compute".to_string()
        });

        assert_eq!(
            result, "fresh context",
            "prefetch should not overwrite existing entry"
        );
        assert_eq!(call_count.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn test_prefetch_does_not_affect_counters() {
        let cache = FocusCache::new();

        cache.prefetch("a.rs", "ctx_a".to_string());
        cache.prefetch("b.rs", "ctx_b".to_string());

        // Prefetch should not increment hit or miss counters
        assert_eq!(cache.hit_count(), 0);
        assert_eq!(cache.miss_count(), 0);

        // But the entries ARE in the cache
        assert_eq!(cache.len(), 2);
    }

    #[test]
    fn test_prefetch_respects_lru_capacity() {
        let cache = FocusCache::new();
        let cap = cache.capacity(); // 16

        // Fill via prefetch
        for i in 0..cap {
            cache.prefetch(&format!("file_{i}.rs"), format!("ctx_{i}"));
        }
        assert_eq!(cache.len(), cap);

        // One more prefetch should evict the oldest
        cache.prefetch("overflow.rs", "ctx_overflow".to_string());
        assert_eq!(cache.len(), cap); // still at capacity

        // file_0 should have been evicted
        let call_count = Arc::new(AtomicUsize::new(0));
        let cc = call_count.clone();
        cache.get_or_compute("file_0.rs", move || {
            cc.fetch_add(1, Ordering::SeqCst);
            "recomputed".to_string()
        });
        assert_eq!(
            call_count.load(Ordering::SeqCst),
            1,
            "file_0 should have been evicted by prefetch overflow"
        );
    }
}
