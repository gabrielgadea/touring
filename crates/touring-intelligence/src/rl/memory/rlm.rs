//! RLM Memory — SQLite-backed tiered memory storage.
//!
//! Connects to `.claude/data/rlm_memory.db` with identical schema to rust-core persistence.
//! Unified from touring/src/memory/rlm.rs (737 LOC)

use chrono::{DateTime, NaiveDateTime, Utc};
use rusqlite::{Connection, OptionalExtension, params, types::ValueRef};
use std::path::Path;
use thiserror::Error;

use super::palace::PalaceHierarchy;
use super::tags;

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

/// Canonical DDL for `memory_entries` — the single source of truth for BOTH
/// write paths (H1 unification, 2026-08-12).
///
/// Design decisions (risk analysis in `docs/plans/2026-08-12-followups-l3/`):
/// - **`key` alone is the PRIMARY KEY**: identity is the canonical name
///   (REGRA #17); `tier` is an attribute, not part of identity. Every
///   production DB (touring + 3 pinned projects, 29,190 rows probed
///   2026-08-12) is key-only, and `memory_tags`/`memory_links` are keyed on
///   the entry key alone — a composite (key, tier) PK would split tag/link
///   identity across tiers.
/// - **Timestamps are TEXT datetime** (`created_at`, `last_accessed_at`) with
///   `accessed_at` kept as INTEGER epoch for the recall hot path; readers go
///   through `cell_to_epoch`, which accepts both shapes.
/// - Columns added to existing DBs via ALTER arrive NULLABLE (SQLite forbids
///   NOT NULL with a non-constant DEFAULT in ADD COLUMN); the constraint
///   difference is inert because every writer sets values explicitly.
pub const MEMORY_ENTRIES_DDL: &str = "CREATE TABLE IF NOT EXISTS memory_entries (
    key TEXT PRIMARY KEY,
    value TEXT NOT NULL,
    tier TEXT NOT NULL DEFAULT 'local',
    entry_type TEXT NOT NULL DEFAULT 'insight',
    access_count INTEGER NOT NULL DEFAULT 0,
    last_accessed_at TEXT NOT NULL DEFAULT (datetime('now')),
    created_at TEXT NOT NULL DEFAULT (datetime('now')),
    accessed_at INTEGER NOT NULL DEFAULT 0,
    file_path TEXT,
    graph_blast_radius INTEGER,
    palace_path TEXT,
    embedding BLOB,
    outcome_reward REAL,
    outcome_context TEXT,
    importance INTEGER,
    pinned INTEGER NOT NULL DEFAULT 0,
    superseded_by TEXT
)";

/// The full write-path record (H1): every field the canonical store path can
/// persist, including the S4 ACO-pheromone fields (reward/importance/pinning/
/// supersession). Construct with [`RichMemoryEntry::new`] and set the optional
/// fields you actually have — absent means NULL (an unobserved outcome is not
/// a failed one), except where the conflict semantics say otherwise.
#[derive(Debug, Clone, Default)]
pub struct RichMemoryEntry<'a> {
    /// Entry key — THE identity (canonical name, REGRA #17).
    pub key: &'a str,
    /// Tier attribute (free text: `local`/`semantic`/`working`/…).
    pub tier: &'a str,
    /// Entry content.
    pub value: &'a str,
    /// Type discriminator (`lesson`, `strategy`, …); caller-level default applies when None.
    pub entry_type: Option<&'a str>,
    /// Optional embedding vector (persisted as little-endian bytes).
    pub embedding: Option<&'a [f32]>,
    /// Source file path, when the entry is anchored to code.
    pub file_path: Option<&'a str>,
    /// Blast radius captured at store time.
    pub graph_blast_radius: Option<i64>,
    /// Palace hierarchy path (`PalaceHierarchy::to_storage`).
    pub palace_path: Option<&'a str>,
    /// Measured outcome in [-1, 1]; None = unobserved (stays NULL).
    pub outcome_reward: Option<f64>,
    /// Context in which the outcome was measured.
    pub outcome_context: Option<&'a str>,
    /// Curator importance in [1, 5]; sticky on conflict (a re-store without
    /// importance keeps the previous judgement).
    pub importance: Option<i64>,
    /// Pinned entries are immune to eviction/decay.
    pub pinned: bool,
    /// Key of the entry this one supersedes (the old entry is retired, not deleted).
    pub supersedes: Option<&'a str>,
    /// Explicit hashtags from the caller (`--tag`); applied with
    /// `TagSource::Explicit` after the automatic derivation.
    pub explicit_tags: &'a [String],
}

impl<'a> RichMemoryEntry<'a> {
    /// Minimal constructor: key + tier + value, everything else absent.
    pub fn new(key: &'a str, tier: &'a str, value: &'a str) -> Self {
        Self {
            key,
            tier,
            value,
            ..Self::default()
        }
    }
}

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
        // H1 (2026-08-12): canonical key-only PK. A legacy table built with
        // `PRIMARY KEY (key, tier)` is rebuilt in place (rows preserved) so
        // the unified write path can rely on `ON CONFLICT(key)`.
        self.migrate_composite_pk()?;
        self.conn.execute(MEMORY_ENTRIES_DDL, [])?;

        // Idempotent column migrations. Legacy DBs predate later columns, and
        // `CREATE TABLE IF NOT EXISTS` never backfills them, so each absent
        // column must be added explicitly. (The `embedding` gap in particular
        // floods `store_insight` with "no column named embedding" warns and
        // eventually crashes the daemon if left unmigrated.) Columns are added
        // before `ensure_indexes` so every index has its column present.
        //
        // SQLite forbids NOT NULL with a non-constant DEFAULT in ADD COLUMN, so
        // NOT NULL columns arrive with a constant default and the datetime
        // columns arrive NULLABLE + a backfill UPDATE — every writer sets the
        // values explicitly, so the constraint difference is inert (G2).
        //
        // The `outcome_*` pair is the `r` of a case `(s, a, r)` (Memento,
        // arXiv 2508.16153): nullable on purpose — an entry whose outcome was
        // never observed is NOT the same as one that scored zero (04/08/2026).
        const COLUMN_MIGRATIONS: &[(&str, &str)] = &[
            (
                "tier",
                "ALTER TABLE memory_entries ADD COLUMN tier TEXT NOT NULL DEFAULT 'local'",
            ),
            (
                "entry_type",
                "ALTER TABLE memory_entries ADD COLUMN entry_type TEXT NOT NULL DEFAULT 'insight'",
            ),
            (
                "access_count",
                "ALTER TABLE memory_entries ADD COLUMN access_count INTEGER NOT NULL DEFAULT 0",
            ),
            (
                "accessed_at",
                "ALTER TABLE memory_entries ADD COLUMN accessed_at INTEGER NOT NULL DEFAULT 0",
            ),
            (
                "last_accessed_at",
                "ALTER TABLE memory_entries ADD COLUMN last_accessed_at TEXT",
            ),
            (
                "created_at",
                "ALTER TABLE memory_entries ADD COLUMN created_at TEXT",
            ),
            (
                "embedding",
                "ALTER TABLE memory_entries ADD COLUMN embedding BLOB",
            ),
            (
                "file_path",
                "ALTER TABLE memory_entries ADD COLUMN file_path TEXT",
            ),
            (
                "graph_blast_radius",
                "ALTER TABLE memory_entries ADD COLUMN graph_blast_radius INTEGER",
            ),
            (
                "palace_path",
                "ALTER TABLE memory_entries ADD COLUMN palace_path TEXT",
            ),
            (
                "outcome_reward",
                "ALTER TABLE memory_entries ADD COLUMN outcome_reward REAL",
            ),
            (
                "outcome_context",
                "ALTER TABLE memory_entries ADD COLUMN outcome_context TEXT",
            ),
            (
                "importance",
                "ALTER TABLE memory_entries ADD COLUMN importance INTEGER",
            ),
            (
                "pinned",
                "ALTER TABLE memory_entries ADD COLUMN pinned INTEGER NOT NULL DEFAULT 0",
            ),
            (
                "superseded_by",
                "ALTER TABLE memory_entries ADD COLUMN superseded_by TEXT",
            ),
        ];
        for (column, ddl) in COLUMN_MIGRATIONS {
            if self.add_column_if_missing(column, ddl)? {
                // Backfills for the NULLABLE datetime pair (see note above).
                if *column == "last_accessed_at" {
                    self.conn.execute(
                        "UPDATE memory_entries SET last_accessed_at = created_at WHERE last_accessed_at IS NULL",
                        [],
                    )?;
                } else if *column == "created_at" {
                    self.conn.execute(
                        "UPDATE memory_entries SET created_at = datetime('now') WHERE created_at IS NULL",
                        [],
                    )?;
                }
            }
        }

        self.ensure_indexes()?;
        self.ensure_fts()?;
        self.ensure_tag_tables()?;
        Ok(())
    }

    /// Rebuilds a legacy `PRIMARY KEY (key, tier)` table into the canonical
    /// key-only shape, preserving every row (H1, 2026-08-12). No-op on
    /// canonical or absent tables. Returns whether a migration happened.
    ///
    /// Deterministic dedupe (REGRA #17): when the same key exists in several
    /// tiers, the most recently accessed row wins (ties broken by higher
    /// access_count), because the copy runs `INSERT OR REPLACE` over rows
    /// ordered oldest-first. Timestamps are converted to the canonical TEXT
    /// datetime shape; `last_accessed_at` derives from `accessed_at`.
    fn migrate_composite_pk(&self) -> Result<bool> {
        if !self.table_exists("memory_entries")? || !self.has_composite_pk()? {
            return Ok(false);
        }
        let extra_cols = self.existing_optional_cols()?;
        let extra_select = if extra_cols.is_empty() {
            String::new()
        } else {
            format!(", {}", extra_cols.join(", "))
        };

        let tx = self.conn.unchecked_transaction()?;
        tx.execute_batch("ALTER TABLE memory_entries RENAME TO memory_entries_pk_migration")?;
        tx.execute(MEMORY_ENTRIES_DDL, [])?;
        tx.execute_batch(&format!(
            "INSERT OR REPLACE INTO memory_entries
             (key, value, tier, entry_type, access_count, last_accessed_at, created_at, accessed_at{extra_select})
             SELECT key, value, tier, entry_type, access_count,
                    datetime(accessed_at, 'unixepoch'),
                    CASE WHEN typeof(created_at) = 'integer'
                         THEN datetime(created_at, 'unixepoch')
                         ELSE created_at END,
                    accessed_at{extra_select}
             FROM memory_entries_pk_migration
             ORDER BY accessed_at ASC, access_count ASC;
             DROP TABLE memory_entries_pk_migration;"
        ))?;
        tx.commit()?;
        tracing::info!(
            "memory_entries: legacy composite (key, tier) PK rebuilt into canonical key-only shape"
        );
        Ok(true)
    }

    /// Returns whether `memory_entries` carries the legacy composite
    /// `PRIMARY KEY (key, tier)` (both columns flagged in `pragma_table_info`).
    fn has_composite_pk(&self) -> Result<bool> {
        let mut stmt = self.conn.prepare(
            "SELECT name FROM pragma_table_info('memory_entries') WHERE pk > 0 ORDER BY pk",
        )?;
        let pk_cols: Vec<String> = stmt
            .query_map([], |row| row.get::<_, String>(0))?
            .collect::<std::result::Result<_, _>>()?;
        Ok(pk_cols.len() == 2 && pk_cols[0] == "key" && pk_cols[1] == "tier")
    }

    /// Lists the canonical optional columns a legacy table already carries
    /// (older add_column migrations may have added some of them).
    fn existing_optional_cols(&self) -> Result<Vec<&'static str>> {
        const OPTIONAL: &[&str] = &[
            "file_path",
            "graph_blast_radius",
            "palace_path",
            "embedding",
            "outcome_reward",
            "outcome_context",
            "importance",
            "pinned",
            "superseded_by",
        ];
        let mut present = Vec::new();
        for col in OPTIONAL {
            if self.column_exists(col)? {
                present.push(*col);
            }
        }
        Ok(present)
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
    /// Returns whether the column was actually added (drives backfills).
    fn add_column_if_missing(&self, column: &str, ddl: &str) -> Result<bool> {
        if !self.column_exists(column)? {
            self.conn.execute(ddl, [])?;
            return Ok(true);
        }
        Ok(false)
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

    /// Creates the hashtag-library tables and their FTS index, idempotently.
    ///
    /// Schema of the faceted memory library (bundle
    /// `docs/plans/2026-08-11-memory-hashtag-library`, decision D2):
    ///
    /// * `memory_tags` — the tag↔item bipartite graph (L1). One row per
    ///   (entry, tag); `facet`/`value` are the namespaced parts of
    ///   `#facet:value` (see `tags.rs`), `full_tag` the canonical render.
    ///   `source` records provenance (`explicit` > `code_sync` > `auto` >
    ///   `backfill`) so trust ordering survives merges.
    /// * `memory_links` — typed memory↔memory relations (L3; A-MEM link
    ///   generation). `id = {src}|{rel}|{dst}` is deterministic by
    ///   construction (REGRA #17): re-deriving the same edge never duplicates.
    /// * `tags_fts` — external-content FTS5 over `memory_tags`, same
    ///   sync-trigger pattern as `memories_fts`, so facet filters can fuse
    ///   exact tag matches with full-text ranking without a second store.
    ///
    /// The DDL itself lives in `tags::{TAG_TABLES_DDL, TAG_FTS_DDL}` — the
    /// RPC write path (`touring-hook-runtime`) consumes the same constants,
    /// so there is exactly one textual copy of the schema.
    fn ensure_tag_tables(&self) -> Result<()> {
        self.conn.execute_batch(tags::TAG_TABLES_DDL)?;
        self.conn.execute_batch(tags::TAG_FTS_DDL)?;
        // Backfill rows written before the FTS/triggers existed (legacy paths
        // or a crashed first run): the NOT IN makes this a no-op whenever the
        // triggers are already keeping the index in sync.
        self.conn.execute_batch(
            "INSERT INTO tags_fts(rowid, full_tag, facet, value)
             SELECT rowid, full_tag, facet, value FROM memory_tags
             WHERE rowid NOT IN (SELECT rowid FROM tags_fts);",
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

    /// The unified write path (H1, 2026-08-12): every store — RLM API or RPC
    /// handler — lands here. One INSERT, one conflict semantics, one schema.
    ///
    /// Conflict semantics on `ON CONFLICT(key)` (documented in
    /// `docs/plans/2026-08-12-followups-l3/strategy-…md`):
    /// - `created_at` is first-write-wins (a re-store never rewrites birth).
    /// - `access_count` increments by 1 per store (production RPC behavior).
    /// - `importance` is sticky: `COALESCE(new, existing)`.
    /// - `outcome_*`/embedding/graph/palace take the new value (NULL when the
    ///   caller has none — re-storing content without a measured outcome does
    ///   not inherit the old measurement).
    /// - `accessed_at` (epoch) and `last_accessed_at` (TEXT) both move to now.
    /// - `supersedes` retires the old entry (`superseded_by`), never deletes.
    /// - Automatic facet derivation + explicit tags apply after the row lands.
    pub fn store_rich(&self, entry: &RichMemoryEntry<'_>) -> Result<Vec<tags::IgnoredTag>> {
        let now = Utc::now().timestamp();
        let entry_type = entry.entry_type.unwrap_or("insight");
        let embedding_bytes: Option<Vec<u8>> = entry
            .embedding
            .map(|emb| emb.iter().flat_map(|f| f.to_le_bytes()).collect());

        self.conn.execute(
            "INSERT INTO memory_entries
             (key, tier, value, entry_type, created_at, accessed_at, last_accessed_at, access_count,
              embedding, file_path, graph_blast_radius, palace_path,
              outcome_reward, outcome_context, importance, pinned)
             VALUES (?1, ?2, ?3, ?4, datetime('now'), ?5, datetime('now'), 1,
                     ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
             ON CONFLICT(key) DO UPDATE SET
                value = excluded.value,
                tier = excluded.tier,
                entry_type = excluded.entry_type,
                accessed_at = excluded.accessed_at,
                last_accessed_at = excluded.last_accessed_at,
                access_count = memory_entries.access_count + 1,
                embedding = excluded.embedding,
                file_path = excluded.file_path,
                graph_blast_radius = excluded.graph_blast_radius,
                palace_path = excluded.palace_path,
                outcome_reward = excluded.outcome_reward,
                outcome_context = excluded.outcome_context,
                importance = COALESCE(excluded.importance, memory_entries.importance),
                pinned = excluded.pinned",
            params![
                entry.key,
                entry.tier,
                entry.value,
                entry_type,
                now,
                embedding_bytes,
                entry.file_path,
                entry.graph_blast_radius,
                entry.palace_path,
                entry.outcome_reward,
                entry.outcome_context,
                entry.importance,
                i64::from(entry.pinned),
            ],
        )?;

        // Retire the superseded entry: it stays in the table for audit and
        // stops surfacing in recall. Pointing at the NEW key (rather than
        // deleting) keeps the correction traceable to what it corrected.
        if let Some(old_key) = entry.supersedes {
            self.conn
                .execute(
                    "UPDATE memory_entries SET superseded_by = ?1 WHERE key = ?2 AND key != ?1",
                    params![entry.key, old_key],
                )
                .ok();
            // Contract P1.5 (2026-08-30): a supersede also RE-POINTS the old
            // node's edges at the successor — proven live before this fix:
            // links(new)=0, links(old)=1, so correcting a linked node silently
            // orphaned the correction. The deterministic id ({src}|{rel}|{dst})
            // must be recomputed, never UPDATEd in place (REGRA #17), and the
            // whole pass is fail-open: an edge problem never loses the store.
            // An edge BETWEEN old and new (e.g. a recorded succession) is kept
            // as-is — re-pointing it would fabricate a self-loop.
            if let Ok(edges) = tags::fetch_links(&self.conn, old_key) {
                for e in edges {
                    if e.src == entry.key || e.dst == entry.key {
                        continue;
                    }
                    let src = if e.src == old_key {
                        entry.key
                    } else {
                        e.src.as_str()
                    };
                    let dst = if e.dst == old_key {
                        entry.key
                    } else {
                        e.dst.as_str()
                    };
                    tags::upsert_link(&self.conn, src, e.rel, dst).ok();
                    self.conn
                        .execute("DELETE FROM memory_links WHERE id = ?1", params![e.id])
                        .ok();
                }
            }
        }

        self.auto_tag_entry(entry.key, entry_type, entry.file_path);
        // Contract P1 (2026-08-30): a rejected explicit tag no longer dies in
        // the daemon log — it returns to the caller so the store RESPONSE can
        // carry `ignored_facets` (both silent-death points elevated; the WARN
        // stays for operators tailing the log).
        let mut ignored = Vec::new();
        for raw in entry.explicit_tags {
            match tags::parse_tag(raw) {
                Ok(tag) => {
                    if let Err(e) = self.tag_entry(entry.key, &tag, tags::TagSource::Explicit) {
                        tracing::warn!(error = %e, key = entry.key, tag = raw, "explicit tag failed, continuing");
                        ignored.push(tags::IgnoredTag {
                            raw: (*raw).to_string(),
                            reason: format!("valid tag but persistence failed: {e}"),
                            suggestion: None,
                        });
                    }
                }
                Err(errs) => {
                    tracing::warn!(
                        key = entry.key,
                        tag = raw,
                        "invalid explicit tag skipped: {errs:?}"
                    );
                    ignored.push(tags::describe_violations(raw, &errs));
                }
            }
        }
        Ok(ignored)
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
        // H1 (2026-08-12): delegates to the unified write path.
        let palace_path = palace.to_storage();
        let mut entry = RichMemoryEntry::new(key, tier.as_str(), value);
        entry.entry_type = Some(entry_type);
        entry.palace_path = Some(&palace_path);
        self.store_rich(&entry).map(|_| ())
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
        // H1 (2026-08-12): delegates to the unified write path. The RLM-API
        // caller-level default for entry_type stays "text" (the RPC surface
        // defaults "insight"); storage semantics are identical for both.
        let mut entry = RichMemoryEntry::new(key, tier.as_str(), value);
        entry.entry_type = Some(entry_type.unwrap_or("text"));
        entry.embedding = embedding;
        if let Some(g) = graph {
            entry.file_path = g.file_path;
            entry.graph_blast_radius = g.blast_radius;
        }
        self.store_rich(&entry).map(|_| ())
    }

    /// Derives and persists the automatic facets of an entry (F1 auto-tagging
    /// of the hashtag library). Runs on every store path via `store_internal`.
    /// A tagging failure must never lose the memory itself: warn and continue,
    /// same discipline as the `PRAGMA optimize` fire-and-forget above.
    fn auto_tag_entry(&self, key: &str, entry_type: &str, file_path: Option<&str>) {
        for tag in tags::derive_tags(key, entry_type, file_path) {
            if let Err(e) = self.tag_entry(key, &tag, tags::TagSource::Auto) {
                tracing::warn!(error = %e, key, "auto-tag failed, continuing");
            }
        }
    }

    /// Attaches one parsed tag to an entry, idempotently (PK is
    /// `(entry_key, full_tag)`; a re-attach only refreshes provenance when the
    /// new source is more trustworthy — explicit > code_sync > auto > backfill).
    pub fn tag_entry(
        &self,
        key: &str,
        tag: &tags::ParsedTag,
        source: tags::TagSource,
    ) -> Result<()> {
        tags::upsert_tag(&self.conn, key, tag, source)?;
        Ok(())
    }

    /// Lists every tag attached to an entry, in citation order of the facets.
    pub fn tags_of(&self, key: &str) -> Result<Vec<tags::ParsedTag>> {
        Ok(tags::fetch_tags(&self.conn, key)?)
    }

    /// Detaches one tag from an entry (the code-sync tombstone path: a codetag
    /// removed from the source is removed here, not hard-deleted elsewhere).
    /// Returns the number of rows removed.
    pub fn remove_tag(&self, key: &str, full_tag: &str) -> Result<usize> {
        Ok(tags::delete_tag(&self.conn, key, full_tag)?)
    }

    /// Returns entries carrying ALL the required tags (conjunctive facet
    /// filter — the L1 bipartite lookup of the hashtag library), most recently
    /// accessed first. Text ranking fuses on top in F3; this is the exact
    /// half of the hybrid.
    pub fn query_tags(
        &self,
        required: &[tags::ParsedTag],
        limit: usize,
    ) -> Result<Vec<MemoryMatch>> {
        let keys = tags::entry_keys_with_all_tags(&self.conn, required, limit)?;
        let mut out = Vec::with_capacity(keys.len());
        for key in keys {
            let row = self.conn.query_row(
                "SELECT tier, value, entry_type, access_count, created_at, accessed_at
                 FROM memory_entries WHERE key = ?1",
                params![key],
                |row| {
                    Ok(MemoryMatch {
                        key: key.clone(),
                        tier: row.get(0)?,
                        value: row.get(1)?,
                        entry_type: row.get(2)?,
                        access_count: row.get(3)?,
                        created_at: Self::cell_to_epoch(row.get_ref(4)?),
                        accessed_at: Self::cell_to_epoch(row.get_ref(5)?),
                        score: 1.0,
                    })
                },
            );
            if let Ok(m) = row {
                out.push(m);
            }
        }
        Ok(out)
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
                    created_at: Self::cell_to_epoch(row.get_ref(4)?),
                    accessed_at: Self::cell_to_epoch(row.get_ref(5)?),
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
             AND (?2 - CASE WHEN typeof(created_at) = 'integer' THEN created_at ELSE CAST(strftime('%s', created_at) AS INTEGER) END) <= ?3",
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
             AND (?2 - CASE WHEN typeof(created_at) = 'integer' THEN created_at ELSE CAST(strftime('%s', created_at) AS INTEGER) END) <= ?3",
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
             AND (?1 - CASE WHEN typeof(created_at) = 'integer' THEN created_at ELSE CAST(strftime('%s', created_at) AS INTEGER) END) > ?2",
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

    /// Full row snapshot for parity assertions (all 16 canonical columns).
    #[derive(Debug, PartialEq)]
    struct RowSnapshot {
        key: String,
        value: String,
        tier: String,
        entry_type: String,
        access_count: i64,
        last_accessed_at: Option<String>,
        created_at: Option<String>,
        accessed_at: i64,
        file_path: Option<String>,
        graph_blast_radius: Option<i64>,
        palace_path: Option<String>,
        embedding: Option<Vec<u8>>,
        outcome_reward: Option<f64>,
        outcome_context: Option<String>,
        importance: Option<i64>,
        pinned: i64,
        superseded_by: Option<String>,
    }

    fn snapshot(conn: &Connection, key: &str) -> RowSnapshot {
        conn.query_row(
            "SELECT key, value, tier, entry_type, access_count, last_accessed_at, created_at,
                    accessed_at, file_path, graph_blast_radius, palace_path, embedding,
                    outcome_reward, outcome_context, importance, pinned, superseded_by
             FROM memory_entries WHERE key = ?1",
            params![key],
            |row| {
                Ok(RowSnapshot {
                    key: row.get(0)?,
                    value: row.get(1)?,
                    tier: row.get(2)?,
                    entry_type: row.get(3)?,
                    access_count: row.get(4)?,
                    last_accessed_at: row.get(5)?,
                    created_at: row.get(6)?,
                    accessed_at: row.get(7)?,
                    file_path: row.get(8)?,
                    graph_blast_radius: row.get(9)?,
                    palace_path: row.get(10)?,
                    embedding: row.get(11)?,
                    outcome_reward: row.get(12)?,
                    outcome_context: row.get(13)?,
                    importance: row.get(14)?,
                    pinned: row.get(15)?,
                    superseded_by: row.get(16)?,
                })
            },
        )
        .expect("snapshot row")
    }

    #[test]
    fn store_rich_first_insert_has_canonical_shape() {
        let dir = TempDir::new().unwrap();
        let mem = RlmMemory::new(&dir.path().join("m.db")).unwrap();
        let mut entry = RichMemoryEntry::new("k1", "semantic", "the lesson");
        entry.entry_type = Some("lesson");
        entry.importance = Some(4);
        entry.pinned = true;
        mem.store_rich(&entry).unwrap();

        let row = snapshot(&mem.conn, "k1");
        assert_eq!(row.key, "k1");
        assert_eq!(row.tier, "semantic");
        assert_eq!(row.entry_type, "lesson");
        assert_eq!(row.access_count, 1, "first store counts one access");
        assert_eq!(row.importance, Some(4));
        assert_eq!(row.pinned, 1);
        // Canonical timestamp shapes: TEXT datetimes + INTEGER epoch.
        assert!(
            row.created_at.as_deref().unwrap().contains('-'),
            "created_at must be TEXT datetime, got {:?}",
            row.created_at
        );
        assert!(row.last_accessed_at.as_deref().unwrap().contains('-'));
        assert!(row.accessed_at > 1_000_000_000, "accessed_at is epoch");
        assert!(
            row.outcome_reward.is_none(),
            "unobserved outcome stays NULL"
        );
    }

    #[test]
    fn store_rich_conflict_semantics_are_pinned() {
        let dir = TempDir::new().unwrap();
        let mem = RlmMemory::new(&dir.path().join("m.db")).unwrap();
        let mut first = RichMemoryEntry::new("k", "local", "v1");
        first.importance = Some(5);
        first.outcome_reward = Some(0.7);
        mem.store_rich(&first).unwrap();
        let born = snapshot(&mem.conn, "k").created_at;

        // Re-store with new content and NO importance/outcome.
        let second = RichMemoryEntry::new("k", "semantic", "v2");
        mem.store_rich(&second).unwrap();
        let row = snapshot(&mem.conn, "k");
        assert_eq!(row.value, "v2");
        assert_eq!(row.tier, "semantic", "tier is an attribute: latest wins");
        assert_eq!(row.access_count, 2, "re-store increments");
        assert_eq!(row.created_at, born, "created_at is first-write-wins");
        assert_eq!(row.importance, Some(5), "importance is sticky");
        assert!(
            row.outcome_reward.is_none(),
            "outcome is per-write: re-store without measurement resets to NULL"
        );

        // An explicit new importance overrides the sticky one.
        let mut third = RichMemoryEntry::new("k", "semantic", "v3");
        third.importance = Some(2);
        mem.store_rich(&third).unwrap();
        assert_eq!(snapshot(&mem.conn, "k").importance, Some(2));
    }

    #[test]
    fn store_rich_supersedes_retires_old_entry() {
        let dir = TempDir::new().unwrap();
        let mem = RlmMemory::new(&dir.path().join("m.db")).unwrap();
        mem.store_rich(&RichMemoryEntry::new("old", "local", "stale"))
            .unwrap();
        let mut new_entry = RichMemoryEntry::new("new", "local", "corrected");
        new_entry.supersedes = Some("old");
        mem.store_rich(&new_entry).unwrap();
        assert_eq!(
            snapshot(&mem.conn, "old").superseded_by.as_deref(),
            Some("new"),
            "old entry retired, not deleted"
        );
        assert!(snapshot(&mem.conn, "new").superseded_by.is_none());
    }

    #[test]
    fn store_rich_reports_ignored_facets_in_the_return_value() {
        // Mutation killed: reverting store_rich to Result<()> / dropping the
        // collection — the silent-death point the P1 contract closes.
        let dir = TempDir::new().unwrap();
        let mem = RlmMemory::new(&dir.path().join("m.db")).unwrap();
        let mut entry = RichMemoryEntry::new("k", "local", "v");
        let raw_tags = vec![
            "#kind:lesson".to_string(),
            "#classe:prova".to_string(),
            "#kynd:x".to_string(),
        ];
        entry.explicit_tags = &raw_tags;
        let ignored = mem.store_rich(&entry).unwrap();
        assert_eq!(ignored.len(), 2, "one valid tag, two rejected");
        assert_eq!(ignored[0].raw, "#classe:prova");
        assert!(
            ignored[0]
                .reason
                .contains("kind, purpose, lang, domain, process, artifact, status"),
            "the reason must TEACH the canonical vocabulary: {}",
            ignored[0].reason
        );
        assert_eq!(
            ignored[1].suggestion,
            Some("kind"),
            "near-miss suggests the canonical facet"
        );
        // The accepted tag really persisted (the report is additive, not a veto).
        // full_tag, not facet: auto-tagging also derives a kind:* row from
        // the entry_type, so counting by facet would see both.
        let tagged: i64 = mem
            .conn
            .query_row(
                "SELECT COUNT(*) FROM memory_tags WHERE entry_key = 'k' AND full_tag = 'kind:lesson'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(tagged, 1);
    }

    #[test]
    fn store_rich_supersede_repoints_the_old_nodes_edges() {
        // Mutation killed: removing the P1.5 re-point block — proven live
        // before the fix: links(new)=0, links(old)=1, a linked node's
        // correction silently orphaned.
        let dir = TempDir::new().unwrap();
        let mem = RlmMemory::new(&dir.path().join("m.db")).unwrap();
        mem.store_rich(&RichMemoryEntry::new("old", "local", "v1"))
            .unwrap();
        mem.store_rich(&RichMemoryEntry::new("other", "local", "peer"))
            .unwrap();
        tags::upsert_link(&mem.conn, "old", tags::LinkRel::RelatesTo, "other").unwrap();
        // An edge BETWEEN old and its successor must be kept, never turned
        // into a self-loop by the re-point.
        tags::upsert_link(&mem.conn, "new", tags::LinkRel::Supersedes, "old").unwrap();

        let mut new_entry = RichMemoryEntry::new("new", "local", "v2");
        new_entry.supersedes = Some("old");
        mem.store_rich(&new_entry).unwrap();

        let new_links = tags::fetch_links(&mem.conn, "new").unwrap();
        assert!(
            new_links.iter().any(|e| e.src == "new" && e.dst == "other"),
            "the old node's edge was re-pointed at the successor: {new_links:?}"
        );
        assert!(
            new_links.iter().any(|e| e.src == "new" && e.dst == "old"),
            "the succession edge survives untouched"
        );
        assert!(
            new_links.iter().all(|e| e.src != e.dst),
            "no self-loop fabricated"
        );
        let old_links = tags::fetch_links(&mem.conn, "old").unwrap();
        assert_eq!(
            old_links.len(),
            1,
            "old keeps only the succession edge: {old_links:?}"
        );
    }

    #[test]
    fn store_rich_parity_with_legacy_rpc_insert() {
        // G6 parity: a first insert through store_rich produces the SAME row
        // the legacy RPC INSERT OR REPLACE produced (the production-dominant
        // writer, 29k rows across 4 projects). The reference SQL below is the
        // pre-unification handler statement, kept here as the contract.
        let dir = TempDir::new().unwrap();
        let legacy_db = dir.path().join("legacy.db");
        let unified_db = dir.path().join("unified.db");

        let legacy = Connection::open(&legacy_db).unwrap();
        legacy.execute(MEMORY_ENTRIES_DDL, []).unwrap();
        legacy
            .execute(
                "INSERT OR REPLACE INTO memory_entries (key, value, tier, entry_type, access_count, last_accessed_at, outcome_reward, outcome_context, importance, pinned)
                 VALUES (?1, ?2, ?3, ?4, COALESCE((SELECT access_count FROM memory_entries WHERE key = ?1), 0) + 1, datetime('now'), ?5, ?6,
                         COALESCE(?7, (SELECT importance FROM memory_entries WHERE key = ?1)), ?8)",
                params!["k", "v", "semantic", "lesson", 0.5f64, "ctx", 3i64, 1i64],
            )
            .unwrap();

        let mem = RlmMemory::new(&unified_db).unwrap();
        let mut entry = RichMemoryEntry::new("k", "semantic", "v");
        entry.entry_type = Some("lesson");
        entry.outcome_reward = Some(0.5);
        entry.outcome_context = Some("ctx");
        entry.importance = Some(3);
        entry.pinned = true;
        mem.store_rich(&entry).unwrap();

        let a = snapshot(&legacy, "k");
        let b = snapshot(&mem.conn, "k");
        // Field-by-field parity on everything the legacy writer set; the
        // columns it never wrote take the canonical defaults in both.
        assert_eq!(a.key, b.key);
        assert_eq!(a.value, b.value);
        assert_eq!(a.tier, b.tier);
        assert_eq!(a.entry_type, b.entry_type);
        assert_eq!(a.access_count, b.access_count);
        assert_eq!(a.outcome_reward, b.outcome_reward);
        assert_eq!(a.outcome_context, b.outcome_context);
        assert_eq!(a.importance, b.importance);
        assert_eq!(a.pinned, b.pinned);
        assert!(a.created_at.is_some() && b.created_at.is_some());
        assert_eq!(a.file_path, b.file_path);
        assert_eq!(a.embedding, b.embedding);
        assert_eq!(a.superseded_by, b.superseded_by);
    }

    #[test]
    fn composite_pk_db_is_rebuilt_rows_preserved() {
        let dir = TempDir::new().unwrap();
        let db = dir.path().join("legacy.db");
        {
            let conn = Connection::open(&db).unwrap();
            conn.execute_batch(
                "CREATE TABLE memory_entries (
                    key TEXT NOT NULL, tier TEXT NOT NULL, value TEXT NOT NULL,
                    entry_type TEXT NOT NULL, created_at INTEGER NOT NULL,
                    accessed_at INTEGER NOT NULL, access_count INTEGER NOT NULL,
                    embedding BLOB, PRIMARY KEY (key, tier)
                );
                INSERT INTO memory_entries VALUES ('dup', 'local', 'old-val', 'text', 1700000000, 1700000000, 2, NULL);
                INSERT INTO memory_entries VALUES ('dup', 'semantic', 'new-val', 'lesson', 1700000100, 1700000100, 5, NULL);
                INSERT INTO memory_entries VALUES ('solo', 'working', 'solo-val', 'text', 1700000200, 1700000200, 1, NULL);",
            )
            .unwrap();
        }
        let mem = RlmMemory::new(&db).unwrap();

        // PK is now key-only.
        let pk_count: i64 = mem
            .conn
            .query_row(
                "SELECT COUNT(*) FROM pragma_table_info('memory_entries') WHERE name = 'key' AND pk = 1",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(pk_count, 1, "key is the sole PK column");

        // Dedupe: latest 'dup' row wins (highest accessed_at), 'solo' intact.
        let dup = snapshot(&mem.conn, "dup");
        assert_eq!(dup.value, "new-val");
        assert_eq!(dup.access_count, 5);
        assert_eq!(
            dup.created_at.as_deref(),
            Some("2023-11-14 22:15:00"),
            "epoch created_at converted to TEXT datetime"
        );
        assert_eq!(dup.last_accessed_at.as_deref(), Some("2023-11-14 22:15:00"));
        assert_eq!(snapshot(&mem.conn, "solo").value, "solo-val");
        let total: i64 = mem
            .conn
            .query_row("SELECT COUNT(*) FROM memory_entries", [], |r| r.get(0))
            .unwrap();
        assert_eq!(total, 2, "two distinct keys survive the dedupe");

        // And the unified write path works on the migrated table.
        mem.store_rich(&RichMemoryEntry::new("dup", "local", "v3"))
            .unwrap();
        assert_eq!(snapshot(&mem.conn, "dup").access_count, 6);
    }

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

    #[test]
    fn test_tag_tables_created_and_idempotent() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test_tags.db");
        // Second open re-runs ensure_schema over the same DB: the migration
        // must be a no-op (CREATE … IF NOT EXISTS + table_exists guards).
        let memory = RlmMemory::new(&db_path).unwrap();
        drop(memory);
        let memory = RlmMemory::new(&db_path).unwrap();

        for table in ["memory_tags", "memory_links", "tags_fts"] {
            assert!(
                memory.table_exists(table).unwrap(),
                "expected table {table} after ensure_tag_tables"
            );
        }

        // The bipartite tag edge round-trips and the FTS trigger keeps
        // tags_fts in sync without any code path opting in.
        memory
            .conn
            .execute(
                "INSERT INTO memory_tags(entry_key, facet, value, full_tag, source)
                 VALUES ('mem:x', 'kind', 'snippet', 'kind:snippet', 'explicit')",
                [],
            )
            .unwrap();
        let hits: i64 = memory
            .conn
            .query_row(
                "SELECT COUNT(*) FROM tags_fts WHERE tags_fts MATCH '\"kind:snippet\"'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(hits, 1, "tags_fts trigger should have indexed the row");

        // Deterministic link id (REGRA #17): re-inserting the same edge is a
        // PK conflict, proving dedupe falls out of the schema itself.
        memory
            .conn
            .execute(
                "INSERT INTO memory_links(id, src, dst, rel)
                 VALUES ('mem:x|extends|mem:y', 'mem:x', 'mem:y', 'extends')",
                [],
            )
            .unwrap();
        let dup = memory.conn.execute(
            "INSERT INTO memory_links(id, src, dst, rel)
             VALUES ('mem:x|extends|mem:y', 'mem:x', 'mem:y', 'extends')",
            [],
        );
        assert!(
            dup.is_err(),
            "duplicate deterministic link id must conflict"
        );
    }

    #[test]
    fn test_store_auto_tags_from_file_path() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test_autotag.db");
        let memory = RlmMemory::new(&db_path).unwrap();

        let graph = GraphMeta {
            file_path: Some("crates/touring-intelligence/src/rl/memory/rlm.rs"),
            blast_radius: Some(3),
        };
        memory
            .store_with_file_path(
                "lesson:rlm:schema",
                MemoryTier::Working,
                "schema lessons",
                Some("lesson"),
                None,
                &graph,
            )
            .unwrap();

        let tags = memory.tags_of("lesson:rlm:schema").unwrap();
        let full: Vec<&str> = tags.iter().map(|t| t.full_tag.as_str()).collect();
        assert!(full.contains(&"lang:rust"), "lang from .rs, got {full:?}");
        assert!(full.contains(&"kind:lesson"), "kind from entry_type");
        assert!(full.contains(&"domain:memory"), "domain from path segments");
        assert!(full.contains(&"status:stable"), "default maturity");
    }

    #[test]
    fn test_explicit_tag_overrides_auto_source() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test_override.db");
        let memory = RlmMemory::new(&db_path).unwrap();

        memory
            .store("k", MemoryTier::Working, "v", None, None)
            .unwrap();
        let tag = tags::parse_tag("#purpose:map-rendering").unwrap();
        memory.tag_entry("k", &tag, tags::TagSource::Auto).unwrap();
        memory
            .tag_entry("k", &tag, tags::TagSource::Explicit)
            .unwrap();
        let source: String = memory
            .conn
            .query_row(
                "SELECT source FROM memory_tags WHERE entry_key='k' AND full_tag='purpose:map-rendering'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(source, "explicit", "higher-trust source must win");
    }

    #[test]
    fn test_query_tags_is_conjunctive() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test_conj.db");
        let memory = RlmMemory::new(&db_path).unwrap();

        for (key, et) in [("a.rs-lesson", "lesson"), ("b.md-note", "doc")] {
            memory
                .store(key, MemoryTier::Working, "v", Some(et), None)
                .unwrap();
        }
        let rust = tags::parse_tag("#lang:rust").unwrap();
        let lesson = tags::parse_tag("#kind:lesson").unwrap();
        memory
            .tag_entry("a.rs-lesson", &rust, tags::TagSource::Explicit)
            .unwrap();
        memory
            .tag_entry("a.rs-lesson", &lesson, tags::TagSource::Explicit)
            .unwrap();
        memory
            .tag_entry("b.md-note", &rust, tags::TagSource::Explicit)
            .unwrap();

        let both = memory
            .query_tags(&[rust.clone(), lesson.clone()], 10)
            .unwrap();
        assert_eq!(both.len(), 1, "only the entry with BOTH tags matches");
        assert_eq!(both[0].key, "a.rs-lesson");

        let any_rust = memory.query_tags(&[rust], 10).unwrap();
        assert_eq!(any_rust.len(), 2);
    }

    #[test]
    fn test_remove_tag_tombstones() {
        let temp_dir = TempDir::new().unwrap();
        let db_path = temp_dir.path().join("test_remove.db");
        let memory = RlmMemory::new(&db_path).unwrap();

        memory
            .store("k", MemoryTier::Working, "v", Some("lesson"), None)
            .unwrap();
        assert!(!memory.tags_of("k").unwrap().is_empty());
        let removed = memory.remove_tag("k", "kind:lesson").unwrap();
        assert_eq!(removed, 1);
        assert!(
            memory
                .tags_of("k")
                .unwrap()
                .iter()
                .all(|t| t.full_tag != "kind:lesson"),
            "removed tag must be gone"
        );
        // And the FTS trigger dropped it too (tombstone propagates).
        let fts_hits: i64 = memory
            .conn
            .query_row(
                "SELECT COUNT(*) FROM tags_fts WHERE tags_fts MATCH '\"kind:lesson\"'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(fts_hits, 0);
    }
}
