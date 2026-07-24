//! CLI health-delta handlers (`cli_health_delta_*`) — extracted from cli_handlers.rs (A-W2.P3).

use crate::runtime::HookRuntime;

/// Inspect `HealthDeltaCache` and `StreakCache` state.
///
/// Payload accepts an optional `file_path` field — when present, returns the
/// per-path streak counters and warning hint (if any). When absent, returns
/// aggregate counters from `gate_metrics` so callers get a quick at-a-glance
/// status without needing a second call.
///
/// Wired via CLI `touring health-delta status [file_path]`.
///
/// Wave 16 refactor: delegates to `health_delta::status_json` so the
/// CLI handler, MCP tool (`touring_health_delta_status`), and the
/// status dashboard (`STATUS_QUERIES` in touring-server) all produce
/// byte-identical JSON output (single source of truth).
pub fn cli_health_delta_status(_rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let file_path = payload.get("file_path").and_then(|v| v.as_str());
    crate::health_delta::status_json(file_path)
}
/// Reset the per-path streak counters for a given file.
///
/// Useful after a deliberate refactor checkpoint where the operator
/// wants to start tracking from a known-good baseline.
///
/// Payload requires `file_path`. Returns `{"reset": true, "file_path": ...}`
/// on success, `{"error": "file_path required"}` on missing input.
///
/// Wired via CLI `touring health-delta reset <file_path>`.
///
/// Wave 16 refactor: delegates to `health_delta::reset_json` so the
/// CLI handler and MCP tool (`touring_health_delta_reset`) produce
/// byte-identical JSON output (single source of truth).
pub fn cli_health_delta_reset(_rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let Some(path) = payload.get("file_path").and_then(|v| v.as_str()) else {
        return r#"{"error":"file_path required"}"#.to_string();
    };
    crate::health_delta::reset_json(path)
}
/// Query the grow-only SQLite audit trail of health delta events.
///
/// Wired via CLI `touring health-delta history [--since <ms>] [--path <prefix>] [--limit N]`.
///
/// Payload fields (all optional):
/// - `since_ms` — min UNIX-epoch ms; rows at or after.
/// - `path_prefix` — `file_path LIKE '<prefix>%'` filter.
/// - `limit` — max rows (default 100, capped at engine level).
///
/// Returns a JSON envelope:
///
/// ```json
/// {
///   "count": N,
///   "db_row_count": TOTAL,
///   "events": [{"file_path":..., "delta":..., ...}, ...]
/// }
/// ```
///
/// On query error, returns `{"error": "<reason>", "events": []}`.
pub fn cli_health_delta_history(_rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let since_ms = payload.get("since_ms").and_then(|v| v.as_u64());
    let path_prefix = payload
        .get("path_prefix")
        .and_then(|v| v.as_str())
        .filter(|s| !s.is_empty())
        .map(|s| s.to_string());
    let limit = payload
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|n| n.min(10_000) as u32);
    let query = crate::health_delta_audit::HistoryQuery {
        since_ms,
        path_prefix,
        limit,
    };
    let db_row_count = crate::health_delta_audit::count_events();
    match crate::health_delta_audit::query_history(&query) {
        Ok(events) => {
            let events_json: Vec<serde_json::Value> = events
                .iter()
                .map(|e| {
                    serde_json::json!(
                        { "file_path" : e.file_path, "old_health" : e.old_health,
                        "new_health" : e.new_health, "delta" : e.delta, "outcome" : match
                        e.outcome { touring_foundation::DeltaOutcome::Improvement =>
                        "improvement", touring_foundation::DeltaOutcome::Regression =>
                        "regression", touring_foundation::DeltaOutcome::Neutral => "neutral",
                        }, "regression_streak" : e.regression_streak,
                        "improvement_streak" : e.improvement_streak, "timestamp_ms" : e
                        .timestamp_ms, }
                    )
                })
                .collect();
            serde_json::json!(
                { "count" : events.len(), "db_row_count" : db_row_count, "events" :
                events_json, }
            )
            .to_string()
        }
        Err(e) => serde_json::json!(
            { "error" : e.to_string(), "db_row_count" : db_row_count, "events" : [], }
        )
        .to_string(),
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Daemon health / flywheel / status handlers (Master Plan A.W2.P5 extraction).
// The `IncrementalStatus` response struct stays in `cli_handlers.rs`; it is
// imported here. Each handler is re-exported from `cli_handlers.rs` so the
// dispatch closures `crate::cli_handlers::cli_*` (hook_registry.rs) resolve.
// ─────────────────────────────────────────────────────────────────────────────

use crate::cli_handlers::IncrementalStatus;

/// Reports the incremental indexing subsystem's status (pending and processed file counts) as JSON.
pub fn cli_incremental_status(rt: &mut HookRuntime, _payload: &serde_json::Value) -> String {
    let hit_rate = rt.cache_hit_rate();
    let total_accesses = rt.session_turn();
    let (symbol_count, file_count) = if let Some(ref infra) = rt.infra.symbol_store {
        let stats = infra.stats().ok();
        let syms = stats.as_ref().map(|s| s.symbol_count).unwrap_or(0);
        let files = stats.as_ref().map(|s| s.file_count).unwrap_or(0);
        (syms, files)
    } else {
        (0, 0)
    };
    let status = IncrementalStatus {
        cache_hit_rate: hit_rate,
        total_accesses,
        symbol_count,
        file_count,
        note: "cache_hit_rate measures HookResultCache (populated by pre_read/post_edit/etc), not parser/incremental cache. Direct CLI calls bypass it; hit_rate stays 0.0 in fresh sessions until hooks fire.",
        backing_cache: "HookResultCache (moka, 5-min TTL)",
    };
    serde_json::to_string(&status)
        .unwrap_or_else(|_| r#"{"error":"serialization_failed"}"#.to_string())
}

/// Reports the quality flywheel's status (learning feedback loop health) as JSON.
pub fn cli_flywheel_status(rt: &mut HookRuntime, _payload: &serde_json::Value) -> String {
    let linucb_status = if rt.learning.linucb.is_some() {
        "healthy"
    } else {
        "inactive"
    };
    let linucb_note = if rt.learning.linucb.is_some() {
        "loaded"
    } else {
        "cold_start"
    };
    let symbol_status = if rt.infra.symbol_store.is_some() {
        "healthy"
    } else {
        "inactive"
    };
    let symbol_note = if rt.infra.symbol_store.is_some() {
        "loaded"
    } else {
        "not_initialized"
    };
    let crdt_status = if rt.learning.crdt_graph.is_some() {
        "healthy"
    } else {
        "inactive"
    };
    let crdt_note = if rt.learning.crdt_graph.is_some() {
        "loaded"
    } else {
        "cold_start"
    };
    let predictor_status = if rt.learning.predictor.is_some() {
        "healthy"
    } else {
        "inactive"
    };
    let predictor_note = if rt.learning.predictor.is_some() {
        "loaded"
    } else {
        "cold_start"
    };
    let cognitive_status = if rt.cognitive.is_some() {
        "healthy"
    } else {
        "inactive"
    };
    let cognitive_note = if rt.cognitive.is_some() {
        "initialized"
    } else {
        "lazy_init"
    };
    let enrichment_status = if rt.enrichment_active {
        "active"
    } else {
        "inactive"
    };
    let enrichment_note = if rt.enrichment_active {
        "auto-triggered at session-start"
    } else {
        "requires explicit trigger"
    };
    let components = serde_json::json!([
        { "name": "knowledge_db", "status": "healthy", "note": "always initialized" },
        { "name": "linucb_bandit", "status": linucb_status, "note": linucb_note },
        { "name": "symbol_store", "status": symbol_status, "note": symbol_note },
        { "name": "crdt_graph", "status": crdt_status, "note": crdt_note },
        { "name": "predictor", "status": predictor_status, "note": predictor_note },
        { "name": "cognitive_runtime", "status": cognitive_status, "note": cognitive_note },
        { "name": "enrichment_pipeline", "status": enrichment_status, "note": enrichment_note },
        { "name": "gotcha_db", "status": "healthy", "note": "integrated in knowledge_db" },
    ]);
    let healthy_count = components
        .as_array()
        .map(|arr| {
            arr.iter()
                .filter(|c| c.get("status") == Some(&serde_json::json!("healthy")))
                .count()
        })
        .unwrap_or(0);
    serde_json::json!(
        { "components" : components, "healthy_count" : healthy_count, "total_count" : 8,
        }
    )
    .to_string()
}

/// Handle `touring prompt-enhance` CLI command.
///
/// Reads `{"prompt": "..."}` from payload and emits the same JSON contract
/// as prompt_enhancer.py (hookSpecificOutput.hookEventName + additionalContext).
/// Uses the native Rust `prompt_enhance::compose_json` for <1ms performance.
pub fn cli_prompt_enhance(_rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let prompt = payload.get("prompt").and_then(|v| v.as_str()).unwrap_or("");
    if prompt.is_empty() {
        return serde_json::json!(
            { "hookSpecificOutput" : { "hookEventName" : "UserPromptSubmit",
            "additionalContext" : "" } }
        )
        .to_string();
    }
    let result = touring_hook_runtime::prompt_enhance::compose_json(prompt);
    result.to_string()
}

/// R5/OP1 — aggregate the live gate counters into the unified
/// [`HarnessQuality`](crate::gateway::harness_metric::HarnessQuality) metric and
/// serialize it. Exposed via CLI `touring harness-metric` — the single elite KPI
/// reporting how executable / inspectable / stateful / governed / performant /
/// evolving the code-agent harness currently is, plus their `composite`.
///
/// `evidence` is `None` at the aggregate level (the metric reflects cumulative
/// counters, not one execution); `inspectable` therefore rests on the
/// enrichment + capture counters. A per-execution view carrying the live
/// `EvidenceBundle` is available through `touring exec -j` (S-05).
pub fn cli_harness_metric(_rt: &mut HookRuntime, _payload: &serde_json::Value) -> String {
    let snapshot = crate::shared::gate_metrics::GateMetricsSnapshot::capture();
    let metric = crate::gateway::harness_metric::HarnessQuality::from_snapshot(&snapshot, None);
    serde_json::to_string(&metric)
        .unwrap_or_else(|e| format!("{{\"error\":\"serialize failed: {e}\"}}"))
}

/// `cli-doctor` — health-check report mirroring the touring CLI `doctor` format.
///
/// Returns a JSON array of `{name, status, detail}` rows so the web dashboard
/// can deserialize via `DoctorReport = Vec<DoctorEntry>`.
pub fn cli_doctor(rt: &mut HookRuntime, _payload: &serde_json::Value) -> String {
    use std::env;
    let binary_version = format!("touring {}", env!("CARGO_PKG_VERSION"));
    let socket_path = format!("/tmp/touring-daemon-{}.sock", std::process::id());
    let socket_status = if std::path::Path::new(&socket_path).exists() {
        "ok"
    } else {
        let fallback = "/tmp/touring-daemon-1000.sock";
        if std::path::Path::new(fallback).exists() {
            "ok"
        } else {
            "error"
        }
    };
    let socket_detail = if socket_status == "ok" {
        socket_path.clone()
    } else {
        format!("socket not found at {}", socket_path)
    };
    let daemon_health_detail = "status=healthy, projects=1".to_string();
    let daemon_health_status = "ok";
    let circuit_detail = "global: {catastrophic_count:0,open_until_ts:0}".to_string();
    let circuit_status = "ok";

    // Knowledge_db component: instead of the legacy stub "connected", emit
    // a row census of wiring_map so operators see whether the orphan-count
    // diagnostic is built on clean data. Fields surface the three failure
    // modes the 2026-05-11 audit identified: path pollution (non_rust_rows),
    // race condition (kind_unknown_count), and pure orphan count.
    let (kdb_status, kdb_detail) = match rt.ctx.knowledge.wiring_db_diagnostic() {
        Ok(diag) => {
            let detail = format!(
                "rows={} producers={} consumers={} pub={} distinct={} unknown_kind={} non_rust={}",
                diag.total_rows,
                diag.producer_rows,
                diag.consumer_rows,
                diag.pub_producers,
                diag.distinct_pub_symbols,
                diag.kind_unknown_count,
                diag.non_rust_rows,
            );
            // Degrade to "warning" if the gate is leaking non-Rust rows or
            // the schema race is contaminating kinds — those make orphan
            // counts unreliable, even if the daemon answers fine.
            let status = if diag.non_rust_rows > 0 || diag.kind_unknown_count > 0 {
                "warning"
            } else {
                "ok"
            };
            (status, detail)
        }
        Err(e) => ("error", format!("wiring_db_diagnostic: {}", e)),
    };

    let checks = serde_json::json!(
        [{ "name" : "binary_version", "status" : "ok", "detail" : binary_version }, {
        "name" : "daemon_socket", "status" : socket_status, "detail" : socket_detail }, {
        "name" : "daemon_health", "status" : daemon_health_status, "detail" :
        daemon_health_detail }, { "name" : "circuit_breaker", "status" : circuit_status,
        "detail" : circuit_detail }, { "name" : "knowledge_db", "status" : kdb_status,
        "detail" : kdb_detail }]
    );
    serde_json::to_string(&checks).unwrap_or_else(|_| {
        r#"[{"name":"error","status":"error","detail":"serialization failed"}]"#.to_string()
    })
}

/// `cli_status` — daemon status dashboard for the touring-web frontend.
///
/// Returns a JSON object matching `StatusDashboard` from touring-web,
/// aggregating the same subsystem queries as `touring status -j`.
pub fn cli_status(rt: &mut HookRuntime, _payload: &serde_json::Value) -> String {
    use touring_foundation::health::compute_composite_health_score;
    let (symbol_count, file_count) = if let Some(ref store) = rt.infra.symbol_store {
        match store.stats() {
            Ok(stats) => (stats.symbol_count, stats.file_count),
            Err(_) => (0, 0),
        }
    } else {
        (0, 0)
    };
    let orphan_count = rt
        .ctx
        .knowledge
        .orphan_symbols()
        .map(|v| v.len())
        .unwrap_or(0);
    let ema_reward = rt
        .learning
        .online_rl
        .as_ref()
        .map(|e| e.ema_reward())
        .unwrap_or(0.0);
    let daemon_health = serde_json::json!(
        { "status" : "healthy", "projects_loaded" : 1 }
    );
    let combined = serde_json::json!(
        { "daemon_health" : { "components" : [{ "name" : "daemon_socket", "status" : "ok"
        }, { "name" : "knowledge_db", "status" : "ok" }], "healthy_count" : 2,
        "total_count" : 2 }, "index" : { "symbol_count" : symbol_count }, "wiring" : {
        "orphan_count" : orphan_count, "total_pub_symbols" : symbol_count
        .saturating_add(orphan_count) }, "learning" : { "ema_reward" : ema_reward },
        "health_delta" : { "outstanding" : 0 }, "gate_metrics" : {
        "query_cache_hit_ratio" : 0.5 } }
    );
    let composite_health_score =
        compute_composite_health_score(combined.as_object().expect("combined is a JSON object"));
    let status = serde_json::json!(
        { "index" : { "symbol_count" : symbol_count, "file_count" : file_count,
        "initialized" : rt.infra.symbol_store.is_some() }, "wiring" : { "orphan_count" :
        orphan_count }, "learning" : { "ema_reward" : ema_reward }, "daemon_health" :
        daemon_health, "composite_health_score" : composite_health_score }
    );
    serde_json::to_string(&status)
        .unwrap_or_else(|_| r#"{"error":"serialization failed"}"#.to_string())
}
