//! Knowledge source abstraction — the boundary between the cognitive engine and
//! the hooks knowledge database.
//!
//! Relocated to `touring-foundation` (the workspace kernel) so that both
//! `touring-intelligence` (which defines the cognitive engine) and
//! `touring-storage` (which holds `ThreadSafeKnowledgeDB` and its
//! `impl KnowledgeSource`) can share the trait without a Cargo dependency
//! cycle. `touring_intelligence::reasoning::bridge` re-exports every item here,
//! so existing consumers (`touring_intelligence::reasoning::bridge::*`) resolve
//! unchanged. (A5 Path-A step-4 cycle-break, 2026-06-16.)

/// A file relation from the hooks knowledge database.
#[derive(Debug, Clone)]
pub struct FileRelation {
    /// Path of the source file in the relation.
    pub source_path: String,
    /// Path of the target file in the relation.
    pub target_path: String,
    /// Kind of relation (e.g. `co-edit`, `import`).
    pub relation_type: String,
}

/// A bash command outcome from the hooks knowledge database.
#[derive(Debug, Clone)]
pub struct BashOutcomeRecord {
    /// Truncated/normalized form of the executed command.
    pub command_short: String,
    /// Process exit code returned by the command.
    pub exit_code: i32,
    /// Whether the command is considered successful.
    pub success: bool,
    /// Extracted error pattern, if the command failed.
    pub error_pattern: Option<String>,
    /// File context associated with the command, if any.
    pub file_context: Option<String>,
}

/// A co-edit pair from the hooks knowledge database.
#[derive(Debug, Clone)]
pub struct CoEditPair {
    /// Path of the first file in the co-edit pair.
    pub file1: String,
    /// Path of the second file in the co-edit pair.
    pub file2: String,
    /// Co-edit association strength.
    pub weight: f64,
}

/// A gotcha (learned error-prevention pattern) from the hooks DB.
#[derive(Debug, Clone)]
pub struct GotchaRecord {
    /// Trigger pattern that matches the gotcha.
    pub pattern: String,
    /// Human-readable description of the pitfall to avoid.
    pub gotcha: String,
    /// Severity label of the gotcha.
    pub severity: String,
    /// Number of times this gotcha has been observed.
    pub hit_count: i64,
}

/// An edit event from the hooks knowledge database.
#[derive(Debug, Clone)]
pub struct EditRecord {
    /// Path of the edited file.
    pub file_path: String,
    /// Kind of edit performed.
    pub edit_type: String,
    /// Error pattern associated with the edit, if any.
    pub error_pattern: Option<String>,
    /// Timestamp string of when the edit occurred.
    pub edited_at: String,
}

/// Risk assessment for a file based on accumulated knowledge.
#[derive(Debug, Clone, Default)]
pub struct FileRisk {
    /// Aggregate risk score for the file (higher is riskier).
    pub risk_score: f64,
    /// Count of recent failures involving the file.
    pub recent_failures: u32,
    /// Number of gotchas associated with the file.
    pub gotcha_count: u32,
    /// Number of files that depend on this file.
    pub dependent_count: u32,
}

/// Abstraction over hooks' SQLite knowledge database.
///
/// Any implementation of this trait can feed knowledge into the cognitive engine.
/// The primary implementation lives in `touring-storage`
/// (`impl KnowledgeSource for ThreadSafeKnowledgeDB`), but this trait enables
/// testing with mock data and alternative knowledge sources.
pub trait KnowledgeSource: Send + Sync {
    /// Retrieve all file relations (import graph edges).
    fn file_relations(&self) -> Vec<FileRelation>;
    /// Retrieve recent bash outcomes for context enrichment.
    fn recent_bash_outcomes(&self, limit: usize) -> Vec<BashOutcomeRecord>;
    /// Retrieve co-edit pairs for file prediction.
    fn coedit_pairs(&self) -> Vec<CoEditPair>;
    /// Retrieve gotchas matching a file path.
    fn gotchas_for_file(&self, file_path: &str) -> Vec<GotchaRecord>;
    /// Retrieve recent edits for session history feeding.
    fn recent_edits(&self, limit: usize) -> Vec<EditRecord>;
    /// Compute risk score for a file (failure rate after edits).
    fn file_risk(&self, file_path: &str) -> FileRisk;
    /// Retrieve files that depend on (import) the given file.
    fn dependents_of(&self, file_path: &str) -> Vec<String>;
    /// Total file count in knowledge DB.
    fn file_count(&self) -> usize;
    /// Total relation count in knowledge DB.
    fn relation_count(&self) -> usize;
}
