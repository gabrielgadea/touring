//! File parser cache using moka::sync::Cache + IncrementalPipeline.
//!
//! Replaces DashMap with a TTL-bounded moka Cache for automatic eviction.
//! Entries idle for >300 s are evicted, preventing unbounded memory growth
//! in long-running daemon sessions.
//!
//! Provides 10× speedup for repeated parses while bounding memory usage.

use moka::sync::Cache;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;
use touring_code::ast::incremental_pipeline::SharedPipeline;

/// Maximum number of file pipelines to keep in the cache.
const MAX_CAPACITY: u64 = 1_000;
/// Evict entries that haven't been accessed for this duration.
const TIME_TO_IDLE: Duration = Duration::from_secs(300);

/// Cache of parsed file pipelines, backed by moka for TTL-based eviction.
///
/// Stores `Arc<SharedPipeline>` for cheap cloning.
/// Thread-safe: moka::sync::Cache uses internal sharding.
pub struct FileParserCache {
    cache: Cache<PathBuf, Arc<SharedPipeline>>,
}

impl FileParserCache {
    /// Create new cache with default capacity and TTL settings.
    pub fn new() -> Self {
        Self {
            cache: Cache::builder()
                .max_capacity(MAX_CAPACITY)
                .time_to_idle(TIME_TO_IDLE)
                .build(),
        }
    }

    /// Get pipeline for path, creating if needed.
    ///
    /// Uses moka's atomic `get_with` to avoid races on concurrent init.
    pub fn get_or_create(&self, path: &Path) -> Arc<SharedPipeline> {
        self.cache
            .get_with(path.to_path_buf(), || Arc::new(SharedPipeline::new()))
    }

    /// Check if path is cached.
    pub fn contains(&self, path: &PathBuf) -> bool {
        self.cache.get(path).is_some()
    }

    /// Invalidate cached pipeline for path.
    ///
    /// EC57: Called from post_write after a file is written, to ensure the next
    /// get_or_create produces a fresh pipeline instead of serving a stale entry.
    /// Short-circuits if the path is not cached (avoids a no-op moka call).
    pub fn invalidate(&self, path: &PathBuf) {
        if !self.contains(path) {
            return;
        }
        self.cache.invalidate(path);
    }

    /// Clear all cached pipelines. Called at session boundaries via `pre_edit::flush_cache`.
    pub fn clear(&self) {
        self.cache.invalidate_all();
        self.run_pending_tasks();
    }

    /// Flush pending moka bookkeeping tasks.
    ///
    /// moka uses asynchronous bookkeeping for `entry_count()` consistency.
    /// Call this before asserting on `len()` or after `clear()`.
    pub fn run_pending_tasks(&self) {
        self.cache.run_pending_tasks();
    }

    /// Current approximate cache size.
    ///
    /// Note: `entry_count()` is eventually consistent in moka.
    /// Call `run_pending_tasks()` before asserting exact counts in tests.
    pub fn len(&self) -> usize {
        self.cache.entry_count() as usize
    }

    /// Returns `true` if the cache contains no entries.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for FileParserCache {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parser_cache_basic() {
        let cache = FileParserCache::new();
        let path = PathBuf::from("test.rs");
        // Initial state: not cached.
        assert!(!cache.contains(&path));
        let _pipeline = cache.get_or_create(&path);
        // moka entry_count is eventually consistent — flush before asserting.
        cache.run_pending_tasks();
        // After get_or_create + flush: entry should be present.
        assert!(cache.contains(&path));
        assert_eq!(cache.len(), 1);
    }

    #[test]
    fn parser_cache_invalidate() {
        let cache = FileParserCache::new();
        let path = PathBuf::from("test.rs");
        cache.get_or_create(&path);
        cache.invalidate(&path);
        // moka eviction is async — run_pending_tasks to flush.
        cache.cache.run_pending_tasks();
        assert!(!cache.contains(&path));
    }

    #[test]
    fn parser_cache_clear() {
        let cache = FileParserCache::new();
        cache.get_or_create(&PathBuf::from("a.rs"));
        cache.get_or_create(&PathBuf::from("b.rs"));
        cache.clear();
        assert_eq!(cache.len(), 0);
    }

    #[test]
    fn parser_cache_get_or_create_idempotent() {
        let cache = FileParserCache::new();
        let path = PathBuf::from("test.rs");
        let p1 = cache.get_or_create(&path);
        let p2 = cache.get_or_create(&path);
        // Same Arc pointer — no double init.
        assert!(Arc::ptr_eq(&p1, &p2));
    }
}
