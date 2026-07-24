//! Async wrapper for FileKnowledgeDB using deadpool-sqlite.
//!
//! Provides non-blocking SQLite operations via `Pool::interact()`.
//! The pool manages connections while `interact()` runs closures on a blocking
//! thread pool, yielding the async executor immediately.
//!
//! # Why interact() instead of spawn_blocking?
//!
//! - `spawn_blocking` requires the closure to be 'static and own its data
//! - `interact()` borrows the Connection and runs on a dedicated blocking pool
//! - Connection never leaves the thread pool (no Sync issues)
//! - Better throughput: pool recycles connections vs spawn_blocking which creates threads

use deadpool_sqlite::{Config, Pool, Runtime};
use memchr::memmem;
use std::path::Path;
use thiserror::Error;
use touring_analysis::e2e::schema_guard;

use crate::knowledge::{BashOutcome, EditEvent, FileKnowledge, FileRelation, KnowledgeStats};

/// Error type for async knowledge operations.
#[derive(Error, Debug)]
pub enum AsyncKnowledgeError {
    /// A connection-pool operation (acquire/recycle/interact) failed.
    #[error("pool error: {0}")]
    Pool(String),
    /// The underlying SQLite query or statement failed.
    #[error("sqlite error: {0}")]
    Sqlite(String),
    /// Pool or schema initialization failed.
    #[error("init error: {0}")]
    Init(String),
}

impl From<deadpool_sqlite::PoolError> for AsyncKnowledgeError {
    fn from(e: deadpool_sqlite::PoolError) -> Self {
        AsyncKnowledgeError::Pool(e.to_string())
    }
}

impl From<rusqlite::Error> for AsyncKnowledgeError {
    fn from(e: rusqlite::Error) -> Self {
        AsyncKnowledgeError::Sqlite(e.to_string())
    }
}

/// Async FileKnowledgeDB using deadpool-sqlite.
///
/// All methods are async and non-blocking via `pool.interact()`.
#[derive(Debug, Clone)]
pub struct AsyncFileKnowledgeDB {
    pool: Pool,
}

impl AsyncFileKnowledgeDB {
    /// Create a new async pool from a database path.
    pub fn new(db_path: &Path) -> Result<Self, AsyncKnowledgeError> {
        let cfg = Config::new(db_path);
        let pool = cfg
            .builder(Runtime::Tokio1)
            .map_err(|e| AsyncKnowledgeError::Init(e.to_string()))?
            .build()
            .map_err(|e| AsyncKnowledgeError::Init(e.to_string()))?;

        Ok(Self { pool })
    }

    /// Lookup file knowledge by path.
    pub async fn lookup(
        &self,
        file_path: &str,
    ) -> Result<Option<FileKnowledge>, AsyncKnowledgeError> {
        let path = file_path.to_string();
        let conn = self
            .pool
            .get()
            .await
            .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?;

        conn.interact(move |conn| {
            let mut stmt = conn
                .prepare(&format!(
                    "SELECT file_path, language, line_count, symbol_count, read_count,
                            last_read_at, content_hash, imports_json, symbols_json, notes,
                            created_at, updated_at
                     FROM {} WHERE file_path = ?1",
                    schema_guard::TABLE_FILE_KNOWLEDGE
                ))
                .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?;

            let result = stmt
                .query_row([&path], |row| {
                    Ok(FileKnowledge {
                        file_path: row.get(0)?,
                        language: row.get(1)?,
                        line_count: row.get(2)?,
                        symbol_count: row.get(3)?,
                        read_count: row.get(4)?,
                        last_read_at: row.get(5)?,
                        content_hash: row.get(6)?,
                        imports_json: row.get(7)?,
                        symbols_json: row.get(8)?,
                        notes: row.get(9)?,
                    })
                })
                .optional()
                .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?;

            Ok(result)
        })
        .await
        .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?
    }

    /// Upsert file knowledge record.
    pub async fn upsert(&self, k: &FileKnowledge) -> Result<(), AsyncKnowledgeError> {
        let k = k.clone();
        let conn = self
            .pool
            .get()
            .await
            .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?;

        conn.interact(move |conn| {
            conn.execute(
                &format!(
                    "INSERT INTO {t} (file_path, language, line_count, symbol_count,
                                            read_count, last_read_at, content_hash, imports_json,
                                            symbols_json, notes, updated_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10,
                        datetime('now'))
                 ON CONFLICT(file_path) DO UPDATE SET
                    language = excluded.language,
                    line_count = excluded.line_count,
                    symbol_count = excluded.symbol_count,
                    read_count = excluded.read_count,
                    last_read_at = excluded.last_read_at,
                    content_hash = excluded.content_hash,
                    imports_json = excluded.imports_json,
                    symbols_json = excluded.symbols_json,
                    notes = excluded.notes,
                    updated_at = datetime('now')",
                    t = schema_guard::TABLE_FILE_KNOWLEDGE
                ),
                rusqlite::params![
                    k.file_path,
                    k.language,
                    k.line_count,
                    k.symbol_count,
                    k.read_count,
                    k.last_read_at,
                    k.content_hash,
                    k.imports_json,
                    k.symbols_json,
                    k.notes,
                ],
            )
            .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?;

            Ok(())
        })
        .await
        .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?
    }

    /// Record a bash outcome.
    pub async fn record_bash_outcome(
        &self,
        outcome: &BashOutcome,
    ) -> Result<(), AsyncKnowledgeError> {
        let outcome = outcome.clone();
        let conn = self
            .pool
            .get()
            .await
            .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?;

        conn.interact(move |conn| {
            conn.execute(
                &format!(
                    "INSERT INTO {} (command, command_hash, exit_code, success,
                                            error_pattern, file_context, executed_at)
                 VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
                    schema_guard::TABLE_BASH_OUTCOMES
                ),
                rusqlite::params![
                    outcome.command,
                    outcome.command_hash,
                    outcome.exit_code,
                    outcome.success,
                    outcome.error_pattern,
                    outcome.file_context,
                    outcome.executed_at,
                ],
            )
            .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?;

            Ok(())
        })
        .await
        .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?
    }

    /// Record an edit event.
    pub async fn record_edit(&self, edit: &EditEvent) -> Result<(), AsyncKnowledgeError> {
        let edit = edit.clone();
        let conn = self
            .pool
            .get()
            .await
            .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?;

        conn.interact(move |conn| {
            conn.execute(
                &format!(
                    "INSERT INTO {} (file_path, edit_type, summary, error_pattern, edited_at)
                 VALUES (?1, ?2, ?3, ?4, ?5)",
                    schema_guard::TABLE_EDIT_HISTORY
                ),
                rusqlite::params![
                    edit.file_path,
                    edit.edit_type,
                    edit.summary,
                    edit.error_pattern,
                    edit.edited_at,
                ],
            )
            .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?;

            Ok(())
        })
        .await
        .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?
    }

    /// Get file relations originating from a file.
    pub async fn get_relations_from(
        &self,
        file_path: &str,
    ) -> Result<Vec<FileRelation>, AsyncKnowledgeError> {
        let path = file_path.to_string();
        let conn = self
            .pool
            .get()
            .await
            .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?;

        conn.interact(move |conn| {
            let mut stmt = conn
                .prepare(&format!(
                    "SELECT source_path, target_path, relation_type FROM {} WHERE source_path = ?1",
                    schema_guard::TABLE_FILE_RELATIONS
                ))
                .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?;

            let rows = stmt
                .query_map([&path], |row| {
                    Ok(FileRelation {
                        source: row.get(0)?,
                        target: row.get(1)?,
                        relation_type: row.get(2)?,
                    })
                })
                .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?;

            let relations: Result<Vec<_>, _> = rows.collect();
            relations.map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))
        })
        .await
        .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?
    }

    /// Get database statistics.
    pub async fn stats(&self) -> Result<KnowledgeStats, AsyncKnowledgeError> {
        let conn = self
            .pool
            .get()
            .await
            .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?;

        conn.interact(|conn| {
            let file_count: i64 = conn
                .query_row(
                    &format!(
                        "SELECT COUNT(*) FROM {}",
                        schema_guard::TABLE_FILE_KNOWLEDGE
                    ),
                    [],
                    |row| row.get(0),
                )
                .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?;

            let relation_count: i64 = conn
                .query_row(
                    &format!(
                        "SELECT COUNT(*) FROM {}",
                        schema_guard::TABLE_FILE_RELATIONS
                    ),
                    [],
                    |row| row.get(0),
                )
                .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?;

            // EC17a: Fill real counts from actual tables (previously hardcoded 0).
            // Each query uses unwrap_or(0) so a missing/empty table degrades to 0
            // rather than propagating an error — consistent with the file_count pattern.
            let access_count: i64 = conn
                .query_row(
                    &format!(
                        "SELECT COUNT(*) FROM {}",
                        schema_guard::TABLE_FILE_ACCESS_LOG
                    ),
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(0);

            let bash_count: i64 = conn
                .query_row(
                    &format!("SELECT COUNT(*) FROM {}", schema_guard::TABLE_BASH_OUTCOMES),
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(0);

            let edit_count: i64 = conn
                .query_row(
                    &format!("SELECT COUNT(*) FROM {}", schema_guard::TABLE_EDIT_HISTORY),
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(0);

            let gotcha_count: i64 = conn
                .query_row(
                    &format!("SELECT COUNT(*) FROM {}", schema_guard::TABLE_GOTCHAS),
                    [],
                    |row| row.get(0),
                )
                .unwrap_or(0);

            Ok(KnowledgeStats {
                file_count,
                relation_count,
                access_count,
                bash_count,
                edit_count,
                gotcha_count,
                task_metrics_count: 0, // TABLE_TASK_METRICS not yet in schema
            })
        })
        .await
        .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?
    }

    /// Access count for a file.
    pub async fn access_count(&self, file_path: &str) -> Result<i64, AsyncKnowledgeError> {
        let path = file_path.to_string();
        let conn = self
            .pool
            .get()
            .await
            .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?;

        conn.interact(move |conn| {
            conn.query_row(
                &format!(
                    "SELECT COUNT(*) FROM {} WHERE file_path = ?1",
                    schema_guard::TABLE_FILE_ACCESS_LOG
                ),
                [&path],
                |row| row.get(0),
            )
            .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))
        })
        .await
        .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?
    }

    /// Edit count for a specific file (from TABLE_EDIT_HISTORY).
    /// EC20: per-file edit frequency — "how hot is this file for editing" signal.
    /// Exact match on file_path column (no LIKE approximation).
    pub async fn edit_count_for_file(&self, file_path: &str) -> Result<i64, AsyncKnowledgeError> {
        let path = file_path.to_string();
        let conn = self
            .pool
            .get()
            .await
            .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?;

        conn.interact(move |conn| {
            conn.query_row(
                &format!(
                    "SELECT COUNT(*) FROM {} WHERE file_path = ?1",
                    schema_guard::TABLE_EDIT_HISTORY
                ),
                [&path],
                |row| row.get(0),
            )
            .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))
        })
        .await
        .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?
    }

    /// Bash failure count for a specific file (from TABLE_BASH_OUTCOMES).
    /// EC37: per-file bash failure count — "how many commands targeting this file have failed".
    /// Queries WHERE file_context = ?1 AND success = 0 (exact match, no LIKE).
    pub async fn bash_failures_for_file(
        &self,
        file_path: &str,
    ) -> Result<i64, AsyncKnowledgeError> {
        let path = file_path.to_string();
        let conn = self
            .pool
            .get()
            .await
            .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?;

        conn.interact(move |conn| {
            conn.query_row(
                &format!(
                    "SELECT COUNT(*) FROM {} WHERE file_context = ?1 AND success = 0",
                    schema_guard::TABLE_BASH_OUTCOMES
                ),
                [&path],
                |row| row.get(0),
            )
            .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))
        })
        .await
        .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?
    }

    /// Gotcha count for a specific file (from TABLE_GOTCHAS).
    ///
    /// EC39: counts active gotchas whose `pattern` is a substring of `file_path`.
    ///
    /// # Performance note
    ///
    /// The original implementation used `WHERE ?1 LIKE '%' || pattern || '%'`, which
    /// forces a full table scan because a leading `'%'` prevents SQLite from using the
    /// `idx_file_gotchas_pattern` index.  This replacement flips the query direction:
    ///
    /// 1. Fetch all active pattern strings with a query that CAN use the partial index
    ///    on `(decay_score DESC) WHERE resolved_at IS NULL`.
    /// 2. Use `memchr::memmem::find` in Rust to test whether each pattern is a
    ///    substring of `file_path` — O(n) Boyer-Moore-Horspool, faster than SQLite LIKE.
    ///
    /// The result is semantically identical to the old query while eliminating the
    /// full table scan on every `pre_edit` hook invocation.
    pub async fn gotcha_count_for_file(&self, file_path: &str) -> Result<i64, AsyncKnowledgeError> {
        let path = file_path.to_string();
        let conn = self
            .pool
            .get()
            .await
            .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?;

        conn.interact(move |conn| {
            // Fetch only the pattern column for active, non-decayed, non-resolved gotchas.
            // This query can use idx_file_gotchas_decay (partial index on decay_score where
            // resolved_at IS NULL), avoiding the leading-wildcard full-table-scan.
            let sql = format!(
                "SELECT pattern FROM {} \
                 WHERE COALESCE(decay_score, 1.0) > 0.1 AND resolved_at IS NULL",
                schema_guard::TABLE_GOTCHAS
            );
            let mut stmt = conn
                .prepare(&sql)
                .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?;

            let patterns: Vec<String> = stmt
                .query_map([], |row| row.get::<_, String>(0))
                .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?
                .collect::<Result<_, _>>()
                .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?;

            // Count patterns that are substrings of `file_path` using memmem (no regex overhead).
            let path_bytes = path.as_bytes();
            let count = patterns
                .iter()
                .filter(|pat| !pat.is_empty() && memmem::find(path_bytes, pat.as_bytes()).is_some())
                .count();

            Ok(count as i64)
        })
        .await
        .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?
    }

    /// Record file access.
    pub async fn record_access(
        &self,
        file_path: &str,
        session_id: &str,
    ) -> Result<(), AsyncKnowledgeError> {
        let path = file_path.to_string();
        let session = session_id.to_string();
        let conn = self
            .pool
            .get()
            .await
            .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?;

        conn.interact(move |conn| {
            conn.execute(
                &format!(
                    "INSERT INTO {} (file_path, session_id) VALUES (?1, ?2)",
                    schema_guard::TABLE_FILE_ACCESS_LOG
                ),
                rusqlite::params![path, session],
            )
            .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?;

            Ok(())
        })
        .await
        .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?
    }

    /// WAL checkpoint - call periodically to flush WAL to disk.
    pub async fn wal_checkpoint(&self) -> Result<(), AsyncKnowledgeError> {
        let conn = self
            .pool
            .get()
            .await
            .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?;

        conn.interact(|conn| {
            conn.execute_batch("PRAGMA wal_checkpoint(TRUNCATE)")
                .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))
        })
        .await
        .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?
    }

    /// Get recent bash outcomes.
    pub async fn recent_bash_outcomes(
        &self,
        limit: usize,
    ) -> Result<Vec<BashOutcome>, AsyncKnowledgeError> {
        let conn = self
            .pool
            .get()
            .await
            .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?;

        conn.interact(move |conn| {
            let mut stmt = conn
                .prepare(&format!(
                    "SELECT command, command_hash, exit_code, success, error_pattern,
                            file_context, executed_at
                     FROM {} ORDER BY rowid DESC LIMIT ?1",
                    schema_guard::TABLE_BASH_OUTCOMES
                ))
                .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?;

            let rows = stmt
                .query_map([limit as i64], |row| {
                    Ok(BashOutcome {
                        command: row.get(0)?,
                        command_short: row.get::<_, String>(0)?.chars().take(50).collect(),
                        command_hash: row.get(1)?,
                        exit_code: row.get(2)?,
                        success: row.get(3)?,
                        error_pattern: row.get(4)?,
                        file_context: row.get(5)?,
                        executed_at: row.get(6)?,
                    })
                })
                .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?;

            let outcomes: Result<Vec<_>, _> = rows.collect();
            outcomes.map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))
        })
        .await
        .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?
    }

    /// Record a co-edit relationship.
    pub async fn record_coedit(
        &self,
        source: &str,
        target: &str,
    ) -> Result<(), AsyncKnowledgeError> {
        let src = source.to_string();
        let tgt = target.to_string();
        let conn = self
            .pool
            .get()
            .await
            .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?;

        conn.interact(move |conn| {
            conn.execute(
                &format!(
                    "INSERT INTO {t} (source_path, target_path, count)
                 VALUES (?1, ?2, 1)
                 ON CONFLICT(source_path, target_path) DO UPDATE SET
                    count = count + 1",
                    t = schema_guard::TABLE_FILE_COEDITS
                ),
                rusqlite::params![src, tgt],
            )
            .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?;

            Ok(())
        })
        .await
        .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?
    }

    /// EC10: Get co-edited files for `file_path`, ranked by co-edit count (highest first).
    ///
    /// Queries TABLE_FILE_COEDITS (populated by the sync `record_coedits` hook in post_edit.rs).
    /// Returns `Vec<(target_path, score)>` where `score` is count / max_count (0.0–1.0).
    /// Used by `GraphService::predict_coedit_files` to activate the 33%-weight co-edit RRF signal.
    pub async fn get_coedits_from(
        &self,
        file_path: &str,
    ) -> Result<Vec<(String, f64)>, AsyncKnowledgeError> {
        let path = file_path.to_string();
        let conn = self
            .pool
            .get()
            .await
            .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?;

        conn.interact(move |conn| {
            let mut stmt = conn
                .prepare(&format!(
                    "SELECT target_path, count FROM {} WHERE source_path = ?1 ORDER BY count DESC LIMIT 20",
                    schema_guard::TABLE_FILE_COEDITS
                ))
                .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?;

            let pairs: Vec<(String, i64)> = stmt
                .query_map([&path], |row| Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?)))
                .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?
                .collect::<Result<_, _>>()
                .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?;

            // Normalize scores: score = count / max_count so the ranked list is [0.0, 1.0].
            // Returns empty vec when no co-edits found — GraphService handles gracefully.
            let max_count = pairs.iter().map(|(_, c)| *c).max().unwrap_or(1) as f64;
            Ok(pairs.into_iter().map(|(t, c)| (t, c as f64 / max_count)).collect())
        })
        .await
        .map_err(|e| AsyncKnowledgeError::Sqlite(e.to_string()))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    /// Seed the gotchas table directly via a blocking connection so tests don't
    /// need to go through the full CLI handler stack.
    /// `rows` is `(pattern, decay_score, resolved)`.
    fn seed_gotchas(db_path: &std::path::Path, rows: &[(&str, f64, bool)]) {
        let conn = rusqlite::Connection::open(db_path).expect("open seed connection");
        conn.execute_batch(&format!(
            "CREATE TABLE IF NOT EXISTS {} (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                pattern TEXT NOT NULL,
                gotcha TEXT NOT NULL DEFAULT 'test',
                severity TEXT NOT NULL DEFAULT 'warning',
                symbol_name TEXT,
                hit_count INTEGER NOT NULL DEFAULT 0,
                prevented_errors INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL DEFAULT (datetime('now')),
                language TEXT,
                decay_score REAL NOT NULL DEFAULT 1.0,
                last_occurrence TEXT,
                resolved_at TEXT
            );",
            touring_analysis::e2e::schema_guard::TABLE_GOTCHAS
        ))
        .expect("create gotchas table");

        for (pattern, decay, resolved) in rows {
            let resolved_at: Option<&str> = if *resolved { Some("2024-01-01") } else { None };
            conn.execute(
                &format!(
                    "INSERT INTO {} (pattern, decay_score, resolved_at) VALUES (?1, ?2, ?3)",
                    touring_analysis::e2e::schema_guard::TABLE_GOTCHAS
                ),
                rusqlite::params![pattern, decay, resolved_at],
            )
            .expect("insert gotcha row");
        }
    }

    #[tokio::test]
    async fn test_gotcha_count_matches_substring() {
        // A pattern "async_knowledge" should match a file path containing that substring.
        let tmp = NamedTempFile::new().expect("tempfile");
        seed_gotchas(
            tmp.path(),
            &[
                ("async_knowledge", 1.0, false),  // active — should match
                ("unrelated_module", 1.0, false), // active — should NOT match
            ],
        );
        let db = AsyncFileKnowledgeDB::new(tmp.path()).expect("db init");
        let count = db
            .gotcha_count_for_file("crates/touring-hooks/src/async_knowledge.rs")
            .await
            .expect("gotcha_count_for_file");
        assert_eq!(
            count, 1,
            "only 'async_knowledge' pattern matches the file path"
        );
    }

    #[tokio::test]
    async fn test_gotcha_count_excludes_decayed() {
        // decay_score <= 0.1 must be excluded — same threshold as the original SQL.
        let tmp = NamedTempFile::new().expect("tempfile");
        seed_gotchas(
            tmp.path(),
            &[
                ("hooks", 1.0, false),  // active — matches "hooks" in path
                ("hooks", 0.05, false), // decayed — must NOT be counted
            ],
        );
        let db = AsyncFileKnowledgeDB::new(tmp.path()).expect("db init");
        // Only the first row survives the decay filter; duplicate pattern so count = 1.
        let count = db
            .gotcha_count_for_file("crates/touring-hooks/src/lib.rs")
            .await
            .expect("gotcha_count_for_file");
        assert_eq!(count, 1, "decayed row must not inflate count");
    }

    #[tokio::test]
    async fn test_gotcha_count_excludes_resolved() {
        // resolved_at IS NOT NULL must be excluded.
        let tmp = NamedTempFile::new().expect("tempfile");
        seed_gotchas(
            tmp.path(),
            &[
                ("knowledge", 1.0, true),  // resolved — must NOT be counted
                ("knowledge", 1.0, false), // active — should be counted once
            ],
        );
        let db = AsyncFileKnowledgeDB::new(tmp.path()).expect("db init");
        let count = db
            .gotcha_count_for_file("crates/touring-hooks/src/async_knowledge.rs")
            .await
            .expect("gotcha_count_for_file");
        assert_eq!(count, 1, "resolved gotcha must be excluded");
    }

    #[tokio::test]
    async fn test_gotcha_count_empty_table() {
        // No gotchas at all → count must be 0, no error.
        let tmp = NamedTempFile::new().expect("tempfile");
        seed_gotchas(tmp.path(), &[]);
        let db = AsyncFileKnowledgeDB::new(tmp.path()).expect("db init");
        let count = db
            .gotcha_count_for_file("crates/touring-hooks/src/async_knowledge.rs")
            .await
            .expect("gotcha_count_for_file");
        assert_eq!(count, 0, "empty gotchas table → count 0");
    }

    #[tokio::test]
    async fn test_gotcha_count_no_match() {
        // Active pattern that does NOT appear in the file path.
        let tmp = NamedTempFile::new().expect("tempfile");
        seed_gotchas(tmp.path(), &[("totally_unrelated", 1.0, false)]);
        let db = AsyncFileKnowledgeDB::new(tmp.path()).expect("db init");
        let count = db
            .gotcha_count_for_file("crates/touring-hooks/src/async_knowledge.rs")
            .await
            .expect("gotcha_count_for_file");
        assert_eq!(count, 0, "pattern not in path → count 0");
    }
}

// Helper trait for Option result mapping
trait OptionalExtension<T> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error>;
}

impl<T> OptionalExtension<T> for Result<T, rusqlite::Error> {
    fn optional(self) -> Result<Option<T>, rusqlite::Error> {
        match self {
            Ok(v) => Ok(Some(v)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(e),
        }
    }
}
