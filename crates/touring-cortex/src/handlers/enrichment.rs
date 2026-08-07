//! Enrichment Handlers — Migrated from Python project hooks that use Touring structures.
//!
//! v10.6: 10 handlers migrated from settings.local.json Python hooks.
//!
//! Context enrichment:
//! - `SymbolEnricherHandler` (H61): `PreToolUse[Read|Edit]` — inject file symbol overview from knowledge DB
//! - `SemanticReadEnricherHandler` (H62): `PreToolUse[Read]` — FTS5 semantic memory enrichment
//! - `BlastRadiusGuardHandler` (H64): `PreToolUse[Edit]` — dependency-based blast radius warning
//! - `LearningSessionStartupHandler` (H69): SessionStart — load workflow recommendations from RLM
//!
//! Validation/advisory:
//! - `VgpAdvisoryHandler` (H63): `PreToolUse[Write]` — warn on unverified struct references
//! - `PgccDriftValidatorHandler` (H66): `PreToolUse[Write|Edit]` — detect symbol drift in edits
//! - `PlanFactCheckerHandler` (H68): `PostToolUse[Write|Edit]` — verify path claims in plan files
//!
//! Learning/capture:
//! - `RlmExecutionCaptureHandler` (H65): `PostToolUse[Task]` — capture ANTT task results to RLM
//!
//! Infrastructure:
//! - `PermissionAutoApproverHandler` (H60): PermissionRequest — auto-approve touring MCP tools
//! - `RustAcceleratorCheckHandler` (H67): SessionStart — verify Rust toolchain availability

use std::path::Path;

use touring_foundation::truncate_str;

use crate::context::CortexContext;
use crate::handler::Handler;
use crate::pipeline::Pipeline;
use crate::types::{HandlerResult, HookEvent};

// ── H60: PermissionAutoApproverHandler ───────────────────────────────

/// Auto-approve `mcp__touring__*` tool permissions to reduce friction.
pub struct PermissionAutoApproverHandler;

impl Handler for PermissionAutoApproverHandler {
    fn name(&self) -> &str {
        "permission_auto_approver"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PermissionRequest]
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let tool_name = ctx.tool_name.as_deref().unwrap_or("");

        if tool_name.starts_with("mcp__touring__") {
            if let Some(ref persistence) = ctx.persistence {
                let _ = persistence.log_hook_event(
                    "PermissionRequest",
                    tool_name,
                    "auto_approved",
                    0.5,
                );
            }
            // Return allow with decision for PermissionRequest
            let context = format!("Touring tool auto-approved: {tool_name}");
            return HandlerResult::allow(self.name(), Some(context));
        }

        HandlerResult::skip(self.name())
    }
}

// ── H61: SymbolEnricherHandler ───────────────────────────────────────

/// Inject file symbol overview from knowledge DB before Read/Edit.
/// Queries the knowledge DB for symbol definitions in the file.
pub struct SymbolEnricherHandler;

const CODE_EXTS: &[&str] = &[".py", ".rs", ".ts", ".tsx", ".js", ".jsx", ".go", ".toml"];

impl Handler for SymbolEnricherHandler {
    fn name(&self) -> &str {
        "symbol_enricher"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PreToolUse]
    }

    fn tool_matcher(&self) -> Option<&str> {
        Some("Read|Edit")
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let file_path = match &ctx.file_path {
            Some(fp) => fp.clone(),
            None => return HandlerResult::skip(self.name()),
        };

        // Only enrich source code files
        let is_code = CODE_EXTS.iter().any(|ext| file_path.ends_with(ext));
        if !is_code {
            return HandlerResult::skip(self.name());
        }

        if ctx.context_budget_remaining < 80 {
            return HandlerResult::skip(self.name());
        }

        // Query knowledge DB for file info
        let rel_path = file_path
            .strip_prefix(ctx.project_root.to_string_lossy().as_ref())
            .unwrap_or(&file_path)
            .trim_start_matches('/');

        if let Ok(Some(fk)) = ctx.knowledge.lookup(rel_path) {
            let mut parts: Vec<String> = Vec::new();

            // Include file notes if any
            if let Some(ref notes) = fk.notes
                && !notes.is_empty()
            {
                let snippet = truncate_str(notes, 100);
                parts.push(format!("notes: {snippet}"));
            }

            // Include relations (imports)
            if let Ok(rels) = ctx.knowledge.get_relations_from(rel_path)
                && !rels.is_empty()
            {
                let imports: Vec<String> = rels
                    .iter()
                    .take(8)
                    .map(|r| {
                        Path::new(&r.target)
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or(&r.target)
                            .to_string()
                    })
                    .collect();
                parts.push(format!("imports({}): {}", rels.len(), imports.join(", ")));
            }

            if parts.is_empty() {
                return HandlerResult::skip(self.name());
            }

            let context = format!("[touring-symbols] {rel_path} → {}", parts.join(" | "));
            return HandlerResult::allow(self.name(), Some(context));
        }

        HandlerResult::skip(self.name())
    }
}

// ── H62: SemanticReadEnricherHandler ─────────────────────────────────

/// Enrich code file reads with semantic memories from FTS5 recall DB.
pub struct SemanticReadEnricherHandler;

const SKIP_PARTS: &[&str] = &[
    ".venv",
    "venv",
    "__pycache__",
    "node_modules",
    ".git",
    "target",
    "site-packages",
];

impl Handler for SemanticReadEnricherHandler {
    fn name(&self) -> &str {
        "semantic_read_enricher"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PreToolUse]
    }

    fn tool_matcher(&self) -> Option<&str> {
        Some("Read")
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let file_path = match &ctx.file_path {
            Some(fp) => fp.clone(),
            None => return HandlerResult::skip(self.name()),
        };

        // Skip non-code files and build directories
        let is_code = CODE_EXTS.iter().any(|ext| file_path.ends_with(ext));
        if !is_code {
            return HandlerResult::skip(self.name());
        }
        if SKIP_PARTS.iter().any(|part| file_path.contains(part)) {
            return HandlerResult::skip(self.name());
        }

        if ctx.context_budget_remaining < 100 {
            return HandlerResult::skip(self.name());
        }

        // Build search query from file stem + parent
        let path = std::path::Path::new(&file_path);
        let stem = path.file_stem().and_then(|s| s.to_str()).unwrap_or("");
        let parent = path
            .parent()
            .and_then(|p| p.file_name())
            .and_then(|n| n.to_str())
            .unwrap_or("");
        let query = format!("{} {}", stem.replace('_', " "), parent.replace('_', " "));

        if query.trim().is_empty() {
            return HandlerResult::skip(self.name());
        }

        // FTS5 search in semantic recall
        if let Some(ref recall) = ctx.recall
            && let Ok(hits) = recall.fts_search(&query, 5)
            && hits.len() >= 2
        {
            let mut lines: Vec<String> = vec![format!("Semantic memories for {file_path}:")];
            for hit in hits.iter().take(3) {
                let snippet = truncate_str(&hit.content, 120);
                lines.push(format!("  [#{}] {}", hit.id, snippet));
            }
            let context = lines.join("\n");
            return HandlerResult::allow(self.name(), Some(context));
        }

        HandlerResult::skip(self.name())
    }
}

// ── H63: VgpAdvisoryHandler ──────────────────────────────────────────

/// Warn when code references known Touring structs not VGP-verified in current session.
pub struct VgpAdvisoryHandler;

const KNOWN_STRUCTS: &[&str] = &[
    "BlastRadius",
    "MemoryMatch",
    "MemoryEntry",
    "MCTSResult",
    "SymbolLocation",
    "SymbolIndex",
    "AstOverviewParams",
    "AstFindParams",
    "GraphParams",
    "MemoryStoreParams",
    "MemoryRecallParams",
    "DecomposeParams",
    "SuggestParams",
    "RefactorParams",
    "MctsSearchParams",
    "SpeculateParams",
];

impl Handler for VgpAdvisoryHandler {
    fn name(&self) -> &str {
        "vgp_advisory"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PreToolUse]
    }

    fn tool_matcher(&self) -> Option<&str> {
        Some("Write")
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let file_path = match &ctx.file_path {
            Some(fp) => fp.clone(),
            None => return HandlerResult::skip(self.name()),
        };

        let is_code = CODE_EXTS.iter().any(|ext| file_path.ends_with(ext));
        if !is_code {
            return HandlerResult::skip(self.name());
        }

        let content = ctx
            .tool_input
            .get("content")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if content.is_empty() {
            return HandlerResult::skip(self.name());
        }

        // Check for VGP session registry
        let registry_path = ctx
            .project_root
            .join(".claude/data/vgp_session_registry.json");
        let verified: std::collections::HashSet<String> = if registry_path.exists() {
            std::fs::read_to_string(&registry_path)
                .ok()
                .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
                .and_then(|v| v.get("verified_structs")?.as_array().cloned())
                .map(|arr| {
                    arr.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default()
        } else {
            std::collections::HashSet::new()
        };

        let unverified: Vec<&str> = KNOWN_STRUCTS
            .iter()
            .filter(|s| content.contains(**s) && !verified.contains(**s))
            .copied()
            .collect();

        if unverified.is_empty() {
            return HandlerResult::skip(self.name());
        }

        let names = unverified.join(", ");
        let context = format!(
            "VGP Advisory: {} unverified struct(s): [{}]. Consider VGP extract+verify.",
            unverified.len(),
            names
        );
        HandlerResult::allow(self.name(), Some(context))
    }
}

// ── H64: BlastRadiusGuardHandler ─────────────────────────────────────

/// Query knowledge DB for files that depend on the edited file. Warn on high blast radius.
pub struct BlastRadiusGuardHandler;

impl Handler for BlastRadiusGuardHandler {
    fn name(&self) -> &str {
        "blast_radius_guard"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PreToolUse]
    }

    fn tool_matcher(&self) -> Option<&str> {
        Some("Edit")
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let file_path = match &ctx.file_path {
            Some(fp) => fp.clone(),
            None => return HandlerResult::skip(self.name()),
        };

        let is_code = CODE_EXTS.iter().any(|ext| file_path.ends_with(ext));
        if !is_code {
            return HandlerResult::skip(self.name());
        }

        let rel_path = file_path
            .strip_prefix(ctx.project_root.to_string_lossy().as_ref())
            .unwrap_or(&file_path)
            .trim_start_matches('/');

        // Query dependents from knowledge DB (reverse dependency lookup)
        let dependents = match ctx.knowledge.get_dependents(rel_path) {
            Ok(deps) => deps,
            Err(_) => return HandlerResult::skip(self.name()),
        };
        let count = dependents.len();

        if count == 0 {
            return HandlerResult::skip(self.name());
        }

        let short_list: Vec<String> = dependents
            .iter()
            .take(8)
            .map(|rel| {
                Path::new(&rel.source)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(&rel.source)
                    .to_string()
            })
            .collect();
        let overflow = if count > 8 {
            format!(" (+{} more)", count - 8)
        } else {
            String::new()
        };

        if let Some(ref persistence) = ctx.persistence {
            let _ = persistence.log_hook_event(
                "PreToolUse",
                "BlastRadiusGuard",
                &format!("file={rel_path} dependents={count}"),
                if count > 10 { 0.3 } else { 0.7 },
            );
        }

        let context = format!(
            "[blast-radius] {rel_path} → {count} dependents: {}{}",
            short_list.join(", "),
            overflow
        );
        HandlerResult::allow(self.name(), Some(context))
    }
}

// ── H65: RlmExecutionCaptureHandler ──────────────────────────────────

/// Capture ANTT-relevant Task completions to RLM Memory for pattern learning.
pub struct RlmExecutionCaptureHandler;

const RELEVANT_PATTERNS: &[&str] = &[
    "antt-",
    "vote-",
    "voto-",
    "process-orchestrator",
    "legal-analyzer",
    "technical-analyzer",
    "reequilibrio",
];

impl Handler for RlmExecutionCaptureHandler {
    fn name(&self) -> &str {
        "rlm_execution_capture"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PostToolUse]
    }

    fn tool_matcher(&self) -> Option<&str> {
        Some("Task")
    }

    fn is_async(&self) -> bool {
        true
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let subagent_type = ctx
            .tool_input
            .get("subagent_type")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Only capture ANTT-relevant executions
        let is_relevant = RELEVANT_PATTERNS
            .iter()
            .any(|p| subagent_type.to_lowercase().contains(p));

        if !is_relevant {
            return HandlerResult::skip(self.name());
        }

        let description = ctx
            .tool_input
            .get("description")
            .or_else(|| ctx.tool_input.get("prompt"))
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .chars()
            .take(200)
            .collect::<String>();

        let result_text = ctx
            .input
            .get("output")
            .or_else(|| ctx.input.get("result"))
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let success = !result_text.to_lowercase().contains("error");

        // Store in RLM memory
        if let Some(ref rlm) = ctx.rlm {
            use touring_intelligence::rl::memory::rlm::MemoryTier;
            let key = format!(
                "exec:{}:{}",
                subagent_type,
                &ctx.session_id[..8.min(ctx.session_id.len())]
            );
            let value = format!(
                "workflow={} success={} desc={}",
                subagent_type, success, description
            );
            let _ = rlm.store(&key, MemoryTier::Working, &value, Some("execution"), None);
        }

        if let Some(ref persistence) = ctx.persistence {
            let _ = persistence.wilson_update(&format!("exec:{subagent_type}"), success);
            let _ = persistence.log_hook_event(
                "PostToolUse",
                "RlmExecutionCapture",
                &format!("agent={subagent_type} success={success}"),
                if success { 0.8 } else { 0.3 },
            );
        }

        HandlerResult::skip(self.name()) // Capture only, no context injection
    }
}

// ── H66: PgccDriftValidatorHandler ───────────────────────────────────

/// Detect symbol drift: compare existing file symbols with new content being written.
/// Uses AST-like heuristic to count function/class definitions.
pub struct PgccDriftValidatorHandler;

impl Handler for PgccDriftValidatorHandler {
    fn name(&self) -> &str {
        "pgcc_drift_validator"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PreToolUse]
    }

    fn tool_matcher(&self) -> Option<&str> {
        Some("Write|Edit|MultiEdit")
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let file_path = match &ctx.file_path {
            Some(fp) if fp.ends_with(".py") || fp.ends_with(".rs") => fp.clone(),
            _ => return HandlerResult::skip(self.name()),
        };

        // Get new content
        let new_content = ctx
            .tool_input
            .get("content")
            .or_else(|| ctx.tool_input.get("new_string"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if new_content.len() < 20 {
            return HandlerResult::skip(self.name());
        }

        // Read existing file for baseline
        let existing = match std::fs::read_to_string(&file_path) {
            Ok(s) => s,
            Err(_) => return HandlerResult::skip(self.name()), // New file
        };

        // Extract symbol names (simple heuristic: def/fn/class lines)
        let extract_symbols = |content: &str| -> std::collections::HashSet<String> {
            content
                .lines()
                .filter_map(|line| {
                    let trimmed = line.trim();
                    if trimmed.starts_with("def ")
                        || trimmed.starts_with("async def ")
                        || trimmed.starts_with("class ")
                        || trimmed.starts_with("fn ")
                        || trimmed.starts_with("pub fn ")
                        || trimmed.starts_with("pub fn ")
                        || trimmed.starts_with("struct ")
                        || trimmed.starts_with("pub struct ")
                    {
                        // Extract name (word after keyword)
                        trimmed
                            .split_whitespace()
                            .find(|w| {
                                !matches!(*w, "def" | "async" | "class" | "fn" | "pub" | "struct")
                            })
                            .map(|w| w.trim_end_matches(['(', ':', '<', '{']).to_string())
                    } else {
                        None
                    }
                })
                .collect()
        };

        let old_syms = extract_symbols(&existing);
        let new_syms = extract_symbols(new_content);

        if old_syms.len() < 2 {
            return HandlerResult::skip(self.name()); // Too few symbols for meaningful drift
        }

        let removed: Vec<&String> = old_syms.difference(&new_syms).collect();
        let drift_rate = removed.len() as f64 / old_syms.len() as f64;

        if let Some(ref persistence) = ctx.persistence {
            let _ = persistence.drift_record("pgcc_symbol_drift", 1.0 - drift_rate);
        }

        if drift_rate > 0.4 {
            let removed_names: Vec<&str> = removed.iter().take(5).map(|s| s.as_str()).collect();
            let context = format!(
                "PGCC drift: {:.0}% symbols removed from {} — [{}]",
                drift_rate * 100.0,
                Path::new(&file_path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or(&file_path),
                removed_names.join(", ")
            );
            HandlerResult::allow(self.name(), Some(context))
        } else {
            HandlerResult::skip(self.name())
        }
    }
}

// ── H67: RustAcceleratorCheckHandler ─────────────────────────────────

/// SessionStart: verify Rust toolchain is available (touring binary, cargo).
pub struct RustAcceleratorCheckHandler;

impl Handler for RustAcceleratorCheckHandler {
    fn name(&self) -> &str {
        "rust_accelerator_check"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::SessionStart]
    }

    fn is_async(&self) -> bool {
        true
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let mut status: Vec<String> = Vec::new();

        // Check touring binary
        let touring_ok = std::process::Command::new("touring")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        status.push(format!(
            "touring={}",
            if touring_ok { "OK" } else { "MISSING" }
        ));

        // Check cargo
        let cargo_ok = std::process::Command::new("cargo")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        status.push(format!("cargo={}", if cargo_ok { "OK" } else { "MISSING" }));

        // Check ruff
        let ruff_ok = std::process::Command::new("ruff")
            .arg("--version")
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .map(|s| s.success())
            .unwrap_or(false);
        status.push(format!("ruff={}", if ruff_ok { "OK" } else { "MISSING" }));

        if let Some(ref persistence) = ctx.persistence {
            let all_ok = touring_ok && cargo_ok && ruff_ok;
            let _ = persistence.log_hook_event(
                "SessionStart",
                "RustAcceleratorCheck",
                &status.join(" "),
                if all_ok { 1.0 } else { 0.5 },
            );
        }

        HandlerResult::skip(self.name()) // No context injection, just logging
    }
}

// ── H68: PlanFactCheckerHandler ──────────────────────────────────────

/// Verify path claims in plan markdown files against real filesystem.
pub struct PlanFactCheckerHandler;

impl Handler for PlanFactCheckerHandler {
    fn name(&self) -> &str {
        "plan_fact_checker"
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
        let file_path = match &ctx.file_path {
            Some(fp) if fp.ends_with(".md") && fp.contains("plans") => fp.clone(),
            _ => return HandlerResult::skip(self.name()),
        };

        // Read the plan file content
        let content = match std::fs::read_to_string(&file_path) {
            Ok(s) => s,
            Err(_) => return HandlerResult::skip(self.name()),
        };

        // Extract path claims: backtick-wrapped paths. The 4 patterns are static
        // literals, so compile them once (P-6: previously recompiled all 4 regexes
        // on every call to this enrichment path).
        static PATH_CLAIM_RES: once_cell::sync::Lazy<Vec<regex::Regex>> =
            once_cell::sync::Lazy::new(|| {
                [
                    r"`(~/.claude/[^\s`]+)`",
                    r"`(\$HOME/.claude/[^\s`]+)`",
                    r"`(\.claude/[^\s`]+\.(?:rs|py|json|toml|sh|md))`",
                    r"`(crates/[^\s`]+\.rs)`",
                ]
                .iter()
                .filter_map(|p| regex::Regex::new(p).ok())
                .collect()
            });

        let mut missing: Vec<String> = Vec::new();
        let home = std::env::var("HOME").unwrap_or_default();

        for re in PATH_CLAIM_RES.iter() {
            for cap in re.captures_iter(&content) {
                if let Some(m) = cap.get(1) {
                    let raw = m.as_str();
                    let expanded = raw.replace('~', &home).replace("$HOME", &home);
                    if !Path::new(&expanded).exists() {
                        missing.push(raw.to_string());
                    }
                }
            }
        }

        if missing.is_empty() {
            return HandlerResult::skip(self.name());
        }

        if let Some(ref persistence) = ctx.persistence {
            let _ = persistence.log_hook_event(
                "PostToolUse",
                "PlanFactChecker",
                &format!("missing={} in {file_path}", missing.len()),
                0.4,
            );
        }

        let list = missing
            .iter()
            .take(5)
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        let overflow = if missing.len() > 5 {
            format!(" (+{} more)", missing.len() - 5)
        } else {
            String::new()
        };
        let context = format!(
            "PlanFactCheck: {} path(s) not found: [{list}]{overflow}",
            missing.len()
        );
        HandlerResult::allow(self.name(), Some(context))
    }
}

// ── H69: LearningSessionStartupHandler ───────────────────────────────

/// Load workflow recommendations from RLM memory at session start.
pub struct LearningSessionStartupHandler;

impl Handler for LearningSessionStartupHandler {
    fn name(&self) -> &str {
        "learning_session_startup"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::SessionStart]
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        if ctx.context_budget_remaining < 200 {
            return HandlerResult::skip(self.name());
        }

        let mut parts: Vec<String> = Vec::new();

        // Load top Wilson-ranked patterns for session guidance
        if let Some(ref persistence) = ctx.persistence
            && let Ok(top) = persistence.wilson_top_k(5)
        {
            for (id, score, successes, trials) in &top {
                if *score > 0.6 && *trials > 2 {
                    parts.push(format!("{id} (conf={score:.2}, {successes}/{trials})"));
                }
            }
        }

        // Load recent workflow executions from RLM
        if let Some(ref rlm) = ctx.rlm
            && let Ok(entries) = rlm.search("exec:", None, 3)
        {
            for entry in &entries {
                let snippet = entry.value.chars().take(80).collect::<String>();
                parts.push(format!("[recent] {snippet}"));
            }
        }

        if parts.is_empty() {
            return HandlerResult::skip(self.name());
        }

        if let Some(ref persistence) = ctx.persistence {
            let _ = persistence.log_hook_event(
                "SessionStart",
                "LearningStartup",
                &format!("loaded={} patterns", parts.len()),
                0.7,
            );
        }

        let context = format!("Session guidance: {}", parts.join(" | "));
        HandlerResult::allow(self.name(), Some(context))
    }
}

// ── H70: CodeFirstPipelineGuardHandler ────────────────────────────────

/// CODE-FIRST enforcement: blocks ANTT skills without verified pipeline state.
/// Checks pipeline_state_*.json existence, phase completion, and quality scores.
pub struct CodeFirstPipelineGuardHandler;

const ANTT_SKILLS: &[&str] = &[
    "antt-preliminary-analyzer",
    "antt-chronology-builder",
    "antt-technical-analyzer",
    "antt-critical-analyzer",
    "antt-legal-analyzer",
    "antt-final-synthesizer",
    "antt-vote-architect",
    "process-analysis-maestria",
    "antt-cognitive-orchestrator",
    "antt-deliberation-analyzer",
    "antt-citation-validator",
    "antt-narrative-architect",
    "antt-document-processor",
    "antt-habilitacao-analyzer",
    "antt-trip-analyzer",
    "antt-document-analyzer",
];

impl Handler for CodeFirstPipelineGuardHandler {
    fn name(&self) -> &str {
        "code_first_pipeline_guard"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PreToolUse]
    }

    fn tool_matcher(&self) -> Option<&str> {
        Some("Skill|Task")
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        // Extract skill name from Skill events or Task prompt
        let skill_name = self.extract_skill_name(ctx);

        if skill_name.is_empty() || !ANTT_SKILLS.iter().any(|s| *s == skill_name) {
            return HandlerResult::skip(self.name());
        }

        // Search for pipeline_state_*.json in relatoria directories
        let relatoria_dir = ctx.project_root.join("analise/relatoria");
        if !relatoria_dir.exists() {
            // No relatoria dir — warn but don't block (might be first run)
            let context = format!(
                "CODE-FIRST WARN: No analise/relatoria/ found for {skill_name}. Run pipeline first."
            );
            return HandlerResult::allow(self.name(), Some(context));
        }

        // Find any pipeline_state file
        let has_pipeline_state = std::fs::read_dir(&relatoria_dir)
            .ok()
            .map(|entries| {
                entries.filter_map(|e| e.ok()).any(|entry| {
                    if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                        // Check inside each process directory
                        std::fs::read_dir(entry.path())
                            .ok()
                            .map(|sub| {
                                sub.filter_map(|s| s.ok()).any(|f| {
                                    f.file_name()
                                        .to_string_lossy()
                                        .starts_with("pipeline_state_")
                                })
                            })
                            .unwrap_or(false)
                    } else {
                        false
                    }
                })
            })
            .unwrap_or(false);

        if !has_pipeline_state {
            let reason = format!(
                "CODE-FIRST BLOCK: No pipeline_state found for {skill_name}. \
                 Run: python -m scripts.process_analysis.pipeline_runner"
            );
            return HandlerResult::block(self.name(), reason);
        }

        if let Some(ref persistence) = ctx.persistence {
            let _ = persistence.log_hook_event(
                "PreToolUse",
                "CodeFirstGuard",
                &format!("skill={skill_name} pipeline=found"),
                0.9,
            );
        }

        HandlerResult::skip(self.name()) // Pipeline exists, allow
    }
}

impl CodeFirstPipelineGuardHandler {
    fn extract_skill_name(&self, ctx: &CortexContext) -> String {
        // For Skill events: tool_input.skill or tool_input.name
        if let Some(name) = ctx.tool_input.get("skill").and_then(|v| v.as_str()) {
            return name.to_string();
        }
        if let Some(name) = ctx.tool_input.get("name").and_then(|v| v.as_str()) {
            return name.to_string();
        }

        // For Task events: extract antt-* from prompt/subagent_type
        if let Some(subagent) = ctx.tool_input.get("subagent_type").and_then(|v| v.as_str())
            && (subagent.starts_with("antt-") || ANTT_SKILLS.contains(&subagent))
        {
            return subagent.to_string();
        }

        // Heuristic: search prompt for antt-* pattern
        if let Some(prompt) = ctx.tool_input.get("prompt").and_then(|v| v.as_str()) {
            for word in prompt.split_whitespace() {
                let w = word.to_lowercase();
                if w.starts_with("antt-") {
                    return w;
                }
            }
        }

        String::new()
    }
}

// ── H71: CipherKnowledgeRetrievalHandler ─────────────────────────────

/// Pre-Read/Write: search .cipher/ patterns for relevant knowledge and inject as context.
/// Feeds retrieval success/failure into RL Wilson scores.
pub struct CipherKnowledgeRetrievalHandler;

impl Handler for CipherKnowledgeRetrievalHandler {
    fn name(&self) -> &str {
        "cipher_knowledge_retrieval"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PreToolUse]
    }

    fn tool_matcher(&self) -> Option<&str> {
        Some("Read|Write|Edit|Grep|Glob")
    }

    fn is_async(&self) -> bool {
        true
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let file_path = match &ctx.file_path {
            Some(fp) => fp.clone(),
            None => return HandlerResult::skip(self.name()),
        };

        if ctx.context_budget_remaining < 100 {
            return HandlerResult::skip(self.name());
        }

        // Search .cipher/patterns/ for relevant knowledge
        let cipher_dir = ctx.project_root.join(".cipher/patterns");
        if !cipher_dir.exists() {
            return HandlerResult::skip(self.name());
        }

        let filename = std::path::Path::new(&file_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("");
        let file_ext = std::path::Path::new(&file_path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("");

        // Scan pattern files for matches
        let mut hits: Vec<String> = Vec::new();
        if let Ok(entries) = std::fs::read_dir(&cipher_dir) {
            for entry in entries.filter_map(|e| e.ok()) {
                if entry.path().extension().is_none_or(|e| e != "json") {
                    continue;
                }
                if let Ok(data) = std::fs::read_to_string(entry.path()) {
                    // Quick check: does this pattern file reference our file?
                    if data.contains(filename) || data.contains(file_ext) {
                        // Extract first matching pattern summary
                        if let Ok(patterns) = serde_json::from_str::<Vec<serde_json::Value>>(&data)
                        {
                            for pat in patterns.iter().take(2) {
                                if let Some(tags) = pat.get("tags").and_then(|t| t.as_str())
                                    && (tags.contains(filename) || tags.contains(file_ext))
                                {
                                    let preview = pat
                                        .get("content_preview")
                                        .and_then(|c| c.as_str())
                                        .unwrap_or("")
                                        .chars()
                                        .take(80)
                                        .collect::<String>();
                                    if !preview.is_empty() {
                                        hits.push(format!("[cipher] {preview}"));
                                    }
                                }
                            }
                        }
                    }
                }
                if hits.len() >= 3 {
                    break;
                }
            }
        }

        // Feed into RL: retrieval success
        if let Some(ref persistence) = ctx.persistence {
            let success = !hits.is_empty();
            let _ = persistence.wilson_update("cipher:retrieval", success);
            let _ = persistence.log_hook_event(
                "PreToolUse",
                "CipherRetrieval",
                &format!("hits={} file={filename}", hits.len()),
                if success { 0.7 } else { 0.2 },
            );
        }

        if hits.is_empty() {
            return HandlerResult::skip(self.name());
        }

        let context = hits.join(" | ");
        HandlerResult::allow(self.name(), Some(context))
    }
}

// ── H72: CipherKnowledgeCaptureHandler ───────────────────────────────

/// Post-Write/Edit: capture file patterns to .cipher/ and feed success into RL.
pub struct CipherKnowledgeCaptureHandler;

impl Handler for CipherKnowledgeCaptureHandler {
    fn name(&self) -> &str {
        "cipher_knowledge_capture"
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
        let file_path = match &ctx.file_path {
            Some(fp) if fp.ends_with(".py") || fp.ends_with(".rs") || fp.ends_with(".ts") => {
                fp.clone()
            }
            _ => return HandlerResult::skip(self.name()),
        };

        let file_ext = std::path::Path::new(&file_path)
            .extension()
            .and_then(|e| e.to_str())
            .unwrap_or("unknown");

        // Store pattern in RLM memory for cross-session learning
        if let Some(ref rlm) = ctx.rlm {
            use touring_intelligence::rl::memory::rlm::MemoryTier;
            let filename = std::path::Path::new(&file_path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("unknown");
            let key = format!("cipher:capture:{filename}");
            let value = format!("type={file_ext} path={file_path}");
            let _ = rlm.store(
                &key,
                MemoryTier::Working,
                &value,
                Some("cipher_capture"),
                None,
            );
        }

        // Feed into RL: capture event
        if let Some(ref persistence) = ctx.persistence {
            let _ = persistence.wilson_update("cipher:capture", true);
            let _ = persistence.log_hook_event(
                "PostToolUse",
                "CipherCapture",
                &format!("file={file_path} type={file_ext}"),
                0.6,
            );
        }

        HandlerResult::skip(self.name()) // Capture only, no context injection
    }
}

// ── H73: CipherAgentOutputCaptureHandler ─────────────────────────────

/// SubagentStop: capture agent outputs into RLM memory for RL feedback loop.
/// Connects previously dead-end .cipher/agent_outputs to Touring RL.
pub struct CipherAgentOutputCaptureHandler;

impl Handler for CipherAgentOutputCaptureHandler {
    fn name(&self) -> &str {
        "cipher_agent_output_capture"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::SubagentStop]
    }

    fn is_async(&self) -> bool {
        true
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let subagent_type = ctx
            .input
            .get("subagent_type")
            .or_else(|| ctx.input.get("subagent_id"))
            .and_then(|v| v.as_str())
            .unwrap_or("unknown");

        let output = ctx
            .input
            .get("output")
            .or_else(|| ctx.input.get("result"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if output.is_empty() || output.len() < 20 {
            return HandlerResult::skip(self.name());
        }

        // Detect success/failure patterns in output
        let output_lower = output.to_lowercase();
        let success = !output_lower.contains("error") && !output_lower.contains("failed");

        // Extract key insights (lines with markers)
        let insights: Vec<&str> = output
            .lines()
            .filter(|l| {
                let t = l.trim();
                t.starts_with("✅")
                    || t.starts_with("PASS")
                    || t.contains("Found")
                    || t.contains("Completed")
                    || t.contains("Generated")
                    || t.contains("Created")
            })
            .take(3)
            .collect();

        // Store in RLM memory — THIS is the new RL connection
        if let Some(ref rlm) = ctx.rlm {
            use touring_intelligence::rl::memory::rlm::MemoryTier;
            let key = format!("cipher:agent:{subagent_type}");
            let summary = if insights.is_empty() {
                format!(
                    "agent={subagent_type} success={success} len={}",
                    output.len()
                )
            } else {
                format!(
                    "agent={subagent_type} success={success} insights={}",
                    insights.join(" | ")
                )
            };
            let _ = rlm.store(
                &key,
                MemoryTier::Working,
                &summary,
                Some("agent_output"),
                None,
            );
        }

        // Feed into Wilson RL scores
        if let Some(ref persistence) = ctx.persistence {
            let _ = persistence.wilson_update(&format!("agent:{subagent_type}"), success);
            let _ = persistence.log_hook_event(
                "SubagentStop",
                "CipherAgentCapture",
                &format!(
                    "agent={subagent_type} success={success} insights={}",
                    insights.len()
                ),
                if success { 0.8 } else { 0.3 },
            );
        }

        HandlerResult::skip(self.name()) // Capture only
    }
}

// ── H74: ObserverLearningLoopHandler ─────────────────────────────────

/// Stop: capture session patterns (cache efficiency, error recovery) into RL.
/// Replaces the Python observer_learning_loop.py with RL-connected version.
pub struct ObserverLearningLoopHandler;

impl Handler for ObserverLearningLoopHandler {
    fn name(&self) -> &str {
        "observer_learning_loop"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::Stop]
    }

    fn is_async(&self) -> bool {
        true
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        // Pattern 1: Session completion — always record
        if let Some(ref persistence) = ctx.persistence {
            let _ = persistence.wilson_update("session:completed", true);
        }

        // Pattern 2: Cache efficiency from usage data
        let usage = ctx.input.get("usage");
        if let Some(usage_obj) = usage {
            let input_tokens = usage_obj
                .get("input_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let output_tokens = usage_obj
                .get("output_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let cache_read = usage_obj
                .get("cache_read_input_tokens")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            let total = input_tokens + output_tokens;

            if total > 0 {
                let cache_ratio = cache_read as f64 / total as f64;

                if let Some(ref persistence) = ctx.persistence {
                    let _ = persistence.drift_record("cache_ratio", cache_ratio);
                    if cache_ratio > 0.3 {
                        let _ = persistence.wilson_update("session:cache_efficient", true);
                    }
                }

                // Store in RLM for cross-session analysis
                if let Some(ref rlm) = ctx.rlm {
                    use touring_intelligence::rl::memory::rlm::MemoryTier;
                    let _ = rlm.store(
                        &format!("session:usage:{}", &ctx.session_id[..8.min(ctx.session_id.len())]),
                        MemoryTier::Working,
                        &format!("total={total} cache_ratio={cache_ratio:.3} input={input_tokens} output={output_tokens}"),
                        Some("session_usage"),
                        None,
                    );
                }
            }
        }

        // Pattern 3: Turn count (high = retry storms)
        let turn_count = ctx
            .input
            .get("turn_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        if turn_count > 20
            && let Some(ref persistence) = ctx.persistence
        {
            let _ = persistence.wilson_update("session:high_turn_count", false); // negative signal
            let _ = persistence.log_hook_event(
                "Stop",
                "ObserverLearning",
                &format!("turn_count={turn_count} (retry storm signal)"),
                0.4,
            );
        }

        HandlerResult::skip(self.name()) // Learning only, no context
    }
}

// ── Registration ─────────────────────────────────────────────────────

/// Register all enrichment handlers.
pub fn register(pipeline: &mut Pipeline) {
    pipeline.register(Box::new(PermissionAutoApproverHandler));
    pipeline.register(Box::new(SymbolEnricherHandler));
    pipeline.register(Box::new(SemanticReadEnricherHandler));
    pipeline.register(Box::new(VgpAdvisoryHandler));
    pipeline.register(Box::new(BlastRadiusGuardHandler));
    pipeline.register(Box::new(RlmExecutionCaptureHandler));
    pipeline.register(Box::new(PgccDriftValidatorHandler));
    pipeline.register(Box::new(RustAcceleratorCheckHandler));
    pipeline.register(Box::new(PlanFactCheckerHandler));
    pipeline.register(Box::new(LearningSessionStartupHandler));
    pipeline.register(Box::new(CodeFirstPipelineGuardHandler));
    pipeline.register(Box::new(CipherKnowledgeRetrievalHandler));
    pipeline.register(Box::new(CipherKnowledgeCaptureHandler));
    pipeline.register(Box::new(CipherAgentOutputCaptureHandler));
    pipeline.register(Box::new(ObserverLearningLoopHandler));
}

#[cfg(test)]
#[path = "enrichment_tests.rs"]
mod tests;
