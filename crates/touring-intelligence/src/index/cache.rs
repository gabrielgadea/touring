//! LRU File Content Cache
//!
//! Provides an in-memory cache for file contents with:
//! - Configurable max size (default: 10MB)
//! - LRU eviction policy
//! - Metadata tracking (mtime, size)

use std::path::{Path, PathBuf};
use std::time::SystemTime;

use lru::LruCache;
use thiserror::Error;

/// Errors that can occur in the file cache
#[derive(Error, Debug)]
pub enum CacheError {
    /// Underlying I/O failure while reading a file into the cache.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// File exceeds the per-entry size limit and cannot be cached.
    #[error("File too large: {size} bytes (max: {max} bytes)")]
    FileTooLarge {
        /// Actual file size in bytes.
        size: usize,
        /// Configured maximum per-entry size in bytes.
        max: usize,
    },

    /// Cache has no room for the entry even after LRU eviction.
    #[error("Cache full: cannot fit entry of {size} bytes")]
    CacheFull {
        /// Size in bytes of the entry that could not fit.
        size: usize,
    },
}

/// Result type for cache operations
pub(crate) type Result<T> = std::result::Result<T, CacheError>;

/// Metadata for a cached file entry
#[derive(Debug, Clone)]
pub struct CacheEntry {
    /// File content
    pub content: String,
    /// Last modification time
    pub mtime: SystemTime,
    /// Content size in bytes
    pub size: usize,
}

impl CacheEntry {
    /// Create a new cache entry
    pub fn new(content: String, mtime: SystemTime) -> Self {
        let size = content.len();
        Self {
            content,
            mtime,
            size,
        }
    }

    /// Check if the entry is stale compared to filesystem
    pub fn is_stale(&self, current_mtime: SystemTime) -> bool {
        self.mtime < current_mtime
    }
}

/// LRU File Content Cache
///
/// Caches file contents with a configurable maximum size.
/// Uses LRU eviction policy when size limit is exceeded.
#[derive(Debug)]
pub struct FileCache {
    /// LRU cache storing entries by path
    cache: LruCache<PathBuf, CacheEntry>,
    /// Maximum total size in bytes
    max_size: usize,
    /// Current total size in bytes
    current_size: usize,
    /// Maximum size for a single file
    max_file_size: usize,
    /// Access count statistics
    hits: u64,
    misses: u64,
}

impl FileCache {
    /// Default maximum cache size (10MB)
    pub(crate) const DEFAULT_MAX_SIZE: usize = 10 * 1024 * 1024;
    /// Default maximum file size (1MB)
    pub(crate) const DEFAULT_MAX_FILE_SIZE: usize = 1024 * 1024;

    /// Create a new file cache with default size
    pub fn new() -> Self {
        Self::with_capacity(Self::DEFAULT_MAX_SIZE)
    }

    /// Create a new file cache with specified max size
    pub(crate) fn with_capacity(max_size: usize) -> Self {
        Self {
            cache: LruCache::unbounded(),
            max_size,
            current_size: 0,
            max_file_size: Self::DEFAULT_MAX_FILE_SIZE,
            hits: 0,
            misses: 0,
        }
    }

    /// Create a new file cache with custom limits
    pub fn with_limits(max_size: usize, max_file_size: usize) -> Self {
        Self {
            cache: LruCache::unbounded(),
            max_size,
            current_size: 0,
            max_file_size,
            hits: 0,
            misses: 0,
        }
    }

    /// Get a cached entry if present and not stale
    pub fn get(&mut self, path: &Path) -> Option<&CacheEntry> {
        // Check if entry exists in cache first
        let in_cache = self.cache.contains(path);

        if !in_cache {
            // Entry not in cache - count as miss and return early
            self.misses += 1;
            return None;
        }

        // Check if stale by comparing mtime
        let is_stale = if let Ok(metadata) = std::fs::metadata(path) {
            if let Ok(current_mtime) = metadata.modified() {
                self.cache
                    .get(path)
                    .map(|e| e.is_stale(current_mtime))
                    .unwrap_or(false)
            } else {
                false
            }
        } else {
            false
        };

        if is_stale {
            // Entry is stale, remove it
            self.remove(path);
            self.misses += 1;
            return None;
        }

        // Entry exists and is not stale - count as hit
        let entry = self.cache.get(path)?;
        self.hits += 1;
        Some(entry)
    }

    /// Get content directly (convenience method)
    pub fn get_content(&mut self, path: &Path) -> Option<&str> {
        self.get(path).map(|e| e.content.as_str())
    }

    /// Insert or update a cache entry
    pub(crate) fn put(&mut self, path: PathBuf, content: String, mtime: SystemTime) -> Result<()> {
        let size = content.len();

        // Check file size limit
        if size > self.max_file_size {
            return Err(CacheError::FileTooLarge {
                size,
                max: self.max_file_size,
            });
        }

        // Check if entry already exists
        if let Some(old_entry) = self.cache.get(&path) {
            self.current_size -= old_entry.size;
        }

        // Evict entries if necessary
        self.evict_if_needed(size)?;

        // Create and insert new entry
        let entry = CacheEntry::new(content, mtime);
        self.current_size += size;
        self.cache.put(path, entry);

        Ok(())
    }

    /// Insert a file by reading it from disk
    pub fn put_file(&mut self, path: &Path) -> Result<()> {
        let content = std::fs::read_to_string(path)?;
        let metadata = std::fs::metadata(path)?;
        let mtime = metadata.modified().unwrap_or_else(|_| SystemTime::now());

        self.put(path.to_path_buf(), content, mtime)
    }

    /// Remove an entry from the cache
    pub(crate) fn remove(&mut self, path: &Path) -> Option<CacheEntry> {
        let entry = self.cache.pop(path)?;
        self.current_size -= entry.size;
        Some(entry)
    }

    /// Evict entries until there's room for the requested size
    fn evict_if_needed(&mut self, required_size: usize) -> Result<()> {
        // Check if we can ever fit this
        if required_size > self.max_size {
            return Err(CacheError::CacheFull {
                size: required_size,
            });
        }

        // Evict LRU entries until we have room
        while self.current_size + required_size > self.max_size {
            if let Some((_, entry)) = self.cache.pop_lru() {
                self.current_size -= entry.size;
            } else {
                // Cache is empty but we still can't fit
                return Err(CacheError::CacheFull {
                    size: required_size,
                });
            }
        }

        Ok(())
    }

    /// Clear all entries from the cache
    pub(crate) fn clear(&mut self) {
        self.cache.clear();
        self.current_size = 0;
    }

    /// Get cache statistics
    pub(crate) fn stats(&self) -> CacheStats {
        let total_requests = self.hits + self.misses;
        let hit_rate = if total_requests > 0 {
            self.hits as f64 / total_requests as f64
        } else {
            0.0
        };

        CacheStats {
            entries: self.cache.len(),
            current_size: self.current_size,
            max_size: self.max_size,
            hits: self.hits,
            misses: self.misses,
            hit_rate,
        }
    }

    /// Check if path is in cache
    pub fn contains(&self, path: &Path) -> bool {
        self.cache.contains(path)
    }

    /// Get current size in bytes
    pub fn current_size(&self) -> usize {
        self.current_size
    }

    /// Get maximum size in bytes
    pub fn max_size(&self) -> usize {
        self.max_size
    }
}

impl Default for FileCache {
    fn default() -> Self {
        Self::new()
    }
}

/// Cache statistics
#[derive(Debug, Clone, Copy)]
pub struct CacheStats {
    /// Number of entries in cache
    pub entries: usize,
    /// Current total size in bytes
    pub current_size: usize,
    /// Maximum size in bytes
    pub max_size: usize,
    /// Cache hits
    pub hits: u64,
    /// Cache misses
    pub misses: u64,
    /// Hit rate (0.0 - 1.0)
    pub hit_rate: f64,
}

impl CacheStats {
    /// Format size in human-readable format
    pub(crate) fn format_size(&self, bytes: usize) -> String {
        const UNITS: &[&str] = &["B", "KB", "MB", "GB"];
        let mut size = bytes as f64;
        let mut unit_idx = 0;

        while size >= 1024.0 && unit_idx < UNITS.len() - 1 {
            size /= 1024.0;
            unit_idx += 1;
        }

        // SAFETY: unit_idx starts at 0 and increments only while unit_idx < UNITS.len() - 1,
        // so unit_idx is always a valid index into UNITS.
        #[allow(clippy::indexing_slicing)]
        let unit = UNITS[unit_idx];
        format!("{:.2} {}", size, unit)
    }

    /// Get current size as formatted string
    pub(crate) fn current_size_formatted(&self) -> String {
        self.format_size(self.current_size)
    }

    /// Get max size as formatted string
    pub(crate) fn max_size_formatted(&self) -> String {
        self.format_size(self.max_size)
    }
}

impl std::fmt::Display for CacheStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Cache: {} entries, {}/{}, hits: {}, misses: {}, hit_rate: {:.1}%",
            self.entries,
            self.current_size_formatted(),
            self.max_size_formatted(),
            self.hits,
            self.misses,
            self.hit_rate * 100.0
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_cache_basic_operations() {
        let mut cache = FileCache::with_capacity(1024); // 1KB cache

        // Create a temp file
        let mut temp_file = NamedTempFile::new().unwrap();
        write!(temp_file, "Hello, World!").unwrap();
        let path = temp_file.path().to_path_buf();

        // Initially not in cache
        assert!(!cache.contains(&path));
        assert!(cache.get(&path).is_none());

        // Insert into cache
        cache.put_file(&path).unwrap();
        assert!(cache.contains(&path));

        // Retrieve from cache
        let entry = cache.get(&path).unwrap();
        assert_eq!(entry.content, "Hello, World!");

        // Check stats
        let stats = cache.stats();
        assert_eq!(stats.entries, 1);
        assert!(stats.hits >= 1);
    }

    #[test]
    fn test_cache_eviction() {
        let mut cache = FileCache::with_capacity(100); // Very small cache

        // Insert first entry
        let path1 = PathBuf::from("/tmp/test1.txt");
        cache
            .put(path1.clone(), "a".repeat(50), SystemTime::now())
            .unwrap();
        assert!(cache.contains(&path1));

        // Insert second entry (should fit)
        let path2 = PathBuf::from("/tmp/test2.txt");
        cache
            .put(path2.clone(), "b".repeat(40), SystemTime::now())
            .unwrap();
        assert!(cache.contains(&path1));
        assert!(cache.contains(&path2));

        // Insert third entry (should evict first)
        let path3 = PathBuf::from("/tmp/test3.txt");
        cache
            .put(path3.clone(), "c".repeat(30), SystemTime::now())
            .unwrap();

        // First entry should be evicted (LRU)
        assert!(!cache.contains(&path1));
        assert!(cache.contains(&path2));
        assert!(cache.contains(&path3));
    }

    #[test]
    fn test_cache_file_too_large() {
        let mut cache = FileCache::with_limits(1000, 50); // 50 byte max file

        let path = PathBuf::from("/tmp/large.txt");
        let result = cache.put(path.clone(), "x".repeat(100), SystemTime::now());

        assert!(matches!(
            result,
            Err(CacheError::FileTooLarge { size: 100, max: 50 })
        ));
    }

    #[test]
    fn test_cache_stats() {
        let mut cache = FileCache::with_capacity(1024);

        let path = PathBuf::from("/tmp/stats_test.txt");
        cache
            .put(path.clone(), "test content".to_string(), SystemTime::now())
            .unwrap();

        // Access multiple times
        let _ = cache.get(&path);
        let _ = cache.get(&path);
        let _ = cache.get(&PathBuf::from("/tmp/missing.txt"));

        let stats = cache.stats();
        assert_eq!(stats.entries, 1);
        assert_eq!(stats.hits, 2);
        assert_eq!(stats.misses, 1);
        assert!((stats.hit_rate - 0.666).abs() < 0.01);
    }

    #[test]
    fn test_cache_stale_detection() {
        use std::thread;
        use std::time::Duration;

        let mut cache = FileCache::new();

        // Create a temp file
        let mut temp_file = NamedTempFile::new().unwrap();
        write!(temp_file, "original").unwrap();
        let path = temp_file.path().to_path_buf();

        // Insert into cache
        cache.put_file(&path).unwrap();
        assert!(cache.get(&path).is_some());

        // Wait a bit and modify file
        thread::sleep(Duration::from_millis(10));
        write!(temp_file, "modified").unwrap();

        // Should detect stale entry
        assert!(cache.get(&path).is_none());
    }

    // ========================================================================
    // Extended integration tests — require pub(crate) access
    // ========================================================================

    #[test]
    fn test_cache_remove_returns_entry() {
        let mut cache = FileCache::with_capacity(1024);
        let path = PathBuf::from("/tmp/remove_test.txt");
        cache
            .put(path.clone(), "content".into(), SystemTime::now())
            .unwrap();
        assert!(cache.contains(&path));

        let removed = cache.remove(&path);
        assert!(removed.is_some(), "remove should return the old entry");
        assert!(!cache.contains(&path));
        assert_eq!(cache.current_size(), 0);
    }

    #[test]
    fn test_cache_remove_nonexistent() {
        let mut cache = FileCache::new();
        let absent = PathBuf::from("/tmp/does_not_exist_12345.txt");
        let result = cache.remove(&absent);
        assert!(result.is_none());
    }

    #[test]
    fn test_cache_clear_resets() {
        let mut cache = FileCache::with_capacity(1024 * 1024);
        cache
            .put(
                PathBuf::from("/tmp/a.txt"),
                "a".repeat(100),
                SystemTime::now(),
            )
            .unwrap();
        cache
            .put(
                PathBuf::from("/tmp/b.txt"),
                "b".repeat(100),
                SystemTime::now(),
            )
            .unwrap();
        assert!(cache.current_size() > 0);

        cache.clear();
        assert_eq!(cache.current_size(), 0);
        assert!(!cache.contains(&PathBuf::from("/tmp/a.txt")));
        assert!(!cache.contains(&PathBuf::from("/tmp/b.txt")));
    }

    #[test]
    fn test_cache_stats_hit_rate_zero() {
        let mut cache = FileCache::new();
        let _ = cache.get(&PathBuf::from("/tmp/a.txt"));
        let _ = cache.get(&PathBuf::from("/tmp/b.txt"));
        let stats = cache.stats();
        assert_eq!(stats.hits, 0);
        assert_eq!(stats.misses, 2);
        assert_eq!(stats.hit_rate, 0.0);
    }

    #[test]
    fn test_cache_stats_hit_rate_perfect() {
        let mut cache = FileCache::with_capacity(1024);
        let path = PathBuf::from("/tmp/perfect.txt");
        cache
            .put(path.clone(), "data".into(), SystemTime::now())
            .unwrap();
        let _ = cache.get(&path);
        let _ = cache.get(&path);
        let _ = cache.get(&path);
        let stats = cache.stats();
        assert_eq!(stats.hits, 3);
        assert_eq!(stats.misses, 0);
        assert_eq!(stats.hit_rate, 1.0);
    }

    #[test]
    fn test_cache_lru_eviction_order() {
        // Cache of 250 bytes: after A(100)+B(100), adding C(100) must evict something
        let mut cache = FileCache::with_capacity(250);
        let path_a = PathBuf::from("/tmp/lru_a.txt");
        let path_b = PathBuf::from("/tmp/lru_b.txt");
        let path_c = PathBuf::from("/tmp/lru_c.txt");

        cache
            .put(path_a.clone(), "a".repeat(100), SystemTime::now())
            .unwrap();
        cache
            .put(path_b.clone(), "b".repeat(100), SystemTime::now())
            .unwrap();

        // Touch A to make it MRU (A becomes most recently used)
        let _ = cache.get(&path_a);

        // Adding C(100) to 200/250 cache requires evicting B (LRU)
        cache
            .put(path_c.clone(), "c".repeat(100), SystemTime::now())
            .unwrap();

        assert!(
            cache.contains(&path_a),
            "A should still be present (was touched)"
        );
        assert!(!cache.contains(&path_b), "B should be evicted (LRU)");
        assert!(cache.contains(&path_c));
    }

    #[test]
    fn test_cache_put_updates_existing() {
        let mut cache = FileCache::with_capacity(1024);
        let path = PathBuf::from("/tmp/update.txt");
        cache
            .put(path.clone(), "v1".into(), SystemTime::now())
            .unwrap();
        let before = cache.get(&path).unwrap();
        assert_eq!(before.content, "v1");

        cache
            .put(path.clone(), "v2".into(), SystemTime::now())
            .unwrap();
        let after = cache.get(&path).unwrap();
        assert_eq!(after.content, "v2");
    }

    #[test]
    fn test_cache_get_content() {
        let mut cache = FileCache::new();
        let path = PathBuf::from("/tmp/content.txt");
        cache
            .put(path.clone(), "direct".into(), SystemTime::now())
            .unwrap();
        assert_eq!(cache.get_content(&path), Some("direct"));
        assert_eq!(cache.get_content(&PathBuf::from("/tmp/absent.txt")), None);
    }

    #[test]
    fn test_cache_max_size_zero_rejects_all() {
        let mut cache = FileCache::with_capacity(0);
        let result = cache.put(
            PathBuf::from("/tmp/small.txt"),
            "x".into(),
            SystemTime::now(),
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_cache_entry_is_stale_when_current_newer() {
        // is_stale returns true when current_mtime > cached_mtime
        // (meaning: file may have been modified since caching)
        let cached_time = SystemTime::now();
        let entry = CacheEntry::new("content".into(), cached_time);
        let newer = cached_time + std::time::Duration::from_secs(1);
        assert!(
            entry.is_stale(newer),
            "newer current mtime means potentially modified"
        );
    }

    #[test]
    fn test_cache_entry_not_stale_when_current_same() {
        let cached_time = SystemTime::now();
        let entry = CacheEntry::new("content".into(), cached_time);
        assert!(!entry.is_stale(cached_time), "same mtime means not stale");
    }

    #[test]
    fn test_cache_format_size_zero() {
        let stats = CacheStats {
            entries: 0,
            current_size: 0,
            max_size: 0,
            hits: 0,
            misses: 0,
            hit_rate: 0.0,
        };
        let formatted = stats.format_size(0);
        assert!(formatted.contains("0"), "format_size(0) should contain 0");
    }

    #[test]
    fn test_cache_format_size_mb() {
        let stats = CacheStats {
            entries: 1,
            current_size: 1024 * 1024,
            max_size: 2 * 1024 * 1024,
            hits: 10,
            misses: 2,
            hit_rate: 0.833,
        };
        let formatted = stats.format_size(1024 * 1024);
        assert!(formatted.contains("MB") || formatted.contains("1.00"));
    }

    #[test]
    fn test_cache_format_size_gb() {
        let stats = CacheStats {
            entries: 1,
            current_size: 1024 * 1024 * 1024,
            max_size: 2 * 1024 * 1024 * 1024,
            hits: 10,
            misses: 2,
            hit_rate: 0.833,
        };
        let formatted = stats.format_size(1024 * 1024 * 1024);
        assert!(formatted.contains("GB") || formatted.contains("1.00"));
    }

    #[test]
    fn test_cache_default() {
        let cache = FileCache::default();
        assert_eq!(cache.current_size(), 0);
        assert!(cache.max_size() > 0);
    }

    #[test]
    fn test_cache_entry_debug() {
        let entry = CacheEntry::new("test".into(), SystemTime::now());
        let debug = format!("{:?}", entry);
        assert!(debug.contains("CacheEntry") || debug.contains("content"));
    }

    #[test]
    fn test_cache_cache_full_error_display() {
        let err = CacheError::CacheFull { size: 500 };
        let display = format!("{}", err);
        assert!(!display.is_empty());
        assert!(display.contains("500") || display.contains("Cache full"));
    }

    #[test]
    fn test_cache_file_too_large_error_display() {
        let err = CacheError::FileTooLarge { size: 100, max: 50 };
        let display = format!("{}", err);
        assert!(!display.is_empty());
        assert!(display.contains("100") || display.contains("50"));
    }

    #[test]
    fn test_cache_io_error_display() {
        let err = CacheError::Io(std::io::Error::new(
            std::io::ErrorKind::NotFound,
            "file not found",
        ));
        let display = format!("{}", err);
        assert!(!display.is_empty());
        assert!(display.contains("IO error") || display.contains("file not found"));
    }

    #[test]
    fn test_cache_stats_display() {
        let stats = CacheStats {
            entries: 10,
            current_size: 1024,
            max_size: 2048,
            hits: 100,
            misses: 20,
            hit_rate: 0.833,
        };
        let display = format!("{}", stats);
        assert!(!display.is_empty());
        assert!(display.contains("10") || display.contains("entries"));
        assert!(display.contains("83") || display.contains("hit_rate"));
    }

    #[test]
    fn test_cache_max_size_accessor() {
        let cache = FileCache::with_capacity(5000);
        assert_eq!(cache.max_size(), 5000);
    }

    #[test]
    fn test_cache_current_size_accessor() {
        let mut cache = FileCache::with_capacity(1024);
        cache
            .put(
                PathBuf::from("/tmp/size.txt"),
                "12345".into(),
                SystemTime::now(),
            )
            .unwrap();
        assert_eq!(cache.current_size(), 5);
    }
}
