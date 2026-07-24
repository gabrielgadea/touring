//! Regression suite for the 2026-05-11 orphan-count diagnostic audit.
//!
//! Locks in the behaviour change for the 5 audit fixes:
//!
//! * **F1**: SQL filter rejects benches/tests/docs/scripts in any path
//!   position (leading or nested) AND non-`.rs` extensions.
//! * **F2**: Producer and consumer rows are stored under workspace-relative
//!   paths regardless of how the caller passes them (absolute vs relative);
//!   `migrate_canonicalize_paths` rewrites legacy absolute paths in place.
//! * **F3**: `record_consumer` invoked for lowercase symbol names (free
//!   functions, methods, modules) successfully resolves the orphan — i.e.
//!   the wiring layer itself never had the uppercase-only restriction;
//!   the bug was in the post_read import resolver, fixed there.
//! * **F4**: `register_pub_symbol`/`record_consumer` silently skip
//!   `.py`/`.md`/`benches/`/`tests/` paths so they never pollute the
//!   producer roster.
//! * **F5**: `wiring_db_diagnostic` surfaces row census fields used by
//!   `touring doctor` to detect pollution.
//!
//! These tests use an in-memory SQLite DB via `FileKnowledgeDB::new(":memory:")`
//! so they have no dependency on the production knowledge.db. Each test
//! builds its own dataset and asserts on the orphan_symbols() return value.

use std::path::Path;
use touring_hooks::knowledge::FileKnowledgeDB;

fn fresh_db() -> FileKnowledgeDB {
    FileKnowledgeDB::new(Path::new(":memory:")).expect("in-memory DB opens")
}

// ── F4: extension + benches/tests gate at write side ──────────────────────────

#[test]
fn f4_py_file_skipped_at_register_pub_symbol() {
    let db = fresh_db();
    db.register_pub_symbol("docs/plans/audit.py", "AuditResult", "class", "public")
        .expect("call returns Ok even when skipped");
    let orphans = db.orphan_symbols().expect("query orphans");
    assert!(
        orphans.iter().all(|e| !e.module_file.ends_with(".py")),
        "no .py producer must reach wiring_map; got: {:?}",
        orphans
    );
}

#[test]
fn f4_leading_benches_path_skipped() {
    let db = fresh_db();
    db.register_pub_symbol(
        "benches/src/hybrid_search_bench.rs",
        "MockEmbeddingProvider",
        "struct",
        "public",
    )
    .expect("call returns Ok even when skipped");
    let orphans = db.orphan_symbols().expect("query orphans");
    assert!(
        orphans.is_empty(),
        "leading-benches/ path must be filtered; got: {:?}",
        orphans
    );
}

#[test]
fn f4_nested_benches_path_skipped() {
    let db = fresh_db();
    db.register_pub_symbol(
        "crates/foo/benches/bench_main.rs",
        "BenchProvider",
        "struct",
        "public",
    )
    .expect("call returns Ok even when skipped");
    assert!(db.orphan_symbols().expect("query orphans").is_empty());
}

#[test]
fn f4_leading_tests_path_skipped() {
    let db = fresh_db();
    db.register_pub_symbol("tests/integration/foo.rs", "Helper", "struct", "public")
        .expect("Ok");
    assert!(db.orphan_symbols().expect("query orphans").is_empty());
}

#[test]
fn f4_eligible_rust_file_still_reaches_wiring_map() {
    let db = fresh_db();
    db.register_pub_symbol("crates/foo/src/lib.rs", "Foo", "struct", "public")
        .expect("Ok");
    let orphans = db.orphan_symbols().expect("query orphans");
    assert_eq!(orphans.len(), 1);
    assert_eq!(orphans[0].symbol_name, "Foo");
}

// ── F1: SQL guard masks legacy non-Rust rows even when the gate misses ──────

#[test]
fn f1_legacy_md_row_filtered_out_by_select() {
    let db = fresh_db();
    // Bypass the gate by inserting raw via SQL — simulating a legacy DB
    // that pre-dates F4. The SELECT must still hide it.
    db.conn_ref()
        .execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, contract_source)
             VALUES ('docs/plan.md', 'Section', 'heading', 'public', 'legacy')",
            [],
        )
        .expect("raw insert");
    let orphans = db.orphan_symbols().expect("query orphans");
    assert!(
        orphans.iter().all(|e| !e.module_file.ends_with(".md")),
        "F1 SQL guard must hide legacy non-.rs rows"
    );
}

// ── F2: path canonicalization at write side ───────────────────────────────────

#[test]
fn f2_absolute_path_at_register_becomes_relative_in_query() {
    let db = fresh_db();
    db.register_pub_symbol(
        "/home/gabrielgadea/.claude/rust/crates/foo/src/lib.rs",
        "Foo",
        "struct",
        "public",
    )
    .expect("Ok");
    let orphans = db.orphan_symbols().expect("query orphans");
    assert_eq!(orphans.len(), 1);
    assert_eq!(orphans[0].module_file, "crates/foo/src/lib.rs");
}

#[test]
fn f2_producer_absolute_consumer_relative_match() {
    let db = fresh_db();
    // Producer registered with absolute path
    db.register_pub_symbol(
        "/home/gabrielgadea/.claude/rust/crates/foo/src/lib.rs",
        "do_thing",
        "function",
        "public",
    )
    .expect("Ok");
    // Consumer registered with relative path — without canonicalization
    // this would fail to JOIN and `do_thing` would be a phantom orphan.
    db.record_consumer(
        "crates/foo/src/lib.rs",
        "do_thing",
        "crates/bar/src/main.rs",
        Some(7),
    )
    .expect("Ok");
    let orphans = db.orphan_symbols().expect("query orphans");
    assert!(
        orphans.is_empty(),
        "canonicalization must wire producer/consumer across abs/rel: {:?}",
        orphans
    );
}

// ── F3: record_consumer accepts any well-formed identifier ───────────────────
//
// The 2026-05-11 audit located the uppercase-only filter in
// `post_read.rs:257` (now removed). The wiring layer itself never had a
// case restriction; we lock that contract in so a future refactor can't
// re-introduce one quietly.

#[test]
fn f3_lowercase_function_resolves_orphan() {
    let db = fresh_db();
    db.register_pub_symbol("crates/foo/src/lib.rs", "compute", "function", "public")
        .expect("Ok");
    // Pre-fix: the import path `foo::compute` was discarded because `compute`
    // starts with a lowercase letter. Now record_consumer must accept it.
    db.record_consumer(
        "crates/foo/src/lib.rs",
        "compute",
        "crates/bar/src/lib.rs",
        Some(42),
    )
    .expect("Ok");
    assert!(db.orphan_symbols().expect("query orphans").is_empty());
}

#[test]
fn f3_lowercase_method_resolves_orphan() {
    let db = fresh_db();
    db.register_pub_symbol(
        "crates/foo/src/lib.rs",
        "validate_format",
        "method",
        "public",
    )
    .expect("Ok");
    db.record_consumer(
        "crates/foo/src/lib.rs",
        "validate_format",
        "crates/bar/src/main.rs",
        Some(99),
    )
    .expect("Ok");
    assert!(db.orphan_symbols().expect("query orphans").is_empty());
}

// ── F5: wiring_db_diagnostic surfaces pollution fields ────────────────────────

#[test]
fn f5_diagnostic_reports_clean_db_as_ok() {
    let db = fresh_db();
    db.register_pub_symbol("crates/foo/src/lib.rs", "Foo", "struct", "public")
        .expect("Ok");
    let diag = db.wiring_db_diagnostic().expect("diagnostic");
    assert_eq!(diag.total_rows, 1);
    assert_eq!(diag.producer_rows, 1);
    assert_eq!(diag.consumer_rows, 0);
    assert_eq!(diag.pub_producers, 1);
    assert_eq!(diag.distinct_pub_symbols, 1);
    assert_eq!(diag.kind_unknown_count, 0);
    assert_eq!(diag.non_rust_rows, 0);
}

#[test]
fn f5_diagnostic_flags_kind_unknown_after_orphan_consumer() {
    let db = fresh_db();
    // record_consumer with no prior producer triggers the COALESCE → 'unknown'
    // fallback in wiring.rs. This is the race-condition gotcha the audit
    // identified — F5 surfaces it instead of hiding it.
    db.record_consumer(
        "crates/foo/src/lib.rs",
        "orphan_method",
        "crates/bar/src/main.rs",
        Some(1),
    )
    .expect("Ok");
    let diag = db.wiring_db_diagnostic().expect("diagnostic");
    assert!(
        diag.kind_unknown_count >= 1,
        "schema-race row must surface as kind_unknown_count >= 1, got {}",
        diag.kind_unknown_count
    );
}

#[test]
fn f5_diagnostic_flags_non_rust_legacy_rows() {
    let db = fresh_db();
    // Inject a legacy non-Rust row bypassing the gate.
    db.conn_ref()
        .execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, contract_source)
             VALUES ('scripts/run.sh', 'main', 'function', 'public', 'legacy')",
            [],
        )
        .expect("raw insert");
    let diag = db.wiring_db_diagnostic().expect("diagnostic");
    assert!(
        diag.non_rust_rows >= 1,
        "non_rust_rows must flag pollution: {}",
        diag.non_rust_rows
    );
}

// ── F9: method-dispatch consumer recording via AST walk ──────────────────────

#[test]
fn f9_find_producer_modules_for_methods_matches_callable_kinds() {
    let db = fresh_db();
    // Register pub method and pub function with the SAME name in different files.
    db.register_pub_symbol("crates/a/src/lib.rs", "validate", "method", "public")
        .expect("Ok");
    db.register_pub_symbol("crates/b/src/lib.rs", "validate", "function", "public")
        .expect("Ok");
    // A struct named the same should NOT match — find_producer_modules_for_methods
    // restricts to method/function/async_function kinds.
    db.register_pub_symbol("crates/c/src/lib.rs", "validate", "struct", "public")
        .expect("Ok");

    let names = vec!["validate".to_string(), "nonexistent".to_string()];
    let matches = db
        .find_producer_modules_for_methods(&names, 10)
        .expect("query");
    let mut files: Vec<String> = matches.iter().map(|(m, _)| m.clone()).collect();
    files.sort();
    assert_eq!(
        files,
        vec![
            "crates/a/src/lib.rs".to_string(),
            "crates/b/src/lib.rs".to_string()
        ],
        "struct symbol must not match callable lookup"
    );
}

#[test]
fn f9_cap_per_name_bounds_fanout() {
    let db = fresh_db();
    // 6 distinct producers of method `clone` — common-name fanout scenario.
    for i in 0..6 {
        let path = format!("crates/x{}/src/lib.rs", i);
        db.register_pub_symbol(&path, "clone", "method", "public")
            .expect("Ok");
    }
    let names = vec!["clone".to_string()];
    let capped = db
        .find_producer_modules_for_methods(&names, 3)
        .expect("query");
    assert_eq!(
        capped.len(),
        3,
        "cap_per_name=3 must bound results regardless of producer count"
    );
}

#[test]
fn f9_method_call_consumer_resolves_orphan_end_to_end() {
    let db = fresh_db();
    // Producer: pub fn do_thing in crates/a — would be orphan without F9.
    db.register_pub_symbol("crates/a/src/lib.rs", "do_thing", "method", "public")
        .expect("Ok");
    assert_eq!(db.orphan_symbols().expect("query").len(), 1);

    // F9 path: caller file walks AST and finds method call `obj.do_thing()`.
    // Production code looks up producers via find_producer_modules_for_methods
    // then records each as consumer. We simulate that lookup+record below.
    let names = vec!["do_thing".to_string()];
    let producers = db
        .find_producer_modules_for_methods(&names, 4)
        .expect("query");
    for (module_file, symbol_name) in &producers {
        db.record_consumer(module_file, symbol_name, "crates/b/src/main.rs", None)
            .expect("Ok");
    }
    // After wiring: producer must be removed from orphan list.
    assert!(
        db.orphan_symbols().expect("query").is_empty(),
        "F9 must resolve method orphan via name-only match"
    );
}

#[test]
fn f9_empty_names_returns_empty() {
    let db = fresh_db();
    db.register_pub_symbol("crates/a/src/lib.rs", "foo", "method", "public")
        .expect("Ok");
    let matches = db.find_producer_modules_for_methods(&[], 4).expect("query");
    assert!(matches.is_empty(), "empty input must return empty output");
}

// ── F7: pub mod declarations excluded from orphan_symbols ────────────────────

#[test]
fn f7_pub_mod_declaration_never_appears_in_orphan_list() {
    let db = fresh_db();
    db.register_pub_symbol("crates/foo/src/lib.rs", "submod", "module", "public")
        .expect("Ok");
    // A real pub fn alongside the pub mod — the fn must surface, the mod must not.
    db.register_pub_symbol("crates/foo/src/lib.rs", "actual_api", "function", "public")
        .expect("Ok");

    let orphans = db.orphan_symbols().expect("query");
    assert!(
        orphans.iter().all(|e| e.symbol_kind != "module"),
        "no symbol_kind='module' may appear in orphans, got: {:?}",
        orphans
            .iter()
            .map(|e| (&e.symbol_kind, &e.symbol_name))
            .collect::<Vec<_>>()
    );
    assert!(
        orphans.iter().any(|e| e.symbol_name == "actual_api"),
        "pub fn in same file must still appear (F7 is narrow): {:?}",
        orphans
    );
}

#[test]
fn f7_orphan_symbols_for_module_also_excludes_pub_mod() {
    let db = fresh_db();
    db.register_pub_symbol("crates/x/src/lib.rs", "nested", "module", "public")
        .expect("Ok");
    db.register_pub_symbol("crates/x/src/lib.rs", "Item", "struct", "public")
        .expect("Ok");
    let only = db
        .orphan_symbols_for_module("crates/x/src/lib.rs")
        .expect("query");
    assert_eq!(
        only.len(),
        1,
        "module must be filtered, struct must remain: {:?}",
        only
    );
    assert_eq!(only[0].symbol_name, "Item");
}

// ── F8: derive-trait method names excluded from orphan_symbols ───────────────

#[test]
fn f8_derive_method_names_excluded_from_orphans() {
    let db = fresh_db();
    // Simulate the ~8 names emitted by common derives that the AST walker
    // (F9) cannot see — they are invoked by the compiler/stdlib internally.
    let derives = [
        "fmt",
        "hash",
        "eq",
        "partial_cmp",
        "cmp",
        "drop",
        "clone",
        "default",
    ];
    for name in &derives {
        db.register_pub_symbol("crates/x/src/lib.rs", name, "method", "public")
            .expect("Ok");
    }
    // A non-derive method with a normal name must still surface.
    db.register_pub_symbol("crates/x/src/lib.rs", "compute", "function", "public")
        .expect("Ok");

    let orphans = db.orphan_symbols().expect("query");
    for d in &derives {
        assert!(
            !orphans.iter().any(|e| e.symbol_name == *d),
            "derive method `{}` must be filtered out of orphan list",
            d
        );
    }
    assert!(
        orphans.iter().any(|e| e.symbol_name == "compute"),
        "non-derive function must still appear"
    );
}

#[test]
fn f8_partial_match_does_not_overfilter() {
    // F8 uses NOT IN (...) — exact match only. `clone_node`, `default_value`,
    // `hashable` must NOT be filtered because they only START WITH the
    // protected names. Locks in that we don't switch to substring match.
    let db = fresh_db();
    let lookalikes = ["clone_node", "default_value", "hashable", "fmt_helper"];
    for name in &lookalikes {
        db.register_pub_symbol("crates/x/src/lib.rs", name, "method", "public")
            .expect("Ok");
    }
    let orphans = db.orphan_symbols().expect("query");
    for name in &lookalikes {
        assert!(
            orphans.iter().any(|e| e.symbol_name == *name),
            "look-alike `{}` must NOT be filtered (F8 is exact match)",
            name
        );
    }
}

// ── F2 migration: idempotent absolute → relative rewrite ──────────────────────

#[test]
fn f2_migrate_canonicalize_paths_rewrites_legacy_absolute_rows() {
    let db = fresh_db();
    // Insert legacy row with absolute path — simulating a wiring_map populated
    // by an old daemon build that did not canonicalize.
    db.conn_ref()
        .execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, contract_source)
             VALUES ('/home/gabrielgadea/.claude/rust/crates/foo/src/lib.rs', 'Bar', 'struct', 'public', 'legacy')",
            [],
        )
        .expect("raw insert");
    let updated = db.migrate_canonicalize_paths().expect("migration");
    assert!(updated > 0, "migration must touch at least one row");
    // Idempotent: second run does nothing.
    let updated_again = db.migrate_canonicalize_paths().expect("idempotent");
    assert_eq!(updated_again, 0, "second run is a no-op");
    // The canonical row is now present.
    let orphans = db.orphan_symbols().expect("query");
    assert!(
        orphans
            .iter()
            .any(|e| e.module_file == "crates/foo/src/lib.rs" && e.symbol_name == "Bar"),
        "post-migration row must be queryable under canonical path: {:?}",
        orphans
    );
}
