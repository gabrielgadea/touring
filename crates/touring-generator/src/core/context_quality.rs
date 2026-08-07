//! Quality / health gate adapters + semantic-graph adapter + score types.
//!
//! Extracted from `core/context.rs` (F-9 modularization): the `QualityGateAdapter`
//! (DB-backed quality analysis), `HealthGateAdapter` (e2e composite-score gate),
//! `SemanticGraphAdapter` (cognitive graph), `PlanSimilarityScore`, and the
//! `HealthDelta` / quality / health / enrichment closure type aliases. Re-exported
//! from `core::context` so the public API is preserved verbatim. The
//! `HEALTH_SCORES` cache and the `SemanticGraphFn` / `CognitiveNexusFn` type
//! aliases stay in `context.rs` and are imported here (cfg-gated).

use crate::core::score::NormalizedScore;
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[cfg(feature = "health-gate")]
use crate::core::context::HEALTH_SCORES;
#[cfg(feature = "cognitive-nexus")]
use crate::core::context::{CognitiveNexusFn, SemanticGraphFn};
#[cfg(any(feature = "quality-gate", feature = "health-gate"))]
use crate::error::GenerateError;
#[cfg(feature = "cognitive-nexus")]
use crate::plan::schema::GeneratorPlan;

// ── HealthDelta closures (Wave 19, 2026-04-18) ───────────────────────────────

/// Wave 19 — pre-commit health record. Receives `(file_path, source)` and
/// caches a unified quality score keyed by `file_path`. Returns the recorded
/// score for diagnostics; `None` for unsupported languages or parse failure.
///
/// Injected from `touring_hooks::health_delta::record_pre_signals` so the
/// generator pipeline shares the SAME cache + counters as `pre_edit` /
/// `pre_write`. This closes the dynamic-quality loop for generated code:
/// every artifact written via `plan-submit` flows through the same
/// streak/regression/improvement state machine that hand-edits use.
pub type HealthDeltaRecordFn = Arc<dyn Fn(&str, &str) -> Option<f32> + Send + Sync>;

/// Wave 19 — post-commit health delta computation. Receives `(file_path,
/// new_source)` and returns `Some((delta, is_regression, is_improvement))`
/// when both pre-record and post-compute succeed; `None` when no pre-record
/// existed (first-observation) or the path is unsupported.
///
/// Tuple shape (vs full `HealthDelta` struct) keeps the closure boundary
/// dependency-free — `touring-generator` does not need to import
/// `touring-hooks::HealthDelta`.
pub type HealthDeltaComputeFn = Arc<dyn Fn(&str, &str) -> Option<(f32, bool, bool)> + Send + Sync>;

// ── QualityGateAdapter (PLN2 — feature quality-gate) ─────────────────────────

/// Post-commit quality gate function type.
#[cfg(feature = "quality-gate")]
pub type QualityGateFn =
    Arc<dyn Fn(&[crate::plan::result::RenderedFile]) -> Result<(), GenerateError> + Send + Sync>;

/// DB-backed quality gate adapter using `touring_analysis::QualityPipeline`.
///
/// Runs quality analysis on rendered `.rs` files before each commit and rejects
/// when antipattern count, unwrap count, or composite quality score exceeds
/// configured thresholds.
///
/// # Thresholds (POTENCIALIZAR defaults)
#[cfg(feature = "quality-gate")]
#[derive(Clone)]
pub struct QualityGateAdapter {
    pipeline: Arc<touring_analysis::quality::QualityPipeline>,
    max_unwraps: usize,
    max_antipatterns: usize,
    min_score: f64,
    /// Minimum `RustQualitySignals::health_score` (from touring-ast syn
    /// semantic analysis) for `.rs` files. Set to `0.0` (default) to
    /// disable the semantic fusion gate. Recommended strict value: `0.6`.
    min_semantic_score: f32,
}

#[cfg(feature = "quality-gate")]
impl std::fmt::Debug for QualityGateAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("QualityGateAdapter")
            .field("max_unwraps", &self.max_unwraps)
            .field("max_antipatterns", &self.max_antipatterns)
            .field("min_score", &self.min_score)
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "quality-gate")]
impl QualityGateAdapter {
    /// Construct a quality gate with POTENCIALIZAR defaults:
    /// `max_unwraps = 10`, `max_antipatterns = 5`, `min_score = 0.5`.
    #[must_use]
    pub fn new(config: touring_analysis::engine::AnalysisConfig) -> Self {
        Self {
            pipeline: Arc::new(touring_analysis::quality::QualityPipeline::new(config)),
            max_unwraps: 10,
            max_antipatterns: 5,
            min_score: 0.5,
            min_semantic_score: 0.0,
        }
    }

    /// Override one or more thresholds.
    #[must_use]
    pub fn with_thresholds(
        mut self,
        max_unwraps: usize,
        max_antipatterns: usize,
        min_score: f64,
    ) -> Self {
        self.max_unwraps = max_unwraps;
        self.max_antipatterns = max_antipatterns;
        self.min_score = min_score;
        self
    }

    /// Enable the syn-backed semantic health gate for `.rs` files.
    ///
    /// When `min_semantic_score > 0.0`, each Rust file is additionally
    /// parsed by `touring_code::ast::rust_semantic::RustSemanticReport` and its
    /// `RustQualitySignals::health_score()` must meet this minimum.
    /// Semantic signals capture generics, lifetimes, unsafe density, and
    /// trait-bound complexity — dimensions tree-sitter cannot express.
    ///
    /// Recommended strict value: `0.6`. Non-Rust files are unaffected.
    #[must_use]
    pub fn with_semantic_threshold(mut self, min_semantic_score: f32) -> Self {
        self.min_semantic_score = min_semantic_score;
        self
    }

    /// Run quality analysis on the given rendered files.
    ///
    /// Returns `Ok(())` if all files pass thresholds. Inspects only `.rs` files.
    ///
    /// # Errors
    /// Returns `GenerateError` if quality thresholds are violated.
    pub fn check(&self, files: &[crate::plan::result::RenderedFile]) -> Result<(), GenerateError> {
        let inputs = Self::extract_inputs(files);
        if inputs.is_empty() {
            return Ok(());
        }
        let reports = self.pipeline.analyze_batch(&inputs);
        for report in &reports {
            if report.unwrap_count > self.max_unwraps {
                return Err(GenerateError::Internal(format!(
                    "quality gate [{}]: {} unwraps in {} exceeds max {}",
                    report.language, report.unwrap_count, report.file_path, self.max_unwraps
                )));
            }
            if report.antipatterns.len() > self.max_antipatterns {
                return Err(GenerateError::Internal(format!(
                    "quality gate [{}]: {} antipatterns in {} exceeds max {}",
                    report.language,
                    report.antipatterns.len(),
                    report.file_path,
                    self.max_antipatterns
                )));
            }
            if report.score < self.min_score {
                return Err(GenerateError::Internal(format!(
                    "quality gate [{}]: score {:.3} in {} below min {:.3}",
                    report.language, report.score, report.file_path, self.min_score
                )));
            }
        }
        // ── Semantic fusion — opt-in, polyglot (P-D parity, 2026-07-03) ─────
        // Runs when min_semantic_score > 0.0. Rust uses touring-ast's deep syn
        // parser (RustQualitySignals — generics, lifetimes, unsafe, trait
        // bounds). Python/TypeScript/JavaScript use the tree-sitter-backed
        // PolyglotQualitySignals (dynamic escapes, async surface, type-annotation
        // coverage) with the SAME health-score penalty shape, so a single
        // `min_semantic_score` bar applies across languages — closing the former
        // "Non-Rust files are unaffected" parity hole. Go/Java/C++ still skip
        // (no deep semantic report yet → honest no-op, never a silent pass).
        if self.min_semantic_score > 0.0 {
            for file in files {
                if let Some((health, detail)) = Self::semantic_health(&file.path, &file.content)
                    && health < self.min_semantic_score
                {
                    return Err(GenerateError::Internal(format!(
                        "quality gate {detail}: health_score {health:.3} in {} below min {:.3}",
                        file.path, self.min_semantic_score,
                    )));
                }
            }
        }
        Ok(())
    }

    /// Semantic health score + diagnostic detail for one rendered file.
    ///
    /// Dispatches Rust → syn-backed `RustQualitySignals`, Python/TS/JS →
    /// tree-sitter-backed `PolyglotQualitySignals` (SAME health-score shape).
    /// Returns `None` for languages without a deep semantic report (Go/Java/C++)
    /// or on parse failure — an honest no-op, never a silent pass.
    fn semantic_health(path: &str, content: &str) -> Option<(f32, String)> {
        match Self::detect_language(path) {
            Some("rust") => touring_analysis::quality::RustQualitySignals::from_source(content)
                .map(|s| {
                    (
                        s.health_score(),
                        format!(
                            "[rust-semantic] unsafe={}, complexity={:.2}, lifetimes={}, bounds={}",
                            s.unsafe_count,
                            s.semantic_complexity,
                            s.lifetime_count,
                            s.trait_bound_count,
                        ),
                    )
                }),
            _ => touring_code::ast::languages::Lang::from_path(std::path::Path::new(path))
                .and_then(|lang| {
                    touring_analysis::quality::PolyglotQualitySignals::from_source(lang, content)
                })
                .map(|s| {
                    (
                        s.health_score(),
                        format!(
                            "[{}-semantic] dynamic_escapes={}, complexity={:.2}, async={}, annotation_coverage={:.2}",
                            s.language,
                            s.dynamic_escape_count,
                            s.semantic_complexity,
                            s.async_count,
                            s.annotation_coverage,
                        ),
                    )
                }),
        }
    }

    /// Compute the average quality score across all analyzed files.
    ///
    /// For `.rs` files with semantic fusion enabled (`min_semantic_score >
    /// 0.0`), the per-file score is the mean of (tree-sitter `report.score`,
    /// syn-backed `RustQualitySignals::health_score()`). This mirrors the
    /// gate's decision surface so `QUALITY_SCORES` — consumed by
    /// `inject_multidim_rewards` as `quality_pass` — reflects the full
    /// fusion verdict, not just the tree-sitter half.
    #[must_use]
    pub fn average_score(&self, files: &[crate::plan::result::RenderedFile]) -> f64 {
        let inputs = Self::extract_inputs(files);
        if inputs.is_empty() {
            return 0.0;
        }
        let reports = self.pipeline.analyze_batch(&inputs);
        if reports.is_empty() {
            return 0.0;
        }
        let fuse_semantic = self.min_semantic_score > 0.0;
        #[allow(clippy::cast_precision_loss)]
        let len = reports.len() as f64;
        let summed: f64 = reports
            .iter()
            .zip(inputs.iter())
            .map(|(report, (path, content, lang))| {
                if !fuse_semantic || *lang != "rust" {
                    return report.score;
                }
                // path is the same String used for the input tuple.
                let _ = path;
                match touring_analysis::quality::RustQualitySignals::from_source(content) {
                    Some(s) => f64::midpoint(report.score, f64::from(s.health_score())),
                    None => report.score,
                }
            })
            .sum();
        summed / len
    }

    pub(crate) fn extract_inputs(
        files: &[crate::plan::result::RenderedFile],
    ) -> Vec<(&str, &str, &str)> {
        files
            .iter()
            .filter_map(|f| {
                let lang = Self::detect_language(&f.path)?;
                Some((f.path.as_str(), f.content.as_str(), lang))
            })
            .collect()
    }

    /// Detect the `QualityPipeline` language identifier from a file path extension.
    ///
    /// Supports all 8 languages implemented by `touring_analysis::quality`:
    /// rust, python, typescript, javascript, go, c, cpp, java.
    /// Returns `None` for unsupported extensions so the gate skips non-code files
    /// (e.g. templates, fixtures, TOML manifests).
    #[must_use]
    pub fn detect_language(path: &str) -> Option<&'static str> {
        let lower = path.to_lowercase();
        let ext = std::path::Path::new(&lower).extension()?.to_str()?;
        Some(match ext {
            "rs" => "rust",
            "py" | "pyi" => "python",
            "ts" | "tsx" => "typescript",
            "js" | "mjs" | "cjs" | "jsx" => "javascript",
            "go" => "go",
            "c" | "h" => "c",
            "cpp" | "cc" | "cxx" | "hpp" | "hxx" => "cpp",
            "java" => "java",
            _ => return None,
        })
    }

    /// Build a `QualityGateFn` closure that wraps `self.check()`.
    #[must_use]
    pub fn into_closure(self: Arc<Self>) -> QualityGateFn {
        Arc::new(move |files: &[crate::plan::result::RenderedFile]| self.check(files))
    }
}

// ── HealthGateAdapter (PLN2 — feature health-gate) ───────────────────────────

/// Post-commit health gate function type.
#[cfg(feature = "health-gate")]
pub type HealthGateFn = Arc<dyn Fn(&str) -> Result<(), GenerateError> + Send + Sync>;

/// Post-commit enrichment trigger — fires the full daemon enrichment pipeline
/// for generated artifacts (Tantivy FTS, gotcha, wiring, knowledge upsert).
/// Non-blocking, fire-and-forget via `tokio::spawn`.
/// Active under feature `enrichment-gate`.
#[cfg(feature = "enrichment-gate")]
pub type EnrichmentTriggerFn = Arc<dyn Fn(&[String], &str) + Send + Sync + 'static>;

/// CLI-based health gate adapter.
///
/// Executes `touring e2e --depth quick -j` as a blocking subprocess and validates
/// the returned `composite_score` and `confidence_lower` against configured minimum
/// thresholds. Advisory only — does not block commit, only logs warnings and injects
/// RL rewards for the next iteration.
///
/// # Thresholds (POTENCIALIZAR defaults)
/// - `min_composite_score = 0.7`
/// - `min_wilson_lower = 0.3`
#[cfg(feature = "health-gate")]
pub struct HealthGateAdapter {
    min_composite_score: f64,
    min_wilson_lower: f64,
}

#[cfg(feature = "health-gate")]
impl std::fmt::Debug for HealthGateAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("HealthGateAdapter")
            .field("min_composite_score", &self.min_composite_score)
            .field("min_wilson_lower", &self.min_wilson_lower)
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "health-gate")]
impl HealthGateAdapter {
    /// Construct with explicit thresholds.
    #[must_use]
    pub fn with_thresholds(min_composite_score: f64, min_wilson_lower: f64) -> Self {
        Self {
            min_composite_score,
            min_wilson_lower,
        }
    }

    /// Execute `touring e2e --depth quick -j` in `project_root` and check thresholds.
    ///
    /// Returns `Ok(())` if the e2e command succeeds and both thresholds are met.
    /// Returns `Err` if thresholds are violated. Does not fail on e2e non-zero exit
    /// (the e2e command can return non-zero in degraded projects — we still parse).
    ///
    /// Uses `tokio::process::Command` for non-blocking I/O when called within
    /// a `Tokio` runtime (e.g., in the health-gate advisory `tokio::spawn`).
    ///
    /// # Errors
    /// Returns `GenerateError` if the e2e command fails or JSON parsing fails.
    pub async fn check(&self, project_root: &str) -> Result<(), GenerateError> {
        let output = tokio::process::Command::new("touring")
            .args(["e2e", "--depth", "quick", "-j"])
            .current_dir(project_root)
            .output()
            .await
            .map_err(|e| GenerateError::Internal(format!("touring e2e: {e}")))?;

        let json: serde_json::Value = serde_json::from_reader(output.stdout.as_slice())
            .map_err(|e| GenerateError::Internal(format!("parse e2e JSON: {e}")))?;

        // touring e2e returns "overall_score" at root level (not "composite_score").
        let composite = json
            .get("overall_score")
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0);
        // wilson_lower is not available in --depth quick JSON; use overall_score as proxy.
        let wilson_lower = composite;

        // Cache scores for RL reward injection (keyed by project_root path).
        if composite > 0.0 {
            HEALTH_SCORES.insert(project_root.to_string(), composite);
        }

        if composite < self.min_composite_score {
            return Err(GenerateError::Internal(format!(
                "health gate: composite {:.3} < min {:.3}",
                composite, self.min_composite_score
            )));
        }
        if wilson_lower < self.min_wilson_lower {
            return Err(GenerateError::Internal(format!(
                "health gate: wilson_lower {:.3} < min {:.3}",
                wilson_lower, self.min_wilson_lower
            )));
        }
        Ok(())
    }

    /// Build a `HealthGateFn` closure that wraps `self.check()`.
    #[must_use]
    pub fn into_closure(self: Arc<Self>) -> HealthGateFn {
        let this = self;
        Arc::new(move |project_root: &str| {
            let this = Arc::clone(&this);
            // into_closure is sync but check is async; use block_in_place for Tokio runtime.
            tokio::runtime::Handle::current()
                .block_on(async move { this.check(project_root).await })
        })
    }
}

// ── SemanticGraphAdapter (PLN2 section 8.1 — feature `cognitive-nexus`) ──────

/// Cognitive graph adapter wrapping `touring_intelligence::reasoning::SemanticGraph`.
///
/// Persists each submitted plan as a `MemoryNode` with `NodeType::Concept`,
/// indexed by `plan_id`. Builds the cross-session knowledge graph that the
/// `semantic_graph_fn` and `cognitive_nexus_fn` closures consume.
///
/// # Wiring
///
/// - `semantic_graph_fn` returns `SymbolRef` entries from the plan's
///   `contracts.symbols_must_exist` whenever a plan is processed — these are
///   exactly the symbols the cognitive nexus would have surfaced as related.
/// - `cognitive_nexus_fn` returns `PlanSimilarityScore` proportional to the
///   plan node's `relevance_score` in the graph; never wired plans return `None`.
///
/// # Persistence
///
/// Constructed with a `GraphPersistence` path. The graph is in-memory; explicit
/// `flush()` call writes a snapshot to the persistence path. The adapter does not
/// flush automatically — callers decide when to persist (typically after a
/// successful commit or a checkpoint).
#[cfg(feature = "cognitive-nexus")]
pub struct SemanticGraphAdapter {
    graph: Arc<touring_intelligence::reasoning::semantic_graph::SemanticGraph>,
}

#[cfg(feature = "cognitive-nexus")]
impl std::fmt::Debug for SemanticGraphAdapter {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SemanticGraphAdapter")
            .finish_non_exhaustive()
    }
}

#[cfg(feature = "cognitive-nexus")]
impl SemanticGraphAdapter {
    /// Construct an in-memory adapter persisting to `persistence_path`.
    ///
    /// The persistence path is only touched when `flush()` is called explicitly.
    #[must_use]
    pub fn new(persistence_path: std::path::PathBuf) -> Self {
        let persistence = Arc::new(
            touring_intelligence::reasoning::persistence::GraphPersistence::new(persistence_path),
        );
        Self {
            graph: Arc::new(
                touring_intelligence::reasoning::semantic_graph::SemanticGraph::new(persistence),
            ),
        }
    }

    /// Construct with a pre-existing `SemanticGraph` (for tests or composition).
    #[must_use]
    pub fn from_graph(
        graph: Arc<touring_intelligence::reasoning::semantic_graph::SemanticGraph>,
    ) -> Self {
        Self { graph }
    }

    /// Returns a clone of the underlying graph for direct access.
    #[must_use]
    pub fn graph(&self) -> Arc<touring_intelligence::reasoning::semantic_graph::SemanticGraph> {
        Arc::clone(&self.graph)
    }

    /// Records a plan in the cognitive graph as a `Concept` node.
    ///
    /// Idempotent — re-recording the same `plan_id` increments `access_count`
    /// thanks to `SemanticGraph::add_node` upsert semantics.
    ///
    /// # Errors
    ///
    /// Returns `Err(String)` when the underlying graph lock is poisoned —
    /// only occurs after a previous panic inside the graph mutex, which is
    /// catastrophic and should not be observed in normal operation.
    pub fn record_plan(
        &self,
        plan: &GeneratorPlan,
    ) -> Result<(), touring_intelligence::reasoning::semantic_graph::SemanticGraphError> {
        let node = touring_intelligence::reasoning::semantic_graph::MemoryNode::new(
            plan.plan_id.to_string(),
            plan.intent.clone(),
            touring_intelligence::reasoning::semantic_graph::NodeType::Concept,
        );
        self.graph.add_node(node)
    }

    /// Builds the `SemanticGraphFn` closure consumed by `GeneratorContext`.
    ///
    /// The closure records the plan in the graph (best-effort; ignores errors)
    /// and returns the plan's required symbols as the "related" set — this
    /// gives downstream stages immediate access to the plan's contractual
    /// dependencies for context enrichment.
    #[must_use]
    pub fn into_semantic_graph_fn(self: Arc<Self>) -> SemanticGraphFn {
        Arc::new(move |plan: &GeneratorPlan| {
            // Best-effort record; an error here is logged but does not block.
            if let Err(e) = self.record_plan(plan) {
                tracing::warn!(plan_id = %plan.plan_id, error = %e, "graph record failed");
            }
            if plan.contracts.symbols_must_exist.is_empty() {
                None
            } else {
                Some(plan.contracts.symbols_must_exist.clone())
            }
        })
    }

    /// Builds the `CognitiveNexusFn` closure consumed by `GeneratorContext`.
    ///
    /// The closure looks up a plan by id (or arbitrary key) and returns a
    /// similarity score based on the node's `relevance_score` if present.
    /// Unknown keys yield `None`, allowing graceful degradation.
    #[must_use]
    pub fn into_cognitive_nexus_fn(self: Arc<Self>) -> CognitiveNexusFn {
        Arc::new(move |key: &str| {
            // Walk the neighbors of the queried node — if it has any, the
            // graph "knows" about it; the relevance count is normalised to
            // a 0..1 confidence score capped at 1.0 (saturates at ~10 neighbors).
            let neighbors = self.graph.neighbors(key);
            if neighbors.is_empty() {
                return None;
            }
            #[allow(clippy::cast_precision_loss)]
            let raw = (neighbors.len() as f64) / 10.0;
            Some(PlanSimilarityScore::clamped(raw.min(1.0)))
        })
    }

    /// Records a directed edge between two `plan_id`s in the graph.
    ///
    /// Used to express "plan B was triggered by plan A" or "plan B is similar
    /// to plan A". Errors are returned to the caller for visibility.
    ///
    /// # Errors
    ///
    /// Returns `Err` when:
    /// - Either `from_id` or `to_id` is not a node in the graph
    /// - The edge would form a self-loop (`from_id == to_id`)
    /// - The underlying graph lock is poisoned (catastrophic — should never happen)
    pub fn link_plans(
        &self,
        from_id: &str,
        to_id: &str,
        weight: f32,
    ) -> Result<(), touring_intelligence::reasoning::semantic_graph::SemanticGraphError> {
        self.graph.add_edge(from_id, to_id, weight)
    }
}

// ── PlanSimilarityScore (PLN2 section 8.1) ───────────────────────────────────

/// Semantic similarity score between plans — wraps `NormalizedScore` for clarity.
///
/// Used by `cognitive_nexus_fn` to rank cross-session plan matches.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize, JsonSchema)]
pub struct PlanSimilarityScore(pub NormalizedScore);

impl PlanSimilarityScore {
    /// Construct from a raw `f64`. Saturates to `[0.0, 1.0]`.
    #[must_use]
    pub fn clamped(v: f64) -> Self {
        Self(NormalizedScore::clamped(v))
    }

    /// Returns the inner `f64` value.
    #[must_use]
    pub fn value(self) -> f64 {
        self.0.value()
    }
}
