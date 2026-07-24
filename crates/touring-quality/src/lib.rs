//! Touring Quality — 50-dimension quality scoring engine.
//!
//! Makes any LLM produce elite-grade code via:
//! - 50-dim scoring with composite + 6-tier mapping
//! - PreToolUse/PostToolUse enforcement hooks (BLOCK on critical violations)
//! - 50 auto-remediation generators (scaffold compliant code)
//!
//! # Architecture
//!
//! ```text
//! target.rs → QualityReport { dimensions: BTreeMap<DimId, DimScore>, composite: f32, tier: Tier }
//!            → CLI output (JSON | HTML | badge | compact)
//! ```
//!
//! # Example
//!
//! ```no_run
//! use touring_quality::{score_target, OutputFormat};
//! use std::path::Path;
//!
//! match score_target(Path::new("src/main.rs"), &[], OutputFormat::Json) {
//!     Ok(report) => {
//!         println!("Composite: {:.2} ({})", report.composite, report.tier);
//!         for (id, dim) in &report.dimensions {
//!             println!("  {}: {:.2} ({})", id, dim.value, dim.status);
//!         }
//!     }
//!     Err(e) => eprintln!("error: {e}"),
//! }
//! ```

#![warn(missing_docs)]
#![warn(rust_2018_idioms)]
// RBP-01 elite-lint ratchet (2026-06-16): prod-unwrap-free (the lone `unwrap`
// token is a detector pattern-string in f1_6_error_handling.rs, not a real call) —
// lock against future bare unwrap in non-test code. This is the elite-harness crate
// itself; it now holds the same `unwrap_used` invariant it scores other crates on.
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

pub mod aggregate;
pub mod builtins;
pub mod change;
pub mod composite;
pub mod gate;
pub mod history;
pub mod report;
pub mod runner;
pub mod scope;
pub mod scope_report;
pub mod score;
pub mod tier;
pub mod verifications;

pub use change::{Change, FileKind, ProposedFile};
pub use gate::{EvidenceKind, EvidenceRef, Gate, GateId, GateOutcome, GateSeverity, GateStatus};
pub use history::{DriftReport, HistoryEntry, ScoreHistory};
pub use report::{ReportFormat, emit_report};
pub use runner::{HarnessConfig, run_harness, should_block};
pub use score::{EliteScore, EliteTier, tier_for};
// Canonical 17-gate set (W2 migration 2026-06-26: builtins moved from
// touring-harness into touring-quality as the unified home).
pub use builtins::{builtin_default_gates, builtin_deny_advisories, read_deny_advisories};

pub use aggregate::AggKind;
pub use composite::{compute_composite, default_weights};
pub use scope::{Scope, ScopeKind};
pub use scope_report::{ScopeReport, score_scope};
pub use tier::{Tier, tier_from_composite};

/// Identifier for one of the 50 quality dimensions.
///
/// Maps 1:1 with the dimensions in `/comprehensive-review:full-review` v1.3.0:
/// - F1.1-F1.12: Code Quality & Architecture (12 dims)
/// - F2.1-F2.13: Security & Performance (13 dims)
/// - F3.1-F3.13: Testing & Documentation (13 dims)
/// - F4.1-F4.12: Best Practices & CI/CD (12 dims)
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum DimId {
    /// F1.1 Cyclomatic/Cognitive Complexity
    F1_1,
    /// F1.2 Maintainability
    F1_2,
    /// F1.3 Code Duplication
    F1_3,
    /// F1.4 Clean Code / SOLID
    F1_4,
    /// F1.5 Technical Debt
    F1_5,
    /// F1.6 Error Handling
    F1_6,
    /// F1.7 Component Boundaries
    F1_7,
    /// F1.8 Dependency Management
    F1_8,
    /// F1.9 API Design
    F1_9,
    /// F1.10 Data Model
    F1_10,
    /// F1.11 Design Patterns
    F1_11,
    /// F1.12 Architectural Consistency
    F1_12,
    /// F2.1 OWASP Top 10
    F2_1,
    /// F2.2 Input Validation
    F2_2,
    /// F2.3 Authentication/Authorization
    F2_3,
    /// F2.4 Cryptographic Issues
    F2_4,
    /// F2.5 Dependency CVEs
    F2_5,
    /// F2.6 Configuration Security
    F2_6,
    /// F2.7 Database Performance
    F2_7,
    /// F2.8 Memory Management
    F2_8,
    /// F2.9 Caching Opportunities
    F2_9,
    /// F2.10 I/O Bottlenecks
    F2_10,
    /// F2.11 Concurrency
    F2_11,
    /// F2.12 Frontend Performance
    F2_12,
    /// F2.13 Scalability
    F2_13,
    /// F3.1 Test Coverage
    F3_1,
    /// F3.2 Test Quality
    F3_2,
    /// F3.3 Test Pyramid
    F3_3,
    /// F3.4 Edge Cases
    F3_4,
    /// F3.5 Test Maintainability
    F3_5,
    /// F3.6 Security Test Gaps
    F3_6,
    /// F3.7 Performance Test Gaps
    F3_7,
    /// F3.8 Inline Documentation
    F3_8,
    /// F3.9 API Documentation
    F3_9,
    /// F3.10 Architecture Documentation
    F3_10,
    /// F3.11 README Completeness
    F3_11,
    /// F3.12 Documentation Accuracy
    F3_12,
    /// F3.13 Changelog/Migration Guides
    F3_13,
    /// F4.1 Language Idioms
    F4_1,
    /// F4.2 Framework Patterns
    F4_2,
    /// F4.3 Deprecated APIs
    F4_3,
    /// F4.4 Modernization Opportunities
    F4_4,
    /// F4.5 Package Management
    F4_5,
    /// F4.6 Build Configuration
    F4_6,
    /// F4.7 CI/CD Pipeline
    F4_7,
    /// F4.8 Deployment Strategy
    F4_8,
    /// F4.9 Infrastructure as Code
    F4_9,
    /// F4.10 Monitoring & Observability
    F4_10,
    /// F4.11 Incident Response
    F4_11,
    /// F4.12 Environment Management
    F4_12,
}

impl DimId {
    /// All 50 dimension IDs in canonical order.
    pub const ALL: &'static [DimId] = &[
        DimId::F1_1,
        DimId::F1_2,
        DimId::F1_3,
        DimId::F1_4,
        DimId::F1_5,
        DimId::F1_6,
        DimId::F1_7,
        DimId::F1_8,
        DimId::F1_9,
        DimId::F1_10,
        DimId::F1_11,
        DimId::F1_12,
        DimId::F2_1,
        DimId::F2_2,
        DimId::F2_3,
        DimId::F2_4,
        DimId::F2_5,
        DimId::F2_6,
        DimId::F2_7,
        DimId::F2_8,
        DimId::F2_9,
        DimId::F2_10,
        DimId::F2_11,
        DimId::F2_12,
        DimId::F2_13,
        DimId::F3_1,
        DimId::F3_2,
        DimId::F3_3,
        DimId::F3_4,
        DimId::F3_5,
        DimId::F3_6,
        DimId::F3_7,
        DimId::F3_8,
        DimId::F3_9,
        DimId::F3_10,
        DimId::F3_11,
        DimId::F3_12,
        DimId::F3_13,
        DimId::F4_1,
        DimId::F4_2,
        DimId::F4_3,
        DimId::F4_4,
        DimId::F4_5,
        DimId::F4_6,
        DimId::F4_7,
        DimId::F4_8,
        DimId::F4_9,
        DimId::F4_10,
        DimId::F4_11,
        DimId::F4_12,
    ];

    /// Canonical string ID like "F1.1", "F2.5", "F4.12".
    pub fn as_str(&self) -> &'static str {
        META_TABLE[*self as usize].0
    }

    /// Short name (e.g. "complexity", "secrets", "cicd").
    pub fn short_name(&self) -> &'static str {
        META_TABLE[*self as usize].1
    }

    /// Long human-readable name.
    pub fn name(&self) -> &'static str {
        META_TABLE[*self as usize].2
    }

    /// Phase bucket (1=Quality, 2=Sec+Perf, 3=Test+Doc, 4=BP+CI/CD).
    pub fn phase(&self) -> u8 {
        match self {
            DimId::F1_1
            | DimId::F1_2
            | DimId::F1_3
            | DimId::F1_4
            | DimId::F1_5
            | DimId::F1_6
            | DimId::F1_7
            | DimId::F1_8
            | DimId::F1_9
            | DimId::F1_10
            | DimId::F1_11
            | DimId::F1_12 => 1,
            DimId::F2_1
            | DimId::F2_2
            | DimId::F2_3
            | DimId::F2_4
            | DimId::F2_5
            | DimId::F2_6
            | DimId::F2_7
            | DimId::F2_8
            | DimId::F2_9
            | DimId::F2_10
            | DimId::F2_11
            | DimId::F2_12
            | DimId::F2_13 => 2,
            DimId::F3_1
            | DimId::F3_2
            | DimId::F3_3
            | DimId::F3_4
            | DimId::F3_5
            | DimId::F3_6
            | DimId::F3_7
            | DimId::F3_8
            | DimId::F3_9
            | DimId::F3_10
            | DimId::F3_11
            | DimId::F3_12
            | DimId::F3_13 => 3,
            DimId::F4_1
            | DimId::F4_2
            | DimId::F4_3
            | DimId::F4_4
            | DimId::F4_5
            | DimId::F4_6
            | DimId::F4_7
            | DimId::F4_8
            | DimId::F4_9
            | DimId::F4_10
            | DimId::F4_11
            | DimId::F4_12 => 4,
        }
    }

    /// Enforcement kind: how strongly this dim should block writes.
    pub fn enforcement(&self) -> Enforcement {
        match self {
            // 6 BLOCK dims (P0)
            DimId::F2_1 | DimId::F2_4 | DimId::F2_5 | DimId::F2_6 | DimId::F4_3 | DimId::F4_5 => {
                Enforcement::Block
            }
            // 13 WARN dims (P1)
            DimId::F1_1
            | DimId::F1_2
            | DimId::F1_3
            | DimId::F1_4
            | DimId::F1_5
            | DimId::F1_6
            | DimId::F3_1
            | DimId::F3_2
            | DimId::F3_3
            | DimId::F3_4
            | DimId::F3_5
            | DimId::F3_6
            | DimId::F3_7 => Enforcement::Warn,
            // 31 ADVISORY dims (P2/P3)
            _ => Enforcement::Advisory,
        }
    }

    /// Cross-file aggregation kind for multi-scope roll-up — how this dimension's
    /// per-file scores fold into a scope-level score (see [`crate::aggregate`]).
    pub fn agg_kind(&self) -> crate::aggregate::AggKind {
        crate::aggregate::AGG_TABLE[*self as usize]
    }
}

impl fmt::Display for DimId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Canonical metadata table for all 50 dimensions.
/// Index = `DimId as usize` (relies on the variant declaration order in `DimId`).
const META_TABLE: [(&str, &str, &str); 50] = [
    // F1.1-F1.12
    ("F1.1", "complexity", "Cyclomatic/Cognitive Complexity"),
    ("F1.2", "maintainability", "Maintainability"),
    ("F1.3", "duplication", "Code Duplication"),
    ("F1.4", "solid", "Clean Code / SOLID"),
    ("F1.5", "tech-debt", "Technical Debt"),
    ("F1.6", "error-handling", "Error Handling"),
    ("F1.7", "boundaries", "Component Boundaries"),
    ("F1.8", "dep-cycles", "Dependency Management"),
    ("F1.9", "api-design", "API Design"),
    ("F1.10", "data-model", "Data Model"),
    ("F1.11", "patterns", "Design Patterns"),
    ("F1.12", "arch-consistency", "Architectural Consistency"),
    // F2.1-F2.13
    ("F2.1", "owasp", "OWASP Top 10"),
    ("F2.2", "input-validation", "Input Validation"),
    ("F2.3", "authz", "Authentication/Authorization"),
    ("F2.4", "secrets", "Cryptographic Issues"),
    ("F2.5", "dep-cves", "Dependency CVEs"),
    ("F2.6", "config", "Configuration Security"),
    ("F2.7", "db-perf", "Database Performance"),
    ("F2.8", "memory", "Memory Management"),
    ("F2.9", "caching", "Caching"),
    ("F2.10", "io", "I/O Bottlenecks"),
    ("F2.11", "concurrency", "Concurrency"),
    ("F2.12", "frontend", "Frontend Performance"),
    ("F2.13", "scalability", "Scalability"),
    // F3.1-F3.13
    ("F3.1", "coverage", "Test Coverage"),
    ("F3.2", "test-quality", "Test Quality"),
    ("F3.3", "test-pyramid", "Test Pyramid"),
    ("F3.4", "edge-cases", "Edge Cases"),
    ("F3.5", "test-maint", "Test Maintainability"),
    ("F3.6", "sec-tests", "Security Test Gaps"),
    ("F3.7", "perf-tests", "Performance Test Gaps"),
    ("F3.8", "inline-doc", "Inline Documentation"),
    ("F3.9", "api-doc", "API Documentation"),
    ("F3.10", "arch-doc", "Architecture Documentation"),
    ("F3.11", "readme", "README Completeness"),
    ("F3.12", "doc-accuracy", "Documentation Accuracy"),
    ("F3.13", "changelog", "Changelog/Migration"),
    // F4.1-F4.12
    ("F4.1", "idioms", "Language Idioms"),
    ("F4.2", "frameworks", "Framework Patterns"),
    ("F4.3", "deprecated", "Deprecated APIs"),
    ("F4.4", "modernization", "Modernization"),
    ("F4.5", "pkg-mgmt", "Package Management"),
    ("F4.6", "build-config", "Build Configuration"),
    ("F4.7", "cicd", "CI/CD Pipeline"),
    ("F4.8", "deploy", "Deployment Strategy"),
    ("F4.9", "iac", "Infrastructure as Code"),
    ("F4.10", "monitoring", "Monitoring & Observability"),
    ("F4.11", "incident", "Incident Response"),
    ("F4.12", "env", "Environment Management"),
];

// Sanity assertion: the META_TABLE must have exactly 50 entries
// (matches DimId::ALL.len() at compile time).
const _: () = assert!(META_TABLE.len() == 50);
const _: () = assert!(DimId::ALL.len() == 50);

/// Enforcement level for a dim violation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Enforcement {
    /// Hard block — fail-closed on PreToolUse:Edit/Write.
    Block,
    /// Soft warn — emit warning with canonical fix link.
    Warn,
    /// Silent — log only (PostToolUse fires).
    Advisory,
}

/// Status of a single dim score.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum DimStatus {
    /// score >= 0.8
    Pass,
    /// 0.5 <= score < 0.8
    Warn,
    /// score < 0.5
    Fail,
    /// The dimension does not apply to this target (W3 2026-07-02): a
    /// language the dimension cannot measure (`Lang::Other`, non-Cargo for a
    /// Cargo-only dim) or a conditional artifact legitimately absent (IaC on a
    /// pure library). **Excluded from the composite denominator** — an
    /// inapplicable dimension must neither inflate nor deflate the score, and
    /// never blocks. This replaces the silent `Pass 1.0` that made poliglota
    /// projects look artificially clean.
    NotApplicable,
}

impl DimStatus {
    /// Classify a score into Pass/Warn/Fail.
    pub fn from_score(score: f32) -> Self {
        if score >= 0.8 {
            DimStatus::Pass
        } else if score >= 0.5 {
            DimStatus::Warn
        } else {
            DimStatus::Fail
        }
    }
}

impl fmt::Display for DimStatus {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            DimStatus::Pass => f.write_str("PASS"),
            DimStatus::Warn => f.write_str("WARN"),
            DimStatus::Fail => f.write_str("FAIL"),
            DimStatus::NotApplicable => f.write_str("N/A"),
        }
    }
}

/// Score for a single dimension.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DimScore {
    /// Numeric score in 0.0..1.0.
    pub value: f32,
    /// Pass/Warn/Fail classification.
    pub status: DimStatus,
    /// Human-readable evidence (e.g. "3 critical CVEs detected").
    pub evidence: String,
    /// Auto-remediation suggestions (touring-native tooling generator names).
    pub suggestions: Vec<String>,
    /// Latency of this check in milliseconds.
    pub latency_ms: u64,
}

impl DimScore {
    /// Construct a perfect score (1.0, Pass).
    pub fn perfect(evidence: impl Into<String>) -> Self {
        Self {
            value: 1.0,
            status: DimStatus::Pass,
            evidence: evidence.into(),
            suggestions: vec![],
            latency_ms: 0,
        }
    }

    /// Construct a `NotApplicable` score (W3 2026-07-02). Carries `value = 1.0`
    /// as a neutral placeholder for display, but the composite EXCLUDES it from
    /// the denominator (see [`crate::composite::compute_composite`]) so it does
    /// not affect the aggregate. Used when a dimension cannot measure the target
    /// (unsupported language, non-Cargo for a Cargo-only dim, conditional
    /// artifact absent).
    pub fn not_applicable(evidence: impl Into<String>) -> Self {
        Self {
            value: 1.0,
            status: DimStatus::NotApplicable,
            evidence: evidence.into(),
            suggestions: vec![],
            latency_ms: 0,
        }
    }

    /// Construct a score from a value, auto-classifying status.
    pub fn from_value(value: f32, evidence: impl Into<String>) -> Self {
        Self {
            value,
            status: DimStatus::from_score(value),
            evidence: evidence.into(),
            suggestions: vec![],
            latency_ms: 0,
        }
    }
}

/// Quality report for a single target (file or workspace).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct QualityReport {
    /// Path to the scored target.
    pub target: PathBuf,
    /// Per-dimension scores (50-dim coverage).
    pub dimensions: BTreeMap<DimId, DimScore>,
    /// Weighted composite score (0.0..1.0).
    pub composite: f32,
    /// Tier classification (Diamond..Unranked).
    pub tier: Tier,
    /// P0 violations that BLOCK builds.
    pub blockers: Vec<DimId>,
    /// P1 violations that WARN.
    pub warnings: Vec<DimId>,
    /// Auto-remediation candidates (touring-native tooling generators).
    pub suggestions: Vec<String>,
    /// Total latency of all dim checks.
    pub total_latency_ms: u64,
    /// Schema version for forward-compat.
    pub schema_version: u32,
}

impl QualityReport {
    /// Schema version constant.
    pub const SCHEMA_VERSION: u32 = 1;

    /// Build a report from a target + per-dim scores.
    pub fn build(target: PathBuf, dimensions: BTreeMap<DimId, DimScore>) -> Self {
        let composite = compute_composite(&dimensions, default_weights());
        // W5 (2026-07-02): quality gate on top of the numeric tier — a BLOCK
        // dimension failure disqualifies (Unranked) regardless of the mean.
        let tier = composite::apply_quality_gate(tier_from_composite(composite), &dimensions);
        let mut blockers = vec![];
        let mut warnings = vec![];
        let mut suggestions = vec![];
        let mut total_latency_ms = 0u64;
        for (id, score) in &dimensions {
            total_latency_ms = total_latency_ms.saturating_add(score.latency_ms);
            match score.status {
                DimStatus::Fail => blockers.push(*id),
                DimStatus::Warn => warnings.push(*id),
                // NotApplicable never blocks or warns (W3).
                DimStatus::Pass | DimStatus::NotApplicable => {}
            }
            for s in &score.suggestions {
                if !suggestions.contains(s) {
                    suggestions.push(s.clone());
                }
            }
        }
        Self {
            target,
            dimensions,
            composite,
            tier,
            blockers,
            warnings,
            suggestions,
            total_latency_ms,
            schema_version: Self::SCHEMA_VERSION,
        }
    }
}

/// Output format for quality reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum OutputFormat {
    /// JSON (machine-readable).
    Json,
    /// Compact human-readable.
    Compact,
    /// HTML dashboard.
    Html,
    /// SVG badge.
    Badge,
}

impl fmt::Display for OutputFormat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            OutputFormat::Json => f.write_str("json"),
            OutputFormat::Compact => f.write_str("compact"),
            OutputFormat::Html => f.write_str("html"),
            OutputFormat::Badge => f.write_str("badge"),
        }
    }
}

impl std::str::FromStr for OutputFormat {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_ascii_lowercase().as_str() {
            "json" => Ok(OutputFormat::Json),
            "compact" => Ok(OutputFormat::Compact),
            "html" => Ok(OutputFormat::Html),
            "badge" => Ok(OutputFormat::Badge),
            other => Err(format!("unknown output format: {}", other)),
        }
    }
}

/// Score a target (file or workspace) on a subset of dimensions.
///
/// If `dims` is empty, scores all 50 dimensions. Otherwise scores only the
/// specified dimensions.
pub fn score_target(target: &Path, dims: &[DimId], format: OutputFormat) -> Result<QualityReport> {
    let _ = format; // format consumed by caller via report_to_string
    score_target_impl(target, dims)
}

fn score_target_impl(target: &Path, dims: &[DimId]) -> Result<QualityReport> {
    use rayon::prelude::*;

    let dims_to_score: Vec<DimId> = if dims.is_empty() {
        DimId::ALL.to_vec()
    } else {
        dims.to_vec()
    };

    let start = std::time::Instant::now();

    // Parallel score all requested dims
    let scores: Vec<(DimId, DimScore)> = dims_to_score
        .par_iter()
        .map(|&dim| {
            let dim_start = std::time::Instant::now();
            let score = verifications::run_verification(dim, target).unwrap_or_else(|e| DimScore {
                value: 0.0,
                status: DimStatus::Fail,
                evidence: format!("verifier error: {}", e),
                suggestions: vec![],
                latency_ms: dim_start.elapsed().as_millis() as u64,
            });
            let mut score = score;
            score.latency_ms = dim_start.elapsed().as_millis() as u64;
            (dim, score)
        })
        .collect();

    let mut dimensions: BTreeMap<DimId, DimScore> = BTreeMap::new();
    for (id, score) in scores {
        dimensions.insert(id, score);
    }

    let mut report = QualityReport::build(target.to_path_buf(), dimensions);
    let elapsed = start.elapsed();
    // Recompute total latency from per-dim (more accurate)
    report.total_latency_ms = report
        .dimensions
        .values()
        .map(|s| s.latency_ms)
        .sum::<u64>()
        .max(elapsed.as_millis() as u64);
    Ok(report)
}

/// Render a QualityReport to a string in the specified format.
pub fn report_to_string(report: &QualityReport, format: OutputFormat) -> Result<String> {
    let rendered: String = match format {
        OutputFormat::Json => serde_json::to_string_pretty(report)
            .map_err(|e| anyhow::anyhow!("json render failed: {}", e))?,
        OutputFormat::Compact => render_compact(report),
        OutputFormat::Html => render_html(report),
        OutputFormat::Badge => render_badge(report),
    };
    Ok::<_, anyhow::Error>(rendered).context("failed to render report")
}

fn render_compact(report: &QualityReport) -> String {
    let mut s = String::new();
    s.push_str(&format!(
        "{} — {:.3} ({})\n",
        report.target.display(),
        report.composite,
        report.tier
    ));
    s.push_str(&format!(
        "  {} dims scored, {} blockers, {} warnings\n",
        report.dimensions.len(),
        report.blockers.len(),
        report.warnings.len()
    ));
    for (id, dim) in &report.dimensions {
        let marker = match dim.status {
            DimStatus::Pass => "✓",
            DimStatus::Warn => "⚠",
            DimStatus::Fail => "✗",
            DimStatus::NotApplicable => "○",
        };
        s.push_str(&format!(
            "  {} {}: {:.3} {}\n",
            marker, id, dim.value, dim.evidence
        ));
    }
    if !report.suggestions.is_empty() {
        s.push_str("\nSuggestions:\n");
        for sug in &report.suggestions {
            s.push_str(&format!("  - {}\n", sug));
        }
    }
    s
}

fn render_html(report: &QualityReport) -> String {
    let tier_class = match report.tier {
        Tier::Diamond => "diamond",
        Tier::Platinum => "platinum",
        Tier::Gold => "gold",
        Tier::Silver => "silver",
        Tier::Bronze => "bronze",
        Tier::Unranked => "unranked",
    };
    let mut rows = String::new();
    for (id, dim) in &report.dimensions {
        let row_class = match dim.status {
            DimStatus::Pass => "pass",
            DimStatus::Warn => "warn",
            DimStatus::Fail => "fail",
            DimStatus::NotApplicable => "na",
        };
        rows.push_str(&format!(
            "<tr class=\"{}\"><td>{}</td><td>{}</td><td>{:.3}</td><td>{}</td><td>{}</td></tr>\n",
            row_class,
            id,
            id.short_name(),
            dim.value,
            dim.status,
            html_escape(&dim.evidence),
        ));
    }
    format!(
        r#"<!DOCTYPE html>
<html><head><title>Quality Report — {target}</title>
<style>
body {{ font-family: monospace; padding: 2em; }}
h1 {{ color: #333; }}
.tier-{tier_class} {{ color: #FFD700; font-weight: bold; }}
table {{ border-collapse: collapse; width: 100%; }}
th, td {{ padding: 8px; text-align: left; border-bottom: 1px solid #ddd; }}
tr.pass {{ background: #e6ffe6; }}
tr.warn {{ background: #fff4e6; }}
tr.fail {{ background: #ffe6e6; }}
</style></head>
<body>
<h1>{target} <span class="tier-{tier_class}">[{tier}]</span> {composite:.3}</h1>
<p>{n_dims} dims, {n_block} blockers, {n_warn} warnings</p>
<table>
<tr><th>ID</th><th>Name</th><th>Score</th><th>Status</th><th>Evidence</th></tr>
{rows}
</table>
</body></html>
"#,
        target = report.target.display(),
        tier = report.tier,
        composite = report.composite,
        n_dims = report.dimensions.len(),
        n_block = report.blockers.len(),
        n_warn = report.warnings.len(),
        rows = rows,
    )
}

fn render_badge(report: &QualityReport) -> String {
    let color = match report.tier {
        Tier::Diamond => "#0AC",
        Tier::Platinum => "#9C9",
        Tier::Gold => "#DA0",
        Tier::Silver => "#AAA",
        Tier::Bronze => "#C80",
        Tier::Unranked => "#E00",
    };
    let label = format!("touring quality {}", report.tier);
    let value = format!("{:.2}", report.composite);
    // Use r##"..."## so the "#xxx" color codes inside don't close the raw string.
    format!(
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="160" height="20">
<linearGradient id="b" x2="0" y2="100%">
<stop offset="0" stop-color="#bbb" stop-opacity=".1"/>
<stop offset="1" stop-opacity=".1"/></linearGradient>
<clipPath id="a"><rect width="160" height="20" rx="3" fill="#fff"/></clipPath>
<g clip-path="url(#a)">
<path fill="#555" d="M0 0h95v20H0z"/>
<path fill="{color}" d="M95 0h65v20H95z"/>
<path fill="url(#b)" d="M0 0h160v20H0z"/>
</g>
<g fill="#fff" text-anchor="middle" font-family="DejaVu Sans,Verdana,Geneva,sans-serif" text-rendering="geometricPrecision" font-size="110">
<text aria-hidden="true" x="485" y="150" fill="#010101" fill-opacity=".3" transform="scale(.1)" textLength="850">{label}</text>
<text x="485" y="140" transform="scale(.1)" fill="#fff" textLength="850">{label}</text>
<text aria-hidden="true" x="1265" y="150" fill="#010101" fill-opacity=".3" transform="scale(.1)" textLength="550">{value}</text>
<text x="1265" y="140" transform="scale(.1)" fill="#fff" textLength="550">{value}</text>
</g>
</svg>"##,
        color = color,
        label = label,
        value = value,
    )
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dim_id_all_50() {
        assert_eq!(DimId::ALL.len(), 50, "must have exactly 50 dims");
    }

    #[test]
    fn test_dim_id_phases() {
        let p1: Vec<_> = DimId::ALL.iter().filter(|d| d.phase() == 1).collect();
        let p2: Vec<_> = DimId::ALL.iter().filter(|d| d.phase() == 2).collect();
        let p3: Vec<_> = DimId::ALL.iter().filter(|d| d.phase() == 3).collect();
        let p4: Vec<_> = DimId::ALL.iter().filter(|d| d.phase() == 4).collect();
        assert_eq!(p1.len(), 12, "Phase 1 should have 12 dims");
        assert_eq!(p2.len(), 13, "Phase 2 should have 13 dims");
        assert_eq!(p3.len(), 13, "Phase 3 should have 13 dims");
        assert_eq!(p4.len(), 12, "Phase 4 should have 12 dims");
        assert_eq!(p1.len() + p2.len() + p3.len() + p4.len(), 50);
    }

    #[test]
    fn test_enforcement_distribution() {
        let blocks: Vec<_> = DimId::ALL
            .iter()
            .filter(|d| d.enforcement() == Enforcement::Block)
            .collect();
        let warns: Vec<_> = DimId::ALL
            .iter()
            .filter(|d| d.enforcement() == Enforcement::Warn)
            .collect();
        let advisories: Vec<_> = DimId::ALL
            .iter()
            .filter(|d| d.enforcement() == Enforcement::Advisory)
            .collect();
        assert_eq!(blocks.len(), 6, "exactly 6 BLOCK dims");
        assert_eq!(warns.len(), 13, "exactly 13 WARN dims");
        assert_eq!(advisories.len(), 31, "exactly 31 ADVISORY dims");
    }

    #[test]
    fn test_dim_status_from_score() {
        assert_eq!(DimStatus::from_score(1.0), DimStatus::Pass);
        assert_eq!(DimStatus::from_score(0.85), DimStatus::Pass);
        assert_eq!(DimStatus::from_score(0.8), DimStatus::Pass);
        assert_eq!(DimStatus::from_score(0.7), DimStatus::Warn);
        assert_eq!(DimStatus::from_score(0.5), DimStatus::Warn);
        assert_eq!(DimStatus::from_score(0.4), DimStatus::Fail);
        assert_eq!(DimStatus::from_score(0.0), DimStatus::Fail);
    }

    #[test]
    fn test_dim_id_str_round_trip() {
        for dim in DimId::ALL {
            let s = dim.as_str();
            assert!(!s.is_empty());
            assert!(s.starts_with("F"));
        }
    }

    #[test]
    fn test_output_format_parse() {
        use std::str::FromStr;
        assert_eq!(
            OutputFormat::from_str("json").expect("json parses"),
            OutputFormat::Json
        );
        assert_eq!(
            OutputFormat::from_str("compact").expect("compact parses"),
            OutputFormat::Compact
        );
        assert!(OutputFormat::from_str("garbage").is_err());
    }
}
