//! C8 — the index answers "who imports this file" from resolved file edges.
//!
//! Extraction sees the text of an import (`crate::foo`, `utils`), never the file it
//! names; the wiring has already resolved that. These tests load edges the way
//! `load_index` does and ask the readers that used to ask a map keyed by module key.
//! Fixtures that need real data go through the real writers: extraction for
//! `index_file`, `register_pub_symbol` / `record_consumer` / `record_coedit` for the
//! wiring, `persist_to` for the symbol store.

use indexmap::IndexMap;
use tempfile::TempDir;
use touring_code::ast::graph::{
    EdgeCoverage, Importers, SymbolIndex, compute_enriched_blast_radius,
};
use touring_code::ast::languages::Lang;
use touring_code::ast::store::{SymbolStore, load_index, load_index_for};
use touring_storage::knowledge::FileKnowledgeDB;

fn s(text: &str) -> String {
    text.to_string()
}

fn edges(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs.iter().map(|(a, b)| (s(a), s(b))).collect()
}

fn files(names: &[&str]) -> Vec<String> {
    names.iter().map(|n| s(n)).collect()
}

fn index_of(source: &[(&str, &str, Lang)]) -> SymbolIndex {
    let mut index = SymbolIndex::new();
    for (path, text, lang) in source {
        index.index_file(path, text, *lang).expect("source indexes");
    }
    index
}

#[test]
fn a_rust_crate_path_is_answered_from_wiring_edges() {
    let mut index = index_of(&[
        (
            "src/a.rs",
            "use crate::foo;\n\npub fn run() {}\n",
            Lang::Rust,
        ),
        ("src/foo.rs", "pub fn foo() {}\n", Lang::Rust),
    ]);
    // Extraction names the module path, never the file: nothing is credited to foo.rs.
    assert!(index.importers_of("src/foo.rs").is_empty());

    index.load_file_edges(
        edges(&[("src/a.rs", "src/foo.rs")]),
        files(&["src/a.rs", "src/foo.rs"]),
    );

    assert_eq!(index.importers_of("src/foo.rs"), [s("src/a.rs")]);
    assert!(
        index
            .blast_radius("src/foo.rs")
            .affected_files
            .contains(&s("src/a.rs"))
    );
    assert!(
        index
            .reverse_deps
            .keys()
            .all(|key| !key.starts_with("crate")),
        "the raw `crate::foo` edge is superseded by the resolved one"
    );
}

/// Extraction only, no wiring: its edges name a module (`utils`), never the file. The
/// readers must still reach the file through its stem, hop after hop, because each hop
/// finds importers as FILE keys and has to look the next one up as a file.
fn stem_chain() -> SymbolIndex {
    index_of(&[
        ("pkg/utils.py", "def helper():\n    pass\n", Lang::Python),
        ("pkg/main.py", "import utils\n", Lang::Python),
        ("pkg/app.py", "import main\n", Lang::Python),
    ])
}

#[test]
fn blast_radius_follows_stem_edges_hop_by_hop() {
    let mut affected = stem_chain().blast_radius("pkg/utils.py").affected_files;
    affected.sort();
    assert_eq!(
        affected,
        ["pkg/app.py", "pkg/main.py", "pkg/utils.py"].map(s)
    );
}

#[test]
fn the_enriched_blast_radius_follows_a_stem_edge() {
    let enriched = compute_enriched_blast_radius(&stem_chain(), "pkg/utils.py", &IndexMap::new());
    assert_eq!(enriched.direct_dependents, [s("pkg/main.py")]);
    assert_eq!(enriched.transitive_dependents, [s("pkg/app.py")]);
}

#[test]
fn the_weighted_blast_radius_follows_stem_edges() {
    let mut reached: Vec<String> = stem_chain()
        .weighted_blast_radius("pkg/utils.py")
        .into_iter()
        .map(|(file, _cost)| file)
        .collect();
    reached.sort();
    assert_eq!(
        reached,
        ["pkg/app.py", "pkg/main.py", "pkg/utils.py"].map(s)
    );
}

#[test]
fn blast_radius_walks_file_edges_hop_by_hop() {
    let mut index = SymbolIndex::new();
    index.load_file_edges(
        edges(&[("b.rs", "a.rs"), ("c.rs", "b.rs"), ("d.rs", "c.rs")]),
        files(&["a.rs", "b.rs", "c.rs", "d.rs"]),
    );

    let all = index.blast_radius("a.rs");
    assert_eq!((all.file_count, all.max_distance), (4, 3));
    assert_eq!(index.blast_radius_with_depth("a.rs", 2).file_count, 3);
}

#[test]
fn replace_edges_of_swaps_the_outgoing_edges_and_prunes_what_empties() {
    let mut index = SymbolIndex::new();
    index.load_file_edges(
        edges(&[("b.rs", "a.rs"), ("b.rs", "c.rs")]),
        files(&["a.rs", "b.rs", "c.rs"]),
    );

    index.replace_edges_of("b.rs", &[s("d.rs")]);
    assert!(index.importers_of("a.rs").is_empty());
    assert!(index.importers_of("c.rs").is_empty());
    assert_eq!(index.importers_of("d.rs"), [s("b.rs")]);
    assert!(!index.reverse_deps.contains_key("a.rs"));
    assert!(!index.reverse_deps.contains_key("c.rs"));

    // A producer listed twice, and the file itself, add no edge.
    index.replace_edges_of("b.rs", &[s("a.rs"), s("a.rs"), s("b.rs")]);
    assert_eq!(index.dependencies["b.rs"].len(), 1);

    // No producers at all is a real zero: the file is still covered, and nothing is left.
    index.replace_edges_of("b.rs", &[]);
    assert!(index.reverse_deps.is_empty());
    assert!(index.dependencies.is_empty());
    assert_eq!(index.importers_status("b.rs"), Importers::Isolated);
}

#[test]
fn hotspots_name_files_by_importers_never_the_module_key_of_a_raw_import() {
    // Two files import `utils` by its bare name: `reverse_deps` holds ONE entry, keyed by the
    // MODULE `utils`, which is not a file. The hotspot must be the file that owns the stem.
    let index = index_of(&[
        ("pkg/utils.py", "def f():\n    pass\n", Lang::Python),
        ("main.py", "import utils\n", Lang::Python),
        ("other.py", "import utils\n", Lang::Python),
    ]);
    assert!(
        index.reverse_deps.contains_key("utils"),
        "control: the raw key is a module"
    );
    assert_eq!(index.hotspots(5), [("pkg/utils.py", 2)]);
}

#[test]
fn hotspots_are_ordered_by_importers_then_path_cut_to_the_limit_and_skip_unimported_files() {
    let mut index = SymbolIndex::new();
    index.load_file_edges(
        edges(&[
            ("a.rs", "hub.rs"),
            ("b.rs", "hub.rs"),
            ("c.rs", "hub.rs"),
            ("a.rs", "x.rs"),
            ("b.rs", "x.rs"),
            ("a.rs", "w.rs"),
            ("b.rs", "w.rs"),
            ("a.rs", "lone.rs"),
        ]),
        files(&[
            "a.rs",
            "b.rs",
            "c.rs",
            "hub.rs",
            "lone.rs",
            "w.rs",
            "x.rs",
            "island.rs",
        ]),
    );
    // hub 3 · w 2 · x 2 (path order breaks the tie) · lone 1; `island.rs` has no importer.
    assert_eq!(
        index.hotspots(10),
        [("hub.rs", 3), ("w.rs", 2), ("x.rs", 2), ("lone.rs", 1)]
    );
    assert_eq!(index.hotspots(2), [("hub.rs", 3), ("w.rs", 2)]);
    assert!(index.hotspots(0).is_empty());
}

#[test]
fn replacing_a_consumers_edges_keeps_the_co_edit_weight_of_a_pair_it_still_has() {
    // The hook that refreshes an edited file hands the index its edges again; the weight
    // of a pair that is still there must survive, or one edit would zero the ranking of
    // every importer of the file it touched.
    let mut index = SymbolIndex::new();
    index.load_file_edges(
        edges(&[("b.rs", "a.rs"), ("b.rs", "c.rs")]),
        files(&["a.rs", "b.rs", "c.rs"]),
    );
    index.apply_co_edit_weights([(s("b.rs"), s("a.rs"), 7), (s("b.rs"), s("c.rs"), 3)]);

    // `b.rs` keeps importing `a.rs`, stops importing `c.rs` and starts importing `d.rs`.
    index.replace_edges_of("b.rs", &[s("a.rs"), s("d.rs")]);
    let weight_toward = |index: &SymbolIndex, producer: &str| {
        index.dependencies["b.rs"]
            .iter()
            .find(|edge| edge.to == producer)
            .map(|edge| edge.co_edit_weight)
    };
    assert_eq!(
        weight_toward(&index, "a.rs"),
        Some(7.0),
        "a kept pair keeps its weight"
    );
    assert_eq!(
        weight_toward(&index, "d.rs"),
        Some(0.0),
        "a new pair has no history"
    );
    assert_eq!(
        weight_toward(&index, "c.rs"),
        None,
        "a dropped pair is gone"
    );
}

#[test]
fn loading_the_same_edges_twice_replaces_them() {
    let load = |index: &mut SymbolIndex| {
        index.load_file_edges(
            edges(&[("b.rs", "a.rs"), ("c.rs", "a.rs")]),
            files(&["a.rs", "b.rs", "c.rs"]),
        );
    };
    let mut index = SymbolIndex::new();
    load(&mut index);
    let once = index.reverse_deps.clone();
    load(&mut index);
    assert_eq!(index.reverse_deps, once);
    assert_eq!(index.importers_of("a.rs"), [s("b.rs"), s("c.rs")]);
}

#[test]
fn a_covered_file_keeps_its_wiring_edges_when_it_is_reindexed() {
    let mut index = SymbolIndex::new();
    index.load_file_edges(edges(&[("b.py", "a.py")]), files(&["a.py", "b.py"]));

    index
        .index_file("b.py", "import os\n\ndef f():\n    pass\n", Lang::Python)
        .unwrap();
    assert_eq!(
        index.importers_of("a.py"),
        [s("b.py")],
        "the resolved edge survives"
    );
    assert!(
        !index.reverse_deps.contains_key("os"),
        "the raw import must not replace what the wiring resolved"
    );
    assert_eq!(index.find_symbol("f").len(), 1, "symbols still refresh");

    // A file the edge source does not cover keeps the raw edges extraction sees.
    index
        .index_file("z.py", "import os\n", Lang::Python)
        .unwrap();
    assert_eq!(index.reverse_deps["os"], [s("z.py")]);
}

#[test]
fn clear_forgets_which_files_the_edge_source_covered() {
    // Production reloads an index with `clear()` and then loads it again: a reload that
    // kept the old coverage would treat every file it covered as owned by an edge source
    // that has not spoken yet, and `index_file` would stop refreshing their edges.
    let mut index = SymbolIndex::new();
    index.load_file_edges(edges(&[("b.py", "a.py")]), files(&["a.py", "b.py"]));

    index.clear();
    index
        .index_file("b.py", "import os\n", Lang::Python)
        .unwrap();

    assert_eq!(
        index.reverse_deps.get("os"),
        Some(&vec![s("b.py")]),
        "after clear() nothing covers b.py: its extracted import is recorded again"
    );
    assert_eq!(
        index.importers_status("a.py"),
        Importers::Uncovered,
        "a.py was covered only by the edge source that clear() dropped"
    );
}

#[test]
fn removing_a_file_cuts_the_resolved_edges_toward_it() {
    let mut index = SymbolIndex::new();
    index.load_file_edges(edges(&[("b.rs", "a.rs")]), files(&["a.rs", "b.rs"]));

    index.remove_file("a.rs");

    assert!(index.importers_of("a.rs").is_empty());
    assert!(
        !index.dependencies.contains_key("b.rs"),
        "b.rs lost its only edge"
    );
    assert_eq!(index.importers_status("a.rs"), Importers::Uncovered);
}

#[test]
fn an_uncovered_file_is_not_isolated() {
    let mut index = SymbolIndex::new();
    index.load_file_edges(
        edges(&[("b.rs", "a.rs")]),
        files(&["a.rs", "b.rs", "lonely.rs"]),
    );

    let importers = [s("b.rs")];
    assert_eq!(index.importers_status("a.rs"), Importers::Known(&importers));
    // The wiring looked at these and found nobody: a real zero.
    assert_eq!(index.importers_status("lonely.rs"), Importers::Isolated);
    assert_eq!(index.importers_status("b.rs"), Importers::Isolated);
    // It never looked at this one: "no importer" here means "unknown".
    assert_eq!(
        index.importers_status("never_seen.rs"),
        Importers::Uncovered
    );

    // An index with no edge source speaks for the files it holds, as it always did.
    let plain = index_of(&[("p.py", "def f():\n    pass\n", Lang::Python)]);
    assert_eq!(plain.importers_status("p.py"), Importers::Isolated);
    assert_eq!(plain.importers_status("q.py"), Importers::Uncovered);
}

#[test]
fn a_homonymous_stem_is_resolved_by_a_wiring_edge() {
    let mut index = index_of(&[
        ("a/utils.py", "def helper():\n    pass\n", Lang::Python),
        ("b/utils.py", "def other():\n    pass\n", Lang::Python),
        ("c.py", "import utils\n", Lang::Python),
    ]);
    assert!(
        index.importers_of("a/utils.py").is_empty(),
        "two files answer to `utils`"
    );
    assert!(index.importers_of("b/utils.py").is_empty());

    index.load_file_edges(
        edges(&[("c.py", "a/utils.py")]),
        files(&["a/utils.py", "b/utils.py", "c.py"]),
    );

    assert_eq!(index.importers_of("a/utils.py"), [s("c.py")]);
    assert!(
        index.importers_of("b/utils.py").is_empty(),
        "c.py never imported this one"
    );
}

#[test]
fn co_edit_history_lands_on_the_edges_in_either_direction() {
    let mut index = SymbolIndex::new();
    index.load_file_edges(edges(&[("b.rs", "a.rs")]), files(&["a.rs", "b.rs"]));
    let cost_of_b = |index: &SymbolIndex| {
        index
            .weighted_blast_radius("a.rs")
            .into_iter()
            .find(|(file, _)| file == "b.rs")
            .map(|(_, cost)| cost)
            .unwrap()
    };
    assert_eq!(cost_of_b(&index), 1.0, "no history: unit cost");

    index.apply_co_edit_weights([
        (s("a.rs"), s("b.rs"), 3),
        (s("b.rs"), s("a.rs"), 2),
        (s("x.rs"), s("y.rs"), 9),
        (s("a.rs"), s("a.rs"), 4),
        (s("a.rs"), s("b.rs"), -1),
    ]);

    assert_eq!(
        index.dependencies["b.rs"][0].co_edit_weight, 5.0,
        "3 + 2, nothing else"
    );
    assert!((cost_of_b(&index) - 1.0 / 6.0).abs() < 1e-12);
}

#[test]
fn ranked_importers_order_by_co_edit_weight_then_in_degree_then_path() {
    let mut index = SymbolIndex::new();
    // hub.rs is imported by five files; popular.rs is itself imported by two.
    index.load_file_edges(
        edges(&[
            ("src/b.rs", "src/hub.rs"),
            ("src/a.rs", "src/hub.rs"),
            ("src/popular.rs", "src/hub.rs"),
            ("src/v.rs", "src/hub.rs"),
            ("src/w.rs", "src/hub.rs"),
            ("src/x.rs", "src/popular.rs"),
            ("src/y.rs", "src/popular.rs"),
        ]),
        files(&[
            "src/hub.rs",
            "src/a.rs",
            "src/b.rs",
            "src/popular.rs",
            "src/v.rs",
            "src/w.rs",
            "src/x.rs",
            "src/y.rs",
        ]),
    );
    // History in either direction of the pair: w.rs weighs 5, v.rs weighs 2.
    index.apply_co_edit_weights([
        (s("src/w.rs"), s("src/hub.rs"), 5),
        (s("src/hub.rs"), s("src/v.rs"), 2),
    ]);

    let ranked = index.ranked_importers("src/hub.rs");
    assert_eq!(
        ranked,
        [
            "src/w.rs",
            "src/v.rs",
            "src/popular.rs",
            "src/a.rs",
            "src/b.rs"
        ],
        "weight first (5 > 2 > none), then in-degree (popular.rs has 2), then path"
    );

    let mut asked: Vec<&str> = index
        .importers_of("src/hub.rs")
        .iter()
        .map(String::as_str)
        .collect();
    let mut sorted = ranked;
    asked.sort_unstable();
    sorted.sort_unstable();
    assert_eq!(
        asked, sorted,
        "the same set importers_of answers, reordered"
    );

    assert!(index.ranked_importers("src/never_seen.rs").is_empty());
}

/// A diamond (c imports both a and b, which import hub), a cycle (hub imports d, which is
/// reached from hub through c), and a tail (e imports d, f imports e).
fn diamond_cycle_tail() -> SymbolIndex {
    let mut index = SymbolIndex::new();
    index.load_file_edges(
        edges(&[
            ("a.rs", "hub.rs"),
            ("b.rs", "hub.rs"),
            ("c.rs", "a.rs"),
            ("c.rs", "b.rs"),
            ("d.rs", "c.rs"),
            ("hub.rs", "d.rs"),
            ("e.rs", "d.rs"),
            ("f.rs", "e.rs"),
        ]),
        files(&["hub.rs", "a.rs", "b.rs", "c.rs", "d.rs", "e.rs", "f.rs"]),
    );
    index
}

#[test]
fn blast_extent_is_the_count_and_depth_of_blast_radius_with_depth() {
    let index = diamond_cycle_tail();
    for start in ["hub.rs", "a.rs", "c.rs", "d.rs", "f.rs", "never_seen.rs"] {
        for depth in [0, 1, 2, 3, 4, usize::MAX] {
            let walked = index.blast_radius_with_depth(start, depth);
            let extent = index.blast_extent(start, depth);
            assert_eq!(
                (extent.file_count, extent.max_distance),
                (walked.file_count, walked.max_distance),
                "{start} to {depth} hops: the count-only walk must be the same walk"
            );
        }
    }
    // The shape is not trivial: the cycle is not counted twice, the diamond reaches c once.
    let whole = index.blast_extent("hub.rs", usize::MAX);
    assert_eq!(
        (whole.file_count, whole.max_distance),
        (7, 5),
        "hub, a, b, c, d, e, f; f is five hops out"
    );
    let one_hop = index.blast_extent("hub.rs", 1);
    assert_eq!((one_hop.file_count, one_hop.max_distance), (3, 1));
    assert_eq!(index.blast_extent("never_seen.rs", 4).file_count, 1);
}

#[test]
fn the_edge_coverage_summary_counts_edges_and_covered_files() {
    assert_eq!(
        SymbolIndex::new().edge_coverage_summary(),
        EdgeCoverage::default(),
        "no edge source: nothing declared, nothing covered"
    );
    let summary = diamond_cycle_tail().edge_coverage_summary();
    assert_eq!(summary.edges, 8);
    assert_eq!(summary.covered_files, 7);
}

#[test]
fn the_enriched_blast_radius_reads_the_loaded_edges() {
    let mut index = SymbolIndex::new();
    index.load_file_edges(
        edges(&[("b.rs", "a.rs"), ("c.rs", "b.rs")]),
        files(&["a.rs", "b.rs", "c.rs"]),
    );

    let enriched = compute_enriched_blast_radius(&index, "a.rs", &IndexMap::new());

    assert_eq!(enriched.direct_dependents, [s("b.rs")]);
    assert_eq!(enriched.transitive_dependents, [s("c.rs")]);
}

#[test]
fn load_index_reads_symbols_edges_coverage_and_weights_from_the_stores() {
    const FOO: &str = "crates/a/src/foo.rs";
    const MAIN: &str = "crates/a/src/main.rs";
    const LONELY: &str = "crates/a/src/lonely.rs";

    let tmp = TempDir::new().unwrap();
    let store = SymbolStore::new(&tmp.path().join("symbols.db")).unwrap();
    let extracted = index_of(&[(FOO, "pub fn foo() {}\n", Lang::Rust)]);
    extracted.persist_to(&store).unwrap();

    let knowledge = FileKnowledgeDB::new(&tmp.path().join("knowledge.db")).unwrap();
    knowledge
        .register_pub_symbol(FOO, "foo", "fn", "public")
        .unwrap();
    knowledge
        .register_pub_symbol(LONELY, "Unused", "struct", "public")
        .unwrap();
    knowledge
        .record_consumer(FOO, "foo", MAIN, Some(1))
        .unwrap();
    knowledge.record_coedit(FOO, MAIN).unwrap();
    knowledge.record_coedit(MAIN, FOO).unwrap();

    let index = load_index(&store, &knowledge).unwrap();

    assert_eq!(
        index.find_symbol("foo").len(),
        1,
        "symbols come from the store"
    );
    let importers = [s(MAIN)];
    assert_eq!(index.importers_status(FOO), Importers::Known(&importers));
    assert_eq!(
        index.importers_status(LONELY),
        Importers::Isolated,
        "wired, and nothing consumes it"
    );
    assert_eq!(
        index.importers_status("crates/a/src/not_wired.rs"),
        Importers::Uncovered,
        "the wiring never looked at it"
    );
    assert_eq!(
        index.dependencies[MAIN][0].co_edit_weight, 2.0,
        "both directions of the co-edit pair, summed"
    );
}

#[test]
fn load_index_for_reads_the_wiring_from_a_path_and_never_creates_a_missing_one() {
    const FOO: &str = "crates/a/src/foo.rs";
    const MAIN: &str = "crates/a/src/main.rs";

    let tmp = TempDir::new().unwrap();
    let store = SymbolStore::new(&tmp.path().join("symbols.db")).unwrap();
    index_of(&[(FOO, "pub fn foo() {}\n", Lang::Rust)])
        .persist_to(&store)
        .unwrap();
    let knowledge_path = tmp.path().join("knowledge.db");

    // No wiring store yet: what `load_into_index` always gave (symbols, the raw stem edges of
    // the `dependencies` table), NO resolved edge and NO coverage — an index with no edge
    // source speaks for the files it holds, as it always did — and nothing is created.
    let bare = load_index_for(&store, &knowledge_path).unwrap();
    assert_eq!(bare.find_symbol("foo").len(), 1);
    assert_eq!(bare.edge_coverage_summary().covered_files, 0);
    assert_eq!(bare.importers_status(FOO), Importers::Isolated);
    assert_eq!(
        bare.importers_status("crates/a/src/never_indexed.rs"),
        Importers::Uncovered,
        "a file it does not hold is not isolated, it is unknown"
    );
    assert!(
        !knowledge_path.exists(),
        "reading must not create the wiring store"
    );

    // With the wiring: the same index `load_index` builds.
    {
        let knowledge = FileKnowledgeDB::new(&knowledge_path).unwrap();
        knowledge
            .register_pub_symbol(FOO, "foo", "fn", "public")
            .unwrap();
        knowledge
            .record_consumer(FOO, "foo", MAIN, Some(1))
            .unwrap();
    }
    let wired = load_index_for(&store, &knowledge_path).unwrap();
    let importers = [s(MAIN)];
    assert_eq!(wired.importers_status(FOO), Importers::Known(&importers));
}

// ── coverage_expected: does the edge source speak for files LIKE this one? ──

/// Asking twice around a change is how a stale cache shows: the first answer fills it.
fn expected(index: &SymbolIndex, file: &str) -> bool {
    index.coverage_expected(file)
}

#[test]
fn coverage_is_expected_for_the_extensions_the_edge_source_covers_and_only_those() {
    let mut index = SymbolIndex::new();
    index.load_file_edges(
        edges(&[("src/a.rs", "src/b.rs")]),
        files(&["src/a.rs", "src/b.rs"]),
    );

    assert!(
        expected(&index, "src/never_seen.rs"),
        "same extension as a covered file"
    );
    assert!(
        expected(&index, "SRC/NEW.RS"),
        "the extension match ignores case"
    );
    assert!(
        !expected(&index, "docs/readme.md"),
        "the wiring covers no .md"
    );
    assert!(!expected(&index, "scripts/run.py"), "nor a .py");
    assert!(
        !expected(&index, "Makefile"),
        "nor a file with no extension"
    );
    // It says nothing about the file itself being covered: that is `importers_status`.
    assert_eq!(
        index.importers_status("src/never_seen.rs"),
        Importers::Uncovered
    );
}

#[test]
fn coverage_is_never_expected_when_no_edge_source_was_loaded() {
    // Symbols only (`index_file`): the index speaks for the files it holds, but there is no
    // edge source, so there is no kind of file it "should" cover.
    let index = index_of(&[("src/a.rs", "pub fn a() {}\n", Lang::Rust)]);
    assert_eq!(index.edge_coverage_summary().covered_files, 0);
    assert!(!expected(&index, "src/a.rs"));
    assert!(!expected(&index, "src/new.rs"));
    assert!(!expected(&SymbolIndex::new(), "src/new.rs"));
}

#[test]
fn replace_edges_of_a_new_kind_of_file_makes_its_extension_expected_at_once() {
    let mut index = SymbolIndex::new();
    index.load_file_edges(
        edges(&[("src/a.rs", "src/b.rs")]),
        files(&["src/a.rs", "src/b.rs"]),
    );
    assert!(
        !expected(&index, "tools/other.py"),
        "control: fills the cache without .py"
    );

    // A hook refreshes a Python file: the source now speaks for `.py`.
    index.replace_edges_of("tools/job.py", &[]);
    assert!(
        expected(&index, "tools/other.py"),
        "stale cache: the new covered .py was not seen"
    );
}

#[test]
fn remove_file_of_the_last_file_of_a_kind_makes_its_extension_unexpected_at_once() {
    let mut index = SymbolIndex::new();
    index.load_file_edges(
        edges(&[("src/a.rs", "src/b.rs")]),
        files(&["src/a.rs", "src/b.rs", "tools/job.py"]),
    );
    assert!(
        expected(&index, "tools/other.py"),
        "control: tools/job.py covers .py, and fills the cache"
    );

    index.remove_file("tools/job.py");
    assert!(
        !expected(&index, "tools/other.py"),
        "stale cache: the last covered .py is gone"
    );
    assert!(
        expected(&index, "src/new.rs"),
        "the .rs coverage is untouched"
    );
}

#[test]
fn load_file_edges_covering_only_producers_still_extends_the_expected_extensions() {
    let mut index = SymbolIndex::new();
    index.load_file_edges(
        edges(&[("src/a.rs", "src/b.rs")]),
        files(&["src/a.rs", "src/b.rs"]),
    );
    assert!(
        !expected(&index, "web/app.ts"),
        "control: fills the cache without .ts"
    );

    // A producer nothing consumes, declared covered: no edge goes through `replace_edges_of`.
    index.load_file_edges(Vec::new(), files(&["web/app.ts"]));
    assert!(
        expected(&index, "web/other.ts"),
        "stale cache: the covered-only .ts was not seen"
    );
}

#[test]
fn clear_forgets_the_expected_extensions() {
    let mut index = SymbolIndex::new();
    index.load_file_edges(
        edges(&[("src/a.rs", "src/b.rs")]),
        files(&["src/a.rs", "src/b.rs"]),
    );
    assert!(expected(&index, "src/new.rs"), "control: fills the cache");
    index.clear();
    assert!(!expected(&index, "src/new.rs"), "stale cache after clear");
}

#[test]
fn a_clone_and_a_serde_round_trip_keep_answering_from_the_coverage_they_hold() {
    let mut index = SymbolIndex::new();
    index.load_file_edges(
        edges(&[("src/a.rs", "src/b.rs")]),
        files(&["src/a.rs", "src/b.rs"]),
    );
    assert!(expected(&index, "src/new.rs"), "fills the cache");

    // The cache is derived, not serialized: a copy and a round trip rebuild it from the set.
    let copy = index.clone();
    assert!(expected(&copy, "src/new.rs") && !expected(&copy, "docs/a.md"));
    let json = serde_json::to_string(&index).expect("serializes");
    let back: SymbolIndex = serde_json::from_str(&json).expect("deserializes");
    assert!(expected(&back, "src/new.rs") && !expected(&back, "docs/a.md"));
}
