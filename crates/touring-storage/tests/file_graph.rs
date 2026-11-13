//! C8 — the wiring read as a file graph.
//!
//! Gated like the code it tests: `FileKnowledgeDB` exists only with the `knowledge`
//! feature (`cargo test -p touring-storage --features knowledge --test file_graph`;
//! building `touring-code`, which asks for it, turns it on too).
//!
//! `wiring_map` already holds the dependency graph production keeps current and
//! resolved. These tests write it through the real writers (`register_pub_symbol`,
//! `record_consumer`, `record_coedit`) and read it back as the three questions a graph
//! consumer asks: which file depends on which, which files the wiring spoke for, and
//! which files were edited together.

#![cfg(feature = "knowledge")]

use tempfile::TempDir;
use touring_storage::knowledge::FileKnowledgeDB;
use touring_storage::knowledge_wiring::WiringOrigin;

const FOO: &str = "crates/a/src/foo.rs";
const MAIN: &str = "crates/a/src/main.rs";
const LONELY: &str = "crates/a/src/lonely.rs";

fn db() -> (TempDir, FileKnowledgeDB) {
    let tmp = TempDir::new().expect("tempdir");
    let db = FileKnowledgeDB::new(&tmp.path().join("knowledge.db")).expect("open db");
    (tmp, db)
}

/// foo.rs defines two symbols; main.rs consumes both. lonely.rs defines one nobody uses.
fn wire(db: &FileKnowledgeDB) {
    db.register_pub_symbol(FOO, "Foo", "struct", "public")
        .unwrap();
    db.register_pub_symbol(FOO, "bar", "fn", "public").unwrap();
    db.register_pub_symbol(LONELY, "Unused", "struct", "public")
        .unwrap();
    db.record_consumer(FOO, "Foo", MAIN, Some(1)).unwrap();
    db.record_consumer(FOO, "bar", MAIN, Some(2)).unwrap();
}

#[test]
fn file_dependency_edges_are_distinct_files_only() {
    let (_tmp, db) = db();
    wire(&db);
    // A file that consumes itself, and a call site that is not a file at all.
    db.record_consumer(FOO, "Foo", FOO, Some(3)).unwrap();
    db.record_consumer(FOO, "Foo", "touring-daemon://dispatch", None)
        .unwrap();

    assert_eq!(
        db.file_dependency_edges().unwrap(),
        [(MAIN.to_string(), FOO.to_string())],
        "two symbols consumed make ONE edge; the self edge and the virtual consumer are not edges"
    );
}

/// The wiring a real project leaves behind, with every row the two reads must treat the
/// same way: consumers recorded out of path order, one consumer on two symbols (a
/// duplicate edge), a file consuming itself, and a virtual call site.
fn wire_with_every_odd_row(db: &FileKnowledgeDB) {
    wire(db);
    db.register_pub_symbol(MAIN, "run", "fn", "public").unwrap();
    db.record_consumer(FOO, "Foo", "crates/a/src/zeta.rs", Some(1))
        .unwrap();
    db.record_consumer(FOO, "bar", "crates/a/src/zeta.rs", Some(2))
        .unwrap();
    db.record_consumer(FOO, "Foo", "crates/a/src/alpha.rs", Some(1))
        .unwrap();
    db.record_consumer(MAIN, "run", "crates/a/src/alpha.rs", Some(4))
        .unwrap();
    db.record_consumer(FOO, "Foo", FOO, Some(3)).unwrap();
    db.record_consumer(MAIN, "run", MAIN, Some(5)).unwrap();
    db.record_consumer(FOO, "Foo", "touring-daemon://dispatch", None)
        .unwrap();
    db.record_consumer(MAIN, "run", "touring-daemon://dispatch", None)
        .unwrap();
}

#[test]
fn file_importers_are_one_producers_column_of_file_dependency_edges() {
    let (_tmp, db) = db();
    wire_with_every_odd_row(&db);
    let edges = db.file_dependency_edges().unwrap();

    // Every file the wiring spoke for, plus one it never saw: the two reads must agree on
    // all of them, so a rule written into one query and not the other fails here.
    let mut files = db.wired_files().unwrap();
    files.push("crates/a/src/never_seen.rs".to_string());
    for file in &files {
        let column: Vec<String> = edges
            .iter()
            .filter(|(_, producer)| producer == file)
            .map(|(consumer, _)| consumer.clone())
            .collect();
        assert_eq!(
            db.file_importers(file).unwrap(),
            column,
            "`file_importers({file})` must be that producer's column of `file_dependency_edges`: \
             the two share one filter (`file_edge_filter!` in knowledge_wiring.rs); a rule added \
             to only one query breaks `touring ast blast` against the graph the index loads"
        );
    }

    assert_eq!(
        db.file_importers(FOO).unwrap(),
        ["crates/a/src/alpha.rs", MAIN, "crates/a/src/zeta.rs"].map(str::to_string),
        "each consumer once, in path order; not FOO itself, not the virtual call site"
    );
    assert_eq!(
        db.file_importers(MAIN).unwrap(),
        ["crates/a/src/alpha.rs".to_string()],
        "MAIN's own consumption of `run` is a self edge, not an importer"
    );
    assert!(
        db.file_importers(LONELY).unwrap().is_empty()
            && db.file_importers("nope.rs").unwrap().is_empty(),
        "a producer nobody consumes and a file never seen both have no importers"
    );
}

/// F13 (13/11/2026): a file edge needs IMPORT evidence, never a name guess alone.
///
/// `ALPHA` consumes `FOO` through a resolved `use`; `MAIN` only calls a bare `bar` that
/// the by-name fallback matched to `FOO` (`ast_inferred`); `ZETA` has both. Measured on
/// the live wiring: 101.730 of the 114.097 consumer rows were guesses, and a pure guess
/// made `domain_circuit.rs` "imported" by 1.236 files, 1.170 of them through the name `map`.
#[test]
fn a_file_edge_requires_import_evidence_not_a_name_guess() {
    let (_tmp, db) = db();
    wire(&db);
    let alpha = "crates/a/src/alpha.rs";
    let zeta = "crates/a/src/zeta.rs";
    // MAIN (from `wire`) holds resolved rows to FOO; a second file only guesses.
    let guesser = "crates/a/src/guesser.rs";
    db.record_consumer_with_origin(FOO, "bar", guesser, Some(7), WiringOrigin::AstInferred)
        .unwrap();
    db.record_consumer(FOO, "Foo", alpha, Some(1)).unwrap();
    db.record_consumer_with_origin(FOO, "bar", zeta, None, WiringOrigin::AstInferred)
        .unwrap();
    db.record_consumer(FOO, "Foo", zeta, Some(3)).unwrap();

    assert_eq!(
        db.file_dependency_edges().unwrap(),
        [(alpha, FOO), (MAIN, FOO), (zeta, FOO)].map(|(c, p)| (c.to_string(), p.to_string())),
        "a resolved row makes the edge (ALPHA, MAIN, ZETA); a pair with only an inferred row \
         (GUESSER) is a guess, not a dependency"
    );
    assert_eq!(
        db.file_importers(FOO).unwrap(),
        [alpha, MAIN, zeta].map(str::to_string),
        "the importers of FOO are the files with import evidence; GUESSER only matched a name"
    );
    assert_eq!(
        db.file_dependencies(guesser).unwrap(),
        Vec::<String>::new(),
        "the pointwise read of the consumer side answers like the whole-table read"
    );
    assert_eq!(
        db.file_dependencies(zeta).unwrap(),
        [FOO.to_string()],
        "one resolved row among inferred ones is enough: the evidence is per pair, not per row"
    );
    assert!(
        db.wired_files().unwrap().iter().any(|f| f == guesser),
        "a guesser is still a file the wiring spoke for (coverage is about what was looked at); \
         only its EDGE needs the evidence"
    );
}

/// A declared producer row or a compiler-resolved one is evidence too: the rule keeps
/// everything that is not a name guess, so a future `scip_resolved` consumer row (H2) or a
/// legacy `ast_read` row still draws its edge.
#[test]
fn every_origin_but_the_name_guess_draws_the_edge() {
    let (_tmp, db) = db();
    db.register_pub_symbol(FOO, "Foo", "struct", "public")
        .unwrap();
    for (file, origin) in [
        ("crates/a/src/c_resolved.rs", WiringOrigin::AstResolved),
        ("crates/a/src/c_scip.rs", WiringOrigin::ScipResolved),
        ("crates/a/src/c_text.rs", WiringOrigin::TextMatched),
        ("crates/a/src/c_guess.rs", WiringOrigin::AstInferred),
    ] {
        db.record_consumer_with_origin(FOO, "Foo", file, Some(1), origin)
            .unwrap();
    }
    db.conn_ref()
        .execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, consumer_file, contract_source) \
             VALUES (?1, 'Foo', 'struct', 'public', 'crates/a/src/c_legacy.rs', 'ast_read')",
            [FOO],
        )
        .unwrap();

    assert_eq!(
        db.file_importers(FOO).unwrap(),
        [
            "crates/a/src/c_legacy.rs",
            "crates/a/src/c_resolved.rs",
            "crates/a/src/c_scip.rs",
            "crates/a/src/c_text.rs",
        ]
        .map(str::to_string),
        "only the `ast_inferred` consumer is left out"
    );
}

#[test]
fn file_importers_read_the_canonical_key_of_the_producer() {
    let (_tmp, db) = db();
    wire(&db);
    assert_eq!(
        db.file_importers(&format!("./{FOO}")).unwrap(),
        [MAIN.to_string()],
        "`./x` and `x` are one key, as in every other wiring_map accessor"
    );
}

#[test]
fn file_dependencies_are_one_consumers_row_of_file_dependency_edges() {
    let (_tmp, db) = db();
    wire_with_every_odd_row(&db);
    let edges = db.file_dependency_edges().unwrap();

    let mut files = db.wired_files().unwrap();
    files.push("crates/a/src/never_seen.rs".to_string());
    for file in &files {
        let mut row: Vec<String> = edges
            .iter()
            .filter(|(consumer, _)| consumer == file)
            .map(|(_, producer)| producer.clone())
            .collect();
        row.sort();
        assert_eq!(
            db.file_dependencies(file).unwrap(),
            row,
            "`file_dependencies({file})` must be that consumer's row of `file_dependency_edges`: \
             one filter (`file_edge_filter!`), so a self edge or a virtual call site never \
             reaches the in-memory index through one door and not the other"
        );
    }
    assert_eq!(
        db.file_dependencies("crates/a/src/alpha.rs").unwrap(),
        [FOO, MAIN].map(str::to_string),
        "alpha consumes FOO and MAIN; each producer once, in path order"
    );
    assert_eq!(
        db.file_dependencies(FOO).unwrap(),
        Vec::<String>::new(),
        "FOO consumes only itself: a self edge is not a dependency"
    );
    assert_eq!(
        db.file_dependencies(&format!("./{}", "crates/a/src/alpha.rs"))
            .unwrap(),
        [FOO, MAIN].map(str::to_string),
        "`./x` and `x` are one key"
    );
}

#[test]
fn wired_files_lists_every_file_the_wiring_spoke_for() {
    let (_tmp, db) = db();
    wire(&db);
    db.record_consumer(FOO, "Foo", "touring-daemon://dispatch", None)
        .unwrap();

    assert_eq!(
        db.wired_files().unwrap(),
        [FOO, LONELY, MAIN].map(str::to_string),
        "producers consumed or not, and real consumers; sorted; no virtual consumer"
    );
}

#[test]
fn an_empty_wiring_has_no_edges_and_covers_nothing() {
    let (_tmp, db) = db();
    assert!(db.file_dependency_edges().unwrap().is_empty());
    assert!(db.wired_files().unwrap().is_empty());
    assert!(db.co_edit_pairs().unwrap().is_empty());
}

#[test]
fn co_edit_pairs_keep_both_directions_as_they_were_recorded() {
    let (_tmp, db) = db();
    db.record_coedit("a.rs", "b.rs").unwrap();
    db.record_coedit("a.rs", "b.rs").unwrap();
    db.record_coedit("b.rs", "a.rs").unwrap();
    db.record_coedit("c.rs", "d.rs").unwrap();

    assert_eq!(
        db.co_edit_pairs().unwrap(),
        [
            ("a.rs".to_string(), "b.rs".to_string(), 2),
            ("b.rs".to_string(), "a.rs".to_string(), 1),
            ("c.rs".to_string(), "d.rs".to_string(), 1),
        ],
        "the reader adds the two directions of a pair; the store does not pre-sum them"
    );
}
