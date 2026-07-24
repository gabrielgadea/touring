//! Stratified Session Context — Stable session cache pattern (E19).
//!
//! Project-level context computed once at session-start and reused across all
//! subsequent hook invocations. Avoids redundant DB queries for stable
//! information (CILA level, knowledge stats, wiring summary).
//!
//! ## Lifecycle
//!
//! 1. **session-start**: `StableSessionContext::compute()` runs DB queries once
//! 2. **pre_edit / pre_read / pre_write / pre_bash**: consume cached values
//! 3. **session-stop**: context is dropped with the runtime
//!
//! Hooks MUST fall back to direct DB queries when `stable_session` is `None`
//! (cold-start, standalone mode, or daemon restart mid-session).

use crate::knowledge::FileKnowledgeDB;

/// Project-level context computed once at session-start and reused across hook
/// invocations. Reduces redundant DB queries for stable information.
#[derive(Clone, Debug)]
pub struct StableSessionContext {
    /// CILA complexity level for this session (0-6).
    /// Cached from the classifier result at session-start.
    pub cila_level: u8,

    /// Knowledge DB stats snapshot — file_count, relation_count, etc.
    /// Avoids 6 COUNT(*) queries on every hook invocation.
    pub file_count: i64,
    /// Cached count of file-to-file relations in the knowledge DB.
    pub relation_count: i64,
    /// Cached count of recorded edit events in the knowledge DB.
    pub edit_count: i64,
    /// Cached count of gotcha rules in the knowledge DB.
    pub gotcha_count: i64,

    /// Dominant project language (e.g. "rust", "python", "typescript").
    /// Determined from the most frequent language in file_knowledge.
    pub project_language: Option<String>,

    /// Symbol count from SymbolStore (if available).
    pub symbol_count: usize,

    /// Compact wiring summary: "{module_count} modules, {orphan_count} orphans".
    /// `None` if wiring data is unavailable.
    pub wiring_summary: Option<String>,

    /// Unix timestamp (seconds) when this context was computed.
    pub computed_at: u64,
}

impl Default for StableSessionContext {
    fn default() -> Self {
        Self {
            cila_level: 3, // default mid-level
            file_count: 0,
            relation_count: 0,
            edit_count: 0,
            gotcha_count: 0,
            project_language: None,
            symbol_count: 0,
            wiring_summary: None,
            computed_at: 0,
        }
    }
}

impl StableSessionContext {
    /// Compute stable context from the knowledge DB and optional symbol store.
    ///
    /// This runs at most 3 queries (stats + dominant language + wiring counts)
    /// and caches the results for the entire session. Any individual query
    /// failure is logged and skipped — the field keeps its default value.
    pub fn compute(
        knowledge: &FileKnowledgeDB,
        cila_level: u8,
        symbol_count: usize,
        wiring_summary: Option<String>,
    ) -> Self {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let (file_count, relation_count, edit_count, gotcha_count) = match knowledge.stats() {
            Ok(stats) => (
                stats.file_count,
                stats.relation_count,
                stats.edit_count,
                stats.gotcha_count,
            ),
            Err(e) => {
                tracing::debug!(error = %e, "E19: stats query failed during stable context compute");
                (0, 0, 0, 0)
            }
        };

        let project_language = query_dominant_language(knowledge);

        Self {
            cila_level,
            file_count,
            relation_count,
            edit_count,
            gotcha_count,
            project_language,
            symbol_count,
            wiring_summary,
            computed_at: now,
        }
    }

    /// Convenience: get the CILA level as a usize (common in signal pipeline).
    pub fn cila_level_usize(&self) -> usize {
        self.cila_level as usize
    }

    /// How many seconds since this context was computed.
    /// Returns 0 if the clock went backwards or `computed_at` is 0.
    pub fn age_secs(&self) -> u64 {
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        now.saturating_sub(self.computed_at)
    }
}

/// Query the most frequent language from file_knowledge.
/// Returns `None` if the table is empty or the query fails.
fn query_dominant_language(knowledge: &FileKnowledgeDB) -> Option<String> {
    // Use the raw connection for a simple aggregate query.
    let result: Result<Option<String>, _> = knowledge.conn_ref().query_row(
        "SELECT language FROM file_knowledge \
         WHERE language IS NOT NULL AND language != '' \
         GROUP BY language ORDER BY COUNT(*) DESC LIMIT 1",
        [],
        |row| row.get(0),
    );
    match result {
        Ok(lang) => lang,
        Err(rusqlite::Error::QueryReturnedNoRows) => None,
        Err(e) => {
            tracing::debug!(error = %e, "E19: dominant language query failed");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, FileKnowledgeDB) {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("test_knowledge.db");
        let db = FileKnowledgeDB::new(&db_path).unwrap();
        (tmp, db)
    }

    #[test]
    fn test_compute_empty_db() {
        let (_tmp, db) = setup();
        let ctx = StableSessionContext::compute(&db, 2, 0, None);

        assert_eq!(ctx.cila_level, 2);
        assert_eq!(ctx.file_count, 0);
        assert_eq!(ctx.relation_count, 0);
        assert_eq!(ctx.edit_count, 0);
        assert_eq!(ctx.gotcha_count, 0);
        assert!(ctx.project_language.is_none());
        assert_eq!(ctx.symbol_count, 0);
        assert!(ctx.wiring_summary.is_none());
        assert!(ctx.computed_at > 0);
    }

    #[test]
    fn test_compute_with_knowledge_data() {
        let (_tmp, db) = setup();

        // Populate some files
        let k1 = crate::knowledge::FileKnowledge {
            file_path: "src/main.rs".into(),
            language: Some("rust".into()),
            line_count: 100,
            symbol_count: 5,
            read_count: 2,
            last_read_at: Some("2024-01-01".into()),
            content_hash: Some("abc".into()),
            imports_json: None,
            symbols_json: None,
            notes: None,
        };
        let k2 = crate::knowledge::FileKnowledge {
            file_path: "src/lib.rs".into(),
            language: Some("rust".into()),
            line_count: 50,
            symbol_count: 3,
            read_count: 1,
            last_read_at: Some("2024-01-01".into()),
            content_hash: Some("def".into()),
            imports_json: None,
            symbols_json: None,
            notes: None,
        };
        let k3 = crate::knowledge::FileKnowledge {
            file_path: "script.py".into(),
            language: Some("python".into()),
            line_count: 20,
            symbol_count: 1,
            read_count: 1,
            last_read_at: Some("2024-01-01".into()),
            content_hash: Some("ghi".into()),
            imports_json: None,
            symbols_json: None,
            notes: None,
        };
        db.upsert(&k1).expect("upsert k1");
        db.upsert(&k2).expect("upsert k2");
        db.upsert(&k3).expect("upsert k3");

        let ctx = StableSessionContext::compute(&db, 4, 42, Some("10 modules, 3 orphans".into()));

        assert_eq!(ctx.cila_level, 4);
        assert_eq!(ctx.file_count, 3);
        assert_eq!(ctx.symbol_count, 42);
        assert_eq!(ctx.project_language, Some("rust".into())); // 2 rust vs 1 python
        assert_eq!(ctx.wiring_summary, Some("10 modules, 3 orphans".into()));
        assert!(ctx.computed_at > 0);
    }

    #[test]
    fn test_default_matches_expected() {
        let ctx = StableSessionContext::default();
        assert_eq!(ctx.cila_level, 3);
        assert_eq!(ctx.file_count, 0);
        assert!(ctx.project_language.is_none());
        assert_eq!(ctx.computed_at, 0);
    }

    #[test]
    fn test_cila_level_usize() {
        let ctx = StableSessionContext {
            cila_level: 5,
            ..Default::default()
        };
        assert_eq!(ctx.cila_level_usize(), 5);
    }

    #[test]
    fn test_age_secs_nonzero_for_fresh() {
        let (_tmp, db) = setup();
        let ctx = StableSessionContext::compute(&db, 0, 0, None);
        // Just computed — age should be 0 or very close
        assert!(
            ctx.age_secs() <= 2,
            "age_secs should be ~0, got {}",
            ctx.age_secs()
        );
    }

    #[test]
    fn test_age_secs_for_old_context() {
        let ctx = StableSessionContext {
            computed_at: 1000, // ancient timestamp
            ..Default::default()
        };
        // Should be a very large number (current epoch - 1000)
        assert!(ctx.age_secs() > 1_000_000);
    }

    #[test]
    fn test_dominant_language_empty_db() {
        let (_tmp, db) = setup();
        assert!(query_dominant_language(&db).is_none());
    }
}
