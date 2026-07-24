//! E2E tests for IndexMap-refactored pheromone structures.
//!
//! Proves: SymbolPheromoneMap, PheromoneGraph, and EnrichedBlastRadius
//! work end-to-end with the complete integration chain.

use indexmap::IndexMap;
#[allow(unused_imports)]
use touring_code::ast::graph::SymbolIndex;
use touring_code::ast::graph::enriched::compute_enriched_blast_radius;
use touring_code::ast::graph::pheromone::{PheromoneGraph, SymbolPheromoneMap};
use touring_code::ast::languages::Lang;

// ─── SymbolPheromoneMap E2E ─────────────────────────────────────────────────

/// Proves: SymbolPheromoneMap records co-access symmetrically and reranks correctly.
#[test]
fn e2e_symbol_pheromone_map_coaccess_rerank() {
    let mut map = SymbolPheromoneMap::new(0.0);

    // Record co-access: ctx accessed with "hot" 3×, with "cold" 1×
    map.record_co_access("ctx", "hot");
    map.record_co_access("ctx", "hot");
    map.record_co_access("ctx", "hot");
    map.record_co_access("ctx", "cold");

    // Verify symmetry: both directions recorded
    assert_eq!(map.pheromone_strength("ctx", "hot"), 3.0);
    assert_eq!(map.pheromone_strength("hot", "ctx"), 3.0);
    assert_eq!(map.pheromone_strength("ctx", "cold"), 1.0);
    assert_eq!(map.pheromone_strength("cold", "ctx"), 1.0);

    // Self-loops ignored
    assert_eq!(map.pheromone_strength("ctx", "ctx"), 0.0);

    // Rerank: hot before cold
    let candidates = vec!["cold".to_string(), "hot".to_string(), "other".to_string()];
    let ranked = map.rerank("ctx", &candidates);
    assert_eq!(ranked[0], "hot");
    assert_eq!(ranked[1], "cold");
    assert_eq!(ranked[2], "other");
}

/// Proves: evaporation decays and prunes weak trails.
#[test]
fn e2e_symbol_pheromone_map_evaporation_prunes_weak() {
    let mut map = SymbolPheromoneMap::new(0.5); // 50% decay per tick

    map.record_co_access("a", "b"); // strength = 1.0
    map.evaporate();
    assert_eq!(map.pheromone_strength("a", "b"), 0.5);

    // With 99.99% evaporation, should prune below threshold (0.001)
    let mut map2 = SymbolPheromoneMap::new(0.9999);
    map2.record_co_access("x", "y");
    map2.evaporate();
    assert_eq!(
        map2.symbol_count(),
        0,
        "pair should be pruned after high evaporation"
    );
}

/// Proves: cap prevents runaway accumulation.
#[test]
fn e2e_symbol_pheromone_map_caps_at_max() {
    let mut map = SymbolPheromoneMap::new(0.0);
    for _ in 0..100 {
        map.record_co_access("a", "b");
    }
    assert_eq!(map.pheromone_strength("a", "b"), 10.0, "capped at 10.0");
}

// ─── PheromoneGraph E2E ────────────────────────────────────────────────────

/// Proves: PheromoneGraph reinforces paths and returns hot edges correctly.
#[test]
fn e2e_pheromone_graph_reinforce_and_hot_edges() {
    let mut graph = PheromoneGraph::new(0.0);

    // Reinforce a traversal: a → b → c → d
    graph.reinforce_path(&["a.rs", "b.rs", "c.rs", "d.rs"]);
    assert_eq!(graph.edge_strength("a.rs", "b.rs"), 1.0);
    assert_eq!(graph.edge_strength("b.rs", "c.rs"), 1.0);
    assert_eq!(graph.edge_strength("c.rs", "d.rs"), 1.0);
    assert_eq!(
        graph.edge_strength("a.rs", "c.rs"),
        0.0,
        "non-consecutive edges not reinforced"
    );

    // Reinforce again: a → b gets strength 2
    graph.reinforce_path(&["a.rs", "b.rs"]);
    assert_eq!(graph.edge_strength("a.rs", "b.rs"), 2.0);

    // Hot edges: top 2 should be a→b (2.0) and b→c (1.0)
    let hot = graph.hot_edges(2);
    assert_eq!(hot.len(), 2);
    assert_eq!(hot[0].0, 2.0, "hottest edge should be a→b with strength 2");
    assert_eq!(hot[1].0, 1.0);
}

/// Proves: self-edges are ignored.
#[test]
fn e2e_pheromone_graph_ignores_self_edge() {
    let mut graph = PheromoneGraph::new(0.0);
    graph.reinforce_path(&["a.rs", "a.rs"]);
    assert_eq!(graph.edge_count(), 0, "self-edge should not be recorded");
}

/// Proves: evaporation decays and prunes.
#[test]
fn e2e_pheromone_graph_evaporation() {
    let mut graph = PheromoneGraph::new(0.5);
    graph.reinforce_path(&["x.rs", "y.rs"]);
    assert_eq!(graph.edge_strength("x.rs", "y.rs"), 1.0);
    graph.evaporate();
    assert!((graph.edge_strength("x.rs", "y.rs") - 0.5).abs() < 1e-9);

    // High evaporation prunes
    let mut graph2 = PheromoneGraph::new(0.9999);
    graph2.reinforce_path(&["p.rs", "q.rs"]);
    graph2.evaporate();
    assert_eq!(
        graph2.edge_count(),
        0,
        "edges should be pruned after high evaporation"
    );
}

// ─── EnrichedBlastRadius E2E ─────────────────────────────────────────────────

/// Proves: compute_enriched_blast_radius categorizes impact correctly.
#[test]
fn e2e_enriched_blast_radius_direct_and_transitive() {
    let mut index = SymbolIndex::new();

    // Index files
    index
        .index_file("a.py", "def helper(): pass", Lang::Python)
        .unwrap();
    index
        .index_file("b.py", "def other(): pass", Lang::Python)
        .unwrap();
    index
        .index_file("c.py", "def another(): pass", Lang::Python)
        .unwrap();

    // Build dependency chain: a → b → c
    index
        .reverse_deps
        .insert("a.py".to_string(), vec!["b.py".to_string()]);
    index
        .reverse_deps
        .insert("b.py".to_string(), vec!["c.py".to_string()]);

    // Co-edit data
    let mut co_edit_data = IndexMap::new();
    co_edit_data.insert("a.py".to_string(), vec!["d.py".to_string()]);

    let result = compute_enriched_blast_radius(&index, "a.py", &co_edit_data);

    // Direct dependents (depth=1): b.py
    assert_eq!(result.direct_dependents, vec!["b.py"]);

    // Transitive dependents (depth>1): c.py (reachable through b, not direct)
    assert!(result.transitive_dependents.contains(&"c.py".to_string()));

    // Co-edited files
    assert_eq!(result.co_edited_files, vec!["d.py"]);

    // Severity: 0.5*1 + 0.3*1 + 0.2*1 / 3 = 0.333...
    assert!(
        (result.severity - 0.333).abs() < 0.01,
        "severity should be ~0.333, got {}",
        result.severity
    );
}

/// Proves: EnrichedBlastRadius handles empty graph gracefully.
#[test]
fn e2e_enriched_blast_radius_empty_graph() {
    let index = SymbolIndex::new();
    let co_edit_data = IndexMap::new();

    let result = compute_enriched_blast_radius(&index, "nonexistent.py", &co_edit_data);

    assert!(result.direct_dependents.is_empty());
    assert!(result.transitive_dependents.is_empty());
    assert!((result.severity - 0.0).abs() < f64::EPSILON);
}

/// Proves: EnrichedBlastRadius severity is bounded [0, 1].
#[test]
fn e2e_enriched_blast_radius_severity_bounded() {
    let mut index = SymbolIndex::new();
    index.index_file("x.py", "x = 1", Lang::Python).unwrap();
    index
        .reverse_deps
        .insert("x.py".to_string(), vec!["y.py".to_string()]);

    let co_edit_data = IndexMap::new();
    let result = compute_enriched_blast_radius(&index, "x.py", &co_edit_data);

    assert!(
        result.severity >= 0.0 && result.severity <= 1.0,
        "severity {} out of bounds",
        result.severity
    );
}

// ─── Integration: All three structures together ───────────────────────────────

/// Proves: HeatMap, SymbolPheromoneMap, and PheromoneGraph can coexist
/// and be used in the same data pipeline.
#[test]
fn e2e_integration_all_pheromone_structures() {
    use touring_code::ast::file_heat::HeatMap;

    // HeatMap — tracks file edit frequency
    let mut heat = HeatMap::new(10);
    heat.record_edit("src/main.rs", 1_000_000.0);
    heat.record_edit("src/main.rs", 1_000_000.0);
    heat.record_edit("lib.rs", 1_000_000.0);
    heat.record_edit("lib.rs", 1_000_000.0);
    heat.record_edit("lib.rs", 1_000_000.0);

    let scores = heat.get_priority_order(1_000_000.0);
    let main_score = scores.iter().find(|(f, _)| *f == "src/main.rs").unwrap().1;
    let lib_score = scores.iter().find(|(f, _)| *f == "lib.rs").unwrap().1;
    assert!(
        lib_score > main_score,
        "more edits = higher heat; lib={}, main={}",
        lib_score,
        main_score
    );

    // SymbolPheromoneMap — tracks symbol co-access
    let mut sym_map = SymbolPheromoneMap::new(0.0);
    sym_map.record_co_access("parse", "validate");
    sym_map.record_co_access("parse", "transform");
    let ranked = sym_map.rerank("parse", &["validate".to_string(), "transform".to_string()]);
    assert_eq!(ranked[0], "validate");

    // PheromoneGraph — tracks file dependency traversal
    let mut graph = PheromoneGraph::new(0.0);
    graph.reinforce_path(&["main.rs", "lib.rs", "util.rs"]);
    assert_eq!(graph.edge_strength("main.rs", "lib.rs"), 1.0);

    // Verify all three are still functional
    assert!(heat.len() >= 2);
    assert!(sym_map.symbol_count() >= 1);
    assert!(graph.edge_count() >= 2);
}
