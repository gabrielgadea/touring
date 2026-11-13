//! C3 — one write path for "indexed file".
//!
//! Indexing, reindexing and removing a file is ONE operation per verb, in memory
//! ([`SymbolIndex`]) and in SQLite ([`SymbolStore`]), and the key grammar (file key ×
//! module key) holds on both sides. Every test indexes real source and asks questions of
//! the result — none builds a row or a map entry by hand.

use serde_json::Value;
use tempfile::TempDir;
use touring_code::ast::graph::SymbolIndex;
use touring_code::ast::languages::Lang;
use touring_code::ast::store::SymbolStore;

/// The whole index as JSON, array order preserved (object keys sort by construction).
fn snapshot(index: &SymbolIndex) -> Value {
    serde_json::to_value(index).expect("SymbolIndex serialises")
}

/// [`snapshot`] with every array sorted — equality up to ordering inside a key.
fn canonical(index: &SymbolIndex) -> Value {
    fn sorted(v: Value) -> Value {
        match v {
            Value::Array(items) => {
                let mut items: Vec<Value> = items.into_iter().map(sorted).collect();
                items.sort_by_key(Value::to_string);
                Value::Array(items)
            }
            Value::Object(map) => {
                Value::Object(map.into_iter().map(|(k, v)| (k, sorted(v))).collect())
            }
            other => other,
        }
    }
    sorted(snapshot(index))
}

fn py(index: &mut SymbolIndex, path: &str, source: &str) {
    index
        .index_file(path, source, Lang::Python)
        .expect("python source indexes");
}

// ── memory: index / reindex / remove ─────────────────────────────────────────

#[test]
fn reindexing_a_file_replaces_what_it_contributed_before() {
    let mut index = SymbolIndex::new();
    py(&mut index, "a.py", "import os\n\ndef old():\n    pass\n");
    py(&mut index, "a.py", "import sys\n\ndef new():\n    pass\n");

    assert!(
        index.find_symbol("old").is_empty(),
        "the old definition must be gone"
    );
    assert_eq!(index.find_symbol("new").len(), 1);
    assert_eq!(index.file_to_symbols["a.py"], vec!["new".to_string()]);
    let targets: Vec<&str> = index.dependencies["a.py"]
        .iter()
        .map(|e| e.to.as_str())
        .collect();
    assert_eq!(targets, vec!["sys"], "only the new import may remain");
    assert!(
        !index.reverse_deps.contains_key("os"),
        "the old import must be withdrawn"
    );
    assert_eq!(index.reverse_deps["sys"], vec!["a.py".to_string()]);

    // Reindexing unchanged source is a no-op, not a doubling.
    let before = snapshot(&index);
    py(&mut index, "a.py", "import sys\n\ndef new():\n    pass\n");
    assert_eq!(snapshot(&index), before);
}

#[test]
fn indexing_then_removing_a_file_restores_the_index() {
    let mut index = SymbolIndex::new();
    py(
        &mut index,
        "keep.py",
        "import os\n\ndef keep():\n    pass\n",
    );
    let before = snapshot(&index);

    py(
        &mut index,
        "x.py",
        "import os\nimport sys\n\ndef x():\n    pass\n",
    );
    index.remove_file("x.py");

    assert_eq!(
        snapshot(&index),
        before,
        "removing the only file indexed since must leave no trace (no empty keys either)"
    );
}

#[test]
fn removing_a_file_keeps_the_importers_edges_while_another_file_shares_its_module_key() {
    let mut index = SymbolIndex::new();
    py(&mut index, "a/utils.py", "def helper():\n    pass\n");
    py(&mut index, "b/utils.py", "def other():\n    pass\n");
    py(&mut index, "c.py", "import utils\n");

    index.remove_file("a/utils.py");
    assert_eq!(
        index.reverse_deps.get("utils"),
        Some(&vec!["c.py".to_string()]),
        "b/utils.py still answers to the module `utils`: c.py keeps importing it"
    );
    assert!(index.dependencies["c.py"].iter().any(|e| e.to == "utils"));

    // The contract the e2e suite already pins: with no file left behind the module key,
    // its importers' edges go — and a file left with no edge has no `dependencies` key,
    // the same shape `index_file` gives a file that imports nothing.
    index.remove_file("b/utils.py");
    assert!(!index.reverse_deps.contains_key("utils"));
    assert!(
        !index.dependencies.contains_key("c.py"),
        "c.py lost its only edge: the key goes with it, no empty list is left behind"
    );
}

#[test]
fn a_homonymous_stem_credits_nobody() {
    // `import utils` names a module, and two indexed files answer to it: crediting both
    // would blame a file for an import it never received. Precision first — no answer.
    let mut index = SymbolIndex::new();
    py(&mut index, "a/utils.py", "def helper():\n    pass\n");
    py(&mut index, "b/utils.py", "def other():\n    pass\n");
    py(&mut index, "c.py", "import utils\n");

    assert!(index.importers_of("a/utils.py").is_empty());
    assert!(index.importers_of("b/utils.py").is_empty());

    // Once one of them is gone the stem is unambiguous again.
    index.remove_file("b/utils.py");
    assert_eq!(index.importers_of("a/utils.py"), ["c.py".to_string()]);
}

#[test]
fn reindexing_a_module_keeps_the_edges_of_the_files_that_import_it() {
    let mut index = SymbolIndex::new();
    py(&mut index, "utils.py", "def helper():\n    pass\n");
    py(&mut index, "main.py", "import utils\n");
    let importers_before = index.reverse_deps.clone();

    // Editing utils.py is not removing it: nobody stops importing it.
    py(
        &mut index,
        "utils.py",
        "def helper():\n    pass\n\ndef extra():\n    pass\n",
    );

    assert_eq!(index.reverse_deps, importers_before);
    assert_eq!(index.importers_of("utils.py"), ["main.py".to_string()]);
    assert!(
        index.dependencies["main.py"]
            .iter()
            .any(|e| e.to == "utils")
    );
}

#[test]
fn importers_of_names_the_files_that_import_a_file_by_its_module_key() {
    let mut index = SymbolIndex::new();
    py(&mut index, "pkg/a.py", "def helper():\n    pass\n");
    py(&mut index, "b.py", "from a import helper\n");
    py(&mut index, "c.py", "import a\nimport sys\n");

    let mut importers: Vec<&str> = index
        .importers_of("pkg/a.py")
        .iter()
        .map(String::as_str)
        .collect();
    importers.sort_unstable();
    assert_eq!(importers, ["b.py", "c.py"]);
    assert!(index.importers_of("pkg/nobody.py").is_empty());
}

#[test]
fn index_files_builds_the_same_index_as_index_file_one_by_one() {
    let files: Vec<(&str, &str, Lang)> = vec![
        (
            "pkg/a.py",
            "import os\n\ndef alpha():\n    pass\n",
            Lang::Python,
        ),
        (
            "pkg/b.py",
            "from a import alpha\n\ndef beta():\n    pass\n",
            Lang::Python,
        ),
        (
            "c.py",
            "import os\nimport sys\n\nclass Gamma:\n    pass\n",
            Lang::Python,
        ),
    ];

    let mut one_by_one = SymbolIndex::new();
    for (path, source, lang) in &files {
        one_by_one.index_file(path, source, *lang).unwrap();
    }
    let mut batched = SymbolIndex::new();
    batched.index_files(&files).unwrap();
    assert_eq!(canonical(&batched), canonical(&one_by_one));

    // Batching the same files again replaces, it does not accumulate.
    batched.index_files(&files).unwrap();
    assert_eq!(canonical(&batched), canonical(&one_by_one));
}

// ── SQLite: the same verbs, the same grammar ────────────────────────────────

fn store_in(tmp: &TempDir) -> (std::path::PathBuf, SymbolStore) {
    let path = tmp.path().join("symbols.db");
    let store = SymbolStore::new(&path).expect("store opens");
    (path, store)
}

#[test]
fn persisted_extraction_answers_who_imports_a_file() {
    let mut index = SymbolIndex::new();
    py(&mut index, "a.py", "def helper():\n    pass\n");
    py(&mut index, "b.py", "from a import helper\n");
    let tmp = TempDir::new().unwrap();
    let (_, store) = store_in(&tmp);
    index.persist_to(&store).unwrap();

    assert_eq!(
        store.get_reverse_deps("a.py").unwrap(),
        vec!["b.py".to_string()]
    );
}

#[test]
fn removing_a_file_from_the_store_leaves_no_orphan_dependency_symbols() {
    let mut index = SymbolIndex::new();
    py(&mut index, "a.py", "from lib import Shared\n");
    py(
        &mut index,
        "b.py",
        "from lib import Shared\nfrom util import tool\n",
    );
    let tmp = TempDir::new().unwrap();
    let (path, store) = store_in(&tmp);
    index.persist_to(&store).unwrap();
    store.remove_file("b.py").unwrap();

    let conn = rusqlite::Connection::open(&path).unwrap();
    let orphans: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM dependency_symbols
             WHERE dep_id NOT IN (SELECT id FROM dependencies)",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(
        orphans, 0,
        "dependency_symbols rows outlived their dependency"
    );

    // A later dependency may reuse the freed rowid; it must not inherit stale symbols.
    // `Shared` is still imported by a.py, so exactly one edge answers to it — a second
    // one would be the new edge wearing the dead edge's symbols.
    let mut later = SymbolIndex::new();
    py(&mut later, "d.py", "from other import Fresh\n");
    later.persist_to(&store).unwrap();
    assert!(store.find_deps_by_symbol("tool").unwrap().is_empty());
    assert_eq!(store.find_deps_by_symbol("Fresh").unwrap().len(), 1);
    let shared = store.find_deps_by_symbol("Shared").unwrap();
    assert_eq!(
        shared.iter().map(|e| e.from.as_str()).collect::<Vec<_>>(),
        ["a.py"],
        "d.py must not inherit the symbols of the dependency that held its rowid"
    );
}

#[test]
fn a_failed_replace_leaves_the_file_as_it_was() {
    let tmp = TempDir::new().unwrap();
    let (path, store) = store_in(&tmp);
    let mut index = SymbolIndex::new();
    py(
        &mut index,
        "t.py",
        "from lib import Kept\n\ndef kept():\n    pass\n",
    );
    index.persist_to(&store).unwrap();
    let replacement = index
        .get_file_symbols("t.py")
        .into_iter()
        .cloned()
        .collect::<Vec<_>>();
    assert_eq!(replacement.len(), 1);
    assert_eq!(store.get_dependencies("t.py").unwrap().len(), 1);

    // Make the LAST step of the replace (writing the new symbols) fail, so every
    // delete before it has already happened — whatever order the deletes run in.
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER refuse_symbols BEFORE INSERT ON symbols
             BEGIN SELECT RAISE(ABORT, 'refused'); END;",
        )
        .unwrap();
    assert!(store.replace_file_symbols("t.py", &replacement).is_err());

    assert_eq!(
        store.find_symbols_in_file("t.py").unwrap().len(),
        1,
        "the replace is atomic: a failure must roll the deletes back"
    );
    assert_eq!(store.get_dependencies("t.py").unwrap().len(), 1);
    assert_eq!(store.find_deps_by_symbol("Kept").unwrap().len(), 1);
}

#[test]
fn a_failed_persist_does_not_leave_half_an_index_behind() {
    let tmp = TempDir::new().unwrap();
    let (path, store) = store_in(&tmp);
    rusqlite::Connection::open(&path)
        .unwrap()
        .execute_batch("DROP TABLE dependency_symbols;")
        .unwrap();

    let mut index = SymbolIndex::new();
    py(
        &mut index,
        "a.py",
        "from lib import Shared\n\ndef f():\n    pass\n",
    );
    assert!(index.persist_to(&store).is_err());
    assert_eq!(
        store.stats().unwrap().symbol_count,
        0,
        "persist is one transaction: a failure must roll every row back"
    );
}

#[test]
fn persist_then_load_round_trips_an_extracted_index() {
    let mut index = SymbolIndex::new();
    py(
        &mut index,
        "pkg/a.py",
        "import os\n\ndef alpha():\n    pass\n",
    );
    py(
        &mut index,
        "pkg/b.py",
        "from a import alpha\n\nclass Beta:\n    pass\n",
    );
    py(&mut index, "c.py", "import sys\n");
    let tmp = TempDir::new().unwrap();
    let (_, store) = store_in(&tmp);
    index.persist_to(&store).unwrap();

    let mut restored = SymbolIndex::new();
    store.load_into_index(&mut restored).unwrap();
    assert_eq!(canonical(&restored), canonical(&index));
}

/// Parity where every stem has ONE owner. A homonymous stem is the declared exception,
/// pinned by `memory_and_sqlite_part_ways_on_a_homonymous_stem` right below.
#[test]
fn memory_and_sqlite_answer_who_imports_a_file_the_same_way_when_every_stem_has_one_owner() {
    let mut index = SymbolIndex::new();
    py(&mut index, "pkg/a.py", "def alpha():\n    pass\n");
    py(
        &mut index,
        "pkg/b.py",
        "from a import alpha\n\ndef beta():\n    pass\n",
    );
    py(&mut index, "c.py", "import a\nimport b\n");
    py(&mut index, "d.py", "import sys\n");
    let tmp = TempDir::new().unwrap();
    let (_, store) = store_in(&tmp);
    index.persist_to(&store).unwrap();

    for file in ["pkg/a.py", "pkg/b.py", "c.py", "d.py", "nobody.py"] {
        let mut in_memory: Vec<String> = index.importers_of(file).to_vec();
        in_memory.sort();
        in_memory.dedup();
        let mut in_sqlite = store.get_reverse_deps(file).unwrap();
        in_sqlite.sort();
        assert_eq!(in_sqlite, in_memory, "one key grammar, two stores: {file}");
    }
}

/// The parity above holds only where every stem has one owner. With a homonym the two
/// stores part ways ON PURPOSE, and this pins the split so no doc can drift from it:
/// memory credits nobody (it will not blame a file for an import it never received),
/// while the SQL has no notion of "indexed files" and credits the importers of the stem
/// to both. Whoever closes the gap changes this test, the doc of `get_reverse_deps`
/// and the glossary together.
#[test]
fn memory_and_sqlite_part_ways_on_a_homonymous_stem() {
    let mut index = SymbolIndex::new();
    py(&mut index, "a/utils.py", "def helper():\n    pass\n");
    py(&mut index, "b/utils.py", "def other():\n    pass\n");
    py(&mut index, "c.py", "import utils\n");
    let tmp = TempDir::new().unwrap();
    let (_, store) = store_in(&tmp);
    index.persist_to(&store).unwrap();

    for homonym in ["a/utils.py", "b/utils.py"] {
        assert!(
            index.importers_of(homonym).is_empty(),
            "memory credits nobody: {homonym}"
        );
        assert_eq!(
            store.get_reverse_deps(homonym).unwrap(),
            ["c.py".to_string()],
            "SQLite credits the importers of the stem to both: {homonym}"
        );
    }
}

#[test]
fn replacing_a_file_in_the_store_swaps_its_symbols_and_drops_its_stale_edge_symbols() {
    let mut index = SymbolIndex::new();
    py(
        &mut index,
        "t.py",
        "from lib import Old\n\ndef old():\n    pass\n",
    );
    let tmp = TempDir::new().unwrap();
    let (_, store) = store_in(&tmp);
    index.persist_to(&store).unwrap();
    assert_eq!(store.find_deps_by_symbol("Old").unwrap().len(), 1);

    let mut edited = SymbolIndex::new();
    py(&mut edited, "t.py", "def new():\n    pass\n");
    let symbols = edited
        .get_file_symbols("t.py")
        .into_iter()
        .cloned()
        .collect::<Vec<_>>();
    store.replace_file_symbols("t.py", &symbols).unwrap();

    let names: Vec<String> = store
        .find_symbols_in_file("t.py")
        .unwrap()
        .into_iter()
        .map(|s| s.symbol_name)
        .collect();
    assert_eq!(names, ["new"]);
    assert!(
        store.find_deps_by_symbol("Old").unwrap().is_empty(),
        "the replaced file's old import must not survive through dependency_symbols"
    );
}
