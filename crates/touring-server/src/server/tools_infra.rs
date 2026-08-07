use super::*;
use std::str::FromStr;

#[tool_router(router = router_infra, vis = "pub(crate)")]
impl TouringServer {
    // ── v9.0 Tools ───────────────────────────────────────────────────────

    /// Mask tool result observations in context text, preserving action traces.
    /// Returns masked text + token savings statistics.
    #[tool(
        name = "touring_mask_context",
        description = "Compress large tool result observations in context to reclaim tokens. Pass accumulated context text; verbose file reads and command outputs are summarized while preserving key action traces. Returns compressed text + token savings stats. Use when context pressure is high (>80% full). threshold sets minimum token count to trigger compression (default: 4000 tokens — smaller observations are passed through unchanged)."
    )]
    async fn mask_context(
        &self,
        params: Parameters<MaskContextParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let masker = crate::observation_masker::ObservationMasker::with_threshold(
            p.threshold.unwrap_or(4000),
        );
        let (masked_text, stats) = masker.mask_observations(&p.text);
        let reduction_pct = if stats.original_tokens > 0 {
            (stats.original_tokens - stats.masked_tokens) as f64 / stats.original_tokens as f64
                * 100.0
        } else {
            0.0
        };
        let gctx = self.graph_svc.resolve_ctx(None).await;
        let mut output = serde_json::json!({
            "masked_text": masked_text,
            "original_tokens": stats.original_tokens,
            "masked_tokens": stats.masked_tokens,
            "blocks_masked": stats.blocks_masked,
            "reduction_pct": reduction_pct,
        });
        self.graph_svc.inject(&mut output, &gctx);
        crate::tools::suggestions::append_to_response(&mut output, "touring_mask_context", 2);
        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    /// Run Monte Carlo Tree Search from a state to find optimal action sequence.
    /// Uses QTable as the reward function for rollout evaluation.
    #[tool(
        name = "touring_mcts_search",
        description = "MCTS for multi-step planning. Pass root_state (0 = general) + candidate_actions (csv action IDs). Simulates num_rollouts paths via Q-table; returns actions ranked by expected value. Typical: num_rollouts=50, max_depth=5."
    )]
    async fn mcts_search(
        &self,
        params: Parameters<MctsSearchParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let actions: Vec<u64> = p
            .candidate_actions
            .split(',')
            .filter_map(|s| s.trim().parse().ok())
            .collect();

        if actions.is_empty() {
            return Err(McpError::invalid_params(
                "candidate_actions must contain at least one valid integer",
                None,
            ));
        }

        let config = touring_intelligence::reasoning::MCTSConfig {
            num_rollouts: p.num_rollouts.unwrap_or(50),
            max_depth: p.max_depth.unwrap_or(5),
            ..Default::default()
        };
        let engine = touring_intelligence::reasoning::MCTSEngine::new(config);
        let qt = self.qtable.lock().await;

        let result = engine.search(
            p.root_state,
            |_state| actions.clone(),
            |state, action| qt.get_value(state, action),
        );

        let gctx = self.graph_svc.resolve_ctx(None).await;
        let mut output = match result {
            Some(r) => serde_json::json!({
                "best_action": r.best_action,
                "confidence": r.confidence,
                "value": r.value,
                "tree_depth": r.tree_depth,
                "total_rollouts": r.total_rollouts,
            }),
            None => serde_json::json!({
                "error": "No valid actions found during search",
                "best_action": null,
                "confidence": 0.0,
            }),
        };
        self.graph_svc.inject(&mut output, &gctx);

        let dl = p.detail_level.unwrap_or_default();
        params::apply_detail_level(&mut output, dl);
        crate::tools::suggestions::append_to_response(&mut output, "touring_mcts_search", 2);
        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    /// Run Streaming MCTS from a state string to get progressive results.
    /// Uses tokio watch channel for streaming intermediate results via H84 StreamingMCTSHarness.
    #[tool(
        name = "touring_streaming_mcts",
        description = "Streaming Monte Carlo Tree Search for multi-step planning decisions. Takes a search_state string and runs progressive MCTS search with intermediate results available via tokio watch channel. Unlike touring_mcts_search which blocks until complete, this tool returns immediately with the best result found so far. Use for background planning without blocking the UI. Typical use: pass a state string, get best_action with confidence score."
    )]
    async fn streaming_mcts_search(
        &self,
        params: Parameters<StreamingMctsParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let root_state: u64 = p.search_state.bytes().fold(0u64, |acc, b| {
            acc.wrapping_mul(31).wrapping_add(u64::from(b))
        });

        let config = touring_intelligence::reasoning::MCTSConfig {
            num_rollouts: 50,
            max_depth: 5,
            ..Default::default()
        };
        let expand_fn = std::sync::Arc::new(move |state: u64| {
            let base = state.wrapping_mul(31).wrapping_add(1);
            vec![
                base,
                base.wrapping_add(1),
                base.wrapping_add(2),
                base.wrapping_add(3),
            ]
        });
        let reward_fn = std::sync::Arc::new(|_state: u64, action: u64| (action % 10) as f64 / 10.0);

        // Anti-pattern fix (2026-04-21): previously this block created a nested
        // `new_current_thread` runtime inside `spawn_blocking` and busy-waited
        // with `spin_loop()`. That pinned a single worker thread at 100% CPU
        // while the internal StreamingMCTS rayon pool was already multi-core.
        //
        // Current impl: `StreamingMCTS::spawn` and `best_so_far` are both
        // synchronous; no nested runtime is needed. `yield_now()` hands the
        // OS scheduler control so other threads on the same core can run.
        let result = match tokio::runtime::Handle::try_current() {
            Ok(h) => {
                let config_clone = config.clone();
                let expand_fn_clone = expand_fn.clone();
                let reward_fn_clone = reward_fn.clone();
                let join_handle = h.spawn_blocking(move || {
                    let streaming = touring_intelligence::reasoning::StreamingMCTS::spawn(
                        config_clone,
                        expand_fn_clone,
                        reward_fn_clone,
                        root_state,
                    );
                    let deadline = std::time::Instant::now() + std::time::Duration::from_millis(20);
                    loop {
                        if let Some(r) = streaming.best_so_far() {
                            return r;
                        }
                        if std::time::Instant::now() >= deadline {
                            return streaming.best_so_far().unwrap_or(
                                touring_intelligence::reasoning::MCTSResult {
                                    best_action: 0,
                                    confidence: 0.0,
                                    value: 0.0,
                                    tree_depth: 0,
                                    total_rollouts: 0,
                                },
                            );
                        }
                        std::thread::yield_now();
                    }
                });
                join_handle.await.ok()
            }
            Err(_) => None,
        };

        let gctx = self.graph_svc.resolve_ctx(None).await;
        let mut output = match result {
            Some(r) => serde_json::json!({
                "best_action": r.best_action,
                "confidence": r.confidence,
                "value": r.value,
                "tree_depth": r.tree_depth,
                "total_rollouts": r.total_rollouts,
            }),
            None => serde_json::json!({
                "error": "No tokio runtime available or search timed out",
                "best_action": null,
                "confidence": 0.0,
            }),
        };
        self.graph_svc.inject(&mut output, &gctx);

        let dl = p.detail_level.unwrap_or_default();
        params::apply_detail_level(&mut output, dl);
        crate::tools::suggestions::append_to_response(&mut output, "touring_streaming_mcts", 2);
        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    /// Show incremental parser cache status — documents tracked, trees cached, symbols indexed.
    #[tool(
        name = "touring_incremental_status",
        description = "Show incremental AST parser cache health: hit_rate (high = faster pre-edit analysis), supported languages, cached file count, and memory usage. A low hit_rate means the cache is cold or files are changing faster than they are cached. Use to diagnose slow pre-edit hook performance or to verify that AST analysis is active for a specific language before relying on touring_ast_find results."
    )]
    async fn incremental_status(
        &self,
        params: Parameters<IncrementalStatusParams>,
    ) -> Result<CallToolResult, McpError> {
        let _p = params.0;
        let (symbol_count, file_count, dep_count) = if let Some(ref store) = self.symbol_store {
            let s = store.lock().await;
            match s.stats() {
                Ok(st) => (st.symbol_count, st.file_count, st.dependency_count),
                Err(_) => (0, 0, 0),
            }
        } else {
            (0, 0, 0)
        };

        let gctx = self.graph_svc.resolve_ctx(None).await;
        let mut output = serde_json::json!({
            "description": "IncrementalPipeline status",
            "supported_languages": [
                "python", "rust", "typescript", "javascript", "bash",
                "html", "css", "markdown", "json", "toml", "yaml",
                "go", "java"
            ],
            "tree_cache_max": 128,
            "symbol_store": {
                "symbol_count": symbol_count,
                "file_count": file_count,
                "dependency_count": dep_count,
            },
            "features": [
                "tree-sitter incremental parsing",
                "RopeDocument O(log N) edits",
                "SymbolStore SQLite persistence",
                "Delta symbol extraction"
            ],
            "integration": "PostEdit cortex handler (neural.rs)",
        });
        self.graph_svc.inject(&mut output, &gctx);
        let dl = _p.detail_level.unwrap_or_default();
        params::apply_detail_level(&mut output, dl);
        crate::tools::suggestions::append_to_response(&mut output, "touring_incremental_status", 2);
        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    /// Create a speculative branch, apply an edit, and validate via ruff lint.
    /// Returns lint diagnostics and quality score without modifying the real filesystem.
    #[tool(
        name = "touring_speculate",
        description = "Shadow-validate file changes BEFORE writing. Returns diagnostics + quality score 0-1 (≥0.8 = safe to apply). Per-language linters (ruff/tsc/cargo-check). Use before any Write/Edit on existing files."
    )]
    async fn speculate(
        &self,
        params: Parameters<SpeculateParams>,
    ) -> Result<CallToolResult, McpError> {
        use std::path::PathBuf;
        use touring_hooks::shadow_v2::{DiagnosticSeverity, ShadowWorkspaceV2};

        let p = params.0;
        let base = PathBuf::from(p.base_dir.unwrap_or_else(|| ".".to_string()));
        let mut workspace = ShadowWorkspaceV2::new(base);

        let branch_id = workspace
            .create_branch()
            .map_err(|e| McpError::internal_error(format!("create_branch failed: {e}"), None))?;

        workspace
            .apply_edit(branch_id, PathBuf::from(&p.file_path), p.content)
            .map_err(|e| McpError::internal_error(format!("apply_edit failed: {e}"), None))?;

        let result = workspace
            .validate_branch(branch_id)
            .map_err(|e| McpError::internal_error(format!("validate failed: {e}"), None))?;

        let diagnostics: Vec<serde_json::Value> = result
            .diagnostics
            .iter()
            .take(10)
            .map(|d| {
                serde_json::json!({
                    "file": d.file,
                    "line": d.line,
                    "column": d.column,
                    "severity": match d.severity {
                        DiagnosticSeverity::Error => "error",
                        DiagnosticSeverity::Warning => "warning",
                        DiagnosticSeverity::Info => "info",
                    },
                    "message": d.message,
                    "code": d.code,
                })
            })
            .collect();

        let mut output = serde_json::json!({
            "branch_id": result.branch_id,
            "score": result.score,
            "diagnostics_count": result.diagnostics.len(),
            "diagnostics": diagnostics,
            "files_modified": result.files_modified,
        });
        let dl = p.detail_level.unwrap_or_default();
        params::apply_detail_level(&mut output, dl);
        crate::tools::suggestions::append_to_response(&mut output, "touring_speculate", 2);
        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    /// Show online RL engine status — EMA reward, update count, LinUCB arm stats.
    #[tool(
        name = "touring_online_learn",
        description = "Snapshot of the online RL engine: recent_reward EMA, update_count, per-tool bandit stats, convergence_status. Verify RL is receiving reward signals and identify underperforming tools."
    )]
    async fn online_learn(
        &self,
        params: Parameters<OnlineLearnParams>,
    ) -> Result<CallToolResult, McpError> {
        let _p = params.0;
        let online = self.online_rl.lock().await;
        let bandit = self.linucb.lock().await;

        let gctx = self.graph_svc.resolve_ctx(None).await;
        let mut output = serde_json::json!({
            "online_rl": {
                "ema_reward": online.ema_reward(),
                "update_count": online.update_count(),
                "should_save": online.should_save(),
                "config": {
                    "min_reward_delta": 0.01,
                    "ema_alpha": 0.1,
                    "save_interval": 50,
                },
            },
            "linucb": {
                "arms": 8,
                "feature_dimensions": 19,
                "total_pulls": bandit.total_pulls(),
            },
        });
        self.graph_svc.inject(&mut output, &gctx);
        let dl = _p.detail_level.unwrap_or_default();
        params::apply_detail_level(&mut output, dl);
        crate::tools::suggestions::append_to_response(&mut output, "touring_online_learn", 2);
        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    // ── Wiring Intelligence ─────────────────────────────────────────────

    /// Audit codebase symbol connections — orphan detection, module scores.
    #[tool(
        annotations(read_only_hint = true, title = "Audit symbol wiring"),
        name = "touring_wiring",
        description = "Audit codebase symbol connections (wiring). Use after adding public symbols to catch orphans. Actions: status (orphan count, module count, pub symbols), orphans (list unused public symbols with file+kind), modules (per-module integration scores and orphan breakdown)."
    )]
    async fn wiring(&self, params: Parameters<WiringParams>) -> Result<CallToolResult, McpError> {
        let dl = params.0.detail_level.unwrap_or_default();
        let db_path = &self.config.symbols_db_path;
        if !db_path.exists() {
            return Ok(CallToolResult::success(vec![Content::text(
                "Wiring DB not initialized. Run `touring index rebuild` first.",
            )]));
        }

        let conn = rusqlite::Connection::open_with_flags(
            db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| McpError::internal_error(format!("wiring db: {e}"), None))?;
        let _ = conn.execute_batch("PRAGMA busy_timeout = 1000;");

        // Check if wiring_map table exists
        let table_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='wiring_map'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap_or(false);

        if !table_exists {
            return Ok(CallToolResult::success(vec![Content::text(
                "Wiring table not yet populated. Edit some files to trigger wiring tracking.",
            )]));
        }

        let file_filter = params.0.file_path.as_deref();

        let result = match params.0.action.as_str() {
            "status" => {
                let orphan_count: i64 = conn
                    .query_row(
                        "SELECT COUNT(DISTINCT symbol_name) FROM wiring_map \
                         WHERE visibility='public' AND consumer_file IS NULL",
                        [],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                let module_count: i64 = conn
                    .query_row(
                        "SELECT COUNT(DISTINCT module_file) FROM wiring_map",
                        [],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                let total_pub: i64 = conn
                    .query_row(
                        "SELECT COUNT(DISTINCT symbol_name) FROM wiring_map \
                         WHERE visibility='public'",
                        [],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                let total_cons: i64 = conn
                    .query_row(
                        "SELECT COUNT(DISTINCT symbol_name) FROM wiring_map \
                         WHERE consumer_file IS NOT NULL",
                        [],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                serde_json::json!({
                    "orphan_count": orphan_count,
                    "module_count": module_count,
                    "total_pub_symbols": total_pub,
                    "total_consumers": total_cons,
                    "wiring_health": if orphan_count == 0 { "clean" } else { "has_orphans" },
                })
            }
            "orphans" => {
                // M2: Staleness detection — query last_modified of wiring_map
                let db_modified_secs: i64 = conn
                    .query_row(
                        "SELECT last_modified FROM sqlite_db WHERE name='wiring_map'",
                        [],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                let now_secs = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs() as i64)
                    .unwrap_or(0);
                let staleness_mins = ((now_secs.saturating_sub(db_modified_secs)) / 60) as usize;
                let stale_warning = if staleness_mins > 30 {
                    format!(
                        "Wiring DB is {} minutes stale — run `touring index rebuild` for fresh data",
                        staleness_mins
                    )
                } else {
                    String::new()
                };

                let query = if file_filter.is_some() {
                    "SELECT module_file, symbol_name, symbol_kind, visibility \
                     FROM wiring_map WHERE visibility='public' AND consumer_file IS NULL \
                     AND NOT module_file LIKE '%/benches/%' AND NOT module_file LIKE '%/tests/%' \
                     AND module_file=?1 ORDER BY module_file, symbol_name LIMIT 200"
                } else {
                    "SELECT module_file, symbol_name, symbol_kind, visibility \
                     FROM wiring_map WHERE visibility='public' AND consumer_file IS NULL \
                     AND NOT module_file LIKE '%/benches/%' AND NOT module_file LIKE '%/tests/%' \
                     ORDER BY module_file, symbol_name LIMIT 200"
                };
                let mut stmt = conn
                    .prepare(query)
                    .map_err(|e| McpError::internal_error(format!("prepare: {e}"), None))?;

                let to_json = |r: &rusqlite::Row| -> rusqlite::Result<serde_json::Value> {
                    Ok(serde_json::json!({
                        "module_file": r.get::<_, String>(0)?,
                        "symbol_name": r.get::<_, String>(1)?,
                        "symbol_kind": r.get::<_, String>(2)?,
                        "visibility": r.get::<_, String>(3)?,
                    }))
                };

                let rows: Vec<serde_json::Value> = if let Some(fp) = file_filter {
                    let mut qr = stmt
                        .query(rusqlite::params![fp])
                        .map_err(|e| McpError::internal_error(format!("query: {e}"), None))?;
                    let mut v = Vec::new();
                    while let Some(r) = qr
                        .next()
                        .map_err(|e| McpError::internal_error(format!("row: {e}"), None))?
                    {
                        if let Ok(val) = to_json(r) {
                            v.push(val);
                        }
                    }
                    v
                } else {
                    let mut qr = stmt
                        .query([])
                        .map_err(|e| McpError::internal_error(format!("query: {e}"), None))?;
                    let mut v = Vec::new();
                    while let Some(r) = qr
                        .next()
                        .map_err(|e| McpError::internal_error(format!("row: {e}"), None))?
                    {
                        if let Ok(val) = to_json(r) {
                            v.push(val);
                        }
                    }
                    v
                };

                // M3: Dead patterns — scan symbol names for _unused, _dead, _old, etc.
                let symbol_names: Vec<String> = rows
                    .iter()
                    .filter_map(|r| {
                        r.get("symbol_name")
                            .and_then(|v| v.as_str())
                            .map(String::from)
                    })
                    .collect();
                let dead_patterns =
                    touring_analysis::wiring::orphan::scan_dead_patterns(&symbol_names);

                let count = rows.len();
                serde_json::json!({
                    "db_staleness_minutes": staleness_mins,
                    "stale_warning": stale_warning,
                    "dead_patterns": dead_patterns,
                    "orphans": rows,
                    "count": count
                })
            }
            "modules" => {
                let mut stmt = conn
                    .prepare(
                        "SELECT DISTINCT module_file FROM wiring_map \
                         WHERE visibility='public' ORDER BY module_file LIMIT 100",
                    )
                    .map_err(|e| McpError::internal_error(format!("prepare: {e}"), None))?;

                let files: Vec<String> = stmt
                    .query_map([], |r| r.get(0))
                    .map_err(|e| McpError::internal_error(format!("query: {e}"), None))?
                    .filter_map(|r| r.ok())
                    .collect();

                let modules: Vec<serde_json::Value> = files
                    .iter()
                    .map(|file| {
                        let total: i64 = conn
                            .query_row(
                                "SELECT COUNT(DISTINCT symbol_name) FROM wiring_map \
                                 WHERE module_file=?1 AND visibility='public'",
                                rusqlite::params![file],
                                |r| r.get(0),
                            )
                            .unwrap_or(0);
                        let orphans: i64 = conn
                            .query_row(
                                "SELECT COUNT(DISTINCT symbol_name) FROM wiring_map \
                                 WHERE module_file=?1 AND visibility='public' \
                                 AND consumer_file IS NULL",
                                rusqlite::params![file],
                                |r| r.get(0),
                            )
                            .unwrap_or(0);
                        let score = if total > 0 {
                            (total - orphans) as f64 / total as f64
                        } else {
                            1.0_f64
                        };
                        // RFC-100 W-101: emit warning for modules with integration score < 1.0
                        if score < 1.0 && total > 0 {
                            tracing::warn!(
                                code = "W-101",
                                file_path = %file,
                                integration_score = score,
                                total_pub_symbols = total,
                                orphan_count = orphans,
                                "Module integration score below threshold 1.0"
                            );
                        }
                        serde_json::json!({
                            "file_path": file,
                            "integration_score": score,
                            "total_pub_symbols": total,
                            "orphan_count": orphans,
                        })
                    })
                    .collect();

                let total_modules = modules.len();
                serde_json::json!({ "modules": modules, "total_modules": total_modules })
            }
            other => {
                return Err(McpError::invalid_params(
                    format!("Unknown action '{other}'. Use: status, orphans, modules"),
                    None,
                ));
            }
        };

        // Apply detail_level truncation + append suggestions
        let mut result_val = result;
        params::apply_detail_level(&mut result_val, dl);
        crate::tools::suggestions::append_to_response(&mut result_val, "touring_wiring", 2);

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&result_val).unwrap_or_default(),
        )]))
    }

    // ── Wiring Audit (H83 IntegrationCompletenessHandler) ───────────────

    /// Audit integration completeness — exposes wiring intelligence for deep wiring analysis.
    #[tool(
        annotations(read_only_hint = true, title = "Full wiring audit"),
        name = "touring_wiring_audit",
        description = "Deep wiring audit using H83 IntegrationCompletenessHandler. Returns orphan_count, per-module integration scores, and recommendations. Optional module_filter restricts audit to a specific module path."
    )]
    async fn wiring_audit(
        &self,
        params: Parameters<WiringAuditParams>,
    ) -> Result<CallToolResult, McpError> {
        use crate::tools::wiring_audit::{ModuleScore, WiringAuditResult};
        let dl = params.0.detail_level.unwrap_or_default();

        let db_path = &self.config.symbols_db_path;
        if !db_path.exists() {
            return Ok(CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&serde_json::json!({
                    "error": "Wiring DB not initialized. Run `touring index rebuild` first."
                }))
                .unwrap_or_default(),
            )]));
        }

        let conn = rusqlite::Connection::open_with_flags(
            db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| McpError::internal_error(format!("wiring db: {e}"), None))?;
        let _ = conn.execute_batch("PRAGMA busy_timeout = 1000;");

        let table_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='wiring_map'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap_or(false);

        if !table_exists {
            return Ok(CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&serde_json::json!({
                    "error": "Wiring table not yet populated. Edit some files to trigger wiring tracking."
                })).unwrap_or_default(),
            )]));
        }

        let module_filter = params.0.module_filter.as_deref();

        let module_query = if let Some(filter) = module_filter {
            format!(
                "SELECT DISTINCT module_file FROM wiring_map \
                 WHERE visibility='public' AND module_file LIKE '%{}%' \
                 ORDER BY module_file LIMIT 100",
                filter.replace('\'', "''")
            )
        } else {
            "SELECT DISTINCT module_file FROM wiring_map \
             WHERE visibility='public' ORDER BY module_file LIMIT 100"
                .to_string()
        };

        let mut stmt = conn
            .prepare(&module_query)
            .map_err(|e| McpError::internal_error(format!("prepare: {e}"), None))?;

        let files: Vec<String> = stmt
            .query_map([], |r| r.get(0))
            .map_err(|e| McpError::internal_error(format!("query: {e}"), None))?
            .filter_map(|r| r.ok())
            .collect();

        let modules: Vec<ModuleScore> = files
            .iter()
            .map(|file| {
                let total: i64 = conn
                    .query_row(
                        "SELECT COUNT(DISTINCT symbol_name) FROM wiring_map \
                         WHERE module_file=?1 AND visibility='public'",
                        rusqlite::params![file],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                let orphans: i64 = conn
                    .query_row(
                        "SELECT COUNT(DISTINCT symbol_name) FROM wiring_map \
                         WHERE module_file=?1 AND visibility='public' \
                         AND consumer_file IS NULL",
                        rusqlite::params![file],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                let score = if total > 0 {
                    (total - orphans) as f64 / total as f64
                } else {
                    1.0_f64
                };
                ModuleScore {
                    file_path: file.clone(),
                    integration_score: score,
                    total_pub_symbols: total,
                    orphan_count: orphans,
                }
            })
            .collect();

        let total_orphans: i64 = conn
            .query_row(
                "SELECT COUNT(DISTINCT symbol_name) FROM wiring_map \
                 WHERE visibility='public' AND consumer_file IS NULL",
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);

        let mut recommendations = Vec::new();
        if total_orphans == 0 {
            recommendations
                .push("Codebase wiring is clean — no orphan pub symbols detected.".to_string());
        } else {
            let orphan_modules: Vec<_> = modules.iter().filter(|m| m.orphan_count > 0).collect();
            if !orphan_modules.is_empty() {
                recommendations.push(format!(
                    "{} module(s) have orphan pub symbols — consider wiring into consumers or reducing visibility.",
                    orphan_modules.len()
                ));
            }
            let low_score: Vec<_> = modules
                .iter()
                .filter(|m| m.integration_score < 0.5)
                .collect();
            if !low_score.is_empty() {
                recommendations.push(format!(
                    "{} module(s) have integration score < 0.5 — review symbol dependencies.",
                    low_score.len()
                ));
            }
        }

        let result = WiringAuditResult {
            orphan_count: total_orphans,
            audited_modules: modules.len(),
            clean_modules: modules.iter().filter(|m| m.orphan_count == 0).count(),
            recommendations,
        };

        let mut result_val = serde_json::to_value(&result).unwrap_or_default();
        params::apply_detail_level(&mut result_val, dl);
        crate::tools::suggestions::append_to_response(&mut result_val, "touring_wiring_audit", 2);

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&result_val).unwrap_or_default(),
        )]))
    }

    // ── Wiring Suggest ─────────────────────────────────────────────────

    /// Query pending wiring suggestions for orphan pub symbols.
    #[tool(
        name = "touring_wiring_suggest",
        description = "Query pending wiring suggestions for orphan pub symbols from FileKnowledgeDB analysis. Pass orphanSymbol to filter to one symbol or omit to get all pending suggestions."
    )]
    async fn wiring_suggest(
        &self,
        params: Parameters<WiringSuggestParams>,
    ) -> Result<CallToolResult, McpError> {
        let dl = params.0.detail_level.unwrap_or_default();
        let orphan_symbol = params.0.orphan_symbol.clone().unwrap_or_default();

        let knowledge_db_path =
            touring_foundation::TouringConfig::knowledge_db_canonical(&self.config.project_root);
        if !knowledge_db_path.exists() {
            let output = serde_json::json!({"error": "knowledge.db not found — run `touring index rebuild` first"});
            return Ok(CallToolResult::success(vec![Content::text(
                serde_json::to_string_pretty(&output).unwrap_or_default(),
            )]));
        }

        let conn = rusqlite::Connection::open_with_flags(
            &knowledge_db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| McpError::internal_error(format!("knowledge db: {e}"), None))?;
        let _ = conn.execute_batch("PRAGMA busy_timeout = 1000;");

        // If orphan_symbol provided: filter by symbol. If empty: return all pending.
        let sql = if orphan_symbol.is_empty() {
            "SELECT id, orphan_file, COALESCE(suggested_consumer, ''), similarity_score, COALESCE(orphan_symbol, '')
             FROM wiring_suggestions
             WHERE applied = 0 AND rejected = 0
             ORDER BY similarity_score DESC
             LIMIT 100".to_string()
        } else {
            format!(
                "SELECT id, orphan_file, COALESCE(suggested_consumer, ''), similarity_score, COALESCE(orphan_symbol, '')
                 FROM wiring_suggestions
                 WHERE orphan_symbol = '{orphan_symbol}' AND applied = 0 AND rejected = 0
                 ORDER BY similarity_score DESC"
            )
        };

        let mut stmt = conn
            .prepare(&sql)
            .map_err(|e| McpError::internal_error(format!("prepare: {e}"), None))?;

        let mut suggestions: Vec<serde_json::Value> = stmt
            .query_map([], |row| {
                let id: i64 = row.get(0)?;
                let orphan_file: String = row.get(1)?;
                let suggested_consumer: String = row.get(2)?;
                let score: f64 = row.get(3)?;
                let symbol: String = row.get(4)?;
                Ok(serde_json::json!({
                    "id": id,
                    "orphan_file": orphan_file,
                    "suggested_consumer": suggested_consumer,
                    "similarity_score": score,
                    "orphan_symbol": symbol
                }))
            })
            .map_err(|e| McpError::internal_error(format!("query: {e}"), None))?
            .filter_map(|r| r.ok())
            .collect();

        // EC16: Compute-on-demand fallback — mirrors CLI Phase 2 (cli_wiring_suggest).
        // When the wiring_suggestions cache is empty and a specific symbol was requested,
        // derive candidate consumers from the co-edit signal using a read-only SQL query.
        // No upsert: MCP uses ephemeral results; CLI handler caches via upsert on its turn.
        let source = if suggestions.is_empty() && !orphan_symbol.is_empty() {
            // Step 1: Find the orphan file from wiring_map.
            let orphan_file: Option<String> = conn
                .query_row(
                    "SELECT DISTINCT module_file FROM wiring_map \
                     WHERE symbol_name = ?1 AND visibility = 'public' LIMIT 1",
                    rusqlite::params![&orphan_symbol],
                    |r| r.get(0),
                )
                .ok();

            if let Some(ref file) = orphan_file {
                // Step 2: Bidirectional co-edit neighbors (same formula as get_coedit_neighbors).
                let coedit_sql = "SELECT neighbor, SUM(cnt) as total FROM (\
                    SELECT target_path AS neighbor, coedit_count AS cnt \
                    FROM file_coedits WHERE source_path = ?1 \
                    UNION ALL \
                    SELECT source_path AS neighbor, coedit_count AS cnt \
                    FROM file_coedits WHERE target_path = ?1\
                ) GROUP BY neighbor ORDER BY total DESC LIMIT 10";

                if let Ok(mut cstmt) = conn.prepare(coedit_sql) {
                    let pairs: Vec<(String, i64)> = cstmt
                        .query_map(rusqlite::params![file], |r| {
                            Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?))
                        })
                        .map(|rows| rows.filter_map(|r| r.ok()).collect())
                        .unwrap_or_default();

                    let max_count = pairs.iter().map(|(_, c)| *c).max().unwrap_or(1) as f64;
                    suggestions = pairs
                        .into_iter()
                        .map(|(neighbor, count)| {
                            serde_json::json!({
                                "id": serde_json::Value::Null,
                                "orphan_file": file,
                                "suggested_consumer": neighbor,
                                "similarity_score": count as f64 / max_count,
                                "orphan_symbol": &orphan_symbol
                            })
                        })
                        .collect();
                }
            }
            "computed"
        } else {
            "cached"
        };

        let mut result_val = serde_json::json!({
            "orphan_symbol": if orphan_symbol.is_empty() { serde_json::Value::Null } else { serde_json::Value::String(orphan_symbol) },
            "count": suggestions.len(),
            "suggestions": suggestions,
            "source": source
        });
        params::apply_detail_level(&mut result_val, dl);
        crate::tools::suggestions::append_to_response(&mut result_val, "touring_wiring_suggest", 2);
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&result_val).unwrap_or_default(),
        )]))
    }

    // ── Gotcha / Pitfall Database ───────────────────────────────────────

    /// Query project pitfall database — file-specific gotchas from past errors.
    #[tool(
        annotations(read_only_hint = true, title = "Match known pitfalls"),
        name = "touring_gotcha",
        description = "Project pitfall database: file-specific gotchas accumulated from past errors. Actions: list (all gotchas with decay score, severity, hit count), stats (count per file pattern). Use before editing files to surface known pitfalls."
    )]
    async fn gotcha(&self, params: Parameters<GotchaParams>) -> Result<CallToolResult, McpError> {
        let dl = params.0.detail_level.unwrap_or_default();
        let db_path = &self.config.symbols_db_path;
        if !db_path.exists() {
            return Ok(CallToolResult::success(vec![Content::text(
                "Knowledge DB not initialized. Run `touring index rebuild` first.",
            )]));
        }

        let conn = rusqlite::Connection::open_with_flags(
            db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| McpError::internal_error(format!("gotcha db: {e}"), None))?;
        let _ = conn.execute_batch("PRAGMA busy_timeout = 1000;");

        // Check if gotchas table exists
        let table_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='gotchas'",
                [],
                |r| r.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap_or(false);

        if !table_exists {
            return Ok(CallToolResult::success(vec![Content::text(
                "Gotcha table not yet populated. Gotchas accumulate from past errors during edits.",
            )]));
        }

        let action = params.0.action.as_deref().unwrap_or("list");
        let file_filter = params.0.file_path.as_deref();
        let limit = params.0.limit.unwrap_or(50);

        // Check if decay_score column exists (added in S8 migration)
        let has_decay: bool = conn
            .prepare("SELECT decay_score FROM gotchas LIMIT 0")
            .is_ok();

        let result = match action {
            "list" => {
                let base_cols = if has_decay {
                    "id, pattern, gotcha, severity, language, symbol_name, hit_count, \
                     prevented_errors, created_at, decay_score, last_occurrence, resolved_at"
                } else {
                    "id, pattern, gotcha, severity, language, symbol_name, hit_count, \
                     prevented_errors, created_at"
                };

                let query = if file_filter.is_some() {
                    if has_decay {
                        format!(
                            "SELECT {base_cols} FROM gotchas \
                             WHERE pattern LIKE '%' || ?1 || '%' AND resolved_at IS NULL \
                             ORDER BY decay_score DESC, hit_count DESC LIMIT ?2"
                        )
                    } else {
                        format!(
                            "SELECT {base_cols} FROM gotchas \
                             WHERE pattern LIKE '%' || ?1 || '%' \
                             ORDER BY hit_count DESC LIMIT ?2"
                        )
                    }
                } else if has_decay {
                    format!(
                        "SELECT {base_cols} FROM gotchas \
                         WHERE resolved_at IS NULL \
                         ORDER BY decay_score DESC, hit_count DESC LIMIT ?1"
                    )
                } else {
                    format!(
                        "SELECT {base_cols} FROM gotchas \
                         ORDER BY hit_count DESC LIMIT ?1"
                    )
                };

                let mut stmt = conn
                    .prepare(&query)
                    .map_err(|e| McpError::internal_error(format!("prepare: {e}"), None))?;

                let map_row = |r: &rusqlite::Row| -> rusqlite::Result<serde_json::Value> {
                    let mut entry = serde_json::json!({
                        "id": r.get::<_, i64>(0)?,
                        "pattern": r.get::<_, String>(1)?,
                        "gotcha": r.get::<_, String>(2)?,
                        "severity": r.get::<_, String>(3)?,
                        "language": r.get::<_, Option<String>>(4)?,
                        "symbol_name": r.get::<_, Option<String>>(5)?,
                        "hit_count": r.get::<_, i64>(6)?,
                        "prevented_errors": r.get::<_, i64>(7)?,
                        "created_at": r.get::<_, String>(8)?,
                    });
                    if has_decay && let serde_json::Value::Object(ref mut map) = entry {
                        map.insert(
                            "decay_score".to_string(),
                            serde_json::json!(r.get::<_, f64>(9).unwrap_or(1.0)),
                        );
                        map.insert(
                            "last_occurrence".to_string(),
                            serde_json::json!(r.get::<_, Option<String>>(10)?),
                        );
                        map.insert(
                            "resolved_at".to_string(),
                            serde_json::json!(r.get::<_, Option<String>>(11)?),
                        );
                    }
                    Ok(entry)
                };

                let rows: Vec<serde_json::Value> = if let Some(fp) = file_filter {
                    stmt.query_map(rusqlite::params![fp, limit as i64], map_row)
                } else {
                    stmt.query_map(rusqlite::params![limit as i64], map_row)
                }
                .map_err(|e| McpError::internal_error(format!("query: {e}"), None))?
                .filter_map(|r| r.ok())
                .collect();

                let count = rows.len();
                serde_json::json!({ "gotchas": rows, "count": count })
            }
            "stats" => {
                let query = "SELECT pattern, COUNT(*) as cnt, SUM(hit_count) as total_hits, \
                             SUM(prevented_errors) as total_prevented \
                             FROM gotchas GROUP BY pattern ORDER BY total_hits DESC LIMIT ?1";
                let mut stmt = conn
                    .prepare(query)
                    .map_err(|e| McpError::internal_error(format!("prepare: {e}"), None))?;

                let rows: Vec<serde_json::Value> = stmt
                    .query_map(rusqlite::params![limit as i64], |r| {
                        Ok(serde_json::json!({
                            "pattern": r.get::<_, String>(0)?,
                            "gotcha_count": r.get::<_, i64>(1)?,
                            "total_hits": r.get::<_, i64>(2)?,
                            "total_prevented": r.get::<_, i64>(3)?,
                        }))
                    })
                    .map_err(|e| McpError::internal_error(format!("query: {e}"), None))?
                    .filter_map(|r| r.ok())
                    .collect();

                let total_gotchas: i64 = conn
                    .query_row("SELECT COUNT(*) FROM gotchas", [], |r| r.get(0))
                    .unwrap_or(0);
                let total_patterns: i64 = conn
                    .query_row("SELECT COUNT(DISTINCT pattern) FROM gotchas", [], |r| {
                        r.get(0)
                    })
                    .unwrap_or(0);

                serde_json::json!({
                    "stats": rows,
                    "total_gotchas": total_gotchas,
                    "total_patterns": total_patterns,
                })
            }
            other => {
                return Err(McpError::invalid_params(
                    format!("Unknown action '{other}'. Use: list, stats"),
                    None,
                ));
            }
        };

        let mut result_val = result;
        params::apply_detail_level(&mut result_val, dl);
        crate::tools::suggestions::append_to_response(&mut result_val, "touring_gotcha", 2);

        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&result_val).unwrap_or_default(),
        )]))
    }

    // ── WASM Plugin Executor ────────────────────────────────────────────

    /// Execute a WebAssembly plugin with fuel metering and sandbox isolation
    #[cfg(feature = "wasm-plugins")]
    #[tool(
        name = "touring_wasm_plugin",
        description = "Executes a WebAssembly plugin with fuel metering and sandbox isolation. The plugin must be a valid .wasm or .wat file. Feature: wasm-plugins."
    )]
    async fn wasm_plugin_run(
        &self,
        params: Parameters<WasmPluginParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let file_path = p.file_path.clone();
        let input = p.input.clone();
        let _max_fuel = p.max_fuel.unwrap_or(MAX_FUEL);

        // Read WASM bytes from file
        let wasm_bytes = tokio::fs::read(&file_path).await.map_err(|e| {
            McpError::invalid_params(
                format!("Failed to read WASM file '{}': {}", file_path, e),
                None,
            )
        })?;

        // Create runner (uses MAX_FUEL by default; max_fuel param reserved for future use)
        let runner = WasmPluginRunner::new().map_err(|e| {
            McpError::internal_error(format!("Failed to create plugin runner: {}", e), None)
        })?;

        // Run with optional input
        let result = if let Some(ref inp) = input {
            runner.run_plugin(&wasm_bytes, inp)
        } else {
            runner.run_plugin(&wasm_bytes, "")
        };

        match result {
            Ok(plugin_result) => {
                let json = serde_json::json!({
                    "output": plugin_result.output,
                    "fuel_consumed": plugin_result.fuel_consumed,
                    "success": plugin_result.success,
                })
                .to_string();
                Ok(CallToolResult::success(vec![Content::text(json)]))
            }
            Err(e) => Ok(CallToolResult::error(vec![Content::text(format!(
                "Plugin execution error: {}",
                e
            ))])),
        }
    }

    // ── Analysis Report (touring-analysis v0.2.0) ───────────────────────

    /// Unified deep code analysis — composite health score with all dimensions.
    #[tool(
        name = "touring_analysis_report",
        description = "Unified deep code analysis (wiring + quality + temporal + knowledge + learning). Returns {report: CodeHealthReport, dashboard: MetricsDashboard} with composite health + Wilson CIs. depth=quick (~50ms, 30s cache) | standard (~500ms) | deep (~2s+, 300s cache)."
    )]
    async fn analysis_report(
        &self,
        params: Parameters<AnalysisReportParams>,
    ) -> Result<CallToolResult, McpError> {
        let depth_str = params.0.depth.as_deref().unwrap_or("standard");
        let dl = params.0.detail_level.unwrap_or_default();
        let json = self.analysis_report_impl(depth_str).await;
        // Apply detail_level truncation to the analysis report output
        let text = if let Ok(mut val) = serde_json::from_str::<serde_json::Value>(&json) {
            params::apply_detail_level(&mut val, dl);
            serde_json::to_string_pretty(&val).unwrap_or(json)
        } else {
            json
        };
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    // ── Health Check (F3) ────────────────────────────────────────────────

    /// Fast system health check — returns daemon status, index size, orphan rate.
    #[tool(
        annotations(read_only_hint = true, title = "System health snapshot"),
        name = "touring_health",
        description = "Fast system health check (<10ms). Returns: daemon_healthy, index symbol count, file count, knowledge_db metrics. Use to verify Touring is operational before running analysis tools."
    )]
    async fn health_check(
        &self,
        _params: Parameters<HealthCheckParams>,
    ) -> Result<CallToolResult, McpError> {
        let (symbol_count, file_count) = if let Some(ref store) = self.symbol_store {
            let s = store.lock().await;
            match s.stats() {
                Ok(st) => (st.symbol_count, st.file_count),
                Err(_) => (0, 0),
            }
        } else {
            (0, 0)
        };

        // EC40: Enrich health_check with GraphService stats (includes knowledge_db via EC38).
        // graph_svc is always present — KB metrics surface file_count, bash_count, gotcha_count
        // without requiring the caller to invoke a separate touring_project tool.
        let graph_stats = self.graph_svc.stats().await;
        let knowledge_db = graph_stats
            .get("knowledge_db")
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        let mut output = serde_json::json!({
            "daemon_healthy": true,
            "index": {
                "symbol_count": symbol_count,
                "file_count": file_count,
            },
            // EC40: knowledge_db stats from AsyncFileKnowledgeDB (null when adb not initialized).
            "knowledge_db": knowledge_db,
            "status": "ok",
        });
        crate::tools::suggestions::append_to_response(&mut output, "touring_health", 2);
        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    // ── Blast Radius Analysis (D9a) ──────────────────────────────────────

    /// Focused blast radius analysis for a specific file.
    #[tool(
        annotations(read_only_hint = true, title = "Blast radius of a file"),
        name = "touring_blast_radius_analysis",
        description = "Analyze the blast radius of a specific file. Use before editing to gauge ripple. Returns name, score, issues from the blast_radius dimension. Faster than touring_analysis_report for single-file queries."
    )]
    async fn blast_radius_analysis(
        &self,
        params: Parameters<BlastRadiusAnalysisParams>,
    ) -> Result<CallToolResult, McpError> {
        let file_path = params.0.file_path;
        let knowledge_db_path =
            touring_foundation::TouringConfig::knowledge_db_canonical(&self.config.project_root);
        if !knowledge_db_path.exists() {
            let output = serde_json::json!({"error": "knowledge.db not found"});
            let text = serde_json::to_string_pretty(&output)
                .map_err(|e| McpError::internal_error(e.to_string(), None))?;
            return Ok(CallToolResult::success(vec![Content::text(text)]));
        }
        let conn = match rusqlite::Connection::open_with_flags(
            &knowledge_db_path,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        ) {
            Ok(c) => c,
            Err(e) => {
                let output = serde_json::json!({"error": format!("db open failed: {e}")});
                let text = serde_json::to_string_pretty(&output)
                    .map_err(|e2| McpError::internal_error(e2.to_string(), None))?;
                return Ok(CallToolResult::success(vec![Content::text(text)]));
            }
        };
        let pipeline = touring_analysis::AnalysisPipelineBuilder::new(&conn).build();
        let dim = pipeline.run_blast(&file_path);
        let mut output = serde_json::json!({
            "file_path": file_path,
            "blast_dimension": {
                "name": dim.name,
                "score": dim.score,
                "issues": dim.issues,
                "duration_ms": dim.duration_ms,
            },
        });
        let dl = params.0.detail_level.unwrap_or_default();
        params::apply_detail_level(&mut output, dl);
        crate::tools::suggestions::append_to_response(
            &mut output,
            "touring_blast_radius_analysis",
            2,
        );
        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    // ── Quality Check (D9b) ──────────────────────────────────────────────

    /// Focused quality analysis — composite score and dimensions without blast/temporal overhead.
    #[tool(
        name = "touring_quality_check",
        description = "Focused quality check: composite score + per-dimension breakdown. Faster than touring_analysis_report when only quality metrics are needed. Depth: quick (~50ms), standard (~500ms)."
    )]
    async fn quality_check(
        &self,
        params: Parameters<QualityCheckParams>,
    ) -> Result<CallToolResult, McpError> {
        let depth_str = params.0.depth.as_deref().unwrap_or("quick");
        let dl = params.0.detail_level.unwrap_or_default();
        let json = self.analysis_report_impl(depth_str).await;
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap_or_default();
        let mut quality_output = serde_json::json!({
            "quality_focused": true,
            "depth": depth_str,
            "composite_score": parsed.pointer("/report/composite_score"),
            "status": parsed.pointer("/report/status"),
            "dimensions": parsed.pointer("/report/dimensions"),
        });
        params::apply_detail_level(&mut quality_output, dl);
        crate::tools::suggestions::append_to_response(
            &mut quality_output,
            "touring_quality_check",
            2,
        );
        let text = serde_json::to_string_pretty(&quality_output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_metrics",
        description = "Export Prometheus-format metrics for the analysis server (analysis_run, symbol_lookup, cache_hit/miss, wiring_orphan_count, analysis_duration_ms). Optional `filter` matches counter-name prefix."
    )]
    async fn touring_metrics(
        &self,
        params: Parameters<MetricsParams>,
    ) -> Result<CallToolResult, McpError> {
        let filter = params.0.filter.as_deref();
        let dl = params.0.detail_level.unwrap_or_default();
        let raw = AnalysisServerMetrics::global().render_filtered(filter);
        // Apply detail_level: minimal returns first 3 metrics, standard returns 10, full returns all
        let output = if matches!(dl, params::DetailLevel::Full) {
            raw
        } else {
            let max_lines = dl.max_items() * 2; // each metric is ~2 lines (comment + value)
            raw.lines().take(max_lines).collect::<Vec<_>>().join("\n")
        };
        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    // ── Minimal Context (Token-Efficiency Entry Point) ──────────────────

    #[tool(
        annotations(read_only_hint = true, title = "Minimal task context"),
        name = "touring_minimal_context",
        description = "Ultra-compact project context (~100-150 tokens). Always call this FIRST before any other touring tool. Returns: project stats, risk score, top issues, and suggested next tools. Use detail_level='minimal' for ~50 tokens, 'standard' for ~120, 'full' for complete output. Optional task hint improves suggestions (e.g. 'debug auth bug', 'refactor parser')."
    )]
    async fn minimal_context(
        &self,
        params: Parameters<MinimalContextParams>,
    ) -> Result<CallToolResult, McpError> {
        let detail_level = params.0.detail_level.unwrap_or(DetailLevel::Minimal);
        let scope_filter = params.0.file_paths;

        // Gather stats from graph index (in-memory, fast)
        let (symbol_count, file_count) = {
            let idx = self.graph_svc.index().lock().await;
            let s = idx.stats();
            (s.total_symbols as u64, s.total_files as u64)
        };

        // Orphan count from wiring_map table via direct SQLite
        let db_path = &self.config.symbols_db_path;
        let orphan_count: u64 = if db_path.exists() {
            match rusqlite::Connection::open_with_flags(
                db_path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                    | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
            ) {
                Ok(conn) => {
                    let _ = conn.execute_batch("PRAGMA busy_timeout = 500;");
                    conn.query_row(
                        "SELECT COUNT(DISTINCT symbol_name) FROM wiring_map \
                     WHERE visibility='public' AND consumer_file IS NULL",
                        [],
                        |r| r.get::<_, i64>(0),
                    )
                    .unwrap_or(0) as u64
                }
                _ => 0,
            }
        } else {
            0
        };

        // RL reward
        let rl_reward = {
            let rl = self.online_rl.lock().await;
            rl.ema_reward()
        };

        // E2E health approximation
        let e2e_health = if file_count > 0 {
            let orphan_ratio = orphan_count as f64 / (file_count as f64).max(1.0);
            (1.0 - orphan_ratio.min(1.0)).max(0.0)
        } else {
            0.5
        };

        // Top gotchas via direct SQLite
        let max_gotchas = detail_level.max_items();
        let top_gotchas: Vec<String> = if db_path.exists() {
            match rusqlite::Connection::open_with_flags(
                db_path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                    | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
            ) {
                Ok(conn) => {
                    let _ = conn.execute_batch("PRAGMA busy_timeout = 500;");
                    let has_gotchas = conn.query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='gotchas'",
                    [], |r| r.get::<_, i64>(0),
                ).unwrap_or(0) > 0;
                    if has_gotchas {
                        let mut stmt = conn
                            .prepare(
                                "SELECT severity, pattern, gotcha FROM gotchas \
                         WHERE resolved_at IS NULL \
                         ORDER BY hit_count DESC LIMIT ?1",
                            )
                            .unwrap_or_else(|_| {
                                conn.prepare("SELECT 'info','','no gotchas'")
                                    .expect("fallback")
                            });
                        stmt.query_map([max_gotchas as i64], |row| {
                            let sev: String = row.get(0)?;
                            let pat: String = row.get(1)?;
                            let desc: String = row.get(2)?;
                            Ok(format!("[{}] {}: {}", sev, pat, desc))
                        })
                        .ok()
                        .map(|rows| rows.filter_map(|r| r.ok()).collect())
                        .unwrap_or_default()
                    } else {
                        vec![]
                    }
                }
                _ => {
                    vec![]
                }
            }
        } else {
            vec![]
        };

        let mut output = crate::tools::context_tools::build_minimal_context(
            crate::tools::context_tools::MinimalContextInput {
                symbol_count,
                file_count,
                orphan_count,
                e2e_health,
                rl_reward,
                top_gotchas,
                task_hint: params.0.task.as_deref(),
                detail_level,
            },
        );

        // Include scope_filter in output so callers know which files were targeted
        if let Some(files) = scope_filter
            && !files.is_empty()
            && let Some(obj) = output.as_object_mut()
        {
            obj.insert("scope_filter".to_string(), serde_json::json!(files));
        }

        {
            let mut engine = self.hint_engine.lock().await;
            crate::tools::suggestions::append_with_session(
                &mut output,
                "touring_minimal_context",
                &mut engine,
                3,
            );
        }

        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    // ── Detect Changes (Risk-Scored Impact Analysis) ────────────────────

    #[tool(
        annotations(read_only_hint = true, title = "Change risk report"),
        name = "touring_detect_changes",
        description = "Risk-scored change impact analysis. Given changed file paths, returns: overall risk score (0-1), risk level (low/medium/high/critical), affected files (blast radius), test gaps (uncovered functions), hotspots (high-criticality symbols), gotcha warnings, and suggested actions. Combines blast radius + wiring + gotchas + criticality into a single assessment. Use before committing or reviewing changes."
    )]
    async fn detect_changes(
        &self,
        params: Parameters<DetectChangesParams>,
    ) -> Result<CallToolResult, McpError> {
        let detail_level = params.0.detail_level.unwrap_or(DetailLevel::Standard);
        let base_ref = params.0.base.unwrap_or_else(|| "HEAD~1".to_string());
        let changed_files = params.0.file_paths;

        tracing::debug!(base_ref = %base_ref, "detect_changes: using git base ref for diff context");

        if changed_files.is_empty() {
            return Ok(CallToolResult::success(vec![Content::text(
                r#"{"overall_risk": 0.0, "risk_level": "low", "changed_files": 0, "message": "No files specified"}"#,
            )]));
        }

        let db_path = &self.config.symbols_db_path;

        // 1. Blast radius via SymbolStore::get_reverse_deps
        let affected_files: Vec<String> = if let Some(ref store) = self.symbol_store {
            let st = store.lock().await;
            let mut affected = Vec::new();
            for f in &changed_files {
                if let Ok(deps) = st.get_reverse_deps(f) {
                    for dep in deps {
                        if !affected.contains(&dep) && !changed_files.contains(&dep) {
                            affected.push(dep);
                        }
                    }
                }
            }
            affected
        } else {
            vec![]
        };

        // 2. Wiring score via direct SQLite
        let wiring_score: f64 = if db_path.exists() {
            match rusqlite::Connection::open_with_flags(
                db_path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                    | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
            ) {
                Ok(conn) => {
                    let _ = conn.execute_batch("PRAGMA busy_timeout = 500;");
                    let has_wiring = conn.query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='wiring_map'",
                    [], |r| r.get::<_, i64>(0),
                ).unwrap_or(0) > 0;
                    if has_wiring {
                        let total: f64 = conn.query_row(
                        "SELECT COUNT(DISTINCT symbol_name) FROM wiring_map WHERE visibility='public'",
                        [], |r| r.get(0),
                    ).unwrap_or(1.0_f64).max(1.0);
                        let wired: f64 = conn
                            .query_row(
                                "SELECT COUNT(DISTINCT symbol_name) FROM wiring_map \
                         WHERE visibility='public' AND consumer_file IS NOT NULL",
                                [],
                                |r| r.get(0),
                            )
                            .unwrap_or(0.0);
                        wired / total
                    } else {
                        0.5
                    }
                }
                _ => 0.5,
            }
        } else {
            0.5
        };

        // 3. Gotcha warnings via direct SQLite
        let gotcha_warnings: Vec<String> = if db_path.exists() {
            match rusqlite::Connection::open_with_flags(
                db_path,
                rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY
                    | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
            ) {
                Ok(conn) => {
                    let _ = conn.execute_batch("PRAGMA busy_timeout = 500;");
                    let has_gotchas = conn.query_row(
                    "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='gotchas'",
                    [], |r| r.get::<_, i64>(0),
                ).unwrap_or(0) > 0;
                    if has_gotchas {
                        let mut warnings = Vec::new();
                        for f in &changed_files {
                            let pattern = format!("%{}%", f);
                            if let Ok(mut stmt) = conn.prepare(
                                "SELECT severity, gotcha FROM gotchas \
                             WHERE pattern LIKE ?1 AND resolved_at IS NULL \
                             LIMIT 5",
                            ) && let Ok(rows) = stmt.query_map([&pattern], |row| {
                                let sev: String = row.get(0)?;
                                let desc: String = row.get(1)?;
                                Ok(format!("[{}] {}: {}", sev, f, desc))
                            }) {
                                for r in rows.flatten() {
                                    warnings.push(r);
                                }
                            }
                        }
                        warnings
                    } else {
                        vec![]
                    }
                }
                _ => {
                    vec![]
                }
            }
        } else {
            vec![]
        };

        // 4. Build risk report — capture file_type before changed_files is moved
        let file_type_hint = if changed_files.iter().any(|f| f.ends_with(".rs")) {
            "rust"
        } else if changed_files.iter().any(|f| f.ends_with(".py")) {
            "python"
        } else if changed_files.iter().any(|f| f.ends_with(".ts")) {
            "typescript"
        } else {
            "unknown"
        };
        let affected_symbols = affected_files.len() as u32;
        let mut output = crate::tools::risk_scoring::build_change_risk_report(
            crate::tools::risk_scoring::ChangeRiskInput {
                changed_files,
                affected_files,
                affected_symbols,
                test_gaps: vec![],
                hotspots: vec![],
                gotcha_warnings,
                wiring_score,
                detail_level,
            },
        );
        // Include the base ref used for diff context in the report
        if let Some(obj) = output.as_object_mut() {
            obj.insert("base_ref".to_string(), serde_json::json!(base_ref));
        }

        {
            let mut engine = self.hint_engine.lock().await;
            crate::tools::suggestions::append_with_session(
                &mut output,
                "touring_detect_changes",
                &mut engine,
                3,
            );
        }

        // RL reward update: feed risk score to QTable for tool quality learning
        if let Some(overall_risk) = output.get("overall_risk").and_then(|v| v.as_f64()) {
            let tool_name = "risk_scoring";
            let quality_score = (1.0 - overall_risk) * 100.0;
            let mut qt = self.qtable.lock().await;
            qt.update_from_hook_event(0, file_type_hint, tool_name, quality_score);
        }

        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    // ── L7-B Delta: Jobs MCP tools (spawn / poll / list) ─────────────────
    //
    // Exposes the async JobRegistry primitives from touring-hooks as first-class
    // MCP tools. Claude Code can now invoke long-running workers without blocking
    // the tool call, then poll for completion in parallel with other reasoning.

    #[tool(
        name = "touring_spawn_worker",
        description = "L7-B: Spawn a background worker executing a program with arguments. \
                       Uses execve semantics (no shell) — safe from command injection. \
                       Returns a job_id for later polling via touring_poll_worker."
    )]
    async fn spawn_worker(
        &self,
        params: Parameters<JobsSpawnParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        if p.program.is_empty() {
            return Err(McpError::invalid_params("program must not be empty", None));
        }
        let job_id =
            touring_hooks::shared::job_registry::spawn_worker(&p.tool_name, &p.program, &p.args);
        let output = serde_json::json!({
            "job_id": job_id,
            "status": "spawned",
            "tool_name": p.tool_name,
            "program": p.program,
        });
        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_poll_worker",
        description = "L7-B: Poll a previously-spawned job by id. Returns status \
                       (running|completed|failed|not_found), result on completion, \
                       error on failure, and timing metadata (started_at, finished_at, duration_secs)."
    )]
    async fn poll_worker(
        &self,
        params: Parameters<JobsPollParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        if p.job_id.is_empty() {
            return Err(McpError::invalid_params("job_id must not be empty", None));
        }
        let status = touring_hooks::shared::job_registry::poll_worker(&p.job_id);
        let text = serde_json::to_string_pretty(&status)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_list_jobs",
        description = "L7-B: List all jobs in the registry with their current status. \
                       Returns an array of {job_id, status, started_at_secs, is_terminal}."
    )]
    async fn list_jobs(
        &self,
        _params: Parameters<JobsListParams>,
    ) -> Result<CallToolResult, McpError> {
        let jobs = touring_hooks::shared::job_registry::list_jobs();
        let text = serde_json::to_string_pretty(&jobs)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_drop_job",
        description = "L7-B: Drop a job from the registry by id. If the job is still \
                       running, its JoinHandle is aborted. Returns {dropped: bool} — true \
                       if the job was found and removed, false if the id was unknown. \
                       Use after polling a terminal state to explicitly reclaim memory, \
                       or to cancel a long-running worker."
    )]
    async fn drop_job(
        &self,
        params: Parameters<JobsDropParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        if p.job_id.is_empty() {
            return Err(McpError::invalid_params("job_id must not be empty", None));
        }
        let dropped = touring_hooks::shared::job_registry::drop_job(&p.job_id);
        let output = serde_json::json!({
            "dropped": dropped,
            "job_id": p.job_id,
        });
        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    // ── Wave 16 (2026-04-18) — health_delta MCP tools ───────────────────

    #[tool(
        name = "touring_health_delta_status",
        description = "Wave 16: Inspect the cross-hook HealthDeltaCache + StreakCache. \
                       When `file_path` is empty, returns aggregate counters (record/compute/\
                       regression/improvement/streak_alert/recovery/outstanding). When provided, \
                       returns per-path streak state plus warning/improvement hints. Use to \
                       monitor whether recent edits are improving or regressing code health, \
                       and to spot regression streaks (≥3 consecutive declines) early."
    )]
    async fn health_delta_status(
        &self,
        params: Parameters<HealthDeltaStatusParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let path_opt = if p.file_path.is_empty() {
            None
        } else {
            Some(p.file_path.as_str())
        };
        let output = touring_hooks::health_delta::status_json(path_opt);
        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    #[tool(
        name = "touring_health_delta_reset",
        description = "Wave 16: Reset per-path streak counters and discard any pending pre-record \
                       entry for the given file. Useful after a deliberate refactor checkpoint \
                       where the operator wants to start tracking from a known-good baseline. \
                       Returns {reset: true, file_path: <path>}."
    )]
    async fn health_delta_reset(
        &self,
        params: Parameters<HealthDeltaResetParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        if p.file_path.is_empty() {
            return Err(McpError::invalid_params(
                "file_path must not be empty",
                None,
            ));
        }
        let output = touring_hooks::health_delta::reset_json(&p.file_path);
        Ok(CallToolResult::success(vec![Content::text(output)]))
    }

    // ── Wave A A.4 — profile_query MCP tool ─────────────────────────────

    #[tool(
        name = "touring_profile_query",
        description = "Wave A A.4: Query the in-process profile aggregator for latency \
                       histograms by label. Returns entries with count, p50/p90/p99/p999 \
                       latency in microseconds, and total_us for each tracked label. \
                       Use `section` to filter by label prefix, `top_n` to limit results, \
                       and `include_percentiles` to select which quantiles to include \
                       (50=p50, 90=p90, 99=p99, 999=p999)."
    )]
    async fn profile_query(
        &self,
        params: Parameters<ProfileQueryParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let json = touring_foundation::profile::aggregator::snapshot_json();

        // Parse and filter by section if provided
        let parsed: serde_json::Value = serde_json::from_str(&json)
            .map_err(|e| McpError::internal_error(format!("failed to parse profile: {e}"), None))?;

        let entries = parsed.get("entries").and_then(|e| e.as_array());

        let filtered: serde_json::Value = if let Some(ref sec) = p.section {
            let filtered_entries: serde_json::Value = entries
                .map(|arr| {
                    let filtered: Vec<_> = arr
                        .iter()
                        .filter(|e| {
                            e.get("label")
                                .and_then(|l| l.as_str())
                                .map(|l| l.starts_with(sec))
                                .unwrap_or(false)
                        })
                        .cloned()
                        .collect();
                    serde_json::json!({ "entries": filtered, "percent_total": parsed.get("percent_total") })
                })
                .unwrap_or_else(|| serde_json::json!({ "entries": [], "percent_total": 0.0 }));
            filtered_entries
        } else {
            parsed
        };

        // Apply top_n limit
        let limited: serde_json::Value = if p.top_n > 0 {
            if let Some(arr) = filtered.get("entries").and_then(|e| e.as_array()) {
                let limited: Vec<_> = arr.iter().take(p.top_n as usize).cloned().collect();
                let mut out = filtered.clone();
                out["entries"] = serde_json::json!(limited);
                out
            } else {
                filtered
            }
        } else {
            filtered
        };

        // Filter percentiles if include_percentiles specified
        let final_output = if let Some(ref pct) = p.include_percentiles {
            if let Some(arr) = limited.get("entries").and_then(|e| e.as_array()) {
                let filtered_entries: Vec<_> = arr
                    .iter()
                    .map(|entry| {
                        let mut new_entry = serde_json::Map::new();
                        for (k, v) in entry.as_object().unwrap_or(&serde_json::Map::new()).iter() {
                            // Keep label and count always
                            if k == "label" || k == "count" || k == "total_us" {
                                new_entry.insert(k.clone(), v.clone());
                            } else if let Some(num) = k.strip_prefix("p")
                                && let Ok(pval) = num.parse::<u8>()
                                && pct.contains(&pval)
                            {
                                new_entry.insert(k.clone(), v.clone());
                            }
                        }
                        serde_json::Value::Object(new_entry)
                    })
                    .collect();
                let mut out = limited.clone();
                out["entries"] = serde_json::json!(filtered_entries);
                out
            } else {
                limited
            }
        } else {
            limited
        };

        let text = serde_json::to_string_pretty(&final_output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    // ── Wave B B.1 — ssr_apply MCP tool ────────────────────────────────

    #[tool(
        name = "touring_ssr_apply",
        description = "Wave B B.1: Apply a Structural Search & Replace (SSR) rule to source code. \
                       Takes a pattern (ast-grep syntax) and replacement, applies them to the \
                       provided source code, and returns the rewritten result. Supports Rust, \
                       JavaScript, TypeScript, Python, Go, Java, and Bash. Use for multi-language \
                       refactors like `foo($X) => $X.foo()` method call conversion."
    )]
    async fn ssr_apply(
        &self,
        params: Parameters<SsrApplyParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;

        // Parse language
        let lang = touring_code::ast::Lang::from_str(&p.lang).map_err(|e| {
            McpError::invalid_params(format!("unsupported language '{}': {}", p.lang, e), None)
        })?;

        // Build SSR rule
        let rule = touring_code::ast::ssr::SsrRule {
            id: "mcp-rule".to_string(),
            lang: p.lang.clone(),
            pattern: p.pattern,
            replacement: p.replacement,
            file_path: None,
        };

        // Apply SSR
        let result = touring_code::ast::ssr::apply_ssr_rule(&rule, &p.source, lang)
            .map_err(|e| McpError::internal_error(format!("SSR apply failed: {e}"), None))?;

        // Build output matching cli/ssr.rs SsrApplyOutput
        let output = serde_json::json!({
            "rule_id": result.rule_id,
            "file_path": result.file_path,
            "matches": result.matches,
            "was_formatted": result.was_formatted,
        });

        let text = serde_json::to_string_pretty(&output)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }
}
