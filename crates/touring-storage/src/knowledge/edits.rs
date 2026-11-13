//! File-edit event recording and retrieval for `FileKnowledgeDB`.
//!
//! Method group extracted verbatim from `knowledge.rs` (1A god-file decomposition);
//! a child-module inherent `impl` block over the parent's `FileKnowledgeDB`.

use super::*;
use rusqlite::params;

impl FileKnowledgeDB {
    /// Record an edit event, optionally with a normalized error pattern.
    pub fn record_edit(
        &self,
        file_path: &str,
        edit_type: &str,
        summary: Option<&str>,
    ) -> Result<(), rusqlite::Error> {
        self.record_edit_with_error(file_path, edit_type, summary, None)
    }
    /// Record an edit event with an optional error pattern for error-driven learning.
    pub fn record_edit_with_error(
        &self,
        file_path: &str,
        edit_type: &str,
        summary: Option<&str>,
        error_pattern: Option<&str>,
    ) -> Result<(), rusqlite::Error> {
        self.record_edit_full(
            file_path,
            edit_type,
            summary,
            error_pattern,
            None,
            None,
            None,
        )
    }
    /// S1.2+S1.3: Record an edit with full context-aware fields.
    #[allow(clippy::too_many_arguments)]
    pub fn record_edit_full(
        &self,
        file_path: &str,
        edit_type: &str,
        summary: Option<&str>,
        error_pattern: Option<&str>,
        language: Option<&str>,
        symbol_context: Option<&str>,
        session_id: Option<&str>,
    ) -> Result<(), rusqlite::Error> {
        let sql = format!(
            "INSERT INTO {}
                (file_path, edit_type, summary, error_pattern, language, symbol_context, session_id)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
            schema_guard::TABLE_EDIT_HISTORY
        );
        self.conn.execute(
            &sql,
            params![
                file_path,
                edit_type,
                summary,
                error_pattern,
                language,
                symbol_context,
                session_id
            ],
        )?;
        Ok(())
    }
    /// Count how many times a specific error pattern has occurred for a file
    /// within the most recent N edits.
    pub fn count_edit_error_pattern(
        &self,
        file_path: &str,
        error_pattern: &str,
        recent_limit: usize,
    ) -> i64 {
        let sql = format!(
            "SELECT COUNT(*) FROM (
                SELECT error_pattern FROM {}
                WHERE file_path = ?1
                ORDER BY id DESC
                LIMIT ?3
            ) WHERE error_pattern = ?2",
            schema_guard::TABLE_EDIT_HISTORY
        );
        let result: Result<i64, _> = self.conn.query_row(
            &sql,
            params![file_path, error_pattern, recent_limit as i64],
            |row| row.get(0),
        );
        result.unwrap_or(0)
    }
    /// The `limit` most recently edited files with their edit counts and last-edit
    /// instants — the seed of a freshly booted heat map, so heat survives a daemon
    /// restart. Only files inside the project: those outside it are recorded under
    /// their absolute path and would crowd the project's own files out of a small map.
    ///
    /// A missing table, a failed `prepare` or an unreadable row is an `Err`, never an empty
    /// list: "no history yet" and "history unreadable" are different facts, and the caller
    /// (the boot seed of the heat map) must be able to say which one it met. A row whose
    /// last instant does not parse has no recency to decay from and is left out.
    pub fn recent_edit_heat(&self, limit: usize) -> Result<Vec<EditHeatRow>, rusqlite::Error> {
        let sql = format!(
            "SELECT file_path, COUNT(*), CAST(strftime('%s', MAX(edited_at)) AS REAL)
             FROM {}
             WHERE file_path NOT LIKE '/%'
             GROUP BY file_path
             ORDER BY MAX(edited_at) DESC, file_path
             LIMIT ?1",
            schema_guard::TABLE_EDIT_HISTORY
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)?,
                row.get::<_, Option<f64>>(2)?,
            ))
        })?;
        let mut heat = Vec::new();
        for row in rows {
            let (file_path, edits, last_edit_epoch) = row?;
            let Some(last_edit_epoch) = last_edit_epoch else {
                continue;
            };
            heat.push(EditHeatRow {
                file_path,
                edits: u32::try_from(edits).unwrap_or(u32::MAX),
                last_edit_epoch,
            });
        }
        Ok(heat)
    }
    /// Get recent edits across ALL files (for cross-file pattern analysis).
    ///
    /// Returns up to `limit` most recent edits regardless of file path.
    pub fn recent_edits_all(&self, limit: usize) -> Result<Vec<EditEvent>, rusqlite::Error> {
        let sql = format!(
            "SELECT file_path, edit_type, summary, error_pattern, edited_at
             FROM {}
             ORDER BY id DESC LIMIT ?1",
            schema_guard::TABLE_EDIT_HISTORY
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![limit as i64], |row| {
            Ok(EditEvent {
                file_path: row.get(0)?,
                edit_type: row.get(1)?,
                summary: row.get(2)?,
                error_pattern: row.get(3)?,
                edited_at: row.get(4)?,
            })
        })?;
        rows.collect()
    }
    /// Get recent edits filtered by session_id (avoids cross-session bleed).
    ///
    /// Falls back to `recent_edits_all` if session_id is empty or no edits match.
    pub fn recent_edits_for_session(
        &self,
        session_id: &str,
        limit: usize,
    ) -> Result<Vec<EditEvent>, rusqlite::Error> {
        if session_id.is_empty() {
            return self.recent_edits_all(limit);
        }
        let sql = format!(
            "SELECT file_path, edit_type, summary, error_pattern, edited_at
             FROM {}
             WHERE session_id = ?1
             ORDER BY id DESC LIMIT ?2",
            schema_guard::TABLE_EDIT_HISTORY
        );
        let mut stmt = self.conn.prepare_cached(&sql)?;
        let rows = stmt.query_map(params![session_id, limit as i64], |row| {
            Ok(EditEvent {
                file_path: row.get(0)?,
                edit_type: row.get(1)?,
                summary: row.get(2)?,
                error_pattern: row.get(3)?,
                edited_at: row.get(4)?,
            })
        })?;
        let results: Vec<EditEvent> = rows.collect::<Result<_, _>>()?;
        if results.is_empty() {
            return self.recent_edits_all(limit);
        }
        Ok(results)
    }
    /// Get recent edits for a file.
    pub fn recent_edits(
        &self,
        file_path: &str,
        limit: usize,
    ) -> Result<Vec<EditEvent>, rusqlite::Error> {
        let sql = format!(
            "SELECT file_path, edit_type, summary, error_pattern, edited_at
             FROM {}
             WHERE file_path = ?1
             ORDER BY id DESC LIMIT ?2",
            schema_guard::TABLE_EDIT_HISTORY
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![file_path, limit as i64], |row| {
            Ok(EditEvent {
                file_path: row.get(0)?,
                edit_type: row.get(1)?,
                summary: row.get(2)?,
                error_pattern: row.get(3)?,
                edited_at: row.get(4)?,
            })
        })?;
        rows.collect()
    }
}
