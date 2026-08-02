//! Enriched-query and file-risk scoring for `FileKnowledgeDB`
//! (`query_extended` LEFT-JOIN enrichment, decayed error history, per-file risk).
//!
//! Method group extracted verbatim from `knowledge.rs` (1A god-file decomposition);
//! a child-module inherent `impl` block over the parent's `FileKnowledgeDB`.

use super::*;
use rusqlite::params;

impl FileKnowledgeDB {
    /// Query file metadata with full enrichment from all specialized tables.
    ///
    /// Uses LEFT JOINs to include data from cognitive_enrichment, module_ecosystem,
    /// file_blake3_registry, file_test_coverage, and file_communities tables.
    /// Returns None if the file is not in the base file_knowledge table.
    pub fn query_extended(
        &self,
        file_path: &str,
    ) -> Result<Option<FileKnowledgeEnriched>, rusqlite::Error> {
        if let Some(cached) = self.extended_cache.get(file_path) {
            return Ok(Some((*cached).clone()));
        }
        let sql = format!(
            "SELECT
                fk.file_path, fk.language, fk.line_count, fk.symbol_count,
                fk.read_count, fk.last_read_at, fk.content_hash,
                fk.imports_json, fk.symbols_json, fk.notes,
                ce.cognitive_score, ce.complexity_signal,
                ce.fan_in_signal, ce.fan_out_signal, ce.doc_signal,
                me.integration_score, me.pub_symbol_count,
                me.import_count, me.re_export_count,
                fb.blake3_hash,
                ft.coverage_pct,
                fc.community_id, fc.modularity_score
            FROM {fk} fk
            LEFT JOIN {ce} ce ON ce.file_path = fk.file_path
            LEFT JOIN {me} me ON me.file_path = fk.file_path
            LEFT JOIN {fb} fb ON fb.file_path = fk.file_path
            LEFT JOIN {ft} ft ON ft.file_path = fk.file_path
            LEFT JOIN {fc} fc ON fc.file_path = fk.file_path
            WHERE fk.file_path = ?1",
            fk = schema_guard::TABLE_FILE_KNOWLEDGE,
            ce = schema_guard::TABLE_COGNITIVE_ENRICHMENT,
            me = schema_guard::TABLE_MODULE_ECOSYSTEM,
            fb = schema_guard::TABLE_FILE_BLAKE3_REGISTRY,
            ft = schema_guard::TABLE_FILE_TEST_COVERAGE,
            fc = schema_guard::TABLE_FILE_COMMUNITIES,
        );
        self.conn
            .query_row(&sql, [file_path], |row| {
                Ok(FileKnowledgeEnriched {
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
                    cognitive_score: row.get(10)?,
                    complexity_signal: row.get(11)?,
                    fan_in_signal: row.get(12)?,
                    fan_out_signal: row.get(13)?,
                    doc_signal: row.get(14)?,
                    integration_score: row.get(15)?,
                    pub_symbol_count: row.get(16)?,
                    import_count: row.get(17)?,
                    re_export_count: row.get(18)?,
                    blake3_hash: row.get(19)?,
                    coverage_pct: row.get(20)?,
                    community_id: row.get(21)?,
                    modularity_score: row.get(22)?,
                })
            })
            .optional()
            .map(|opt| {
                if let Some(ref enriched) = opt {
                    self.extended_cache
                        .insert(file_path.to_string(), Arc::new(enriched.clone()));
                }
                opt
            })
    }
    /// Query error patterns with exponential decay weighting.
    ///
    /// Uses the formula: `weight = exp(-ln(2) * age_days / half_life_days)`
    /// where `age_days` is the time since the edit. This naturally weights
    /// recent errors higher while still considering older systemic patterns.
    ///
    /// Returns patterns with weighted_count > 1.5 (approximately: 2+ recent
    /// or 3+ older occurrences), sorted by weighted_count descending.
    ///
    /// **Implementation note**: Decay is computed in Rust (not SQL) because
    /// SQLite's `EXP()` requires the math extension which may not be available
    /// in all bundled builds.
    pub fn recent_errors_with_decay(
        &self,
        file_path: &str,
        half_life_days: f64,
    ) -> Vec<WeightedErrorPattern> {
        let decay_sql = format!(
            "SELECT error_pattern, language,
                    julianday('now') - julianday(edited_at) as age_days
             FROM {}
             WHERE file_path = ?1 AND error_pattern IS NOT NULL
             ORDER BY id DESC
             LIMIT 200",
            schema_guard::TABLE_EDIT_HISTORY
        );
        let mut stmt = match self.conn.prepare(&decay_sql) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };
        let decay_constant = (2.0_f64).ln() / half_life_days;
        let rows = match stmt.query_map(params![file_path], |row| {
            let pattern: String = row.get(0)?;
            let language: Option<String> = row.get(1)?;
            let age_days: f64 = row.get::<_, f64>(2).unwrap_or(0.0);
            Ok((pattern, language, age_days))
        }) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };
        use std::collections::HashMap;
        let mut aggregates: HashMap<(String, Option<String>), f64> = HashMap::new();
        for row in rows.flatten() {
            let (pattern, language, age_days) = row;
            let weight = (-decay_constant * age_days).exp();
            *aggregates.entry((pattern, language)).or_insert(0.0) += weight;
        }
        let mut results: Vec<WeightedErrorPattern> = aggregates
            .into_iter()
            .filter(|(_, w)| *w > 1.5)
            .map(
                |((pattern, language), weighted_count)| WeightedErrorPattern {
                    pattern,
                    language,
                    weighted_count,
                },
            )
            .collect();
        results.sort_by(|a, b| {
            b.weighted_count
                .partial_cmp(&a.weighted_count)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        results.truncate(10);
        results
    }
    /// Query the file risk score from `file_risk_scores` table.
    ///
    /// Returns the historical failure rate (0.0-1.0) for a file based on
    /// file_edit_history × bash_outcomes correlation. Returns 0.0 if the table
    /// doesn't exist or the file has no risk data.
    /// Increment file risk score for a file after a tool failure.
    ///
    /// If the file exists in `file_risk_scores`, increments `edits_with_failure`
    /// and recalculates `failure_rate`. If not, creates a new entry.
    /// Called by PostToolUseFailureHandler for real-time risk updates.
    pub fn increment_file_risk(&self, file_path: &str) -> Result<(), rusqlite::Error> {
        let update_sql = format!(
            "UPDATE {} SET
                edits_with_failure = edits_with_failure + 1,
                total_edits = total_edits + 1,
                failure_rate = CAST(edits_with_failure + 1 AS REAL) / (total_edits + 1),
                last_updated = strftime('%s', 'now')
             WHERE file_path = ?1",
            schema_guard::TABLE_FILE_RISK_SCORES
        );
        let updated = self.conn.execute(&update_sql, params![file_path])?;
        if updated == 0 {
            let insert_sql = format!(
                "INSERT OR IGNORE INTO {}
                    (file_path, total_edits, edits_with_failure, failure_rate, last_updated)
                 VALUES (?1, 1, 1, 1.0, strftime('%s', 'now'))",
                schema_guard::TABLE_FILE_RISK_SCORES
            );
            let _ = self.conn.execute(&insert_sql, params![file_path]);
        }
        Ok(())
    }
    /// Returns the accumulated edit-failure risk score for the given file.
    pub fn file_risk_score(&self, file_path: &str) -> f64 {
        let sql = format!(
            "SELECT failure_rate FROM {} WHERE file_path = ?1",
            schema_guard::TABLE_FILE_RISK_SCORES
        );
        self.conn
            .query_row(&sql, params![file_path], |row| row.get(0))
            .unwrap_or(0.0)
    }
}
