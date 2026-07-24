//! Data-model types for the file knowledge graph.
//!
//! Plain serde / data structs extracted from `knowledge.rs` (1A god-file
//! decomposition). Re-exported by the parent `knowledge` module via
//! `pub use models::*`, so external paths
//! (`touring_hooks_core::knowledge::FileKnowledge`, …) are unchanged.

/// File metadata and knowledge accumulated from reads.
#[derive(Debug, Clone, Default)]
pub struct FileKnowledge {
    /// Path of the file this record describes.
    pub file_path: String,
    /// Detected source language, if known.
    pub language: Option<String>,
    /// Number of lines in the file.
    pub line_count: i64,
    /// Number of symbols discovered in the file.
    pub symbol_count: i64,
    /// How many times the file has been read.
    pub read_count: i64,
    /// Timestamp of the most recent read, if any.
    pub last_read_at: Option<String>,
    /// Content hash used for change detection, if computed.
    pub content_hash: Option<String>,
    /// JSON-serialized list of import paths.
    pub imports_json: Option<String>,
    /// JSON-serialized list of symbol names.
    pub symbols_json: Option<String>,
    /// Accumulated notes/gotchas about this file.
    pub notes: Option<String>,
}

/// Extended file metadata combining FileKnowledge with enrichment from specialized tables.
///
/// Created via [`FileKnowledgeDB::query_extended()`] which joins across:
/// - `file_knowledge` (base 10 fields)
/// - `cognitive_enrichment` (cognitive_score, complexity_signal, fan_in_signal, fan_out_signal, doc_signal)
/// - `module_ecosystem` (integration_score, pub_symbol_count, import_count, re_export_count)
/// - `file_blake3_registry` (blake3_hash)
/// - `file_test_coverage` (coverage_pct)
/// - `file_communities` (community_id, modularity_score)
#[derive(Debug, Clone, Default, serde::Serialize)]
pub struct FileKnowledgeEnriched {
    /// Path of the file this record describes.
    pub file_path: String,
    /// Detected source language, if known.
    pub language: Option<String>,
    /// Number of lines in the file.
    pub line_count: i64,
    /// Number of symbols discovered in the file.
    pub symbol_count: i64,
    /// How many times the file has been read.
    pub read_count: i64,
    /// Timestamp of the most recent read, if any.
    pub last_read_at: Option<String>,
    /// Content hash used for change detection, if computed.
    pub content_hash: Option<String>,
    /// JSON-serialized list of import paths.
    pub imports_json: Option<String>,
    /// JSON-serialized list of symbol names.
    pub symbols_json: Option<String>,
    /// Accumulated notes/gotchas about this file.
    pub notes: Option<String>,
    /// Overall cognitive complexity score (0.0–1.0).
    pub cognitive_score: Option<f64>,
    /// Complexity component of the cognitive score.
    pub complexity_signal: Option<f64>,
    /// Fan-in (incoming dependency) signal.
    pub fan_in_signal: Option<f64>,
    /// Fan-out (outgoing dependency) signal.
    pub fan_out_signal: Option<f64>,
    /// Documentation-coverage signal.
    pub doc_signal: Option<f64>,
    /// Module integration score (ratio of wired public symbols).
    pub integration_score: Option<f64>,
    /// Number of public symbols declared in the module.
    pub pub_symbol_count: Option<i64>,
    /// Number of import statements in the file.
    pub import_count: Option<i64>,
    /// Number of re-exports (`pub use`) in the file.
    pub re_export_count: Option<i64>,
    /// BLAKE3 content hash for integrity verification.
    pub blake3_hash: Option<String>,
    /// Line/branch coverage percentage, if measured.
    pub coverage_pct: Option<f64>,
    /// Louvain/Leiden community id the file belongs to.
    pub community_id: Option<i64>,
    /// Modularity score of the file's community.
    pub modularity_score: Option<f64>,
}

impl FileKnowledgeEnriched {
    /// Create from a base FileKnowledge with all enrichment fields set to None.
    pub fn from_base(base: FileKnowledge) -> Self {
        Self {
            file_path: base.file_path,
            language: base.language,
            line_count: base.line_count,
            symbol_count: base.symbol_count,
            read_count: base.read_count,
            last_read_at: base.last_read_at,
            content_hash: base.content_hash,
            imports_json: base.imports_json,
            symbols_json: base.symbols_json,
            notes: base.notes,
            ..Default::default()
        }
    }
}

/// A directed relationship between two files.
#[derive(Debug, Clone)]
pub struct FileRelation {
    /// Source file of the relationship.
    pub source: String,
    /// Target file the source relates to.
    pub target: String,
    /// Kind of relationship (e.g. `"imports"`, `"co_edit"`).
    pub relation_type: String,
}

/// An outcome from a bash command execution.
#[derive(Debug, Clone, Default)]
pub struct BashOutcome {
    /// Full command text that was executed.
    pub command: String,
    /// Truncated command text for display.
    pub command_short: String,
    /// SHA-256 hash of the full command text, used as upsert key to avoid
    /// collisions when truncated commands share the same first 500 chars.
    pub command_hash: String,
    /// Process exit code.
    pub exit_code: i64,
    /// Whether the command succeeded.
    pub success: bool,
    /// Normalized error pattern when the command failed, if any.
    pub error_pattern: Option<String>,
    /// File the command was associated with, if any.
    pub file_context: Option<String>,
    /// Timestamp when the command was executed.
    pub executed_at: String,
}

/// An edit event tracked for change awareness.
#[derive(Debug, Clone)]
pub struct EditEvent {
    /// File that was edited.
    pub file_path: String,
    /// Kind of edit (e.g. `"edit"`, `"write"`).
    pub edit_type: String,
    /// Optional human-readable summary of the change.
    pub summary: Option<String>,
    /// Normalized error pattern if the edit failed (e.g., "string_not_found").
    pub error_pattern: Option<String>,
    /// Timestamp when the edit occurred.
    pub edited_at: String,
}

/// S1.3: Error pattern with exponential-decay-weighted count across sessions.
#[derive(Debug, Clone)]
pub struct WeightedErrorPattern {
    /// Normalized error pattern (e.g., "string_not_found").
    pub pattern: String,
    /// Language context (e.g., "python", "rust"). May be None for old entries.
    pub language: Option<String>,
    /// Weighted count using exponential decay (7-day half-life default).
    pub weighted_count: f64,
}

/// A structured gotcha entry for proactive error prevention.
///
/// Gotchas are pattern-based warnings that fire when a file matching the
/// pattern is read. They replace unstructured notes with trackable,
/// measurable entries that accumulate hit counts and prevented-error stats.
#[derive(Debug, Clone)]
pub struct Gotcha {
    /// Database row id.
    pub id: i64,
    /// File/path pattern that triggers this gotcha.
    pub pattern: String,
    /// The warning message shown when the gotcha fires.
    pub gotcha: String,
    /// Severity level (e.g. `"warning"`, `"error"`).
    pub severity: String,
    /// Language the gotcha applies to, if scoped.
    pub language: Option<String>,
    /// Symbol the gotcha is associated with, if any.
    pub symbol_name: Option<String>,
    /// Number of times this gotcha has fired.
    pub hit_count: i64,
    /// Number of errors estimated to have been prevented.
    pub prevented_errors: i64,
    /// Timestamp when the gotcha was created.
    pub created_at: String,
}

/// A single benchmark run result stored in the knowledge database.
#[derive(Debug, Clone)]
pub struct BenchmarkRun {
    /// Database row id of the run.
    pub run_id: i64,
    /// Commit hash the benchmark was run against.
    pub commit_hash: String,
    /// Name of the benchmark.
    pub bench_name: String,
    /// 50th-percentile latency in milliseconds.
    pub p50_ms: f64,
    /// 95th-percentile latency in milliseconds.
    pub p95_ms: f64,
    /// 99th-percentile latency in milliseconds.
    pub p99_ms: f64,
    /// Number of samples collected.
    pub samples: i64,
    /// Timestamp when the run completed.
    pub ran_at: String,
}
