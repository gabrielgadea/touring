//! Shared global FileParserCache singleton.
//!
//! Eliminates the three-way cache fragmentation caused by separate
//! `OnceLock<FileParserCache>` instances in pre_read, pre_edit, and post_write.
//! When pre_read warms cache for file.rs, pre_edit now hits the same cache
//! instead of creating a separate instance (hit_rate=0.0 root cause).
//!
//! EC57 pattern: post_write invalidates before warming, so cache entries
//! stay fresh across hook phases.

use std::path::PathBuf;
use std::sync::OnceLock;

use crate::parser_cache::FileParserCache;

static GLOBAL_PARSER_CACHE: OnceLock<FileParserCache> = OnceLock::new();

/// Get the global shared FileParserCache instance.
///
/// First call lazily initializes. All subsequent calls return the same
/// instance — pre_read, pre_edit, and post_write all share the same cache.
pub fn global_cache() -> &'static FileParserCache {
    GLOBAL_PARSER_CACHE.get_or_init(FileParserCache::new)
}

/// Invalidate a path in the global cache.
///
/// Call this from post_write after file modification to ensure the next
/// hook phase gets a fresh pipeline (EC57 pattern).
pub fn global_invalidate(path: &PathBuf) {
    global_cache().invalidate(path);
}
