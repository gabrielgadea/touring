//! Enforcement Handlers — CILA strategy enforcer + data structure validator + S1 handlers.
//!
//! Ported from:
//! - `.claude/rust-core/src/bin/hook_strategy_enforcer.rs`
//! - `.claude/rust-core/src/bin/hook_data_structure.rs`
//! - `.claude/hooks/validation/code_first_pipeline_validator.py` (S1)
//! - `.claude/hooks/validation/siac_pre_validator.py` (S1)
//! - `.claude/hooks/validation/aco_rollback_enforcer.py` (S1)
//! - `.claude/hooks/validation/user_prompt_validator.py` (S1)
//! - `.claude/hooks/validation/siac_hook_entry.py` (S1)
//! - `.claude/hooks/validation/siac_orchestrator.py` (S1)

use std::collections::HashSet;
use std::path::Path;

use once_cell::sync::Lazy;
use regex::Regex;

use crate::context::CortexContext;
use crate::handler::Handler;
use crate::pipeline::Pipeline;
use crate::types::{HandlerResult, HookEvent};
use touring_hooks as hooks;

// ══════════════════════════════════════════════════════════════════════
// 1. StrategyEnforcerHandler
// ══════════════════════════════════════════════════════════════════════

/// CILA Strategy Enforcer — advisory warnings on Task tool calls.
///
/// Ported from `hook_strategy_enforcer.rs`. When the Task tool is invoked,
/// classifies the prompt via `IntentClassifier` and injects CILA-level
/// warnings:
/// - L1+: CODE-FIRST workflow reminder
/// - L2+: DISCOVER obligation
/// - L3+: pipeline_state requirement
/// - L4 ACO: GeneratorGraph recommendation
///
/// **Sync, never blocks** (advisory only).
pub struct StrategyEnforcerHandler {
    classifier: hooks::IntentClassifier,
}

impl Default for StrategyEnforcerHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl StrategyEnforcerHandler {
    /// Creates a handler with a freshly initialized intent classifier.
    pub fn new() -> Self {
        Self {
            classifier: hooks::IntentClassifier::new(),
        }
    }
}

/// Check if any `pipeline_state_*.json` file exists under `analise/relatoria/`.
fn check_pipeline_state(project_root: &Path) -> bool {
    let relatoria = project_root.join("analise").join("relatoria");
    if !relatoria.exists() {
        return false;
    }
    scan_dir_for_pattern(&relatoria, "pipeline_state_", ".json")
}

/// Check if `approved_strategy.json` exists under `analise/relatoria/`.
fn check_approved_strategy(project_root: &Path) -> bool {
    let relatoria = project_root.join("analise").join("relatoria");
    if !relatoria.exists() {
        return false;
    }
    scan_dir_for_exact(&relatoria, "approved_strategy.json")
}

/// Recursively scan for files matching prefix+suffix.
fn scan_dir_for_pattern(dir: &Path, prefix: &str, suffix: &str) -> bool {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return false,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if scan_dir_for_pattern(&path, prefix, suffix) {
                return true;
            }
        } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name.starts_with(prefix) && name.ends_with(suffix) {
                return true;
            }
        }
    }
    false
}

/// Recursively scan for a file with exact name.
fn scan_dir_for_exact(dir: &Path, target: &str) -> bool {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(_) => return false,
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            if scan_dir_for_exact(&path, target) {
                return true;
            }
        } else if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
            if name == target {
                return true;
            }
        }
    }
    false
}

/// Build enforcement context string from classification result.
fn enforce_and_build_context(
    level: u8,
    routing_strategy: &str,
    requires_pipeline: bool,
    requires_code_first: bool,
    project_root: &Path,
) -> Option<String> {
    let mut warnings: Vec<String> = Vec::new();
    let mut violations: Vec<String> = Vec::new();

    // L3+ requires pipeline_state
    if requires_pipeline {
        let pipeline_exists = check_pipeline_state(project_root);
        if !pipeline_exists {
            violations.push(format!(
                "L{} requires pipeline_state check. Run pipeline_runner BEFORE activating skills.",
                level
            ));
        } else {
            let approved = check_approved_strategy(project_root);
            if !approved {
                violations.push(format!(
                    "L{} requires B2.5 approval gate. \
                     approved_strategy.json not found. \
                     Run interactive_checkpoint (B2.5) before F10-F16 analysis.",
                    level
                ));
            }
        }
    }

    // L1+ should follow CODE-FIRST
    if requires_code_first {
        warnings.push(format!(
            "L{} requires CODE-FIRST workflow: DISCOVER\u{2192}EXECUTE\u{2192}SYNTHESIZE.",
            level
        ));
    }

    // L2+ must run DISCOVER before spawning sub-tasks
    if level >= 2 {
        warnings.push(format!(
            "DISCOVER OBRIGAT\u{00d3}RIO (L{}): \
             python scripts/discover.py \"<keyword>\" \
             \u{2014} find existing scripts before creating new ones.",
            level
        ));
    }

    // ACO mode (L4): GeneratorGraph recommended
    if routing_strategy == "aco_orchestration" {
        warnings.push(format!(
            "ACO L{}: GeneratorGraph recomendado antes de prosseguir. \
             Execute: python scripts/aco/orchestrator.py --intent '<inten\u{00e7}\u{00e3}o>'",
            level
        ));
    }

    if warnings.is_empty() && violations.is_empty() {
        return None;
    }

    let action = "WARN";

    let mut parts = vec![format!("CILA-Enforcer L{}: {}", level, action)];
    for v in &violations {
        parts.push(format!("VIOLATION: {}", v));
    }
    for w in &warnings {
        parts.push(format!("WARNING: {}", w));
    }

    Some(parts.join(". ") + ".")
}

impl Handler for StrategyEnforcerHandler {
    fn name(&self) -> &str {
        "strategy_enforcer"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PreToolUse]
    }

    fn tool_matcher(&self) -> Option<&str> {
        Some("Task")
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        // Extract prompt from Task tool_input
        let prompt = ctx
            .tool_input
            .get("prompt")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if prompt.trim().is_empty() {
            return HandlerResult::skip(self.name());
        }

        let result = self.classifier.classify(prompt);

        match enforce_and_build_context(
            result.level,
            &result.routing_strategy,
            result.requires_pipeline,
            result.requires_code_first,
            &ctx.project_root,
        ) {
            Some(context_msg) => HandlerResult::allow(self.name(), Some(context_msg)),
            None => HandlerResult::skip(self.name()),
        }
    }
}

// ══════════════════════════════════════════════════════════════════════
// 2. DataStructureHandler
// ══════════════════════════════════════════════════════════════════════

/// Canonical top-level directories from DataPaths (mirrors paths.py).
static CANONICAL_TOP_DIRS: Lazy<HashSet<&'static str>> = Lazy::new(|| {
    [
        "raw",
        "databases",
        "bm25_index",
        "vector_store",
        "graph_store",
        "reports",
        "archives",
        ".personal",
    ]
    .into_iter()
    .collect()
});

/// Archive directory name — writes are blocked (read-only after archiving).
const ARCHIVE_DIR: &str = "archives";

/// Data Structure Validator — governance enforcement for `data/` paths.
///
/// Ported from `hook_data_structure.rs`. On Write/Edit targeting `data/`,
/// validates:
/// - Must write to canonical DataPaths directory
/// - Must follow naming conventions (snake_case, ASCII, no accents/spaces)
/// - Must not write to `archives/` (read-only)
/// - Warns on non-canonical top-level dirs
///
/// **Sync, can warn** (blocks archive writes).
pub struct DataStructureHandler;

/// Check if a filename contains non-ASCII characters or spaces.
fn has_accent_or_space(name: &str) -> bool {
    name.contains(' ') || !name.is_ascii()
}

/// Extract the relative path under `data/` from a file path.
/// Returns `None` if the path is not under `data/`.
fn extract_data_rel_path(file_path: &str) -> Option<String> {
    let segments: Vec<&str> = file_path.split('/').filter(|s| !s.is_empty()).collect();
    let data_idx = segments.iter().position(|&s| s == "data")?;
    // SAFETY: data_idx is from position() so data_idx < segments.len(),
    // therefore data_idx + 1 <= segments.len() which is valid for slicing.
    #[allow(clippy::indexing_slicing)]
    let after_data = &segments[data_idx + 1..];
    if after_data.is_empty() {
        return Some(String::new());
    }
    Some(after_data.join("/"))
}

/// Result of validating a proposed write to a data/ path.
struct DataValidation {
    decision: &'static str,
    reason: String,
}

/// Validate a proposed write to a data/ path.
fn validate_data_write(file_path: &str) -> DataValidation {
    let rel_path = match extract_data_rel_path(file_path) {
        Some(p) => p,
        None => {
            return DataValidation {
                decision: "allow",
                reason: "path not under data/".to_string(),
            };
        }
    };

    if rel_path.is_empty() {
        return DataValidation {
            decision: "warn",
            reason: "writing directly to data/ root is unusual".to_string(),
        };
    }

    let top_dir = rel_path.split('/').next().unwrap_or(&rel_path);

    // Block writes to archives/
    if top_dir == ARCHIVE_DIR {
        return DataValidation {
            decision: "block",
            reason: format!(
                "data/archives/ is read-only. Use organize_archives.py to move files in. Attempted: data/{}",
                rel_path
            ),
        };
    }

    // Warn for non-canonical top-level dir
    if !CANONICAL_TOP_DIRS.contains(top_dir) {
        let mut sorted_dirs: Vec<&&str> = CANONICAL_TOP_DIRS.iter().collect();
        sorted_dirs.sort();
        return DataValidation {
            decision: "warn",
            reason: format!(
                "'{}' is not a canonical DataPaths directory. Canonical: {:?}. Update DataPaths before creating new top-level dirs.",
                top_dir, sorted_dirs
            ),
        };
    }

    // Warn for naming convention violations
    let filename = rel_path.rsplit('/').next().unwrap_or(&rel_path);
    if has_accent_or_space(filename) {
        return DataValidation {
            decision: "warn",
            reason: format!(
                "Filename '{}' contains accents or spaces. Use snake_case ASCII names per data governance rules.",
                filename
            ),
        };
    }

    DataValidation {
        decision: "allow",
        reason: format!("canonical path: data/{}", rel_path),
    }
}

impl Handler for DataStructureHandler {
    fn name(&self) -> &str {
        "data_structure"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PreToolUse]
    }

    fn tool_matcher(&self) -> Option<&str> {
        Some("Write|Edit")
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let file_path = ctx.tool_file_path();

        // Quick check: does the path contain "data/"?
        if file_path.is_empty() || !file_path.contains("data/") {
            return HandlerResult::skip(self.name());
        }

        let result = validate_data_write(file_path);

        match result.decision {
            "block" => HandlerResult::block(self.name(), result.reason),
            "warn" => HandlerResult::allow(self.name(), Some(result.reason)),
            _ => HandlerResult::skip(self.name()),
        }
    }
}

// ── Registration ──────────────────────────────────────────────────────

/// Register enforcement handlers (8 total: 2 original + 6 S1).
pub fn register(pipeline: &mut Pipeline) {
    pipeline.register(Box::new(StrategyEnforcerHandler::new()));
    pipeline.register(Box::new(DataStructureHandler));
    // S1 handlers
    pipeline.register(Box::new(CodeFirstPipelineValidator));
    pipeline.register(Box::new(SiacPreValidator));
    pipeline.register(Box::new(AcoRollbackEnforcer));
    pipeline.register(Box::new(UserPromptValidator));
    pipeline.register(Box::new(SiacHookEntry));
    pipeline.register(Box::new(SiacOrchestrator));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::CortexContext;
    use crate::types::Decision;
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    #[allow(clippy::arc_with_non_send_sync)] // single-threaded test context
    fn make_test_ctx(event: HookEvent, input: serde_json::Value) -> (TempDir, CortexContext) {
        let tmp = TempDir::new().unwrap();
        let db = FileKnowledgeDB::new(&tmp.path().join("test.db")).unwrap();
        let knowledge = Arc::new(db);
        let ctx = CortexContext::from_input(event, input, knowledge, tmp.path().to_path_buf());
        (tmp, ctx)
    }

    // ── StrategyEnforcerHandler ─────────────────────────────────────

    #[test]
    fn test_strategy_enforcer_skip_non_task() {
        let input = serde_json::json!({
            "tool_name": "Read",
            "tool_input": {"file_path": "/tmp/x.py"}
        });
        let (_tmp, mut ctx) = make_test_ctx(HookEvent::PreToolUse, input);
        let handler = StrategyEnforcerHandler::new();
        // Read does not match "Task" tool_matcher, so pipeline would not dispatch.
        // But if we call execute directly, it should skip due to empty prompt.
        let result = handler.execute(&mut ctx);
        assert!(matches!(result.decision, Decision::Skip));
    }

    #[test]
    fn test_strategy_enforcer_l0_no_context() {
        let ctx_msg = enforce_and_build_context(0, "direct", false, false, Path::new("/tmp"));
        assert!(ctx_msg.is_none(), "L0 should produce no context");
    }

    #[test]
    fn test_strategy_enforcer_l1_code_first() {
        let ctx_msg = enforce_and_build_context(1, "compute", false, true, Path::new("/tmp"));
        assert!(ctx_msg.is_some());
        let msg = ctx_msg.unwrap();
        assert!(msg.contains("CILA-Enforcer L1"));
        assert!(msg.contains("CODE-FIRST"));
    }

    #[test]
    fn test_strategy_enforcer_l2_discover() {
        let ctx_msg = enforce_and_build_context(2, "tool_use", false, true, Path::new("/tmp"));
        assert!(ctx_msg.is_some());
        let msg = ctx_msg.unwrap();
        assert!(msg.contains("DISCOVER"));
        assert!(msg.contains("L2"));
    }

    #[test]
    fn test_strategy_enforcer_l3_pipeline_violation() {
        let ctx_msg = enforce_and_build_context(3, "pipeline", true, true, Path::new("/tmp"));
        assert!(ctx_msg.is_some());
        let msg = ctx_msg.unwrap();
        assert!(msg.contains("VIOLATION"));
        assert!(msg.contains("pipeline_state"));
    }

    #[test]
    fn test_strategy_enforcer_l4_aco() {
        let ctx_msg =
            enforce_and_build_context(4, "aco_orchestration", false, true, Path::new("/tmp"));
        assert!(ctx_msg.is_some());
        let msg = ctx_msg.unwrap();
        assert!(msg.contains("ACO"));
        assert!(msg.contains("GeneratorGraph"));
        assert!(!msg.contains("pipeline_state"));
    }

    #[test]
    fn test_strategy_enforcer_with_task_tool() {
        let input = serde_json::json!({
            "tool_name": "Task",
            "tool_input": {"prompt": "calcule o VPL da concessao"}
        });
        let (_tmp, mut ctx) = make_test_ctx(HookEvent::PreToolUse, input);
        let handler = StrategyEnforcerHandler::new();
        let result = handler.execute(&mut ctx);
        // L1 compute should produce CODE-FIRST warning
        assert!(matches!(result.decision, Decision::Allow));
        assert!(!result.context_lines.is_empty());
        assert!(result.context_lines[0].contains("CILA-Enforcer"));
    }

    #[test]
    fn test_strategy_enforcer_empty_prompt() {
        let input = serde_json::json!({
            "tool_name": "Task",
            "tool_input": {"prompt": ""}
        });
        let (_tmp, mut ctx) = make_test_ctx(HookEvent::PreToolUse, input);
        let handler = StrategyEnforcerHandler::new();
        let result = handler.execute(&mut ctx);
        assert!(matches!(result.decision, Decision::Skip));
    }

    // ── DataStructureHandler ────────────────────────────────────────

    #[test]
    fn test_data_structure_skip_non_data() {
        let input = serde_json::json!({
            "tool_name": "Write",
            "tool_input": {"file_path": "/home/user/src/main.rs"}
        });
        let (_tmp, mut ctx) = make_test_ctx(HookEvent::PreToolUse, input);
        let handler = DataStructureHandler;
        let result = handler.execute(&mut ctx);
        assert!(matches!(result.decision, Decision::Skip));
    }

    #[test]
    fn test_data_structure_allow_canonical() {
        let v = validate_data_write("/home/user/data/raw/document.md");
        assert_eq!(v.decision, "allow");
        assert!(v.reason.contains("canonical path"));
    }

    #[test]
    fn test_data_structure_block_archive() {
        let v = validate_data_write("/home/user/data/archives/old_file.md");
        assert_eq!(v.decision, "block");
        assert!(v.reason.contains("read-only"));
    }

    #[test]
    fn test_data_structure_warn_non_canonical() {
        let v = validate_data_write("/home/user/data/unknown_dir/file.txt");
        assert_eq!(v.decision, "warn");
        assert!(v.reason.contains("not a canonical"));
    }

    #[test]
    fn test_data_structure_warn_accent() {
        let v = validate_data_write("/home/user/data/raw/relat\u{00f3}rio.md");
        assert_eq!(v.decision, "warn");
        assert!(v.reason.contains("accents or spaces"));
    }

    #[test]
    fn test_data_structure_warn_space() {
        let v = validate_data_write("/home/user/data/raw/my file.txt");
        assert_eq!(v.decision, "warn");
        assert!(v.reason.contains("accents or spaces"));
    }

    #[test]
    fn test_data_structure_allow_databases() {
        let v = validate_data_write("data/databases/index.db");
        assert_eq!(v.decision, "allow");
    }

    #[test]
    fn test_data_structure_handler_blocks_archive() {
        let input = serde_json::json!({
            "tool_name": "Write",
            "tool_input": {"file_path": "/project/data/archives/old.md"}
        });
        let (_tmp, mut ctx) = make_test_ctx(HookEvent::PreToolUse, input);
        let handler = DataStructureHandler;
        let result = handler.execute(&mut ctx);
        assert!(matches!(result.decision, Decision::Block(_)));
    }

    #[test]
    fn test_extract_data_rel_path() {
        assert_eq!(
            extract_data_rel_path("/home/user/data/raw/file.md").as_deref(),
            Some("raw/file.md")
        );
        assert_eq!(
            extract_data_rel_path("data/databases/test.db").as_deref(),
            Some("databases/test.db")
        );
        assert!(extract_data_rel_path("/home/user/src/main.rs").is_none());
        assert_eq!(
            extract_data_rel_path("/home/user/data/").as_deref(),
            Some("")
        );
    }

    #[test]
    fn test_has_accent_or_space() {
        assert!(!has_accent_or_space("file_name.txt"));
        assert!(has_accent_or_space("file name.txt"));
        assert!(has_accent_or_space("relat\u{00f3}rio.md"));
    }
}

// ══════════════════════════════════════════════════════════════════════
// 3. CodeFirstPipelineValidator (S1)
// ══════════════════════════════════════════════════════════════════════

/// ANTT skill names that require pipeline_state validation.
static ANTT_SKILLS: Lazy<Vec<&'static str>> = Lazy::new(|| {
    vec![
        "antt-preliminary-analyzer",
        "antt-vote-architect",
        "antt-normative-mapper",
        "antt-contract-analyzer",
        "antt-financial-analyzer",
        "antt-risk-assessor",
        "antt-compliance-checker",
        "antt-tariff-reviewer",
        "antt-concession-analyst",
        "antt-regulatory-impact",
        "antt-process-orchestrator",
        "antt-document-classifier",
        "antt-sei-parser",
        "antt-legal-opinion",
        "antt-deliberation-drafter",
        "antt-rag-intelligence",
    ]
});

/// Regex for ANTT process numbers: 50500.XXXXXX-YYYY-ZZ or 50505.XXXXXX-YYYY-ZZ
static RE_PROCESS_NUMBER: Lazy<Regex> = Lazy::new(|| {
    Regex::new(r"50500\.\d{6}-\d{4}-\d{2}|50505\.\d{6}-\d{4}-\d{2}").expect("valid regex literal")
});

/// Code-First Pipeline Validator — blocks ANTT skill invocations without pipeline_state.
///
/// Ported from `code_first_pipeline_validator.py` + `code_first_validator_entry.py`.
/// When the Task tool invokes an ANTT skill, checks that pipeline_state_*.json
/// exists and has adequate quality scores. **Fail-open**: errors → Skip.
pub struct CodeFirstPipelineValidator;

impl CodeFirstPipelineValidator {
    /// Check pipeline_state for a given process number.
    fn validate_pipeline(project_root: &Path, process_number: &str) -> Result<(), String> {
        // Normalize process number for directory lookup (replace dots/dashes)
        let process_dir = process_number.replace(['.', '-'], "_");
        let relatoria = project_root.join("analise").join("relatoria");

        // Search for pipeline_state_*.json in the process directory
        let process_path = relatoria.join(&process_dir);
        if process_path.is_dir() && scan_dir_for_pattern(&process_path, "pipeline_state_", ".json")
        {
            return Self::validate_quality_scores(&process_path);
        }

        // Fallback: search all subdirs of relatoria
        if relatoria.is_dir() && scan_dir_for_pattern(&relatoria, "pipeline_state_", ".json") {
            // Found somewhere — validate quality scores at relatoria level
            return Self::validate_quality_scores(&relatoria);
        }

        Err(format!(
            "BLOCK: Pipeline state not found for process {}. \
             Run `python -m scripts.process_analysis.pipeline_runner <path>` \
             (F1-F9) BEFORE activating ANTT skills.",
            process_number
        ))
    }

    /// Validate that pipeline_state phases have quality_score >= 0.7.
    fn validate_quality_scores(dir: &Path) -> Result<(), String> {
        let entries = match std::fs::read_dir(dir) {
            Ok(e) => e,
            Err(_) => return Ok(()), // fail-open
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with("pipeline_state_") && name.ends_with(".json") {
                    // Try to read and parse
                    if let Ok(content) = std::fs::read_to_string(&path) {
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                            // Check phases quality scores
                            if let Some(phases) = json.get("phases").and_then(|p| p.as_object()) {
                                for (phase_name, phase_data) in phases {
                                    if let Some(score) =
                                        phase_data.get("quality_score").and_then(|s| s.as_f64())
                                    {
                                        if score < 0.7 {
                                            return Err(format!(
                                                "BLOCK: Phase '{}' has quality_score {:.2} (< 0.7). \
                                                 Re-run pipeline to improve quality before analysis.",
                                                phase_name, score
                                            ));
                                        }
                                    }
                                }
                            }
                        }
                    }
                    // Found a pipeline state file — pass
                    return Ok(());
                }
            }
        }
        Ok(()) // fail-open
    }
}

impl Handler for CodeFirstPipelineValidator {
    fn name(&self) -> &str {
        "code_first_pipeline_validator"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PreToolUse]
    }

    fn tool_matcher(&self) -> Option<&str> {
        Some("Task")
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let prompt = match ctx.tool_input.get("prompt").and_then(|v| v.as_str()) {
            Some(p) if !p.is_empty() => p,
            _ => return HandlerResult::skip(self.name()),
        };

        let prompt_lower = prompt.to_lowercase();

        // Check if prompt mentions an ANTT skill
        let mentions_antt_skill = ANTT_SKILLS.iter().any(|skill| prompt_lower.contains(skill));
        if !mentions_antt_skill {
            return HandlerResult::skip(self.name());
        }

        // Look for process number
        let process_number = match RE_PROCESS_NUMBER.find(prompt) {
            Some(m) => m.as_str().to_string(),
            None => {
                // ANTT skill without process number — warn but don't block
                return HandlerResult::allow(
                    self.name(),
                    Some(
                        "ANTT skill detected but no process number found. \
                         Ensure pipeline_state exists before analysis."
                            .to_string(),
                    ),
                );
            }
        };

        // Validate pipeline state exists
        match Self::validate_pipeline(&ctx.project_root, &process_number) {
            Ok(()) => HandlerResult::skip(self.name()),
            Err(reason) => HandlerResult::block(self.name(), reason),
        }
    }
}

// ══════════════════════════════════════════════════════════════════════
// 4. SiacPreValidator (S1)
// ══════════════════════════════════════════════════════════════════════

/// Regex: mutable default list in function parameters — `def func(x=[])`.
static RE_MUTABLE_DEFAULT: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"def\s+\w+\(.*=\s*\[\]").expect("valid regex literal"));

/// Regex: bare `except:` (no exception type).
static RE_BARE_EXCEPT: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"except\s*:").expect("valid regex literal"));

/// Regex: dangerous `eval(` or `exec(` calls.
static RE_EVAL_EXEC: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\beval\(|\bexec\(").expect("valid regex literal"));

/// SIAC Pre-Validator — lightweight syntax checks on Python writes/edits.
///
/// Ported from `siac_pre_validator.py`. Checks for common anti-patterns:
/// - Mutable default arguments (`def f(x=[])`)
/// - Bare `except:` clauses
/// - `eval()` / `exec()` usage
///
/// **Advisory only** — always returns Allow with context warnings. Never blocks.
pub struct SiacPreValidator;

impl Handler for SiacPreValidator {
    fn name(&self) -> &str {
        "siac_pre_validator"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PreToolUse]
    }

    fn tool_matcher(&self) -> Option<&str> {
        Some("Write|Edit")
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        // Only validate .py files
        let file_path = ctx.tool_file_path();

        if !file_path.ends_with(".py") {
            return HandlerResult::skip(self.name());
        }

        // Extract content to validate
        let content = ctx
            .tool_input
            .get("content") // Write tool
            .or_else(|| ctx.tool_input.get("new_string")) // Edit tool
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if content.is_empty() {
            return HandlerResult::skip(self.name());
        }

        let mut warnings: Vec<String> = Vec::new();

        if RE_MUTABLE_DEFAULT.is_match(content) {
            warnings.push(
                "Mutable default argument detected (def f(x=[])). \
                 Use None as default and initialize inside the function."
                    .to_string(),
            );
        }

        if RE_BARE_EXCEPT.is_match(content) {
            warnings.push(
                "Bare `except:` detected. \
                 Specify exception type (e.g., `except ValueError:`)."
                    .to_string(),
            );
        }

        if RE_EVAL_EXEC.is_match(content) {
            warnings.push(
                "eval()/exec() detected. \
                 Avoid dynamic code execution \u{2014} use ast.literal_eval() or safer alternatives."
                    .to_string(),
            );
        }

        if warnings.is_empty() {
            HandlerResult::skip(self.name())
        } else {
            HandlerResult::allow(
                self.name(),
                Some(format!("SIAC-PRE: {}", warnings.join(" | "))),
            )
        }
    }
}

// ══════════════════════════════════════════════════════════════════════
// 5. AcoRollbackEnforcer (S1)
// ══════════════════════════════════════════════════════════════════════

/// Regex: ACO-related keywords (case-insensitive).
static RE_ACO_KEYWORDS: Lazy<Regex> = Lazy::new(|| {
    regex::RegexBuilder::new(r"\baco\b|\bgenerator\b|\borchestrat")
        .case_insensitive(true)
        .build()
        .expect("valid regex literal")
});

/// Regex: rollback/recovery-related keywords (case-insensitive).
static RE_ROLLBACK_KEYWORDS: Lazy<Regex> = Lazy::new(|| {
    regex::RegexBuilder::new(
        r"\brollback\b|\brevert\b|\bundo\b|\brecovery\b|\bfallback\b|\bcheckpoint\b",
    )
    .case_insensitive(true)
    .build()
    .expect("valid regex literal")
});

/// ACO Rollback Enforcer — advisory reminder for rollback strategy.
///
/// Ported from `aco_rollback_enforcer.py`. When a Task prompt is ACO-related
/// but doesn't mention rollback/recovery, injects a Portuguese reminder.
///
/// **Advisory only** — always returns Allow. Never blocks.
pub struct AcoRollbackEnforcer;

impl Handler for AcoRollbackEnforcer {
    fn name(&self) -> &str {
        "aco_rollback_enforcer"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PreToolUse]
    }

    fn tool_matcher(&self) -> Option<&str> {
        Some("Task")
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let prompt = match ctx.tool_input.get("prompt").and_then(|v| v.as_str()) {
            Some(p) if !p.is_empty() => p,
            _ => return HandlerResult::skip(self.name()),
        };

        // Check if ACO-related
        if !RE_ACO_KEYWORDS.is_match(prompt) {
            return HandlerResult::skip(self.name());
        }

        // Check if rollback/recovery is mentioned
        if RE_ROLLBACK_KEYWORDS.is_match(prompt) {
            return HandlerResult::skip(self.name());
        }

        // ACO task without rollback mention — inject reminder
        HandlerResult::allow(
            self.name(),
            Some(
                "ACO-ROLLBACK: Tarefa ACO detectada sem men\u{00e7}\u{00e3}o a rollback/recovery. \
                 Lembrete: toda opera\u{00e7}\u{00e3}o ACO deve incluir estrat\u{00e9}gia de rollback \
                 (checkpoint antes, validate depois, rollback se falhar). \
                 Tr\u{00ed}ade: execute_script + validate_script + rollback_script."
                    .to_string(),
            ),
        )
    }
}

// ══════════════════════════════════════════════════════════════════════
// 6. UserPromptValidator (S1)
// ══════════════════════════════════════════════════════════════════════

/// Regex: CPF pattern (XXX.XXX.XXX-XX).
static RE_CPF: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\d{3}\.\d{3}\.\d{3}-\d{2}").expect("valid regex literal"));

/// Regex: CNPJ pattern (XX.XXX.XXX/XXXX-XX).
static RE_CNPJ: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\d{2}\.\d{3}\.\d{3}/\d{4}-\d{2}").expect("valid regex literal"));

/// Maximum prompt length before warning.
const MAX_PROMPT_LENGTH: usize = 50000;

/// User Prompt Validator — advisory checks on user prompts.
///
/// Ported from `user_prompt_validator.py`. Checks for:
/// - Excessively long prompts (> 50k chars)
/// - PII patterns (CPF, CNPJ)
///
/// **Advisory only** — always returns Allow. Never blocks.
pub struct UserPromptValidator;

impl Handler for UserPromptValidator {
    fn name(&self) -> &str {
        "user_prompt_validator"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::UserPromptSubmit]
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        // Claude Code sends "userMessage" for UserPromptSubmit; fall back to "prompt"
        let prompt = match ctx
            .input
            .get("userMessage")
            .or_else(|| ctx.input.get("prompt"))
            .and_then(|v| v.as_str())
        {
            Some(p) if !p.is_empty() => p,
            _ => return HandlerResult::skip(self.name()),
        };

        let mut warnings: Vec<String> = Vec::new();

        if prompt.len() > MAX_PROMPT_LENGTH {
            warnings.push(format!(
                "Prompt muito longo ({} chars > {} limite). \
                 Considere dividir em partes menores para melhor processamento.",
                prompt.len(),
                MAX_PROMPT_LENGTH
            ));
        }

        if RE_CPF.is_match(prompt) {
            warnings.push(
                "PII detectado: padr\u{00e3}o de CPF encontrado no prompt. \
                 Remova dados pessoais sens\u{00ed}veis antes de prosseguir."
                    .to_string(),
            );
        }

        if RE_CNPJ.is_match(prompt) {
            warnings.push(
                "PII detectado: padr\u{00e3}o de CNPJ encontrado no prompt. \
                 Verifique se \u{00e9} necess\u{00e1}rio incluir este dado."
                    .to_string(),
            );
        }

        if warnings.is_empty() {
            HandlerResult::skip(self.name())
        } else {
            HandlerResult::allow(
                self.name(),
                Some(format!("PROMPT-VALIDATOR: {}", warnings.join(" | "))),
            )
        }
    }
}

// ══════════════════════════════════════════════════════════════════════
// 7. SiacHookEntry (S1)
// ══════════════════════════════════════════════════════════════════════

/// SIAC Hook Entry — lightweight PostToolUse recorder for Write/Edit on .py files.
///
/// Ported from `siac_hook_entry.py`. The full SIAC motors (Motor1-4) are
/// Python-only; this handler records the edit event in the knowledge DB
/// for later analysis.
///
/// **Passive** — always returns Skip after recording.
pub struct SiacHookEntry;

impl Handler for SiacHookEntry {
    fn name(&self) -> &str {
        "siac_hook_entry"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PostToolUse]
    }

    fn tool_matcher(&self) -> Option<&str> {
        Some("Write|Edit")
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let file_path = ctx.tool_file_path();

        if !file_path.ends_with(".py") {
            return HandlerResult::skip(self.name());
        }

        let edit_type = ctx.tool_name.as_deref().unwrap_or("unknown").to_lowercase();

        // Record the edit event in the knowledge DB (fail-open)
        let _ = ctx
            .knowledge
            .record_edit(file_path, &edit_type, Some("siac_hook_entry recorded"));

        HandlerResult::skip(self.name())
    }
}

// ══════════════════════════════════════════════════════════════════════
// 8. SiacOrchestrator (S1)
// ══════════════════════════════════════════════════════════════════════

/// SIAC Orchestrator — placeholder for full 4-motor SIAC orchestration.
///
/// Ported from `siac_orchestrator.py`. The full orchestration (Motor1: syntax,
/// Motor2: complexity, Motor3: patterns, Motor4: security) requires Python AST
/// analysis via a Tier 3 Python bridge — not yet available in Rust.
///
/// For now: records the event and returns Skip.
///
/// NOTE(S6): Implement full motor orchestration via Tier 3 Python bridge.
pub struct SiacOrchestrator;

impl Handler for SiacOrchestrator {
    fn name(&self) -> &str {
        "siac_orchestrator"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PostToolUse]
    }

    fn tool_matcher(&self) -> Option<&str> {
        Some("Write|Edit")
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let file_path = ctx.tool_file_path();

        if !file_path.ends_with(".py") {
            return HandlerResult::skip(self.name());
        }

        let edit_type = ctx.tool_name.as_deref().unwrap_or("unknown").to_lowercase();

        // Record the edit event (fail-open)
        // NOTE(S6): Full 4-motor orchestration requires Tier 3 Python bridge.
        let _ =
            ctx.knowledge
                .record_edit(file_path, &edit_type, Some("siac_orchestrator placeholder"));

        HandlerResult::skip(self.name())
    }
}

// ══════════════════════════════════════════════════════════════════════
// S1 Tests
// ══════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests_s1 {
    use super::*;
    use crate::context::CortexContext;
    use crate::types::Decision;
    use std::sync::Arc;
    use tempfile::TempDir;
    use touring_hooks::knowledge::FileKnowledgeDB;

    #[allow(clippy::arc_with_non_send_sync)] // single-threaded test context
    fn make_test_ctx(event: HookEvent, input: serde_json::Value) -> (TempDir, CortexContext) {
        let tmp = TempDir::new().unwrap();
        let db = FileKnowledgeDB::new(&tmp.path().join("test.db")).unwrap();
        let knowledge = Arc::new(db);
        let ctx = CortexContext::from_input(event, input, knowledge, tmp.path().to_path_buf());
        (tmp, ctx)
    }

    // ── CodeFirstPipelineValidator ──────────────────────────────────

    #[test]
    fn test_cfpv_skip_non_antt() {
        let input = serde_json::json!({
            "tool_name": "Task",
            "tool_input": {"prompt": "calcule o VPL da concessao"}
        });
        let (_tmp, mut ctx) = make_test_ctx(HookEvent::PreToolUse, input);
        let handler = CodeFirstPipelineValidator;
        let result = handler.execute(&mut ctx);
        assert!(matches!(result.decision, Decision::Skip));
    }

    #[test]
    fn test_cfpv_antt_skill_no_process() {
        let input = serde_json::json!({
            "tool_name": "Task",
            "tool_input": {"prompt": "use antt-preliminary-analyzer to check the document"}
        });
        let (_tmp, mut ctx) = make_test_ctx(HookEvent::PreToolUse, input);
        let handler = CodeFirstPipelineValidator;
        let result = handler.execute(&mut ctx);
        // Should allow with warning (no process number)
        assert!(matches!(result.decision, Decision::Allow));
        assert!(!result.context_lines.is_empty());
        assert!(result.context_lines[0].contains("no process number"));
    }

    #[test]
    fn test_cfpv_antt_skill_with_process_no_pipeline() {
        let input = serde_json::json!({
            "tool_name": "Task",
            "tool_input": {
                "prompt": "run antt-vote-architect for process 50500.123456-2024-01"
            }
        });
        let (_tmp, mut ctx) = make_test_ctx(HookEvent::PreToolUse, input);
        let handler = CodeFirstPipelineValidator;
        let result = handler.execute(&mut ctx);
        // Should block (no pipeline_state found)
        assert!(matches!(result.decision, Decision::Block(_)));
    }

    #[test]
    fn test_cfpv_antt_skill_with_pipeline_state() {
        let input = serde_json::json!({
            "tool_name": "Task",
            "tool_input": {
                "prompt": "run antt-preliminary-analyzer for 50500.654321-2025-02"
            }
        });
        let (tmp, mut ctx) = make_test_ctx(HookEvent::PreToolUse, input);

        // Create pipeline_state file in relatoria
        let relatoria = tmp.path().join("analise").join("relatoria");
        std::fs::create_dir_all(&relatoria).unwrap();
        let state = serde_json::json!({
            "phases": {
                "F1": {"quality_score": 0.85},
                "F2": {"quality_score": 0.90}
            }
        });
        std::fs::write(
            relatoria.join("pipeline_state_test.json"),
            serde_json::to_string(&state).unwrap(),
        )
        .unwrap();

        let handler = CodeFirstPipelineValidator;
        let result = handler.execute(&mut ctx);
        // Should skip (pipeline found and quality OK)
        assert!(matches!(result.decision, Decision::Skip));
    }

    #[test]
    fn test_cfpv_empty_prompt() {
        let input = serde_json::json!({
            "tool_name": "Task",
            "tool_input": {"prompt": ""}
        });
        let (_tmp, mut ctx) = make_test_ctx(HookEvent::PreToolUse, input);
        let handler = CodeFirstPipelineValidator;
        let result = handler.execute(&mut ctx);
        assert!(matches!(result.decision, Decision::Skip));
    }

    #[test]
    fn test_cfpv_low_quality_score_blocks() {
        let input = serde_json::json!({
            "tool_name": "Task",
            "tool_input": {
                "prompt": "run antt-risk-assessor for 50505.111111-2025-03"
            }
        });
        let (tmp, mut ctx) = make_test_ctx(HookEvent::PreToolUse, input);

        // Create pipeline_state with low quality
        let relatoria = tmp.path().join("analise").join("relatoria");
        std::fs::create_dir_all(&relatoria).unwrap();
        let state = serde_json::json!({
            "phases": {
                "F1": {"quality_score": 0.3}
            }
        });
        std::fs::write(
            relatoria.join("pipeline_state_low.json"),
            serde_json::to_string(&state).unwrap(),
        )
        .unwrap();

        let handler = CodeFirstPipelineValidator;
        let result = handler.execute(&mut ctx);
        assert!(matches!(result.decision, Decision::Block(_)));
        if let Decision::Block(reason) = &result.decision {
            assert!(reason.contains("quality_score"));
        }
    }

    // ── SiacPreValidator ────────────────────────────────────────────

    #[test]
    fn test_siac_pre_skip_non_py() {
        let input = serde_json::json!({
            "tool_name": "Write",
            "tool_input": {
                "file_path": "/tmp/test.rs",
                "content": "fn main() { eval(something); }"
            }
        });
        let (_tmp, mut ctx) = make_test_ctx(HookEvent::PreToolUse, input);
        let handler = SiacPreValidator;
        let result = handler.execute(&mut ctx);
        assert!(matches!(result.decision, Decision::Skip));
    }

    #[test]
    fn test_siac_pre_detects_mutable_default() {
        let input = serde_json::json!({
            "tool_name": "Write",
            "tool_input": {
                "file_path": "/tmp/test.py",
                "content": "def process(items=[]):\n    return items"
            }
        });
        let (_tmp, mut ctx) = make_test_ctx(HookEvent::PreToolUse, input);
        let handler = SiacPreValidator;
        let result = handler.execute(&mut ctx);
        assert!(matches!(result.decision, Decision::Allow));
        assert!(result.context_lines[0].contains("Mutable default"));
    }

    #[test]
    fn test_siac_pre_detects_bare_except() {
        let input = serde_json::json!({
            "tool_name": "Edit",
            "tool_input": {
                "file_path": "/project/module.py",
                "new_string": "try:\n    x()\nexcept:\n    pass"
            }
        });
        let (_tmp, mut ctx) = make_test_ctx(HookEvent::PreToolUse, input);
        let handler = SiacPreValidator;
        let result = handler.execute(&mut ctx);
        assert!(matches!(result.decision, Decision::Allow));
        assert!(result.context_lines[0].contains("Bare `except:`"));
    }

    #[test]
    fn test_siac_pre_detects_eval() {
        let input = serde_json::json!({
            "tool_name": "Write",
            "tool_input": {
                "file_path": "/project/danger.py",
                "content": "result = eval(user_input)"
            }
        });
        let (_tmp, mut ctx) = make_test_ctx(HookEvent::PreToolUse, input);
        let handler = SiacPreValidator;
        let result = handler.execute(&mut ctx);
        assert!(matches!(result.decision, Decision::Allow));
        assert!(result.context_lines[0].contains("eval()"));
    }

    #[test]
    fn test_siac_pre_clean_code_skips() {
        let input = serde_json::json!({
            "tool_name": "Write",
            "tool_input": {
                "file_path": "/project/clean.py",
                "content": "def process(items=None):\n    if items is None:\n        items = []\n    return items"
            }
        });
        let (_tmp, mut ctx) = make_test_ctx(HookEvent::PreToolUse, input);
        let handler = SiacPreValidator;
        let result = handler.execute(&mut ctx);
        assert!(matches!(result.decision, Decision::Skip));
    }

    #[test]
    fn test_siac_pre_multiple_warnings() {
        let input = serde_json::json!({
            "tool_name": "Write",
            "tool_input": {
                "file_path": "/project/bad.py",
                "content": "def f(x=[]):\n    try:\n        eval(x)\n    except:\n        pass"
            }
        });
        let (_tmp, mut ctx) = make_test_ctx(HookEvent::PreToolUse, input);
        let handler = SiacPreValidator;
        let result = handler.execute(&mut ctx);
        assert!(matches!(result.decision, Decision::Allow));
        let ctx_line = &result.context_lines[0];
        assert!(ctx_line.contains("Mutable default"));
        assert!(ctx_line.contains("Bare `except:`"));
        assert!(ctx_line.contains("eval()"));
    }

    // ── AcoRollbackEnforcer ─────────────────────────────────────────

    #[test]
    fn test_aco_rollback_skip_non_aco() {
        let input = serde_json::json!({
            "tool_name": "Task",
            "tool_input": {"prompt": "create a new endpoint for the API"}
        });
        let (_tmp, mut ctx) = make_test_ctx(HookEvent::PreToolUse, input);
        let handler = AcoRollbackEnforcer;
        let result = handler.execute(&mut ctx);
        assert!(matches!(result.decision, Decision::Skip));
    }

    #[test]
    fn test_aco_rollback_warns_no_rollback() {
        let input = serde_json::json!({
            "tool_name": "Task",
            "tool_input": {"prompt": "run the ACO generator to produce new scripts"}
        });
        let (_tmp, mut ctx) = make_test_ctx(HookEvent::PreToolUse, input);
        let handler = AcoRollbackEnforcer;
        let result = handler.execute(&mut ctx);
        assert!(matches!(result.decision, Decision::Allow));
        assert!(result.context_lines[0].contains("ACO-ROLLBACK"));
    }

    #[test]
    fn test_aco_rollback_skip_with_rollback() {
        let input = serde_json::json!({
            "tool_name": "Task",
            "tool_input": {"prompt": "run ACO orchestrator with checkpoint and rollback strategy"}
        });
        let (_tmp, mut ctx) = make_test_ctx(HookEvent::PreToolUse, input);
        let handler = AcoRollbackEnforcer;
        let result = handler.execute(&mut ctx);
        assert!(matches!(result.decision, Decision::Skip));
    }

    #[test]
    fn test_aco_rollback_empty_prompt() {
        let input = serde_json::json!({
            "tool_name": "Task",
            "tool_input": {"prompt": ""}
        });
        let (_tmp, mut ctx) = make_test_ctx(HookEvent::PreToolUse, input);
        let handler = AcoRollbackEnforcer;
        let result = handler.execute(&mut ctx);
        assert!(matches!(result.decision, Decision::Skip));
    }

    #[test]
    fn test_aco_rollback_case_insensitive() {
        let input = serde_json::json!({
            "tool_name": "Task",
            "tool_input": {"prompt": "Execute the Orchestrator pipeline for new data"}
        });
        let (_tmp, mut ctx) = make_test_ctx(HookEvent::PreToolUse, input);
        let handler = AcoRollbackEnforcer;
        let result = handler.execute(&mut ctx);
        // "Orchestrator" should match case-insensitively
        assert!(matches!(result.decision, Decision::Allow));
        assert!(result.context_lines[0].contains("ACO-ROLLBACK"));
    }

    // ── UserPromptValidator ─────────────────────────────────────────

    #[test]
    fn test_upv_skip_normal_prompt() {
        let input = serde_json::json!({
            "prompt": "analise o contrato de concessao"
        });
        let (_tmp, mut ctx) = make_test_ctx(HookEvent::UserPromptSubmit, input);
        let handler = UserPromptValidator;
        let result = handler.execute(&mut ctx);
        assert!(matches!(result.decision, Decision::Skip));
    }

    #[test]
    fn test_upv_warns_cpf() {
        let input = serde_json::json!({
            "prompt": "busque informacoes sobre o CPF 123.456.789-01 no sistema"
        });
        let (_tmp, mut ctx) = make_test_ctx(HookEvent::UserPromptSubmit, input);
        let handler = UserPromptValidator;
        let result = handler.execute(&mut ctx);
        assert!(matches!(result.decision, Decision::Allow));
        assert!(result.context_lines[0].contains("CPF"));
    }

    #[test]
    fn test_upv_warns_cnpj() {
        let input = serde_json::json!({
            "prompt": "verifique o CNPJ 12.345.678/0001-99 da concessionaria"
        });
        let (_tmp, mut ctx) = make_test_ctx(HookEvent::UserPromptSubmit, input);
        let handler = UserPromptValidator;
        let result = handler.execute(&mut ctx);
        assert!(matches!(result.decision, Decision::Allow));
        assert!(result.context_lines[0].contains("CNPJ"));
    }

    #[test]
    fn test_upv_warns_long_prompt() {
        let long_prompt = "a".repeat(60000);
        let input = serde_json::json!({
            "prompt": long_prompt
        });
        let (_tmp, mut ctx) = make_test_ctx(HookEvent::UserPromptSubmit, input);
        let handler = UserPromptValidator;
        let result = handler.execute(&mut ctx);
        assert!(matches!(result.decision, Decision::Allow));
        assert!(result.context_lines[0].contains("muito longo"));
    }

    #[test]
    fn test_upv_empty_prompt() {
        let input = serde_json::json!({
            "prompt": ""
        });
        let (_tmp, mut ctx) = make_test_ctx(HookEvent::UserPromptSubmit, input);
        let handler = UserPromptValidator;
        let result = handler.execute(&mut ctx);
        assert!(matches!(result.decision, Decision::Skip));
    }

    #[test]
    fn test_upv_multiple_pii() {
        let input = serde_json::json!({
            "prompt": "CPF 111.222.333-44 e CNPJ 55.666.777/8888-99"
        });
        let (_tmp, mut ctx) = make_test_ctx(HookEvent::UserPromptSubmit, input);
        let handler = UserPromptValidator;
        let result = handler.execute(&mut ctx);
        assert!(matches!(result.decision, Decision::Allow));
        let ctx_line = &result.context_lines[0];
        assert!(ctx_line.contains("CPF"));
        assert!(ctx_line.contains("CNPJ"));
    }

    // ── SiacHookEntry ───────────────────────────────────────────────

    #[test]
    fn test_siac_entry_skip_non_py() {
        let input = serde_json::json!({
            "tool_name": "Write",
            "tool_input": {
                "file_path": "/tmp/test.rs",
                "content": "fn main() {}"
            }
        });
        let (_tmp, mut ctx) = make_test_ctx(HookEvent::PostToolUse, input);
        let handler = SiacHookEntry;
        let result = handler.execute(&mut ctx);
        assert!(matches!(result.decision, Decision::Skip));
    }

    #[test]
    fn test_siac_entry_records_py_write() {
        let input = serde_json::json!({
            "tool_name": "Write",
            "tool_input": {
                "file_path": "/project/module.py",
                "content": "print('hello')"
            }
        });
        let (tmp, mut ctx) = make_test_ctx(HookEvent::PostToolUse, input);
        let handler = SiacHookEntry;
        let result = handler.execute(&mut ctx);
        assert!(matches!(result.decision, Decision::Skip));

        // Verify the edit was recorded in the knowledge DB
        let db = FileKnowledgeDB::new(&tmp.path().join("test.db")).unwrap();
        let edits = db.recent_edits("/project/module.py", 5).unwrap();
        assert!(!edits.is_empty());
        assert_eq!(edits[0].edit_type, "write");
    }

    #[test]
    fn test_siac_entry_records_py_edit() {
        let input = serde_json::json!({
            "tool_name": "Edit",
            "tool_input": {
                "file_path": "/project/module.py",
                "new_string": "fixed code"
            }
        });
        let (tmp, mut ctx) = make_test_ctx(HookEvent::PostToolUse, input);
        let handler = SiacHookEntry;
        let result = handler.execute(&mut ctx);
        assert!(matches!(result.decision, Decision::Skip));

        let db = FileKnowledgeDB::new(&tmp.path().join("test.db")).unwrap();
        let edits = db.recent_edits("/project/module.py", 5).unwrap();
        assert!(!edits.is_empty());
        assert_eq!(edits[0].edit_type, "edit");
    }

    // ── SiacOrchestrator ────────────────────────────────────────────

    #[test]
    fn test_siac_orch_skip_non_py() {
        let input = serde_json::json!({
            "tool_name": "Write",
            "tool_input": {
                "file_path": "/tmp/test.js",
                "content": "console.log('hi')"
            }
        });
        let (_tmp, mut ctx) = make_test_ctx(HookEvent::PostToolUse, input);
        let handler = SiacOrchestrator;
        let result = handler.execute(&mut ctx);
        assert!(matches!(result.decision, Decision::Skip));
    }

    #[test]
    fn test_siac_orch_records_py() {
        let input = serde_json::json!({
            "tool_name": "Write",
            "tool_input": {
                "file_path": "/project/script.py",
                "content": "import os"
            }
        });
        let (tmp, mut ctx) = make_test_ctx(HookEvent::PostToolUse, input);
        let handler = SiacOrchestrator;
        let result = handler.execute(&mut ctx);
        assert!(matches!(result.decision, Decision::Skip));

        // Verify recording
        let db = FileKnowledgeDB::new(&tmp.path().join("test.db")).unwrap();
        let edits = db.recent_edits("/project/script.py", 5).unwrap();
        assert!(!edits.is_empty());
    }

    // ── Process number regex ────────────────────────────────────────

    #[test]
    fn test_process_number_regex() {
        assert!(RE_PROCESS_NUMBER.is_match("50500.123456-2024-01"));
        assert!(RE_PROCESS_NUMBER.is_match("50505.654321-2025-12"));
        assert!(!RE_PROCESS_NUMBER.is_match("50501.123456-2024-01"));
        assert!(!RE_PROCESS_NUMBER.is_match("random text"));
    }

    // ── ANTT skill detection ────────────────────────────────────────

    #[test]
    fn test_antt_skill_detection() {
        let prompt = "use antt-vote-architect to draft";
        let prompt_lower = prompt.to_lowercase();
        assert!(ANTT_SKILLS.iter().any(|s| prompt_lower.contains(s)));

        let prompt2 = "just a normal task";
        let prompt2_lower = prompt2.to_lowercase();
        assert!(!ANTT_SKILLS.iter().any(|s| prompt2_lower.contains(s)));
    }
}
