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

/// `cli-code-mode-run` — contabiliza, NO DAEMON, uma execução de code mode.
///
/// `record_code_mode_run` incrementa um counter do PROCESSO. O executor
/// (`ctx_execute_impl`) já o chama, mas o `touring run` é um CLI efêmero: o
/// incremento morria com ele, enquanto o `journal_run` da linha seguinte —
/// que escreve em disco — sobrevivia. Medido em 24/08/2026: 163 execuções no
/// `run_journal.jsonl` contra `code_mode_runs_count = 0` no daemon, que é de
/// onde o `gate-metrics` lê.
///
/// O adaptador CLI retransmite por aqui, espelhando o que a C2-W0 já fazia
/// para as sub-chamadas do `--orchestrate` (contadas no daemon, em
/// `daemon.rs`, quando chegam pelo socket). Fail-open no chamador: perder um
/// counter nunca pode derrubar a execução do usuário.
pub fn cli_code_mode_run(_rt: &mut HookRuntime, payload: &serde_json::Value) -> String {
    let bytes_elided = payload
        .get("bytes_elided")
        .and_then(serde_json::Value::as_u64)
        .unwrap_or(0);
    crate::shared::gate_metrics::record_code_mode_run(bytes_elided);
    format!("{{\"recorded\":true,\"bytes_elided\":{bytes_elided}}}")
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
    // modes the 2026-05-11 audit identified: path pollution
    // (non_wireable_rows), race condition (kind_unknown_count), and pure orphan
    // count. Kept byte-for-byte in step with `touring-server`'s
    // `check_wiring_diagnostic` — the handler operators actually reach. This
    // one is dispatched as `cli-doctor` but invoked only by IPC tests, which is
    // precisely how a field added here on 2026-08-19 (`root=`) shipped without
    // ever appearing in anyone's output.
    let (kdb_status, kdb_detail) = match rt.ctx.knowledge.wiring_db_diagnostic() {
        Ok(diag) => {
            // `root` first, because it is the field whose ABSENCE hid a bug for
            // two months: the wiring paths of one project were being
            // canonicalized against another project's root, and no diagnostic
            // said which root was in force (2026-08-19). A census of rows is
            // only meaningful once you know what the rows are relative to.
            let detail = format!(
                "root={} polyglot={} rows={} producers={} consumers={} pub={} distinct={} unknown_kind={} non_wireable={} unread={}",
                rt.ctx
                    .knowledge
                    .workspace_root()
                    .unwrap_or("<não derivável>"),
                if rt.ctx.knowledge.polyglot() {
                    "on"
                } else {
                    "off"
                },
                diag.total_rows,
                diag.producer_rows,
                diag.consumer_rows,
                diag.pub_producers,
                diag.distinct_pub_symbols,
                diag.kind_unknown_count,
                diag.non_wireable_rows,
                diag.unread_rows,
            );
            // Degrade to "warning" only for rows NO read admits, or for the
            // schema race contaminating kinds — those make orphan counts
            // unreliable even when the daemon answers fine. `unread_rows` is
            // deliberately absent from this condition: a Python source the
            // current mode filters out is a mode setting, not a defect.
            let status = if diag.non_wireable_rows > 0 || diag.kind_unknown_count > 0 {
                "warning"
            } else {
                "ok"
            };
            (status, detail)
        }
        Err(e) => ("error", format!("wiring_db_diagnostic: {}", e)),
    };

    // I16 (2026-09-13): the seal on the latest rebuild, in step with
    // `touring-server`'s `check_index_generation` (the handler operators reach).
    let (gen_status, gen_detail) = match rt.ctx.knowledge.index_generation_state(std::process::id())
    {
        Ok(state) => (
            match state.state {
                "complete" | "none" => "ok",
                "building" => "building",
                _ => "partial",
            },
            state.detail,
        ),
        Err(e) => ("error", format!("index_generation: {e}")),
    };

    let checks = serde_json::json!(
        [{ "name" : "binary_version", "status" : "ok", "detail" : binary_version }, {
        "name" : "daemon_socket", "status" : socket_status, "detail" : socket_detail }, {
        "name" : "daemon_health", "status" : daemon_health_status, "detail" :
        daemon_health_detail }, { "name" : "circuit_breaker", "status" : circuit_status,
        "detail" : circuit_detail }, { "name" : "knowledge_db", "status" : kdb_status,
        "detail" : kdb_detail }, { "name" : "index_generation", "status" : gen_status,
        "detail" : gen_detail }]
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
    // W7 S-7.3 (plano code-mode-total) — a régua de adoção no dashboard:
    // runs / bash_calls (denominador vivo do W0) + resumo dos gate_events.
    // Ausência de denominador = null, nunca 0 (Lei L2).
    let gm = crate::shared::gate_metrics::global();
    let runs = gm
        .code_mode_runs_count
        .load(std::sync::atomic::Ordering::Relaxed);
    let bash_calls = gm
        .bash_calls_total_count
        .load(std::sync::atomic::Ordering::Relaxed);
    let adoption_ratio = if bash_calls > 0 {
        serde_json::json!(runs as f64 / bash_calls as f64)
    } else {
        serde_json::Value::Null
    };
    let gates: serde_json::Map<String, serde_json::Value> =
        crate::shared::gate_metrics::GateId::all()
            .iter()
            .map(|g| {
                let denied = gm.gate_events[*g as usize]
                    [crate::shared::gate_metrics::GateEvent::Denied as usize]
                    .load(std::sync::atomic::Ordering::Relaxed);
                let followed = gm.gate_events[*g as usize]
                    [crate::shared::gate_metrics::GateEvent::Followed as usize]
                    .load(std::sync::atomic::Ordering::Relaxed);
                (
                    g.label().to_string(),
                    serde_json::json!({ "denied": denied, "followed": followed }),
                )
            })
            .collect();
    let (pipeline, heat) = pipeline_and_heat_status(rt);
    let file_edges = file_edges_status(rt);
    let status = serde_json::json!(
        { "index" : { "symbol_count" : symbol_count, "file_count" : file_count,
        "initialized" : rt.infra.symbol_store.is_some() }, "wiring" : { "orphan_count" :
        orphan_count }, "learning" : { "ema_reward" : ema_reward }, "daemon_health" :
        daemon_health, "composite_health_score" : composite_health_score,
        "code_mode" : { "runs" : runs, "bash_calls" : bash_calls,
        "adoption_ratio" : adoption_ratio, "gates" : gates }, "pipeline" : pipeline,
        "heat" : heat, "file_edges" : file_edges, "cortex_pipeline" : "not_counted" }
    );
    serde_json::to_string(&status)
        .unwrap_or_else(|_| r#"{"error":"serialization failed"}"#.to_string())
}

/// C7: the `pipeline` and `heat` sections of `touring status -j` — what the daemon's
/// incremental pipeline holds, what its memory budget has evicted, and how much heat
/// the budget and the session-start prefetch have used. Absence is `null`, never `0`
/// (no pipeline; a hook holding the heat map at this instant). `budget_source` says who
/// decided `budget_mb`: `env` (the knob), `default` (derived from the machine) or
/// `disabled` (`TOURING_PIPELINE_MEMORY_MB=0`).
///
/// The daemon holds one more real pipeline, the cortex's (`incremental_indexing.rs`),
/// which this budget does not govern and these numbers do not include: `cli_status`
/// says so with `"cortex_pipeline": "not_counted"` rather than underestimating.
fn pipeline_and_heat_status(rt: &HookRuntime) -> (serde_json::Value, serde_json::Value) {
    let stats = rt.infra.pipeline.as_ref().map(|p| p.stats());
    let pipeline = stats.map_or(serde_json::Value::Null, |s| {
        serde_json::json!({
            "documents": s.documents,
            "trees": s.trees,
            "memory_mb": s.memory_mb,
            "budget_mb": s.budget_mb,
            "budget_source": rt.infra.pipeline_budget_source.as_str(),
            "evictions": s.evictions,
        })
    });
    let tracked = rt
        .learning
        .heat_map
        .try_borrow()
        .ok()
        .map(|heat| heat.len());
    let mut heat = serde_json::json!({
        "tracked": tracked,
        "consulted": stats.map(|s| s.heat_consulted),
        "prefetched": stats.map(|s| s.heat_prefetched),
    });
    // A `null` is never left to be read as a zero: the section says why, field by field.
    let mut reasons: Vec<&str> = Vec::new();
    if tracked.is_none() {
        reasons.push("tracked: the heat map is held by a hook right now");
    }
    if stats.is_none() {
        reasons.push("consulted, prefetched: the daemon has no incremental pipeline");
    }
    if !reasons.is_empty() {
        heat["reason"] = serde_json::Value::String(reasons.join("; "));
    }
    (pipeline, heat)
}

/// C8: the `file_edges` section of `touring status -j` — whether the dependency index (and
/// with it the wiring's resolved file-to-file edges the hooks' graph signals read) is loaded,
/// how big it is, and who decided. `source` is `default` or `disabled`
/// (`TOURING_FILE_EDGES_DISABLED=1`, read when the runtime was built); `enabled` is false
/// exactly when it is `disabled`. `edges` and `covered_files` are `null`, never `0`, while no
/// index is loaded, and `reason` says why: an unloaded index is not an index with no edges.
/// Asking never loads it: the index loads on the first read that needs it (a hook's graph
/// signal), so an enabled project nobody has read yet reports `edges: null` and the reason
/// below — the state right after the daemon opened it — not a number the status itself paid for.
fn file_edges_status(rt: &HookRuntime) -> serde_json::Value {
    let source = rt.infra.file_edges_source;
    let coverage = rt
        .infra
        .symbol_index
        .get()
        .map(|generation| generation.index().edge_coverage_summary());
    let mut section = serde_json::json!({
        "enabled": source.enabled(),
        "source": source.as_str(),
        "edges": coverage.map(|c| c.edges),
        "covered_files": coverage.map(|c| c.covered_files),
    });
    if coverage.is_none() {
        let reason = if source.enabled() {
            "the dependency index is not loaded yet: it loads on the first hook read that \
             needs a graph signal (and `touring index rebuild` refreshes it once loaded)"
        } else {
            "TOURING_FILE_EDGES_DISABLED is set: the index is not loaded and the graph \
             signals stay silent (unset it and restart the daemon)"
        };
        section["reason"] = serde_json::Value::String(reason.to_string());
    }
    section
}

#[cfg(test)]
mod status_pipeline_tests {
    use super::cli_status;
    use crate::runtime::HookRuntime;
    use touring_code::ast::SharedPipeline;

    /// Comment padding: a big document with almost no symbols to extract.
    fn padded(bytes: usize) -> String {
        format!(
            "pub fn f() {{}}\n{}",
            "// padding padding padding padding padding padding padding\n".repeat(bytes / 58 + 1)
        )
    }

    fn status_of(rt: &mut HookRuntime) -> serde_json::Value {
        serde_json::from_str(&cli_status(rt, &serde_json::json!({}))).expect("status json")
    }

    #[test]
    fn status_reports_the_pipeline_memory_and_what_the_budget_did() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let mut rt = HookRuntime::new(tmp.path()).expect("runtime");
        rt.infra.pipeline = Some(SharedPipeline::new().with_memory_budget_mb(1));
        for name in ["a.rs", "b.rs", "c.rs"] {
            rt.process_file(name, &padded(400 * 1024))
                .expect("process_file");
        }

        let status = status_of(&mut rt);
        let pipeline = &status["pipeline"];
        assert_eq!(pipeline["budget_mb"], 1, "{status}");
        assert_eq!(
            pipeline["evictions"], 1,
            "3 x 400 KiB over 1 MiB: one victim"
        );
        assert_eq!(pipeline["documents"], 2);
        assert_eq!(pipeline["trees"], 2, "the victim's tree left with it");
        let memory = pipeline["memory_mb"].as_f64().expect("memory_mb");
        assert!(memory > 0.5 && memory <= 1.0, "inside the budget: {memory}");
    }

    #[test]
    fn status_names_where_the_pipeline_budget_came_from() {
        use touring_hook_runtime::hook_runtime::BudgetSource;
        let tmp = tempfile::tempdir().expect("tempdir");
        let mut rt = HookRuntime::new(tmp.path()).expect("runtime");
        for source in [
            BudgetSource::Env,
            BudgetSource::Default,
            BudgetSource::Disabled,
        ] {
            rt.infra.pipeline_budget_source = source;
            assert_eq!(
                status_of(&mut rt)["pipeline"]["budget_source"],
                source.as_str()
            );
        }
    }

    /// An index holding one resolved edge (`app.rs` imports `lib.rs`) over two covered files.
    fn index_with_one_edge() -> touring_code::ast::SymbolIndex {
        let mut index = touring_code::ast::SymbolIndex::new();
        index.load_file_edges(
            [("src/app.rs".to_string(), "src/lib.rs".to_string())],
            ["src/app.rs".to_string(), "src/lib.rs".to_string()],
        );
        index
    }

    #[test]
    fn status_file_edges_on_default_reports_the_loaded_index() {
        use touring_hook_runtime::index_slot::FileEdgesSource;
        let tmp = tempfile::tempdir().expect("tempdir");
        let mut rt = HookRuntime::new(tmp.path()).expect("runtime");
        rt.infra.file_edges_source = FileEdgesSource::Default;
        rt.infra.symbol_index.replace(index_with_one_edge());

        let section = status_of(&mut rt)["file_edges"].clone();
        assert_eq!(section["enabled"], true, "{section}");
        assert_eq!(section["source"], "default", "{section}");
        assert_eq!(section["edges"], 1, "{section}");
        assert_eq!(section["covered_files"], 2, "{section}");
        assert!(
            section.get("reason").is_none(),
            "numbers are present, so nothing to explain: {section}"
        );
    }

    #[test]
    fn status_file_edges_when_disabled_says_so_and_never_prints_zero() {
        use touring_hook_runtime::index_slot::{FileEdgesSource, IndexLoad};
        let tmp = tempfile::tempdir().expect("tempdir");
        let mut rt = HookRuntime::new(tmp.path()).expect("runtime");
        rt.infra.file_edges_source = FileEdgesSource::Disabled;
        // The switch is honoured by the loader itself: nothing is read, the slot stays empty.
        assert_eq!(rt.load_symbol_index(), IndexLoad::Disabled);

        let section = status_of(&mut rt)["file_edges"].clone();
        assert_eq!(section["enabled"], false, "{section}");
        assert_eq!(section["source"], "disabled", "{section}");
        assert!(
            section["edges"].is_null() && section["covered_files"].is_null(),
            "an index that was not loaded is not an index with 0 edges: {section}"
        );
        assert!(
            section["reason"]
                .as_str()
                .is_some_and(|r| r.contains("TOURING_FILE_EDGES_DISABLED")),
            "the null names the switch: {section}"
        );
    }

    #[test]
    fn status_file_edges_enabled_but_not_loaded_yet_says_so() {
        use touring_hook_runtime::index_slot::FileEdgesSource;
        let tmp = tempfile::tempdir().expect("tempdir");
        let mut rt = HookRuntime::new(tmp.path()).expect("runtime");
        rt.infra.file_edges_source = FileEdgesSource::Default;

        let section = status_of(&mut rt)["file_edges"].clone();
        assert_eq!(section["enabled"], true, "{section}");
        assert!(section["edges"].is_null(), "{section}");
        assert!(
            section["reason"]
                .as_str()
                .is_some_and(|r| r.contains("not loaded yet")),
            "{section}"
        );
    }

    /// Asking for the status is not a read that needs the graph: it must leave the slot empty
    /// and must not even attempt the load (an attempt on a project with no store would leave
    /// the `failed` mark, which `index rebuild` clears — so the mark is the witness).
    #[test]
    fn status_never_loads_the_dependency_index() {
        use touring_hook_runtime::index_slot::FileEdgesSource;
        let tmp = tempfile::tempdir().expect("tempdir");
        let mut rt = HookRuntime::new(tmp.path()).expect("runtime");
        rt.infra.file_edges_source = FileEdgesSource::Default;
        // On demand, like every runtime the daemon builds: the one that could load.
        rt.infra.index_on_demand = true;

        for _ in 0..2 {
            let _ = status_of(&mut rt);
        }
        assert!(
            !rt.infra.symbol_index.is_loaded(),
            "status must not load the index"
        );
        assert!(
            !rt.infra.symbol_index.load_failed(),
            "status must not even try: an attempt on a store-less project would mark the slot failed"
        );
    }

    #[test]
    fn status_declares_the_cortex_pipeline_not_counted_instead_of_underestimating() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let mut rt = HookRuntime::new(tmp.path()).expect("runtime");
        assert_eq!(status_of(&mut rt)["cortex_pipeline"], "not_counted");
    }

    #[test]
    fn status_reports_how_much_heat_is_tracked_consulted_and_prefetched() {
        let tmp = tempfile::tempdir().expect("tempdir");
        std::fs::create_dir_all(tmp.path().join("src")).expect("src");
        std::fs::write(tmp.path().join("src/hot.rs"), "pub fn hot() {}\n").expect("write");
        let mut rt = HookRuntime::new(tmp.path()).expect("runtime");
        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs_f64())
            .unwrap_or(0.0);
        rt.learning
            .heat_map
            .borrow_mut()
            .record_edit("src/hot.rs", now);
        assert_eq!(rt.prefetch_hot_files(4), 1);

        let status = status_of(&mut rt);
        assert_eq!(status["heat"]["tracked"], 1, "{status}");
        assert_eq!(status["heat"]["prefetched"], 1);
        assert_eq!(
            status["heat"]["consulted"], 0,
            "no eviction has needed the heat yet"
        );
    }

    #[test]
    fn status_without_a_pipeline_says_null_never_zero() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let mut rt = HookRuntime::new(tmp.path()).expect("runtime");
        rt.infra.pipeline = None;
        let status = status_of(&mut rt);
        assert!(status["pipeline"].is_null(), "{status}");
        assert!(
            status["heat"]["prefetched"].is_null(),
            "absence is not zero"
        );
        let reason = status["heat"]["reason"].as_str().unwrap_or_default();
        assert!(
            reason.contains("consulted, prefetched") && reason.contains("no incremental pipeline"),
            "the null fields say why: {status}"
        );
        assert!(
            !reason.contains("tracked"),
            "`tracked` was readable, so it is not blamed: {status}"
        );
    }

    #[test]
    fn status_says_why_tracked_is_null_when_a_hook_holds_the_heat_map() {
        let tmp = tempfile::tempdir().expect("tempdir");
        let mut rt = HookRuntime::new(tmp.path()).expect("runtime");
        let free = status_of(&mut rt);
        assert_eq!(free["heat"]["tracked"], 0, "control: a free map is read");
        assert!(
            free["heat"].get("reason").is_none(),
            "nothing is null, so there is nothing to explain: {free}"
        );

        // `cli_status` takes `&mut HookRuntime`, which the held guard forbids: read the
        // section through the function the handler calls.
        let _held = rt.learning.heat_map.borrow_mut();
        let (_, busy) = super::pipeline_and_heat_status(&rt);
        assert!(busy["tracked"].is_null(), "{busy}");
        assert!(
            busy["reason"]
                .as_str()
                .is_some_and(|r| r.contains("tracked") && r.contains("held by a hook")),
            "the null says why: {busy}"
        );
    }
}
