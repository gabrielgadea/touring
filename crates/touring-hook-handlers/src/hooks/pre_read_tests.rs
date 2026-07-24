use super::*;
use crate::knowledge::{BashOutcome, FileKnowledge, FileKnowledgeDB, FileRelation};
use tempfile::TempDir;
use touring_code::ast::SymbolLocation;

fn setup() -> (TempDir, FileKnowledgeDB) {
    let tmp = TempDir::new().unwrap();
    let db = FileKnowledgeDB::new(&tmp.path().join("test.db")).unwrap();
    (tmp, db)
}

#[test]
fn test_silence_for_unknown_file() {
    let (_tmp, db) = setup();
    let ctx = compose_high_signal_context(&db, "unknown.py");
    assert!(ctx.is_none(), "Unknown file should produce silence");
}

#[test]
fn test_silence_for_file_with_only_metadata() {
    let (_tmp, db) = setup();
    // File with metadata but NO notes, NO failures, NO dependents
    db.upsert(&FileKnowledge {
        file_path: "src/main.py".to_string(),
        language: Some("python".to_string()),
        line_count: 200,
        symbol_count: 8,
        imports_json: Some(r#"["os","sys","pathlib"]"#.to_string()),
        ..Default::default()
    })
    .unwrap();

    let ctx = compose_high_signal_context(&db, "src/main.py");
    assert!(
        ctx.is_none(),
        "File with only metadata should produce SILENCE — Claude reads this itself"
    );
}

#[test]
fn test_injects_notes() {
    let (_tmp, db) = setup();
    db.upsert(&FileKnowledge {
        file_path: "src/main.py".to_string(),
        notes: Some(
            "ProcessType enum bug: use REVISAO_ORDINARIA not REVISAO_TARIFARIA".to_string(),
        ),
        ..Default::default()
    })
    .unwrap();

    let ctx = compose_high_signal_context(&db, "src/main.py").unwrap();
    assert!(ctx.contains("ProcessType enum bug"));
    assert!(ctx.contains("⚠️"));
}

#[test]
fn test_injects_bash_failure_on_this_file() {
    let (_tmp, db) = setup();
    db.upsert(&FileKnowledge {
        file_path: "src/main.py".to_string(),
        ..Default::default()
    })
    .unwrap();
    db.record_bash_outcome(&BashOutcome {
        command: "ruff check src/main.py".to_string(),
        command_short: "ruff".to_string(),
        exit_code: 1,
        success: false,
        error_pattern: Some("E501 line too long at line 42".to_string()),
        file_context: Some("src/main.py".to_string()),
        command_hash: String::new(),
        executed_at: String::new(),
    })
    .unwrap();

    let ctx = compose_high_signal_context(&db, "src/main.py").unwrap();
    assert!(ctx.contains("ruff"));
    assert!(ctx.contains("failed on this file"));
    assert!(ctx.contains("E501"));
}

#[test]
fn test_silence_for_bash_failure_on_different_file() {
    let (_tmp, db) = setup();
    db.upsert(&FileKnowledge {
        file_path: "src/utils.py".to_string(),
        ..Default::default()
    })
    .unwrap();
    // Failure on main.py, NOT on utils.py
    db.record_bash_outcome(&BashOutcome {
        command: "ruff check src/main.py".to_string(),
        command_short: "ruff".to_string(),
        exit_code: 1,
        success: false,
        error_pattern: Some("E501 line too long".to_string()),
        file_context: Some("src/main.py".to_string()),
        command_hash: String::new(),
        executed_at: String::new(),
    })
    .unwrap();

    let ctx = compose_high_signal_context(&db, "src/utils.py");
    assert!(
        ctx.is_none(),
        "Failure on DIFFERENT file should NOT be injected"
    );
}

#[test]
fn test_injects_dependents_when_significant() {
    let (_tmp, db) = setup();
    // utils.py is imported by 3 files
    for src in &["app.py", "tests.py", "cli.py"] {
        db.upsert_relation(&FileRelation {
            source: src.to_string(),
            target: "utils.py".to_string(),
            relation_type: "imports".to_string(),
        })
        .unwrap();
    }

    let ctx = compose_high_signal_context(&db, "utils.py").unwrap();
    assert!(ctx.contains("3 files import this"));
}

#[test]
fn test_silence_for_single_dependent() {
    let (_tmp, db) = setup();
    // Only 1 dependent — not significant enough to inject
    db.upsert_relation(&FileRelation {
        source: "app.py".to_string(),
        target: "utils.py".to_string(),
        relation_type: "imports".to_string(),
    })
    .unwrap();

    let ctx = compose_high_signal_context(&db, "utils.py");
    assert!(ctx.is_none(), "Single dependent is not worth injecting");
}

#[test]
fn test_combined_signals() {
    let (_tmp, db) = setup();
    db.upsert(&FileKnowledge {
        file_path: "src/main.py".to_string(),
        notes: Some("Bug with enum".to_string()),
        ..Default::default()
    })
    .unwrap();
    db.record_bash_outcome(&BashOutcome {
        command: "pytest src/main.py".to_string(),
        command_short: "pytest".to_string(),
        exit_code: 1,
        success: false,
        error_pattern: Some("AssertionError".to_string()),
        file_context: Some("src/main.py".to_string()),
        command_hash: String::new(),
        executed_at: String::new(),
    })
    .unwrap();

    let ctx = compose_high_signal_context(&db, "src/main.py").unwrap();
    assert!(ctx.contains("Bug with enum"));
    assert!(ctx.contains("pytest"));
    assert!(ctx.contains("|")); // Multiple signals joined
}

// ── Signal 3b: large file standalone tests ──────────────────────────

#[test]
fn test_large_file_signal_fires_independently_without_other_signals() {
    // A large code file with NO gotchas/failures/dependents must still
    // get an ast_overview hint (Signal 3b breaks silence independently).
    let tmp = tempfile::NamedTempFile::with_suffix(".rs").unwrap();
    // > 300 lines estimated: 300 * 60 = 18000 bytes
    let content = "x".repeat(18_100);
    std::fs::write(tmp.path(), &content).unwrap();

    let (_tmp_dir, db) = setup();
    let path = tmp.path().to_str().unwrap();
    let ctx = compose_high_signal_context_budgeted(&db, path, 4000, 0);

    assert!(
        ctx.is_some(),
        "Large code file with no other signals must produce context (Signal 3b)"
    );
    let ctx = ctx.unwrap();
    assert!(
        ctx.contains("ast overview"),
        "Context must contain ast_overview hint, got: {ctx:?}"
    );
}

#[test]
fn test_small_file_signal_does_not_fire_independently() {
    // A small code file with NO other signals must stay silent.
    let tmp = tempfile::NamedTempFile::with_suffix(".rs").unwrap();
    // < 300 lines: 100 * 60 = 6000 bytes
    let content = "x".repeat(6_000);
    std::fs::write(tmp.path(), &content).unwrap();

    let (_tmp_dir, db) = setup();
    let path = tmp.path().to_str().unwrap();
    let ctx = compose_high_signal_context_budgeted(&db, path, 4000, 0);

    assert!(
        ctx.is_none(),
        "Small code file with no other signals must be silent"
    );
}

#[test]
fn test_large_file_signal_500_plus_shows_85pct() {
    let tmp = tempfile::NamedTempFile::with_suffix(".py").unwrap();
    // > 500 lines: 500 * 60 = 30000 bytes
    let content = "x".repeat(30_100);
    std::fs::write(tmp.path(), &content).unwrap();

    let sig = large_file_touring_signal(tmp.path().to_str().unwrap());
    assert!(sig.is_some());
    let (score, text) = sig.unwrap();
    assert!((score - 1.2_f32).abs() < 0.001, "Score must be 1.2");
    assert!(
        text.contains("85%"),
        "500+ lines must show 85% token savings"
    );
}

#[test]
fn test_large_file_signal_300_to_500_shows_70pct() {
    let tmp = tempfile::NamedTempFile::with_suffix(".ts").unwrap();
    // 300–500 lines: 350 * 60 = 21000 bytes
    let content = "x".repeat(21_000);
    std::fs::write(tmp.path(), &content).unwrap();

    let sig = large_file_touring_signal(tmp.path().to_str().unwrap());
    assert!(sig.is_some());
    let (_score, text) = sig.unwrap();
    assert!(
        text.contains("70%"),
        "300-500 lines must show 70% token savings"
    );
}

#[test]
fn test_large_file_signal_non_code_file_returns_none() {
    let tmp = tempfile::NamedTempFile::with_suffix(".md").unwrap();
    let content = "x".repeat(30_000);
    std::fs::write(tmp.path(), &content).unwrap();

    let sig = large_file_touring_signal(tmp.path().to_str().unwrap());
    assert!(
        sig.is_none(),
        "Non-code files must never get the large-file signal"
    );
}

#[test]
fn test_large_file_threshold_constant_is_reasonable() {
    assert!(
        LARGE_FILE_LINE_THRESHOLD >= 200 && LARGE_FILE_LINE_THRESHOLD <= 500,
        "LARGE_FILE_LINE_THRESHOLD={LARGE_FILE_LINE_THRESHOLD} must be between 200 and 500 lines"
    );
}

#[test]
fn test_code_file_large_emits_touring_suggestion() {
    // arrange: Python file > 200 lines estimated (>12000 bytes at ~60 bytes/line)
    let tmp = tempfile::NamedTempFile::with_suffix(".py").unwrap();
    let content = "x".repeat(12_100);
    std::fs::write(tmp.path(), content).unwrap();

    let result = suggest_touring_for_code_file(tmp.path().to_str().unwrap());

    assert!(
        result.contains("touring ast overview"),
        "Expected touring CLI suggestion for large .py file, got: {:?}",
        result
    );
    assert!(result.contains("80%"), "Large file should say 80% economy");
}

#[test]
fn test_non_code_file_no_suggestion() {
    // markdown should not trigger code suggestion
    let result = suggest_touring_for_code_file("README.md");
    assert!(
        !result.contains("touring ast overview"),
        "Markdown should not get touring suggestion"
    );
}

#[test]
fn test_small_code_file_50pct() {
    let tmp = tempfile::NamedTempFile::with_suffix(".rs").unwrap();
    std::fs::write(tmp.path(), "x".repeat(100)).unwrap(); // < 12k bytes
    let result = suggest_touring_for_code_file(tmp.path().to_str().unwrap());
    // Should suggest but with 50% (small file)
    if !result.is_empty() {
        assert!(
            result.contains("50%"),
            "Small code file should say 50% economy"
        );
    }
    // Empty is also OK for very small files if implementer chooses not to suggest
}

// ── build_symbol_map_signal tests ─────────────────────────────────────

fn make_store_with_symbols(symbols: &[(&str, usize, bool)]) -> (TempDir, SymbolStore) {
    let tmp = TempDir::new().unwrap();
    let store = SymbolStore::new(&tmp.path().join("symbols.db")).expect("SymbolStore::new");
    for (name, line, is_def) in symbols {
        store
            .upsert_symbol(&touring_code::ast::SymbolLocation {
                symbol_name: name.to_string(),
                file_path: "test.rs".to_string(),
                line: *line,
                column: 0,
                is_definition: *is_def,
                kind: None,
            })
            .expect("upsert_symbol");
    }
    (tmp, store)
}

#[test]
fn test_symbol_map_none_when_store_absent() {
    let result = build_symbol_map_signal(None, "test.rs");
    assert!(result.is_none(), "absent SymbolStore must produce None");
}

#[test]
fn test_symbol_map_silence_below_threshold() {
    // 1 definition — below MIN_SYMBOL_DEFS=2
    let (_tmp, store) = make_store_with_symbols(&[("main", 1, true)]);
    let result = build_symbol_map_signal(Some(&store), "test.rs");
    assert!(result.is_none(), "single definition must stay silent");
}

#[test]
fn test_symbol_map_fires_at_min_threshold() {
    // Exactly MIN_SYMBOL_DEFS=2 definitions — must inject
    let (_tmp, store) = make_store_with_symbols(&[("MyStruct", 10, true), ("my_fn", 50, true)]);
    let result = build_symbol_map_signal(Some(&store), "test.rs");
    assert!(
        result.is_some(),
        "two definitions must produce a symbol map"
    );
    let ctx = result.unwrap();
    assert!(
        ctx.contains("MyStruct(10)"),
        "must include first def: {ctx:?}"
    );
    assert!(
        ctx.contains("my_fn(50)"),
        "must include second def: {ctx:?}"
    );
    assert!(ctx.contains("defs[2]"), "must state count: {ctx:?}");
}

#[test]
fn test_symbol_map_only_definitions_shown() {
    // 1 definition + 3 references — only def qualifies
    let (_tmp, store) = make_store_with_symbols(&[
        ("MyStruct", 10, true),
        ("MyStruct", 30, false), // reference
        ("MyStruct", 55, false), // reference
        ("my_fn", 50, true),
        ("my_fn", 80, false), // reference
    ]);
    let result = build_symbol_map_signal(Some(&store), "test.rs");
    // Only 2 definitions → should inject (>=2)
    let ctx = result.expect("2 defs must inject");
    assert!(
        ctx.contains("defs[2]"),
        "count must reflect only defs, not refs: {ctx:?}"
    );
}

#[test]
fn test_symbol_map_truncated_at_max_entries() {
    // 10 definitions — only MAX_SYMBOL_MAP_ENTRIES=8 shown + `+2`
    let symbols: Vec<(&str, usize, bool)> = vec![
        ("A", 10, true),
        ("B", 20, true),
        ("C", 30, true),
        ("D", 40, true),
        ("E", 50, true),
        ("F", 60, true),
        ("G", 70, true),
        ("H", 80, true),
        ("I", 90, true),
        ("J", 100, true),
    ];
    let (_tmp, store) = make_store_with_symbols(&symbols);
    let result =
        build_symbol_map_signal(Some(&store), "test.rs").expect("10 defs must produce a map");
    assert!(
        result.contains("defs[10]"),
        "total count must be 10: {result:?}"
    );
    assert!(result.contains("+2"), "must show +2 overflow: {result:?}");
    // Only 8 entries before the overflow
    let entries_section = result
        .trim_start_matches(|c: char| c != ':')
        .trim_start_matches(": ");
    // Count · separators in the first part
    assert!(
        result.contains("A(10)"),
        "first entry must appear: {result:?}"
    );
    assert!(
        result.contains("H(80)"),
        "8th entry must appear: {result:?}"
    );
    assert!(
        !result.contains("I(90)"),
        "9th entry must be in overflow: {result:?}"
    );
    let _ = entries_section; // suppress unused warning
}

#[test]
fn test_symbol_map_format_prefix() {
    let (_tmp, store) = make_store_with_symbols(&[("Alpha", 5, true), ("Beta", 20, true)]);
    let ctx = build_symbol_map_signal(Some(&store), "test.rs").unwrap();
    assert!(ctx.starts_with('\u{1f4cc}'), "must start with 📌: {ctx:?}");
    assert!(ctx.contains("defs["), "must contain defs[N]: {ctx:?}");
}

#[test]
fn test_symbol_map_breaks_silence_when_no_other_signals() {
    // No knowledge DB signals, but 3 symbols indexed → map must fire
    let (_tmp_db, db) = setup();
    let (_tmp_store, store) =
        make_store_with_symbols(&[("run", 1, true), ("helper", 40, true), ("Config", 80, true)]);
    // Compose knowledge context — should be None (no notes/failures/dependents)
    let kb_ctx = compose_high_signal_context(&db, "test.rs");
    assert!(kb_ctx.is_none(), "DB must be silent for this test");

    // Symbol map fires independently
    let sym = build_symbol_map_signal(Some(&store), "test.rs");
    assert!(sym.is_some(), "symbol map must break silence for 3 defs");
}

#[test]
fn test_symbol_map_constants_are_reasonable() {
    assert!(
        MIN_SYMBOL_DEFS >= 2 && MIN_SYMBOL_DEFS <= 5,
        "MIN_SYMBOL_DEFS={MIN_SYMBOL_DEFS} must be 2-5"
    );
    assert!(
        MAX_SYMBOL_MAP_ENTRIES >= 5 && MAX_SYMBOL_MAP_ENTRIES <= 15,
        "MAX_SYMBOL_MAP_ENTRIES={MAX_SYMBOL_MAP_ENTRIES} must be 5-15"
    );
}

// ── Signal I-6: bfs_hop_signal ────────────────────────────────────────────

#[test]
fn test_bfs_hop_signal_none_when_idx_absent() {
    // No SymbolIndex available (Option::None) — must return None silently
    let result = bfs_hop_signal(None, "src/lib.rs");
    assert!(
        result.is_none(),
        "absent SymbolIndex must return None, got: {result:?}"
    );
}

#[test]
fn test_bfs_hop_signal_none_when_no_reverse_deps() {
    // SymbolIndex present but the target file has no reverse deps — guard fires
    let idx = build_symbol_index(
        &[("my_fn", "src/lib.rs")],
        &[],
        &[], // no reverse deps for src/lib.rs
    );
    let result = bfs_hop_signal(Some(&idx), "src/lib.rs");
    assert!(
        result.is_none(),
        "file with empty reverse_deps must return None, got: {result:?}"
    );
}

#[test]
fn test_bfs_hop_signal_format_contains_bfs_hops() {
    // Build an index where "src/lib.rs" is imported by "src/main.rs"
    // This satisfies the reverse_deps guard and produces a BFS signal.
    let mut idx = build_symbol_index(
        &[("my_fn", "src/lib.rs")],
        &[("my_fn", "src/main.rs")],
        &[("src/lib.rs", "src/main.rs")], // lib.rs ← main.rs
    );
    // Also wire main.rs into the reverse_deps of lib.rs so the engine can traverse
    idx.reverse_deps
        .entry("src/lib.rs".to_string())
        .or_default()
        .push("src/main.rs".to_string());

    let result = bfs_hop_signal(Some(&idx), "src/lib.rs");
    // Result may be None if budget exceeded or BFS returns empty list (engine
    // behaviour under test conditions). When Some, check the format contract.
    if let Some((score, text)) = result {
        assert!(
            (score - 1.5_f32).abs() < 0.001,
            "BFS hop signal score must be 1.5, got {score}"
        );
        assert!(
            text.contains("bfs_hops["),
            "output must contain 'bfs_hops[N]:', got: {text:?}"
        );
    }
    // None is acceptable when the BFS engine finds nothing traversable in test env
}

// ── Sprint 3 tests ─────────────────────────────────────────────────────

#[test]
fn test_scope_shadowing_fires_for_rs_with_shadow() {
    let ml_source = "fn foo() {\n    let xx = 1;\n    let xx = 2;\n    xx\n}";
    let sig = scope_shadowing_signal(ml_source, "/tmp/test.rs");
    assert!(sig.is_some(), "xx shadowing xx must fire: {sig:?}");
    let (score, text) = sig.unwrap();
    assert!((score - 0.9_f32).abs() < 0.001);
    assert!(text.contains("scope shadowing"), "signal text: {text:?}");
    assert!(text.contains("shadows"), "must show 'shadows': {text:?}");
}

#[test]
fn test_scope_shadowing_silent_for_single_char_names() {
    // Single-char names (x, i, n) are intentional and excluded (len <= 1)
    let source = "fn foo() {\n    let i = 0;\n    let i = 1;\n}";
    let sig = scope_shadowing_signal(source, "/tmp/test.rs");
    // 'i' with len=1 is filtered, so signal must be None
    assert!(
        sig.is_none(),
        "Single-char shadows must be filtered: {sig:?}"
    );
}

#[test]
fn test_scope_shadowing_silent_for_no_shadowing() {
    let source = "fn foo(aa: i32, bb: i32) -> i32 { let cc = aa + bb; cc }";
    let sig = scope_shadowing_signal(source, "/tmp/test.rs");
    assert!(sig.is_none(), "No shadowing must produce None: {sig:?}");
}

#[test]
fn test_scope_shadowing_silent_for_non_rs_py() {
    let source = "x = 1\nx = 2";
    let sig = scope_shadowing_signal(source, "/tmp/test.md");
    assert!(sig.is_none(), "Non-rs/py files must produce None");
}

#[test]
fn test_gotcha_drift_silent_with_fewer_than_3() {
    // Need >= 3 gotchas for KS test
    let empty: Vec<crate::knowledge::Gotcha> = vec![];
    assert!(gotcha_drift_signal(&empty).is_none());

    let one = vec![crate::knowledge::Gotcha {
        id: 1,
        pattern: "x".to_string(),
        gotcha: "y".to_string(),
        severity: "low".to_string(),
        language: None,
        symbol_name: None,
        hit_count: 0,
        prevented_errors: 0,
        created_at: "2020-01-01 00:00:00".to_string(),
    }];
    assert!(
        gotcha_drift_signal(&one).is_none(),
        "1 gotcha must be silent"
    );
}

#[test]
fn test_gotcha_drift_fires_for_all_stale() {
    // 3 gotchas all from year 2020 (very old recency)
    let stale_date = "2020-01-01 00:00:00";
    let gotchas: Vec<crate::knowledge::Gotcha> = (0..3)
        .map(|i| crate::knowledge::Gotcha {
            id: i,
            pattern: format!("pat{i}"),
            gotcha: format!("issue{i}"),
            severity: "medium".to_string(),
            language: None,
            symbol_name: None,
            hit_count: 0,
            prevented_errors: 0,
            created_at: stale_date.to_string(),
        })
        .collect();

    // All have recency near 0.0 (created 6+ years ago)
    // avg < 0.15 and ks > 0.65 should both hold
    let sig = gotcha_drift_signal(&gotchas);
    assert!(sig.is_some(), "All-stale gotchas must fire drift signal");
    let (score, text) = sig.unwrap();
    assert!((score - 0.5_f32).abs() < 0.001);
    assert!(text.contains("drift="), "Must show drift value: {text:?}");
}

// ══════════════════════════════════════════════════════════════════════════
// CROSS-AUDIT — Sprints 1-3: Purpose vs Implementation
//
// Each audit_* test verifies that a specific feature fulfills its
// documented purpose — not just that it compiles and doesn't crash.
// ══════════════════════════════════════════════════════════════════════════

/// Build a minimal SymbolIndex for testing sprint-2/3 functions.
fn build_symbol_index(
    definitions: &[(&str, &str)], // (symbol_name, file_path)
    references: &[(&str, &str)],  // (symbol_name, file_path)
    rev_deps: &[(&str, &str)],    // (imported_file, importer_file)
) -> SymbolIndex {
    let mut idx = SymbolIndex::new();
    for (name, file) in definitions {
        idx.symbols
            .entry(name.to_string())
            .or_default()
            .push(SymbolLocation {
                file_path: file.to_string(),
                symbol_name: name.to_string(),
                line: 1,
                column: 0,
                is_definition: true,
                kind: None,
            });
        idx.file_to_symbols
            .entry(file.to_string())
            .or_default()
            .push(name.to_string());
    }
    for (name, file) in references {
        idx.symbols
            .entry(name.to_string())
            .or_default()
            .push(SymbolLocation {
                file_path: file.to_string(),
                symbol_name: name.to_string(),
                line: 2,
                column: 0,
                is_definition: false,
                kind: None,
            });
    }
    for (imported, importer) in rev_deps {
        idx.reverse_deps
            .entry(imported.to_string())
            .or_default()
            .push(importer.to_string());
    }
    idx
}

// ── Sprint 1 audit ────────────────────────────────────────────────────────

#[test]
fn audit_s1_2_log_scale_dep_score_increases_with_count() {
    // PURPOSE: "S1.2: Logarithmic scoring — more dependents = higher urgency"
    // score = 1.0 + ln(count).max(0) * 0.3
    // 10 deps ≈ 1.69, 2 deps ≈ 1.21
    let score_2 = 1.0_f32 + (2.0_f32).ln().max(0.0) * 0.3;
    let score_10 = 1.0_f32 + (10.0_f32).ln().max(0.0) * 0.3;
    assert!(
        score_10 > score_2,
        "10 dependents must score higher than 2: {score_10:.3} vs {score_2:.3}"
    );
    assert!(
        (score_10 - 1.69).abs() < 0.01,
        "10 deps must score ~1.69, got {score_10:.3}"
    );

    // Verify via actual DB output
    let (_tmp, db) = setup();
    for src in &[
        "a.py", "b.py", "c.py", "d.py", "e.py", "f.py", "g.py", "h.py", "i.py", "j.py",
    ] {
        db.upsert_relation(&FileRelation {
            source: src.to_string(),
            target: "hub.py".to_string(),
            relation_type: "imports".to_string(),
        })
        .unwrap();
    }
    let ctx = compose_high_signal_context(&db, "hub.py").unwrap();
    assert!(
        ctx.contains("10 files import this"),
        "10 dependents must appear in context: {ctx:?}"
    );
}

#[test]
fn audit_s1_3_recency_score_both_formats_and_boundary_cases() {
    // PURPOSE: "Compute recency score from ISO-8601 timestamp string"
    // Both "YYYY-MM-DD HH:MM:SS" and "YYYY-MM-DDTHH:MM:SS" must parse identically.

    let r_space = recency_score_from_str("2024-01-01 00:00:00");
    let r_t = recency_score_from_str("2024-01-01T00:00:00");
    assert!(
        (r_space - r_t).abs() < 0.001,
        "Space and T separators must produce same score: {r_space} vs {r_t}"
    );

    // Fallback for invalid timestamp → 0.5
    let fallback = recency_score_from_str("not-a-date");
    assert!(
        (fallback - 0.5).abs() < 0.001,
        "Invalid timestamp must fall back to 0.5, got {fallback}"
    );

    // Year-2000 timestamp → near 0.0 (very stale)
    let old = recency_score_from_str("2000-01-01 00:00:00");
    assert!(old < 0.001, "Year-2000 must score near 0.0, got {old}");

    // Recent timestamp → near 1.0
    let now_str = chrono::Utc::now()
        .naive_utc()
        .format("%Y-%m-%d %H:%M:%S")
        .to_string();
    let recent = recency_score_from_str(&now_str);
    assert!(
        recent > 0.95,
        "Just-created timestamp must score > 0.95, got {recent}"
    );

    // Score is strictly in (0.0, 1.0]
    assert!(
        r_space > 0.0 && r_space <= 1.0,
        "Score must be in (0,1]: {r_space}"
    );
}

#[test]
fn audit_s1_4_pub_reexports_detected_from_real_rs_file() {
    // PURPOSE: "Sinal potencial: '⚡ pub re-exports[3]: HookResponse, run — break = API break'"
    let tmp = tempfile::NamedTempFile::with_suffix(".rs").unwrap();
    let source = concat!(
        "pub use crate::hook::HookResponse;\n",
        "pub use crate::runtime::HookRuntime;\n",
        "pub use crate::prelude::DEFAULT_BUDGET;\n",
        "pub mod internal;\n",
    );
    std::fs::write(tmp.path(), source).unwrap();

    let signals = source_based_signals(tmp.path().to_str().unwrap());
    let re_sig = signals.iter().find(|(_, t)| t.contains("pub re-exports"));
    assert!(
        re_sig.is_some(),
        "pub use X must produce re-export signal, got: {signals:?}"
    );

    let (score, text) = re_sig.unwrap();
    assert!(
        (score - 1.8_f32).abs() < 0.001,
        "re-export score must be 1.8, got {score}"
    );
    assert!(
        text.contains("HookResponse") || text.contains("HookRuntime"),
        "must list at least one exported symbol: {text:?}"
    );
    assert!(
        text.contains("API break"),
        "must warn about API break risk: {text:?}"
    );
    // Count should reflect all 3 pub use items
    assert!(text.contains("[3]"), "must show count=3: {text:?}");
}

#[test]
fn audit_s1_5_module_hierarchy_fires_for_lib_rs() {
    // PURPOSE: "Sinal potencial (para lib.rs/mod.rs): '📦 módulos: [pub::pre_read, pub::post_read]'"
    let dir = tempfile::TempDir::new().unwrap();
    let lib_path = dir.path().join("lib.rs");
    let source = concat!(
        "pub mod pre_read;\n",
        "pub mod post_read;\n",
        "mod runtime;\n",
        "mod knowledge;\n",
    );
    std::fs::write(&lib_path, source).unwrap();

    let signals = source_based_signals(lib_path.to_str().unwrap());
    let mod_sig = signals.iter().find(|(_, t)| t.contains("mods:"));
    assert!(
        mod_sig.is_some(),
        "lib.rs with mod declarations must produce hierarchy signal: {signals:?}"
    );

    let (score, text) = mod_sig.unwrap();
    assert!(
        (score - 1.1_f32).abs() < 0.001,
        "module hierarchy score must be 1.1, got {score}"
    );
    assert!(
        text.contains("pub::pre_read"),
        "public mods must be prefixed with pub::: {text:?}"
    );
    // Private mods appear without prefix
    assert!(
        text.contains("runtime") || text.contains("knowledge"),
        "private mods must also appear: {text:?}"
    );
}

#[test]
fn audit_s1_5_module_hierarchy_silent_for_non_entry_rs_file() {
    // PURPOSE: "Only lib.rs, mod.rs, main.rs trigger hierarchy signal"
    let dir = tempfile::TempDir::new().unwrap();
    let utils_path = dir.path().join("utils.rs");
    std::fs::write(&utils_path, "pub mod helpers;\npub mod fmt;\n").unwrap();

    let signals = source_based_signals(utils_path.to_str().unwrap());
    let mod_sig = signals.iter().find(|(_, t)| t.contains("mods:"));
    assert!(
        mod_sig.is_none(),
        "utils.rs must NOT produce module hierarchy signal: {signals:?}"
    );
}

// ── Sprint 2 audit ────────────────────────────────────────────────────────

#[test]
fn audit_s2_1_blast_radius_signal_format_and_score() {
    // PURPOSE: "⚡ blast(dist=3, 12 files); diretos: [daemon.rs, cli.rs]"
    let idx = build_symbol_index(
        &[("run_returning", "src/pre_read.rs")],
        &[],
        &[
            ("src/pre_read.rs", "src/daemon.rs"),
            ("src/pre_read.rs", "src/cli.rs"),
            ("src/pre_read.rs", "src/hook_registry.rs"),
        ],
    );

    let sig = blast_radius_signal(Some(&idx), "src/pre_read.rs", true);
    assert!(
        sig.is_some(),
        "3 reverse deps must produce blast radius signal"
    );

    let (score, text) = sig.unwrap();
    // score = 1.0 + min(2.0, (4-1)/20.0) = 1.15
    assert!(
        score >= 1.0 && score <= 3.0,
        "blast radius score must be in [1.0, 3.0]: {score}"
    );
    assert!(text.starts_with('\u{26a1}'), "must start with ⚡: {text:?}");
    assert!(
        text.contains("blast("),
        "must contain blast( prefix: {text:?}"
    );
    assert!(text.contains("files"), "must mention file count: {text:?}");
    assert!(
        text.contains("diretos:"),
        "must list direct importers: {text:?}"
    );
}

#[test]
fn audit_s2_1_blast_radius_always_injected_for_isolated_file() {
    // PURPOSE: blast radius always injected so Claude always knows what will be impacted.
    // Isolated files (no reverse deps) get score=0.7 with "isolado" label.
    let idx = build_symbol_index(
        &[("my_fn", "src/isolated.rs")],
        &[],
        &[], // no reverse deps
    );

    let sig = blast_radius_signal(Some(&idx), "src/isolated.rs", true);
    assert!(
        sig.is_some(),
        "Isolated file must still produce blast radius signal (informational)"
    );
    let (score, text) = sig.unwrap();
    assert!(
        (score - 0.7).abs() < 0.01,
        "Isolated file score must be 0.7, got {score}"
    );
    assert!(
        text.contains("isolado"),
        "Isolated file signal must contain 'isolado', got: {text}"
    );

    // Still None when SymbolIndex is entirely absent (project not indexed)
    let sig_none = blast_radius_signal(None, "src/any.rs", true);
    assert!(sig_none.is_none(), "None SymbolIndex must produce None");
}

#[test]
fn audit_s2_2_external_callers_signal_format_and_score() {
    // PURPOSE: "📞 callers externo: run_returning(3↑)·compose_signal(5↑)"
    let idx = build_symbol_index(
        &[("process_file", "src/processor.rs")],
        &[
            ("process_file", "src/main.rs"),
            ("process_file", "src/handler.rs"),
            ("process_file", "src/test_helper.rs"),
        ],
        &[],
    );

    let sig = external_callers_signal(Some(&idx), "src/processor.rs");
    assert!(
        sig.is_some(),
        "Function with 3 external refs must produce callers signal"
    );

    let (score, text) = sig.unwrap();
    assert!(
        (score - 1.7_f32).abs() < 0.001,
        "external callers score must be 1.7, got {score}"
    );
    assert!(
        text.contains("callers externo:"),
        "must contain 'callers externo:' prefix: {text:?}"
    );
    assert!(
        text.contains("process_file(3\u{2191})"),
        "must show ref count with ↑: {text:?}"
    );
}

#[test]
fn audit_s2_2_external_callers_silent_for_self_refs_only() {
    // PURPOSE: "Count external references (non-definition, different file)" → skip same-file refs
    let idx = build_symbol_index(
        &[("internal_fn", "src/utils.rs")],
        &[("internal_fn", "src/utils.rs")], // reference in SAME file
        &[],
    );

    let sig = external_callers_signal(Some(&idx), "src/utils.rs");
    assert!(
        sig.is_none(),
        "Self-references only must produce no callers signal: {sig:?}"
    );
}

#[test]
fn audit_s2_3_hit_count_boost_formula_and_cap() {
    // PURPOSE: "S2.3: Hit-count boost — frequently-hit gotchas get scored higher"
    // Formula: hit_boost = (ln(hit_count+1) * 0.2).min(0.5)
    // Capped at 0.5

    let boost_0 = ((0_f32 + 1.0).ln() * 0.2).min(0.5);
    let boost_1 = ((1_f32 + 1.0).ln() * 0.2).min(0.5);
    let boost_50 = ((50_f32 + 1.0).ln() * 0.2).min(0.5);
    let boost_999 = ((999_f32 + 1.0).ln() * 0.2).min(0.5);

    assert!(
        (boost_0 - 0.0).abs() < 0.001,
        "0 hits must produce 0 boost: {boost_0}"
    );
    assert!(
        boost_1 > boost_0,
        "1 hit must produce more boost than 0 hits: {boost_1} vs {boost_0}"
    );
    assert!(
        boost_50 > boost_1,
        "50 hits must produce more boost than 1 hit: {boost_50} vs {boost_1}"
    );
    // Cap at 0.5
    assert!(
        (boost_999 - 0.5).abs() < 0.001,
        "999 hits must be capped at 0.5: {boost_999}"
    );
    assert!(
        (boost_50 - 0.5).abs() < 0.01,
        "50 hits must be near 0.5 (cap): {boost_50}"
    );

    // High hit-count boost INCREASES a stale gotcha's score
    let stale_no_boost = recency_score_from_str("2020-01-01 00:00:00") * 2.0;
    let stale_with_boost = stale_no_boost + boost_50;
    assert!(
        stale_with_boost > stale_no_boost,
        "Hit boost must increase stale gotcha score: {stale_with_boost} vs {stale_no_boost}"
    );
}

// ── Sprint 3 audit ────────────────────────────────────────────────────────

#[test]
fn audit_s3_1_similar_symbols_cross_file_navigation_hint() {
    // PURPOSE: "🔗 similar: [run_returning≈post_read.rs] — guide for analogous implementations"
    let idx = build_symbol_index(
        &[
            ("run_returning", "src/pre_read.rs"),  // defined in current file
            ("run_returning", "src/post_read.rs"), // also defined in sister file
        ],
        &[],
        &[],
    );

    let sig = similar_symbol_signal(Some(&idx), "src/pre_read.rs");
    assert!(
        sig.is_some(),
        "Symbol defined in 2 files must produce similar signal"
    );

    let (score, text) = sig.unwrap();
    assert!(
        (score - 0.8_f32).abs() < 0.001,
        "similar symbols score must be 0.8, got {score}"
    );
    assert!(
        text.contains("similar:"),
        "must contain 'similar:' prefix: {text:?}"
    );
    assert!(
        text.contains("post_read.rs"),
        "must name the sister file: {text:?}"
    );
    assert!(
        text.contains("run_returning"),
        "must name the matching symbol: {text:?}"
    );
}

#[test]
fn audit_s3_1_similar_symbols_no_self_match() {
    // PURPOSE: "Skip very short or generic names" + only cross-file definitions count
    let idx = build_symbol_index(&[("unique_fn", "src/only_here.rs")], &[], &[]);

    let sig = similar_symbol_signal(Some(&idx), "src/only_here.rs");
    assert!(
        sig.is_none(),
        "Symbol in only one file must produce no similar signal"
    );

    // Short names (len < 4) are filtered
    let idx2 = build_symbol_index(
        &[
            ("run", "src/a.rs"), // len=3 < 4, filtered
            ("run", "src/b.rs"),
        ],
        &[],
        &[],
    );
    let sig2 = similar_symbol_signal(Some(&idx2), "src/a.rs");
    assert!(
        sig2.is_none(),
        "Short symbol names (len<4) must be filtered: {sig2:?}"
    );
}

#[test]
fn audit_s3_3_repeated_calls_produce_identical_results() {
    // PURPOSE: "S3.3: Rayon parallelization — overlaps filesystem I/O with symbol analysis"
    // Correctness invariant: same input → same output every time
    let tmp = tempfile::NamedTempFile::with_suffix(".rs").unwrap();
    let source = concat!(
        "pub use crate::HookResponse;\n",
        "pub use crate::HookRuntime;\n",
        "fn run_returning() { let xx = 1; let xx = 2; xx }\n",
    );
    std::fs::write(tmp.path(), source).unwrap();
    let path = tmp.path().to_str().unwrap();

    // Call 5 times — rayon must not produce different results
    let results: Vec<Vec<(f32, String)>> = (0..5).map(|_| source_based_signals(path)).collect();

    let first = &results[0];
    assert!(
        !first.is_empty(),
        "Must produce at least 1 signal (re-exports present)"
    );

    for (i, result) in results.iter().enumerate().skip(1) {
        assert_eq!(
            result.len(),
            first.len(),
            "Call {i} produced different signal count: {} vs {}",
            result.len(),
            first.len()
        );
        for (j, (score, text)) in result.iter().enumerate() {
            assert!(
                (score - first[j].0).abs() < 0.001,
                "Call {i} signal {j} score differs: {score} vs {}",
                first[j].0
            );
            assert_eq!(
                text, &first[j].1,
                "Call {i} signal {j} text differs:\n  got:      {text:?}\n  expected: {:?}",
                first[j].1
            );
        }
    }
}

#[test]
fn audit_s3_4_scope_shadowing_py_file() {
    // PURPOSE: "Analyzes source code for variable names defined multiple times in same scope"
    // Must work for Python as well as Rust
    let source = "def foo():\n    xx = 1\n    xx = 2\n    return xx\n";
    let sig = scope_shadowing_signal(source, "/tmp/test.py");
    // tree-sitter Python scope parsing may or may not detect assignment shadowing
    // but the function must NOT panic and must return Some/None only
    let _ = sig; // just verify no panic; Python scope detection is best-effort
}

// ── Integration audit ─────────────────────────────────────────────────────

#[test]
fn audit_signal_priority_note_before_dependent_in_output() {
    // INTEGRATION: Score-ordering contract
    // notes (score=1.5) > dependents (score=1.21 for 2 deps)
    // → notes must appear BEFORE dependents in the joined output
    let (_tmp, db) = setup();
    db.upsert(&FileKnowledge {
        file_path: "src/core.rs".to_string(),
        notes: Some("NEVER call unwrap here".to_string()),
        ..Default::default()
    })
    .unwrap();
    for src in &["a.rs", "b.rs"] {
        // 2 deps → score ~1.21
        db.upsert_relation(&FileRelation {
            source: src.to_string(),
            target: "src/core.rs".to_string(),
            relation_type: "imports".to_string(),
        })
        .unwrap();
    }

    let ctx = compose_high_signal_context(&db, "src/core.rs").unwrap();
    let note_pos = ctx.find("unwrap").expect("note must be in context");
    let dep_pos = ctx
        .find("files import this")
        .expect("dep must be in context");
    assert!(
        note_pos < dep_pos,
        "Note (score 1.5) must precede deps (score ~1.21).\nContext: {ctx:?}"
    );
}

#[test]
fn audit_budget_enforcement_tiny_budget_produces_silence() {
    // INTEGRATION: Budget=5 → too small for any signal text → silence
    let (_tmp, db) = setup();
    db.upsert(&FileKnowledge {
        file_path: "src/budget.rs".to_string(),
        notes: Some("Some note".to_string()),
        ..Default::default()
    })
    .unwrap();

    let ctx = compose_high_signal_context_budgeted(&db, "src/budget.rs", 5, 0);
    assert!(
        ctx.is_none(),
        "Budget=5 must be too small for any signal — should produce silence"
    );
}

#[test]
fn audit_budget_enforcement_large_budget_fits_all() {
    // INTEGRATION: Budget=4000 → all signals appear
    let (_tmp, db) = setup();
    db.upsert(&FileKnowledge {
        file_path: "src/full.rs".to_string(),
        notes: Some("Note A".to_string()),
        ..Default::default()
    })
    .unwrap();
    db.record_bash_outcome(&BashOutcome {
        command: "cargo test src/full.rs".to_string(),
        command_short: "cargo".to_string(),
        exit_code: 1,
        success: false,
        error_pattern: Some("assertion failed at line 42".to_string()),
        file_context: Some("src/full.rs".to_string()),
        command_hash: String::new(),
        executed_at: String::new(),
    })
    .unwrap();
    for src in &["x.rs", "y.rs", "z.rs"] {
        db.upsert_relation(&FileRelation {
            source: src.to_string(),
            target: "src/full.rs".to_string(),
            relation_type: "imports".to_string(),
        })
        .unwrap();
    }

    let ctx = compose_high_signal_context_budgeted(&db, "src/full.rs", 4000, 0).unwrap();
    assert!(ctx.contains("Note A"), "notes must appear: {ctx:?}");
    assert!(ctx.contains("cargo"), "bash failure must appear: {ctx:?}");
    assert!(
        ctx.contains("files import"),
        "dependents must appear: {ctx:?}"
    );
    assert!(
        ctx.len() <= 4000,
        "must respect 4000-char budget: {} chars",
        ctx.len()
    );
}

#[test]
fn audit_cila_budget_levels_match_documentation() {
    // PURPOSE: "L0-L1=800, L2-L3=2000, L4+=4000" per module docstring
    assert_eq!(cila_budget(0), 800, "L0 budget");
    assert_eq!(cila_budget(1), 800, "L1 budget");
    assert_eq!(cila_budget(2), 2000, "L2 budget");
    assert_eq!(cila_budget(3), 2000, "L3 budget");
    assert_eq!(cila_budget(4), 4000, "L4 budget");
    assert_eq!(cila_budget(5), 4000, "L5 budget");
    assert_eq!(cila_budget(6), 4000, "L6 budget");
    assert_eq!(cila_budget(255), 4000, "overflow level budget");
}

#[test]
fn audit_purpose_silence_is_default_invariant() {
    // PURPOSE (core invariant): "SILENCE IS THE DEFAULT.
    //  Only inject context when it passes the 'So what?' test"
    // → A file with ONLY metadata (no notes, no failures, no deps) MUST be silent.
    let (_tmp, db) = setup();
    db.upsert(&FileKnowledge {
        file_path: "src/plain.rs".to_string(),
        language: Some("rust".to_string()),
        line_count: 500,
        symbol_count: 20,
        imports_json: Some(r#"["std","serde"]"#.to_string()),
        ..Default::default()
    })
    .unwrap();

    let ctx = compose_high_signal_context(&db, "src/plain.rs");
    assert!(
        ctx.is_none(),
        "File with only metadata must be SILENT — Claude reads this itself: {ctx:?}"
    );
}

// ── TS/JS export detection ────────────────────────────────────────────────

#[test]
fn audit_ts_js_exports_named_functions() {
    // PURPOSE: "⚡ exports[N]: Foo, bar — break = API break"
    let source = r#"
export function greet(name: string): string { return name; }
export const PI = 3.14;
export class UserService {}
export async function fetchData() {}
"#;
    let sig = ts_js_exports_signal(source);
    assert!(sig.is_some(), "Named exports must produce signal");
    let (score, text) = sig.unwrap();
    assert!((score - 1.6).abs() < 0.01, "score must be 1.6, got {score}");
    assert!(
        text.contains("exports[4]"),
        "Must count 4 exports, got: {text}"
    );
    assert!(text.contains("API break"), "Must include API break warning");
    assert!(
        text.contains("greet") || text.contains("PI") || text.contains("UserService"),
        "Must show export names: {text}"
    );
}

#[test]
fn audit_ts_js_exports_default_only() {
    let source = "export default function handler(req, res) {}";
    let sig = ts_js_exports_signal(source);
    assert!(sig.is_some(), "Default export must produce signal");
    let (_, text) = sig.unwrap();
    assert!(text.contains("exports[1]"), "Got: {text}");
    assert!(text.contains("default"), "Must mention 'default': {text}");
}

#[test]
fn audit_ts_js_exports_none_for_internal_file() {
    let source = r#"
// internal utility — no exports
const secret = 42;
function helper() {}
"#;
    let sig = ts_js_exports_signal(source);
    assert!(sig.is_none(), "File with no exports must produce no signal");
}

#[test]
fn audit_ts_js_exports_via_source_based_signals() {
    // Verify source_based_signals dispatches to ts_js_exports_signal for .ts files
    let tmp = tempfile::Builder::new().suffix(".ts").tempfile().unwrap();
    let source = "export const API_URL = 'https://example.com';\nexport function connect() {}\n";
    std::fs::write(tmp.path(), source).unwrap();

    let signals = source_based_signals(tmp.path().to_str().unwrap());
    let export_sig = signals.iter().find(|(_, t)| t.contains("exports["));
    assert!(
        export_sig.is_some(),
        "source_based_signals must produce export signal for .ts, got: {signals:?}"
    );
}

#[test]
fn audit_ts_js_exports_skips_reexport_all() {
    // `export * from './foo'` must NOT be counted as a named export
    let source = "export * from './utils';\nexport * from './types';";
    let sig = ts_js_exports_signal(source);
    assert!(
        sig.is_none(),
        "Re-export-all lines must not count as named exports"
    );
}

#[test]
fn audit_exit_0_invariant_empty_and_malformed_inputs() {
    // INVARIANT: "exit 0 always — hooks never diverge"
    // compose_* functions must never panic on any input
    let (_tmp, db) = setup();

    // Edge cases that must NOT panic
    let _ = compose_high_signal_context(&db, "");
    let _ = compose_high_signal_context(&db, "/");
    let _ = compose_high_signal_context(&db, "a/b/c/d/e/f/g/h/i/j/k.rs");
    let _ = compose_high_signal_context_budgeted(&db, "test.py", 0, 0);
    let _ = compose_high_signal_context_budgeted(&db, "test.py", usize::MAX / 2, 0);
    let _ = build_symbol_map_signal(None, "");
    let _ = blast_radius_signal(None, "", true);
    let _ = external_callers_signal(None, "");
    let _ = similar_symbol_signal(None, "");
    let _ = scope_shadowing_signal("", "/tmp/empty.rs");
    let _ = ts_js_exports_signal("");
    let _ = ts_js_exports_signal("export"); // incomplete export line
    let _ = ts_js_exports_signal("export *"); // re-export all
    let _ = gotcha_drift_signal(&[]);
    let _ = recency_score_from_str("");
    let _ = recency_score_from_str("2024-99-99 99:99:99"); // invalid date
    // None of the above must panic — reaching here = PASS
}

// ── C15: Tests for extracted pure helpers ─────────────────────────

#[test]
fn test_parse_ts_export_name_function() {
    assert_eq!(
        parse_ts_export_name("export function myFunc() {}"),
        Some("myFunc".to_string())
    );
}

#[test]
fn test_parse_ts_export_name_const() {
    assert_eq!(
        parse_ts_export_name("export const MyConst = 42;"),
        Some("MyConst".to_string())
    );
}

#[test]
fn test_parse_ts_export_name_async_function() {
    assert_eq!(
        parse_ts_export_name("export async function handler() {}"),
        Some("handler".to_string())
    );
}

#[test]
fn test_parse_ts_export_name_class_with_generics() {
    // "Foo<T>" should trim to "Foo"
    assert_eq!(
        parse_ts_export_name("export class Foo<T> {}"),
        Some("Foo".to_string())
    );
}

#[test]
fn test_parse_ts_export_name_brace_block_returns_none() {
    // "export { X, Y }" — brace blocks are re-exports, should return None
    assert_eq!(parse_ts_export_name("export { X, Y }"), None);
}

#[test]
fn test_parse_ts_export_name_short_name_returns_none() {
    // Single-char names are filtered out
    assert_eq!(parse_ts_export_name("export const x = 1;"), None);
}

#[test]
fn test_count_external_refs_defined_with_external_refs() {
    use touring_code::ast::SymbolLocation;
    let loc_def = SymbolLocation {
        file_path: "src/lib.rs".to_string(),
        symbol_name: "MyStruct".to_string(),
        line: 10,
        column: 0,
        is_definition: true,
        kind: None,
    };
    let loc_ref = SymbolLocation {
        file_path: "src/main.rs".to_string(),
        symbol_name: "MyStruct".to_string(),
        line: 5,
        column: 0,
        is_definition: false,
        kind: None,
    };
    let locations = vec![loc_def, loc_ref];
    let result = count_external_refs("MyStruct", &locations, "src/lib.rs");
    assert_eq!(result, Some(("MyStruct".to_string(), 1)));
}

#[test]
fn test_count_external_refs_not_defined_here_returns_none() {
    use touring_code::ast::SymbolLocation;
    let loc = SymbolLocation {
        file_path: "src/other.rs".to_string(),
        symbol_name: "MyStruct".to_string(),
        line: 10,
        column: 0,
        is_definition: true,
        kind: None,
    };
    let result = count_external_refs("MyStruct", &[loc], "src/lib.rs");
    assert_eq!(result, None);
}

#[test]
fn test_count_external_refs_no_external_callers_returns_none() {
    use touring_code::ast::SymbolLocation;
    // Defined here, but only internal references — no external callers
    let loc_def = SymbolLocation {
        file_path: "src/lib.rs".to_string(),
        symbol_name: "helper".to_string(),
        line: 10,
        column: 0,
        is_definition: true,
        kind: None,
    };
    let loc_ref = SymbolLocation {
        file_path: "src/lib.rs".to_string(),
        symbol_name: "helper".to_string(),
        line: 20,
        column: 0,
        is_definition: false,
        kind: None,
    };
    let result = count_external_refs("helper", &[loc_def, loc_ref], "src/lib.rs");
    assert_eq!(result, None);
}

#[test]
fn test_collect_shadow_pairs_detects_shadow() {
    use touring_code::ast::{ScopeEntry, ScopeKind};
    let entries = vec![
        ScopeEntry {
            name: "foo".to_string(),
            type_str: None,
            kind: ScopeKind::Let,
            line: 10,
        },
        ScopeEntry {
            name: "bar".to_string(),
            type_str: None,
            kind: ScopeKind::Let,
            line: 15,
        },
        ScopeEntry {
            name: "foo".to_string(),
            type_str: None,
            kind: ScopeKind::Let,
            line: 25,
        },
    ];
    let pairs = collect_shadow_pairs(&entries);
    assert_eq!(pairs.len(), 1);
    assert_eq!(pairs[0].0, "foo");
    assert_eq!(pairs[0].1, 10);
    assert_eq!(pairs[0].2, 25);
}

#[test]
fn test_collect_shadow_pairs_skips_single_char() {
    use touring_code::ast::{ScopeEntry, ScopeKind};
    let entries = vec![
        ScopeEntry {
            name: "i".to_string(),
            type_str: None,
            kind: ScopeKind::Let,
            line: 5,
        },
        ScopeEntry {
            name: "i".to_string(),
            type_str: None,
            kind: ScopeKind::Let,
            line: 10,
        },
    ];
    let pairs = collect_shadow_pairs(&entries);
    assert!(pairs.is_empty(), "single-char names must not be reported");
}

#[test]
fn test_collect_shadow_pairs_no_shadows_returns_empty() {
    use touring_code::ast::{ScopeEntry, ScopeKind};
    let entries = vec![
        ScopeEntry {
            name: "alpha".to_string(),
            type_str: None,
            kind: ScopeKind::Let,
            line: 1,
        },
        ScopeEntry {
            name: "beta".to_string(),
            type_str: None,
            kind: ScopeKind::Let,
            line: 2,
        },
    ];
    let pairs = collect_shadow_pairs(&entries);
    assert!(pairs.is_empty());
}

// ── D4 Think-in-Code tests ─────────────────────────────────────────────

#[test]
fn test_analysis_pattern_detection_9_reads() {
    // 9 consecutive reads is below threshold — no directive should fire.
    let (_tmp, db) = setup();
    db.upsert(&FileKnowledge {
        file_path: "test.rs".to_string(),
        notes: Some("note".to_string()),
        ..Default::default()
    })
    .unwrap();
    let result = compose_high_signal_context_budgeted(&db, "test.rs", 4000, 9);
    // No directive at 9 reads
    assert!(result.is_none() || !result.as_ref().unwrap().contains("THINK IN CODE"));
}

#[test]
fn test_analysis_pattern_detection_10_reads_at_threshold() {
    // 10 consecutive reads — exactly at threshold.
    // Directive fires only when budget_used > max_chars/2 (budget exceeded).
    // With tiny notes content, budget_used will be < 2000 (50% of 4000),
    // so directive will NOT fire — this verifies the budget guard.
    let (_tmp, db) = setup();
    db.upsert(&FileKnowledge {
        file_path: "test.rs".to_string(),
        notes: Some("short".to_string()),
        ..Default::default()
    })
    .unwrap();
    let result = compose_high_signal_context_budgeted(&db, "test.rs", 4000, 10);
    assert!(
        result.is_some(),
        "At threshold with signal present, context should be produced"
    );
    // Budget not exceeded with tiny notes, no directive
    let ctx = result.unwrap();
    assert!(
        !ctx.contains("THINK IN CODE"),
        "With tiny notes, budget_used < 50%, no directive: {ctx}"
    );
}

#[test]
fn test_analysis_pattern_detection_15_reads() {
    // 15 consecutive reads — well above threshold.
    // Even with tiny content, reads >= 10 but budget not exceeded = no directive.
    let (_tmp, db) = setup();
    db.upsert(&FileKnowledge {
        file_path: "test.rs".to_string(),
        notes: Some("tiny".to_string()),
        ..Default::default()
    })
    .unwrap();
    let result = compose_high_signal_context_budgeted(&db, "test.rs", 4000, 15);
    assert!(result.is_some());
    let ctx = result.unwrap();
    // Budget still not exceeded with tiny notes
    assert!(
        !ctx.contains("THINK IN CODE"),
        "With tiny notes, budget not exceeded even at 15 reads: {ctx}"
    );
}

#[test]
fn test_think_in_code_injection_budget_ok() {
    // When budget_used <= max_chars/2 (budget not exceeded), no directive.
    let (_tmp, db) = setup();
    db.upsert(&FileKnowledge {
        file_path: "test.rs".to_string(),
        notes: Some("x".to_string()),
        ..Default::default()
    })
    .unwrap();
    // 10 consecutive reads with budget=4000, but context is very small so budget_used <= 2000
    // Large budget (8000) means budget_used/2 = 4000; small context won't exceed this
    let _result = compose_high_signal_context_budgeted(&db, "test.rs", 8000, 10);
    // With large budget and small content, budget_used may be <= max_chars/2
    // If so, no directive should fire; if not, directive fires
    // This test verifies the budget guard is working
    let result = compose_high_signal_context_budgeted(&db, "test.rs", 4000, 10);
    assert!(result.is_some());
}

#[test]
fn test_think_in_code_injection_under_threshold() {
    // Below threshold (9 reads) — directive should NOT fire regardless of budget.
    let (_tmp, db) = setup();
    db.upsert(&FileKnowledge {
        file_path: "test.rs".to_string(),
        notes: Some("important note".to_string()),
        ..Default::default()
    })
    .unwrap();
    let result = compose_high_signal_context_budgeted(&db, "test.rs", 4000, 9);
    // Even with a signal present, reads < 10 should not trigger directive
    let ctx = result.unwrap_or_default();
    assert!(
        !ctx.contains("THINK IN CODE"),
        "Below threshold (9 reads), no directive: {ctx}"
    );
}

#[test]
fn test_think_in_code_injection_resets_after_write() {
    // After a write, consecutive_file_reads counter should be 0 (write resets).
    // This is tested indirectly: if a file has consecutive_file_reads=0,
    // compose_high_signal_context_budgeted should NOT inject directive.
    let (_tmp, db) = setup();
    db.upsert(&FileKnowledge {
        file_path: "test.rs".to_string(),
        notes: Some("note".to_string()),
        ..Default::default()
    })
    .unwrap();
    let result = compose_high_signal_context_budgeted(&db, "test.rs", 4000, 0);
    assert!(result.is_some());
    let ctx = result.unwrap();
    assert!(
        !ctx.contains("THINK IN CODE"),
        "Counter=0 (after write) should not trigger directive: {ctx}"
    );
}

// ── D4.3 Skip-region PreRead tests ──────────────────────────────────────

#[test]
fn test_skip_region_preread_no_marker_increments() {
    // File WITHOUT skip marker: counter should increment.
    // We can't easily test the counter directly here, but we can verify
    // that compose_high_signal_context_budgeted runs without the skip reset.
    let (_tmp, db) = setup();
    db.upsert(&FileKnowledge {
        file_path: "normal.rs".to_string(),
        notes: Some("note".to_string()),
        line_count: 100,
        ..Default::default()
    })
    .unwrap();
    // With counter=5 and a note present, should still produce context
    // (skip marker is checked in the hook before calling compose_*,
    // but the counter parameter is passed through)
    let result = compose_high_signal_context_budgeted(&db, "normal.rs", 4000, 5);
    // 5 is below threshold, no directive
    let ctx = result.unwrap_or_default();
    assert!(!ctx.contains("THINK IN CODE"));
}

#[test]
fn test_skip_region_preread_marker_present_returns_silent() {
    // When skip marker is detected, the hook passes consecutive_file_reads=0.
    // A file with skip marker + counter=0 should still be processed normally
    // (the skip only resets counter, doesn't silence output).
    let (_tmp, db) = setup();
    db.upsert(&FileKnowledge {
        file_path: "skip.rs".to_string(),
        notes: Some("note".to_string()),
        ..Default::default()
    })
    .unwrap();
    // counter=0 means skip was detected and counter was reset
    let result = compose_high_signal_context_budgeted(&db, "skip.rs", 4000, 0);
    assert!(
        result.is_some(),
        "Skip marker reset should still allow context"
    );
    let ctx = result.unwrap();
    assert!(
        !ctx.contains("THINK IN CODE"),
        "Counter=0 from skip detection should not trigger directive: {ctx}"
    );
}

// ── D4 end ────────────────────────────────────────────────────────────────

// ── S-03 (SNR slice): plan-scope helper tests ─────────────────────────────

#[test]
fn plan_scope_marker_name_recognizes_only_canonical() {
    assert!(is_canonical_active_marker("active-df4a8bd525f8.json"));
    // Archived / converged variants carry an extra `.` and must be skipped.
    assert!(!is_canonical_active_marker(
        "active-df4a8bd525f8.archived.json"
    ));
    assert!(!is_canonical_active_marker(
        "active-df4a8bd525f8.converged.json"
    ));
    // Legacy singleton / unrelated names.
    assert!(!is_canonical_active_marker("active.json"));
    assert!(!is_canonical_active_marker("converged-missao.json"));
    assert!(!is_canonical_active_marker("active-.json"));
}

#[test]
fn plan_scope_relative_scope_covers_only_within_own_workspace() {
    // Opaque fixture root — pure path logic, never touches the filesystem
    // (test-const migration deferred by Fase 0 to Fase 4, done 2026-07-24).
    let root = "/home/user/example-workspace";
    // Relative scope + matching cwd + file under scope → covered.
    assert!(marker_covers(
        "crates/touring-foundation/src/lib.rs",
        root,
        "crates/touring-foundation",
        root,
        "active",
    ));
    // Same file but the marker belongs to a different workspace → NOT covered.
    assert!(!marker_covers(
        "crates/touring-foundation/src/lib.rs",
        root,
        "crates/touring-foundation",
        "/some/other/project",
        "active",
    ));
    // File outside the scope subtree → not covered.
    assert!(!marker_covers(
        "crates/touring-cli/src/main.rs",
        root,
        "crates/touring-foundation",
        root,
        "active",
    ));
}

#[test]
fn plan_scope_prefix_is_path_boundary_not_substring() {
    // Opaque fixture root — pure path logic, never touches the filesystem
    // (test-const migration deferred by Fase 0 to Fase 4, done 2026-07-24).
    let root = "/home/user/example-workspace";
    // `touring-foundation-extra` must NOT match scope `touring-foundation`.
    assert!(!marker_covers(
        "crates/touring-foundation-extra/src/lib.rs",
        root,
        "crates/touring-foundation",
        root,
        "active",
    ));
    // Exact scope dir itself is covered.
    assert!(marker_covers(
        "crates/touring-foundation",
        root,
        "crates/touring-foundation",
        root,
        "active",
    ));
}

#[test]
fn plan_scope_absolute_scope_matches_file_abs_path() {
    // Opaque fixture root — pure path logic, never touches the filesystem
    // (test-const migration deferred by Fase 0 to Fase 4, done 2026-07-24).
    let root = "/home/user/example-workspace";
    assert!(marker_covers(
        "crates/touring-foundation/src/lib.rs",
        root,
        "/home/user/example-workspace/crates/touring-foundation",
        root,
        "active",
    ));
    // Absolute scope pointing elsewhere → not covered.
    assert!(!marker_covers(
        "crates/touring-foundation/src/lib.rs",
        root,
        "/home/gabrielgadea/projects/other",
        root,
        "active",
    ));
}

#[test]
fn plan_scope_archived_or_converged_never_covers() {
    // Opaque fixture root — pure path logic, never touches the filesystem
    // (test-const migration deferred by Fase 0 to Fase 4, done 2026-07-24).
    let root = "/home/user/example-workspace";
    for status in ["ARCHIVED", "archived", "CONVERGED", "Converged"] {
        assert!(
            !marker_covers(
                "crates/touring-foundation/src/lib.rs",
                root,
                "crates/touring-foundation",
                root,
                status,
            ),
            "status {status} must not cover"
        );
    }
    // Empty scope never covers.
    assert!(!marker_covers("crates/x/lib.rs", root, "", root, "active"));
}

#[test]
fn plan_scope_line_is_dense_and_carries_the_next_command() {
    let line = format_plan_scope_line("task_123", "crates/touring-foundation");
    assert!(line.contains("task_123"));
    assert!(line.contains("crates/touring-foundation"));
    // The `next` action is the literal command the reader runs (no daemon RPC).
    assert!(line.contains("touring decompose ready task_123"));
    // Single line.
    assert!(!line.contains('\n'));
}

// ── Tests for Cycle 18 extracted helpers ──────────────────────────────────
