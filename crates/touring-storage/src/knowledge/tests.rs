#![allow(clippy::indexing_slicing)]
use super::*;
use tempfile::TempDir;

fn test_db() -> (TempDir, FileKnowledgeDB) {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test_knowledge.db");
    let db = FileKnowledgeDB::new(&db_path).unwrap();
    (tmp, db)
}

#[test]
fn test_schema_creation() {
    let (_tmp, db) = test_db();
    let stats = db.stats().unwrap();
    assert_eq!(stats.file_count, 0);
    assert_eq!(stats.relation_count, 0);
}

#[test]
fn test_file_knowledge_upsert_and_lookup() {
    let (_tmp, db) = test_db();
    let k = FileKnowledge {
        file_path: "src/main.py".to_string(),
        language: Some("python".to_string()),
        line_count: 100,
        symbol_count: 5,
        content_hash: Some("abc123".to_string()),
        imports_json: Some(r#"["os","sys"]"#.to_string()),
        symbols_json: Some(r#"["main","helper"]"#.to_string()),
        ..Default::default()
    };
    db.upsert(&k).unwrap();
    let result = db.lookup("src/main.py").unwrap().unwrap();
    assert_eq!(result.language.as_deref(), Some("python"));
    assert_eq!(result.line_count, 100);
    assert_eq!(result.read_count, 1);
    db.upsert(&k).unwrap();
    let result2 = db.lookup("src/main.py").unwrap().unwrap();
    assert_eq!(result2.read_count, 2);
}

#[test]
fn test_file_knowledge_lookup_missing() {
    let (_tmp, db) = test_db();
    let result = db.lookup("nonexistent.py").unwrap();
    assert!(result.is_none());
}

#[test]
fn test_file_relations() {
    let (_tmp, db) = test_db();
    let rel = FileRelation {
        source: "a.py".to_string(),
        target: "b.py".to_string(),
        relation_type: "imports".to_string(),
    };
    db.upsert_relation(&rel).unwrap();
    let from = db.get_relations_from("a.py").unwrap();
    assert_eq!(from.len(), 1);
    assert_eq!(from[0].target, "b.py");
    let deps = db.get_dependents("b.py").unwrap();
    assert_eq!(deps.len(), 1);
    assert_eq!(deps[0].source, "a.py");
}

#[test]
fn test_replace_relations() {
    let (_tmp, db) = test_db();
    let r1 = FileRelation {
        source: "a.py".to_string(),
        target: "b.py".to_string(),
        relation_type: "imports".to_string(),
    };
    db.upsert_relation(&r1).unwrap();
    assert_eq!(db.get_relations_from("a.py").unwrap().len(), 1);
    let new_rels = vec![FileRelation {
        source: "a.py".to_string(),
        target: "c.py".to_string(),
        relation_type: "imports".to_string(),
    }];
    db.replace_relations_from("a.py", &new_rels).unwrap();
    let from = db.get_relations_from("a.py").unwrap();
    assert_eq!(from.len(), 1);
    assert_eq!(from[0].target, "c.py");
}

#[test]
fn test_bash_outcomes() {
    let (_tmp, db) = test_db();
    let outcome = BashOutcome {
        command: "ruff check src/main.py".to_string(),
        command_short: "ruff".to_string(),
        command_hash: String::new(),
        exit_code: 1,
        success: false,
        error_pattern: Some("E501 line too long".to_string()),
        file_context: Some("src/main.py".to_string()),
        executed_at: String::new(),
    };
    db.record_bash_outcome(&outcome).unwrap();
    let results = db.find_bash_outcomes("ruff", 5).unwrap();
    assert_eq!(results.len(), 1);
    assert!(!results[0].success);
    assert!(!results[0].command_hash.is_empty());
    let failures = db.recent_failures_for_file("main.py", 5).unwrap();
    assert_eq!(failures.len(), 1);
}

#[test]
fn test_bash_outcome_hash_stored() {
    let (_tmp, db) = test_db();
    let outcome1 = BashOutcome {
        command: "cargo test --all".to_string(),
        command_short: "cargo".to_string(),
        command_hash: String::new(),
        exit_code: 1,
        success: false,
        error_pattern: Some("test failed".to_string()),
        file_context: None,
        executed_at: String::new(),
    };
    db.record_bash_outcome(&outcome1).unwrap();
    let outcome2 = BashOutcome {
        command: "cargo test --all".to_string(),
        command_short: "cargo".to_string(),
        command_hash: String::new(),
        exit_code: 0,
        success: true,
        error_pattern: None,
        file_context: None,
        executed_at: String::new(),
    };
    db.record_bash_outcome(&outcome2).unwrap();
    let results = db.find_bash_outcomes("cargo", 10).unwrap();
    assert_eq!(results.len(), 2);
    let hash = super::sha256_hex("cargo test --all");
    let by_hash = db.find_bash_outcomes_by_hash(&hash, 10).unwrap();
    assert_eq!(
        by_hash.len(),
        2,
        "both runs of the same command found by hash"
    );
}

#[test]
fn test_bash_outcome_different_commands_no_collision() {
    let (_tmp, db) = test_db();
    let long_prefix = "a".repeat(500);
    let cmd1 = format!("{long_prefix}_command_ONE");
    let cmd2 = format!("{long_prefix}_command_TWO");
    let outcome1 = BashOutcome {
        command: cmd1,
        command_short: "aaa".to_string(),
        command_hash: String::new(),
        exit_code: 0,
        success: true,
        error_pattern: None,
        file_context: None,
        executed_at: String::new(),
    };
    let outcome2 = BashOutcome {
        command: cmd2,
        command_short: "aaa".to_string(),
        command_hash: String::new(),
        exit_code: 1,
        success: false,
        error_pattern: None,
        file_context: None,
        executed_at: String::new(),
    };
    db.record_bash_outcome(&outcome1).unwrap();
    db.record_bash_outcome(&outcome2).unwrap();
    let results = db.find_bash_outcomes("aaa", 10).unwrap();
    assert_eq!(results.len(), 2, "different commands must not collide");
}

#[test]
fn test_edit_history() {
    let (_tmp, db) = test_db();
    db.record_edit("src/main.py", "Edit", Some("Fixed import"))
        .unwrap();
    let edits = db.recent_edits("src/main.py", 5).unwrap();
    assert_eq!(edits.len(), 1);
    assert_eq!(edits[0].edit_type, "Edit");
}

#[test]
fn test_file_access_log() {
    let (_tmp, db) = test_db();
    db.record_access("src/main.py", "session-1").unwrap();
    db.record_access("src/main.py", "session-1").unwrap();
    let count = db.access_count("src/main.py").unwrap();
    assert_eq!(count, 2);
}

#[test]
fn test_append_note() {
    let (_tmp, db) = test_db();
    let k = FileKnowledge {
        file_path: "src/main.py".to_string(),
        ..Default::default()
    };
    db.upsert(&k).unwrap();
    db.append_note("src/main.py", "Bug with enum").unwrap();
    let result = db.lookup("src/main.py").unwrap().unwrap();
    assert_eq!(result.notes.as_deref(), Some("Bug with enum"));
    db.append_note("src/main.py", "Fixed in v2").unwrap();
    let result2 = db.lookup("src/main.py").unwrap().unwrap();
    assert_eq!(result2.notes.as_deref(), Some("Bug with enum; Fixed in v2"));
}

#[test]
fn test_stats() {
    let (_tmp, db) = test_db();
    let k = FileKnowledge {
        file_path: "a.py".to_string(),
        ..Default::default()
    };
    db.upsert(&k).unwrap();
    db.upsert_relation(&FileRelation {
        source: "a.py".to_string(),
        target: "b.py".to_string(),
        relation_type: "imports".to_string(),
    })
    .unwrap();
    db.record_access("a.py", "s1").unwrap();
    let stats = db.stats().unwrap();
    assert_eq!(stats.file_count, 1);
    assert_eq!(stats.relation_count, 1);
    assert_eq!(stats.access_count, 1);
}

// ── Gotcha Tests ──────────────────────────────────────────────────

#[test]
fn test_add_gotcha() {
    let (_tmp, db) = test_db();
    let id = db
        .add_gotcha(
            "rust_bridge",
            "Field is m.matched_text NOT m.text",
            "error",
            Some("LegalPatternMatcher"),
        )
        .unwrap();
    assert!(id > 0);
    let all = db.list_gotchas();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].pattern, "rust_bridge");
    assert_eq!(all[0].gotcha, "Field is m.matched_text NOT m.text");
    assert_eq!(all[0].severity, "error");
    assert_eq!(all[0].symbol_name.as_deref(), Some("LegalPatternMatcher"));
    assert_eq!(all[0].hit_count, 0);
    assert_eq!(all[0].prevented_errors, 0);
}

#[test]
fn test_get_gotchas_for_file() {
    let (_tmp, db) = test_db();
    db.add_gotcha("rust_bridge", "Use matched_text not text", "error", None)
        .unwrap();
    let matches = db.get_gotchas_for_file("scripts/aco/rust_bridge.py");
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].gotcha, "Use matched_text not text");
    let matches2 = db.get_gotchas_for_file("packages/kazuba-core/rust_bridge/utils.py");
    assert_eq!(matches2.len(), 1);
}

#[test]
fn test_gotcha_no_match() {
    let (_tmp, db) = test_db();
    db.add_gotcha("rust_bridge", "Use matched_text not text", "error", None)
        .unwrap();
    let matches = db.get_gotchas_for_file("scripts/aco/pipeline.py");
    assert!(
        matches.is_empty(),
        "File without pattern should return empty"
    );
}

#[test]
fn test_gotcha_multiple_matches() {
    let (_tmp, db) = test_db();
    db.add_gotcha(
        "compute_",
        "Always verify Decimal precision",
        "warning",
        None,
    )
    .unwrap();
    db.add_gotcha(
        "vantajosidade",
        "Use py_extract_batch not extract_batch",
        "error",
        None,
    )
    .unwrap();
    let matches =
        db.get_gotchas_for_file("scripts/process_analysis/phases/compute_vantajosidade.py");
    assert_eq!(matches.len(), 2);
    assert_eq!(matches[0].severity, "warning");
    assert_eq!(matches[1].severity, "error");
}

#[test]
fn test_increment_gotcha_hit() {
    let (_tmp, db) = test_db();
    let id = db
        .add_gotcha("bridge", "test gotcha", "info", None)
        .unwrap();
    db.increment_gotcha_hit(id);
    db.increment_gotcha_hit(id);
    db.increment_gotcha_hit(id);
    let all = db.list_gotchas();
    assert_eq!(all[0].hit_count, 3);
}

#[test]
fn test_increment_gotcha_prevented() {
    let (_tmp, db) = test_db();
    let id = db
        .add_gotcha("bridge", "test gotcha", "warning", None)
        .unwrap();
    db.increment_gotcha_prevented(id);
    db.increment_gotcha_prevented(id);
    let all = db.list_gotchas();
    assert_eq!(all[0].prevented_errors, 2);
}

#[test]
fn test_gotcha_stats() {
    let (_tmp, db) = test_db();
    let (total, hits, prevented) = db.gotcha_stats();
    assert_eq!(total, 0);
    assert_eq!(hits, 0);
    assert_eq!(prevented, 0);
    let id1 = db
        .add_gotcha("pattern_a", "gotcha a", "error", None)
        .unwrap();
    let id2 = db
        .add_gotcha("pattern_b", "gotcha b", "warning", None)
        .unwrap();
    db.increment_gotcha_hit(id1);
    db.increment_gotcha_hit(id1);
    db.increment_gotcha_hit(id2);
    db.increment_gotcha_prevented(id1);
    let (total, hits, prevented) = db.gotcha_stats();
    assert_eq!(total, 2);
    assert_eq!(hits, 3);
    assert_eq!(prevented, 1);
}

#[test]
fn test_list_gotchas() {
    let (_tmp, db) = test_db();
    assert!(db.list_gotchas().is_empty());
    db.add_gotcha("p1", "g1", "error", None).unwrap();
    db.add_gotcha("p2", "g2", "warning", Some("SomeSymbol"))
        .unwrap();
    db.add_gotcha("p3", "g3", "info", None).unwrap();
    let all = db.list_gotchas();
    assert_eq!(all.len(), 3);
    assert_eq!(all[0].pattern, "p1");
    assert_eq!(all[1].pattern, "p2");
    assert_eq!(all[2].pattern, "p3");
    assert_eq!(all[1].symbol_name.as_deref(), Some("SomeSymbol"));
    assert!(all[2].symbol_name.is_none());
}

// ── S2.2: Gotcha F1 Confidence Scoring Tests ─────────────────────

#[test]
fn test_gotcha_f1_proxy() {
    let (_tmp, db) = test_db();
    assert!(db.gotcha_f1_scores().is_empty());
    let id1 = db
        .add_gotcha("bridge", "field gotcha", "error", None)
        .unwrap();
    for _ in 0..10 {
        db.increment_gotcha_hit(id1);
    }
    for _ in 0..8 {
        db.increment_gotcha_prevented(id1);
    }
    let id2 = db
        .add_gotcha("compute_", "decimal gotcha", "warning", None)
        .unwrap();
    for _ in 0..5 {
        db.increment_gotcha_hit(id2);
    }
    db.increment_gotcha_prevented(id2);
    let _id3 = db
        .add_gotcha("unused", "never fires", "info", None)
        .unwrap();
    let scores = db.gotcha_f1_scores();
    assert_eq!(scores.len(), 3);
    assert_eq!(scores[0].0, id1);
    assert!(
        (scores[0].1 - 0.8).abs() < 1e-10,
        "f1 for gotcha 1 should be 0.8"
    );
    assert_eq!(scores[1].0, id2);
    assert!(
        (scores[1].1 - 0.2).abs() < 1e-10,
        "f1 for gotcha 2 should be 0.2"
    );
    assert!(
        (scores[2].1 - 0.0).abs() < 1e-10,
        "f1 for gotcha 3 should be 0.0"
    );
}

#[test]
fn test_gotcha_f1_all_prevented() {
    let (_tmp, db) = test_db();
    let id = db
        .add_gotcha("perfect", "always works", "error", None)
        .unwrap();
    for _ in 0..20 {
        db.increment_gotcha_hit(id);
        db.increment_gotcha_prevented(id);
    }
    let scores = db.gotcha_f1_scores();
    assert_eq!(scores.len(), 1);
    assert!(
        (scores[0].1 - 1.0).abs() < 1e-10,
        "perfect gotcha should have f1=1.0"
    );
}

#[test]
fn test_archive_low_quality() {
    let (_tmp, db) = test_db();
    let id_good = db
        .add_gotcha("good_pattern", "useful gotcha", "error", None)
        .unwrap();
    for _ in 0..10 {
        db.increment_gotcha_hit(id_good);
    }
    for _ in 0..9 {
        db.increment_gotcha_prevented(id_good);
    }
    let id_bad = db
        .add_gotcha("bad_pattern", "noisy gotcha", "warning", None)
        .unwrap();
    for _ in 0..10 {
        db.increment_gotcha_hit(id_bad);
    }
    db.increment_gotcha_prevented(id_bad);
    let id_young = db
        .add_gotcha("young_pattern", "new gotcha", "info", None)
        .unwrap();
    db.increment_gotcha_hit(id_young);
    db.increment_gotcha_hit(id_young);
    assert_eq!(db.list_gotchas().len(), 3);
    let archived = db.archive_low_quality_gotchas(5, 0.5);
    assert_eq!(archived, 1, "only bad_pattern should be archived");
    let remaining = db.list_gotchas();
    assert_eq!(remaining.len(), 2);
    assert!(
        remaining.iter().any(|g| g.id == id_good),
        "good gotcha should remain"
    );
    assert!(
        remaining.iter().any(|g| g.id == id_young),
        "young gotcha should remain (not enough evals)"
    );
    assert!(
        !remaining.iter().any(|g| g.id == id_bad),
        "bad gotcha should be deleted"
    );
}

#[test]
fn test_archive_low_quality_empty() {
    let (_tmp, db) = test_db();
    assert_eq!(db.archive_low_quality_gotchas(5, 0.5), 0);
}

#[test]
fn test_archive_low_quality_none_eligible() {
    let (_tmp, db) = test_db();
    let id = db
        .add_gotcha("great", "excellent gotcha", "error", None)
        .unwrap();
    for _ in 0..20 {
        db.increment_gotcha_hit(id);
        db.increment_gotcha_prevented(id);
    }
    let archived = db.archive_low_quality_gotchas(5, 0.5);
    assert_eq!(archived, 0, "high-quality gotchas should not be archived");
    assert_eq!(db.list_gotchas().len(), 1);
}

// ── Co-edit Tracking Tests (S2.1) ────────────────────────────────

#[test]
fn test_coedit_table_creation() {
    let (_tmp, db) = test_db();
    db.record_coedit("a.rs", "b.rs").unwrap();
    let neighbors = db.get_coedit_neighbors("a.rs", 10);
    assert_eq!(neighbors.len(), 1);
    assert_eq!(neighbors[0].0, "b.rs");
    assert_eq!(neighbors[0].1, 1);
}

#[test]
fn test_record_coedit_increments_count() {
    let (_tmp, db) = test_db();
    db.record_coedit("src/lib.rs", "src/main.rs").unwrap();
    db.record_coedit("src/lib.rs", "src/main.rs").unwrap();
    db.record_coedit("src/lib.rs", "src/main.rs").unwrap();
    let neighbors = db.get_coedit_neighbors("src/lib.rs", 10);
    assert_eq!(neighbors.len(), 1);
    assert_eq!(neighbors[0].0, "src/main.rs");
    assert_eq!(neighbors[0].1, 3);
}

#[test]
fn test_get_coedit_neighbors_sorted_by_count() {
    let (_tmp, db) = test_db();
    for _ in 0..5 {
        db.record_coedit("a.rs", "b.rs").unwrap();
    }
    for _ in 0..2 {
        db.record_coedit("a.rs", "c.rs").unwrap();
    }
    for _ in 0..8 {
        db.record_coedit("a.rs", "d.rs").unwrap();
    }
    let neighbors = db.get_coedit_neighbors("a.rs", 10);
    assert_eq!(neighbors.len(), 3);
    assert_eq!(neighbors[0].0, "d.rs");
    assert_eq!(neighbors[0].1, 8);
    assert_eq!(neighbors[1].0, "b.rs");
    assert_eq!(neighbors[1].1, 5);
    assert_eq!(neighbors[2].0, "c.rs");
    assert_eq!(neighbors[2].1, 2);
}

#[test]
fn test_coedit_bidirectional() {
    let (_tmp, db) = test_db();
    for _ in 0..3 {
        db.record_coedit("a.rs", "b.rs").unwrap();
    }
    for _ in 0..2 {
        db.record_coedit("b.rs", "a.rs").unwrap();
    }
    let neighbors_a = db.get_coedit_neighbors("a.rs", 10);
    assert_eq!(neighbors_a.len(), 1);
    assert_eq!(neighbors_a[0].0, "b.rs");
    assert_eq!(neighbors_a[0].1, 5);
    let neighbors_b = db.get_coedit_neighbors("b.rs", 10);
    assert_eq!(neighbors_b.len(), 1);
    assert_eq!(neighbors_b[0].0, "a.rs");
    assert_eq!(neighbors_b[0].1, 5);
}

#[test]
fn test_decay_coedits_reduces_count() {
    let (_tmp, db) = test_db();
    db.conn
        .execute(
            "INSERT INTO file_coedits (source_path, target_path, coedit_count, last_coedit_at)
                 VALUES ('a.rs', 'b.rs', 10, datetime('now', '-30 days'))",
            [],
        )
        .unwrap();
    db.record_coedit("c.rs", "d.rs").unwrap();
    let affected = db.decay_coedits(7.0).unwrap();
    assert!(affected > 0, "old record should be decayed");
    let neighbors = db.get_coedit_neighbors("a.rs", 10);
    assert_eq!(neighbors.len(), 1);
    assert_eq!(neighbors[0].1, 5);
    let neighbors2 = db.get_coedit_neighbors("c.rs", 10);
    assert_eq!(neighbors2.len(), 1);
    assert_eq!(neighbors2[0].1, 1);
}

#[test]
fn test_decay_coedits_deletes_expired() {
    let (_tmp, db) = test_db();
    db.conn
        .execute(
            "INSERT INTO file_coedits (source_path, target_path, coedit_count, last_coedit_at)
                 VALUES ('x.rs', 'y.rs', 1, datetime('now', '-30 days'))",
            [],
        )
        .unwrap();
    let affected = db.decay_coedits(7.0).unwrap();
    assert!(affected > 0);
    let neighbors = db.get_coedit_neighbors("x.rs", 10);
    assert!(neighbors.is_empty(), "expired co-edit should be deleted");
}

#[test]
fn test_recent_accessed_files() {
    let (_tmp, db) = test_db();
    db.record_access("src/a.rs", "session1").unwrap();
    db.record_access("src/b.rs", "session1").unwrap();
    db.record_access("src/c.rs", "session1").unwrap();
    db.record_access("src/a.rs", "session1").unwrap();
    let recent = db.recent_accessed_files("src/a.rs", 5);
    assert!(!recent.is_empty());
    assert!(!recent.contains(&"src/a.rs".to_string()));
    assert!(recent.contains(&"src/b.rs".to_string()) || recent.contains(&"src/c.rs".to_string()));
}

#[test]
fn test_get_coedit_neighbors_top_k_limit() {
    let (_tmp, db) = test_db();
    for i in 0..5 {
        let target = format!("file_{i}.rs");
        for _ in 0..=i {
            db.record_coedit("main.rs", &target).unwrap();
        }
    }
    let neighbors = db.get_coedit_neighbors("main.rs", 2);
    assert_eq!(neighbors.len(), 2);
    assert_eq!(neighbors[0].0, "file_4.rs");
    assert_eq!(neighbors[1].0, "file_3.rs");
}

// ── I7: Gotcha Deduplication Tests ──────────────────────────────

#[test]
fn test_gotcha_dedup_same_pattern_language() {
    let (_tmp, db) = test_db();
    let id1 = db
        .add_gotcha("bridge", "Use matched_text", "error", None)
        .unwrap();
    assert_eq!(db.list_gotchas().len(), 1);
    let id2 = db
        .add_gotcha("bridge", "Updated gotcha text", "warning", None)
        .unwrap();
    assert_eq!(id1, id2, "same pattern+language should return same id");
    let all = db.list_gotchas();
    assert_eq!(all.len(), 1, "duplicate insert must not create second row");
    assert_eq!(
        all[0].gotcha, "Use matched_text",
        "original text should be preserved"
    );
    assert_eq!(
        all[0].severity, "error",
        "original severity should be preserved"
    );
    assert_eq!(
        all[0].hit_count, 1,
        "hit_count should be incremented on conflict"
    );
}

#[test]
fn test_gotcha_dedup_different_language() {
    let (_tmp, db) = test_db();
    let id1 = db
        .add_gotcha_with_language("bridge", "Python gotcha", "error", None, Some("python"))
        .unwrap();
    let id2 = db
        .add_gotcha_with_language("bridge", "Rust gotcha", "error", None, Some("rust"))
        .unwrap();
    assert_ne!(
        id1, id2,
        "different languages should create distinct entries"
    );
    assert_eq!(db.list_gotchas().len(), 2);
}

#[test]
fn test_gotcha_language_field_stored() {
    let (_tmp, db) = test_db();
    db.add_gotcha_with_language("bridge", "gotcha", "error", None, Some("rust"))
        .unwrap();
    let all = db.list_gotchas();
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].language.as_deref(), Some("rust"));
}

// ── I9: Cleanup Old Entries Tests ───────────────────────────────

#[test]
fn test_cleanup_old_entries_deletes_old() {
    let (_tmp, db) = test_db();
    db.conn
        .execute(
            "INSERT INTO file_access_log (file_path, session_id, accessed_at)
                 VALUES ('old.rs', 's1', datetime('now', '-40 days'))",
            [],
        )
        .unwrap();
    db.conn
        .execute(
            "INSERT INTO edit_history (file_path, edit_type, edited_at)
                 VALUES ('old.rs', 'Edit', datetime('now', '-40 days'))",
            [],
        )
        .unwrap();
    db.conn
        .execute(
            "INSERT INTO bash_outcomes (command, command_short, executed_at)
                 VALUES ('old cmd', 'old', datetime('now', '-40 days'))",
            [],
        )
        .unwrap();
    db.record_access("recent.rs", "s2").unwrap();
    db.record_edit("recent.rs", "Edit", Some("recent")).unwrap();
    db.record_bash_outcome(&BashOutcome {
        command: "recent cmd".to_string(),
        command_short: "recent".to_string(),
        command_hash: String::new(),
        exit_code: 0,
        success: true,
        error_pattern: None,
        file_context: None,
        executed_at: String::new(),
    })
    .unwrap();
    let deleted = db.cleanup_old_entries(30).unwrap();
    assert_eq!(deleted, 3, "should delete 3 old entries (1 per table)");
    assert_eq!(db.access_count("recent.rs").unwrap(), 1);
    assert_eq!(db.recent_edits("recent.rs", 10).unwrap().len(), 1);
    assert_eq!(db.find_bash_outcomes("recent", 10).unwrap().len(), 1);
    assert_eq!(db.access_count("old.rs").unwrap(), 0);
}

#[test]
fn test_cleanup_old_entries_no_recent_deleted() {
    let (_tmp, db) = test_db();
    db.record_access("a.rs", "s1").unwrap();
    db.record_edit("a.rs", "Edit", None).unwrap();
    let deleted = db.cleanup_old_entries(30).unwrap();
    assert_eq!(deleted, 0, "recent entries must not be deleted");
}

// ── P1.4: ThreadSafeKnowledgeDB tests ───────────────────────────

#[test]
fn test_threadsafe_db_send_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<super::ThreadSafeKnowledgeDB>();
}

#[test]
fn test_threadsafe_db_basic_ops() {
    let tmp = TempDir::new().unwrap();
    let db = super::ThreadSafeKnowledgeDB::new(&tmp.path().join("ts.db")).unwrap();
    let stats = db.stats().unwrap();
    assert_eq!(stats.file_count, 0);
    db.record_access("test.rs", "s1").unwrap();
    let stats = db.stats().unwrap();
    assert_eq!(stats.access_count, 1);
}

#[test]
fn test_threadsafe_db_multithread() {
    use std::sync::Arc;
    let tmp = TempDir::new().unwrap();
    let db = Arc::new(super::ThreadSafeKnowledgeDB::new(&tmp.path().join("mt.db")).unwrap());
    let mut handles = vec![];
    for i in 0..4 {
        let db_clone = Arc::clone(&db);
        handles.push(std::thread::spawn(move || {
            let file = format!("file_{i}.rs");
            db_clone.record_access(&file, "session").unwrap();
        }));
    }
    for h in handles {
        h.join().unwrap();
    }
    let stats = db.stats().unwrap();
    assert_eq!(stats.access_count, 4);
}

#[test]
fn test_threadsafe_db_with_closure() {
    let tmp = TempDir::new().unwrap();
    let db = super::ThreadSafeKnowledgeDB::new(&tmp.path().join("cl.db")).unwrap();
    db.with(|inner| {
        inner
            .upsert(&FileKnowledge {
                file_path: "test.py".into(),
                line_count: 42,
                ..Default::default()
            })
            .unwrap();
    })
    .unwrap();
    let result = db.lookup("test.py").unwrap().unwrap();
    assert_eq!(result.line_count, 42);
}

#[test]
fn test_wiring_map_table_exists() {
    let (_tmp, db) = test_db();
    let count: i64 = db
        .conn
        .query_row("SELECT COUNT(*) FROM wiring_map", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn test_module_ecosystem_table_exists() {
    let (_tmp, db) = test_db();
    let count: i64 = db
        .conn
        .query_row("SELECT COUNT(*) FROM module_ecosystem", [], |r| r.get(0))
        .unwrap();
    assert_eq!(count, 0);
}

#[test]
fn test_file_relations_has_imported_symbols() {
    let (_tmp, db) = test_db();
    let rel = FileRelation {
        source: "a.rs".into(),
        target: "b.rs".into(),
        relation_type: "imports".into(),
    };
    db.upsert_relation(&rel).unwrap();
    let sql = format!(
        "SELECT imported_symbols FROM {} WHERE source_path = ?1",
        schema_guard::TABLE_FILE_RELATIONS
    );
    let syms: String = db
        .conn
        .query_row(&sql, params!["a.rs"], |r| r.get(0))
        .unwrap();
    assert_eq!(syms, "[]");
}

// ── Master Plan C.W2.P3.T9: Schema migration V7→V8 fixture tests ──
//
// The real schema-upgrade path lives in `FileKnowledgeDB::new`: it reads
// `PRAGMA user_version`, and when `version < SCHEMA_VERSION` runs
// `ensure_schema()` + `migrate_schema()` then stamps `user_version =
// SCHEMA_VERSION`. These tests build a faithful "V7" fixture (a DB holding
// real data but missing the newest V8-era column + stamped user_version=7),
// run the upgrade, and prove (a) the V8 column reappears, (b) pre-existing
// data survives, and (c) the upgrade is idempotent (twice == once).
//
// The V7→V8 delta marker used here is the `workspace_root` column on
// wiring_map (PLT-2026-06-02 — the latest migration block in
// `migrate_schema`). If a future migration adds a newer column, update the
// marker below AND keep the drift guard (`test_schema_version_drift_guard`)
// honest.

/// Open a `FileKnowledgeDB` at full current schema, then degrade it to a
/// faithful "V7" on-disk state: drop the newest V8-era column and stamp
/// `user_version = 7`. Returns the temp dir (keep alive) and the db path.
///
/// SQLite `DROP COLUMN` requires SQLite >= 3.35; the workspace pins
/// `rusqlite = "0.38"` with the `bundled` feature, whose vendored SQLite is
/// well above that floor.
fn build_v7_fixture() -> (TempDir, std::path::PathBuf) {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("v7_fixture.db");
    {
        let db = FileKnowledgeDB::new(&db_path).unwrap();
        let k = FileKnowledge {
            file_path: "src/legacy.rs".to_string(),
            language: Some("rust".to_string()),
            line_count: 42,
            symbol_count: 7,
            content_hash: Some("deadbeef".to_string()),
            ..Default::default()
        };
        db.upsert(&k).unwrap();
        db.conn
            .execute(
                &format!(
                    "INSERT INTO {} (module_file, symbol_name, consumer_file)
                         VALUES ('crates/foo/src/lib.rs', 'foo_fn', 'crates/bar/src/lib.rs')",
                    schema_guard::TABLE_WIRING_MAP
                ),
                [],
            )
            .unwrap();
    }
    {
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(&format!(
            "ALTER TABLE {} DROP COLUMN workspace_root;",
            schema_guard::TABLE_WIRING_MAP
        ))
        .unwrap();
        conn.execute_batch("PRAGMA user_version = 7;").unwrap();
    }
    (tmp, db_path)
}

/// Open a `FileKnowledgeDB` at full current schema, then degrade it to a
/// faithful "V8" on-disk state: drop the table V9 adds and stamp
/// `user_version = 8`.
///
/// This is the fixture the drift guard demands for the v8→v9 bump. It also
/// reproduces the live defect that motivated the bump: a DB already stamped at
/// the current version never re-runs `ensure_schema`, so a table added only
/// there stays missing forever.
fn build_v8_fixture() -> (TempDir, std::path::PathBuf) {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("v8_fixture.db");
    {
        let db = FileKnowledgeDB::new(&db_path).unwrap();
        db.register_pub_symbol("crates/foo/src/lib.rs", "foo_fn", "function", "public")
            .unwrap();
    }
    {
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch(&format!(
            "DROP TABLE IF EXISTS {};",
            schema_guard::TABLE_WIRING_UNRESOLVED
        ))
        .unwrap();
        conn.execute_batch("PRAGMA user_version = 8;").unwrap();
    }
    (tmp, db_path)
}

/// Returns true iff the named table exists.
fn table_exists(conn: &Connection, table: &str) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name=?1",
        [table],
        |r| r.get::<_, i64>(0),
    )
    .map(|c| c > 0)
    .unwrap_or(false)
}

#[test]
fn test_migration_v8_to_v9_adds_wiring_unresolved_and_preserves_data() {
    let (_tmp, db_path) = build_v8_fixture();
    {
        let conn = Connection::open(&db_path).unwrap();
        assert_eq!(
            conn.query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
                .unwrap(),
            8,
            "fixture should start at V8"
        );
        assert!(
            !table_exists(&conn, schema_guard::TABLE_WIRING_UNRESOLVED),
            "V8 fixture must NOT have wiring_unresolved"
        );
    }

    let db = FileKnowledgeDB::new(&db_path).unwrap();
    assert_eq!(
        db.conn
            .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        SCHEMA_VERSION,
        "upgrade must stamp current version"
    );
    assert!(
        table_exists(&db.conn, schema_guard::TABLE_WIRING_UNRESOLVED),
        "V8->V9 migration must create wiring_unresolved"
    );
    // The table being there is not enough — it has to WORK, because the whole
    // point of the bump is that the writes were failing into `let _ =`.
    db.record_unresolved_import("ghost::mod", "Ghost", "crates/c/src/m.rs", Some(3), "rust")
        .unwrap();
    assert_eq!(db.name_only_candidates(), Some(1));

    let producers: i64 = db
        .conn
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM {} WHERE consumer_file IS NULL",
                schema_guard::TABLE_WIRING_MAP
            ),
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(producers, 1, "wiring_map row must survive upgrade");
}

/// The v9 bump broke the daemon; this test is why it cannot happen again.
///
/// `migrate_schema` rewrites touring-hooks producer rows into consumer rows
/// that all share the literal `touring-daemon://dispatch`. Run twice over the
/// same data, the second pass recreates a tuple that already exists and
/// `idx_wiring_unique` rejects it — `FileKnowledgeDB::new` then returns Err and
/// the daemon answers every request with "Cannot open knowledge DB". Observed
/// live on 2026-08-07: the project went offline the moment SCHEMA_VERSION moved
/// 8 → 9, on a defect that had been latent since the UPDATE was written.
///
/// A migration that runs exactly once looks identical to a correct one until
/// the next bump. This asserts the second run.
#[test]
fn test_migrate_schema_survives_a_second_pass_over_populated_wiring() {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("rerun.db");
    {
        let db = FileKnowledgeDB::new(&db_path).unwrap();
        // Two producers under touring-hooks with the SAME symbol name in
        // different modules, plus one already carrying the daemon consumer —
        // the shape that collides on the second pass.
        db.register_pub_symbol(
            "crates/touring-hooks/src/a.rs",
            "shared_fn",
            "function",
            "public",
        )
        .unwrap();
        db.register_pub_symbol(
            "crates/touring-hooks/src/b.rs",
            "shared_fn",
            "function",
            "public",
        )
        .unwrap();
        db.conn
            .execute(
                &format!(
                    "INSERT INTO {} (module_file, symbol_name, symbol_kind, visibility, consumer_file, consumer_type)
                     VALUES ('crates/touring-hooks/src/a.rs', 'shared_fn', 'function', 'public',
                             'touring-daemon://dispatch', 'daemon_hook')",
                    schema_guard::TABLE_WIRING_MAP
                ),
                [],
            )
            .unwrap();
    }
    // Force the version gate open again, exactly as a SCHEMA_VERSION bump does.
    {
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("PRAGMA user_version = 0;").unwrap();
    }
    let reopened = FileKnowledgeDB::new(&db_path);
    assert!(
        reopened.is_ok(),
        "re-running migrate_schema must not fail: {:?}",
        reopened.err()
    );
    let db = reopened.unwrap();
    assert_eq!(
        db.conn
            .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
            .unwrap(),
        SCHEMA_VERSION,
        "a successful re-run must stamp the version"
    );
}

#[test]
fn test_migration_v8_to_v9_is_idempotent() {
    let (_tmp, db_path) = build_v8_fixture();
    for _ in 0..3 {
        let db = FileKnowledgeDB::new(&db_path).unwrap();
        assert_eq!(
            db.conn
                .query_row("PRAGMA user_version", [], |r| r.get::<_, u32>(0))
                .unwrap(),
            SCHEMA_VERSION
        );
        assert!(table_exists(&db.conn, schema_guard::TABLE_WIRING_UNRESOLVED));
    }
}

/// Returns true iff the named column exists on the given table.
fn column_exists(conn: &Connection, table: &str, column: &str) -> bool {
    conn.prepare(&format!("SELECT {column} FROM {table} LIMIT 0"))
        .is_ok()
}

#[test]
fn test_migration_v7_to_v8_upgrades_schema_and_preserves_data() {
    let (_tmp, db_path) = build_v7_fixture();
    {
        let conn = Connection::open(&db_path).unwrap();
        let ver: u32 = conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(ver, 7, "fixture should start at V7");
        assert!(
            !column_exists(&conn, schema_guard::TABLE_WIRING_MAP, "workspace_root"),
            "V7 fixture must NOT have the workspace_root column"
        );
    }
    let db = FileKnowledgeDB::new(&db_path).unwrap();
    let ver: u32 = db
        .conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(ver, SCHEMA_VERSION, "upgrade must stamp current version");
    assert!(
        column_exists(&db.conn, schema_guard::TABLE_WIRING_MAP, "workspace_root"),
        "V7->V8 migration must add the workspace_root column"
    );
    let survivor = db.lookup("src/legacy.rs").unwrap();
    assert!(
        survivor.is_some(),
        "file_knowledge row must survive upgrade"
    );
    let survivor = survivor.unwrap();
    assert_eq!(survivor.line_count, 42);
    assert_eq!(survivor.symbol_count, 7);
    assert_eq!(survivor.content_hash.as_deref(), Some("deadbeef"));
    let wiring_rows: i64 = db
        .conn
        .query_row(
            &format!("SELECT COUNT(*) FROM {}", schema_guard::TABLE_WIRING_MAP),
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(wiring_rows, 1, "wiring_map row must survive upgrade");
}

#[test]
fn test_migration_v7_to_v8_is_idempotent() {
    let (_tmp, db_path) = build_v7_fixture();
    {
        let db = FileKnowledgeDB::new(&db_path).unwrap();
        let ver: u32 = db
            .conn
            .query_row("PRAGMA user_version", [], |r| r.get(0))
            .unwrap();
        assert_eq!(ver, SCHEMA_VERSION);
    }
    let schema_after_first: Vec<String> = {
        let conn = Connection::open(&db_path).unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT type || '|' || name || '|' || COALESCE(sql, '')
                     FROM sqlite_master
                     WHERE name NOT LIKE 'sqlite_%'
                     ORDER BY type, name",
            )
            .unwrap();
        stmt.query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    };
    let db = FileKnowledgeDB::new(&db_path).unwrap();
    let ver: u32 = db
        .conn
        .query_row("PRAGMA user_version", [], |r| r.get(0))
        .unwrap();
    assert_eq!(
        ver, SCHEMA_VERSION,
        "second open must stay at current version"
    );
    let schema_after_second: Vec<String> = {
        let conn = Connection::open(&db_path).unwrap();
        let mut stmt = conn
            .prepare(
                "SELECT type || '|' || name || '|' || COALESCE(sql, '')
                     FROM sqlite_master
                     WHERE name NOT LIKE 'sqlite_%'
                     ORDER BY type, name",
            )
            .unwrap();
        stmt.query_map([], |r| r.get::<_, String>(0))
            .unwrap()
            .collect::<Result<Vec<_>, _>>()
            .unwrap()
    };
    assert_eq!(
        schema_after_first, schema_after_second,
        "migration must be idempotent: upgrading twice == upgrading once"
    );
    assert!(
        db.lookup("src/legacy.rs").unwrap().is_some(),
        "data must survive the idempotent second open"
    );
}

#[test]
fn test_migrate_schema_directly_is_idempotent() {
    let (_tmp, db) = test_db();
    db.migrate_schema()
        .expect("first migrate_schema must succeed");
    db.migrate_schema()
        .expect("second migrate_schema must be a safe no-op (idempotent)");
    assert!(
        column_exists(&db.conn, schema_guard::TABLE_WIRING_MAP, "workspace_root"),
        "workspace_root column present after repeated migrate_schema"
    );
}

/// Drift guard: if `SCHEMA_VERSION` is bumped without adding a corresponding
/// fixture + migration test, THIS test fails loudly. When you bump the
/// version: (1) add the new column/table migration to `migrate_schema`,
/// (2) update `build_v7_fixture` to drop the NEW newest column, and
/// (3) update the expected value below.
#[test]
fn test_schema_version_drift_guard() {
    assert_eq!(
        SCHEMA_VERSION, 10,
        "SCHEMA_VERSION changed to {} — bump this guard AND add a fixture/migration test \
             (see Master Plan C.W2.P3.T9: build_v7_fixture + \
             test_migration_v7_to_v8_upgrades_schema_and_preserves_data)",
        SCHEMA_VERSION
    );
}
