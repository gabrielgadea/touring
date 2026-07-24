//! Touring Index — File watching, LRU caching, and incremental symbol indexing
//!
pub mod cache;
pub mod incremental;
pub mod watcher;

// Re-exports from touring_code::semantics for Definition-backed queries
pub use touring_code::semantics::{
    Definition, DefinitionId, DefinitionKind, FileRange, Usage, UsageKind,
};

/// SIMD-accelerated file similarity using touring-simd cosine similarity.
#[cfg(feature = "simd-similarity")]
pub mod similarity;

/// RL-guided cache priority using touring-learning LinUCB bandit.
#[cfg(feature = "smart-cache")]
pub mod smart_cache;

// Re-exports matching the original touring-server::index public API
/// Cache entry with metadata and LRU eviction support.
pub use cache::{CacheEntry, CacheError, CacheStats, FileCache};
/// Incremental index building and stats.
pub use incremental::{FileIndexInfo, IncrementalIndex, IncrementalIndexBuilder, IndexStats};
/// File watcher with debouncing, gitignore, and event batching.
pub use watcher::{EventBatcher, FileEvent, FileEventType, FileWatcher, FileWatcherBuilder};

/// SIMD-accelerated file similarity index.
#[cfg(feature = "simd-similarity")]
pub use similarity::{FileFeatureVector, FileSimilarityIndex};

/// RL-guided cache eviction priority.
#[cfg(feature = "smart-cache")]
pub use smart_cache::{CachePriorityFeatures, SmartCachePriority};
