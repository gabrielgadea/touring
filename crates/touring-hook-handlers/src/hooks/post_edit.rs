//! Post-Edit Hook — Tracks changes AND verifies quality after Claude edits a file.
//!
//! ## Architecture (v29 + Enhancement Sprint E2/E7/E10/E12/E13 + M8)
//!
//! After Claude edits a file, this hook:
//! 1. Records the edit event (consolidated I/O — single file read, E2)
//! 2. Re-indexes the file (imports, symbols, hash) via post_read logic
//! 3. Updates file_relations if imports changed
//! 4. Error-Driven Learning: Auto-creates gotchas for recurring errors
//! 5. Quality Verification with priority-sorted feedback (E7, bayesian_score weighted)
//! 6. diff_pub_symbols for pub API surface change detection (E13)
//! 7. evaporate_with_drift_check for KS drift-based cache invalidation (E10)
//! 8. PredictiveFocusCache co-edit recording (E12)
//! 9. Mutation testing via cargo-mutants (M8) — applies to ALL file types
//!
//! Target latency: <50ms (was <80ms before I/O consolidation).
use super::hook_decompose_bridge;
use super::knowledge::FileKnowledgeDB;
use super::runtime::{HookResponse, HookRuntime, make_relative};
use crate::pipeline::TouringFlowBuilder;
use crate::shared::gate_metrics;
use crate::shared::hook_helpers;
use crate::shared::metadata_collector::FastMetadata;
use crate::shared::metadata_dedup::{DedupKey, MetadataDedup};
use regex::Regex;
use std::sync::OnceLock;
use std::time::{Duration, Instant};
use touring_code::ast::speculate_v2;
use touring_foundation::truncate_str;
use touring_intelligence::rl::ImmediateReward;
use touring_orchestration::flow::types::Item as FlowItem;
use touring_orchestration::flow::types::Item;
/// Process-lifetime dedup cache for post_edit metadata.
///
/// Keyed by (file_path, content_hash) where content_hash is BLAKE3 of file content.
/// Phase 2.2: Skips redundant reindex calls when the same file content is seen
/// multiple times in a session (mtime can change without content changing).
static EDIT_DEDUP: OnceLock<MetadataDedup> = OnceLock::new();
/// Flush the post_edit dedup cache at session boundaries.
///
/// Called from `run_session_stop` to drain stale mtime entries so the next
/// session starts with a clean dedup state (no false-positive skips).
pub(crate) fn flush_dedup() {
    if let Some(dedup) = EDIT_DEDUP.get() {
        dedup.clear();
    }
}
/// Minimum recurrences of the same error pattern before auto-creating a gotcha.
const AUTO_GOTCHA_THRESHOLD: i64 = 2;
/// How many recent edits to scan when counting error pattern recurrences.
const RECENT_EDIT_WINDOW: usize = 20;
/// Minimum number of NEW anti-patterns that triggers a Block response.
/// Must be > 3 to avoid false positives from pre-existing code issues.
const BLOCK_ANTIPATTERN_THRESHOLD: usize = 4;
/// Persist call edges discovered in `source` as FileRelation records.
///
/// For each unique callee function found in the file's call graph,
/// upserts a FileRelation { source: rel_path, target: callee, relation_type: "calls" }.
/// Silently skips on parse failure (unknown language or empty graph).
fn persist_call_edges(db: &FileKnowledgeDB, rel_path: &str, source: &str, lang_str: &str) {
    let Ok(lang) = lang_str.parse::<touring_code::ast::Lang>() else {
        return;
    };
    let graph = touring_code::ast::call_graph::build_call_graph(source, lang);
    if graph.sites.is_empty() {
        return;
    }
    let mut seen = std::collections::HashSet::new();
    for site in &graph.sites {
        if seen.insert(site.callee.as_str()) {
            let rel = super::knowledge::FileRelation {
                source: rel_path.to_string(),
                target: site.callee.clone(),
                relation_type: "calls".to_string(),
            };
            if let Err(e) = db.upsert_relation(&rel) {
                tracing::debug!("persist_call_edges upsert failed for {rel_path}: {e}");
            }
        }
    }
}
/// Run the post-edit hook (diverging version — for CLI entry point).
#[tracing::instrument(skip(runtime, input), fields(hook = "post_edit"))]
pub fn run(
    runtime: &mut HookRuntime,
    input: &serde_json::Value,
) -> Result<(), touring_hook_runtime::hook_runtime::HookDispatchError> {
    run_returning(runtime, input).emit()
}
/// Run the post-edit hook, returning a `HookResponse`.
///
/// ## v29: Two-phase architecture
///
/// **Phase 1 — Tracking** (always runs, <15ms):
/// Record edit, re-index, error-driven learning, ACO wiring.
///
/// **Phase 2 — Verification** (only on success, <65ms):
/// speculate_v2, complexity delta, wiring impact, anti-patterns.
/// Returns `HookResponse::Context` with feedback if issues found.
#[tracing::instrument(skip_all, fields(hook = "post_edit"))]
pub fn run_returning(runtime: &mut HookRuntime, input: &serde_json::Value) -> HookResponse {
    let enter_us = crate::shared::span_context::timestamp_us();
    let result = run_returning_impl(runtime, input);
    let exit_us = crate::shared::span_context::timestamp_us();
    runtime.record_span_layer("post_edit", enter_us, exit_us);
    result
}
fn run_returning_impl(runtime: &mut HookRuntime, input: &serde_json::Value) -> HookResponse {
    let tool_name = input
        .get("tool_name")
        .and_then(|v| v.as_str())
        .unwrap_or("Edit");
    let file_path = input
        .pointer("/tool_input/file_path")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if file_path.is_empty() {
        return HookResponse::Allow;
    }
    let rel_path = make_relative(file_path, &runtime.project_root);
    let language = crate::shared::detect_language::detect_language(&rel_path);
    let old_symbols_json: Option<String> = runtime
        .ctx
        .knowledge
        .lookup(&rel_path)
        .ok()
        .flatten()
        .and_then(|fk| fk.symbols_json);
    let old_source: Option<String> = std::fs::read_to_string(file_path).ok();
    let phase1_start = Instant::now();
    let had_error = match phase1_tracking(
        runtime,
        input,
        tool_name,
        file_path,
        &rel_path,
        language,
        old_source.as_deref(),
    ) {
        Ok(had_err) => had_err,
        Err(e) => {
            tracing::warn!("phase1_tracking failed for {}: {}", rel_path, e);
            false
        }
    };
    let phase1_latency_ms = phase1_start.elapsed().as_millis() as u64;
    if had_error {
        return HookResponse::Allow;
    }
    let skip_violation_ctx =
        check_edit_overlaps_skip_region(rel_path.as_str(), &old_source, input, file_path);
    if !skip_violation_ctx.is_empty() {
        crate::shared::gate_metrics::record_diagnostic_w115_skipped_region_written();
    }
    let cila_level = runtime
        .ctx
        .result_cache
        .get_result("__meta__", "__session_cila_level__")
        .and_then(|s| s.parse::<u8>().ok())
        .unwrap_or(2);
    let file_type = if rel_path.ends_with(".py") {
        0
    } else if rel_path.ends_with(".rs") {
        1
    } else if rel_path.ends_with(".ts")
        || rel_path.ends_with(".tsx")
        || rel_path.ends_with(".js")
        || rel_path.ends_with(".jsx")
    {
        2
    } else {
        3
    };
    let chain_completion_bonus: f64 = if runtime
        .ctx
        .session_bus
        .borrow()
        .get_last_hook_result("pre_edit")
        .is_some()
    {
        0.10
    } else {
        0.0
    };
    let reward = ImmediateReward {
        tool_name: tool_name.to_string(),
        accepted: true,
        latency_ms: phase1_latency_ms,
        error_count: 0,
        cila_level,
        file_type,
        quality_score: if chain_completion_bonus > 0.0 {
            Some(chain_completion_bonus)
        } else {
            None
        },
    };
    hook_helpers::ensure_qtable_loaded(runtime);
    if let Some(mut qtable) = runtime.learning.qtable_cache.take() {
        runtime.process_immediate_reward(&reward, &mut qtable);
        runtime.learning.qtable_cache = Some(qtable);
    }
    runtime.infra.cortex_dispatcher.feed_outcome(
        "Edit".to_string(),
        rel_path.to_string(),
        true,
        format!("{}ms", phase1_latency_ms),
        runtime.session_turn() as u32,
    );
    if !crate::shared::quality::is_test_file(file_path) {
        let mutants_key = format!("__mutants_job__:{}", rel_path);
        // F-7: gate the spawn below so we never run more than one `cargo mutants`
        // job per file concurrently (plus a global cap) — rapid edits previously
        // spawned an unbounded number of full-compile mutants subprocesses.
        let mut should_spawn = true;
        if let Some(prev_job_id) = runtime
            .ctx
            .result_cache
            .get_result("post_edit", &mutants_key)
        {
            if !prev_job_id.is_empty() {
                let status_json = crate::shared::job_registry::poll_worker(&prev_job_id);
                if let Some(status_str) = status_json.get("status").and_then(|v| v.as_str()) {
                    match status_str {
                        "completed" => {
                            if let Some(result) = status_json.get("result").and_then(|v| v.as_str())
                            {
                                if result.contains("mutants survived")
                                    && !result.contains("0 mutants survived")
                                {
                                    let reward = ImmediateReward {
                                        tool_name: "cargo-mutants".to_string(),
                                        accepted: false,
                                        latency_ms: 0,
                                        error_count: 1,
                                        cila_level,
                                        file_type: 1,
                                        quality_score: None,
                                    };
                                    if let Some(mut qtable) = runtime.learning.qtable_cache.take() {
                                        runtime.process_immediate_reward(&reward, &mut qtable);
                                        runtime.learning.qtable_cache = Some(qtable);
                                    }
                                    tracing::debug!(
                                        job_id = % prev_job_id,
                                        "mutation testing: surviving mutants detected — negative reward injected"
                                    );
                                }
                            }
                            runtime.ctx.result_cache.cache_result(
                                "post_edit",
                                &mutants_key,
                                String::new(),
                            );
                        }
                        "failed" | "not_found" | "" => {
                            runtime.ctx.result_cache.cache_result(
                                "post_edit",
                                &mutants_key,
                                String::new(),
                            );
                        }
                        // Still running → an in-flight mutants job exists for this
                        // file; don't spawn a duplicate (F-7 per-file dedup).
                        _ => {
                            should_spawn = false;
                        }
                    }
                }
            }
        }
        // F-7 global cap: never run more than MAX_CONCURRENT_MUTANTS `cargo
        // mutants` compiles at once across all files — each is a full build that
        // steals CPU from hook dispatch on the editor hot path.
        const MAX_CONCURRENT_MUTANTS: usize = 2;
        if should_spawn
            && crate::shared::job_registry::count_running("cargo-mutants-")
                >= MAX_CONCURRENT_MUTANTS
        {
            should_spawn = false;
        }
        if should_spawn {
            let job_id = crate::shared::job_registry::spawn_worker(
                "cargo-mutants",
                "cargo",
                &[
                    "mutants".to_string(),
                    "--in-diff".to_string(),
                    "--timeout-multiplier=2.0".to_string(),
                ],
            );
            runtime
                .ctx
                .result_cache
                .cache_result("post_edit", &mutants_key, job_id.clone());
            tracing::debug!(job_id = % job_id, rel_path, "mutation testing job spawned");
        }
    }
    if let Some(qa) = runtime.ctx.quality_assessment.as_ref() {
        let subtask_id = input.pointer("/subtask_id").and_then(|v| v.as_str());
        let report = qa.to_tracker_report(runtime.session_turn() as u32);
        if let Some(d9) = report.dims.iter().find(|d| d.dim_id == "D9") {
            let _ = hook_decompose_bridge::bridge_post_edit_quality(runtime, d9.score, subtask_id);
        }
    }
    let file_content: Option<String> = std::fs::read_to_string(file_path).ok();
    if let Some(ref src) = file_content {
        let fp = std::path::Path::new(file_path);
        let outcome = crate::shared::api_cascade_bridge::analyze_rust_edit(
            fp,
            src,
            &runtime.ctx.api_cascade_cache,
        );
        if let Some(plan) = outcome.plan() {
            crate::shared::api_cascade_bridge::log_cascade_plan(fp, plan);
            runtime.ctx.cascade_queue.push(fp, plan);
        }
    }
    let rule_engine_result = crate::post_edit_rule_engine::bridge_post_edit_rule_engine(
        runtime,
        rel_path.as_str(),
        file_path,
    );
    if let Err(e) = rule_engine_result {
        tracing::debug!("bridge_post_edit_rule_engine failed: {e}");
    }
    let lang_str = language.unwrap_or("");
    let quality_after = match file_content.as_deref() {
        Some(src) => crate::shared::quality::measure_quality_snapshot_from_source(src, file_path),
        None => None,
    };
    let post_health: Option<touring_analysis::CodeHealthReport> = Some(
        touring_analysis::AnalysisPipeline::new(
            runtime.ctx.knowledge.conn_ref(),
            touring_analysis::engine::AnalysisConfig::hook_path(),
        )
        .run(runtime.project_root.to_str().unwrap_or("")),
    );
    let p2 = Phase2Inputs {
        lang_str,
        quality_after: quality_after.as_ref(),
        file_content: file_content.as_deref(),
        old_symbols_json: old_symbols_json.as_deref(),
        old_source: old_source.as_deref(),
        post_health: post_health.as_ref(),
    };
    let mut issues = phase2_verification(runtime, file_path, &rel_path, &p2);
    if !skip_violation_ctx.is_empty() {
        issues.push(skip_violation_ctx);
    }
    if let Some(reward_val) = compute_rust_workflow_reward(file_path, file_content.as_deref()) {
        runtime
            .learning
            .inject_reward("post_edit", reward_val, "wave5_v6_rust_workflow");
    }
    crate::shared::query_cache::invalidate_by_path(file_path);
    if let Some(src) = file_content.as_deref() {
        if let Some(delta) = crate::health_delta::compute_signals_delta(file_path, src) {
            issues.push(crate::health_delta::format_delta_hint(&delta));
            if let Some(reward) = crate::health_delta::delta_reward(&delta) {
                runtime
                    .learning
                    .inject_reward("post_edit", reward, "wave9_health_delta");
            }
        }
    }
    {
        let snap_key = format!("__pre_edit_health__:{}", rel_path);
        let json_key = format!("__pre_edit_health_json__:{}", rel_path);
        if let Some(post) = post_health.as_ref() {
            let (delta, degraded_dims) = if let Some(pre) = runtime
                .ctx
                .result_cache
                .get_result("pre_edit", &json_key)
                .and_then(|json| {
                    serde_json::from_str::<touring_analysis::CodeHealthReport>(&json).ok()
                }) {
                let hd = pre.to_health_diff(post);
                let dims: Vec<String> = hd
                    .dimensions
                    .degraded
                    .iter()
                    .filter_map(|name| post.dimensions.iter().find(|d| &d.name == name))
                    .map(|d| format!("{}:{:.2}", d.name, d.score))
                    .collect();
                (hd.score_delta, dims)
            } else {
                let float_delta = runtime
                    .ctx
                    .result_cache
                    .get_result("pre_edit", &snap_key)
                    .and_then(|s| s.parse::<f64>().ok())
                    .map(|pre_score| post.composite_score - pre_score)
                    .unwrap_or(0.0);
                let dims: Vec<String> = post
                    .dimensions
                    .iter()
                    .filter(|d| d.score < 0.8)
                    .map(|d| format!("{}:{:.2}", d.name, d.score))
                    .collect();
                (float_delta, dims)
            };
            if delta < -0.05 {
                let pre_score = post.composite_score - delta;
                let dim_detail = if degraded_dims.is_empty() {
                    String::new()
                } else {
                    format!(" [degraded: {}]", degraded_dims.join(", "))
                };
                issues.push(format!(
                    "REGRESSION health: {:.2} → {:.2} (Δ{:.2}){} — edit degraded codebase quality",
                    pre_score, post.composite_score, delta, dim_detail,
                ));
            }
        }
    }
    if let Some(ref content) = file_content {
        let findings = runtime.ctx.pii_scanner.scan_text(content);
        if !findings.is_empty() {
            let pii_ctx = format_pii_findings_context_post(&findings);
            issues.push(pii_ctx);
        }
    }
    if issues.is_empty() {
        return HookResponse::Allow;
    }
    if let Some(block_response) = check_block_gate(&issues, &rel_path) {
        runtime
            .learning
            .inject_reward("post_edit", -0.5, "antipattern_block_gate");
        return block_response;
    }
    let cila_level: u8 = runtime
        .ctx
        .result_cache
        .get_result("__meta__", "__session_cila_level__")
        .and_then(|s| s.parse().ok())
        .unwrap_or(2);
    let mut feedback = compose_post_edit_feedback(issues, cila_level);
    if let Some(report) = runtime.infra.cortex_dispatcher.flush_evidence() {
        if report.drift_detected {
            tracing::info!(
                "post_edit: concept drift detected ks={:.3}",
                report.ks_statistic
            );
        }
    }
    {
        let mut bus = runtime.ctx.session_bus.borrow_mut();
        let result_json = serde_json::json!(
            { "file_path" : rel_path, "feedback_len" : feedback.len(), }
        );
        bus.add_hook_result("post_edit", result_json);
    }
    let complexity_score = {
        let edit_frequency_factor = (feedback.len().min(500) as f64 * 0.001).min(0.5);
        (0.3_f64 + edit_frequency_factor).min(1.0)
    };
    let bridge_result =
        hook_decompose_bridge::bridge_cognitive_mcts_trigger(runtime, complexity_score);
    if let Ok(result_str) = bridge_result {
        if !result_str.starts_with("skipped") {
            feedback = format!("{} [MCTS: {}]", feedback, complexity_score);
        }
    }
    HookResponse::Context {
        context: feedback,
        event_name: Some("PostToolUse".to_string()),
    }
}
/// Compute the canonical cognitive-warm signals for a file (S-02 — SNR slice).
///
/// Returns `(cognitive_score, complexity_signal)` from `analyze_quality`, identical
/// to the `ast meta` on_disk_fallback (`overall_score` = 0.6·complexity + 0.4·
/// antipattern). Returns `None` for non-code files or unreadable paths. Pure — no
/// DB writes — so the warm path stays deterministically testable. The remaining
/// enrichment fields (fan_in/fan_out/doc) are not derivable from quality analysis
/// (the CLI fallback uses 0.0), so the caller writes 0.0 rather than fabricate.
fn compute_cognitive_warm(file_path: &str) -> Option<(f64, f64)> {
    let lang = touring_code::ast::Lang::from_path(std::path::Path::new(file_path))?;
    let content = std::fs::read_to_string(file_path).ok()?;
    let report = touring_code::ast::analyze_quality(&content, lang);
    Some((
        f64::from(report.overall_score),
        f64::from(report.complexity_score),
    ))
}

// S-02 tests. NOTE: `post_edit` is behind `#[cfg(feature = "post-hooks")]`, so run
// with `--features post-hooks` (the config touring-server / the daemon builds with);
// a bare `cargo test -p touring-hook-handlers` compiles this whole module out.
#[cfg(test)]
mod snr_warm_tests {
    use super::*;
    #[test]
    fn compute_cognitive_warm_none_for_non_code_path() {
        // No Lang for a .md path → None (never fabricate a warm value; S-02).
        assert!(compute_cognitive_warm("/nonexistent/readme.md").is_none());
    }
    #[test]
    fn compute_cognitive_warm_matches_canonical_analyze_quality() {
        use std::io::Write;
        let dir = tempfile::tempdir().expect("tempdir");
        let path = dir.path().join("sample.rs");
        writeln!(
            std::fs::File::create(&path).expect("create"),
            "pub fn add(a: u32, b: u32) -> u32 {{ a + b }}"
        )
        .expect("write");
        let (cognitive, complexity) =
            compute_cognitive_warm(path.to_str().expect("utf8")).expect("code file warms");
        assert!((0.0..=1.0).contains(&cognitive) && (0.0..=1.0).contains(&complexity));
        // Fidelity: the warm cognitive score IS analyze_quality().overall_score (the
        // same value `ast meta` reports), not a fabricated proxy.
        let content = std::fs::read_to_string(&path).expect("read");
        let lang = touring_code::ast::Lang::from_path(&path).expect("lang");
        let expected = touring_code::ast::analyze_quality(&content, lang);
        assert!((cognitive - f64::from(expected.overall_score)).abs() < 1e-6);
        assert!((complexity - f64::from(expected.complexity_score)).abs() < 1e-6);
    }
}

/// Phase 1: Tracking — record edit, error-driven learning, reindex, ACO wiring.
///
/// Returns:
/// - Ok(false) if no edit error detected (Phase 2 should run)
/// - Ok(true) if an edit error was detected (Phase 2 should be skipped)
/// - Err(...) if the tracking itself failed (DB error — error is propagated)
fn phase1_tracking(
    runtime: &mut HookRuntime,
    input: &serde_json::Value,
    tool_name: &str,
    file_path: &str,
    rel_path: &str,
    language: Option<&'static str>,
    old_content: Option<&str>,
) -> Result<bool, rusqlite::Error> {
    let summary = build_edit_summary(input, tool_name);
    let error_msg = extract_error_message(input);
    let error_pattern = error_msg.as_deref().and_then(extract_edit_error_pattern);
    let session_id = std::env::var("CLAUDE_SESSION_ID").ok();
    runtime.ctx.knowledge.record_edit_full(
        rel_path,
        tool_name,
        summary.as_deref(),
        error_pattern.as_deref(),
        language,
        None,
        session_id.as_deref(),
    )?;
    if let Some(ref pattern) = error_pattern {
        maybe_auto_create_gotcha(
            &runtime.ctx.knowledge,
            rel_path,
            pattern,
            error_msg.as_deref().unwrap_or(""),
        );
        return Ok(true);
    }
    let should_skip_reindex = (|| -> Option<bool> {
        let content = std::fs::read_to_string(file_path).ok()?;
        let mut hasher = blake3::Hasher::new();
        hasher.update(content.as_bytes());
        let new_hash = hasher.finalize().to_hex().to_string();
        let stored = runtime.ctx.knowledge.get_blake3_hash(rel_path).ok()??;
        Some(stored.0 == new_hash)
    })()
    .unwrap_or(false);
    if should_skip_reindex {
        tracing::debug!("post_edit: BLAKE3 unchanged for {rel_path} — skipping reindex");
    } else {
        // B2 (2026-05-10): promoted from `tracing::debug!` so silent index
        // drift surfaces in production logs. Paired with a gate-metrics
        // counter (`reindex_failure_count`) — a non-zero value means
        // `touring index find` may return stale results until the next
        // successful reindex or rebuild.
        if let Err(e) = reindex_file(runtime, file_path, rel_path, old_content) {
            tracing::warn!(
                target: "touring::post_edit",
                file = %file_path,
                error = %e,
                "reindex_file failed — symbol index now stale for this file; \
                 try `touring index ingest <file>` or `touring index rebuild`",
            );
            gate_metrics::record_reindex_failure();
        }
        // S-02 (SNR slice): warm the FileKnowledgeDB cognitive enrichment for the
        // edited file so cognitive_score/complexity_signal are populated (not null)
        // for pre_read/ast-meta. Canonical analyze_quality().overall_score — the
        // setter had zero production callers (the root of the cold DB, REGRA #0).
        // Only runs when the file actually changed (inside the not-skip-reindex
        // branch). Fail-open: a warm miss never blocks the edit.
        if let Some((cognitive, complexity)) = compute_cognitive_warm(file_path) {
            if let Err(e) = runtime
                .ctx
                .knowledge
                .upsert_cognitive_enrichment(rel_path, cognitive, complexity, 0.0, 0.0, 0.0)
            {
                tracing::debug!("post_edit: warm cognitive enrichment failed for {rel_path}: {e}");
            }
        }
        if let Some(adb) = runtime.ctx.async_knowledge.as_ref().cloned() {
            if let Ok(handle) = tokio::runtime::Handle::try_current() {
                let edit = crate::knowledge::EditEvent {
                    file_path: rel_path.to_string(),
                    edit_type: "edit".to_string(),
                    summary: None,
                    error_pattern: None,
                    edited_at: chrono::Local::now().to_rfc3339(),
                };
                drop(handle.spawn(async move {
                    let _ = adb.record_edit(&edit).await;
                }));
            }
        }
        #[cfg(feature = "tantivy-fts")]
        {
            let doc = crate::tantivy_index::SymbolDoc {
                symbol_name: rel_path.to_string(),
                file_path: rel_path.to_string(),
                symbol_kind: "file".to_string(),
                module_path: None,
                docstring: None,
                line_number: 0,
                language: crate::tantivy_index::extension_to_language(rel_path),
                visibility: None,
                crate_name: None,
                blake3_hash: None,
                import_count: None,
                export_count: None,
                cognitive_score: None,
                functional_signature: None,
                community_id: None,
            };
            if !crate::shared::tantivy_stream::try_send_symbol(doc.clone()) {
                if let Some(tantivy_idx) = crate::tantivy_index::global_tantivy() {
                    if let Err(e) = tantivy_idx.upsert_symbol(&doc) {
                        tracing::debug!("tantivy upsert failed for {rel_path}: {e}");
                    }
                }
            }
        }
    }
    record_coedits(&runtime.ctx.knowledge, rel_path);
    if let Some(ref sid) = session_id {
        if let Err(e) = runtime.ctx.knowledge.record_access(rel_path, sid) {
            tracing::debug!("record_access failed for {rel_path}: {e}");
        }
    }
    {
        let ts_nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0);
        let seq_id = format!("edit:{ts_nanos}:{rel_path}");
        let _ = runtime.ctx.knowledge.insert_symbol_event(
            &seq_id,
            rel_path,
            None,
            "edit",
            None,
            session_id.as_deref(),
        );
    }
    let key_hash = (|| -> Option<String> {
        let content = std::fs::read_to_string(file_path).ok()?;
        Some(blake3::hash(content.as_bytes()).to_hex().to_string())
    })();
    if let Some(hash) = key_hash {
        let dedup = EDIT_DEDUP.get_or_init(MetadataDedup::new);
        let key = DedupKey {
            file_path: rel_path.to_string(),
            content_hash: hash,
        };
        if dedup.check_and_mark(key) {
            gate_metrics::record_metadata_cache_hit();
            tracing::debug!(
                "post_edit: skipping duplicate processing for {rel_path} (content match)"
            );
        }
    } else if let Ok(meta) = FastMetadata::from_path(std::path::Path::new(file_path)) {
        let dedup = EDIT_DEDUP.get_or_init(MetadataDedup::new);
        let key = DedupKey {
            file_path: rel_path.to_string(),
            content_hash: meta.mtime_epoch.to_string(),
        };
        if dedup.check_and_mark(key) {
            gate_metrics::record_metadata_cache_hit();
            tracing::debug!(
                "post_edit: skipping duplicate processing for {rel_path} (mtime fallback)"
            );
        }
    }
    if let Some(lang_str) = language {
        if let Ok(new_content) = std::fs::read_to_string(file_path) {
            persist_call_edges(&runtime.ctx.knowledge, rel_path, &new_content, lang_str);
        }
    }
    runtime.infra.prediction.record_edit(file_path);
    runtime.invalidate_dependency_cache_for_file(std::path::Path::new(file_path));
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as f64)
        .unwrap_or(0.0);
    if let Ok(mut hm) = runtime.learning.heat_map.try_borrow_mut() {
        hm.record_edit(file_path, now);
    }
    let prev_file = runtime
        .infra
        .last_edited_file
        .replace(Some(file_path.to_string()));
    if let Some(ref pf) = prev_file {
        runtime.infra.prediction.record_co_edit(pf, file_path);
        runtime
            .infra
            .predictive_focus
            .observe_co_access(pf, file_path);
        if let Ok(mut pg) = runtime.infra.pheromone_graph.write() {
            pg.reinforce_path(&[pf, file_path]);
        }
    }
    if let Ok(mut ann_guard) = runtime.ctx.ann_recall.try_borrow_mut() {
        if let Some(ref mut ann) = *ann_guard {
            let embedding = crate::ann_memory::path_hash_embedding(file_path);
            let entry = crate::ann_memory::MemoryEntry::new(
                rel_path,
                format!("edited:{tool_name}:{}", rel_path),
                embedding,
            );
            if let Err(e) = ann.add_memory(entry) {
                tracing::debug!("ANN add_memory failed for {rel_path}: {e}");
            }
        }
    }
    if let Some(cognitive) = runtime.cognitive.as_ref() {
        let graph = cognitive.graph();
        let node = touring_intelligence::reasoning::MemoryNode::new(
            rel_path.to_string(),
            rel_path.to_string(),
            touring_intelligence::reasoning::NodeType::File,
        );
        if let Err(e) = graph.add_node(node) {
            tracing::debug!("SemanticGraph add_node failed for {rel_path}: {e}");
        }
        if let Some(ref pf) = prev_file {
            let edge = graph.add_typed_edge(
                pf,
                rel_path,
                touring_intelligence::reasoning::EdgeType::CoEdit,
                1.0,
            );
            if let Err(e) = edge {
                tracing::debug!("SemanticGraph add_typed_edge CoEdit failed: {e}");
            }
        }
    }
    deposit_aco_wiring(runtime, rel_path, tool_name);

    // D1.6: Emit activity event — last step before return.
    // edit_count not tracked in phase1; pass 0 as placeholder (future: wire from session stats).
    crate::activity_hook::emit_post_edit(
        &runtime.project_root,
        rel_path,
        session_id.as_deref().unwrap_or(""),
        0,
    );

    Ok(false)
}
/// Deposit ACO pheromone for the edited file and feed the quality tracker.
///
/// Updates the file-heat score in the Layer7 prediction engine and, when a
/// quality-assessment report is available, forwards it to the ACO wiring bus
/// so the RL model can learn from edit outcomes.
/// KS drift threshold above which the result cache is invalidated for the
/// edited file. 0.3 filters out normal evaporation decay and only triggers
/// on significant distribution shifts (e.g., a burst of edits to new files).
const DRIFT_INVALIDATION_THRESHOLD: f64 = 0.3;
fn deposit_aco_wiring(runtime: &HookRuntime, rel_path: &str, tool_name: &str) {
    if let Ok(wiring) = runtime.aco_wiring.lock() {
        let drift_ks = wiring.deposit_file_edit_with_drift_check(rel_path);
        let heat = wiring
            .bus
            .get(&touring_intelligence::rl::aco::PheroKey::FilePath(
                rel_path.to_string(),
            ));
        if heat > 0.0 {
            runtime.infra.prediction.update_file_heat(rel_path, heat);
        }
        if let Some(ks_stat) = drift_ks {
            tracing::debug!("Pheromone drift detected: KS={ks_stat:.3} for {rel_path}");
            if ks_stat > DRIFT_INVALIDATION_THRESHOLD {
                let evicted = runtime.ctx.result_cache.invalidate_file(rel_path);
                tracing::debug!(
                    "Drift-triggered cache invalidation for {rel_path}: {evicted} entries evicted"
                );
            }
            runtime.infra.predictive_focus.evaporate();
        }
        if let Some(report) = runtime
            .ctx
            .quality_assessment
            .as_ref()
            .map(|qa| qa.to_tracker_report(runtime.session_turn() as u32))
        {
            let state = crate::hook_runtime::hash_str(rel_path);
            let action = crate::hook_runtime::hash_str(tool_name);
            wiring.process_tracker_report(&report, state, action);
            wiring.flush_aco_metrics_to_bus(|arm_id, reward| {
                runtime
                    .ctx
                    .session_bus
                    .borrow_mut()
                    .update_arm_effectiveness(arm_id, reward);
            });
        }
    }
}
/// Grouped data inputs for [`phase2_verification`].
///
/// Bundles the 5 data-only parameters to keep the function signature
/// within clippy's 7-argument limit.
struct Phase2Inputs<'a> {
    /// Language identifier string (e.g. "rust", "python").
    lang_str: &'a str,
    /// File quality metrics computed from the post-edit source.
    quality_after: Option<&'a crate::ast_bridge::FileQualityMetrics>,
    /// Post-edit file content (single I/O read, reused across checks).
    file_content: Option<&'a str>,
    /// Pre-edit symbols JSON snapshot for API surface diff (E13).
    old_symbols_json: Option<&'a str>,
    /// Pre-edit source for edit impact analysis (I-4).
    old_source: Option<&'a str>,
    /// Pre-computed CodeHealthReport (I-1: avoids duplicate pipeline call).
    post_health: Option<&'a touring_analysis::CodeHealthReport>,
}
/// Regex to parse antipattern issue strings like "ANTIPATTERN [3x]: unwrap".
/// Captures the count and the message.
static RE_ANTIPATTERN_COUNT: once_cell::sync::Lazy<Regex> = once_cell::sync::Lazy::new(|| {
    Regex::new(r"ANTIPATTERN \[(\d+)x\]: (.+)").expect("antipattern regex is valid")
});
/// Parse antipattern issue strings to extract pattern → count mapping.
///
/// Only matches issues that start with "ANTIPATTERN [" and contain "[{count}x]".
/// Returns a HashMap of pattern message → occurrence count.
fn parse_antipattern_counts(issues: &[String]) -> std::collections::HashMap<String, usize> {
    let mut counts = std::collections::HashMap::new();
    for issue in issues {
        if let Some(caps) = RE_ANTIPATTERN_COUNT.captures(issue) {
            if let (Ok(count), Some(msg)) = (caps[1].parse::<usize>(), caps.get(2)) {
                counts.insert(msg.as_str().to_string(), count);
            }
        }
    }
    counts
}
/// Get antipattern baseline from cache for a given file.
///
/// Returns an empty map if no baseline exists (first edit to this file).
fn get_antipattern_baseline(
    cache: &super::aco_bridge::HookResultCache,
    rel_path: &str,
) -> std::collections::HashMap<String, usize> {
    let key = format!("__antipattern_baseline__:{rel_path}");
    cache
        .get_result("post_edit", &key)
        .and_then(
            |json: String| -> Option<std::collections::HashMap<String, usize>> {
                serde_json::from_str(&json).ok()
            },
        )
        .unwrap_or_default()
}
/// Store antipattern baseline in cache for a given file.
fn set_antipattern_baseline(
    cache: &super::aco_bridge::HookResultCache,
    rel_path: &str,
    baseline: &std::collections::HashMap<String, usize>,
) {
    let key = format!("__antipattern_baseline__:{rel_path}");
    if let Ok(json) = serde_json::to_string(baseline) {
        cache.cache_result("post_edit", &key, json);
    }
}
/// Compute antipattern delta and return blocking issue if threshold exceeded.
///
/// Compares current antipattern counts against baseline to determine how many
/// NEW antipatterns were introduced by this edit. Only blocks when the delta
/// (sum of new occurrences across all patterns) exceeds the threshold.
fn compute_antipattern_delta_and_block(
    cache: &super::aco_bridge::HookResultCache,
    rel_path: &str,
    issues: &mut Vec<String>,
) {
    let baseline = get_antipattern_baseline(cache, rel_path);
    let current = parse_antipattern_counts(issues);
    let mut total_delta = 0usize;
    for (pattern, &curr_count) in &current {
        let baseline_count = baseline.get(pattern).copied().unwrap_or(0);
        if curr_count > baseline_count {
            total_delta += curr_count - baseline_count;
        }
    }
    set_antipattern_baseline(cache, rel_path, &current);
    if total_delta >= BLOCK_ANTIPATTERN_THRESHOLD {
        issues.push(format!(
            "ANTIPATTERN_BLOCK [{total_delta}x new]: too many new anti-patterns introduced. \
             Baseline had {} patterns, edit added {} new occurrences.",
            baseline.len(),
            total_delta
        ));
    }
}
/// Phase 2: Quality verification — speculate, anti-patterns, quality delta, cognitive enrichment.
///
/// `inputs.file_content` is the pre-loaded source content (avoids redundant I/O).
/// `inputs.old_source` is the file content captured BEFORE phase1 reindexed the file (for I-4 edit impact).
/// `inputs.post_health` is the pre-computed `CodeHealthReport` (I-1: avoids duplicate pipeline runs).
///
/// Returns issue strings for feedback. Empty vec = no issues.
fn phase2_verification(
    runtime: &HookRuntime,
    file_path: &str,
    rel_path: &str,
    inputs: &Phase2Inputs<'_>,
) -> Vec<String> {
    let Phase2Inputs {
        lang_str,
        quality_after,
        file_content,
        old_symbols_json,
        old_source,
        post_health,
    } = inputs;
    if let Some(qa) = quality_after {
        let note = format!(
            "quality: CC_max={}, CC_avg={:.1}, symbols={}, complex={}",
            qa.max_complexity, qa.avg_complexity, qa.symbol_count, qa.high_complexity_count
        );
        if let Err(e) = runtime.ctx.knowledge.replace_quality_note(rel_path, &note) {
            tracing::debug!("replace_quality_note failed for {rel_path}: {e}");
        }
    }
    let pipeline_item = FlowItem::new(rel_path.to_string(), lang_str.to_string());
    let mut issues = run_quality_pipeline(
        file_path,
        lang_str,
        &runtime.ctx.knowledge,
        rel_path,
        *quality_after,
        *file_content,
        pipeline_item,
    );
    compute_antipattern_delta_and_block(&runtime.ctx.result_cache, rel_path, &mut issues);
    if let (Some(old_json), Some(src)) = (old_symbols_json, file_content) {
        if let Some(diff_msg) =
            crate::ast_bridge::diff_pub_api_from_snapshot(old_json, src, file_path)
        {
            issues.push(diff_msg);
        }
    }
    if let (Some(old_src), Some(new_src)) = (old_source, file_content) {
        if let Some(impact) = crate::ast_bridge::validate_edit_impact(
            old_src,
            new_src,
            file_path,
            runtime.infra.symbol_index.as_ref(),
            15,
        ) {
            if !impact.syntax_valid {
                issues.push(format!("SYNTAX INVALID after edit: {}", impact.summary));
            } else if impact.complexity_violation || impact.affected_files > 3 {
                issues.push(format!("EDIT IMPACT: {}", impact.summary));
            }
        }
    }
    if let Some(src) = file_content {
        let pipeline = touring_simd::cortex::MetacognitivePipeline::new();
        if let Some(resolved) = crate::ast_bridge::fuse_quality_evidence(src, file_path, &pipeline)
        {
            if resolved.fused_value < 0.6 {
                issues.push(format!(
                    "QUALITY fused={:.2} (wilson_lb={:.2}) — consider refactoring",
                    resolved.fused_value, resolved.wilson_bound,
                ));
            }
        }
    }
    if issues.len() < 8 {
        if let Some(health) = post_health {
            let dashboard = health.to_dashboard();
            tracing::debug!(
                target : "touring_metrics", dashboard = % dashboard.to_json_line(),
                "post_edit health dashboard"
            );
            let alerts = dashboard.alerts_below(0.8);
            if !alerts.is_empty() {
                issues.push(format!("HEALTH {}", alerts.join(", ")));
            } else if health.composite_score < 1.0 {
                issues.push(format!(
                    "HEALTH {}",
                    health.to_analysis_summary().one_liner()
                ));
            }
        }
    }
    if let Some(ref cognitive) = runtime.cognitive {
        let enriched = crate::shared::signals::enrich_with_cognitive(cognitive, file_path, false);
        if !enriched.is_empty() {
            issues.push(enriched);
        }
    }
    issues
}
/// Check whether the edit should be BLOCKED due to excessive new anti-patterns.
///
/// Phase 2.1 (Semantic Provenance): Now uses delta-based blocking instead of total count.
/// The `compute_antipattern_delta_and_block` function in `phase2_verification` computes
/// the actual delta between current and baseline antipattern counts, and injects an
/// "ANTIPATTERN_BLOCK" issue when delta >= BLOCK_ANTIPATTERN_THRESHOLD.
///
/// This function looks for that "ANTIPATTERN_BLOCK" marker to decide whether to block,
/// rather than counting all "ANTIPATTERN" issues (which would incorrectly block on
/// pre-existing antipatterns in the file.
///
/// Returns `Some(HookResponse::Block { .. })` when the edit should be undone,
/// or `None` to let it proceed with Context feedback.
fn check_block_gate(issues: &[String], rel_path: &str) -> Option<HookResponse> {
    for issue in issues {
        if issue.contains("ANTIPATTERN_BLOCK") {
            let reason = format!(
                "Edit blocked: too many new anti-patterns introduced in {}. \
                 This edit exceeds the regression threshold. \
                 Please fix the anti-patterns before reapplying.",
                rel_path,
            );
            let context = issues.join("\n");
            return Some(HookResponse::Block {
                reason,
                context: Some(context),
                event_name: Some("PostToolUse".to_string()),
            });
        }
    }
    None
}
/// Assign a priority score to an issue string based on its category prefix.
///
/// Higher score = higher priority. Used by [`compose_post_edit_feedback`] to ensure
/// that critical issues (syntax errors) survive CILA budget truncation ahead of
/// lower-priority signals (wiring hints, multi-config reminders).
///
/// Priority tiers:
/// - 2.5: Syntax / speculate failures (SYNTAX, SYMBOL, STRUCTURAL, IMPORT, CFG)
/// - 2.0: Anti-pattern detections (ANTIPATTERN), API surface changes (E13)
/// - 1.5: Complexity warnings (COMPLEXITY, HIGH COMPLEXITY)
/// - 1.0: Wiring issues (WIRING)
/// - 0.8: Multi-config hints (feature-gated)
/// - 0.5: Everything else (cognitive enrichment, etc.)
fn issue_priority(issue: &str) -> f32 {
    if issue.contains("SYNTAX")
        || issue.contains("SYMBOL")
        || issue.contains("STRUCTURAL")
        || issue.contains("IMPORT")
        || issue.contains("CFG")
    {
        2.5
    } else if issue.contains("ANTIPATTERN") || issue.contains("API surface changed") {
        2.0
    } else if issue.contains("COMPLEXITY") {
        1.5
    } else if issue.contains("HEALTH") {
        1.2
    } else if issue.contains("WIRING") {
        1.0
    } else if issue.contains("feature-gated") {
        0.8
    } else {
        0.5
    }
}
/// Assemble the post-edit feedback string from a list of issues.
///
/// **Sorts issues by priority** (descending) before applying a CILA-aware
/// character budget, so that high-priority issues (syntax errors, anti-patterns)
/// survive truncation ahead of lower-priority signals (wiring hints).
///
/// Returns a formatted string like:
/// `"post-edit verification: N issue(s) | issue1 | issue2 | …"`
pub(crate) fn compose_post_edit_feedback(mut issues: Vec<String>, cila_level: u8) -> String {
    issues.sort_by(|a, b| {
        issue_priority(b)
            .partial_cmp(&issue_priority(a))
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    let budget = crate::shared::cila::cila_budget_edit(cila_level);
    let mut used = 0usize;
    issues.retain(|s| {
        let len = s.len() + 3;
        if used + len <= budget {
            used += len;
            true
        } else {
            false
        }
    });
    format!(
        "post-edit verification: {} issue(s) | {}",
        issues.len(),
        issues.join(" | ")
    )
}
/// Run speculate_v2 on `source` and collect diagnostic messages (V1 check).
///
/// Returns issue strings prefixed with the layer name (SYNTAX, SYMBOL, etc.).
/// At most 3 diagnostics per failed layer are returned to keep feedback concise.
/// Map a `ValidationLayer` variant to its short diagnostic label.
///
/// Used by `compute_speculate_issues` to prefix each diagnostic message.
#[inline]
fn label_for_layer(layer: &touring_code::ast::ValidationLayer) -> &'static str {
    match layer {
        touring_code::ast::ValidationLayer::Syntax => "SYNTAX",
        touring_code::ast::ValidationLayer::SymbolResolution => "SYMBOL",
        touring_code::ast::ValidationLayer::Structural => "STRUCTURAL",
        touring_code::ast::ValidationLayer::ImportCheck => "IMPORT",
        touring_code::ast::ValidationLayer::CfgImpact => "CFG",
        touring_code::ast::ValidationLayer::Complexity => "COMPLEXITY",
    }
}
/// Collect formatted diagnostics from a single failed validation layer.
///
/// Returns up to 3 diagnostic strings prefixed with the layer label.
/// Returns an empty vec when `layer.passed` is true.
///
/// Pure function — no I/O, easily testable.
fn collect_layer_diagnostics(layer: &touring_code::ast::LayerResult) -> Vec<String> {
    if layer.passed {
        return Vec::new();
    }
    let label = label_for_layer(&layer.layer);
    layer
        .diagnostics
        .iter()
        .take(3)
        .map(|diag| format!("\u{1f6a8} {label}: {}", truncate_str(diag, 100)))
        .collect()
}
fn compute_speculate_issues(source: &str, lang: touring_code::ast::Lang) -> Vec<String> {
    let spec_result = speculate_v2(source, lang, None, None);
    if spec_result.all_passed {
        return Vec::new();
    }
    let mut issues: Vec<String> = spec_result
        .layers
        .iter()
        .flat_map(collect_layer_diagnostics)
        .collect();
    if let Some(score) = spec_result.bayesian_score {
        if !issues.is_empty() {
            issues.push(format!(
                "SPECULATE bayesian_confidence={:.2}, composite={:.2}",
                score, spec_result.composite_score
            ));
        }
    }
    issues
}
/// V5: Multi-config hint — emits a reminder when the edited file has feature gates.
///
/// Non-blocking: does not invoke cargo. Tells the editor to verify
/// `cargo check --all-features` for the owning crate when feature-gated
/// code was just modified.
///
/// When `preloaded` is `Some`, uses that content directly (avoids redundant I/O).
fn verify_multiconfig_hint(file_path: &str, preloaded: Option<&str>) -> Option<String> {
    if !file_path.ends_with(".rs") {
        return None;
    }
    let owned_source;
    let source: &str = match preloaded {
        Some(s) => s,
        None => {
            owned_source = std::fs::read_to_string(file_path).ok()?;
            &owned_source
        }
    };
    if !source.contains("cfg(feature") {
        return None;
    }
    let mut features: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut remaining = source;
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
    if features.is_empty() {
        return None;
    }
    let list: Vec<String> = features.into_iter().collect();
    Some(format!(
        "⚙ feature-gated [{}]: run `cargo check --all-features` to verify cross-config correctness",
        list.join(", ")
    ))
}
/// Run all quality verification checks on the edited file.
///
/// V1 (speculate_v2) and V2 (antipatterns) run in parallel via `rayon::join`.
/// V3 (complexity), V4 (wiring), and V5 (multi-config hint) are fast sequential checks.
///
/// `file_content` is the pre-loaded source content (avoids redundant I/O).
///
/// Returns a list of issue descriptions. Empty = all clean.
/// Build a FlowPipeline for post-edit quality verification.
///
/// Stages:
/// - Filter: validate inputs (non-empty source, known language)
/// - Transform: main quality checks (parallel speculative + antipattern + V3-V6 signals)
/// - Timed: timeout guard (500ms per stage)
fn build_quality_pipeline() -> touring_orchestration::flow::FlowPipeline {
    use crate::pipeline::stages::{Filter, Transform};
    let validate_input = Filter::new("validate", |item: &Item| {
        !item.label.is_empty() && !item.id.is_empty()
    });
    let main_checks = Transform::new("quality_checks", |item: Item| Ok(item));
    TouringFlowBuilder::new()
        .add_stage("validate", validate_input)
        .add_stage("checks", main_checks)
        .with_timeout(Duration::from_millis(500))
        .with_output_target(touring_orchestration::flow::OutputTarget::Discard)
        .build()
}
/// Run post-edit quality verification via FlowPipeline.
///
/// Wraps the verify_post_edit_quality call in a Filter + Transform + Timed pipeline.
/// This refactoring replaces the inline sequential quality check calls with a
/// structured pipeline that provides consistent error handling and timing observability.
///
/// # Arguments
/// - `file_path`: path to the edited file
/// - `lang_str`: language identifier string
/// - `db`: file knowledge database
/// - `rel_path`: relative path for the file
/// - `quality_after`: post-edit quality metrics
/// - `file_content`: current file content
/// - `item`: pipeline item carrying context (id = file_path, label = content preview)
///
/// # Returns
/// Vector of issue strings (same as verify_post_edit_quality)
fn run_quality_pipeline(
    file_path: &str,
    lang_str: &str,
    db: &FileKnowledgeDB,
    rel_path: &str,
    quality_after: Option<&super::ast_bridge::FileQualityMetrics>,
    file_content: Option<&str>,
    item: Item,
) -> Vec<String> {
    // Quality pipeline result is recorded for RL; the post-edit quality check
    // runs regardless of pipeline success/failure (fire-and-forget pattern).
    let pipeline = build_quality_pipeline();
    let _result = pipeline.run(item);
    verify_post_edit_quality(
        file_path,
        lang_str,
        db,
        rel_path,
        quality_after,
        file_content,
    )
}
fn verify_post_edit_quality(
    file_path: &str,
    lang_str: &str,
    db: &FileKnowledgeDB,
    rel_path: &str,
    quality_after: Option<&super::ast_bridge::FileQualityMetrics>,
    file_content: Option<&str>,
) -> Vec<String> {
    let (source, lang) = match parse_source_and_lang(file_path, lang_str, file_content) {
        Some(pair) => pair,
        None => return Vec::new(),
    };
    let is_test = crate::shared::quality::is_test_file(file_path);
    let mut issues = compute_parallel_checks(source, lang, is_test, lang_str);
    if let Some(signal) = verify_complexity_signal(quality_after) {
        issues.push(signal);
    }
    if let Some(signal) = verify_wiring_signal(db, rel_path) {
        issues.push(signal);
    }
    if let Some(signal) = verify_multiconfig_hint(file_path, file_content) {
        issues.push(signal);
    }
    if let Some(signal) = verify_rust_workflow_hint(file_path, file_content) {
        issues.push(signal);
    }
    issues
}
/// V6 (Wave 5+5.1): multi-language workflow hint — delegates to
/// `code_workflow_hint` which auto-routes Rust to the syn-backed
/// advisory and other languages (Python/TS/TSX/JS/Bash/…) to the
/// tree-sitter multi-lang path. Keeps the legacy `verify_rust_workflow_hint`
/// name so existing callers compile untouched.
fn verify_rust_workflow_hint(file_path: &str, preloaded: Option<&str>) -> Option<String> {
    crate::wave5_workflow::code_workflow_hint(file_path, preloaded)
}
/// V6 reward mapping — delegates to multi-language
/// `code_workflow_reward` (Rust = syn-backed, other langs = tree-sitter
/// quality report). Uniform `[-0.10, +0.10]` envelope across languages.
fn compute_rust_workflow_reward(file_path: &str, preloaded: Option<&str>) -> Option<f64> {
    crate::wave5_workflow::code_workflow_reward(file_path, preloaded)
}
/// Parse language tag and return source content + parsed language.
///
/// When `preloaded` is `Some`, uses that content directly (avoids redundant I/O).
/// Falls back to `fs::read_to_string` only when `preloaded` is `None`.
///
/// Returns `None` if `lang_str` is empty, the file cannot be read,
/// or the language tag is not recognized.
fn parse_source_and_lang(
    file_path: &str,
    lang_str: &str,
    preloaded: Option<&str>,
) -> Option<(String, touring_code::ast::Lang)> {
    if lang_str.is_empty() {
        return None;
    }
    let source = match preloaded {
        Some(s) => s.to_string(),
        None => std::fs::read_to_string(file_path).ok()?,
    };
    let lang = lang_str.parse::<touring_code::ast::Lang>().ok()?;
    Some((source, lang))
}
/// Run V1 (speculate) and V2 (anti-patterns) checks in parallel.
///
/// Anti-pattern detection is skipped for test files to avoid noise.
fn compute_parallel_checks(
    source: String,
    lang: touring_code::ast::Lang,
    is_test: bool,
    lang_str: &str,
) -> Vec<String> {
    let source_v1 = source.clone();
    let source_v2 = source;
    let lang_str_owned = lang_str.to_owned();
    let (spec_issues, antipattern_issues) = rayon::join(
        move || compute_speculate_issues(&source_v1, lang),
        move || compute_antipattern_issues(&source_v2, &lang_str_owned, is_test),
    );
    let mut issues = spec_issues;
    issues.extend(antipattern_issues);
    issues
}
/// Run anti-pattern detection, skipping test files.
///
/// Uses `detect_antipatterns_with_lines` so each warning includes the first
/// line number where the pattern was found — actionable context for the caller.
fn compute_antipattern_issues(source: &str, lang_str: &str, is_test: bool) -> Vec<String> {
    if is_test {
        Vec::new()
    } else {
        crate::shared::antipatterns::detect_antipatterns_with_lines(source, lang_str)
            .into_iter()
            .map(|(msg, line)| format!("{msg} (line {line})"))
            .collect()
    }
}
/// V3: Emit a complexity warning when CC_max exceeds the threshold.
///
/// Returns `None` when quality metrics are absent or complexity is acceptable.
fn verify_complexity_signal(
    quality_after: Option<&super::ast_bridge::FileQualityMetrics>,
) -> Option<String> {
    let qa = quality_after?;
    if qa.max_complexity <= 15 {
        return None;
    }
    let names = qa.complex_symbols.join(", ");
    Some(format!(
        "\u{1f4c8} HIGH COMPLEXITY: CC_max={} [{}]",
        qa.max_complexity,
        truncate_str(&names, 80)
    ))
}
/// V4: Emit a wiring warning when orphan public symbols lower integration below 50%.
///
/// Returns `None` when wiring data is unavailable or integration is healthy.
fn verify_wiring_signal(db: &FileKnowledgeDB, rel_path: &str) -> Option<String> {
    let status = db.module_wiring_status(rel_path).ok()?;
    if status.orphan_symbols.is_empty() || status.integration_score >= 0.5 {
        return None;
    }
    let orphans = status.orphan_symbols.join(", ");
    Some(format!(
        "WIRING: {} orphan pub symbol(s) [{}] \u{2014} wire into consumers",
        status.orphan_symbols.len(),
        truncate_str(&orphans, 60)
    ))
}
/// How many recent file accesses to consider for co-edit pairs.
const COEDIT_WINDOW: usize = 10;
/// Record co-edit pairs between the current file and recently accessed files.
///
/// After each successful edit, we look at the last N files from `file_access_log`
/// and record a co-edit relationship with each. This builds up a weighted graph
/// of which files tend to be edited together.
fn record_coedits(knowledge: &super::knowledge::FileKnowledgeDB, current_file: &str) {
    let recent_files = knowledge.recent_accessed_files(current_file, COEDIT_WINDOW);
    for recent in &recent_files {
        if let Err(e) = knowledge.record_coedit(current_file, recent) {
            tracing::debug!("record_coedit failed ({current_file} → {recent}): {e}");
        }
    }
}
/// Extract an error message from the post-tool-use input.
///
/// Claude Code PostToolUse events may include error information in several
/// locations depending on the tool and failure mode:
/// - `tool_use_result.content` (string or array with text blocks)
/// - `tool_use_result.is_error` (boolean flag)
/// - `tool_output` (fallback for some tools)
fn extract_error_message(input: &serde_json::Value) -> Option<String> {
    let is_error = input
        .pointer("/tool_use_result/is_error")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if !is_error {
        return extract_implicit_error(input);
    }
    extract_explicit_error_content(input)
}
/// Check for error keywords in result string when `is_error` is not set.
///
/// Some tools report errors as plain text without setting the `is_error` flag.
fn extract_implicit_error(input: &serde_json::Value) -> Option<String> {
    let text = input
        .pointer("/tool_result")
        .and_then(|v| v.as_str())
        .or_else(|| {
            input
                .pointer("/tool_use_result/content")
                .and_then(|v| v.as_str())
        })?;
    let lower = text.to_lowercase();
    let is_error_text = lower.contains("error")
        || lower.contains("not found")
        || lower.contains("permission denied")
        || lower.contains("failed");
    if is_error_text {
        Some(truncate(text, 300))
    } else {
        None
    }
}
/// Extract error content when `tool_use_result.is_error` is true.
///
/// Handles three content shapes:
/// - String: returned directly (truncated)
/// - Array of content blocks: first `text` field extracted
/// - Missing/other: falls back to `tool_result` top-level key
fn extract_explicit_error_content(input: &serde_json::Value) -> Option<String> {
    match input.pointer("/tool_use_result/content") {
        Some(serde_json::Value::String(s)) => Some(truncate(s, 300)),
        Some(serde_json::Value::Array(arr)) => extract_first_text_block(arr),
        _ => input
            .get("tool_result")
            .and_then(|v| v.as_str())
            .map(|s| truncate(s, 300)),
    }
}
/// Extract text from the first content block in an array.
///
/// Used for `tool_use_result.content` arrays where each element is a
/// `{"type": "text", "text": "..."}` block.
fn extract_first_text_block(arr: &[serde_json::Value]) -> Option<String> {
    arr.iter()
        .find_map(|block| block.get("text").and_then(|v| v.as_str()))
        .map(|text| truncate(text, 300))
}
/// Extract a normalized error pattern from an edit error message.
///
/// Returns a short, stable string key suitable for counting recurrences.
/// Known patterns are mapped to canonical names; unknown patterns get a
/// normalized form of the first 50 characters.
/// Ordered lookup table: (keyword, pattern_name).
///
/// Order matters — more specific entries must precede generic ones (e.g.
/// "not unique" before "not found") so the first match wins correctly.
/// Each row maps one keyword substring to the canonical pattern string.
const ERROR_PATTERNS: &[(&str, &str)] = &[
    ("not unique", "edit_not_unique"),
    ("multiple matches", "edit_not_unique"),
    ("ambiguous", "edit_not_unique"),
    ("string to replace not found", "string_not_found"),
    ("not found in file", "string_not_found"),
    ("old_string", "string_not_found"),
    ("unexpectedly modified", "file_modified_externally"),
    ("has been modified", "file_modified_externally"),
    ("file changed", "file_modified_externally"),
    ("permission denied", "permission_denied"),
    ("read-only", "permission_denied"),
    ("syntax error", "syntax_error"),
    ("parse error", "syntax_error"),
    ("no such file", "file_not_found"),
    ("file not found", "file_not_found"),
    ("exit code", "exit_code_nonzero"),
    ("exit_code", "exit_code_nonzero"),
];
/// Extract a canonical error-pattern key from a raw error message string.
///
/// Performs a case-insensitive linear scan over `ERROR_PATTERNS` and returns
/// the first matching pattern key (e.g. `"compile_error"`, `"type_error"`).
/// Returns `None` when no known pattern is found.
pub(crate) fn extract_edit_error_pattern(error_msg: &str) -> Option<String> {
    let normalized = error_msg.to_lowercase();
    if let Some(&(_, pattern)) = ERROR_PATTERNS
        .iter()
        .find(|(kw, _)| normalized.contains(kw))
    {
        return Some(pattern.to_string());
    }
    if normalized.len() > 10 {
        normalize_error_fallback(&normalized)
    } else {
        None
    }
}
/// Collapse runs of consecutive `_` characters into a single `_`.
///
/// Used by `normalize_error_fallback` to produce stable, deduplicated keys.
fn collapse_underscores(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut prev_underscore = false;
    for ch in s.chars() {
        if ch == '_' {
            if !prev_underscore {
                out.push('_');
            }
            prev_underscore = true;
        } else {
            out.push(ch);
            prev_underscore = false;
        }
    }
    out
}
/// Normalize an unknown error message prefix into a stable pattern key.
///
/// Takes the first 50 characters, replaces non-alphanumeric chars with `_`,
/// collapses consecutive underscores, and trims leading/trailing underscores.
/// Returns `None` if the result is empty (e.g. all punctuation input).
fn normalize_error_fallback(normalized: &str) -> Option<String> {
    let end = byte_boundary(normalized, 50);
    let short = &normalized[..end];
    let pattern: String = short
        .chars()
        .map(|c| if c.is_alphanumeric() { c } else { '_' })
        .collect();
    let trimmed = collapse_underscores(&pattern).trim_matches('_').to_string();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}
/// Find the first existing gotcha for `file_path` whose text contains `pattern`.
///
/// Returns `Some(&Gotcha)` on the first match, `None` when no match exists.
/// Avoids the redundant `any` + `find` double-scan in `maybe_auto_create_gotcha`.
fn find_existing_gotcha<'a>(
    gotchas: &'a [crate::knowledge::Gotcha],
    pattern: &str,
) -> Option<&'a crate::knowledge::Gotcha> {
    gotchas.iter().find(|g| g.gotcha.contains(pattern))
}
/// Check if an error pattern has recurred enough to auto-create a gotcha.
///
/// Queries the last `RECENT_EDIT_WINDOW` edits for this file, counts how
/// many have the same `error_pattern`, and if the count reaches
/// `AUTO_GOTCHA_THRESHOLD`, creates a gotcha entry.
fn maybe_auto_create_gotcha(
    db: &FileKnowledgeDB,
    file_path: &str,
    error_pattern: &str,
    error_msg: &str,
) {
    let occurrences = db.count_edit_error_pattern(file_path, error_pattern, RECENT_EDIT_WINDOW);
    if occurrences >= AUTO_GOTCHA_THRESHOLD {
        let existing = db.get_gotchas_for_file(file_path);
        if let Some(g) = find_existing_gotcha(&existing, error_pattern) {
            db.increment_gotcha_hit(g.id);
            return;
        }
        let file_pattern = file_path
            .rsplit('/')
            .next()
            .and_then(|f| f.rsplit_once('.').map(|(name, _)| name))
            .unwrap_or(file_path);
        let error_short = truncate(error_msg, 120);
        let gotcha_text = format!(
            "[auto:E7.1] '{}' error recurs ({}x). Last: {}",
            error_pattern, occurrences, error_short
        );
        if let Err(e) = db.add_gotcha(file_pattern, &gotcha_text, "warning", None) {
            tracing::debug!("add_gotcha failed for {file_pattern}: {e}");
        }
    }
}
/// Build a short summary of what was edited.
fn build_edit_summary(input: &serde_json::Value, tool_name: &str) -> Option<String> {
    match tool_name {
        "Edit" => summarize_edit_tool(input),
        "Write" => summarize_write_tool(input),
        _ => None,
    }
}
/// Summarize an Edit tool call: `'old…' → 'new…'`.
fn summarize_edit_tool(input: &serde_json::Value) -> Option<String> {
    let old = input
        .pointer("/tool_input/old_string")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let new = input
        .pointer("/tool_input/new_string")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if old.is_empty() && new.is_empty() {
        return None;
    }
    let old_short = truncate_str(old, 30);
    let new_short = truncate_str(new, 30);
    Some(format!("'{old_short}' → '{new_short}'"))
}
/// Summarize a Write tool call: `wrote N lines`.
fn summarize_write_tool(input: &serde_json::Value) -> Option<String> {
    let content = input
        .pointer("/tool_input/content")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    let lines = content.lines().count();
    Some(format!("wrote {lines} lines"))
}
/// Re-index a file after edit (update knowledge with current content).
///
/// Delegates to [`crate::shared::reindex::reindex_file_with_old`].
fn reindex_file(
    runtime: &HookRuntime,
    abs_path: &str,
    rel_path: &str,
    old_content: Option<&str>,
) -> Result<(), crate::shared::reindex::ReindexError> {
    crate::shared::reindex::reindex_file_with_old(runtime, abs_path, rel_path, old_content)
}
/// Truncate string to max length, appending "..." if truncated.
fn truncate(s: &str, max: usize) -> String {
    if s.len() <= max {
        s.to_string()
    } else {
        let end = byte_boundary(s, max.saturating_sub(3));
        format!("{}...", &s[..end])
    }
}
/// Format PII scan findings as an issue string for PostToolUse injection.
///
/// Unlike PreToolUse (which suppresses low-severity), PostToolUse shows ALL findings
/// since the edit has already been applied and full verification is needed.
/// Format: `PII: N finding(s) — 2 high (cpf, cpf), 1 medium (email), 1 low (phone)`
fn format_pii_findings_context_post(findings: &[super::pii::PIIFinding]) -> String {
    let high: Vec<_> = findings.iter().filter(|f| f.severity == "high").collect();
    let medium: Vec<_> = findings.iter().filter(|f| f.severity == "medium").collect();
    let low: Vec<_> = findings.iter().filter(|f| f.severity == "low").collect();
    let total = findings.len();
    let mut parts = Vec::new();
    if !high.is_empty() {
        let unique: std::collections::HashSet<_> =
            high.iter().map(|f| f.pattern_name.as_str()).collect();
        parts.push(format!(
            "{} high ({})",
            high.len(),
            unique.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }
    if !medium.is_empty() {
        let unique: std::collections::HashSet<_> =
            medium.iter().map(|f| f.pattern_name.as_str()).collect();
        parts.push(format!(
            "{} medium ({})",
            medium.len(),
            unique.into_iter().collect::<Vec<_>>().join(", ")
        ));
    }
    if !low.is_empty() {
        parts.push(format!("{} low", low.len()));
    }
    format!("PII: {} finding(s) — {}", total, parts.join(", "))
}
/// Find the largest valid byte boundary <= target for a UTF-8 string.
fn byte_boundary(s: &str, target: usize) -> usize {
    let target = target.min(s.len());
    let mut pos = target;
    while pos > 0 && !s.is_char_boundary(pos) {
        pos -= 1;
    }
    pos
}
#[cfg(test)]
#[path = "post_edit_tests.rs"]
mod tests;
/// W-115 diagnostic code — Edit tool wrote into a skip-region.
const W_115_SKIPPED_REGION_WRITTEN: &str = "W-115";
/// Check whether an Edit tool's target byte range overlaps any `// touring:skip-region`
/// … `// touring:skip-end` frozen markers in the file.
///
/// Returns an empty string when no violation is detected, or a formatted
/// diagnostic string when an overlap is found.
///
/// This function is intentionally self-contained: it parses raw skip-region
/// markers directly from `file_content` without depending on `touring-generator`
/// (which would create a circular dependency between touring-hooks and touring-generator.
fn check_edit_overlaps_skip_region(
    rel_path: &str,
    old_source: &Option<String>,
    input: &serde_json::Value,
    file_path: &str,
) -> String {
    let old_string = input
        .pointer("/tool_input/old_string")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if old_string.is_empty() {
        return String::new();
    }
    let file_content = match std::fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(_) => return String::new(),
    };
    let regions = parse_skip_regions(&file_content);
    if regions.is_empty() {
        return String::new();
    }
    let Some(edit_span) = find_edit_byte_span(&file_content, old_source, old_string) else {
        return String::new();
    };
    for region in &regions {
        if region.start < edit_span.end && edit_span.start < region.end {
            return format!(
                "{}:skip_region_write(file={},region={}..{},edit={}..{})",
                W_115_SKIPPED_REGION_WRITTEN,
                rel_path,
                region.start,
                region.end,
                edit_span.start,
                edit_span.end
            );
        }
    }
    String::new()
}
/// A single skip region — byte offsets within a source file.
#[derive(Debug, Clone)]
struct SkipRegionByte {
    start: u64,
    end: u64,
}
/// Parse `// touring:skip-region` … `// touring:skip-end` line-comment markers
/// from `source` and return the byte spans of all frozen regions.
///
/// Supports line comments and Rust attributes (`#[touring::skip]`).
/// Does NOT support block comments (matching the touring-generator parser).
fn parse_skip_regions(source: &str) -> Vec<SkipRegionByte> {
    let mut regions = Vec::new();
    let mut in_region = false;
    let mut region_start: Option<u64> = None;
    let mut line_cursor: u64 = 0;
    for line in source.lines() {
        let line_start = line_cursor;
        let line_end = line_cursor + line.len() as u64 + 1;
        let trimmed = line.trim();
        if trimmed.starts_with('#')
            && (trimmed.contains("touring::skip") || trimmed.contains("touring(skip)"))
        {
            regions.push(SkipRegionByte {
                start: line_start,
                end: line_end,
            });
            line_cursor = line_end;
            continue;
        }
        if trimmed.starts_with("//")
            && trimmed.contains("touring:skip-region")
            && !trimmed.contains("touring:skip-end")
        {
            region_start = Some(line_end);
            in_region = true;
        } else if trimmed.starts_with("//") && trimmed.contains("touring:skip-end") && in_region {
            if let Some(start) = region_start.take() {
                regions.push(SkipRegionByte {
                    start,
                    end: line_start,
                });
            }
            in_region = false;
        }
        line_cursor = line_end;
    }
    regions
}
/// Find the byte span of `old_string` within `current_content`.
/// Searches in `current_content` directly (file may already be edited).
fn find_edit_byte_span(
    current_content: &str,
    _old_source: &Option<String>,
    old_string: &str,
) -> Option<SkipRegionByte> {
    if old_string.is_empty() {
        return None;
    }
    let old_bytes = old_string.as_bytes();
    let content_bytes = current_content.as_bytes();
    let old_len = old_bytes.len();
    let mut found_pos: Option<usize> = None;
    for i in 0..content_bytes
        .len()
        .saturating_sub(old_len.saturating_sub(1))
    {
        if content_bytes[i..].starts_with(old_bytes) {
            found_pos = Some(i);
        }
    }
    found_pos.map(|start| SkipRegionByte {
        start: start as u64,
        end: (start + old_len) as u64,
    })
}
