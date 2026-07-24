//! Bash-command outcome recording and lookup for [`FileKnowledgeDB`].
//!
//! Method group extracted verbatim from `knowledge.rs` (1A god-file decomposition);
//! a child-module inherent `impl` block over the parent's `FileKnowledgeDB`.

use super::*;
use rusqlite::params;

impl FileKnowledgeDB {
    /// Record a bash command outcome.
    ///
    /// Stores `command_hash` (SHA-256 of the full command text) alongside
    /// the truncated `command` field. This allows precise lookups by full
    /// command content even when the stored `command` is truncated to 500
    /// chars. Each invocation creates a new row (no dedup) to preserve
    /// failure-rate statistics across multiple runs.
    pub fn record_bash_outcome(&self, outcome: &BashOutcome) -> Result<(), rusqlite::Error> {
        let hash = if outcome.command_hash.is_empty() {
            sha256_hex(&outcome.command)
        } else {
            outcome.command_hash.clone()
        };
        let sql = format!(
            "INSERT INTO {}
                (command, command_short, command_hash, exit_code, success, error_pattern, file_context)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            schema_guard::TABLE_BASH_OUTCOMES
        );
        self.conn.execute(
            &sql,
            params![
                outcome.command,
                outcome.command_short,
                hash,
                outcome.exit_code,
                outcome.success as i64,
                outcome.error_pattern,
                outcome.file_context,
            ],
        )?;
        Ok(())
    }
    /// Find outcomes by the full command hash (exact match).
    ///
    /// Unlike `find_bash_outcomes` which matches by `command_short`,
    /// this method matches by the SHA-256 hash of the full command text.
    /// This prevents collisions between different commands that share the
    /// same truncated prefix.
    pub fn find_bash_outcomes_by_hash(
        &self,
        command_hash: &str,
        limit: usize,
    ) -> Result<Vec<BashOutcome>, rusqlite::Error> {
        let sql = format!(
            "SELECT command, command_short, exit_code, success, error_pattern,
                    file_context, executed_at, command_hash
             FROM {}
             WHERE command_hash = ?1
             ORDER BY id DESC LIMIT ?2",
            schema_guard::TABLE_BASH_OUTCOMES
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![command_hash, limit as i64], |row| {
            Ok(BashOutcome {
                command: row.get(0)?,
                command_short: row.get(1)?,
                command_hash: row.get(7)?,
                exit_code: row.get(2)?,
                success: row.get::<_, i64>(3)? != 0,
                error_pattern: row.get(4)?,
                file_context: row.get(5)?,
                executed_at: row.get(6)?,
            })
        })?;
        rows.collect()
    }
    /// Find recent outcomes for commands matching a short key.
    pub fn find_bash_outcomes(
        &self,
        command_short: &str,
        limit: usize,
    ) -> Result<Vec<BashOutcome>, rusqlite::Error> {
        let sql = format!(
            "SELECT command, exit_code, success, error_pattern, file_context, executed_at,
                    COALESCE(command_hash, '') as command_hash
             FROM {}
             WHERE command_short = ?1
             ORDER BY id DESC LIMIT ?2",
            schema_guard::TABLE_BASH_OUTCOMES
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let cmd_short = command_short.to_string();
        let rows = stmt.query_map(params![command_short, limit as i64], |row| {
            Ok(BashOutcome {
                command: row.get(0)?,
                command_short: String::new(),
                command_hash: row.get(6)?,
                exit_code: row.get(1)?,
                success: row.get::<_, i64>(2)? != 0,
                error_pattern: row.get(3)?,
                file_context: row.get(4)?,
                executed_at: row.get(5)?,
            })
        })?;
        let mut results: Vec<BashOutcome> = rows.collect::<Result<Vec<_>, _>>()?;
        for r in &mut results {
            r.command_short = cmd_short.clone();
        }
        Ok(results)
    }
    /// B-5: bulk-read the most recent bash outcomes across *all* command classes
    /// (no `command_short` filter) — the experiential substrate distilled into an
    /// action predictor by `cli_predict_action`. Ordered newest-first, capped at
    /// `limit`.
    pub fn recent_bash_outcomes(&self, limit: usize) -> Result<Vec<BashOutcome>, rusqlite::Error> {
        let sql = format!(
            "SELECT command, command_short, exit_code, success, error_pattern, file_context,
                    executed_at, COALESCE(command_hash, '') as command_hash
             FROM {}
             ORDER BY id DESC LIMIT ?1",
            schema_guard::TABLE_BASH_OUTCOMES
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(BashOutcome {
                command: row.get(0)?,
                command_short: row.get(1)?,
                exit_code: row.get(2)?,
                success: row.get::<_, i64>(3)? != 0,
                error_pattern: row.get(4)?,
                file_context: row.get(5)?,
                executed_at: row.get(6)?,
                command_hash: row.get(7)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>()
    }
    /// Find recent failures for any command (cross-cutting).
    pub fn recent_failures_for_file(
        &self,
        file_path: &str,
        limit: usize,
    ) -> Result<Vec<BashOutcome>, rusqlite::Error> {
        let sql = format!(
            "SELECT command, command_short, exit_code, success, error_pattern, file_context,
                    executed_at, COALESCE(command_hash, '') as command_hash
             FROM {}
             WHERE success = 0 AND file_context LIKE '%' || ?1 || '%'
             ORDER BY id DESC LIMIT ?2",
            schema_guard::TABLE_BASH_OUTCOMES
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![file_path, limit as i64], |row| {
            Ok(BashOutcome {
                command: row.get(0)?,
                command_short: row.get(1)?,
                command_hash: row.get(7)?,
                exit_code: row.get(2)?,
                success: row.get::<_, i64>(3)? != 0,
                error_pattern: row.get(4)?,
                file_context: row.get(5)?,
                executed_at: row.get(6)?,
            })
        })?;
        rows.collect()
    }
}
