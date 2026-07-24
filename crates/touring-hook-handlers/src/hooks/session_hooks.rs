//! Session Lifecycle Hooks — session-start and session-stop.
//!
//! session-start: Loads knowledge stats, trend data, and predictor readiness,
//!   then injects session continuity context. Initializes ACO quality tracking.
//! session-stop: Generates final quality report and persists session summary
//!   for cross-session intelligence.

use super::error_predictor::ErrorPredictor;
use super::runtime::HookRuntime;
use super::session_insights::{self, SessionInsights};
use crate::cross_agent_ledger::{ActorId, CrossAgentLedger};
use crate::gateway::harness_contract::HarnessContract;
use crate::schemas::validate_payload;

use touring_analysis::e2e::schema_guard;
use touring_intelligence::rl::aco::tracker::TrackerStatus;

use std::sync::Arc;

/// Run the session-start hook. Injects knowledge stats, trend, and predictor status.
/// Also initializes ACO quality tracking for the session.
#[tracing::instrument(skip(runtime, input), fields(hook = "session_start"))]
pub fn run_session_start(
    runtime: &mut HookRuntime,
    input: &serde_json::Value,
) -> Result<(), touring_hook_runtime::hook_runtime::HookDispatchError> {
    // D9: Validate payload with typed schema — fail fast on malformed input.
    let validated = match validate_payload::<crate::schemas::SessionStartPayload>(input) {
        Ok(v) => v,
        Err(errors) => return Err(format!("session_start validation failed: {}", errors).into()),
    };
    let session_id = validated
        .session_id
        .unwrap_or_else(|| "unknown".to_string());

    // Initialize ACO quality tracking for this session
    runtime.reset_quality_tracking(&session_id);

    // R4: Warm-start bandit from previous session snapshot
    warm_start_bandit(runtime);

    // ES4 P1: Warm-load the durable action world model (X4 PREDICT data source) so
    // a restarted daemon inherits accumulated outcome history instead of predicting
    // a flat 0.5 cold-start. Once-per-process + fail-open; configures the snapshot
    // path for the debounced + session-stop persists.
    let _ = crate::gateway::outcome_learner::warm_load_global_model(&runtime.project_root);

    // Initialize cognitive engine for this session — enables MCTS, GoT, semantic graph
    runtime.init_cognitive();

    // C1: Spawn background cognitive tasks — 500ms warm cache loop keeps cognitive hot
    runtime.spawn_cognitive_background_tasks();

    // F1: Subscribe cognitive engine to CortexDispatcher broadcast for drift detection
    runtime.subscribe_to_cortex_dispatcher();

    // P3.1: Activate enrichment pipeline after cognitive init
    runtime.trigger_enrichment();

    // H1-C: Initialize dependency cache from SQLite file relations — enables blast radius analysis
    runtime.init_dependency_cache();

    // Initialize ANN semantic memory recall — enables cross-session similarity search
    runtime.init_ann_memory();

    // P3.3: Initialize Entity Registry for symbol disambiguation — lazy init, safe to call multiple times
    runtime.init_entity_registry();

    // Q3: Sync gotchas from YAML rule library into SQLite cache on first session start.
    // Loads all *.yaml/*.yml rules under the gotchas/ directory into the knowledge DB
    // so that pre_read/pre_edit hooks can surface them as GOTCHA signals.
    {
        use crate::gotcha_loader::sync_to_sqlite;
        // Productization Fase 0: follow the canonical workspace root
        // (`TOURING_WORKSPACE_ROOT` override → historical global default).
        let gotcha_root = std::env::var("TOURING_WORKSPACE_ROOT")
            .map(|r| format!("{}/docs/gotchas", r.trim_end_matches('/')))
            .unwrap_or_else(|_| "/home/gabrielgadea/projects/touring/docs/gotchas".to_string());
        let gotcha_dir = std::path::Path::new(&gotcha_root);
        if gotcha_dir.is_dir() {
            let _ = sync_to_sqlite(runtime, gotcha_dir);
        }
    }

    // D5: Load previous GoT snapshot for session continuity
    load_got_snapshot(runtime, &session_id);

    // I9: Cleanup entries older than 30 days to keep DB bounded
    let _ = runtime.ctx.knowledge.cleanup_old_entries(30);

    // T2.4: Pre-warm QTable cache to eliminate disk I/O in post-tool-rl
    if runtime.learning.qtable_cache.is_none() {
        let qtable_path = runtime.project_root.join(".claude/data/qtable.rkyv");
        if qtable_path.exists() {
            if let Ok((loaded, _rev)) = touring_intelligence::rl::QTable::load_rkyv(&qtable_path) {
                runtime.learning.qtable_cache = Some(loaded);
                tracing::debug!("QTable cache pre-warmed from disk");
            }
        } else {
            runtime.learning.qtable_cache = Some(touring_intelligence::rl::QTable::new());
        }
    }

    // S12: Pre-warm result cache with context for most accessed files
    prewarm_result_cache(runtime);

    // U10: Warm up the Tantivy FTS global singleton so the IndexReader is hot for queries.
    // open_or_create is called here via global_tantivy() — subsequent calls are O(1) OnceLock
    // reads. Failure is logged and silently swallowed (exit-0 invariant).
    #[cfg(feature = "tantivy-fts")]
    {
        if let Some(idx) = crate::tantivy_index::global_tantivy() {
            tracing::debug!(docs = idx.stats().total_docs, "tantivy warmup: index ready");
        }
    }

    // U20: Self-healing Tantivy health gate — logs degraded state after warmup.
    // Runs only when tantivy-fts feature is active. Exit-0 invariant: no panic, no block.
    #[cfg(feature = "tantivy-fts")]
    {
        if let Some(idx) = crate::tantivy_index::global_tantivy() {
            let stats = idx.stats();
            if stats.total_docs == 0 {
                tracing::warn!(
                    "tantivy health gate: index is EMPTY (0 docs). \
                     Possible degraded state — expected >0 if symbols are indexed."
                );
            } else {
                tracing::debug!(
                    total_docs = stats.total_docs,
                    size_bytes = stats.index_size_bytes,
                    "tantivy health gate: PASS"
                );
            }
        }
    }

    // Get knowledge stats — fallback to early return on error (e.g. DB locked during
    // daemon restart). Session-start must never fail — exit 0 invariant.
    // NOTE: We use `return Ok(())` instead of `emit_allow()` because `emit_allow()`
    // calls `process::exit(0)` which would kill the entire daemon process when this
    // handler runs inside the daemon's dispatch pipeline. Returning Ok(()) produces
    // the same effect: the hook registry wrapper returns `String::new()` → empty
    // output → client interprets as "allow".
    let stats = match runtime.ctx.knowledge.stats() {
        Ok(s) => s,
        Err(e) => {
            tracing::debug!(error = %e, "stats query failed — using empty defaults");
            return Ok(()); // safe for both daemon and standalone contexts
        }
    };

    if stats.file_count == 0 && stats.bash_count == 0 {
        // No accumulated knowledge yet — nothing to inject
        return Ok(());
    }

    // Compose context about accumulated knowledge
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

    // ── Trend data from prior session insights ──
    let data_dir = runtime.project_root.join(".claude").join("data");
    let mut current_insights =
        session_insights::extract_session_insights(&runtime.ctx.knowledge, &session_id);

    // EC44: Enrich insights with RL convergence metrics (td_error_ema, avg_reward,
    // total_updates, is_converging). First real caller of extract_evolution_insights().
    // Uses qtable_cache if loaded — gracefully skips when QTable unavailable.
    session_insights::extract_evolution_insights(
        &mut current_insights,
        runtime
            .learning
            .qtable_cache
            .as_ref()
            .map(|qt| qt.metrics()),
    );

    // EC44: Persist enriched insights so next session load_latest() returns RL-aware data.
    if let Err(e) = current_insights.save(&data_dir) {
        tracing::debug!("session insights save failed (non-critical): {e}");
    }

    if let Some(prior) = SessionInsights::load_latest(&data_dir) {
        // EC45: First real caller of summary_line() — injects compact prior session
        // summary into session start context per its docstring: "for injection into session context".
        parts.push(prior.summary_line());
        let trend = session_insights::compute_trend(&current_insights, &prior);
        parts.push(format!("trend={}", trend.trend_direction));
        if !trend.new_gotchas.is_empty() {
            parts.push(format!("{} new gotchas", trend.new_gotchas.len()));
        }
    }

    // ── Error predictor readiness (T2.5: also pre-warm cache) ──
    let mut predictor = ErrorPredictor::new();
    let seq_count = predictor.train_from_db(&runtime.ctx.knowledge);
    if predictor.is_ready() {
        parts.push(format!("predictor=ready({seq_count} seq)"));
    } else if seq_count > 0 {
        parts.push(format!("predictor=warming({seq_count} seq)"));
    }
    // T2.5: Cache the trained predictor for use by pre_edit/pre_write
    runtime.ctx.error_predictor = Some(predictor);
    runtime.ctx.error_predictor_last_trained = Some(std::time::Instant::now());

    // ── ACO quality tracking status ──
    parts.push("quality_tracking=active".to_string());

    // ── Cognitive engine status ──
    if runtime.cognitive.is_some() {
        parts.push("cognitive=active".to_string());
    } else {
        parts.push("cognitive=init_failed".to_string());
    }

    // S6: Classify session intent to inform pre-hook context budgets.
    // Store CILA level in result_cache so pre_read can adjust budget dynamically.
    if let Some(context_text) = input
        .get("context")
        .or_else(|| input.get("message"))
        .and_then(|v| v.as_str())
    {
        let cila_result = runtime.ctx.classifier.classify(context_text);
        runtime.ctx.result_cache.cache_result(
            "__meta__",
            "__session_cila_level__",
            cila_result.level.to_string(),
        );
        tracing::debug!(
            cila_level = cila_result.level,
            "S6: session CILA level stored for intent-aware context"
        );
    }

    // E19: Compute stable session context — project-level data cached for all hooks.
    // Must run after CILA classification (S6) so we have the CILA level.
    let session_cila: u8 = runtime
        .ctx
        .result_cache
        .get_result("__meta__", "__session_cila_level__")
        .and_then(|v| v.parse().ok())
        .unwrap_or(3);

    let symbol_count = runtime
        .infra
        .symbol_store
        .as_ref()
        .and_then(|store| store.stats().ok())
        .map(|s| s.symbol_count)
        .unwrap_or(0);

    let wiring_summary = query_wiring_summary(&runtime.ctx.knowledge);

    let stable = crate::shared::session_context::StableSessionContext::compute(
        &runtime.ctx.knowledge,
        session_cila,
        symbol_count,
        wiring_summary,
    );
    tracing::debug!(
        cila = stable.cila_level,
        files = stable.file_count,
        symbols = stable.symbol_count,
        lang = ?stable.project_language,
        "E19: stable session context computed"
    );
    *runtime.ctx.stable_session.borrow_mut() = Some(stable);

    let context = format!(
        "Touring Knowledge: {} | session={}",
        parts.join(", "),
        &session_id[..session_id.len().min(8)]
    );

    // ES2 P3 — re-attend HarnessContract on session start so the constitutional
    // digest is stored in `HookRuntime` for X9 LEARN to compare pre vs post
    // (ES2 P4 self-verifying loop). Honest carve-out: cannot force model-layer
    // attention non-eviction (KV-cache / serving layer).
    //
    // BUGFIX (2026-06-05): this block + the ledger init below were previously
    // placed AFTER `emit_context_for_event`, which is `-> !` (calls
    // `std::process::exit(0)`). The process exited before they ran, so the
    // constitutional attestation and cross-agent ledger were dead code. The
    // emit is now the LAST statement so this substrate actually executes.
    let constitution_root = runtime.project_root.join(".claude");
    let contract = HarnessContract::attest(&constitution_root);
    runtime.contract_attestation = Some(contract.clone());
    tracing::info!(
        digest = %contract.digest,
        attested = contract.attested,
        claims = contract.claims.len(),
        "session_start: HarnessContract re-attest (X9 LEARN baseline established)"
    );

    // ES3 P4 — derive ActorId (deterministic) and open the cross-agent
    // outcome ledger. Substrate-only delivery: producers (emit_gate_reward)
    // wire in P4.3, the consumer-side `LedgerConsumer` poll loop is deferred
    // to a followup wave. Honest scope: capability-readiness, not current
    // demand (CAH roadmap §3).
    let _ = open_cross_agent_ledger(runtime, &session_id);

    // Emit context + exit LAST (diverging `-> !`). Everything that mutates
    // `runtime` state for this session must run before this line.
    HookRuntime::emit_context_for_event(&context, Some("SessionStart"));
}

/// Open the cross-agent outcome ledger (ES3 P4) — fail-open helper. Returns
/// `()` because the function is a side-effect initializer (the runtime state
/// is the side effect; no value is propagated). Failures degrade to solo mode
/// (runtime.cross_agent_ledger stays `None`).
fn open_cross_agent_ledger(runtime: &mut HookRuntime, session_id: &str) {
    let actor = ActorId::derive(session_id, "primary");
    runtime.actor_id = Some(actor.clone());
    let ledger_data_dir = runtime.project_root.join(".claude").join("touring");
    let dir_result = std::fs::create_dir_all(&ledger_data_dir);
    if let Err(e) = dir_result {
        tracing::debug!(error = %e, "cross-agent ledger: dir create failed (fail-open)");
        return;
    }
    let open_result = CrossAgentLedger::open(&ledger_data_dir);
    let ledger = match open_result {
        Ok(l) => l,
        Err(e) => {
            tracing::debug!(error = %e, "cross-agent ledger: open failed (fail-open to solo mode)");
            return;
        }
    };
    if let Err(e) = ledger.register_actor(&actor) {
        tracing::debug!(error = %e, "cross-agent ledger: register_actor failed (fail-open)");
        return;
    }
    tracing::info!(
        actor = %actor,
        path = %ledger_data_dir.display(),
        "cross-agent ledger opened (ES3 P4, capability-readiness substrate)"
    );
    runtime.cross_agent_ledger = Some(Arc::new(ledger));
}

/// E19: Query a compact wiring summary from module_ecosystem.
/// Returns `Some("N modules, M orphans")` or `None` if the table is empty/missing.
fn query_wiring_summary(knowledge: &super::knowledge::FileKnowledgeDB) -> Option<String> {
    let conn = knowledge.conn_ref();

    let module_count: i64 = conn
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM {}",
                schema_guard::TABLE_MODULE_ECOSYSTEM
            ),
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    if module_count == 0 {
        return None;
    }

    // Orphans: modules with integration_score < 0.5 (consistent with CLI handlers)
    let orphan_count: i64 = conn
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM {} WHERE integration_score < 0.5",
                schema_guard::TABLE_MODULE_ECOSYSTEM
            ),
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    Some(format!(
        "{module_count} modules, {orphan_count} low-integration"
    ))
}

/// S12: Pre-warm the result cache with context for the most accessed files.
///
/// Called at session-start so that the first pre-read hook invocations
/// find their context already cached (avoiding cold-start latency).
fn prewarm_result_cache(rt: &mut HookRuntime) {
    const MAX_PREWARM: usize = 15;

    // Query most accessed files (excludes internal markers like __session_end__)
    let files = match rt.ctx.knowledge.top_accessed_files(MAX_PREWARM) {
        Ok(f) => f,
        Err(e) => {
            tracing::debug!(error = %e, "prewarm: could not query top files");
            return;
        }
    };

    let mut warmed = 0usize;
    for file_path in &files {
        // Skip if already cached
        if rt
            .ctx
            .result_cache
            .get_result("pre-read", file_path)
            .is_some()
        {
            continue;
        }
        if let Some(ctx) = crate::pre_read::compose_high_signal_context_budgeted(
            &rt.ctx.knowledge,
            file_path,
            crate::pre_read::DEFAULT_CONTEXT_BUDGET,
            0,
        ) {
            rt.ctx.result_cache.cache_result("pre-read", file_path, ctx);
            warmed += 1;
        }
    }

    if warmed > 0 {
        tracing::debug!(warmed_files = warmed, "S12: session-start prewarm complete");
    }
}

/// Run the session-stop hook. Generates final quality report and records session end.
#[tracing::instrument(skip(runtime, input), fields(hook = "session_stop"))]
pub fn run_session_stop(
    runtime: &mut HookRuntime,
    input: &serde_json::Value,
) -> Result<(), touring_hook_runtime::hook_runtime::HookDispatchError> {
    // D9: Validate payload with typed schema — fail fast on malformed input.
    let validated = match validate_payload::<crate::schemas::SessionStopPayload>(input) {
        Ok(v) => v,
        Err(errors) => return Err(format!("session_stop validation failed: {}", errors).into()),
    };
    let session_id = validated
        .session_id
        .unwrap_or_else(|| "unknown".to_string());

    // Record a session end marker in the access log
    let _ = runtime
        .ctx
        .knowledge
        .record_access("__session_end__", &session_id);

    // Generate final quality report if tracking was active
    let quality_summary = if let Some(report) = runtime.quality_report(1) {
        let status_str = match report.status {
            TrackerStatus::Pass => "PASS",
            TrackerStatus::Veto => "VETO",
            TrackerStatus::Halt => "HALT",
        };
        Some(serde_json::json!({
            "status": status_str,
            "composite": report.composite,
            "dimensions": report.dims.len(),
            "iteration": report.iteration,
            "cache_hit_rate": runtime.cache_hit_rate(),
        }))
    } else {
        None
    };

    // W6: Inject session-level RL outcome rewards before printing summary or persisting bandit.
    // Must run before the quality_summary is partially moved by the stats block below.
    inject_session_outcome_rewards(runtime, &quality_summary);

    // Get final stats for summary
    if let Ok(stats) = runtime.ctx.knowledge.stats() {
        let mut summary = serde_json::json!({
            "event": "session_end",
            "session_id": session_id,
            "stats": stats,
        });
        if let Some(quality) = quality_summary {
            // SAFETY: summary is a freshly created json!({}) object
            #[allow(clippy::indexing_slicing)]
            {
                summary["quality_report"] = quality;
            }
        }
        // Output summary to stdout (shown in transcript for Stop hooks)
        println!("{}", serde_json::to_string(&summary).unwrap_or_default());
    }

    // C1: Flush per-session OnceLock caches — prevents stale mtime/pipeline entries
    // across sessions. Must run before gotcha decay so state is clean.
    crate::post_edit::flush_dedup();
    crate::post_write::flush_dedup();
    crate::pre_edit::flush_cache();

    // S8: Update gotcha decay scores at session end
    let _ = runtime.ctx.knowledge.update_gotcha_decay();

    // D5: Persist GoT snapshot for next session warm-start
    save_got_snapshot(runtime, &session_id);

    // R4: Persist bandit state for next session warm-start
    if let Err(e) = runtime.save_bandit() {
        tracing::warn!("Failed to save bandit snapshot: {e}");
    }

    // R18: Persist CRDT graph state at session end (fix cold_start issue)
    if let Err(e) = runtime.save_crdt_graph() {
        tracing::warn!("Failed to save CRDT graph: {e}");
    }

    // S4-S2: Persist AgenticRL state for next session warm-start
    if let Err(e) = runtime.save_agentic_rl() {
        tracing::warn!("Failed to save AgenticRL state: {e}");
    }

    // A-hook-4: Query top-10 accessed files + upsert_session_file_summary
    if let Ok(top_files) = runtime
        .ctx
        .knowledge
        .top_accessed_files_in_session(&session_id, 10)
    {
        for file_path in top_files {
            // Gather file knowledge (shared for purpose + skeleton)
            let fk_opt = runtime.ctx.knowledge.lookup(&file_path).ok().flatten();

            // Purpose: from symbols list or language fallback
            let purpose = fk_opt.as_ref().and_then(|fk| {
                fk.symbols_json
                    .as_ref()
                    .and_then(|s| {
                        serde_json::from_str::<Vec<String>>(s).ok().map(|symbols| {
                            if symbols.is_empty() {
                                fk.language.clone().unwrap_or_default()
                            } else {
                                symbols.join(", ")
                            }
                        })
                    })
                    .or_else(|| fk.language.clone())
            });

            // Gather top 3 gotchas for this file (single call)
            let gotchas = runtime.ctx.knowledge.get_gotchas_for_file(&file_path);
            let top_gotchas: Vec<String> =
                gotchas.iter().take(3).map(|g| g.gotcha.clone()).collect();
            let top_gotchas_json = if top_gotchas.is_empty() {
                None
            } else {
                serde_json::to_string(&top_gotchas).ok()
            };

            // Blast severity from highest hit_count in gotchas
            let blast_severity = gotchas.first().map(|g| {
                if g.hit_count > 10 {
                    "critical"
                } else if g.hit_count > 5 {
                    "high"
                } else if g.hit_count > 2 {
                    "medium"
                } else {
                    "low"
                }
            });

            // Skeleton JSON from file knowledge
            let skeleton_json = fk_opt.and_then(|fk| {
                serde_json::to_string(&serde_json::json!({
                    "language": fk.language,
                    "line_count": fk.line_count,
                    "symbol_count": fk.symbol_count,
                    "read_count": fk.read_count,
                }))
                .ok()
            });

            let _ = runtime.ctx.knowledge.upsert_session_file_summary(
                &file_path,
                &session_id,
                skeleton_json.as_deref(),
                purpose.as_deref(),
                top_gotchas_json.as_deref(),
                blast_severity,
            );
        }
    }

    // ES4 P1: Final flush of the durable action world model at the session boundary,
    // capturing any observations folded since the last debounced persist. Fail-open:
    // no-op until a warm-load configured the snapshot path; never blocks exit-0.
    let _ = crate::gateway::outcome_learner::persist_global_model();

    // EC9: Fire-and-forget async WAL checkpoint at session boundary.
    // Queues a PRAGMA wal_checkpoint(TRUNCATE) on the async DB so that edit/bash
    // records written by EC5/EC6 reach the main DB file before the next session starts.
    // Non-blocking: daemon EC8 handles the final flush on process exit.
    if let Some(adb) = runtime.ctx.async_knowledge.as_ref().cloned() {
        if let Ok(handle) = tokio::runtime::Handle::try_current() {
            drop(handle.spawn(async move {
                let _ = adb.wal_checkpoint().await;
            }));
        }
    }

    // U10: Commit any pending Tantivy writes at session boundary.
    // Ensures symbols upserted during this session are visible to the next session's
    // warmup. Failure is logged and never propagates (exit-0 invariant).
    #[cfg(feature = "tantivy-fts")]
    {
        if let Some(idx) = crate::tantivy_index::global_tantivy() {
            if let Err(e) = idx.commit() {
                tracing::debug!("tantivy session-stop commit failed (non-critical): {e}");
            }
        }
    }

    Ok(())
}

/// R4: Warm-start the bandit from the previous session's snapshot.
///
/// Loads `bandit_snapshot.json` from `.claude/data/`, deserializes, and imports
/// into the current bandit. If the snapshot doesn't exist or is invalid, falls
/// back to cold start (the bandit begins with uniform priors).
fn warm_start_bandit(runtime: &mut HookRuntime) {
    let snapshot_path = runtime
        .project_root
        .join(".claude/data/bandit_snapshot.json");

    if !snapshot_path.exists() {
        tracing::debug!("No bandit snapshot found — cold start");
        return;
    }

    let data = match std::fs::read_to_string(&snapshot_path) {
        Ok(d) => d,
        Err(e) => {
            tracing::warn!("Failed to read bandit snapshot: {e}");
            return;
        }
    };

    let snapshot: touring_intelligence::rl::BanditSnapshot = match serde_json::from_str(&data) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("Failed to deserialize bandit snapshot: {e}");
            return;
        }
    };

    // Initialize the bandit and import the snapshot
    let bandit = runtime.get_bandit();
    match bandit.import_snapshot(&snapshot) {
        Ok(()) => {
            tracing::info!(
                bandit_type = %snapshot.bandit_type,
                total_pulls = snapshot.total_pulls,
                "Warm-started bandit from previous session"
            );
        }
        Err(e) => {
            tracing::warn!("Failed to import bandit snapshot: {e} — cold start");
        }
    }
}

/// D5: Load a previous GoT snapshot from the project's snapshot store.
///
/// Tries to load by the current `session_id` first, then falls back to the
/// most recent snapshot. Stores the loaded snapshot in `runtime.got_snapshot`
/// for use during the session. Failures are logged but never fatal.
fn load_got_snapshot(runtime: &mut HookRuntime, session_id: &str) {
    let db_path = touring_foundation::TouringConfig::graph_db_canonical(&runtime.project_root);

    let store = match crate::got_snapshot_store::GoTSnapshotStore::new(&db_path) {
        Ok(s) => s,
        Err(e) => {
            tracing::debug!("GoT snapshot store unavailable: {e}");
            return;
        }
    };

    // Try exact session match first, then fall back to most recent
    match store.load_by_session(session_id) {
        Ok(Some(snapshot)) => {
            tracing::info!(
                session_id,
                nodes = snapshot.nodes.len(),
                "D5: restored GoT snapshot for session"
            );
            runtime.got_snapshot = Some(snapshot);
            return;
        }
        Ok(None) => {} // no exact match — try latest
        Err(e) => {
            tracing::debug!("D5: failed to load GoT snapshot by session: {e}");
        }
    }

    match store.load_latest() {
        Ok(Some((prev_sid, snapshot))) => {
            tracing::info!(
                prev_session = %prev_sid,
                nodes = snapshot.nodes.len(),
                "D5: restored GoT snapshot from previous session"
            );
            runtime.got_snapshot = Some(snapshot);
        }
        Ok(None) => {
            tracing::debug!("D5: no previous GoT snapshot found");
        }
        Err(e) => {
            tracing::debug!("D5: failed to load latest GoT snapshot: {e}");
        }
    }
}

/// D5: Save the current GoT snapshot to the project's snapshot store.
///
/// Only saves if `runtime.got_snapshot` is `Some`. Failures are logged
/// but never fatal (exit 0 invariant).
fn save_got_snapshot(runtime: &HookRuntime, session_id: &str) {
    let snapshot = match &runtime.got_snapshot {
        Some(s) => s,
        None => return,
    };

    let db_path = touring_foundation::TouringConfig::graph_db_canonical(&runtime.project_root);

    let store = match crate::got_snapshot_store::GoTSnapshotStore::new(&db_path) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("D5: cannot open GoT snapshot store for save: {e}");
            return;
        }
    };

    match store.save(session_id, snapshot) {
        Ok(()) => {
            tracing::info!(
                session_id,
                nodes = snapshot.nodes.len(),
                "D5: saved GoT snapshot"
            );
        }
        Err(e) => {
            tracing::warn!("D5: failed to save GoT snapshot: {e}");
        }
    }
}

/// W6: Inject session-level RL outcome rewards at session stop.
///
/// Feeds two coarse signals into the LinUCB bandit arm pool:
/// - `"session"` arm: the session quality composite score (0.0–1.0)
/// - `"session_productivity"` arm: edit/bash operation ratio (1.0 = balanced, 0.4 = bash-heavy)
///
/// These are injected **before** `save_bandit()` so the warm-start snapshot
/// for the next session already includes the outcome signal.
fn inject_session_outcome_rewards(
    runtime: &mut HookRuntime,
    quality_summary: &Option<serde_json::Value>,
) {
    // Signal 1: session quality composite → reward on "session" arm
    if let Some(composite) = quality_summary
        .as_ref()
        .and_then(|qs| qs.get("composite"))
        .and_then(|v| v.as_f64())
    {
        runtime
            .learning
            .inject_reward("session", composite, "session_quality_composite");
        tracing::debug!(composite = composite, "W6: session quality reward injected");
    }

    // Signal 2: edit productivity ratio → reward on "session_productivity" arm
    if let Ok(stats) = runtime.ctx.knowledge.stats() {
        let total_ops = (stats.edit_count + stats.bash_count) as f64;
        if total_ops > 0.0 {
            let edit_ratio = stats.edit_count as f64 / total_ops;
            // Balanced mix (0.2–0.7 edits) is healthiest; bash-heavy sessions score lower.
            let reward = if edit_ratio > 0.2 && edit_ratio <= 0.7 {
                1.0
            } else if edit_ratio > 0.7 {
                0.6 // mostly editing, minimal exploration
            } else {
                0.4 // mostly bash, little direct editing
            };
            runtime
                .learning
                .inject_reward("session_productivity", reward, "edit_bash_ratio");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn setup() -> (TempDir, HookRuntime) {
        let tmp = TempDir::new().unwrap();
        let root = tmp.path().to_path_buf();
        std::fs::create_dir_all(root.join(".claude/data")).unwrap();
        let rt = HookRuntime::new(&root).unwrap();
        (tmp, rt)
    }

    #[test]
    fn test_session_stop_records_end() {
        let (_tmp, mut rt) = setup();
        let input = serde_json::json!({"session_id": "test-123"});
        let result = run_session_stop(&mut rt, &input);
        assert!(result.is_ok());

        // Verify access was logged
        let count = rt.ctx.knowledge.access_count("__session_end__").unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_session_stop_with_quality_report() {
        let (_tmp, mut rt) = setup();

        // Initialize tracking and record some outcomes
        rt.reset_quality_tracking("quality-test-session");
        rt.record_hook_outcome(super::super::aco_bridge::HookOutcome {
            hook_name: "pre_read".into(),
            success: true,
            latency_ms: 5,
            context_injected: true,
            knowledge_captured: false,
            error: None,
        });
        rt.record_hook_outcome(super::super::aco_bridge::HookOutcome {
            hook_name: "post_read".into(),
            success: true,
            latency_ms: 10,
            context_injected: false,
            knowledge_captured: true,
            error: None,
        });

        let input = serde_json::json!({"session_id": "quality-test-session"});
        let result = run_session_stop(&mut rt, &input);
        assert!(result.is_ok());

        // Verify quality report is available
        let report = rt.quality_report(1).unwrap();
        assert_eq!(report.dims.len(), 9);
        assert!(report.composite > 0.0);
    }

    #[test]
    fn test_top_accessed_files_excludes_internal() {
        let (_tmp, rt) = setup();

        // Record real file accesses and internal markers
        rt.ctx.knowledge.record_access("src/main.rs", "s1").unwrap();
        rt.ctx.knowledge.record_access("src/main.rs", "s1").unwrap();
        rt.ctx.knowledge.record_access("src/lib.rs", "s1").unwrap();
        rt.ctx
            .knowledge
            .record_access("__session_end__", "s1")
            .unwrap();
        rt.ctx
            .knowledge
            .record_access("__subagent_stop__", "s1")
            .unwrap();

        let top = rt.ctx.knowledge.top_accessed_files(10).unwrap();

        // Internal markers (double underscore) must be excluded
        assert!(!top.iter().any(|f| f.starts_with("__")));
        // Real files must be present, ordered by frequency
        assert_eq!(top.len(), 2);
        assert_eq!(top[0], "src/main.rs"); // 2 accesses
        assert_eq!(top[1], "src/lib.rs"); // 1 access
    }

    #[test]
    fn test_top_accessed_files_respects_limit() {
        let (_tmp, rt) = setup();

        for i in 0..20 {
            rt.ctx
                .knowledge
                .record_access(&format!("file_{i}.rs"), "s1")
                .unwrap();
        }

        let top = rt.ctx.knowledge.top_accessed_files(5).unwrap();
        assert_eq!(top.len(), 5);
    }

    #[test]
    fn test_prewarm_result_cache_populates_cache() {
        let (_tmp, mut rt) = setup();

        // Record some file accesses so top_accessed_files returns data
        rt.ctx.knowledge.record_access("src/main.rs", "s1").unwrap();
        rt.ctx.knowledge.record_access("src/lib.rs", "s1").unwrap();

        // Cache should be empty initially
        assert!(
            rt.ctx
                .result_cache
                .get_result("pre-read", "src/main.rs")
                .is_none()
        );

        // Run prewarm
        prewarm_result_cache(&mut rt);

        // Prewarm runs compose_high_signal_context_budgeted for each file.
        // For files with no gotchas/outcomes/relations, it returns None and
        // nothing is cached — this is correct behavior (no false positives).
        // The test validates that the function runs without error.
    }

    #[test]
    fn test_prewarm_skips_already_cached() {
        let (_tmp, mut rt) = setup();

        // Record access and pre-populate cache
        rt.ctx.knowledge.record_access("src/main.rs", "s1").unwrap();
        rt.ctx
            .result_cache
            .cache_result("pre-read", "src/main.rs", "existing-context".to_string());

        prewarm_result_cache(&mut rt);

        // Existing cache entry must NOT be overwritten
        let cached = rt
            .ctx
            .result_cache
            .get_result("pre-read", "src/main.rs")
            .unwrap();
        assert_eq!(cached, "existing-context");
    }

    // ── D5: GoT Snapshot session integration tests ───────────────────────

    fn build_test_snapshot(session_id: &str) -> touring_intelligence::reasoning::GoTSnapshot {
        use touring_intelligence::reasoning::got::{GotEngine, GotNode};
        let mut engine = GotEngine::new(3);
        engine.add_node(GotNode::new(1, "analyze", 1.0));
        engine.add_node(GotNode::new(2, "synthesize", 0.8));
        engine.add_edge(1, 2);
        touring_intelligence::reasoning::GoTSnapshot::from_engine(&engine, session_id)
    }

    #[test]
    fn test_load_got_snapshot_no_store() {
        // When no snapshot DB exists, load_got_snapshot should be a no-op
        let (_tmp, mut rt) = setup();
        assert!(rt.got_snapshot.is_none());

        load_got_snapshot(&mut rt, "test-session");

        // No DB file, no snapshot — should remain None
        assert!(rt.got_snapshot.is_none());
    }

    #[test]
    fn test_save_and_load_got_snapshot_roundtrip() {
        let (_tmp, mut rt) = setup();
        let session_id = "roundtrip-test";

        // Set a snapshot on the runtime
        rt.got_snapshot = Some(build_test_snapshot(session_id));

        // Save it
        save_got_snapshot(&rt, session_id);

        // Verify the DB file was created
        let db_path = touring_foundation::TouringConfig::graph_db_canonical(&rt.project_root);
        assert!(db_path.exists(), "snapshot DB should be created on save");

        // Clear the runtime snapshot and reload
        rt.got_snapshot = None;
        load_got_snapshot(&mut rt, session_id);

        let loaded = rt.got_snapshot.as_ref().expect("snapshot should be loaded");
        assert_eq!(loaded.session_id, session_id);
        assert_eq!(loaded.nodes.len(), 2);
        assert_eq!(loaded.max_depth, 3);
    }

    #[test]
    fn test_load_got_snapshot_falls_back_to_latest() {
        let (_tmp, mut rt) = setup();

        // Save a snapshot under a different session ID
        rt.got_snapshot = Some(build_test_snapshot("old-session"));
        save_got_snapshot(&rt, "old-session");

        // Clear and try to load under a new session ID (no exact match)
        rt.got_snapshot = None;
        load_got_snapshot(&mut rt, "new-session");

        // Should fall back to the latest snapshot ("old-session")
        let loaded = rt
            .got_snapshot
            .as_ref()
            .expect("should fall back to latest");
        assert_eq!(loaded.session_id, "old-session");
        assert_eq!(loaded.nodes.len(), 2);
    }

    #[test]
    fn test_save_got_snapshot_noop_when_none() {
        let (_tmp, mut rt) = setup();

        // No snapshot set — save should be a no-op (nothing written to the store)
        save_got_snapshot(&rt, "test-session");

        // Verify the no-op: loading back should yield nothing
        load_got_snapshot(&mut rt, "test-session");
        assert!(
            rt.got_snapshot.is_none(),
            "no snapshot should be loadable when none was saved"
        );
    }

    #[test]
    fn test_session_start_loads_got_snapshot() {
        let (_tmp, mut rt) = setup();

        // Pre-seed a snapshot in the store
        let db_path = touring_foundation::TouringConfig::graph_db_canonical(&rt.project_root);
        let store =
            crate::got_snapshot_store::GoTSnapshotStore::new(&db_path).expect("store creation");
        store
            .save("sess-abc", &build_test_snapshot("sess-abc"))
            .expect("save");

        // Run session start — should load the snapshot
        let input = serde_json::json!({"session_id": "sess-abc"});
        let result = run_session_start(&mut rt, &input);
        assert!(result.is_ok());

        let loaded = rt
            .got_snapshot
            .as_ref()
            .expect("snapshot should be loaded by session-start");
        assert_eq!(loaded.session_id, "sess-abc");
    }
}
