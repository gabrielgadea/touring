//! Unified blast radius analysis with strategy dispatch.
//!
//! Wraps existing implementations in touring-ast behind a common trait,
//! selecting the best available strategy based on latency tier and index state.
//!
//! # Strategies
//!
//! | Strategy | Tier | Feature | Algorithm |
//! |---|---|---|---|
//! | [`BfsStrategy`] | `Medium` (<10 ms) | `blast-radius` | Exact BFS over SymbolIndex reverse-dep graph |
//! | [`HnswStrategy`] | `Slow` (>10 ms) | `ann-blast` | Approximate nearest-neighbour via 64-dim FNV-1a path embeddings |
//!
//! ## Factory methods
//!
//! Use [`BlastRadiusEngine::bfs_only`] for the common exact-BFS case:
//!
//! ```no_run
//! use std::sync::Arc;
//! use touring_code::ast::SymbolIndex;
//! use touring_analysis::blast_radius::BlastRadiusEngine;
//!
//! let index = Arc::new(SymbolIndex::new());
//! let engine = BlastRadiusEngine::bfs_only(index);
//! ```
//!
//! Use [`BlastRadiusEngine::hnsw_only`] (requires feature `ann-blast`) for deep
//! analysis paths where approximate neighbours over the whole file graph are
//! acceptable:
//!
//! ```ignore
//! // Only available with `ann-blast` feature
//! let engine = BlastRadiusEngine::hnsw_only(&symbol_index, 10);
//! ```
//!
//! ## Feature gates
//!
//! - `blast-radius` — enables this entire module and `BfsStrategy`
//! - `ann-blast` — additionally enables `HnswStrategy`, [`HNSW_EMBED_DIM`],
//!   and [`path_hash_embedding`]; requires `touring-simd/ann`

mod bfs;
pub mod warning;

#[cfg(feature = "ann-blast")]
mod hnsw;

#[cfg(feature = "ann-blast")]
pub use hnsw::{HNSW_EMBED_DIM, HnswStrategy, path_hash_embedding};

pub use bfs::BfsStrategy;
pub use warning::BlastWarning;

use crate::engine::AnalysisConfig;
use std::sync::Arc;
use std::time::Instant;
use touring_code::ast::SymbolIndex;

/// Latency tier for strategy selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
pub enum LatencyTier {
    /// <1ms — in-memory graph only.
    Fast,
    /// <10ms — SQLite + BFS traversal.
    Medium,
    /// >10ms — full codebase scan.
    Slow,
}

/// A file affected by a change, with distance and co-edit weight.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct AffectedFile {
    /// Relative path to the affected file.
    pub path: String,
    /// Hop distance from the changed file (1 = direct dependent).
    pub distance: usize,
    /// Co-edit weight from historical correlation (0.0–1.0).
    pub co_edit_weight: f64,
}

/// Result of a blast radius computation.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BlastRadiusResult {
    /// The file that was changed.
    pub start_file: String,
    /// Files affected by the change, sorted by distance.
    pub affected_files: Vec<AffectedFile>,
    /// Which strategy produced this result.
    pub strategy_used: String,
    /// Computation time in milliseconds.
    pub duration_ms: u64,
    /// Whether the result was truncated due to budget or depth limit.
    pub truncated: bool,
}

/// Strategy trait for blast radius computation.
///
/// Implementations wrap different algorithms (BFS, HNSW, petgraph).
pub trait BlastRadiusStrategy: Send + Sync {
    /// Human-readable name of the strategy.
    fn name(&self) -> &'static str;

    /// Compute blast radius from a single file path.
    fn compute(&self, start_file: &str, config: &AnalysisConfig) -> BlastRadiusResult;

    /// Latency tier for strategy selection.
    fn latency_tier(&self) -> LatencyTier;
}

/// Unified engine that selects the best available strategy.
///
/// On the hook path, uses BFS with depth cap from AnalysisConfig.
/// For deep analysis, uses the full BFS without depth limit.
pub struct BlastRadiusEngine {
    strategies: Vec<Box<dyn BlastRadiusStrategy>>,
}

impl BlastRadiusEngine {
    /// Create an engine with the given strategies.
    pub fn new(strategies: Vec<Box<dyn BlastRadiusStrategy>>) -> Self {
        Self { strategies }
    }

    /// HNSW factory — NOT recommended for BlastRadiusEngine use.
    ///
    /// ## Why this factory exists but is not wired into BlastRadiusEngine
    ///
    /// `BlastRadiusEngine::compute()` selects strategies by latency tier — there
    /// is no deep-only gate, so a sole `HnswStrategy` (which returns
    /// `LatencyTier::Slow`) would be selected for *all* callers, including the
    /// hot hook-path where exact BFS is required.
    ///
    /// HNSW is instead activated via `HnswSignalLayer` in the signal pipeline
    /// (`touring-hooks/src/shared/signal_pipeline.rs:630`), which is CILA-gated
    /// (runs only at CILA >= 5) — the correct deep-analysis entry point.
    ///
    /// This factory is preserved for any future direct `BlastRadiusEngine`
    /// users who genuinely want approximate-only behaviour.
    #[cfg(feature = "ann-blast")]
    pub fn hnsw_only(symbol_index: &touring_code::ast::SymbolIndex, k: usize) -> Self {
        Self::new(vec![Box::new(HnswStrategy::new(symbol_index, k))])
    }

    /// Create an engine pre-loaded with the BFS exact strategy.
    ///
    /// Convenience factory for the most common single-strategy setup.
    /// `BfsStrategy` uses `LatencyTier::Medium` and is suitable for all paths.
    ///
    /// For hook-path use (with budget), pass the result to
    /// [`BlastRadiusEngine::compute_with_start`] with a shared pipeline timer.
    ///
    /// # Example
    /// ```no_run
    /// use std::sync::Arc;
    /// use touring_code::ast::SymbolIndex;
    /// use touring_analysis::blast_radius::BlastRadiusEngine;
    ///
    /// let index = Arc::new(SymbolIndex::new());
    /// let engine = BlastRadiusEngine::bfs_only(index);
    /// ```
    pub fn bfs_only(index: Arc<SymbolIndex>) -> Self {
        Self::new(vec![Box::new(BfsStrategy::new(index))])
    }

    /// Compute blast radius using the best available strategy.
    ///
    /// Strategy selection: picks the first strategy whose latency tier
    /// is compatible with the budget. Falls back to the last strategy.
    pub fn compute(&self, start_file: &str, config: &AnalysisConfig) -> BlastRadiusResult {
        self.compute_with_start(start_file, config, Instant::now())
    }

    /// Compute blast radius with an externally-supplied pipeline start time.
    ///
    /// Use this from E2E pipelines that share a single `budget_ms` across
    /// multiple phases. If the shared budget is already exhausted when this
    /// method is called, it returns immediately with `strategy_used =
    /// "budget-exceeded"` and `truncated = true` — no BFS is attempted.
    pub fn compute_with_start(
        &self,
        start_file: &str,
        config: &AnalysisConfig,
        pipeline_start: Instant,
    ) -> BlastRadiusResult {
        // Pre-execution budget check: abort immediately if budget is already
        // exhausted before we even start the strategy.
        if let Some(budget_ms) = config.budget_ms {
            let elapsed = pipeline_start.elapsed().as_millis() as u64;
            if elapsed >= budget_ms {
                return BlastRadiusResult {
                    start_file: start_file.to_string(),
                    affected_files: vec![],
                    strategy_used: "budget-exceeded".to_string(),
                    duration_ms: elapsed,
                    truncated: true,
                };
            }
        }

        if self.strategies.is_empty() {
            return BlastRadiusResult {
                start_file: start_file.to_string(),
                affected_files: vec![],
                strategy_used: "none".to_string(),
                duration_ms: pipeline_start.elapsed().as_millis() as u64,
                truncated: false,
            };
        }

        // Select strategy based on budget
        let strategy = if config.budget_ms.is_some() {
            // Hook path: prefer faster strategies
            self.strategies
                .iter()
                .find(|s| s.latency_tier() != LatencyTier::Slow)
                .or_else(|| self.strategies.first())
                .expect("non-empty strategies checked above")
        } else {
            // Deep analysis: use most thorough strategy (last one)
            self.strategies.last().expect("non-empty strategies")
        };

        let mut result = strategy.compute(start_file, config);

        // Always use engine-level wall-clock time for consistency
        result.duration_ms = pipeline_start.elapsed().as_millis() as u64;

        // Enforce budget truncation post-hoc
        if let Some(budget) = config.budget_ms
            && result.duration_ms > budget
        {
            result.truncated = true;
        }

        result
    }

    /// Compute blast radius with a hard wall-clock timeout.
    ///
    /// Wraps `compute_with_start` with a dedicated [`AnalysisConfig`] that
    /// sets `budget_ms` to the given `budget`. Returns `None` if the result was
    /// truncated (budget exceeded), `Some(result)` otherwise.
    ///
    /// This is the preferred method for hook-path callers that need a strict
    /// latency bound without sharing an external pipeline timer.
    ///
    /// # Example
    ///
    /// ```
    /// use std::sync::Arc;
    /// use std::time::Duration;
    /// use touring_code::ast::SymbolIndex;
    /// use touring_analysis::blast_radius::BlastRadiusEngine;
    ///
    /// let index = Arc::new(SymbolIndex::new());
    /// let engine = BlastRadiusEngine::bfs_only(index);
    /// // Returns None if computation exceeds 40ms.
    /// let result = engine.compute_with_timeout("src/lib.rs", Duration::from_millis(40));
    /// ```
    pub fn compute_with_timeout(
        &self,
        start_file: &str,
        budget: std::time::Duration,
    ) -> Option<BlastRadiusResult> {
        let budget_ms = budget.as_millis() as u64;
        let config = AnalysisConfig::hook_path().with_budget(budget_ms);
        let pipeline_start = Instant::now();
        let result = self.compute_with_start(start_file, &config, pipeline_start);
        if result.truncated {
            tracing::debug!(
                file = %start_file,
                elapsed_ms = result.duration_ms,
                budget_ms = budget_ms,
                "blast radius compute_with_timeout: result truncated (budget exceeded)"
            );
            None
        } else {
            Some(result)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct MockStrategy {
        name: &'static str,
        tier: LatencyTier,
        file_count: usize,
    }

    impl BlastRadiusStrategy for MockStrategy {
        fn name(&self) -> &'static str {
            self.name
        }

        fn compute(&self, start_file: &str, _config: &AnalysisConfig) -> BlastRadiusResult {
            BlastRadiusResult {
                start_file: start_file.to_string(),
                affected_files: (0..self.file_count)
                    .map(|i| AffectedFile {
                        path: format!("file_{i}.rs"),
                        distance: i + 1,
                        co_edit_weight: 0.0,
                    })
                    .collect(),
                strategy_used: self.name.to_string(),
                duration_ms: 1,
                truncated: false,
            }
        }

        fn latency_tier(&self) -> LatencyTier {
            self.tier
        }
    }

    #[test]
    fn test_engine_empty_strategies() {
        let engine = BlastRadiusEngine::new(vec![]);
        let result = engine.compute("test.rs", &AnalysisConfig::standard());
        assert!(result.affected_files.is_empty());
        assert_eq!(result.strategy_used, "none");
    }

    #[test]
    fn test_engine_selects_fast_strategy_on_hook_path() {
        let engine = BlastRadiusEngine::new(vec![
            Box::new(MockStrategy {
                name: "fast",
                tier: LatencyTier::Fast,
                file_count: 2,
            }),
            Box::new(MockStrategy {
                name: "slow",
                tier: LatencyTier::Slow,
                file_count: 10,
            }),
        ]);
        let result = engine.compute("test.rs", &AnalysisConfig::hook_path());
        assert_eq!(result.strategy_used, "fast");
        assert_eq!(result.affected_files.len(), 2);
    }

    #[test]
    fn test_engine_selects_last_strategy_for_deep() {
        let engine = BlastRadiusEngine::new(vec![
            Box::new(MockStrategy {
                name: "fast",
                tier: LatencyTier::Fast,
                file_count: 2,
            }),
            Box::new(MockStrategy {
                name: "thorough",
                tier: LatencyTier::Slow,
                file_count: 10,
            }),
        ]);
        let result = engine.compute("test.rs", &AnalysisConfig::deep());
        assert_eq!(result.strategy_used, "thorough");
        assert_eq!(result.affected_files.len(), 10);
    }

    #[test]
    fn test_affected_file_distance_ordering() {
        let engine = BlastRadiusEngine::new(vec![Box::new(MockStrategy {
            name: "bfs",
            tier: LatencyTier::Medium,
            file_count: 5,
        })]);
        let result = engine.compute("root.rs", &AnalysisConfig::standard());
        for (i, f) in result.affected_files.iter().enumerate() {
            assert_eq!(f.distance, i + 1);
        }
    }

    #[test]
    fn test_compute_with_start_exhausted_budget_returns_early() {
        let engine = BlastRadiusEngine::new(vec![Box::new(MockStrategy {
            name: "bfs",
            tier: LatencyTier::Medium,
            file_count: 10,
        })]);
        // Simulate a pipeline that started 1 second ago with a 1ms budget.
        let past = Instant::now() - std::time::Duration::from_secs(1);
        let config = AnalysisConfig {
            budget_ms: Some(1),
            ..AnalysisConfig::standard()
        };
        let result = engine.compute_with_start("src/lib.rs", &config, past);
        assert_eq!(result.strategy_used, "budget-exceeded");
        assert!(result.truncated);
        assert!(result.affected_files.is_empty());
    }

    #[test]
    fn test_compute_with_start_fresh_budget_runs_normally() {
        let engine = BlastRadiusEngine::new(vec![Box::new(MockStrategy {
            name: "bfs",
            tier: LatencyTier::Medium,
            file_count: 3,
        })]);
        let config = AnalysisConfig::hook_path();
        let result = engine.compute_with_start("src/lib.rs", &config, Instant::now());
        assert_ne!(result.strategy_used, "budget-exceeded");
        assert_eq!(result.affected_files.len(), 3);
    }

    #[test]
    fn test_compute_delegates_to_compute_with_start() {
        let engine = BlastRadiusEngine::new(vec![Box::new(MockStrategy {
            name: "bfs",
            tier: LatencyTier::Medium,
            file_count: 2,
        })]);
        let result = engine.compute("src/lib.rs", &AnalysisConfig::standard());
        assert_eq!(result.affected_files.len(), 2);
        assert_eq!(result.start_file, "src/lib.rs");
    }
}
