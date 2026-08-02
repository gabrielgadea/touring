//! FileKnowledgeDB — SQLite-backed knowledge graph for files.
//!
//! Stores file metadata, symbols, imports, relations, access patterns,
//! bash outcomes, and edit history. Uses WAL mode for concurrent access
//! from multiple hook processes.

use crate::shared::moka_policies::{build_knowledge_extended_cache, sample_stats, MokaCacheStats};
use crate::TouringError;
use moka::sync::Cache;
use rusqlite::{params, Connection, OptionalExtension};
use sha2::{Digest, Sha256};
use std::path::Path;
use std::sync::{Arc, Mutex};
use touring_analysis::e2e::schema_guard;
use touring_foundation::mvkl::{Artifact, KnowledgeLayer, LayerLevel, QueryResult, SemanticGraph};

/// 5-dimensional cognitive score vector.
type CognitiveScores = (f64, f64, f64, f64, f64);

/// Compute SHA-256 hex digest of a string.
fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    let result = hasher.finalize();
    result.iter().fold(String::with_capacity(64), |mut acc, b| {
        use std::fmt::Write;
        let _ = write!(acc, "{b:02x}");
        acc
    })
}

mod models;
pub use models::*;
mod analytics;
mod bash;
mod core;
mod edits;
mod gotchas;
mod metadata;
mod query;
mod schema;

/// SQLite-backed file knowledge database.
///
/// The optional `extended_cache` absorbs repeated `query_extended` lookups —
/// each miss runs a 6-way `LEFT JOIN` that dominates pre_read / pre_edit /
/// pre_write hook latency on large workspaces. The cache is a
/// `moka::sync::Cache<String, Arc<FileKnowledgeEnriched>>` governed by
/// `shared::moka_policies` (W-TinyLFU admission + LRU eviction + TTL/TTI).
#[derive(Debug)]
pub struct FileKnowledgeDB {
    conn: Connection,
    /// Query-result cache for `query_extended`. Keys are normalized
    /// `file_path` strings; values are reference-counted so clones stay O(1).
    extended_cache: Cache<String, Arc<FileKnowledgeEnriched>>,
}

// Schema version is defined in touring-core::migration (single source of truth).
// Bump it there when adding new migration blocks to `migrate_schema`.
use touring_foundation::migration::SCHEMA_VERSION;

impl FileKnowledgeDB {
    /// Open or create the knowledge database at the given path.
    pub fn new(db_path: &Path) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open(db_path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA temp_store = MEMORY;
             PRAGMA cache_size = -2000;
             PRAGMA busy_timeout = 5000;
             PRAGMA mmap_size = 268435456;",
        )?;
        let db = Self {
            conn,
            extended_cache: build_knowledge_extended_cache(),
        };
        let version: u32 = db
            .conn
            .query_row("PRAGMA user_version", [], |row| row.get(0))?;
        if version < SCHEMA_VERSION {
            db.ensure_schema()?;
            db.migrate_schema()?;
            db.conn
                .execute_batch(&format!("PRAGMA user_version = {SCHEMA_VERSION};"))?;
        }
        let _ = db.migrate_canonicalize_paths();
        Ok(db)
    }
    /// Test-only constructor — wraps an existing Connection directly.
    ///
    /// The caller must ensure `ensure_schema()` has been run (or pass a
    /// connection that already has the schema). Use `new()` for production paths.
    pub fn from_conn(conn: Connection) -> Self {
        Self {
            conn,
            extended_cache: build_knowledge_extended_cache(),
        }
    }
    /// Invalidate a single file's entry in the extended-query cache.
    ///
    /// Callers that mutate any of the 6 tables feeding `query_extended`
    /// should invoke this to prevent stale reads. TTL provides a safety net
    /// even when call-sites forget.
    pub fn invalidate_extended_cache(&self, file_path: &str) {
        self.extended_cache.invalidate(file_path);
    }
    /// Drop every entry from the extended-query cache (maintenance API).
    pub fn invalidate_extended_cache_all(&self) {
        self.extended_cache.invalidate_all();
    }
    /// Sample cache metrics for observability.
    pub fn extended_cache_stats(&self) -> MokaCacheStats {
        sample_stats(&self.extended_cache)
    }
    /// Get a reference to the underlying SQLite connection.
    ///
    /// Exposed for the KnowledgeSource adapter in touring-server, which needs
    /// to run aggregate queries not covered by the individual accessor methods.
    pub fn conn_ref(&self) -> &Connection {
        &self.conn
    }
}

/// Batched signals for the pre-read hook (P2.2 optimization).
#[derive(Debug, Clone, Default)]
pub struct PreReadSignals {
    /// Notes/gotchas from file_knowledge.
    pub notes: Option<String>,
    /// Latest failure: (command, error_pattern).
    pub latest_failure: Option<(String, Option<String>)>,
    /// Names of dependent files (top 5).
    pub dependent_names: Vec<String>,
    /// Total dependent count.
    pub dependent_count: usize,
    /// Structured gotchas matching this file.
    pub gotchas: Vec<Gotcha>,
}

/// Thread-safe wrapper around `FileKnowledgeDB`.
///
/// `rusqlite::Connection` is `Send` but NOT `Sync`, so `FileKnowledgeDB`
/// cannot be shared across threads directly. This wrapper uses a `Mutex`
/// to serialize access, enabling multi-threaded hook architectures.
///
/// # Performance
/// For hook processes (short-lived, single-threaded), prefer `FileKnowledgeDB`
/// directly. Use `ThreadSafeKnowledgeDB` when the DB needs to be shared
/// across threads (e.g., async hook pipelines, connection pooling).
pub struct ThreadSafeKnowledgeDB {
    inner: std::sync::Mutex<FileKnowledgeDB>,
}

/// Error from [`ThreadSafeKnowledgeDB`] thread-safe accessors
/// (F-8 / RBP-03: typed in place of `String`).
#[derive(Debug, thiserror::Error)]
pub enum KnowledgeDbError {
    /// The inner mutex was poisoned by a panic in another thread.
    #[error("Knowledge DB mutex poisoned: {0}")]
    MutexPoisoned(String),
    /// The underlying `FileKnowledgeDB` operation failed.
    #[error("{0}")]
    Db(String),
}

impl ThreadSafeKnowledgeDB {
    /// Open or create a thread-safe knowledge database.
    pub fn new(db_path: &Path) -> Result<Self, rusqlite::Error> {
        let db = FileKnowledgeDB::new(db_path)?;
        Ok(Self {
            inner: std::sync::Mutex::new(db),
        })
    }
    /// Execute a closure with exclusive access to the underlying DB.
    pub fn with<F, R>(&self, f: F) -> Result<R, KnowledgeDbError>
    where
        F: FnOnce(&FileKnowledgeDB) -> R,
    {
        let guard = self
            .inner
            .lock()
            .map_err(|e| KnowledgeDbError::MutexPoisoned(e.to_string()))?;
        Ok(f(&guard))
    }
    /// Execute a mutable closure (for operations that need `&mut`).
    pub fn with_mut<F, R>(&self, f: F) -> Result<R, KnowledgeDbError>
    where
        F: FnOnce(&mut FileKnowledgeDB) -> R,
    {
        let mut guard = self
            .inner
            .lock()
            .map_err(|e| KnowledgeDbError::MutexPoisoned(e.to_string()))?;
        Ok(f(&mut guard))
    }
    /// Look up a file's knowledge (thread-safe).
    pub fn lookup(&self, file_path: &str) -> Result<Option<FileKnowledge>, KnowledgeDbError> {
        self.with(|db| {
            db.lookup(file_path)
                .map_err(|e| KnowledgeDbError::Db(e.to_string()))
        })?
    }
    /// Get knowledge stats (thread-safe).
    pub fn stats(&self) -> Result<KnowledgeStats, KnowledgeDbError> {
        self.with(|db| db.stats().map_err(|e| KnowledgeDbError::Db(e.to_string())))?
    }
    /// Record an access (thread-safe).
    pub fn record_access(
        &self,
        file_path: &str,
        session_id: &str,
    ) -> Result<(), KnowledgeDbError> {
        self.with(|db| {
            db.record_access(file_path, session_id)
                .map_err(|e| KnowledgeDbError::Db(e.to_string()))
        })?
    }
}

// SAFETY: ThreadSafeKnowledgeDB wraps FileKnowledgeDB (which contains a
// rusqlite::Connection that is !Sync) in a std::sync::Mutex. The Mutex
// serializes all access, ensuring only one thread can interact with the
// Connection at a time, making it safe to share references across threads.
unsafe impl Sync for ThreadSafeKnowledgeDB {}

impl std::fmt::Debug for ThreadSafeKnowledgeDB {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("ThreadSafeKnowledgeDB")
            .field("locked", &self.inner.try_lock().is_err())
            .finish()
    }
}

/// Summary statistics for the knowledge DB.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct KnowledgeStats {
    /// Number of tracked files.
    pub file_count: i64,
    /// Number of file-to-file relations.
    pub relation_count: i64,
    /// Total recorded file accesses.
    pub access_count: i64,
    /// Number of recorded bash outcomes.
    pub bash_count: i64,
    /// Number of recorded edit events.
    pub edit_count: i64,
    /// Number of stored gotchas.
    pub gotcha_count: i64,
    /// Task decomposition metrics count (hooks_task_lifecycle integration).
    #[serde(default)]
    pub task_metrics_count: i64,
}

/// Task lifecycle metrics for decomposed tasks.
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct TaskMetrics {
    /// Identifier of the decomposed task.
    pub task_id: String,
    /// Total time to complete the task, in seconds.
    pub completion_time_secs: f64,
    /// Number of subtasks the task was decomposed into.
    pub subtask_count: i64,
    /// Fraction of subtasks that succeeded (0.0–1.0).
    pub success_rate: f64,
    /// Aggregate quality score for the completed task.
    pub quality_score: f64,
}

impl FileKnowledgeDB {
    /// Record task decomposition metrics to task_decompositions table.
    ///
    /// Uses INSERT OR REPLACE with the metrics JSON in the `metrics` column.
    /// The `task_type` and `description` are filled from the TaskMetrics fields.
    pub fn record_task_metrics(&self, metrics: &TaskMetrics) -> Result<(), rusqlite::Error> {
        let now = chrono::Local::now().to_rfc3339();
        let metrics_json = serde_json::json!(
            { "completion_time_secs" : metrics.completion_time_secs, "subtask_count" :
            metrics.subtask_count, "success_rate" : metrics.success_rate, "quality_score"
            : metrics.quality_score, }
        )
        .to_string();
        self.conn.execute(
            "INSERT OR REPLACE INTO task_decompositions
             (task_id, task_type, description, cila_level, created_at, updated_at, status, metrics)
             VALUES (?1, 'metrics', 'task-lifecycle-metrics', 3, ?2, ?2, 'completed', ?3)",
            rusqlite::params![metrics.task_id, now, metrics_json],
        )?;
        Ok(())
    }
    /// Return consumer files for a public symbol — queries wiring_map table.
    ///
    /// Returns `Vec<(consumer_file, import_line_hint)>` pairs.
    pub fn consumers_of_symbol(
        &self,
        module_file: &str,
        symbol_name: &str,
    ) -> Result<Vec<(String, Option<u32>)>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT consumer_file, import_line FROM wiring_map
             WHERE module_file = ?1 AND symbol_name = ?2
               AND consumer_file IS NOT NULL",
        )?;
        let rows = stmt.query_map(params![module_file, symbol_name], |row| {
            let file: String = row.get(0)?;
            let line: Option<i64> = row.get(1)?;
            Ok((file, line.map(|l| l as u32)))
        })?;
        let mut result = Vec::new();
        for r in rows {
            result.push(r?);
        }
        Ok(result)
    }
}

#[cfg(test)]
mod tests;

/// Bridge connecting the MVKL knowledge layer to touring-hooks.
///
/// This struct wraps the semantic graph (L2 layer) and provides a
/// [`KnowledgeLayer`] implementation so the semantic graph can be queried
/// and indexed through the standard MVKL interface.
pub struct MvklKnowledgeBridge {
    /// The underlying semantic graph (L2 layer), shared via `Arc<Mutex>` for interior mutability.
    graph: Arc<Mutex<SemanticGraph>>,
}

impl MvklKnowledgeBridge {
    /// Create a new empty bridge.
    pub fn new() -> Self {
        Self {
            graph: Arc::new(Mutex::new(SemanticGraph::new())),
        }
    }
    /// Add a semantic relationship between two definitions.
    pub fn add_relation(
        &self,
        from: String,
        to: String,
        kind: touring_foundation::mvkl::layer2_semantic_graph::SemanticRelationKind,
    ) {
        self.graph.lock().unwrap().add_edge(
            touring_foundation::mvkl::layer2_semantic_graph::SemanticRelation {
                from,
                to,
                kind,
                metadata: None,
            },
        );
    }
    /// Get all relations for a given definition id.
    pub fn get_relations(
        &self,
        id: &str,
    ) -> Vec<touring_foundation::mvkl::layer2_semantic_graph::SemanticRelation> {
        self.graph.lock().unwrap().get_relations(id)
    }
}

impl Default for MvklKnowledgeBridge {
    fn default() -> Self {
        Self::new()
    }
}

impl Clone for MvklKnowledgeBridge {
    fn clone(&self) -> Self {
        Self {
            graph: self.graph.clone(),
        }
    }
}

impl KnowledgeLayer for MvklKnowledgeBridge {
    fn query(&self, query: &str, layer: LayerLevel) -> QueryResult {
        if layer == LayerLevel::L2 {
            let rels = self.graph.lock().unwrap().get_relations(query);
            let items: Vec<serde_json::Value> = rels
                .iter()
                .map(|r| {
                    serde_json::json!(
                        { "from" : r.from, "to" : r.to, "kind" : format!("{:?}", r.kind),
                        }
                    )
                })
                .collect();
            let score = if items.is_empty() { 0.0 } else { 1.0 };
            return QueryResult {
                layer_results: vec![touring_foundation::mvkl::LayerResult {
                    level: LayerLevel::L2,
                    items,
                    score,
                }],
                score,
            };
        }
        QueryResult {
            layer_results: Vec::new(),
            score: 0.0,
        }
    }
    fn index(&mut self, artifact: &Artifact) -> Result<(), String> {
        self.graph.lock().unwrap().add_node(
            touring_foundation::mvkl::layer2_semantic_graph::SemanticNode {
                id: artifact.path.clone(),
                file_path: artifact.path.clone(),
                line: 0,
                kind: artifact
                    .language
                    .clone()
                    .unwrap_or_else(|| "file".to_string()),
            },
        );
        Ok(())
    }
}
