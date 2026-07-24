//! Shadow Workspace V2 — Multi-branch speculative execution engine.
//!
//! Extends the single-overlay design of the original Shadow Workspace (V1, removed)
//! with N parallel speculative branches. Each branch holds its own
//! `HashMap<PathBuf, String>` overlay, enabling concurrent "what-if" edits
//! that are scored independently before the best one is committed to disk.
//!
//! ## Design Decisions
//!
//! - **No FUSE dependency**: Pure Rust `HashMap` overlays achieve 90%+ of a
//!   virtual filesystem's benefit without `libfuse3-dev` or blocking mount threads.
//! - **Graceful degradation**: When linters (`ruff`, `cargo check`, `tsc`) are unavailable,
//!   branches score 1.0 (assume clean). Files with unknown extensions are skipped.
//! - **Zero filesystem mutations** until `commit_branch` is explicitly called.
//! - **Backward compatible**: `shadow.rs` is untouched; V2 is additive.

use std::collections::HashMap;
use std::io::Write as IoWrite;
use std::path::{Path, PathBuf};
use std::process::Command;
use tempfile::NamedTempFile;
use touring_code::ast::speculate_v2;

// ---------------------------------------------------------------------------
// Diagnostic types
// ---------------------------------------------------------------------------

/// Severity level for a lint diagnostic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DiagnosticSeverity {
    /// A blocking diagnostic that fails the branch (compile/lint error).
    Error,
    /// A non-blocking diagnostic that lowers the branch score.
    Warning,
    /// An informational diagnostic with no scoring impact.
    Info,
}

/// A single lint diagnostic produced by speculative validation.
#[derive(Debug, Clone)]
pub struct LintDiagnosticV2 {
    /// Originating file path (as stored in the branch overlay).
    pub file: String,
    /// 1-based line number.
    pub line: u32,
    /// 1-based column number.
    pub column: u32,
    /// Severity classification.
    pub severity: DiagnosticSeverity,
    /// Human-readable message.
    pub message: String,
    /// Linter rule code (e.g. `"F401"`, `"E501"`). `None` if unavailable.
    pub code: Option<String>,
}

/// Summary of a branch after validation.
#[derive(Debug, Clone)]
pub struct BranchResult {
    /// Unique branch identifier.
    pub branch_id: u64,
    /// Quality score in `[0.0, 1.0]`. Higher is better.
    pub score: f64,
    /// All lint diagnostics found in this branch.
    pub diagnostics: Vec<LintDiagnosticV2>,
    /// Paths of files that were modified in this branch.
    pub files_modified: Vec<String>,
}

// ---------------------------------------------------------------------------
// SpeculativeBranch
// ---------------------------------------------------------------------------

/// A single speculative branch with its own file overlay.
#[derive(Debug)]
pub struct SpeculativeBranch {
    id: u64,
    overlay: HashMap<PathBuf, String>,
    diagnostics: Vec<LintDiagnosticV2>,
    score: f64,
}

impl SpeculativeBranch {
    fn new(id: u64) -> Self {
        Self {
            id,
            overlay: HashMap::new(),
            diagnostics: Vec::new(),
            score: 1.0, // assume clean until validated
        }
    }

    /// Number of files in this branch's overlay.
    fn file_count(&self) -> usize {
        self.overlay.len()
    }
}

// ---------------------------------------------------------------------------
// ShadowWorkspaceV2
// ---------------------------------------------------------------------------

/// Default maximum number of concurrent branches.
const DEFAULT_MAX_BRANCHES: usize = 8;

/// Enhanced shadow workspace supporting N parallel speculative branches.
///
/// Each branch maintains an independent `HashMap<PathBuf, String>` overlay.
/// Reads check the branch overlay first, then fall back to the real filesystem.
/// Validation writes to tempfiles and invokes `ruff check`, scoring each branch
/// by its diagnostic profile. The best branch can then be committed atomically.
#[derive(Debug)]
pub struct ShadowWorkspaceV2 {
    base_dir: PathBuf,
    branches: Vec<SpeculativeBranch>,
    max_branches: usize,
    next_branch_id: u64,
}

/// Errors from [`ShadowWorkspaceV2`] branch create / edit / read / validate / commit operations.
#[derive(Debug, thiserror::Error)]
pub enum ShadowWorkspaceError {
    /// The maximum number of speculative branches has been reached.
    #[error("Maximum branch limit reached ({0})")]
    MaxBranches(usize),
    /// No branch exists with the given id.
    #[error("Branch {0} not found")]
    BranchNotFound(u64),
    /// Could not read a file from the real filesystem.
    #[error("Failed to read {path}: {source}")]
    Read {
        /// Path that failed to read.
        path: String,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// A target path had no parent directory.
    #[error("Path has no parent: {0}")]
    NoParent(String),
    /// Could not create a parent directory during commit.
    #[error("Failed to create directory {path}: {source}")]
    CreateDir {
        /// Directory path that failed to create.
        path: String,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// Could not create the atomic-write temp file.
    #[error("Failed to create temp file in {path}: {source}")]
    TempFile {
        /// Parent directory the temp file was created in.
        path: String,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// Could not write content into the temp file.
    #[error("Failed to write temp file for {path}: {source}")]
    WriteTemp {
        /// Target path the temp file was staging.
        path: String,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// Could not atomically persist the temp file to its final path.
    #[error("Failed to persist {path}: {source}")]
    Persist {
        /// Final path the temp file failed to persist to.
        path: String,
        /// Underlying temp-file persist error.
        #[source]
        source: tempfile::PersistError,
    },
}

impl ShadowWorkspaceV2 {
    /// Create a new workspace rooted at `base_dir` with the default branch limit.
    pub fn new(base_dir: PathBuf) -> Self {
        Self {
            base_dir,
            branches: Vec::new(),
            max_branches: DEFAULT_MAX_BRANCHES,
            next_branch_id: 1,
        }
    }

    /// Create a new workspace with a custom branch limit.
    pub fn with_max_branches(base_dir: PathBuf, max: usize) -> Self {
        Self {
            base_dir,
            branches: Vec::new(),
            max_branches: max.max(1), // at least 1
            next_branch_id: 1,
        }
    }

    /// Create a new speculative branch. Returns the branch id.
    ///
    /// Returns `Err` if the maximum number of branches has been reached.
    pub fn create_branch(&mut self) -> Result<u64, ShadowWorkspaceError> {
        if self.branches.len() >= self.max_branches {
            return Err(ShadowWorkspaceError::MaxBranches(self.max_branches));
        }
        let id = self.next_branch_id;
        self.next_branch_id += 1;
        self.branches.push(SpeculativeBranch::new(id));
        Ok(id)
    }

    /// Apply an edit to a specific branch's overlay.
    pub fn apply_edit(
        &mut self,
        branch_id: u64,
        path: PathBuf,
        content: String,
    ) -> Result<(), ShadowWorkspaceError> {
        let branch = self
            .find_branch_mut(branch_id)
            .ok_or(ShadowWorkspaceError::BranchNotFound(branch_id))?;
        branch.overlay.insert(path, content);
        Ok(())
    }

    /// Read a file from a branch. Checks the branch overlay first, then the
    /// real filesystem (resolving relative paths against `base_dir`).
    pub fn read_file(&self, branch_id: u64, path: &Path) -> Result<String, ShadowWorkspaceError> {
        let branch = self
            .find_branch(branch_id)
            .ok_or(ShadowWorkspaceError::BranchNotFound(branch_id))?;

        if let Some(content) = branch.overlay.get(path) {
            return Ok(content.clone());
        }

        let full_path = if path.is_absolute() {
            path.to_path_buf()
        } else {
            self.base_dir.join(path)
        };

        std::fs::read_to_string(&full_path).map_err(|e| ShadowWorkspaceError::Read {
            path: full_path.display().to_string(),
            source: e,
        })
    }

    /// Validate a branch by linting all its overlay files.
    /// Updates the branch's diagnostics and score in place.
    pub fn validate_branch(
        &mut self,
        branch_id: u64,
    ) -> Result<BranchResult, ShadowWorkspaceError> {
        // Collect overlay data first to avoid borrow issues.
        let overlay_snapshot: Vec<(PathBuf, String)> = {
            let branch = self
                .find_branch(branch_id)
                .ok_or(ShadowWorkspaceError::BranchNotFound(branch_id))?;
            branch
                .overlay
                .iter()
                .map(|(k, v)| (k.clone(), v.clone()))
                .collect()
        };

        let mut all_diagnostics = Vec::new();
        let mut files_modified = Vec::new();

        for (path, content) in &overlay_snapshot {
            files_modified.push(path.display().to_string());

            let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("");
            let display_path = path.display().to_string();

            // Fast-path: try tree-sitter speculative validation first (<200ms).
            // speculate_v2 covers rust, python, typescript, javascript via tree-sitter.
            let lang_opt = touring_code::ast::Lang::from_path(path);

            // Only validate code languages — skip markup (md, html, css, json, toml, yaml)
            if let Some(lang) = lang_opt.filter(|l| l.is_code()) {
                let spec = speculate_v2(content, lang, None, None);
                if spec.all_passed {
                    // Tree-sitter found no issues — skip slow external linter.
                    continue;
                }
                // Tree-sitter found issues — convert to LintDiagnosticV2 and trust them.
                let fast_diags = convert_speculate_diagnostics(&spec, &display_path);
                if !fast_diags.is_empty() {
                    all_diagnostics.extend(fast_diags);
                    continue;
                }
            }

            // Fallback: external linter (for languages not covered by tree-sitter,
            // or when speculate_v2 returned no diagnostics despite not passing).
            let diags = match ext {
                "py" => run_ruff_on_content(content, &display_path),
                "rs" => run_cargo_check_on_content(content, &display_path),
                "ts" | "tsx" => run_tsc_on_content(content, &display_path),
                _ => Vec::new(),
            };
            all_diagnostics.extend(diags);
        }

        let score = score_diagnostics(&all_diagnostics);

        // Update the branch in place.
        let branch = self
            .find_branch_mut(branch_id)
            .ok_or(ShadowWorkspaceError::BranchNotFound(branch_id))?;
        branch.diagnostics = all_diagnostics.clone();
        branch.score = score;

        Ok(BranchResult {
            branch_id,
            score,
            diagnostics: all_diagnostics,
            files_modified,
        })
    }

    /// Select the best branch (highest score, ties broken by fewest files).
    /// Returns `None` if there are no branches.
    pub fn select_best_branch(&self) -> Option<&SpeculativeBranch> {
        self.branches.iter().max_by(|a, b| {
            a.score
                .partial_cmp(&b.score)
                .unwrap_or(std::cmp::Ordering::Equal)
                .then_with(|| {
                    // Fewer modified files wins on tie.
                    b.file_count().cmp(&a.file_count())
                })
        })
    }

    /// Commit a branch's edits to the real filesystem.
    /// Returns the list of paths that were written.
    pub fn commit_branch(&self, branch_id: u64) -> Result<Vec<PathBuf>, ShadowWorkspaceError> {
        let branch = self
            .find_branch(branch_id)
            .ok_or(ShadowWorkspaceError::BranchNotFound(branch_id))?;

        let mut written = Vec::new();
        for (path, content) in &branch.overlay {
            let full_path = if path.is_absolute() {
                path.clone()
            } else {
                self.base_dir.join(path)
            };

            // Ensure parent directory exists.
            let parent = full_path
                .parent()
                .ok_or_else(|| ShadowWorkspaceError::NoParent(full_path.display().to_string()))?;
            std::fs::create_dir_all(parent).map_err(|e| ShadowWorkspaceError::CreateDir {
                path: parent.display().to_string(),
                source: e,
            })?;

            // Atomic write via rename: create temp file in the SAME directory as the
            // target to guarantee same filesystem (cross-filesystem rename → EXDEV error).
            let mut tmp =
                NamedTempFile::new_in(parent).map_err(|e| ShadowWorkspaceError::TempFile {
                    path: parent.display().to_string(),
                    source: e,
                })?;
            tmp.write_all(content.as_bytes())
                .map_err(|e| ShadowWorkspaceError::WriteTemp {
                    path: full_path.display().to_string(),
                    source: e,
                })?;
            tmp.persist(&full_path)
                .map_err(|e| ShadowWorkspaceError::Persist {
                    path: full_path.display().to_string(),
                    source: e,
                })?;
            written.push(full_path);
        }
        Ok(written)
    }

    /// Discard a branch, freeing its memory.
    pub fn discard_branch(&mut self, branch_id: u64) {
        self.branches.retain(|b| b.id != branch_id);
    }

    /// Discard all branches (instant reset).
    pub fn reset(&mut self) {
        self.branches.clear();
        // Keep next_branch_id monotonically increasing.
    }

    /// Get results for all branches (for comparison).
    pub fn compare_branches(&self) -> Vec<BranchResult> {
        self.branches
            .iter()
            .map(|b| BranchResult {
                branch_id: b.id,
                score: b.score,
                diagnostics: b.diagnostics.clone(),
                files_modified: b.overlay.keys().map(|p| p.display().to_string()).collect(),
            })
            .collect()
    }

    /// Number of active branches.
    pub fn branch_count(&self) -> usize {
        self.branches.len()
    }

    /// Maximum allowed branches.
    pub fn max_branches(&self) -> usize {
        self.max_branches
    }

    // -- private helpers --

    fn find_branch(&self, id: u64) -> Option<&SpeculativeBranch> {
        self.branches.iter().find(|b| b.id == id)
    }

    fn find_branch_mut(&mut self, id: u64) -> Option<&mut SpeculativeBranch> {
        self.branches.iter_mut().find(|b| b.id == id)
    }
}

// ---------------------------------------------------------------------------
// speculate_v2 fast-path conversion
// ---------------------------------------------------------------------------

/// Convert `SpeculateResult` diagnostics into `LintDiagnosticV2` entries.
///
/// `speculate_v2` returns per-layer `Vec<String>` diagnostics. We convert each
/// non-empty diagnostic string into a `LintDiagnosticV2` with `Error` severity
/// (tree-sitter flagged it) and line 0 (tree-sitter diagnostics are not always
/// positional).
fn convert_speculate_diagnostics(
    result: &touring_code::ast::SpeculateResult,
    file_path: &str,
) -> Vec<LintDiagnosticV2> {
    let mut out = Vec::new();
    for layer in &result.layers {
        if layer.passed {
            continue;
        }
        let severity = match layer.layer {
            touring_code::ast::ValidationLayer::Syntax => DiagnosticSeverity::Error,
            touring_code::ast::ValidationLayer::SymbolResolution => DiagnosticSeverity::Warning,
            touring_code::ast::ValidationLayer::Structural => DiagnosticSeverity::Warning,
            touring_code::ast::ValidationLayer::ImportCheck => DiagnosticSeverity::Info,
            // CfgImpact is informational — always passes, uses Info severity.
            touring_code::ast::ValidationLayer::CfgImpact => DiagnosticSeverity::Info,
            // Complexity exceeding threshold is a warning, not a hard error.
            touring_code::ast::ValidationLayer::Complexity => DiagnosticSeverity::Warning,
        };
        for msg in &layer.diagnostics {
            out.push(LintDiagnosticV2 {
                file: file_path.to_string(),
                line: 0,
                column: 0,
                severity,
                message: msg.clone(),
                code: None,
            });
        }
    }
    out
}

// ---------------------------------------------------------------------------
// Scoring
// ---------------------------------------------------------------------------

/// Score diagnostics on a 0.0..=1.0 scale.
///
/// ```text
/// score = 1.0 - (errors * 0.3) - (warnings * 0.05) - (infos * 0.01)
/// ```
///
/// Clamped to `[0.0, 1.0]`.
fn score_diagnostics(diagnostics: &[LintDiagnosticV2]) -> f64 {
    let mut errors: f64 = 0.0;
    let mut warnings: f64 = 0.0;
    let mut infos: f64 = 0.0;

    for d in diagnostics {
        match d.severity {
            DiagnosticSeverity::Error => errors += 1.0,
            DiagnosticSeverity::Warning => warnings += 1.0,
            DiagnosticSeverity::Info => infos += 1.0,
        }
    }

    (1.0 - (errors * 0.3) - (warnings * 0.05) - (infos * 0.01)).clamp(0.0, 1.0)
}

// ---------------------------------------------------------------------------
// Ruff integration
// ---------------------------------------------------------------------------

/// Classify a ruff rule code into a severity level.
///
/// - `E1xx`-`E4xx`, `E9xx` (syntax/runtime) and `F` (pyflakes) → `Error`
/// - `E5xx` (style, e.g. E501 line too long) and `W` (warning) → `Warning`
/// - Everything else (conventions, info) → `Info`
fn classify_severity(code: &str) -> DiagnosticSeverity {
    let first = code.chars().next();
    match first {
        Some('F') => DiagnosticSeverity::Error,
        Some('E') => {
            // E5xx codes are style warnings (e.g. E501 line-too-long).
            // E1xx-E4xx and E9xx are real errors.
            let second = code.chars().nth(1);
            if second == Some('5') {
                DiagnosticSeverity::Warning
            } else {
                DiagnosticSeverity::Error
            }
        }
        Some('W') => DiagnosticSeverity::Warning,
        _ => DiagnosticSeverity::Info,
    }
}

/// Run `ruff check` on content via a tempfile. Returns parsed diagnostics.
///
/// Graceful degradation: returns an empty `Vec` if ruff is not installed or
/// the tempfile cannot be created.
fn run_ruff_on_content(content: &str, original_path: &str) -> Vec<LintDiagnosticV2> {
    let tmp = match tempfile::Builder::new().suffix(".py").tempfile() {
        Ok(t) => t,
        Err(_) => return Vec::new(),
    };

    if tmp.as_file().write_all(content.as_bytes()).is_err() {
        return Vec::new();
    }

    let output = Command::new("ruff")
        .args(["check", "--output-format=json", "--no-fix"])
        .arg(tmp.path())
        .output();

    match output {
        Ok(out) => parse_ruff_json(&String::from_utf8_lossy(&out.stdout), original_path),
        Err(_) => Vec::new(),
    }
}

/// Parse ruff JSON output into `LintDiagnosticV2` entries.
fn parse_ruff_json(json_output: &str, file_path: &str) -> Vec<LintDiagnosticV2> {
    let trimmed = json_output.trim();
    if trimmed.is_empty() || trimmed == "[]" {
        return Vec::new();
    }

    let entries: Vec<serde_json::Value> = match serde_json::from_str(trimmed) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    entries
        .iter()
        .map(|entry| {
            let line = entry
                .get("location")
                .and_then(|l| l.get("row"))
                .and_then(|r| r.as_u64())
                .unwrap_or(0) as u32;
            let column = entry
                .get("location")
                .and_then(|l| l.get("column"))
                .and_then(|c| c.as_u64())
                .unwrap_or(0) as u32;
            let code_str = entry
                .get("code")
                .and_then(|c| c.as_str())
                .unwrap_or("unknown")
                .to_string();
            let message = entry
                .get("message")
                .and_then(|m| m.as_str())
                .unwrap_or("")
                .to_string();
            let severity = classify_severity(&code_str);

            LintDiagnosticV2 {
                file: file_path.to_string(),
                line,
                column,
                severity,
                message,
                code: Some(code_str),
            }
        })
        .collect()
}

// ---------------------------------------------------------------------------
// cargo check integration
// ---------------------------------------------------------------------------

/// Extract unique feature names from `#[cfg(feature = "...")]` patterns in Rust source.
///
/// Returns names that are valid feature identifiers (alphanumeric, `-`, `_`).
/// Used to inject empty feature definitions into the shadow `Cargo.toml` so that
/// `--all-features` compiles feature-gated code paths without pulling in deps.
pub(crate) fn extract_cfg_feature_names(content: &str) -> Vec<String> {
    let mut features: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut remaining = content;
    while let Some(pos) = remaining.find("feature = \"") {
        let after = &remaining[pos + 11..];
        if let Some(end) = after.find('"') {
            let name = &after[..end];
            if !name.is_empty()
                && name
                    .chars()
                    .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
            {
                features.insert(name.to_string());
            }
        }
        remaining = &remaining[pos + 1..];
    }
    features.into_iter().collect()
}

/// Build a Cargo.toml string with empty feature definitions for shadow project.
///
/// Empty features (`feat = []`) have no dependencies — they only enable the
/// `#[cfg(feature = "...")]` code paths so that `--all-features` compiles them.
fn build_shadow_cargo_toml_with_features(features: &[String]) -> String {
    if features.is_empty() {
        return "[package]\nname = \"shadow_check\"\nversion = \"0.1.0\"\nedition = \"2021\"\n"
            .to_string();
    }
    let feat_entries: Vec<String> = features.iter().map(|f| format!("{} = []", f)).collect();
    format!(
        "[package]\nname = \"shadow_check\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\n[features]\n{}\n",
        feat_entries.join("\n")
    )
}

/// Run `cargo check --message-format=json` on Rust content via a temporary project.
///
/// Creates a minimal single-lib Cargo project in a tempdir, writes the
/// content to `src/lib.rs`, and invokes `cargo check --message-format=json`.
///
/// When feature gates are detected, injects empty feature definitions and runs
/// a second `--all-features` pass to catch cross-config correctness issues.
///
/// Graceful degradation: returns an empty `Vec` if `cargo` is not in PATH
/// or if the temporary project cannot be created.
fn run_cargo_check_on_content(content: &str, original_path: &str) -> Vec<LintDiagnosticV2> {
    let tmp_dir = match tempfile::TempDir::new() {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };

    // Detect feature gates early to build the right Cargo.toml on first pass.
    let features = extract_cfg_feature_names(content);
    let cargo_toml_content = build_shadow_cargo_toml_with_features(&features);

    if std::fs::write(tmp_dir.path().join("Cargo.toml"), &cargo_toml_content).is_err() {
        return Vec::new();
    }

    let src_dir = tmp_dir.path().join("src");
    if std::fs::create_dir(&src_dir).is_err() {
        return Vec::new();
    }

    if std::fs::write(src_dir.join("lib.rs"), content).is_err() {
        return Vec::new();
    }

    // First pass: default features.
    let default_out = Command::new("cargo")
        .args(["check", "--message-format=json", "--quiet"])
        .current_dir(tmp_dir.path())
        .env("CARGO_INCREMENTAL", "0")
        .output();

    let mut diagnostics = match default_out {
        Ok(out) => parse_cargo_json(&String::from_utf8_lossy(&out.stdout), original_path),
        Err(_) => return Vec::new(),
    };

    // Second pass: --all-features, only when there are feature gates.
    // Empty feature defs enable the cfg code paths without pulling in any deps.
    if !features.is_empty() {
        let all_out = Command::new("cargo")
            .args([
                "check",
                "--all-features",
                "--message-format=json",
                "--quiet",
            ])
            .current_dir(tmp_dir.path())
            .env("CARGO_INCREMENTAL", "0")
            .output();

        if let Ok(out) = all_out {
            let all_diags = parse_cargo_json(&String::from_utf8_lossy(&out.stdout), original_path);
            // Merge: add only diagnostics not already surfaced by the default pass.
            for d in all_diags {
                if !diagnostics
                    .iter()
                    .any(|e| e.message == d.message && e.line == d.line)
                {
                    diagnostics.push(d);
                }
            }
        }
    }

    diagnostics
}

/// Parse NDJSON output of `cargo check --message-format=json`.
///
/// Each line is a JSON object. Only `"compiler-message"` entries with level
/// `"error"` or `"warning"` are returned. Notes, help, and summary lines are
/// discarded.
fn parse_cargo_json(json_output: &str, file_path: &str) -> Vec<LintDiagnosticV2> {
    let mut diagnostics = Vec::new();

    for line in json_output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        let entry: serde_json::Value = match serde_json::from_str(line) {
            Ok(v) => v,
            Err(_) => continue,
        };

        if entry.get("reason").and_then(|r| r.as_str()) != Some("compiler-message") {
            continue;
        }

        let msg = match entry.get("message") {
            Some(m) => m,
            None => continue,
        };

        let level = msg.get("level").and_then(|l| l.as_str()).unwrap_or("");
        let severity = match level {
            "error" | "ice" => DiagnosticSeverity::Error,
            "warning" => DiagnosticSeverity::Warning,
            _ => continue, // skip "note", "help", "failure-note"
        };

        let message_text = msg
            .get("message")
            .and_then(|m| m.as_str())
            .unwrap_or("")
            .to_string();

        // Skip "aborting due to N previous error(s)" summary messages.
        if message_text.starts_with("aborting due to") {
            continue;
        }

        let (line_no, col_no) = msg
            .get("spans")
            .and_then(|s| s.as_array())
            .and_then(|arr| arr.first())
            .map(|span| {
                let l = span.get("line_start").and_then(|v| v.as_u64()).unwrap_or(0) as u32;
                let c = span
                    .get("column_start")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0) as u32;
                (l, c)
            })
            .unwrap_or((0, 0));

        let code = msg
            .get("code")
            .and_then(|c| c.get("code"))
            .and_then(|c| c.as_str())
            .map(|s| s.to_string());

        diagnostics.push(LintDiagnosticV2 {
            file: file_path.to_string(),
            line: line_no,
            column: col_no,
            severity,
            message: message_text,
            code,
        });
    }

    diagnostics
}

// ---------------------------------------------------------------------------
// tsc integration
// ---------------------------------------------------------------------------

/// Run `tsc --noEmit` on TypeScript/TSX content via a temporary project.
///
/// Creates a minimal `tsconfig.json` and writes the content to a temp `.ts`
/// (or `.tsx`) file, then invokes `tsc --noEmit --project tsconfig.json`.
///
/// Graceful degradation: returns an empty `Vec` if `tsc` is not in PATH.
fn run_tsc_on_content(content: &str, original_path: &str) -> Vec<LintDiagnosticV2> {
    let tmp_dir = match tempfile::TempDir::new() {
        Ok(d) => d,
        Err(_) => return Vec::new(),
    };

    let is_tsx = original_path.ends_with(".tsx");
    let src_name = if is_tsx { "source.tsx" } else { "source.ts" };

    if std::fs::write(tmp_dir.path().join(src_name), content).is_err() {
        return Vec::new();
    }

    // skipLibCheck avoids failures from missing @types packages.
    // jsx:"preserve" for .tsx lets TSC accept JSX without requiring @types/react.
    let jsx_option = if is_tsx { r#","jsx":"preserve""# } else { "" };
    let tsconfig = format!(
        r#"{{"compilerOptions":{{"strict":true,"noEmit":true,"skipLibCheck":true{jsx_option}}},"files":["{src_name}"]}}"#
    );
    if std::fs::write(tmp_dir.path().join("tsconfig.json"), tsconfig).is_err() {
        return Vec::new();
    }

    let output = Command::new("tsc")
        .args(["--noEmit", "--project", "tsconfig.json"])
        .current_dir(tmp_dir.path())
        .output();

    match output {
        Ok(out) => {
            // tsc writes diagnostics to stdout; some versions use stderr.
            let stdout = String::from_utf8_lossy(&out.stdout);
            let stderr = String::from_utf8_lossy(&out.stderr);
            let combined = format!("{stdout}{stderr}");
            parse_tsc_output(&combined, original_path)
        }
        Err(_) => Vec::new(),
    }
}

/// Parse `tsc` text diagnostic output into `LintDiagnosticV2` entries.
///
/// TSC diagnostic format:
/// ```text
/// source.ts(line,col): error TS2304: Cannot find name 'foo'.
/// ```
fn parse_tsc_output(output: &str, file_path: &str) -> Vec<LintDiagnosticV2> {
    let mut diagnostics = Vec::new();

    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }

        // Locate the opening '(' that delimits the coords section.
        let paren_open = match line.find('(') {
            Some(p) => p,
            None => continue,
        };

        // Safety: paren_open is a byte index from find('('), and '(' is ASCII (1 byte),
        // so paren_open + 1 is always a valid char boundary.
        let after_paren = match line.get(paren_open + 1..) {
            Some(s) => s,
            None => continue,
        };
        let paren_close = match after_paren.find(')') {
            Some(p) => p,
            None => continue,
        };

        // Safety: paren_close is from find(')') on after_paren, ')' is ASCII.
        let coords = after_paren.get(..paren_close).unwrap_or("");
        let rest = after_paren.get(paren_close + 1..).unwrap_or("");

        // Parse "line,col" from the coords section.
        let (line_no, col_no) = if let Some(comma) = coords.find(',') {
            // Safety: ',' is ASCII (1 byte), so comma and comma + 1 are valid boundaries.
            let l = coords
                .get(..comma)
                .unwrap_or("")
                .trim()
                .parse::<u32>()
                .unwrap_or(0);
            let c = coords
                .get(comma + 1..)
                .unwrap_or("")
                .trim()
                .parse::<u32>()
                .unwrap_or(0);
            (l, c)
        } else {
            (0, 0)
        };

        if line_no == 0 {
            continue;
        }

        // rest: ": error TS2304: Cannot find name 'foo'."
        let rest = rest.trim_start_matches(':').trim();

        let (severity, rest_after_level) = if let Some(r) = rest.strip_prefix("error") {
            (DiagnosticSeverity::Error, r)
        } else if let Some(r) = rest.strip_prefix("warning") {
            (DiagnosticSeverity::Warning, r)
        } else {
            continue; // skip unrecognized levels
        };

        let rest_after_level = rest_after_level.trim();

        // Extract optional TS error code ("TS2304") and the message text.
        // Safety: ':' is ASCII (1 byte), so colon_pos and colon_pos + 1 are valid boundaries.
        let (code, message) = if let Some(colon_pos) = rest_after_level.find(':') {
            let code_part = rest_after_level.get(..colon_pos).unwrap_or("").trim();
            let msg_part = rest_after_level
                .get(colon_pos + 1..)
                .unwrap_or("")
                .trim()
                .to_string();
            let code_opt = if code_part.starts_with("TS") {
                Some(code_part.to_string())
            } else {
                None
            };
            (code_opt, msg_part)
        } else {
            (None, rest_after_level.to_string())
        };

        diagnostics.push(LintDiagnosticV2 {
            file: file_path.to_string(),
            line: line_no,
            column: col_no,
            severity,
            message,
            code,
        });
    }

    diagnostics
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing, clippy::len_zero)]
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    #[test]
    fn test_extract_cfg_feature_names_finds_features() {
        let content = r#"
#[cfg(feature = "quantization")]
fn a() {}
#[cfg(not(feature = "ebpf"))]
fn b() {}
#[cfg(all(feature = "hnsw-working", feature = "quantization"))]
fn c() {}
"#;
        let mut names = extract_cfg_feature_names(content);
        names.sort();
        assert_eq!(names, vec!["ebpf", "hnsw-working", "quantization"]);
    }

    #[test]
    fn test_extract_cfg_feature_names_empty_when_no_gates() {
        assert!(extract_cfg_feature_names("pub fn foo() {}").is_empty());
    }

    #[test]
    fn test_build_shadow_cargo_toml_with_features_empty() {
        let toml = build_shadow_cargo_toml_with_features(&[]);
        assert!(toml.contains("[package]"));
        assert!(!toml.contains("[features]"));
    }

    #[test]
    fn test_build_shadow_cargo_toml_with_features_non_empty() {
        let features = vec!["quantization".to_string(), "ebpf".to_string()];
        let toml = build_shadow_cargo_toml_with_features(&features);
        assert!(toml.contains("[features]"));
        assert!(toml.contains("quantization = []"));
        assert!(toml.contains("ebpf = []"));
    }

    // -- Branch creation --

    #[test]
    fn test_create_branch_returns_unique_id() {
        let tmp = TempDir::new().unwrap();
        let mut ws = ShadowWorkspaceV2::new(tmp.path().to_path_buf());

        let id1 = ws.create_branch().unwrap();
        let id2 = ws.create_branch().unwrap();
        let id3 = ws.create_branch().unwrap();

        assert_ne!(id1, id2);
        assert_ne!(id2, id3);
        assert_ne!(id1, id3);
        assert_eq!(ws.branch_count(), 3);
    }

    #[test]
    fn test_max_branches_limit() {
        let tmp = TempDir::new().unwrap();
        let mut ws = ShadowWorkspaceV2::with_max_branches(tmp.path().to_path_buf(), 2);

        assert!(ws.create_branch().is_ok());
        assert!(ws.create_branch().is_ok());
        let err = ws.create_branch();
        assert!(err.is_err());
        assert!(
            err.unwrap_err()
                .to_string()
                .contains("Maximum branch limit")
        );
    }

    // -- Apply edit --

    #[test]
    fn test_apply_edit_to_branch() {
        let tmp = TempDir::new().unwrap();
        let mut ws = ShadowWorkspaceV2::new(tmp.path().to_path_buf());
        let id = ws.create_branch().unwrap();

        let result = ws.apply_edit(id, PathBuf::from("test.py"), "x = 1\n".to_string());
        assert!(result.is_ok());

        let content = ws.read_file(id, Path::new("test.py")).unwrap();
        assert_eq!(content, "x = 1\n");
    }

    #[test]
    fn test_nonexistent_branch_returns_error() {
        let tmp = TempDir::new().unwrap();
        let mut ws = ShadowWorkspaceV2::new(tmp.path().to_path_buf());

        let err = ws.apply_edit(999, PathBuf::from("x.py"), "y".to_string());
        assert!(err.is_err());
        assert!(err.unwrap_err().to_string().contains("999"));

        let err = ws.read_file(999, Path::new("x.py"));
        assert!(err.is_err());

        let err = ws.validate_branch(999);
        assert!(err.is_err());

        let err = ws.commit_branch(999);
        assert!(err.is_err());
    }

    // -- Read file --

    #[test]
    fn test_read_file_overlay_priority() {
        let tmp = TempDir::new().unwrap();
        let real_file = tmp.path().join("data.py");
        fs::write(&real_file, "original = True\n").unwrap();

        let mut ws = ShadowWorkspaceV2::new(tmp.path().to_path_buf());
        let id = ws.create_branch().unwrap();

        // Before overlay: reads from filesystem.
        let content = ws.read_file(id, &real_file).unwrap();
        assert_eq!(content, "original = True\n");

        // After overlay: reads from branch.
        ws.apply_edit(id, real_file.clone(), "modified = True\n".to_string())
            .unwrap();
        let content = ws.read_file(id, &real_file).unwrap();
        assert_eq!(content, "modified = True\n");

        // Filesystem is untouched.
        assert_eq!(fs::read_to_string(&real_file).unwrap(), "original = True\n");
    }

    #[test]
    fn test_read_file_fallback_to_filesystem() {
        let tmp = TempDir::new().unwrap();
        let real_file = tmp.path().join("exists.txt");
        fs::write(&real_file, "hello fs").unwrap();

        let mut ws = ShadowWorkspaceV2::new(tmp.path().to_path_buf());
        let id = ws.create_branch().unwrap();

        // Relative path resolved against base_dir.
        let content = ws.read_file(id, Path::new("exists.txt")).unwrap();
        assert_eq!(content, "hello fs");

        // Absolute path works too.
        let content = ws.read_file(id, &real_file).unwrap();
        assert_eq!(content, "hello fs");
    }

    // -- Validate branch --

    #[test]
    fn test_validate_branch_clean_file() {
        let tmp = TempDir::new().unwrap();
        let mut ws = ShadowWorkspaceV2::new(tmp.path().to_path_buf());
        let id = ws.create_branch().unwrap();

        ws.apply_edit(
            id,
            PathBuf::from("clean.py"),
            "x = 1\ny = x + 2\nprint(y)\n".to_string(),
        )
        .unwrap();

        let result = ws.validate_branch(id).unwrap();
        // Clean code: score should be 1.0 (or graceful degradation if ruff absent).
        assert!(
            result.score >= 0.99,
            "Clean code should score ~1.0, got {}",
            result.score
        );
        assert_eq!(result.files_modified.len(), 1);
    }

    #[test]
    fn test_validate_branch_with_errors() {
        let tmp = TempDir::new().unwrap();
        let mut ws = ShadowWorkspaceV2::new(tmp.path().to_path_buf());
        let id = ws.create_branch().unwrap();

        // F401: unused import.
        ws.apply_edit(
            id,
            PathBuf::from("bad.py"),
            "import os\nx = 1\n".to_string(),
        )
        .unwrap();

        let result = ws.validate_branch(id).unwrap();
        // If ruff is installed, score < 1.0. If not, graceful degradation = 1.0.
        if !result.diagnostics.is_empty() {
            assert!(
                result.score < 1.0,
                "Branch with lint errors should score < 1.0"
            );
            assert!(
                result
                    .diagnostics
                    .iter()
                    .any(|d| d.code.as_deref() == Some("F401"))
            );
        }
    }

    #[test]
    fn test_validate_branch_rust_uses_cargo_check() {
        let tmp = TempDir::new().unwrap();
        let mut ws = ShadowWorkspaceV2::new(tmp.path().to_path_buf());
        let id = ws.create_branch().unwrap();

        // Type error: assigning a string literal to an i32 binding.
        ws.apply_edit(
            id,
            PathBuf::from("lib.rs"),
            "fn foo() { let _x: i32 = \"not a number\"; }".to_string(),
        )
        .unwrap();

        let result = ws.validate_branch(id).unwrap();
        assert_eq!(result.files_modified.len(), 1, "Modified file is tracked");
        // If cargo is available: diagnostics present, score < 1.0.
        // If cargo is unavailable: graceful degradation, score == 1.0.
        if !result.diagnostics.is_empty() {
            assert!(
                result.score < 1.0,
                "Rust type errors should lower the score, got {}",
                result.score
            );
        }
    }

    #[test]
    fn test_validate_branch_unknown_extension_skipped() {
        let tmp = TempDir::new().unwrap();
        let mut ws = ShadowWorkspaceV2::new(tmp.path().to_path_buf());
        let id = ws.create_branch().unwrap();

        // Unknown extensions (e.g. .md, .json) are not validated.
        ws.apply_edit(
            id,
            PathBuf::from("README.md"),
            "# {{ invalid template syntax".to_string(),
        )
        .unwrap();

        let result = ws.validate_branch(id).unwrap();
        assert!(
            result.diagnostics.is_empty(),
            "Unknown extensions produce no diagnostics"
        );
        assert!((result.score - 1.0).abs() < f64::EPSILON);
    }

    // -- Select best branch --

    #[test]
    fn test_select_best_branch_picks_highest_score() {
        let tmp = TempDir::new().unwrap();
        let mut ws = ShadowWorkspaceV2::new(tmp.path().to_path_buf());

        let id_bad = ws.create_branch().unwrap();
        let id_good = ws.create_branch().unwrap();

        // Manually set scores via validate internals (construct diagnostics).
        {
            let bad = ws.find_branch_mut(id_bad).unwrap();
            bad.diagnostics.push(LintDiagnosticV2 {
                file: "bad.py".to_string(),
                line: 1,
                column: 1,
                severity: DiagnosticSeverity::Error,
                message: "error".to_string(),
                code: Some("E001".to_string()),
            });
            bad.score = score_diagnostics(&bad.diagnostics);
        }
        {
            let good = ws.find_branch_mut(id_good).unwrap();
            // No diagnostics => score 1.0.
            good.score = score_diagnostics(&good.diagnostics);
        }

        let best = ws.select_best_branch().unwrap();
        assert_eq!(best.id, id_good);
        assert!((best.score - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_select_best_branch_tie_broken_by_fewer_files() {
        let tmp = TempDir::new().unwrap();
        let mut ws = ShadowWorkspaceV2::new(tmp.path().to_path_buf());

        let id_many = ws.create_branch().unwrap();
        let id_few = ws.create_branch().unwrap();

        // Both branches: score 1.0 (no diagnostics).
        // id_many has 3 files, id_few has 1 file.
        ws.apply_edit(id_many, PathBuf::from("a.rs"), "a".into())
            .unwrap();
        ws.apply_edit(id_many, PathBuf::from("b.rs"), "b".into())
            .unwrap();
        ws.apply_edit(id_many, PathBuf::from("c.rs"), "c".into())
            .unwrap();

        ws.apply_edit(id_few, PathBuf::from("only.rs"), "x".into())
            .unwrap();

        let best = ws.select_best_branch().unwrap();
        assert_eq!(best.id, id_few, "Tie broken by fewer files modified");
    }

    #[test]
    fn test_select_best_branch_empty_returns_none() {
        let tmp = TempDir::new().unwrap();
        let ws = ShadowWorkspaceV2::new(tmp.path().to_path_buf());
        assert!(ws.select_best_branch().is_none());
    }

    // -- Commit branch --

    #[test]
    fn test_commit_branch_writes_files() {
        let tmp = TempDir::new().unwrap();
        let mut ws = ShadowWorkspaceV2::new(tmp.path().to_path_buf());
        let id = ws.create_branch().unwrap();

        ws.apply_edit(
            id,
            PathBuf::from("output.py"),
            "committed = True\n".to_string(),
        )
        .unwrap();
        ws.apply_edit(
            id,
            PathBuf::from("sub/nested.txt"),
            "nested content\n".to_string(),
        )
        .unwrap();

        let written = ws.commit_branch(id).unwrap();
        assert_eq!(written.len(), 2);

        // Verify files are on disk.
        let output = fs::read_to_string(tmp.path().join("output.py")).unwrap();
        assert_eq!(output, "committed = True\n");

        let nested = fs::read_to_string(tmp.path().join("sub/nested.txt")).unwrap();
        assert_eq!(nested, "nested content\n");
    }

    // -- Discard branch --

    #[test]
    fn test_discard_branch_frees_memory() {
        let tmp = TempDir::new().unwrap();
        let mut ws = ShadowWorkspaceV2::new(tmp.path().to_path_buf());
        let id1 = ws.create_branch().unwrap();
        let id2 = ws.create_branch().unwrap();
        assert_eq!(ws.branch_count(), 2);

        ws.discard_branch(id1);
        assert_eq!(ws.branch_count(), 1);

        // Discarding nonexistent branch is a no-op.
        ws.discard_branch(id1);
        assert_eq!(ws.branch_count(), 1);

        // Remaining branch still accessible.
        assert!(ws.read_file(id2, Path::new("anything")).is_err()); // file not found, but branch exists
        assert!(ws.find_branch(id2).is_some());
    }

    // -- Reset --

    #[test]
    fn test_reset_clears_all_branches() {
        let tmp = TempDir::new().unwrap();
        let mut ws = ShadowWorkspaceV2::new(tmp.path().to_path_buf());
        ws.create_branch().unwrap();
        ws.create_branch().unwrap();
        ws.create_branch().unwrap();
        assert_eq!(ws.branch_count(), 3);

        ws.reset();
        assert_eq!(ws.branch_count(), 0);

        // IDs keep incrementing after reset.
        let id = ws.create_branch().unwrap();
        assert!(id > 3, "IDs should be monotonically increasing after reset");
    }

    // -- Compare branches --

    #[test]
    fn test_compare_branches_returns_all() {
        let tmp = TempDir::new().unwrap();
        let mut ws = ShadowWorkspaceV2::new(tmp.path().to_path_buf());
        ws.create_branch().unwrap();
        ws.create_branch().unwrap();

        let results = ws.compare_branches();
        assert_eq!(results.len(), 2);
    }

    // -- Scoring --

    #[test]
    fn test_scoring_no_diagnostics() {
        let score = score_diagnostics(&[]);
        assert!((score - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_scoring_errors_vs_warnings() {
        // 1 error => 1.0 - 0.3 = 0.7
        let one_error = vec![LintDiagnosticV2 {
            file: "f.py".into(),
            line: 1,
            column: 1,
            severity: DiagnosticSeverity::Error,
            message: "err".into(),
            code: None,
        }];
        assert!((score_diagnostics(&one_error) - 0.7).abs() < f64::EPSILON);

        // 1 warning => 1.0 - 0.05 = 0.95
        let one_warning = vec![LintDiagnosticV2 {
            file: "f.py".into(),
            line: 1,
            column: 1,
            severity: DiagnosticSeverity::Warning,
            message: "warn".into(),
            code: None,
        }];
        assert!((score_diagnostics(&one_warning) - 0.95).abs() < f64::EPSILON);

        // 1 info => 1.0 - 0.01 = 0.99
        let one_info = vec![LintDiagnosticV2 {
            file: "f.py".into(),
            line: 1,
            column: 1,
            severity: DiagnosticSeverity::Info,
            message: "info".into(),
            code: None,
        }];
        assert!((score_diagnostics(&one_info) - 0.99).abs() < f64::EPSILON);

        // 4 errors => 1.0 - 1.2 = clamped to 0.0
        let many_errors: Vec<_> = (0..4)
            .map(|i| LintDiagnosticV2 {
                file: "f.py".into(),
                line: i + 1,
                column: 1,
                severity: DiagnosticSeverity::Error,
                message: format!("err {i}"),
                code: None,
            })
            .collect();
        assert!((score_diagnostics(&many_errors)).abs() < f64::EPSILON);
    }

    #[test]
    fn test_scoring_mixed() {
        // 2 errors + 3 warnings + 1 info = 1.0 - 0.6 - 0.15 - 0.01 = 0.24
        let diags = vec![
            LintDiagnosticV2 {
                file: "f.py".into(),
                line: 1,
                column: 1,
                severity: DiagnosticSeverity::Error,
                message: "e1".into(),
                code: None,
            },
            LintDiagnosticV2 {
                file: "f.py".into(),
                line: 2,
                column: 1,
                severity: DiagnosticSeverity::Error,
                message: "e2".into(),
                code: None,
            },
            LintDiagnosticV2 {
                file: "f.py".into(),
                line: 3,
                column: 1,
                severity: DiagnosticSeverity::Warning,
                message: "w1".into(),
                code: None,
            },
            LintDiagnosticV2 {
                file: "f.py".into(),
                line: 4,
                column: 1,
                severity: DiagnosticSeverity::Warning,
                message: "w2".into(),
                code: None,
            },
            LintDiagnosticV2 {
                file: "f.py".into(),
                line: 5,
                column: 1,
                severity: DiagnosticSeverity::Warning,
                message: "w3".into(),
                code: None,
            },
            LintDiagnosticV2 {
                file: "f.py".into(),
                line: 6,
                column: 1,
                severity: DiagnosticSeverity::Info,
                message: "i1".into(),
                code: None,
            },
        ];
        let score = score_diagnostics(&diags);
        assert!(
            (score - 0.24).abs() < f64::EPSILON,
            "Expected 0.24, got {score}"
        );
    }

    // -- Classify severity --

    #[test]
    fn test_classify_severity() {
        // E5xx are style warnings (e.g. line too long).
        assert_eq!(classify_severity("E501"), DiagnosticSeverity::Warning);
        assert_eq!(classify_severity("E502"), DiagnosticSeverity::Warning);
        // E1xx-E4xx, E9xx are real errors.
        assert_eq!(classify_severity("E101"), DiagnosticSeverity::Error);
        assert_eq!(classify_severity("E401"), DiagnosticSeverity::Error);
        assert_eq!(classify_severity("E999"), DiagnosticSeverity::Error);
        // F codes are errors.
        assert_eq!(classify_severity("F401"), DiagnosticSeverity::Error);
        // W codes are warnings.
        assert_eq!(classify_severity("W291"), DiagnosticSeverity::Warning);
        // Everything else is info.
        assert_eq!(classify_severity("C901"), DiagnosticSeverity::Info);
        assert_eq!(classify_severity("I001"), DiagnosticSeverity::Info);
        assert_eq!(classify_severity("unknown"), DiagnosticSeverity::Info);
    }

    // -- parse_ruff_json --

    #[test]
    fn test_parse_ruff_json_empty() {
        assert!(parse_ruff_json("", "f.py").is_empty());
        assert!(parse_ruff_json("[]", "f.py").is_empty());
    }

    #[test]
    fn test_parse_ruff_json_valid() {
        let json = r#"[{
            "location": {"row": 3, "column": 8},
            "code": "F401",
            "message": "unused import"
        }]"#;
        let diags = parse_ruff_json(json, "my_file.py");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 3);
        assert_eq!(diags[0].column, 8);
        assert_eq!(diags[0].code.as_deref(), Some("F401"));
        assert_eq!(diags[0].file, "my_file.py");
        assert_eq!(diags[0].severity, DiagnosticSeverity::Error);
    }

    #[test]
    fn test_parse_ruff_json_invalid() {
        assert!(parse_ruff_json("not json", "f.py").is_empty());
    }

    // -- parse_cargo_json --

    #[test]
    fn test_parse_cargo_json_empty() {
        assert!(parse_cargo_json("", "lib.rs").is_empty());
        assert!(parse_cargo_json("\n\n", "lib.rs").is_empty());
    }

    #[test]
    fn test_parse_cargo_json_build_finished_ignored() {
        let json = r#"{"reason":"build-finished","success":false}"#;
        assert!(parse_cargo_json(json, "lib.rs").is_empty());
    }

    #[test]
    fn test_parse_cargo_json_compiler_message_error() {
        let json = r#"{"reason":"compiler-message","package_id":"x 0.1.0","manifest_path":"/tmp/Cargo.toml","target":{"kind":["lib"],"crate_types":["lib"],"name":"x","src_path":"src/lib.rs","edition":"2021","doc":true,"doctest":true,"test":true},"message":{"rendered":"error[E0308]","children":[],"code":{"code":"E0308","explanation":""},"level":"error","message":"mismatched types","spans":[{"byte_end":25,"byte_start":15,"column_end":26,"column_start":16,"expansion":null,"file_name":"src/lib.rs","is_primary":true,"label":null,"line_end":1,"line_start":1,"suggested_replacement":null,"suggestion_applicability":null,"text":[]}]}}"#;
        let diags = parse_cargo_json(json, "src/lib.rs");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, DiagnosticSeverity::Error);
        assert_eq!(diags[0].code.as_deref(), Some("E0308"));
        assert_eq!(diags[0].message, "mismatched types");
        assert_eq!(diags[0].line, 1);
        assert_eq!(diags[0].column, 16);
        assert_eq!(diags[0].file, "src/lib.rs");
    }

    #[test]
    fn test_parse_cargo_json_skips_notes_and_help() {
        let note = r#"{"reason":"compiler-message","package_id":"x","manifest_path":"Cargo.toml","target":{},"message":{"rendered":"note:","children":[],"code":null,"level":"note","message":"note content","spans":[]}}"#;
        let help = r#"{"reason":"compiler-message","package_id":"x","manifest_path":"Cargo.toml","target":{},"message":{"rendered":"help:","children":[],"code":null,"level":"help","message":"help content","spans":[]}}"#;
        let aborting = r#"{"reason":"compiler-message","package_id":"x","manifest_path":"Cargo.toml","target":{},"message":{"rendered":"aborting","children":[],"code":null,"level":"error","message":"aborting due to 1 previous error","spans":[]}}"#;
        assert!(parse_cargo_json(note, "lib.rs").is_empty(), "notes ignored");
        assert!(parse_cargo_json(help, "lib.rs").is_empty(), "help ignored");
        assert!(
            parse_cargo_json(aborting, "lib.rs").is_empty(),
            "aborting ignored"
        );
    }

    #[test]
    fn test_parse_cargo_json_warning() {
        let json = r#"{"reason":"compiler-message","package_id":"x","manifest_path":"Cargo.toml","target":{},"message":{"rendered":"warning:","children":[],"code":{"code":"dead_code","explanation":null},"level":"warning","message":"function is never used: `foo`","spans":[{"byte_end":10,"byte_start":4,"column_end":8,"column_start":4,"expansion":null,"file_name":"src/lib.rs","is_primary":true,"label":null,"line_end":2,"line_start":2,"suggested_replacement":null,"suggestion_applicability":null,"text":[]}]}}"#;
        let diags = parse_cargo_json(json, "src/lib.rs");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].severity, DiagnosticSeverity::Warning);
        assert_eq!(diags[0].line, 2);
        assert_eq!(diags[0].column, 4);
    }

    // -- parse_tsc_output --

    #[test]
    fn test_parse_tsc_output_empty() {
        assert!(parse_tsc_output("", "x.ts").is_empty());
        assert!(parse_tsc_output("\n\n", "x.ts").is_empty());
    }

    #[test]
    fn test_parse_tsc_output_error() {
        let output = "source.ts(5,3): error TS2304: Cannot find name 'foo'.";
        let diags = parse_tsc_output(output, "src/myfile.ts");
        assert_eq!(diags.len(), 1);
        assert_eq!(diags[0].line, 5);
        assert_eq!(diags[0].column, 3);
        assert_eq!(diags[0].severity, DiagnosticSeverity::Error);
        assert_eq!(diags[0].code.as_deref(), Some("TS2304"));
        assert_eq!(diags[0].message, "Cannot find name 'foo'.");
        assert_eq!(diags[0].file, "src/myfile.ts");
    }

    #[test]
    fn test_parse_tsc_output_multiple_diagnostics() {
        let output = "file.ts(1,1): error TS2304: Cannot find name 'x'.\nfile.ts(3,5): warning TS6133: 'y' is declared but its value is never read.";
        let diags = parse_tsc_output(output, "file.ts");
        assert_eq!(diags.len(), 2);
        assert_eq!(diags[0].severity, DiagnosticSeverity::Error);
        assert_eq!(diags[0].line, 1);
        assert_eq!(diags[1].severity, DiagnosticSeverity::Warning);
        assert_eq!(diags[1].line, 3);
    }

    #[test]
    fn test_parse_tsc_output_skips_unrecognized_level() {
        // Lines without "error"/"warning" after ')' should be skipped.
        let output = "file.ts(1,1): message TS2304: Something.\nsome line without parens at all";
        let diags = parse_tsc_output(output, "file.ts");
        assert!(
            diags.is_empty(),
            "Unrecognized levels produce no diagnostics"
        );
    }

    #[test]
    fn test_parse_tsc_output_zero_line_skipped() {
        // If line number is 0 or unparseable, skip the entry.
        let output = "file.ts(abc,def): error TS2304: bad coords.";
        let diags = parse_tsc_output(output, "file.ts");
        assert!(diags.is_empty(), "Unparseable line numbers are skipped");
    }

    // -- Branch isolation --

    #[test]
    fn test_branches_are_isolated() {
        let tmp = TempDir::new().unwrap();
        let mut ws = ShadowWorkspaceV2::new(tmp.path().to_path_buf());
        let id1 = ws.create_branch().unwrap();
        let id2 = ws.create_branch().unwrap();

        ws.apply_edit(id1, PathBuf::from("shared.py"), "branch_1_version\n".into())
            .unwrap();
        ws.apply_edit(id2, PathBuf::from("shared.py"), "branch_2_version\n".into())
            .unwrap();

        let content1 = ws.read_file(id1, Path::new("shared.py")).unwrap();
        let content2 = ws.read_file(id2, Path::new("shared.py")).unwrap();

        assert_eq!(content1, "branch_1_version\n");
        assert_eq!(content2, "branch_2_version\n");
        assert_ne!(content1, content2);
    }
}
