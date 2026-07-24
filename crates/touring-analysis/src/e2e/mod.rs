//! E2E analysis orchestrator with schema-guarded phases.
//!
//! Provides schema validation helpers (single source of truth for table names)
//! and an `AnalysisSummary` convenience type that wraps a `CodeHealthReport`
//! with pre-computed actionable fields for CLI display and alerting.

// schema_guard relocated to touring-foundation (A5 move-utils-down, 2026-06-15);
// re-exported so `touring_analysis::e2e::schema_guard` + the validate_* helpers resolve
// unchanged for all consumers (incl. the no-touch touring-cli). The old
// `src/e2e/schema_guard.rs` is orphaned on disk (Gabriel git-rm, REGRA #11).
pub use touring_foundation::schema_guard::{
    self, validate_graph_tables, validate_knowledge_tables, validate_memory_tables,
};

use crate::engine::Depth;
use crate::health::{CodeHealthReport, HealthStatus};
use crate::pipeline::AnalysisPipelineBuilder;
use serde::{Deserialize, Serialize};

/// Lightweight configuration for a full E2E analysis pass.
///
/// Pass this to [`run_e2e`] along with DB connections to execute a
/// depth-appropriate analysis without constructing the builder manually.
///
/// # Example
/// ```no_run
/// use touring_analysis::e2e::{E2eConfig, run_e2e};
/// use touring_analysis::engine::Depth;
///
/// let conn = rusqlite::Connection::open_in_memory().expect("open in-memory db");
/// let config = E2eConfig { project_root: ".".to_string(), depth: Depth::Standard };
/// let report = run_e2e(&config, &conn, None);
/// println!("{}", report.to_analysis_summary().one_liner());
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct E2eConfig {
    /// Project root path passed to wiring and quality analysis.
    pub project_root: String,
    /// Analysis depth: controls which dimensions run and their sample sizes.
    pub depth: Depth,
}

/// Run a full E2E analysis pass, returning a composite `CodeHealthReport`.
///
/// Orchestrates [`AnalysisPipelineBuilder`] with depth-appropriate config,
/// optional `graph.db` connection, and `run_parallel()`.
///
/// # Arguments
/// - `config` — project root + depth preset
/// - `knowledge_conn` — open connection to `knowledge.db`
/// - `graph_conn` — optional open connection to `graph.db` (enables learning dimension)
pub fn run_e2e(
    config: &E2eConfig,
    knowledge_conn: &rusqlite::Connection,
    graph_conn: Option<&rusqlite::Connection>,
) -> CodeHealthReport {
    let analysis_config = config.depth.to_config();
    let mut builder = AnalysisPipelineBuilder::new(knowledge_conn).config(analysis_config);
    if let Some(gc) = graph_conn {
        builder = builder.graph_conn(gc);
    }
    builder.build().run_parallel(&config.project_root)
}

/// Concise, actionable summary derived from a full `CodeHealthReport`.
///
/// Suitable for single-line CLI output, alerting thresholds, and JSON
/// serialization to a monitoring system. Produced via
/// [`CodeHealthReport::to_analysis_summary`].
///
/// # Example
/// ```no_run
/// use touring_analysis::{AnalysisConfig, AnalysisPipeline};
///
/// let conn = rusqlite::Connection::open_in_memory().expect("open");
/// let pipeline = AnalysisPipeline::new(&conn, AnalysisConfig::standard());
/// let report = pipeline.run(".");
/// let summary = report.to_analysis_summary();
/// println!("{}", summary.one_liner());
/// ```
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AnalysisSummary {
    /// Composite health score (0.0–1.0).
    pub score: f64,
    /// Overall status label: `"healthy"`, `"degraded"`, or `"critical"`.
    pub status: String,
    /// Wilson 95% CI: `[lower, upper]`.
    pub confidence_interval: [f64; 2],
    /// Whether the project passes the health gate (score ≥ 0.8).
    pub passes: bool,
    /// Ordered list of actionable issues across all dimensions (max 5).
    pub top_issues: Vec<String>,
    /// Dimension scores keyed by name.
    pub dimensions: std::collections::HashMap<String, f64>,
    /// Total analysis time in milliseconds.
    pub duration_ms: u64,
}

impl AnalysisSummary {
    /// Single-line summary for CLI display.
    ///
    /// Format: `HEALTHY 0.87 [wiring:0.95 quality:0.83] 124ms — no issues`
    pub fn one_liner(&self) -> String {
        let status = self.status.to_uppercase();
        let dims: Vec<String> = {
            let mut pairs: Vec<_> = self.dimensions.iter().collect();
            pairs.sort_by_key(|(k, _)| k.as_str());
            pairs.iter().map(|(k, v)| format!("{k}:{v:.2}")).collect()
        };
        let issues = self
            .top_issues
            .first()
            .cloned()
            .unwrap_or_else(|| "no issues".to_string());
        format!(
            "{status} {:.2} [{}] {}ms — {issues}",
            self.score,
            dims.join(" "),
            self.duration_ms,
        )
    }
}

impl CodeHealthReport {
    /// Derive a concise `AnalysisSummary` from this report.
    ///
    /// Extracts score, status, CI, issues, and per-dimension scores into a
    /// flat, serialisable struct for alerting and single-line CLI output.
    pub fn to_analysis_summary(&self) -> AnalysisSummary {
        let status = match self.status {
            HealthStatus::Healthy => "healthy",
            HealthStatus::Degraded => "degraded",
            HealthStatus::Critical => "critical",
        }
        .to_string();

        let dimensions: std::collections::HashMap<String, f64> = self
            .dimensions
            .iter()
            .map(|d| (d.name.clone(), d.score))
            .collect();

        let top_issues: Vec<String> = self
            .dimensions
            .iter()
            .flat_map(|d| d.issues.iter().cloned())
            .take(5)
            .collect();

        AnalysisSummary {
            score: self.composite_score,
            status,
            confidence_interval: [self.confidence_lower, self.confidence_upper],
            passes: self.composite_score >= 0.8,
            top_issues,
            dimensions,
            duration_ms: self.total_duration_ms,
        }
    }
}
