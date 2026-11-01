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

/// A database at the canonical `<root>/.claude/touring/knowledge.db` layout,
/// returned together with the TempDir that owns it and the root itself.
///
/// The F2 tests need a database whose root can be DERIVED, because that is
/// where the canonicalizer now reads it from (2026-08-19). Their history is the
/// argument for it: they first hardcoded `/home/gabrielgadea/.claude/rust/`,
/// which the 2026-07-24 relocation froze; then they read
/// `TOURING_WORKSPACE_ROOT` — the same variable production read, so both sides
/// agreed while both were wrong, and the test could not have caught a daemon
/// canonicalizing one project's paths against another's root. Owning the root
/// makes the assertion hermetic: it depends on nothing outside the test.
fn rooted_db() -> (tempfile::TempDir, FileKnowledgeDB, String) {
    let tmp = tempfile::tempdir().expect("tempdir");
    let db_dir = tmp.path().join(".claude").join("touring");
    std::fs::create_dir_all(&db_dir).expect("mkdir .claude/touring");
    let db = FileKnowledgeDB::new(&db_dir.join("knowledge.db")).expect("DB opens");
    let root = format!("{}/", tmp.path().to_string_lossy());
    assert_eq!(
        db.workspace_root(),
        Some(root.as_str()),
        "the root must be derived from the DB's own location"
    );
    (tmp, db, root)
}

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
    let (_tmp, db, root) = rooted_db();
    db.register_pub_symbol(
        &format!("{root}crates/foo/src/lib.rs"),
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
    let (_tmp, db, root) = rooted_db();
    // Producer registered with absolute path
    db.register_pub_symbol(
        &format!("{root}crates/foo/src/lib.rs"),
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
    assert_eq!(diag.non_wireable_rows, 0);
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
fn f5_diagnostic_flags_non_wireable_legacy_rows() {
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
        diag.non_wireable_rows >= 1,
        "non_wireable_rows must flag pollution: {}",
        diag.non_wireable_rows
    );
}

// ── 2026-08-19: "is it Rust?" was the wrong question ─────────────────────────
//
// The predicate that decided `warning` asked whether `module_file` ends in
// `.rs`. Measured across four projects, that errs in BOTH directions at once,
// and each direction gets a test below. The replacement asks the writer's own
// question — "would any read admit this file?" — via `is_wireable_source`.

/// Direction 1 — FALSE NEGATIVE, the one that hid in the flagship project.
///
/// `benches/src/*.rs` ends in `.rs`, so the old predicate called `touring`
/// itself clean (102 such rows on 2026-08-19). But the write gate REJECTS
/// `benches/`, and the read filter is extension-only, so those rows do reach
/// the orphan queries and inflate the count — exactly the unreliability the
/// warning exists to signal, invisible to the warning.
#[test]
fn a_rust_file_the_gate_rejects_is_still_pollution() {
    let db = fresh_db();
    db.conn_ref()
        .execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, contract_source)
             VALUES ('benches/src/throughput.rs', 'bench_main', 'function', 'public', 'legacy')",
            [],
        )
        .expect("raw insert");
    let diag = db.wiring_db_diagnostic().expect("diagnostic");
    assert_eq!(
        diag.non_wireable_rows, 1,
        "a bench file is not wiring under ANY mode — ending in .rs must not absolve it"
    );
    assert_eq!(
        diag.total_rows, 0,
        "and it must stay out of the census, because no query reads it"
    );
}

/// Direction 2 — FALSE POSITIVE, the one that punished polyglot projects.
///
/// A first-party Python source in a project with the opt-in ON is readable
/// wiring. It must land in the census and judge nothing. With the opt-in OFF
/// the same row is `unread` — a mode setting, still not a defect.
#[test]
fn a_first_party_python_source_is_wiring_not_pollution() {
    let db = fresh_db();
    db.conn_ref()
        .execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, contract_source)
             VALUES ('packages/core/models.py', 'User', 'class', 'public', 'legacy')",
            [],
        )
        .expect("raw insert");
    let diag = db.wiring_db_diagnostic().expect("diagnostic");
    assert_eq!(
        diag.non_wireable_rows, 0,
        "a .py source file is admissible under the maximum vocabulary — it may never judge"
    );
    // `fresh_db` carries no polyglot opt-in, so the row is filtered from every
    // query. That is the honest report: unread, not polluted.
    assert_eq!(
        diag.unread_rows, 1,
        "with the opt-in off the row is unread — the answer to 'why is my wiring thin?'"
    );
    assert_eq!(
        diag.total_rows, 0,
        "and it is correctly absent from the census"
    );
}

/// The two counters partition the rows: every stored row is either judged,
/// filtered, or counted — never two of those, never none.
#[test]
fn every_row_is_counted_exactly_once() {
    let db = fresh_db();
    for (module, symbol) in [
        ("crates/foo/src/lib.rs", "Kept"),                  // census
        ("benches/src/b.rs", "Bench"),                      // non_wireable
        ("apps/x/venv/lib/site-packages/d/m.py", "Vendor"), // non_wireable
        ("scripts/run.sh", "main"),                         // non_wireable
        ("packages/core/models.py", "User"),                // unread (opt-in off)
    ] {
        db.conn_ref()
            .execute(
                "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, contract_source)
                 VALUES (?1, ?2, 'function', 'public', 'legacy')",
                rusqlite::params![module, symbol],
            )
            .expect("raw insert");
    }
    let diag = db.wiring_db_diagnostic().expect("diagnostic");
    assert_eq!(
        diag.total_rows + diag.non_wireable_rows + diag.unread_rows,
        5,
        "census + judged + filtered must account for every stored row exactly once"
    );
    assert_eq!(diag.total_rows, 1, "only the Rust source is readable here");
    assert_eq!(
        diag.non_wireable_rows, 3,
        "bench, vendored venv and shell script"
    );
    assert_eq!(diag.unread_rows, 1, "the .py source, filtered by the mode");
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
        .find_producer_modules_for_methods(&names, 10, None)
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
        .find_producer_modules_for_methods(&names, 3, None)
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
        .find_producer_modules_for_methods(&names, 4, None)
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
    let matches = db
        .find_producer_modules_for_methods(&[], 4, None)
        .expect("query");
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
    let (_tmp, db, root) = rooted_db();
    // Insert legacy row with absolute path — simulating a wiring_map populated
    // by an old daemon build that did not canonicalize.
    db.conn_ref()
        .execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, contract_source)
             VALUES (?1, 'Bar', 'struct', 'public', 'legacy')",
            [format!("{root}crates/foo/src/lib.rs")],
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

// ── 2026-08-19: rows that belong to ANOTHER project ───────────────────────────

#[test]
fn migration_evicts_foreign_rows_and_keeps_local_ones() {
    let (_tmp, db, root) = rooted_db();
    // A local producer, recorded the way the indexer records it.
    db.register_pub_symbol("crates/foo/src/lib.rs", "Local", "struct", "public")
        .expect("local producer");
    // A row describing a file in a DIFFERENT project. This is not hypothetical:
    // `konverter`'s database held 7.893 of them, all pointing into `analise`,
    // written while one global daemon indexed every project into one graph.
    // They inflate the orphan count and fabricate cycles between modules that
    // never met.
    db.conn_ref()
        .execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, contract_source)
             VALUES ('/home/someone/projects/other/src/lib.rs', 'Foreign', 'struct', 'public', 'legacy')",
            [],
        )
        .expect("foreign insert");
    // And one whose PRODUCER is local but whose consumer lives abroad.
    db.conn_ref()
        .execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, consumer_file)
             VALUES ('crates/foo/src/lib.rs', 'Local', 'struct', 'public', '/home/someone/projects/other/src/main.rs')",
            [],
        )
        .expect("foreign consumer insert");

    let touched = db.migrate_canonicalize_paths().expect("migration");
    assert!(
        touched >= 2,
        "both foreign rows must be evicted, got {touched}"
    );

    let foreign: i64 = db
        .conn_ref()
        .query_row(
            "SELECT COUNT(*) FROM wiring_map WHERE module_file LIKE '/%' OR consumer_file LIKE '/%'",
            [],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(
        foreign, 0,
        "no absolute path may survive under a derived root"
    );

    let local: i64 = db
        .conn_ref()
        .query_row(
            "SELECT COUNT(*) FROM wiring_map WHERE module_file = 'crates/foo/src/lib.rs'",
            [],
            |r| r.get(0),
        )
        .expect("count");
    assert_eq!(local, 1, "the local producer must survive the eviction");

    // Idempotent — a second pass has nothing left to do.
    assert_eq!(db.migrate_canonicalize_paths().expect("again"), 0);
    let _ = root;
}

#[test]
fn a_db_without_a_derivable_root_never_deletes() {
    // `:memory:` has no location, so no root, so nothing can be judged foreign.
    // The migration must do NOTHING rather than fall back to a guess — the
    // guess used to be an environment variable pointing at another project,
    // which is what aimed a DELETE at the wrong data for two months.
    let db = fresh_db();
    db.conn_ref()
        .execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, contract_source)
             VALUES ('/home/someone/projects/other/src/lib.rs', 'Untouched', 'struct', 'public', 'legacy')",
            [],
        )
        .expect("insert");
    let before: i64 = db
        .conn_ref()
        .query_row("SELECT COUNT(*) FROM wiring_map", [], |r| r.get(0))
        .expect("count");
    if db.workspace_root().is_none() {
        assert_eq!(db.migrate_canonicalize_paths().expect("migration"), 0);
        let after: i64 = db
            .conn_ref()
            .query_row("SELECT COUNT(*) FROM wiring_map", [], |r| r.get(0))
            .expect("count");
        assert_eq!(before, after, "no row may be removed without a root");
    }
}
