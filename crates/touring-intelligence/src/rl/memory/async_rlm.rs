//! AsyncRlmMemory — non-blocking write path for RLM memory.
//!
//! # Design
//!
//! SQLite writes via `RlmMemory` take ~5–15 ms. In the PostToolUse hook path,
//! this latency is unacceptable (hooks must complete in <2s). `AsyncRlmMemory`
//! solves this with a **write-through cache + background persistence** pattern:
//!
//! ```text
//! store(k, v)
//!   ├── in-memory cache (RwLock<HashMap>) ← immediate, <1 μs
//!   └── mpsc channel → background task → SQLite batch (~5-15 ms, off hot path)
//!
//! recall(k)
//!   ├── cache hit  → return immediately (<1 μs)
//!   └── cache miss → SQLite read (sync, ~1-3 ms) + populate cache
//! ```
//!
//! # Batch FSYNC (Phase 1.3)
//!
//! Multiple writes within the batch window are grouped into a single SQLite
//! transaction, reducing fsync overhead from N×15ms to ~15ms total.
//!
//! # Requirements
//!
//! Requires feature `async-memory` and a running `tokio` runtime.
//!
//! ```toml
//! touring-learning = { features = ["async-memory"] }
//! ```
//!
//! # Thread safety
//!
//! `AsyncRlmMemory` is `Send + Sync`. The `Arc<RwLock<_>>` cache allows
//! concurrent reads; `store` acquires a write lock only to insert into the cache.

#[cfg(feature = "async-memory")]
use tokio::sync::{mpsc, oneshot};

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use moka::sync::Cache;

use crate::rl::error::{LearningError, LearningResult};
use crate::rl::memory::rlm::{MemoryTier, RlmMemory};

// ---------------------------------------------------------------------------
// Write operations for the background persistence channel
// ---------------------------------------------------------------------------

/// Commands sent to the background SQLite writer task.
///
/// Only compiled when the `async-memory` feature is enabled.
#[cfg(feature = "async-memory")]
enum WriteOp {
    /// Store a key-value pair in the specified memory tier.
    Store {
        key: String,
        value: String,
        tier: MemoryTier,
        entry_type: Option<String>,
    },
    /// Remove a key from a specific tier (best-effort; ignores NotFound).
    Delete { key: String, tier: MemoryTier },
    /// Flush all pending writes; notify sender when done.
    Flush(oneshot::Sender<()>),
}

/// Batch window configuration for write-behind buffering.
#[cfg(feature = "async-memory")]
#[derive(Debug, Clone)]
pub struct BatchConfig {
    /// Maximum time to wait before flushing a batch (ms).
    pub window_ms: u64,
    /// Maximum number of ops to buffer before forced flush.
    pub max_batch_size: usize,
}

#[cfg(feature = "async-memory")]
impl Default for BatchConfig {
    fn default() -> Self {
        Self {
            window_ms: 50,
            max_batch_size: 100,
        }
    }
}

// ---------------------------------------------------------------------------
// AsyncRlmMemory
// ---------------------------------------------------------------------------

/// Default channel capacity for bounded write channel.
///
/// Under backpressure, the channel buffers up to this many operations before
/// senders either block or drop. 1024 is sufficient for hook-rate writes
/// (typically <100 ops/s) while preventing unbounded memory growth.
///
/// Only used on the `async-memory` feature path; the sync fallback passes
/// its own capacity literal.
#[cfg(feature = "async-memory")]
const DEFAULT_CHANNEL_CAPACITY: usize = 1024;

/// Async-aware RLM memory with non-blocking writes.
///
/// See [module docs](self) for architecture overview.
pub struct AsyncRlmMemory {
    /// Hot cache: all reads go here first (sub-microsecond).
    ///
    /// Migrated 2026-04-16 from `Arc<{tokio,std}::sync::RwLock<HashMap<String,
    /// String>>>` (behind `#[cfg(feature = "async-memory")]`) to a single
    /// `moka::sync::Cache<String, Arc<String>>`. moka is thread-safe and
    /// callable from both sync and async contexts with zero `.await`
    /// boundaries, so the prior feature-gated type duplication is gone —
    /// one cache serves both code paths. `Arc<String>` values mean hits are
    /// O(1) refcount bumps; the old impl cloned every read.
    cache: Cache<String, Arc<String>>,

    /// Underlying sync persistent store (used for cache-miss reads and
    /// background write target).
    persistent: Arc<Mutex<RlmMemory>>,

    /// Channel to the background writer task.
    /// Bounded with [`DEFAULT_CHANNEL_CAPACITY`] to prevent unbounded growth.
    /// `None` when compiled without `async-memory` feature (synchronous fallback).
    #[cfg(feature = "async-memory")]
    write_tx: mpsc::Sender<WriteOp>,

    /// Default tier used when tier is not specified by caller.
    default_tier: MemoryTier,
}

/// Maximum entries retained in the AsyncRlmMemory hot cache.
/// Conservatively bounded so long sessions cannot leak unbounded memory —
/// the prior `HashMap` had no eviction at all.
const ASYNC_RLM_CACHE_CAPACITY: u64 = 16_384;

/// Build the shared moka configuration for the hot cache. No TTL because
/// callers explicitly manage lifetime via `delete`/`clear`; LRU eviction
/// trims cold entries when `max_capacity` is reached.
fn build_async_rlm_cache() -> Cache<String, Arc<String>> {
    Cache::builder()
        .max_capacity(ASYNC_RLM_CACHE_CAPACITY)
        .build()
}

impl std::fmt::Debug for AsyncRlmMemory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AsyncRlmMemory")
            .field("default_tier", &self.default_tier)
            .finish_non_exhaustive()
    }
}

impl AsyncRlmMemory {
    /// Open (or create) an `AsyncRlmMemory` at `db_path`.
    ///
    /// Spawns a background tokio task for SQLite persistence.
    /// Requires a running tokio runtime.
    ///
    /// # Errors
    ///
    /// Returns [`LearningError::Io`] if the database cannot be opened.
    #[cfg(feature = "async-memory")]
    pub async fn new(db_path: &str) -> LearningResult<Self> {
        let path = PathBuf::from(db_path);

        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(LearningError::Io)?;
        }

        let rlm = RlmMemory::new(&path).map_err(|e| LearningError::Persistence(e.to_string()))?;

        let persistent = Arc::new(Mutex::new(rlm));
        let cache = build_async_rlm_cache();

        let (write_tx, write_rx) = mpsc::channel::<WriteOp>(DEFAULT_CHANNEL_CAPACITY);

        // Spawn background writer with batch windowing
        tokio::spawn(background_writer(
            Arc::clone(&persistent),
            write_rx,
            BatchConfig::default(),
        ));

        Ok(Self {
            cache,
            persistent,
            write_tx,
            default_tier: MemoryTier::Working,
        })
    }

    /// Synchronous constructor for environments without async runtime.
    ///
    /// Falls back to blocking SQLite writes on every `store()` call.
    /// Intended for tests or CLI tools that do not run a tokio executor.
    ///
    /// Available without feature gate for maximum compatibility.
    pub fn new_sync(db_path: &str) -> LearningResult<Self> {
        let path = PathBuf::from(db_path);

        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(LearningError::Io)?;
        }

        let rlm = RlmMemory::new(&path).map_err(|e| LearningError::Persistence(e.to_string()))?;

        Ok(Self {
            cache: build_async_rlm_cache(),
            persistent: Arc::new(Mutex::new(rlm)),
            #[cfg(feature = "async-memory")]
            write_tx: {
                // Bounded channel with capacity 1 — the first send will succeed and
                // subsequent sends will return Full since no receiver holds the other end.
                // Store() will detect Full and fall back to synchronous writes.
                let (tx, _rx) = mpsc::channel::<WriteOp>(1);
                tx
            },
            default_tier: MemoryTier::Working,
        })
    }

    /// Store a key-value pair (non-blocking hot path).
    ///
    /// 1. Writes to the in-memory cache immediately (<1 μs).
    /// 2. Enqueues a `WriteOp::Store` for background SQLite persistence.
    ///
    /// If the background channel is closed (unlikely), falls back to a
    /// synchronous SQLite write within this call.
    ///
    /// # Errors
    ///
    /// Returns an error only if the synchronous fallback write fails.
    pub fn store(&self, key: &str, value: &str, tier: MemoryTier) -> LearningResult<()> {
        self.store_typed(key, value, tier, None)
    }

    /// Store with an explicit `entry_type` tag (e.g., `"lesson"`, `"reward"`).
    pub fn store_typed(
        &self,
        key: &str,
        value: &str,
        tier: MemoryTier,
        entry_type: Option<&str>,
    ) -> LearningResult<()> {
        // 1. Write-through cache (always succeeds)
        self.cache_insert(key, value);

        // 2. Enqueue background write (async path) or write synchronously (fallback)
        #[cfg(feature = "async-memory")]
        {
            let op = WriteOp::Store {
                key: key.to_owned(),
                value: value.to_owned(),
                tier,
                entry_type: entry_type.map(str::to_owned),
            };
            // try_send: synchronous on bounded channel; returns Full if at capacity,
            // Closed if receiver dropped. Both → synchronous fallback.
            if self.write_tx.try_send(op).is_err() {
                // Channel closed or full — fall back to synchronous write
                return self.sync_store(key, value, tier, entry_type);
            }
        }

        #[cfg(not(feature = "async-memory"))]
        self.sync_store(key, value, tier, entry_type)?;

        Ok(())
    }

    /// Recall a value by key (reads cache first, then SQLite on miss).
    ///
    /// # Errors
    ///
    /// Returns [`LearningError::Persistence`] if the SQLite read fails.
    #[cfg(feature = "async-memory")]
    pub async fn recall(&self, key: &str) -> LearningResult<Option<String>> {
        // Fast path: moka hit is a lock-free Arc<String> clone — no `.await`.
        if let Some(val) = self.cache.get(key) {
            return Ok(Some((*val).clone()));
        }

        // Cache miss: delegate to synchronous SQLite with spawn_blocking
        let persistent = Arc::clone(&self.persistent);
        let key_owned = key.to_owned();
        let tier = self.default_tier;

        let result = tokio::task::spawn_blocking(move || {
            let guard = persistent
                .lock()
                .map_err(|e| LearningError::Persistence(format!("mutex poisoned: {e}")))?;
            guard
                .get(&key_owned, tier)
                .map_err(|e| LearningError::Persistence(e.to_string()))
        })
        .await
        .map_err(|e| LearningError::Persistence(format!("spawn_blocking join error: {e}")))??;

        // Populate cache on hit
        if let Some(ref val) = result {
            self.cache_insert(key, val);
        }

        Ok(result)
    }

    /// Synchronous recall — available without feature gate for tests/CLI.
    pub fn recall_sync(&self, key: &str) -> LearningResult<Option<String>> {
        // Fast path: moka `get` is sync and safe in both feature configs —
        // the prior duplicated cfg branches collapse to a single line.
        if let Some(val) = self.cache.get(key) {
            return Ok(Some((*val).clone()));
        }

        // SQLite fallback
        let guard = self
            .persistent
            .lock()
            .map_err(|e| LearningError::Persistence(format!("mutex poisoned: {e}")))?;
        guard
            .get(key, self.default_tier)
            .map_err(|e| LearningError::Persistence(e.to_string()))
    }

    /// Wait for all pending writes to be persisted to SQLite.
    ///
    /// Useful in tests and shutdown sequences. Blocks the calling future until
    /// the background writer confirms all queued `WriteOp`s are flushed.
    #[cfg(feature = "async-memory")]
    pub async fn flush(&self) -> LearningResult<()> {
        let (tx, rx) = oneshot::channel();
        self.write_tx
            .send(WriteOp::Flush(tx))
            .await
            .map_err(|_| LearningError::Persistence("background writer channel closed".into()))?;
        rx.await
            .map_err(|_| LearningError::Persistence("flush reply channel dropped".into()))
    }

    /// Number of entries currently in the hot cache.
    ///
    /// Kept `async` for backward compatibility — the body is a lock-free
    /// moka `entry_count` that needs no `.await`.
    ///
    /// Runs `run_pending_tasks` first so recent `insert`/`invalidate`
    /// operations are reflected in the returned count — moka's internal
    /// maintenance queue is otherwise processed lazily and `entry_count`
    /// can lag writes by a few microseconds.
    #[cfg(feature = "async-memory")]
    pub async fn cache_len(&self) -> usize {
        self.cache.run_pending_tasks();
        self.cache.entry_count() as usize
    }

    /// Sync variant of `cache_len`. Exposed without feature gate so tests,
    /// CLI tools, and the non-async build path can observe cache size
    /// without needing a tokio runtime.
    pub fn cache_len_sync(&self) -> usize {
        self.cache.run_pending_tasks();
        self.cache.entry_count() as usize
    }

    // ------------------------------------------------------------------
    // Private helpers
    // ------------------------------------------------------------------

    /// Insert a key-value pair into the in-memory cache (fire-and-forget).
    ///
    /// moka `insert` is lock-free + sync, so the prior `try_write`
    /// optimistic-skip pattern is no longer needed — writes always succeed
    /// immediately without risk of contention-induced data loss.
    fn cache_insert(&self, key: &str, value: &str) {
        self.cache
            .insert(key.to_owned(), Arc::new(value.to_owned()));
    }

    /// Remove a key from a memory tier (non-blocking hot path).
    ///
    /// 1. Removes the key from the in-memory cache immediately (<1 μs).
    /// 2. Enqueues a `WriteOp::Delete` for background SQLite removal.
    ///
    /// If the background channel is closed, falls back to a synchronous SQLite
    /// delete within this call.
    ///
    /// # Errors
    ///
    /// Returns an error only if the synchronous fallback delete fails.
    pub fn delete(&self, key: &str, tier: MemoryTier) -> LearningResult<()> {
        // 1. Remove from write-through cache — moka invalidate is sync + lock-free,
        // no cfg branches needed (prior impl duplicated a try_write/write dance).
        self.cache.invalidate(key);

        // 2. Enqueue background delete (async path) or delete synchronously (fallback)
        #[cfg(feature = "async-memory")]
        {
            // EC64: wire WriteOp::Delete — send delete through the same background
            // writer channel used by store(), keeping write ordering consistent.
            let op = WriteOp::Delete {
                key: key.to_owned(),
                tier,
            };
            // try_send: synchronous on bounded channel; falls back to sync write if full/closed.
            if self.write_tx.try_send(op).is_err() {
                return self.sync_delete(key, tier);
            }
        }

        #[cfg(not(feature = "async-memory"))]
        self.sync_delete(key, tier)?;

        Ok(())
    }

    /// Synchronous SQLite write (fallback when async channel is unavailable).
    fn sync_store(
        &self,
        key: &str,
        value: &str,
        tier: MemoryTier,
        entry_type: Option<&str>,
    ) -> LearningResult<()> {
        let guard = self
            .persistent
            .lock()
            .map_err(|e| LearningError::Persistence(format!("mutex poisoned: {e}")))?;
        guard
            .store(key, tier, value, entry_type, None)
            .map_err(|e| LearningError::Persistence(e.to_string()))
    }

    /// Synchronous SQLite delete (fallback when async channel is unavailable).
    fn sync_delete(&self, key: &str, tier: MemoryTier) -> LearningResult<()> {
        let guard = self
            .persistent
            .lock()
            .map_err(|e| LearningError::Persistence(format!("mutex poisoned: {e}")))?;
        guard
            .delete(key, tier)
            .map(|_| ())
            .map_err(|e| LearningError::Persistence(e.to_string()))
    }
}

// ---------------------------------------------------------------------------
// Background writer task
// ---------------------------------------------------------------------------

/// Drain the `WriteOp` channel with batch windowing and apply to SQLite.
///
/// Groups writes that arrive within `batch_config.window_ms` into a single
/// [`apply_batch`] call, reducing fsync overhead from N×15 ms to ~15 ms/window.
/// Exits cleanly when the sender half of the channel is dropped.
#[cfg(feature = "async-memory")]
async fn background_writer(
    persistent: Arc<Mutex<RlmMemory>>,
    mut rx: mpsc::Receiver<WriteOp>,
    batch_config: BatchConfig,
) {
    use tokio::time::{Duration, Instant};

    loop {
        // Block until the first op arrives (no busy-spin)
        let first = match rx.recv().await {
            Some(op) => op,
            None => break,
        };

        // Fast-path: Flush with no pending ops — reply immediately
        if let WriteOp::Flush(reply) = first {
            let _ = reply.send(());
            continue;
        }

        // Start a batch window
        let mut batch: Vec<WriteOp> = vec![first];
        let deadline = Instant::now() + Duration::from_millis(batch_config.window_ms);

        // Drain channel within the window or until max_batch_size
        loop {
            if batch.len() >= batch_config.max_batch_size {
                break;
            }
            let now = Instant::now();
            if now >= deadline {
                break;
            }
            match tokio::time::timeout(deadline - now, rx.recv()).await {
                Ok(Some(WriteOp::Flush(reply))) => {
                    // Flush in-band: apply current batch, then reply
                    apply_batch(&persistent, std::mem::take(&mut batch)).await;
                    let _ = reply.send(());
                    break;
                }
                Ok(Some(op)) => batch.push(op),
                Ok(None) => {
                    // Channel closed — flush remaining ops before exiting
                    if !batch.is_empty() {
                        apply_batch(&persistent, batch).await;
                    }
                    tracing::debug!("async_rlm background writer exiting (channel closed)");
                    return;
                }
                Err(_) => break, // window expired
            }
        }

        if !batch.is_empty() {
            apply_batch(&persistent, batch).await;
        }
    }

    tracing::debug!("async_rlm background writer exiting (channel closed)");
}

/// Apply a batch of [`WriteOp`]s in a single `spawn_blocking` call.
///
/// Using a single call per batch amortises the async/sync boundary crossing
/// and allows SQLite to coalesce the writes into fewer fsyncs.
#[cfg(feature = "async-memory")]
async fn apply_batch(persistent: &Arc<Mutex<RlmMemory>>, ops: Vec<WriteOp>) {
    if ops.is_empty() {
        return;
    }
    let result = tokio::task::spawn_blocking({
        let persistent = Arc::clone(persistent);
        move || {
            let guard = match persistent.lock() {
                Ok(g) => g,
                Err(e) => {
                    tracing::warn!("async_rlm mutex poisoned in apply_batch: {}", e);
                    return;
                }
            };
            for op in ops {
                match op {
                    WriteOp::Store {
                        key,
                        value,
                        tier,
                        entry_type,
                    } => {
                        if let Err(e) = guard.store(&key, tier, &value, entry_type.as_deref(), None)
                        {
                            tracing::debug!(key = %key, "async_rlm batch store error: {}", e);
                        }
                    }
                    WriteOp::Delete { key, tier } => {
                        if let Err(e) = guard.delete(&key, tier) {
                            tracing::debug!(key = %key, "async_rlm batch delete error: {}", e);
                        }
                    }
                    WriteOp::Flush(_) => {
                        // Flush ops handled before reaching apply_batch
                    }
                }
            }
        }
    })
    .await;

    if let Err(e) = result {
        tracing::warn!("async_rlm apply_batch join error: {}", e);
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn sync_store_and_recall() {
        let dir = tempdir().expect("tempdir");
        let db_path = dir.path().join("test.db").to_string_lossy().to_string();

        let mem = AsyncRlmMemory::new_sync(&db_path).expect("new_sync");
        mem.store("k1", "hello", MemoryTier::Working)
            .expect("store");

        let val = mem.recall_sync("k1").expect("recall");
        assert_eq!(val, Some("hello".to_owned()));
    }

    #[test]
    fn sync_cache_hit_avoids_sqlite() {
        let dir = tempdir().expect("tempdir");
        let db_path = dir.path().join("test2.db").to_string_lossy().to_string();

        let mem = AsyncRlmMemory::new_sync(&db_path).expect("new_sync");
        mem.store("key", "cached_value", MemoryTier::Ephemeral)
            .expect("store");

        // Second recall hits cache (not SQLite)
        let v1 = mem.recall_sync("key").expect("first recall");
        let v2 = mem.recall_sync("key").expect("second recall");
        assert_eq!(v1, v2);
    }

    #[test]
    fn sync_recall_missing_key_returns_none() {
        let dir = tempdir().expect("tempdir");
        let db_path = dir.path().join("test3.db").to_string_lossy().to_string();

        let mem = AsyncRlmMemory::new_sync(&db_path).expect("new_sync");
        let val = mem.recall_sync("nonexistent").expect("recall");
        assert_eq!(val, None);
    }

    #[cfg(feature = "async-memory")]
    #[tokio::test]
    async fn async_store_and_flush() {
        let dir = tempdir().expect("tempdir");
        let db_path = dir
            .path()
            .join("async_test.db")
            .to_string_lossy()
            .to_string();

        let mem = AsyncRlmMemory::new(&db_path).await.expect("new async");
        mem.store("ak1", "async_val", MemoryTier::Working)
            .expect("store");

        // Flush ensures background write completes before we recall
        mem.flush().await.expect("flush");

        let val = mem.recall("ak1").await.expect("recall");
        assert_eq!(val, Some("async_val".to_owned()));
    }

    #[cfg(feature = "async-memory")]
    #[tokio::test]
    async fn async_cache_populated_on_miss() {
        let dir = tempdir().expect("tempdir");
        let db_path = dir
            .path()
            .join("async_test2.db")
            .to_string_lossy()
            .to_string();

        let mem = AsyncRlmMemory::new(&db_path).await.expect("new async");
        mem.store("ck", "cv", MemoryTier::Core).expect("store");
        mem.flush().await.expect("flush");

        // First recall: SQLite miss (cache may not have it yet if try_write lost)
        // After recall, cache is populated.
        let v1 = mem.recall("ck").await.expect("first recall");
        let cache_size = mem.cache_len().await;
        assert!(cache_size >= 1);
        assert_eq!(v1, Some("cv".to_owned()));
    }
}
