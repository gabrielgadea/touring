//! Aggregate analytics and maintenance for [`FileKnowledgeDB`]
//! (edit/co-edit analytics, success-rate, decayed error history, DB cleanup,
//! WAL checkpoint, aggregate stats, batched pre-read signals).
//!
//! Method group extracted verbatim from `knowledge.rs` (1A god-file decomposition);
//! a child-module inherent `impl` block over the parent's `FileKnowledgeDB`.

use super::*;
use rusqlite::params;

impl FileKnowledgeDB {
    #[cfg(feature = "session-hooks")]
    /// Top edited files from edit_history, grouped and ordered by edit count.
    pub fn top_edited_files(
        &self,
        limit: usize,
    ) -> Vec<touring_foundation::insights::EditedFileInsight> {
        let sql = format!(
            "SELECT file_path, COUNT(*) as cnt
             FROM {}
             GROUP BY file_path
             ORDER BY cnt DESC
             LIMIT ?1",
            schema_guard::TABLE_EDIT_HISTORY
        );
        let mut stmt = match self.conn.prepare(&sql) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = match stmt.query_map(params![limit as i64], |row| {
            Ok(touring_foundation::insights::EditedFileInsight {
                file_path: row.get(0)?,
                edit_count: row.get(1)?,
            })
        }) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };
        rows.filter_map(|r| r.ok()).collect()
    }
    /// Compute bash command success rate (0.0..=1.0).
    ///
    /// Returns 1.0 if no commands were recorded (vacuous truth).
    pub fn bash_success_rate(&self) -> f64 {
        let sql = format!(
            "SELECT COUNT(*), COALESCE(SUM(success), 0) FROM {}",
            schema_guard::TABLE_BASH_OUTCOMES
        );
        let result: Result<(i64, i64), _> = self
            .conn
            .query_row(&sql, [], |row| Ok((row.get(0)?, row.get(1)?)));
        match result {
            Ok((total, successes)) if total > 0 => successes as f64 / total as f64,
            _ => 1.0,
        }
    }
    /// Return per-day error rates (1.0 - success_rate) from bash_outcomes history.
    ///
    /// Used by the drift-detection layer in `pre_read` to compare baseline
    /// error rates against the current session error rate and flag divergence.
    /// Returns up to 30 days of history, oldest first.
    pub fn error_rate_history(&self) -> Result<Vec<f64>, rusqlite::Error> {
        let sql = format!(
            "SELECT
                 DATE(executed_at) AS day,
                 COUNT(*) AS total,
                 COALESCE(SUM(CASE WHEN success = 0 THEN 1 ELSE 0 END), 0) AS failures
             FROM {}
             WHERE executed_at >= DATE('now', '-30 days')
             GROUP BY day
             ORDER BY day ASC",
            schema_guard::TABLE_BASH_OUTCOMES
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rates: Vec<f64> = stmt
            .query_map([], |row| {
                let total: i64 = row.get(1)?;
                let failures: i64 = row.get(2)?;
                Ok(if total > 0 {
                    failures as f64 / total as f64
                } else {
                    0.0
                })
            })?
            .filter_map(|r| r.ok())
            .collect();
        Ok(rates)
    }
    /// Increment co-edit weight between two files.
    ///
    /// Records that `source` and `target` were edited in the same session.
    /// If a record already exists, increments the count and updates the timestamp.
    pub fn record_coedit(&self, source: &str, target: &str) -> Result<(), rusqlite::Error> {
        let sql = format!(
            "INSERT INTO {fc} (source_path, target_path, coedit_count, last_coedit_at)
             VALUES (?1, ?2, 1, datetime('now'))
             ON CONFLICT(source_path, target_path) DO UPDATE SET
                coedit_count = coedit_count + 1,
                last_coedit_at = datetime('now')",
            fc = schema_guard::TABLE_FILE_COEDITS
        );
        self.conn.execute(&sql, params![source, target])?;
        Ok(())
    }
    /// Get top-K co-edited files for a given file, ordered by weight descending.
    ///
    /// Searches both directions: files co-edited WITH this file (as source)
    /// and files that co-edited this file (as target). Deduplicates and sums
    /// counts from both directions.
    pub fn get_coedit_neighbors(&self, file_path: &str, top_k: usize) -> Vec<(String, i64)> {
        let sql = format!(
            "SELECT neighbor, SUM(cnt) as total FROM (
                SELECT target_path AS neighbor, coedit_count AS cnt
                FROM {fc} WHERE source_path = ?1
                UNION ALL
                SELECT source_path AS neighbor, coedit_count AS cnt
                FROM {fc} WHERE target_path = ?1
             )
             GROUP BY neighbor
             ORDER BY total DESC
             LIMIT ?2",
            fc = schema_guard::TABLE_FILE_COEDITS
        );
        let mut stmt = match self.conn.prepare(&sql) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = match stmt.query_map(params![file_path, top_k as i64], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)?))
        }) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };
        rows.filter_map(|r| r.ok()).collect()
    }
    /// Decay co-edit weights older than `half_life_days`.
    ///
    /// Halves the `coedit_count` for records not updated within the half-life
    /// window. Records that would decay to 0 are deleted entirely.
    /// Returns the number of records affected (updated + deleted).
    pub fn decay_coedits(&self, half_life_days: f64) -> Result<usize, rusqlite::Error> {
        let delete_sql = format!(
            "DELETE FROM {}
             WHERE coedit_count <= 1
               AND julianday('now') - julianday(last_coedit_at) > ?1",
            schema_guard::TABLE_FILE_COEDITS
        );
        let deleted = self.conn.execute(&delete_sql, params![half_life_days])?;
        let update_sql = format!(
            "UPDATE {}
             SET coedit_count = coedit_count / 2
             WHERE julianday('now') - julianday(last_coedit_at) > ?1",
            schema_guard::TABLE_FILE_COEDITS
        );
        let updated = self.conn.execute(&update_sql, params![half_life_days])?;
        Ok(deleted + updated)
    }
    /// Get recent file accesses (for co-edit detection in post_edit).
    ///
    /// Returns up to `limit` distinct file paths accessed most recently,
    /// excluding the given `exclude_path`.
    pub fn recent_accessed_files(&self, exclude_path: &str, limit: usize) -> Vec<String> {
        let sql = format!(
            "SELECT DISTINCT file_path FROM {}
             WHERE file_path != ?1
             ORDER BY id DESC
             LIMIT ?2",
            schema_guard::TABLE_FILE_ACCESS_LOG
        );
        let mut stmt = match self.conn.prepare(&sql) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = match stmt.query_map(params![exclude_path, limit as i64], |row| {
            row.get::<_, String>(0)
        }) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };
        rows.filter_map(|r| r.ok()).collect()
    }
    /// Delete entries older than `max_age_days` from unbounded tables.
    ///
    /// Targets: `file_access_log`, `file_edit_history`, `bash_outcomes`.
    /// Intended to be called from the session-start hook to keep DB size bounded.
    /// Returns the total number of rows deleted across all three tables.
    pub fn cleanup_old_entries(&self, max_age_days: u32) -> Result<usize, rusqlite::Error> {
        let days = max_age_days as f64;
        let deleted_access = self.conn.execute(
            &format!(
                "DELETE FROM {} WHERE julianday('now') - julianday(accessed_at) > ?1",
                schema_guard::TABLE_FILE_ACCESS_LOG
            ),
            params![days],
        )?;
        let deleted_edits = self.conn.execute(
            &format!(
                "DELETE FROM {} WHERE julianday('now') - julianday(edited_at) > ?1",
                schema_guard::TABLE_EDIT_HISTORY
            ),
            params![days],
        )?;
        let deleted_bash = self.conn.execute(
            &format!(
                "DELETE FROM {} WHERE julianday('now') - julianday(executed_at) > ?1",
                schema_guard::TABLE_BASH_OUTCOMES
            ),
            params![days],
        )?;
        Ok(deleted_access + deleted_edits + deleted_bash)
    }
    /// Checkpoint the WAL file — flush all WAL frames to the main database file.
    /// Safe to call at shutdown; no-op if WAL has no pending frames.
    ///
    /// S-M5: `PRAGMA optimize` runs after checkpoint so SQLite can update index
    /// statistics with the freshly-written data. This improves query plans for
    /// the composite indexes added in S-M1 and S-M2.
    pub fn wal_checkpoint(&self) -> Result<(), rusqlite::Error> {
        self.conn
            .execute_batch("PRAGMA wal_checkpoint(TRUNCATE); PRAGMA optimize;")?;
        Ok(())
    }
    /// Get a summary of the knowledge DB state.
    pub fn stats(&self) -> Result<KnowledgeStats, rusqlite::Error> {
        let file_count: i64 = self.conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM {}",
                schema_guard::TABLE_FILE_KNOWLEDGE
            ),
            [],
            |r| r.get(0),
        )?;
        let relation_count: i64 = self.conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM {}",
                schema_guard::TABLE_FILE_RELATIONS
            ),
            [],
            |r| r.get(0),
        )?;
        let access_count: i64 = self.conn.query_row(
            &format!(
                "SELECT COUNT(*) FROM {}",
                schema_guard::TABLE_FILE_ACCESS_LOG
            ),
            [],
            |r| r.get(0),
        )?;
        let bash_count: i64 = self.conn.query_row(
            &format!("SELECT COUNT(*) FROM {}", schema_guard::TABLE_BASH_OUTCOMES),
            [],
            |r| r.get(0),
        )?;
        let edit_count: i64 = self.conn.query_row(
            &format!("SELECT COUNT(*) FROM {}", schema_guard::TABLE_EDIT_HISTORY),
            [],
            |r| r.get(0),
        )?;
        let gotcha_count: i64 = self.conn.query_row(
            &format!("SELECT COUNT(*) FROM {}", schema_guard::TABLE_GOTCHAS),
            [],
            |r| r.get(0),
        )?;
        let task_metrics_count: i64 = self
            .conn
            .query_row("SELECT COUNT(*) FROM task_decompositions", [], |r| r.get(0))
            .unwrap_or(0);
        Ok(KnowledgeStats {
            file_count,
            relation_count,
            access_count,
            bash_count,
            edit_count,
            gotcha_count,
            task_metrics_count,
        })
    }
    /// Batch query for pre-read hook: fetches notes, failures, and dependents
    /// in a single DB transaction (1 round-trip instead of 3).
    ///
    /// Returns `(notes, recent_failure, dependents)`.
    pub fn batch_pre_read_signals(
        &self,
        file_path: &str,
    ) -> Result<PreReadSignals, rusqlite::Error> {
        let tx = self.conn.unchecked_transaction()?;
        let notes_sql = format!(
            "SELECT notes FROM {} WHERE file_path = ?1",
            schema_guard::TABLE_FILE_KNOWLEDGE
        );
        let notes: Option<String> = tx
            .query_row(&notes_sql, params![file_path], |row| row.get(0))
            .optional()?
            .flatten();
        let failure_sql = format!(
            "SELECT command, error_pattern FROM {}
             WHERE success = 0 AND file_context LIKE '%' || ?1 || '%'
             ORDER BY id DESC LIMIT 1",
            schema_guard::TABLE_BASH_OUTCOMES
        );
        let latest_failure: Option<(String, Option<String>)> = tx
            .query_row(&failure_sql, params![file_path], |row| {
                Ok((row.get(0)?, row.get(1)?))
            })
            .optional()?;
        let deps_sql = format!(
            "SELECT source_path FROM {}
             WHERE target_path = ?1
             ORDER BY source_path LIMIT 5",
            schema_guard::TABLE_FILE_RELATIONS
        );
        let dependents: Vec<String> = {
            let mut stmt = tx.prepare(&deps_sql)?;
            let rows = stmt.query_map(params![file_path], |row| row.get::<_, String>(0))?;
            let collected: Vec<String> = rows.filter_map(|r| r.ok()).collect();
            collected
        };
        let dep_count_sql = format!(
            "SELECT COUNT(*) FROM {} WHERE target_path = ?1",
            schema_guard::TABLE_FILE_RELATIONS
        );
        let dependent_count: i64 =
            tx.query_row(&dep_count_sql, params![file_path], |row| row.get(0))?;
        let gotchas = self.get_gotchas_for_file(file_path);
        tx.commit()?;
        Ok(PreReadSignals {
            notes,
            latest_failure,
            dependent_names: dependents,
            dependent_count: dependent_count as usize,
            gotchas,
        })
    }
}
