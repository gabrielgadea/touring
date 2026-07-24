//! Bridge between touring-hooks knowledge DB and touring-cognitive's KnowledgeSource trait.
//!
//! The `KnowledgeSource` impl for `ThreadSafeKnowledgeDB` now lives in
//! `touring-storage::knowledge::cognitive_bridge` (where `ThreadSafeKnowledgeDB`
//! is defined), satisfying the orphan rule. Callers in `touring-hook-runtime`
//! coerce `ThreadSafeKnowledgeDB` to `Arc<dyn KnowledgeSource>` transparently —
//! the impl is carried by the type regardless of which crate re-exports it.

#[cfg(test)]
mod tests {
    use crate::knowledge::{FileRelation, ThreadSafeKnowledgeDB};
    use tempfile::TempDir;
    use touring_intelligence::reasoning::bridge::KnowledgeSource;

    fn make_tsdb() -> (TempDir, ThreadSafeKnowledgeDB) {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("test.db");
        let tsdb = ThreadSafeKnowledgeDB::new(&db_path).unwrap();
        (dir, tsdb)
    }

    #[test]
    fn test_knowledge_source_empty_db() {
        let (_dir, tsdb) = make_tsdb();
        let ks: &dyn KnowledgeSource = &tsdb;
        assert_eq!(ks.file_relations().len(), 0);
        assert_eq!(ks.recent_bash_outcomes(10).len(), 0);
        assert_eq!(ks.coedit_pairs().len(), 0);
        assert_eq!(ks.recent_edits(10).len(), 0);
        assert_eq!(ks.file_count(), 0);
        assert_eq!(ks.relation_count(), 0);
    }

    #[test]
    fn test_knowledge_source_file_relations() {
        let (_dir, tsdb) = make_tsdb();
        tsdb.with(|db| {
            db.upsert_relation(&FileRelation {
                source: "src/a.py".into(),
                target: "src/b.py".into(),
                relation_type: "imports".into(),
            })
            .unwrap();
            db.upsert_relation(&FileRelation {
                source: "src/a.py".into(),
                target: "src/c.py".into(),
                relation_type: "imports".into(),
            })
            .unwrap();
        })
        .unwrap();

        let ks: &dyn KnowledgeSource = &tsdb;
        let rels = ks.file_relations();
        assert_eq!(rels.len(), 2);
    }

    #[test]
    fn test_knowledge_source_file_risk_default() {
        let (_dir, tsdb) = make_tsdb();
        let ks: &dyn KnowledgeSource = &tsdb;
        let risk = ks.file_risk("nonexistent.py");
        assert_eq!(risk.risk_score, 0.0);
        assert_eq!(risk.gotcha_count, 0);
    }

    #[test]
    fn test_knowledge_source_gotchas() {
        let (_dir, tsdb) = make_tsdb();
        tsdb.with(|db| {
            db.add_gotcha("src/main.py", "Watch out: circular import", "warning", None)
                .unwrap();
        })
        .unwrap();

        let ks: &dyn KnowledgeSource = &tsdb;
        let gotchas = ks.gotchas_for_file("src/main.py");
        assert_eq!(gotchas.len(), 1);
        assert_eq!(gotchas[0].severity, "warning");
    }

    #[test]
    fn test_knowledge_source_dependents() {
        let (_dir, tsdb) = make_tsdb();
        tsdb.with(|db| {
            db.upsert_relation(&FileRelation {
                source: "src/a.py".into(),
                target: "src/b.py".into(),
                relation_type: "imports".into(),
            })
            .unwrap();
        })
        .unwrap();

        let ks: &dyn KnowledgeSource = &tsdb;
        let deps = ks.dependents_of("src/b.py");
        assert_eq!(deps.len(), 1);
        assert_eq!(deps[0], "src/a.py");
    }
}
