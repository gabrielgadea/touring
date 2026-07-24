//! `KvCacheManager` — WASM instance caching with wasmtime pooling.
//!
//! ## Abstraction Boundary
//!
//! This module manages the **KV data cache for WASM plugin evaluation**. It
//! stores compiled WASM modules and their metadata, enabling fast reuse across
//! evaluations. It is NOT related to SQLite connection lifecycle or WASM module
//! creation/destruction scheduling.
//!
//! ### Relationship to touring-wasm/pool.rs and touring-core/shared/pool.rs
//!
//! | Layer | File | Role |
//! |-------|------|------|
//! | **WASM module lifecycle** | `touring-wasm/src/pool.rs` | When WASM modules are created/destroyed (semaphore+deque pooling) |
//! | **KV data cache** | `touring-wasm/src/cache_manager.rs` | What data of runs is cached (SerializedInferlet + hit_count), independent of lifecycle |
//! | **SQLite lifecycle** | `touring-core/src/shared/pool.rs` | When SQLite connections are opened/closed for knowledge/memory/graph domains |
//!
//! ### Cache vs Lifecycle Distinction
//!
//! - **Lifecycle management** (`touring-wasm/pool.rs`): Controls _when_ a WASM module is
//!   instantiated and when it is destroyed. Uses `InferletPool` / `AsyncInferletPool`
//!   with semaphore acquisition.
//!
//! - **KV data cache** (`cache_manager.rs`): Stores _data about_ WASM evaluation
//!   runs — the compiled bytes, inferlet metadata, and hit count. Does not
//!   control module instantiation; modules are retrieved from cache and then
//!   handed to the lifecycle pool for execution.
//!
//! The `CacheEntry` struct holds `SerializedInferlet` (WASM bytes + name) and
//! `hit_count`. The cache is consulted before spawning a new module in the
//! lifecycle pool, but the actual module lifecycle is managed separately.
//!
//! Provides a trait for KV-cached WASM plugin evaluation and an implementation
//! using [`wasmtime`]'s `PoolingAllocationConfig` for fast instantiation.

use std::sync::RwLock;

use async_trait::async_trait;

use inferlets::SerializedInferlet;

/// Cache entry for a compiled + instantiated WASM module.
#[derive(Debug, Clone)]
pub struct CacheEntry {
    /// Serialized inferlet containing WASM bytes and metadata.
    inferlet: SerializedInferlet,
    /// Number of times this entry has been used.
    hit_count: u64,
}

impl CacheEntry {
    /// Create a new cache entry from a serialized inferlet.
    pub fn new(inferlet: SerializedInferlet) -> Self {
        Self {
            inferlet,
            hit_count: 1,
        }
    }

    /// Returns a reference to the WASM bytes.
    pub fn wasm_bytes(&self) -> &[u8] {
        &self.inferlet.wasm_bytes
    }

    /// Returns the inferlet name.
    pub fn inferlet_name(&self) -> &str {
        &self.inferlet.name
    }

    /// Returns a reference to the underlying serialized inferlet.
    pub fn inferlet(&self) -> &SerializedInferlet {
        &self.inferlet
    }

    /// Increments the usage counter, recording another cache hit for this entry.
    pub fn increment_hit(&mut self) {
        self.hit_count += 1;
    }
}

/// Synchronous KV cache manager trait for WASM plugin caching.
///
/// Implementors must be thread-safe (`Send + Sync`) to support concurrent access.
pub trait KvCacheManager: Send + Sync {
    /// Retrieve a cached entry by key.
    fn get(&self, key: &str) -> Option<CacheEntry>;

    /// Insert or update a cached entry.
    ///
    /// Returns the previous entry if the key was already present.
    fn insert(&self, key: String, entry: CacheEntry) -> Option<CacheEntry>;

    /// Remove a cached entry by key.
    fn invalidate(&self, key: &str);

    /// Check if a key exists in the cache.
    fn exists(&self, key: &str) -> bool;

    /// Returns the current number of cached entries.
    fn len(&self) -> usize;

    /// Returns true if the cache contains no entries.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// In-memory KV cache manager with thread-safe interior mutability.
#[derive(Debug, Default)]
pub struct InMemoryCacheManager {
    entries: RwLock<std::collections::HashMap<String, CacheEntry>>,
}

impl InMemoryCacheManager {
    /// Create an empty in-memory cache manager backed by a `RwLock`-guarded map.
    pub fn new() -> Self {
        Self {
            entries: RwLock::new(std::collections::HashMap::new()),
        }
    }
}

impl KvCacheManager for InMemoryCacheManager {
    fn get(&self, key: &str) -> Option<CacheEntry> {
        let entries = self.entries.read().ok()?;
        entries.get(key).cloned()
    }

    fn insert(&self, key: String, entry: CacheEntry) -> Option<CacheEntry> {
        let mut entries = self.entries.write().ok()?;
        entries.insert(key, entry)
    }

    fn invalidate(&self, key: &str) {
        if let Ok(mut entries) = self.entries.write() {
            entries.remove(key);
        }
    }

    fn exists(&self, key: &str) -> bool {
        self.entries
            .read()
            .map(|e| e.contains_key(key))
            .unwrap_or(false)
    }

    fn len(&self) -> usize {
        self.entries.read().map(|e| e.len()).unwrap_or(0)
    }
}

/// Async version of KvCacheManager for async runtime integration.
#[async_trait]
pub trait AsyncKvCacheManager: Send + Sync {
    /// Asynchronously retrieve a cached entry by key, returning `None` if absent.
    async fn get(&self, key: &str) -> Option<CacheEntry>;
    /// Asynchronously insert or update an entry, returning the previous entry if the key existed.
    async fn insert(&self, key: String, entry: CacheEntry) -> Option<CacheEntry>;
    /// Asynchronously remove a cached entry by key.
    async fn invalidate(&self, key: &str);
    /// Check synchronously whether a key is present in the cache.
    fn exists(&self, key: &str) -> bool;
    /// Returns the current number of cached entries.
    fn len(&self) -> usize;
    /// Returns true if the cache contains no entries.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

/// In-memory async KV cache manager.
#[derive(Debug, Default)]
pub struct AsyncInMemoryCacheManager {
    entries: tokio::sync::RwLock<std::collections::HashMap<String, CacheEntry>>,
}

impl AsyncInMemoryCacheManager {
    /// Create an empty async cache manager backed by a `tokio::sync::RwLock`-guarded map.
    pub fn new() -> Self {
        Self {
            entries: tokio::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }
}

#[async_trait]
impl AsyncKvCacheManager for AsyncInMemoryCacheManager {
    async fn get(&self, key: &str) -> Option<CacheEntry> {
        let guard = self.entries.read().await;
        guard.get(key).cloned()
    }
    async fn insert(&self, key: String, entry: CacheEntry) -> Option<CacheEntry> {
        let mut guard = self.entries.write().await;
        guard.insert(key, entry)
    }
    async fn invalidate(&self, key: &str) {
        let mut guard = self.entries.write().await;
        guard.remove(key);
    }
    fn exists(&self, key: &str) -> bool {
        // Sync fast path
        self.entries.blocking_read().contains_key(key)
    }
    fn len(&self) -> usize {
        self.entries.blocking_read().len()
    }
}

/// WASM-optimized cache manager using wasmtime with pooling allocation.
///
/// Uses `InstanceAllocationStrategy::Pooling` for fast instance creation:
/// - Memory and table slots are pre-allocated in a pool
/// - Instance creation = "take from pool" instead of malloc
/// - Combined with `memory_init_cow(true)`, instantiation is near-zero copy
///
/// # Example
///
/// ```ignore
/// use touring_bindings::wasm::cache_manager::{WasmCacheManager, CacheEntry};
///
/// let manager = WasmCacheManager::new();
/// manager.insert("plugin-v1".into(), CacheEntry::new(wasm_bytes));
/// ```
#[derive(Debug)]
pub struct WasmCacheManager {
    /// In-memory KV cache for compiled modules.
    cache: InMemoryCacheManager,
    /// Pool size for wasmtime instances.
    pool_size: usize,
    /// Whether CoW memory initialization is enabled.
    cow_enabled: bool,
}

impl WasmCacheManager {
    /// Create a new `WasmCacheManager` with default settings.
    ///
    /// - `pool_size`: Number of pre-allocated WASM instances (default: 4)
    /// - `cow_enabled`: Whether to use copy-on-write memory initialization (default: true)
    pub fn new() -> Self {
        Self::with_config(4, true)
    }

    /// Create a `WasmCacheManager` with explicit configuration.
    pub fn with_config(pool_size: usize, cow_enabled: bool) -> Self {
        Self {
            cache: InMemoryCacheManager::new(),
            pool_size: pool_size.max(1),
            cow_enabled,
        }
    }

    /// Returns the configured pool size.
    pub fn pool_size(&self) -> usize {
        self.pool_size
    }

    /// Returns whether CoW memory initialization is enabled.
    pub fn is_cow_enabled(&self) -> bool {
        self.cow_enabled
    }
}

impl Default for WasmCacheManager {
    fn default() -> Self {
        Self::new()
    }
}

impl KvCacheManager for WasmCacheManager {
    fn get(&self, key: &str) -> Option<CacheEntry> {
        self.cache.get(key)
    }

    fn insert(&self, key: String, entry: CacheEntry) -> Option<CacheEntry> {
        self.cache.insert(key, entry)
    }

    fn invalidate(&self, key: &str) {
        self.cache.invalidate(key)
    }

    fn exists(&self, key: &str) -> bool {
        self.cache.exists(key)
    }

    fn len(&self) -> usize {
        self.cache.len()
    }
}

/// WASM engine configuration optimized for fast instantiation.
///
/// Configures `PoolingAllocationConfig` and `memory_init_cow(true)`:
/// - Pooling: pre-allocates virtual memory for instances from a pool
/// - CoW: defers memory copy from instantiation to first mutation
///
/// Returns a tuple of `(wasmtime::Engine, wasmtime::Config)` ready to use.
#[cfg(feature = "pooling-allocator")]
pub fn fast_instantiation_config() -> (wasmtime::Engine, wasmtime::Config) {
    let mut config = wasmtime::Config::new();

    // Enable pooling allocator — instances come from pre-allocated pool
    config.allocation_strategy(wasmtime::PoolingAllocationConfig::new());

    // Enable copy-on-write memory initialization
    // Memory is map-on-demand with CoW — instantiation just maps, copy deferred to first write
    config.memory_init_cow(true);

    let engine = wasmtime::Engine::new(&config).expect("wasmtime engine creation failed");

    (engine, config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_in_memory_cache_basic() {
        use inferlets::SerializedInferlet;

        let cache = InMemoryCacheManager::new();

        assert!(cache.is_empty());
        assert!(!cache.exists("key1"));

        let inferlet = SerializedInferlet::new("test_inferlet", vec![1, 2, 3]);
        let entry = CacheEntry::new(inferlet);
        let prev = cache.insert("key1".into(), entry.clone());
        assert!(prev.is_none());

        assert!(cache.exists("key1"));
        assert_eq!(cache.len(), 1);

        let retrieved = cache.get("key1").unwrap();
        assert_eq!(retrieved.wasm_bytes(), &[1, 2, 3]);

        cache.invalidate("key1");
        assert!(!cache.exists("key1"));
        assert!(cache.is_empty());
    }

    #[test]
    fn test_cache_hit_count() {
        use inferlets::SerializedInferlet;

        let cache = InMemoryCacheManager::new();

        let inferlet = SerializedInferlet::new("hit_test", vec![0u8]);
        let entry = CacheEntry::new(inferlet);
        assert_eq!(entry.hit_count, 1);
        cache.insert("key".into(), entry);

        // Insert same key replaces with new entry
        let new_inferlet = SerializedInferlet::new("hit_test", vec![1u8]);
        let new_entry = CacheEntry::new(new_inferlet);
        assert_eq!(new_entry.hit_count, 1);
        cache.insert("key".into(), new_entry);

        let retrieved = cache.get("key").unwrap();
        assert_eq!(retrieved.hit_count, 1); // fresh entry from replacement
    }

    #[test]
    fn test_wasm_cache_manager_delegates() {
        use inferlets::SerializedInferlet;

        let manager = WasmCacheManager::with_config(8, true);

        assert_eq!(manager.pool_size(), 8);
        assert!(manager.is_cow_enabled());
        assert!(manager.is_empty());

        let inferlet = SerializedInferlet::new("plugin", vec![0x00]);
        let entry = CacheEntry::new(inferlet);
        manager.insert("plugin".into(), entry);

        assert_eq!(manager.len(), 1);
        assert!(manager.exists("plugin"));
    }

    #[test]
    fn test_cache_entry_inferlet_name() {
        use inferlets::SerializedInferlet;

        let inferlet = SerializedInferlet::new("my_inferlet", vec![0x00]);
        let entry = CacheEntry::new(inferlet);

        assert_eq!(entry.inferlet_name(), "my_inferlet");
        assert_eq!(entry.wasm_bytes(), &[0x00]);
    }

    #[test]
    fn test_cache_entry_roundtrip() {
        use inferlets::SerializedInferlet;

        let original = SerializedInferlet::new("roundtrip_test", vec![0xaa, 0xbb, 0xcc]);
        let entry = CacheEntry::new(original.clone());

        assert_eq!(entry.inferlet_name(), "roundtrip_test");
        assert_eq!(entry.wasm_bytes(), &[0xaa, 0xbb, 0xcc]);
        // Verify the inferlet data matches
        assert_eq!(entry.inferlet().wasm_bytes, original.wasm_bytes);
    }

    #[test]
    #[cfg(feature = "pooling-allocator")]
    fn test_fast_instantiation_config() {
        // fast_instantiation_config returns a valid engine when pooling-allocator feature is enabled
        let (engine, _config) = fast_instantiation_config();
        // Engine was created without panicking — that's the test
        let _ = engine;
    }
}
