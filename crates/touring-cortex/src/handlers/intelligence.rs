//! Intelligence Handlers — ACO phase0, DSPy perception, prompt recall.
//!
//! Ported from:
//! - `.claude/rust-core/src/bin/hook_aco_phase0.rs`
//! - `.claude/rust-core/src/bin/hook_dspy_perception.rs`
//! - `.claude/rust-core/src/touring/handlers/prompt_submit.rs`

use std::collections::HashSet;

use once_cell::sync::Lazy;
use regex::Regex;

use touring_foundation::truncate_str;

use crate::context::CortexContext;
use crate::handler::Handler;
use crate::pipeline::Pipeline;
use crate::types::{HandlerResult, HookEvent};

// ══════════════════════════════════════════════════════════════════════
// 1. AcoPhase0Handler — MUST ALWAYS BE ACTIVE
// ══════════════════════════════════════════════════════════════════════

/// Pre-compiled combined regex for ACO trigger detection (case-insensitive).
///
/// Patterns: --aco, --swarm, --agents-team, orquestracao completa, ciclo aco,
/// 7 fases, fase 0, intentspec, objectivespec.
static ACO_PATTERN: Lazy<Regex> = Lazy::new(|| {
    Regex::new(
        r"(?i)--aco\b|--swarm\b|--agents-team\b|orquestra[çc][ãa]o\s+completa|ciclo\s+aco\b|7\s+fases|fase\s+0\b|\bintentspec\b|\bobjectivespec\b"
    )
    .expect("ACO regex must compile")
});

/// Guidance message injected when ACO mode is detected.
const ACO_GUIDANCE: &str = "\
ACO MODE ATIVO: Ciclo Adaptive Cognitive Orchestration detectado. \
Seguir sequ\u{00ea}ncia: Phase0(IntentSpec) \u{2192} Phase1(ObjectiveSpec) \u{2192} \
Phase2(StrategyGen) \u{2192} Phase3(Execute) \u{2192} Phase4(Evaluate) \u{2192} \
Phase5(Refine) \u{2192} Phase6(Persist). \
Cada fase DEVE produzir artefato verific\u{00e1}vel. \
Rollback dispon\u{00ed}vel entre fases.";

/// Detect if the prompt contains any ACO trigger pattern.
fn detect_aco_mode(prompt: &str) -> bool {
    ACO_PATTERN.is_match(prompt)
}

/// ACO Phase0 Detector — scans prompts for ACO trigger patterns.
///
/// Ported from `hook_aco_phase0.rs`. On UserPromptSubmit, regex-scans
/// the prompt for ACO trigger patterns and injects ACO workflow guidance
/// if detected.
///
/// **MUST ALWAYS BE ACTIVE** (no CILA level filtering).
/// **Sync, never blocks.**
pub struct AcoPhase0Handler;

impl Handler for AcoPhase0Handler {
    fn name(&self) -> &str {
        "aco_phase0"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::UserPromptSubmit]
    }

    // No tool_matcher — always active on UserPromptSubmit

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        // Claude Code sends "userMessage" for UserPromptSubmit; fall back to "prompt"
        let prompt = ctx
            .input
            .get("userMessage")
            .or_else(|| ctx.input.get("prompt"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if prompt.trim().is_empty() {
            return HandlerResult::skip(self.name());
        }

        if detect_aco_mode(prompt) {
            HandlerResult::allow(self.name(), Some(ACO_GUIDANCE.to_string()))
        } else {
            HandlerResult::skip(self.name())
        }
    }
}

// ══════════════════════════════════════════════════════════════════════
// NOTE: DspyPerceptionHandler was merged into ClassifyIntentHandler (S0.1).
// Classification now lives in neural.rs via CILA router (more sophisticated).
// ══════════════════════════════════════════════════════════════════════
// 3. PromptRecallHandler
// ══════════════════════════════════════════════════════════════════════

/// Stop words filtered from queries (same set as the touring-daemon).
const STOP_WORDS: &[&str] = &[
    "the", "and", "for", "with", "this", "that", "have", "from", "will", "would", "could",
    "should", "into", "about", "been", "they", "then", "when", "what", "also", "more", "some",
    "there", "their", "than", "over", "such", "very", "just", "your", "were", "here", "where",
];

/// Extract keywords from free text: stop-word filtered, 4+ chars, deduped.
fn extract_keywords(text: &str, max_words: usize) -> Vec<String> {
    let cleaned: String = text
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { ' ' })
        .collect();

    let mut seen = HashSet::new();
    cleaned
        .split_whitespace()
        .filter(|w| w.len() > 3 && !STOP_WORDS.contains(&w.to_lowercase().as_str()))
        .map(|w| w.to_lowercase())
        .filter(|w| seen.insert(w.clone()))
        .take(max_words)
        .collect()
}

/// Maximum chars for recall output (S0.2 Context Diet).
const MAX_RECALL_CHARS: usize = 150;

/// Minimum keyword matches required to emit recall (confidence gate S0.2).
const MIN_KEYWORD_MATCHES: usize = 2;

/// Prompt Recall — extracts keywords and searches knowledge DB for context.
///
/// Adapted from `touring/handlers/prompt_submit.rs`. On UserPromptSubmit,
/// extracts keywords from the prompt, persists the prompt event in the
/// knowledge DB, and searches for relevant file knowledge to inject as
/// context recall.
///
/// Gated by MIN_KEYWORD_MATCHES and truncated to MAX_RECALL_CHARS (S0.2).
///
/// **Async, never blocks.**
pub struct PromptRecallHandler;

impl Handler for PromptRecallHandler {
    fn name(&self) -> &str {
        "prompt_recall"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::UserPromptSubmit]
    }

    fn is_async(&self) -> bool {
        true
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let prompt = ctx
            .input
            .get("userMessage")
            .or_else(|| ctx.input.get("prompt"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let keywords = extract_keywords(prompt, 8);
        if keywords.is_empty() {
            return HandlerResult::skip(self.name());
        }

        let _ = ctx
            .knowledge
            .record_access("__prompt_event__", &ctx.session_id);

        let mut snippets: Vec<String> = Vec::new();
        recall_file_knowledge(ctx, &keywords, &mut snippets);
        recall_rlm_entries(ctx, &keywords, &mut snippets);
        recall_rl_top_action(ctx, &mut snippets);
        recall_gotchas(ctx, prompt, &mut snippets);

        if snippets.len() < MIN_KEYWORD_MATCHES {
            return HandlerResult::skip(self.name());
        }

        let mut context = format!("Touring: {}", snippets.join(" | "));
        if context.len() > MAX_RECALL_CHARS {
            context.truncate(MAX_RECALL_CHARS);
            context.push_str("...");
        }
        HandlerResult::allow(self.name(), Some(context))
    }
}

// ── Recall helpers (extracted from PromptRecallHandler::execute) ─────

/// Source 1: file knowledge notes + failed bash outcomes from touring_knowledge.db.
fn recall_file_knowledge(ctx: &CortexContext, keywords: &[String], snippets: &mut Vec<String>) {
    for keyword in keywords.iter().take(4) {
        if let Ok(Some(fk)) = ctx.knowledge.lookup(keyword) {
            if let Some(ref notes) = fk.notes {
                if !notes.is_empty() {
                    snippets.push(format!(
                        "[file:{}] {}",
                        fk.file_path,
                        truncate_str(notes, 100)
                    ));
                }
            }
        }
        if let Ok(outcomes) = ctx.knowledge.find_bash_outcomes(keyword, 2) {
            for outcome in outcomes {
                if !outcome.success {
                    if let Some(ref err) = outcome.error_pattern {
                        snippets.push(format!(
                            "[cmd:{}] {}",
                            outcome.command_short,
                            truncate_str(err, 100)
                        ));
                    }
                }
            }
        }
        if snippets.len() >= 3 {
            break;
        }
    }
}

/// Source 2: insights, lessons, regression patterns from consolidated memory.db.
fn recall_rlm_entries(ctx: &CortexContext, keywords: &[String], snippets: &mut Vec<String>) {
    let rlm_path = touring_foundation::TouringConfig::memory_db_canonical(&ctx.project_root);
    let conn = match open_rlm_readonly(&rlm_path) {
        Some(c) => c,
        None => return,
    };
    for keyword in keywords.iter().take(3) {
        let pattern = format!("%{keyword}%");
        let Ok(mut stmt) = conn.prepare(
            "SELECT key, value FROM rlm_entries \
             WHERE (value LIKE ?1 OR key LIKE ?1) \
             AND entry_type IN ('insight','failure_pattern','regression_pattern','session_insight','lesson') \
             ORDER BY access_count DESC LIMIT 2",
        ) else { continue };
        let Ok(rows) = stmt.query_map(rusqlite::params![pattern], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        }) else {
            continue;
        };
        for row in rows.flatten() {
            let short_key = row.0.rsplit(':').next().unwrap_or(&row.0);
            snippets.push(format!(
                "[memory:{short_key}] {}",
                truncate_str(&row.1, 100)
            ));
            if snippets.len() >= 5 {
                return;
            }
        }
        if snippets.len() >= 5 {
            return;
        }
    }
}

/// Source 3: top QTable action from consolidated graph.db.
fn recall_rl_top_action(ctx: &CortexContext, snippets: &mut Vec<String>) {
    let rlm_path = touring_foundation::TouringConfig::graph_db_canonical(&ctx.project_root);
    let conn = match open_rlm_readonly(&rlm_path) {
        Some(c) => c,
        None => return,
    };
    let Ok(mut stmt) = conn
        .prepare("SELECT state_action, q_value FROM learning_qtable ORDER BY q_value DESC LIMIT 1")
    else {
        return;
    };
    let Ok(rows) = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
    }) else {
        return;
    };
    for (sa, qv) in rows.flatten() {
        if qv > 0.5 {
            snippets.push(format!("RL top: {sa} (Q={qv:.1})"));
        }
    }
}

/// Source 4: gotchas matching the prompt text.
fn recall_gotchas(ctx: &CortexContext, prompt: &str, snippets: &mut Vec<String>) {
    let gotchas = ctx.knowledge.get_gotchas_for_file(prompt);
    for g in gotchas.iter().take(2) {
        snippets.push(format!(
            "⚠ GOTCHA [{}]: {}",
            g.severity,
            truncate_str(&g.gotcha, 100)
        ));
    }
}

/// Open rlm_memory.db in read-only mode, returning None if unavailable.
fn open_rlm_readonly(path: &std::path::Path) -> Option<rusqlite::Connection> {
    if !path.exists() {
        return None;
    }
    rusqlite::Connection::open_with_flags(
        path,
        rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
    )
    .ok()
}

// ── Registration ──────────────────────────────────────────────────────

// ══════════════════════════════════════════════════════════════════════
// H42: TouringMemoryInjector — UserPromptSubmit memory + RL injection
// ══════════════════════════════════════════════════════════════════════

/// Inject relevant memories + RL suggestions into prompt context (L2+ only).
pub struct TouringMemoryInjectorHandler;

impl Handler for TouringMemoryInjectorHandler {
    fn name(&self) -> &str {
        "touring_memory_injector"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::UserPromptSubmit]
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        if ctx.context_budget_remaining < 80 {
            return HandlerResult::skip(self.name());
        }

        let prompt = ctx
            .input
            .get("userMessage")
            .or_else(|| ctx.input.get("prompt"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if prompt.len() < 12 {
            return HandlerResult::skip(self.name());
        }

        if estimate_cila_level(prompt) < 2 {
            return HandlerResult::skip(self.name());
        }

        let mut parts: Vec<String> = Vec::new();
        inject_fts5_recall(ctx, prompt, &mut parts);
        inject_rlm_matches(ctx, prompt, &mut parts);
        inject_wilson_suggestions(ctx, &mut parts);

        if parts.is_empty() {
            return HandlerResult::skip(self.name());
        }

        let context = format!("Touring: {}", parts.join(" | "));
        HandlerResult::allow(self.name(), Some(context))
    }
}

// ── Memory injection helpers (extracted from TouringMemoryInjectorHandler) ──

/// Simple CILA level estimation from prompt keywords.
fn estimate_cila_level(prompt: &str) -> u8 {
    let lower = prompt.to_lowercase();
    if lower.contains("orquestr")
        || lower.contains("aco")
        || lower.contains("swarm")
        || lower.contains("multi-agent")
    {
        4
    } else if lower.contains("processo")
        || lower.contains("50500")
        || lower.contains("pipeline")
        || lower.contains("antt")
    {
        3
    } else if lower.contains("busque")
        || lower.contains("analise")
        || lower.contains("search")
        || lower.contains("explore")
    {
        2
    } else {
        0
    }
}

/// FTS5 search on semantic_recall.db for memory chunks.
fn inject_fts5_recall(ctx: &CortexContext, prompt: &str, parts: &mut Vec<String>) {
    let recall = match &ctx.recall {
        Some(r) => r,
        None => return,
    };
    let keywords: Vec<&str> = prompt
        .split_whitespace()
        .filter(|w| w.len() >= 4)
        .take(6)
        .collect();
    if keywords.is_empty() {
        return;
    }
    let query = keywords.join(" ");
    if let Ok(chunks) = recall.fts_search(&query, 3) {
        for chunk in &chunks {
            let snippet: String = chunk.content.chars().take(80).collect();
            parts.push(format!("[memory] {snippet}"));
        }
    }
}

/// RLM search for matching memory entries.
fn inject_rlm_matches(ctx: &CortexContext, prompt: &str, parts: &mut Vec<String>) {
    let rlm = match &ctx.rlm {
        Some(r) => r,
        None => return,
    };
    let query: String = prompt.chars().take(100).collect();
    if let Ok(matches) = rlm.search(&query, None, 3) {
        for m in &matches {
            let snippet: String = m.value.chars().take(80).collect();
            parts.push(format!("[rlm] {snippet}"));
        }
    }
}

/// QTable/Wilson best action suggestions.
fn inject_wilson_suggestions(ctx: &CortexContext, parts: &mut Vec<String>) {
    let persistence = match &ctx.persistence {
        Some(p) => p,
        None => return,
    };
    let events = persistence.load_hook_events_since(0, 1);
    if events.is_empty() {
        return;
    }
    if let Ok(top) = persistence.wilson_top_k(2) {
        for (id, score, _, _) in &top {
            if *score > 0.6 {
                parts.push(format!("RL top: {id} (Q={score:.1})"));
            }
        }
    }
}

// ══════════════════════════════════════════════════════════════════════
// H43: Crystallizer — PreCompact learning data crystallization
// ══════════════════════════════════════════════════════════════════════

/// Crystallize session learnings before context compaction.
pub struct CrystallizerHandler;

impl Handler for CrystallizerHandler {
    fn name(&self) -> &str {
        "crystallizer"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PreCompact]
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let persistence = match &ctx.persistence {
            Some(p) => p,
            None => return HandlerResult::skip(self.name()),
        };

        if ctx.context_budget_remaining < 60 {
            return HandlerResult::skip(self.name());
        }

        // Aggregate last 2h of events
        let events = persistence.recent_hook_events(7200).unwrap_or_default();
        if events.is_empty() {
            return HandlerResult::skip(self.name());
        }

        // Tool counts
        let mut tool_counts: std::collections::HashMap<String, usize> =
            std::collections::HashMap::new();
        let mut total_reward = 0.0f64;
        for (_, _, tool, reward) in &events {
            *tool_counts.entry(tool.clone()).or_default() += 1;
            total_reward += reward;
        }

        let success_rate = if !events.is_empty() {
            let positive = events.iter().filter(|(_, _, _, r)| *r > 0.0).count();
            (positive as f64 / events.len() as f64 * 100.0).round()
        } else {
            0.0
        };

        // Top 3 tools by usage
        let mut sorted_tools: Vec<_> = tool_counts.iter().collect();
        sorted_tools.sort_by(|a, b| b.1.cmp(a.1));
        let top_tools: Vec<String> = sorted_tools
            .iter()
            .take(3)
            .map(|(t, c)| format!("{t}:{c}"))
            .collect();

        // Drift check
        let mut drift_info = String::new();
        if let Ok(Some((trend, _mean, _std))) = persistence.drift_detect("reward_distribution") {
            if trend.abs() > 0.1 {
                let dir = if trend > 0.0 { "↑" } else { "↓" };
                drift_info = format!(" DRIFT:reward{dir}{trend:.2}");
            }
        }

        // Wilson top patterns
        let wilson_info = persistence
            .wilson_top_k(2)
            .unwrap_or_default()
            .iter()
            .map(|(id, score, _, _)| format!("{id}({score:.2})"))
            .collect::<Vec<_>>()
            .join(",");

        let crystal = format!(
            "tools=[{}] | events={} | success={success_rate:.0}%{drift_info} | top=[{wilson_info}]",
            top_tools.join(" "),
            events.len(),
        );

        // Store crystal in RlmMemory (survives compaction)
        if let Some(ref rlm) = ctx.rlm {
            let _ = rlm.store(
                &format!("session_crystal:{}", ctx.session_id),
                touring_intelligence::rl::memory::rlm::MemoryTier::Working,
                &crystal,
                Some("session_crystal"),
                None,
            );
        }

        let _ = persistence.log_hook_event("PreCompact", "Crystallizer", &crystal, 0.5);

        HandlerResult::allow(self.name(), Some(crystal))
    }
}

/// Register intelligence handlers.
pub fn register(pipeline: &mut Pipeline) {
    pipeline.register(Box::new(AcoPhase0Handler));
    pipeline.register(Box::new(PromptRecallHandler));
    pipeline.register(Box::new(McpRecommendHandler));
    pipeline.register(Box::new(TouringMemoryInjectorHandler));
    pipeline.register(Box::new(CrystallizerHandler));
}

// ══════════════════════════════════════════════════════════════════════
// MCP Recommend Handler — suggests touring tools for specific tool calls
// ══════════════════════════════════════════════════════════════════════

/// Recommends touring MCP tools when the LLM is about to use a native tool
/// that has a SUPERIOR touring equivalent.
///
/// The LLM habitually uses Read/Edit/Grep because they are familiar, but
/// touring tools offer AST-aware operations that are more precise:
///
/// | Native Tool | Touring Superior | Why |
/// |-------------|-----------------|-----|
/// | Grep/Glob   | touring_ast_find | AST symbol search vs text match |
/// | Edit        | touring_ast_edit | replace_body via AST, no old_string needed |
/// | Edit        | touring_refactor | rename with blast_radius + syntax validation |
/// | Edit(risky) | touring_speculate | shadow workspace validation before commit |
/// | TaskCreate  | touring_decompose | formal DAG with dependency validation |
pub struct McpRecommendHandler;

impl Handler for McpRecommendHandler {
    fn name(&self) -> &str {
        "mcp_recommend"
    }

    fn events(&self) -> &[HookEvent] {
        &[HookEvent::PreToolUse]
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        let tool = ctx.tool_name.as_deref().unwrap_or("");

        match tool {
            // ── Grep/Glob → touring_ast_find when searching for code symbols ──
            "Grep" | "Glob" => {
                let pattern = ctx.tool_input
                    .get("pattern")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                // Detect symbol-like patterns (snake_case, CamelCase, no special regex)
                if looks_like_symbol_name(pattern) {
                    return HandlerResult::allow(
                        self.name(),
                        Some(format!(
                            "MCP_RECOMMEND: touring_ast_find(symbol_name='{}', definitions_only=true) — AST-aware search, more precise than grep",
                            truncate_str(pattern, 40)
                        )),
                    );
                }
                HandlerResult::skip(self.name())
            }

            // ── Edit/Write → touring_ast_edit or touring_refactor ──
            "Edit" | "MultiEdit" => {
                let old_str = ctx.tool_input
                    .get("old_string")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let new_str = ctx.tool_input
                    .get("new_string")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");

                // Case 1: Entire function replacement (def/fn keyword in both old and new)
                if is_function_edit(old_str, new_str) {
                    return HandlerResult::allow(
                        self.name(),
                        Some("MCP_RECOMMEND: touring_ast_edit(action=replace_body) — AST surgery replaces function body without needing exact old_string"
                            .to_string()),
                    );
                }

                // Case 2: Rename pattern (same structure, different identifier)
                if looks_like_rename(old_str, new_str) {
                    return HandlerResult::allow(
                        self.name(),
                        Some("MCP_RECOMMEND: touring_refactor(action=rename) — rename with blast_radius + syntax validation"
                            .to_string()),
                    );
                }

                // Case 3: High-risk file
                if let Some(ref fp) = ctx.file_path {
                    let risk = ctx.knowledge.file_risk_score(fp);
                    if risk >= 0.5 {
                        return HandlerResult::allow(
                            self.name(),
                            Some(format!(
                                "MCP_RECOMMEND: touring_speculate — shadow-validate this edit (file_risk={:.0}%)",
                                risk * 100.0
                            )),
                        );
                    }
                }
                HandlerResult::skip(self.name())
            }

            // ── TaskCreate → touring_decompose ──
            "TaskCreate" => {
                HandlerResult::allow(
                    self.name(),
                    Some("MCP_RECOMMEND: touring_decompose — formal DAG with dependencies, validated contracts".to_string()),
                )
            }

            // ── EnterPlanMode → touring_mcts_search ──
            "EnterPlanMode" => {
                HandlerResult::allow(
                    self.name(),
                    Some("MCP_RECOMMEND: touring_mcts_search — multi-step planning via Monte Carlo Tree Search with UCB1".to_string()),
                )
            }

            // ── Agent → touring_session for subagent context ──
            "Agent" => {
                HandlerResult::allow(
                    self.name(),
                    Some("MCP_RECOMMEND: touring_session(action=status) — check session state before spawning subagent".to_string()),
                )
            }

            _ => HandlerResult::skip(self.name()),
        }
    }
}

/// Heuristic: does this pattern look like a code symbol name?
/// Matches: snake_case, CamelCase, UPPER_CASE identifiers.
/// Does NOT match: regex patterns with special chars.
fn looks_like_symbol_name(pattern: &str) -> bool {
    if pattern.is_empty() || pattern.len() < 3 {
        return false;
    }
    // Must start with letter or underscore
    let first = pattern.chars().next().unwrap_or(' ');
    if !first.is_alphabetic() && first != '_' {
        return false;
    }
    // Must be mostly alphanumeric + underscore (no regex special chars)
    let alnum_ratio = pattern
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '_' || *c == '.')
        .count() as f64
        / pattern.len() as f64;
    alnum_ratio >= 0.85
}

/// Heuristic: does this edit look like a function replacement?
fn is_function_edit(old_str: &str, new_str: &str) -> bool {
    let fn_keywords = ["def ", "fn ", "async fn ", "function ", "pub fn "];
    let old_has_fn = fn_keywords.iter().any(|kw| old_str.contains(kw));
    let new_has_fn = fn_keywords.iter().any(|kw| new_str.contains(kw));
    // Both have function keyword AND old is at least 3 lines
    old_has_fn && new_has_fn && old_str.lines().count() >= 3
}

/// Heuristic: does this edit look like a rename?
/// (same structure, one identifier changed)
fn looks_like_rename(old_str: &str, new_str: &str) -> bool {
    if old_str.is_empty() || new_str.is_empty() {
        return false;
    }
    // Simple check: similar length, few character differences
    let len_ratio =
        old_str.len().min(new_str.len()) as f64 / old_str.len().max(new_str.len()) as f64;
    if len_ratio < 0.8 {
        return false;
    }
    // Count differing characters (approximate edit distance)
    let diffs = old_str
        .chars()
        .zip(new_str.chars())
        .filter(|(a, b)| a != b)
        .count();
    let total = old_str.len().max(new_str.len());
    // Less than 20% of characters changed → likely a rename
    diffs > 0 && (diffs as f64 / total as f64) < 0.2
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

    // ── AcoPhase0Handler ────────────────────────────────────────────

    #[test]
    fn test_aco_detect_aco_flag() {
        assert!(detect_aco_mode("execute --aco completo"));
    }

    #[test]
    fn test_aco_detect_swarm_flag() {
        assert!(detect_aco_mode("run --swarm mode"));
    }

    #[test]
    fn test_aco_detect_agents_team() {
        assert!(detect_aco_mode("start --agents-team analysis"));
    }

    #[test]
    fn test_aco_detect_orquestracao_completa() {
        assert!(detect_aco_mode("quero orquestra\u{00e7}\u{00e3}o completa"));
    }

    #[test]
    fn test_aco_detect_orquestracao_no_accent() {
        assert!(detect_aco_mode("quero orquestracao completa"));
    }

    #[test]
    fn test_aco_detect_ciclo_aco() {
        assert!(detect_aco_mode("iniciar ciclo aco agora"));
    }

    #[test]
    fn test_aco_detect_7_fases() {
        assert!(detect_aco_mode("execute as 7 fases"));
    }

    #[test]
    fn test_aco_detect_fase_0() {
        assert!(detect_aco_mode("come\u{00e7}ar pela fase 0"));
    }

    #[test]
    fn test_aco_detect_intentspec() {
        assert!(detect_aco_mode("criar IntentSpec para o projeto"));
    }

    #[test]
    fn test_aco_detect_objectivespec() {
        assert!(detect_aco_mode("gerar ObjectiveSpec completo"));
    }

    #[test]
    fn test_aco_detect_case_insensitive() {
        assert!(detect_aco_mode("EXECUTE --ACO COMPLETO"));
        assert!(detect_aco_mode("Ciclo ACO agora"));
        assert!(detect_aco_mode("INTENTSPEC"));
    }

    #[test]
    fn test_aco_no_match_normal() {
        assert!(!detect_aco_mode("hello world"));
        assert!(!detect_aco_mode("analyze this file"));
    }

    #[test]
    fn test_aco_no_partial_match() {
        assert!(!detect_aco_mode("taco bell"));
    }

    #[test]
    fn test_aco_handler_injects_guidance() {
        let input = serde_json::json!({"prompt": "execute --aco completo"});
        let (_tmp, mut ctx) = make_test_ctx(HookEvent::UserPromptSubmit, input);
        let handler = AcoPhase0Handler;
        let result = handler.execute(&mut ctx);
        assert!(matches!(result.decision, Decision::Allow));
        assert!(!result.context_lines.is_empty());
        assert!(result.context_lines[0].contains("ACO MODE ATIVO"));
        assert!(result.context_lines[0].contains("Phase0(IntentSpec)"));
    }

    #[test]
    fn test_aco_handler_skip_normal_prompt() {
        let input = serde_json::json!({"prompt": "hello world"});
        let (_tmp, mut ctx) = make_test_ctx(HookEvent::UserPromptSubmit, input);
        let handler = AcoPhase0Handler;
        let result = handler.execute(&mut ctx);
        assert!(matches!(result.decision, Decision::Skip));
    }

    #[test]
    fn test_aco_handler_skip_empty() {
        let input = serde_json::json!({"prompt": ""});
        let (_tmp, mut ctx) = make_test_ctx(HookEvent::UserPromptSubmit, input);
        let handler = AcoPhase0Handler;
        let result = handler.execute(&mut ctx);
        assert!(matches!(result.decision, Decision::Skip));
    }

    #[test]
    fn test_aco_guidance_contains_phases() {
        assert!(ACO_GUIDANCE.contains("Phase0(IntentSpec)"));
        assert!(ACO_GUIDANCE.contains("Phase6(Persist)"));
        assert!(ACO_GUIDANCE.contains("Rollback"));
    }

    // ── PromptRecallHandler ─────────────────────────────────────────

    #[test]
    fn test_extract_keywords_filters_stops() {
        let kw = extract_keywords("the quick brown fox jumps over the lazy dog", 10);
        assert!(!kw.contains(&"the".to_string()));
        assert!(!kw.contains(&"over".to_string()));
        assert!(kw.contains(&"quick".to_string()));
    }

    #[test]
    fn test_extract_keywords_dedup() {
        let kw = extract_keywords("ruff ruff pyright ruff", 10);
        assert_eq!(kw.iter().filter(|w| *w == "ruff").count(), 1);
    }

    #[test]
    fn test_extract_keywords_max_limit() {
        let kw = extract_keywords(
            "alpha bravo charlie delta echo foxtrot golf hotel india juliet",
            5,
        );
        assert!(kw.len() <= 5);
    }

    #[test]
    fn test_prompt_recall_handler_skip_empty() {
        let input = serde_json::json!({"prompt": ""});
        let (_tmp, mut ctx) = make_test_ctx(HookEvent::UserPromptSubmit, input);
        let handler = PromptRecallHandler;
        let result = handler.execute(&mut ctx);
        assert!(matches!(result.decision, Decision::Skip));
    }

    #[test]
    fn test_prompt_recall_handler_skip_no_matches() {
        let input = serde_json::json!({"prompt": "run ruff and pyright validation"});
        let (_tmp, mut ctx) = make_test_ctx(HookEvent::UserPromptSubmit, input);
        let handler = PromptRecallHandler;
        let result = handler.execute(&mut ctx);
        // No knowledge in DB, so skip
        assert!(matches!(result.decision, Decision::Skip));
    }

    #[test]
    fn test_prompt_recall_handler_skip_single_match() {
        // S0.2: MIN_KEYWORD_MATCHES = 2, so a single match should skip
        let input = serde_json::json!({"prompt": "check the ruff status"});
        let (_tmp, mut ctx) = make_test_ctx(HookEvent::UserPromptSubmit, input);

        ctx.knowledge
            .upsert(&touring_hooks::knowledge::FileKnowledge {
                file_path: "ruff".to_string(),
                notes: Some("ruff config has strict mode enabled".to_string()),
                ..Default::default()
            })
            .unwrap();

        let handler = PromptRecallHandler;
        let result = handler.execute(&mut ctx);
        // Only 1 snippet — below MIN_KEYWORD_MATCHES threshold
        assert!(matches!(result.decision, Decision::Skip));
    }

    #[test]
    fn test_prompt_recall_handler_with_multiple_matches() {
        // S0.2: Need ≥2 matches to emit recall
        let input = serde_json::json!({"prompt": "check ruff and cargo status"});
        let (_tmp, mut ctx) = make_test_ctx(HookEvent::UserPromptSubmit, input);

        ctx.knowledge
            .upsert(&touring_hooks::knowledge::FileKnowledge {
                file_path: "ruff".to_string(),
                notes: Some("ruff config has strict mode enabled".to_string()),
                ..Default::default()
            })
            .unwrap();
        ctx.knowledge
            .upsert(&touring_hooks::knowledge::FileKnowledge {
                file_path: "cargo".to_string(),
                notes: Some("cargo workspace with 8 crates".to_string()),
                ..Default::default()
            })
            .unwrap();

        let handler = PromptRecallHandler;
        let result = handler.execute(&mut ctx);
        assert!(matches!(result.decision, Decision::Allow));
        assert!(!result.context_lines.is_empty());
        assert!(result.context_lines[0].contains("Touring:"));
    }

    #[test]
    fn test_prompt_recall_handler_truncates_output() {
        // S0.2: Output must be ≤ MAX_RECALL_CHARS + "..."
        let input = serde_json::json!({"prompt": "check ruff and cargo status"});
        let (_tmp, mut ctx) = make_test_ctx(HookEvent::UserPromptSubmit, input);

        // Add verbose notes to trigger truncation
        ctx.knowledge
            .upsert(&touring_hooks::knowledge::FileKnowledge {
                file_path: "ruff".to_string(),
                notes: Some("x".repeat(100)),
                ..Default::default()
            })
            .unwrap();
        ctx.knowledge
            .upsert(&touring_hooks::knowledge::FileKnowledge {
                file_path: "cargo".to_string(),
                notes: Some("y".repeat(100)),
                ..Default::default()
            })
            .unwrap();

        let handler = PromptRecallHandler;
        let result = handler.execute(&mut ctx);
        if matches!(result.decision, Decision::Allow) {
            // MAX_RECALL_CHARS (150) + "..." (3) = 153 max
            assert!(result.context_lines[0].len() <= MAX_RECALL_CHARS + 3);
        }
    }

    #[test]
    fn test_prompt_recall_handler_with_bash_failures() {
        let input = serde_json::json!({"prompt": "run cargo build and ruff check"});
        let (_tmp, mut ctx) = make_test_ctx(HookEvent::UserPromptSubmit, input);

        // Add two failed bash outcomes to meet MIN_KEYWORD_MATCHES
        ctx.knowledge
            .record_bash_outcome(&touring_hooks::knowledge::BashOutcome {
                command: "cargo build".to_string(),
                command_short: "cargo".to_string(),
                command_hash: String::new(),
                exit_code: 1,
                success: false,
                error_pattern: Some("missing lifetime specifier".to_string()),
                file_context: None,
                executed_at: "2026-03-14".to_string(),
            })
            .unwrap();
        ctx.knowledge
            .record_bash_outcome(&touring_hooks::knowledge::BashOutcome {
                command: "ruff check".to_string(),
                command_short: "ruff".to_string(),
                command_hash: String::new(),
                exit_code: 1,
                success: false,
                error_pattern: Some("E501 line too long".to_string()),
                file_context: None,
                executed_at: "2026-03-14".to_string(),
            })
            .unwrap();

        let handler = PromptRecallHandler;
        let result = handler.execute(&mut ctx);
        assert!(matches!(result.decision, Decision::Allow));
        assert!(!result.context_lines.is_empty());
        assert!(result.context_lines[0].contains("Touring:"));
    }

    #[test]
    fn test_prompt_recall_is_async() {
        assert!(PromptRecallHandler.is_async());
    }
}
