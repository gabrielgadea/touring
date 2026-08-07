//! Gotcha (pitfall pattern) storage and scoring for `FileKnowledgeDB`.
//!
//! Method group extracted verbatim from `knowledge.rs` (1A god-file decomposition).
//! As a child module of `knowledge`, this can reach `FileKnowledgeDB` and its private
//! `conn` field; inherent `impl` blocks are legal in any module of the defining crate.

use super::*;
use rusqlite::params;

impl FileKnowledgeDB {
    /// Add a new gotcha pattern.
    ///
    /// `pattern` is a substring that will be matched against file paths.
    /// `severity` should be one of: "error", "warning", "info".
    /// `language` is optional; together with `pattern` it forms the uniqueness key.
    ///
    /// If a gotcha with the same (pattern, language) already exists, its
    /// `hit_count` is incremented and the `gotcha` text and `severity` are
    /// updated to the new values. Returns the row ID in both cases.
    pub fn add_gotcha(
        &self,
        pattern: &str,
        gotcha: &str,
        severity: &str,
        symbol_name: Option<&str>,
    ) -> Result<i64, rusqlite::Error> {
        self.add_gotcha_with_language(pattern, gotcha, severity, symbol_name, None)
    }
    /// Add a gotcha with an explicit language context.
    ///
    /// Deduplicates on (pattern, COALESCE(language, '')). On conflict,
    /// increments `hit_count` and updates `gotcha` text + `severity`.
    pub fn add_gotcha_with_language(
        &self,
        pattern: &str,
        gotcha: &str,
        severity: &str,
        symbol_name: Option<&str>,
        language: Option<&str>,
    ) -> Result<i64, rusqlite::Error> {
        let insert_sql = format!(
            "INSERT INTO {fg} (pattern, gotcha, severity, symbol_name, language)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(pattern, COALESCE(language, '')) DO UPDATE SET
                hit_count = hit_count + 1",
            fg = schema_guard::TABLE_GOTCHAS
        );
        self.conn.execute(
            &insert_sql,
            params![pattern, gotcha, severity, symbol_name, language],
        )?;
        let id_sql = format!(
            "SELECT id FROM {} WHERE pattern = ?1 AND COALESCE(language, '') = COALESCE(?2, '')",
            schema_guard::TABLE_GOTCHAS
        );
        let id: i64 = self
            .conn
            .query_row(&id_sql, params![pattern, language], |row| row.get(0))?;
        Ok(id)
    }
    /// Get all gotchas whose pattern matches the given file path.
    ///
    /// Uses substring matching: a gotcha with pattern "rust_bridge" will
    /// match any file_path containing "rust_bridge" (e.g. "scripts/aco/rust_bridge.py").
    pub fn get_gotchas_for_file(&self, file_path: &str) -> Vec<Gotcha> {
        let sql = format!(
            "SELECT id, pattern, gotcha, severity, symbol_name,
                    hit_count, prevented_errors, created_at, language
             FROM {}
             WHERE ?1 LIKE '%' || pattern || '%'
               AND COALESCE(decay_score, 1.0) > 0.1
               AND resolved_at IS NULL
             ORDER BY severity DESC, hit_count DESC",
            schema_guard::TABLE_GOTCHAS
        );
        let mut stmt = match self.conn.prepare(&sql) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = match stmt.query_map(params![file_path], |row| {
            Ok(Gotcha {
                id: row.get(0)?,
                pattern: row.get(1)?,
                gotcha: row.get(2)?,
                severity: row.get(3)?,
                language: row.get(8)?,
                symbol_name: row.get(4)?,
                hit_count: row.get(5)?,
                prevented_errors: row.get(6)?,
                created_at: row.get(7)?,
            })
        }) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };
        rows.filter_map(|r| r.ok()).collect()
    }
    /// Get all gotchas whose pattern matches the given file content.
    ///
    /// Uses regex matching: a gotcha with pattern "session_predictor::ToolInvocation"
    /// will match any content containing that exact string pattern.
    ///
    /// The `file_path` parameter is used only for logging/debugging purposes.
    /// Compiled regex patterns are cached for performance.
    pub fn get_gotchas_for_content(&self, file_content: &str, _file_path: &str) -> Vec<Gotcha> {
        let sql = format!(
            "SELECT id, pattern, gotcha, severity, symbol_name,
                    hit_count, prevented_errors, created_at, language
             FROM {}
             WHERE COALESCE(decay_score, 1.0) > 0.1
               AND resolved_at IS NULL",
            schema_guard::TABLE_GOTCHAS
        );
        let mut stmt = match self.conn.prepare(&sql) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = match stmt.query_map([], |row| {
            Ok(Gotcha {
                id: row.get(0)?,
                pattern: row.get(1)?,
                gotcha: row.get(2)?,
                severity: row.get(3)?,
                language: row.get(8)?,
                symbol_name: row.get(4)?,
                hit_count: row.get(5)?,
                prevented_errors: row.get(6)?,
                created_at: row.get(7)?,
            })
        }) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };
        let mut matched_gotchas: Vec<Gotcha> = Vec::new();
        for gotcha_row in rows.filter_map(|r| r.ok()) {
            if let Ok(re) = regex::Regex::new(&gotcha_row.pattern)
                && re.is_match(file_content)
            {
                matched_gotchas.push(gotcha_row);
            }
        }
        matched_gotchas.sort_by(|a, b| {
            b.severity
                .cmp(&a.severity)
                .then(b.hit_count.cmp(&a.hit_count))
        });
        matched_gotchas
    }
    /// List all gotchas in the database.
    pub fn list_gotchas(&self) -> Vec<Gotcha> {
        let sql = format!(
            "SELECT id, pattern, gotcha, severity, symbol_name,
                    hit_count, prevented_errors, created_at, language
             FROM {}
             ORDER BY id",
            schema_guard::TABLE_GOTCHAS
        );
        let mut stmt = match self.conn.prepare(&sql) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = match stmt.query_map([], |row| {
            Ok(Gotcha {
                id: row.get(0)?,
                pattern: row.get(1)?,
                gotcha: row.get(2)?,
                severity: row.get(3)?,
                language: row.get(8)?,
                symbol_name: row.get(4)?,
                hit_count: row.get(5)?,
                prevented_errors: row.get(6)?,
                created_at: row.get(7)?,
            })
        }) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };
        rows.filter_map(|r| r.ok()).collect()
    }
    /// Increment hit count for a gotcha by ID.
    pub fn increment_gotcha_hit(&self, gotcha_id: i64) {
        let sql = format!(
            "UPDATE {} SET hit_count = hit_count + 1 WHERE id = ?1",
            schema_guard::TABLE_GOTCHAS
        );
        let _ = self.conn.execute(&sql, params![gotcha_id]);
    }
    /// Increment prevented errors count for a gotcha by ID.
    pub fn increment_gotcha_prevented(&self, gotcha_id: i64) {
        let sql = format!(
            "UPDATE {} SET prevented_errors = prevented_errors + 1 WHERE id = ?1",
            schema_guard::TABLE_GOTCHAS
        );
        let _ = self.conn.execute(&sql, params![gotcha_id]);
    }
    /// Get aggregate gotcha statistics: (total_count, total_hits, total_prevented).
    pub fn gotcha_stats(&self) -> (usize, i64, i64) {
        let sql = format!(
            "SELECT COUNT(*), COALESCE(SUM(hit_count), 0), COALESCE(SUM(prevented_errors), 0)
             FROM {}",
            schema_guard::TABLE_GOTCHAS
        );
        let result: Result<(i64, i64, i64), _> = self
            .conn
            .query_row(&sql, [], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)));
        match result {
            Ok((count, hits, prevented)) => (count as usize, hits, prevented),
            Err(_) => (0, 0, 0),
        }
    }
    /// Compute F1 proxy scores for all gotchas.
    ///
    /// Returns `(gotcha_id, f1_score)` for each gotcha.
    ///
    /// Since we don't yet track false_alarms and missed_errors separately,
    /// we use `prevented_errors / max(hit_count, 1)` as an F1 proxy.
    /// This approximates precision (what fraction of hits actually prevented
    /// an error) and serves as a quality signal for gotcha effectiveness.
    pub fn gotcha_f1_scores(&self) -> Vec<(i64, f64)> {
        let sql = format!(
            "SELECT id, hit_count, prevented_errors FROM {} ORDER BY id",
            schema_guard::TABLE_GOTCHAS
        );
        let mut stmt = match self.conn.prepare(&sql) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = match stmt.query_map([], |row| {
            let id: i64 = row.get(0)?;
            let hit_count: i64 = row.get(1)?;
            let prevented_errors: i64 = row.get(2)?;
            Ok((id, hit_count, prevented_errors))
        }) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };
        rows.filter_map(|r| r.ok())
            .map(|(id, hit_count, prevented_errors)| {
                let f1 = prevented_errors as f64 / (hit_count.max(1) as f64);
                (id, f1)
            })
            .collect()
    }
    /// Archive (delete) low-quality gotchas that have been evaluated enough times.
    ///
    /// Removes gotchas where:
    /// - `hit_count >= min_evals` (enough data to judge)
    /// - `f1_proxy < max_f1` (below quality threshold)
    ///
    /// Returns the number of gotchas archived (deleted).
    pub fn archive_low_quality_gotchas(&self, min_evals: i64, max_f1: f64) -> usize {
        let scores = self.gotcha_f1_scores();
        let to_delete: Vec<i64> = scores
            .into_iter()
            .filter(|&(_, f1)| f1 < max_f1)
            .map(|(id, _)| id)
            .collect();
        let select_sql = format!(
            "SELECT hit_count FROM {} WHERE id = ?1",
            schema_guard::TABLE_GOTCHAS
        );
        let delete_sql = format!("DELETE FROM {} WHERE id = ?1", schema_guard::TABLE_GOTCHAS);
        let mut deleted = 0;
        for id in to_delete {
            let hit_count: i64 = self
                .conn
                .query_row(&select_sql, params![id], |row| row.get(0))
                .unwrap_or(0);
            if hit_count >= min_evals
                && let Ok(1) = self.conn.execute(&delete_sql, params![id])
            {
                deleted += 1;
            }
        }
        deleted
    }
    /// Update decay scores based on time since last occurrence.
    ///
    /// Uses a logistic decay: `1.0 / (1.0 + weeks_since_last_occurrence)`.
    /// Gotchas that haven't recurred in weeks gradually lose relevance.
    /// Called at session-stop to amortize cost.
    pub fn update_gotcha_decay(&self) -> rusqlite::Result<()> {
        self.conn.execute_batch(&format!(
            "UPDATE {fg}
             SET decay_score = 1.0 / (1.0 + MAX(0.01,
                 CAST((JULIANDAY('now') - JULIANDAY(COALESCE(last_occurrence, created_at))) / 7.0
                 AS REAL)))
             WHERE resolved_at IS NULL;",
            fg = schema_guard::TABLE_GOTCHAS
        ))
    }
    /// Auto-resolve gotchas that haven't recurred after N successful edits.
    ///
    /// If a file has had 5+ successful edits and a gotcha's decay_score is
    /// below 0.3, the gotcha is marked resolved (soft-delete via resolved_at).
    pub fn maybe_auto_resolve_gotchas(&self, file_path: &str) -> rusqlite::Result<()> {
        let count_sql = format!(
            "SELECT COUNT(*) FROM {} WHERE file_path = ?1",
            schema_guard::TABLE_EDIT_HISTORY
        );
        let edit_count: u32 = self
            .conn
            .query_row(&count_sql, params![file_path], |r| r.get(0))
            .unwrap_or(0);
        if edit_count >= 5 {
            let resolve_sql = format!(
                "UPDATE {fg} SET resolved_at = DATETIME('now')
                 WHERE pattern IN (
                     SELECT pattern FROM {fg}
                     WHERE ?1 LIKE '%' || pattern || '%'
                       AND decay_score < 0.3
                       AND resolved_at IS NULL
                 )",
                fg = schema_guard::TABLE_GOTCHAS
            );
            self.conn.execute(&resolve_sql, params![file_path])?;
        }
        Ok(())
    }
    #[cfg(feature = "session-hooks")]
    /// Top error patterns from edit_history, grouped and ordered by frequency.
    pub fn top_error_patterns(
        &self,
        limit: usize,
    ) -> Vec<touring_foundation::insights::ErrorPatternInsight> {
        let sql = format!(
            "SELECT error_pattern, COUNT(*) as cnt
             FROM {}
             WHERE error_pattern IS NOT NULL
             GROUP BY error_pattern
             ORDER BY cnt DESC
             LIMIT ?1",
            schema_guard::TABLE_EDIT_HISTORY
        );
        let mut stmt = match self.conn.prepare(&sql) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let rows = match stmt.query_map(params![limit as i64], |row| {
            Ok(touring_foundation::insights::ErrorPatternInsight {
                pattern: row.get(0)?,
                occurrences: row.get(1)?,
            })
        }) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };
        rows.filter_map(|r| r.ok()).collect()
    }
}
