//! Pre-Edit Hook — Impact analysis before Claude edits or writes a file.
//!
//! Before Claude edits a file, this hook:
//! 1. Queries file_relations to find dependents (who imports this file?)
//! 2. Queries recent edits (was this file recently changed?)
//! 3. Injects impact context
//!
//! Target latency: <10ms.

use super::error_predictor::ErrorPredictor;
use super::idempotency::{IdempotencyConfig, check_idempotency};
use super::knowledge::FileKnowledgeDB;
use super::pii::PIIFinding;
use super::pre_edit_prevention;
use super::runtime::{HookResponse, HookRuntime, make_relative};
use crate::schemas::{validate_payload, validation_deny};
use crate::shared::cila::cila_budget_edit;
use crate::shared::hook_helpers;
use crate::shared::metadata_collector::FastMetadata;
use crate::shared::parser_cache_global::global_cache;
#[allow(unused_imports)]
// ResultExt trait needed in scope for .unwrap_or_debug() calls on deref
use crate::shared::result_ext::{OptionExt, ResultExt};
use crate::shared::signal_pipeline::{SignalContext, SignalPipeline, StaticSignalLayer};
use crate::shared::signals::{blast_radius_signal, merge_signals_rrf};
use touring_foundation::diagnostic::DiagnosticCode;
use touring_foundation::truncate_str;

/// Flush the pre_edit parser cache at session boundaries.
///
/// Called from `run_session_stop` to release all cached `Arc<SharedPipeline>`
/// entries so the next session starts fresh (avoids stale pipeline state).
pub(crate) fn flush_cache() {
    global_cache().clear();
}

/// Run the pre-edit hook (diverging version — for use by the CLI entry point).
#[tracing::instrument(skip(runtime, input), fields(hook = "pre_edit"))]
pub fn run(
    runtime: &HookRuntime,
    input: &serde_json::Value,
) -> Result<(), touring_hook_runtime::hook_runtime::HookDispatchError> {
    run_returning(runtime, input).emit()
}

/// Run the pre-edit hook, returning a `HookResponse` instead of diverging.
///
/// Used by the daemon to handle the hook without calling `process::exit`.
#[tracing::instrument(skip_all, fields(hook = "pre_edit"))]
pub fn run_returning(runtime: &HookRuntime, input: &serde_json::Value) -> HookResponse {
    // Wave 13: Unified Observability — record pre_edit enter/exit hop in span context.
    let enter_us = crate::shared::span_context::timestamp_us();
    let result = run_returning_impl(runtime, input);
    let exit_us = crate::shared::span_context::timestamp_us();
    runtime.record_span_layer("pre_edit", enter_us, exit_us);
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
                            code: std::borrow::Cow::Borrowed("missing"),
                            message: Some(std::borrow::Cow::Borrowed(
                                "tool_input missing from input",
                            )),
                            params: std::collections::HashMap::new(),
                        },
                    );
                    e
                },
                "pre_edit",
            );
        }
    };
    let validated = match validate_payload::<crate::schemas::PreEditPayload>(tool_input) {
        Ok(v) => v,
        Err(errors) => return validation_deny(&errors, "pre_edit"),
    };
    let file_path = validated.file_path.as_str();

    if file_path.is_empty() {
        return HookResponse::Allow;
    }

    let old_string = validated.old_string.as_deref().unwrap_or("");
    let rel_path = make_relative(file_path, &runtime.project_root);

    // FileParserCache: warm up the per-file pipeline entry so speculate_v2 callers
    // (pre_write, post_edit Phase 2) find an already-initialised SharedPipeline.
    // Graceful: errors are silently ignored — never blocks the hook.
    {
        let cache = global_cache();
        let path_buf = std::path::PathBuf::from(file_path);
        let _pipeline = cache.get_or_create(&path_buf);
    }

    // FastMetadata: inject file-size signal into context for large-file awareness.
    // Only injected when file is large (>500 KB) to avoid noise on routine edits.
    let file_meta_sig: Option<String> = FastMetadata::from_path(std::path::Path::new(file_path))
        .ok()
        .filter(|m| m.file_size_bytes > 512 * 1024)
        .map(|m| {
            format!(
                "file-size: {:.0} KB — large file, prefer surgical edits",
                m.file_size_bytes / 1024
            )
        });

    // L7-B Alpha: Read CILA level for enrichment policy gate.
    // Pattern mirrors pre_write.rs:90-103 — stable_session first, result_cache fallback.
    let cila_level: u8 = hook_helpers::cila_level_from_runtime(runtime, 3);

    // L7-B Alpha: Gate expensive knowledge-DB enrichment query behind CILA policy.
    // At L0/L1 (Reflexo/Associação), skip the DB query entirely — minimal context
    // keeps pre_edit latency < 2ms. At L2+, run full enrichment.
    let mut context =
        if crate::shared::cila::should_enrich(runtime.enrichment_active, cila_level, "Edit") {
            // L7-B Gamma: record full-enrichment path for gate observability metrics
            crate::shared::gate_metrics::record_pre_edit_full();
            compose_edit_context(Some(runtime), &runtime.ctx.knowledge, &rel_path)
                .unwrap_or_default()
        } else {
            // L7-B Gamma: record fast-path for gate observability metrics
            crate::shared::gate_metrics::record_pre_edit_fast_path();
            // Fast-path: skip compose_edit_context (avoids knowledge DB gotcha+relations query).
            // Signals from session_bus cache and inline checks (blast, antipatterns, PII)
            // continue to run — they're already cheap.
            String::new()
        };

    // FA-1: Read cached blast radius from SessionBus (populated by pre_read).
    // Skips redundant re-computation when the file was recently read.
    let cached_blast = runtime.ctx.session_bus.borrow().get_blast_radius(&rel_path);

    // FA-1: Read active plan hint from SessionBus (set by decompose/MCTS planning).
    // Allows pre_edit to prioritize signals relevant to the current task.
    let plan_hint = runtime.ctx.session_bus.borrow().active_plan_hint.clone();

    // ── Signal B: Blast radius — who depends on this file (FA-1: uses session_bus cache) ──
    // B-301 gate needs blast_count — compute it once and pass as argument to compose_quality_evolution.
    let (_blast_count, blast_sig): (usize, Option<(f32, String)>) = if let Some(count) =
        cached_blast
    {
        // G2: RFC-100 B-300 — emit BlastWarning when cached blast radius exceeds threshold.
        if count > 10 {
            use touring_analysis::blast_radius::BlastWarning;
            let w = BlastWarning::HighBlast {
                symbol: rel_path.clone(),
                affected_files: count,
                threshold: 10,
            };
            tracing::warn!(
                code = w.code_str(),
                message = %format!("{count} files affected by blast from {rel_path} (threshold=10)"),
                severity = "warning",
                file_path = %rel_path,
            );
        }
        (
            count,
            Some((1.0_f32, format!("blast cached({} deps)", count))),
        )
    } else if let Some(idx) = runtime.infra.symbol_index.as_ref() {
        // Fallback: compute fresh via SymbolIndex
        let bc = crate::shared::signals::blast_radius_file_count(Some(idx), &rel_path).unwrap_or(0);
        let sig = blast_radius_signal(Some(idx), &rel_path, true);
        (bc, sig)
    } else {
        (0, None)
    };
    if let Some((_score, sig)) = blast_sig {
        if !context.is_empty() {
            context.push_str(" | ");
        }
        context.push_str(&sig);
    }

    // FA-2: Inject active plan hint into context for task-aware editing.
    // Set by decompose/MCTS hooks via session_bus.signal_plan_active().
    if let Some(plan) = &plan_hint
        && !plan.is_empty()
    {
        if !context.is_empty() {
            context.push_str(" | ");
        }
        context.push_str(&format!("plan: {}", plan));
    }

    // A9: Hook chaining — read pre_read's result from session_bus and inject into context.
    // Enables pre_edit to know what pre_read saw (e.g., context_len from prior read).
    if let Some(pre_read_result) = runtime
        .ctx
        .session_bus
        .borrow()
        .get_last_hook_result("pre_read")
        && let (Some(fp), Some(clen)) = (
            pre_read_result.get("file_path").and_then(|v| v.as_str()),
            pre_read_result.get("context_len").and_then(|v| v.as_u64()),
        )
    {
        // Only inject if same file was read before editing
        if fp == rel_path {
            if !context.is_empty() {
                context.push_str(" | ");
            }
            context.push_str(&format!("chain:pre_read(ctx_len={})", clen));
        }
    }

    // FastMetadata: inject large-file warning when file exceeds 500 KB.
    if let Some(ref sig) = file_meta_sig {
        if !context.is_empty() {
            context.push_str(" | ");
        }
        context.push_str(sig);
    }

    // Append touring suggestion if old_string spans an entire function
    let suggestion = suggest_replace_symbol_body(old_string);
    if !suggestion.is_empty() {
        if !context.is_empty() {
            context.push_str(" | ");
        }
        context.push_str(&suggestion);
    }

    // ── Rust anti-pattern detection on new content ──
    let new_string = validated.new_string.as_deref().unwrap_or("");
    // ── Signals 6a+6b: Antipatterns + import prediction (RRF-fused for Rust) ──
    // For Rust files: merge antipattern warnings and import suggestions via RRF so
    // items that rank high in both lists are surfaced first. Uses merge_signals_rrf
    // (unscored RRF) since both sources produce plain string lists.
    // For non-Rust files: only import prediction applies.
    if !new_string.is_empty() {
        if rel_path.ends_with(".rs") {
            let antipatterns = check_rust_antipatterns(new_string);
            let unresolved: Vec<String> =
                detect_unresolved_types(new_string, &runtime.ctx.knowledge, &rel_path)
                    .into_iter()
                    .take(3)
                    .map(|s| format!("import needed: {s}"))
                    .collect();
            // RRF fusion: items present in both lists rank highest.
            let merged = merge_signals_rrf(&[antipatterns, unresolved], 60.0);
            for sig in &merged {
                if !context.is_empty() {
                    context.push_str(" | ");
                }
                context.push_str(sig);
            }
        } else {
            let unresolved = detect_unresolved_types(new_string, &runtime.ctx.knowledge, &rel_path);
            for suggestion in unresolved.iter().take(3) {
                if !context.is_empty() {
                    context.push_str(" | ");
                }
                context.push_str(&format!("import needed: {suggestion}"));
            }
        }
    }

    // A.3 Idempotency gate: check format(format(x)) == format(x) for Rust files.
    // Wired after validated.new_string is extracted and after CILA gate check.
    // On violation: record counter, downgrade score by 0.3, emit Q-220 diagnostic.
    if !new_string.is_empty() && rel_path.ends_with(".rs") {
        let idempotency_cfg = IdempotencyConfig::default();
        if let Err(payload) = check_idempotency(
            std::path::Path::new(&rel_path),
            new_string,
            &idempotency_cfg,
        ) {
            crate::shared::gate_metrics::record_diagnostic_q220_nonidempotent_emitted();
            if !context.is_empty() {
                context.push_str(" | ");
            }
            context.push_str(&format!(
                "Q-220:idempotency_violation(file={},first={}B,second={}B,bytes_differ={})",
                payload.file_path, payload.first_len, payload.second_len, payload.diff_bytes
            ));
        }
    }

    // ── Signal 15: HDG-8 cognitive complexity on new content ──
    // Estimates cyclomatic-like cognitive complexity of the replacement string.
    // Only injected when complexity is non-trivial (> 5) to avoid noise on small edits.
    if !new_string.is_empty() {
        let complexity = touring_analysis::estimate_cognitive_complexity(new_string);
        if complexity > 5 {
            let label = if complexity > 20 {
                "high"
            } else if complexity > 10 {
                "medium"
            } else {
                "elevated"
            };
            let sig =
                format!("complexity: {label} ({complexity}) — consider extracting sub-functions");
            if !context.is_empty() {
                context.push_str(" | ");
            }
            context.push_str(&sig);
        }
    }

    // ── PII Scan: detect Brazilian PII in new content (PreToolUse) ──
    // Scans the replacement text for CPF, CNPJ, RG, CNH, SUS, SEI, email, phone.
    // High/medium findings are injected as security signals.
    if !new_string.is_empty() {
        let findings = runtime.ctx.pii_scanner.scan_text(new_string);
        if !findings.is_empty() {
            let pii_ctx = format_pii_findings_context(&findings);
            if !context.is_empty() {
                context.push_str(" | ");
            }
            context.push_str(&pii_ctx);
        }
    }

    // ── EC32: Cognitive enrichment — inject file risk and bash failure signals ──
    // Replicates post_edit.rs:675 / post_write.rs:210 / pre_write.rs:309 pattern.
    // Always runs regardless of CILA gate — cognitive signals are pre-computed and cheap.
    if let Some(ref cognitive) = runtime.cognitive {
        let enriched = crate::shared::signals::enrich_with_cognitive(cognitive, file_path, false);
        if !enriched.is_empty() {
            if !context.is_empty() {
                context.push_str(" | ");
            }
            context.push_str(&enriched);
        }
    }

    // ── Signal 15b + B1-store: AnalysisPipeline composite health (TTL-cached 30s) ──
    // Single pipeline run serves both Signal 15b (display) and B1-store (HealthDiff snapshot).
    // Cache key: "pre_edit:analysis_health:<rel_path>". B1 key: "__pre_edit_health__:<rel_path>".
    {
        let cache_key = format!("analysis_health:{}", rel_path);
        let snap_key = format!("__pre_edit_health__:{}", rel_path);
        let cached = runtime.ctx.result_cache.get_result("pre_edit", &cache_key);
        if cached.is_none() {
            // Cold path: single pipeline run for both Signal 15b display + B1-store snapshot
            let conn = runtime.ctx.knowledge.conn_ref();
            let health = touring_analysis::AnalysisPipeline::new(
                conn,
                touring_analysis::engine::AnalysisConfig::hook_path(),
            )
            .run(
                runtime
                    .project_root
                    .to_str()
                    .unwrap_or_debug("", "pre_edit: project_root fallback"),
            );
            // G3: Build AnalysisInsights enriched with quality trend from temporal DB.
            let insights = {
                let base = touring_analysis::AnalysisInsights::from_report(&health);
                let trend = touring_analysis::quality_trend(conn, 5);
                // EC47: First caller of with_orphan_count() — extracts orphan_count from
                // wiring dimension metrics so AnalysisInsights.to_context_string() reports
                // the real orphan count instead of the default 0.
                let orphan_count: usize = health
                    .dimensions
                    .iter()
                    .find(|d| d.name == "wiring")
                    .and_then(|d| d.metrics.get("orphan_count"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or_debug(0, "pre_edit: orphan_count fallback")
                    as usize;
                base.with_quality_trend(&trend)
                    .with_orphan_count(orphan_count)
            };
            let sig = format!(
                "project: {} | {}",
                insights.health_status,
                insights.to_context_string()
            );
            runtime
                .ctx
                .result_cache
                .cache_result("pre_edit", &cache_key, sig.clone());
            // B1-store: persist composite_score for post_edit HealthDiff regression detection
            runtime.ctx.result_cache.cache_result(
                "pre_edit",
                &snap_key,
                format!("{:.4}", health.composite_score),
            );
            // EC52: Also store full health JSON so post_edit can call to_health_diff()
            // for a typed HealthDiff (richer than float-only delta).
            let json_snap_key = format!("__pre_edit_health_json__:{}", rel_path);
            runtime.ctx.result_cache.cache_result(
                "pre_edit",
                &json_snap_key,
                health.to_json_pretty(),
            );
            if !context.is_empty() {
                context.push_str(" | ");
            }
            context.push_str(&sig);
        } else if let Some(cached_sig) = cached {
            if !context.is_empty() {
                context.push_str(" | ");
            }
            context.push_str(&cached_sig);
            // B1-store warm path: re-store from cached score (keeps snapshot fresh for post_edit)
            if let Some(score_str) = cached_sig
                .split('(')
                .nth(1)
                .and_then(|s| s.strip_suffix(')'))
            {
                runtime
                    .ctx
                    .result_cache
                    .cache_result("pre_edit", &snap_key, score_str.to_string());
            }
        }
    }

    // ── EC-P: Pensieve lookup — past failure pattern detection pre-edit ──
    // Checks if this file's path has similarity to paths that previously triggered
    // failures recorded in the Pensieve ANN index (via post_bash fail recording).
    // Uses the same command_to_states hashing as pre_bash.rs for consistent embeddings.
    {
        let states = crate::shared::command_hash::command_to_states(&rel_path);
        if !states.is_empty()
            && let Ok(pensieve) = runtime.learning.pensieve.try_borrow()
        {
            let penalty = match states.first() {
                Some(&single) if states.len() == 1 => pensieve.check_known_failure(single),
                _ => pensieve.check_known_failure_seq(&states),
            };
            if let Some(sim) = penalty {
                let sig = format!(
                    "⚠ pensieve: similar path had recorded failures ({:.0}% match)",
                    sim * 100.0
                );
                if !context.is_empty() {
                    context.push_str(" | ");
                }
                context.push_str(&sig);
            }
        }
    }

    // Budget-aware assembly via SignalPipeline: sort by score, truncate to CILA budget.
    // Wrapping the assembled context in a single StaticSignalLayer gives consistent
    // budget enforcement across CILA levels without restructuring existing signal logic.
    let context = if context.is_empty() {
        String::new()
    } else {
        let budget = cila_budget_edit(cila_level);
        let pipeline = SignalPipeline::new(budget).add_layer(StaticSignalLayer::new(
            "pre_edit_assembled",
            vec![(1.0_f32, context)],
        ));
        pipeline
            .execute(
                &SignalContext::new(&rel_path, "")
                    .with_cila(cila_level as usize)
                    .with_hook("pre_edit"),
            )
            .unwrap_or_default()
    };

    // D1.6: Extract context_len before the match moves `context`.
    let context_len = context.len();
    let result = match context.is_empty() {
        true => HookResponse::Allow,
        false => {
            // A6/A8: Store hook result for chaining via SessionBus hook_results.
            // post_edit and other hooks can retrieve via get_last_hook_result("pre_edit").
            let result_json = serde_json::json!({
                "file_path": rel_path,
                "context_len": context.len(),
            });
            {
                let mut bus = runtime.ctx.session_bus.borrow_mut();
                bus.add_hook_result("pre_edit", result_json);
            }
            HookResponse::Context {
                context,
                event_name: Some("PreToolUse".to_string()),
            }
        }
    };
    // D1.6: Emit activity event before returning (both Allow and Context paths).
    crate::activity_hook::emit_pre_edit(&runtime.project_root, &rel_path, context_len);
    result
}

/// Compose edit impact context.
///
/// Follows HIGH-SIGNAL-ONLY philosophy — only inject information that
/// changes Claude's editing approach:
///
/// 1. **Dependents** (invisible without graph) — impact awareness
/// 2. **Quality gate failures** (prevents trial-and-error BLOCK loops)
/// 3. **Notes/gotchas** (accumulated non-visible knowledge)
/// 4. **Edit frequency** (churn awareness)
pub fn compose_edit_context(
    runtime: Option<&HookRuntime>,
    db: &FileKnowledgeDB,
    file_path: &str,
) -> Option<String> {
    // Wave 13: Unified Observability — record pre_edit enter/exit hop in span context.
    let enter_us = crate::shared::span_context::timestamp_us();
    let result = compose_edit_context_impl(runtime, db, file_path);
    let exit_us = crate::shared::span_context::timestamp_us();
    if let Some(rt) = runtime {
        rt.record_span_layer("pre_edit_ctx", enter_us, exit_us);
    }
    result
}

fn compose_edit_context_impl(
    runtime: Option<&HookRuntime>,
    db: &FileKnowledgeDB,
    file_path: &str,
) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();

    // ── Signal 1: Dependents (who imports this file?) ──
    if let Ok(dependents) = db.get_dependents(file_path)
        && !dependents.is_empty()
    {
        let dep_files: Vec<&str> = dependents
            .iter()
            .take(5)
            .map(|r| r.source.as_str())
            .collect();
        parts.push(format!(
            "impact: {} file(s) import this [{}]",
            dependents.len(),
            short_list(&dep_files)
        ));
    }

    // ── Signal I-5: Callgraph enrichment — who calls functions in this file ──
    // Reads file from disk + build_call_graph to find callers of top-level symbols.
    // Language-guarded (Rust + Python only via touring_code::ast::Lang). Fallback: .ok() = silent.
    if let Some(callgraph_sig) = callgraph_signal_for_file(file_path) {
        parts.push(callgraph_sig);
    }

    // ── Signal 2: Quality gate / lint failures on THIS file (HIGHEST priority) ──
    // Prevents trial-and-error: if ruff/pyright/code_standards previously failed
    // on this file, Claude needs to fix pre-existing violations WITH the edit,
    // otherwise the quality gate will BLOCK repeatedly.
    if let Ok(failures) = db.recent_failures_for_file(file_path, 5) {
        let lint_failures: Vec<_> = failures
            .iter()
            .filter(|f| {
                let cmd = f.command.to_lowercase();
                cmd.contains("ruff")
                    || cmd.contains("pyright")
                    || cmd.contains("lint")
                    || cmd.contains("code_standards")
            })
            .collect();

        if let Some(latest) = lint_failures.first() {
            let cmd_short = latest
                .command
                .split_whitespace()
                .next()
                .unwrap_or_debug("linter", "pre_edit: cmd_short fallback");
            let err = latest
                .error_pattern
                .as_deref()
                .unwrap_or_debug("violations found", "pre_edit: error_pattern fallback");
            let short_err = truncate_str(err, 100);
            parts.push(format!(
                "⚠️ quality: `{cmd_short}` previously failed — fix pre-existing violations WITH your edit to avoid BLOCK: {short_err}"
            ));
        }
    }

    // ── Signal 3: Notes/gotchas (accumulated knowledge) ──
    if let Ok(Some(k)) = db.lookup(file_path)
        && let Some(notes) = &k.notes
        && !notes.is_empty()
    {
        let short = truncate_str(notes, 100);
        parts.push(format!("note: {short}"));
    }

    // ── Signal 4: Edit frequency (churn awareness) ──
    if let Ok(edits) = db.recent_edits(file_path, 5)
        && edits.len() >= 3
    {
        parts.push(format!("{}x edited recently", edits.len()));
    }

    // ── Signal 5: File risk score (RL-computed historical failure rate) ──
    let risk = db.file_risk_score(file_path);
    if risk >= 0.3 {
        let level = if risk >= 0.5 { "HIGH" } else { "MEDIUM" };
        parts.push(format!(
            "⚠ file_risk: {level} ({:.0}% failure rate after edits — verify carefully)",
            risk * 100.0
        ));
    }

    // ── Signal 6: Gotcha patterns (audit-learned anti-patterns) ──
    let gotchas = db.get_gotchas_for_file(file_path);
    for g in gotchas.iter().take(2) {
        let short_gotcha = truncate_str(&g.gotcha, 120);
        parts.push(format!("⚠ GOTCHA [{}]: {}", g.severity, short_gotcha));
        // Track gotcha hits for pattern learning
        db.increment_gotcha_hit(g.id);
    }

    // ── Signal 7: Pre-edit prevention (decay-weighted cross-session patterns) ──
    if let Some(prevention_ctx) = pre_edit_prevention::compose_pre_edit_warning(db, file_path) {
        // Prevention module already filters for high-confidence only.
        // Deduplicate: skip if gotchas already covered the same info.
        let has_gotcha = parts.iter().any(|p| p.to_uppercase().contains("GOTCHA"));
        let prevention_has_gotcha = prevention_ctx.to_uppercase().contains("GOTCHA");
        if !has_gotcha || !prevention_has_gotcha {
            parts.push(format!(
                "prevention: {}",
                truncate_str(&prevention_ctx, 200)
            ));
        }
    }

    // ── Signal 7a (D5.7): Entity disambiguation — warn on ambiguous pub API changes ──
    // Uses the project-local EntityRegistry to detect when an edit targets a generic
    // symbol name (Handler, Index, Manager, etc.) that has multiple definitions.
    // Skips silently when runtime is unavailable (graceful degradation).
    if let Some(rt) = runtime
        && let Some(entity_hint) = rt.infra.entity_registry.borrow().as_ref()
        && let Ok(true) = entity_hint.is_generic(file_path)
        && let Ok(result) = entity_hint.resolve(file_path, None, None, 5)
        && result.disambiguated_count > 1
    {
        let hint = format!(
            "⚠ ENTITY AMBIGUITY: '{}' has {} definitions. Best match: {} ({}, line {}), confidence {:.2}",
            file_path,
            result.disambiguated_count,
            result
                .candidates
                .first()
                .map(|c| c.entity_code.as_str())
                .unwrap_or("?"),
            result
                .candidates
                .first()
                .map(|c| c.module_path.as_str())
                .unwrap_or("?"),
            result.candidates.first().map(|c| c.line).unwrap_or(0),
            result
                .candidates
                .first()
                .map(|c| c.confidence)
                .unwrap_or(0.0),
        );
        parts.push(hint);
        let _ = entity_hint.bump_pattern_hit(file_path);
    }

    // ── Signal 8: Error predictions (Markov-based proactive warnings) ──
    {
        let mut predictor = ErrorPredictor::new();
        let learned = predictor.train_from_db(db);
        if learned > 0 {
            predictor.record_edit(file_path, "Edit");
            if let Some(pred) = predictor.predict() {
                parts.push(format!(
                    "⚠ PREDICTED: {}% chance of '{}' ({}x observed)",
                    (pred.probability * 100.0) as u32,
                    truncate_str(&pred.error_pattern, 60),
                    pred.observations
                ));
            }
        }
    }

    // ── Signal 9: Quality evolution (AST-driven proactive improvement suggestions) ──
    // Reads the CURRENT file from disk, computes quality metrics via AST,
    // and injects specific improvement suggestions for the LLM to apply
    // as part of the edit — creating an evolutionary self-improvement loop.
    if let Some(quality_ctx) = compose_quality_evolution(runtime, file_path, file_path) {
        parts.push(quality_ctx);
    }

    // ── Signal 10: File symbols overview (structural map for self-contained context) ──
    // Gives the LLM a complete map of the file so it doesn't need to issue
    // additional Read calls. This is the key "no more searching" enabler.
    if let Some(overview) = compose_file_overview(file_path) {
        parts.push(overview);
    }

    // ── Signal 12 (Wave 5, 2026-04-18): Rust workflow PROACTIVE advisory ──
    // Mirrors the post_edit V6 check BUT fires BEFORE the edit takes
    // effect — reads the current file state via `wave5_workflow` and
    // surfaces pub_surface, complexity band, unsafe/async counts so
    // Claude Code can adjust its plan (e.g. decompose before
    // refactoring, call out breaking API surface risk).
    //
    // Non-blocking: the hint is advisory only. Skips silently when the
    // file is non-Rust, empty, or trivially simple.
    if let Some(workflow_hint) = compose_rust_workflow_advisory(file_path) {
        parts.push(workflow_hint);
    }

    // ── Wave 10/11 (2026-04-18): Record pre-edit signals for post_edit delta ──
    // Non-blocking: reads the file from disk and caches a unified
    // quality score in `[0.0, 1.0]` so the matching `post_edit` can
    // compute a signed delta. Wave 11 widens this from Rust-only
    // (`record_pre_health`, syn) to multi-lang (`record_pre_signals`,
    // syn for Rust + tree-sitter for Python/TS/TSX/JS/Bash/Go/…).
    if let Ok(pre_src) = std::fs::read_to_string(file_path) {
        let _ = crate::health_delta::record_pre_signals(file_path, &pre_src);
    }

    // ── Wave 14 (2026-04-18): Surface streak warnings to CC ───────────
    // When the per-path regression streak crosses STREAK_ALERT_THRESHOLD
    // (=3 consecutive declines), inject a textual ⚠ warning so CC sees
    // the trend in the next edit context. Symmetrical positive
    // confirmation when the file is on an improvement streak.
    // Both helpers return `None` below threshold → silent skip.
    if let Some(warn) = crate::health_delta::streak_warning_hint(file_path) {
        parts.push(warn);
    } else if let Some(positive) = crate::health_delta::improvement_streak_hint(file_path) {
        parts.push(positive);
    }

    // ── Signal 11: Wiring Check — orphan pub symbols in this file ──
    if let Ok(status) = db.module_wiring_status(file_path)
        && !status.orphan_symbols.is_empty()
        && status.integration_score < 1.0
    {
        let orphan_list = status.orphan_symbols.join(", ");
        let short = truncate_str(&orphan_list, 80);
        // R6-S1: Suggest generator CLI to scaffold a wiring plan for orphans.
        // Mirrors R5-S4 (post_write) — surfaces the automation trigger at pre-edit
        // time too, so Claude Code can choose to generate a consumer before editing.
        let first_orphan = status
            .orphan_symbols
            .first()
            .map(String::as_str)
            .unwrap_or("symbol");
        parts.push(format!(
                "wiring({:.0}%): {} orphan pub symbol(s) [{}] — run: touring generate plan-suggest --intent \"wire {} into a consumer caller\"",
                status.integration_score * 100.0,
                status.orphan_symbols.len(),
                short,
                first_orphan,
            ));
    }

    // ── Signal 6c: Ecosystem fit — warn about modules with low integration ──
    {
        let low_mods = crate::ecosystem::low_integration_modules(db, 0.5);
        // Only include modules relevant to the current file's dependency graph
        let relevant: Vec<_> = low_mods
            .iter()
            .filter(|(path, _)| {
                // Show if the current file IS a low-integration module, or
                // if this file imports from a low-integration module
                path == file_path
                    || db
                        .get_dependents(path)
                        .ok()
                        .map(|deps| deps.iter().any(|d| d.source == file_path))
                        .unwrap_or_debug(false, "pre_edit: dependent check fallback")
            })
            .take(3)
            .collect();
        if !relevant.is_empty() {
            let mod_list: Vec<String> = relevant
                .iter()
                .map(|(path, score)| format!("{}({:.0}%)", path, score * 100.0))
                .collect();
            parts.push(format!(
                "ecosystem: {} low-integration module(s) [{}]",
                relevant.len(),
                mod_list.join(", "),
            ));
        }
    }

    // ── Signal 6d: Entry point guard — warn when editing a project anchor file ──
    // EC53: First production caller of ecosystem::entry_points().
    // Entry points (main.rs, lib.rs) are the project's public API boundary —
    // editing them has maximum blast radius. This signal surfaces that context.
    {
        let eps = crate::ecosystem::entry_points(db);
        if eps.iter().any(|ep| ep == file_path) {
            parts.push(format!(
                "entry-point: '{}' is a project anchor ({} registered) — edits here have maximum blast radius",
                file_path,
                eps.len(),
            ));
        }
    }

    // ── Signal 12: Co-edit neighbors (temporal coupling signal) ──
    // Files frequently edited together are semantically coupled even without
    // explicit import edges. Warns LLM of likely cascade effects so related
    // files can be updated in the same editing session.
    {
        let coedit_neighbors = db.get_coedit_neighbors(file_path, 5);
        if !coedit_neighbors.is_empty() {
            let names: Vec<&str> = coedit_neighbors.iter().map(|(p, _)| p.as_str()).collect();
            parts.push(format!(
                "co-edits: {} file(s) frequently edited together [{}]",
                coedit_neighbors.len(),
                short_list(&names)
            ));
        }
    }

    // ── Signal 13: Functional chain status — broken / active chains ──
    // Broken chains (weight 2.0) are the most critical signal: a previously-valid
    // data-flow path is now severed, likely by a prior edit that removed a symbol.
    // Active chains surface the file's role in the broader data-flow graph so
    // Claude can preserve invariants while editing.
    if let Some((_, chain_sig)) = crate::functional_wiring::functional_chain_signal(db, file_path) {
        parts.push(chain_sig);
    }

    // ── Signal 14: Tantivy BM25 — module-level context from related files ──
    // Mirrors pre_read/pre_write: surface related docstrings, symbol kinds, and
    // crate-origin siblings so Claude edits in context of the surrounding module.
    // Feature-gated (tantivy-fts is ON by default in touring-hooks).
    #[cfg(feature = "tantivy-fts")]
    {
        if let Some((_, s)) = crate::shared::signals::tantivy_related_docs_signal(runtime.map(|r| r.project_root.as_path()), file_path) {
            parts.push(s);
        }
        if let Some((_, s)) = crate::shared::signals::tantivy_kind_context_signal(runtime.map(|r| r.project_root.as_path()), file_path) {
            parts.push(s);
        }
        if let Some((_, s)) = crate::shared::signals::tantivy_crate_origin_signal(runtime.map(|r| r.project_root.as_path()), file_path) {
            parts.push(s);
        }
    }

    // ── Signal 15: Extended metadata — test coverage + community affinity ──
    // coverage_pct surfaces files with low test coverage before editing so Claude
    // knows to add tests. community_id reveals module grouping for architectural context.
    if let Ok(Some(ext)) = db.query_extended(file_path)
        && let Some(sig) = hook_helpers::build_file_meta_signal(&ext)
    {
        parts.push(sig);
    }

    if parts.is_empty() {
        return None;
    }

    Some(parts.join(" | "))
}

/// Compose quality evolution context using AST analysis.
///
/// This is the **self-improvement engine**: it reads the file being edited,
/// computes quality metrics, and returns specific actionable suggestions
/// that the LLM should apply alongside the requested edit.
///
/// The evolutionary loop:
/// 1. post_read stores quality baseline in knowledge DB
/// 2. pre_edit (HERE) reads current quality and suggests improvements
/// 3. LLM applies improvements alongside the edit
/// 4. post_edit measures delta and feeds reward to RL
fn compose_quality_evolution(
    runtime: Option<&HookRuntime>,
    file_path: &str,
    rel_path: &str,
) -> Option<String> {
    // Skip test files — unwrap/assert/loose functions are idiomatic in tests
    let path_lower = file_path.to_lowercase();
    if path_lower.contains("/tests/")
        || path_lower.contains("_test.")
        || path_lower.contains("test_")
    {
        return None;
    }

    // B-301: Compute blast_count here so it's in scope for the B-301 gate below.
    // Uses session_bus cache first (FA-1), then falls back to fresh computation.
    let blast_count: usize = {
        let rt = runtime?;
        if let Some(count) = rt.ctx.session_bus.borrow().get_blast_radius(rel_path) {
            count
        } else if let Some(idx) = rt.infra.symbol_index.as_ref() {
            crate::shared::signals::blast_radius_file_count(Some(idx), rel_path).unwrap_or(0)
        } else {
            0
        }
    };

    // Parse file via AST for quality metrics
    let source = std::fs::read_to_string(file_path).ok()?;
    let metrics = super::ast_bridge::analyze_file_quality(&source, file_path)?;

    let mut suggestions: Vec<String> = Vec::new();

    // High complexity functions — actionable: decompose
    if !metrics.complex_symbols.is_empty() {
        let names = metrics
            .complex_symbols
            .iter()
            .take(3)
            .map(|s| s.as_str())
            .collect::<Vec<_>>()
            .join(", ");
        suggestions.push(format!("CC>{}: [{}] — consider decomposing", 10, names));
    }

    // Overall quality score: avg complexity threshold
    if metrics.avg_complexity > 8.0 {
        suggestions.push(format!(
            "avg_CC={:.1} (target <8) — simplify logic paths",
            metrics.avg_complexity
        ));
    }

    // TDG grade signal (S1): compute composite quality grade and warn on D/F.
    //
    // Uses complexity as the primary dimension (available from `metrics`);
    // remaining dimensions default to neutral (1.0 = no penalty) since we
    // only have file-level AST data here (no coverage/churn/duplication DB).
    // This produces a conservative grade: D/F only fires on genuinely complex
    // files, not on coverage gaps the AST layer cannot observe.
    //
    // Wave 12 (2026-04-27): the anonymous block was dissolved so `tdg` survives
    // into the B-301 gate below — promoting B-301 from a 1-dim avg_complexity
    // proxy to the 6-dim TDG composite already computed here.
    let complexity_score =
        (1.0_f64 - (metrics.avg_complexity as f64 / 20.0).min(1.0)).clamp(0.0, 1.0);
    // Each high-complexity symbol contributes a 10% penalty, capped at 40%.
    let antipatterns_score =
        (1.0_f64 - (metrics.high_complexity_count as f64 * 0.10).min(0.40)).clamp(0.0, 1.0);
    let tdg = touring_analysis::quality::TdgReport::from_components(
        complexity_score,
        1.0, // coverage — no data at pre-edit time
        1.0, // duplication — no data at pre-edit time
        0.0, // churn — neutral (no FileKnowledgeDB at this call-site)
        1.0, // entropy — no Rust semantic signals at this call-site
        antipatterns_score,
    );
    // to_diagnostic_opt() returns Some only for grades D and F.
    if let Some(diag) = tdg.to_diagnostic_opt() {
        // Wave 8 S5 (synergy maximization): emit Q-201/Q-202 RFC-100
        // diagnostic via tracing — completes the loop from TDG signal
        // (Wave S1, v4.12) to structured RFC-100 emission. Previously
        // the grade was only surfaced as a suggestion string; now it
        // is also a machine-readable code consumable by gate_metrics
        // counters and downstream observability tools.
        tracing::warn!(
            code = %diag.code,
            severity = %diag.severity,
            message = %diag.message,
            grade = %tdg.grade_letter(),
            composite = tdg.composite,
            file_path = %file_path,
            "TDG grade triggered Q-2xx diagnostic"
        );
        suggestions.push(format!(
            "TDG: grade {} ({:.2}) — {}",
            tdg.grade_letter(),
            tdg.composite,
            tdg.grade.recommended_action(),
        ));
    }

    // Q-230: High antipattern density — actual antipattern hits vs statement count.
    // Compute antipattern rate: antipattern hits / estimated statements.
    let antipattern_hits =
        touring_analysis::quality::antipatterns::detect_antipatterns(&source, "rust");
    let antipattern_count = antipattern_hits.len();
    // Use symbol count as proxy for statement count (capped to avoid division issues).
    let statement_count = (metrics.symbol_count as f64).clamp(1.0, 1000.0);
    let antipattern_rate = (antipattern_count as f64 / statement_count).min(1.0);
    const Q230_THRESHOLD: f64 = 0.30;
    if antipattern_rate > Q230_THRESHOLD {
        use touring_analysis::quality::QualityFinding;
        let finding = QualityFinding::HighAntipatternDensity {
            file: file_path.to_string(),
            antipattern_rate,
            threshold: Q230_THRESHOLD,
        };
        let diag = finding.to_diagnostic();
        tracing::warn!(
            code = %diag.code,
            severity = %diag.severity,
            message = %diag.message,
            file_path = %file_path,
            antipattern_rate = %format!("{:.1}%", antipattern_rate * 100.0),
            threshold = %format!("{:.1}%", Q230_THRESHOLD * 100.0),
            "Q-230 HighAntipatternDensity emitted"
        );
        suggestions.push(format!(
            "Q-230: antipattern rate {:.1}% (> {:.0}%) — fix unwrap/todo/panic patterns",
            antipattern_rate * 100.0,
            Q230_THRESHOLD * 100.0
        ));
    }

    // Q-240: High cyclomatic complexity — CC > 20 in any symbol.
    const Q240_THRESHOLD: usize = 20;
    if metrics.max_complexity > Q240_THRESHOLD as u16 {
        use touring_analysis::quality::QualityFinding;
        let finding = QualityFinding::HighCyclomatic {
            file: file_path.to_string(),
            cyclomatic_complexity: metrics.max_complexity as usize,
            threshold: Q240_THRESHOLD,
        };
        let diag = finding.to_diagnostic();
        tracing::warn!(
            code = %diag.code,
            severity = %diag.severity,
            message = %diag.message,
            file_path = %file_path,
            max_complexity = metrics.max_complexity,
            threshold = Q240_THRESHOLD,
            "Q-240 HighCyclomatic emitted"
        );
        suggestions.push(format!(
            "Q-240: CC={} (max > {}) — decompose high-complexity symbols",
            metrics.max_complexity, Q240_THRESHOLD
        ));
    }

    // B-301: RefactorRequired — fire when blast_count > 20 AND TDG composite < 0.4.
    //
    // Wave 12 (2026-04-27): consumes `tdg.composite` (6-dim) computed above instead
    // of recomputing a 1-dim avg_complexity proxy locally. This makes B-301
    // consistent with Q-201/Q-202 grade emission and captures coverage,
    // duplication, churn, entropy, and antipattern dimensions that the previous
    // local recompute ignored. Threshold 0.4 unchanged (Wave 11 spec).
    const B301_BLAST_THRESHOLD: usize = 20;
    const B301_QUALITY_THRESHOLD: f64 = 0.40;
    if blast_count > B301_BLAST_THRESHOLD && tdg.composite < B301_QUALITY_THRESHOLD {
        use touring_analysis::blast_radius::BlastWarning;
        let finding = BlastWarning::RefactorRequired {
            file: file_path.to_string(),
            quality_score: tdg.composite,
            blast_radius: blast_count,
        };
        let diag = finding.to_diagnostic();
        tracing::warn!(
            code = %diag.code,
            severity = %diag.severity,
            message = %diag.message,
            file_path = %file_path,
            blast_count,
            quality_score = %format!("{:.2}", tdg.composite),
            grade = %tdg.grade_letter(),
            "B-301 RefactorRequired: high blast ({blast_count}) + low TDG composite ({:.2}, grade {})",
            tdg.composite,
            tdg.grade_letter()
        );
        suggestions.push(format!(
            "B-301: blast={} + TDG={:.2} (grade {}) — refactor before editing",
            blast_count,
            tdg.composite,
            tdg.grade_letter()
        ));
    }

    if suggestions.is_empty() {
        return None;
    }

    Some(format!("quality_evolution: {}", suggestions.join("; ")))
}

/// Signal 12 (Wave 5): proactive Rust workflow advisory for pre-edit.
///
/// Reads the CURRENT file state (before the edit is applied) and runs
/// the `wave5_workflow::rust_workflow_hint` helper. Returns `None` for
/// non-Rust files, missing/empty files, or trivially simple Rust that
/// would not benefit from an advisory.
///
/// This complements `compose_quality_evolution` (which reports
/// tree-sitter-level complexity) with `syn`-level semantic depth —
/// generics, trait bounds, async/unsafe counts.
///
/// Format example:
/// ```text
/// ⚙ rust-workflow: pub_surface=7 complexity=0.42 (complex) unsafe=1 async_fns=3
/// ```
fn compose_rust_workflow_advisory(file_path: &str) -> Option<String> {
    // Skip large files to stay within pre-edit's ~2s budget.
    let metadata = std::fs::metadata(file_path).ok()?;
    if metadata.len() > 100_000 {
        return None;
    }
    // Wave 5.1: delegate to the multi-language `code_workflow_hint` so
    // Python/TS/TSX/JS/Bash files receive a language-aware advisory in
    // pre_edit just like Rust does. `None` source arg lets the helper
    // read from disk. Returns `None` for unknown extensions.
    crate::wave5_workflow::code_workflow_hint(file_path, None)
}

/// Signal 10: File symbols overview — compact structural map.
///
/// Gives the LLM a complete structural overview of the file being edited
/// so it doesn't need to issue additional Read/Grep calls to understand
/// what's in the file. This is the key enabler for "self-contained context".
///
/// Format: `📋 overview: (Nf funcs, Nc classes, NL lines) [name:L10-50, name:L60-80, ...]`
///
/// Performance: capped at 100KB file size and 20 symbols to avoid timeout.
fn compose_file_overview(file_path: &str) -> Option<String> {
    // Skip very large files to avoid timeout (pre-edit has 2s budget)
    let metadata = std::fs::metadata(file_path).ok()?;
    if metadata.len() > 100_000 {
        return Some("📋 overview: file >100KB — use Read tool for structure".to_string());
    }

    let source = std::fs::read_to_string(file_path).ok()?;
    let symbols = super::ast_bridge::extract_enriched_symbols(&source, file_path)?;

    if symbols.is_empty() {
        return None;
    }

    let line_count = source.lines().count();

    // Count by kind category
    let func_count = symbols
        .iter()
        .filter(|s| {
            matches!(
                s.kind,
                touring_code::ast::SymbolKind::Function
                    | touring_code::ast::SymbolKind::AsyncFunction
                    | touring_code::ast::SymbolKind::Method
            )
        })
        .count();
    let type_count = symbols
        .iter()
        .filter(|s| {
            matches!(
                s.kind,
                touring_code::ast::SymbolKind::Class
                    | touring_code::ast::SymbolKind::Struct
                    | touring_code::ast::SymbolKind::Enum
                    | touring_code::ast::SymbolKind::Trait
                    | touring_code::ast::SymbolKind::Interface
            )
        })
        .count();

    // Build compact symbol list: top-level first, then methods.
    // Show up to 20 symbols with kind abbreviation.
    let mut parts: Vec<String> = Vec::with_capacity(20);
    let mut unknown_count: usize = 0;
    for s in symbols.iter().take(20) {
        let kind_abbr = match &s.kind {
            touring_code::ast::SymbolKind::Class => "cls",
            touring_code::ast::SymbolKind::Struct => "struct",
            touring_code::ast::SymbolKind::Enum => "enum",
            touring_code::ast::SymbolKind::Trait => "trait",
            touring_code::ast::SymbolKind::Interface => "iface",
            touring_code::ast::SymbolKind::Function
            | touring_code::ast::SymbolKind::AsyncFunction => "fn",
            touring_code::ast::SymbolKind::Method => "method",
            touring_code::ast::SymbolKind::Constant | touring_code::ast::SymbolKind::Static => {
                "const"
            }
            touring_code::ast::SymbolKind::Other(_) => {
                unknown_count += 1;
                ""
            }
            _ => "",
        };
        if kind_abbr.is_empty() {
            parts.push(format!("{}:L{}-{}", s.name, s.line, s.end_line));
        } else {
            parts.push(format!(
                "{} {}:L{}-{}",
                kind_abbr, s.name, s.line, s.end_line
            ));
        }
    }

    // P2: Warn when file has uncategorized symbols (language not fully supported)
    if unknown_count > 0 {
        parts.push(format!("\u{26a0}\u{fe0f} {} symbol(s) unknown lang — language may lack full tree-sitter support", unknown_count));
    }

    let overflow = if symbols.len() > 20 {
        format!(" +{} more", symbols.len() - 20)
    } else {
        String::new()
    };

    Some(format!(
        "📋 overview: ({func_count}fn, {type_count}types, {line_count}L) [{}]{overflow}",
        parts.join(", ")
    ))
}

/// Heuristic: does old_string appear to replace an entire function/method?
/// Checks for function-defining keywords + return/closing pattern.
pub(crate) fn edit_spans_entire_function(old_string: &str) -> bool {
    let has_fn_start = old_string.contains("def ")
        || old_string.contains("fn ")
        || old_string.contains("function ")
        || old_string.contains("pub fn ")
        || old_string.contains("async fn ");
    let has_fn_end = old_string.contains("return ")
        || old_string.trim_end().ends_with('}')
        || old_string.contains("\n}\n");
    // At least 3 lines to qualify as "entire function"
    let line_count = old_string.lines().count();
    has_fn_start && has_fn_end && line_count >= 3
}

/// If old_string spans an entire function, suggest using touring_ast_edit.
pub(crate) fn suggest_replace_symbol_body(old_string: &str) -> String {
    if !edit_spans_entire_function(old_string) {
        return String::new();
    }
    "💡 Touring: `mcp__touring__touring_ast_edit` é mais preciso \
     para substituir funções completas — garante sintaxe e referências"
        .to_string()
}

/// Check new_string content for known Rust anti-patterns when editing .rs files.
fn check_rust_antipatterns(new_string: &str) -> Vec<String> {
    let mut warnings: Vec<String> = Vec::new();

    // Pattern 1: Unsafe UTF-8 string slicing — &s[..N] or &s[..expr.min(N)]
    // Matches: &foo[..100], &s[..s.len().min(N)], &bar[..bar.len().min(200)]
    if new_string.contains("[..") && !new_string.contains("truncate_str") {
        // Check if it looks like string slicing (not array/vec slicing)
        let lines: Vec<&str> = new_string.lines().collect();
        for line in &lines {
            let trimmed = line.trim();
            if trimmed.contains("[..") && trimmed.contains(".len()") && trimmed.contains(".min(") {
                warnings.push(
                    "RUST ANTIPATTERN: &s[..s.len().min(N)] can panic on multi-byte UTF-8. Use touring_foundation::truncate_str(s, N)".to_string()
                );
                break;
            }
        }
    }

    // Pattern 2: Direct byte-index slicing on strings
    // Matches: &err[..100], &notes[..120] (literal index, not variable)
    // Lazy: compiled once, reused across all calls (0μs after first). MSRV-safe (1.75+).
    static RE_DIRECT_SLICE: once_cell::sync::Lazy<regex::Regex> =
        once_cell::sync::Lazy::new(|| {
            regex::Regex::new(r#"&\w+\[\.\.\d+\]"#).expect("static regex is valid")
        });
    if RE_DIRECT_SLICE.is_match(new_string) {
        warnings.push(
            "RUST ANTIPATTERN: &var[..N] with literal index can panic on UTF-8. Use truncate_str()"
                .to_string(),
        );
    }

    warnings
}

/// Detect type references in new content that might need importing.
///
/// Scans for PascalCase identifiers not covered by the file's current imports
/// or local symbols. If a match exists in the wiring_map (known pub symbol from
/// another module), suggests the specific `use` path.
///
/// B2 fix: delegates suggestion construction to `touring_code::ast::wiring::suggest_imports`.
/// B4 fix: queries `all_pub_symbols()` (not just orphans) for full recall.
fn detect_unresolved_types(
    new_content: &str,
    db: &FileKnowledgeDB,
    current_file: &str,
) -> Vec<String> {
    // Get current file's imports and local symbols from knowledge
    let current_imports: Vec<String> = db
        .lookup(current_file)
        .ok()
        .flatten()
        .and_then(|k| k.imports_json)
        .and_then(|j| serde_json::from_str(&j).ok())
        .unwrap_or_default();

    let current_symbols: Vec<String> = db
        .lookup(current_file)
        .ok()
        .flatten()
        .and_then(|k| k.symbols_json)
        .and_then(|j| serde_json::from_str::<Vec<serde_json::Value>>(&j).ok())
        .map(|syms| {
            syms.iter()
                .filter_map(|s| s.get("name").and_then(|n| n.as_str()).map(String::from))
                .collect()
        })
        .unwrap_or_default();

    // Build sets for O(1) lookup
    let imported_set: std::collections::HashSet<&str> = current_imports
        .iter()
        .filter_map(|i| i.rsplit("::").next())
        .collect();

    let local_set: std::collections::HashSet<&str> =
        current_symbols.iter().map(|s| s.as_str()).collect();

    // Step 1: Detect unresolved PascalCase identifiers in the new content
    let mut unresolved = Vec::new();
    for word in new_content.split(|c: char| !c.is_alphanumeric() && c != '_') {
        if word.len() >= 2
            && word.chars().next().is_some_and(|c| c.is_uppercase())
            && !word.chars().all(|c| c.is_uppercase() || c == '_')
            && !imported_set.contains(word)
            && !local_set.contains(word)
            && !is_rust_builtin(word)
            && !unresolved.contains(&word.to_string())
        {
            unresolved.push(word.to_string());
        }
    }

    if unresolved.is_empty() {
        return Vec::new();
    }

    // Step 2: Get all pub symbols from wiring_map and build known_modules list
    let known_modules: Vec<(String, String)> = db
        .all_pub_symbols()
        .unwrap_or_default()
        .into_iter()
        .map(|e| (e.symbol_name, e.module_file))
        .collect();

    // Step 3: Use touring_code::ast::wiring::suggest_imports for suggestion construction (B2 fix)
    let ast_suggestions = touring_code::ast::wiring::suggest_imports(&unresolved, &known_modules);

    // Step 4: Format as user-facing strings
    ast_suggestions
        .iter()
        .map(|s| {
            format!(
                "`use {}::{}` (from {})",
                s.source_module, s.symbol_name, s.reason
            )
        })
        .collect()
}

/// Check if a name is a common builtin type that does not need importing.
fn is_rust_builtin(name: &str) -> bool {
    matches!(
        name,
        "String"
            | "Vec"
            | "Option"
            | "Result"
            | "Box"
            | "Arc"
            | "Mutex"
            | "RwLock"
            | "HashMap"
            | "HashSet"
            | "BTreeMap"
            | "BTreeSet"
            | "VecDeque"
            | "Path"
            | "PathBuf"
            | "Duration"
            | "Instant"
            | "Ok"
            | "Err"
            | "Some"
            | "None"
            | "Self"
            | "Default"
            | "Send"
            | "Sync"
            | "Clone"
            | "Debug"
            | "Display"
            | "Serialize"
            | "Deserialize"
            | "Value"
    )
}

/// Signal I-5: Build a callgraph context signal for a file being edited.
///
/// Reads the file from disk, finds the symbol with the most callers, then
/// delegates to `callgraph_enrichment::enrich_with_callgraph` for full
/// caller/callee/hotspot enrichment, formatted via `format_callgraph_context`.
///
/// Only fires for Rust and Python. Returns `None` on any I/O or parse error.
fn callgraph_signal_for_file(file_path: &str) -> Option<String> {
    // Detect language from extension — only Rust and Python supported
    let lang_str = match std::path::Path::new(file_path)
        .extension()
        .and_then(|e| e.to_str())
    {
        Some("rs") => "rust",
        Some("py") => "python",
        _ => return None,
    };

    let source = std::fs::read_to_string(file_path).ok()?;

    // Find the symbol with the most callers — that's the highest-impact target.
    // Build call graph once to identify the hot symbol, then enrich it fully.
    let lang: touring_code::ast::Lang = lang_str.parse().ok()?;
    let graph = touring_code::ast::call_graph::build_call_graph(&source, lang);

    let top_symbol: Option<String> = {
        let mut counts: Vec<(String, usize)> = graph
            .sites
            .iter()
            .fold(
                std::collections::HashMap::<String, std::collections::HashSet<String>>::new(),
                |mut acc, e| {
                    acc.entry(e.callee.clone())
                        .or_default()
                        .insert(e.caller.clone());
                    acc
                },
            )
            .into_iter()
            .map(|(callee, callers)| (callee, callers.len()))
            .collect();
        counts.sort_by_key(|b| std::cmp::Reverse(b.1));
        counts.into_iter().next().map(|(sym, _)| sym)
    };

    // Enrich via callgraph_enrichment module for caller/callee/hotspot detail.
    let sym = top_symbol.as_deref()?;
    let info = crate::callgraph_enrichment::enrich_with_callgraph(&source, lang_str, Some(sym))?;
    Some(crate::callgraph_enrichment::format_callgraph_context(
        &info, sym,
    ))
}

/// Format PII scan findings as a compact security signal for PreToolUse injection.
///
/// Shows count by severity: `PII: 2 high (cpf, cpf) | 1 medium (email)`.
/// High-severity findings (CPF, CNPJ, RG, CNH, SUS) are always shown.
/// Medium-severity (processos SEI, email) are shown if count is small.
/// Low-severity (phone) is suppressed to avoid noise.
fn format_pii_findings_context(findings: &[PIIFinding]) -> String {
    let high: Vec<_> = findings.iter().filter(|f| f.severity == "high").collect();
    let medium: Vec<_> = findings.iter().filter(|f| f.severity == "medium").collect();
    let _low: Vec<_> = findings.iter().filter(|f| f.severity == "low").collect();

    let mut parts = Vec::new();

    if !high.is_empty() {
        let unique: std::collections::HashSet<_> =
            high.iter().map(|f| f.pattern_name.as_str()).collect();
        parts.push(format!(
            "PII: {} high ({})",
            high.len(),
            unique.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }
    if !medium.is_empty() {
        // For medium, show count + first type to avoid verbosity
        let first = medium
            .first()
            .map(|f| f.pattern_name.as_str())
            .unwrap_or_debug("unknown", "pre_edit: PII first pattern fallback");
        let extra = if medium.len() > 1 {
            format!(" +{}", medium.len() - 1)
        } else {
            String::new()
        };
        parts.push(format!("PII: {} medium ({}{})", medium.len(), first, extra));
    }
    // Suppress low-severity (phone numbers) — too noisy for PreToolUse

    parts.join(" | ")
}

/// Shorten a list of file paths for display.
fn short_list(items: &[&str]) -> String {
    items
        .iter()
        .map(|p| {
            // Show just filename
            std::path::Path::new(p)
                .file_name()
                .and_then(|f| f.to_str())
                .unwrap_or_debug(p, "pre_edit: file_name fallback")
        })
        .collect::<Vec<_>>()
        .join(", ")
}

#[cfg(test)]
#[path = "pre_edit_tests.rs"]
mod tests;
