//! ESAA (Event Sourcing Agent Architecture) — Rust computational cores.
//!
//! - QueryCache\<V\>: Thread-safe LRU cache with TTL expiry and hit/miss statistics.
//! - EventBuffer: Write-ahead batch buffer for domain events.
//!
//! Unified from rust-core/src/aco/esaa.rs (1346 LOC)
//! PyO3 wrappers (PyQueryCache, PyEventBuffer, PyEventProjector, verify_chain_parallel)
//! removed — they belong in touring-python.

use std::sync::RwLock;
use std::time::Instant;

use lru::LruCache;
use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// QueryCache
// ---------------------------------------------------------------------------

/// Statistics for a QueryCache instance.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct QueryCacheStats {
    /// Number of lookups that found a live entry.
    pub hits: u64,
    /// Number of lookups that found nothing (or an expired entry).
    pub misses: u64,
    /// Number of entries evicted due to capacity pressure.
    pub evictions: u64,
    /// Number of entries dropped because their TTL elapsed.
    pub expired: u64,
    /// Current number of entries held in the cache.
    pub current_size: usize,
    /// Maximum capacity of the cache.
    pub max_size: usize,
}

impl QueryCacheStats {
    /// Fraction of lookups that were hits, in `[0.0, 1.0]` (`0.0` if no lookups).
    pub fn hit_rate(&self) -> f64 {
        let total = self.hits + self.misses;
        if total == 0 {
            0.0
        } else {
            self.hits as f64 / total as f64
        }
    }
}

struct CacheEntry<V> {
    value: V,
    created_at: Instant,
}

impl<V: std::fmt::Debug> std::fmt::Debug for CacheEntry<V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CacheEntry")
            .field("value", &self.value)
            .field("created_at", &self.created_at)
            .finish()
    }
}

struct CacheInner<V> {
    lru: LruCache<String, CacheEntry<V>>,
    ttl: Option<std::time::Duration>,
    hits: u64,
    misses: u64,
    evictions: u64,
    expired: u64,
}

impl<V: std::fmt::Debug> std::fmt::Debug for CacheInner<V> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CacheInner")
            .field("ttl", &self.ttl)
            .field("hits", &self.hits)
            .field("misses", &self.misses)
            .field("evictions", &self.evictions)
            .field("expired", &self.expired)
            .finish()
    }
}

/// Thread-safe LRU cache with optional TTL expiry.
#[derive(Debug)]
pub struct QueryCache<V: Clone> {
    inner: RwLock<CacheInner<V>>,
    max_size: usize,
}

impl<V: Clone> QueryCache<V> {
    /// Create a cache bounded to `max_size` entries with optional TTL in ms.
    pub fn new(max_size: usize, ttl_ms: Option<u64>) -> Self {
        assert!(max_size > 0, "max_size must be > 0");
        let cap = match std::num::NonZeroUsize::new(max_size) {
            Some(c) => c,
            None => {
                return Self {
                    inner: RwLock::new(CacheInner {
                        lru: LruCache::unbounded(),
                        ttl: ttl_ms.map(std::time::Duration::from_millis),
                        hits: 0,
                        misses: 0,
                        evictions: 0,
                        expired: 0,
                    }),
                    max_size: 1,
                };
            }
        };
        Self {
            inner: RwLock::new(CacheInner {
                lru: LruCache::new(cap),
                ttl: ttl_ms.map(std::time::Duration::from_millis),
                hits: 0,
                misses: 0,
                evictions: 0,
                expired: 0,
            }),
            max_size,
        }
    }

    /// Look up `key`, returning a clone of the value if present and not expired.
    pub fn get(&self, key: &str) -> Option<V> {
        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = inner.lru.peek(key) {
            if let Some(ttl) = inner.ttl {
                if entry.created_at.elapsed() > ttl {
                    inner.lru.pop(key);
                    inner.expired += 1;
                    inner.misses += 1;
                    return None;
                }
            }
            let val = inner.lru.get(key).map(|e| e.value.clone());
            inner.hits += 1;
            val
        } else {
            inner.misses += 1;
            None
        }
    }

    /// Insert or replace `value` under `key`, evicting the LRU entry if full.
    pub fn put(&self, key: String, value: V) {
        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());
        let was_full = inner.lru.len() == inner.lru.cap().get();
        let is_new = !inner.lru.contains(&key);
        inner.lru.put(
            key,
            CacheEntry {
                value,
                created_at: Instant::now(),
            },
        );
        if was_full && is_new {
            inner.evictions += 1;
        }
    }

    /// Remove a single entry by key; returns `true` if it was present.
    pub fn invalidate(&self, key: &str) -> bool {
        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());
        inner.lru.pop(key).is_some()
    }

    /// Remove all entries whose key contains `pattern`; returns the count removed.
    pub fn invalidate_pattern(&self, pattern: &str) -> usize {
        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());
        let keys_to_remove: Vec<String> = inner
            .lru
            .iter()
            .filter(|(k, _)| k.contains(pattern))
            .map(|(k, _)| k.clone())
            .collect();
        let count = keys_to_remove.len();
        for key in keys_to_remove {
            inner.lru.pop(&key);
        }
        count
    }

    /// Drop all entries; returns the number that were removed.
    pub fn clear(&self) -> usize {
        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());
        let count = inner.lru.len();
        inner.lru.clear();
        count
    }

    /// Snapshot the cache's current hit/miss/eviction statistics.
    pub fn stats(&self) -> QueryCacheStats {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        QueryCacheStats {
            hits: inner.hits,
            misses: inner.misses,
            evictions: inner.evictions,
            expired: inner.expired,
            current_size: inner.lru.len(),
            max_size: self.max_size,
        }
    }

    /// Current number of entries held in the cache.
    pub fn len(&self) -> usize {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        inner.lru.len()
    }

    /// `true` when the cache holds no entries.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// `true` if `key` is present and (if a TTL is set) not yet expired.
    pub fn contains(&self, key: &str) -> bool {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = inner.lru.peek(key) {
            if let Some(ttl) = inner.ttl {
                return entry.created_at.elapsed() <= ttl;
            }
            true
        } else {
            false
        }
    }
}

// ---------------------------------------------------------------------------
// EventBuffer
// ---------------------------------------------------------------------------

/// Statistics for an EventBuffer instance.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EventBufferStats {
    /// Number of events currently buffered (not yet flushed).
    pub buffer_size: usize,
    /// Total number of flushes performed over the buffer's lifetime.
    pub flush_count: u64,
    /// Milliseconds elapsed since the last flush.
    pub last_flush_ms_ago: f64,
}

/// Configuration for EventBuffer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventBufferConfig {
    /// Flush once the buffer reaches this many events.
    pub max_batch_size: usize,
    /// Flush once the oldest unflushed event reaches this age in milliseconds.
    pub max_age_ms: u64,
}

impl Default for EventBufferConfig {
    fn default() -> Self {
        Self {
            max_batch_size: 100,
            max_age_ms: 50,
        }
    }
}

#[derive(Debug)]
struct BufferInner {
    events: Vec<String>,
    last_flush: Instant,
    flush_count: u64,
    shutdown: bool,
}

/// Write-ahead batch buffer for domain events.
#[derive(Debug)]
pub struct EventBuffer {
    config: EventBufferConfig,
    inner: RwLock<BufferInner>,
}

/// Errors that can occur during EventBuffer operations.
#[derive(Debug, Clone, thiserror::Error)]
pub enum EventBufferError {
    /// The buffer has been shut down and no longer accepts events.
    #[error("EventBuffer has been shut down")]
    Shutdown,
}

/// Error from [`EsaaCoordinator::register`] (F-8 / RBP-03: typed in place of `String`).
#[derive(Debug, thiserror::Error)]
pub enum EsaaRegisterError {
    /// A subsystem with the same id is already registered.
    #[error("subsystem already registered: {0}")]
    AlreadyRegistered(String),
}

impl EventBuffer {
    /// Construct an empty buffer governed by the given config.
    pub fn new(config: EventBufferConfig) -> Self {
        Self {
            inner: RwLock::new(BufferInner {
                events: Vec::new(),
                last_flush: Instant::now(),
                flush_count: 0,
                shutdown: false,
            }),
            config,
        }
    }

    /// Append a JSON-encoded event; errors if the buffer is shut down.
    pub fn push(&self, event_json: String) -> Result<(), EventBufferError> {
        // M3 fix: directly acquire write lock (removed TOCTOU read-lock fast-path)
        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());
        if inner.shutdown {
            return Err(EventBufferError::Shutdown);
        }
        inner.events.push(event_json);
        Ok(())
    }

    /// Drain and return all buffered events, recording a flush.
    pub fn flush(&self) -> Vec<String> {
        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());
        if inner.events.is_empty() {
            return Vec::new();
        }
        let batch = std::mem::take(&mut inner.events);
        inner.last_flush = Instant::now();
        inner.flush_count += 1;
        batch
    }

    /// Number of events currently buffered.
    pub fn len(&self) -> usize {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        inner.events.len()
    }

    /// `true` when no events are buffered.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// `true` when the batch-size or max-age flush threshold has been reached.
    pub fn is_ready(&self) -> bool {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        if inner.events.is_empty() {
            return false;
        }
        if inner.events.len() >= self.config.max_batch_size {
            return true;
        }
        let elapsed_ms = inner.last_flush.elapsed().as_millis() as u64;
        elapsed_ms >= self.config.max_age_ms
    }

    /// Mark the buffer shut down; optionally drain and return remaining events.
    pub fn shutdown(&self, flush_remaining: bool) -> Option<Vec<String>> {
        let mut inner = self.inner.write().unwrap_or_else(|e| e.into_inner());
        inner.shutdown = true;
        if flush_remaining {
            let batch = std::mem::take(&mut inner.events);
            inner.flush_count += 1;
            Some(batch)
        } else {
            None
        }
    }

    /// Snapshot the buffer's current size, flush count, and time since flush.
    pub fn stats(&self) -> EventBufferStats {
        let inner = self.inner.read().unwrap_or_else(|e| e.into_inner());
        EventBufferStats {
            buffer_size: inner.events.len(),
            flush_count: inner.flush_count,
            last_flush_ms_ago: inner.last_flush.elapsed().as_secs_f64() * 1000.0,
        }
    }
}

// ── rkyv Zero-Copy Event Store ─────────────────────────────────────────

/// Event record optimized for rkyv zero-copy serialization.
/// Used for IPC between Touring processes (NOT for Claude Code hook boundary).
#[derive(rkyv::Archive, rkyv::Deserialize, rkyv::Serialize, Debug, Clone)]
#[archive(compare(PartialEq), check_bytes)]
#[archive_attr(derive(Debug))]
pub struct EventRecord {
    /// Session the event belongs to.
    pub session_id: String,
    /// Name of the tool that produced the event.
    pub tool_name: String,
    /// Recorded outcome (e.g. `"success"`, `"failure"`).
    pub outcome: String,
    /// Execution latency in milliseconds.
    pub latency_ms: u64,
    /// CILA complexity level associated with the action.
    pub cila_level: u8,
    /// Event timestamp in milliseconds since the Unix epoch.
    pub timestamp_ms: u64,
}

/// Zero-copy reader for rkyv-serialized event files via mmap.
#[derive(Debug)]
pub struct EsaaRkyvReader {
    mmap: memmap2::Mmap,
}

impl EsaaRkyvReader {
    /// Open an rkyv event file for zero-copy reading.
    pub fn open(path: &std::path::Path) -> Result<Self, EsaaReaderError> {
        let file = std::fs::File::open(path).map_err(EsaaReaderError::Open)?;
        // SAFETY: The file handle is valid (just opened successfully above). The mmap is
        // read-only (default for MmapOptions) and no concurrent writers exist because we
        // only read event files that have already been fully written. The mmap lifetime is
        // tied to `EsaaRkyvReader` which owns it, so it cannot outlive the mapping.
        let mmap =
            unsafe { memmap2::MmapOptions::new().map(&file) }.map_err(EsaaReaderError::Mmap)?;
        Ok(Self { mmap })
    }

    /// Read and validate an EventRecord at the given byte offset.
    #[allow(clippy::indexing_slicing)]
    pub fn read_validated(&self, offset: usize) -> Result<&ArchivedEventRecord, EsaaReaderError> {
        if offset >= self.mmap.len() {
            return Err(EsaaReaderError::OutOfBounds {
                offset,
                size: self.mmap.len(),
            });
        }
        rkyv::check_archived_root::<EventRecord>(&self.mmap[offset..])
            .map_err(|e| EsaaReaderError::Validate(e.to_string()))
    }

    /// Read an EventRecord at offset WITHOUT validation (faster, unsafe).
    ///
    /// # Safety
    ///
    /// - `offset` must be within bounds of the memory-mapped region.
    /// - The data at `offset` must be a valid `rkyv`-archived `EventRecord`,
    ///   i.e., it must have been produced by `rkyv::to_bytes::<EventRecord, _>()`.
    /// - The archived data must be properly aligned for `ArchivedEventRecord`.
    #[allow(clippy::indexing_slicing)]
    pub unsafe fn read_unchecked(&self, offset: usize) -> &ArchivedEventRecord {
        // SAFETY: per this fn's `# Safety` contract, the caller guarantees `offset`
        // is in bounds and the bytes there are a valid, aligned `rkyv`-archived
        // `EventRecord` — exactly the preconditions `archived_root` requires.
        unsafe { rkyv::archived_root::<EventRecord>(&self.mmap[offset..]) }
    }

    /// Get the total size of the mmap'd file.
    pub fn len(&self) -> usize {
        self.mmap.len()
    }

    /// Check if the mmap is empty.
    pub fn is_empty(&self) -> bool {
        self.mmap.is_empty()
    }
}

/// Serialize an EventRecord to bytes for writing to an rkyv file.
pub fn serialize_event_record(record: &EventRecord) -> Result<rkyv::AlignedVec, EsaaReaderError> {
    rkyv::to_bytes::<_, 256>(record).map_err(|e| EsaaReaderError::Serialize(e.to_string()))
}

/// Errors returned by the [`EsaaRkyvReader`] zero-copy reader and the
/// [`serialize_event_record`] writer helper.
///
/// `Open` / `Mmap` wrap the underlying [`std::io::Error`] via `#[source]`;
/// `OutOfBounds` is a structured bounds violation; `Validate` / `Serialize`
/// carry the rkyv diagnostic. Display messages match the prior `String`
/// contract verbatim.
#[derive(Debug, thiserror::Error)]
pub enum EsaaReaderError {
    /// Opening the event file failed.
    #[error("Cannot open rkyv file: {0}")]
    Open(#[source] std::io::Error),
    /// Memory-mapping the event file failed.
    #[error("Cannot mmap file: {0}")]
    Mmap(#[source] std::io::Error),
    /// Requested offset is beyond the mapped file size.
    #[error("Offset {offset} beyond file size {size}")]
    OutOfBounds {
        /// The requested byte offset.
        offset: usize,
        /// The total size of the mapped file.
        size: usize,
    },
    /// rkyv archive validation failed (corrupt record).
    #[error("rkyv validation failed: {0}")]
    Validate(String),
    /// rkyv serialization of an event record failed.
    #[error("rkyv serialization failed: {0}")]
    Serialize(String),
}

// ---------------------------------------------------------------------------
// INS-8: ESAA Complete 24 Subsystems
// ---------------------------------------------------------------------------

use indexmap::IndexMap;

/// Input envelope for ESAA subsystem processing.
pub struct EsaaInput {
    /// The event type used for routing decisions.
    pub event_type: String,
    /// Raw payload bytes.
    pub payload: Vec<u8>,
    /// Arbitrary key-value metadata.
    pub metadata: std::collections::HashMap<String, String>,
}

/// Output produced by an ESAA subsystem after processing.
pub struct EsaaOutput {
    /// Whether the subsystem processed the input successfully.
    pub success: bool,
    /// Result payload bytes.
    pub result: Vec<u8>,
    /// Processing latency in microseconds.
    pub latency_us: u64,
}

/// Trait implemented by all ESAA subsystems.
pub trait EsaaSubsystem: Send + Sync {
    /// Returns the unique identifier of this subsystem.
    fn subsystem_id(&self) -> &str;
    /// Processes an input and produces an output.
    fn process(&self, input: &EsaaInput) -> EsaaOutput;
    /// Returns `true` if the subsystem is healthy.
    fn health_check(&self) -> bool;
}

/// Central coordinator that manages subsystem registration and event routing.
pub struct EsaaCoordinator {
    subsystems: IndexMap<String, Box<dyn EsaaSubsystem>>,
    routing_table: std::collections::HashMap<String, Vec<String>>,
}

impl EsaaCoordinator {
    /// Creates a new empty coordinator.
    pub fn new() -> Self {
        Self {
            subsystems: IndexMap::new(),
            routing_table: std::collections::HashMap::new(),
        }
    }

    /// Registers a subsystem. Returns `Err` if the ID is already registered.
    ///
    /// Consistent with `PhaseRegistry::register()` — duplicates are rejected so
    /// accidental overwrites do not silently corrupt the routing table.
    pub fn register(&mut self, subsystem: Box<dyn EsaaSubsystem>) -> Result<(), EsaaRegisterError> {
        let id = subsystem.subsystem_id().to_owned();
        if self.subsystems.contains_key(&id) {
            return Err(EsaaRegisterError::AlreadyRegistered(id));
        }
        self.subsystems.insert(id, subsystem);
        Ok(())
    }

    /// Routes an input to the appropriate subsystems.
    ///
    /// If there is a routing entry for `input.event_type`, only those subsystems
    /// are called. Otherwise, ALL registered subsystems are called.
    pub fn route(&self, input: EsaaInput) -> Vec<EsaaOutput> {
        let target_ids: Option<&Vec<String>> = self.routing_table.get(&input.event_type);

        match target_ids {
            Some(ids) => ids
                .iter()
                .filter_map(|id| self.subsystems.get(id))
                .map(|s| s.process(&input))
                .collect(),
            None => self
                .subsystems
                .values()
                .map(|s| s.process(&input))
                .collect(),
        }
    }

    /// Returns a map of subsystem ID to health status.
    pub fn health_report(&self) -> std::collections::HashMap<String, bool> {
        self.subsystems
            .iter()
            .map(|(id, s)| (id.clone(), s.health_check()))
            .collect()
    }

    /// Adds a routing entry: events of `event_type` will be dispatched to `subsystem_id`.
    pub fn add_route(&mut self, event_type: &str, subsystem_id: &str) {
        self.routing_table
            .entry(event_type.to_owned())
            .or_default()
            .push(subsystem_id.to_owned());
    }
}

impl Default for EsaaCoordinator {
    fn default() -> Self {
        Self::new()
    }
}

// ---------------------------------------------------------------------------
// 24 ESAA Subsystems
// ---------------------------------------------------------------------------

/// Macro for scaffold subsystems — extensible skeletons with pass-through behaviour.
macro_rules! define_esaa_subsystem {
    ($name:ident) => {
        /// ESAA subsystem: `$name`.
        pub struct $name {
            id: String,
        }

        impl $name {
            /// Creates a new instance with the given identifier.
            pub fn new(id: impl Into<String>) -> Self {
                Self { id: id.into() }
            }
        }

        impl EsaaSubsystem for $name {
            fn subsystem_id(&self) -> &str {
                &self.id
            }

            fn process(&self, input: &EsaaInput) -> EsaaOutput {
                EsaaOutput {
                    success: true,
                    result: input.payload.clone(),
                    latency_us: 1,
                }
            }

            fn health_check(&self) -> bool {
                true
            }
        }
    };
}

// Scaffold subsystems (18) — pass-through, extensible by domain.
define_esaa_subsystem!(Planner);
define_esaa_subsystem!(Executor);
define_esaa_subsystem!(EsaaCoordinatorSubsystem);
define_esaa_subsystem!(Scheduler);
define_esaa_subsystem!(Notifier);
define_esaa_subsystem!(Aggregator);
define_esaa_subsystem!(Dispatcher);
define_esaa_subsystem!(Collector);
define_esaa_subsystem!(Reporter);
define_esaa_subsystem!(Auditor);
define_esaa_subsystem!(Archiver);
define_esaa_subsystem!(Indexer);
define_esaa_subsystem!(Searcher);
define_esaa_subsystem!(Learner);
define_esaa_subsystem!(Predictor);
define_esaa_subsystem!(Optimizer);
define_esaa_subsystem!(Balancer);
define_esaa_subsystem!(Observer);

// ---------------------------------------------------------------------------
// Differentiated subsystems (6) — real domain behaviour
// ---------------------------------------------------------------------------

/// Routes events: result contains the event_type bytes for downstream routing info.
pub struct Router {
    id: String,
}
impl Router {
    /// Create a `Router` subsystem with the given id.
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}
impl EsaaSubsystem for Router {
    fn subsystem_id(&self) -> &str {
        &self.id
    }
    fn process(&self, input: &EsaaInput) -> EsaaOutput {
        EsaaOutput {
            success: true,
            result: input.event_type.as_bytes().to_vec(),
            latency_us: 1,
        }
    }
    fn health_check(&self) -> bool {
        true
    }
}

/// Validates that payload is non-empty. Returns `success=false` for empty payloads.
pub struct Validator {
    id: String,
}
impl Validator {
    /// Create a `Validator` subsystem with the given id.
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}
impl EsaaSubsystem for Validator {
    fn subsystem_id(&self) -> &str {
        &self.id
    }
    fn process(&self, input: &EsaaInput) -> EsaaOutput {
        let ok = !input.payload.is_empty();
        EsaaOutput {
            success: ok,
            result: if ok {
                input.payload.clone()
            } else {
                b"validation_failed: empty payload".to_vec()
            },
            latency_us: 1,
        }
    }
    fn health_check(&self) -> bool {
        true
    }
}

/// Monitors system health: always returns a `b"ok"` heartbeat.
pub struct Monitor {
    id: String,
}
impl Monitor {
    /// Create a `Monitor` subsystem with the given id.
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}
impl EsaaSubsystem for Monitor {
    fn subsystem_id(&self) -> &str {
        &self.id
    }
    fn process(&self, _input: &EsaaInput) -> EsaaOutput {
        EsaaOutput {
            success: true,
            result: b"ok".to_vec(),
            latency_us: 1,
        }
    }
    fn health_check(&self) -> bool {
        true
    }
}

/// Transforms payload: applies wrapping byte increment (+1 mod 256) to each byte.
pub struct Transformer {
    id: String,
}
impl Transformer {
    /// Create a `Transformer` subsystem with the given id.
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}
impl EsaaSubsystem for Transformer {
    fn subsystem_id(&self) -> &str {
        &self.id
    }
    fn process(&self, input: &EsaaInput) -> EsaaOutput {
        let result: Vec<u8> = input.payload.iter().map(|b| b.wrapping_add(1)).collect();
        EsaaOutput {
            success: true,
            result,
            latency_us: 1,
        }
    }
    fn health_check(&self) -> bool {
        true
    }
}

/// Filters events: only passes through if metadata contains key `"allowed"`.
pub struct Filter {
    id: String,
}
impl Filter {
    /// Create a `Filter` subsystem with the given id.
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}
impl EsaaSubsystem for Filter {
    fn subsystem_id(&self) -> &str {
        &self.id
    }
    fn process(&self, input: &EsaaInput) -> EsaaOutput {
        let allowed = input.metadata.contains_key("allowed");
        EsaaOutput {
            success: allowed,
            result: if allowed {
                input.payload.clone()
            } else {
                b"filtered".to_vec()
            },
            latency_us: 1,
        }
    }
    fn health_check(&self) -> bool {
        true
    }
}

/// Analyzes payload: result is a UTF-8 metrics string `"len=N,meta=M"`.
pub struct Analyzer {
    id: String,
}
impl Analyzer {
    /// Create an `Analyzer` subsystem with the given id.
    pub fn new(id: impl Into<String>) -> Self {
        Self { id: id.into() }
    }
}
impl EsaaSubsystem for Analyzer {
    fn subsystem_id(&self) -> &str {
        &self.id
    }
    fn process(&self, input: &EsaaInput) -> EsaaOutput {
        let report = format!("len={},meta={}", input.payload.len(), input.metadata.len());
        EsaaOutput {
            success: true,
            result: report.into_bytes(),
            latency_us: 1,
        }
    }
    fn health_check(&self) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_cache_put_and_get() {
        let cache = QueryCache::<String>::new(10, None);
        cache.put("k1".into(), "v1".into());
        assert_eq!(cache.get("k1"), Some("v1".into()));
        assert_eq!(cache.get("k3"), None);
    }

    #[test]
    fn test_cache_lru_eviction() {
        let cache = QueryCache::<String>::new(3, None);
        cache.put("a".into(), "1".into());
        cache.put("b".into(), "2".into());
        cache.put("c".into(), "3".into());
        assert_eq!(cache.get("a"), Some("1".into()));
        cache.put("d".into(), "4".into());
        assert_eq!(cache.get("b"), None);
    }

    #[test]
    fn test_cache_ttl_expiry() {
        let cache = QueryCache::<String>::new(10, Some(50));
        cache.put("k1".into(), "v1".into());
        assert_eq!(cache.get("k1"), Some("v1".into()));
        thread::sleep(Duration::from_millis(80));
        assert_eq!(cache.get("k1"), None);
    }

    #[test]
    fn test_buffer_push_and_flush() {
        let buf = EventBuffer::new(EventBufferConfig {
            max_batch_size: 100,
            max_age_ms: 5000,
        });
        buf.push("e1".into()).unwrap();
        buf.push("e2".into()).unwrap();
        let batch = buf.flush();
        assert_eq!(batch.len(), 2);
        assert!(buf.is_empty());
    }

    #[test]
    fn test_buffer_batch_size_trigger() {
        let buf = EventBuffer::new(EventBufferConfig {
            max_batch_size: 3,
            max_age_ms: 60_000,
        });
        buf.push("e1".into()).unwrap();
        assert!(!buf.is_ready());
        buf.push("e2".into()).unwrap();
        buf.push("e3".into()).unwrap();
        assert!(buf.is_ready());
    }

    #[test]
    fn test_buffer_shutdown() {
        let buf = EventBuffer::new(EventBufferConfig::default());
        buf.push("e1".into()).unwrap();
        let remaining = buf.shutdown(true);
        assert_eq!(remaining.unwrap().len(), 1);
        assert!(buf.push("e2".into()).is_err());
    }

    #[test]
    fn test_rkyv_event_record_roundtrip() {
        let record = EventRecord {
            session_id: "sess_001".into(),
            tool_name: "Read".into(),
            outcome: "success".into(),
            latency_ms: 42,
            cila_level: 2,
            timestamp_ms: 1_700_000_000_000,
        };
        let bytes = serialize_event_record(&record).unwrap();
        let archived = rkyv::check_archived_root::<EventRecord>(&bytes).unwrap();
        assert_eq!(archived.tool_name.as_str(), "Read");
        assert_eq!(archived.latency_ms, 42);
        assert_eq!(archived.cila_level, 2);
    }

    #[test]
    fn test_rkyv_mmap_reader() {
        use std::io::Write;
        use tempfile::NamedTempFile;

        let record = EventRecord {
            session_id: "sess_002".into(),
            tool_name: "Bash".into(),
            outcome: "error".into(),
            latency_ms: 150,
            cila_level: 3,
            timestamp_ms: 1_700_000_001_000,
        };
        let bytes = serialize_event_record(&record).unwrap();

        let mut tmpfile = NamedTempFile::new().unwrap();
        tmpfile.write_all(&bytes).unwrap();
        tmpfile.flush().unwrap();

        let reader = EsaaRkyvReader::open(tmpfile.path()).unwrap();
        assert!(!reader.is_empty());
        let archived = reader.read_validated(0).unwrap();
        assert_eq!(archived.tool_name.as_str(), "Bash");
        assert_eq!(archived.outcome.as_str(), "error");
        assert_eq!(archived.latency_ms, 150);
    }

    // --- ESAA Subsystem tests (INS-8) ---

    #[test]
    fn test_esaa_coordinator_register_all_24() {
        let mut coord = EsaaCoordinator::new();
        coord.register(Box::new(Router::new("router-1"))).unwrap();
        coord.register(Box::new(Planner::new("planner-1"))).unwrap();
        coord
            .register(Box::new(Executor::new("executor-1")))
            .unwrap();
        coord
            .register(Box::new(Validator::new("validator-1")))
            .unwrap();
        coord.register(Box::new(Monitor::new("monitor-1"))).unwrap();
        coord
            .register(Box::new(EsaaCoordinatorSubsystem::new("coordinator-1")))
            .unwrap();
        coord
            .register(Box::new(Scheduler::new("scheduler-1")))
            .unwrap();
        coord
            .register(Box::new(Notifier::new("notifier-1")))
            .unwrap();
        coord
            .register(Box::new(Aggregator::new("aggregator-1")))
            .unwrap();
        coord
            .register(Box::new(Transformer::new("transformer-1")))
            .unwrap();
        coord.register(Box::new(Filter::new("filter-1"))).unwrap();
        coord
            .register(Box::new(Dispatcher::new("dispatcher-1")))
            .unwrap();
        coord
            .register(Box::new(Collector::new("collector-1")))
            .unwrap();
        coord
            .register(Box::new(Analyzer::new("analyzer-1")))
            .unwrap();
        coord
            .register(Box::new(Reporter::new("reporter-1")))
            .unwrap();
        coord.register(Box::new(Auditor::new("auditor-1"))).unwrap();
        coord
            .register(Box::new(Archiver::new("archiver-1")))
            .unwrap();
        coord.register(Box::new(Indexer::new("indexer-1"))).unwrap();
        coord
            .register(Box::new(Searcher::new("searcher-1")))
            .unwrap();
        coord.register(Box::new(Learner::new("learner-1"))).unwrap();
        coord
            .register(Box::new(Predictor::new("predictor-1")))
            .unwrap();
        coord
            .register(Box::new(Optimizer::new("optimizer-1")))
            .unwrap();
        coord
            .register(Box::new(Balancer::new("balancer-1")))
            .unwrap();
        coord
            .register(Box::new(Observer::new("observer-1")))
            .unwrap();

        let report = coord.health_report();
        assert_eq!(report.len(), 24);
        assert!(report.values().all(|&v| v));
    }

    #[test]
    fn test_esaa_coordinator_register_duplicate_fails() {
        let mut coord = EsaaCoordinator::new();
        coord.register(Box::new(Router::new("router-1"))).unwrap();
        let result = coord.register(Box::new(Router::new("router-1")));
        assert!(result.is_err(), "duplicate registration must fail");
        assert!(result.unwrap_err().to_string().contains("router-1"));
    }

    #[test]
    fn test_esaa_router_routes_correctly() {
        let mut coord = EsaaCoordinator::new();
        coord.register(Box::new(Router::new("router-1"))).unwrap();
        coord.register(Box::new(Planner::new("planner-1"))).unwrap();
        coord.add_route("event_x", "router-1");

        let input = EsaaInput {
            event_type: "event_x".into(),
            payload: vec![1, 2, 3],
            metadata: std::collections::HashMap::new(),
        };
        let outputs = coord.route(input);
        assert_eq!(outputs.len(), 1);
        assert!(outputs[0].success);
        // Router returns event_type bytes as routing info
        assert_eq!(outputs[0].result, b"event_x".to_vec());
    }

    fn make_input() -> EsaaInput {
        EsaaInput {
            event_type: "test".into(),
            payload: vec![42],
            metadata: std::collections::HashMap::new(),
        }
    }

    #[test]
    fn test_esaa_planner_processes_input() {
        let s = Planner::new("planner-1");
        let out = s.process(&make_input());
        assert!(out.success);
    }

    #[test]
    fn test_esaa_executor_runs() {
        let s = Executor::new("executor-1");
        let out = s.process(&make_input());
        assert!(out.success);
    }

    #[test]
    fn test_esaa_validator_validates() {
        let s = Validator::new("validator-1");
        assert!(s.health_check());
        // Non-empty payload → success
        let out = s.process(&make_input());
        assert!(out.success);
        assert_eq!(out.result, vec![42]);
        // Empty payload → failure
        let empty = EsaaInput {
            event_type: "t".into(),
            payload: vec![],
            metadata: std::collections::HashMap::new(),
        };
        let out_empty = s.process(&empty);
        assert!(!out_empty.success);
    }

    #[test]
    fn test_esaa_monitor_health_check() {
        let s = Monitor::new("monitor-1");
        assert!(s.health_check());
        // Monitor always returns b"ok" heartbeat
        let out = s.process(&make_input());
        assert!(out.success);
        assert_eq!(out.result, b"ok".to_vec());
    }

    #[test]
    fn test_esaa_coordinator_route_dispatches() {
        let mut coord = EsaaCoordinator::new();
        coord.register(Box::new(Router::new("router-1"))).unwrap();
        coord.register(Box::new(Planner::new("planner-1"))).unwrap();

        let input = EsaaInput {
            event_type: "unknown_event".into(),
            payload: vec![10],
            metadata: std::collections::HashMap::new(),
        };
        // No routing entry → dispatches to ALL
        let outputs = coord.route(input);
        assert_eq!(outputs.len(), 2);
    }

    #[test]
    fn test_esaa_health_report_all_healthy() {
        let mut coord = EsaaCoordinator::new();
        coord.register(Box::new(Router::new("r"))).unwrap();
        coord.register(Box::new(Planner::new("p"))).unwrap();
        let report = coord.health_report();
        assert!(report.values().all(|&v| v));
    }

    #[test]
    fn test_esaa_scheduler_schedule() {
        let s = Scheduler::new("scheduler-1");
        let out = s.process(&make_input());
        assert!(out.success);
        assert_eq!(out.latency_us, 1);
    }

    #[test]
    fn test_esaa_notifier_notify() {
        let s = Notifier::new("notifier-1");
        let out = s.process(&make_input());
        assert!(out.success);
    }

    #[test]
    fn test_esaa_aggregator_aggregate() {
        let s = Aggregator::new("aggregator-1");
        let out = s.process(&make_input());
        assert!(out.success);
    }

    #[test]
    fn test_esaa_transformer_transform() {
        let s = Transformer::new("transformer-1");
        let out = s.process(&make_input());
        assert!(out.success);
        // payload=[42] → transformed=[43] (wrapping byte+1)
        assert_eq!(out.result, vec![43]);
    }

    #[test]
    fn test_esaa_filter_filter() {
        let s = Filter::new("filter-1");
        // Without "allowed" key → filtered out
        let out = s.process(&make_input());
        assert!(!out.success);
        assert_eq!(out.result, b"filtered".to_vec());
        // With "allowed" key → passes through
        let mut allowed_input = make_input();
        allowed_input
            .metadata
            .insert("allowed".to_string(), "yes".to_string());
        let out_allowed = s.process(&allowed_input);
        assert!(out_allowed.success);
        assert_eq!(out_allowed.result, vec![42]);
    }

    #[test]
    fn test_esaa_dispatcher_dispatch() {
        let s = Dispatcher::new("dispatcher-1");
        let out = s.process(&make_input());
        assert!(out.success);
    }

    #[test]
    fn test_esaa_collector_collect() {
        let s = Collector::new("collector-1");
        let out = s.process(&make_input());
        assert!(out.success);
    }

    #[test]
    fn test_esaa_analyzer_analyze() {
        let s = Analyzer::new("analyzer-1");
        let out = s.process(&make_input());
        assert!(out.success);
        // Result is metrics string "len=1,meta=0"
        let report = String::from_utf8(out.result).unwrap();
        assert!(
            report.starts_with("len="),
            "expected metrics string, got: {report}"
        );
    }

    #[test]
    fn test_esaa_reporter_report() {
        let s = Reporter::new("reporter-1");
        let out = s.process(&make_input());
        assert!(out.success);
    }

    #[test]
    fn test_esaa_auditor_audit() {
        let s = Auditor::new("auditor-1");
        let out = s.process(&make_input());
        assert!(out.success);
    }

    #[test]
    fn test_esaa_archiver_archive() {
        let s = Archiver::new("archiver-1");
        let out = s.process(&make_input());
        assert!(out.success);
    }

    #[test]
    fn test_esaa_indexer_index() {
        let s = Indexer::new("indexer-1");
        let out = s.process(&make_input());
        assert!(out.success);
    }

    #[test]
    fn test_esaa_searcher_search() {
        let s = Searcher::new("searcher-1");
        let out = s.process(&make_input());
        assert!(out.success);
    }

    #[test]
    fn test_esaa_learner_learn() {
        let s = Learner::new("learner-1");
        let out = s.process(&make_input());
        assert!(out.success);
    }

    #[test]
    fn test_esaa_predictor_predict() {
        let s = Predictor::new("predictor-1");
        let out = s.process(&make_input());
        assert!(out.success);
    }

    #[test]
    fn test_esaa_balancer_balance() {
        let s = Balancer::new("balancer-1");
        let out = s.process(&make_input());
        assert!(out.success);
    }

    // --- Concurrent cache access tests ---

    #[test]
    fn test_concurrent_cache_reads_and_writes() {
        let cache = Arc::new(QueryCache::<String>::new(100, None));

        // Pre-populate
        for i in 0..50 {
            cache.put(format!("key_{i}"), format!("val_{i}"));
        }

        let mut handles = Vec::new();

        // Spawn 4 writer threads
        for t in 0..4 {
            let cache_ref = Arc::clone(&cache);
            handles.push(thread::spawn(move || {
                for i in 0..100 {
                    cache_ref.put(format!("t{t}_key_{i}"), format!("t{t}_val_{i}"));
                }
            }));
        }

        // Spawn 4 reader threads
        for _ in 0..4 {
            let cache_ref = Arc::clone(&cache);
            handles.push(thread::spawn(move || {
                for i in 0..100 {
                    // Reads may return Some or None depending on timing
                    let _ = cache_ref.get(&format!("key_{i}"));
                    let _ = cache_ref.contains(&format!("key_{i}"));
                }
            }));
        }

        // All threads must complete without panic or deadlock
        for h in handles {
            h.join().expect("Thread panicked during concurrent access");
        }

        // Cache should still be functional after concurrent access
        cache.put("final_key".into(), "final_val".into());
        assert_eq!(cache.get("final_key"), Some("final_val".into()));

        let stats = cache.stats();
        assert!(
            stats.hits + stats.misses > 0,
            "Stats should have recorded operations"
        );
    }

    #[test]
    fn test_concurrent_cache_invalidation() {
        let cache = Arc::new(QueryCache::<u64>::new(50, None));

        // Pre-populate
        for i in 0..50 {
            cache.put(format!("item_{i}"), i);
        }

        let mut handles = Vec::new();

        // Writer thread
        let cache_w = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 50..100 {
                cache_w.put(format!("item_{i}"), i);
            }
        }));

        // Invalidator thread
        let cache_inv = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..50 {
                cache_inv.invalidate(&format!("item_{i}"));
            }
        }));

        // Pattern invalidator thread
        let cache_pat = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            let _ = cache_pat.invalidate_pattern("item_1");
        }));

        // Reader thread
        let cache_r = Arc::clone(&cache);
        handles.push(thread::spawn(move || {
            for i in 0..100 {
                let _ = cache_r.get(&format!("item_{i}"));
            }
        }));

        for h in handles {
            h.join()
                .expect("Thread panicked during concurrent invalidation");
        }

        // Cache still functional
        cache.put("post_test".into(), 999);
        assert_eq!(cache.get("post_test"), Some(999));
    }
}
