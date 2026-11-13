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

use super::signals::{
    blast_aggregate_signal, cut_to_chars, normalize_scores, score_cmp, short_path_label,
};
use std::sync::{Arc, OnceLock};
use touring_analysis::{AnalysisConfig, BlastRadiusStrategy, HnswStrategy};
use touring_code::ast::graph::pheromone::PheromoneGraph;
use touring_code::ast::{ImpactCategory, SymbolIndex, compute_enriched_blast_radius};

// ─── Types ──────────────────────────────────────────────────────────────

// Session B F4-pre (2026-06-10): the signal-layer vocabulary (SignalContext /
// LayerMetrics / SignalLayer) moved to `touring-hooks-shared::signal_layer` so
// leaf-side producers (e.g. `ast_grep_signal`) can implement the trait without
// reaching back into this crate. Re-exported here so every historical
// `crate::shared::signal_pipeline::{SignalContext, SignalLayer}` path — and the
// `impl SignalLayer` blocks below — keep resolving unchanged.
pub use touring_hooks_shared::signal_layer::{
    LayerMetrics, ProposedChange, SignalContext, SignalLayer,
};

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
        // Cross-audit 04/09/2026 — este corpo era uma COPIA integral de
        // `execute_with_metrics`, e a sua copia media cada camada num
        // `let mut _metrics` com underscore: a medicao era paga em toda invocacao
        // de hook e descartada por construcao. Duas copias do mesmo laco divergem
        // no primeiro dia em que alguem corrige so uma. Uma fonte, e quem nao quer
        // as metricas descarta aqui.
        self.execute_with_metrics(ctx).0
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

/// Build a [`SignalPipeline`] pre-configured with the graph-integrated layers.
///
/// Convenience constructor for composing the full analysis stack:
///
/// | Layer | Type | CILA gate | Cost |
/// |---|---|---|---|
/// | BFS blast radius | [`BlastRadiusSignalLayer`] | ≥ 2 | one walk of the index |
/// | Enriched blast radius | [`EnrichedBlastRadiusSignalLayer`] | ≥ 3 | one walk of the index |
/// | Weighted Dijkstra blast | [`WeightedBlastSignalLayer`] | ≥ 4 | one Dijkstra |
/// | ACO pheromone hot-edges | [`PheromoneGraphSignalLayer`] | ≥ 2 | <1ms |
/// | HNSW approximate NN | [`HnswSignalLayer`] | ≥ 5 | ~50ms (cold start, cached) |
///
/// The three blast layers register as ONE entry, [`BlastGroupLayer`]: each of them says
/// its piece in one line, and the group keeps the whole under the text ceiling (default
/// 400 characters, `TOURING_GRAPH_BLAST_MAX_CHARS`). Before, every layer emitted one
/// signal PER affected file, and a hub filled the 3.200-character budget with
/// `blast:<path> (hop 1)` lines in path order — about 3 KB of context per read. The
/// HNSW layer is wrapped in [`CilaGatedLayer`] to prevent cold-start cost on hooks where
/// `cila_level < 5`; the group and the pheromone layer carry their own
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
    // A private cache: the HNSW index lives as long as this pipeline, i.e. one call.
    build_graph_pipeline_with_cache(index, graph, budget, Arc::new(OnceLock::new()))
}

/// [`build_graph_pipeline`] with the HNSW cache supplied by the caller.
///
/// The runtime keeps one cache per loaded generation of the index (see
/// `touring_hook_runtime::index_slot::IndexGeneration::hnsw_cache`), so the CILA >= 5 layer
/// pays the O(n log n) build once per generation instead of once per read: the
/// pipeline of a hook call is built and dropped, the cache is not. A cache must be paired
/// with the index it was built from; the generation carries both for that reason.
pub fn build_graph_pipeline_with_cache(
    index: Arc<SymbolIndex>,
    graph: Arc<std::sync::RwLock<PheromoneGraph>>,
    budget: usize,
    hnsw_cache: Arc<OnceLock<HnswStrategy>>,
) -> SignalPipeline {
    SignalPipeline::new(budget)
        // CILA>=2: the blast layers as one bounded text — BFS radius (>=2), enriched
        // impact split (>=3), co-edit-weighted nearest files (>=4); each member gates
        // itself, the group keeps the whole under the ceiling.
        .add_layer(BlastGroupLayer::new(Arc::clone(&index)))
        // CILA>=2: ACO pheromone hot-edges (co-access trail signals)
        .add_layer(CilaGatedLayer::new(
            PheromoneGraphSignalLayer::new(Arc::clone(&graph)),
            2,
        ))
        // CILA>=5: HNSW approximate nearest-neighbor (cold-start cached)
        .add_layer(CilaGatedLayer::new(
            HnswSignalLayer::with_cache(Arc::clone(&index), 10, hnsw_cache),
            5,
        ))
}

// ─── Graph-Integrated Layers ────────────────────────────────────────────

/// BFS blast radius layer — how far a change reaches, as ONE line.
///
/// Produces at most one signal (`blast: N arquivos, até d saltos; diretos (D): …
/// (+K outros); lista completa: touring ast blast <file>`), built by
/// [`blast_aggregate_signal`]: the count and depth of the radius, the importers that
/// matter most (co-edit weight, then in-degree, then path), and the route to the rest.
/// It was one signal per affected file, so a hub shipped thousands of them.
///
/// The count is the whole radius ([`SymbolIndex::blast_radius_with_depth`], the number
/// the one-line `blast_radius_signal` prints), not the same-crate slice the engine's hook
/// path keeps: two numbers for one file is a contradiction the reader cannot resolve.
/// Silent unless the index KNOWS an importer — an isolated or uncovered file is
/// `blast_radius_signal`'s to say.
///
/// Only runs at CILA level ≥ 2. Normally reached through [`BlastGroupLayer`], which
/// keeps its text and its siblings' under one ceiling.
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
        blast_aggregate_signal(&self.index, ctx.file_path, graph_blast_text_cap())
            .into_iter()
            .collect()
    }

    fn should_run(&self, cila_level: usize) -> bool {
        cila_level >= 2
    }
}

/// Enriched blast radius layer — direct, transitive, and co-edited dependents, counted.
///
/// Uses [`compute_enriched_blast_radius`] for semantic categorisation of impact and says
/// it as one line: `impacto: 12 direct, 1920 transitive (severidade 0.31)` — the
/// categories' wire names ([`ImpactCategory::signal_prefix`]), the empty ones left out.
/// The score is the severity (0–1) discounted by the most direct non-empty category
/// ([`ImpactCategory::signal_discount`]: direct=100%, transitive=50%, co-edited=30%). It
/// was one signal per file of each category.
///
/// Silent when no category holds a file. Only runs at CILA level ≥ 3.
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
        let mut counted: Vec<String> = Vec::new();
        let mut strongest: Option<ImpactCategory> = None;
        for category in ImpactCategory::ALL {
            let count = enriched.files(category).len();
            if count > 0 {
                if strongest.is_none() {
                    strongest = Some(category);
                }
                counted.push(format!("{count} {}", category.signal_prefix()));
            }
        }
        // The signal weighs what its most direct non-empty category weighs.
        let Some(strongest) = strongest else {
            return Vec::new();
        };
        vec![(
            enriched.severity as f32 * strongest.signal_discount(),
            format!(
                "impacto: {} (severidade {:.2})",
                counted.join(", "),
                enriched.severity
            ),
        )]
    }

    fn should_run(&self, cila_level: usize) -> bool {
        cila_level >= 3
    }
}

/// How many of the nearest files the weighted layer names.
const WEIGHTED_NEAREST_SHOWN: usize = 3;

/// Weighted blast radius layer — Dijkstra co-edit-weighted dependency distance, as ONE line.
///
/// Uses [`SymbolIndex::weighted_blast_radius`] for O(V log V) Dijkstra traversal
/// with co-edit-history-weighted edges (`w = 1.0 / (1.0 + co_edit_weight)`), and names
/// the nearest few: `custo de co-edição: a/x.rs 0.01, b/y.rs 0.02 (de 1920 arquivos)`.
/// Score = `1.0 / (1.0 + cost)` of the nearest. The start file (cost = 0.0) is filtered
/// out, and files at the same cost come in path order (the index fixes the tie), so the
/// few named are the same on every run.
///
/// Silent when nothing else is reachable. Only runs at CILA level ≥ 4.
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
        // Ascending cost, ties in path order; the start file (cost 0) is not a dependent.
        let reached: Vec<(String, f64)> = self
            .index
            .weighted_blast_radius(ctx.file_path)
            .into_iter()
            .filter(|(_, cost)| *cost > 0.0)
            .collect();
        let Some((_, nearest_cost)) = reached.first() else {
            return Vec::new();
        };
        let nearest = reached
            .iter()
            .take(WEIGHTED_NEAREST_SHOWN)
            .map(|(path, cost)| format!("{} {cost:.2}", short_path_label(path)))
            .collect::<Vec<_>>()
            .join(", ");
        vec![(
            1.0_f32 / (1.0 + *nearest_cost as f32),
            format!(
                "custo de co-edição: {nearest} (de {} arquivos)",
                reached.len()
            ),
        )]
    }

    fn should_run(&self, cila_level: usize) -> bool {
        cila_level >= 4
    }
}

/// Default ceiling, in characters, on the blast text the graph pipeline emits.
const GRAPH_BLAST_TEXT_MAX_CHARS: usize = 400;

/// Environment knob that overrides [`GRAPH_BLAST_TEXT_MAX_CHARS`] (a character count).
const GRAPH_BLAST_TEXT_ENV: &str = "TOURING_GRAPH_BLAST_MAX_CHARS";

/// Smallest ceiling the knob accepts: the bare blast line with a short path is ~110
/// characters, and a ceiling below it would cut the count it exists to deliver.
const GRAPH_BLAST_TEXT_MIN_CHARS: usize = 120;

/// Largest ceiling the knob accepts: the pipeline's own context budget.
const GRAPH_BLAST_TEXT_LIMIT_CHARS: usize = 3200;

/// The ceiling `raw` asks for: nothing set, or an integer inside the accepted range. An
/// invalid value is NOT silently honoured or dropped: the default holds and the log says
/// why, naming the accepted range, so a typo does not become a different ceiling.
fn resolve_graph_blast_text_cap(raw: Option<&str>) -> usize {
    let Some(raw) = raw.map(str::trim).filter(|value| !value.is_empty()) else {
        return GRAPH_BLAST_TEXT_MAX_CHARS;
    };
    match raw.parse::<usize>() {
        Ok(chars)
            if (GRAPH_BLAST_TEXT_MIN_CHARS..=GRAPH_BLAST_TEXT_LIMIT_CHARS).contains(&chars) =>
        {
            chars
        }
        _ => {
            tracing::warn!(
                "{GRAPH_BLAST_TEXT_ENV}={raw:?} is not an integer between \
                 {GRAPH_BLAST_TEXT_MIN_CHARS} and {GRAPH_BLAST_TEXT_LIMIT_CHARS}; using the \
                 default {GRAPH_BLAST_TEXT_MAX_CHARS}. Set it to a character count such as 600, \
                 or unset it."
            );
            GRAPH_BLAST_TEXT_MAX_CHARS
        }
    }
}

/// The ceiling in force: read from the environment once per process (so an invalid value
/// warns once, not on every hook call).
fn graph_blast_text_cap() -> usize {
    static CAP: OnceLock<usize> = OnceLock::new();
    *CAP.get_or_init(|| {
        resolve_graph_blast_text_cap(std::env::var(GRAPH_BLAST_TEXT_ENV).ok().as_deref())
    })
}

/// The blast layers as ONE signal under ONE ceiling.
///
/// Runs [`BlastRadiusSignalLayer`] (CILA ≥ 2), [`EnrichedBlastRadiusSignalLayer`]
/// (≥ 3) and [`WeightedBlastSignalLayer`] (≥ 4), each of which says its piece in a line,
/// and joins what they said with ` | `, the pipeline's own separator, under
/// `TOURING_GRAPH_BLAST_MAX_CHARS` (default 400 characters). The radius line comes first
/// and always stays; a later line joins only whole and only while it fits. The group
/// scores as the radius line does. Silent when the radius layer is: an isolated or
/// uncovered file has no radius to describe, and what its siblings could add depends on
/// one.
///
/// Why a group: layers that each emit per-file signals cannot be bounded by one number,
/// and the pipeline's budget truncates by signal, in score order, not by meaning.
pub struct BlastGroupLayer {
    members: Vec<Box<dyn SignalLayer>>,
}

impl BlastGroupLayer {
    /// The three blast layers over `index`, radius line first.
    pub fn new(index: Arc<SymbolIndex>) -> Self {
        Self {
            members: vec![
                Box::new(BlastRadiusSignalLayer::new(Arc::clone(&index))),
                Box::new(EnrichedBlastRadiusSignalLayer::new(Arc::clone(&index))),
                Box::new(WeightedBlastSignalLayer::new(index)),
            ],
        }
    }
}

/// Join `parts` (radius line first) into one signal of at most `max_chars` characters.
fn compose_blast_parts(parts: Vec<(f32, String)>, max_chars: usize) -> Option<(f32, String)> {
    let mut parts = parts.into_iter();
    let (score, first) = parts.next()?;
    let mut text = cut_to_chars(&first, max_chars);
    for (_, part) in parts {
        let joined = text.chars().count() + " | ".len() + part.chars().count();
        if joined <= max_chars {
            text.push_str(" | ");
            text.push_str(&part);
        }
    }
    Some((score, text))
}

impl SignalLayer for BlastGroupLayer {
    fn name(&self) -> &'static str {
        "blast_radius_group"
    }

    fn enrich(&self, ctx: &SignalContext<'_>) -> Vec<(f32, String)> {
        let parts: Vec<(f32, String)> = self
            .members
            .iter()
            .filter(|member| member.should_run(ctx.cila_level))
            .flat_map(|member| member.enrich(ctx))
            .collect();
        compose_blast_parts(parts, graph_blast_text_cap())
            .into_iter()
            .collect()
    }

    fn should_run(&self, cila_level: usize) -> bool {
        cila_level >= 2
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
        Self::with_cache(index, k, Arc::new(std::sync::OnceLock::new()))
    }

    /// Create a layer over a cache the caller owns, so the HNSW index survives the layer.
    ///
    /// The cache must have been (or will be) built from `index` and nothing else: the
    /// runtime hands both out together as one `IndexGeneration`.
    pub fn with_cache(
        index: Arc<SymbolIndex>,
        k: usize,
        cache: Arc<std::sync::OnceLock<HnswStrategy>>,
    ) -> Self {
        Self { index, cache, k }
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

// ─── S0 (2026-09-01): the hook call sites' SignalContext v2 ─────────────
//
// The three pre-hooks used to run their pipelines on
// `SignalContext::new(&rel_path, "")` — no tool, no proposed change — so no
// `SignalLayer` could look at the code about to be written. These builders are
// the single place the live contexts are assembled; a layer that needs the
// proposal reads `ctx.proposed` / `ctx.analysable_text()`.

/// Context for `pre_write`: the whole proposed file content travels with it.
pub fn context_for_write<'a>(
    rel_path: &'a str,
    content: &'a str,
    cila: usize,
) -> SignalContext<'a> {
    SignalContext::new(rel_path, "")
        .with_cila(cila)
        .with_hook("pre_write")
        .with_tool_name("Write")
        .with_proposed(ProposedChange::Write { content })
}

/// Context for `pre_edit`: the exact replacement travels with it.
pub fn context_for_edit<'a>(
    rel_path: &'a str,
    old_string: &'a str,
    new_string: &'a str,
    cila: usize,
) -> SignalContext<'a> {
    SignalContext::new(rel_path, "")
        .with_cila(cila)
        .with_hook("pre_edit")
        .with_tool_name("Edit")
        .with_proposed(ProposedChange::Edit {
            old_string,
            new_string,
        })
}

/// Context for `pre_read`: no mutation, only the tool identity.
pub fn context_for_read(rel_path: &str, cila: usize) -> SignalContext<'_> {
    SignalContext::new(rel_path, "")
        .with_cila(cila)
        .with_hook("pre_read")
        .with_tool_name("Read")
}

// ─── Tests ──────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    // ── S0 (2026-09-01): the three hook call sites build their context here ──

    #[test]
    fn context_for_write_carries_tool_and_proposed_content() {
        let ctx = context_for_write("src/new.rs", "fn fresh() {}", 4);
        assert_eq!(ctx.hook_name, "pre_write");
        assert_eq!(ctx.tool_name, "Write");
        assert_eq!(ctx.cila_level, 4);
        assert_eq!(ctx.file_path, "src/new.rs");
        assert_eq!(
            ctx.proposed,
            Some(ProposedChange::Write {
                content: "fn fresh() {}"
            })
        );
        assert_eq!(ctx.analysable_text(), "fn fresh() {}");
    }

    #[test]
    fn context_for_edit_carries_old_and_new_strings() {
        let ctx = context_for_edit("src/lib.rs", "fn a() {}", "fn a() { 1 }", 3);
        assert_eq!(ctx.hook_name, "pre_edit");
        assert_eq!(ctx.tool_name, "Edit");
        assert_eq!(
            ctx.proposed,
            Some(ProposedChange::Edit {
                old_string: "fn a() {}",
                new_string: "fn a() { 1 }"
            })
        );
        assert_eq!(ctx.analysable_text(), "fn a() { 1 }");
    }

    #[test]
    fn context_for_read_has_tool_but_no_proposal() {
        let ctx = context_for_read("src/lib.rs", 2);
        assert_eq!(ctx.hook_name, "pre_read");
        assert_eq!(ctx.tool_name, "Read");
        assert!(ctx.proposed.is_none());
        assert_eq!(
            ctx.analysable_text(),
            "",
            "nothing proposed, nothing on hand"
        );
    }

    #[test]
    fn test_empty_pipeline_returns_none() {
        let pipeline = SignalPipeline::new(1000);
        let ctx = SignalContext::new("test.rs", "fn main() {}");
        assert!(pipeline.execute(&ctx).is_none());
    }

    /// Cross-audit 04/09/2026 — `execute` era uma COPIA integral de
    /// `execute_with_metrics`, e media cada camada num `_metrics` descartado por
    /// construcao. Agora delega. Este teste e' o que impede as duas de divergirem
    /// de novo: duas copias do mesmo laco so ficam iguais enquanto ninguem corrige
    /// uma delas.
    #[test]
    fn execute_e_execute_with_metrics_nunca_divergem() {
        let pipeline = SignalPipeline::new(1000)
            .with_normalize(false)
            .add_layer(StaticSignalLayer::new(
                "alta",
                vec![(0.9, "sinal alto".to_string())],
            ))
            .add_layer(StaticSignalLayer::new(
                "baixa",
                vec![(0.2, "sinal baixo".to_string())],
            ));
        let ctx = SignalContext::new("test.rs", "");
        let (com_metricas, metricas) = pipeline.execute_with_metrics(&ctx);
        assert_eq!(
            pipeline.execute(&ctx),
            com_metricas,
            "as duas rotas tem de dar a MESMA saida"
        );
        assert_eq!(metricas.len(), 2, "uma metrica por camada que rodou");
        assert!(
            metricas.iter().all(|m| m.signal_count == 1),
            "cada camada estatica produz 1 sinal: {metricas:?}"
        );
    }

    /// A medicao chegou a TODAS as camadas, nao so as duas que a tinham a mao.
    /// `StaticSignalLayer` nunca teve um `layer_metrics` proprio e agora e' medivel
    /// pelo metodo default do trait — a expansao que justificou nao inventar um
    /// chamador de fachada para as duas orfas.
    #[test]
    fn qualquer_camada_e_medivel_pelo_default_do_trait() {
        let camada = StaticSignalLayer::new(
            "sem_metrica_propria",
            vec![(0.7, "um".to_string()), (0.3, "dois".to_string())],
        );
        let ctx = SignalContext::new("test.rs", "");
        let m = camada.metrics(&ctx);
        assert_eq!(m.name, "sem_metrica_propria");
        assert_eq!(m.signal_count, 2, "conta os sinais que a camada produziu");
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

    /// C8: the runtime keeps ONE HNSW cache per loaded generation of the index and hands it to
    /// every read's pipeline; the pipeline is built and dropped per read, the cache is not.
    #[test]
    fn the_hnsw_layer_builds_into_the_cache_it_is_given_and_every_pipeline_reuses_it() {
        use std::sync::{Arc, OnceLock, RwLock};
        use touring_code::ast::SymbolIndex;
        use touring_code::ast::graph::pheromone::PheromoneGraph;
        let index = Arc::new(SymbolIndex::new());
        let graph = Arc::new(RwLock::new(PheromoneGraph::new(0.1)));
        let cache: Arc<OnceLock<HnswStrategy>> = Arc::new(OnceLock::new());
        let pipeline_over = |cache: &Arc<OnceLock<HnswStrategy>>| {
            build_graph_pipeline_with_cache(
                Arc::clone(&index),
                Arc::clone(&graph),
                2000,
                Arc::clone(cache),
            )
        };

        // Below CILA 5 the layer does not run: nothing is built, the cache stays empty.
        let _ = pipeline_over(&cache).execute(&SignalContext::new("x.rs", "").with_cila(4));
        assert!(
            cache.get().is_none(),
            "control: CILA 4 must not build the HNSW graph"
        );

        // At CILA 5 the first read builds INTO the cache the caller owns.
        let _ = pipeline_over(&cache).execute(&SignalContext::new("x.rs", "").with_cila(5));
        let built = cache
            .get()
            .expect("the CILA 5 run must build into the supplied cache, not a private one")
            as *const HnswStrategy;

        // A later read, through a brand new pipeline over the same cache, finds it built.
        let _ = pipeline_over(&cache).execute(&SignalContext::new("y.rs", "").with_cila(5));
        assert!(
            std::ptr::eq(built, cache.get().expect("still built")),
            "the second pipeline reused the first read's HNSW graph"
        );
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

    // ── C8: the blast layers say their piece in ONE line, under ONE ceiling ──

    const HUB: &str = "crates/hub/src/hub.rs";

    /// A hub: 12 direct importers, each imported by 160 more files — 1.932 files in the
    /// radius besides the hub itself. The five heaviest by co-edit history are, in order,
    /// d07 d03 d11 d00 d05.
    fn hub_index() -> Arc<SymbolIndex> {
        let mut idx = SymbolIndex::new();
        let mut edges: Vec<(String, String)> = Vec::new();
        let mut covered = vec![HUB.to_string()];
        for d in 0..12 {
            let direct = format!("crates/app/src/d{d:02}.rs");
            edges.push((direct.clone(), HUB.to_string()));
            covered.push(direct.clone());
            for t in 0..160 {
                let leaf = format!("crates/leaf/src/l{d:02}_{t:03}.rs");
                edges.push((leaf.clone(), direct.clone()));
                covered.push(leaf);
            }
        }
        idx.load_file_edges(edges, covered);
        idx.apply_co_edit_weights([7, 3, 11, 0, 5].iter().enumerate().map(|(rank, d)| {
            (
                format!("crates/app/src/d{d:02}.rs"),
                HUB.to_string(),
                100 - 10 * rank as i64,
            )
        }));
        Arc::new(idx)
    }

    #[test]
    fn test_hub_is_one_signal_per_layer_not_one_per_file() {
        let index = hub_index();
        let ctx = SignalContext::new(HUB, "").with_cila(4);
        // Before: one signal PER affected file from each of the three layers (1.932, 1.932
        // and 1.932 here; ~1.920 x 3 on the touring project's hubs).
        assert_eq!(
            BlastRadiusSignalLayer::new(Arc::clone(&index))
                .enrich(&ctx)
                .len(),
            1,
            "the radius is one line"
        );
        assert_eq!(
            EnrichedBlastRadiusSignalLayer::new(Arc::clone(&index))
                .enrich(&ctx)
                .len(),
            1,
            "the impact split is one line"
        );
        assert_eq!(
            WeightedBlastSignalLayer::new(Arc::clone(&index))
                .enrich(&ctx)
                .len(),
            1,
            "the nearest files are one line"
        );
    }

    #[test]
    fn test_enriched_layer_counts_the_categories() {
        let index = hub_index();
        let ctx = SignalContext::new(HUB, "").with_cila(4);
        let (score, text) = EnrichedBlastRadiusSignalLayer::new(index)
            .enrich(&ctx)
            .pop()
            .expect("a hub has impact");
        assert!(
            text.starts_with("impacto: 12 direct, 1920 transitive (severidade "),
            "12 direct importers, 12 x 160 beyond them, empty categories left out: {text}"
        );
        assert!(score > 0.0 && score <= 1.0, "severity-scaled, got {score}");
    }

    #[test]
    fn test_weighted_layer_names_the_nearest_in_cost_then_path_order() {
        let index = hub_index();
        let ctx = SignalContext::new(HUB, "").with_cila(4);
        let (score, text) = WeightedBlastSignalLayer::new(index)
            .enrich(&ctx)
            .pop()
            .expect("a hub reaches files");
        assert!(
            text.starts_with(
                "custo de co-edição: src/d07.rs 0.01, src/d03.rs 0.01, src/d11.rs 0.01 "
            ),
            "the three heaviest edges cost least (1/(1+100), 1/(1+90), 1/(1+80)): {text}"
        );
        assert!(
            text.ends_with("(de 1932 arquivos)"),
            "every other file counted: {text}"
        );
        assert!(score > 0.9, "the nearest costs about 0.01, got {score}");
    }

    #[test]
    fn test_blast_group_on_a_hub_is_one_bounded_line_with_the_right_count_and_heaviest_five() {
        let index = hub_index();
        let walked = index.blast_radius_with_depth(HUB, 4);
        let group = BlastGroupLayer::new(Arc::clone(&index));
        let ctx = SignalContext::new(HUB, "").with_cila(4);

        let signals = group.enrich(&ctx);
        assert_eq!(signals.len(), 1, "the group is one signal");
        let (_, text) = signals.first().expect("one signal");

        assert!(
            text.chars().count() <= GRAPH_BLAST_TEXT_MAX_CHARS,
            "{} chars over the {GRAPH_BLAST_TEXT_MAX_CHARS} ceiling: {text}",
            text.chars().count()
        );
        assert!(
            text.starts_with(&format!(
                "blast: {} arquivos, até {} saltos; ",
                walked.file_count, walked.max_distance
            )),
            "the count is the walk's: {text}"
        );
        assert!(
            text.contains(
                "diretos (12): src/d07.rs, src/d03.rs, src/d11.rs, src/d00.rs, src/d05.rs (+7 outros)"
            ),
            "the five heaviest in weight order, 12 - 5 = 7 counted: {text}"
        );
        assert!(
            text.contains(&format!("touring ast blast {HUB}")),
            "teaches the route to the full list: {text}"
        );
        assert!(
            text.contains("impacto: 12 direct, 1920 transitive"),
            "{text}"
        );
        assert!(text.contains("custo de co-edição: "), "{text}");
        assert!(
            !text.contains("crates/leaf") && !text.contains("hop"),
            "no per-file lines: {text}"
        );
    }

    #[test]
    fn test_blast_group_adds_a_line_per_cila_level() {
        let index = hub_index();
        let group = BlastGroupLayer::new(index);
        let text_at = |cila: usize| {
            group
                .enrich(&SignalContext::new(HUB, "").with_cila(cila))
                .pop()
                .map(|(_, text)| text)
        };

        assert!(!group.should_run(1), "off below CILA 2");
        assert!(group.should_run(2));
        let two = text_at(2).expect("CILA 2 gives the radius line");
        let three = text_at(3).expect("CILA 3");
        let four = text_at(4).expect("CILA 4");
        assert!(
            two.starts_with("blast: ") && !two.contains("impacto:"),
            "{two}"
        );
        assert!(
            three.contains("impacto:") && !three.contains("custo de co-edição"),
            "{three}"
        );
        assert!(
            four.contains("impacto:") && four.contains("custo de co-edição"),
            "{four}"
        );
    }

    #[test]
    fn test_blast_group_is_silent_for_a_file_with_no_known_importer() {
        let mut idx = SymbolIndex::new();
        idx.load_file_edges(
            std::iter::empty::<(String, String)>(),
            ["src/orphan.rs".to_string()],
        );
        let group = BlastGroupLayer::new(Arc::new(idx));
        for file in ["src/orphan.rs", "docs/never_read.md"] {
            assert!(
                group
                    .enrich(&SignalContext::new(file, "").with_cila(5))
                    .is_empty(),
                "{file}: isolated and uncovered are blast_radius_signal's to say, not the group's"
            );
        }
    }

    #[test]
    fn test_graph_pipeline_on_a_hub_stays_under_the_ceiling() {
        // The measured defect: at CILA 4 a hub's graph text filled the 3.200-character
        // budget (median 3.144 over 50 files) in path order. Through the real factory:
        let index = hub_index();
        let graph = Arc::new(std::sync::RwLock::new(PheromoneGraph::new(0.1)));
        let pipeline = build_graph_pipeline(Arc::clone(&index), graph, 3200);
        let out = pipeline
            .execute(&SignalContext::new(HUB, "").with_cila(4))
            .expect("a hub has a blast text");
        assert!(
            out.chars().count() <= GRAPH_BLAST_TEXT_MAX_CHARS,
            "{} chars: {out}",
            out.chars().count()
        );
        assert!(
            out.starts_with("blast: 1933 arquivos, até 2 saltos; "),
            "{out}"
        );
    }

    #[test]
    fn test_compose_blast_parts_keeps_the_first_whole_and_later_ones_only_whole() {
        let first = (2.5_f32, "R".repeat(100));
        let second = (0.4_f32, "S".repeat(60));
        let third = (0.9_f32, "T".repeat(30));

        // Room for all: joined by the pipeline's separator, scored as the first.
        let (score, all) =
            compose_blast_parts(vec![first.clone(), second.clone(), third.clone()], 400)
                .expect("a first part");
        assert!((score - 2.5).abs() < f32::EPSILON);
        assert_eq!(all, format!("{} | {} | {}", first.1, second.1, third.1));

        // 100 + 3 + 60 = 163 > 150: the second is dropped whole (not cut), the third fits.
        let (_, partial) =
            compose_blast_parts(vec![first.clone(), second.clone(), third.clone()], 150)
                .expect("a first part");
        assert_eq!(partial, format!("{} | {}", first.1, third.1));

        // A first part over the ceiling is cut and says so; nothing is appended after it.
        let (_, cut) = compose_blast_parts(vec![first.clone(), second], 90).expect("a first part");
        assert_eq!(cut.chars().count(), 90);
        assert!(cut.ends_with("..."), "{cut}");

        assert!(compose_blast_parts(Vec::new(), 400).is_none());
    }

    #[test]
    fn test_graph_blast_text_ceiling_knob_defaults_and_rejects_the_invalid() {
        let default = GRAPH_BLAST_TEXT_MAX_CHARS;
        assert_eq!(default, 400, "the documented default");
        assert_eq!(resolve_graph_blast_text_cap(None), default);
        assert_eq!(resolve_graph_blast_text_cap(Some("")), default);
        assert_eq!(resolve_graph_blast_text_cap(Some("  ")), default);
        assert_eq!(resolve_graph_blast_text_cap(Some("600")), 600);
        assert_eq!(resolve_graph_blast_text_cap(Some(" 800 ")), 800);
        assert_eq!(
            resolve_graph_blast_text_cap(Some("120")),
            120,
            "the floor is accepted"
        );
        assert_eq!(
            resolve_graph_blast_text_cap(Some("3200")),
            3200,
            "the limit is accepted"
        );
        for invalid in ["119", "3201", "0", "-5", "abc", "400.5", "4e2"] {
            assert_eq!(
                resolve_graph_blast_text_cap(Some(invalid)),
                default,
                "{invalid:?} is not a ceiling in range: the default holds (and the log warns)"
            );
        }
    }
}
