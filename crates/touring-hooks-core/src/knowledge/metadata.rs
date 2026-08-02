//! Auxiliary metadata key-value stores for `FileKnowledgeDB`
//! (feature flags, todos, communities, coverage, blake3 registry, session summaries,
//! symbol events, wiring suggestions, benchmarks, cognitive enrichment).
//!
//! Method group extracted verbatim from `knowledge.rs` (1A god-file decomposition);
//! a child-module inherent `impl` block over the parent's `FileKnowledgeDB`.

use super::*;
use rusqlite::params;

impl FileKnowledgeDB {
    /// Upsert a feature flag for a file (INSERT OR REPLACE).
    pub fn upsert_feature_flag(
        &self,
        file_path: &str,
        feature_name: &str,
        lang: &str,
    ) -> crate::errors::Result<()> {
        let sql = format!(
            "INSERT OR REPLACE INTO {} (file_path, feature_name, lang, detected_at)
             VALUES (?1, ?2, ?3, datetime('now'))",
            schema_guard::TABLE_FILE_FEATURE_FLAGS
        );
        self.conn
            .execute(&sql, params![file_path, feature_name, lang])?;
        Ok(())
    }
    /// Batch upsert feature flags for a file.
    pub fn upsert_feature_flags_batch(
        &self,
        file_path: &str,
        features: &[(&str, &str)],
    ) -> crate::errors::Result<()> {
        let sql = format!(
            "INSERT OR REPLACE INTO {} (file_path, feature_name, lang, detected_at)
             VALUES (?1, ?2, ?3, datetime('now'))",
            schema_guard::TABLE_FILE_FEATURE_FLAGS
        );
        let mut stmt = self.conn.prepare(&sql)?;
        for (feature_name, lang) in features {
            stmt.execute(params![file_path, feature_name, lang])?;
        }
        Ok(())
    }
    /// Insert a TODO/FIXME entry for a file.
    pub fn insert_todo(
        &self,
        file_path: &str,
        line_num: i64,
        kind: &str,
        content: &str,
    ) -> crate::errors::Result<i64> {
        let sql = format!(
            "INSERT INTO {} (file_path, line_num, kind, content, resolved, created_at)
             VALUES (?1, ?2, ?3, ?4, 0, datetime('now'))",
            schema_guard::TABLE_FILE_TODOS
        );
        self.conn
            .execute(&sql, params![file_path, line_num, kind, content])?;
        Ok(self.conn.last_insert_rowid())
    }
    /// Mark a TODO as resolved.
    pub fn resolve_todo(&self, id: i64) -> crate::errors::Result<()> {
        let sql = format!(
            "UPDATE {} SET resolved = 1 WHERE id = ?1",
            schema_guard::TABLE_FILE_TODOS
        );
        self.conn.execute(&sql, params![id])?;
        Ok(())
    }
    /// Get unresolved TODOs for a file.
    pub fn get_unresolved_todos(
        &self,
        file_path: &str,
    ) -> crate::errors::Result<Vec<(i64, i64, String, String)>> {
        let sql = format!(
            "SELECT id, line_num, kind, content FROM {}
             WHERE file_path = ?1 AND resolved = 0
             ORDER BY line_num",
            schema_guard::TABLE_FILE_TODOS
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![file_path], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?;
        rows.collect::<Result<Vec<_>, rusqlite::Error>>()
            .map_err(TouringError::from)
    }
    /// Upsert edge confidence score for a file relationship.
    pub fn upsert_edge_confidence(
        &self,
        source_path: &str,
        target_path: &str,
        relation_type: &str,
        confidence_level: &str,
    ) -> crate::errors::Result<()> {
        let sql = format!(
            "INSERT OR REPLACE INTO {}
             (source_path, target_path, relation_type, confidence_level, computed_at)
             VALUES (?1, ?2, ?3, ?4, datetime('now'))",
            schema_guard::TABLE_EDGE_CONFIDENCE
        );
        self.conn.execute(
            &sql,
            params![source_path, target_path, relation_type, confidence_level],
        )?;
        Ok(())
    }
    /// Upsert file community assignment (Louvain clustering).
    pub fn upsert_file_community(
        &self,
        file_path: &str,
        community_id: i64,
        modularity_score: f64,
    ) -> crate::errors::Result<()> {
        let sql = format!(
            "INSERT OR REPLACE INTO {}
             (file_path, community_id, modularity_score, assigned_at)
             VALUES (?1, ?2, ?3, datetime('now'))",
            schema_guard::TABLE_FILE_COMMUNITIES
        );
        self.conn
            .execute(&sql, params![file_path, community_id, modularity_score])?;
        self.invalidate_extended_cache(file_path);
        Ok(())
    }
    /// Get file community (if assigned).
    pub fn get_file_community(&self, file_path: &str) -> crate::errors::Result<Option<(i64, f64)>> {
        let sql = format!(
            "SELECT community_id, modularity_score FROM {} WHERE file_path = ?1",
            schema_guard::TABLE_FILE_COMMUNITIES
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query(params![file_path])?;
        if let Some(row) = rows.next()? {
            Ok(Some((row.get(0)?, row.get(1)?)))
        } else {
            Ok(None)
        }
    }
    /// Upsert test coverage for a file.
    pub fn upsert_test_coverage(
        &self,
        file_path: &str,
        coverage_pct: f64,
        tested_functions: i64,
        total_functions: i64,
    ) -> crate::errors::Result<()> {
        let sql = format!(
            "INSERT OR REPLACE INTO {}
             (file_path, coverage_pct, tested_functions, total_functions, updated_at)
             VALUES (?1, ?2, ?3, ?4, datetime('now'))",
            schema_guard::TABLE_FILE_TEST_COVERAGE
        );
        self.conn.execute(
            &sql,
            params![file_path, coverage_pct, tested_functions, total_functions],
        )?;
        self.invalidate_extended_cache(file_path);
        Ok(())
    }
    /// Upsert BLAKE3 hash for a file.
    pub fn upsert_blake3_registry(
        &self,
        file_path: &str,
        blake3_hash: &str,
        symbol_count: i64,
        merkle_parent: Option<&str>,
    ) -> Result<(), TouringError> {
        let sql = format!(
            "INSERT OR REPLACE INTO {}
             (file_path, blake3_hash, symbol_count, merkle_parent, last_indexed_at)
             VALUES (?1, ?2, ?3, ?4, datetime('now'))",
            schema_guard::TABLE_FILE_BLAKE3_REGISTRY
        );
        self.conn.execute(
            &sql,
            params![file_path, blake3_hash, symbol_count, merkle_parent],
        )?;
        self.invalidate_extended_cache(file_path);
        Ok(())
    }
    /// Get BLAKE3 hash for a file.
    pub fn get_blake3_hash(&self, file_path: &str) -> Result<Option<(String, i64)>, TouringError> {
        let sql = format!(
            "SELECT blake3_hash, symbol_count FROM {} WHERE file_path = ?1",
            schema_guard::TABLE_FILE_BLAKE3_REGISTRY
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query(params![file_path])?;
        if let Some(row) = rows.next()? {
            Ok(Some((row.get(0)?, row.get(1)?)))
        } else {
            Ok(None)
        }
    }
    /// Upsert session-file skeleton summary.
    pub fn upsert_session_file_summary(
        &self,
        file_path: &str,
        session_id: &str,
        skeleton_json: Option<&str>,
        purpose: Option<&str>,
        top_gotchas_json: Option<&str>,
        blast_severity: Option<&str>,
    ) -> crate::errors::Result<()> {
        let sql = format!(
            "INSERT OR REPLACE INTO {}
             (file_path, session_id, skeleton_json, purpose, top_gotchas_json, blast_severity, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'))",
            schema_guard::TABLE_SESSION_FILE_SUMMARY
        );
        self.conn.execute(
            &sql,
            params![
                file_path,
                session_id,
                skeleton_json,
                purpose,
                top_gotchas_json,
                blast_severity
            ],
        )?;
        Ok(())
    }
    /// Query top-N most-accessed files in a session (for session_file_summary wiring).
    pub fn top_accessed_files_in_session(
        &self,
        session_id: &str,
        limit: usize,
    ) -> crate::errors::Result<Vec<String>> {
        let sql = format!(
            "SELECT file_path FROM {}
             WHERE session_id = ?1
             ORDER BY accessed_at DESC
             LIMIT ?2",
            schema_guard::TABLE_FILE_ACCESS_LOG
        );
        let mut stmt = self.conn.prepare_cached(&sql)?;
        let rows = stmt.query_map(params![session_id, limit as i64], |row| row.get(0))?;
        rows.collect::<Result<Vec<_>, rusqlite::Error>>()
            .map_err(TouringError::from)
    }
    /// Insert a symbol-level event (for symbol event log).
    pub fn insert_symbol_event(
        &self,
        sequence_id: &str,
        file_path: &str,
        blake3_hash: Option<&str>,
        operation: &str,
        symbol_name: Option<&str>,
        agent_id: Option<&str>,
    ) -> Result<i64, TouringError> {
        let sql = format!(
            "INSERT INTO {} (sequence_id, file_path, blake3_hash, operation, symbol_name, agent_id, timestamp)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'))",
            schema_guard::TABLE_SYMBOL_EVENTS_LOG
        );
        self.conn.execute(
            &sql,
            params![
                sequence_id,
                file_path,
                blake3_hash,
                operation,
                symbol_name,
                agent_id
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }
    /// Upsert a wiring suggestion (orphan symbol → suggested consumer).
    pub fn upsert_wiring_suggestion(
        &self,
        orphan_symbol: &str,
        orphan_file: &str,
        suggested_consumer: Option<&str>,
        similarity_score: f64,
        community_id: Option<i64>,
    ) -> crate::errors::Result<i64> {
        let sql = format!(
            "INSERT OR REPLACE INTO {}
             (orphan_symbol, orphan_file, suggested_consumer, similarity_score, community_id, applied, rejected, created_at)
             VALUES (?1, ?2, ?3, ?4, ?5, 0, 0, datetime('now'))",
            schema_guard::TABLE_WIRING_SUGGESTIONS
        );
        self.conn.execute(
            &sql,
            params![
                orphan_symbol,
                orphan_file,
                suggested_consumer,
                similarity_score,
                community_id
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }
    /// Mark a wiring suggestion as applied.
    pub fn apply_wiring_suggestion(&self, id: i64) -> crate::errors::Result<()> {
        let sql = format!(
            "UPDATE {} SET applied = 1 WHERE id = ?1",
            schema_guard::TABLE_WIRING_SUGGESTIONS
        );
        self.conn.execute(&sql, params![id])?;
        Ok(())
    }
    /// Mark a wiring suggestion as rejected.
    pub fn reject_wiring_suggestion(&self, id: i64) -> crate::errors::Result<()> {
        let sql = format!(
            "UPDATE {} SET rejected = 1 WHERE id = ?1",
            schema_guard::TABLE_WIRING_SUGGESTIONS
        );
        self.conn.execute(&sql, params![id])?;
        Ok(())
    }
    /// Get pending wiring suggestions for an orphan symbol.
    ///
    /// FIX-5 (2026-04-13): self-heal for schema drift. The
    /// `wiring_suggestions` table is part of the DDL in `ensure_schema`,
    /// but databases minted before the table was added to the DDL still
    /// have `user_version = 8` without the table — `ensure_schema` is
    /// skipped by the version gate. An idempotent `CREATE TABLE IF NOT
    /// EXISTS` at the read site closes the drift without requiring a
    /// schema-version bump or a full migration pass.
    pub fn get_pending_wiring_suggestions(
        &self,
        orphan_symbol: &str,
    ) -> crate::errors::Result<Vec<(i64, String, String, f64)>> {
        let _ = self.conn.execute_batch(&format!(
            "CREATE TABLE IF NOT EXISTS {wsg} (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                orphan_symbol TEXT NOT NULL,
                orphan_file TEXT NOT NULL,
                suggested_consumer TEXT,
                similarity_score REAL NOT NULL DEFAULT 0.0,
                community_id INTEGER,
                applied INTEGER NOT NULL DEFAULT 0,
                rejected INTEGER NOT NULL DEFAULT 0,
                created_at TEXT DEFAULT (datetime('now'))
            );
            CREATE INDEX IF NOT EXISTS idx_wiring_suggest_orphan
                ON {wsg}(orphan_symbol) WHERE applied = 0;",
            wsg = schema_guard::TABLE_WIRING_SUGGESTIONS,
        ));
        let sql = format!(
            "SELECT id, orphan_file, COALESCE(suggested_consumer, ''), similarity_score
             FROM {}
             WHERE orphan_symbol = ?1 AND applied = 0 AND rejected = 0
             ORDER BY similarity_score DESC",
            schema_guard::TABLE_WIRING_SUGGESTIONS
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![orphan_symbol], |row| {
            Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
        })?;
        rows.collect::<Result<Vec<_>, rusqlite::Error>>()
            .map_err(TouringError::from)
    }
    /// Insert a benchmark run result.
    pub fn insert_benchmark_run(
        &self,
        commit_hash: &str,
        bench_name: &str,
        p50_ms: f64,
        p95_ms: f64,
        p99_ms: f64,
        samples: i64,
    ) -> crate::errors::Result<i64> {
        let sql = format!(
            "INSERT OR IGNORE INTO {}
             (commit_hash, bench_name, p50_ms, p95_ms, p99_ms, samples, ran_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'))",
            schema_guard::TABLE_METADATA_BENCHMARK_RUNS
        );
        self.conn.execute(
            &sql,
            params![commit_hash, bench_name, p50_ms, p95_ms, p99_ms, samples],
        )?;
        Ok(self.conn.last_insert_rowid())
    }
    /// Get all feature flags for a given file path, sorted alphabetically.
    pub fn get_feature_flags_for_file(
        &self,
        file_path: &str,
    ) -> crate::errors::Result<Vec<String>> {
        let sql = format!(
            "SELECT feature_name FROM {} WHERE file_path = ?1 ORDER BY feature_name",
            schema_guard::TABLE_FILE_FEATURE_FLAGS
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![file_path], |row| row.get::<_, String>(0))?;
        rows.collect::<Result<Vec<_>, rusqlite::Error>>()
            .map_err(TouringError::from)
    }
    /// Get the edge confidence score between two files, inserting a default "medium"
    /// (0.5) entry if none exists.
    pub fn get_edge_confidence_or_insert(
        &self,
        source: &str,
        target: &str,
        relation_type: &str,
    ) -> Result<f64, rusqlite::Error> {
        let sql = format!(
            "SELECT confidence_level FROM {} WHERE source_path = ?1 AND target_path = ?2 AND relation_type = ?3",
            schema_guard::TABLE_EDGE_CONFIDENCE
        );
        let result: Option<String> = self
            .conn
            .query_row(&sql, params![source, target, relation_type], |row| {
                row.get(0)
            })
            .optional()?;
        match result {
            Some(level) => Ok(match level.as_str() {
                "high" => 1.0,
                "low" => 0.2,
                _ => 0.5,
            }),
            None => {
                let insert_sql = format!(
                    "INSERT OR IGNORE INTO {} (source_path, target_path, relation_type, confidence_level) \
                     VALUES (?1, ?2, ?3, 'medium')",
                    schema_guard::TABLE_EDGE_CONFIDENCE
                );
                self.conn
                    .execute(&insert_sql, params![source, target, relation_type])?;
                Ok(0.5)
            }
        }
    }
    /// Get all benchmark runs for a given bench name, most recent first.
    pub fn get_benchmark_runs(
        &self,
        bench_name: &str,
    ) -> Result<Vec<BenchmarkRun>, rusqlite::Error> {
        let sql = format!(
            "SELECT run_id, commit_hash, bench_name, p50_ms, p95_ms, p99_ms, samples, ran_at \
             FROM {} WHERE bench_name = ?1 ORDER BY run_id DESC",
            schema_guard::TABLE_METADATA_BENCHMARK_RUNS
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let rows = stmt.query_map(params![bench_name], |row| {
            Ok(BenchmarkRun {
                run_id: row.get(0)?,
                commit_hash: row.get(1)?,
                bench_name: row.get(2)?,
                p50_ms: row.get(3)?,
                p95_ms: row.get(4)?,
                p99_ms: row.get(5)?,
                samples: row.get(6)?,
                ran_at: row.get(7)?,
            })
        })?;
        rows.collect()
    }
    /// Get test coverage for a file: returns (coverage_pct, tested_functions, total_functions)
    /// or None if no record exists.
    pub fn get_test_coverage(
        &self,
        file_path: &str,
    ) -> Result<Option<(f64, i64, i64)>, rusqlite::Error> {
        let sql = format!(
            "SELECT coverage_pct, tested_functions, total_functions FROM {} WHERE file_path = ?1",
            schema_guard::TABLE_FILE_TEST_COVERAGE
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query(params![file_path])?;
        if let Some(row) = rows.next()? {
            Ok(Some((row.get(0)?, row.get(1)?, row.get(2)?)))
        } else {
            Ok(None)
        }
    }
    /// Upsert cognitive enrichment scores for a file.
    pub fn upsert_cognitive_enrichment(
        &self,
        file_path: &str,
        cognitive_score: f64,
        complexity_signal: f64,
        fan_in_signal: f64,
        fan_out_signal: f64,
        doc_signal: f64,
    ) -> Result<(), rusqlite::Error> {
        let sql = format!(
            "INSERT OR REPLACE INTO {}
             (file_path, cognitive_score, complexity_signal, fan_in_signal, fan_out_signal, doc_signal, updated_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, datetime('now'))",
            schema_guard::TABLE_COGNITIVE_ENRICHMENT
        );
        self.conn.execute(
            &sql,
            params![
                file_path,
                cognitive_score,
                complexity_signal,
                fan_in_signal,
                fan_out_signal,
                doc_signal
            ],
        )?;
        self.invalidate_extended_cache(file_path);
        Ok(())
    }
    /// Get cognitive enrichment for a file.
    pub fn get_cognitive_enrichment(
        &self,
        file_path: &str,
    ) -> crate::errors::Result<Option<CognitiveScores>> {
        let sql = format!(
            "SELECT cognitive_score, complexity_signal, fan_in_signal, fan_out_signal, doc_signal
             FROM {} WHERE file_path = ?1",
            schema_guard::TABLE_COGNITIVE_ENRICHMENT
        );
        let mut stmt = self.conn.prepare(&sql)?;
        let mut rows = stmt.query(params![file_path])?;
        if let Some(row) = rows.next()? {
            Ok(Some((
                row.get(0)?,
                row.get(1)?,
                row.get(2)?,
                row.get(3)?,
                row.get(4)?,
            )))
        } else {
            Ok(None)
        }
    }
}
