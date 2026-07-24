//! KnowledgeSource impl for ThreadSafeKnowledgeDB.
//!
//! `ThreadSafeKnowledgeDB` is defined in this crate (`touring-storage`), so
//! implementing a foreign trait (`KnowledgeSource` from `touring-intelligence`)
//! here is legal under Rust's orphan rules — the local type satisfies E0117.
//!
//! This module is gated on `feature = "knowledge"` (which pulls in
//! `dep:touring-intelligence`).

use touring_foundation::knowledge_source::{
    BashOutcomeRecord, CoEditPair, EditRecord, FileRelation as CogFileRelation, FileRisk,
    GotchaRecord, KnowledgeSource,
};
use touring_foundation::schema_guard;

use super::ThreadSafeKnowledgeDB;

impl KnowledgeSource for ThreadSafeKnowledgeDB {
    fn file_relations(&self) -> Vec<CogFileRelation> {
        self.with(|db| {
            let result = db.conn_ref().prepare(&format!(
                "SELECT source_path, target_path, relation_type FROM {} LIMIT 5000",
                schema_guard::TABLE_FILE_RELATIONS
            ));
            let mut stmt = match result {
                Ok(s) => s,
                Err(_) => return Vec::new(),
            };
            let rows = stmt.query_map([], |row| {
                Ok(CogFileRelation {
                    source_path: row.get(0)?,
                    target_path: row.get(1)?,
                    relation_type: row.get(2)?,
                })
            });
            match rows {
                Ok(r) => r.filter_map(|x| x.ok()).collect(),
                Err(_) => Vec::new(),
            }
        })
        .unwrap_or_default()
    }
    fn recent_bash_outcomes(&self, limit: usize) -> Vec<BashOutcomeRecord> {
        self.with(|db| {
            let result = db.conn_ref().prepare(&format!(
                "SELECT command_short, exit_code, success, error_pattern, file_context \
                 FROM {} ORDER BY executed_at DESC LIMIT ?1",
                schema_guard::TABLE_BASH_OUTCOMES
            ));
            let mut stmt = match result {
                Ok(s) => s,
                Err(_) => return Vec::new(),
            };
            let rows = stmt.query_map(rusqlite::params![limit as i64], |row| {
                Ok(BashOutcomeRecord {
                    command_short: row.get(0)?,
                    exit_code: row.get(1)?,
                    success: row.get::<_, i64>(2)? != 0,
                    error_pattern: row.get(3)?,
                    file_context: row.get(4)?,
                })
            });
            match rows {
                Ok(r) => r.filter_map(|x| x.ok()).collect(),
                Err(_) => Vec::new(),
            }
        })
        .unwrap_or_default()
    }
    fn coedit_pairs(&self) -> Vec<CoEditPair> {
        self.with(|db| {
            let result = db.conn_ref().prepare(&format!(
                "SELECT source_path, target_path, weight FROM {} ORDER BY weight DESC LIMIT 500",
                schema_guard::TABLE_FILE_COEDITS
            ));
            let mut stmt = match result {
                Ok(s) => s,
                Err(_) => return Vec::new(),
            };
            let rows = stmt.query_map([], |row| {
                Ok(CoEditPair {
                    file1: row.get(0)?,
                    file2: row.get(1)?,
                    weight: row.get(2)?,
                })
            });
            match rows {
                Ok(r) => r.filter_map(|x| x.ok()).collect(),
                Err(_) => Vec::new(),
            }
        })
        .unwrap_or_default()
    }
    fn gotchas_for_file(&self, file_path: &str) -> Vec<GotchaRecord> {
        self.with(|db| {
            db.get_gotchas_for_file(file_path)
                .into_iter()
                .map(|g| GotchaRecord {
                    pattern: g.pattern,
                    gotcha: g.gotcha,
                    severity: g.severity,
                    hit_count: g.hit_count,
                })
                .collect()
        })
        .unwrap_or_default()
    }
    fn recent_edits(&self, limit: usize) -> Vec<EditRecord> {
        self.with(|db| match db.recent_edits_all(limit) {
            Ok(edits) => edits
                .into_iter()
                .map(|e| EditRecord {
                    file_path: e.file_path,
                    edit_type: e.edit_type,
                    error_pattern: e.error_pattern,
                    edited_at: e.edited_at,
                })
                .collect(),
            Err(_) => Vec::new(),
        })
        .unwrap_or_default()
    }
    fn file_risk(&self, file_path: &str) -> FileRisk {
        self.with(|db| {
            let risk_score = db.file_risk_score(file_path);
            let gotcha_count = db.get_gotchas_for_file(file_path).len() as u32;
            let dependent_count = match db.get_dependents(file_path) {
                Ok(deps) => deps.len() as u32,
                Err(_) => 0,
            };
            FileRisk {
                risk_score,
                recent_failures: 0,
                gotcha_count,
                dependent_count,
            }
        })
        .unwrap_or_default()
    }
    fn dependents_of(&self, file_path: &str) -> Vec<String> {
        self.with(|db| match db.get_dependents(file_path) {
            Ok(deps) => deps.into_iter().map(|d| d.source).collect(),
            Err(_) => Vec::new(),
        })
        .unwrap_or_default()
    }
    fn file_count(&self) -> usize {
        self.with(|db| match db.stats() {
            Ok(s) => s.file_count as usize,
            Err(_) => 0,
        })
        .unwrap_or(0)
    }
    fn relation_count(&self) -> usize {
        self.with(|db| match db.stats() {
            Ok(s) => s.relation_count as usize,
            Err(_) => 0,
        })
        .unwrap_or(0)
    }
}
