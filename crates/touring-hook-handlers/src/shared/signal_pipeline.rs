//! Tower-style Signal Pipeline — composable, testable signal enrichment.
//!
//! # Architecture
//!
//! Each signal enricher implements [`SignalLayer`], producing scored
//! `(f32, String)` tuples. Layers are composed into a [`SignalPipeline`]
//! that executes them in order, collects results, normalizes scores,
//! and assembles budget-limited output.
//!
//! ```text
//! SignalPipeline
//!   ├── Layer 1: DependentsSignal     → Vec<(f32, String)>
//!   ├── Layer 2: GotchaSignal         → Vec<(f32, String)>
//!   ├── Layer 3: BlastRadiusSignal    → Vec<(f32, String)>
//!   ├── Layer 4: CognitiveSignal      → Vec<(f32, String)>
//!   └── ... N layers
//!   │
//!   ▼
//!   normalize_scores() → sort_by(score_cmp) → budget truncation → String
//! ```
//!
//! # Benefits over ad-hoc Vec accumulation
//!
//! - **Testable**: Each layer can be tested independently with mock context.
//! - **Composable**: Add/remove layers without touching other code.
//! - **Observable**: Per-layer timing and signal count metrics.
//! - **Budget-aware**: Truncation applied as a final step, not scattered.

use super::signals::{normalize_scores, score_cmp};
use std::sync::Arc;
use touring_analysis::{AnalysisConfig, BlastRadiusEngine, BlastRadiusStrategy, HnswStrategy};
use touring_code::ast::graph::pheromone::PheromoneGraph;
use touring_code::ast::{SymbolIndex, compute_enriched_blast_radius};

// ─── Types ──────────────────────────────────────────────────────────────

// Session B F4-pre (2026-06-10): the signal-layer vocabulary (SignalContext /
// LayerMetrics / SignalLayer) moved to `touring-hooks-shared::signal_layer` so
// leaf-side producers (e.g. `ast_grep_signal`) can implement the trait without
// reaching back into this crate. Re-exported here so every historical
// `crate::shared::signal_pipeline::{SignalContext, SignalLayer}` path — and the
// `impl SignalLayer` blocks below — keep resolving unchanged.
pub use touring_hooks_shared::signal_layer::{LayerMetrics, SignalContext, SignalLayer};

// ─── Pipeline ───────────────────────────────────────────────────────────

/// Composable signal pipeline that executes layers and assembles output.
pub struct SignalPipeline {
    layers: Vec<Box<dyn SignalLayer>>,
    /// Maximum output size in chars.
    budget: usize,
    /// Whether to normalize scores before assembly.
    normalize: bool,
}

impl SignalPipeline {
    /// Create an empty pipeline with the given budget.
    pub fn new(budget: usize) -> Self {
        Self {
            layers: Vec::new(),
            budget,
            normalize: true,
        }
    }

    /// Add a layer to the pipeline.
    pub fn add_layer(mut self, layer: impl SignalLayer + 'static) -> Self {
        self.layers.push(Box::new(layer));
        self
    }

    /// Set whether to normalize scores before assembly.
    pub fn with_normalize(mut self, normalize: bool) -> Self {
        self.normalize = normalize;
        self
    }

    /// Set the output budget.
    pub fn with_budget(mut self, budget: usize) -> Self {
        self.budget = budget;
        self
    }

    /// Merge all layers from `other` into this pipeline.
    ///
    /// Moves layers from `other` in order, preserving insertion sequence.
    /// Used by callers that build a base pipeline and then extend it with
    /// a pre-configured pipeline (e.g., `build_graph_pipeline`).
    pub fn extend(mut self, other: SignalPipeline) -> Self {
        self.layers.extend(other.layers);
        self
    }

    /// Execute all layers and assemble scored output.
    ///
    /// Returns `None` if no signals were produced.
    pub fn execute(&self, ctx: &SignalContext<'_>) -> Option<String> {
        let mut all_signals: Vec<(f32, String)> = Vec::with_capacity(self.layers.len() * 2);
        let mut _metrics: Vec<LayerMetrics> = Vec::with_capacity(self.layers.len());

        for layer in &self.layers {
            if !layer.should_run(ctx.cila_level) {
                continue;
            }

            let start = std::time::Instant::now();
            let signals = layer.enrich(ctx);
            let duration_us = start.elapsed().as_micros() as u64;

            _metrics.push(LayerMetrics {
                name: layer.name(),
                signal_count: signals.len(),
                duration_us,
            });

            all_signals.extend(signals);
        }

        if all_signals.is_empty() {
            return None;
        }

        // Normalize if enabled
        if self.normalize {
            normalize_scores(&mut all_signals);
        }

        // Sort by score descending
        all_signals.sort_by(score_cmp);

        // P1 (SNR gating, default OFF): prune low-relevance signals before assembly.
        super::signals::apply_relevance_cutoff(&mut all_signals);

        // Budget-aware assembly
        let mut output = String::new();
        for (_, text) in &all_signals {
            if output.len() + text.len() > self.budget && !output.is_empty() {
                break;
            }
            if !output.is_empty() {
                output.push_str(" | ");
            }
            output.push_str(text);
        }

        if output.is_empty() {
            None
        } else {
            Some(output)
        }
    }

    /// Execute and return metrics alongside the output.
    pub fn execute_with_metrics(
        &self,
        ctx: &SignalContext<'_>,
    ) -> (Option<String>, Vec<LayerMetrics>) {
        let mut all_signals: Vec<(f32, String)> = Vec::with_capacity(self.layers.len() * 2);
        let mut metrics: Vec<LayerMetrics> = Vec::with_capacity(self.layers.len());

        for layer in &self.layers {
            if !layer.should_run(ctx.cila_level) {
                continue;
            }

            let start = std::time::Instant::now();
            let signals = layer.enrich(ctx);
            let duration_us = start.elapsed().as_micros() as u64;

            metrics.push(LayerMetrics {
                name: layer.name(),
                signal_count: signals.len(),
                duration_us,
            });

            all_signals.extend(signals);
        }

        if all_signals.is_empty() {
            return (None, metrics);
        }

        if self.normalize {
            normalize_scores(&mut all_signals);
        }
        all_signals.sort_by(score_cmp);

        // P1 (SNR gating, default OFF): prune low-relevance signals before assembly.
        super::signals::apply_relevance_cutoff(&mut all_signals);

        let mut output = String::new();
        for (_, text) in &all_signals {
            if output.len() + text.len() > self.budget && !output.is_empty() {
                break;
            }
            if !output.is_empty() {
                output.push_str(" | ");
            }
            output.push_str(text);
        }

        let result = if output.is_empty() {
            None
        } else {
            Some(output)
        };
        (result, metrics)
    }

    /// Number of registered layers.
    pub fn layer_count(&self) -> usize {
        self.layers.len()
    }
}

// ─── Built-in Layers ────────────────────────────────────────────────────

/// A simple layer that always produces a fixed signal (useful for testing).
pub struct StaticSignalLayer {
    layer_name: &'static str,
    signals: Vec<(f32, String)>,
}

impl StaticSignalLayer {
    /// Create a layer that always produces the given signals.
    pub fn new(name: &'static str, signals: Vec<(f32, String)>) -> Self {
        Self {
            layer_name: name,
            signals,
        }
    }
}

impl SignalLayer for StaticSignalLayer {
    fn name(&self) -> &'static str {
        self.layer_name
    }
    fn enrich(&self, _ctx: &SignalContext<'_>) -> Vec<(f32, String)> {
        self.signals.clone()
    }
}

/// A layer that only runs at CILA level >= threshold.
pub struct CilaGatedLayer<L: SignalLayer> {
    inner: L,
    min_cila: usize,
}

impl<L: SignalLayer> CilaGatedLayer<L> {
    /// Wrap a layer to only run at CILA level >= min_cila.
    pub fn new(inner: L, min_cila: usize) -> Self {
        Self { inner, min_cila }
    }
}

impl<L: SignalLayer> SignalLayer for CilaGatedLayer<L> {
    fn name(&self) -> &'static str {
        self.inner.name()
    }
    fn enrich(&self, ctx: &SignalContext<'_>) -> Vec<(f32, String)> {
        self.inner.enrich(ctx)
    }
    fn should_run(&self, cila_level: usize) -> bool {
        cila_level >= self.min_cila
    }
}

/// A layer backed by a closure (for quick prototyping).
pub struct FnSignalLayer<F>
where
    F: Fn(&SignalContext<'_>) -> Vec<(f32, String)> + Send + Sync,
{
    layer_name: &'static str,
    f: F,
}

impl<F> FnSignalLayer<F>
where
    F: Fn(&SignalContext<'_>) -> Vec<(f32, String)> + Send + Sync,
{
    /// Create a layer from a closure.
    pub fn new(name: &'static str, f: F) -> Self {
        Self {
            layer_name: name,
            f,
        }
    }
}

impl<F> SignalLayer for FnSignalLayer<F>
where
    F: Fn(&SignalContext<'_>) -> Vec<(f32, String)> + Send + Sync,
{
    fn name(&self) -> &'static str {
        self.layer_name
    }
    fn enrich(&self, ctx: &SignalContext<'_>) -> Vec<(f32, String)> {
        (self.f)(ctx)
    }
}

// ─── Pipeline Factory ───────────────────────────────────────────────────

/// Build a [`SignalPipeline`] pre-configured with all 5 graph-integrated layers.
///
/// Convenience constructor for composing the full analysis stack:
///
/// | Layer | Type | CILA gate | Cost |
/// |---|---|---|---|
/// | BFS blast radius | [`BlastRadiusSignalLayer`] | ≥ 2 | ~8ms (budget-capped) |
/// | Enriched blast radius | [`EnrichedBlastRadiusSignalLayer`] | ≥ 3 | ~2ms |
/// | Weighted Dijkstra blast | [`WeightedBlastSignalLayer`] | ≥ 4 | ~5ms |
/// | ACO pheromone hot-edges | [`PheromoneGraphSignalLayer`] | ≥ 2 | <1ms |
/// | HNSW approximate NN | [`HnswSignalLayer`] | ≥ 5 | ~50ms (cold start, cached) |
///
/// The HNSW layer is wrapped in [`CilaGatedLayer`] to prevent cold-start cost
/// on hooks where `cila_level < 5`. All other layers use their built-in
/// [`SignalLayer::should_run`] gate.
///
/// # Example
/// ```ignore
/// use std::sync::{Arc, RwLock};
/// use touring_code::ast::{SymbolIndex, graph::pheromone::PheromoneGraph};
/// use crate::shared::signal_pipeline::build_graph_pipeline;
///
/// let pipeline = build_graph_pipeline(
///     Arc::new(SymbolIndex::new()),
///     Arc::new(RwLock::new(PheromoneGraph::new(0.1))),
///     3200,
/// );
/// let ctx = SignalContext::new("src/lib.rs", "fn main() {}").with_cila(4);
/// if let Some(output) = pipeline.execute(&ctx) {
///     // output contains ranked, budget-truncated graph signals
/// }
/// ```
pub fn build_graph_pipeline(
    index: Arc<SymbolIndex>,
    graph: Arc<std::sync::RwLock<PheromoneGraph>>,
    budget: usize,
) -> SignalPipeline {
    SignalPipeline::new(budget)
        // CILA>=2: BFS blast radius — fast hop-distance impact via BlastRadiusEngine
        .add_layer(CilaGatedLayer::new(
            BlastRadiusSignalLayer::new(Arc::clone(&index)),
            2,
        ))
        // CILA>=3: enriched blast radius (exact hop distances per affected file)
        .add_layer(CilaGatedLayer::new(
            EnrichedBlastRadiusSignalLayer::new(Arc::clone(&index)),
            3,
        ))
        // CILA>=4: weighted Dijkstra blast (edge-weight-aware impact ranking)
        .add_layer(CilaGatedLayer::new(
            WeightedBlastSignalLayer::new(Arc::clone(&index)),
            4,
        ))
        // CILA>=2: ACO pheromone hot-edges (co-access trail signals)
        .add_layer(CilaGatedLayer::new(
            PheromoneGraphSignalLayer::new(Arc::clone(&graph)),
            2,
        ))
        // CILA>=5: HNSW approximate nearest-neighbor (cold-start cached)
        .add_layer(CilaGatedLayer::new(
            HnswSignalLayer::new(Arc::clone(&index), 10),
            5,
        ))
}

// ─── Graph-Integrated Layers ────────────────────────────────────────────

/// BFS blast radius layer — exact hop-distance impact via [`BlastRadiusEngine`].
///
/// Produces one scored signal per affected file: `score = 1.0 / (1.0 + distance)`.
/// Uses an 8 ms hook budget via [`AnalysisConfig::hook_path`] to stay within the
/// pre-edit latency envelope.
///
/// Only runs at CILA level ≥ 2. Recommended usage with [`CilaGatedLayer`]:
/// ```ignore
/// pipeline.add_layer(CilaGatedLayer::new(BlastRadiusSignalLayer::new(Arc::clone(&idx)), 2))
/// ```
pub struct BlastRadiusSignalLayer {
    index: Arc<SymbolIndex>,
}

impl BlastRadiusSignalLayer {
    /// Create a new layer backed by the given symbol index.
    pub fn new(index: Arc<SymbolIndex>) -> Self {
        Self { index }
    }
}

impl SignalLayer for BlastRadiusSignalLayer {
    fn name(&self) -> &'static str {
        "blast_radius_bfs"
    }

    fn enrich(&self, ctx: &SignalContext<'_>) -> Vec<(f32, String)> {
        let engine = BlastRadiusEngine::bfs_only(Arc::clone(&self.index));
        let config = AnalysisConfig::hook_path().with_budget(8);
        let result = engine.compute(ctx.file_path, &config);
        result
            .affected_files
            .into_iter()
            .map(|af| {
                let score = 1.0_f32 / (1.0 + af.distance as f32);
                (score, format!("blast:{} (hop {})", af.path, af.distance))
            })
            .collect()
    }

    fn should_run(&self, cila_level: usize) -> bool {
        cila_level >= 2
    }
}

/// Enriched blast radius layer — direct, transitive, and co-edited dependents.
///
/// Uses [`compute_enriched_blast_radius`] for semantic categorisation of impact.
/// Severity score (0–1) is propagated from the enriched result and discounted by
/// dependency kind: direct=100%, transitive=50%, co-edited=30%.
///
/// Only runs at CILA level ≥ 3.
pub struct EnrichedBlastRadiusSignalLayer {
    index: Arc<SymbolIndex>,
}

impl EnrichedBlastRadiusSignalLayer {
    /// Create a new layer backed by the given symbol index.
    pub fn new(index: Arc<SymbolIndex>) -> Self {
        Self { index }
    }
}

impl SignalLayer for EnrichedBlastRadiusSignalLayer {
    fn name(&self) -> &'static str {
        "blast_radius_enriched"
    }

    fn enrich(&self, ctx: &SignalContext<'_>) -> Vec<(f32, String)> {
        // Wave 12 fix (REGRA #0): touring-ast::compute_enriched_blast_radius now
        // takes &IndexMap (was &HashMap pre-Wave). Pre-existing inter-crate drift
        // surfaced by `update-touring` build. The empty default carries no
        // co-edit signals; behaviour preserved.
        let co_edit: indexmap::IndexMap<String, Vec<String>> = Default::default();
        let enriched = compute_enriched_blast_radius(&self.index, ctx.file_path, &co_edit);
        let severity = enriched.severity as f32;
        let mut signals = Vec::new();
        for dep in &enriched.direct_dependents {
            signals.push((severity, format!("direct:{dep}")));
        }
        for dep in &enriched.transitive_dependents {
            signals.push((severity * 0.5, format!("transitive:{dep}")));
        }
        for co in &enriched.co_edited_files {
            signals.push((severity * 0.3, format!("co_edit:{co}")));
        }
        signals
    }

    fn should_run(&self, cila_level: usize) -> bool {
        cila_level >= 3
    }
}

/// Weighted blast radius layer — Dijkstra co-edit-weighted dependency distance.
///
/// Uses [`SymbolIndex::weighted_blast_radius`] for O(V log V) Dijkstra traversal
/// with co-edit-history-weighted edges (`w = 1.0 / (1.0 + co_edit_weight)`).
/// Score = `1.0 / (1.0 + cost)`. The start file (cost = 0.0) is filtered out.
///
/// Only runs at CILA level ≥ 4.
pub struct WeightedBlastSignalLayer {
    index: Arc<SymbolIndex>,
}

impl WeightedBlastSignalLayer {
    /// Create a new layer backed by the given symbol index.
    pub fn new(index: Arc<SymbolIndex>) -> Self {
        Self { index }
    }
}

impl SignalLayer for WeightedBlastSignalLayer {
    fn name(&self) -> &'static str {
        "blast_radius_weighted"
    }

    fn enrich(&self, ctx: &SignalContext<'_>) -> Vec<(f32, String)> {
        self.index
            .weighted_blast_radius(ctx.file_path)
            .into_iter()
            .filter(|(_, cost)| *cost > 0.0)
            .map(|(path, cost)| {
                let score = 1.0_f32 / (1.0 + cost as f32);
                (score, format!("weighted:{path} (cost {cost:.2})"))
            })
            .collect()
    }

    fn should_run(&self, cila_level: usize) -> bool {
        cila_level >= 4
    }
}

/// Pheromone hot-edges layer — top ACO trail signals from [`PheromoneGraph`].
///
/// Surfaces the top 5 most-reinforced file-to-file paths as scored signals.
/// Score is normalised relative to the strongest pheromone trail in the graph.
/// The [`PheromoneGraph`] is shared via `Arc<RwLock<_>>` for composability with
/// the hook runtime's pheromone state.
///
/// `hot_edges` returns `&str` slices that borrow from the graph's internal
/// `HashMap`; they are cloned to owned `String`s before releasing the read lock.
///
/// Only runs at CILA level ≥ 2.
pub struct PheromoneGraphSignalLayer {
    graph: Arc<std::sync::RwLock<PheromoneGraph>>,
}

impl PheromoneGraphSignalLayer {
    /// Create a new layer backed by the given shared pheromone graph.
    pub fn new(graph: Arc<std::sync::RwLock<PheromoneGraph>>) -> Self {
        Self { graph }
    }
}

impl SignalLayer for PheromoneGraphSignalLayer {
    fn name(&self) -> &'static str {
        "pheromone_hot_edges"
    }

    fn enrich(&self, _ctx: &SignalContext<'_>) -> Vec<(f32, String)> {
        // Clone edges before releasing the lock — hot_edges returns &str
        // borrows from internal HashMap keys that must not escape the guard.
        let edges: Vec<(f64, String, String)> = match self.graph.read() {
            Ok(guard) => guard
                .hot_edges(5)
                .into_iter()
                .map(|(s, from, to)| (s, from.to_owned(), to.to_owned()))
                .collect(),
            Err(_) => return vec![],
        };
        let max_strength = edges.iter().map(|(s, _, _)| *s).fold(0.0_f64, f64::max);
        if max_strength == 0.0 {
            return vec![];
        }
        edges
            .into_iter()
            .map(|(strength, from, to)| {
                let score = (strength / max_strength) as f32;
                (score, format!("pheromone:{from} \u{2192} {to}"))
            })
            .collect()
    }

    fn should_run(&self, cila_level: usize) -> bool {
        cila_level >= 2
    }
}

/// HNSW approximate nearest-neighbour blast radius layer.
///
/// Uses [`HnswStrategy`] with a lazy singleton cache: the first `enrich()` call
/// builds the HNSW index (O(n log n) over all files in the symbol index);
/// subsequent calls reuse it via [`std::sync::OnceLock`].
///
/// Only runs at CILA level ≥ 5. This layer is expensive to initialise — always
/// wrap in [`CilaGatedLayer`] to avoid cold-start cost on lower CILA levels:
/// ```ignore
/// pipeline.add_layer(CilaGatedLayer::new(HnswSignalLayer::new(Arc::clone(&idx), 10), 5))
/// ```
pub struct HnswSignalLayer {
    index: Arc<SymbolIndex>,
    cache: Arc<std::sync::OnceLock<HnswStrategy>>,
    k: usize,
}

impl HnswSignalLayer {
    /// Create a new layer. `k` is the HNSW neighbour count (typical: 5–20).
    pub fn new(index: Arc<SymbolIndex>, k: usize) -> Self {
        Self {
            index,
            cache: Arc::new(std::sync::OnceLock::new()),
            k,
        }
    }
}

impl SignalLayer for HnswSignalLayer {
    fn name(&self) -> &'static str {
        "blast_radius_hnsw"
    }

    fn enrich(&self, ctx: &SignalContext<'_>) -> Vec<(f32, String)> {
        let strategy = self
            .cache
            .get_or_init(|| HnswStrategy::new(self.index.as_ref(), self.k));
        let config = AnalysisConfig::hook_path();
        let result = strategy.compute(ctx.file_path, &config);
        result
            .affected_files
            .into_iter()
            .map(|af| {
                let score = 1.0_f32 / (1.0 + af.distance as f32);
                (score, format!("hnsw:{} (hop {})", af.path, af.distance))
            })
            .collect()
    }

    fn should_run(&self, cila_level: usize) -> bool {
        cila_level >= 5
    }
}

// ─── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_empty_pipeline_returns_none() {
        let pipeline = SignalPipeline::new(1000);
        let ctx = SignalContext::new("test.rs", "fn main() {}");
        assert!(pipeline.execute(&ctx).is_none());
    }

    #[test]
    fn test_single_layer_produces_output() {
        let pipeline = SignalPipeline::new(1000).add_layer(StaticSignalLayer::new(
            "test",
            vec![(1.0, "signal one".to_string())],
        ));
        let ctx = SignalContext::new("test.rs", "");
        let result = pipeline.execute(&ctx);
        assert!(result.is_some());
        assert!(
            result
                .expect("static layer must produce signal")
                .contains("signal one")
        );
    }

    #[test]
    fn test_multiple_layers_sorted_by_score() {
        let pipeline = SignalPipeline::new(1000)
            .with_normalize(false)
            .add_layer(StaticSignalLayer::new(
                "low",
                vec![(0.5, "low priority".to_string())],
            ))
            .add_layer(StaticSignalLayer::new(
                "high",
                vec![(2.0, "high priority".to_string())],
            ));
        let ctx = SignalContext::new("test.rs", "");
        let result = pipeline
            .execute(&ctx)
            .expect("multi-layer pipeline must produce output");
        // High priority should come first
        assert!(result.starts_with("high priority"));
    }

    #[test]
    fn test_budget_truncation() {
        let pipeline =
            SignalPipeline::new(30)
                .with_normalize(false)
                .add_layer(StaticSignalLayer::new(
                    "a",
                    vec![
                        (2.0, "short".to_string()), // 5 chars
                        (
                            1.0,
                            "this is a much longer signal that exceeds budget".to_string(),
                        ),
                    ],
                ));
        let ctx = SignalContext::new("test.rs", "");
        let result = pipeline
            .execute(&ctx)
            .expect("budget pipeline must produce output");
        assert!(result.contains("short"));
        // The long signal should be truncated by budget
        assert!(result.len() <= 60); // short + separator + maybe long
    }

    #[test]
    fn test_cila_gated_layer() {
        let inner =
            StaticSignalLayer::new("expensive", vec![(1.0, "expensive signal".to_string())]);
        let gated = CilaGatedLayer::new(inner, 3);

        // Should NOT run at CILA L0
        let pipeline_low = SignalPipeline::new(1000).add_layer(StaticSignalLayer::new(
            "always",
            vec![(1.0, "always here".to_string())],
        ));
        let ctx_low = SignalContext::new("test.rs", "").with_cila(0);
        assert!(!gated.should_run(ctx_low.cila_level));

        // Should run at CILA L3+
        let ctx_high = SignalContext::new("test.rs", "").with_cila(3);
        assert!(gated.should_run(ctx_high.cila_level));

        let _ = pipeline_low; // suppress unused warning
    }

    #[test]
    fn test_fn_signal_layer() {
        let layer = FnSignalLayer::new("custom", |ctx: &SignalContext<'_>| {
            if ctx.file_path.ends_with(".rs") {
                vec![(1.0, "rust file detected".to_string())]
            } else {
                vec![]
            }
        });

        let ctx_rs = SignalContext::new("main.rs", "");
        assert_eq!(layer.enrich(&ctx_rs).len(), 1);

        let ctx_py = SignalContext::new("main.py", "");
        assert_eq!(layer.enrich(&ctx_py).len(), 0);
    }

    #[test]
    fn test_execute_with_metrics() {
        let pipeline = SignalPipeline::new(1000).add_layer(StaticSignalLayer::new(
            "fast",
            vec![(1.0, "signal".to_string())],
        ));
        let ctx = SignalContext::new("test.rs", "");
        let (result, metrics) = pipeline.execute_with_metrics(&ctx);
        assert!(result.is_some());
        assert_eq!(metrics.len(), 1);
        assert_eq!(metrics[0].name, "fast");
        assert_eq!(metrics[0].signal_count, 1);
    }

    #[test]
    fn test_normalization_enabled() {
        let pipeline =
            SignalPipeline::new(1000)
                .with_normalize(true)
                .add_layer(StaticSignalLayer::new(
                    "a",
                    vec![(100.0, "high".to_string()), (1.0, "low".to_string())],
                ));
        let ctx = SignalContext::new("test.rs", "");
        // Should still work — normalization brings to [0,1]
        let result = pipeline.execute(&ctx);
        assert!(result.is_some());
    }

    #[test]
    fn test_layer_count() {
        let pipeline = SignalPipeline::new(1000)
            .add_layer(StaticSignalLayer::new("a", vec![]))
            .add_layer(StaticSignalLayer::new("b", vec![]))
            .add_layer(StaticSignalLayer::new("c", vec![]));
        assert_eq!(pipeline.layer_count(), 3);
    }

    #[test]
    fn test_signal_context_with_hook_sets_hook_name() {
        let ctx = SignalContext::new("main.rs", "fn foo() {}").with_hook("pre_edit");
        assert_eq!(ctx.hook_name, "pre_edit");
        assert_eq!(ctx.file_path, "main.rs");
    }

    #[test]
    fn test_signal_context_with_cila_sets_level() {
        let ctx = SignalContext::new("lib.rs", "").with_cila(6);
        assert_eq!(ctx.cila_level, 6);
    }

    #[test]
    fn test_fn_layer_accesses_source_and_hook_name() {
        let layer = FnSignalLayer::new("context_aware", |ctx: &SignalContext<'_>| {
            let mut sigs = Vec::new();
            if !ctx.source.is_empty() {
                sigs.push((0.8, format!("source_len:{}", ctx.source.len())));
            }
            if ctx.hook_name == "pre_edit" {
                sigs.push((0.5, "pre_edit_hook".to_string()));
            }
            sigs
        });

        let ctx = SignalContext::new("main.rs", "fn main() {}").with_hook("pre_edit");
        let signals = layer.enrich(&ctx);
        assert_eq!(signals.len(), 2);
        assert!(signals[0].1.starts_with("source_len:"));
        assert_eq!(signals[1].1, "pre_edit_hook");
    }

    #[test]
    fn test_cila_gated_layer_skipped_in_pipeline() {
        // Pipeline with a gated layer at min_cila=4 — context at L2 should skip it.
        // Only the ungated static layer should contribute signals.
        let pipeline = SignalPipeline::new(1000)
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
            "gated layer must be skipped at L2"
        );
    }

    #[test]
    fn test_cila_gated_layer_runs_in_pipeline_at_threshold() {
        let pipeline =
            SignalPipeline::new(1000)
                .with_normalize(false)
                .add_layer(CilaGatedLayer::new(
                    StaticSignalLayer::new("expensive", vec![(2.0, "gated_signal".to_string())]),
                    4,
                ));

        let ctx_high = SignalContext::new("main.rs", "").with_cila(4);
        let result = pipeline.execute(&ctx_high);
        assert!(
            result.is_some(),
            "gated layer should run at min_cila threshold"
        );
        assert!(
            result
                .expect("gated layer must produce signal at threshold")
                .contains("gated_signal")
        );
    }

    #[test]
    fn test_execute_with_metrics_skipped_layers_not_counted() {
        // Layers that don't run (should_run=false) must not appear in metrics.
        let pipeline = SignalPipeline::new(1000)
            .add_layer(StaticSignalLayer::new(
                "always",
                vec![(1.0, "present".to_string())],
            ))
            .add_layer(CilaGatedLayer::new(
                StaticSignalLayer::new("skipped", vec![(2.0, "absent".to_string())]),
                5, // min_cila=5
            ));

        let ctx = SignalContext::new("main.rs", "").with_cila(0);
        let (result, metrics) = pipeline.execute_with_metrics(&ctx);
        // Only the non-gated layer ran
        assert_eq!(metrics.len(), 1, "only one layer should appear in metrics");
        assert_eq!(metrics[0].name, "always");
        assert!(result.is_some());
        assert!(
            result
                .expect("non-gated layer must produce present signal")
                .contains("present")
        );
    }

    #[test]
    fn test_execute_with_metrics_all_layers_skipped_returns_none() {
        let pipeline = SignalPipeline::new(1000).add_layer(CilaGatedLayer::new(
            StaticSignalLayer::new("gated", vec![(1.0, "signal".to_string())]),
            6, // min_cila=6
        ));

        let ctx = SignalContext::new("main.rs", "").with_cila(0);
        let (result, metrics) = pipeline.execute_with_metrics(&ctx);
        assert!(result.is_none(), "no layers ran — must return None");
        assert!(metrics.is_empty(), "no layers ran — metrics must be empty");
    }

    #[test]
    fn test_budget_first_signal_exactly_fits() {
        // Signal text is exactly budget chars — should be included with no truncation.
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
    fn test_pipeline_separator_between_signals() {
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

    // ── Graph-Integrated Layer Tests ──────────────────────────────────────────

    #[test]
    fn test_blast_radius_layer_empty_index_returns_empty() {
        use std::sync::Arc;
        use touring_code::ast::SymbolIndex;
        let layer = BlastRadiusSignalLayer::new(Arc::new(SymbolIndex::new()));
        let ctx = SignalContext::new("src/lib.rs", "").with_cila(3);
        assert!(
            layer.enrich(&ctx).is_empty(),
            "empty index must yield no blast signals"
        );
    }

    #[test]
    fn test_blast_radius_layer_should_run_threshold() {
        use std::sync::Arc;
        use touring_code::ast::SymbolIndex;
        let layer = BlastRadiusSignalLayer::new(Arc::new(SymbolIndex::new()));
        assert!(!layer.should_run(0), "must not run at CILA 0");
        assert!(!layer.should_run(1), "must not run at CILA 1");
        assert!(layer.should_run(2), "must run at CILA 2");
        assert!(layer.should_run(6), "must run at CILA 6");
    }

    #[test]
    fn test_enriched_blast_radius_layer_empty_index() {
        use std::sync::Arc;
        use touring_code::ast::SymbolIndex;
        let layer = EnrichedBlastRadiusSignalLayer::new(Arc::new(SymbolIndex::new()));
        let ctx = SignalContext::new("src/lib.rs", "").with_cila(4);
        assert!(
            layer.enrich(&ctx).is_empty(),
            "empty index must yield no enriched signals"
        );
    }

    #[test]
    fn test_enriched_blast_radius_layer_should_run_threshold() {
        use std::sync::Arc;
        use touring_code::ast::SymbolIndex;
        let layer = EnrichedBlastRadiusSignalLayer::new(Arc::new(SymbolIndex::new()));
        assert!(!layer.should_run(2), "must not run at CILA 2");
        assert!(layer.should_run(3), "must run at CILA 3");
        assert!(layer.should_run(6), "must run at CILA 6");
    }

    #[test]
    fn test_weighted_blast_layer_empty_index_returns_empty() {
        use std::sync::Arc;
        use touring_code::ast::SymbolIndex;
        let layer = WeightedBlastSignalLayer::new(Arc::new(SymbolIndex::new()));
        let ctx = SignalContext::new("src/main.rs", "").with_cila(5);
        assert!(
            layer.enrich(&ctx).is_empty(),
            "empty index must yield no weighted signals"
        );
    }

    #[test]
    fn test_weighted_blast_layer_should_run_threshold() {
        use std::sync::Arc;
        use touring_code::ast::SymbolIndex;
        let layer = WeightedBlastSignalLayer::new(Arc::new(SymbolIndex::new()));
        assert!(!layer.should_run(3), "must not run at CILA 3");
        assert!(layer.should_run(4), "must run at CILA 4");
        assert!(layer.should_run(6), "must run at CILA 6");
    }

    #[test]
    fn test_pheromone_layer_empty_graph_returns_empty() {
        use std::sync::{Arc, RwLock};
        use touring_code::ast::graph::pheromone::PheromoneGraph;
        let graph = Arc::new(RwLock::new(PheromoneGraph::new(0.1)));
        let layer = PheromoneGraphSignalLayer::new(graph);
        let ctx = SignalContext::new("src/lib.rs", "");
        assert!(
            layer.enrich(&ctx).is_empty(),
            "empty graph must produce no pheromone signals"
        );
    }

    #[test]
    fn test_pheromone_layer_hot_edges_format() {
        use std::sync::{Arc, RwLock};
        use touring_code::ast::graph::pheromone::PheromoneGraph;
        let mut pg = PheromoneGraph::new(0.1);
        pg.reinforce_path(&["a.rs", "b.rs", "c.rs"]);
        let layer = PheromoneGraphSignalLayer::new(Arc::new(RwLock::new(pg)));
        let ctx = SignalContext::new("a.rs", "");
        let signals = layer.enrich(&ctx);
        // reinforce_path deposits on consecutive pairs: a→b and b→c
        assert_eq!(signals.len(), 2, "two edges expected: a→b, b→c");
        for (score, label) in &signals {
            assert!(*score > 0.0 && *score <= 1.0, "score must be in (0, 1]");
            assert!(
                label.contains(" \u{2192} "),
                "label must use ' → ' separator"
            );
        }
    }

    #[test]
    fn test_pheromone_layer_should_run_threshold() {
        use std::sync::{Arc, RwLock};
        use touring_code::ast::graph::pheromone::PheromoneGraph;
        let layer = PheromoneGraphSignalLayer::new(Arc::new(RwLock::new(PheromoneGraph::new(0.1))));
        assert!(!layer.should_run(1), "must not run at CILA 1");
        assert!(layer.should_run(2), "must run at CILA 2");
    }

    #[test]
    fn test_hnsw_layer_empty_index_no_panic() {
        use std::sync::Arc;
        use touring_code::ast::SymbolIndex;
        let layer = HnswSignalLayer::new(Arc::new(SymbolIndex::new()), 5);
        let ctx = SignalContext::new("src/lib.rs", "").with_cila(6);
        // Must not panic on empty index; result is empty but well-defined
        let _ = layer.enrich(&ctx);
    }

    #[test]
    fn test_hnsw_layer_should_run_threshold() {
        use std::sync::Arc;
        use touring_code::ast::SymbolIndex;
        let layer = HnswSignalLayer::new(Arc::new(SymbolIndex::new()), 5);
        assert!(!layer.should_run(4), "must not run at CILA 4");
        assert!(layer.should_run(5), "must run at CILA 5");
        assert!(layer.should_run(6), "must run at CILA 6");
    }

    #[test]
    fn test_hnsw_layer_cache_reused_on_second_call() {
        use std::sync::Arc;
        use touring_code::ast::SymbolIndex;
        let layer = HnswSignalLayer::new(Arc::new(SymbolIndex::new()), 3);
        let ctx = SignalContext::new("x.rs", "");
        // First call builds HNSW index via OnceLock::get_or_init
        let _ = layer.enrich(&ctx);
        // Second call must reuse the cache — no panic, no deadlock
        let _ = layer.enrich(&ctx);
    }

    #[test]
    fn test_all_graph_layers_compose_in_pipeline() {
        use std::sync::{Arc, RwLock};
        use touring_code::ast::SymbolIndex;
        use touring_code::ast::graph::pheromone::PheromoneGraph;
        let index = Arc::new(SymbolIndex::new());
        let graph = Arc::new(RwLock::new(PheromoneGraph::new(0.1)));
        let pipeline = SignalPipeline::new(2000)
            .add_layer(BlastRadiusSignalLayer::new(Arc::clone(&index)))
            .add_layer(EnrichedBlastRadiusSignalLayer::new(Arc::clone(&index)))
            .add_layer(WeightedBlastSignalLayer::new(Arc::clone(&index)))
            .add_layer(PheromoneGraphSignalLayer::new(Arc::clone(&graph)))
            .add_layer(HnswSignalLayer::new(Arc::clone(&index), 5));
        assert_eq!(
            pipeline.layer_count(),
            5,
            "all 5 graph layers must register"
        );
        let ctx = SignalContext::new("src/lib.rs", "fn main() {}").with_cila(6);
        // Empty index + empty graph → no signals produced; must not panic
        assert!(pipeline.execute(&ctx).is_none());
    }
}
