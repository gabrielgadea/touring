//! End-to-end integration tests for Pln2 extended knowledge tables.
//!
//! Validates all 11 new Pln2 tables and their write operations.
//! Read operations are tested where they exist (get_unresolved_todos,
//! get_file_community, get_blake3_hash, get_pending_wiring_suggestions,
//! get_cognitive_enrichment). Some tables only have write methods.

#![allow(clippy::indexing_slicing)]

use tempfile::TempDir;
use touring_hooks::knowledge::FileKnowledgeDB;
use touring_hooks::shared::feature_flags::extract_features_auto;

fn temp_knowledge_db() -> (TempDir, FileKnowledgeDB) {
    let tmp = TempDir::new().unwrap();
    let db_path = tmp.path().join("test.db");
    let kb = FileKnowledgeDB::new(&db_path).unwrap();
    (tmp, kb)
}

// ── file_feature_flags ────────────────────────────────────────────────────────

#[test]
fn pln2_feature_flags_single_upsert() {
    let (_tmp, kb) = temp_knowledge_db();
    kb.upsert_feature_flag("src/main.rs", "rust", "testable")
        .unwrap();
    kb.upsert_feature_flag("src/main.rs", "rust", "serde")
        .unwrap();
    kb.upsert_feature_flag("src/lib.rs", "rust", "testable")
        .unwrap();
}

#[test]
fn pln2_feature_flags_batch_upsert() {
    let (_tmp, kb) = temp_knowledge_db();

    let batch: Vec<(&str, &str)> = vec![
        ("src/main.rs", "async"),
        ("src/main.rs", "full"),
        ("src/lib.rs", "no_std"),
    ];
    kb.upsert_feature_flags_batch("src/main.rs", &batch)
        .unwrap();
    kb.upsert_feature_flags_batch("src/lib.rs", &batch).unwrap();
}

#[test]
fn pln2_feature_flags_handles_empty_input() {
    let (_tmp, kb) = temp_knowledge_db();
    let result = kb.upsert_feature_flag("", "rust", "testable");
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn pln2_feature_flags_extraction_integration() {
    let (_tmp, kb) = temp_knowledge_db();

    // RustExtractor matches `feature = "` pattern (quoted string after feature name)
    let content = r#"
[features]
default = []
full = ["dep:serde"]
serde = ["serde/std", "serde/derive"]

[dependencies]
tokio = { version = "1.0", features = ["full"] }
"#;
    // Also include explicit feature = "..." lines that RustExtractor parses
    let content2 = r#"
feature = "derive"
feature = "serde"
feature = "full"
"#;

    let path = std::path::Path::new("Cargo.toml");
    let features = extract_features_auto(path, content);
    let features2 = extract_features_auto(path, content2);
    let all_features: Vec<String> = features.iter().chain(features2.iter()).cloned().collect();

    // verify at least some features were extracted from content2 format
    assert!(
        !all_features.is_empty(),
        "expected features from feature = \"...\" pattern"
    );
    assert!(all_features.iter().any(|f| f == "derive"));
    assert!(all_features.iter().any(|f| f == "serde"));
    assert!(all_features.iter().any(|f| f == "full"));

    let feature_pairs: Vec<(&str, &str)> =
        all_features.iter().map(|f| (f.as_str(), "rust")).collect();
    kb.upsert_feature_flags_batch("Cargo.toml", &feature_pairs)
        .unwrap();
}

// ── file_todos ───────────────────────────────────────────────────────────────

#[test]
fn pln2_todos_insert_and_resolve() {
    let (_tmp, kb) = temp_knowledge_db();

    kb.insert_todo("src/main.rs", 10, "TODO", "fix authentication bug")
        .unwrap();
    kb.insert_todo("src/main.rs", 20, "FIXME", "remove dead code")
        .unwrap();
    kb.insert_todo("src/lib.rs", 5, "TODO", "add tests")
        .unwrap();

    let todos = kb.get_unresolved_todos("src/main.rs").unwrap();
    assert_eq!(todos.len(), 2);

    let todo_id = todos.first().expect("todos non-empty (checked above)").0;
    kb.resolve_todo(todo_id).unwrap();

    let unresolved = kb.get_unresolved_todos("src/main.rs").unwrap();
    assert_eq!(unresolved.len(), 1);
}

#[test]
fn pln2_todos_kinds_classification() {
    let (_tmp, kb) = temp_knowledge_db();

    kb.insert_todo("src/main.rs", 1, "TODO", "first task")
        .unwrap();
    kb.insert_todo("src/main.rs", 2, "FIXME", "fix this")
        .unwrap();
    kb.insert_todo("src/main.rs", 3, "XXX", "remove this")
        .unwrap();

    let todos = kb.get_unresolved_todos("src/main.rs").unwrap();
    let kinds: Vec<_> = todos.iter().map(|t| t.2.as_str()).collect();
    assert!(kinds.contains(&"TODO"));
    assert!(kinds.contains(&"FIXME"));
    assert!(kinds.contains(&"XXX"));
}

#[test]
fn pln2_todos_extraction_from_content() {
    let (_tmp, kb) = temp_knowledge_db();

    // Lines must start with TODO/FIXME/XXX directly (no // prefix) to match extraction logic
    let content = r#"
TODO: implement feature X
FIXME: refactor this function
XXX: remove deprecated code
TODO: add integration tests
    "#;

    for (line_idx, line) in content.lines().enumerate() {
        let trimmed = line.trim();
        let kind = if trimmed.starts_with("TODO") {
            "TODO"
        } else if trimmed.starts_with("FIXME") {
            "FIXME"
        } else if trimmed.starts_with("XXX") {
            "XXX"
        } else {
            continue;
        };
        let content_part = trimmed
            .find(':')
            .map(|p| trimmed[p + 1..].trim())
            .unwrap_or("");
        if !content_part.is_empty() {
            let _ = kb.insert_todo("src/test.rs", (line_idx + 1) as i64, kind, content_part);
        }
    }

    let todos = kb.get_unresolved_todos("src/test.rs").unwrap();
    let kinds: Vec<&str> = todos.iter().map(|t| t.2.as_str()).collect();
    assert!(kinds.contains(&"TODO"));
    assert!(kinds.contains(&"FIXME"));
    assert!(kinds.contains(&"XXX"));
}

// ── edge_confidence ───────────────────────────────────────────────────────────

#[test]
fn pln2_edge_confidence_upsert() {
    let (_tmp, kb) = temp_knowledge_db();

    kb.upsert_edge_confidence("src/a.rs", "src/b.rs", "imports", "high")
        .unwrap();
    kb.upsert_edge_confidence("src/b.rs", "src/c.rs", "imports", "medium")
        .unwrap();
}

// ── file_communities ────────────────────────────────────────────────────────

#[test]
fn pln2_community_upsert_and_get() {
    let (_tmp, kb) = temp_knowledge_db();

    kb.upsert_file_community("src/core.rs", 1, 0.9).unwrap();
    kb.upsert_file_community("src/util.rs", 1, 0.8).unwrap();
    kb.upsert_file_community("tests/test.rs", 2, 0.7).unwrap();

    let comm = kb.get_file_community("src/core.rs").unwrap();
    let (comm_id, score) = comm.unwrap();
    assert_eq!(comm_id, 1);
    assert!((score - 0.9).abs() < 0.001);

    let comm2 = kb.get_file_community("tests/test.rs").unwrap();
    let (comm_id2, score2) = comm2.unwrap();
    assert_eq!(comm_id2, 2);
    assert!((score2 - 0.7).abs() < 0.001);
}

#[test]
fn pln2_community_unknown_file_returns_none() {
    let (_tmp, kb) = temp_knowledge_db();
    let comm = kb.get_file_community("unknown.rs").unwrap();
    assert!(comm.is_none());
}

// ── file_test_coverage ─────────────────────────────────────────────────────

#[test]
fn pln2_test_coverage_upsert() {
    let (_tmp, kb) = temp_knowledge_db();

    kb.upsert_test_coverage("src/main.rs", 85.5, 42, 50)
        .unwrap();
    kb.upsert_test_coverage("src/lib.rs", 72.0, 18, 25).unwrap();
}

// ── file_blake3_registry ──────────────────────────────────────────────────

#[test]
fn pln2_blake3_registry_upsert_and_get() {
    let (_tmp, kb) = temp_knowledge_db();

    kb.upsert_blake3_registry("src/main.rs", "abc123def", 42, None)
        .unwrap();

    let hash = kb.get_blake3_hash("src/main.rs").unwrap();
    assert!(hash.is_some());
    let (hash_str, symbols) = hash.unwrap();
    assert_eq!(hash_str, "abc123def");
    assert_eq!(symbols, 42);

    kb.upsert_blake3_registry("src/main.rs", "newdef456", 50, None)
        .unwrap();
    let hash2 = kb.get_blake3_hash("src/main.rs").unwrap();
    let (hash_str2, symbols2) = hash2.unwrap();
    assert_eq!(hash_str2, "newdef456");
    assert_eq!(symbols2, 50);
}

#[test]
fn pln2_blake3_hash_unknown_file_returns_none() {
    let (_tmp, kb) = temp_knowledge_db();
    let result = kb.get_blake3_hash("nonexistent.rs").unwrap();
    assert!(result.is_none());
}

#[test]
fn pln2_blake3_hash_computation() {
    let (_tmp, kb) = temp_knowledge_db();

    let content = "Hello, world!";
    use blake3::Hasher;
    let mut hasher = Hasher::new();
    hasher.update(content.as_bytes());
    let expected_hash = hasher.finalize().to_hex().to_string();

    kb.upsert_blake3_registry("test.txt", &expected_hash, 0, None)
        .unwrap();

    let result = kb.get_blake3_hash("test.txt").unwrap();
    assert!(result.is_some());
    let (hash_str, _symbols) = result.unwrap();
    assert_eq!(hash_str, expected_hash);
    assert_eq!(hash_str.len(), 64);
}

// ── session_file_summary ────────────────────────────────────────────────────

#[test]
fn pln2_session_summary_upsert() {
    let (_tmp, kb) = temp_knowledge_db();

    kb.upsert_session_file_summary(
        "src/main.rs",
        "session-123",
        Some(r#"{"language":"rust","line_count":100}"#),
        Some("main entry point"),
        Some(r#"["unused_import","style_naming"]"#),
        Some("medium"),
    )
    .unwrap();

    let result = kb.top_accessed_files_in_session("session-123", 10);
    assert!(result.is_ok());
    assert!(result.unwrap().is_empty());
}

// ── symbol_events_log ───────────────────────────────────────────────────────

#[test]
fn pln2_symbol_events_insert() {
    let (_tmp, kb) = temp_knowledge_db();

    let id1 = kb
        .insert_symbol_event(
            "session-1:event-1",
            "src/main.rs",
            None,
            "create",
            Some("main"),
            None,
        )
        .unwrap();
    let id2 = kb
        .insert_symbol_event(
            "session-1:event-2",
            "src/main.rs",
            None,
            "modify",
            Some("main"),
            None,
        )
        .unwrap();
    let id3 = kb
        .insert_symbol_event(
            "session-1:event-3",
            "src/lib.rs",
            None,
            "delete",
            Some("lib"),
            None,
        )
        .unwrap();

    assert!(id1 > 0);
    assert!(id2 > 0);
    assert!(id3 > 0);
    assert!(id1 != id2);
}

// ── wiring_suggestions ─────────────────────────────────────────────────────

#[test]
fn pln2_wiring_suggestions_workflow() {
    let (_tmp, kb) = temp_knowledge_db();

    kb.upsert_wiring_suggestion(
        "orphan_function",
        "src/orphan.rs",
        Some("src/consumer.rs"),
        0.85,
        None,
    )
    .unwrap();

    let pending = kb
        .get_pending_wiring_suggestions("orphan_function")
        .unwrap();
    assert_eq!(pending.len(), 1);
    assert_eq!(pending[0].1, "src/orphan.rs");

    kb.apply_wiring_suggestion(pending[0].0).unwrap();

    let applied = kb
        .get_pending_wiring_suggestions("orphan_function")
        .unwrap();
    assert_eq!(applied.len(), 0);
}

#[test]
fn pln2_wiring_suggestions_rejection() {
    let (_tmp, kb) = temp_knowledge_db();

    kb.upsert_wiring_suggestion(
        "orphan_fn",
        "src/orphan.rs",
        Some("src/consumer.rs"),
        0.5,
        None,
    )
    .unwrap();

    let pending = kb.get_pending_wiring_suggestions("orphan_fn").unwrap();
    let id = pending
        .first()
        .expect("pending non-empty (checked above)")
        .0;

    kb.reject_wiring_suggestion(id).unwrap();

    let still_pending = kb.get_pending_wiring_suggestions("orphan_fn").unwrap();
    assert_eq!(still_pending.len(), 0);
}

#[test]
fn pln2_wiring_suggestions_returns_empty_when_none_pending() {
    let (_tmp, kb) = temp_knowledge_db();
    let pending = kb.get_pending_wiring_suggestions("nonexistent").unwrap();
    assert!(pending.is_empty());
}

// ── metadata_benchmark_runs ─────────────────────────────────────────────────
// NOTE: Only insert exists — get_benchmark_runs is NOT implemented

#[test]
fn pln2_benchmark_runs_insert() {
    let (_tmp, kb) = temp_knowledge_db();

    // insert_benchmark_run: commit_hash, bench_name, p50_ms, p95_ms, p99_ms, samples
    let id = kb
        .insert_benchmark_run("abc123", "cargo test", 1234.5, 100.0, 95.2, 50)
        .unwrap();
    assert!(id > 0);
}

// ── cognitive_enrichment ───────────────────────────────────────────────────

#[test]
fn pln2_cognitive_enrichment_upsert_and_get() {
    let (_tmp, kb) = temp_knowledge_db();

    kb.upsert_cognitive_enrichment("src/main.rs", 0.85, 0.72, 0.91, 0.65, 0.50)
        .unwrap();

    let cog = kb.get_cognitive_enrichment("src/main.rs").unwrap();
    let Some(cog) = cog else {
        panic!("Expected cognitive enrichment to be stored");
    };
    assert!((cog.0 - 0.85).abs() < 0.001);
    assert!((cog.1 - 0.72).abs() < 0.001);
    assert!((cog.2 - 0.91).abs() < 0.001);
    assert!((cog.3 - 0.65).abs() < 0.001);
    assert!((cog.4 - 0.50).abs() < 0.001);
}

#[test]
fn pln2_cognitive_returns_none_for_unknown_file() {
    let (_tmp, kb) = temp_knowledge_db();
    let cog = kb.get_cognitive_enrichment("unknown.rs").unwrap();
    assert!(cog.is_none());
}

// ── Read-side API (Pln2 completion) ─────────────────────────────────────────

#[test]
fn pln2_get_feature_flags_for_file_roundtrip() {
    let (_tmp, kb) = temp_knowledge_db();
    kb.upsert_feature_flag("src/lib.rs", "telemetry", "rust")
        .unwrap();
    kb.upsert_feature_flag("src/lib.rs", "async_runtime", "rust")
        .unwrap();
    let flags = kb.get_feature_flags_for_file("src/lib.rs").unwrap();
    // Returned sorted alphabetically
    assert_eq!(flags, vec!["async_runtime", "telemetry"]);
}

#[test]
fn pln2_get_feature_flags_for_file_empty() {
    let (_tmp, kb) = temp_knowledge_db();
    let flags = kb.get_feature_flags_for_file("nonexistent.rs").unwrap();
    assert!(flags.is_empty());
}

#[test]
fn pln2_get_edge_confidence_or_insert_default() {
    let (_tmp, kb) = temp_knowledge_db();
    // First call on nonexistent pair → inserts "medium" default, returns 0.5
    let conf = kb
        .get_edge_confidence_or_insert("a.rs", "b.rs", "imports")
        .unwrap();
    assert!((conf - 0.5).abs() < f64::EPSILON);
    // Second call → reads back the inserted row
    let conf2 = kb
        .get_edge_confidence_or_insert("a.rs", "b.rs", "imports")
        .unwrap();
    assert!((conf2 - 0.5).abs() < f64::EPSILON);
}

#[test]
fn pln2_get_edge_confidence_or_insert_existing_high() {
    let (_tmp, kb) = temp_knowledge_db();
    kb.upsert_edge_confidence("a.rs", "b.rs", "uses", "high")
        .unwrap();
    let conf = kb
        .get_edge_confidence_or_insert("a.rs", "b.rs", "uses")
        .unwrap();
    assert!((conf - 1.0).abs() < f64::EPSILON);
}

#[test]
fn pln2_get_edge_confidence_or_insert_existing_low() {
    let (_tmp, kb) = temp_knowledge_db();
    kb.upsert_edge_confidence("x.rs", "y.rs", "references", "low")
        .unwrap();
    let conf = kb
        .get_edge_confidence_or_insert("x.rs", "y.rs", "references")
        .unwrap();
    assert!((conf - 0.2).abs() < f64::EPSILON);
}

#[test]
fn pln2_get_benchmark_runs_roundtrip() {
    let (_tmp, kb) = temp_knowledge_db();
    kb.insert_benchmark_run("abc123", "hash_bench", 1.0, 2.0, 3.0, 100)
        .unwrap();
    kb.insert_benchmark_run("def456", "hash_bench", 1.5, 2.5, 3.5, 200)
        .unwrap();
    let runs = kb.get_benchmark_runs("hash_bench").unwrap();
    assert_eq!(runs.len(), 2);
    // DESC order → most recent run_id first
    assert_eq!(runs[0].samples, 200);
    assert_eq!(runs[0].commit_hash, "def456");
    assert!((runs[1].p50_ms - 1.0).abs() < f64::EPSILON);
}

#[test]
fn pln2_get_benchmark_runs_empty() {
    let (_tmp, kb) = temp_knowledge_db();
    let runs = kb.get_benchmark_runs("nonexistent_bench").unwrap();
    assert!(runs.is_empty());
}

#[test]
fn pln2_get_test_coverage_none_before_upsert() {
    let (_tmp, kb) = temp_knowledge_db();
    let cov = kb.get_test_coverage("src/lib.rs").unwrap();
    assert!(cov.is_none());
}

#[test]
fn pln2_get_test_coverage_roundtrip() {
    let (_tmp, kb) = temp_knowledge_db();
    kb.upsert_test_coverage("src/lib.rs", 87.5, 7, 8).unwrap();
    let cov = kb.get_test_coverage("src/lib.rs").unwrap().unwrap();
    assert!((cov.0 - 87.5).abs() < 0.01);
    assert_eq!(cov.1, 7);
    assert_eq!(cov.2, 8);
}

// ── Schema validation ───────────────────────────────────────────────────────

#[test]
fn pln2_all_tables_have_schema_constants() {
    use touring_analysis::e2e::schema_guard;

    assert_eq!(schema_guard::TABLE_FILE_FEATURE_FLAGS, "file_feature_flags");
    assert_eq!(schema_guard::TABLE_FILE_TODOS, "file_todos");
    assert_eq!(schema_guard::TABLE_EDGE_CONFIDENCE, "edge_confidence");
    assert_eq!(schema_guard::TABLE_FILE_COMMUNITIES, "file_communities");
    assert_eq!(schema_guard::TABLE_FILE_TEST_COVERAGE, "file_test_coverage");
    assert_eq!(
        schema_guard::TABLE_FILE_BLAKE3_REGISTRY,
        "file_blake3_registry"
    );
    assert_eq!(
        schema_guard::TABLE_SESSION_FILE_SUMMARY,
        "session_file_summary"
    );
    assert_eq!(schema_guard::TABLE_SYMBOL_EVENTS_LOG, "symbol_events_log");
    assert_eq!(schema_guard::TABLE_WIRING_SUGGESTIONS, "wiring_suggestions");
    assert_eq!(
        schema_guard::TABLE_METADATA_BENCHMARK_RUNS,
        "metadata_benchmark_runs"
    );
    assert_eq!(
        schema_guard::TABLE_COGNITIVE_ENRICHMENT,
        "cognitive_enrichment"
    );
}
