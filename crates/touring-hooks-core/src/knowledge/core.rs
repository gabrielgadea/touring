//! Core file-knowledge CRUD, relations, and access tracking for [`FileKnowledgeDB`]
//! (`lookup`/`upsert`/notes, file relations, access counts, failure counts).
//!
//! Method group extracted verbatim from `knowledge.rs` (1A god-file decomposition);
//! a child-module inherent `impl` block over the parent's `FileKnowledgeDB`.

use super::*;
use rusqlite::params;

impl FileKnowledgeDB {
    /// Look up knowledge about a file.
    pub fn lookup(&self, file_path: &str) -> Result<Option<FileKnowledge>, rusqlite::Error> {
        let sql = format!(
            "SELECT file_path, language, line_count, symbol_count, read_count,
                    last_read_at, content_hash, imports_json, symbols_json, notes
             FROM {} WHERE file_path = ?1",
            schema_guard::TABLE_FILE_KNOWLEDGE
        );
        self.conn
            .query_row(&sql, params![file_path], |row| {
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
    }
    /// Insert or update file knowledge. Increments read_count on update.
    pub fn upsert(&self, k: &FileKnowledge) -> Result<(), rusqlite::Error> {
        let sql = format!(
            "INSERT INTO {fk}
                (file_path, language, line_count, symbol_count, read_count,
                 last_read_at, content_hash, imports_json, symbols_json, notes, updated_at)
             VALUES (?1, ?2, ?3, ?4, 1, datetime('now'), ?5, ?6, ?7, ?8, datetime('now'))
             ON CONFLICT(file_path) DO UPDATE SET
                language = COALESCE(?2, language),
                line_count = ?3,
                symbol_count = ?4,
                read_count = read_count + 1,
                last_read_at = datetime('now'),
                content_hash = COALESCE(?5, content_hash),
                imports_json = COALESCE(?6, imports_json),
                symbols_json = COALESCE(?7, symbols_json),
                notes = COALESCE(?8, notes),
                updated_at = datetime('now')",
            fk = schema_guard::TABLE_FILE_KNOWLEDGE
        );
        self.conn.execute(
            &sql,
            params![
                k.file_path,
                k.language,
                k.line_count,
                k.symbol_count,
                k.content_hash,
                k.imports_json,
                k.symbols_json,
                k.notes,
            ],
        )?;
        self.invalidate_extended_cache(&k.file_path);
        Ok(())
    }
    /// Update notes/gotchas for a file (append).
    pub fn append_note(&self, file_path: &str, note: &str) -> Result<(), rusqlite::Error> {
        let sql = format!(
            "UPDATE {} SET
                notes = CASE
                    WHEN notes IS NULL THEN ?2
                    ELSE notes || '; ' || ?2
                END,
                updated_at = datetime('now')
             WHERE file_path = ?1",
            schema_guard::TABLE_FILE_KNOWLEDGE
        );
        self.conn.execute(&sql, params![file_path, note])?;
        Ok(())
    }
    /// Replace (not append) quality-specific notes for a file.
    ///
    /// Strips any existing "quality: ..." segment from notes before setting
    /// the new one. Prevents unbounded note growth from repeated edits.
    pub fn replace_quality_note(
        &self,
        file_path: &str,
        quality_note: &str,
    ) -> Result<(), rusqlite::Error> {
        let sql = format!(
            "SELECT notes FROM {} WHERE file_path = ?1",
            schema_guard::TABLE_FILE_KNOWLEDGE
        );
        let current: Option<String> = self
            .conn
            .query_row(&sql, params![file_path], |row| row.get(0))
            .unwrap_or(None);
        let new_notes = match current {
            Some(existing) => {
                let cleaned: Vec<&str> = existing
                    .split("; ")
                    .filter(|s| !s.starts_with("quality:"))
                    .collect();
                if cleaned.is_empty() {
                    quality_note.to_string()
                } else {
                    format!("{}; {}", cleaned.join("; "), quality_note)
                }
            }
            None => quality_note.to_string(),
        };
        let update_sql = format!(
            "UPDATE {} SET notes = ?2, updated_at = datetime('now') WHERE file_path = ?1",
            schema_guard::TABLE_FILE_KNOWLEDGE
        );
        self.conn
            .execute(&update_sql, params![file_path, new_notes])?;
        Ok(())
    }
    /// Get all relations FROM a file.
    pub fn get_relations_from(
        &self,
        file_path: &str,
    ) -> Result<Vec<FileRelation>, rusqlite::Error> {
        let sql = format!(
            "SELECT source_path, target_path, relation_type
             FROM {} WHERE source_path = ?1 LIMIT 20",
            schema_guard::TABLE_FILE_RELATIONS
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![file_path], |row| {
            Ok(FileRelation {
                source: row.get(0)?,
                target: row.get(1)?,
                relation_type: row.get(2)?,
            })
        })?;
        rows.collect()
    }
    /// Get all files that depend ON this file (reverse lookup).
    pub fn get_dependents(&self, file_path: &str) -> Result<Vec<FileRelation>, rusqlite::Error> {
        let sql = format!(
            "SELECT source_path, target_path, relation_type
             FROM {} WHERE target_path = ?1 LIMIT 20",
            schema_guard::TABLE_FILE_RELATIONS
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![file_path], |row| {
            Ok(FileRelation {
                source: row.get(0)?,
                target: row.get(1)?,
                relation_type: row.get(2)?,
            })
        })?;
        rows.collect()
    }
    /// H1-C: Return ALL file relations for seeding the petgraph DependencyCache.
    ///
    /// Used once at daemon startup by `HookRuntime::init_dependency_cache()`.
    /// Returns an empty Vec (not an error) if the table is empty or missing.
    pub fn all_file_relations(&self) -> Vec<FileRelation> {
        let sql = format!(
            "SELECT source_path, target_path, relation_type FROM {}",
            schema_guard::TABLE_FILE_RELATIONS
        );
        let Ok(mut stmt) = self.conn.prepare(&sql) else {
            return Vec::new();
        };
        stmt.query_map([], |row| {
            Ok(FileRelation {
                source: row.get(0)?,
                target: row.get(1)?,
                relation_type: row.get(2)?,
            })
        })
        .and_then(|rows| rows.collect())
        .unwrap_or_default()
    }
    /// Insert or ignore a file relation.
    pub fn upsert_relation(&self, rel: &FileRelation) -> Result<(), rusqlite::Error> {
        let sql = format!(
            "INSERT OR IGNORE INTO {} (source_path, target_path, relation_type)
             VALUES (?1, ?2, ?3)",
            schema_guard::TABLE_FILE_RELATIONS
        );
        self.conn
            .execute(&sql, params![rel.source, rel.target, rel.relation_type])?;
        Ok(())
    }
    /// Replace all relations FROM a source file.
    pub fn replace_relations_from(
        &self,
        source: &str,
        relations: &[FileRelation],
    ) -> Result<(), rusqlite::Error> {
        let sql = format!(
            "DELETE FROM {} WHERE source_path = ?1",
            schema_guard::TABLE_FILE_RELATIONS
        );
        self.conn.execute(&sql, params![source])?;
        for rel in relations {
            self.upsert_relation(rel)?;
        }
        Ok(())
    }
    /// Record a file access event.
    pub fn record_access(&self, file_path: &str, session_id: &str) -> Result<(), rusqlite::Error> {
        let sql = format!(
            "INSERT INTO {} (file_path, session_id) VALUES (?1, ?2)",
            schema_guard::TABLE_FILE_ACCESS_LOG
        );
        self.conn.execute(&sql, params![file_path, session_id])?;
        Ok(())
    }
    /// Increment and return the failure count for a failure_key.
    /// Used by bridge_post_tool_failure to track repeated failures per pattern.
    pub fn increment_failure_count(&self, failure_key: &str) -> Result<u32, rusqlite::Error> {
        self.conn
            .execute(
                "INSERT INTO failure_counts (failure_key, count, last_updated)
             VALUES (?1, 1, datetime('now'))
             ON CONFLICT(failure_key) DO UPDATE SET count = count + 1, last_updated = datetime('now')",
                params![failure_key],
            )?;
        let count: i64 = self.conn.query_row(
            "SELECT count FROM failure_counts WHERE failure_key = ?1",
            params![failure_key],
            |row| row.get(0),
        )?;
        Ok(count as u32)
    }
    /// Count total accesses for a file.
    pub fn access_count(&self, file_path: &str) -> Result<i64, rusqlite::Error> {
        let sql = format!(
            "SELECT COUNT(*) FROM {} WHERE file_path = ?1",
            schema_guard::TABLE_FILE_ACCESS_LOG
        );
        self.conn
            .query_row(&sql, params![file_path], |row| row.get(0))
    }
    /// Count total edit history records across all files.
    ///
    /// Used to verify that edits are being recorded successfully.
    /// A zero return after post_edit hooks run indicates tracking is broken.
    pub fn edit_history_count(&self) -> Result<i64, rusqlite::Error> {
        let sql = format!("SELECT COUNT(*) FROM {}", schema_guard::TABLE_EDIT_HISTORY);
        self.conn.query_row(&sql, [], |row| row.get(0))
    }
    /// Return the N most frequently accessed file paths (excluding internal markers).
    ///
    /// Internal markers like `__session_end__` and `__subagent_stop__` are excluded
    /// by checking that the path does not start with `__`.
    ///
    /// Used by session-start prewarm to populate the result cache with context
    /// for files the user is most likely to read again.
    pub fn top_accessed_files(&self, limit: usize) -> Result<Vec<String>, rusqlite::Error> {
        let sql = format!(
            "SELECT file_path, COUNT(*) as cnt
             FROM {}
             WHERE file_path NOT LIKE '\\_\\_%' ESCAPE '\\'
             GROUP BY file_path
             ORDER BY cnt DESC
             LIMIT ?1",
            schema_guard::TABLE_FILE_ACCESS_LOG
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let files = stmt
            .query_map(params![limit as i64], |row| row.get::<_, String>(0))?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(files)
    }
}
