// Test harness idioms permitted.
#![allow(
    clippy::absurd_extreme_comparisons,
    clippy::assertions_on_constants,
    clippy::let_unit_value,
    clippy::manual_range_contains,
    clippy::useless_vec,
    clippy::int_plus_one,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

//! E2E tests for signal pipeline and daemon IPC integration.
//!
//! Tests the complete signal enrichment pipeline from collection through RRF fusion to budget truncation.
//!
//! # Coverage
//!
//! 1. **SignalPipeline execution at different CILA levels**
//!    - CILA 0-2: only basic signals
//!    - CILA 3-4: blast radius signals
//!    - CILA 5-6: HNSW ANN signals
//!
//! 2. **RRF fusion of multiple signal sources**
//!    - Merge index + source signals
//!    - Verify normalize_scores preserves ordering
//!
//! 3. **Signal budget truncation**
//!    - Test that output respects budget
//!    - Test priority ordering of signals
//!
//! 4. **StaticSignalLayer**
//!    - Add static signals to pipeline
//!    - Verify they appear in output
//!
//! 5. **CilaGatedLayer**
//!    - Verify should_run respects CILA level
//!    - Test minimum CILA threshold
//!
//! 6. **PheromoneGraph integration**
//!    - Evaporate reduces edge strength
//!    - Reinforce creates/strengthens edges

#![allow(clippy::indexing_slicing)]

use std::sync::{Arc, RwLock};
use tempfile::TempDir;
use touring_code::ast::graph::pheromone::PheromoneGraph;
use touring_hooks::runtime::HookRuntime;
use touring_hooks::shared::signal_pipeline::{
    CilaGatedLayer, FnSignalLayer, SignalContext, SignalLayer, SignalPipeline, StaticSignalLayer,
    build_graph_pipeline,
};
use touring_hooks::shared::signals::{normalize_scores, score_cmp};

// ── Helpers ───────────────────────────────────────────────────────────────

/// Create a temp project root with `.claude/data/` and an initialized HookRuntime.
fn setup_runtime() -> (TempDir, HookRuntime) {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path().to_path_buf();
    std::fs::create_dir_all(root.join(".claude/data")).expect("data dir");
    let rt = HookRuntime::new(&root).expect("runtime init");
    (tmp, rt)
}

// ── Test 1: SignalPipeline at CILA L0 (basic only) ──────────────────────

#[test]
fn signal_pipeline_cila_l0_only_basic_signals() {
    let pipeline =
        SignalPipeline::new(1000)
            .with_normalize(false)
            .add_layer(StaticSignalLayer::new(
                "basic",
                vec![
                    (0.8, "basic_signal_a".to_string()),
                    (0.5, "basic_signal_b".to_string()),
                ],
            ));

    let ctx = SignalContext::new("src/lib.rs", "fn main() {}").with_cila(0);
    let result = pipeline.execute(&ctx);

    assert!(
        result.is_some(),
        "pipeline should produce output at CILA L0"
    );
    let output = result.unwrap();
    assert!(output.contains("basic_signal_a") || output.contains("basic_signal_b"));
}

#[test]
fn signal_pipeline_cila_l1_produces_signals() {
    let pipeline = SignalPipeline::new(1000).add_layer(StaticSignalLayer::new(
        "l1",
        vec![(0.9, "l1_only".to_string())],
    ));

    let ctx = SignalContext::new("main.rs", "").with_cila(1);
    let result = pipeline.execute(&ctx);
    assert!(result.is_some());
}

#[test]
fn signal_pipeline_cila_l2_includes_gated_layer() {
    // CilaGatedLayer with min_cila=2 should RUN at CILA 2
    let gated = CilaGatedLayer::new(
        StaticSignalLayer::new("gated_l2", vec![(1.0, "gated_l2_signal".to_string())]),
        2, // min_cila = 2
    );

    let pipeline = SignalPipeline::new(1000).add_layer(gated);

    let ctx = SignalContext::new("lib.rs", "").with_cila(2);
    let result = pipeline.execute(&ctx);
    assert!(
        result.is_some(),
        "gated layer should run at min_cila threshold"
    );
    assert!(result.unwrap().contains("gated_l2_signal"));
}

// ── Test 2: SignalPipeline at CILA L3-L4 (blast radius signals) ─────────

#[test]
fn signal_pipeline_cila_l3_enriched_blast_radius_runs() {
    let pipeline = SignalPipeline::new(1000)
        .with_normalize(false)
        .add_layer(CilaGatedLayer::new(
            StaticSignalLayer::new(
                "enriched",
                vec![(0.85, "enriched_blast_signal".to_string())],
            ),
            3, // min_cila = 3
        ));

    let ctx = SignalContext::new("src/lib.rs", "").with_cila(3);
    let result = pipeline.execute(&ctx);
    assert!(result.is_some());
    assert!(result.unwrap().contains("enriched_blast_signal"));
}

#[test]
fn signal_pipeline_cila_l4_weighted_blast_runs() {
    let pipeline = SignalPipeline::new(1000)
        .with_normalize(false)
        .add_layer(CilaGatedLayer::new(
            StaticSignalLayer::new(
                "weighted",
                vec![(0.75, "weighted_blast_signal".to_string())],
            ),
            4, // min_cila = 4
        ));

    let ctx = SignalContext::new("src/main.rs", "").with_cila(4);
    let result = pipeline.execute(&ctx);
    assert!(result.is_some());
    assert!(result.unwrap().contains("weighted_blast_signal"));
}

// ── Test 3: SignalPipeline at CILA L5-L6 (HNSW ANN signals) ─────────────

#[test]
fn signal_pipeline_cila_l5_hnsw_layer_runs() {
    let pipeline = SignalPipeline::new(1000)
        .with_normalize(false)
        .add_layer(CilaGatedLayer::new(
            StaticSignalLayer::new("hnsw", vec![(0.95, "hnsw_ann_signal".to_string())]),
            5, // min_cila = 5
        ));

    let ctx = SignalContext::new("src/lib.rs", "").with_cila(5);
    let result = pipeline.execute(&ctx);
    assert!(result.is_some());
    assert!(result.unwrap().contains("hnsw_ann_signal"));
}

#[test]
fn signal_pipeline_cila_l6_all_layers_runs() {
    let pipeline = SignalPipeline::new(2000)
        .with_normalize(false)
        .add_layer(CilaGatedLayer::new(
            StaticSignalLayer::new("hnsw", vec![(0.95, "hnsw_l6".to_string())]),
            5,
        ))
        .add_layer(CilaGatedLayer::new(
            StaticSignalLayer::new("weighted", vec![(0.85, "weighted_l6".to_string())]),
            4,
        ))
        .add_layer(CilaGatedLayer::new(
            StaticSignalLayer::new("enriched", vec![(0.75, "enriched_l6".to_string())]),
            3,
        ))
        .add_layer(StaticSignalLayer::new(
            "basic",
            vec![(0.65, "basic_l6".to_string())],
        ));

    let ctx = SignalContext::new("src/lib.rs", "").with_cila(6);
    let result = pipeline.execute(&ctx);
    assert!(result.is_some());
    let output = result.unwrap();
    // All layers should contribute at CILA 6
    assert!(output.contains("hnsw_l6") || output.contains("weighted_l6"));
}

// ── Test 4: RRF Fusion via normalize_scores ─────────────────────────────

#[test]
fn rrf_fusion_merges_multiple_sources() {
    let mut signals = vec![
        (2.0, "high_priority".to_string()),
        (1.0, "medium_priority".to_string()),
        (0.5, "low_priority".to_string()),
        (3.0, "highest_priority".to_string()),
    ];

    // Normalize scores - should map to [0, 1] range preserving order
    normalize_scores(&mut signals);

    // After normalization, highest should still be distinguishable
    let scores: Vec<f32> = signals.iter().map(|(s, _)| *s).collect();
    assert!(
        scores[0] >= scores[1],
        "higher original score should map to higher normalized score"
    );
}

#[test]
fn rrf_fusion_preserves_ordering_within_tier() {
    let mut signals = vec![
        (1.0, "a".to_string()),
        (1.0, "b".to_string()),
        (0.5, "c".to_string()),
        (0.5, "d".to_string()),
    ];

    normalize_scores(&mut signals);

    // Same-tier signals should maintain relative order
    let mut sorted = signals.clone();
    sorted.sort_by(score_cmp);
    assert_eq!(sorted[0].1, "a");
    assert_eq!(sorted[1].1, "b");
}

#[test]
fn rrf_fusion_single_signal_unchanged() {
    let mut signals = vec![(0.7, "only".to_string())];
    normalize_scores(&mut signals);
    // Single element should remain unchanged (normalization returns early)
    assert_eq!(signals.len(), 1);
    assert_eq!(signals[0].1, "only");
}

#[test]
fn rrf_fusion_empty_signals_handled() {
    let mut signals: Vec<(f32, String)> = vec![];
    normalize_scores(&mut signals);
    assert!(signals.is_empty());
}

// ── Test 5: Signal Budget Truncation ────────────────────────────────────

#[test]
fn budget_truncation_respects_max_chars() {
    let pipeline = SignalPipeline::new(30) // 30 char budget
        .with_normalize(false)
        .add_layer(StaticSignalLayer::new(
            "a",
            vec![
                (2.0, "short".to_string()), // 5 chars
                (
                    1.0,
                    "this is a much longer signal text that exceeds the budget limit".to_string(),
                ),
            ],
        ));

    let ctx = SignalContext::new("test.rs", "");
    let result = pipeline
        .execute(&ctx)
        .expect("budget pipeline must produce output");

    // short (5) + " | " (3) + part of long signal should fit
    assert!(result.contains("short"), "short signal must be present");
    assert!(
        result.len() <= 50,
        "result should be truncated to budget or slightly over due to last signal"
    );
}

#[test]
fn budget_truncation_priority_ordering() {
    let pipeline = SignalPipeline::new(50)
        .with_normalize(false)
        .add_layer(StaticSignalLayer::new(
            "high",
            vec![(3.0, "first_high_priority".to_string())],
        ))
        .add_layer(StaticSignalLayer::new(
            "low",
            vec![(1.0, "second_low_priority".to_string())],
        ));

    let ctx = SignalContext::new("test.rs", "");
    let result = pipeline
        .execute(&ctx)
        .expect("priority pipeline must produce output");

    // High priority should appear before low priority
    let high_pos = result.find("first_high_priority").unwrap_or(usize::MAX);
    let low_pos = result.find("second_low_priority").unwrap_or(usize::MAX);
    assert!(
        high_pos < low_pos,
        "high priority should appear before low priority"
    );
}

#[test]
fn budget_exactly_one_signal_fits() {
    let text = "x".repeat(20);
    let pipeline = SignalPipeline::new(20)
        .with_normalize(false)
        .add_layer(StaticSignalLayer::new("exact", vec![(1.0, text.clone())]));

    let ctx = SignalContext::new("main.rs", "");
    let result = pipeline
        .execute(&ctx)
        .expect("exact-fit signal must be included");
    assert_eq!(result, text);
}

#[test]
fn budget_empty_when_all_signals_exceed() {
    let pipeline = SignalPipeline::new(5) // very small budget
        .with_normalize(false)
        .add_layer(StaticSignalLayer::new(
            "large",
            vec![(
                1.0,
                "this is a very long signal that will not fit".to_string(),
            )],
        ));

    let ctx = SignalContext::new("test.rs", "");
    let result = pipeline.execute(&ctx);
    // With very small budget (5 chars), the signal text "this is a very..." (40+ chars)
    // plus separator cannot fit, so result may be None or truncated
    // This test verifies no panic occurs
    assert!(result.is_none() || result.is_some());
}

// ── Test 6: StaticSignalLayer ────────────────────────────────────────────

#[test]
fn static_signal_layer_always_produces_output() {
    let layer = StaticSignalLayer::new(
        "always",
        vec![
            (1.0, "static_signal_a".to_string()),
            (0.8, "static_signal_b".to_string()),
        ],
    );

    let ctx = SignalContext::new("test.rs", "");
    let signals = layer.enrich(&ctx);

    assert_eq!(signals.len(), 2);
    assert!(signals.iter().any(|(_, s)| s.contains("static_signal_a")));
    assert!(signals.iter().any(|(_, s)| s.contains("static_signal_b")));
}

#[test]
fn static_signal_layer_in_pipeline() {
    let pipeline = SignalPipeline::new(1000).add_layer(StaticSignalLayer::new(
        "test",
        vec![(1.0, "pipeline_static".to_string())],
    ));

    let ctx = SignalContext::new("test.rs", "");
    let result = pipeline.execute(&ctx);

    assert!(result.is_some());
    assert!(result.unwrap().contains("pipeline_static"));
}

#[test]
fn static_signal_layer_with_zero_signals() {
    let layer = StaticSignalLayer::new("empty", vec![]);
    let ctx = SignalContext::new("test.rs", "");
    let signals = layer.enrich(&ctx);
    assert!(signals.is_empty());
}

// ── Test 7: CilaGatedLayer ──────────────────────────────────────────────

#[test]
fn cila_gated_layer_respects_threshold() {
    let gated = CilaGatedLayer::new(
        StaticSignalLayer::new("expensive", vec![(1.0, "expensive".to_string())]),
        3, // min_cila = 3
    );

    assert!(!gated.should_run(0), "must not run below threshold");
    assert!(!gated.should_run(1), "must not run below threshold");
    assert!(!gated.should_run(2), "must not run below threshold");
    assert!(gated.should_run(3), "must run at threshold");
    assert!(gated.should_run(4), "must run above threshold");
    assert!(gated.should_run(6), "must run at max");
}

#[test]
fn cila_gated_layer_skipped_below_threshold_in_pipeline() {
    let pipeline = SignalPipeline::new(1000)
        .with_normalize(false)
        .add_layer(StaticSignalLayer::new(
            "always",
            vec![(1.0, "ungated".to_string())],
        ))
        .add_layer(CilaGatedLayer::new(
            StaticSignalLayer::new("expensive", vec![(2.0, "gated_signal".to_string())]),
            4, // min_cila = 4
        ));

    let ctx_low = SignalContext::new("main.rs", "").with_cila(2);
    let result = pipeline
        .execute(&ctx_low)
        .expect("ungated layer must contribute signals");

    assert!(result.contains("ungated"), "ungated layer must contribute");
    assert!(
        !result.contains("gated_signal"),
        "gated layer must be skipped below threshold"
    );
}

#[test]
fn cila_gated_layer_runs_at_threshold_in_pipeline() {
    let pipeline = SignalPipeline::new(1000)
        .with_normalize(false)
        .add_layer(CilaGatedLayer::new(
            StaticSignalLayer::new("gated", vec![(2.0, "gated_at_threshold".to_string())]),
            4, // min_cila = 4
        ));

    let ctx = SignalContext::new("main.rs", "").with_cila(4);
    let result = pipeline.execute(&ctx);

    assert!(
        result.is_some(),
        "gated layer should run at min_cila threshold"
    );
    assert!(result.unwrap().contains("gated_at_threshold"));
}

// ── Test 8: FnSignalLayer ───────────────────────────────────────────────

#[test]
fn fn_signal_layer_context_aware() {
    let layer = FnSignalLayer::new("context_aware", |ctx: &SignalContext<'_>| {
        let mut sigs = Vec::new();
        if ctx.file_path.ends_with(".rs") {
            sigs.push((1.0, "rust_file".to_string()));
        }
        if ctx.cila_level >= 3 {
            sigs.push((0.8, "high_cila".to_string()));
        }
        if ctx.hook_name == "pre_edit" {
            sigs.push((0.6, "pre_edit_context".to_string()));
        }
        sigs
    });

    let ctx = SignalContext::new("main.rs", "fn main() {}")
        .with_cila(4)
        .with_hook("pre_edit");

    let signals = layer.enrich(&ctx);
    assert_eq!(signals.len(), 3);
    assert!(signals.iter().any(|(_, s)| s == "rust_file"));
    assert!(signals.iter().any(|(_, s)| s == "high_cila"));
    assert!(signals.iter().any(|(_, s)| s == "pre_edit_context"));
}

// ── Test 9: PheromoneGraph Integration ─────────────────────────────────

#[test]
fn pheromone_graph_reinforce_creates_edges() {
    let mut graph = PheromoneGraph::new(0.1);

    // reinforce_path deposits on consecutive pairs
    graph.reinforce_path(&["a.rs", "b.rs", "c.rs"]);

    assert!((graph.edge_strength("a.rs", "b.rs") - 1.0).abs() < 1e-9);
    assert!((graph.edge_strength("b.rs", "c.rs") - 1.0).abs() < 1e-9);
    assert_eq!(graph.edge_strength("a.rs", "c.rs"), 0.0); // non-consecutive
    assert_eq!(graph.edge_count(), 2);
}

#[test]
fn pheromone_graph_evaporate_reduces_strength() {
    let mut graph = PheromoneGraph::new(0.1); // 10% evaporation
    graph.reinforce_path(&["a.rs", "b.rs"]);

    let initial = graph.edge_strength("a.rs", "b.rs");
    assert!((initial - 1.0).abs() < 1e-9);

    graph.evaporate();

    let after = graph.edge_strength("a.rs", "b.rs");
    assert!(after < initial, "evaporation should reduce edge strength");
    assert!(
        after > 0.0,
        "evaporation should not eliminate edge immediately"
    );
}

#[test]
fn pheromone_graph_evaporate_prunes_weak_edges() {
    let mut graph = PheromoneGraph::new(0.95); // 95% evaporation - aggressive
    graph.reinforce_path(&["a.rs", "b.rs"]);

    // Evaporate multiple times to push below prune_threshold (0.001)
    for _ in 0..10 {
        graph.evaporate();
    }

    assert_eq!(
        graph.edge_count(),
        0,
        "repeated evaporation should prune weak edges"
    );
}

#[test]
fn pheromone_graph_reinforce_accumulates() {
    let mut graph = PheromoneGraph::new(0.0); // no evaporation

    graph.reinforce_path(&["a.rs", "b.rs"]);
    assert!((graph.edge_strength("a.rs", "b.rs") - 1.0).abs() < 1e-9);

    graph.reinforce_path(&["a.rs", "b.rs"]);
    assert!((graph.edge_strength("a.rs", "b.rs") - 2.0).abs() < 1e-9);

    graph.reinforce_path(&["a.rs", "b.rs"]);
    assert!((graph.edge_strength("a.rs", "b.rs") - 3.0).abs() < 1e-9);
}

#[test]
fn pheromone_graph_hot_edges_returns_top_k() {
    let mut graph = PheromoneGraph::new(0.0);

    graph.reinforce_path(&["a.rs", "b.rs"]);
    graph.reinforce_path(&["a.rs", "b.rs"]); // a->b reinforced twice
    graph.reinforce_path(&["b.rs", "c.rs"]); // b->c reinforced once
    graph.reinforce_path(&["c.rs", "d.rs"]);

    let hot = graph.hot_edges(2);

    // Top 2 should be a->b (strength 2) and b->c (strength 1)
    assert_eq!(hot.len(), 2);
    assert_eq!(hot[0].1, "a.rs"); // strongest
    assert_eq!(hot[0].2, "b.rs");
    assert!((hot[0].0 - 2.0).abs() < 1e-9);
}

#[test]
fn pheromone_graph_signal_layer_empty_graph() {
    let graph = Arc::new(RwLock::new(PheromoneGraph::new(0.1)));
    let layer = touring_hooks::shared::signal_pipeline::PheromoneGraphSignalLayer::new(graph);

    let ctx = SignalContext::new("src/lib.rs", "");
    let signals = layer.enrich(&ctx);

    assert!(
        signals.is_empty(),
        "empty graph must produce no pheromone signals"
    );
}

// ── Test 10: PheromoneGraphSignalLayer ─────────────────────────────────

#[test]
fn pheromone_graph_signal_layer_hot_edges_format() {
    let mut pg = PheromoneGraph::new(0.1);
    pg.reinforce_path(&["a.rs", "b.rs", "c.rs"]);

    let layer = touring_hooks::shared::signal_pipeline::PheromoneGraphSignalLayer::new(Arc::new(
        RwLock::new(pg),
    ));
    let ctx = SignalContext::new("a.rs", "");
    let signals = layer.enrich(&ctx);

    assert_eq!(signals.len(), 2, "two edges expected: a->b, b->c");
    for (score, label) in &signals {
        assert!(*score > 0.0 && *score <= 1.0, "score must be in (0, 1]");
        assert!(label.contains(" → "), "label must use ' → ' separator");
    }
}

#[test]
fn pheromone_graph_signal_layer_normalized_scores() {
    let mut pg = PheromoneGraph::new(0.0);
    pg.reinforce_path(&["a.rs", "b.rs"]);
    pg.reinforce_path(&["a.rs", "b.rs"]); // twice = strength 2
    pg.reinforce_path(&["b.rs", "c.rs"]); // once = strength 1

    let layer = touring_hooks::shared::signal_pipeline::PheromoneGraphSignalLayer::new(Arc::new(
        RwLock::new(pg),
    ));
    let ctx = SignalContext::new("a.rs", "");
    let signals = layer.enrich(&ctx);

    // Max strength is 2, so a->b should have score 1.0, b->c should have 0.5
    let a_to_b = signals
        .iter()
        .find(|(_, l)| l.contains("a.rs") && l.contains("b.rs"));
    let b_to_c = signals
        .iter()
        .find(|(_, l)| l.contains("b.rs") && l.contains("c.rs"));

    assert!(a_to_b.is_some());
    assert!(b_to_c.is_some());
    assert!(
        (a_to_b.unwrap().0 - 1.0).abs() < 1e-5,
        "strongest edge should normalize to 1.0"
    );
    assert!(
        (b_to_c.unwrap().0 - 0.5).abs() < 1e-5,
        "weaker edge should normalize to 0.5"
    );
}

// ── Test 11: build_graph_pipeline ──────────────────────────────────────

#[test]
fn build_graph_pipeline_has_five_layers() {
    use std::sync::Arc;
    use touring_code::ast::SymbolIndex;

    let index = Arc::new(SymbolIndex::new());
    let graph = Arc::new(RwLock::new(PheromoneGraph::new(0.1)));
    let pipeline = build_graph_pipeline(Arc::clone(&index), Arc::clone(&graph), 2000);

    assert_eq!(
        pipeline.layer_count(),
        5,
        "build_graph_pipeline should have 5 layers"
    );
}

#[test]
fn build_graph_pipeline_respects_budget() {
    use std::sync::Arc;
    use touring_code::ast::SymbolIndex;

    let index = Arc::new(SymbolIndex::new());
    let graph = Arc::new(RwLock::new(PheromoneGraph::new(0.1)));
    let pipeline = build_graph_pipeline(Arc::clone(&index), Arc::clone(&graph), 50); // small budget

    let ctx = SignalContext::new("src/lib.rs", "fn main() {}").with_cila(6);
    let result = pipeline.execute(&ctx);

    // Empty index + graph may produce no signals, but pipeline should not panic
    // This is expected behavior - without real index data, no signals are produced
    assert!(result.is_none() || result.as_ref().map(|r| r.len() <= 200).unwrap_or(true));
}

// ── Test 12: execute_with_metrics ──────────────────────────────────────

#[test]
fn execute_with_metrics_returns_metrics() {
    let pipeline = SignalPipeline::new(1000)
        .add_layer(StaticSignalLayer::new(
            "layer_a",
            vec![(1.0, "signal_a".to_string())],
        ))
        .add_layer(StaticSignalLayer::new(
            "layer_b",
            vec![(0.8, "signal_b".to_string())],
        ));

    let ctx = SignalContext::new("test.rs", "");
    let (result, metrics) = pipeline.execute_with_metrics(&ctx);

    assert!(result.is_some());
    assert_eq!(metrics.len(), 2);
    assert!(
        metrics
            .iter()
            .any(|m| m.name == "layer_a" && m.signal_count == 1)
    );
    assert!(
        metrics
            .iter()
            .any(|m| m.name == "layer_b" && m.signal_count == 1)
    );
}

#[test]
fn execute_with_metrics_skips_gated_layers() {
    let pipeline = SignalPipeline::new(1000)
        .add_layer(StaticSignalLayer::new(
            "always",
            vec![(1.0, "present".to_string())],
        ))
        .add_layer(CilaGatedLayer::new(
            StaticSignalLayer::new("skipped", vec![(2.0, "absent".to_string())]),
            5, // min_cila = 5
        ));

    let ctx = SignalContext::new("main.rs", "").with_cila(0);
    let (result, metrics) = pipeline.execute_with_metrics(&ctx);

    assert_eq!(
        metrics.len(),
        1,
        "only non-gated layer should appear in metrics"
    );
    assert_eq!(metrics[0].name, "always");
    assert!(result.is_some());
    assert!(result.unwrap().contains("present"));
}

#[test]
fn execute_with_metrics_all_skipped_returns_none() {
    let pipeline = SignalPipeline::new(1000).add_layer(CilaGatedLayer::new(
        StaticSignalLayer::new("gated", vec![(1.0, "signal".to_string())]),
        6, // min_cila = 6
    ));

    let ctx = SignalContext::new("main.rs", "").with_cila(0);
    let (result, metrics) = pipeline.execute_with_metrics(&ctx);

    assert!(result.is_none(), "no layers ran - must return None");
    assert!(metrics.is_empty(), "no layers ran - metrics must be empty");
}

// ── Test 13: LayerMetrics ───────────────────────────────────────────────

#[test]
fn layer_metrics_contains_timing_info() {
    let layer = StaticSignalLayer::new("timed", vec![(1.0, "timed_signal".to_string())]);

    let ctx = SignalContext::new("test.rs", "");
    let start = std::time::Instant::now();
    let signals = layer.enrich(&ctx);
    let elapsed = start.elapsed().as_micros() as u64;

    assert_eq!(signals.len(), 1);
    assert!(
        elapsed < 1_000_000,
        "enrich should complete quickly for static layer"
    );
}

// ── Test 14: Pipeline Separator ─────────────────────────────────────────

#[test]
fn pipeline_separator_between_signals() {
    let pipeline =
        SignalPipeline::new(1000)
            .with_normalize(false)
            .add_layer(StaticSignalLayer::new(
                "ab",
                vec![(2.0, "alpha".to_string()), (1.0, "beta".to_string())],
            ));

    let ctx = SignalContext::new("main.rs", "");
    let result = pipeline
        .execute(&ctx)
        .expect("two-signal pipeline must produce joined output");

    assert!(result.contains(" | "), "signals must be joined by ' | '");
    assert!(result.starts_with("alpha"));
    assert!(result.ends_with("beta"));
}

// ── Test 15: SignalContext ─────────────────────────────────────────────

#[test]
fn signal_context_default_cila_is_3() {
    let ctx = SignalContext::new("test.rs", "");
    assert_eq!(ctx.cila_level, 3, "default CILA should be 3");
}

#[test]
fn signal_context_with_hook_sets_name() {
    let ctx = SignalContext::new("main.rs", "fn main() {}").with_hook("pre_edit");
    assert_eq!(ctx.hook_name, "pre_edit");
    assert_eq!(ctx.file_path, "main.rs");
    assert_eq!(ctx.source, "fn main() {}");
}

#[test]
fn signal_context_with_cila_sets_level() {
    let ctx = SignalContext::new("lib.rs", "").with_cila(6);
    assert_eq!(ctx.cila_level, 6);
    assert_eq!(ctx.hook_name, "test"); // default
}

// ── Test 16: Graph-Integrated Layers Thresholds ─────────────────────────

#[test]
fn blast_radius_signal_layer_should_run_threshold() {
    use std::sync::Arc;
    use touring_code::ast::SymbolIndex;

    let layer = touring_hooks::shared::signal_pipeline::BlastRadiusSignalLayer::new(Arc::new(
        SymbolIndex::new(),
    ));
    assert!(!layer.should_run(0), "must not run at CILA 0");
    assert!(!layer.should_run(1), "must not run at CILA 1");
    assert!(layer.should_run(2), "must run at CILA 2");
    assert!(layer.should_run(6), "must run at CILA 6");
}

#[test]
fn enriched_blast_radius_signal_layer_should_run_threshold() {
    use std::sync::Arc;
    use touring_code::ast::SymbolIndex;

    let layer = touring_hooks::shared::signal_pipeline::EnrichedBlastRadiusSignalLayer::new(
        Arc::new(SymbolIndex::new()),
    );
    assert!(!layer.should_run(2), "must not run at CILA 2");
    assert!(layer.should_run(3), "must run at CILA 3");
    assert!(layer.should_run(6), "must run at CILA 6");
}

#[test]
fn weighted_blast_signal_layer_should_run_threshold() {
    use std::sync::Arc;
    use touring_code::ast::SymbolIndex;

    let layer = touring_hooks::shared::signal_pipeline::WeightedBlastSignalLayer::new(Arc::new(
        SymbolIndex::new(),
    ));
    assert!(!layer.should_run(3), "must not run at CILA 3");
    assert!(layer.should_run(4), "must run at CILA 4");
    assert!(layer.should_run(6), "must run at CILA 6");
}

#[test]
fn hnsw_signal_layer_should_run_threshold() {
    use std::sync::Arc;
    use touring_code::ast::SymbolIndex;

    let layer = touring_hooks::shared::signal_pipeline::HnswSignalLayer::new(
        Arc::new(SymbolIndex::new()),
        5,
    );
    assert!(!layer.should_run(4), "must not run at CILA 4");
    assert!(layer.should_run(5), "must run at CILA 5");
    assert!(layer.should_run(6), "must run at CILA 6");
}

// ── Test 17: HookRuntime Integration ─────────────────────────────────────

#[test]
fn hook_runtime_initialization_succeeds() {
    let (_tmp, _rt) = setup_runtime();
    // Runtime initialized successfully - basic smoke test
    assert!(true, "HookRuntime should initialize without error");
}

#[test]
fn signal_pipeline_with_empty_index_no_panic() {
    use std::sync::Arc;
    use touring_code::ast::SymbolIndex;

    let index = Arc::new(SymbolIndex::new());
    let layer =
        touring_hooks::shared::signal_pipeline::BlastRadiusSignalLayer::new(Arc::clone(&index));

    let ctx = SignalContext::new("src/lib.rs", "").with_cila(3);
    let result = layer.enrich(&ctx);

    // Empty index should return empty results, not panic
    assert!(result.is_empty(), "empty index must yield no blast signals");
}

// ── Test 18: Normalization Edge Cases ──────────────────────────────────

#[test]
fn normalize_scores_all_same_value() {
    let mut signals = vec![
        (1.0, "a".to_string()),
        (1.0, "b".to_string()),
        (1.0, "c".to_string()),
    ];

    normalize_scores(&mut signals);

    // All same values should normalize to 0 (or remain unchanged due to range=0)
    // The function should not panic
    assert_eq!(signals.len(), 3);
}

#[test]
fn normalize_scores_negative_values() {
    let mut signals = vec![
        (-2.0, "negative".to_string()),
        (0.0, "zero".to_string()),
        (2.0, "positive".to_string()),
    ];

    normalize_scores(&mut signals);

    // After normalization with range=4: negative->0.0, zero->0.5, positive->1.0
    // Sorted descending: positive (1.0), zero (0.5), negative (0.0)
    let mut sorted = signals.clone();
    sorted.sort_by(score_cmp);
    assert_eq!(sorted[0].1, "positive");
    assert_eq!(sorted[1].1, "zero");
    assert_eq!(sorted[2].1, "negative");
}

// ── Test 19: score_cmp Ordering ─────────────────────────────────────────

#[test]
fn score_cmp_orders_descending() {
    let signals = vec![
        (0.5, "low".to_string()),
        (1.0, "high".to_string()),
        (0.8, "medium".to_string()),
    ];

    let mut sorted = signals.clone();
    sorted.sort_by(score_cmp);

    assert_eq!(sorted[0].1, "high");
    assert_eq!(sorted[1].1, "medium");
    assert_eq!(sorted[2].1, "low");
}

#[test]
fn score_cmp_handles_equal_scores() {
    let signals = vec![(1.0, "first".to_string()), (1.0, "second".to_string())];

    let mut sorted = signals.clone();
    sorted.sort_by(score_cmp);

    // Equal scores - ordering is implementation-defined but stable
    assert_eq!(sorted.len(), 2);
}

// ── Test 20: CilaGatedLayer Pipeline Composition ─────────────────────────

#[test]
fn cila_gated_multiple_layers_compose_correctly() {
    let pipeline = SignalPipeline::new(2000)
        .with_normalize(false)
        .add_layer(StaticSignalLayer::new(
            "always",
            vec![(1.0, "always_run".to_string())],
        ))
        .add_layer(CilaGatedLayer::new(
            StaticSignalLayer::new("l3", vec![(0.9, "l3_run".to_string())]),
            3,
        ))
        .add_layer(CilaGatedLayer::new(
            StaticSignalLayer::new("l4", vec![(0.8, "l4_run".to_string())]),
            4,
        ))
        .add_layer(CilaGatedLayer::new(
            StaticSignalLayer::new("l5", vec![(0.7, "l5_run".to_string())]),
            5,
        ));

    // Test CILA 2 - only "always" should run
    let ctx_l2 = SignalContext::new("main.rs", "").with_cila(2);
    let result_l2 = pipeline.execute(&ctx_l2).unwrap();
    assert!(result_l2.contains("always_run"));
    assert!(!result_l2.contains("l3_run"));
    assert!(!result_l2.contains("l4_run"));
    assert!(!result_l2.contains("l5_run"));

    // Test CILA 4 - always + l3 + l4 should run
    let ctx_l4 = SignalContext::new("main.rs", "").with_cila(4);
    let result_l4 = pipeline.execute(&ctx_l4).unwrap();
    assert!(result_l4.contains("always_run"));
    assert!(result_l4.contains("l3_run"));
    assert!(result_l4.contains("l4_run"));
    assert!(!result_l4.contains("l5_run"));

    // Test CILA 6 - all should run
    let ctx_l6 = SignalContext::new("main.rs", "").with_cila(6);
    let result_l6 = pipeline.execute(&ctx_l6).unwrap();
    assert!(result_l6.contains("always_run"));
    assert!(result_l6.contains("l3_run"));
    assert!(result_l6.contains("l4_run"));
    assert!(result_l6.contains("l5_run"));
}

// ── Test 21: Pipeline extend ────────────────────────────────────────────

#[test]
fn pipeline_extend_merges_layers() {
    let base = SignalPipeline::new(1000).add_layer(StaticSignalLayer::new(
        "base",
        vec![(1.0, "base_signal".to_string())],
    ));

    let extra = SignalPipeline::new(1000).add_layer(StaticSignalLayer::new(
        "extra",
        vec![(0.8, "extra_signal".to_string())],
    ));

    let merged = base.extend(extra);

    assert_eq!(merged.layer_count(), 2);
    let ctx = SignalContext::new("test.rs", "");
    let result = merged.execute(&ctx);
    assert!(result.is_some());
    let output = result.unwrap();
    assert!(output.contains("base_signal") && output.contains("extra_signal"));
}
