//! Pre-Read Hook — HIGH-SIGNAL-ONLY context injection.
//!
//! Philosophy: SILENCE IS THE DEFAULT.
//! Only inject context when it passes the "So what?" test:
//! - Is it actionable? (Claude can DO something differently)
//! - Is it non-derivable? (Claude WON'T find this by reading the file)
//! - Is it specific to THIS file? (Not generic information)
//!
//! What we DO inject:
//! - Notes/gotchas: accumulated experience NOT visible in the file
//! - Bash failures on THIS file: "ruff failed here" changes Claude's approach
//! - Dependents (≥2): "3 files import this" → Claude checks impact
//!
//! What we DO NOT inject:
//! - Language, line count, symbol count (Claude sees this in 0.5s)
//! - Import list (Claude reads the imports themselves)
//! - Read frequency (irrelevant to current decision)
//! - Generic metadata that Claude discovers by reading
//!
//! Target latency: <10ms.

use super::knowledge::FileKnowledgeDB;
use super::runtime::traits::KnowledgeDB;
use super::runtime::{HookResponse, HookRuntime, make_relative};
use crate::schemas::{validate_payload, validation_deny};
use crate::shared::gate_metrics;
use crate::shared::hook_helpers;
use crate::shared::metadata_collector::TokenBudget;
use crate::shared::parser_cache_global::global_cache;
use crate::shared::result_ext::{OptionExt, ResultExt};
use crate::shared::signal_pipeline::{
    SignalContext, SignalPipeline, StaticSignalLayer, build_graph_pipeline,
};
use crate::shared::signals::{
    assemble_scored_context, blast_radius_signal, enrich_with_cognitive, normalize_scores,
    score_cmp,
};
use crate::shared::span_context::timestamp_us;
use std::borrow::Cow;
use touring_code::ast::{ModuleTree, SymbolIndex, SymbolStore, build_scope_map};
use touring_foundation::mvkl::FileIndex;
use touring_foundation::truncate_str;
use touring_simd::{DriftDetection, DriftDetector};

/// Returns the context budget (in characters) for a given CILA level.
///
/// Budget can be overridden via environment variables:
/// - `TOURING_CILA_BUDGET_L0`, `TOURING_CILA_BUDGET_L1`: L0-L1 default = 800
/// - `TOURING_CILA_BUDGET_L2`, `TOURING_CILA_BUDGET_L3`: L2-L3 default = 2000
/// - `TOURING_CILA_BUDGET_L4`, `TOURING_CILA_BUDGET_L5`, `TOURING_CILA_BUDGET_L6`: L4+ default = 4000
///
/// If the env var is set but invalid, falls back to the default.
/// Delegates to [`crate::shared::cila::cila_budget_read`].
fn cila_budget(cila_level: u8) -> usize {
    crate::shared::cila::cila_budget_read(cila_level)
}

/// Quality threshold above which we boost context injection confidence.
const QUALITY_HIGH_THRESHOLD: f64 = 0.80;
/// Quality threshold below which we inject a warning signal.
const QUALITY_LOW_THRESHOLD: f64 = 0.50;

/// Build metacognitive quality context from HookQualityAssessment.
///
/// # Arguments
/// * `runtime` — HookRuntime with access to quality_assessment
/// * `max_chars` — Budget cap for the quality signal
///
/// # Returns
/// A quality signal string if assessment exists, or `None`.
///
/// Philosophy: When hook quality is HIGH, Claude should be more receptive to
/// context injection. When quality is LOW, Claude should be more skeptical.
/// This is metacognitive awareness for the pre-read hook.
fn build_quality_context(runtime: &HookRuntime, max_chars: usize) -> Option<String> {
    let qa = runtime.quality_assessment()?;

    // Use D6 Reliability (success rate) as the primary quality signal.
    // Falls back to precision (D1) if no hooks have fired yet.
    let total = qa.streaming_stats.total();
    let quality_score = if total == 0 {
        // No hooks fired yet — be optimistic, inject quality signal with neutral score.
        // This is the warm-up phase; quality tracking is just starting.
        1.0
    } else {
        qa.streaming_stats.success_rate()
    };

    // Build the quality signal based on threshold.
    // Format: "quality: <label> <score> [hooks:N]" where label is HIGH/LOW/MEDIUM.
    let (_label, color_signal) = if quality_score >= QUALITY_HIGH_THRESHOLD {
        (
            "HIGH",
            "quality: high {:.2} [hooks:{}]".replace("{:.2}", &format!("{:.2}", quality_score)),
        )
    } else if quality_score < QUALITY_LOW_THRESHOLD {
        (
            "LOW",
            format!("quality: LOW {:.2} [hooks:{}]", quality_score, total),
        )
    } else {
        (
            "MEDIUM",
            format!("quality: {:.2} [hooks:{}]", quality_score, total),
        )
    };

    // Truncate if needed to fit budget.
    let signal: &str = if color_signal.len() > max_chars {
        truncate_str(&color_signal, max_chars)
    } else {
        &color_signal
    };

    // When quality is LOW, also append a warning to be more conservative with context.
    // This helps Claude understand why context might be sparse or uncertain.
    if quality_score < QUALITY_LOW_THRESHOLD && total > 3 {
        // Only warn if we've seen enough hooks to establish a pattern.
        let warning = " (low quality - context may be unreliable)";
        let rem = max_chars.saturating_sub(signal.len());
        if warning.len() < rem {
            return Some(format!("{}{}", signal, warning));
        }
    }

    Some(signal.to_string())
}

/// Run the pre-read hook (diverging version — for use by the CLI entry point).
#[tracing::instrument(skip(runtime, input), fields(hook = "pre_read"))]
pub fn run(
    runtime: &HookRuntime,
    input: &serde_json::Value,
) -> Result<(), touring_hook_runtime::hook_runtime::HookDispatchError> {
    run_returning(runtime, input).emit()
}

/// Run the pre-read hook, returning a `HookResponse` instead of diverging.
///
/// Used by the daemon to handle the hook without calling `process::exit`.
pub fn run_returning(runtime: &HookRuntime, input: &serde_json::Value) -> HookResponse {
    // Wave 13: Unified Observability — record pre_read enter/exit hop in span context.
    let enter_us = timestamp_us();
    let result = run_returning_impl(runtime, input);
    let exit_us = timestamp_us();
    runtime.record_span_layer("pre_read", enter_us, exit_us);
    result
}

fn run_returning_impl(runtime: &HookRuntime, input: &serde_json::Value) -> HookResponse {
    // D9: Validate payload with typed schema — fail fast on malformed input.
    // Extract /tool_input first since payload schemas model the inner object.
    let tool_input = match input.get("tool_input") {
        Some(v) => v,
        None => {
            return validation_deny(
                &{
                    let mut e = validator::ValidationErrors::new();
                    e.add(
                        "tool_input",
                        validator::ValidationError {
                            code: Cow::Borrowed("missing"),
                            message: Some(Cow::Borrowed("tool_input missing from input")),
                            params: std::collections::HashMap::new(),
                        },
                    );
                    e
                },
                "pre_read",
            );
        }
    };
    let validated = match validate_payload::<crate::schemas::PreReadPayload>(tool_input) {
        Ok(v) => v,
        Err(errors) => return validation_deny(&errors, "pre_read"),
    };
    let file_path = validated.file_path.as_str();

    if file_path.is_empty() {
        return HookResponse::Allow;
    }

    // ── Path normalization: resolve relative paths to absolute ──
    // If the file_path is relative, normalize it using CWD so downstream
    // tools always work with absolute paths. Returns ContextWithUpdatedInput
    // to both inject context AND correct the tool input.
    let (file_path, needs_updated_input) = if !file_path.starts_with('/') {
        match normalize_relative_path(file_path) {
            Some(abs) => (abs, true),
            None => (file_path.to_string(), false),
        }
    } else {
        (file_path.to_string(), false)
    };
    let file_path = file_path.as_str();

    // D4.1: Track consecutive file reads to detect bulk-read analysis patterns.
    // Increment counter on each pre_read invocation; reset if file has skip markers.
    let skip_marker_detected = std::fs::read_to_string(file_path)
        .map(|content| content.contains("touring:skip") || content.contains("touring::skip"))
        .unwrap_or(false);

    // D4.1: Track consecutive file reads to detect bulk-read analysis patterns.
    // Increment counter on each pre_read invocation; reset if file has skip markers.
    let consecutive_file_reads = {
        let mut bus = runtime.ctx.session_bus.borrow_mut();
        if skip_marker_detected {
            bus.consecutive_file_reads = 0;
        } else {
            bus.consecutive_file_reads += 1;
        }
        bus.consecutive_file_reads
    };

    // FileParserCache: warm up the per-file pipeline entry so callers
    // find an already-initialised SharedPipeline. Graceful: errors are
    // silently ignored — never blocks the hook.
    {
        let cache = global_cache();
        let path_buf = std::path::PathBuf::from(file_path);
        let _pipeline = cache.get_or_create(&path_buf);
    }

    let rel_path = make_relative(file_path, &runtime.project_root);

    // S-26: Record file access in HeatMap for read frequency tracking
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as f64)
        .unwrap_or(0.0);
    if let Ok(mut hm) = runtime.learning.heat_map.try_borrow_mut() {
        hm.record_access(file_path, now);
    }
    // EC7: Fire-and-forget async record_access to AsyncFileKnowledgeDB.
    // Captures file access in the async knowledge DB alongside the HeatMap.
    // session_id from CLAUDE_SESSION_ID env var — empty string if unavailable.
    if let Some(adb) = runtime.ctx.async_knowledge.as_ref().cloned()
        && let Ok(handle) = tokio::runtime::Handle::try_current()
    {
        let path = rel_path.to_string();
        let sid = std::env::var("CLAUDE_SESSION_ID").unwrap_or_default();
        drop(handle.spawn(async move {
            let _ = adb.record_access(&path, &sid).await;
        }));
    }

    // S6/E19: Read session CILA level — prefer stable session context,
    // fall back to result_cache for standalone/cold-start mode.
    let cila_level: u8 = hook_helpers::cila_level_from_runtime(runtime, 3);

    let max_chars = cila_budget(cila_level);

    // Layer 1: DB + symbol-map + cognitive signals (fast, sequential)
    let mut context = build_db_context(
        runtime,
        &rel_path,
        file_path,
        max_chars,
        consecutive_file_reads,
    );

    // TokenBudget: per-file token cap for context injection.
    // Converts char-based CILA budget to a token estimate (chars / 4).
    // Gates Layer 2+ from running when Layer 1 already exhausted the token cap.
    let mut token_budget = TokenBudget::pre_read();
    let layer1_tokens = TokenBudget::estimate_from_chars(
        context
            .as_ref()
            .map(|c| c.len())
            .unwrap_or_debug(0, "pre_read: context length fallback"),
    );
    let _ = token_budget.allocate(layer1_tokens);

    // Layer 2: AST/source + parallel index signals via SignalPipeline.
    // DB context already assembled above; remaining budget fed to the pipeline.
    let remaining = remaining_budget(&context, max_chars);
    if remaining > 0
        && token_budget.has_remaining()
        && let Some(extra) = build_parallel_signal_pipeline(
            runtime,
            file_path,
            &rel_path,
            remaining,
            cila_level as usize,
        )
    {
        let extra_tokens = TokenBudget::estimate_from_chars(extra.len());
        if token_budget.allocate(extra_tokens) > 0 {
            append_to_context(&mut context, extra);
        } else {
            // Budget exhausted — signal pipeline output dropped.
            gate_metrics::record_metadata_backpressure_dropped();
        }
    }

    // FA-1: SessionBus signal injection — record file read + cache blast radius.
    // Flows to pre_edit (via blast_radius_cache) and post_tool_rl (last_read_file),
    // completing the perception → execution path of the Fasciculus Arcuatus pattern.
    {
        let blast_count = runtime
            .infra
            .symbol_index
            .as_ref()
            .map(|idx| idx.blast_radius_with_depth(&rel_path, 4).file_count)
            .unwrap_or_debug(0, "pre_read: blast_radius fallback");
        let mut bus = runtime.ctx.session_bus.borrow_mut();
        bus.signal_file_read(rel_path.clone(), None);
        bus.cache_blast_radius(&rel_path, blast_count);

        // FA-1 ANN: pre-warm context by searching for similar past files.
        // Uses path-hash embedding to find semantically related files
        // from ANN memory, then caches results in SessionBus for pre_edit.
        if let Some(ref ann_recall) = *runtime.ctx.ann_recall.borrow()
            && !ann_recall.is_empty()
        {
            let query_emb = crate::ann_memory::path_hash_embedding(&rel_path);
            let ann_results = ann_recall.search(&query_emb, 5);
            if !ann_results.is_empty() {
                // Convert MemorySearchResult → SearchResult for SessionBus caching.
                let cacheable: Vec<_> = ann_results
                    .into_iter()
                    .map(|r| crate::ann_memory::SearchResult::new(r.id, r.score))
                    .collect();
                bus.cache_ann_results(&rel_path, cacheable);
            }
        }
    }
    // Layer 4: Metacognitive quality signal — modulates context based on HookQualityAssessment.
    // When quality is HIGH, context injection is more trustworthy.
    // When quality is LOW, inject a warning so Claude adjusts confidence.
    // Budget guard: 60 chars minimum to fit a meaningful quality signal.
    let remaining = remaining_budget(&context, max_chars);
    if remaining > 60
        && let Some(quality_signal) = build_quality_context(runtime, remaining)
    {
        append_to_context(&mut context, quality_signal);
    }

    // Layer 3: S4 Drift detection — compare baseline error rates vs current session.
    // High divergence indicates environmental or codebase changes affecting reliability.
    if let Ok(baseline_rates) = runtime.ctx.knowledge.error_rate_history() {
        let current_rate = 1.0 - runtime.ctx.knowledge.bash_success_rate();
        if let Some((_score, msg)) = error_rate_drift_signal(&baseline_rates, current_rate) {
            append_to_context(&mut context, msg);
        }
    }

    // Layer 5 (S-03, SNR slice): plan-scope — when THIS file belongs to an active
    // loop-engineering plan, surface the plan + the command for its ready subtasks.
    maybe_inject_plan_scope(&mut context, &rel_path, &runtime.project_root, max_chars);

    match context {
        Some(ctx) if !ctx.is_empty() => {
            // S-00.5 (SNR slice): instrument the ACTUAL pre_read enrichment payload
            // so the STR proxy measures content injection, not just cli_suggester
            // nudges (R4) + budget-pressure at 75%/90% (R3). Fail-open counters.
            record_enrichment_metrics(ctx.len(), max_chars);
            // S7: Log the file for which context was injected.
            // post_tool_rl reads this to correlate context utility with tool success.
            runtime.ctx.result_cache.cache_result(
                "__meta__",
                "__context_injection_file__",
                rel_path.to_string(),
            );
            let response = build_read_response(ctx.clone(), needs_updated_input, input, file_path);
            // A8: Store hook result for chaining via SessionBus hook_results.
            // post_read and other hooks can retrieve it via get_last_hook_result("pre_read").
            {
                let mut bus = runtime.ctx.session_bus.borrow_mut();
                let result_json = serde_json::json!({
                    "file_path": rel_path,
                    "context_len": ctx.len(),
                });
                bus.add_hook_result("pre_read", result_json);
            }
            response
        }
        _ if needs_updated_input => {
            let fallback_ctx = format!("Path normalized: {} -> {}", rel_path, file_path);
            build_read_response(fallback_ctx, true, input, file_path)
        }
        _ => HookResponse::Allow,
    }
}

/// Record the STR proxy + budget-pressure signals for an emitted enrichment
/// payload (S-00.5). Extracted from `run_returning_impl` to keep its complexity
/// bounded. `record_enrichment_emitted` makes `mean_enrichment_bytes` (the STR
/// proxy) measure the real pre_read payload (R4); the 75%/90% ratio checks emit
/// budget-pressure signals (R3), mirroring the MCP `ctx_*` thresholds. Fail-open:
/// every recorder is an infallible atomic counter.
fn record_enrichment_metrics(emitted: usize, max_chars: usize) {
    touring_foundation::gate_metrics_snapshot::record_enrichment_emitted(emitted);
    if max_chars == 0 {
        return;
    }
    let ratio = emitted as f64 / max_chars as f64;
    if ratio >= 0.90 {
        touring_foundation::gate_metrics_snapshot::record_ctx_budget_alert();
    } else if ratio >= 0.75 {
        touring_foundation::gate_metrics_snapshot::record_ctx_budget_warning();
    }
}

/// S-03 call-site wrapper (0 branches, so `run_returning_impl` keeps its
/// pre-existing complexity). Budget-gates the plan-scope line and appends it.
fn maybe_inject_plan_scope(
    context: &mut Option<String>,
    rel_path: &str,
    project_root: &std::path::Path,
    max_chars: usize,
) {
    let remaining = remaining_budget(context, max_chars);
    if let Some(signal) = plan_scope_signal(rel_path, project_root, remaining) {
        append_to_context(context, signal);
    }
}

/// S-03 (SNR slice): plan-scope injection. When the file being read belongs to
/// an active loop-engineering plan — a `decompose` DAG tracked by a per-project
/// marker under `~/.claude/loop-engineering/active-<hash>.json` — inject one
/// dense line naming the plan and the command to see its ready subtasks. This
/// re-surfaces "where am I in the plan" when a plan file is reopened: the
/// LangGraph namespace-scoped-retrieval pattern applied to file focus.
///
/// Silence-default: no active marker covers this file → `None` (no injection).
/// Kill-switch: `TOURING_PLAN_SCOPE=0` disables it entirely. Fail-open: any I/O
/// or parse error yields `None`, never an error. No daemon RPC on the hot path —
/// the marker JSON already carries task + scope; the `next` action is the literal
/// command, not a pre-computed value. Requires at least ~80 chars of budget.
fn plan_scope_signal(
    rel_path: &str,
    project_root: &std::path::Path,
    max_chars: usize,
) -> Option<String> {
    if max_chars < 80 {
        return None;
    }
    // Kill-switch (a human decision — mirrors the slice's flag discipline).
    if std::env::var("TOURING_PLAN_SCOPE")
        .map(|v| v == "0" || v.eq_ignore_ascii_case("false"))
        .unwrap_or(false)
    {
        return None;
    }
    let home = std::env::var("HOME").ok()?;
    let dir = std::path::Path::new(&home)
        .join(".claude")
        .join("loop-engineering");
    let project_root_str = project_root.to_str().unwrap_or_default();
    for entry in std::fs::read_dir(&dir).ok()?.flatten() {
        let name = entry.file_name();
        if !is_canonical_active_marker(&name.to_string_lossy()) {
            continue;
        }
        let Ok(raw) = std::fs::read_to_string(entry.path()) else {
            continue;
        };
        let Ok(marker) = serde_json::from_str::<serde_json::Value>(&raw) else {
            continue;
        };
        let field = |k: &str| marker.get(k).and_then(|v| v.as_str()).unwrap_or_default();
        let (task, scope, cwd, status) =
            (field("task"), field("scope"), field("cwd"), field("status"));
        if marker_covers(rel_path, project_root_str, scope, cwd, status) {
            let line = format_plan_scope_line(task, scope);
            return Some(truncate_str(&line, max_chars).to_string());
        }
    }
    None
}

/// Is `name` the canonical active marker `active-<hex>.json` (NOT the archived /
/// converged variants `active-<hex>.archived.json` / `.converged.json`)? The
/// canonical name has no extra `.` between the `active-` prefix and `.json`.
fn is_canonical_active_marker(name: &str) -> bool {
    match name
        .strip_prefix("active-")
        .and_then(|s| s.strip_suffix(".json"))
    {
        Some(mid) => !mid.is_empty() && !mid.contains('.'),
        None => false,
    }
}

/// Pure coverage test: does an active marker (`scope`, `cwd`, `status`) cover the
/// file at `rel_path` (relative to `project_root`)? A relative marker scope is
/// only honored when the marker's `cwd` matches this workspace (so a relative
/// scope from another project never false-matches). An absolute scope is matched
/// against the file's absolute path. Archived / converged markers never cover.
fn marker_covers(rel_path: &str, project_root: &str, scope: &str, cwd: &str, status: &str) -> bool {
    if scope.is_empty() {
        return false;
    }
    let up = status.to_ascii_uppercase();
    if up == "ARCHIVED" || up == "CONVERGED" {
        return false;
    }
    let scope = scope.trim_end_matches('/');
    if let Some(abs_scope) = scope.strip_prefix('/') {
        // Absolute scope: compare against the file's absolute path.
        let abs = format!("{}/{}", project_root.trim_end_matches('/'), rel_path);
        path_within(&abs, &format!("/{abs_scope}"))
    } else {
        // Relative scope: only valid within the marker's own workspace.
        cwd == project_root && path_within(rel_path, scope)
    }
}

/// Is `path` inside `base` at a path boundary (exact, or `base/…`)? Avoids the
/// `crates/touring-foo` vs `crates/touring-foobar` false prefix match.
fn path_within(path: &str, base: &str) -> bool {
    path == base
        || path
            .strip_prefix(base)
            .map(|rest| rest.starts_with('/'))
            .unwrap_or(false)
}

/// Format the single dense plan-scope line. The `next` action is the literal
/// command (not a daemon call): the reader runs it to see ready subtasks.
fn format_plan_scope_line(task: &str, scope: &str) -> String {
    format!(
        "📋 plan-scope: this file ∈ active loop `{task}` (scope `{scope}`) · next: `touring decompose ready {task}`"
    )
}

/// Build the appropriate `HookResponse` for the pre-read result.
///
/// When `needs_updated_input` is true, returns `ContextWithUpdatedInput` to
/// both inject context AND normalize the file path in the tool input.
/// Otherwise returns a plain `Context` response.
fn build_read_response(
    ctx: String,
    needs_updated_input: bool,
    input: &serde_json::Value,
    file_path: &str,
) -> HookResponse {
    if needs_updated_input {
        let mut updated = input
            .get("tool_input")
            .cloned()
            .unwrap_or_else(|| serde_json::json!({}));
        #[allow(clippy::indexing_slicing)]
        {
            updated["file_path"] = serde_json::Value::String(file_path.to_string());
        }
        HookResponse::ContextWithUpdatedInput {
            context: ctx,
            event_name: Some("PreToolUse".to_string()),
            updated_input: updated,
        }
    } else {
        HookResponse::Context {
            context: ctx,
            event_name: Some("PreToolUse".to_string()),
        }
    }
}

/// Normalize a relative file path to an absolute path.
///
/// Uses `std::fs::canonicalize` when the path exists on disk.
/// Falls back to joining with the current working directory when
/// canonicalization fails (e.g., path doesn't exist yet).
///
/// Returns `None` if even the CWD-based fallback fails.
fn normalize_relative_path(relative: &str) -> Option<String> {
    // Try canonicalize first — resolves symlinks, validates existence.
    if let Ok(abs) = std::fs::canonicalize(relative) {
        return abs.to_str().map(String::from);
    }

    // Fallback: join with CWD for paths that don't exist yet.
    if let Ok(cwd) = std::env::current_dir() {
        let joined = cwd.join(relative);
        return joined.to_str().map(String::from);
    }

    None
}

/// Build the fast, sequential DB-layer context:
/// knowledge DB signals → symbol map → cognitive enrichment.
///
/// Returns the assembled context string (or `None` if nothing to inject).
fn build_db_context(
    runtime: &HookRuntime,
    rel_path: &str,
    file_path: &str,
    max_chars: usize,
    consecutive_file_reads: u32,
) -> Option<String> {
    let mut context = compose_high_signal_context_budgeted(
        &runtime.ctx.knowledge,
        rel_path,
        max_chars,
        consecutive_file_reads,
    );

    // Symbol map: proactive navigation hint injected before reading.
    // Allows Claude to jump directly to the relevant symbol via Read { offset, limit }.
    // Prepended (highest priority) within budget.
    if let Some(sym_map) = build_symbol_map_signal(runtime.symbol_store(), rel_path) {
        prepend_symbol_map(&mut context, sym_map, max_chars);
    }

    // Cognitive enrichment: risk, gotchas, predicted co-edits
    if let Some(ref cognitive) = runtime.cognitive {
        let enriched = enrich_with_cognitive(cognitive, file_path, true);
        if !enriched.is_empty() {
            append_cognitive_section(&mut context, enriched);
        }
    }

    // Knowledge health: quick indicator of DB coverage and reliability.
    // Only injected when budget permits (> 80 chars remaining).
    // Uses touring-analysis knowledge dimension for a consistent score.
    let rem = remaining_budget(&context, max_chars);
    if rem > 80 {
        let know = touring_analysis::knowledge::analyze_knowledge(runtime.ctx.knowledge.conn_ref());
        if know.total_files > 0 {
            let signal = format!(
                "knowledge: {}/{} files indexed (score {:.2})",
                know.total_files, know.total_files, know.score,
            );
            if signal.len() + 3 <= rem {
                match context.as_mut() {
                    Some(ctx) => {
                        ctx.push_str(" | ");
                        ctx.push_str(&signal);
                    }
                    None => context = Some(signal),
                }
            }
        }
    }

    // A3: Composite health signal — wiring + quality + temporal dimensions before read.
    // Injected after the knowledge signal to provide a full-spectrum health view.
    // Budget guard rem2 > 120 is stricter than knowledge's 80 to protect <10ms SLA:
    // AnalysisPipeline::run() is heavier than analyze_knowledge().
    let rem2 = remaining_budget(&context, max_chars);
    if rem2 > 120 {
        let health = touring_analysis::AnalysisPipeline::new(
            runtime.ctx.knowledge.conn_ref(),
            touring_analysis::engine::AnalysisConfig::hook_path(),
        )
        .run(
            runtime
                .project_root
                .to_str()
                .unwrap_or_debug("", "pre_read: project_root fallback"),
        );
        let status_label = match health.status {
            touring_analysis::HealthStatus::Healthy => "healthy",
            touring_analysis::HealthStatus::Degraded => "degraded",
            touring_analysis::HealthStatus::Critical => "critical",
        };
        // Sort by score ascending so the worst dimensions appear first — most actionable.
        let mut dims = health.dimensions.clone();
        dims.sort_by(|a, b| {
            a.score
                .partial_cmp(&b.score)
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        let dim_summary: Vec<String> = dims
            .iter()
            .take(3)
            .map(|d| format!("{}:{:.2}", d.name, d.score))
            .collect();
        // Compact format: "health: degraded 0.71 [wiring:0.62 quality:0.80]"
        let signal = format!(
            "health: {status_label} {:.2} [{}]",
            health.composite_score,
            dim_summary.join(" "),
        );
        let rem3 = remaining_budget(&context, max_chars);
        if signal.len() + 3 <= rem3 {
            append_to_context(&mut context, signal);
        }
    }

    // ── Functional chain signal — broken / active chains for this file ──
    // Broken chains (weight 2.0) are the most critical: a previously-valid
    // data-flow path is now severed. Injected before Claude reads the file
    // so it understands the chain context from the first glance.
    // Budget guard: min 50 chars to fit a meaningful chain signal.
    let rem_fc = remaining_budget(&context, max_chars);
    if rem_fc > 50
        && let Some((_, chain_sig)) =
            crate::functional_wiring::functional_chain_signal(&runtime.ctx.knowledge, rel_path)
    {
        append_to_context(&mut context, chain_sig);
    }

    // ── S3: Error prediction — Markov-predicted error from edit history ──
    // Projects the most likely next error based on the sequence of edits recorded
    // in the knowledge DB. Injected pre-read so Claude can watch for the pattern
    // while reading rather than discovering it only after writing.
    let rem_ep = remaining_budget(&context, max_chars);
    if rem_ep > 60
        && let Some(ref predictor) = runtime.ctx.error_predictor
        && let Some(pred) = predictor.predict()
    {
        let pat_len = pred.error_pattern.len().min(60);
        let sig = format!(
            "⚠ predicted: {}% chance of '{}' ({}× observed)",
            (pred.probability * 100.0) as u32,
            &pred.error_pattern[..pat_len],
            pred.observations
        );
        let rem_ep2 = remaining_budget(&context, max_chars);
        if sig.len() + 3 <= rem_ep2 {
            append_to_context(&mut context, sig);
        }
    }

    // ── S4: Extended metadata — test coverage + community affinity ──
    // coverage_pct surfaces untested files before reading; community_id reveals
    // which module cluster this file belongs to for architectural awareness.
    let rem_ext = remaining_budget(&context, max_chars);
    if rem_ext > 40
        && let Ok(Some(ext)) = runtime.ctx.knowledge.query_extended(rel_path)
        && let Some(sig) = hook_helpers::build_file_meta_signal(&ext)
    {
        let rem_ext2 = remaining_budget(&context, max_chars);
        if sig.len() + 3 <= rem_ext2 {
            append_to_context(&mut context, sig);
        }
    }

    context
}

/// Prepend `sym_map` to `context` within `max_chars` budget.
///
/// Inserts as `"{sym_map} | {existing}"` when space permits.
/// Falls back to setting context = sym_map when context was `None` and map fits.
fn prepend_symbol_map(context: &mut Option<String>, sym_map: String, max_chars: usize) {
    match context.as_mut() {
        Some(ctx) if ctx.len() + 3 + sym_map.len() <= max_chars => {
            let existing = std::mem::take(ctx);
            *ctx = format!("{sym_map} | {existing}");
        }
        Some(_) => {} // budget exhausted — skip symbol map
        None => {
            if sym_map.len() <= max_chars {
                *context = Some(sym_map);
            }
        }
    }
}

/// Append `enriched` cognitive text to `context`, separated by ` | `.
fn append_cognitive_section(context: &mut Option<String>, enriched: String) {
    match context.as_mut() {
        Some(ctx) => {
            ctx.push_str(" | ");
            ctx.push_str(&enriched);
        }
        None => *context = Some(enriched),
    }
}

/// Compute how many characters remain in `max_chars` after accounting for `context`.
///
/// Reserves 3 bytes for the " | " separator when context is already present.
/// Pure function — no I/O.
fn remaining_budget(context: &Option<String>, max_chars: usize) -> usize {
    let used = context.as_ref().map_or(0, |c| c.len());
    let separator = if context.is_some() { 3 } else { 0 };
    max_chars.saturating_sub(used + separator)
}

/// Append `extra` to `context`, inserting a " | " separator when needed.
///
/// Pure function — no I/O.
fn append_to_context(context: &mut Option<String>, extra: String) {
    match context.as_mut() {
        Some(ctx) => {
            ctx.push_str(" | ");
            ctx.push_str(&extra);
        }
        None => *context = Some(extra),
    }
}

/// Build and execute a [`SignalPipeline`] for AST/parallel signals.
///
/// Collects two layers:
/// 1. **Index layer** (static): blast radius, external callers, similar symbols,
///    and L7 co-edit predictions — evaluated up-front because `SymbolIndex` is
///    not `'static` and cannot be captured in a closure.
/// 2. **Source layer** (fn): filesystem read + tree-sitter parsing, executed on
///    the hook thread pool so I/O overlaps with pipeline assembly.
///
/// Returns the assembled context string within `budget` chars, or `None`.
/// Collect index-layer signals in parallel: blast radius, external callers,
/// similar symbols, and L7 co-edit predictions.
///
/// These signals require `&SymbolIndex` which is not `'static`, so they must
/// be gathered before the `SignalPipeline` (which requires owned/`'static` data).
fn collect_index_signals(
    runtime: &HookRuntime,
    file_path: &str,
    rel_path: &str,
) -> Vec<(f32, String)> {
    use crate::shared::thread_pool::with_hook_pool;

    let idx_opt = runtime.infra.symbol_index.as_ref();
    let rp = rel_path.to_string();
    let ((blast_sig, callers_sig), similar_sig) = with_hook_pool(|| {
        rayon::join(
            || {
                rayon::join(
                    || blast_radius_signal(idx_opt, &rp, true),
                    || external_callers_signal(idx_opt, &rp),
                )
            },
            || crate::shared::signals::similar_symbol_signal_for_path(idx_opt, &rp),
        )
    });

    let mut signals: Vec<(f32, String)> = Vec::new();
    if let Some(s) = blast_sig {
        signals.push(s);
    }
    if let Some(s) = callers_sig {
        signals.push(s);
    }
    if let Some(s) = similar_sig {
        signals.push(s);
    }

    // ── Signal I-6: BFS blast radius enrichment — hop distances per affected file ──
    // Supplements the existing `blast_radius_signal` (count+max) with precise hop distances
    // via BlastRadiusEngine::bfs_only(). Budget: 8ms. Falls back to count-only silently.
    // Only fires when SymbolIndex is available and has reverse_deps for this file.
    if let Some(s) = bfs_hop_signal(idx_opt, rel_path) {
        signals.push(s);
    }

    // L7: co-edit predictions (owned data — safe to include here)
    let l7_preds = runtime.infra.prediction.predict_next(file_path, 3);
    if let Some(s) = build_l7_pred_signal(&l7_preds) {
        signals.push(s);
    }

    // E12: ACO pheromone-guided predictions from PredictiveFocusCache.
    // Use L7 prediction candidates as known_files for pheromone ranking,
    // falling back to session files when L7 predictions are empty.
    let pfc_candidates = if l7_preds.is_empty() {
        runtime.infra.prediction.session_files()
    } else {
        l7_preds.iter().map(|p| p.path.clone()).collect()
    };
    if let Some(s) = build_pfc_signal(&runtime.infra.predictive_focus, file_path, &pfc_candidates) {
        signals.push(s);
    }

    // ANN recall: surface related memories from persistent ANN index
    if let Some(s) = ann_recall_signal(runtime, file_path) {
        signals.push(s);
    }

    // Tantivy BM25: related docstrings from OTHER files (same module concepts)
    #[cfg(feature = "tantivy-fts")]
    if let Some(s) =
        crate::shared::signals::tantivy_related_docs_signal(Some(&runtime.project_root), rel_path)
    {
        signals.push(s);
    }

    // Tantivy fuzzy: similar file names across the project
    #[cfg(feature = "tantivy-fts")]
    if let Some(s) =
        crate::shared::signals::tantivy_fuzzy_file_signal(Some(&runtime.project_root), rel_path)
    {
        signals.push(s);
    }

    // Tantivy kind context: symbols of the same kind across the project
    #[cfg(feature = "tantivy-fts")]
    if let Some(s) =
        crate::shared::signals::tantivy_kind_context_signal(Some(&runtime.project_root), rel_path)
    {
        signals.push(s);
    }

    // Tantivy crate origin: other files from the same crate
    #[cfg(feature = "tantivy-fts")]
    if let Some(s) =
        crate::shared::signals::tantivy_crate_origin_signal(Some(&runtime.project_root), rel_path)
    {
        signals.push(s);
    }

    // Tantivy fuzzy symbol: symbol names similar to current file's basename
    #[cfg(feature = "tantivy-fts")]
    if let Some(s) =
        crate::shared::signals::tantivy_fuzzy_symbol_signal(Some(&runtime.project_root), rel_path)
    {
        signals.push(s);
    }

    // ── Wave 14 (2026-04-18): Surface streak warnings on Read ─────────
    // When CC re-reads a file that has crossed the regression streak
    // alert threshold, push a high-weight ⚠ warning so it appears
    // prominently in the read context. Improvement streaks get a
    // confirmation hint at the same weight. Both helpers fail-silent
    // when the streak is below threshold.
    if let Some(warn) = crate::health_delta::streak_warning_hint(file_path) {
        signals.push((1.5_f32, warn));
    } else if let Some(positive) = crate::health_delta::improvement_streak_hint(file_path) {
        signals.push((1.0_f32, positive));
    }

    signals
}

fn build_parallel_signal_pipeline(
    runtime: &HookRuntime,
    file_path: &str,
    rel_path: &str,
    budget: usize,
    cila_level: usize,
) -> Option<String> {
    use crate::shared::signals::merge_scored_signals_rrf;
    use crate::shared::thread_pool::with_hook_pool;

    // Collect both signal lists eagerly so we can fuse them with RRF
    // before feeding into the pipeline for budget truncation.
    let index_signals = collect_index_signals(runtime, file_path, rel_path);
    let source_signals = with_hook_pool(|| source_based_signals(file_path));

    // RRF fusion: merge index + source signals with Reciprocal Rank Fusion.
    // When one list is empty this is a no-op (preserves the other list's order).
    let fused = merge_scored_signals_rrf(&[index_signals, source_signals], 60.0);

    if fused.is_empty() {
        return None;
    }

    // Base pipeline: RRF-fused static signals (always present).
    let mut pipeline =
        SignalPipeline::new(budget).add_layer(StaticSignalLayer::new("rrf_fused", fused));

    // CILA>=2: Graph signal layers — only wired when SymbolIndex is available.
    // SymbolIndex is cloned into Arc to satisfy 'static bounds of SignalPipeline.
    // PheromoneGraph Arc is cloned from InfraRuntime (cheap Arc::clone).
    if let Some(idx) = runtime.infra.symbol_index.as_ref() {
        let arc_idx = std::sync::Arc::new(idx.clone());
        let arc_graph = std::sync::Arc::clone(&runtime.infra.pheromone_graph);
        pipeline = pipeline.extend(build_graph_pipeline(arc_idx, arc_graph, budget));
    }

    // Wave v4.29.0: AstGrepRiskSignalLayer — defensive risk patterns
    // (unwrap/panic/eval/exec) per language. Layer's own should_run gates at
    // CILA >= 2; the project_root is the runtime's detected workspace root so
    // ctx.file_path can stay relative.
    pipeline = pipeline.add_layer(
        crate::shared::ast_grep_signal::AstGrepRiskSignalLayer::with_root(
            runtime.project_root.clone(),
        ),
    );

    pipeline.execute(
        &SignalContext::new(rel_path, "")
            .with_cila(cila_level)
            .with_hook("pre_read"),
    )
}

/// Default token budget for context injection (characters).
/// Keeps injected context concise — prevents bloating Claude's context window.
pub const DEFAULT_CONTEXT_BUDGET: usize = 3200;

/// Minimum indexed definitions to trigger symbol map injection.
/// Below this threshold the file is too small to benefit from a navigation map.
const MIN_SYMBOL_DEFS: usize = 2;

/// Maximum number of definition entries shown in the symbol map.
const MAX_SYMBOL_MAP_ENTRIES: usize = 8;

/// Build a compact symbol map for proactive navigation context.
///
/// Injected *before* Claude reads the file so it can target specific line ranges
/// via `Read { offset, limit }` instead of reading the entire file.
///
/// Only `is_definition=true` entries are shown — references clutter the map.
/// Returns `None` when:
/// - `SymbolStore` is unavailable (project not indexed yet)
/// - Fewer than `MIN_SYMBOL_DEFS` definitions (file too trivial to map)
///
/// Format: `📌 defs[N]: Name1(12)·Name2(45)·…` (with `+K` suffix if truncated)
pub(crate) fn build_symbol_map_signal(
    store: Option<&SymbolStore>,
    file_path: &str,
) -> Option<String> {
    let store = store?;
    let symbols = store.find_symbols_in_file(file_path).ok()?;

    let defs: Vec<_> = symbols.iter().filter(|s| s.is_definition).collect();

    if defs.len() < MIN_SYMBOL_DEFS {
        return None;
    }

    let total = defs.len();
    let mut parts: Vec<String> = defs
        .iter()
        .take(MAX_SYMBOL_MAP_ENTRIES)
        .map(|s| format!("{}({})", s.symbol_name, s.line))
        .collect();

    if total > MAX_SYMBOL_MAP_ENTRIES {
        parts.push(format!("+{}", total - MAX_SYMBOL_MAP_ENTRIES));
    }

    Some(format!("\u{1f4cc} defs[{total}]: {}", parts.join("\u{b7}")))
}

/// Compose context ONLY from high-signal sources.
/// Returns None if nothing worth saying → hook stays silent.
///
/// P2.2: Uses batched DB query (1 round-trip instead of 3 separate queries).
/// S5: Delegates to budgeted version with DEFAULT_CONTEXT_BUDGET.
pub fn compose_high_signal_context(db: &FileKnowledgeDB, file_path: &str) -> Option<String> {
    compose_high_signal_context_budgeted(db, file_path, DEFAULT_CONTEXT_BUDGET, 0)
}

/// Compose context with an explicit character budget.
///
/// Signals are ranked by `recency × weight` and truncated to fit within
/// `max_chars`. This prevents large gotcha lists or dependency trees from
/// bloating injected context beyond what's useful.
///
/// Weights: Gotchas=2.0, Bash failures=1.5, Notes=1.5, Dependents=1.0.
/// Recency: `1.0 / (1.0 + days_since)` — recent signals score higher.
pub fn compose_high_signal_context_budgeted(
    db: &FileKnowledgeDB,
    file_path: &str,
    max_chars: usize,
    consecutive_file_reads: u32,
) -> Option<String> {
    // Batch query: notes + failures + dependents in 1 transaction.
    let signals_data = db.batch_pre_read_signals(file_path).ok()?;

    // Collect (score, text) pairs for ranking
    let mut scored_signals: Vec<(f32, String)> = Vec::new();

    // ── Signals 1/1b/1c/2: gotcha, history, failure signals ──
    collect_gotcha_history_signals(db, &signals_data, &mut scored_signals);

    // ── Signal 3/3b: dependency + large-file signals ──
    // S1.1: Compute line_est ONCE, pass to both Signal 3b and Signal 4.
    let line_est = if is_code_file(file_path) {
        std::fs::metadata(file_path)
            .map(|m| m.len() / 60)
            .unwrap_or_debug(0, "pre_read: line_est fallback")
    } else {
        0
    };
    collect_dependency_signals(&signals_data, file_path, line_est, &mut scored_signals);

    // ── No high-signal information → SILENCE ──
    if scored_signals.is_empty() {
        return None;
    }

    // S9: Normalize scores to [0, 1] before sorting
    normalize_scores(&mut scored_signals);
    // Sort by score DESC (highest priority first)
    scored_signals.sort_by(score_cmp);

    // P1 (SNR gating, default OFF): prune low-relevance signals before assembly so
    // only the high-signal tail reaches the budget (LlamaIndex similarity_cutoff).
    crate::shared::signals::apply_relevance_cutoff(&mut scored_signals);

    // ── Budget-aware assembly (S2) ──
    let (result_parts, budget_used) = assemble_scored_context(&scored_signals, max_chars);

    if result_parts.is_empty() {
        return None;
    }

    // ── Signal 4: Touring tool suggestion for code files (only when other signals exist) ──
    // Philosophy: augment existing high-signal context — never break silence on its own.
    // S1.1: Reuse precomputed line_est to avoid a second fs::metadata call.
    let suggestion = suggest_touring_for_code_file_with_est(file_path, line_est);
    if !suggestion.is_empty() && budget_used + 3 + suggestion.len() <= max_chars {
        // Append suggestion within budget
        let mut joined = result_parts.join(" | ");
        joined.push_str(" | ");
        joined.push_str(&suggestion);
        return Some(joined);
    }

    // ── D4.2 + I-14 Think-in-Code directive injection ──
    // Inject directive when: (1) bulk-read pattern detected (≥ THRESHOLD reads
    //   without writes), (2) budget < 50% remaining — avoids polluting high-budget sessions.
    // I-14 lowered threshold from 10 → 5 to enforce context-mode mandate
    // earlier (env-tunable via TOURING_THINK_IN_CODE_THRESHOLD).
    let threshold: u32 = std::env::var("TOURING_THINK_IN_CODE_THRESHOLD")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(5);
    if consecutive_file_reads >= threshold && budget_used > max_chars / 2 {
        let directive = "THINK IN CODE: Write and execute code to answer analysis questions instead of describing what you would do.";
        if budget_used + directive.len() + 15 <= max_chars {
            let mut joined = result_parts.join(" | ");
            joined.push_str(" | ");
            joined.push_str(directive);
            return Some(joined);
        }
    }

    Some(result_parts.join(" | "))
}

/// D39 L0 FileIndex fast-path provider for pre_read enrichment.
///
/// Uses MVKL's L0 FileIndex to provide token-level fast-path lookups
/// during pre_read context assembly. This enables O(1) token matching
/// against indexed source files rather than scanning the filesystem.
///
/// # Arguments
/// * `path` — Target file path to index
/// * `query` — Token query to search in the file index
///
/// # Returns
/// Vector of matching `FileIndexEntry`s, or empty vec if not found / not indexed.
pub fn pre_read_file_index_provider(
    path: &str,
    query: &str,
) -> Vec<touring_foundation::mvkl::FileIndexEntry> {
    // D39: L0 FileIndex provides raw token-level indexing.
    // Initialize a transient index for fast-path lookup.
    let mut index = FileIndex::new();

    // Read file content and index it.
    if let Ok(content) = std::fs::read_to_string(path) {
        index.index_file(path, &content);
    }

    // Search the indexed content for the query token.
    index.search(query)
}

/// Collect notes, gotchas, drift, and bash-failure signals into `out`.
///
/// Groups Signals 1, 1b, 1c, and 2 from `compose_high_signal_context_budgeted`
/// into a single cohesive helper — all relate to "what went wrong / what to watch".
fn collect_gotcha_history_signals(
    db: &FileKnowledgeDB,
    signals_data: &super::knowledge::PreReadSignals,
    out: &mut Vec<(f32, String)>,
) {
    // ── Signal 1: Notes (HIGH value — not visible in file) ──
    if let Some(notes) = &signals_data.notes
        && !notes.is_empty()
    {
        let short = truncate_str(notes, 120);
        out.push((1.5, format!("\u{26a0}\u{fe0f} {short}")));
    }

    // ── Signal 1b: Structured gotchas (pattern-matched, trackable) ──
    for g in &signals_data.gotchas {
        db.increment_gotcha_hit(g.id);
        let recency = recency_score_from_str(&g.created_at);
        // S2.3: Hit-count boost — frequently-hit gotchas get scored higher
        let hit_boost = ((g.hit_count.max(0) as f32 + 1.0).ln() * 0.2).min(0.5);
        let score = recency * 2.0 + hit_boost;
        let sym = g
            .symbol_name
            .as_deref()
            .map(|s| format!(" ({s})"))
            .unwrap_or_default();
        out.push((
            score,
            format!("\u{26a0} GOTCHA [{}]{}: {}", g.severity, sym, g.gotcha),
        ));
    }

    // ── Signal 1c: S3.2 Gotcha drift — fires only when >= 3 gotchas and all very stale ──
    if let Some(sig) = gotcha_drift_signal(&signals_data.gotchas) {
        out.push(sig);
    }

    // ── Signal 2: Bash failures on THIS SPECIFIC file (cross-cutting) ──
    if let Some((command, error_pattern)) = &signals_data.latest_failure {
        let cmd_short = command.split_whitespace().next().unwrap_or("cmd");
        let err = error_pattern.as_deref().unwrap_or("error");
        let short_err = truncate_str(err, 80);
        // Recent failures are very relevant — weight=1.5, recency=1.0 (assumed recent)
        out.push((
            1.5,
            format!("`{cmd_short}` failed on this file: {short_err}"),
        ));
    }
}

/// Collect dependency-count and large-file signals into `out`.
///
/// Groups Signals 3 and 3b from `compose_high_signal_context_budgeted`
/// — both relate to file impact/size awareness.
///
/// `line_est` should be pre-computed by the caller (avoids redundant `fs::metadata`).
fn collect_dependency_signals(
    signals_data: &super::knowledge::PreReadSignals,
    file_path: &str,
    line_est: u64,
    out: &mut Vec<(f32, String)>,
) {
    // ── Signal 3: Dependents ≥ 2 (impact awareness — invisible without graph) ──
    if signals_data.dependent_count >= 2 {
        let dep_names: Vec<&str> = signals_data
            .dependent_names
            .iter()
            .map(|r| {
                std::path::Path::new(r)
                    .file_name()
                    .and_then(|f| f.to_str())
                    .unwrap_or(r)
            })
            .collect();
        // S1.2: Logarithmic scoring — more dependents = higher urgency
        let dep_score = 1.0_f32 + (signals_data.dependent_count as f32).ln().max(0.0) * 0.3;
        out.push((
            dep_score,
            format!(
                "{} files import this: [{}]",
                signals_data.dependent_count,
                dep_names.join(", ")
            ),
        ));
    }

    // ── Signal 3b: Proactive CLI hint for large code files ──
    // Large files (> LARGE_FILE_LINE_THRESHOLD estimated lines) benefit significantly
    // from `touring ast overview` — injecting this hint can independently break silence.
    // Smaller files still get the suggestion as an appendage (Signal 4 below).
    if let Some(large_sig) = large_file_touring_signal_with_est(file_path, line_est) {
        out.push(large_sig);
    }
}

/// Compute days elapsed since a `NaiveDateTime`, clamped to ≥0.01.
fn days_since_naive(created: chrono::NaiveDateTime) -> f32 {
    let now = chrono::Utc::now().naive_utc();
    let hours = (now - created).num_hours().max(0) as f32;
    (hours / 24.0).max(0.01)
}

/// Compute recency score from an ISO-8601 timestamp string.
/// Returns `1.0 / (1.0 + days_since)` where `days_since` is clamped to ≥0.01.
/// Falls back to 0.5 if the timestamp cannot be parsed.
fn recency_score_from_str(timestamp: &str) -> f32 {
    let parsed = chrono::NaiveDateTime::parse_from_str(timestamp, "%Y-%m-%d %H:%M:%S")
        .or_else(|_| chrono::NaiveDateTime::parse_from_str(timestamp, "%Y-%m-%dT%H:%M:%S"));
    match parsed {
        Ok(created) => 1.0 / (1.0 + days_since_naive(created)),
        Err(_) => 0.5, // Unknown recency — neutral score
    }
}

/// Estimated line count above which `touring ast overview` is injected independently.
/// Below this threshold the suggestion is appended only when other signals exist.
const LARGE_FILE_LINE_THRESHOLD: u64 = 300;

/// Returns a scored signal for large code files, suitable for the ranked signal assembly.
///
/// For files estimated >= `LARGE_FILE_LINE_THRESHOLD` lines, the `touring ast overview`
/// hint is valuable enough to break silence on its own — Claude would otherwise read the
/// entire file and waste tokens. Score=1.2 (between gotchas=2.0 and dependents=1.0).
///
/// Returns `None` for non-code files or small files (those get Signal 4 appendage only).
///
/// S1.1: Backward-compatible wrapper — computes `line_est` via `fs::metadata`.
/// Internal callers should prefer `large_file_touring_signal_with_est` to avoid redundant I/O.
/// Retained for test backward-compatibility.
#[cfg(test)]
fn large_file_touring_signal(path: &str) -> Option<(f32, String)> {
    if !is_code_file(path) {
        return None;
    }
    let line_est = std::fs::metadata(path)
        .map(|m| m.len() / 60)
        .unwrap_or_debug(0, "pre_read: large file line_est fallback");
    large_file_touring_signal_with_est(path, line_est)
}

/// S1.1: Core implementation accepting pre-computed `line_est`.
fn large_file_touring_signal_with_est(path: &str, line_est: u64) -> Option<(f32, String)> {
    if !is_code_file(path) || line_est < LARGE_FILE_LINE_THRESHOLD {
        return None;
    }
    let pct: u8 = if line_est > 500 { 85 } else { 70 };
    let fname = std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path);
    Some((
        1.2_f32,
        format!(
            "\u{1f4a1} ~{line_est} linhas: `touring ast overview {fname}` mapeia sem ler completo (~{pct}% tokens)"
        ),
    ))
}

/// Returns a CLI command suggestion for a code file.
///
/// Suggests `touring ast overview <file>` (Bash CLI) instead of the MCP form,
/// so Claude Code can run it directly without a tool call round-trip.
///
/// S1.1: Backward-compatible wrapper — computes `line_est` via `fs::metadata`.
/// Internal callers should prefer `suggest_touring_for_code_file_with_est`.
/// Retained for test backward-compatibility.
#[cfg(test)]
pub(crate) fn suggest_touring_for_code_file(path: &str) -> String {
    if !is_code_file(path) {
        return String::new();
    }
    let line_est = std::fs::metadata(path)
        .map(|m| m.len() / 60)
        .unwrap_or_debug(0, "pre_read: suggest_touring line_est fallback");
    suggest_touring_for_code_file_with_est(path, line_est)
}

/// S1.1: Core implementation accepting pre-computed `line_est`.
fn suggest_touring_for_code_file_with_est(path: &str, line_est: u64) -> String {
    if !is_code_file(path) {
        return String::new();
    }
    let pct: u8 = if line_est > 200 { 80 } else { 50 };
    let fname = std::path::Path::new(path)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or(path);
    format!("\u{1f4a1} `touring ast overview {fname}` mapeia sem ler completo (~{pct}% tokens)")
}

fn is_code_file(path: &str) -> bool {
    matches!(
        std::path::Path::new(path)
            .extension()
            .and_then(|e| e.to_str()),
        Some("py" | "rs" | "ts" | "tsx" | "js" | "jsx" | "go" | "cpp" | "c" | "h")
    )
}

// ── Sprint 2: BlastRadius + External Callers ────────────────────────────

/// Signal I-6: BFS hop-distance enrichment via BlastRadiusEngine.
///
/// Supplements the existing `blast_radius_signal` (which shows count + max distance)
/// with precise per-file hop distances via `BlastRadiusEngine::bfs_only()`.
/// Shows top 5 `AffectedFile` entries sorted by distance ascending.
///
/// Budget guard: 8ms via `AnalysisConfig::hook_path().with_budget(8)`.
/// Falls back to `None` (silent) if BFS is not possible or budget exceeded.
/// Score: 1.5
fn bfs_hop_signal(idx_opt: Option<&SymbolIndex>, rel_path: &str) -> Option<(f32, String)> {
    let idx = idx_opt?;

    // Guard: only fire when this file has at least one reverse dep
    if idx
        .reverse_deps
        .get(rel_path)
        .map_or(true, |v| v.is_empty())
    {
        return None;
    }

    use std::sync::Arc;
    use touring_analysis::blast_radius::BlastRadiusEngine;
    use touring_analysis::engine::AnalysisConfig;

    let config = AnalysisConfig::hook_path().with_budget(8);
    let engine = BlastRadiusEngine::bfs_only(Arc::new(idx.clone()));
    let result = engine.compute(rel_path, &config);

    if result.affected_files.is_empty() || result.strategy_used == "budget-exceeded" {
        return None;
    }

    let shown: Vec<String> = result
        .affected_files
        .iter()
        .take(5)
        .map(|f| {
            let name = std::path::Path::new(&f.path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&f.path);
            format!("{name}(hop{})", f.distance)
        })
        .collect();

    let total = result.affected_files.len();
    let suffix = if total > 5 {
        format!(" +{} more", total - 5)
    } else {
        String::new()
    };

    Some((
        1.5_f32,
        format!("bfs_hops[{total}]: {}{suffix}", shown.join(", ")),
    ))
}

/// S2.2: External callers signal from SymbolIndex.
///
/// For each symbol defined in this file, counts references from other files.
/// Shows top 3 most-referenced symbols. Score=1.7.
/// Accepts `Option<&SymbolIndex>` so it can be called inside rayon::join.
fn external_callers_signal(idx_opt: Option<&SymbolIndex>, rel_path: &str) -> Option<(f32, String)> {
    let idx = idx_opt?;

    let mut caller_counts: Vec<(String, usize)> = idx
        .symbols
        .iter()
        .filter_map(|(sym_name, locations)| count_external_refs(sym_name, locations, rel_path))
        .collect();

    if caller_counts.is_empty() {
        return None;
    }

    caller_counts.sort_by_key(|b| std::cmp::Reverse(b.1));
    let label = format_caller_list(&caller_counts, 3);
    Some((1.7_f32, format!("\u{1f4de} callers externo: {label}")))
}

/// Count external references for a symbol defined in `rel_path`.
///
/// Returns `Some((name, count))` when the symbol is defined in this file
/// and has at least one reference from another file; `None` otherwise.
fn count_external_refs(
    sym_name: &str,
    locations: &[touring_code::ast::SymbolLocation],
    rel_path: &str,
) -> Option<(String, usize)> {
    let defined_here = locations
        .iter()
        .any(|loc| loc.file_path == rel_path && loc.is_definition);
    if !defined_here {
        return None;
    }
    let ext_refs = locations
        .iter()
        .filter(|loc| !loc.is_definition && loc.file_path != rel_path)
        .count();
    if ext_refs > 0 {
        Some((sym_name.to_owned(), ext_refs))
    } else {
        None
    }
}

/// Format a sorted caller list as `"Name(N↑)·Name(N↑)…"` (up to `max_show` entries).
fn format_caller_list(caller_counts: &[(String, usize)], max_show: usize) -> String {
    caller_counts
        .iter()
        .take(max_show)
        .map(|(name, count)| format!("{name}({count}\u{2191})"))
        .collect::<Vec<_>>()
        .join("\u{b7}")
}

/// Build the L7 co-edit prediction signal from a prediction slice.
///
/// Returns `None` when `preds` is empty (hook stays silent).
/// Format: `"🔮 L7: file.rs (87%), other.rs (43%)"`
fn build_l7_pred_signal(
    preds: &[crate::layer7_prediction::PredictedFile],
) -> Option<(f32, String)> {
    if preds.is_empty() {
        return None;
    }
    let pred_str = preds
        .iter()
        .map(|p| {
            let name = std::path::Path::new(&p.path)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(&p.path);
            format!("{name} ({:.0}%)", p.confidence * 100.0)
        })
        .collect::<Vec<_>>()
        .join(", ");
    Some((0.9, format!("\u{1f52e} L7: {pred_str}")))
}

/// E12: Build a signal from PredictiveFocusCache pheromone-ranked predictions.
///
/// Uses the given candidates (L7 predictions or session files) for pheromone
/// ranking.  Returns a signal only when PFC ranks at least one candidate —
/// i.e., the file was historically co-accessed with `current_file`.
fn build_pfc_signal(
    pfc: &touring_intelligence::reasoning::PredictiveFocusCache,
    current_file: &str,
    candidates: &[String],
) -> Option<(f32, String)> {
    if candidates.is_empty() {
        return None;
    }
    let ranked = pfc.prefetch_likely(current_file, candidates, 3);
    if ranked.is_empty() {
        return None;
    }
    let names: Vec<&str> = ranked
        .iter()
        .map(|p| {
            std::path::Path::new(p)
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(p.as_str())
        })
        .collect();
    Some((0.85, format!("\u{1f41c} PFC: {}", names.join(", "))))
}

// ── Sprint 1: Source-based signals (re-exports + module hierarchy) ───────

/// Collect source-based signals for any code file.
///
/// Signals fired per language:
/// - `.rs`: ModuleTree pub re-exports (S1.4) + module hierarchy (S1.5) + scope shadowing (S3.4)
/// - `.py`: scope shadowing (S3.4)
/// - `.ts`/`.tsx`/`.js`/`.jsx`: export detection
/// - All code files: scope shadowing where `build_scope_map` supports the language
///
/// Source is read once and reused for all applicable extractors.
fn source_based_signals(file_path: &str) -> Vec<(f32, String)> {
    if !is_code_file(file_path) {
        return Vec::new();
    }
    let Ok(source) = std::fs::read_to_string(file_path) else {
        return Vec::new();
    };
    let mut signals = Vec::new();

    // S1.4 + S1.5: ModuleTree signals (Rust only)
    if file_path.ends_with(".rs") {
        let tree = ModuleTree::build_from_source(&source, file_path);
        if let Some(sig) = rust_reexport_signal(&tree) {
            signals.push(sig);
        }
        if let Some(sig) = rust_module_hierarchy_signal(&tree, file_path) {
            signals.push(sig);
        }
    }

    // TypeScript / JavaScript: export detection (ModuleTree not available for these langs)
    if matches!(
        std::path::Path::new(file_path)
            .extension()
            .and_then(|e| e.to_str()),
        Some("ts" | "tsx" | "js" | "jsx" | "mjs" | "cjs")
    ) && let Some(sig) = ts_js_exports_signal(&source)
    {
        signals.push(sig);
    }

    // S3.4: ScopeMap shadowing warnings (Rust + Python — build_scope_map limitation)
    if let Some(sig) = scope_shadowing_signal(&source, file_path) {
        signals.push(sig);
    }

    signals
}

/// ANN recall signal: find semantically similar memories from past edits.
///
/// Uses the persistent ANN index (backed by memory.db) to surface
/// relevant memories that aren't derivable from the file content itself.
/// Returns `None` if ANN recall is not initialized or has no results.
pub(crate) fn ann_recall_signal(runtime: &HookRuntime, file_path: &str) -> Option<(f32, String)> {
    let ann_ref = runtime.ctx.ann_recall.borrow();
    let ann = ann_ref.as_ref()?;
    if ann.is_empty() {
        return None;
    }

    // Build a simple hash embedding from file path tokens.
    // This is a deterministic, O(1) embedding that clusters files
    // by path structure (e.g., "src/hooks/pre_read.rs" ≈ "src/hooks/post_edit.rs").
    let query_embedding = crate::ann_memory::path_hash_embedding(file_path);
    let results = ann.search(&query_embedding, 3);

    if results.is_empty() {
        return None;
    }

    let summary: Vec<String> = results
        .iter()
        .filter(|r| r.score > 0.3) // only meaningful similarity
        .take(3)
        .map(|r| format!("{} ({:.0}%)", r.id, r.score * 100.0))
        .collect();

    if summary.is_empty() {
        return None;
    }

    Some((0.7, format!("ANN related: {}", summary.join(", "))))
}

/// S1.4: Build a pub re-exports signal from a Rust `ModuleTree`.
///
/// Returns `None` when the root has no `pub use` re-exports.
/// Score = 1.8 (high) because changing a re-export breaks downstream API.
fn rust_reexport_signal(tree: &ModuleTree) -> Option<(f32, String)> {
    if tree.root.re_exports.is_empty() {
        return None;
    }
    let shown: Vec<&str> = tree
        .root
        .re_exports
        .iter()
        .take(4)
        .map(|s| s.as_str())
        .collect();
    let extra = if tree.root.re_exports.len() > 4 {
        format!("+{}", tree.root.re_exports.len() - 4)
    } else {
        String::new()
    };
    Some((
        1.8_f32,
        format!(
            "\u{26a1} pub re-exports[{}]: {} {} \u{2014} break = API break",
            tree.root.re_exports.len(),
            shown.join(", "),
            extra
        ),
    ))
}

/// S1.5: Build a module hierarchy signal for Rust entry-point files.
///
/// Only fires for `lib.rs`, `mod.rs`, and `main.rs` with at least one child module.
/// Score = 1.1 (informational) — helps navigate large crate entry points.
fn rust_module_hierarchy_signal(tree: &ModuleTree, file_path: &str) -> Option<(f32, String)> {
    let fname = std::path::Path::new(file_path)
        .file_name()
        .and_then(|f| f.to_str())
        .unwrap_or_debug("", "pre_read: file_name fallback");
    if !matches!(fname, "lib.rs" | "mod.rs" | "main.rs") || tree.root.children.is_empty() {
        return None;
    }
    let mods: Vec<String> = tree
        .root
        .children
        .iter()
        .take(6)
        .map(|n| {
            if n.is_pub {
                format!("pub::{}", n.name)
            } else {
                n.name.clone()
            }
        })
        .collect();
    Some((1.1_f32, format!("\u{1f4e6} mods: [{}]", mods.join(", "))))
}

/// Scan source lines and collect TypeScript/JavaScript named and default exports.
///
/// Returns `(named_exports, has_default)`. Skips re-export-all (`export *`)
/// and comment lines. Used by [`ts_js_exports_signal`].
/// Classify a single TypeScript/JavaScript export line.
///
/// Returns `None` for lines that should be skipped (re-export-all, comments,
/// non-export lines). Returns `Some(None)` for `export default`. Returns
/// `Some(Some(name))` for a named export.
///
/// Pure function — no I/O, easily testable.
fn classify_export_line(line: &str) -> Option<Option<String>> {
    let t = line.trim_start();
    if !t.starts_with("export ") {
        return None; // not an export line — skip
    }
    if t.starts_with("export *") || t.starts_with("export //") {
        return None; // re-export-all or comment — skip
    }
    if t.starts_with("export default") {
        return Some(None); // default export
    }
    Some(parse_ts_export_name(t)) // named export (may be None if name unparseable)
}

fn collect_ts_js_exports(source: &str) -> (Vec<String>, bool) {
    let mut named: Vec<String> = Vec::new();
    let mut has_default = false;

    for line in source.lines() {
        match classify_export_line(line) {
            None => {}                            // skip
            Some(None) => has_default = true,     // default export
            Some(Some(name)) => named.push(name), // named export
        }
    }

    (named, has_default)
}

/// Format a scored export signal from collected named + default exports.
///
/// Returns `None` when there are no exports. Shows up to 4 names with an
/// overflow suffix. Score = 1.6 (same tier as external callers).
fn format_exports_signal(named: Vec<String>, has_default: bool) -> Option<(f32, String)> {
    let total = named.len() + usize::from(has_default);
    if total == 0 {
        return None;
    }
    let mut shown: Vec<String> = named.iter().take(4).cloned().collect();
    if has_default {
        shown.push("default".to_string());
    }
    let extra = if total > 4 {
        format!("+{}", total - 4)
    } else {
        String::new()
    };
    Some((
        1.6_f32,
        format!(
            "\u{26a1} exports[{total}]: {} {extra} \u{2014} break = API break",
            shown.join(", ")
        ),
    ))
}

/// Detect top-level exports in TypeScript / JavaScript source.
///
/// Scans for lines starting with `export ` to extract named and default
/// exports. Reports up to 4 names (with `+N` overflow). Score = 1.6 (same
/// tier as external callers) because exported symbols are API surface.
///
/// Only inspects non-comment lines; does NOT parse JSDoc or triple-slash.
/// Returns `None` when no exports are found (e.g., internal utility files).
fn ts_js_exports_signal(source: &str) -> Option<(f32, String)> {
    let (named, has_default) = collect_ts_js_exports(source);
    format_exports_signal(named, has_default)
}

/// Extract the exported identifier from a TypeScript/JavaScript `export` line.
///
/// Skips keyword tokens (`async`, `const`, `function`, etc.) to find the
/// first identifier. Trims generic/parameter suffixes (`Foo<T>` → `Foo`).
/// Returns `None` for re-export blocks (`export { X }`) and names shorter than 2 chars.
fn parse_ts_export_name(export_line: &str) -> Option<String> {
    let tokens: Vec<&str> = export_line.split_whitespace().collect();
    let name_token = tokens
        .iter()
        .skip(1) // skip "export"
        .find(|&&w| {
            !matches!(
                w,
                "async"
                    | "const"
                    | "let"
                    | "var"
                    | "function"
                    | "class"
                    | "type"
                    | "interface"
                    | "abstract"
                    | "declare"
                    | "enum"
                    | "{"
                    | "default"
            )
        })?;
    // Trim generic/paren/equals suffixes: "Foo<T>" → "Foo"
    let clean = name_token
        .split(['<', '(', '=', ':', ','])
        .next()
        .unwrap_or(name_token)
        .trim_end_matches(|c: char| !c.is_alphanumeric() && c != '_' && c != '$');
    if clean.len() >= 2 {
        Some(clean.to_string())
    } else {
        None
    }
}

// ── Sprint 3: Scope Shadowing + Similar Symbols + Gotcha Drift ─────────

/// Detect the scope-map language from a file path extension.
///
/// Only Rust and Python are supported by `build_scope_map`.
/// Returns `None` for all other extensions.
fn detect_scope_lang(file_path: &str) -> Option<touring_code::ast::Lang> {
    if file_path.ends_with(".rs") {
        Some(touring_code::ast::Lang::Rust)
    } else if file_path.ends_with(".py") {
        Some(touring_code::ast::Lang::Python)
    } else {
        None
    }
}

/// S3.4: Detect variable shadowing via ScopeMap.
///
/// Analyzes source code for variable names that are defined multiple times
/// in the same scope tree. Single-character names (e.g., `i`, `x`) are
/// excluded since those are typically intentional rebindings.
fn scope_shadowing_signal(source: &str, file_path: &str) -> Option<(f32, String)> {
    let lang = detect_scope_lang(file_path)?;

    let scope = build_scope_map(source, lang);
    if scope.entries.len() < 2 {
        return None;
    }

    let mut shadows = collect_shadow_pairs(&scope.entries);
    if shadows.is_empty() {
        return None;
    }

    // Sort by shadowing line (later line = more recent shadow)
    shadows.sort_unstable_by_key(|(_, _, second)| *second);

    let shown: Vec<String> = shadows
        .iter()
        .take(2)
        .map(|(name, first, second)| format!("{name}@{second} shadows @{first}"))
        .collect();

    Some((
        0.9_f32,
        format!("\u{26a0} scope shadowing: {}", shown.join(", ")),
    ))
}

/// Collect `(name, first_line, shadow_line)` pairs from scope entries.
///
/// Skips single-character names (intentional rebindings like `i`, `x`).
/// A shadow is detected when the same name appears at two or more lines.
fn collect_shadow_pairs(entries: &[touring_code::ast::ScopeEntry]) -> Vec<(String, usize, usize)> {
    let mut by_name: std::collections::HashMap<&str, Vec<usize>> = std::collections::HashMap::new();
    for entry in entries {
        if entry.name.len() <= 1 {
            continue;
        }
        by_name
            .entry(entry.name.as_str())
            .or_default()
            .push(entry.line);
    }
    by_name
        .iter()
        .filter_map(|(name, lines)| {
            let first = *lines.first()?;
            let second = *lines.get(1)?;
            Some(((*name).to_owned(), first, second))
        })
        .collect()
}

/// S3.1: Thin test helper — delegates to shared implementation.
/// Used only in tests; production code calls `similar_symbol_signal_for_path` directly.
#[cfg(test)]
fn similar_symbol_signal(idx_opt: Option<&SymbolIndex>, rel_path: &str) -> Option<(f32, String)> {
    crate::shared::signals::similar_symbol_signal_for_path(idx_opt, rel_path)
}

/// S3.2: Detect gotcha staleness via DriftDetector KS test.
///
/// Compares this file's gotcha recency distribution against a "healthy"
/// reference. High KS stat + low average recency = gotchas likely stale.
/// Only fires when >= 3 gotchas exist (KS test needs samples).
fn gotcha_drift_signal(gotchas: &[crate::knowledge::Gotcha]) -> Option<(f32, String)> {
    if gotchas.len() < 3 {
        return None;
    }

    let recency: Vec<f64> = gotchas
        .iter()
        .map(|g| recency_score_from_str(&g.created_at) as f64)
        .collect();

    // Reference: healthy gotcha population mixes recent and aged entries
    let healthy_ref = [0.9_f64, 0.6, 0.4, 0.2, 0.1];
    let ks = DriftDetector::new().ks_statistic(&recency, &healthy_ref);

    let avg = recency.iter().sum::<f64>() / recency.len() as f64;

    // High drift (concentrated distribution) + low avg recency = stale
    if ks > 0.65 && avg < 0.15 {
        Some((
            0.5_f32,
            format!("\u{1f4c5} gotchas podem estar desatualizados (drift={ks:.2})"),
        ))
    } else {
        None
    }
}

/// S4: Detect distribution drift in command error rates via KS + JS divergence.
///
/// Compares baseline error rates (from SQLite history) against current session
/// rates. High divergence indicates the command success pattern has shifted,
/// suggesting environmental or codebase changes affecting tool reliability.
fn error_rate_drift_signal(baseline_rates: &[f64], current_rate: f64) -> Option<(f32, String)> {
    if baseline_rates.is_empty() {
        return None;
    }

    let detector = DriftDetector::new();
    let ks = detector.ks_statistic(baseline_rates, &[current_rate]);
    let js = detector.js_divergence(baseline_rates, &[current_rate]);

    // Thresholds: KS > 0.15 or JS > 0.1 indicates significant drift
    if ks > 0.15 || js > 0.1 {
        Some((
            0.6_f32,
            format!(
                "\u{26a0}\u{fe0f} distribution drift detected: KS={:.3} JS={:.3}",
                ks, js
            ),
        ))
    } else {
        None
    }
}

#[cfg(test)]
#[path = "pre_read_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "pre_read_tests_b.rs"]
mod tests_b;
