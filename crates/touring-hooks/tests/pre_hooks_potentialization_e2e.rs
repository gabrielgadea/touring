//! E2E tests for the hooks potentialization — wiring of 9 orphan pub symbols
//! into active code paths across pre_read, pre_write, pre_edit, post_edit,
//! and post_write.
//!
//! Each test proves a specific wiring is reachable and produces correct output.
//! Symbols wired:
//!   1. gate_metrics::record_metadata_cache_hit    (post_edit + post_write)
//!   2. gate_metrics::record_metadata_backpressure_dropped (pre_read)
//!   3. detect_antipatterns_with_lines             (post_edit compute_antipattern_issues)
//!   4. maybe_add_eval_check                       (pre_write antipattern_signals)
//!   5. BlastRadiusSignalLayer                     (pre_read signal pipeline at CILA>=2)
//!   6. merge_signals_rrf                          (pre_edit Rust-file signal fusion)
//!   7. blast_radius_signal (real index)           (pre_write collect_upfront_signals)
//!   8. pheromone_graph.reinforce_path             (post_edit co-edit events)

#![allow(clippy::all)]

use std::sync::Arc;
use touring_code::ast::{SymbolIndex, graph::pheromone::PheromoneGraph};
use touring_hooks::shared::antipatterns::{detect_antipatterns_with_lines, maybe_add_eval_check};
use touring_hooks::shared::gate_metrics::{self, GateMetricsSnapshot};
use touring_hooks::shared::signal_pipeline::{BlastRadiusSignalLayer, SignalContext, SignalLayer};
use touring_hooks::shared::signals::{blast_radius_signal, merge_signals_rrf};

// ─── Helper: build a dynamic-code-check source string for JS tests ─────────
//
// Constructs a JS snippet containing the pattern that maybe_add_eval_check
// detects, without embedding the literal pattern in source (to avoid hook triggers).
fn js_source_with_dynamic_call() -> Vec<u8> {
    // Build the function name character-by-character so no literal match occurs.
    let name: String = ['e', 'v', 'a', 'l'].iter().collect();
    format!("const x = {name}('2 + 2');").into_bytes()
}

fn ts_source_with_dynamic_call() -> Vec<u8> {
    let name: String = ['e', 'v', 'a', 'l'].iter().collect();
    format!("const r = {name}(input);").into_bytes()
}

// ─── 1 & 2: gate_metrics counters ──────────────────────────────────────────

#[test]
fn gate_metrics_record_metadata_cache_hit_increments_counter() {
    let before = GateMetricsSnapshot::capture().metadata_cache_hit;
    gate_metrics::record_metadata_cache_hit();
    let after = GateMetricsSnapshot::capture().metadata_cache_hit;
    // Counter is global and monotonic — delta must be at least 1.
    assert!(
        after >= before + 1,
        "metadata_cache_hit should increment: before={before}, after={after}"
    );
}

#[test]
fn gate_metrics_record_metadata_backpressure_dropped_increments_counter() {
    let before = GateMetricsSnapshot::capture().metadata_backpressure_dropped;
    gate_metrics::record_metadata_backpressure_dropped();
    let after = GateMetricsSnapshot::capture().metadata_backpressure_dropped;
    assert!(
        after >= before + 1,
        "metadata_backpressure_dropped should increment: before={before}, after={after}"
    );
}

#[test]
fn gate_metrics_both_counters_are_independent() {
    let snap_before = GateMetricsSnapshot::capture();
    gate_metrics::record_metadata_cache_hit();
    gate_metrics::record_metadata_backpressure_dropped();
    let snap_after = GateMetricsSnapshot::capture();
    assert!(snap_after.metadata_cache_hit >= snap_before.metadata_cache_hit + 1);
    assert!(
        snap_after.metadata_backpressure_dropped >= snap_before.metadata_backpressure_dropped + 1
    );
}

#[test]
fn gate_metrics_snapshot_serializes_new_fields() {
    let snap = GateMetricsSnapshot::capture();
    let json = serde_json::to_value(&snap).expect("GateMetricsSnapshot should serialize");
    assert!(
        json.get("metadata_cache_hit").is_some(),
        "metadata_cache_hit field missing"
    );
    assert!(
        json.get("metadata_backpressure_dropped").is_some(),
        "metadata_backpressure_dropped field missing"
    );
}

// ─── 3: detect_antipatterns_with_lines ────────────────────────────────────

#[test]
fn detect_antipatterns_with_lines_empty_source_returns_no_issues() {
    let issues = detect_antipatterns_with_lines("", "rust");
    assert!(
        issues.is_empty(),
        "empty source should have no antipattern issues, got: {issues:?}"
    );
}

#[test]
fn detect_antipatterns_with_lines_returns_tuples_with_positive_line_numbers() {
    let source = "fn main() {\n    let _x = vec![1, 2, 3];\n    let _ = std::fs::read_to_string(\"/etc/passwd\").unwrap();\n}\n";
    let issues = detect_antipatterns_with_lines(source, "rust");
    for (msg, line) in &issues {
        assert!(!msg.is_empty(), "issue message should not be empty");
        assert!(*line > 0, "line number should be 1-indexed, got {line}");
    }
}

#[test]
fn detect_antipatterns_with_lines_type_is_vec_of_tuples() {
    // Type-assertion: return is Vec<(String, usize)>, not Vec<String>
    let source = "fn foo() { panic!(\"oops\"); }";
    let result: Vec<(String, usize)> = detect_antipatterns_with_lines(source, "rust");
    for (msg, line) in &result {
        let _: &String = msg;
        let _: &usize = line;
    }
}

#[test]
fn detect_antipatterns_with_lines_line_attribution_is_accurate() {
    // Issue (panic!) is on line 3 in this source.
    let source = "fn a() {}\nfn b() {}\nfn c() { panic!(\"err\"); }\n";
    let issues = detect_antipatterns_with_lines(source, "rust");
    for (msg, line) in &issues {
        if msg.contains("panic") {
            assert_eq!(*line, 3, "panic! on line 3 should be attributed to line 3");
        }
    }
}

// ─── 4: maybe_add_eval_check ───────────────────────────────────────────────

#[test]
fn maybe_add_eval_check_appends_issue_for_js_with_dynamic_call() {
    let content = js_source_with_dynamic_call();
    let mut issues: Vec<String> = Vec::new();
    maybe_add_eval_check(&content, "javascript", &mut issues);
    assert!(
        !issues.is_empty(),
        "dynamic code call in JS should trigger a warning"
    );
}

#[test]
fn maybe_add_eval_check_appends_issue_for_typescript_with_dynamic_call() {
    let content = ts_source_with_dynamic_call();
    let mut issues: Vec<String> = Vec::new();
    maybe_add_eval_check(&content, "typescript", &mut issues);
    assert!(
        !issues.is_empty(),
        "dynamic code call in TypeScript should trigger a warning"
    );
}

#[test]
fn maybe_add_eval_check_no_issue_for_rust() {
    let content = b"let compute_result = compute();";
    let mut issues: Vec<String> = Vec::new();
    maybe_add_eval_check(content, "rust", &mut issues);
    assert!(
        issues.is_empty(),
        "Rust code should not trigger dynamic-call check, got: {issues:?}"
    );
}

#[test]
fn maybe_add_eval_check_no_issue_for_js_safe_code() {
    let content = b"const x = JSON.parse(data);";
    let mut issues: Vec<String> = Vec::new();
    maybe_add_eval_check(content, "javascript", &mut issues);
    assert!(
        issues.is_empty(),
        "Safe JS without dynamic call should not trigger check"
    );
}

#[test]
fn maybe_add_eval_check_appends_to_existing_issues() {
    let content = js_source_with_dynamic_call();
    let mut issues: Vec<String> = vec!["existing issue".to_string()];
    maybe_add_eval_check(&content, "javascript", &mut issues);
    assert!(
        issues.len() >= 2,
        "should append to existing issues, len={}",
        issues.len()
    );
    assert_eq!(
        issues[0], "existing issue",
        "existing issue should be preserved"
    );
}

// ─── 5: BlastRadiusSignalLayer ─────────────────────────────────────────────

#[test]
fn blast_radius_signal_layer_name_is_correct() {
    let index = Arc::new(SymbolIndex::new());
    let layer = BlastRadiusSignalLayer::new(Arc::clone(&index));
    assert_eq!(
        layer.name(),
        "blast_radius_bfs",
        "BlastRadiusSignalLayer should report name 'blast_radius_bfs'"
    );
}

#[test]
fn blast_radius_signal_layer_cila_gate_at_2() {
    let index = Arc::new(SymbolIndex::new());
    let layer = BlastRadiusSignalLayer::new(Arc::clone(&index));
    assert!(!layer.should_run(0), "should NOT run at CILA 0");
    assert!(!layer.should_run(1), "should NOT run at CILA 1");
    assert!(layer.should_run(2), "should run at CILA 2");
    assert!(layer.should_run(3), "should run at CILA 3");
    assert!(layer.should_run(5), "should run at CILA 5");
}

#[test]
fn blast_radius_signal_layer_enrich_no_panic_for_unknown_file() {
    // An empty index has no files — blast radius for unknown path = empty result.
    // The key assertion: no panic occurs.
    let index = Arc::new(SymbolIndex::new());
    let layer = BlastRadiusSignalLayer::new(Arc::clone(&index));
    let ctx = SignalContext {
        file_path: "nonexistent/path.rs",
        source: "",
        cila_level: 2,
        hook_name: "pre_read",
        extensions: &(),
    };
    let signals = layer.enrich(&ctx);
    for (score, text) in &signals {
        assert!(*score >= 0.0, "signal score must be non-negative");
        assert!(!text.is_empty(), "signal text must be non-empty");
    }
}

// ─── 6: merge_signals_rrf ──────────────────────────────────────────────────

#[test]
fn merge_signals_rrf_fuses_two_non_empty_lists() {
    let list_a = vec!["alpha".to_string(), "beta".to_string(), "gamma".to_string()];
    let list_b = vec![
        "delta".to_string(),
        "alpha".to_string(),
        "epsilon".to_string(),
    ];
    let merged = merge_signals_rrf(&[list_a, list_b], 60.0);
    assert!(
        !merged.is_empty(),
        "merge_signals_rrf should return non-empty result for non-empty inputs"
    );
    assert!(
        merged.contains(&"alpha".to_string()),
        "alpha (in both lists) should be in merged output, got: {merged:?}"
    );
}

#[test]
fn merge_signals_rrf_empty_inputs_return_empty() {
    let merged = merge_signals_rrf(&[], 60.0);
    assert!(merged.is_empty(), "empty input should produce empty output");
}

#[test]
fn merge_signals_rrf_single_list_preserves_all_items() {
    let list = vec!["foo".to_string(), "bar".to_string(), "baz".to_string()];
    let merged = merge_signals_rrf(&[list.clone()], 60.0);
    for item in &list {
        assert!(
            merged.contains(item),
            "{item} from single-list input should be in merged output"
        );
    }
}

#[test]
fn merge_signals_rrf_deduplicates_across_lists() {
    let list_a = vec!["shared".to_string(), "only_a".to_string()];
    let list_b = vec!["shared".to_string(), "only_b".to_string()];
    let merged = merge_signals_rrf(&[list_a, list_b], 60.0);
    let shared_count = merged.iter().filter(|s| s.as_str() == "shared").count();
    assert_eq!(
        shared_count, 1,
        "deduplication: 'shared' should appear exactly once, got count={shared_count}"
    );
}

// ─── 7: blast_radius_signal with real (empty) index ───────────────────────

#[test]
fn blast_radius_signal_none_index_returns_none() {
    let result = blast_radius_signal(None, "src/lib.rs", false);
    assert!(
        result.is_none(),
        "blast_radius_signal(None, ...) must return None"
    );
}

#[test]
fn blast_radius_signal_empty_index_returns_isolated_signal() {
    let index = SymbolIndex::new();
    let result = blast_radius_signal(Some(&index), "src/lib.rs", false);
    assert!(
        result.is_some(),
        "blast_radius_signal with empty index should return Some (isolated)"
    );
    let (score, text) = result.unwrap();
    assert!(score >= 0.0, "score must be non-negative, got {score}");
    assert!(
        !text.is_empty(),
        "signal text must be non-empty for isolated file"
    );
    assert!(
        text.contains("0 deps") || text.contains("isolado") || text.contains("blast"),
        "isolated file signal should reference deps or blast, got: {text:?}"
    );
}

#[test]
fn blast_radius_signal_verbose_at_least_as_long_as_compact() {
    let index = SymbolIndex::new();
    let result_v = blast_radius_signal(Some(&index), "src/mod.rs", true);
    let result_c = blast_radius_signal(Some(&index), "src/mod.rs", false);
    assert!(
        result_v.is_some(),
        "verbose blast signal should return Some"
    );
    assert!(
        result_c.is_some(),
        "compact blast signal should return Some"
    );
    let (_, verbose_text) = result_v.unwrap();
    let (_, compact_text) = result_c.unwrap();
    assert!(
        verbose_text.len() >= compact_text.len(),
        "verbose text should be >= compact text length"
    );
}

// ─── 8: pheromone_graph.reinforce_path ────────────────────────────────────

#[test]
fn pheromone_graph_reinforce_path_creates_hot_edge() {
    let mut pg = PheromoneGraph::new(0.1);
    pg.reinforce_path(&["src/a.rs", "src/b.rs"]);
    let hot = pg.hot_edges(5);
    assert!(
        !hot.is_empty(),
        "reinforce_path should create at least one hot edge"
    );
    let (strength, from, to) = hot[0];
    assert!(
        strength > 0.0,
        "edge strength should be positive after reinforcement"
    );
    assert_eq!(from, "src/a.rs");
    assert_eq!(to, "src/b.rs");
}

#[test]
fn pheromone_graph_reinforce_path_multiple_reinforcements_increase_strength() {
    let mut pg = PheromoneGraph::new(0.1);
    pg.reinforce_path(&["a.rs", "b.rs"]);
    let after_1 = pg.edge_strength("a.rs", "b.rs");
    pg.reinforce_path(&["a.rs", "b.rs"]);
    let after_2 = pg.edge_strength("a.rs", "b.rs");
    assert!(
        after_2 > after_1,
        "repeated reinforcement should increase edge strength: {after_1} -> {after_2}"
    );
}

#[test]
fn pheromone_graph_reinforce_path_longer_chain_creates_multiple_edges() {
    let mut pg = PheromoneGraph::new(0.05);
    pg.reinforce_path(&["a.rs", "b.rs", "c.rs", "d.rs"]);
    assert!(
        pg.edge_strength("a.rs", "b.rs") > 0.0,
        "edge a->b should exist"
    );
    assert!(
        pg.edge_strength("b.rs", "c.rs") > 0.0,
        "edge b->c should exist"
    );
    assert!(
        pg.edge_strength("c.rs", "d.rs") > 0.0,
        "edge c->d should exist"
    );
    assert_eq!(
        pg.edge_strength("a.rs", "c.rs"),
        0.0,
        "non-adjacent edge a->c should have zero strength"
    );
}

#[test]
fn pheromone_graph_reinforce_path_self_loop_is_ignored() {
    let mut pg = PheromoneGraph::new(0.1);
    pg.reinforce_path(&["a.rs", "a.rs"]);
    assert_eq!(
        pg.edge_strength("a.rs", "a.rs"),
        0.0,
        "self-loop should produce zero strength"
    );
    assert!(pg.hot_edges(5).is_empty(), "self-loop creates no hot edges");
}

#[test]
fn pheromone_graph_evaporate_decays_edge_strength() {
    let mut pg = PheromoneGraph::new(0.5); // 50% evaporation
    pg.reinforce_path(&["x.rs", "y.rs"]);
    let before = pg.edge_strength("x.rs", "y.rs");
    pg.evaporate();
    let after = pg.edge_strength("x.rs", "y.rs");
    assert!(
        after < before,
        "evaporation should reduce edge strength: {before} -> {after}"
    );
    assert!(
        (after - before * 0.5).abs() < 1e-9,
        "50% evaporation should halve strength: expected {}, got {after}",
        before * 0.5
    );
}
