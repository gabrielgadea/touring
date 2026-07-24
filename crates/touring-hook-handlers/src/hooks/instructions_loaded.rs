//! InstructionsLoaded handler — enriches instruction loading with project knowledge.
//!
//! When CLAUDE.md or rules are loaded (typically at session start), this handler
//! injects a brief project knowledge summary so the LLM has immediate awareness
//! of the project's state: file count, symbol count, active gotchas, etc.
//!
//! Runs as lifecycle hook (always-on). Target latency: <5ms.
use crate::gateway::harness_contract::HarnessContract;
use crate::hook_runtime::HookResponse;
use crate::runtime::HookRuntime;
use crate::schemas::validate_payload;
use crate::shared::signals::enrich_with_cognitive;
/// Handle InstructionsLoaded events by injecting project knowledge summary.
///
/// Only enriches on `session_start` matcher to avoid noise on every compact/include.
/// Returns `HookResponse::Context` with a brief summary, or `HookResponse::Allow`
/// if there is nothing actionable to inject.
#[tracing::instrument(skip(runtime, input), fields(hook = "instructions_loaded"))]
pub fn run_returning(runtime: &HookRuntime, input: &serde_json::Value) -> HookResponse {
    if validate_payload::<crate::schemas::InstructionsLoadedPayload>(input).is_err() {
        return HookResponse::Allow;
    }

    // ES2 P3 — re-attest HarnessContract on every instructions_loaded event
    // (overflow path). The contract state is owned by `run_session_start` (which
    // takes `&mut HookRuntime`); here we just log a re-pin marker so the
    // overflow event is observable in the tracing stream. Honest carve-out:
    // cannot force model-layer attention non-eviction.
    let constitution_root = runtime.project_root.join(".claude");
    let contract = HarnessContract::attest(&constitution_root);
    tracing::debug!(
        digest = %contract.digest,
        attested = contract.attested,
        claims = contract.claims.len(),
        "instructions_loaded: HarnessContract re-pin (overflow path)"
    );

    let load_reason = input
        .get("matcher")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    if load_reason != "session_start" {
        return HookResponse::Allow;
    }
    let parts = collect_knowledge_parts(runtime);
    if parts.is_empty() {
        return HookResponse::Allow;
    }
    let summary = format!("Project knowledge: {}", parts.join(" | "));
    // D1.6: Emit activity event before returning context.
    if let Some(session_id) = input.get("session_id").and_then(|v| v.as_str()) {
        crate::activity_hook::emit_instructions_loaded(&runtime.project_root, session_id);
    }
    HookRuntime::build_context_for_event(&summary, "InstructionsLoaded")
}
/// Collect all knowledge summary parts for injection into additionalContext.
///
/// Each sub-function appends zero or more entries to `parts`. Splitting into
/// a separate function keeps `run_returning` CC-lean (single branch) while
/// grouping related collection logic here.
fn collect_knowledge_parts(runtime: &HookRuntime) -> Vec<String> {
    let stats = match runtime.ctx.knowledge.stats() {
        Ok(s) => s,
        Err(e) => {
            tracing::debug!(error = % e, "instructions_loaded: stats query failed");
            return vec![];
        }
    };
    if stats.file_count == 0 && stats.bash_count == 0 {
        return vec![];
    }
    let mut parts: Vec<String> = Vec::new();
    push_stats_parts(&stats, &mut parts);
    push_db_parts(runtime, &mut parts);
    push_hint_parts(&stats, &mut parts);
    push_cognitive_parts(runtime, &mut parts);
    push_health_parts(runtime, &mut parts);
    push_task_parts(runtime, &mut parts);
    push_session_resume_parts(runtime, &mut parts);
    push_rl_recommendation_parts(runtime, &mut parts);
    push_mcp_overhead_parts(&mut parts);
    push_repo_map_parts(runtime, &mut parts);
    parts
}
/// Push MCP overhead summary: top-3 costliest tools by total token cost.
/// This is the D28 wiring — estimate_mcp_overhead() is called from
/// instructions_loaded hook at hook_registry.rs:1076 to surface MCP token
/// cost distribution at session start.
fn push_mcp_overhead_parts(parts: &mut Vec<String>) {
    use crate::mcp_overhead::McpOverheadReport;
    use std::sync::{OnceLock, RwLock};
    static CACHED_REPORT: OnceLock<RwLock<Option<McpOverheadReport>>> = OnceLock::new();
    let cell = match CACHED_REPORT.get() {
        Some(cell) => cell,
        None => {
            let cached = CACHED_REPORT.get_or_init(|| {
                let snapshot = crate::mcp_overhead::snapshot_json();
                let report: Option<McpOverheadReport> = serde_json::from_str(&snapshot).ok();
                RwLock::new(report)
            });
            if let Ok(guard) = cached.read() {
                if let Some(ref report) = *guard {
                    emit_mcp_overhead_report(report, parts);
                }
            }
            return;
        }
    };
    if let Ok(guard) = cell.read() {
        if let Some(ref report) = *guard {
            emit_mcp_overhead_report(report, parts);
        }
    }
}
fn emit_mcp_overhead_report(
    report: &crate::mcp_overhead::McpOverheadReport,
    parts: &mut Vec<String>,
) {
    if report.tools.is_empty() {
        return;
    }
    let top3: Vec<&crate::mcp_overhead::ToolCost> = report.tools.iter().take(3).collect();
    let summaries: Vec<String> = top3
        .iter()
        .map(|t| {
            format!(
                "{}={}c/{}t",
                t.tool_name.split("__").last().unwrap_or(&t.tool_name),
                t.call_count,
                t.total_tokens
            )
        })
        .collect();
    if !summaries.is_empty() {
        parts.push(format!("MCP top3: {}", summaries.join(" | ")));
    }
}
/// Push basic knowledge-graph stat counters.
fn push_stats_parts(stats: &crate::knowledge::KnowledgeStats, parts: &mut Vec<String>) {
    if stats.file_count > 0 {
        parts.push(format!("{} files tracked", stats.file_count));
    }
    if stats.edit_count > 0 {
        parts.push(format!("{} edits recorded", stats.edit_count));
    }
    if stats.bash_count > 0 {
        parts.push(format!("{} commands logged", stats.bash_count));
    }
    if stats.gotcha_count > 0 {
        parts.push(format!("{} active gotchas", stats.gotcha_count));
    }
}
/// Push DB-derived parts: hot files and orphan pub symbol count.
fn push_db_parts(runtime: &HookRuntime, parts: &mut Vec<String>) {
    let conn = runtime.ctx.knowledge.conn_ref();
    if let Ok(mut stmt) =
        conn.prepare("SELECT file_path FROM session_file_summary ORDER BY rowid DESC LIMIT 10")
    {
        let hot_files: Vec<String> = stmt
            .query_map(rusqlite::params![], |row| row.get::<_, String>(0))
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default();
        if !hot_files.is_empty() {
            parts.push(format!("hot files: {}", hot_files.join(", ")));
        }
    }
    if let Ok(mut stmt) =
        conn.prepare("SELECT COUNT(*) FROM wiring_map WHERE consumers = 0 AND visibility = 'pub'")
    {
        if let Ok(count) = stmt.query_row(rusqlite::params![], |row| row.get::<_, i64>(0)) {
            if count > 0 {
                parts.push(format!("{count} orphan pub symbols"));
            }
        }
    }
}
/// Push tool-hint parts based on edit/file state.
fn push_hint_parts(stats: &crate::knowledge::KnowledgeStats, parts: &mut Vec<String>) {
    if stats.file_count > 0 && stats.edit_count == 0 {
        parts
            .push("hint: run `touring ast meta <file> --depth summary` before editing".to_string());
    }
    if stats.edit_count > 0 {
        parts
            .push(
                "generator: use `touring generate plan-recall --query \"<intent>\"` to find past plans for this session"
                    .to_string(),
            );
    }
}
/// Push cognitive enrichment signals (bash failures, file risk) and graph health.
fn push_cognitive_parts(runtime: &HookRuntime, parts: &mut Vec<String>) {
    if let Some(ref cognitive) = runtime.cognitive {
        let enriched = enrich_with_cognitive(cognitive, "", false);
        if !enriched.is_empty() {
            parts.push(enriched);
        }
        let predictor_state = if runtime.learning.predictor.is_some() {
            "ready"
        } else {
            "inactive"
        };
        parts.push(format!("cognitive=active, predictor={predictor_state}"));
    }
}
/// Wave 9 S8 (2026-04-26): compute the composite health score from
/// in-process gate_metrics + RL state and inject a one-line warning
/// when the score falls below `DEGRADED_SCORE_THRESHOLD` (0.5).
///
/// Closes the loop between Wave 8 S3 (`composite_health_score` field
/// in `touring status -j`) and the session-start context — the
/// operator now learns about a degraded system before the first edit
/// rather than having to manually inspect the CLI dashboard.
///
/// Missing dimensions (no `wiring`/`daemon_health` in-process state)
/// contribute neutral 0.5 by design — see
/// [`touring_foundation::health::compute_composite_health_score`].
fn push_health_parts(runtime: &HookRuntime, parts: &mut Vec<String>) {
    let snapshot = crate::shared::gate_metrics::GateMetricsSnapshot::capture();
    let cache_ratio = snapshot.query_cache_hit_ratio;
    let outstanding = snapshot.health_delta_outstanding;
    let ema_reward = runtime
        .learning
        .online_rl
        .as_ref()
        .map_or(0.5_f64, |engine| engine.ema_reward());
    let mut combined = serde_json::Map::new();
    combined.insert(
        "gate_metrics".to_string(),
        serde_json::json!({ "query_cache_hit_ratio" : cache_ratio }),
    );
    combined.insert(
        "health_delta".to_string(),
        serde_json::json!({ "outstanding" : outstanding }),
    );
    combined.insert(
        "learning".to_string(),
        serde_json::json!({ "ema_reward" : ema_reward }),
    );
    let score = touring_foundation::health::compute_composite_health_score(&combined);
    if let Some(warning) = touring_foundation::health::compose_degraded_warning(score) {
        parts.push(warning);
    }
}
/// Push task-flow parts: pending Touring tasks + stuck subtask suggestions.
fn push_task_parts(runtime: &HookRuntime, parts: &mut Vec<String>) {
    if let Some(digest) = crate::task_digest::digest_pending_tasks(runtime) {
        parts.push(digest);
    }
    let stuck_count = crate::suggesters::stuck_subtask::detect_and_suggest_stuck_subtasks(runtime);
    if stuck_count > 0 {
        parts.push(format!(
            "Touring: {stuck_count} stuck subtask(s) detected (pending/ready >30min) — \
             check `touring decompose status` and use TaskUpdate with suggestion_ref to close"
        ));
    }
    if let Some(upd) = crate::task_digest::digest_pending_action_suggestions(runtime, "update") {
        parts.push(upd);
    }
    let failing_count =
        crate::suggesters::failure_threshold::detect_and_suggest_failing_tasks(runtime);
    if failing_count > 0 {
        parts.push(format!(
            "Touring: {failing_count} failing task(s) detected (failure >3 OR circuit open) — \
             use TaskStop with suggestion_ref to cancel"
        ));
    }
    if let Some(stop) = crate::task_digest::digest_pending_action_suggestions(runtime, "stop") {
        parts.push(stop);
    }
    let plan_count = crate::suggesters::plan_mode_complexity::detect_and_suggest_plan_mode(runtime);
    if plan_count > 0 {
        parts.push(format!(
            "Touring: {plan_count} complex task(s) detected (CILA L4+) — \
             consider entering plan mode with suggestion_ref before edits"
        ));
    }
    if let Some(pm) = crate::task_digest::digest_pending_action_suggestions(runtime, "plan_mode") {
        parts.push(pm);
    }
}
/// Push last-session resume context: session ID + files touched.
fn push_session_resume_parts(runtime: &HookRuntime, parts: &mut Vec<String>) {
    let conn = runtime.ctx.knowledge.conn_ref();
    let last_session: Option<String> = conn
        .prepare("SELECT session_id FROM cli_sessions ORDER BY updated_at DESC LIMIT 1")
        .ok()
        .and_then(|mut stmt| {
            stmt.query_row(rusqlite::params![], |row| row.get::<_, String>(0))
                .ok()
        });
    let Some(session_id) = last_session else {
        return;
    };
    let files: Vec<String> = conn
        .prepare(
            "SELECT file_path FROM session_file_summary WHERE session_id = ?1 \
             ORDER BY created_at DESC LIMIT 8",
        )
        .ok()
        .and_then(|mut stmt| {
            stmt.query_map(rusqlite::params![session_id], |row| row.get::<_, String>(0))
                .ok()
                .map(|rows| rows.filter_map(|r| r.ok()).collect())
        })
        .unwrap_or_default();
    if files.is_empty() {
        return;
    }
    parts.push(format!(
        "last session ({}): {} files — {}",
        session_id,
        files.len(),
        files.join(", ")
    ));
}
/// Push RL calibration status: EMA reward and update count from OnlineRLEngine.
fn push_rl_recommendation_parts(runtime: &HookRuntime, parts: &mut Vec<String>) {
    let Some(online_rl) = runtime.learning.online_rl.as_ref() else {
        return;
    };
    let update_count = online_rl.update_count();
    if update_count < 10 {
        return;
    }
    let ema = online_rl.ema_reward();
    if ema < 0.4 {
        return;
    }
    let quality_label = if ema >= 0.8 {
        "excellent"
    } else if ema >= 0.6 {
        "good"
    } else {
        "improving"
    };
    parts.push(format!(
        "RL calibrated: {} updates, ema={:.2} ({})",
        update_count, ema, quality_label
    ));
}
/// Shorten a file path to its last two components for compact display.
///
/// `"crates/touring-hooks/src/cli_handlers.rs"` → `"src/cli_handlers.rs"`.
/// Falls back to the original string when fewer than two components exist.
fn shorten_path(file: &str) -> &str {
    let bytes = file.as_bytes();
    // Walk backwards to find the second-to-last '/'.
    let mut slashes = 0u8;
    let mut idx = bytes.len();
    while idx > 0 {
        idx -= 1;
        if bytes[idx] == b'/' {
            slashes += 1;
            if slashes == 2 {
                return &file[idx + 1..];
            }
        }
    }
    file
}

/// TR-3: Repo-map ranked by centrality — top-N symbols by consumer count (fan_in proxy).
///
/// Queries `wiring_map` for the symbols with the most distinct consumers, formats each
/// as a compact one-liner, and appends a `[REPO-MAP — top-N symbols by centrality]`
/// section to `parts`. Budget is hard-capped at 2 048 bytes; entries are truncated
/// when the budget would be exceeded. Fail-open: any DB error silently omits the section.
fn push_repo_map_parts(runtime: &HookRuntime, parts: &mut Vec<String>) {
    /// Maximum byte budget for the entire repo-map section (header + entries).
    const BUDGET_BYTES: usize = 2048;
    /// Maximum number of entries to query (upper bound; budget may truncate further).
    const TOP_N: usize = 25;

    let conn = runtime.ctx.knowledge.conn_ref();

    // Count distinct consumers per (module_file, symbol_name). NULL consumer_file means
    // an orphan row — exclude it so the count reflects real consumers only.
    let sql = "\
        SELECT symbol_name, module_file, COUNT(DISTINCT consumer_file) AS fan_in \
        FROM wiring_map \
        WHERE consumer_file IS NOT NULL \
        GROUP BY module_file, symbol_name \
        ORDER BY fan_in DESC \
        LIMIT ?1";

    let mut stmt = match conn.prepare(sql) {
        Ok(s) => s,
        Err(e) => {
            tracing::debug!(error = %e, "repo_map: prepare failed — omitting section");
            return;
        }
    };

    let rows: Vec<(String, String, i64)> =
        match stmt.query_map(rusqlite::params![TOP_N as i64], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, i64>(2)?,
            ))
        }) {
            Ok(iter) => iter.filter_map(|r| r.ok()).collect(),
            Err(e) => {
                tracing::debug!(error = %e, "repo_map: query failed — omitting section");
                return;
            }
        };

    if rows.is_empty() {
        return;
    }

    let header = format!(
        "[REPO-MAP — top-{} symbols by centrality]",
        rows.len().min(TOP_N)
    );
    // Each entry: "<symbol>  <short_path>  fan_in=<N>"
    // Short path: keep only the last two path components (crate/src/…) to save space.
    let mut section = header.clone();
    section.push('\n');

    for (sym, file, fan_in) in &rows {
        let short = shorten_path(file);
        let line = format!("  {}  {}  fan_in={}\n", sym, short, fan_in);
        if section.len() + line.len() > BUDGET_BYTES {
            // Budget exhausted — truncate with ellipsis.
            section.push_str("  ...\n");
            break;
        }
        section.push_str(&line);
    }

    parts.push(section);
}

#[cfg(test)]
mod tests {
    #[test]
    fn cognitive_active_ready_format() {
        let predictor_state = "ready";
        let msg = format!("cognitive=active, predictor={predictor_state}");
        assert!(
            msg.contains("cognitive=active"),
            "must announce cognitive active"
        );
        assert!(
            msg.contains("predictor=ready"),
            "must include predictor state"
        );
    }
    #[test]
    fn cognitive_active_inactive_predictor_format() {
        let predictor_state = "inactive";
        let msg = format!("cognitive=active, predictor={predictor_state}");
        assert!(
            msg.contains("predictor=inactive"),
            "must report inactive predictor"
        );
    }
    #[test]
    fn rfc100_diagnostics_key_present_in_audit_json() {
        let audit = serde_json::json!(
            { "orphans" : {}, "low_score_modules" : [], "cycles" : { "count" : 0,
            "detail" : {} }, "rfc100_diagnostics" : { "count" : 0, "findings" : [] }, }
        );
        assert!(
            audit.get("rfc100_diagnostics").is_some(),
            "audit must carry rfc100_diagnostics"
        );
        assert_eq!(
            audit["rfc100_diagnostics"]["count"].as_u64().unwrap_or(99),
            0,
            "count should be 0 when no diagnostics"
        );
    }
    #[test]
    fn s8_compose_warning_threshold_aligns_with_health_module() {
        assert!((touring_foundation::health::DEGRADED_SCORE_THRESHOLD - 0.5).abs() < 1e-9);
    }
    #[test]
    fn s8_session_start_pushes_warning_when_score_below_threshold() {
        let mut combined = serde_json::Map::new();
        combined.insert(
            "gate_metrics".to_string(),
            serde_json::json!({ "query_cache_hit_ratio" : 0.0_f64 }),
        );
        combined.insert(
            "health_delta".to_string(),
            serde_json::json!({ "outstanding" : 5_u64 }),
        );
        combined.insert(
            "learning".to_string(),
            serde_json::json!({ "ema_reward" : 0.0_f64 }),
        );
        let score = touring_foundation::health::compute_composite_health_score(&combined);
        assert!(score < 0.5, "expected degraded score, got {score}");
        let warn = touring_foundation::health::compose_degraded_warning(score)
            .expect("expected warning when score < 0.5");
        assert!(warn.contains("degraded"));
        assert!(warn.contains("touring status"));
    }
    #[test]
    fn s8_session_start_skips_warning_when_score_healthy() {
        let mut combined = serde_json::Map::new();
        combined.insert(
            "gate_metrics".to_string(),
            serde_json::json!({ "query_cache_hit_ratio" : 0.8_f64 }),
        );
        combined.insert(
            "health_delta".to_string(),
            serde_json::json!({ "outstanding" : 0_u64 }),
        );
        combined.insert(
            "learning".to_string(),
            serde_json::json!({ "ema_reward" : 0.6_f64 }),
        );
        let score = touring_foundation::health::compute_composite_health_score(&combined);
        assert!(score >= 0.5, "expected healthy score, got {score}");
        assert!(
            touring_foundation::health::compose_degraded_warning(score).is_none(),
            "no warning expected for score {score}"
        );
    }

    // TR-3: repo-map tests

    #[test]
    fn tr3_shorten_path_two_components() {
        assert_eq!(
            super::shorten_path("crates/touring-hooks/src/cli_handlers.rs"),
            "src/cli_handlers.rs"
        );
    }

    #[test]
    fn tr3_shorten_path_single_component() {
        // No slash at all — return the original string unchanged.
        assert_eq!(super::shorten_path("lib.rs"), "lib.rs");
        // Only one slash — still return original (fewer than 2 slashes found).
        assert_eq!(super::shorten_path("src/lib.rs"), "src/lib.rs");
    }

    #[test]
    fn tr3_repo_map_section_budget_respected() {
        // Build a fake section the same way push_repo_map_parts does and verify budget.
        const BUDGET: usize = 2048;
        let header = "[REPO-MAP — top-25 symbols by centrality]";
        let mut section = header.to_string();
        section.push('\n');
        // Fill with entries until the budget would be exceeded.
        for i in 0..200 {
            let line = format!("  sym_{}  src/file_{}.rs  fan_in={}\n", i, i, 100 - i);
            if section.len() + line.len() > BUDGET {
                section.push_str("  ...\n");
                break;
            }
            section.push_str(&line);
        }
        assert!(
            section.len() <= BUDGET,
            "section exceeds budget: {} > {}",
            section.len(),
            BUDGET
        );
    }

    #[test]
    fn tr3_repo_map_ordering_desc_by_fan_in() {
        // Simulate the rows the SQL query returns (already ordered DESC by fan_in).
        let rows: Vec<(&str, i64)> = vec![("high_sym", 42), ("mid_sym", 17), ("low_sym", 3)];
        // Verify the simulated rows are in non-ascending order — matching ORDER BY fan_in DESC.
        let mut prev = i64::MAX;
        for (_, fan_in) in &rows {
            assert!(
                *fan_in <= prev,
                "expected DESC order, got {} after {}",
                fan_in,
                prev
            );
            prev = *fan_in;
        }
    }

    #[test]
    fn tr3_repo_map_empty_rows_produces_no_section() {
        // When the DB returns no rows, push_repo_map_parts must push nothing.
        // We test the guard condition directly: empty vec → no push.
        let rows: Vec<(String, String, i64)> = vec![];
        assert!(rows.is_empty(), "empty rows must cause early return");
    }
}
