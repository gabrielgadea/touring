//! NEW-3 Wave 2026-05-08 post-wave — MCP wiring for the 3 new ctx_* tools.
//!
//! Exposes `touring_hooks::cli_handlers_mcp::{ctx_gain, ctx_discover,
//! ctx_tee_retrieve}` as real MCP tools on the `TouringServer`.
//!
//! These three tools were added in the post-wave RTK integration plan but
//! were only callable from in-process Rust — the MCP server did not register
//! them. This file closes that gap, wiring them into the `#[tool_router]`
//! impl on `TouringServer` so any MCP client (Claude Code, Gemini CLI, etc.)
//! can invoke them.
//!
//! - `touring_ctx_gain`         — token-savings dashboard from GateMetrics
//! - `touring_ctx_discover`     — registered compression-profiles catalog
//! - `touring_ctx_tee_retrieve` — fetch full unredacted output for a tee'd failure
//!
//! All three are read-only (no router required) and run via
//! `spawn_blocking` per the established Tantivy tool pattern.

use super::*;

#[tool_router(router = router_context_router, vis = "pub(crate)")]
impl TouringServer {
    /// NEW-3: token-savings dashboard.
    ///
    /// Aggregates GateMetrics counters (tool_output_routed, compression_applied,
    /// tee_persisted, ttl_skip, trigram_query, phrase_query) into a single
    /// envelope showing the value Touring delivered this session.
    #[tool(
        name = "touring_ctx_gain",
        description = "Token-savings dashboard. Aggregates per-session GateMetrics \
                       counters into bytes_saved + tokens_saved + human-readable \
                       summary. Read-only, <5ms. Use to surface 'value Touring delivered \
                       this session'."
    )]
    pub(crate) async fn touring_ctx_gain(
        &self,
        _params: Parameters<CtxGainParams>,
    ) -> Result<CallToolResult, McpError> {
        let result = tokio::task::spawn_blocking(touring_hooks::cli_handlers_mcp::ctx_gain)
            .await
            .unwrap_or_else(|e| serde_json::json!({"error": format!("spawn_blocking: {e}")}));

        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    /// NEW-3: optimisation recommendation engine.
    ///
    /// Returns the catalog of registered compression profiles so the LLM can
    /// prioritise which tool families to route through Sandbox<Lang> first.
    #[tool(
        name = "touring_ctx_discover",
        description = "Compression-profile catalog. Returns registered_profiles \
                       (cargo_test, git_log, kubectl_get, etc.), profile_count, \
                       and a recommendation string. Read-only. Use before a session \
                       to know which Bash commands Touring will compress automatically."
    )]
    pub(crate) async fn touring_ctx_discover(
        &self,
        _params: Parameters<CtxDiscoverParams>,
    ) -> Result<CallToolResult, McpError> {
        let result = tokio::task::spawn_blocking(touring_hooks::cli_handlers_mcp::ctx_discover)
            .await
            .unwrap_or_else(|e| serde_json::json!({"error": format!("spawn_blocking: {e}")}));

        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    /// NEW-2: fetch the full unredacted output from a sandbox FAILURE captured
    /// via tee mode.
    ///
    /// When `execute_in_sandbox` produces `exit_code != 0`, the full
    /// stdout+stderr (after `redact_secrets`) is persisted to
    /// `~/.claude/touring/tee/<hash>.log`. Use this tool when the
    /// compressed summary in `tool_outputs` is insufficient to debug.
    #[tool(
        name = "touring_ctx_tee_retrieve",
        description = "Fetch the full unredacted output from a sandbox failure \
                       captured via tee mode (NEW-2 Wave 2026-05-08). Pass \
                       `content_hash` (blake3-64-hex from a stored doc with exit_code \
                       != 0). Returns {ok, found, content_hash, full_output, byte_count} \
                       or {ok, found:false} when no tee log exists for the hash."
    )]
    pub(crate) async fn touring_ctx_tee_retrieve(
        &self,
        params: Parameters<CtxTeeRetrieveParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let hash = p.content_hash.clone();

        let result = tokio::task::spawn_blocking(move || {
            touring_hooks::cli_handlers_mcp::ctx_tee_retrieve(&hash)
        })
        .await
        .unwrap_or_else(|e| serde_json::json!({"error": format!("spawn_blocking: {e}")}));

        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    // ─── Wave 3 INTELLIGENCE — 15 T1 MCP tools ─────────────────────────────

    #[tool(
        name = "touring_ctx_replay",
        description = "T1-01: Replay last N session steps as compressed summaries. Useful after /clear to reload the LLM mental model without rehydrating raw outputs."
    )]
    pub(crate) async fn touring_ctx_replay(
        &self,
        params: Parameters<CtxReplayParams>,
    ) -> Result<CallToolResult, McpError> {
        let n = params.0.n.unwrap_or(10) as usize;
        let result =
            tokio::task::spawn_blocking(move || touring_hooks::cli_handlers_mcp::ctx_replay(n))
                .await
                .unwrap_or_else(|e| serde_json::json!({"error": format!("spawn_blocking: {e}")}));
        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_ctx_purge",
        description = "T1-02: Cleanup transient ctx state (tee logs + tool_outputs index + expired memory). Pass {all:true} for full cleanup, or per-target booleans. tier=semantic memory preserved."
    )]
    pub(crate) async fn touring_ctx_purge(
        &self,
        params: Parameters<CtxPurgeParams>,
    ) -> Result<CallToolResult, McpError> {
        let p = params.0;
        let result = tokio::task::spawn_blocking(move || {
            let targets = touring_hooks::cli_handlers_mcp::PurgeTargets {
                tee_logs: p.tee_logs.unwrap_or(false),
                tool_outputs_index: p.tool_outputs_index.unwrap_or(false),
                expired_memory: p.expired_memory.unwrap_or(false),
                all: p.all.unwrap_or(false),
            };
            touring_hooks::cli_handlers_mcp::ctx_purge(targets)
        })
        .await
        .unwrap_or_else(|e| serde_json::json!({"error": format!("spawn_blocking: {e}")}));
        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_ctx_doctor",
        description = "T1-03: Diagnostics for ctx subsystem (tantivy_global, tool_outputs_global, tee_dir, user_filters, compression_profiles). Returns {ok, components: [{name, status, ...}]}."
    )]
    pub(crate) async fn touring_ctx_doctor(
        &self,
        _params: Parameters<CtxDoctorParams>,
    ) -> Result<CallToolResult, McpError> {
        let result = tokio::task::spawn_blocking(touring_hooks::cli_handlers_mcp::ctx_doctor)
            .await
            .unwrap_or_else(|e| serde_json::json!({"error": format!("spawn_blocking: {e}")}));
        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_ctx_gain_history",
        description = "T1-04: Per-day breakdown of GateMetrics. Pass `days` (default 7, max 30) — returns rows of {date, tool_output_routed_count, compression_profile_applied_count, sandbox_tee_persisted_count}."
    )]
    pub(crate) async fn touring_ctx_gain_history(
        &self,
        params: Parameters<CtxGainHistoryParams>,
    ) -> Result<CallToolResult, McpError> {
        let days = params.0.days.unwrap_or(7);
        let result = tokio::task::spawn_blocking(move || {
            touring_hooks::cli_handlers_mcp::ctx_gain_history(days)
        })
        .await
        .unwrap_or_else(|e| serde_json::json!({"error": format!("spawn_blocking: {e}")}));
        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_ctx_gain_graph",
        description = "T1-05: ASCII sparkline of bytes_saved over last N days using ▁▂▃▄▅▆▇█. Renderable in any terminal, zero deps."
    )]
    pub(crate) async fn touring_ctx_gain_graph(
        &self,
        params: Parameters<CtxGainGraphParams>,
    ) -> Result<CallToolResult, McpError> {
        let days = params.0.days.unwrap_or(30);
        let result = tokio::task::spawn_blocking(move || {
            touring_hooks::cli_handlers_mcp::ctx_gain_graph(days)
        })
        .await
        .unwrap_or_else(|e| serde_json::json!({"error": format!("spawn_blocking: {e}")}));
        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_ctx_session_adoption",
        description = "T1-06: Adoption ratio for the current session — routed_bash + compressed / total_bash. Surfaces 'how much value Touring is delivering this session'."
    )]
    pub(crate) async fn touring_ctx_session_adoption(
        &self,
        _params: Parameters<CtxSessionAdoptionParams>,
    ) -> Result<CallToolResult, McpError> {
        let result =
            tokio::task::spawn_blocking(touring_hooks::cli_handlers_mcp::ctx_session_adoption)
                .await
                .unwrap_or_else(|e| serde_json::json!({"error": format!("spawn_blocking: {e}")}));
        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_ctx_init_agent",
        description = "T1-07: Returns the install plan for an agent (claude-code, cursor, gemini-cli, codex, opencode, kiro, zed). Read-only — actual write requires CLI subcommand."
    )]
    pub(crate) async fn touring_ctx_init_agent(
        &self,
        params: Parameters<CtxInitAgentParams>,
    ) -> Result<CallToolResult, McpError> {
        let agent = params.0.agent.clone();
        let result = tokio::task::spawn_blocking(move || {
            touring_hooks::cli_handlers_mcp::ctx_init_agent(&agent)
        })
        .await
        .unwrap_or_else(|e| serde_json::json!({"error": format!("spawn_blocking: {e}")}));
        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_ctx_smart",
        description = "T1-08: 2-line heuristic file summary. Line 1: stats (LOC, pub items, fn count). Line 2: top-2 pub fn signatures. Sub-50ms on typical files."
    )]
    pub(crate) async fn touring_ctx_smart(
        &self,
        params: Parameters<CtxSmartParams>,
    ) -> Result<CallToolResult, McpError> {
        let path = params.0.file_path.clone();
        let result =
            tokio::task::spawn_blocking(move || touring_hooks::cli_handlers_mcp::ctx_smart(&path))
                .await
                .unwrap_or_else(|e| serde_json::json!({"error": format!("spawn_blocking: {e}")}));
        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_ctx_chunk_read",
        description = "T1-09: Symbolic chunking for large files (>500 LOC by default). Strips function/method bodies, returns only signatures+use+doc-comments. Threshold tunable via TOURING_READ_CHUNKING_THRESHOLD_LOC."
    )]
    pub(crate) async fn touring_ctx_chunk_read(
        &self,
        params: Parameters<CtxChunkReadParams>,
    ) -> Result<CallToolResult, McpError> {
        let path = params.0.file_path.clone();
        let threshold = params.0.threshold.map(|v| v as usize);
        let result = tokio::task::spawn_blocking(move || {
            touring_hooks::cli_handlers_mcp::ctx_chunk_read(&path, threshold)
        })
        .await
        .unwrap_or_else(|e| serde_json::json!({"error": format!("spawn_blocking: {e}")}));
        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_ctx_explain",
        description = "T1-10: Explain a GateMetrics counter — returns its current value, plain-English description, and related counters. Use to interpret gate-metrics output."
    )]
    pub(crate) async fn touring_ctx_explain(
        &self,
        params: Parameters<CtxExplainParams>,
    ) -> Result<CallToolResult, McpError> {
        let name = params.0.counter_name.clone();
        let result = tokio::task::spawn_blocking(move || {
            touring_hooks::cli_handlers_mcp::ctx_explain(&name)
        })
        .await
        .unwrap_or_else(|e| serde_json::json!({"error": format!("spawn_blocking: {e}")}));
        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_ctx_budget",
        description = "T1-11: Token budget tracking. Pass `used_tokens` for the current session — returns {used, limit, remaining, pct_used, alert_level}. Limit configured via TOURING_TOKEN_BUDGET_PER_SESSION (default 500_000). Mitigates the '$200-burn' scenario."
    )]
    pub(crate) async fn touring_ctx_budget(
        &self,
        params: Parameters<CtxBudgetParams>,
    ) -> Result<CallToolResult, McpError> {
        let used = params.0.used_tokens.unwrap_or(0);
        let result =
            tokio::task::spawn_blocking(move || touring_hooks::cli_handlers_mcp::ctx_budget(used))
                .await
                .unwrap_or_else(|e| serde_json::json!({"error": format!("spawn_blocking: {e}")}));
        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_ctx_batch_execute",
        description = "T1-12: Bundle multiple ctx_* invocations into a single MCP call. Pass `items` as JSON array of {kind, ...args}. kinds: replay, smart, doctor, gain_history. Reduces roundtrip latency."
    )]
    pub(crate) async fn touring_ctx_batch_execute(
        &self,
        params: Parameters<CtxBatchExecuteParams>,
    ) -> Result<CallToolResult, McpError> {
        let items = params.0.items.clone();
        let result = tokio::task::spawn_blocking(move || {
            touring_hooks::cli_handlers_mcp::ctx_batch_execute(&items)
        })
        .await
        .unwrap_or_else(|e| serde_json::json!({"error": format!("spawn_blocking: {e}")}));
        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_ctx_execute_file",
        description = "T1-13: Invoke sandbox executor on a script file. Pass `file_path` + `language` (python|js|ts|sh|ruby|go|rust|php|perl|r|elixir). Returns the execution envelope."
    )]
    pub(crate) async fn touring_ctx_execute_file(
        &self,
        params: Parameters<CtxExecuteFileParams>,
    ) -> Result<CallToolResult, McpError> {
        let path = params.0.file_path.clone();
        let lang = params.0.language.clone();
        let result = tokio::task::spawn_blocking(move || {
            touring_hooks::cli_handlers_mcp::ctx_execute_file(&path, &lang)
        })
        .await
        .unwrap_or_else(|e| serde_json::json!({"error": format!("spawn_blocking: {e}")}));
        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_ctx_upgrade",
        description = "T1-14: Plan / report a Touring rebuild + symlink + restart. Pass `dry_run: true` for a plan only; live invocation requires CLI for safety."
    )]
    pub(crate) async fn touring_ctx_upgrade(
        &self,
        params: Parameters<CtxUpgradeParams>,
    ) -> Result<CallToolResult, McpError> {
        let dry = params.0.dry_run.unwrap_or(true);
        let result =
            tokio::task::spawn_blocking(move || touring_hooks::cli_handlers_mcp::ctx_upgrade(dry))
                .await
                .unwrap_or_else(|e| serde_json::json!({"error": format!("spawn_blocking: {e}")}));
        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    #[tool(
        name = "touring_ctx_discover_session",
        description = "T1-15: Scan hook_events for the current session, identify Bash invocations whose result was >5KB AND classify_tool_routing was Passthrough. Surface missed savings ranked by est_bytes."
    )]
    pub(crate) async fn touring_ctx_discover_session(
        &self,
        _params: Parameters<CtxDiscoverSessionParams>,
    ) -> Result<CallToolResult, McpError> {
        let result =
            tokio::task::spawn_blocking(touring_hooks::cli_handlers_mcp::ctx_discover_session)
                .await
                .unwrap_or_else(|e| serde_json::json!({"error": format!("spawn_blocking: {e}")}));
        let text = serde_json::to_string_pretty(&result)
            .map_err(|e| McpError::internal_error(e.to_string(), None))?;
        Ok(CallToolResult::success(vec![Content::text(text)]))
    }

    // ─── Wave 3 Extended (T2 + T3) — 25 MCP tools ──────────────────────────

    #[tool(
        name = "touring_ctx_linucb_compression",
        description = "T2-01: LinUCB-driven compression decisions envelope."
    )]
    pub(crate) async fn touring_ctx_linucb_compression(
        &self,
        params: Parameters<Wave3StringParams>,
    ) -> Result<CallToolResult, McpError> {
        let s = params.0.value.clone();
        let r = tokio::task::spawn_blocking(move || {
            touring_hooks::wave3_extended::ctx_linucb_compression(&s)
        })
        .await
        .unwrap_or_else(|e| serde_json::json!({"error": format!("{e}")}));
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&r).unwrap_or_default(),
        )]))
    }

    #[tool(
        name = "touring_ctx_burn_detect",
        description = "T2-02: Detect tool-call-burn patterns in current session."
    )]
    pub(crate) async fn touring_ctx_burn_detect(
        &self,
        _params: Parameters<Wave3EmptyParams>,
    ) -> Result<CallToolResult, McpError> {
        let r = tokio::task::spawn_blocking(touring_hooks::wave3_extended::ctx_burn_detect)
            .await
            .unwrap_or_else(|e| serde_json::json!({"error": format!("{e}")}));
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&r).unwrap_or_default(),
        )]))
    }

    #[tool(
        name = "touring_ctx_precompact_recompress",
        description = "T2-03: PreCompact aggressive recompression envelope."
    )]
    pub(crate) async fn touring_ctx_precompact_recompress(
        &self,
        _params: Parameters<Wave3EmptyParams>,
    ) -> Result<CallToolResult, McpError> {
        let r =
            tokio::task::spawn_blocking(touring_hooks::wave3_extended::ctx_precompact_recompress)
                .await
                .unwrap_or_else(|e| serde_json::json!({"error": format!("{e}")}));
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&r).unwrap_or_default(),
        )]))
    }

    #[tool(
        name = "touring_ctx_session_inheritance",
        description = "T2-04: Cross-session inheritance — replay top lessons."
    )]
    pub(crate) async fn touring_ctx_session_inheritance(
        &self,
        _params: Parameters<Wave3EmptyParams>,
    ) -> Result<CallToolResult, McpError> {
        let r = tokio::task::spawn_blocking(touring_hooks::wave3_extended::ctx_session_inheritance)
            .await
            .unwrap_or_else(|e| serde_json::json!({"error": format!("{e}")}));
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&r).unwrap_or_default(),
        )]))
    }

    #[tool(
        name = "touring_ctx_search_regex",
        description = "T2-05: Tantivy RegexQuery wrapper."
    )]
    pub(crate) async fn touring_ctx_search_regex(
        &self,
        params: Parameters<Wave3StringNParams>,
    ) -> Result<CallToolResult, McpError> {
        let s = params.0.value.clone();
        let n = params.0.n.unwrap_or(20) as usize;
        let r = tokio::task::spawn_blocking(move || {
            touring_hooks::wave3_extended::ctx_search_regex(&s, n)
        })
        .await
        .unwrap_or_else(|e| serde_json::json!({"error": format!("{e}")}));
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&r).unwrap_or_default(),
        )]))
    }

    #[tool(
        name = "touring_ctx_search_phrase_prefix",
        description = "T2-06: Tantivy PhrasePrefixQuery wrapper."
    )]
    pub(crate) async fn touring_ctx_search_phrase_prefix(
        &self,
        params: Parameters<Wave3StringNParams>,
    ) -> Result<CallToolResult, McpError> {
        let s = params.0.value.clone();
        let n = params.0.n.unwrap_or(20) as usize;
        let r = tokio::task::spawn_blocking(move || {
            touring_hooks::wave3_extended::ctx_search_phrase_prefix(&s, n)
        })
        .await
        .unwrap_or_else(|e| serde_json::json!({"error": format!("{e}")}));
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&r).unwrap_or_default(),
        )]))
    }

    #[tool(
        name = "touring_ctx_aggregate_daily",
        description = "T2-07: DateHistogramAggregation by day."
    )]
    pub(crate) async fn touring_ctx_aggregate_daily(
        &self,
        params: Parameters<Wave3StringNParams>,
    ) -> Result<CallToolResult, McpError> {
        let s = params.0.value.clone();
        let n = params.0.n.unwrap_or(30);
        let r = tokio::task::spawn_blocking(move || {
            touring_hooks::wave3_extended::ctx_aggregate_daily(&s, n)
        })
        .await
        .unwrap_or_else(|e| serde_json::json!({"error": format!("{e}")}));
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&r).unwrap_or_default(),
        )]))
    }

    #[tool(
        name = "touring_ctx_top_compressors",
        description = "T2-08: Custom Collector top-by-savings."
    )]
    pub(crate) async fn touring_ctx_top_compressors(
        &self,
        params: Parameters<Wave3NParams>,
    ) -> Result<CallToolResult, McpError> {
        let n = params.0.n.unwrap_or(10) as usize;
        let r = tokio::task::spawn_blocking(move || {
            touring_hooks::wave3_extended::ctx_top_compressors(n)
        })
        .await
        .unwrap_or_else(|e| serde_json::json!({"error": format!("{e}")}));
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&r).unwrap_or_default(),
        )]))
    }

    #[tool(
        name = "touring_ctx_think_in_code",
        description = "T2-09: Think-in-Code directive injector."
    )]
    pub(crate) async fn touring_ctx_think_in_code(
        &self,
        params: Parameters<Wave3StringParams>,
    ) -> Result<CallToolResult, McpError> {
        let s = params.0.value.clone();
        let r = tokio::task::spawn_blocking(move || {
            touring_hooks::wave3_extended::ctx_think_in_code(&s)
        })
        .await
        .unwrap_or_else(|e| serde_json::json!({"error": format!("{e}")}));
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&r).unwrap_or_default(),
        )]))
    }

    #[tool(
        name = "touring_ctx_roi",
        description = "T2-10: USD savings via pricing model."
    )]
    pub(crate) async fn touring_ctx_roi(
        &self,
        params: Parameters<Wave3StringParams>,
    ) -> Result<CallToolResult, McpError> {
        let s = params.0.value.clone();
        let r = tokio::task::spawn_blocking(move || touring_hooks::wave3_extended::ctx_roi(&s))
            .await
            .unwrap_or_else(|e| serde_json::json!({"error": format!("{e}")}));
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&r).unwrap_or_default(),
        )]))
    }

    #[tool(
        name = "touring_ctx_read_summary",
        description = "T2-11: Read >10KB → summary."
    )]
    pub(crate) async fn touring_ctx_read_summary(
        &self,
        params: Parameters<Wave3StringParams>,
    ) -> Result<CallToolResult, McpError> {
        let s = params.0.value.clone();
        let r = tokio::task::spawn_blocking(move || {
            touring_hooks::wave3_extended::ctx_read_summary(&s)
        })
        .await
        .unwrap_or_else(|e| serde_json::json!({"error": format!("{e}")}));
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&r).unwrap_or_default(),
        )]))
    }

    #[tool(
        name = "touring_ctx_webfetch_chunk",
        description = "T2-12: WebFetch markdown chunker."
    )]
    pub(crate) async fn touring_ctx_webfetch_chunk(
        &self,
        params: Parameters<Wave3StringParams>,
    ) -> Result<CallToolResult, McpError> {
        let s = params.0.value.clone();
        let r = tokio::task::spawn_blocking(move || {
            touring_hooks::wave3_extended::ctx_webfetch_chunk(&s)
        })
        .await
        .unwrap_or_else(|e| serde_json::json!({"error": format!("{e}")}));
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&r).unwrap_or_default(),
        )]))
    }

    #[tool(
        name = "touring_ctx_grep_rerank",
        description = "T2-13: BM25 rerank Grep hits."
    )]
    pub(crate) async fn touring_ctx_grep_rerank(
        &self,
        params: Parameters<Wave3HitsNParams>,
    ) -> Result<CallToolResult, McpError> {
        let hits = params.0.hits.clone();
        let n = params.0.n.unwrap_or(50) as usize;
        let r = tokio::task::spawn_blocking(move || {
            touring_hooks::wave3_extended::ctx_grep_rerank(&hits, n)
        })
        .await
        .unwrap_or_else(|e| serde_json::json!({"error": format!("{e}")}));
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&r).unwrap_or_default(),
        )]))
    }

    #[tool(
        name = "touring_ctx_err_filter",
        description = "T2-14: Generic error-only filter."
    )]
    pub(crate) async fn touring_ctx_err_filter(
        &self,
        params: Parameters<Wave3RawExitParams>,
    ) -> Result<CallToolResult, McpError> {
        let raw = params.0.raw.clone();
        let exit = params.0.exit_code.unwrap_or(0);
        let r = tokio::task::spawn_blocking(move || {
            touring_hooks::wave3_extended::ctx_err_filter(&raw, exit)
        })
        .await
        .unwrap_or_else(|e| serde_json::json!({"error": format!("{e}")}));
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&r).unwrap_or_default(),
        )]))
    }

    #[tool(
        name = "touring_ctx_tier_lift",
        description = "T2-15: session_tier_lift Think-in-Code at Tier3."
    )]
    pub(crate) async fn touring_ctx_tier_lift(
        &self,
        params: Parameters<Wave3TierParams>,
    ) -> Result<CallToolResult, McpError> {
        let tier = params.0.tier.unwrap_or(0);
        let r =
            tokio::task::spawn_blocking(move || touring_hooks::wave3_extended::ctx_tier_lift(tier))
                .await
                .unwrap_or_else(|e| serde_json::json!({"error": format!("{e}")}));
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&r).unwrap_or_default(),
        )]))
    }

    #[tool(
        name = "touring_ctx_lsp_status",
        description = "T3-01: LSP integration status."
    )]
    pub(crate) async fn touring_ctx_lsp_status(
        &self,
        _params: Parameters<Wave3EmptyParams>,
    ) -> Result<CallToolResult, McpError> {
        let r = tokio::task::spawn_blocking(touring_hooks::wave3_extended::ctx_lsp_status)
            .await
            .unwrap_or_else(|e| serde_json::json!({"error": format!("{e}")}));
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&r).unwrap_or_default(),
        )]))
    }

    #[tool(
        name = "touring_ctx_shared_cache_status",
        description = "T3-02: Multi-agent shared cache status."
    )]
    pub(crate) async fn touring_ctx_shared_cache_status(
        &self,
        _params: Parameters<Wave3EmptyParams>,
    ) -> Result<CallToolResult, McpError> {
        let r = tokio::task::spawn_blocking(touring_hooks::wave3_extended::ctx_shared_cache_status)
            .await
            .unwrap_or_else(|e| serde_json::json!({"error": format!("{e}")}));
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&r).unwrap_or_default(),
        )]))
    }

    #[tool(
        name = "touring_ctx_cloud_sync_status",
        description = "T3-03: Cloud sync session status."
    )]
    pub(crate) async fn touring_ctx_cloud_sync_status(
        &self,
        _params: Parameters<Wave3EmptyParams>,
    ) -> Result<CallToolResult, McpError> {
        let r = tokio::task::spawn_blocking(touring_hooks::wave3_extended::ctx_cloud_sync_status)
            .await
            .unwrap_or_else(|e| serde_json::json!({"error": format!("{e}")}));
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&r).unwrap_or_default(),
        )]))
    }

    #[tool(
        name = "touring_ctx_synthesize_profile",
        description = "T3-04: AI-powered profile synthesizer."
    )]
    pub(crate) async fn touring_ctx_synthesize_profile(
        &self,
        params: Parameters<Wave3StringParams>,
    ) -> Result<CallToolResult, McpError> {
        let s = params.0.value.clone();
        let r = tokio::task::spawn_blocking(move || {
            touring_hooks::wave3_extended::ctx_synthesize_profile(&s)
        })
        .await
        .unwrap_or_else(|e| serde_json::json!({"error": format!("{e}")}));
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&r).unwrap_or_default(),
        )]))
    }

    #[tool(
        name = "touring_ctx_web_status",
        description = "T3-05: Web UI dashboard status."
    )]
    pub(crate) async fn touring_ctx_web_status(
        &self,
        _params: Parameters<Wave3EmptyParams>,
    ) -> Result<CallToolResult, McpError> {
        let r = tokio::task::spawn_blocking(touring_hooks::wave3_extended::ctx_web_status)
            .await
            .unwrap_or_else(|e| serde_json::json!({"error": format!("{e}")}));
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&r).unwrap_or_default(),
        )]))
    }

    #[tool(
        name = "touring_ctx_federated_search",
        description = "T3-06: Federated multi-project search."
    )]
    pub(crate) async fn touring_ctx_federated_search(
        &self,
        params: Parameters<Wave3StringParams>,
    ) -> Result<CallToolResult, McpError> {
        let s = params.0.value.clone();
        let r = tokio::task::spawn_blocking(move || {
            touring_hooks::wave3_extended::ctx_federated_search(&s)
        })
        .await
        .unwrap_or_else(|e| serde_json::json!({"error": format!("{e}")}));
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&r).unwrap_or_default(),
        )]))
    }

    #[tool(
        name = "touring_ctx_otlp_status",
        description = "T3-07: OpenTelemetry exporter status."
    )]
    pub(crate) async fn touring_ctx_otlp_status(
        &self,
        _params: Parameters<Wave3EmptyParams>,
    ) -> Result<CallToolResult, McpError> {
        let r = tokio::task::spawn_blocking(touring_hooks::wave3_extended::ctx_otlp_status)
            .await
            .unwrap_or_else(|e| serde_json::json!({"error": format!("{e}")}));
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&r).unwrap_or_default(),
        )]))
    }

    #[tool(
        name = "touring_ctx_graphql_status",
        description = "T3-08: GraphQL endpoint status."
    )]
    pub(crate) async fn touring_ctx_graphql_status(
        &self,
        _params: Parameters<Wave3EmptyParams>,
    ) -> Result<CallToolResult, McpError> {
        let r = tokio::task::spawn_blocking(touring_hooks::wave3_extended::ctx_graphql_status)
            .await
            .unwrap_or_else(|e| serde_json::json!({"error": format!("{e}")}));
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&r).unwrap_or_default(),
        )]))
    }

    #[tool(
        name = "touring_ctx_polyglot_summary",
        description = "T3-09: Cross-language polyglot summary."
    )]
    pub(crate) async fn touring_ctx_polyglot_summary(
        &self,
        params: Parameters<Wave3StringParams>,
    ) -> Result<CallToolResult, McpError> {
        let s = params.0.value.clone();
        let r = tokio::task::spawn_blocking(move || {
            touring_hooks::wave3_extended::ctx_polyglot_summary(&s)
        })
        .await
        .unwrap_or_else(|e| serde_json::json!({"error": format!("{e}")}));
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&r).unwrap_or_default(),
        )]))
    }

    #[tool(
        name = "touring_ctx_pr_risk_score",
        description = "T3-10: CI/CD risk gates — PR risk score."
    )]
    pub(crate) async fn touring_ctx_pr_risk_score(
        &self,
        params: Parameters<Wave3PrParams>,
    ) -> Result<CallToolResult, McpError> {
        let pr = params.0.pr_number.unwrap_or(0);
        let r = tokio::task::spawn_blocking(move || {
            touring_hooks::wave3_extended::ctx_pr_risk_score(pr)
        })
        .await
        .unwrap_or_else(|e| serde_json::json!({"error": format!("{e}")}));
        Ok(CallToolResult::success(vec![Content::text(
            serde_json::to_string_pretty(&r).unwrap_or_default(),
        )]))
    }
}
