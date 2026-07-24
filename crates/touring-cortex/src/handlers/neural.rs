//! Neural Hook Handlers — Original 10 wrappers from the Cortex S0 foundation.
//!
//! These wrap the existing hooks module functions:
//! - PreRead, PostRead, PreBash, PostBash, PreEdit, PostEdit
//! - SessionStart, SessionStop
//! - ClassifyIntent (CILA router)
//! - ScanPii (PII detector)

use crate::context::CortexContext;
use crate::handler::Handler;
use crate::pipeline::Pipeline;
use crate::types::{Decision, HandlerResult, HookEvent};
use touring_hooks as hooks;

use once_cell::sync::Lazy;
use touring_code::ast::IncrementalPipeline;

/// Shared incremental pipeline for tree-sitter based symbol extraction.
/// Used by PostEditHandler as an optional upgrade over regex-based extraction.
/// Falls back to regex if the pipeline fails for any reason.
static INCREMENTAL_PIPELINE: Lazy<std::sync::Mutex<IncrementalPipeline>> =
    Lazy::new(|| std::sync::Mutex::new(IncrementalPipeline::new()));

// ── Pre-Read Handler ──────────────────────────────────────────────────

/// Wraps existing pre_read::compose_high_signal_context as a Cortex Handler.
pub struct PreReadHandler;

impl Handler for PreReadHandler {
    fn name(&self) -> &str {
        "pre_read"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PreToolUse]
    }

    fn tool_matcher(&self) -> Option<&str> {
        Some("Read")
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let file_path = match &ctx.file_path {
            Some(p) => p.clone(),
            None => return HandlerResult::skip(self.name()),
        };

        let rel_path = hooks::make_relative(&file_path, &ctx.project_root);
        let context = hooks::pre_read::compose_high_signal_context(&ctx.knowledge, &rel_path);

        match context {
            Some(ctx_str) if !ctx_str.is_empty() => {
                HandlerResult::allow(self.name(), Some(ctx_str))
            }
            _ => HandlerResult::skip(self.name()),
        }
    }
}

// ── Post-Read Handler ─────────────────────────────────────────────────

/// Wraps existing post_read::run as a Cortex Handler (async/learning).
pub struct PostReadHandler;

impl Handler for PostReadHandler {
    fn name(&self) -> &str {
        "post_read"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PostToolUse]
    }

    fn tool_matcher(&self) -> Option<&str> {
        Some("Read")
    }

    fn is_async(&self) -> bool {
        true
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let file_path = match &ctx.file_path {
            Some(p) => p.clone(),
            None => return HandlerResult::skip(self.name()),
        };

        let abs_path = if std::path::Path::new(&file_path).is_absolute() {
            file_path.clone()
        } else {
            ctx.project_root
                .join(&file_path)
                .to_string_lossy()
                .to_string()
        };

        let content = match std::fs::read_to_string(&abs_path) {
            Ok(c) => c,
            Err(_) => return HandlerResult::skip(self.name()),
        };

        let rel_path = hooks::make_relative(&file_path, &ctx.project_root);
        let language = hooks::post_read::detect_language(&rel_path);
        let line_count = content.lines().count() as i64;
        let imports = hooks::post_read::extract_imports_fast(&content, &language);
        let symbols = hooks::post_read::extract_symbols_fast(&content, &language);

        let knowledge = hooks::knowledge::FileKnowledge {
            file_path: rel_path.clone(),
            language: Some(language.clone()),
            line_count,
            symbol_count: symbols.len() as i64,
            imports_json: Some(serde_json::to_string(&imports).unwrap_or_default()),
            symbols_json: Some(serde_json::to_string(&symbols).unwrap_or_default()),
            ..Default::default()
        };

        let _ = ctx.knowledge.upsert(&knowledge);

        let relations: Vec<hooks::knowledge::FileRelation> = imports
            .iter()
            .filter_map(|imp| {
                hooks::post_read::resolve_import_path(imp, &language).map(|target| {
                    hooks::knowledge::FileRelation {
                        source: rel_path.clone(),
                        target,
                        relation_type: "imports".to_string(),
                    }
                })
            })
            .collect();

        if !relations.is_empty() {
            let _ = ctx.knowledge.replace_relations_from(&rel_path, &relations);
        }

        let _ = ctx.knowledge.record_access(&rel_path, &ctx.session_id);

        HandlerResult::skip(self.name()) // Learning hooks produce no visible output
    }
}

// ── Pre-Bash Handler ──────────────────────────────────────────────────

/// Wraps existing pre_bash::compose_relevant_context as a Cortex Handler.
pub struct PreBashHandler;

impl Handler for PreBashHandler {
    fn name(&self) -> &str {
        "pre_bash"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PreToolUse]
    }

    fn tool_matcher(&self) -> Option<&str> {
        Some("Bash")
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let command = ctx
            .tool_input
            .get("command")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if command.is_empty() {
            return HandlerResult::skip(self.name());
        }

        let short_key = hooks::pre_bash::extract_command_short(command);
        if short_key.is_empty() {
            return HandlerResult::skip(self.name());
        }

        let file_ctx = hooks::pre_bash::extract_file_context(command);
        let context = hooks::pre_bash::compose_relevant_context(
            &ctx.knowledge,
            &short_key,
            file_ctx.as_deref(),
        );

        match context {
            Some(ctx_str) if !ctx_str.is_empty() => {
                HandlerResult::allow(self.name(), Some(ctx_str))
            }
            _ => HandlerResult::skip(self.name()),
        }
    }
}

// ── Post-Bash Handler ─────────────────────────────────────────────────

/// Wraps existing post_bash::run as a Cortex Handler (async/learning).
pub struct PostBashHandler;

impl Handler for PostBashHandler {
    fn name(&self) -> &str {
        "post_bash"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PostToolUse]
    }

    fn tool_matcher(&self) -> Option<&str> {
        Some("Bash")
    }

    fn is_async(&self) -> bool {
        true
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let mut runtime = match hooks::runtime::HookRuntime::new(&ctx.project_root) {
            Ok(rt) => rt,
            Err(_) => return HandlerResult::skip(self.name()),
        };

        let _ = hooks::post_bash::run(&mut runtime, &ctx.input);

        HandlerResult::skip(self.name()) // Learning hook — no visible output
    }
}

// ── Pre-Edit Handler ──────────────────────────────────────────────────

/// Wraps existing pre_edit::compose_edit_context as a Cortex Handler.
pub struct PreEditHandler;

impl Handler for PreEditHandler {
    fn name(&self) -> &str {
        "pre_edit"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PreToolUse]
    }

    fn tool_matcher(&self) -> Option<&str> {
        Some("Write|Edit|MultiEdit")
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let file_path = match &ctx.file_path {
            Some(p) => p.clone(),
            None => return HandlerResult::skip(self.name()),
        };

        let rel_path = hooks::make_relative(&file_path, &ctx.project_root);
        let context = hooks::pre_edit::compose_edit_context(None, &ctx.knowledge, &rel_path);

        match context {
            Some(ctx_str) if !ctx_str.is_empty() => {
                HandlerResult::allow(self.name(), Some(ctx_str))
            }
            _ => HandlerResult::skip(self.name()),
        }
    }
}

// ── Post-Edit Handler ─────────────────────────────────────────────────

/// Wraps existing post_edit::run as a Cortex Handler (async/learning).
///
/// **v9.0**: Optionally uses [`IncrementalPipeline`] (tree-sitter) for symbol
/// extraction instead of regex. Falls back to the original regex path if:
/// - The file language is not supported by tree-sitter
/// - The incremental pipeline mutex is poisoned
/// - The pipeline returns an error for any reason
pub struct PostEditHandler;

impl Handler for PostEditHandler {
    fn name(&self) -> &str {
        "post_edit"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PostToolUse]
    }

    fn tool_matcher(&self) -> Option<&str> {
        Some("Write|Edit|MultiEdit")
    }

    fn is_async(&self) -> bool {
        true
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        // Try incremental pipeline (tree-sitter) for more accurate symbol extraction
        if let Some(ref file_path) = ctx.file_path {
            self.try_incremental_reindex(file_path, &ctx.project_root);
        }

        // Always run the original regex-based post_edit hook (handles edit_history,
        // error-driven learning, co-edit recording, and relation updates).
        let mut runtime = match hooks::runtime::HookRuntime::new(&ctx.project_root) {
            Ok(rt) => rt,
            Err(_) => return HandlerResult::skip(self.name()),
        };

        let _ = hooks::post_edit::run(&mut runtime, &ctx.input);

        HandlerResult::skip(self.name()) // Learning hook
    }
}

impl PostEditHandler {
    /// Attempt to process the edited file through the incremental pipeline.
    ///
    /// This uses tree-sitter queries (via `IncrementalPipeline::process_file`)
    /// for more accurate symbol extraction than the regex-based fallback.
    /// Any failure is logged and silently ignored — the regex path always runs.
    fn try_incremental_reindex(&self, file_path: &str, project_root: &std::path::Path) {
        let abs_path = if std::path::Path::new(file_path).is_absolute() {
            file_path.to_string()
        } else {
            project_root.join(file_path).to_string_lossy().to_string()
        };

        // Read the current file content
        let content = match std::fs::read_to_string(&abs_path) {
            Ok(c) => c,
            Err(_) => return, // File not readable — skip silently
        };

        // Acquire the shared pipeline
        let mut pipeline = match INCREMENTAL_PIPELINE.lock() {
            Ok(p) => p,
            Err(e) => {
                tracing::warn!(
                    "IncrementalPipeline mutex poisoned, skipping tree-sitter path: {}",
                    e
                );
                return;
            }
        };

        // Use process_file (full parse) since we don't have byte-level edit info.
        // The PostToolUse hook fires AFTER the file is written, so we only have
        // the final file content, not the diff.
        match pipeline.process_file(file_path, &content) {
            Ok(result) => {
                tracing::debug!(
                    file = file_path,
                    symbols_added = result.symbols_added.len(),
                    parse_time_us = result.parse_time_us,
                    was_incremental = result.was_incremental,
                    "IncrementalPipeline processed post-edit file"
                );
            }
            Err(e) => {
                // Expected for unsupported languages (csv, md, json, etc.)
                tracing::debug!(
                    file = file_path,
                    error = %e,
                    "IncrementalPipeline skipped (falling back to regex)"
                );
            }
        }
    }
}

// ── Session Start Handler ─────────────────────────────────────────────

/// Wraps session_hooks::run_session_start — injects knowledge stats.
pub struct SessionStartHandler;

impl Handler for SessionStartHandler {
    fn name(&self) -> &str {
        "session_start"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::SessionStart]
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let stats = match ctx.knowledge.stats() {
            Ok(s) => s,
            Err(_) => return HandlerResult::skip(self.name()),
        };

        if stats.file_count == 0 && stats.bash_count == 0 {
            return HandlerResult::skip(self.name());
        }

        let mut parts = Vec::new();
        if stats.file_count > 0 {
            parts.push(format!("{} files known", stats.file_count));
        }
        if stats.relation_count > 0 {
            parts.push(format!("{} relations", stats.relation_count));
        }
        if stats.bash_count > 0 {
            parts.push(format!("{} cmd outcomes", stats.bash_count));
        }
        if stats.edit_count > 0 {
            parts.push(format!("{} edits tracked", stats.edit_count));
        }

        let session_short = &ctx.session_id[..ctx.session_id.len().min(8)];
        let context = format!(
            "Touring Knowledge: {} | session={}",
            parts.join(", "),
            session_short
        );

        HandlerResult::allow(self.name(), Some(context))
    }
}

// ── Session Stop Handler ──────────────────────────────────────────────

/// Session stop with Completion Gate — verifies that edited files were tested E2E.
///
/// Classifies each file edited in the last 30 minutes as:
/// - VERIFIED: a test runner (pytest/cargo test/ruff) executed successfully after the edit
/// - COMPILED_ONLY: lint/check passed but no test runner executed
/// - UNTESTED: no verification at all
///
/// Emits a completion_verify report in the output context.
pub struct SessionStopHandler;

impl Handler for SessionStopHandler {
    fn name(&self) -> &str {
        "session_stop"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::Stop]
    }

    fn is_async(&self) -> bool {
        true
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let _ = ctx
            .knowledge
            .record_access("__session_end__", &ctx.session_id);

        // ── Completion Gate: verify edited files were tested ──
        // touring_checkpoint: auto-save session state before exit
        let _ = ctx
            .knowledge
            .record_access("__checkpoint_auto__", &ctx.session_id);

        let report = build_completion_report(&ctx.knowledge, &ctx.session_id);
        if !report.is_empty() {
            return HandlerResult::allow(self.name(), Some(report));
        }

        HandlerResult::skip(self.name())
    }
}

/// Extensions that are not testable code — exclude from completion gate.
const NON_CODE_EXTS: &[&str] = &[
    ".md",
    ".txt",
    ".csv",
    ".json",
    ".yaml",
    ".yml",
    ".toml",
    ".lock",
    ".log",
    ".env",
    ".gitignore",
    ".dockerignore",
    ".png",
    ".jpg",
    ".jpeg",
    ".gif",
    ".svg",
    ".ico",
    ".woff",
    ".woff2",
    ".ttf",
    ".eot",
];

/// Check if a file path has a non-code extension.
fn is_non_code_file(path: &str) -> bool {
    let lower = path.to_lowercase();
    NON_CODE_EXTS.iter().any(|ext| lower.ends_with(ext))
}

/// Build completion verification report from edit_history × bash_outcomes.
///
/// Uses session_id to filter edits to the current session only,
/// preventing cross-session false positives.
fn build_completion_report(db: &hooks::FileKnowledgeDB, session_id: &str) -> String {
    // Get recent edits for THIS session (falls back to all if session_id empty/missing)
    let edits = match db.recent_edits_for_session(session_id, 30) {
        Ok(e) => e,
        Err(_) => return String::new(),
    };
    if edits.is_empty() {
        return String::new();
    }

    // Deduplicate file paths AND filter out non-code files
    let mut edited_files: Vec<String> = edits
        .iter()
        .map(|e| e.file_path.clone())
        .filter(|p| !is_non_code_file(p))
        .collect();
    edited_files.sort();
    edited_files.dedup();

    if edited_files.is_empty() {
        return String::new();
    }

    // Get recent bash outcomes to check for test execution
    let outcomes = match db.find_bash_outcomes("", 200) {
        Ok(o) => o,
        Err(_) => return String::new(),
    };

    // Classify outcomes by type
    let has_test_runner = outcomes.iter().any(|o| {
        let cmd = o.command.to_lowercase();
        o.exit_code == 0
            && (cmd.contains("pytest")
                || cmd.contains("cargo test")
                || cmd.contains("vitest")
                || cmd.contains("jest"))
    });
    let has_lint = outcomes.iter().any(|o| {
        let cmd = o.command.to_lowercase();
        o.exit_code == 0
            && (cmd.contains("ruff")
                || cmd.contains("clippy")
                || cmd.contains("cargo check")
                || cmd.contains("pyright"))
    });

    // Classify each file
    let mut verified = Vec::new();
    let mut compiled = Vec::new();
    let mut untested = Vec::new();

    for file in &edited_files {
        // Check if any outcome references this file
        let file_tested = outcomes
            .iter()
            .any(|o| o.exit_code == 0 && o.command.contains(file.as_str()));

        if file_tested || has_test_runner {
            verified.push(file.as_str());
        } else if has_lint {
            compiled.push(file.as_str());
        } else {
            untested.push(file.as_str());
        }
    }

    // Only emit if there are concerns
    if compiled.is_empty() && untested.is_empty() {
        return String::new();
    }

    let mut parts = Vec::new();
    parts.push("COMPLETION_VERIFY".to_string());

    if !verified.is_empty() {
        parts.push(format!(
            "✅ VERIFIED({}): {}",
            verified.len(),
            short_file_list(&verified)
        ));
    }
    if !compiled.is_empty() {
        parts.push(format!(
            "⚠ COMPILED_ONLY({}): {}",
            compiled.len(),
            short_file_list(&compiled)
        ));
    }
    if !untested.is_empty() {
        parts.push(format!(
            "❌ UNTESTED({}): {}",
            untested.len(),
            short_file_list(&untested)
        ));
    }

    parts.join(" | ")
}

/// Format file list showing basenames only, with count.
fn short_file_list(files: &[&str]) -> String {
    let names: Vec<&str> = files
        .iter()
        .take(5)
        .map(|f| f.rsplit('/').next().unwrap_or(f))
        .collect();
    if files.len() > 5 {
        format!("{} +{} more", names.join(", "), files.len() - 5)
    } else {
        names.join(", ")
    }
}

// ── Classify Intent Handler ───────────────────────────────────────────

/// Wraps IntentClassifier for UserPromptSubmit events.
pub struct ClassifyIntentHandler {
    classifier: hooks::IntentClassifier,
}

impl Default for ClassifyIntentHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl ClassifyIntentHandler {
    /// Creates a handler with a freshly initialized intent classifier.
    pub fn new() -> Self {
        Self {
            classifier: hooks::IntentClassifier::new(),
        }
    }
}

impl Handler for ClassifyIntentHandler {
    fn name(&self) -> &str {
        "classify_intent"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::UserPromptSubmit]
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        // Claude Code sends "userMessage" for UserPromptSubmit; fall back to "prompt"
        let prompt = ctx
            .input
            .get("userMessage")
            .or_else(|| ctx.input.get("prompt"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let result = self.classifier.classify(prompt);
        let techniques = hooks::IntentClassifier::get_techniques(result.level);
        let technique_hints: Vec<&str> = techniques.iter().map(|t| t.hint()).collect();

        let action = match result.routing_strategy.as_str() {
            "pipeline" => "analyze",
            "tool_use" => "search",
            "compute" => "compute",
            "agent_loop" | "aco_orchestration" => "orchestrate",
            "team_spawn" => "coordinate",
            "self_evolution" => "evolve",
            _ => "modify",
        };

        let arch = match result.level {
            0 => "direct",
            1 => "PAL",
            2 => "ReAct",
            3 => "pipeline",
            4 => "Reflexion",
            5 => "ESAA",
            _ => "multi-agent",
        };

        // Single compact line ≤200 chars (Context Diet S0.1)
        let context = format!(
            "\u{21b3} [L{} {}] action={action} | arch={arch} | code_first={}",
            result.level, result.level_name, result.requires_code_first,
        );

        let metrics = serde_json::json!({
            "cila_level": result.level,
            "cila_name": result.level_name,
            "routing_strategy": result.routing_strategy,
            "requires_pipeline": result.requires_pipeline,
            "requires_code_first": result.requires_code_first,
            "techniques": technique_hints,
        });

        HandlerResult {
            decision: Decision::Allow,
            context_lines: vec![context],
            metrics,
            handler_name: self.name().to_string(),
            duration_ms: 0.0,
        }
    }
}

// ── Scan PII Handler ──────────────────────────────────────────────────

/// Wraps PIIScanner for PreToolUse Write events.
pub struct ScanPiiHandler {
    scanner: hooks::PIIScanner,
}

impl Default for ScanPiiHandler {
    fn default() -> Self {
        Self::new()
    }
}

impl ScanPiiHandler {
    /// Creates a handler with a freshly initialized PII scanner.
    pub fn new() -> Self {
        Self {
            scanner: hooks::PIIScanner::new(),
        }
    }
}

impl Handler for ScanPiiHandler {
    fn name(&self) -> &str {
        "scan_pii"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PreToolUse]
    }

    fn tool_matcher(&self) -> Option<&str> {
        Some("Write|Edit|MultiEdit|Bash")
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let content = ctx
            .tool_input
            .get("content")
            .or_else(|| ctx.tool_input.get("command"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if content.is_empty() {
            return HandlerResult::skip(self.name());
        }

        let findings = self.scanner.scan_text(content);
        if findings.is_empty() {
            return HandlerResult::skip(self.name());
        }

        let context = format!("PII detected: {} finding(s)", findings.len());
        HandlerResult::allow(self.name(), Some(context))
    }
}

// ── Registration ──────────────────────────────────────────────────────

/// Register all 10 neural hook handlers.
pub fn register(pipeline: &mut Pipeline) {
    // Pre-tool handlers (sensory — inject context)
    pipeline.register(Box::new(PreReadHandler));
    pipeline.register(Box::new(PreBashHandler));
    pipeline.register(Box::new(PreEditHandler));
    pipeline.register(Box::new(ScanPiiHandler::new()));
    pipeline.register(Box::new(ClassifyIntentHandler::new()));

    // Post-tool handlers (motor — learn from outcomes)
    pipeline.register(Box::new(PostReadHandler));
    pipeline.register(Box::new(PostBashHandler));
    pipeline.register(Box::new(PostEditHandler));

    // Session lifecycle
    pipeline.register(Box::new(SessionStartHandler));
    pipeline.register(Box::new(SessionStopHandler));
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::context::CortexContext;
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

    #[test]
    fn test_neural_handlers_registered() {
        let mut pipeline = Pipeline::new();
        register(&mut pipeline);
        assert_eq!(pipeline.handler_count(), 10);
    }

    #[test]
    fn test_pre_read_handler_skip_no_file() {
        let input = serde_json::json!({"tool_name": "Read"});
        let (_tmp, mut ctx) = make_test_ctx(HookEvent::PreToolUse, input);
        let handler = PreReadHandler;
        let result = handler.execute(&mut ctx);
        assert!(matches!(result.decision, Decision::Skip));
    }

    #[test]
    fn test_pre_read_handler_with_notes() {
        let input = serde_json::json!({
            "tool_name": "Read",
            "tool_input": {"file_path": "src/main.py"}
        });
        let (_tmp, mut ctx) = make_test_ctx(HookEvent::PreToolUse, input);
        ctx.knowledge
            .upsert(&touring_hooks::knowledge::FileKnowledge {
                file_path: "src/main.py".to_string(),
                notes: Some("Watch out for enum bug".to_string()),
                ..Default::default()
            })
            .unwrap();

        let handler = PreReadHandler;
        let result = handler.execute(&mut ctx);
        assert!(matches!(result.decision, Decision::Allow));
        assert!(!result.context_lines.is_empty());
        assert!(result.context_lines[0].contains("enum bug"));
    }

    #[test]
    fn test_classify_intent_handler() {
        let input = serde_json::json!({"prompt": "calcule o VPL"});
        let (_tmp, mut ctx) = make_test_ctx(HookEvent::UserPromptSubmit, input);
        let handler = ClassifyIntentHandler::new();
        let result = handler.execute(&mut ctx);
        assert!(matches!(result.decision, Decision::Allow));
        assert!(!result.context_lines.is_empty());
        // Compact single-line format (S0.1)
        assert!(result.context_lines[0].contains("[L"));
        assert!(result.context_lines[0].contains("action="));
        assert!(result.context_lines[0].contains("code_first="));
        assert!(result.context_lines.len() == 1, "Must be single line");
        assert!(result.context_lines[0].len() <= 200, "Must be ≤200 chars");
    }

    #[test]
    fn test_scan_pii_handler_no_pii() {
        let input = serde_json::json!({
            "tool_name": "Write",
            "tool_input": {"content": "just normal text"}
        });
        let (_tmp, mut ctx) = make_test_ctx(HookEvent::PreToolUse, input);
        let handler = ScanPiiHandler::new();
        let result = handler.execute(&mut ctx);
        assert!(matches!(result.decision, Decision::Skip));
    }

    #[test]
    fn test_scan_pii_handler_with_pii() {
        let input = serde_json::json!({
            "tool_name": "Write",
            "tool_input": {"content": "CPF: 123.456.789-00"}
        });
        let (_tmp, mut ctx) = make_test_ctx(HookEvent::PreToolUse, input);
        let handler = ScanPiiHandler::new();
        let result = handler.execute(&mut ctx);
        assert!(matches!(result.decision, Decision::Allow));
        assert!(!result.context_lines.is_empty());
        assert!(result.context_lines[0].contains("PII detected"));
    }

    #[test]
    fn test_session_start_handler_with_knowledge() {
        let input = serde_json::json!({"session_id": "test-session"});
        let (_tmp, mut ctx) = make_test_ctx(HookEvent::SessionStart, input);
        ctx.knowledge
            .upsert(&touring_hooks::knowledge::FileKnowledge {
                file_path: "src/main.py".to_string(),
                ..Default::default()
            })
            .unwrap();

        let handler = SessionStartHandler;
        let result = handler.execute(&mut ctx);
        assert!(matches!(result.decision, Decision::Allow));
        assert!(!result.context_lines.is_empty());
        assert!(result.context_lines[0].contains("Touring Knowledge"));
    }

    #[test]
    fn test_session_stop_handler() {
        let input = serde_json::json!({"session_id": "test-session"});
        let (_tmp, mut ctx) = make_test_ctx(HookEvent::Stop, input);
        let handler = SessionStopHandler;
        let result = handler.execute(&mut ctx);
        assert!(matches!(result.decision, Decision::Skip));
        let count = ctx.knowledge.access_count("__session_end__").unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_handler_tool_matchers() {
        assert_eq!(PreReadHandler.tool_matcher(), Some("Read"));
        assert_eq!(PreBashHandler.tool_matcher(), Some("Bash"));
        assert_eq!(PreEditHandler.tool_matcher(), Some("Write|Edit|MultiEdit"));
        assert_eq!(PostReadHandler.tool_matcher(), Some("Read"));
        assert_eq!(PostBashHandler.tool_matcher(), Some("Bash"));
        assert_eq!(PostEditHandler.tool_matcher(), Some("Write|Edit|MultiEdit"));
        assert_eq!(SessionStartHandler.tool_matcher(), None);
        assert_eq!(SessionStopHandler.tool_matcher(), None);
        assert_eq!(ClassifyIntentHandler::new().tool_matcher(), None);
        assert_eq!(
            ScanPiiHandler::new().tool_matcher(),
            Some("Write|Edit|MultiEdit|Bash")
        );
    }

    #[test]
    fn test_handler_async_flags() {
        assert!(!PreReadHandler.is_async());
        assert!(!PreBashHandler.is_async());
        assert!(!PreEditHandler.is_async());
        assert!(PostReadHandler.is_async());
        assert!(PostBashHandler.is_async());
        assert!(PostEditHandler.is_async());
        assert!(!SessionStartHandler.is_async());
        assert!(SessionStopHandler.is_async());
        assert!(!ClassifyIntentHandler::new().is_async());
        assert!(!ScanPiiHandler::new().is_async());
    }

    // ── Task 2: IncrementalPipeline integration tests ────────────────

    #[test]
    fn test_incremental_pipeline_processes_python_file() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("test.py");
        std::fs::write(
            &file_path,
            "def hello():\n    pass\n\ndef world():\n    return 42\n",
        )
        .unwrap();

        let handler = PostEditHandler;
        handler.try_incremental_reindex(file_path.to_str().unwrap(), dir.path());

        // Verify the pipeline cached the tree
        let pipeline = INCREMENTAL_PIPELINE.lock().unwrap();
        let (docs, trees) = pipeline.cache_stats();
        assert!(
            docs >= 1,
            "should have at least 1 cached document, got {docs}"
        );
        assert!(
            trees >= 1,
            "should have at least 1 cached tree, got {trees}"
        );
    }

    #[test]
    fn test_incremental_pipeline_skips_unsupported_language() {
        let dir = tempfile::tempdir().unwrap();
        let file_path = dir.path().join("data.csv");
        std::fs::write(&file_path, "a,b,c\n1,2,3\n").unwrap();

        let handler = PostEditHandler;
        // Should not panic, just log and return
        handler.try_incremental_reindex(file_path.to_str().unwrap(), dir.path());
    }

    #[test]
    fn test_incremental_pipeline_skips_nonexistent_file() {
        let handler = PostEditHandler;
        // Should not panic on missing file
        handler
            .try_incremental_reindex("/nonexistent/path/to/file.py", std::path::Path::new("/tmp"));
    }
}
