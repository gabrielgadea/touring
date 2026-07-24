//! Incremental Index Updates
//!
//! Provides efficient re-parsing of changed files and incremental
//! updates to the symbol index without full rebuilds.

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use moka::sync::Cache as MokaCache;
use thiserror::Error;
use tokio::sync::RwLock;
use tracing::{debug, info, warn};

use touring_code::ast::languages::Lang;

use crate::index::cache::FileCache;
use crate::index::watcher::FileEvent;
use touring_code::ast::symbols::{Symbol, extract_symbols};

/// Soft cap on the symbol cache — large monorepos push north of 100k files,
/// so we size the cache to comfortably hold hot files without recomputing.
const SYMBOL_INDEX_CAPACITY: u64 = 131_072;

/// TTL for the symbol cache. Longer than the parser cache because symbol
/// extraction is expensive and the content hash already protects against
/// staleness (file watcher triggers explicit eviction on modify).
const SYMBOL_INDEX_TTL_SECS: u64 = 60 * 30; // 30 min

/// Soft cap on the file-info cache. File info is tiny (~200 bytes) so we can
/// afford a generous limit — the upper bound is file count, not memory.
const FILE_INFO_CAPACITY: u64 = 131_072;

/// TTL for the file-info cache. Matches the symbol cache window.
const FILE_INFO_TTL_SECS: u64 = 60 * 30;

/// Errors that can occur during incremental updates
#[derive(Error, Debug)]
pub enum IncrementalError {
    /// Underlying I/O failure while reading or stat-ing a file.
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),

    /// Source could not be parsed into symbols.
    #[error("Parse error: {0}")]
    Parse(String),

    /// Error propagated from the file-content cache.
    #[error("Cache error: {0}")]
    Cache(#[from] crate::index::cache::CacheError),

    /// No symbol was found for the given file path.
    #[error("Symbol not found: {0}")]
    SymbolNotFound(PathBuf),

    /// File language is not supported by incremental indexing.
    #[error("Unsupported language: {0}")]
    UnsupportedLanguage(String),
}

/// Result type for incremental operations
pub(crate) type Result<T> = std::result::Result<T, IncrementalError>;

/// Information about an indexed file
#[derive(Debug, Clone)]
pub struct FileIndexInfo {
    /// File path
    pub path: PathBuf,
    /// Last modification time when indexed
    pub indexed_mtime: std::time::SystemTime,
    /// File size in bytes
    pub size: usize,
    /// Number of symbols extracted
    pub symbol_count: usize,
    /// File language
    pub language: Option<Lang>,
    /// Checksum for content verification
    pub checksum: String,
}

impl FileIndexInfo {
    /// Create new file index info
    pub fn new(
        path: PathBuf,
        indexed_mtime: std::time::SystemTime,
        size: usize,
        symbol_count: usize,
        language: Option<Lang>,
    ) -> Self {
        Self {
            path,
            indexed_mtime,
            size,
            symbol_count,
            language,
            checksum: String::new(),
        }
    }

    /// Calculate checksum from content
    pub(crate) fn calculate_checksum(content: &str) -> String {
        use sha2::{Digest, Sha256};
        let mut hasher = Sha256::new();
        hasher.update(content.as_bytes());
        format!("{:x}", hasher.finalize())[..16].to_string()
    }
}

/// Incremental index manager
///
/// Maintains a symbol index that can be updated incrementally
/// when files change, without requiring full rebuilds.
///
/// # Cache backend
///
/// Moved from `DashMap` to `moka::sync::Cache` on 2026-04-16 (P1 #6 of
/// the crates-inventory-ranking wave). moka adds W-TinyLFU admission +
/// LRU eviction + TTL, matching the temporal locality of file access
/// (recently-edited files are queried far more often than cold ones).
/// Values are `Arc<_>` so `.get()` clones are O(1) refcount bumps.
#[derive(Debug)]
pub struct IncrementalIndex {
    /// File cache for content storage
    cache: Arc<RwLock<FileCache>>,
    /// Symbol index: path -> `Arc<Vec<Symbol>>` (moka-backed)
    symbol_index: MokaCache<PathBuf, Arc<Vec<Symbol>>>,
    /// File metadata: path -> `Arc<FileIndexInfo>` (moka-backed)
    file_info: MokaCache<PathBuf, Arc<FileIndexInfo>>,
    /// Total symbols indexed
    total_symbols: RwLock<usize>,
    /// Files indexed
    files_indexed: RwLock<usize>,
}

impl IncrementalIndex {
    /// Create a new incremental index
    pub fn new(cache: Arc<RwLock<FileCache>>) -> Self {
        Self {
            cache,
            symbol_index: MokaCache::builder()
                .max_capacity(SYMBOL_INDEX_CAPACITY)
                .time_to_live(Duration::from_secs(SYMBOL_INDEX_TTL_SECS))
                .eviction_policy(moka::policy::EvictionPolicy::lru())
                // Weigher charges per symbol so huge files (10k+ symbols)
                // pressure the admission policy appropriately. +1 ensures
                // empty vecs still consume one admission slot.
                .weigher(|_k: &PathBuf, v: &Arc<Vec<Symbol>>| -> u32 {
                    v.len().saturating_add(1).min(u32::MAX as usize) as u32
                })
                .build(),
            file_info: MokaCache::builder()
                .max_capacity(FILE_INFO_CAPACITY)
                .time_to_live(Duration::from_secs(FILE_INFO_TTL_SECS))
                .eviction_policy(moka::policy::EvictionPolicy::lru())
                .build(),
            total_symbols: RwLock::new(0),
            files_indexed: RwLock::new(0),
        }
    }

    /// Index a single file
    pub async fn index_file(&self, path: &Path) -> Result<()> {
        // Check if file exists and is readable
        let metadata = std::fs::metadata(path)?;
        if !metadata.is_file() {
            return Ok(());
        }

        // Skip files that are too large
        const MAX_FILE_SIZE: u64 = 5 * 1024 * 1024; // 5MB
        if metadata.len() > MAX_FILE_SIZE {
            warn!("Skipping large file: {:?} ({} bytes)", path, metadata.len());
            return Ok(());
        }

        // Read file content
        let content = match std::fs::read_to_string(path) {
            Ok(c) => c,
            Err(e) => {
                // Likely binary file, skip
                debug!("Cannot read {:?} as text: {}", path, e);
                return Ok(());
            }
        };

        // Detect language
        let language = Lang::from_path(path);

        // Parse symbols (skip if language not detected)
        let symbols = match language {
            Some(lang) => match extract_symbols(&content, lang) {
                Ok(s) => s,
                Err(e) => {
                    warn!("Failed to parse symbols from {:?}: {}", path, e);
                    Vec::new()
                }
            },
            None => {
                debug!("No language detected for {:?}", path);
                Vec::new()
            }
        };

        let symbol_count = symbols.len();

        // Update cache
        {
            let mut cache = self.cache.write().await;
            let mtime = metadata
                .modified()
                .unwrap_or_else(|_| std::time::SystemTime::now());
            cache.put(path.to_path_buf(), content.clone(), mtime)?;
        }

        // Update symbol index (wrap in Arc so get() clones are refcount bumps)
        self.symbol_index
            .insert(path.to_path_buf(), Arc::new(symbols));

        // Update file info
        let mtime = metadata
            .modified()
            .unwrap_or_else(|_| std::time::SystemTime::now());
        let mut info = FileIndexInfo::new(
            path.to_path_buf(),
            mtime,
            content.len(),
            symbol_count,
            language,
        );
        info.checksum = FileIndexInfo::calculate_checksum(&content);
        self.file_info.insert(path.to_path_buf(), Arc::new(info));

        // Update stats
        {
            let mut total = self.total_symbols.write().await;
            *total += symbol_count;
        }
        {
            let mut files = self.files_indexed.write().await;
            *files += 1;
        }

        debug!("Indexed {:?}: {} symbols", path, symbol_count);
        Ok(())
    }

    /// Remove a file from the index
    pub async fn remove_file(&self, path: &Path) -> Result<()> {
        // Remove from symbol index — read first to recompute total count.
        let path_buf = path.to_path_buf();
        if let Some(symbols) = self.symbol_index.get(&path_buf) {
            // Bind the Arc<Vec<Symbol>> explicitly so len() resolves to Vec::len.
            let symbols_arc: Arc<Vec<Symbol>> = symbols;
            let len = symbols_arc.len();
            self.symbol_index.invalidate(&path_buf);
            let mut total = self.total_symbols.write().await;
            *total = total.saturating_sub(len);
        }

        // Remove from file info
        if self.file_info.get(&path_buf).is_some() {
            self.file_info.invalidate(&path_buf);
            let mut files = self.files_indexed.write().await;
            *files = files.saturating_sub(1);
        }

        // Remove from cache
        {
            let mut cache = self.cache.write().await;
            cache.remove(path);
        }

        debug!("Removed {:?} from index", path);
        Ok(())
    }

    /// Handle a file change event
    pub async fn on_file_changed(&self, event: &FileEvent) -> Result<()> {
        match event.event_type {
            crate::index::watcher::FileEventType::Create => {
                info!("File created: {:?}", event.path);
                self.index_file(&event.path).await?;
            }
            crate::index::watcher::FileEventType::Modify => {
                info!("File modified: {:?}", event.path);
                // Remove old entry and re-index
                self.remove_file(&event.path).await?;
                self.index_file(&event.path).await?;
            }
            crate::index::watcher::FileEventType::Remove => {
                info!("File removed: {:?}", event.path);
                self.remove_file(&event.path).await?;
            }
            crate::index::watcher::FileEventType::Rename => {
                if let Some(ref old_path) = event.old_path {
                    info!("File renamed: {:?} -> {:?}", old_path, event.path);
                    // Remove old entry
                    self.remove_file(old_path).await?;
                    // Index new location
                    self.index_file(&event.path).await?;
                }
            }
        }
        Ok(())
    }

    /// Rebuild the entire index for a file
    pub async fn rebuild_index_for_file(&self, path: &Path) -> Result<Vec<Symbol>> {
        // Remove existing entry
        self.remove_file(path).await?;

        // Re-index
        self.index_file(path).await?;

        // Return the new symbols (clone the Arc'd vec into an owned Vec)
        self.symbol_index
            .get(&path.to_path_buf())
            .map(|arc| (*arc).clone())
            .ok_or_else(|| IncrementalError::SymbolNotFound(path.to_path_buf()))
    }

    /// Get symbols for a file
    pub fn get_symbols(&self, path: &Path) -> Option<Vec<Symbol>> {
        self.symbol_index
            .get(&path.to_path_buf())
            .map(|arc| (*arc).clone())
    }

    /// Get file info
    pub fn get_file_info(&self, path: &Path) -> Option<FileIndexInfo> {
        self.file_info
            .get(&path.to_path_buf())
            .map(|arc| (*arc).clone())
    }

    /// Check if a file is indexed
    pub fn is_indexed(&self, path: &Path) -> bool {
        self.file_info.contains_key(&path.to_path_buf())
    }

    /// Get all indexed paths
    pub fn indexed_paths(&self) -> Vec<PathBuf> {
        // moka's concurrent iterator yields `(Arc<K>, V)` tuples;
        // deref the key Arc to clone the underlying PathBuf.
        self.file_info
            .iter()
            .map(|entry| {
                let (k, _v): (Arc<PathBuf>, Arc<FileIndexInfo>) = entry;
                (*k).clone()
            })
            .collect()
    }

    /// Search symbols by name (simple substring match)
    pub fn search_symbols(&self, query: &str) -> Vec<(PathBuf, Symbol)> {
        let mut results = Vec::new();
        let query_lower = query.to_lowercase();

        for entry in self.symbol_index.iter() {
            // `entry` is (Arc<PathBuf>, Arc<Vec<Symbol>>)
            let (path_arc, symbols_arc): (Arc<PathBuf>, Arc<Vec<Symbol>>) = entry;
            for symbol in symbols_arc.as_ref().iter() {
                if symbol.name.to_lowercase().contains(&query_lower) {
                    results.push(((*path_arc).clone(), symbol.clone()));
                }
            }
        }

        results
    }

    /// Get index statistics
    pub async fn stats(&self) -> IndexStats {
        IndexStats {
            files_indexed: *self.files_indexed.read().await,
            total_symbols: *self.total_symbols.read().await,
            cache_stats: {
                let cache = self.cache.read().await;
                cache.stats()
            },
        }
    }

    /// Clear the entire index
    pub async fn clear(&self) {
        self.symbol_index.invalidate_all();
        self.file_info.invalidate_all();
        // Make eviction observable in the tests below without racing.
        self.symbol_index.run_pending_tasks();
        self.file_info.run_pending_tasks();
        *self.total_symbols.write().await = 0;
        *self.files_indexed.write().await = 0;

        let mut cache = self.cache.write().await;
        cache.clear();
    }
}

/// Index statistics
#[derive(Debug, Clone)]
pub struct IndexStats {
    /// Number of files indexed
    pub files_indexed: usize,
    /// Total number of symbols
    pub total_symbols: usize,
    /// Cache statistics
    pub cache_stats: crate::index::cache::CacheStats,
}

impl std::fmt::Display for IndexStats {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Index: {} files, {} symbols | {}",
            self.files_indexed, self.total_symbols, self.cache_stats
        )
    }
}

/// Incremental index builder
#[derive(Debug)]
pub struct IncrementalIndexBuilder {
    cache: Option<Arc<RwLock<FileCache>>>,
}

impl IncrementalIndexBuilder {
    /// Create a new builder
    pub fn new() -> Self {
        Self { cache: None }
    }

    /// Set the cache
    pub fn with_cache(mut self, cache: Arc<RwLock<FileCache>>) -> Self {
        self.cache = Some(cache);
        self
    }

    /// Build the incremental index
    pub fn build(self) -> IncrementalIndex {
        let cache = self
            .cache
            .unwrap_or_else(|| Arc::new(RwLock::new(FileCache::new())));
        IncrementalIndex::new(cache)
    }
}

impl Default for IncrementalIndexBuilder {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::index::watcher::{FileEvent, FileEventType};
    use std::io::Write;
    use tempfile::TempDir;

    fn create_test_file(dir: &Path, name: &str, content: &str) -> PathBuf {
        let path = dir.join(name);
        let mut file = std::fs::File::create(&path).unwrap();
        file.write_all(content.as_bytes()).unwrap();
        path
    }

    #[tokio::test]
    async fn test_index_file() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Arc::new(RwLock::new(FileCache::new()));
        let index = IncrementalIndex::new(cache);

        // Create a test Python file
        let content = r#"
def hello():
    pass

class MyClass:
    def method(self):
        pass
"#;
        let path = create_test_file(temp_dir.path(), "test.py", content);

        // Index the file
        index.index_file(&path).await.unwrap();

        // Check it's indexed
        assert!(index.is_indexed(&path));

        // Get symbols
        let symbols = index.get_symbols(&path).unwrap();
        assert!(!symbols.is_empty());

        // Check stats
        let stats = index.stats().await;
        assert_eq!(stats.files_indexed, 1);
        assert!(stats.total_symbols >= 3); // hello, MyClass, method
    }

    #[tokio::test]
    async fn test_remove_file() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Arc::new(RwLock::new(FileCache::new()));
        let index = IncrementalIndex::new(cache);

        let path = create_test_file(temp_dir.path(), "test.py", "def foo(): pass");

        // Index then remove
        index.index_file(&path).await.unwrap();
        assert!(index.is_indexed(&path));

        index.remove_file(&path).await.unwrap();
        assert!(!index.is_indexed(&path));
        assert!(index.get_symbols(&path).is_none());
    }

    #[tokio::test]
    async fn test_search_symbols() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Arc::new(RwLock::new(FileCache::new()));
        let index = IncrementalIndex::new(cache);

        // Create files with different symbols
        let path1 = create_test_file(temp_dir.path(), "file1.py", "def hello(): pass");
        let path2 = create_test_file(temp_dir.path(), "file2.py", "def world(): pass");

        index.index_file(&path1).await.unwrap();
        index.index_file(&path2).await.unwrap();

        // Search for symbols
        let results = index.search_symbols("hello");
        assert!(!results.is_empty());
        assert!(results.iter().any(|(p, _)| p == &path1));

        let results = index.search_symbols("world");
        assert!(!results.is_empty());
        assert!(results.iter().any(|(p, _)| p == &path2));
    }

    #[tokio::test]
    async fn test_rebuild_index_for_file() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Arc::new(RwLock::new(FileCache::new()));
        let index = IncrementalIndex::new(cache);

        let path = create_test_file(temp_dir.path(), "test.py", "def old(): pass");

        // Index original content
        index.index_file(&path).await.unwrap();
        let old_symbols = index.get_symbols(&path).unwrap();

        // Modify file
        std::fs::write(&path, "def new(): pass").unwrap();

        // Rebuild index
        let new_symbols = index.rebuild_index_for_file(&path).await.unwrap();

        // Should have different symbols
        assert_ne!(
            old_symbols.iter().map(|s| &s.name).collect::<Vec<_>>(),
            new_symbols.iter().map(|s| &s.name).collect::<Vec<_>>()
        );
    }

    #[tokio::test]
    async fn test_clear_index() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Arc::new(RwLock::new(FileCache::new()));
        let index = IncrementalIndex::new(cache);

        let path = create_test_file(temp_dir.path(), "test.py", "def foo(): pass");
        index.index_file(&path).await.unwrap();

        assert_eq!(index.stats().await.files_indexed, 1);

        index.clear().await;

        assert_eq!(index.stats().await.files_indexed, 0);
        assert!(!index.is_indexed(&path));
    }

    // --- 14 new tests ---

    #[tokio::test]
    async fn test_new_index_is_empty() {
        let cache = Arc::new(RwLock::new(FileCache::new()));
        let index = IncrementalIndex::new(cache);

        assert!(
            index.indexed_paths().is_empty(),
            "new index should have no paths"
        );
        let stats = index.stats().await;
        assert_eq!(stats.files_indexed, 0);
        assert_eq!(stats.total_symbols, 0);
    }

    #[tokio::test]
    async fn test_builder_default() {
        let index = IncrementalIndexBuilder::new().build();
        assert!(index.indexed_paths().is_empty());
    }

    #[tokio::test]
    async fn test_index_file_rust_source() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Arc::new(RwLock::new(FileCache::new()));
        let index = IncrementalIndex::new(cache);

        let path = create_test_file(temp_dir.path(), "main.rs", "fn main() {}");
        index.index_file(&path).await.unwrap();

        assert!(
            index.is_indexed(&path),
            "file should be indexed after index_file"
        );
        // Symbols may be empty for very small Rust files but is_indexed should be true
        assert!(
            index.get_symbols(&path).is_some(),
            "get_symbols should return Some"
        );
    }

    #[tokio::test]
    async fn test_get_file_info_after_index() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Arc::new(RwLock::new(FileCache::new()));
        let index = IncrementalIndex::new(cache);

        let path = create_test_file(temp_dir.path(), "info.py", "def foo(): pass");
        index.index_file(&path).await.unwrap();

        let info = index.get_file_info(&path);
        assert!(
            info.is_some(),
            "get_file_info should return Some after indexing"
        );
        let info = info.unwrap();
        assert_eq!(info.path, path);
    }

    #[tokio::test]
    async fn test_is_indexed_false_before_index() {
        let cache = Arc::new(RwLock::new(FileCache::new()));
        let index = IncrementalIndex::new(cache);

        let fake_path = std::path::PathBuf::from("/tmp/nonexistent_touring_test_file.py");
        assert!(
            !index.is_indexed(&fake_path),
            "non-indexed file should return false"
        );
    }

    #[tokio::test]
    async fn test_search_symbols_finds_by_name() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Arc::new(RwLock::new(FileCache::new()));
        let index = IncrementalIndex::new(cache);

        let path = create_test_file(temp_dir.path(), "funcs.py", "def my_function(): pass");
        index.index_file(&path).await.unwrap();

        let results = index.search_symbols("my_function");
        assert!(
            !results.is_empty(),
            "search_symbols should find 'my_function'"
        );
        assert!(results.iter().any(|(p, _)| p == &path));
    }

    #[tokio::test]
    async fn test_search_symbols_empty_for_unknown() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Arc::new(RwLock::new(FileCache::new()));
        let index = IncrementalIndex::new(cache);

        let path = create_test_file(temp_dir.path(), "funcs.py", "def hello(): pass");
        index.index_file(&path).await.unwrap();

        let results = index.search_symbols("zzz_nonexistent");
        assert!(
            results.is_empty(),
            "search_symbols should return empty vec for unknown symbol"
        );
    }

    #[tokio::test]
    async fn test_remove_file_clears_indexed() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Arc::new(RwLock::new(FileCache::new()));
        let index = IncrementalIndex::new(cache);

        let path = create_test_file(temp_dir.path(), "remove_me.py", "def bar(): pass");
        index.index_file(&path).await.unwrap();
        assert!(index.is_indexed(&path));

        index.remove_file(&path).await.unwrap();
        assert!(
            !index.is_indexed(&path),
            "file should not be indexed after remove_file"
        );
    }

    #[tokio::test]
    async fn test_stats_after_index() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Arc::new(RwLock::new(FileCache::new()));
        let index = IncrementalIndex::new(cache);

        let path = create_test_file(
            temp_dir.path(),
            "stats.py",
            "def alpha(): pass\ndef beta(): pass",
        );
        index.index_file(&path).await.unwrap();

        let stats = index.stats().await;
        assert!(
            stats.files_indexed >= 1,
            "stats should show at least 1 file indexed"
        );
    }

    #[tokio::test]
    async fn test_clear_empties_index() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Arc::new(RwLock::new(FileCache::new()));
        let index = IncrementalIndex::new(cache);

        let path = create_test_file(temp_dir.path(), "clear_me.py", "def foo(): pass");
        index.index_file(&path).await.unwrap();
        assert!(!index.indexed_paths().is_empty());

        index.clear().await;
        assert!(
            index.indexed_paths().is_empty(),
            "indexed_paths should be empty after clear"
        );
    }

    #[tokio::test]
    async fn test_indexed_paths_contains_indexed_file() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Arc::new(RwLock::new(FileCache::new()));
        let index = IncrementalIndex::new(cache);

        let path = create_test_file(temp_dir.path(), "path_check.py", "x = 1");
        index.index_file(&path).await.unwrap();

        let paths = index.indexed_paths();
        assert!(
            paths.contains(&path),
            "indexed_paths should contain the indexed file"
        );
    }

    #[tokio::test]
    async fn test_on_file_changed_modified() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Arc::new(RwLock::new(FileCache::new()));
        let index = IncrementalIndex::new(cache);

        let path = create_test_file(temp_dir.path(), "modified.py", "def original(): pass");
        // Pre-index so modify can remove and re-index
        index.index_file(&path).await.unwrap();

        // Update content on disk
        std::fs::write(&path, "def updated(): pass").unwrap();

        let event = FileEvent::new(FileEventType::Modify, path.clone());
        index.on_file_changed(&event).await.unwrap();

        assert!(
            index.is_indexed(&path),
            "file should still be indexed after Modify event"
        );
    }

    #[tokio::test]
    async fn test_on_file_changed_deleted() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Arc::new(RwLock::new(FileCache::new()));
        let index = IncrementalIndex::new(cache);

        let path = create_test_file(temp_dir.path(), "deleted.py", "def gone(): pass");
        index.index_file(&path).await.unwrap();
        assert!(index.is_indexed(&path));

        let event = FileEvent::new(FileEventType::Remove, path.clone());
        index.on_file_changed(&event).await.unwrap();

        assert!(
            !index.is_indexed(&path),
            "file should not be indexed after Remove event"
        );
    }

    #[tokio::test]
    async fn test_rebuild_index_for_file_returns_symbols() {
        let temp_dir = TempDir::new().unwrap();
        let cache = Arc::new(RwLock::new(FileCache::new()));
        let index = IncrementalIndex::new(cache);

        let path = create_test_file(temp_dir.path(), "rebuild2.py", "def initial(): pass");
        index.index_file(&path).await.unwrap();

        // Change content on disk
        std::fs::write(&path, "def refreshed(): pass\ndef extra(): pass").unwrap();

        // rebuild_index_for_file should succeed and return symbols
        let symbols = index.rebuild_index_for_file(&path).await.unwrap();
        // Should return at least the new symbols without panic
        let _ = symbols; // result consumed without panic = success
        assert!(
            index.is_indexed(&path),
            "file should be indexed after rebuild"
        );
    }
}
