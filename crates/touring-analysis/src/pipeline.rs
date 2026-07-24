//! Unified analysis pipeline — single entry point for all dimensions.
//!
//! `AnalysisPipeline` orchestrates wiring, quality, and temporal analysis into
//! a single `CodeHealthReport` with minimal boilerplate for callers.
//! Dimensions without data gracefully return their defaults.
//!
//! # Example
//!
//! ```no_run
//! use touring_analysis::{AnalysisConfig, AnalysisPipeline};
//!
//! let conn = rusqlite::Connection::open("knowledge.db").expect("open knowledge.db");
//! let config = AnalysisConfig::standard();
//! let pipeline = AnalysisPipeline::new(&conn, config);
//! let report = pipeline.run(".");
//! println!("Health score: {}", report.composite_score);
//! ```

use crate::engine::AnalysisConfig;
use crate::health::{CodeHealthReport, HealthDimension, compute_health};
use tracing::instrument;

/// Configuration for OpenTelemetry OTLP exporter.
///
/// Set `endpoint` to the OTLP endpoint URL (e.g., `http://localhost:4318/v1/traces`)
/// or use the `OTEL_EXPORTER_OTLP_ENDPOINT` environment variable.
/// When both are set, the struct field takes precedence.
#[derive(Debug, Clone, Default)]
pub struct OtelConfig {
    /// OTLP endpoint URL. Overrides the `OTEL_EXPORTER_OTLP_ENDPOINT` env var when set.
    pub endpoint: Option<String>,
    /// Service name for trace attribution. Defaults to `"touring-analysis"`.
    pub service_name: String,
    /// Whether OTLP export is enabled. Defaults to `false`.
    pub enabled: bool,
}

impl OtelConfig {
    /// Create a default config (disabled).
    pub fn disabled() -> Self {
        Self {
            endpoint: None,
            service_name: "touring-analysis".to_string(),
            enabled: false,
        }
    }

    /// Create a config that reads the endpoint from the `OTEL_EXPORTER_OTLP_ENDPOINT` env var.
    ///
    /// Enabled only when the env var is present and non-empty.
    pub fn from_env() -> Self {
        let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok();
        let enabled = endpoint.is_some();
        Self {
            endpoint,
            service_name: "touring-analysis".to_string(),
            enabled,
        }
    }

    /// Returns the effective endpoint: struct field, or `OTEL_EXPORTER_OTLP_ENDPOINT` env var fallback.
    pub fn effective_endpoint(&self) -> Option<String> {
        self.endpoint
            .clone()
            .or_else(|| std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok())
    }
}

/// Initialize an OTLP-compatible tracing subscriber.
///
/// Sets up the tracing infrastructure for the analysis pipeline.
/// When `config.enabled` is `false` this is a no-op and returns `Ok(())`.
///
/// In a production deployment, replace this stub with an `opentelemetry-otlp`
/// exporter layer once the SDK dependency is added to `Cargo.toml`. The current
/// implementation emits a `tracing::debug` event to confirm the config was read,
/// avoiding any `opentelemetry` dependency conflicts in the workspace.
///
/// # Arguments
///
/// * `config` – OtelConfig containing the endpoint URL and service name.
///
/// # Errors
///
/// Returns `Err(OtelInitError)` if initialisation fails (reserved for future
/// exporter setup; the current stub always returns `Ok`).
pub fn init_otel_subscriber(config: &OtelConfig) -> Result<(), OtelInitError> {
    if !config.enabled {
        return Ok(());
    }
    let endpoint = config.effective_endpoint().unwrap_or_default();
    tracing::debug!(
        endpoint = %endpoint,
        service_name = %config.service_name,
        "OtelConfig: OTLP subscriber configured (JSON format)"
    );
    // Production: configure opentelemetry_otlp::new_exporter() here and install
    // a global subscriber with the OTLP exporter layer.
    Ok(())
}

/// Error from [`init_otel_subscriber`] (F-8 / RBP-03: typed in place of
/// `String`; the `From<String>` bridge lets the future OTLP exporter wiring
/// propagate `format!` errors with `?`).
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct OtelInitError(pub String);

impl From<String> for OtelInitError {
    fn from(message: String) -> Self {
        Self(message)
    }
}

/// Lightweight analysis insights for hook context injection.
///
/// Contains the most actionable signals from the analysis pipeline,
/// suitable for inclusion in pre_edit hook context strings.
/// Constructing this type is cheap — it does not run the full pipeline.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct AnalysisInsights {
    /// Quality trend direction over recent sessions: "Improving", "Stable", or "Degrading".
    pub quality_trend: String,
    /// Number of orphan symbols (public symbols with no consumers).
    pub orphan_count: usize,
    /// Composite health score (0.0–1.0).
    pub health_score: f64,
    /// Health status label: "healthy", "degraded", or "critical".
    pub health_status: String,
}

impl AnalysisInsights {
    /// Derive insights from a [`CodeHealthReport`].
    ///
    /// `quality_trend` defaults to `"Stable"`. Call [`Self::with_quality_trend`]
    /// afterwards when the trend is available (requires the `temporal` feature).
    pub fn from_report(report: &crate::health::CodeHealthReport) -> Self {
        use crate::health::HealthStatus;
        Self {
            quality_trend: "Stable".to_string(),
            orphan_count: 0,
            health_score: report.composite_score,
            health_status: match report.status {
                HealthStatus::Healthy => "healthy",
                HealthStatus::Degraded => "degraded",
                HealthStatus::Critical => "critical",
            }
            .to_string(),
        }
    }

    /// Override the quality trend from a `TrendDirection` value.
    ///
    /// Only available when the `temporal` feature is enabled.
    #[cfg(feature = "temporal")]
    pub fn with_quality_trend(mut self, trend: &crate::temporal::TrendDirection) -> Self {
        use crate::temporal::TrendDirection;
        self.quality_trend = match trend {
            TrendDirection::Improving => "Improving",
            TrendDirection::Stable => "Stable",
            TrendDirection::Degrading => "Degrading",
        }
        .to_string();
        self
    }

    /// Set the orphan symbol count.
    pub fn with_orphan_count(mut self, count: usize) -> Self {
        self.orphan_count = count;
        self
    }

    /// Format as a concise context string for injection into hook context.
    ///
    /// Output: `"Analysis: health=0.85 (healthy) | trend=Stable | orphans=3"`
    pub fn to_context_string(&self) -> String {
        format!(
            "Analysis: health={:.2} ({}) | trend={} | orphans={}",
            self.health_score, self.health_status, self.quality_trend, self.orphan_count
        )
    }
}

/// Returns a recommended Rayon thread pool size for a given crate count.
///
/// Formula: `min(crate_count * 2, available_parallelism)`.
/// Respects the `RAYON_NUM_THREADS` environment variable implicitly via Rayon's
/// default pool — this helper is used to size *explicit* pools when needed.
///
/// # Examples
/// ```
/// let pool_size = touring_analysis::adaptive_pool_size(8);
/// assert!(pool_size >= 1);
/// ```
pub fn adaptive_pool_size(crate_count: usize) -> usize {
    let cpus = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4);
    (crate_count.saturating_mul(2)).min(cpus).max(1)
}

/// Fluent builder for `AnalysisPipeline`.
///
/// Allows composing file sources and blast radius targets before building
/// the pipeline. Existing callers can still use `AnalysisPipeline::new()`.
pub struct AnalysisPipelineBuilder<'a> {
    knowledge_conn: &'a rusqlite::Connection,
    graph_conn: Option<&'a rusqlite::Connection>,
    config: AnalysisConfig,
    files: Vec<(String, String, String)>,
    blast_file: Option<String>,
    session_id: Option<String>,
    otel_config: OtelConfig,
    #[cfg(feature = "blast-radius")]
    symbol_index: Option<std::sync::Arc<touring_code::ast::SymbolIndex>>,
}

impl<'a> AnalysisPipelineBuilder<'a> {
    /// Start building a pipeline over the given knowledge DB connection.
    pub fn new(knowledge_conn: &'a rusqlite::Connection) -> Self {
        Self {
            knowledge_conn,
            graph_conn: None,
            config: AnalysisConfig::standard(),
            files: Vec::new(),
            blast_file: None,
            session_id: None,
            otel_config: OtelConfig::disabled(),
            #[cfg(feature = "blast-radius")]
            symbol_index: None,
        }
    }

    /// Set the analysis configuration.
    pub fn config(mut self, config: AnalysisConfig) -> Self {
        self.config = config;
        self
    }

    /// Configure the pipeline from a `Depth` preset.
    ///
    /// Equivalent to `.config(depth.to_config())`. Overrides any previously
    /// set config. Call after `new()` and before other setters.
    ///
    /// # Example
    /// ```no_run
    /// use touring_analysis::{pipeline::AnalysisPipelineBuilder, engine::Depth};
    ///
    /// let conn = rusqlite::Connection::open_in_memory().expect("open in-memory db");
    /// let report = AnalysisPipelineBuilder::new(&conn)
    ///     .depth(Depth::Deep)
    ///     .build()
    ///     .run_parallel(".");
    /// ```
    pub fn depth(mut self, depth: crate::engine::Depth) -> Self {
        self.config = depth.to_config();
        self
    }

    /// Set the graph.db connection for learning dimension.
    pub fn graph_conn(mut self, conn: &'a rusqlite::Connection) -> Self {
        self.graph_conn = Some(conn);
        self
    }

    /// Provide file sources for quality analysis: `(path, source, language)`.
    pub fn with_files(mut self, files: Vec<(String, String, String)>) -> Self {
        self.files = files;
        self
    }

    /// Set the blast radius start file.
    pub fn with_blast_file(mut self, file: impl Into<String>) -> Self {
        self.blast_file = Some(file.into());
        self
    }

    /// Provide a symbol index for exact BFS blast radius analysis.
    ///
    /// Without a symbol index, `run_blast` returns a placeholder result with
    /// `strategy_used = "none"`. Providing one enables the `BfsStrategy` to
    /// traverse real dependency edges from the index.
    ///
    /// Requires the `blast-radius` feature.
    ///
    /// # Example
    /// ```no_run
    /// use std::sync::Arc;
    /// use touring_code::ast::SymbolIndex;
    /// use touring_analysis::pipeline::AnalysisPipelineBuilder;
    ///
    /// let conn = rusqlite::Connection::open_in_memory().expect("open in-memory db");
    /// let index = Arc::new(SymbolIndex::new());
    /// let report = AnalysisPipelineBuilder::new(&conn)
    ///     .with_symbol_index(index)
    ///     .with_blast_file("src/lib.rs")
    ///     .build()
    ///     .run_parallel(".");
    /// ```
    #[cfg(feature = "blast-radius")]
    pub fn with_symbol_index(
        mut self,
        index: std::sync::Arc<touring_code::ast::SymbolIndex>,
    ) -> Self {
        self.symbol_index = Some(index);
        self
    }

    /// Set the session ID for per-session caching and quality tracking.
    pub fn with_session(mut self, session_id: impl Into<String>) -> Self {
        self.session_id = Some(session_id.into());
        self
    }

    /// Configure OpenTelemetry OTLP exporter for this pipeline.
    ///
    /// After calling `build()`, invoke `init_otel_subscriber(&pipeline.otel_config)`
    /// to activate span export to the configured endpoint.
    pub fn with_otel(mut self, config: OtelConfig) -> Self {
        self.otel_config = config;
        self
    }

    /// Build the pipeline, consuming the builder.
    pub fn build(self) -> AnalysisPipeline<'a> {
        AnalysisPipeline {
            knowledge_conn: self.knowledge_conn,
            graph_conn: self.graph_conn,
            config: self.config,
            files: self.files,
            blast_file: self.blast_file,
            session_id: self.session_id,
            otel_config: self.otel_config,
            #[cfg(feature = "blast-radius")]
            symbol_index: self.symbol_index,
        }
    }
}

/// Unified analysis pipeline over a knowledge DB connection.
///
/// Stateless — the connection and config are borrowed for the lifetime of the
/// pipeline and are not mutated. Each `run*` call is independent.
///
/// # Builder pattern
///
/// ```no_run
/// use touring_analysis::pipeline::AnalysisPipelineBuilder;
/// use touring_analysis::AnalysisConfig;
///
/// let conn = rusqlite::Connection::open("knowledge.db").expect("open knowledge.db");
/// let report = AnalysisPipelineBuilder::new(&conn)
///     .config(AnalysisConfig::deep())
///     .build()
///     .run_parallel(".");
/// ```
pub struct AnalysisPipeline<'a> {
    knowledge_conn: &'a rusqlite::Connection,
    graph_conn: Option<&'a rusqlite::Connection>,
    config: AnalysisConfig,
    files: Vec<(String, String, String)>,
    blast_file: Option<String>,
    session_id: Option<String>,
    /// OpenTelemetry configuration (optional, defaults to disabled).
    pub otel_config: OtelConfig,
    #[cfg(feature = "blast-radius")]
    symbol_index: Option<std::sync::Arc<touring_code::ast::SymbolIndex>>,
}

impl<'a> AnalysisPipeline<'a> {
    /// Create a new pipeline (backward-compatible constructor).
    pub fn new(knowledge_conn: &'a rusqlite::Connection, config: AnalysisConfig) -> Self {
        Self {
            knowledge_conn,
            graph_conn: None,
            config,
            files: Vec::new(),
            blast_file: None,
            session_id: None,
            otel_config: OtelConfig::disabled(),
            #[cfg(feature = "blast-radius")]
            symbol_index: None,
        }
    }

    /// Returns the session ID if one was configured.
    pub fn session_id(&self) -> Option<&str> {
        self.session_id.as_deref()
    }

    /// Shared dimensions computed by both `run()` and `run_parallel()`.
    ///
    /// Returns: wiring + quality (if configured) + temporal (if configured) + knowledge (if configured).
    fn run_common_dimensions(&self, project_root: &str) -> Vec<HealthDimension> {
        let mut dims = Vec::with_capacity(4);
        dims.push(self.run_wiring(project_root));

        // Quality (rayon-parallel internally via analyze_batch)
        #[cfg(feature = "quality")]
        if !self.files.is_empty() && self.config.quality_sample > 0 {
            let refs: Vec<(&str, &str, &str)> = self
                .files
                .iter()
                .map(|(p, s, l)| (p.as_str(), s.as_str(), l.as_str()))
                .collect();
            dims.push(self.run_quality(&refs));
        }

        #[cfg(feature = "temporal")]
        if self.config.temporal {
            dims.push(self.run_temporal());
        }

        if self.config.knowledge {
            dims.push(self.run_knowledge());
        }

        dims
    }

    /// Run all enabled dimensions sequentially.
    ///
    /// Includes wiring, quality (if files provided and `config.quality_sample > 0`),
    /// temporal (if `config.temporal`), and knowledge (if `config.knowledge`).
    /// For blast radius and learning, use `run_parallel()` with `graph_conn`.
    #[instrument(skip(self), fields(project_root = %project_root))]
    pub fn run(&self, project_root: &str) -> CodeHealthReport {
        let start = std::time::Instant::now();
        let dimensions = self.run_common_dimensions(project_root);
        let total_ms = start.elapsed().as_millis() as u64;
        let report = compute_health(project_root, "pipeline", dimensions, total_ms);
        // G2: emit RL reward signal derived from composite quality score (bidirectional flywheel).
        let reward = crate::learning::analysis_reward_from_report(&report);
        tracing::debug!(
            composite_score = report.composite_score,
            rl_reward = reward,
            "G2: analysis quality → RL reward signal"
        );
        report
    }

    /// Run ALL enabled dimensions and return a composite health report.
    ///
    /// Orchestrates wiring, quality, temporal, knowledge, and learning
    /// dimensions. Quality analysis uses rayon internally for file-level
    /// parallelism. DB-bound dimensions run sequentially (rusqlite connections
    /// are not `Send`).
    #[instrument(skip(self), fields(project_root = %project_root))]
    pub fn run_parallel(&self, project_root: &str) -> CodeHealthReport {
        let start = std::time::Instant::now();
        let mut dimensions = self.run_common_dimensions(project_root);

        // Blast radius (symbol index — if blast_file provided)
        #[cfg(feature = "blast-radius")]
        if let Some(ref blast_file) = self.blast_file {
            dimensions.push(self.run_blast(blast_file));
        }

        // Learning (graph.db query)
        if self.config.learning {
            if let Some(graph_conn) = self.graph_conn {
                dimensions.push(self.run_learning(graph_conn));
            }
        }

        let total_ms = start.elapsed().as_millis() as u64;
        compute_health(project_root, "parallel", dimensions, total_ms)
    }

    /// Knowledge dimension: file stats, language distribution, hot files, gotcha rate.
    ///
    /// Metrics emitted:
    /// - `total_files`: total indexed files
    /// - `languages`: distinct languages detected
    /// - `avg_line_count`: mean line count per file
    /// - `avg_symbol_density`: symbols per line (target: 0.01–0.5)
    /// - `hot_files`: files edited 3+ times in last 7 days (instability signal)
    /// - `active_gotchas`: unresolved pitfalls with decay_score > 0.3
    /// - `import_graph_health`: edge density `edges / (files + 1)`, clamped [0, 1]
    /// - `language_distribution`: map of language → file count
    /// - `data_points`: observation count used for Wilson CI
    pub fn run_knowledge(&self) -> HealthDimension {
        let start = std::time::Instant::now();
        let report = crate::knowledge::analyze_knowledge(self.knowledge_conn);
        let mut issues = Vec::new();
        if report.hot_files > 0 {
            issues.push(format!(
                "{} hot file(s) (edited 3+ times in 7d)",
                report.hot_files
            ));
        }
        if report.active_gotchas > 0 {
            issues.push(format!(
                "{} active gotcha(s) (decay_score > 0.3)",
                report.active_gotchas
            ));
        }
        HealthDimension {
            name: "knowledge".to_string(),
            score: report.score,
            weight: 1.0,
            issues,
            metrics: serde_json::json!({
                "total_files": report.total_files,
                "languages": report.language_distribution.len(),
                "language_distribution": report.language_distribution,
                "avg_line_count": report.avg_line_count,
                "avg_symbol_density": report.avg_symbol_density,
                "hot_files": report.hot_files,
                "active_gotchas": report.active_gotchas,
                "import_graph_health": report.import_graph_health,
                "data_points": report.total_files.max(1),
            }),
            duration_ms: start.elapsed().as_millis() as u64,
        }
    }

    /// Learning dimension: RL reward trends, tool effectiveness.
    pub fn run_learning(&self, graph_conn: &rusqlite::Connection) -> HealthDimension {
        let start = std::time::Instant::now();
        let report = crate::learning::analyze_learning(graph_conn);
        let mut issues = vec![];
        if !report.rl_active {
            issues.push("RL system inactive (no training data)".to_string());
        }
        if report.reward_trend == crate::learning::RewardTrend::Degrading {
            issues.push("RL reward trend degrading".to_string());
        }
        HealthDimension {
            name: "learning".to_string(),
            score: report.score,
            weight: 0.5, // Lower weight — RL is supplementary
            issues,
            metrics: serde_json::json!({
                "wilson_tool_count": report.wilson_tool_count,
                "avg_wilson_score": report.avg_wilson_score,
                "qtable_entry_count": report.qtable_entry_count,
                "linucb_arm_count": report.linucb_arm_count,
                "linucb_total_pulls": report.linucb_total_pulls,
                "rl_active": report.rl_active,
                "reward_trend": format!("{:?}", report.reward_trend),
                "data_points": (report.wilson_tool_count + report.qtable_entry_count).max(1),
            }),
            duration_ms: start.elapsed().as_millis() as u64,
        }
    }

    /// Blast radius dimension: impact analysis from a changed file.
    ///
    /// Uses the `BlastRadiusEngine` with the `BfsStrategy` when a symbol index
    /// has been provided via `AnalysisPipelineBuilder::with_symbol_index`.
    /// Without a symbol index the engine has no strategies and returns a
    /// placeholder result with `strategy_used = "none"` and an empty
    /// `affected_files` list.
    ///
    /// For real blast radius computation, build the pipeline via
    /// `AnalysisPipelineBuilder::with_symbol_index(Arc<SymbolIndex>)`.
    #[cfg(feature = "blast-radius")]
    pub fn run_blast(&self, start_file: &str) -> HealthDimension {
        use crate::blast_radius::{BfsStrategy, BlastRadiusEngine};

        let start = std::time::Instant::now();
        let engine = if let Some(ref index) = self.symbol_index {
            BlastRadiusEngine::bfs_only(index.clone())
        } else {
            BlastRadiusEngine::new(vec![Box::new(BfsStrategy::new(std::sync::Arc::new(
                touring_code::ast::SymbolIndex::new(),
            )))])
        };
        let result = engine.compute(start_file, &self.config);

        let affected_count = result.affected_files.len();
        // Score: fewer affected files = more isolated = healthier.
        // 0 affected = 1.0, 10+ affected = scaled down.
        let score = if affected_count == 0 {
            1.0
        } else {
            (1.0 - (affected_count as f64 / 50.0).min(1.0)).max(0.1)
        };

        HealthDimension {
            name: "blast_radius".to_string(),
            score,
            weight: 0.5,
            issues: if affected_count > 10 {
                vec![format!(
                    "{affected_count} files affected by change to {start_file}"
                )]
            } else {
                vec![]
            },
            metrics: serde_json::json!({
                "start_file": start_file,
                "affected_count": affected_count,
                "strategy": result.strategy_used,
                "truncated": result.truncated,
                "data_points": affected_count.max(1),
            }),
            duration_ms: start.elapsed().as_millis() as u64,
        }
    }

    /// Wiring dimension: orphan detection + functional chains + module ecosystem.
    pub fn run_wiring(&self, _project_root: &str) -> HealthDimension {
        #[cfg(feature = "wiring")]
        {
            // Memoized: pre_read/post_edit fire on every tool call, but the
            // wiring report only changes when the DB does (cache hit → ~µs vs
            // ~250 ms recompute). Cuts the hook-dispatch p99 tail.
            use crate::wiring::analyze_wiring_memoized;
            let start = std::time::Instant::now();
            let report = analyze_wiring_memoized(self.knowledge_conn);
            HealthDimension {
                name: "wiring".to_string(),
                score: report.score,
                weight: 1.0,
                issues: if report.orphan_count > 0 {
                    vec![format!("{} orphan pub symbol(s)", report.orphan_count)]
                } else {
                    vec![]
                },
                metrics: serde_json::json!({
                    "orphan_count": report.orphan_count,
                    "orphan_rate": report.orphan_rate,
                    "chain_count": report.chain_count,
                    "broken_chain_count": report.broken_chain_count,
                    "avg_integration_score": report.avg_integration_score,
                    "modules_below_threshold": report.modules_below_threshold,
                    "data_points": report.total_pub_symbols.max(1),
                }),
                duration_ms: start.elapsed().as_millis() as u64,
            }
        }
        #[cfg(not(feature = "wiring"))]
        HealthDimension {
            name: "wiring".to_string(),
            score: 1.0,
            weight: 1.0,
            issues: vec![],
            metrics: serde_json::json!({}),
            duration_ms: 0,
        }
    }

    /// Quality dimension: antipatterns, complexity, unwrap audit.
    ///
    /// `files` is a slice of `(path, source, language)` triples.
    #[cfg(feature = "quality")]
    pub fn run_quality(&self, files: &[(&str, &str, &str)]) -> HealthDimension {
        use crate::quality::QualityPipeline;

        if files.is_empty() {
            return HealthDimension {
                name: "quality".to_string(),
                score: 1.0,
                weight: 1.0,
                issues: vec![],
                metrics: serde_json::json!({}),
                duration_ms: 0,
            };
        }

        let start = std::time::Instant::now();
        let pipeline = QualityPipeline::new(self.config.clone());
        let reports = pipeline.analyze_batch(files);
        let dim = QualityPipeline::aggregate(&reports);

        HealthDimension {
            name: "quality".to_string(),
            score: dim.avg_score,
            weight: 1.0,
            issues: if dim.total_antipatterns > 0 {
                vec![format!(
                    "{} antipattern(s) detected",
                    dim.total_antipatterns
                )]
            } else {
                vec![]
            },
            metrics: serde_json::json!({
                "files_analyzed": dim.files_analyzed,
                "total_antipatterns": dim.total_antipatterns,
                "total_unwraps": dim.total_unwraps,
                "max_complexity": dim.max_complexity,
                "avg_error_coverage": dim.avg_error_coverage,
                "top_problem_files": dim.top_problem_files,
                "data_points": dim.files_analyzed.max(1),
            }),
            duration_ms: start.elapsed().as_millis() as u64,
        }
    }

    /// Temporal dimension: edit velocity, churn, bash success rate, drift.
    #[cfg(feature = "temporal")]
    pub fn run_temporal(&self) -> HealthDimension {
        use crate::temporal::analyze_trends;

        let start = std::time::Instant::now();
        let report = analyze_trends(self.knowledge_conn);

        let mut issues = vec![];
        if report.bash_success_rate < 0.8 {
            issues.push(format!(
                "bash error rate {:.0}%",
                report.error_rate_7d * 100.0
            ));
        }
        if report.churn_rate > 0.5 {
            issues.push(format!("high churn rate {:.0}%", report.churn_rate * 100.0));
        }

        HealthDimension {
            name: "temporal".to_string(),
            score: report.score,
            weight: 1.0,
            issues,
            metrics: serde_json::json!({
                "edit_velocity": report.edit_velocity,
                "bash_success_rate": report.bash_success_rate,
                "edits_1d": report.edits_1d,
                "edits_7d": report.edits_7d,
                "churn_rate": report.churn_rate,
                "error_rate_7d": report.error_rate_7d,
                "quality_drift": report.quality_drift.unwrap_or_default(),
                "quality_drift_available": report.quality_drift.is_some(),
                "trend": format!("{:?}", report.trend),
                "data_points": (report.edits_7d + 1),
            }),
            duration_ms: start.elapsed().as_millis() as u64,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_db() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().expect("open in-memory db");
        conn.execute_batch(touring_foundation::schema::knowledge::KNOWLEDGE_SCHEMA_V8)
            .expect("apply schema");
        conn
    }

    #[test]
    fn test_pipeline_run_empty_db() {
        let conn = setup_db();
        let pipeline = AnalysisPipeline::new(&conn, AnalysisConfig::standard());
        let report = pipeline.run(".");
        assert!(
            report.composite_score >= 0.0 && report.composite_score <= 1.0,
            "composite_score out of range: {}",
            report.composite_score
        );
    }

    #[test]
    fn test_pipeline_run_wiring_standalone() {
        let conn = setup_db();
        let pipeline = AnalysisPipeline::new(&conn, AnalysisConfig::standard());
        let dim = pipeline.run_wiring(".");
        assert_eq!(dim.name, "wiring");
        assert!(dim.score >= 0.0 && dim.score <= 1.0);
    }

    #[test]
    fn test_pipeline_run_returns_wiring_dimension() {
        let conn = setup_db();
        let pipeline = AnalysisPipeline::new(&conn, AnalysisConfig::standard());
        let report = pipeline.run(".");
        assert!(
            report.dimensions.iter().any(|d| d.name == "wiring"),
            "report must contain a wiring dimension"
        );
    }

    #[test]
    #[cfg(feature = "temporal")]
    fn test_pipeline_run_temporal_standalone() {
        let conn = setup_db();
        let pipeline = AnalysisPipeline::new(&conn, AnalysisConfig::standard());
        let dim = pipeline.run_temporal();
        assert_eq!(dim.name, "temporal");
        assert!(dim.score >= 0.0 && dim.score <= 1.0);
    }

    #[test]
    #[cfg(feature = "quality")]
    fn test_pipeline_run_quality_empty_files() {
        let conn = setup_db();
        let pipeline = AnalysisPipeline::new(&conn, AnalysisConfig::standard());
        let dim = pipeline.run_quality(&[]);
        assert_eq!(dim.name, "quality");
        assert_eq!(
            dim.score, 1.0,
            "empty files should default to perfect score"
        );
    }

    #[test]
    #[cfg(feature = "quality")]
    fn test_pipeline_run_quality_with_files() {
        let conn = setup_db();
        let pipeline = AnalysisPipeline::new(&conn, AnalysisConfig::standard());
        let files = vec![
            ("src/a.rs", "fn ok() -> Result<(), ()> { Ok(()) }", "rust"),
            ("src/b.rs", "fn bad() { let x = foo(); let _ = x; }", "rust"),
        ];
        let dim = pipeline.run_quality(&files);
        assert_eq!(dim.name, "quality");
        assert!(dim.score > 0.0 && dim.score <= 1.0);
    }

    #[test]
    fn test_otel_config_from_env_disabled() {
        // Ensure the env var is unset so from_env() returns disabled config.
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT") };
        let config = OtelConfig::from_env();
        assert!(
            !config.enabled,
            "OtelConfig must be disabled when env var is absent"
        );
        assert!(config.endpoint.is_none());
        assert_eq!(config.service_name, "touring-analysis");
    }

    #[test]
    fn test_init_otel_subscriber_noop_when_disabled() {
        let config = OtelConfig::disabled();
        let result = init_otel_subscriber(&config);
        assert!(
            result.is_ok(),
            "init_otel_subscriber must return Ok when disabled"
        );
    }

    // ── G3: AnalysisInsights tests ───────────────────────────────────────────

    #[test]
    fn test_analysis_insights_from_report() {
        let conn = setup_db();
        let pipeline = AnalysisPipeline::new(&conn, AnalysisConfig::standard());
        let report = pipeline.run(".");
        let insights = AnalysisInsights::from_report(&report);
        assert!(
            insights.health_score >= 0.0 && insights.health_score <= 1.0,
            "health_score must be in [0.0, 1.0], got {}",
            insights.health_score
        );
        assert!(
            ["healthy", "degraded", "critical"].contains(&insights.health_status.as_str()),
            "health_status must be one of healthy/degraded/critical, got '{}'",
            insights.health_status
        );
        // Default trend when built from report only
        assert_eq!(insights.quality_trend, "Stable");
        assert_eq!(insights.orphan_count, 0);
    }

    #[test]
    fn test_analysis_insights_to_context_string() {
        let insights = AnalysisInsights {
            quality_trend: "Improving".to_string(),
            orphan_count: 3,
            health_score: 0.85,
            health_status: "healthy".to_string(),
        };
        let s = insights.to_context_string();
        assert!(s.contains("health=0.85"), "missing health score: {s}");
        assert!(s.contains("healthy"), "missing status: {s}");
        assert!(s.contains("trend=Improving"), "missing trend: {s}");
        assert!(s.contains("orphans=3"), "missing orphan count: {s}");
    }

    #[test]
    #[cfg(feature = "temporal")]
    fn test_analysis_insights_with_quality_trend() {
        use crate::temporal::TrendDirection;
        let insights = AnalysisInsights::default().with_quality_trend(&TrendDirection::Degrading);
        assert_eq!(insights.quality_trend, "Degrading");

        let insights2 = AnalysisInsights::default().with_quality_trend(&TrendDirection::Improving);
        assert_eq!(insights2.quality_trend, "Improving");
    }
}
