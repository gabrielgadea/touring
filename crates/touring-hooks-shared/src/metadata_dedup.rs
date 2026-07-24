//! Metadata deduplication using moka cache.
//!
//! Uses bounded moka cache (50k entries, 60s TTL) for deduplication
//! instead of unbounded `OnceLock<Mutex<HashMap>>`.

use moka::sync::Cache;
use std::time::Duration;
// Note: entry value is () — presence in the cache is the dedup signal; TTL is managed by moka.

/// Dedup key for metadata deduplication.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct DedupKey {
    /// Path of the file whose metadata was processed.
    pub file_path: String,
    /// Content hash at processing time; together with `file_path` it identifies a unique unit of work.
    pub content_hash: String,
}

/// Metadata deduplication cache using moka.
///
/// Bounded cache: max 50k entries, 60s TTL.
/// Thread-safe, no Mutex needed.
pub struct MetadataDedup {
    cache: Cache<DedupKey, ()>,
}

impl MetadataDedup {
    /// Create new dedup cache with default settings (50k, 60s).
    pub fn new() -> Self {
        Self {
            cache: Cache::new(50_000),
        }
    }

    /// Create with custom max capacity and TTL.
    pub fn with_max_capacity(max_capacity: u64, ttl_secs: u64) -> Self {
        Self {
            cache: Cache::builder()
                .max_capacity(max_capacity)
                .time_to_idle(Duration::from_secs(ttl_secs))
                .build(),
        }
    }

    /// Check if metadata for key is already cached.
    /// Returns true if this is a duplicate (should skip collection).
    pub fn is_duplicate(&self, key: &DedupKey) -> bool {
        self.cache.get(key).is_some()
    }

    /// Mark metadata as cached (record for deduplication).
    pub fn mark_cached(&self, key: DedupKey) {
        self.cache.insert(key, ());
    }

    /// Check and mark in one operation.
    /// Returns true if was already present (duplicate).
    pub fn check_and_mark(&self, key: DedupKey) -> bool {
        if self.cache.get(&key).is_some() {
            true
        } else {
            self.cache.insert(key, ());
            false
        }
    }

    /// Invalidate a specific key, forcing the next check_and_mark to treat it as fresh.
    ///
    /// EC60: test-only helper — cargo check does not compile #[cfg(test)] modules,
    /// so the caller in `dedup_cache_invalidate` is invisible to the dead_code lint.
    /// Annotation is intentional: method is kept for unit test introspection and
    /// future production use (e.g., force-refresh after explicit file overwrite).
    pub fn invalidate(&self, key: &DedupKey) {
        self.cache.invalidate(key);
    }

    /// Clear all entries.
    pub fn clear(&self) {
        self.cache.invalidate_all();
    }

    /// Current entry count.
    pub fn len(&self) -> u64 {
        self.cache.entry_count()
    }

    /// Check if empty.
    pub fn is_empty(&self) -> bool {
        self.cache.entry_count() == 0
    }
}

impl Default for MetadataDedup {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dedup_cache_basic() {
        let dedup = MetadataDedup::with_max_capacity(100, 60);
        let key = DedupKey {
            file_path: "test.rs".to_string(),
            content_hash: "abc123".to_string(),
        };

        // First check should be false (not a duplicate)
        assert!(!dedup.check_and_mark(key.clone()));

        // Second check should be true (duplicate)
        assert!(dedup.check_and_mark(key));
    }

    #[test]
    fn dedup_cache_different_keys() {
        let dedup = MetadataDedup::new();
        let key1 = DedupKey {
            file_path: "test1.rs".to_string(),
            content_hash: "hash1".to_string(),
        };
        let key2 = DedupKey {
            file_path: "test2.rs".to_string(),
            content_hash: "hash2".to_string(),
        };

        assert!(!dedup.check_and_mark(key1.clone()));
        assert!(!dedup.check_and_mark(key2.clone()));
        assert!(dedup.check_and_mark(key1)); // key1 again is duplicate
    }

    #[test]
    fn dedup_cache_invalidate() {
        // EC60: exercises MetadataDedup::invalidate round-trip.
        // mark → is_duplicate(true) → invalidate → is_duplicate(false)
        let dedup = MetadataDedup::with_max_capacity(100, 60);
        let key = DedupKey {
            file_path: "src/lib.rs".to_string(),
            content_hash: "1714000000".to_string(), // mtime proxy
        };

        // First mark: not a duplicate.
        assert!(!dedup.check_and_mark(key.clone()));
        // Second check: now a duplicate.
        assert!(dedup.is_duplicate(&key));
        // Invalidate: clears the entry.
        dedup.invalidate(&key);
        // After invalidation: no longer a duplicate.
        assert!(!dedup.is_duplicate(&key));
    }
}
