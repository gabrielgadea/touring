//! Unified Deep Code Analysis Engine for Touring.
//!
//! Provides a composable pipeline for multi-dimensional code analysis:
//! - **Blast radius**: unified API over BFS/HNSW/petgraph strategies
//! - **Quality**: antipatterns, complexity, unwrap audit, error coverage
//! - **Wiring**: orphan detection, functional chain tracing, module ecosystem integration
//! - **Health**: composite score with Wilson confidence intervals
//! - **Temporal**: edit velocity, churn rate, bash success rate, KS drift detection
//! - **Knowledge**: file stats, language distribution, hot files, gotcha rate
//! - **Learning**: RL health, Wilson scores, Q-table richness, LinUCB arms
//! - **E2E**: schema-guarded analysis phases
//!
//! # Quick Start
//!
//! ```no_run
//! use touring_analysis::{AnalysisConfig, AnalysisPipeline};
//!
//! let conn = rusqlite::Connection::open("knowledge.db").expect("open knowledge.db");
//! let pipeline = AnalysisPipeline::new(&conn, AnalysisConfig::standard());
//! let report = pipeline.run(".");
//! println!("Health: {:.2} ({:?})", report.composite_score, report.status);
//! ```
//!
//! # Public Blast Radius API
//!
//! The `blast-radius` feature exposes a strategy trait and two built-in implementations:
//!
//! | Type | Feature | Tier | Description |
//! |---|---|---|---|
//! | [`BfsStrategy`] | `blast-radius` | Medium | Exact BFS over SymbolIndex reverse-dep graph |
//! | [`HnswStrategy`] | `ann-blast` | Slow | Approximate-nearest-neighbour via HNSW path embeddings |
//!
//! Both are created through factory methods on [`BlastRadiusEngine`]:
//!
//! ```no_run
//! use std::sync::Arc;
//! use touring_code::ast::SymbolIndex;
//! use touring_analysis::BlastRadiusEngine;
//!
//! let index = Arc::new(SymbolIndex::new());
//!
//! // Exact BFS — suitable for all paths including hot hooks
//! let engine = BlastRadiusEngine::bfs_only(index);
//! ```
//!
//! The `ann-blast` factory (`BlastRadiusEngine::hnsw_only`) and its associated
//! constants ([`HNSW_EMBED_DIM`], [`path_hash_embedding`]) are available only
//! when both `blast-radius` and `ann-blast` features are enabled.
//!
//! ## Primary Consumer
//!
//! `touring-hooks/src/cli_e2e.rs` is the main consumer of this crate at runtime.
//! Its `phase_learning()` function drives the LinUCB RL feedback loop by calling
//! [`compute_blast_reward`], which normalises the symbol index edge count into a
//! reward in `[0.1, 1.0]` for injection into the `ArmKind::BlastRadius` bandit arm.
//!
//! This crate is stateless — it takes DB connections and indexes as parameters
//! rather than owning persistent state. The daemon owns all state in HookRuntime;
//! this crate is pure computation over that state.
//!
//! # Feature-Gated Pattern Scanners
//!
//! Two SIMD-accelerated functions use `aho-corasick` for multi-pattern matching.
//! Both are always exported but return empty vecs when their feature is off:
//!
//! | Function | Feature gate | Patterns matched |
//! |---|---|---|
//! | [`scan_dead_patterns`] | `simd-wiring` | `_unused`, `_dead`, `_old`, `_deprecated`, `_legacy`, `_stub` |
//! | [`detect_churn_patterns`] | `simd-temporal-ac` | `tmp/`, `_bak`, `.bak`, `_old`, `_backup` |
//!
//! `touring-hooks` activates both features, so at runtime these functions use the
//! aho-corasick fast path. External crates that do not need SIMD scanning can omit
//! the features and get the zero-cost fallback automatically.
//!
//! # pln2 Integration Points (cross-audit session 2026-04-07)
//!
//! The following APIs were wired by `touring-hooks` as part of pln2:
//!
//! - `scan_dead_patterns` — called in `cli_wiring_orphans` to enrich orphan output
//! - `detect_churn_patterns` — called in `cli_e2e::phase_wiring` for churn metrics
//! - `WiringFingerprintStore` + `analyze_wiring_incremental` — incremental wiring in E2E
//! - `OtelConfig` + `init_otel_subscriber` — OTEL init in E2E deep path
//! - `analysis_reward_from_report` — RL reward closure in E2E cross-validation phase

#![deny(missing_docs)]
// RBP-01 elite-lint ratchet (2026-06-16): prod-unwrap-free (the `.unwrap()`
// occurrences are detector pattern-strings in quality/{antipatterns,unwrap_audit}.rs,
// not real calls) — lock against future bare unwrap in non-test code.
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

pub mod error;
pub use error::{AnalysisError, AnalysisResult};

pub mod error_rate;
pub use error_rate::{ErrorRateTracker, ModuleErrorRate};

pub mod engine;

#[cfg(feature = "blast-radius")]
pub mod blast_radius;

#[cfg(feature = "quality")]
pub mod quality;

/// Sentrux Master Plan Wave 2 P3 (2026-05-09) — TOML metric budget DSL.
///
/// `MetricRule` budgets evaluated against the [`quality::signal::Workspace`]
/// and [`quality::signal::WorkspaceQualitySignal`] computed in Wave 1.
/// Complementary to the YAML+regex autofix engine in `touring_foundation::rules`
/// (different naming `MetricRule` to avoid homonimia per VP-Scout Cadeia 4).
pub mod rules;

#[cfg(feature = "wiring")]
pub mod wiring;
#[cfg(feature = "wiring")]
pub use wiring::module_cycles::{ModuleCycleAnalyzer, ModuleCycleReport};
#[cfg(feature = "wiring")]
pub use wiring::{
    ChainResult, CyclePath, OrphanResult, WiringFingerprintStore, WiringReport, analyze_chains,
    analyze_wiring, analyze_wiring_incremental, count_import_cycles, count_orphans,
    dead_code_detect, detect_import_cycles, scan_dead_patterns,
};

pub mod e2e;
pub mod health;
pub mod pipeline;

#[cfg(feature = "temporal")]
pub mod temporal;

pub mod cache;
pub mod knowledge;
pub mod learning;
pub mod report;

// Wave 5 (2026-04-18) — RustSec advisory DB integration.
// See src/security.rs for the SecurityDb / SecurityAdvisory API.
pub mod osv;
pub use osv::{OsvBatchQuery, OsvPackage, OsvQuery};
pub mod security;
pub use security::{SecurityAdvisory, SecurityDb};

// Re-exports for convenience
pub use engine::{AnalysisConfig, Depth};

#[cfg(feature = "blast-radius")]
pub use blast_radius::{
    AffectedFile, BfsStrategy, BlastRadiusEngine, BlastRadiusResult, BlastRadiusStrategy,
    LatencyTier,
};

#[cfg(all(feature = "blast-radius", feature = "ann-blast"))]
pub use blast_radius::{HNSW_EMBED_DIM, HnswStrategy, path_hash_embedding};
pub use cache::CachedAnalysisPipeline;
pub use e2e::{AnalysisSummary, E2eConfig, run_e2e, schema_guard};
pub use health::{CodeHealthReport, HealthDimension, HealthStatus};
pub use knowledge::{KnowledgeReport, analyze_knowledge};
pub use learning::{LearningReport, RewardTrend, analyze_learning};
pub use pipeline::{
    AnalysisInsights, AnalysisPipeline, AnalysisPipelineBuilder, OtelConfig, adaptive_pool_size,
    init_otel_subscriber,
};
pub use report::{DimensionDiff, HealthDiff, MetricsDashboard};

#[cfg(feature = "quality")]
pub use quality::{
    TestProxy, analyze_antipatterns, analyze_complexity, analyze_unwraps,
    estimate_cognitive_complexity,
    quality_finding::QualityFinding,
    security::{SecurityAnalyzer, SecurityReport},
};

#[cfg(feature = "temporal")]
pub use temporal::{
    TrendDirection, TrendReport, analyze_trends, detect_churn_patterns, quality_trend,
};

pub use learning::{analysis_reward_from_report, compute_blast_reward};
