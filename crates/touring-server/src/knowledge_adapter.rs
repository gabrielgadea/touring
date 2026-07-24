//! KnowledgeSource adapter — bridges touring-hooks' FileKnowledgeDB
//! to touring-cognitive's KnowledgeSource trait.
//!
//! This adapter lives in touring-server because it depends on both
//! touring-hooks (data source) and touring-cognitive (trait definition).
//! It maps field names and types between the two crates.

use std::sync::Mutex;

use touring_hooks::FileKnowledgeDB;
use touring_intelligence::reasoning::bridge::{
    BashOutcomeRecord, CoEditPair, EditRecord, FileRelation, FileRisk, GotchaRecord,
    KnowledgeSource,
};

/// Adapter that implements `KnowledgeSource` by delegating to `FileKnowledgeDB`.
///
/// Wraps `FileKnowledgeDB` in a `Mutex` because `rusqlite::Connection` is not `Sync`.
/// The Mutex adds ~20ns overhead per call — negligible vs SQLite query time.
#[derive(Debug)]
pub struct KnowledgeAdapter {
    db: Mutex<FileKnowledgeDB>,
}

impl KnowledgeAdapter {
    /// Create a new adapter wrapping a FileKnowledgeDB instance.
    pub fn new(db: FileKnowledgeDB) -> Self {
        Self { db: Mutex::new(db) }
    }

    /// Lock the database, recovering from poisoned state.
    fn lock_db(&self) -> std::sync::MutexGuard<'_, FileKnowledgeDB> {
        self.db.lock().unwrap_or_else(|e| e.into_inner())
    }
}

impl KnowledgeSource for KnowledgeAdapter {
    fn file_relations(&self) -> Vec<FileRelation> {
        let db = self.lock_db();
        let stats = match db.stats() {
            Ok(s) => s,
            Err(_) => return vec![],
        };
        if stats.relation_count == 0 {
            return vec![];
        }

        let result: Result<Vec<FileRelation>, _> = db
            .conn_ref()
            .prepare(
                "SELECT source_path, target_path, relation_type FROM file_relations LIMIT 1000",
            )
            .and_then(|mut stmt| {
                let rows = stmt.query_map([], |row| {
                    Ok(FileRelation {
                        source_path: row.get(0)?,
                        target_path: row.get(1)?,
                        relation_type: row.get(2)?,
                    })
                })?;
                rows.collect()
            });

        result.unwrap_or_default()
    }

    fn recent_bash_outcomes(&self, limit: usize) -> Vec<BashOutcomeRecord> {
        let db = self.lock_db();
        let result: Result<Vec<BashOutcomeRecord>, _> = db
            .conn_ref()
            .prepare(
                "SELECT command_short, exit_code, success, error_pattern, file_context
             FROM bash_outcomes ORDER BY id DESC LIMIT ?1",
            )
            .and_then(|mut stmt| {
                let rows = stmt.query_map(rusqlite::params![limit as i64], |row| {
                    Ok(BashOutcomeRecord {
                        command_short: row.get(0)?,
                        exit_code: row.get(1)?,
                        success: row.get(2)?,
                        error_pattern: row.get(3)?,
                        file_context: row.get(4)?,
                    })
                })?;
                rows.collect()
            });

        result.unwrap_or_default()
    }

    fn coedit_pairs(&self) -> Vec<CoEditPair> {
        let db = self.lock_db();
        let result: Result<Vec<CoEditPair>, _> = db
            .conn_ref()
            .prepare(
                "SELECT source_path, target_path, coedit_count
             FROM file_coedits WHERE coedit_count > 0 LIMIT 500",
            )
            .and_then(|mut stmt| {
                let rows = stmt.query_map([], |row| {
                    let count: i64 = row.get(2)?;
                    Ok(CoEditPair {
                        file1: row.get(0)?,
                        file2: row.get(1)?,
                        weight: count as f64,
                    })
                })?;
                rows.collect()
            });

        result.unwrap_or_default()
    }

    fn gotchas_for_file(&self, file_path: &str) -> Vec<GotchaRecord> {
        let db = self.lock_db();
        db.get_gotchas_for_file(file_path)
            .into_iter()
            .map(|g| GotchaRecord {
                pattern: g.pattern,
                gotcha: g.gotcha,
                severity: g.severity,
                hit_count: g.hit_count,
            })
            .collect()
    }

    fn recent_edits(&self, limit: usize) -> Vec<EditRecord> {
        let db = self.lock_db();
        match db.recent_edits_all(limit) {
            Ok(edits) => edits
                .into_iter()
                .map(|e| EditRecord {
                    file_path: e.file_path,
                    edit_type: e.edit_type,
                    error_pattern: e.error_pattern,
                    edited_at: e.edited_at,
                })
                .collect(),
            Err(_) => vec![],
        }
    }

    fn file_risk(&self, file_path: &str) -> FileRisk {
        let db = self.lock_db();
        let risk_score = db.file_risk_score(file_path);
        let gotchas = db.get_gotchas_for_file(file_path);
        let dependents = db.get_dependents(file_path).unwrap_or_default();

        // Count recent failures from bash_outcomes + edit_history
        let recent_failures: u32 = db
            .conn_ref()
            .query_row(
                "SELECT COUNT(*) FROM bash_outcomes
                 WHERE file_context = ?1 AND success = 0
                 AND executed_at > datetime('now', '-7 days')",
                rusqlite::params![file_path],
                |row| row.get::<_, i64>(0),
            )
            .unwrap_or(0) as u32;

        FileRisk {
            risk_score,
            recent_failures,
            gotcha_count: gotchas.len() as u32,
            dependent_count: dependents.len() as u32,
        }
    }

    fn dependents_of(&self, file_path: &str) -> Vec<String> {
        let db = self.lock_db();
        db.get_dependents(file_path)
            .unwrap_or_default()
            .into_iter()
            .map(|r| r.source)
            .collect()
    }

    fn file_count(&self) -> usize {
        let db = self.lock_db();
        db.stats().map(|s| s.file_count as usize).unwrap_or(0)
    }

    fn relation_count(&self) -> usize {
        let db = self.lock_db();
        db.stats().map(|s| s.relation_count as usize).unwrap_or(0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use tempfile::TempDir;

    fn setup_db(dir: &Path) -> FileKnowledgeDB {
        FileKnowledgeDB::new(&dir.join("test.db")).unwrap()
    }

    #[test]
    fn test_adapter_empty_db() {
        let tmp = TempDir::new().unwrap();
        let db = setup_db(tmp.path());
        let adapter = KnowledgeAdapter::new(db);

        assert_eq!(adapter.file_count(), 0);
        assert_eq!(adapter.relation_count(), 0);
        assert!(adapter.file_relations().is_empty());
        assert!(adapter.coedit_pairs().is_empty());
        assert!(adapter.recent_bash_outcomes(10).is_empty());
        assert!(adapter.recent_edits(10).is_empty());
        assert!(adapter.gotchas_for_file("any.py").is_empty());
        assert!(adapter.dependents_of("any.py").is_empty());

        let risk = adapter.file_risk("any.py");
        assert_eq!(risk.risk_score, 0.0);
    }

    #[test]
    fn test_adapter_with_file_knowledge() {
        let tmp = TempDir::new().unwrap();
        let db = setup_db(tmp.path());
        let fk = touring_hooks::FileKnowledge {
            file_path: "src/main.py".into(),
            language: Some("python".into()),
            line_count: 100,
            symbol_count: 5,
            read_count: 1,
            symbols_json: Some(r#"["main","process"]"#.into()),
            imports_json: Some(r#"["os","sys"]"#.into()),
            ..Default::default()
        };
        db.upsert(&fk).unwrap();

        let adapter = KnowledgeAdapter::new(db);
        assert_eq!(adapter.file_count(), 1);
    }

    #[test]
    fn test_adapter_gotchas_mapping() {
        let tmp = TempDir::new().unwrap();
        let db = setup_db(tmp.path());
        db.conn_ref()
            .execute(
                "INSERT INTO gotchas (pattern, gotcha, severity, hit_count, prevented_errors, created_at)
                 VALUES ('main.py', 'Watch for circular imports', 'warning', 3, 1, datetime('now'))",
                [],
            )
            .unwrap();

        let adapter = KnowledgeAdapter::new(db);
        let gotchas = adapter.gotchas_for_file("src/main.py");
        assert_eq!(gotchas.len(), 1);
        assert_eq!(gotchas[0].gotcha, "Watch for circular imports");
        assert_eq!(gotchas[0].severity, "warning");
        assert_eq!(gotchas[0].hit_count, 3);
    }

    #[test]
    fn test_adapter_implements_knowledge_source_trait() {
        let tmp = TempDir::new().unwrap();
        let db = setup_db(tmp.path());
        let adapter = KnowledgeAdapter::new(db);

        // Verify the adapter can be used as a KnowledgeSource trait object
        let source: &dyn KnowledgeSource = &adapter;
        assert_eq!(source.file_count(), 0);
    }

    #[test]
    fn test_adapter_is_send_sync() {
        fn assert_send_sync<T: Send + Sync>() {}
        assert_send_sync::<KnowledgeAdapter>();
    }
}
