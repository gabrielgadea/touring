//! Wave 23 — E2E equivalence proof: `wiring_modules_aggregate` ≡ legacy
//! per-module loop over `module_wiring_status`.
//!
//! Wave 22 (S-Q1a) replaced an `O(N*3)` per-module query loop in
//! `cli_wiring_modules` with a single `GROUP BY` aggregate. The unit tests
//! in `src/wiring.rs` cover the new aggregate's behaviour in isolation. This
//! E2E test pins the *equivalence* between the two implementations across a
//! diverse fixture so any future regression that drifts one path from the
//! other fails loudly.
//!
//! Wired off `FileKnowledgeDB` directly (the same surface that
//! `cli-wiring-modules` consumes), so this also exercises the SQL path
//! end-to-end on a real on-disk SQLite file.

use tempfile::TempDir;
use touring_hooks::knowledge::FileKnowledgeDB;

const FP_EPS: f64 = 1e-9;

/// Create a fresh on-disk knowledge DB for the test.
fn fresh_db() -> (TempDir, FileKnowledgeDB) {
    let tmp = TempDir::new().expect("tempdir");
    let path = tmp.path().join("knowledge.db");
    let db = FileKnowledgeDB::new(&path).expect("open FileKnowledgeDB");
    (tmp, db)
}

/// Seed a heterogeneous fixture exercising every interesting combination:
/// fully orphan modules, fully wired modules, partially wired modules,
/// modules with multiple consumers per symbol, and modules with non-public
/// visibility entries that must be excluded from totals.
fn seed_diverse_fixture(db: &FileKnowledgeDB) -> Vec<&'static str> {
    // Module A — fully orphan (3 pub, 0 consumers).
    db.register_pub_symbol("src/a.rs", "A1", "struct", "public")
        .unwrap();
    db.register_pub_symbol("src/a.rs", "A2", "function", "public")
        .unwrap();
    db.register_pub_symbol("src/a.rs", "A3", "enum", "public")
        .unwrap();

    // Module B — fully wired (2 pub, both with consumers, B1 has multiple).
    db.register_pub_symbol("src/b.rs", "B1", "struct", "public")
        .unwrap();
    db.register_pub_symbol("src/b.rs", "B2", "trait", "public")
        .unwrap();
    db.record_consumer("src/b.rs", "B1", "src/b_consumer1.rs", Some(10))
        .unwrap();
    db.record_consumer("src/b.rs", "B1", "src/b_consumer2.rs", Some(11))
        .unwrap();
    db.record_consumer("src/b.rs", "B2", "src/b_consumer1.rs", Some(12))
        .unwrap();

    // Module C — partial (4 pub, 2 wired).
    db.register_pub_symbol("src/c.rs", "C1", "struct", "public")
        .unwrap();
    db.register_pub_symbol("src/c.rs", "C2", "struct", "public")
        .unwrap();
    db.register_pub_symbol("src/c.rs", "C3", "function", "public")
        .unwrap();
    db.register_pub_symbol("src/c.rs", "C4", "function", "public")
        .unwrap();
    db.record_consumer("src/c.rs", "C1", "src/c_consumer.rs", None)
        .unwrap();
    db.record_consumer("src/c.rs", "C2", "src/c_consumer.rs", None)
        .unwrap();

    // Module D — single pub symbol with a single consumer (boundary case).
    db.register_pub_symbol("src/d.rs", "D1", "function", "public")
        .unwrap();
    db.record_consumer("src/d.rs", "D1", "src/d_consumer.rs", None)
        .unwrap();

    // Module E — mixed visibility: pub + crate-visible. Aggregate must count
    // ONLY the public symbol; the legacy path filters the same way.
    db.register_pub_symbol("src/e.rs", "E1", "struct", "public")
        .unwrap();
    db.register_pub_symbol("src/e.rs", "E2_internal", "struct", "pub(crate)")
        .unwrap();
    db.record_consumer("src/e.rs", "E1", "src/e_consumer.rs", None)
        .unwrap();
    // A consumer recorded against the non-public symbol must NOT inflate the
    // wired_count for module E in either implementation.
    db.record_consumer("src/e.rs", "E2_internal", "src/e_consumer.rs", None)
        .unwrap();

    vec!["src/a.rs", "src/b.rs", "src/c.rs", "src/d.rs", "src/e.rs"]
}

#[test]
fn wiring_modules_aggregate_matches_legacy_per_module_loop() {
    let (_tmp, db) = fresh_db();
    let modules = seed_diverse_fixture(&db);

    // ── New path (Wave 22): single GROUP BY aggregate. ──────────────────
    let agg_rows = db
        .wiring_modules_aggregate()
        .expect("aggregate query must succeed");

    // Expect one row per module with at least one public symbol — module E
    // contributes a single row even though it also has a `pub(crate)` row.
    assert_eq!(
        agg_rows.len(),
        modules.len(),
        "aggregate should return exactly one row per module with public symbols"
    );

    // Aggregate query is `ORDER BY module_file`; verify the ordering is stable
    // so callers that rely on it (cli_wiring_modules dashboard) are protected.
    let mut sorted = modules.clone();
    sorted.sort();
    let agg_paths: Vec<&str> = agg_rows.iter().map(|r| r.module_file.as_str()).collect();
    assert_eq!(agg_paths, sorted, "aggregate must be ORDER BY module_file");

    // ── Legacy path: O(N) loop over `module_wiring_status`. ─────────────
    for module in &modules {
        let agg = agg_rows
            .iter()
            .find(|r| r.module_file == *module)
            .unwrap_or_else(|| panic!("aggregate row missing for module {module}"));

        let legacy = db
            .module_wiring_status(module)
            .expect("legacy module_wiring_status must succeed");

        // Equivalence axes — totals, wired counts, and integration score.
        assert_eq!(
            agg.total_pub as usize, legacy.total_pub_symbols,
            "total_pub mismatch for module {module}"
        );
        assert_eq!(
            agg.wired_count as usize, legacy.symbols_with_consumers,
            "wired_count mismatch for module {module}"
        );

        let agg_score = agg.integration_score();
        let delta = (agg_score - legacy.integration_score).abs();
        assert!(
            delta < FP_EPS,
            "integration_score mismatch for {module}: agg={agg_score} legacy={} (delta={delta})",
            legacy.integration_score
        );
    }
}

#[test]
fn wiring_modules_aggregate_empty_db_matches_legacy() {
    let (_tmp, db) = fresh_db();

    let agg = db
        .wiring_modules_aggregate()
        .expect("aggregate on empty DB");
    assert!(agg.is_empty(), "empty DB must yield zero aggregate rows");

    // Legacy query against a non-existent module returns zeroes (no error)
    // — confirm both paths agree on the "nothing here" semantic.
    let legacy = db
        .module_wiring_status("src/does_not_exist.rs")
        .expect("legacy query against missing module");
    assert_eq!(legacy.total_pub_symbols, 0);
    assert_eq!(legacy.symbols_with_consumers, 0);
    // Empty modules score 1.0 on both paths.
    assert!((legacy.integration_score - 1.0).abs() < FP_EPS);
}

#[test]
fn wiring_modules_aggregate_distinct_consumers_match_legacy() {
    // Multiple consumers of the SAME symbol must collapse to a single
    // wired_count contribution in both paths (DISTINCT semantics).
    let (_tmp, db) = fresh_db();
    db.register_pub_symbol("src/m.rs", "S", "function", "public")
        .unwrap();

    for i in 0..5 {
        db.record_consumer("src/m.rs", "S", &format!("src/c_{i}.rs"), Some(i))
            .unwrap();
    }

    let agg_rows = db.wiring_modules_aggregate().unwrap();
    assert_eq!(agg_rows.len(), 1);
    assert_eq!(agg_rows[0].total_pub, 1, "single pub symbol");
    assert_eq!(
        agg_rows[0].wired_count, 1,
        "DISTINCT collapses 5 consumers → 1"
    );

    let legacy = db.module_wiring_status("src/m.rs").unwrap();
    assert_eq!(legacy.total_pub_symbols, 1);
    assert_eq!(legacy.symbols_with_consumers, 1);
    assert!((agg_rows[0].integration_score() - legacy.integration_score).abs() < FP_EPS);
}
