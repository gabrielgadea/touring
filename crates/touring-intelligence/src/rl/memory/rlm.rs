//! RLM Memory — SQLite-backed tiered memory storage.
//!
//! Connects to `.claude/data/rlm_memory.db` with identical schema to rust-core persistence.
//! Unified from touring/src/memory/rlm.rs (737 LOC)

use chrono::{DateTime, NaiveDateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params, types::ValueRef};
use std::path::Path;
use thiserror::Error;

use super::palace::PalaceHierarchy;

/// Errors that can occur in RLM memory operations.
#[derive(Error, Debug)]
pub enum RlmError {
    /// Underlying SQLite failure.
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
    /// The provided tier name could not be parsed into a `MemoryTier`.
    #[error("Invalid tier: {0}")]
    InvalidTier(String),
    /// No entry exists for the requested key.
    #[error("Key not found: {0}")]
    KeyNotFound(String),
}

/// Result type for RLM operations.
pub type Result<T> = std::result::Result<T, RlmError>;

/// Memory tier classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
pub enum MemoryTier {
    /// Short-lived memory, garbage-collected aggressively.
    Ephemeral,
    /// Session-scoped working memory.
    Working,
    /// Project-scoped reference memory.
    Reference,
    /// Most persistent, highest-priority memory.
    Core,
}

impl std::fmt::Display for MemoryTier {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl MemoryTier {
    /// Returns the retention priority (higher = more persistent).
    #[must_use]
    pub fn priority(&self) -> u8 {
        match self {
            Self::Ephemeral => 0,
            Self::Working => 1,
            Self::Reference => 2,
            Self::Core => 3,
        }
    }

    /// Returns the canonical lowercase string name of the tier.
    pub fn as_str(&self) -> &'static str {
        match self {
            MemoryTier::Ephemeral => "ephemeral",
            MemoryTier::Working => "working",
            MemoryTier::Reference => "reference",
            MemoryTier::Core => "core",
        }
    }

    /// Parses a tier name (with aliases) into a `MemoryTier`.
    pub fn parse_tier(s: &str) -> Result<Self> {
        match s.to_lowercase().as_str() {
            "ephemeral" | "reflexive" => Ok(MemoryTier::Ephemeral),
            "working" | "session" => Ok(MemoryTier::Working),
            "reference" | "project" => Ok(MemoryTier::Reference),
            "core" => Ok(MemoryTier::Core),
            _ => Err(RlmError::InvalidTier(s.to_string())),
        }
    }
}

/// A matched memory entry with relevance score.
#[derive(Debug, Clone)]
pub struct MemoryMatch {
    /// Key of the matched entry.
    pub key: String,
    /// Tier the entry belongs to.
    pub tier: String,
    /// Stored value of the entry.
    pub value: String,
    /// Optional entry type classifier.
    pub entry_type: Option<String>,
    /// Relevance score of the match.
    pub score: f32,
    /// Number of times the entry has been accessed.
    pub access_count: i64,
    /// Unix timestamp of entry creation.
    pub created_at: i64,
    /// Unix timestamp of the last access.
    pub accessed_at: i64,
}

/// RLM Memory storage backed by SQLite.
#[derive(Debug)]
pub struct RlmMemory {
    conn: Connection,
}

/// Safe mmap_size default: 4 GB. Clamped to avoid exceeding system limits.
const SAFE_MMAP_SIZE: u64 = 4_294_967_296; // 4 GB

impl RlmMemory {
    /// Opens (or creates) the RLM memory database at `db_path`.
    pub fn new(db_path: &Path) -> Result<Self> {
        let conn = Connection::open(db_path)?;
        conn.execute_batch(
            "PRAGMA journal_mode = WAL;
             PRAGMA synchronous = NORMAL;
             PRAGMA temp_store = MEMORY;
             PRAGMA cache_size = -8000;",
        )?;

        // Set mmap_size with a safe fallback — if the PRAGMA fails
        // (e.g., OS rejects the mapping), log a warning and continue
        // without memory-mapped I/O.
        let mmap_stmt = format!("PRAGMA mmap_size = {};", SAFE_MMAP_SIZE);
        if let Err(e) = conn.execute_batch(&mmap_stmt) {
            tracing::warn!(
                mmap_size = SAFE_MMAP_SIZE,
                error = %e,
                "PRAGMA mmap_size failed, continuing without mmap"
            );
        }

        let memory = Self { conn };
        memory.ensure_schema()?;

        // S-M5: PRAGMA optimize after schema creation — updates index statistics
        // so SQLite can choose optimal query plans from the first query.
        // Fire-and-forget: failure is non-fatal (DB still functional).
        if let Err(e) = memory.conn.execute_batch("PRAGMA optimize;") {
            tracing::warn!(error = %e, "PRAGMA optimize failed in RlmMemory::new, continuing");
        }

        Ok(memory)
    }

    fn ensure_schema(&self) -> Result<()> {
        self.conn.execute(
            "CREATE TABLE IF NOT EXISTS memory_entries (
                key TEXT NOT NULL,
                tier TEXT NOT NULL,
                value TEXT NOT NULL,
                entry_type TEXT NOT NULL,
                created_at INTEGER NOT NULL,
                accessed_at INTEGER NOT NULL,
                access_count INTEGER NOT NULL,
                embedding BLOB,
                PRIMARY KEY (key, tier)
            )",
            [],
        )?;

        // Idempotent column migrations. Legacy DBs predate later columns, and
        // `CREATE TABLE IF NOT EXISTS` never backfills them, so each absent
        // column must be added explicitly. (The `embedding` gap in particular
        // floods `store_insight` with "no column named embedding" warns and
        // eventually crashes the daemon if left unmigrated.) Columns are added
        // before `ensure_indexes` so every index has its column present.
        self.add_column_if_missing(
            "accessed_at",
            "ALTER TABLE memory_entries ADD COLUMN accessed_at INTEGER NOT NULL DEFAULT 0",
        )?;
        self.add_column_if_missing(
            "embedding",
            "ALTER TABLE memory_entries ADD COLUMN embedding BLOB",
        )?;
        self.add_column_if_missing(
            "file_path",
            "ALTER TABLE memory_entries ADD COLUMN file_path TEXT",
        )?;
        self.add_column_if_missing(
            "graph_blast_radius",
            "ALTER TABLE memory_entries ADD COLUMN graph_blast_radius INTEGER",
        )?;
        self.add_column_if_missing(
            "palace_path",
            "ALTER TABLE memory_entries ADD COLUMN palace_path TEXT",
        )?;

        // The `r` of a case `(s, a, r)`.
        //
        // Memento (arXiv 2508.16153, Eq. 12) writes every case to the bank as a
        // (state, action, reward) triple, and its optimal retrieval policy is a
        // softmax over the value of those cases (Eq. 7) — not over similarity.
        // Touring's bank stored only (key, value): entries carried no notion of
        // whether the lesson they hold ever WORKED, so recall could rank by
        // resemblance alone. These two columns are what a value-ranked recall
        // needs to exist at all (04/08/2026).
        //
        // Nullable on purpose: an entry whose outcome was never observed is
        // NOT the same as one that scored zero, and collapsing the two would
        // teach the ranker that unmeasured means bad.
        self.add_column_if_missing(
            "outcome_reward",
            "ALTER TABLE memory_entries ADD COLUMN outcome_reward REAL",
        )?;
        self.add_column_if_missing(
            "outcome_context",
            "ALTER TABLE memory_entries ADD COLUMN outcome_context TEXT",
        )?;

        self.ensure_indexes()?;
        self.ensure_fts()?;
        Ok(())
    }

    /// Returns whether `column` exists on the `memory_entries` table.
    fn column_exists(&self, column: &str) -> Result<bool> {
        let mut stmt = self
            .conn
            .prepare("SELECT COUNT(*) FROM pragma_table_info('memory_entries') WHERE name = ?1")?;
        let count = stmt
            .query_row(params![column], |row| row.get::<_, i64>(0))
            .unwrap_or(0);
        Ok(count > 0)
    }

    /// Returns whether a table named `name` exists in the database.
    fn table_exists(&self, name: &str) -> Result<bool> {
        let mut stmt = self
            .conn
            .prepare("SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name = ?1")?;
        let count = stmt
            .query_row(params![name], |row| row.get::<_, i64>(0))
            .unwrap_or(0);
        Ok(count > 0)
    }

    /// Idempotently applies `ddl` (an `ALTER TABLE … ADD COLUMN …` statement)
    /// when `column` is absent from `memory_entries`. Collapses the five legacy
    /// column migrations into one reusable step (no per-column copy-paste).
    fn add_column_if_missing(&self, column: &str, ddl: &str) -> Result<()> {
        if !self.column_exists(column)? {
            self.conn.execute(ddl, [])?;
        }
        Ok(())
    }

    /// Creates the secondary indexes `memory_entries` relies on, idempotently.
    ///
    /// Includes the Sprint 8 P1 fix: legacy DBs created with `PRIMARY KEY (key)`
    /// only lacked any constraint on `(key, tier)`, so every UPSERT
    /// (`INSERT … ON CONFLICT(key, tier)`) failed with "ON CONFLICT clause does
    /// not match any PRIMARY KEY or UNIQUE constraint". A UNIQUE INDEX on
    /// `(key, tier)` is a valid conflict target with identical semantics, and a
    /// no-op on modern DBs where the composite PRIMARY KEY already provides it.
    fn ensure_indexes(&self) -> Result<()> {
        self.conn.execute_batch(
            "CREATE INDEX IF NOT EXISTS idx_memory_tier ON memory_entries(tier);
             CREATE INDEX IF NOT EXISTS idx_memory_accessed ON memory_entries(accessed_at DESC);
             CREATE INDEX IF NOT EXISTS idx_memory_type ON memory_entries(entry_type);
             CREATE INDEX IF NOT EXISTS idx_memory_file_path
                 ON memory_entries(file_path) WHERE file_path IS NOT NULL;
             CREATE INDEX IF NOT EXISTS idx_memory_palace_path
                 ON memory_entries(palace_path) WHERE palace_path IS NOT NULL;
             CREATE UNIQUE INDEX IF NOT EXISTS idx_memory_key_tier
                 ON memory_entries(key, tier);",
        )?;
        Ok(())
    }

    /// Creates the FTS5 full-text index and its sync triggers, idempotently.
    ///
    /// `memories_fts` is an external-content table (`content='memory_entries'`),
    /// so no data is duplicated; the triggers keep it in sync and the index is
    /// backfilled from existing rows on first creation. Replaces the O(n) `LIKE`
    /// scans `search()` would otherwise perform.
    fn ensure_fts(&self) -> Result<()> {
        if !self.table_exists("memories_fts")? {
            self.conn.execute_batch(
                "CREATE VIRTUAL TABLE memories_fts USING fts5(
                    key, value, entry_type,
                    content='memory_entries',
                    content_rowid='rowid'
                );
                -- Backfill existing rows into the FTS index
                INSERT INTO memories_fts(rowid, key, value, entry_type)
                    SELECT rowid, key, value, entry_type FROM memory_entries;",
            )?;
        }

        // Triggers are idempotent; recreate on every startup to handle new DBs.
        self.conn.execute_batch(
            "CREATE TRIGGER IF NOT EXISTS memory_fts_ai
             AFTER INSERT ON memory_entries BEGIN
                 INSERT INTO memories_fts(rowid, key, value, entry_type)
                 VALUES (new.rowid, new.key, new.value, new.entry_type);
             END;

             CREATE TRIGGER IF NOT EXISTS memory_fts_ad
             AFTER DELETE ON memory_entries BEGIN
                 INSERT INTO memories_fts(memories_fts, rowid, key, value, entry_type)
                 VALUES ('delete', old.rowid, old.key, old.value, old.entry_type);
             END;

             CREATE TRIGGER IF NOT EXISTS memory_fts_au
             AFTER UPDATE ON memory_entries BEGIN
                 INSERT INTO memories_fts(memories_fts, rowid, key, value, entry_type)
                 VALUES ('delete', old.rowid, old.key, old.value, old.entry_type);
                 INSERT INTO memories_fts(rowid, key, value, entry_type)
                 VALUES (new.rowid, new.key, new.value, new.entry_type);
             END;",
        )?;
        Ok(())
    }

    /// Build an FTS5 MATCH query from arbitrary user input.
    /// Each whitespace-separated token is double-quoted to prevent FTS5 operator
    /// injection. Multiple tokens are implicitly ANDed (FTS5 default).
    fn fts_query(query: &str) -> String {
        query
            .split_whitespace()
            .filter(|t| !t.is_empty())
            .map(|t| format!("\"{}\"", t.replace('"', "\"\"")))
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// Stores a memory entry in the given tier with an optional embedding.
    pub fn store(
        &self,
        key: &str,
        tier: MemoryTier,
        value: &str,
        entry_type: Option<&str>,
        embedding: Option<&[f32]>,
    ) -> Result<()> {
        self.store_internal(key, tier, value, entry_type, embedding, None)
    }

    /// Stores a memory entry along with graph-context metadata (file path, blast radius).
    pub fn store_with_file_path(
        &self,
        key: &str,
        tier: MemoryTier,
        value: &str,
        entry_type: Option<&str>,
        embedding: Option<&[f32]>,
        graph: &GraphMeta<'_>,
    ) -> Result<()> {
        self.store_internal(key, tier, value, entry_type, embedding, Some(graph))
    }

    /// Store a memory entry with palace hierarchy metadata.
    ///
    /// # Arguments
    ///
    /// * `key` - Memory key (unique within tier)
    /// * `tier` - Memory tier classification
    /// * `value` - Memory content
    /// * `palace` - PalaceHierarchy containing wing/room/closet/drawer path
    /// * `entry_type` - Optional type discriminator
    ///
    /// # Example
    ///
    /// ```
    /// use touring_intelligence::rl::memory::rlm::{RlmMemory, MemoryTier};
    /// use touring_intelligence::rl::memory::palace::PalaceHierarchy;
    /// use tempfile::TempDir;
    ///
    /// let temp_dir = TempDir::new().unwrap();
    /// let db_path = temp_dir.path().join("test.db");
    /// let memory = RlmMemory::new(&db_path).unwrap();
    ///
    /// let palace = PalaceHierarchy::new(
    ///     "gabriel".to_string(),
    ///     Some("memory".to_string()),
    ///     Some("rlm".to_string()),
    ///     Some("test_entry".to_string()),
    /// ).unwrap();
    ///
    /// memory.store_with_palace(
    ///     "test_key",
    ///     MemoryTier::Working,
    ///     "test_value",
    ///     &palace,
    ///     "test",
    /// ).unwrap();
    /// ```
    pub fn store_with_palace(
        &self,
        key: &str,
        tier: MemoryTier,
        value: &str,
        palace: &PalaceHierarchy,
        entry_type: &str,
    ) -> Result<()> {
        let now = Utc::now().timestamp();
        let tier_str = tier.as_str();
        let palace_path = palace.to_storage();

        self.conn.execute(
            "INSERT INTO memory_entries
             (key, tier, value, entry_type, created_at, accessed_at, access_count, palace_path)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7)
             ON CONFLICT(key, tier) DO UPDATE SET
                value = excluded.value,
                entry_type = excluded.entry_type,
                accessed_at = excluded.accessed_at,
                palace_path = excluded.palace_path",
            params![key, tier_str, value, entry_type, now, now, palace_path],
        )?;
        Ok(())
    }

    /// Query entries by palace path prefix (e.g., "gabriel.memory.*").
    ///
    /// Returns entries where palace_path starts with the given prefix.
    /// Results are ordered by most recently accessed.
    ///
    /// # Arguments
    ///
    /// * `palace_prefix` - Prefix to match (e.g., "gabriel.memory" matches "gabriel.memory.*")
    /// * `top_k` - Maximum number of results to return
    ///
    /// # Returns
    ///
    /// Vector of tuples: (key, value, palace_path)
    ///
    /// # Example
    ///
    /// ```
    /// use touring_intelligence::rl::memory::rlm::{RlmMemory, MemoryTier};
    /// use touring_intelligence::rl::memory::palace::PalaceHierarchy;
    /// use tempfile::TempDir;
    ///
    /// let temp_dir = TempDir::new().unwrap();
    /// let db_path = temp_dir.path().join("test.db");
    /// let memory = RlmMemory::new(&db_path).unwrap();
    ///
    /// let results = memory.query_by_palace("gabriel", 10).unwrap();
    /// ```
    pub fn query_by_palace(
        &self,
        palace_prefix: &str,
        top_k: usize,
    ) -> Result<Vec<(String, String, String)>> {
        let prefix = if palace_prefix.ends_with('.') {
            palace_prefix.trim_end_matches('.').to_string()
        } else {
            format!("{}.", palace_prefix)
        };

        let mut stmt = self.conn.prepare(
            "SELECT key, value, palace_path
             FROM memory_entries
             WHERE palace_path IS NOT NULL AND palace_path LIKE ?1 || '%'
             ORDER BY accessed_at DESC
             LIMIT ?2",
        )?;

        let rows = stmt
            .query_map(params![prefix, top_k as i64], |row| {
                Ok((
                    row.get::<_, String>(0)?,
                    row.get::<_, String>(1)?,
                    row.get::<_, String>(2)?,
                ))
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    /// Shared upsert path for `store` and `store_with_file_path`.
    ///
    /// When `graph` is `None`, the base 8-column schema is used.
    /// When `graph` is `Some`, the two extra columns (`file_path`,
    /// `graph_blast_radius`) are included in the INSERT.
    fn store_internal(
        &self,
        key: &str,
        tier: MemoryTier,
        value: &str,
        entry_type: Option<&str>,
        embedding: Option<&[f32]>,
        graph: Option<&GraphMeta<'_>>,
    ) -> Result<()> {
        let now = Utc::now().timestamp();
        let tier_str = tier.as_str();
        let entry_type = entry_type.unwrap_or("text");
        let embedding_bytes: Option<Vec<u8>> =
            embedding.map(|emb| emb.iter().flat_map(|f| f.to_le_bytes()).collect());

        match graph {
            None => {
                self.conn.execute(
                    "INSERT INTO memory_entries
                     (key, tier, value, entry_type, created_at, accessed_at, access_count, embedding)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7)
                     ON CONFLICT(key, tier) DO UPDATE SET
                        value = excluded.value,
                        entry_type = excluded.entry_type,
                        accessed_at = excluded.accessed_at,
                        embedding = excluded.embedding",
                    params![key, tier_str, value, entry_type, now, now, embedding_bytes],
                )?;
            }
            Some(g) => {
                self.conn.execute(
                    "INSERT INTO memory_entries
                     (key, tier, value, entry_type, created_at, accessed_at, access_count,
                      embedding, file_path, graph_blast_radius)
                     VALUES (?1, ?2, ?3, ?4, ?5, ?6, 0, ?7, ?8, ?9)
                     ON CONFLICT(key, tier) DO UPDATE SET
                        value = excluded.value,
                        entry_type = excluded.entry_type,
                        accessed_at = excluded.accessed_at,
                        embedding = excluded.embedding,
                        file_path = excluded.file_path,
                        graph_blast_radius = excluded.graph_blast_radius",
                    params![
                        key,
                        tier_str,
                        value,
                        entry_type,
                        now,
                        now,
                        embedding_bytes,
                        g.file_path,
                        g.blast_radius
                    ],
                )?;
            }
        }
        Ok(())
    }

    /// Retrieves an entry's value by key and tier, bumping its access count.
    pub fn get(&self, key: &str, tier: MemoryTier) -> Result<Option<String>> {
        let tier_str = tier.as_str();
        let now = Utc::now().timestamp();
        self.conn.execute(
            "UPDATE memory_entries SET access_count = access_count + 1, accessed_at = ?1 WHERE key = ?2 AND tier = ?3",
            params![now, key, tier_str],
        )?;
        let value: Option<String> = self
            .conn
            .query_row(
                "SELECT value FROM memory_entries WHERE key = ?1 AND tier = ?2",
                params![key, tier_str],
                |row| row.get(0),
            )
            .optional()?;
        Ok(value)
    }

    /// Full-text searches entries, optionally filtered by tier, returning the top matches.
    pub fn search(
        &self,
        query: &str,
        tier_filter: Option<MemoryTier>,
        top_k: usize,
    ) -> Result<Vec<MemoryMatch>> {
        let now = Utc::now().timestamp();
        let fts_q = Self::fts_query(query);

        // Empty query after tokenization — return nothing rather than a full scan.
        if fts_q.is_empty() {
            return Ok(Vec::new());
        }

        // FTS5 MATCH with optional tier filter. JOIN to memory_entries for scoring and
        // full row data; FTS5 provides the rowid for the join key.
        // FTS5 MATCH requires the real table name (not an alias) in the WHERE clause.
        let sql = "SELECT m.key, m.tier, m.value, m.entry_type,
                          m.created_at, m.accessed_at, m.access_count,
                          (m.access_count + 1) * (1.0 / (1.0 + (?1 - m.accessed_at) / 86400.0)) AS score
                   FROM memories_fts
                   JOIN memory_entries m ON memories_fts.rowid = m.rowid
                   WHERE memories_fts MATCH ?2 AND (?3 IS NULL OR m.tier = ?3)
                   ORDER BY score DESC
                   LIMIT ?4";

        let tier_str: Option<&str> = tier_filter.as_ref().map(|t| t.as_str());

        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt
            .query_map(params![now, fts_q, tier_str, top_k as i64], |row| {
                Ok(MemoryMatch {
                    key: row.get(0)?,
                    tier: row.get(1)?,
                    value: row.get(2)?,
                    entry_type: row.get(3)?,
                    created_at: row.get(4)?,
                    accessed_at: row.get(5)?,
                    access_count: row.get(6)?,
                    score: row.get(7)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;

        Ok(rows)
    }

    /// Normalize a loosely-typed `created_at`/`accessed_at` cell to epoch
    /// seconds, tolerating every storage class SQLite may hold there.
    ///
    /// `memory_entries` is a table shared by writers with divergent conventions:
    /// the RLM store binds an `i64` epoch — coerced to a digit *string* (e.g.
    /// `"1777658590"`) by the column's TEXT affinity — while other writers
    /// (e.g. `touring-hook-runtime`'s CEG store) rely on the column DEFAULT
    /// `datetime('now')`, an ISO-8601 string (e.g. `"2026-04-07 10:51:27"`). A
    /// plain `row.get::<_, i64>()` aborts the whole scan on the first TEXT cell
    /// with "Invalid column type Text".
    ///
    /// This read-side adapter is the correct seam for that affinity mismatch
    /// (A12): rather than `CAST`-ing in SQL — which silently truncates an
    /// ISO-8601 string to its leading year (`"2026-..."` → `2026`) — it inspects
    /// the actual [`ValueRef`] storage class and parses each form to a real
    /// epoch, never failing; only genuinely unparseable input yields `0`.
    fn cell_to_epoch(value: ValueRef<'_>) -> i64 {
        match value {
            ValueRef::Integer(i) => i,
            ValueRef::Real(r) => r as i64,
            ValueRef::Text(bytes) => {
                let s = std::str::from_utf8(bytes).unwrap_or("").trim();
                // 1. epoch already stored as a digit string (RLM i64 → TEXT affinity)
                if let Ok(epoch) = s.parse::<i64>() {
                    return epoch;
                }
                // 2. SQLite `datetime('now')` default: "YYYY-MM-DD HH:MM:SS" (UTC)
                if let Ok(dt) = NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S") {
                    return dt.and_utc().timestamp();
                }
                // 3. RFC3339 / ISO-8601 with explicit offset
                if let Ok(dt) = DateTime::parse_from_rfc3339(s) {
                    return dt.timestamp();
                }
                0
            }
            ValueRef::Null | ValueRef::Blob(_) => 0,
        }
    }

    /// Scan all entries whose key starts with `prefix`, bypassing FTS.
    ///
    /// Uses `WHERE key LIKE ?1 || '%'` for an exact key-prefix lookup, unlike
    /// `search()` which does full-text FTS5 matching over values. Use this when
    /// you need to enumerate structured sub-trees (e.g. diary project entries).
    ///
    /// `created_at`/`accessed_at` are normalized through `Self::cell_to_epoch`,
    /// so a row written by any of the shared table's writers (epoch-as-TEXT,
    /// ISO-8601 TEXT, or native INTEGER) is read correctly instead of aborting
    /// the scan — the read-adapter seam for the affinity mismatch (A12).
    pub fn scan_prefix(
        &self,
        prefix: &str,
        tier_filter: Option<MemoryTier>,
        limit: usize,
    ) -> Result<Vec<MemoryMatch>> {
        let tier_str: Option<&str> = tier_filter.as_ref().map(|t| t.as_str());
        let sql = "SELECT key, tier, value, entry_type, \
                   created_at, accessed_at, access_count, 1.0 AS score
                   FROM memory_entries
                   WHERE key LIKE ?1 AND (?2 IS NULL OR tier = ?2)
                   ORDER BY key ASC
                   LIMIT ?3";
        let like_pattern = format!("{}%", prefix);
        let mut stmt = self.conn.prepare(sql)?;
        let rows = stmt
            .query_map(params![like_pattern, tier_str, limit as i64], |row| {
                Ok(MemoryMatch {
                    key: row.get(0)?,
                    tier: row.get(1)?,
                    value: row.get(2)?,
                    entry_type: row.get(3)?,
                    created_at: Self::cell_to_epoch(row.get_ref(4)?),
                    accessed_at: Self::cell_to_epoch(row.get_ref(5)?),
                    access_count: row.get(6)?,
                    score: row.get(7)?,
                })
            })?
            .collect::<std::result::Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Deletes an entry by key and tier, returning whether a row was removed.
    pub fn delete(&self, key: &str, tier: MemoryTier) -> Result<bool> {
        let tier_str = tier.as_str();
        let rows_affected = self.conn.execute(
            "DELETE FROM memory_entries WHERE key = ?1 AND tier = ?2",
            params![key, tier_str],
        )?;
        Ok(rows_affected > 0)
    }

    /// Returns aggregate memory statistics, including per-tier counts.
    pub fn stats(&self) -> Result<MemoryStats> {
        let total_count: i64 =
            self.conn
                .query_row("SELECT COUNT(*) FROM memory_entries", [], |row| row.get(0))?;

        let mut stmt = self.conn.prepare(
            "SELECT tier, COUNT(*), SUM(access_count) FROM memory_entries GROUP BY tier",
        )?;

        let mut tier_counts = std::collections::HashMap::new();
        let mut tier_access = std::collections::HashMap::new();

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, i64>(2)?,
            ))
        })?;

        for row in rows {
            let (tier, count, access) = row?;
            tier_counts.insert(tier.clone(), count);
            tier_access.insert(tier, access);
        }

        Ok(MemoryStats {
            total_entries: total_count,
            tier_counts,
            tier_access_counts: tier_access,
        })
    }
}

/// Optional graph-context metadata for a memory entry.
///
/// Fields are `pub(crate)`: `GraphMeta` is internal graph-context detail
/// constructed only within this crate (the type is re-exported for naming, but
/// never built by external consumers), so its fields stay encapsulated.
#[derive(Debug, Clone, Default)]
pub struct GraphMeta<'a> {
    /// File path the memory entry relates to, if any.
    pub(crate) file_path: Option<&'a str>,
    /// Blast radius of the associated file, if known.
    pub(crate) blast_radius: Option<i64>,
}

/// Statistics about memory usage.
#[derive(Debug, Clone)]
pub struct MemoryStats {
    /// Total number of stored entries across all tiers.
    pub total_entries: i64,
    /// Number of entries per tier name.
    pub tier_counts: std::collections::HashMap<String, i64>,
    /// Total access count per tier name.
    pub tier_access_counts: std::collections::HashMap<String, i64>,
}

/// Tier promotion/demotion policy configuration.
#[derive(Debug, Clone)]
pub struct TierPolicy {
    /// Promote Ephemeral -> Working after N accesses within window.
    pub ephemeral_promote_accesses: i64,
    /// Time window, in seconds, for counting Ephemeral promotion accesses.
    pub ephemeral_promote_window_secs: i64,
    /// Promote Working -> Reference after N accesses within window.
    pub working_promote_accesses: i64,
    /// Time window, in seconds, for counting Working promotion accesses.
    pub working_promote_window_secs: i64,
    /// Demote Core -> Reference after N seconds without access.
    pub core_demote_secs: i64,
    /// Demote Reference -> Working after N seconds without access.
    pub reference_demote_secs: i64,
    /// Delete Ephemeral entries older than N seconds.
    pub ephemeral_ttl_secs: i64,
}

impl Default for TierPolicy {
    fn default() -> Self {
        Self {
            ephemeral_promote_accesses: 3,
            ephemeral_promote_window_secs: 86400, // 24h
            working_promote_accesses: 10,
            working_promote_window_secs: 604800, // 7 days
            core_demote_secs: 2592000,           // 30 days
            reference_demote_secs: 1209600,      // 14 days
            ephemeral_ttl_secs: 172800,          // 48h
        }
    }
}

/// Report of tier maintenance operations.
#[derive(Debug, Default)]
pub struct TierMaintenanceReport {
    /// Number of entries promoted from Ephemeral to Working.
    pub promoted_ephemeral_to_working: usize,
    /// Number of entries promoted from Working to Reference.
    pub promoted_working_to_reference: usize,
    /// Number of entries demoted from Core to Reference.
    pub demoted_core_to_reference: usize,
    /// Number of entries demoted from Reference to Working.
    pub demoted_reference_to_working: usize,
    /// Number of expired Ephemeral entries garbage-collected.
    pub gc_ephemeral: usize,
}

impl RlmMemory {
    /// Run tier maintenance: promote, demote, and garbage-collect.
    /// Call once per session-start.
    #[allow(clippy::field_reassign_with_default)] // fields assigned from SQL results, struct literal not practical
    pub fn maintain_tiers(&self, policy: &TierPolicy) -> Result<TierMaintenanceReport> {
        let now = Utc::now().timestamp();
        let mut report = TierMaintenanceReport::default();

        // 1. Promote Ephemeral -> Working (accessed enough within window)
        report.promoted_ephemeral_to_working = self.conn.execute(
            "UPDATE memory_entries SET tier = 'working'
             WHERE tier = 'ephemeral'
             AND access_count >= ?1
             AND (?2 - created_at) <= ?3",
            params![
                policy.ephemeral_promote_accesses,
                now,
                policy.ephemeral_promote_window_secs
            ],
        )?;

        // 2. Promote Working -> Reference (accessed enough within window)
        report.promoted_working_to_reference = self.conn.execute(
            "UPDATE memory_entries SET tier = 'reference'
             WHERE tier = 'working'
             AND access_count >= ?1
             AND (?2 - created_at) <= ?3",
            params![
                policy.working_promote_accesses,
                now,
                policy.working_promote_window_secs
            ],
        )?;

        // 3. Demote Core -> Reference (stale)
        report.demoted_core_to_reference = self.conn.execute(
            "UPDATE memory_entries SET tier = 'reference'
             WHERE tier = 'core'
             AND (?1 - accessed_at) > ?2",
            params![now, policy.core_demote_secs],
        )?;

        // 4. Demote Reference -> Working (stale)
        report.demoted_reference_to_working = self.conn.execute(
            "UPDATE memory_entries SET tier = 'working'
             WHERE tier = 'reference'
             AND (?1 - accessed_at) > ?2",
            params![now, policy.reference_demote_secs],
        )?;

        // 5. GC expired Ephemeral entries
        report.gc_ephemeral = self.conn.execute(
            "DELETE FROM memory_entries
             WHERE tier = 'ephemeral'
             AND (?1 - created_at) > ?2",
            params![now, policy.ephemeral_ttl_secs],
        )?;

        Ok(report)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rusqlite::types::ValueRef;
    use tempfile::TempDir;

    #[test]
    fn cell_to_epoch_handles_all_storage_classes() {
        // Native INTEGER epoch (a fresh INTEGER-affinity RLM table).
        assert_eq!(
            RlmMemory::cell_to_epoch(ValueRef::Integer(1_777_658_590)),
            1_777_658_590
        );
        // Epoch coerced to a digit string by TEXT affinity (the live diary rows).
        assert_eq!(
            RlmMemory::cell_to_epoch(ValueRef::Text(b"1777658590")),
            1_777_658_590
        );
        // SQLite `datetime('now')` default form "YYYY-MM-DD HH:MM:SS"; Y2K UTC = 946684800.
        // CAST AS INTEGER would have truncated this to 2000 — the adapter parses it fully.
        assert_eq!(
            RlmMemory::cell_to_epoch(ValueRef::Text(b"2000-01-01 00:00:00")),
            946_684_800
        );
        // RFC3339 / ISO-8601 with explicit offset → same instant.
        assert_eq!(
            RlmMemory::cell_to_epoch(ValueRef::Text(b"2000-01-01T00:00:00+00:00")),
            946_684_800
        );
        // Real degrades (defensive) toward zero.
        assert_eq!(RlmMemory::cell_to_epoch(ValueRef::Real(123.9)), 123);
        // NULL and unparseable text yield 0 rather than aborting the scan.
        assert_eq!(RlmMemory::cell_to_epoch(ValueRef::Null), 0);
        assert_eq!(
            RlmMemory::cell_to_epoch(ValueRef::Text(b"not-a-timestamp")),
            0
        );
        assert_eq!(RlmMemory::cell_to_epoch(ValueRef::Text(b"")), 0);
    }

    #[test]
    fn scan_prefix_reads_text_datetime_created_at() {
        // Regression for A12: reproduce the shared-schema condition where a row's
        // created_at is an ISO-8601 TEXT string (written via the `datetime('now')`
        // default path), not the native i64 a fresh RLM table stores. Before the
        // read-adapter this aborted the whole scan with "Invalid column type Text".
        let dir = TempDir::new().unwrap();
        let mem = RlmMemory::new(&dir.path().join("rlm.db")).unwrap();
        mem.conn
            .execute(
                "INSERT INTO memory_entries \
                 (key, tier, value, entry_type, created_at, accessed_at, access_count) \
                 VALUES ('diary:x', 'working', 'payload', 'text', \
                 '2000-01-01 00:00:00', '2000-01-01 00:00:00', 0)",
                [],
            )
            .unwrap();
        let rows = mem.scan_prefix("diary:", None, 10).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].value, "payload");
        // Parsed to a real epoch (946684800), not 0 and not the CAST-truncated 2000.
        assert_eq!(rows[0].created_at, 946_684_800);
    }

    // ── public-API coverage (store / get / search / delete / stats / palace) ──

    /// Fresh on-disk RLM over a temp dir; the `TempDir` must outlive the store.
    fn fresh() -> (TempDir, RlmMemory) {
        let dir = TempDir::new().unwrap();
        let mem = RlmMemory::new(&dir.path().join("rlm.db")).unwrap();
        (dir, mem)
    }

    #[test]
    fn store_and_get_round_trips() {
        let (_d, mem) = fresh();
        mem.store("k1", MemoryTier::Working, "hello", Some("lesson"), None)
            .unwrap();
        assert_eq!(
            mem.get("k1", MemoryTier::Working).unwrap().as_deref(),
            Some("hello")
        );
        // Keys are unique per tier — a different tier does not collide.
        assert!(mem.get("k1", MemoryTier::Core).unwrap().is_none());
    }

    #[test]
    fn store_upserts_on_key_tier_conflict() {
        let (_d, mem) = fresh();
        mem.store("k", MemoryTier::Working, "v1", None, None)
            .unwrap();
        mem.store("k", MemoryTier::Working, "v2", None, None)
            .unwrap();
        assert_eq!(
            mem.get("k", MemoryTier::Working).unwrap().as_deref(),
            Some("v2")
        );
        assert_eq!(mem.stats().unwrap().total_entries, 1);
    }

    #[test]
    fn search_finds_stored_entry_via_fts() {
        let (_d, mem) = fresh();
        mem.store(
            "doc1",
            MemoryTier::Reference,
            "the quick brown fox",
            Some("note"),
            None,
        )
        .unwrap();
        mem.store(
            "doc2",
            MemoryTier::Reference,
            "lazy dog sleeps",
            Some("note"),
            None,
        )
        .unwrap();
        let hits = mem.search("brown", None, 10).unwrap();
        assert_eq!(hits.len(), 1);
        assert_eq!(hits[0].key, "doc1");
        // Empty query returns nothing rather than scanning the whole table.
        assert!(mem.search("", None, 10).unwrap().is_empty());
    }

    #[test]
    fn delete_removes_entry_and_reports_outcome() {
        let (_d, mem) = fresh();
        mem.store("gone", MemoryTier::Working, "x", None, None)
            .unwrap();
        assert!(mem.delete("gone", MemoryTier::Working).unwrap());
        assert!(mem.get("gone", MemoryTier::Working).unwrap().is_none());
        // Deleting an absent row reports `false` rather than erroring.
        assert!(!mem.delete("gone", MemoryTier::Working).unwrap());
    }

    #[test]
    fn stats_counts_entries_per_tier() {
        let (_d, mem) = fresh();
        mem.store("a", MemoryTier::Working, "1", None, None)
            .unwrap();
        mem.store("b", MemoryTier::Working, "2", None, None)
            .unwrap();
        mem.store("c", MemoryTier::Core, "3", None, None).unwrap();
        let stats = mem.stats().unwrap();
        assert_eq!(stats.total_entries, 3);
        assert_eq!(stats.tier_counts.get("working").copied(), Some(2));
        assert_eq!(stats.tier_counts.get("core").copied(), Some(1));
    }

    #[test]
    fn store_with_file_path_persists_graph_meta() {
        let (_d, mem) = fresh();
        // Exercises the Some(graph) upsert arm and `GraphMeta` (pub(crate)
        // fields, constructed here intra-crate).
        let graph = GraphMeta {
            file_path: Some("src/lib.rs"),
            blast_radius: Some(7),
        };
        mem.store_with_file_path(
            "fp",
            MemoryTier::Working,
            "body",
            Some("lesson"),
            None,
            &graph,
        )
        .unwrap();
        assert_eq!(
            mem.get("fp", MemoryTier::Working).unwrap().as_deref(),
            Some("body")
        );
    }

    #[test]
    fn store_with_palace_then_query_by_palace() {
        let (_d, mem) = fresh();
        let palace = PalaceHierarchy::parse("gabriel.memory.test").unwrap();
        mem.store_with_palace("pk", MemoryTier::Reference, "palace-val", &palace, "lesson")
            .unwrap();
        let hits = mem.query_by_palace("gabriel", 10).unwrap();
        assert!(hits.iter().any(|(k, v, _)| k == "pk" && v == "palace-val"));
    }

    #[test]
    fn maintain_tiers_succeeds_on_default_policy() {
        let (_d, mem) = fresh();
        mem.store("m", MemoryTier::Ephemeral, "v", None, None)
            .unwrap();
        // A just-created entry with 0 accesses triggers no promotion under the
        // default policy; the call must still succeed and report a zeroed move.
        let report = mem.maintain_tiers(&TierPolicy::default()).unwrap();
        assert_eq!(report.promoted_ephemeral_to_working, 0);
    }

    #[test]
    fn test_store_and_get() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test_rlm.db");
        let memory = RlmMemory::new(&db_path).unwrap();

        memory
            .store(
                "test_key",
                MemoryTier::Working,
                "test_value",
                Some("test_type"),
                None,
            )
            .unwrap();

        let value = memory.get("test_key", MemoryTier::Working).unwrap();
        assert_eq!(value, Some("test_value".to_string()));

        let missing = memory.get("nonexistent", MemoryTier::Working).unwrap();
        assert_eq!(missing, None);
    }

    #[test]
    fn test_search() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test_rlm.db");
        let memory = RlmMemory::new(&db_path).unwrap();

        memory
            .store("key1", MemoryTier::Working, "value with apple", None, None)
            .unwrap();
        memory
            .store(
                "apple_key",
                MemoryTier::Working,
                "another value",
                None,
                None,
            )
            .unwrap();

        let results = memory.search("apple", None, 10).unwrap();
        assert_eq!(results.len(), 2);
    }
}
