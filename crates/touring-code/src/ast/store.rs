//! SymbolStore — SQLite-backed persistence for the AST symbol graph.
//!
//! Persists `SymbolLocation` and `DependencyEdge` data from the in-memory
//! `SymbolIndex` to disk so that indexed symbols survive process restarts.
//! Uses WAL mode for concurrent access from multiple hook processes.

use std::collections::HashSet;
use std::path::Path;

use rusqlite::{Connection, params};

use crate::ast::graph::{DependencyEdge, SymbolIndex, SymbolLocation};

/// Upsert (insert-or-update) for a single `symbols` row. Shared by
/// [`SymbolStore::upsert_symbol`] and every batched prepared-statement path so
/// the schema-coupled column list + conflict clause live in exactly one place —
/// changing the schema now touches one string instead of four.
const UPSERT_SYMBOL_SQL: &str =
    "INSERT INTO symbols (name, file_path, line, column_offset, is_definition, kind, updated_at)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'))
     ON CONFLICT(name, file_path, line) DO UPDATE SET
        column_offset = ?4,
        is_definition = ?5,
        kind = COALESCE(?6, kind),
        updated_at = datetime('now')";

/// Observer notified when symbols change via [`SymbolStore::apply_change_set`].
///
/// Implementations must be `Send + Sync` to be usable across threads.
pub trait SymbolChangeObserver: Send + Sync {
    /// Called after a successful [`SymbolStore::apply_change_set`] commit.
    ///
    /// `changes` is a reference to the applied change set.
    fn on_symbol_change(&self, changes: &SymbolChangeSet);
}

/// SQLite-backed persistence layer for the symbol graph.
///
/// The inner `Connection` is wrapped in a `Mutex` so that `SymbolStore` is
/// both `Send` and `Sync` — rusqlite `Connection` is `Send` but not `Sync`.
/// All public methods acquire the lock for the duration of the operation.
pub struct SymbolStore {
    conn: std::sync::Mutex<Connection>,
    observers: Vec<std::sync::Arc<dyn SymbolChangeObserver>>,
    /// H3: broadcast sender for async streaming of change sets.
    /// Enabled only with the `async-pipeline` feature.
    #[cfg(feature = "async-pipeline")]
    stream_sender: std::sync::Arc<tokio::sync::broadcast::Sender<SymbolChangeSet>>,
}

impl std::fmt::Debug for SymbolStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SymbolStore")
            .field("observers_count", &self.observers.len())
            .finish_non_exhaustive()
    }
}

/// Summary statistics for the symbol store.
#[derive(Debug, Clone, serde::Serialize)]
pub struct StoreStats {
    /// Number of symbols currently recorded in the store.
    pub symbol_count: usize,
    /// Number of distinct files tracked by the store.
    pub file_count: usize,
    /// Number of symbol-to-symbol dependency edges recorded.
    pub dependency_count: usize,
}

impl SymbolStore {
    /// Open or create the symbol store at the given path.
    ///
    /// Enables WAL mode and creates tables if they don't exist.
    pub fn new(db_path: &Path) -> Result<Self, rusqlite::Error> {
        let conn = Connection::open(db_path)?;

        // WAL mode for concurrent hook processes
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA temp_store = MEMORY;
             PRAGMA cache_size = -2000;
             PRAGMA busy_timeout = 5000;
             PRAGMA mmap_size = 268435456;",
        )?;

        let store = Self {
            conn: std::sync::Mutex::new(conn),
            observers: Vec::new(),
            #[cfg(feature = "async-pipeline")]
            stream_sender: std::sync::Arc::new(tokio::sync::broadcast::channel(256).0),
        };
        store.ensure_schema()?;
        store.ensure_access_columns()?;
        Ok(store)
    }

    /// Register an observer to be notified on every successful `apply_change_set`.
    ///
    /// Observers are called synchronously after the COMMIT, in registration order.
    /// `subscribe` takes `&mut self` — call it during setup before sharing the store.
    pub fn subscribe(&mut self, observer: std::sync::Arc<dyn SymbolChangeObserver>) {
        self.observers.push(observer);
    }

    /// H3: Subscribe to an async stream of symbol change sets.
    ///
    /// Returns a `tokio::sync::broadcast::Receiver` that fires after each
    /// successful `apply_change_set` commit. Multiple receivers may exist
    /// concurrently (broadcast semantics — each receiver gets its own copy).
    ///
    /// Only available with the `async-pipeline` feature.
    #[cfg(feature = "async-pipeline")]
    pub fn subscribe_stream(&self) -> tokio::sync::broadcast::Receiver<SymbolChangeSet> {
        self.stream_sender.subscribe()
    }

    /// Create all tables and indexes if they don't exist.
    fn ensure_schema(&self) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().expect("symbol store conn lock");
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS symbols (
                id INTEGER PRIMARY KEY,
                name TEXT NOT NULL,
                file_path TEXT NOT NULL,
                line INTEGER NOT NULL,
                column_offset INTEGER NOT NULL DEFAULT 0,
                is_definition INTEGER NOT NULL DEFAULT 1,
                updated_at TEXT DEFAULT (datetime('now')),
                UNIQUE(name, file_path, line)
            );

            CREATE INDEX IF NOT EXISTS idx_symbols_name ON symbols(name);
            CREATE INDEX IF NOT EXISTS idx_symbols_file ON symbols(file_path);
            -- Composite index for find_symbols_in_file() which filters by file_path then sorts by name
            CREATE INDEX IF NOT EXISTS idx_symbols_file_name ON symbols(file_path, name);

            -- INS-A2: access tracking columns (idempotent via ALTER TABLE IF NOT EXISTS alternative)
            -- Added via ensure_access_columns() called after table creation.

            CREATE TABLE IF NOT EXISTS dependencies (
                id INTEGER PRIMARY KEY,
                from_file TEXT NOT NULL,
                to_file TEXT NOT NULL,
                symbols_json TEXT NOT NULL DEFAULT '[]',
                updated_at TEXT DEFAULT (datetime('now')),
                UNIQUE(from_file, to_file)
            );

            CREATE INDEX IF NOT EXISTS idx_deps_from ON dependencies(from_file);
            CREATE INDEX IF NOT EXISTS idx_deps_to ON dependencies(to_file);",
        )?;

        // Add co_edit_weight column if it doesn't exist (idempotent migration)
        conn.execute_batch("ALTER TABLE dependencies ADD COLUMN co_edit_weight REAL DEFAULT 0.0;")
            .ok(); // Silently ignore "duplicate column" error

        // A2 (2026-06-21): canonical symbol `kind` column (function/class/const/…).
        // Idempotent ADD COLUMN — `.ok()` swallows the "duplicate column name"
        // error on every existing project DB, so the migration is order- and
        // version-independent (no PRAGMA gate needed). NULL for legacy rows;
        // `row_to_symbol` reads it defensively and `index find` surfaces it.
        conn.execute_batch("ALTER TABLE symbols ADD COLUMN kind TEXT;")
            .ok();

        conn.execute_batch(
            "

            -- Normalised symbol table: replaces `symbols_json` for indexed lookups.
            -- `ON DELETE CASCADE` ensures rows are cleaned up when the parent dep is removed.
            CREATE TABLE IF NOT EXISTS dependency_symbols (
                dep_id INTEGER NOT NULL REFERENCES dependencies(id) ON DELETE CASCADE,
                symbol_name TEXT NOT NULL,
                UNIQUE(dep_id, symbol_name)
            );

            CREATE INDEX IF NOT EXISTS idx_dep_symbols_dep ON dependency_symbols(dep_id);
            CREATE INDEX IF NOT EXISTS idx_dep_symbols_name ON dependency_symbols(symbol_name);",
        )?;

        // Backfill dependency_symbols from symbols_json for existing rows.
        // INSERT OR IGNORE is idempotent: safe to run on every startup.
        conn.execute_batch(
            "INSERT OR IGNORE INTO dependency_symbols (dep_id, symbol_name)
             SELECT d.id, j.value
             FROM dependencies d, json_each(d.symbols_json) j
             WHERE d.symbols_json != '[]';",
        )?;

        // FTS5 setup: create virtual table + sync triggers if not present.
        let fts_exists: bool = conn.query_row(
            "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='symbols_fts'",
            [],
            |row| row.get::<_, i64>(0),
        )? > 0;

        if !fts_exists {
            conn.execute_batch(
                "CREATE VIRTUAL TABLE symbols_fts USING fts5(
                    name, file_path,
                    content='symbols',
                    content_rowid='id'
                );
                -- Backfill existing rows
                INSERT INTO symbols_fts(rowid, name, file_path)
                    SELECT id, name, file_path FROM symbols;",
            )?;
        }

        // Idempotent triggers: recreate if absent (no CREATE TRIGGER IF NOT EXISTS in older SQLite)
        conn.execute_batch(
            "CREATE TRIGGER IF NOT EXISTS symbol_fts_ai
             AFTER INSERT ON symbols BEGIN
                 INSERT INTO symbols_fts(rowid, name, file_path)
                 VALUES (new.id, new.name, new.file_path);
             END;

             CREATE TRIGGER IF NOT EXISTS symbol_fts_ad
             AFTER DELETE ON symbols BEGIN
                 INSERT INTO symbols_fts(symbols_fts, rowid, name, file_path)
                 VALUES ('delete', old.id, old.name, old.file_path);
             END;

             CREATE TRIGGER IF NOT EXISTS symbol_fts_au
             AFTER UPDATE ON symbols BEGIN
                 INSERT INTO symbols_fts(symbols_fts, rowid, name, file_path)
                 VALUES ('delete', old.id, old.name, old.file_path);
                 INSERT INTO symbols_fts(rowid, name, file_path)
                 VALUES (new.id, new.name, new.file_path);
             END;",
        )?;

        Ok(())
    }

    // ── INS-A2: access tracking ──────────────────────────────────────────

    /// Add `access_count` and `last_accessed` columns to `symbols` if absent.
    ///
    /// SQLite does not support `ALTER TABLE ... ADD COLUMN IF NOT EXISTS` before
    /// version 3.37. We use the PRAGMA table_info approach for compatibility.
    fn ensure_access_columns(&self) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().expect("symbol store conn lock");
        let has_access_count: bool = conn
            .prepare("PRAGMA table_info(symbols)")?
            .query_map([], |row| row.get::<_, String>(1))?
            .any(|r| r.map(|n| n == "access_count").unwrap_or(false));

        if !has_access_count {
            conn.execute_batch(
                "ALTER TABLE symbols ADD COLUMN access_count INTEGER NOT NULL DEFAULT 0;
                 ALTER TABLE symbols ADD COLUMN last_accessed TEXT;",
            )?;
            conn.execute_batch(
                "CREATE INDEX IF NOT EXISTS idx_symbols_access
                 ON symbols(access_count DESC);",
            )?;
        }
        Ok(())
    }

    /// Increment the access counter for a symbol and record access time.
    ///
    /// No-ops gracefully when the symbol row does not exist yet.
    pub fn record_symbol_access(&self, name: &str, file_path: &str) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().expect("symbol store conn lock");
        conn.execute(
            "UPDATE symbols
             SET access_count = access_count + 1,
                 last_accessed = datetime('now')
             WHERE name = ?1 AND file_path = ?2",
            params![name, file_path],
        )?;
        Ok(())
    }

    /// Return the top `limit` hot symbols by access count descending.
    pub fn get_hot_symbols(&self, limit: usize) -> Result<Vec<SymbolLocation>, rusqlite::Error> {
        let conn = self.conn.lock().expect("symbol store conn lock");
        let mut stmt = conn.prepare(
            "SELECT name, file_path, line, column_offset, is_definition, kind
             FROM symbols
             WHERE is_definition = 1
             ORDER BY access_count DESC
             LIMIT ?1",
        )?;
        let rows = stmt.query_map(params![limit as i64], row_to_symbol)?;
        rows.collect()
    }

    // ── Symbol CRUD ──────────────────────────────────────────────────────

    /// Insert or update a single symbol location.
    pub fn upsert_symbol(&self, sym: &SymbolLocation) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().expect("symbol store conn lock");
        conn.execute(
            UPSERT_SYMBOL_SQL,
            params![
                sym.symbol_name,
                sym.file_path,
                sym.line as i64,
                sym.column as i64,
                sym.is_definition as i64,
                sym.kind,
            ],
        )?;
        Ok(())
    }

    /// Insert or update a single dependency edge.
    ///
    /// Also maintains the normalised `dependency_symbols` table so that
    /// `WHERE symbol_name = ?` queries are direct index lookups instead of
    /// O(n) JSON deserialisation in Rust.
    pub fn upsert_dependency(&self, edge: &DependencyEdge) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().expect("symbol store conn lock");
        let symbols_json =
            serde_json::to_string(&edge.symbols).unwrap_or_else(|_| "[]".to_string());
        conn.execute(
            "INSERT INTO dependencies (from_file, to_file, symbols_json, co_edit_weight, updated_at)
             VALUES (?1, ?2, ?3, ?4, datetime('now'))
             ON CONFLICT(from_file, to_file) DO UPDATE SET
                symbols_json = ?3,
                co_edit_weight = ?4,
                updated_at = datetime('now')",
            params![edge.from, edge.to, symbols_json, edge.co_edit_weight],
        )?;

        // Maintain the normalised dependency_symbols table.
        let dep_id: i64 = conn.query_row(
            "SELECT id FROM dependencies WHERE from_file = ?1 AND to_file = ?2",
            params![edge.from, edge.to],
            |row| row.get(0),
        )?;
        conn.execute(
            "DELETE FROM dependency_symbols WHERE dep_id = ?1",
            params![dep_id],
        )?;
        if !edge.symbols.is_empty() {
            let mut sym_stmt = conn.prepare_cached(
                "INSERT OR IGNORE INTO dependency_symbols (dep_id, symbol_name) VALUES (?1, ?2)",
            )?;
            for sym in &edge.symbols {
                sym_stmt.execute(params![dep_id, sym])?;
            }
        }

        Ok(())
    }

    /// Find all dependency edges that import a specific symbol by name.
    ///
    /// Uses the normalised `dependency_symbols` index — O(log n) lookup
    /// instead of O(n) JSON deserialisation.
    pub fn find_deps_by_symbol(
        &self,
        symbol_name: &str,
    ) -> Result<Vec<DependencyEdge>, rusqlite::Error> {
        let conn = self.conn.lock().expect("symbol store conn lock");
        let mut stmt = conn.prepare(
            "SELECT d.from_file, d.to_file, d.symbols_json
             FROM dependency_symbols ds
             JOIN dependencies d ON ds.dep_id = d.id
             WHERE ds.symbol_name = ?1",
        )?;
        let rows = stmt.query_map(params![symbol_name], row_to_dep)?;
        rows.collect()
    }

    /// Find all locations of a symbol by exact name.
    pub fn find_symbol(&self, name: &str) -> Result<Vec<SymbolLocation>, rusqlite::Error> {
        let conn = self.conn.lock().expect("symbol store conn lock");
        let mut stmt = conn.prepare(
            "SELECT name, file_path, line, column_offset, is_definition, kind
             FROM symbols WHERE name = ?1 AND is_definition = 1",
        )?;
        let rows = stmt.query_map(params![name], row_to_symbol)?;
        rows.collect()
    }

    /// Find all reference (call-site) locations of a symbol by exact name.
    ///
    /// Returns only `is_definition = 0` rows (`@reference.call`, `kind="call"`) —
    /// the complement of [`Self::find_symbol`], which returns only definitions.
    /// Empty unless the index was rebuilt with reference extraction enabled
    /// (the default; disable per-project with `TOURING_INDEX_REFERENCES=0`).
    pub fn find_references(&self, name: &str) -> Result<Vec<SymbolLocation>, rusqlite::Error> {
        let conn = self.conn.lock().expect("symbol store conn lock");
        let mut stmt = conn.prepare(
            "SELECT name, file_path, line, column_offset, is_definition, kind
             FROM symbols WHERE name = ?1 AND is_definition = 0
             ORDER BY file_path, line",
        )?;
        let rows = stmt.query_map(params![name], row_to_symbol)?;
        rows.collect()
    }

    /// All locations of a symbol — definitions AND references — by exact name.
    ///
    /// Refactoring (rename / find-references) must touch every occurrence, so it
    /// needs both row classes; ordinary symbol lookups and VGP should use the
    /// definition-only [`Self::find_symbol`]. Definitions sort first.
    pub fn find_all_locations(&self, name: &str) -> Result<Vec<SymbolLocation>, rusqlite::Error> {
        let conn = self.conn.lock().expect("symbol store conn lock");
        let mut stmt = conn.prepare(
            "SELECT name, file_path, line, column_offset, is_definition, kind
             FROM symbols WHERE name = ?1
             ORDER BY is_definition DESC, file_path, line",
        )?;
        let rows = stmt.query_map(params![name], row_to_symbol)?;
        rows.collect()
    }

    /// Find all symbols defined in a specific file.
    pub fn find_symbols_in_file(
        &self,
        file_path: &str,
    ) -> Result<Vec<SymbolLocation>, rusqlite::Error> {
        let conn = self.conn.lock().expect("symbol store conn lock");
        let mut stmt = conn.prepare(
            "SELECT name, file_path, line, column_offset, is_definition, kind
             FROM symbols WHERE file_path = ?1 AND is_definition = 1 ORDER BY line",
        )?;
        let rows = stmt.query_map(params![file_path], row_to_symbol)?;
        rows.collect()
    }

    /// Search symbols whose name starts with the given prefix.
    ///
    /// Uses the FTS5 `symbols_fts` virtual table for indexed prefix matching
    /// (`prefix*`) instead of a full-table `LIKE 'prefix%'` scan.
    pub fn search_symbols(&self, prefix: &str) -> Result<Vec<SymbolLocation>, rusqlite::Error> {
        if prefix.is_empty() {
            return Ok(Vec::new());
        }
        // FTS5 prefix query: "term*" matches all names starting with "term".
        // Escape internal quotes to avoid injection through FTS query syntax.
        let fts_query = format!("\"{}\"*", prefix.replace('"', "\"\""));
        let conn = self.conn.lock().expect("symbol store conn lock");
        let mut stmt = conn.prepare(
            "SELECT s.name, s.file_path, s.line, s.column_offset, s.is_definition, s.kind
             FROM symbols_fts
             JOIN symbols s ON symbols_fts.rowid = s.id
             WHERE symbols_fts MATCH ?1
             AND s.is_definition = 1
             ORDER BY s.name
             LIMIT 100",
        )?;
        let rows = stmt.query_map(params![fts_query], row_to_symbol)?;
        rows.collect()
    }

    /// Get all dependency edges originating from a file.
    pub fn get_dependencies(
        &self,
        file_path: &str,
    ) -> Result<Vec<DependencyEdge>, rusqlite::Error> {
        let conn = self.conn.lock().expect("symbol store conn lock");
        let mut stmt = conn.prepare(
            "SELECT from_file, to_file, symbols_json
             FROM dependencies WHERE from_file = ?1",
        )?;
        let rows = stmt.query_map(params![file_path], row_to_dep)?;
        rows.collect()
    }

    /// Get all files that depend on (import from) the given file.
    pub fn get_reverse_deps(&self, file_path: &str) -> Result<Vec<String>, rusqlite::Error> {
        let conn = self.conn.lock().expect("symbol store conn lock");
        let mut stmt =
            conn.prepare("SELECT DISTINCT from_file FROM dependencies WHERE to_file = ?1")?;
        let rows = stmt.query_map(params![file_path], |row| row.get(0))?;
        rows.collect()
    }

    /// Remove all symbols and dependencies associated with a file.
    ///
    /// This cascades: removes symbols defined in the file AND dependency
    /// edges originating from the file.
    pub fn remove_file(&self, file_path: &str) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().expect("symbol store conn lock");
        conn.execute(
            "DELETE FROM symbols WHERE file_path = ?1",
            params![file_path],
        )?;
        conn.execute(
            "DELETE FROM dependencies WHERE from_file = ?1",
            params![file_path],
        )?;
        Ok(())
    }

    /// Get summary statistics about the store.
    pub fn stats(&self) -> Result<StoreStats, rusqlite::Error> {
        let conn = self.conn.lock().expect("symbol store conn lock");
        let symbol_count: i64 = conn.query_row("SELECT COUNT(*) FROM symbols", [], |r| r.get(0))?;
        let file_count: i64 =
            conn.query_row("SELECT COUNT(DISTINCT file_path) FROM symbols", [], |r| {
                r.get(0)
            })?;
        let dependency_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM dependencies", [], |r| r.get(0))?;
        Ok(StoreStats {
            symbol_count: symbol_count as usize,
            file_count: file_count as usize,
            dependency_count: dependency_count as usize,
        })
    }

    /// Return up to `limit` symbols starting at `offset`, ordered by id.
    ///
    /// Used by the Tantivy bulk-reindex pipeline to iterate the full symbol
    /// store in bounded-memory pages without loading all 297k+ rows at once.
    pub fn symbols_page(
        &self,
        limit: usize,
        offset: usize,
    ) -> Result<Vec<SymbolLocation>, rusqlite::Error> {
        let conn = self.conn.lock().expect("symbol store conn lock");
        let mut stmt = conn.prepare(
            "SELECT name, file_path, line, column_offset, is_definition, kind
             FROM symbols
             WHERE is_definition = 1
             ORDER BY id
             LIMIT ?1 OFFSET ?2",
        )?;
        let rows = stmt.query_map(params![limit as i64, offset as i64], row_to_symbol)?;
        rows.collect()
    }

    /// Get distinct file paths from the symbols table.
    ///
    /// Returns up to `limit` file paths, ordered by most recently updated.
    ///
    /// # Bug fix (2026-05-14)
    ///
    /// The previous implementation used
    /// `SELECT DISTINCT file_path FROM symbols ORDER BY MAX(updated_at) DESC`,
    /// which mixes a bare `SELECT DISTINCT` with an unaggregated `MAX()` in
    /// the `ORDER BY` clause. SQLite silently collapsed the result set to a
    /// single aggregate row (observed: 3 rows returned out of 2633 distinct
    /// file paths in a real workspace) instead of producing a proper "most
    /// recent" listing — making this method unusable for any sweep / GC
    /// workload. The first real consumer (the stale-file sweep in
    /// `cli_index_rebuild`) was reporting `stale_files_purged: 0` even when
    /// files had clearly been deleted. The query now uses a `GROUP BY` so
    /// `MAX(updated_at)` is a proper per-group aggregate.
    pub fn get_indexed_files(&self, limit: usize) -> Result<Vec<String>, rusqlite::Error> {
        let conn = self.conn.lock().expect("symbol store conn lock");
        let mut stmt = conn.prepare(
            "SELECT file_path FROM symbols GROUP BY file_path \
             ORDER BY MAX(updated_at) DESC LIMIT ?1",
        )?;
        let files = stmt
            .query_map([limit as i64], |r| r.get(0))?
            .filter_map(|r| r.ok())
            .collect();
        Ok(files)
    }

    // ── Bulk operations ──────────────────────────────────────────────────

    /// Bulk-insert symbols and dependency edges inside a single transaction.
    ///
    /// Much faster than individual upserts for initial indexing.
    pub(crate) fn bulk_index(
        &self,
        symbols: &[SymbolLocation],
        edges: &[DependencyEdge],
    ) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().expect("symbol store conn lock");
        conn.execute_batch("BEGIN")?;

        {
            let mut sym_stmt = conn.prepare_cached(UPSERT_SYMBOL_SQL)?;

            for sym in symbols {
                sym_stmt.execute(params![
                    sym.symbol_name,
                    sym.file_path,
                    sym.line as i64,
                    sym.column as i64,
                    sym.is_definition as i64,
                    sym.kind,
                ])?;
            }
        }

        {
            let mut dep_stmt = conn.prepare_cached(
                "INSERT INTO dependencies (from_file, to_file, symbols_json, co_edit_weight, updated_at)
                 VALUES (?1, ?2, ?3, ?4, datetime('now'))
                 ON CONFLICT(from_file, to_file) DO UPDATE SET
                    symbols_json = ?3,
                    co_edit_weight = ?4,
                    updated_at = datetime('now')",
            )?;

            for edge in edges {
                let symbols_json =
                    serde_json::to_string(&edge.symbols).unwrap_or_else(|_| "[]".to_string());
                dep_stmt.execute(params![
                    edge.from,
                    edge.to,
                    symbols_json,
                    edge.co_edit_weight
                ])?;
            }
        }

        // Rebuild dependency_symbols from symbols_json for all rows.
        // This is O(n) but bulk_index runs infrequently and keeps the
        // normalised table in sync with symbols_json in a single pass.
        conn.execute_batch(
            "DELETE FROM dependency_symbols;
             INSERT OR IGNORE INTO dependency_symbols (dep_id, symbol_name)
             SELECT d.id, j.value
             FROM dependencies d, json_each(d.symbols_json) j
             WHERE d.symbols_json != '[]';",
        )?;

        conn.execute_batch("COMMIT")?;
        Ok(())
    }

    /// Load all persisted symbols and edges into a `SymbolIndex`.
    ///
    /// Used on startup to restore the in-memory graph from disk.
    pub fn load_into_index(&self, index: &mut SymbolIndex) -> Result<usize, rusqlite::Error> {
        let conn = self.conn.lock().expect("symbol store conn lock");
        let mut loaded = 0usize;

        // Load symbols
        {
            let mut stmt = conn.prepare(
                "SELECT name, file_path, line, column_offset, is_definition, kind FROM symbols WHERE is_definition = 1",
            )?;
            let rows = stmt.query_map([], row_to_symbol)?;
            for row in rows {
                let sym = row?;
                // Populate symbols map
                index
                    .symbols
                    .entry(sym.symbol_name.clone())
                    .or_default()
                    .push(sym.clone());

                // Populate file_to_symbols map
                if sym.is_definition {
                    index
                        .file_to_symbols
                        .entry(sym.file_path.clone())
                        .or_default()
                        .push(sym.symbol_name.clone());
                }

                loaded += 1;
            }
        }

        // Load dependency edges
        {
            let mut stmt =
                conn.prepare("SELECT from_file, to_file, symbols_json FROM dependencies")?;
            let rows = stmt.query_map([], row_to_dep)?;
            for row in rows {
                let edge = row?;

                // Populate dependencies map
                index
                    .dependencies
                    .entry(edge.from.clone())
                    .or_default()
                    .push(edge.clone());

                // Populate reverse_deps map
                index
                    .reverse_deps
                    .entry(edge.to.clone())
                    .or_default()
                    .push(edge.from.clone());
            }
        }

        Ok(loaded)
    }

    /// Atomically replace all symbols for a file.
    /// Removes old symbols and inserts new ones in a single transaction.
    pub fn replace_file_symbols(
        &self,
        file_path: &str,
        symbols: &[SymbolLocation],
    ) -> Result<(), rusqlite::Error> {
        let conn = self.conn.lock().expect("symbol store conn lock");
        conn.execute_batch("BEGIN")?;

        // Remove old symbols for this file
        conn.execute(
            "DELETE FROM symbols WHERE file_path = ?1",
            params![file_path],
        )?;
        conn.execute(
            "DELETE FROM dependencies WHERE from_file = ?1",
            params![file_path],
        )?;

        // Insert new symbols
        {
            let mut stmt = conn.prepare_cached(UPSERT_SYMBOL_SQL)?;
            for sym in symbols {
                stmt.execute(params![
                    sym.symbol_name,
                    sym.file_path,
                    sym.line as i64,
                    sym.column as i64,
                    sym.is_definition as i64,
                    sym.kind,
                ])?;
            }
        }

        conn.execute_batch("COMMIT")?;
        Ok(())
    }
}

/// A detected symbol rename: the same symbol was removed under one name
/// and re-added under a different name at a nearby location.
///
/// Detected by cross-matching `remove` × `upsert` candidates where
/// `levenshtein(old_name, new_name) ≤ threshold` AND the new symbol
/// is a definition (`is_definition = true`).
#[derive(Debug, Clone)]
pub struct RenameCandidate {
    /// Original symbol name (the one being removed).
    pub from_name: String,
    /// New symbol name (the one being added).
    pub to_name: String,
    /// File path where the rename occurred.
    pub file_path: String,
    /// Line number of the old symbol.
    pub from_line: usize,
    /// Line number of the new symbol.
    pub to_line: usize,
}

/// A change set for incremental symbol updates.
///
/// Instead of replacing all symbols for a file, only the changed symbols
/// are persisted. This reduces write amplification for small edits.
#[derive(Debug, Default, Clone)]
pub struct SymbolChangeSet {
    /// Symbols to add or update (upsert semantics).
    pub upsert: Vec<SymbolLocation>,
    /// Symbols to remove, identified by `(name, file_path, line)`.
    pub remove: Vec<(String, String, usize)>,
    /// Detected renames: removed+added pairs with similar names.
    ///
    /// When a rename is detected, the old symbol is still in `remove`
    /// and the new symbol is still in `upsert`. The `renames` field
    /// provides the higher-level interpretation.
    #[allow(dead_code)] // consumed by observers and diagnostic tooling
    pub renames: Vec<RenameCandidate>,
}

impl SymbolChangeSet {
    /// Returns `true` if the change set has no operations.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.upsert.is_empty() && self.remove.is_empty()
    }

    /// Total number of operations (upserts + removals).
    #[must_use]
    pub fn len(&self) -> usize {
        self.upsert.len() + self.remove.len()
    }
}

/// Compute edit distance between two strings using the 2-row Wagner-Fischer DP algorithm.
///
/// Returns the Levenshtein distance in O(min(a,b)) space.
/// Caps computation early when `max_dist` exceeded (returns `max_dist + 1`).
#[allow(clippy::indexing_slicing)] // bounds are provably safe: indices always in 0..=n/m by loop invariant
fn levenshtein(a: &str, b: &str, max_dist: usize) -> usize {
    if a == b {
        return 0;
    }
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    let (a, b) = if a.len() > b.len() {
        (&b, &a)
    } else {
        (&a, &b)
    };
    let n = a.len();
    let m = b.len();

    // Early exit: length difference alone exceeds max_dist.
    if m - n > max_dist {
        return max_dist + 1;
    }

    let mut prev: Vec<usize> = (0..=n).collect();
    let mut curr = vec![0usize; n + 1];

    for j in 1..=m {
        curr[0] = j;
        let mut row_min = j;
        for i in 1..=n {
            let cost = if a[i - 1] == b[j - 1] { 0 } else { 1 };
            curr[i] = (prev[i - 1] + cost).min(prev[i] + 1).min(curr[i - 1] + 1);
            if curr[i] < row_min {
                row_min = curr[i];
            }
        }
        if row_min > max_dist {
            return max_dist + 1; // Early termination
        }
        std::mem::swap(&mut prev, &mut curr);
    }
    prev[n]
}

impl SymbolStore {
    /// Apply a change set atomically in a single transaction.
    ///
    /// More efficient than full re-indexing for incremental file edits.
    /// Removals are processed first, then upserts.
    pub fn apply_change_set(&self, changes: &SymbolChangeSet) -> Result<(), rusqlite::Error> {
        if changes.is_empty() {
            return Ok(());
        }

        // Acquire lock once for the entire transaction.
        let conn = self.conn.lock().expect("symbol store conn lock");
        conn.execute_batch("BEGIN IMMEDIATE")?;

        // Inner block so we can ROLLBACK on any error before returning.
        // We use a labelled block instead of a closure to avoid re-borrowing
        // issues with MutexGuard across a closure boundary.
        let result: Result<(), rusqlite::Error> = (|| {
            // Phase 1: removals
            {
                let mut del_stmt = conn.prepare_cached(
                    "DELETE FROM symbols WHERE name = ?1 AND file_path = ?2 AND line = ?3",
                )?;
                for (name, file_path, line) in &changes.remove {
                    del_stmt.execute(params![name, file_path, *line as i64])?;
                }
            }

            // Phase 2: upserts
            {
                let mut ups_stmt = conn.prepare_cached(UPSERT_SYMBOL_SQL)?;
                for sym in &changes.upsert {
                    ups_stmt.execute(params![
                        sym.symbol_name,
                        sym.file_path,
                        sym.line as i64,
                        sym.column as i64,
                        sym.is_definition as i64,
                        sym.kind,
                    ])?;
                }
            }

            Ok(())
        })();

        match result {
            Ok(()) => {
                conn.execute_batch("COMMIT")?;
                // Release the lock before notifying observers to avoid holding it
                // across potentially expensive observer callbacks.
                drop(conn);
                for observer in &self.observers {
                    observer.on_symbol_change(changes);
                }
                // H3: broadcast to async stream subscribers (fire-and-forget; ignore if no receivers).
                #[cfg(feature = "async-pipeline")]
                let _ = self.stream_sender.send(changes.clone());
                Ok(())
            }
            Err(e) => {
                // Best-effort rollback — ignore rollback error to surface the original.
                let _ = conn.execute_batch("ROLLBACK");
                Err(e)
            }
        }
    }

    /// Compute the change set between currently stored symbols for a file
    /// and a new set of symbols.
    ///
    /// Comparison key is `(symbol_name, line)`. Symbols present only in
    /// the new set are upserts; symbols present only in the store are removals.
    /// Symbols at the same `(name, line)` with different columns are upserts
    /// (the existing row is updated via ON CONFLICT).
    pub fn diff_symbols(
        &self,
        file_path: &str,
        new_symbols: &[SymbolLocation],
    ) -> Result<SymbolChangeSet, rusqlite::Error> {
        let current = self.find_symbols_in_file(file_path)?;

        // Build lookup sets keyed by (name, line)
        let current_keys: HashSet<(&str, usize)> = current
            .iter()
            .map(|s| (s.symbol_name.as_str(), s.line))
            .collect();

        let new_keys: HashSet<(&str, usize)> = new_symbols
            .iter()
            .map(|s| (s.symbol_name.as_str(), s.line))
            .collect();

        let mut changeset = SymbolChangeSet::default();

        // Additions: in new but not in current
        for sym in new_symbols {
            if !current_keys.contains(&(sym.symbol_name.as_str(), sym.line)) {
                changeset.upsert.push(sym.clone());
            }
        }

        // Removals: in current but not in new
        for sym in &current {
            if !new_keys.contains(&(sym.symbol_name.as_str(), sym.line)) {
                changeset
                    .remove
                    .push((sym.symbol_name.clone(), sym.file_path.clone(), sym.line));
            }
        }

        // Rename detection: cross-match removes × upserts with similar names.
        // Threshold: levenshtein distance ≤ max(1, name_len / 5) ≤ 3.
        for (from_name, from_file, from_line) in &changeset.remove {
            for new_sym in &changeset.upsert {
                if !new_sym.is_definition {
                    continue; // Only match against definitions
                }
                let max_dist = (from_name.len() / 5).clamp(1, 3);
                let dist = levenshtein(from_name, &new_sym.symbol_name, max_dist);
                if dist <= max_dist {
                    changeset.renames.push(RenameCandidate {
                        from_name: from_name.clone(),
                        to_name: new_sym.symbol_name.clone(),
                        file_path: from_file.clone(),
                        from_line: *from_line,
                        to_line: new_sym.line,
                    });
                    break; // One best match per removed symbol
                }
            }
        }

        Ok(changeset)
    }
}

// ── Row mapping helpers ──────────────────────────────────────────────────

fn row_to_symbol(row: &rusqlite::Row<'_>) -> Result<SymbolLocation, rusqlite::Error> {
    Ok(SymbolLocation {
        symbol_name: row.get(0)?,
        file_path: row.get(1)?,
        line: row.get::<_, i64>(2)? as usize,
        column: row.get::<_, i64>(3)? as usize,
        is_definition: row.get::<_, i64>(4)? != 0,
        // Defensive: SELECTs that omit the `kind` column (index 5) degrade to
        // `None` instead of erroring — mirrors `row_to_dep`'s co_edit_weight
        // fallback. Legacy NULL rows also map to `None`.
        kind: row.get::<_, Option<String>>(5).unwrap_or(None),
    })
}

fn row_to_dep(row: &rusqlite::Row<'_>) -> Result<DependencyEdge, rusqlite::Error> {
    let from: String = row.get(0)?;
    let to: String = row.get(1)?;
    let symbols_json: String = row.get(2)?;
    let symbols: Vec<String> = serde_json::from_str(&symbols_json).unwrap_or_default();
    let co_edit_weight: f64 = row.get(3).unwrap_or(0.0);
    Ok(DependencyEdge {
        from,
        to,
        symbols,
        co_edit_weight,
    })
}

// ── Persist helper on SymbolIndex ────────────────────────────────────────

impl SymbolIndex {
    /// Write all in-memory symbols and edges to the given store.
    ///
    /// Uses `bulk_index` for performance. Clears and re-inserts to ensure
    /// the persistent state matches the in-memory state exactly.
    pub fn persist_to(&self, store: &SymbolStore) -> Result<(), rusqlite::Error> {
        // Collect all symbol locations
        let symbols: Vec<SymbolLocation> = self
            .symbols
            .values()
            .flat_map(|v| v.iter().cloned())
            .collect();

        // Collect all dependency edges
        let edges: Vec<DependencyEdge> = self
            .dependencies
            .values()
            .flat_map(|v| v.iter().cloned())
            .collect();

        store.bulk_index(&symbols, &edges)
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)] // test vecs are asserted non-empty before indexing
    use super::*;
    use tempfile::TempDir;

    fn test_store() -> (TempDir, SymbolStore) {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("test_symbols.db");
        let store = SymbolStore::new(&db_path).unwrap();
        (tmp, store)
    }

    fn make_symbol(name: &str, file_path: &str, line: usize) -> SymbolLocation {
        SymbolLocation {
            symbol_name: name.to_string(),
            file_path: file_path.to_string(),
            line,
            column: 0,
            is_definition: true,
            kind: None,
        }
    }

    fn make_edge(from: &str, to: &str, symbols: &[&str]) -> DependencyEdge {
        DependencyEdge {
            from: from.to_string(),
            to: to.to_string(),
            symbols: symbols.iter().map(|s| s.to_string()).collect(),
            co_edit_weight: 0.0,
        }
    }

    #[test]
    fn test_symbol_kind_round_trips_through_store() {
        // A2: `kind` must survive upsert → SQLite → find_symbol (the path that
        // serves `touring index find`). Covers classification round-trip,
        // legacy `None` preservation, and the COALESCE clause that protects an
        // existing kind from being wiped by a later kind-less re-upsert.
        let (_tmp, store) = test_store();

        // 1. A classified symbol round-trips its kind.
        let func = make_symbol("handler", "src/api.py", 12).with_kind(Some("function".to_string()));
        store.upsert_symbol(&func).unwrap();
        let found = store.find_symbol("handler").unwrap();
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].kind.as_deref(), Some("function"));

        // 2. An unclassified symbol stays `None` (legacy / reference rows).
        let bare = make_symbol("legacy", "src/api.py", 20);
        store.upsert_symbol(&bare).unwrap();
        assert_eq!(store.find_symbol("legacy").unwrap()[0].kind, None);

        // 3. COALESCE: a kind-less re-upsert of the same (name,file,line) keeps the kind.
        let func_ref = make_symbol("handler", "src/api.py", 12); // kind = None
        store.upsert_symbol(&func_ref).unwrap();
        assert_eq!(
            store.find_symbol("handler").unwrap()[0].kind.as_deref(),
            Some("function"),
            "COALESCE(?6, kind) must preserve an existing kind on a None re-upsert"
        );
    }

    #[test]
    fn test_find_symbol_excludes_references_find_references_returns_them() {
        // B safe-by-construction: definitions and references coexist in the
        // table, but `find_symbol` returns only definitions (is_definition=1) so
        // every definition consumer (VGP, suggester, wiring) is unaffected;
        // references are reachable only via the explicit `find_references`.
        let (_tmp, store) = test_store();

        let def = make_symbol("target", "src/lib.rs", 5).with_kind(Some("function".to_string()));
        store.upsert_symbol(&def).unwrap();
        // A call-site of `target` from another file (is_definition=false).
        let call = SymbolLocation::new("src/main.rs", "target", 42, 8, false)
            .with_kind(Some("call".to_string()));
        store.upsert_symbol(&call).unwrap();

        // find_symbol → definitions only.
        let defs = store.find_symbol("target").unwrap();
        assert_eq!(defs.len(), 1, "find_symbol must exclude references");
        assert!(defs[0].is_definition);
        assert_eq!(defs[0].kind.as_deref(), Some("function"));

        // find_references → references only.
        let refs = store.find_references("target").unwrap();
        assert_eq!(refs.len(), 1, "find_references must return the call-site");
        assert!(!refs[0].is_definition);
        assert_eq!(refs[0].kind.as_deref(), Some("call"));
        assert_eq!(refs[0].file_path, "src/main.rs");
        assert_eq!(refs[0].line, 42);
    }

    #[test]
    fn test_create_store_fresh_db() {
        let (_tmp, store) = test_store();
        let stats = store.stats().unwrap();
        assert_eq!(stats.symbol_count, 0);
        assert_eq!(stats.file_count, 0);
        assert_eq!(stats.dependency_count, 0);
    }

    #[test]
    fn test_upsert_and_find_symbol() {
        let (_tmp, store) = test_store();

        let sym = make_symbol("hello", "src/main.py", 10);
        store.upsert_symbol(&sym).unwrap();

        let results = store.find_symbol("hello").unwrap();
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].symbol_name, "hello");
        assert_eq!(results[0].file_path, "src/main.py");
        assert_eq!(results[0].line, 10);
        assert!(results[0].is_definition);

        // Upsert same symbol updates without error
        store.upsert_symbol(&sym).unwrap();
        let results2 = store.find_symbol("hello").unwrap();
        assert_eq!(results2.len(), 1);
    }

    #[test]
    fn test_bulk_index() {
        let (_tmp, store) = test_store();

        let symbols = vec![
            make_symbol("foo", "a.py", 1),
            make_symbol("bar", "a.py", 10),
            make_symbol("baz", "b.py", 5),
        ];
        let edges = vec![
            make_edge("a.py", "b.py", &["baz"]),
            make_edge("b.py", "c.py", &["util"]),
        ];

        store.bulk_index(&symbols, &edges).unwrap();

        let stats = store.stats().unwrap();
        assert_eq!(stats.symbol_count, 3);
        assert_eq!(stats.file_count, 2); // a.py and b.py have symbols
        assert_eq!(stats.dependency_count, 2);
    }

    #[test]
    fn test_search_prefix() {
        let (_tmp, store) = test_store();

        let symbols = vec![
            make_symbol("process_file", "a.py", 1),
            make_symbol("process_data", "a.py", 20),
            make_symbol("parse_input", "b.py", 5),
        ];
        store.bulk_index(&symbols, &[]).unwrap();

        let results = store.search_symbols("process").unwrap();
        assert_eq!(results.len(), 2);
        assert!(results.iter().all(|s| s.symbol_name.starts_with("process")));

        let results2 = store.search_symbols("parse").unwrap();
        assert_eq!(results2.len(), 1);
        assert_eq!(results2[0].symbol_name, "parse_input");

        // No match
        let results3 = store.search_symbols("zzz_no_match").unwrap();
        assert!(results3.is_empty());
    }

    #[test]
    fn test_dependencies() {
        let (_tmp, store) = test_store();

        let edges = vec![
            make_edge("a.py", "b.py", &["helper"]),
            make_edge("a.py", "c.py", &["util", "config"]),
            make_edge("d.py", "b.py", &["helper"]),
        ];
        store.bulk_index(&[], &edges).unwrap();

        // Forward deps from a.py
        let deps = store.get_dependencies("a.py").unwrap();
        assert_eq!(deps.len(), 2);

        // Reverse deps for b.py (who imports b.py?)
        let rev = store.get_reverse_deps("b.py").unwrap();
        assert_eq!(rev.len(), 2);
        assert!(rev.contains(&"a.py".to_string()));
        assert!(rev.contains(&"d.py".to_string()));

        // Reverse deps for c.py
        let rev_c = store.get_reverse_deps("c.py").unwrap();
        assert_eq!(rev_c.len(), 1);
        assert_eq!(rev_c[0], "a.py");

        // No reverse deps for a.py
        let rev_a = store.get_reverse_deps("a.py").unwrap();
        assert!(rev_a.is_empty());
    }

    #[test]
    fn test_remove_file_cascades() {
        let (_tmp, store) = test_store();

        let symbols = vec![
            make_symbol("foo", "a.py", 1),
            make_symbol("bar", "a.py", 10),
            make_symbol("baz", "b.py", 5),
        ];
        let edges = vec![
            make_edge("a.py", "b.py", &["baz"]),
            make_edge("b.py", "c.py", &["util"]),
        ];
        store.bulk_index(&symbols, &edges).unwrap();

        // Verify baseline
        assert_eq!(store.stats().unwrap().symbol_count, 3);
        assert_eq!(store.stats().unwrap().dependency_count, 2);

        // Remove file a.py
        store.remove_file("a.py").unwrap();

        // Symbols from a.py should be gone
        let in_a = store.find_symbols_in_file("a.py").unwrap();
        assert!(in_a.is_empty());

        // Symbol in b.py still exists
        let in_b = store.find_symbols_in_file("b.py").unwrap();
        assert_eq!(in_b.len(), 1);

        // Dependency from a.py should be gone, but b.py -> c.py remains
        let deps_a = store.get_dependencies("a.py").unwrap();
        assert!(deps_a.is_empty());
        let deps_b = store.get_dependencies("b.py").unwrap();
        assert_eq!(deps_b.len(), 1);

        // Stats reflect removal
        let stats = store.stats().unwrap();
        assert_eq!(stats.symbol_count, 1);
        assert_eq!(stats.dependency_count, 1);
    }

    #[test]
    fn test_stats() {
        let (_tmp, store) = test_store();

        // Empty store
        let stats = store.stats().unwrap();
        assert_eq!(stats.symbol_count, 0);
        assert_eq!(stats.file_count, 0);
        assert_eq!(stats.dependency_count, 0);

        // After indexing
        let symbols = vec![
            make_symbol("alpha", "x.rs", 1),
            make_symbol("beta", "x.rs", 10),
            make_symbol("gamma", "y.rs", 1),
            make_symbol("delta", "z.rs", 1),
        ];
        let edges = vec![make_edge("x.rs", "y.rs", &["gamma"])];
        store.bulk_index(&symbols, &edges).unwrap();

        let stats = store.stats().unwrap();
        assert_eq!(stats.symbol_count, 4);
        assert_eq!(stats.file_count, 3); // x.rs, y.rs, z.rs
        assert_eq!(stats.dependency_count, 1);
    }

    #[test]
    fn test_find_symbols_in_file() {
        let (_tmp, store) = test_store();

        let symbols = vec![
            make_symbol("foo", "main.py", 1),
            make_symbol("bar", "main.py", 20),
            make_symbol("baz", "other.py", 5),
        ];
        store.bulk_index(&symbols, &[]).unwrap();

        let in_main = store.find_symbols_in_file("main.py").unwrap();
        assert_eq!(in_main.len(), 2);
        // Ordered by line
        assert_eq!(in_main[0].line, 1);
        assert_eq!(in_main[1].line, 20);

        let in_other = store.find_symbols_in_file("other.py").unwrap();
        assert_eq!(in_other.len(), 1);

        let in_missing = store.find_symbols_in_file("nope.py").unwrap();
        assert!(in_missing.is_empty());
    }

    #[test]
    fn test_load_into_index() {
        let (_tmp, store) = test_store();

        let symbols = vec![
            make_symbol("foo", "a.py", 1),
            make_symbol("bar", "b.py", 10),
        ];
        let edges = vec![make_edge("a.py", "b.py", &["bar"])];
        store.bulk_index(&symbols, &edges).unwrap();

        // Load into a fresh index
        let mut index = SymbolIndex::new();
        let loaded = store.load_into_index(&mut index).unwrap();
        assert_eq!(loaded, 2);

        // Verify symbols
        let locs = index.find_symbol("foo");
        assert_eq!(locs.len(), 1);
        assert_eq!(locs[0].file_path, "a.py");

        // Verify file_to_symbols
        let file_syms = index.get_file_symbols("a.py");
        assert_eq!(file_syms.len(), 1);

        // Verify dependencies
        assert!(index.dependencies.contains_key("a.py"));
        assert_eq!(index.dependencies["a.py"].len(), 1);

        // Verify reverse deps
        assert!(index.reverse_deps.contains_key("b.py"));
        assert!(index.reverse_deps["b.py"].contains(&"a.py".to_string()));
    }

    #[test]
    fn test_persist_and_reload_roundtrip() {
        let (_tmp, store) = test_store();

        // Build an index in memory
        let mut original = SymbolIndex::new();
        original.symbols.insert(
            "greet".to_string(),
            vec![make_symbol("greet", "hello.py", 5)],
        );
        original
            .file_to_symbols
            .insert("hello.py".to_string(), vec!["greet".to_string()]);
        original.dependencies.insert(
            "hello.py".to_string(),
            vec![make_edge("hello.py", "utils.py", &["fmt"])],
        );
        original
            .reverse_deps
            .insert("utils.py".to_string(), vec!["hello.py".to_string()]);

        // Persist to store
        original.persist_to(&store).unwrap();

        // Load into a new index
        let mut restored = SymbolIndex::new();
        store.load_into_index(&mut restored).unwrap();

        // Verify round-trip
        assert_eq!(restored.find_symbol("greet").len(), 1);
        assert_eq!(restored.find_symbol("greet")[0].line, 5);
        assert!(restored.dependencies.contains_key("hello.py"));
        assert!(restored.reverse_deps.contains_key("utils.py"));
    }

    #[test]
    fn test_replace_file_symbols_atomic() {
        let (_tmp, store) = test_store();

        // Seed initial symbols for two files
        let initial = vec![
            make_symbol("old_fn", "target.py", 1),
            make_symbol("old_cls", "target.py", 20),
            make_symbol("other_fn", "other.py", 5),
        ];
        let edges = vec![
            make_edge("target.py", "other.py", &["other_fn"]),
            make_edge("other.py", "target.py", &["old_fn"]),
        ];
        store.bulk_index(&initial, &edges).unwrap();

        // Verify baseline
        assert_eq!(store.find_symbols_in_file("target.py").unwrap().len(), 2);
        assert_eq!(store.get_dependencies("target.py").unwrap().len(), 1);
        assert_eq!(store.find_symbols_in_file("other.py").unwrap().len(), 1);

        // Replace symbols for target.py atomically
        let new_symbols = vec![
            make_symbol("new_fn", "target.py", 3),
            make_symbol("new_cls", "target.py", 30),
            make_symbol("new_helper", "target.py", 50),
        ];
        store
            .replace_file_symbols("target.py", &new_symbols)
            .unwrap();

        // Old symbols gone, new symbols present
        let in_target = store.find_symbols_in_file("target.py").unwrap();
        assert_eq!(in_target.len(), 3);
        assert_eq!(in_target[0].symbol_name, "new_fn");
        assert_eq!(in_target[1].symbol_name, "new_cls");
        assert_eq!(in_target[2].symbol_name, "new_helper");

        // Old symbols no longer findable
        assert!(store.find_symbol("old_fn").unwrap().is_empty());
        assert!(store.find_symbol("old_cls").unwrap().is_empty());

        // Dependencies from target.py cleared
        assert!(store.get_dependencies("target.py").unwrap().is_empty());

        // other.py untouched
        assert_eq!(store.find_symbols_in_file("other.py").unwrap().len(), 1);
        // Dependency from other.py -> target.py still exists
        assert_eq!(store.get_dependencies("other.py").unwrap().len(), 1);
    }

    // ── SymbolChangeSet tests ──────────────────────────────────────────

    #[test]
    fn test_apply_change_set_empty() {
        let (_tmp, store) = test_store();
        let empty = SymbolChangeSet::default();
        assert!(empty.is_empty());
        assert_eq!(empty.len(), 0);
        store.apply_change_set(&empty).unwrap();
        assert_eq!(store.stats().unwrap().symbol_count, 0);
    }

    #[test]
    fn test_apply_change_set_upserts_only() {
        let (_tmp, store) = test_store();

        let changes = SymbolChangeSet {
            upsert: vec![
                make_symbol("alpha", "a.py", 1),
                make_symbol("beta", "a.py", 10),
            ],
            remove: vec![],
            renames: vec![],
        };
        store.apply_change_set(&changes).unwrap();

        assert_eq!(store.stats().unwrap().symbol_count, 2);
        assert_eq!(store.find_symbol("alpha").unwrap().len(), 1);
        assert_eq!(store.find_symbol("beta").unwrap().len(), 1);
    }

    #[test]
    fn test_apply_change_set_removals_only() {
        let (_tmp, store) = test_store();

        // Seed data
        let initial = vec![
            make_symbol("foo", "a.py", 1),
            make_symbol("bar", "a.py", 10),
        ];
        store.bulk_index(&initial, &[]).unwrap();
        assert_eq!(store.stats().unwrap().symbol_count, 2);

        // Remove one
        let changes = SymbolChangeSet {
            upsert: vec![],
            remove: vec![("foo".to_string(), "a.py".to_string(), 1)],
            renames: vec![],
        };
        store.apply_change_set(&changes).unwrap();

        assert_eq!(store.stats().unwrap().symbol_count, 1);
        assert!(store.find_symbol("foo").unwrap().is_empty());
        assert_eq!(store.find_symbol("bar").unwrap().len(), 1);
    }

    #[test]
    fn test_apply_change_set_mixed() {
        let (_tmp, store) = test_store();

        // Seed data
        store
            .bulk_index(&[make_symbol("old", "a.py", 1)], &[])
            .unwrap();

        // Remove old, add new
        let changes = SymbolChangeSet {
            upsert: vec![make_symbol("new_fn", "a.py", 5)],
            remove: vec![("old".to_string(), "a.py".to_string(), 1)],
            renames: vec![],
        };
        store.apply_change_set(&changes).unwrap();

        assert!(store.find_symbol("old").unwrap().is_empty());
        assert_eq!(store.find_symbol("new_fn").unwrap().len(), 1);
    }

    #[test]
    fn test_diff_symbols_detects_additions() {
        let (_tmp, store) = test_store();

        // Seed: file has "foo"
        store
            .bulk_index(&[make_symbol("foo", "a.py", 1)], &[])
            .unwrap();

        // Diff with ["foo", "bar"]
        let new_symbols = vec![
            make_symbol("foo", "a.py", 1),
            make_symbol("bar", "a.py", 10),
        ];
        let diff = store.diff_symbols("a.py", &new_symbols).unwrap();

        assert_eq!(diff.upsert.len(), 1, "Should detect 1 addition (bar)");
        assert_eq!(diff.upsert[0].symbol_name, "bar");
        assert!(diff.remove.is_empty(), "No removals expected");
    }

    #[test]
    fn test_diff_symbols_detects_removals() {
        let (_tmp, store) = test_store();

        // Seed: file has "foo" and "bar"
        store
            .bulk_index(
                &[
                    make_symbol("foo", "a.py", 1),
                    make_symbol("bar", "a.py", 10),
                ],
                &[],
            )
            .unwrap();

        // New set only has "foo"
        let new_symbols = vec![make_symbol("foo", "a.py", 1)];
        let diff = store.diff_symbols("a.py", &new_symbols).unwrap();

        assert!(diff.upsert.is_empty(), "No additions expected");
        assert_eq!(diff.remove.len(), 1, "Should detect 1 removal (bar)");
        assert_eq!(diff.remove[0].0, "bar");
    }

    #[test]
    fn test_diff_symbols_empty_file() {
        let (_tmp, store) = test_store();

        // File not in store yet
        let new_symbols = vec![
            make_symbol("alpha", "new.py", 1),
            make_symbol("beta", "new.py", 5),
        ];
        let diff = store.diff_symbols("new.py", &new_symbols).unwrap();

        assert_eq!(diff.upsert.len(), 2, "All symbols should be additions");
        assert!(diff.remove.is_empty());
    }

    #[test]
    fn test_diff_then_apply_roundtrip() {
        let (_tmp, store) = test_store();

        // Seed: file has "old_fn" at line 1
        store
            .bulk_index(&[make_symbol("old_fn", "a.py", 1)], &[])
            .unwrap();

        // New state: "old_fn" removed, "new_fn" added
        let new_symbols = vec![make_symbol("new_fn", "a.py", 5)];
        let diff = store.diff_symbols("a.py", &new_symbols).unwrap();

        assert_eq!(diff.upsert.len(), 1);
        assert_eq!(diff.remove.len(), 1);

        // Apply the diff
        store.apply_change_set(&diff).unwrap();

        // Verify final state
        assert!(store.find_symbol("old_fn").unwrap().is_empty());
        assert_eq!(store.find_symbol("new_fn").unwrap().len(), 1);
        assert_eq!(store.find_symbol("new_fn").unwrap()[0].line, 5);
    }

    #[test]
    fn test_replace_file_symbols_empty_replaces_with_nothing() {
        let (_tmp, store) = test_store();

        let initial = vec![make_symbol("will_go", "doomed.py", 1)];
        store.bulk_index(&initial, &[]).unwrap();
        assert_eq!(store.find_symbols_in_file("doomed.py").unwrap().len(), 1);

        // Replace with empty set = effectively a delete
        store.replace_file_symbols("doomed.py", &[]).unwrap();
        assert!(store.find_symbols_in_file("doomed.py").unwrap().is_empty());
        assert_eq!(store.stats().unwrap().symbol_count, 0);
    }

    // ── dependency_symbols (P1.4.3): relational symbol table ────────────

    #[test]
    fn test_find_deps_by_symbol_basic() {
        let (_tmp, store) = test_store();

        let edges = vec![
            make_edge("a.py", "lib.py", &["MyClass", "helper"]),
            make_edge("b.py", "lib.py", &["MyClass"]),
            make_edge("c.py", "other.py", &["unrelated"]),
        ];
        store.bulk_index(&[], &edges).unwrap();

        // Both a.py and b.py import MyClass
        let mut deps = store.find_deps_by_symbol("MyClass").unwrap();
        deps.sort_by_key(|e| e.from.clone());
        assert_eq!(deps.len(), 2);
        assert_eq!(deps[0].from, "a.py");
        assert_eq!(deps[1].from, "b.py");

        // Only a.py imports helper
        let helper_deps = store.find_deps_by_symbol("helper").unwrap();
        assert_eq!(helper_deps.len(), 1);
        assert_eq!(helper_deps[0].from, "a.py");

        // Nothing imports "nonexistent"
        let none = store.find_deps_by_symbol("nonexistent").unwrap();
        assert!(none.is_empty());
    }

    #[test]
    fn test_find_deps_by_symbol_after_upsert() {
        let (_tmp, store) = test_store();

        // Initial edge: a.py imports OldClass from lib.py
        store
            .upsert_dependency(&DependencyEdge {
                from: "a.py".to_string(),
                to: "lib.py".to_string(),
                symbols: vec!["OldClass".to_string()],
                co_edit_weight: 0.0,
            })
            .unwrap();

        assert_eq!(store.find_deps_by_symbol("OldClass").unwrap().len(), 1);

        // Update: a.py now imports NewClass instead
        store
            .upsert_dependency(&DependencyEdge {
                from: "a.py".to_string(),
                to: "lib.py".to_string(),
                symbols: vec!["NewClass".to_string()],
                co_edit_weight: 0.0,
            })
            .unwrap();

        // OldClass should be gone, NewClass present
        assert!(
            store.find_deps_by_symbol("OldClass").unwrap().is_empty(),
            "OldClass should be removed after upsert"
        );
        assert_eq!(
            store.find_deps_by_symbol("NewClass").unwrap().len(),
            1,
            "NewClass should be present after upsert"
        );
    }

    #[test]
    fn test_dependency_symbols_cascade_on_remove_file() {
        let (_tmp, store) = test_store();

        let edges = vec![
            make_edge("a.py", "lib.py", &["SharedClass"]),
            make_edge("b.py", "lib.py", &["SharedClass"]),
        ];
        store.bulk_index(&[], &edges).unwrap();

        assert_eq!(store.find_deps_by_symbol("SharedClass").unwrap().len(), 2);

        // Remove a.py — its dependency row cascades to dependency_symbols
        store.remove_file("a.py").unwrap();

        let remaining = store.find_deps_by_symbol("SharedClass").unwrap();
        assert_eq!(
            remaining.len(),
            1,
            "Only b.py should remain after removing a.py"
        );
        assert_eq!(remaining[0].from, "b.py");
    }

    #[test]
    fn test_levenshtein_basic() {
        assert_eq!(levenshtein("foo", "foo", 3), 0);
        assert_eq!(levenshtein("foo", "fob", 3), 1);
        // distance is 3 but max_dist=1, early exit returns max_dist+1=2
        assert_eq!(levenshtein("foo", "bar", 1), 2);
        // identical empty strings
        assert_eq!(levenshtein("", "", 0), 0);
        // length difference alone triggers early exit
        assert_eq!(levenshtein("a", "abcde", 1), 2);
    }

    #[test]
    fn test_rename_detection_basic() {
        let (_tmp, store) = test_store();

        let old_sym = SymbolLocation {
            symbol_name: "foo".to_string(),
            file_path: "src/main.rs".to_string(),
            line: 10,
            column: 0,
            is_definition: true,
            kind: None,
        };
        store.upsert_symbol(&old_sym).unwrap();

        let new_symbols = vec![
            SymbolLocation {
                symbol_name: "fob".to_string(),
                file_path: "src/main.rs".to_string(),
                line: 12,
                column: 0,
                is_definition: true,
                kind: None,
            },
            SymbolLocation {
                symbol_name: "bar".to_string(),
                file_path: "src/main.rs".to_string(),
                line: 20,
                column: 0,
                is_definition: true,
                kind: None,
            },
        ];

        let changeset = store.diff_symbols("src/main.rs", &new_symbols).unwrap();

        assert_eq!(changeset.remove.len(), 1);
        assert_eq!(changeset.remove[0].0, "foo");
        assert_eq!(changeset.upsert.len(), 2);

        // "foo" -> "fob": levenshtein=1, threshold=max(1, 3/5)=1 => rename detected
        assert_eq!(
            changeset.renames.len(),
            1,
            "Expected exactly one rename candidate"
        );
        let rename = &changeset.renames[0];
        assert_eq!(rename.from_name, "foo");
        assert_eq!(rename.to_name, "fob");
        assert_eq!(rename.file_path, "src/main.rs");
        assert_eq!(rename.from_line, 10);
        assert_eq!(rename.to_line, 12);
    }

    #[test]
    fn test_rename_detection_no_false_positive_for_unrelated_symbols() {
        let (_tmp, store) = test_store();

        let old_sym = SymbolLocation {
            symbol_name: "process_data".to_string(),
            file_path: "src/lib.rs".to_string(),
            line: 5,
            column: 0,
            is_definition: true,
            kind: None,
        };
        store.upsert_symbol(&old_sym).unwrap();

        let new_symbols = vec![SymbolLocation {
            symbol_name: "render_view".to_string(),
            file_path: "src/lib.rs".to_string(),
            line: 5,
            column: 0,
            is_definition: true,
            kind: None,
        }];

        let changeset = store.diff_symbols("src/lib.rs", &new_symbols).unwrap();

        assert!(
            changeset.renames.is_empty(),
            "No rename expected for completely different symbol names"
        );
    }

    // ── C7: Property-based tests (proptest) ──────────────────────────────────

    #[cfg(test)]
    mod proptest_tests {
        use super::*;
        use proptest::prelude::*;

        /// Strategy: arbitrary short symbol name (alphanumeric, 1–20 chars).
        fn symbol_name() -> impl Strategy<Value = String> {
            "[a-z][a-z0-9_]{0,19}".prop_map(|s| s)
        }

        /// Strategy: arbitrary file path segment.
        fn file_path() -> impl Strategy<Value = String> {
            "[a-z][a-z0-9_]{0,9}".prop_map(|s| format!("src/{s}.rs"))
        }

        /// A `SymbolLocation` with controlled contents.
        fn symbol_location() -> impl Strategy<Value = SymbolLocation> {
            (symbol_name(), file_path(), 1usize..500usize).prop_map(|(name, path, line)| {
                SymbolLocation {
                    symbol_name: name,
                    file_path: path,
                    line,
                    column: 0,
                    is_definition: true,
                    kind: None,
                }
            })
        }

        proptest! {
            /// An empty SymbolChangeSet must report `is_empty() == true`
            /// regardless of how it was constructed.
            #[test]
            fn prop_empty_changeset_is_empty(_dummy in 0u8..255) {
                let cs = SymbolChangeSet::default();
                prop_assert!(cs.is_empty());
            }

            /// Any SymbolChangeSet with at least one upsert must NOT be empty.
            #[test]
            fn prop_nonempty_upsert_not_empty(sym in symbol_location()) {
                let cs = SymbolChangeSet {
                    upsert: vec![sym],
                    remove: vec![],
                    renames: vec![],
                };
                prop_assert!(!cs.is_empty());
            }

            /// Any SymbolChangeSet with at least one remove must NOT be empty.
            #[test]
            fn prop_nonempty_remove_not_empty(
                name in symbol_name(),
                path in file_path(),
                line in 1usize..500usize,
            ) {
                let cs = SymbolChangeSet {
                    upsert: vec![],
                    remove: vec![(name, path, line)],
                    renames: vec![],
                };
                prop_assert!(!cs.is_empty());
            }

            /// Clone of a SymbolChangeSet must have identical is_empty() result.
            #[test]
            fn prop_clone_preserves_empty(
                upsert_count in 0usize..5,
                sym in symbol_location(),
            ) {
                let upserts = if upsert_count > 0 { vec![sym] } else { vec![] };
                let cs = SymbolChangeSet {
                    upsert: upserts,
                    remove: vec![],
                    renames: vec![],
                };
                prop_assert_eq!(cs.is_empty(), cs.clone().is_empty());
            }
        }
    }

    // ── W3-cleanup regression test (2026-05-14) ─────────────────────────
    // Locks in the SQL fix in `get_indexed_files`. The previous query
    // mixed `SELECT DISTINCT` with an unaggregated `MAX(updated_at)` in
    // `ORDER BY`, which made SQLite silently compress the result to a
    // single aggregate row instead of returning every distinct path.

    /// REQUIREMENT `get_indexed_files` returns ALL distinct file_paths
    /// BOUNDARY 0 paths, 1 path, N paths
    /// COVER the SQL fix landed 2026-05-14 (SELECT … GROUP BY file_path)
    #[test]
    fn test_get_indexed_files_returns_every_distinct_path() {
        let (_tmp, store) = test_store();

        // Empty store: 0 paths.
        let empty = store.get_indexed_files(1_000).unwrap();
        assert!(empty.is_empty(), "expected empty for empty store");

        // Insert symbols across 5 distinct files, multiple symbols each.
        for i in 0..5 {
            for j in 0..3 {
                let sym = make_symbol(&format!("sym_{i}_{j}"), &format!("file_{i}.rs"), j + 1);
                store.upsert_symbol(&sym).unwrap();
            }
        }

        // Must return ALL 5 distinct file paths — not a collapsed 1-row.
        let listed = store.get_indexed_files(1_000).unwrap();
        assert_eq!(
            listed.len(),
            5,
            "regression: get_indexed_files returned {} rows instead of 5 distinct paths. Got: {:?}",
            listed.len(),
            listed
        );
        for i in 0..5 {
            let expected = format!("file_{i}.rs");
            assert!(listed.contains(&expected), "missing path: {expected}");
        }
    }

    /// REQUIREMENT `get_indexed_files` honors the `limit` parameter
    /// BOUNDARY limit=1, limit < total, limit > total
    /// COVER LIMIT ?1 binding
    #[test]
    fn test_get_indexed_files_honors_limit() {
        let (_tmp, store) = test_store();
        for i in 0..10 {
            let sym = make_symbol(&format!("s{i}"), &format!("file_{i}.rs"), 1);
            store.upsert_symbol(&sym).unwrap();
        }

        assert_eq!(store.get_indexed_files(1).unwrap().len(), 1);
        assert_eq!(store.get_indexed_files(5).unwrap().len(), 5);
        assert_eq!(store.get_indexed_files(10).unwrap().len(), 10);
        // Over-limit returns all available without error.
        assert_eq!(store.get_indexed_files(usize::MAX).unwrap().len(), 10);
    }
}
