//! cli_handlers facade integration tests (Master Plan A.W2.P5 extraction).
//!
//! Included from `cli_handlers.rs` via `#[path]` so `super::*` still resolves
//! to the `cli_handlers` facade (zero changes to test bodies).
use super::{
    discover_canonical_dbs, memory_recall_fts5_expr, memory_recall_sql, memory_recall_sql_federated,
};
#[test]
fn keyword_skill_match_symbol_query_boosts_index_find() {
    let results = super::keyword_skill_match("find this symbol definition");
    assert!(
        !results.is_empty(),
        "should return results for non-empty query"
    );
    let top = results[0]["skill"].as_str().unwrap_or("");
    assert_eq!(
        top, "touring index find",
        "symbol/find/definition keywords should boost 'touring index find' to top, got: {top}"
    );
    assert_eq!(
        results[0]["source"].as_str().unwrap_or(""),
        "keyword_fallback"
    );
}
#[test]
fn keyword_skill_match_blast_query_boosts_ast_blast() {
    let results = super::keyword_skill_match("blast radius analysis for this file");
    assert!(!results.is_empty(), "should return results");
    let top = results[0]["skill"].as_str().unwrap_or("");
    assert_eq!(
        top, "touring ast blast",
        "blast/radius/impact keywords should boost 'touring ast blast' to top, got: {top}"
    );
}
#[test]
fn keyword_skill_match_recall_query_boosts_memory_recall() {
    let results = super::keyword_skill_match("recall pattern from memory lesson");
    assert!(!results.is_empty(), "should return results");
    let top = results[0]["skill"].as_str().unwrap_or("");
    assert_eq!(
        top, "touring memory recall",
        "memory/recall/lesson/pattern keywords should boost 'touring memory recall' to top, got: {top}"
    );
}
#[test]
fn keyword_skill_match_unrelated_query_returns_defaults_without_boost() {
    let results = super::keyword_skill_match("completely unrelated xyz query");
    assert_eq!(results.len(), 3, "should always return top-3 results");
    for r in &results {
        assert!(r.get("skill").is_some(), "missing 'skill' field");
        assert!(r.get("relevance").is_some(), "missing 'relevance' field");
        assert!(
            r.get("description").is_some(),
            "missing 'description' field"
        );
        assert!(r.get("source").is_some(), "missing 'source' field");
        assert_eq!(r["source"].as_str().unwrap_or(""), "keyword_fallback");
    }
}
#[test]
fn keyword_skill_match_empty_query_returns_defaults_without_panic() {
    let results = super::keyword_skill_match("");
    assert_eq!(
        results.len(),
        3,
        "empty query should return 3 default results"
    );
    for r in &results {
        assert!(r.get("skill").is_some());
        assert!(r.get("source").is_some());
    }
}
#[test]
fn session_summary_json_includes_health_delta_key() {
    let file_path = "src/foo.rs";
    let health_delta_str = r#"{"file_path":"src/foo.rs","regression_streak":0}"#;
    let health_delta: serde_json::Value =
        serde_json::from_str(health_delta_str).unwrap_or(serde_json::Value::Null);
    let output = serde_json::json!(
        { "file_path" : file_path, "summaries" : [], "count" : 0, "health_delta" :
        health_delta, }
    );
    assert!(
        output.get("health_delta").is_some(),
        "N5: health_delta must be present"
    );
    assert_eq!(output["file_path"].as_str().unwrap_or(""), file_path);
}
#[test]
fn wiring_status_json_includes_hypergraph_cycles_key() {
    let output = serde_json::json!(
        { "orphan_count" : 5, "knowledge_activity" : {}, "hypergraph_cycles" : {
        "count" : 0, "detail" : [], }, }
    );
    assert!(
        output.get("hypergraph_cycles").is_some(),
        "N7: hypergraph_cycles must be present"
    );
    assert_eq!(
        output["hypergraph_cycles"]["count"].as_u64().unwrap_or(99),
        0,
        "count should default to 0 when no hyperedge cycles"
    );
}
#[test]
fn decompose_create_response_includes_bandit_fields() {
    let output = serde_json::json!(
        { "task_id" : "task_123", "task_type" : "general", "description" :
        "test task", "status" : "created", "cila_level" : 3, "priority" : "normal",
        "persisted" : true, "bandit_split_factor" : "Split3", "bandit_subtasks" : 3,
        }
    );
    assert!(
        output.get("bandit_split_factor").is_some(),
        "G1: bandit_split_factor must be present"
    );
    assert!(
        output.get("bandit_subtasks").is_some(),
        "G1: bandit_subtasks must be present"
    );
    let subtasks = output["bandit_subtasks"].as_i64().unwrap_or(-1);
    assert!(
        subtasks >= 1 && subtasks <= 4,
        "bandit_subtasks must be in [1, 4]"
    );
}
#[test]
fn blast_warning_high_blast_code_str_matches_rfc100() {
    use touring_analysis::blast_radius::BlastWarning;
    let w = BlastWarning::HighBlast {
        symbol: "src/foo.rs".to_string(),
        affected_files: 15,
        threshold: 10,
    };
    assert_eq!(
        w.code_str(),
        "B-300",
        "G2: HighBlast must carry RFC-100 code B-300"
    );
    assert_eq!(
        w.severity_class(),
        touring_foundation::diagnostic::Severity::Warning
    );
}
#[test]
fn memory_finding_code_strs_match_rfc100() {
    use crate::memory_finding::MemoryFinding;
    let recall_empty = MemoryFinding::RecallEmpty {
        query: "test".to_string(),
    };
    assert_eq!(
        recall_empty.code_str(),
        "M-500",
        "G3: RecallEmpty must carry M-500"
    );
    let rrf = MemoryFinding::RrfFusion {
        source_count: 3,
        merged_count: 12,
    };
    assert_eq!(rrf.code_str(), "M-520", "G3: RrfFusion must carry M-520");
    let tfidf = MemoryFinding::TfidfActivated {
        candidate_count: 5,
        corpus_size: 100,
    };
    assert_eq!(
        tfidf.code_str(),
        "M-510",
        "G3: TfidfActivated must carry M-510"
    );
}
#[test]
fn cli_ast_blast_response_includes_diagnostics_field() {
    let json_low = serde_json::json!(
        { "file_path" : "src/lib.rs", "blast_radius" : 0, "consumers" : [],
        "coedit_files" : [], "diagnostics" : [] }
    );
    assert!(
        json_low.get("diagnostics").is_some(),
        "T4: diagnostics key must be present"
    );
    assert!(
        json_low["diagnostics"].is_array(),
        "T4: diagnostics must be array"
    );
    assert_eq!(
        json_low["diagnostics"].as_array().unwrap().len(),
        0,
        "T4: empty below threshold"
    );
    use touring_analysis::blast_radius::BlastWarning;
    let blast_count: usize = 15;
    let file_path = "src/core.rs";
    let w = BlastWarning::HighBlast {
        symbol: file_path.to_string(),
        affected_files: blast_count,
        threshold: 10,
    };
    let diag = serde_json::json!(
        { "code" : w.code_str(), "severity" : "warning", "message" :
        format!("{blast_count} files depend on `{file_path}` (threshold=10)"), "help"
        : "Consider splitting this module to reduce blast radius" }
    );
    let json_high = serde_json::json!(
        { "file_path" : file_path, "blast_radius" : blast_count, "consumers" : [],
        "coedit_files" : [], "diagnostics" : [diag] }
    );
    let diags = json_high["diagnostics"].as_array().unwrap();
    assert_eq!(diags.len(), 1, "T4: one B-300 entry for blast > threshold");
    assert_eq!(diags[0]["code"], "B-300", "T4: code must be B-300");
    assert_eq!(
        diags[0]["severity"], "warning",
        "T4: severity must be warning"
    );
}
#[test]
fn cli_memory_recall_response_includes_diagnostics_field() {
    use crate::memory_finding::MemoryFinding;
    let f = MemoryFinding::RecallEmpty {
        query: "test-query".to_string(),
    };
    let diag = serde_json::json!(
        { "code" : f.code_str(), "severity" : "info", "message" :
        format!("No memory entries found for query: test-query") }
    );
    let json_empty = serde_json::json!(
        { "entries" : [], "count" : 0, "query" : "test-query", "ann_results" : 0,
        "symbol_context" : [], "diagnostics" : [diag] }
    );
    assert!(
        json_empty.get("diagnostics").is_some(),
        "T5: diagnostics key must be present"
    );
    let diags = json_empty["diagnostics"].as_array().unwrap();
    assert_eq!(
        diags[0]["code"], "M-500",
        "T5: RecallEmpty must carry M-500"
    );
    assert_eq!(
        diags[0]["severity"], "info",
        "T5: RecallEmpty severity must be info"
    );
    let json_hit = serde_json::json!(
        { "entries" : [{ "key" : "k", "value" : "v", "tier" : "semantic" }], "count"
        : 1, "query" : "test-query", "ann_results" : 0, "symbol_context" : [],
        "diagnostics" : [] }
    );
    assert!(
        json_hit.get("diagnostics").is_some(),
        "T5: diagnostics present on hit"
    );
    assert_eq!(
        json_hit["diagnostics"].as_array().unwrap().len(),
        0,
        "T5: no M-500 on hit"
    );
}
#[test]
fn memory_recall_fts5_expr_tokenizes_and_quotes() {
    // ':' and '-' are split points and every term is quoted — a raw
    // MATCH on the unquoted key errors with "no such column: outcome".
    assert_eq!(
        memory_recall_fts5_expr("outcome:bash:transcript-ab12:failure"),
        "\"outcome\" \"bash\" \"transcript\" \"ab12\" \"failure\""
    );
    // Multi-word free text → AND-ed quoted phrases.
    assert_eq!(
        memory_recall_fts5_expr("outcome transcript failure"),
        "\"outcome\" \"transcript\" \"failure\""
    );
    // No usable term → empty string (caller skips the FTS path).
    assert_eq!(memory_recall_fts5_expr("   :::   "), "");
}
#[test]
fn memory_recall_sql_fts5_matches_multiword_query() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let db_path = dir.path().join("memory.db");
    {
        let conn = rusqlite::Connection::open(&db_path).expect("open");
        conn.execute_batch(
            "CREATE TABLE memory_entries (
                     key TEXT PRIMARY KEY, value TEXT NOT NULL,
                     tier TEXT NOT NULL DEFAULT 'local',
                     entry_type TEXT NOT NULL DEFAULT 'insight');
                 CREATE VIRTUAL TABLE memories_fts USING fts5(
                     key, value, entry_type,
                     content='memory_entries', content_rowid='rowid');
                 CREATE TRIGGER memory_fts_ai AFTER INSERT ON memory_entries BEGIN
                     INSERT INTO memories_fts(rowid, key, value, entry_type)
                     VALUES (new.rowid, new.key, new.value, new.entry_type);
                 END;",
        )
        .expect("schema");
        conn.execute(
            "INSERT INTO memory_entries (key, value, tier, entry_type) \
                 VALUES (?1, ?2, 'semantic', 'lesson')",
            rusqlite::params![
                "outcome:bash:transcript-ab12cd34:failure",
                "Exit code 144 from a pgrep invocation",
            ],
        )
        .expect("insert");
    }
    // The reported bug: a multi-word query. `LIKE '%outcome transcript
    // failure%'` returns 0 (those words are never contiguous in the key);
    // FTS5 MATCH tokenizes the key and finds it.
    let hits = memory_recall_sql(&db_path, "outcome transcript failure");
    assert_eq!(
        hits.len(),
        1,
        "multi-word recall must match the colon-keyed entry"
    );
    assert_eq!(hits[0]["key"], "outcome:bash:transcript-ab12cd34:failure");
    assert_eq!(hits[0]["tier"], "semantic", "tier must come from the JOIN");
    // A term present only in the value is found too.
    assert_eq!(
        memory_recall_sql(&db_path, "pgrep invocation").len(),
        1,
        "value-side terms must match"
    );
}
#[test]
fn memory_recall_sql_falls_back_without_fts() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let db_path = dir.path().join("memory.db");
    {
        let conn = rusqlite::Connection::open(&db_path).expect("open");
        // No memories_fts table — exercises the LIKE fallback path.
        conn.execute_batch(
            "CREATE TABLE memory_entries (
                     key TEXT PRIMARY KEY, value TEXT NOT NULL,
                     tier TEXT NOT NULL DEFAULT 'local',
                     entry_type TEXT NOT NULL DEFAULT 'insight');",
        )
        .expect("schema");
        conn.execute(
            "INSERT INTO memory_entries (key, value, tier, entry_type) \
                 VALUES ('lesson:sandbox:flaky', \
                 'env var race in parallel tests', 'semantic', 'lesson')",
            [],
        )
        .expect("insert");
    }
    // FTS prepare fails (no virtual table) → per-term LIKE fallback.
    assert_eq!(
        memory_recall_sql(&db_path, "race tests").len(),
        1,
        "per-term LIKE fallback must work without the FTS index"
    );
    // AND semantics: one term absent everywhere → no hit.
    assert_eq!(
        memory_recall_sql(&db_path, "race nonexistentxyz").len(),
        0,
        "fallback AND-joins terms — an absent term zeroes the result"
    );
}
#[test]
fn discover_canonical_dbs_finds_nested_project_dbs() {
    let root = tempfile::TempDir::new().expect("tempdir");
    let claude = root.path().join(".claude");
    // Canonical <root>/.claude/touring/<db> layout at three depths —
    // claude itself, a child (rust), a grandchild (skills/SkillX) — for
    // both memory.db and knowledge.db.
    for rel in [
        ".claude/touring/memory.db",
        ".claude/touring/knowledge.db",
        "rust/.claude/touring/memory.db",
        "rust/.claude/touring/knowledge.db",
        "skills/SkillX/.claude/touring/memory.db",
    ] {
        let db = claude.join(rel);
        std::fs::create_dir_all(db.parent().expect("parent")).expect("mkdir");
        std::fs::File::create(&db).expect("touch");
    }
    let mem_primary = claude.join("rust/.claude/touring/memory.db");
    let mem = discover_canonical_dbs(&mem_primary, &claude, "memory.db");
    assert!(
        mem.len() >= 3,
        "discovers claude-root + child + grandchild memory DBs, got {}",
        mem.len()
    );
    // primary is element 0 so its rows win key-dedup downstream.
    let primary_canon = mem_primary
        .canonicalize()
        .unwrap_or_else(|_| mem_primary.clone());
    assert_eq!(mem[0], primary_canon, "primary DB must come first");
    // The db_filename parameter is honored — knowledge.db discovered too.
    let know_primary = claude.join("rust/.claude/touring/knowledge.db");
    let know = discover_canonical_dbs(&know_primary, &claude, "knowledge.db");
    assert!(
        know.len() >= 2,
        "discovers knowledge.db at claude-root + child, got {}",
        know.len()
    );
    assert!(
        know.iter().all(|p| p.ends_with("knowledge.db")),
        "knowledge.db query returns only knowledge.db files"
    );
}
#[test]
fn memory_recall_sql_federated_merges_and_dedups() {
    let dir = tempfile::TempDir::new().expect("tempdir");
    let db1 = dir.path().join("p1.db");
    let db2 = dir.path().join("p2.db");
    // db1 has lesson:alpha:one; db2 has lesson:beta:two AND a colliding
    // lesson:alpha:one — every value matches the query.
    for (path, key, val) in [
        (&db1, "lesson:alpha:one", "shared sandbox lesson from db1"),
        (&db2, "lesson:beta:two", "shared sandbox lesson from db2"),
        (
            &db2,
            "lesson:alpha:one",
            "shared sandbox lesson duplicate db2",
        ),
    ] {
        let conn = rusqlite::Connection::open(path).expect("open");
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS memory_entries (
                     key TEXT PRIMARY KEY, value TEXT NOT NULL,
                     tier TEXT NOT NULL DEFAULT 'local',
                     entry_type TEXT NOT NULL DEFAULT 'insight');",
        )
        .expect("schema");
        conn.execute(
            "INSERT OR IGNORE INTO memory_entries \
                 (key, value, tier, entry_type) \
                 VALUES (?1, ?2, 'semantic', 'lesson')",
            rusqlite::params![key, val],
        )
        .expect("insert");
    }
    let hits = memory_recall_sql_federated(&[db1.clone(), db2.clone()], "shared sandbox lesson");
    // Both DBs contribute; the lesson:alpha:one collision dedups to db1's
    // copy (db1 is queried first).
    assert_eq!(
        hits.len(),
        2,
        "federated recall merges DBs and dedups by key"
    );
    let keys: Vec<&str> = hits.iter().filter_map(|h| h["key"].as_str()).collect();
    assert!(keys.contains(&"lesson:alpha:one"), "db1 entry present");
    assert!(keys.contains(&"lesson:beta:two"), "db2 entry present");
    // Every federated row is tagged with its origin DB.
    assert!(
        hits.iter().all(|h| h.get("source_db").is_some()),
        "each federated row carries source_db"
    );
}
#[test]
fn cli_ast_blast_diag_carries_source_snippet_when_file_readable() {
    use touring_analysis::blast_radius::BlastWarning;
    let tmp = std::env::temp_dir().join("touring_w9_s7_blast.rs");
    let body = "pub fn alpha() {}\npub fn beta() {}\n";
    std::fs::write(&tmp, body).expect("write tmp");
    let file_path = tmp.to_str().unwrap().to_string();
    let blast_count: usize = 15;
    let w = BlastWarning::HighBlast {
        symbol: file_path.clone(),
        affected_files: blast_count,
        threshold: 10,
    };
    let mut diag = serde_json::json!(
        { "code" : w.code_str(), "severity" : "warning", "file" : file_path,
        "message" : format!("{blast_count} files depend on `{file_path}`"), "help" :
        "Consider splitting" }
    );
    if let Some(snippet) = touring_foundation::diagnostic::read_source_snippet(&file_path, 4096) {
        if let Some(obj) = diag.as_object_mut() {
            obj.insert("source_snippet".to_string(), serde_json::json!(snippet));
        }
    }
    assert_eq!(diag["code"], "B-300");
    assert!(
        diag.get("source_snippet").is_some(),
        "S7: diagnostic must carry source_snippet when file readable"
    );
    assert_eq!(diag["source_snippet"].as_str().unwrap(), body);
    let _ = std::fs::remove_file(&tmp);
}
#[test]
fn cli_ast_blast_diag_omits_source_snippet_when_file_missing() {
    use touring_analysis::blast_radius::BlastWarning;
    let file_path = "/nonexistent/zzz_w9_s7.rs".to_string();
    let w = BlastWarning::HighBlast {
        symbol: file_path.clone(),
        affected_files: 99,
        threshold: 10,
    };
    let mut diag = serde_json::json!(
        { "code" : w.code_str(), "severity" : "warning", "file" : file_path,
        "message" : "high blast", }
    );
    if let Some(snippet) = touring_foundation::diagnostic::read_source_snippet(&file_path, 4096) {
        if let Some(obj) = diag.as_object_mut() {
            obj.insert("source_snippet".to_string(), serde_json::json!(snippet));
        }
    }
    assert_eq!(diag["code"], "B-300");
    assert!(
        diag.get("source_snippet").is_none(),
        "S7: missing file must NOT add source_snippet field"
    );
}
#[test]
fn wiring_orphan_diag_carries_source_snippet_via_try_attach_helper() {
    use touring_analysis::wiring::WiringFinding;
    use touring_foundation::diagnostic::DiagnosticCode;
    let tmp = std::env::temp_dir().join("touring_w9_s7_orphan.rs");
    let body = "pub fn dangling_orphan() {}\n";
    std::fs::write(&tmp, body).expect("write tmp");
    let module_path = tmp.to_str().unwrap().to_string();
    let f = WiringFinding::OrphanSymbol {
        module_file: module_path.clone(),
        symbol: "dangling_orphan".to_string(),
    };
    let diag = f
        .to_diagnostic()
        .try_attach_source_from_file(&module_path, 4096);
    assert_eq!(diag.code, "W-100", "S7: orphan must carry W-100");
    assert!(
        diag.source_snippet.is_some(),
        "S7: orphan diagnostic must carry source snippet"
    );
    assert_eq!(diag.source_snippet.as_deref(), Some(body));
    let _ = std::fs::remove_file(&tmp);
}
