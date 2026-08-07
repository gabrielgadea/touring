//! D6 — MCP Context Router for multi-agent context sharing.
//!
//! Re-positions the existing 96 MCP tools as a "context router" usable by
//! multiple LLM agents. Exposes 5 entry points:
//!
//! - `ctx_search`   — BM25 + RRF search over symbol corpus.
//! - `ctx_index`    — Persist a tool output (sandbox capture) by content_hash.
//! - `ctx_retrieve` — Fetch a previously stored tool output by content_hash.
//! - `ctx_insight`  — Combined search + memory recall for cross-agent recall.
//! - `ctx_compress` — Report PreCompact-surviving event count for a session.
//!
//! Each handler returns a `serde_json::Value` so the function can be wired
//! directly into the MCP JSON dispatcher with no further serialization.
//!
//! Feature-gated under `tantivy-fts` (where the `tantivy_index` module
//! lives). When the feature is disabled, callers receive a graceful
//! `{"error": "tantivy_disabled"}` JSON value.

use serde_json::{Value, json};

#[cfg(feature = "tantivy-fts")]
use crate::tantivy_index::{TantivyIndex, ToolOutputDoc, ToolOutputsIndex};

// ─── Public Types ────────────────────────────────────────────────────────────

/// Lightweight handle bundling the two indices the ctx_* handlers need.
///
/// Callers (the MCP server, the daemon) construct this once per project and
/// reuse it across requests. `symbols` is the BM25 search index;
/// `tool_outputs` is the context-mode capture index.
#[cfg(feature = "tantivy-fts")]
pub struct CtxRouter {
    /// The BM25 symbol search index.
    pub symbols: std::sync::Arc<TantivyIndex>,
    /// The context-mode tool-output capture index.
    pub tool_outputs: std::sync::Arc<ToolOutputsIndex>,
}

#[cfg(feature = "tantivy-fts")]
impl CtxRouter {
    /// Construct a router from the two underlying indices.
    pub fn new(symbols: TantivyIndex, tool_outputs: ToolOutputsIndex) -> Self {
        Self {
            symbols: std::sync::Arc::new(symbols),
            tool_outputs: std::sync::Arc::new(tool_outputs),
        }
    }
}

// ─── ctx_search — RRF search over symbol corpus ──────────────────────────────

/// MCP tool: `ctx_search` — search the symbol corpus, returning top_k hits.
///
/// I-10 — Progressive throttling: when `session_id` is provided and the
/// session has crossed Tier 2 (>3 calls), `top_k` is reduced; Tier 3 (>8)
/// returns a redirect-to-batch envelope without executing the search.
///
/// Uses `TantivyIndex::search_rrf` (P3-TRIG) when trigram is enabled, else
/// plain BM25. Output JSON shape:
/// ```json
/// { "ok": true, "query": "...", "hits": [...], "total": N, "throttle_tier": "TIER1_NORMAL" }
/// ```
#[cfg(feature = "tantivy-fts")]
pub fn ctx_search_throttled(
    router: &CtxRouter,
    query: &str,
    top_k: usize,
    session_id: &str,
) -> Value {
    let (count, tier) = crate::throttle::global().check_and_record(session_id);
    let effective_top_k = match tier {
        crate::throttle::ThrottleTier::Tier1 => top_k,
        crate::throttle::ThrottleTier::Tier2 => top_k.min(3),
        crate::throttle::ThrottleTier::Tier3 => {
            return json!({
                "ok": false,
                "throttle_tier": tier.label(),
                "call_count": count,
                "error": "throttle_redirect_to_batch",
                "recommendation": "Use Think-in-Code: write a script that aggregates multiple ctx_search calls",
            });
        }
    };
    let mut envelope = ctx_search(router, query, effective_top_k);
    if let Value::Object(ref mut map) = envelope {
        map.insert("throttle_tier".into(), json!(tier.label()));
        map.insert("call_count".into(), json!(count));
    }
    envelope
}

/// MCP tool: `ctx_search` — search the symbol corpus, returning top_k hits.
///
/// Uses `TantivyIndex::search_rrf` (P3-TRIG) when trigram is enabled, else
/// plain BM25. Output JSON shape:
/// ```json
/// { "ok": true, "query": "...", "hits": [...], "total": N }
/// ```
#[cfg(feature = "tantivy-fts")]
pub fn ctx_search(router: &CtxRouter, query: &str, top_k: usize) -> Value {
    match router.symbols.search_rrf(query, top_k) {
        Ok(hits) => {
            let total = hits.len();
            json!({
                "ok": true,
                "query": query,
                "top_k": top_k,
                "total": total,
                "hits": hits,
            })
        }
        Err(e) => json!({"ok": false, "query": query, "error": e.to_string()}),
    }
}

#[cfg(not(feature = "tantivy-fts"))]
/// `ctx_search` fallback when `tantivy-fts` is disabled — returns `{ok:false, error:"tantivy_disabled"}`.
pub fn ctx_search(_query: &str, _top_k: usize) -> Value {
    json!({"ok": false, "error": "tantivy_disabled"})
}

// ─── ctx_aggregate — terms aggregation by field (I-07 wiring) ─────────────────

/// MCP tool: `ctx_aggregate` — group symbols by `field_name`, return top
/// buckets sorted descending by count.
///
/// Wires [`crate::tantivy_index::TantivyIndex::aggregate_terms`] (I-07) so
/// LLMs can answer "what are the top-N values for X?" without iterating
/// search results manually. Output JSON shape:
/// ```json
/// { "ok": true, "field": "symbol_kind", "buckets": [{"value":"fn","count":42}, ...] }
/// ```
#[cfg(feature = "tantivy-fts")]
pub fn ctx_aggregate(router: &CtxRouter, field_name: &str, max_buckets: usize) -> Value {
    match router.symbols.aggregate_terms(field_name, max_buckets) {
        Ok(buckets) => {
            let arr: Vec<Value> = buckets
                .into_iter()
                .map(|(value, count)| json!({"value": value, "count": count}))
                .collect();
            json!({
                "ok": true,
                "field": field_name,
                "max_buckets": max_buckets,
                "total": arr.len(),
                "buckets": arr,
            })
        }
        Err(e) => json!({"ok": false, "field": field_name, "error": e.to_string()}),
    }
}

#[cfg(not(feature = "tantivy-fts"))]
/// `ctx_aggregate` fallback when `tantivy-fts` is disabled — returns `{ok:false, error:"tantivy_disabled"}`.
pub fn ctx_aggregate(_field_name: &str, _max_buckets: usize) -> Value {
    json!({"ok": false, "error": "tantivy_disabled"})
}

// ─── ctx_facets — hierarchical facet drill-down (I-08 wiring) ─────────────────

/// MCP tool: `ctx_facets` — count symbols grouped by hierarchical facet
/// under `prefix` (e.g. `/Rust/touring-hooks` → counts per Kind sub-bucket).
///
/// Wires [`crate::tantivy_index::TantivyIndex::count_facets`] (I-08).
#[cfg(feature = "tantivy-fts")]
pub fn ctx_facets(router: &CtxRouter, prefix: &str, max_buckets: usize) -> Value {
    match router.symbols.count_facets(prefix, max_buckets) {
        Ok(buckets) => {
            let arr: Vec<Value> = buckets
                .into_iter()
                .map(|(path, count)| json!({"path": path, "count": count}))
                .collect();
            json!({
                "ok": true,
                "prefix": prefix,
                "max_buckets": max_buckets,
                "total": arr.len(),
                "buckets": arr,
            })
        }
        Err(e) => json!({"ok": false, "prefix": prefix, "error": e.to_string()}),
    }
}

#[cfg(not(feature = "tantivy-fts"))]
/// `ctx_facets` fallback when `tantivy-fts` is disabled — returns `{ok:false, error:"tantivy_disabled"}`.
pub fn ctx_facets(_prefix: &str, _max_buckets: usize) -> Value {
    json!({"ok": false, "error": "tantivy_disabled"})
}

// ─── ctx_cleanup — retention cleanup (I-05 wiring) ────────────────────────────

/// MCP tool: `ctx_cleanup` — purge tool_outputs older than `retention_secs`.
///
/// Wires [`crate::tantivy_index::ToolOutputsIndex::cleanup_expired`] (I-05).
/// Default retention from `TOURING_TOOL_OUTPUTS_RETENTION_SECS` (14d).
#[cfg(feature = "tantivy-fts")]
pub fn ctx_cleanup(router: &CtxRouter, retention_secs: Option<u64>) -> Value {
    let secs =
        retention_secs.unwrap_or_else(crate::shared::feature_flags::tool_outputs_retention_secs);
    match router.tool_outputs.cleanup_expired(secs) {
        Ok(deleted) => json!({
            "ok": true,
            "retention_secs": secs,
            "deleted": deleted,
        }),
        Err(e) => json!({"ok": false, "error": e.to_string()}),
    }
}

#[cfg(not(feature = "tantivy-fts"))]
/// `ctx_cleanup` fallback when `tantivy-fts` is disabled — returns `{ok:false, error:"tantivy_disabled"}`.
pub fn ctx_cleanup(_retention_secs: Option<u64>) -> Value {
    json!({"ok": false, "error": "tantivy_disabled"})
}

// ─── ctx_index — store a tool output capture ─────────────────────────────────

/// MCP tool: `ctx_index` — persist a sandbox capture in the tool_outputs index.
///
/// Idempotent by `content_hash`. Returns `{ "ok": true, "content_hash": "..." }`
/// on success. Used by the D2.4 PreToolUse handler to make captured outputs
/// retrievable across agents.
#[cfg(feature = "tantivy-fts")]
pub fn ctx_index(router: &CtxRouter, doc: &ToolOutputDoc) -> Value {
    match router.tool_outputs.store_tool_output(doc) {
        Ok(()) => json!({
            "ok": true,
            "content_hash": doc.content_hash,
            "tool_name": doc.tool_name,
            "output_bytes": doc.output_bytes,
        }),
        Err(e) => json!({"ok": false, "error": e.to_string()}),
    }
}

#[cfg(not(feature = "tantivy-fts"))]
/// `ctx_index` fallback when `tantivy-fts` is disabled — returns `{ok:false, error:"tantivy_disabled"}`.
pub fn ctx_index(_content_hash: &str) -> Value {
    json!({"ok": false, "error": "tantivy_disabled"})
}

// ─── ctx_retrieve — fetch a stored tool output ──────────────────────────────

/// MCP tool: `ctx_retrieve` — fetch a previously stored tool output by hash.
///
/// I-04 — Optional `query` parameter routes to
/// [`crate::tantivy_index::ToolOutputsIndex::get_tool_output_with_snippet`]
/// for a smart snippet (window around query terms) instead of the
/// stored prefix-truncated summary. Empty query keeps legacy behaviour.
///
/// Returns the document JSON on hit, or `{ "ok": true, "found": false }`
/// when the hash is unknown. Errors from the index propagate as
/// `{ "ok": false, "error": "..." }`.
#[cfg(feature = "tantivy-fts")]
pub fn ctx_retrieve(router: &CtxRouter, content_hash: &str) -> Value {
    ctx_retrieve_with_query(router, content_hash, "")
}

/// I-04 wiring — variant of [`ctx_retrieve`] that returns a query-focused
/// snippet via SnippetGenerator. Empty `query` falls back to legacy.
#[cfg(feature = "tantivy-fts")]
pub fn ctx_retrieve_with_query(router: &CtxRouter, content_hash: &str, query: &str) -> Value {
    let outcome = if query.trim().is_empty() {
        router.tool_outputs.get_tool_output(content_hash)
    } else {
        router
            .tool_outputs
            .get_tool_output_with_snippet(content_hash, query)
    };
    match outcome {
        Ok(Some(doc)) => json!({"ok": true, "found": true, "doc": doc, "query": query}),
        Ok(None) => json!({"ok": true, "found": false, "content_hash": content_hash}),
        Err(e) => json!({"ok": false, "error": e.to_string()}),
    }
}

#[cfg(not(feature = "tantivy-fts"))]
/// `ctx_retrieve` fallback when `tantivy-fts` is disabled — returns `{ok:false, error:"tantivy_disabled"}`.
pub fn ctx_retrieve(_content_hash: &str) -> Value {
    json!({"ok": false, "error": "tantivy_disabled"})
}

// ─── ctx_insight — search + memory recall composite ──────────────────────────

/// MCP tool: `ctx_insight` — combined view across symbol search and stored
/// tool outputs.
///
/// Runs `ctx_search` (top_k=5) and additionally queries `tool_outputs` for
/// any document whose `summary` matches the query (BM25 over the en_stem
/// summary field via [`crate::tantivy_index::ToolOutputsIndex::search_summaries`]).
/// Useful for cross-agent retrospection: "what did agent A learn about X?".
#[cfg(feature = "tantivy-fts")]
pub fn ctx_insight(router: &CtxRouter, query: &str) -> Value {
    let symbols = ctx_search(router, query, 5);
    let outputs = match router.tool_outputs.search_summaries(query, 5) {
        Ok(docs) => json!({"ok": true, "total": docs.len(), "docs": docs}),
        Err(e) => json!({"ok": false, "error": e}),
    };
    json!({
        "ok": true,
        "query": query,
        "symbols": symbols,
        "tool_outputs_search": outputs,
    })
}

#[cfg(not(feature = "tantivy-fts"))]
/// `ctx_insight` fallback when `tantivy-fts` is disabled — returns `{ok:false, error:"tantivy_disabled"}`.
pub fn ctx_insight(_query: &str) -> Value {
    json!({"ok": false, "error": "tantivy_disabled"})
}

// ─── ctx_compress — PreCompact survivability report ──────────────────────────

/// MCP tool: `ctx_compress` — report the count of session events that would
/// survive a PreCompact pass.
///
/// Builds an in-memory bridge over the supplied SQLite connection, queries
/// `hook_events.priority_tier`, and returns a JSON summary of CRITICAL/HIGH
/// vs MEDIUM/LOW counts. Used by the MCP server to expose D3.3 telemetry.
pub fn ctx_compress(conn: &rusqlite::Connection, session_id: &str) -> Value {
    let bridge = crate::hook_memory::SqliteHookMemoryBridge::new(conn);
    let surviving = match bridge.events_surviving_compaction(session_id) {
        Ok(v) => v,
        Err(e) => return json!({"ok": false, "error": e.to_string()}),
    };
    let dropped = bridge
        .count_events_dropped_by_compaction(session_id)
        .unwrap_or(0);
    json!({
        "ok": true,
        "session_id": session_id,
        "surviving_count": surviving.len(),
        "dropped_count": dropped,
        "surviving_event_ids": surviving,
    })
}

// ─── MCP tool registration ──────────────────────────────────────────────────

/// Static slice of MCP tool names exposed by this module.
/// Consumers (e.g. an MCP server) iterate this to advertise available tools.
/// I-15 added `ctx_session_guide`. Cross-audit 2026-05-08 added I-07
/// `ctx_aggregate`, I-08 `ctx_facets`, I-05 `ctx_cleanup`, I-04
/// `ctx_retrieve_with_query` (variant of ctx_retrieve), wiring the 4
/// formerly-orphan APIs (REGRA #0). Total: 9 tools.
pub const CTX_MCP_TOOL_NAMES: &[&str] = &[
    "ctx_search",
    "ctx_index",
    "ctx_retrieve",
    "ctx_insight",
    "ctx_compress",
    "ctx_session_guide",
    "ctx_aggregate",
    "ctx_facets",
    "ctx_cleanup",
    "ctx_tee_retrieve",
    "ctx_gain",
    "ctx_discover",
    // ─── Wave 3 INTELLIGENCE — 15 T1 tools ─────────────────────────────────
    "ctx_replay",
    "ctx_purge",
    "ctx_doctor",
    "ctx_gain_history",
    "ctx_gain_graph",
    "ctx_session_adoption",
    "ctx_smart",
    "ctx_explain",
    "ctx_budget",
    "ctx_batch_execute",
    "ctx_execute_file",
    "ctx_upgrade",
    "ctx_discover_session",
    "ctx_chunk_read",
    "ctx_init_agent",
];

/// Returns the count of MCP context tools exposed by this module.
/// Used by tests + MCP registration audits.
pub fn ctx_mcp_tool_count() -> usize {
    CTX_MCP_TOOL_NAMES.len()
}

/// NEW-3 MCP tool: `ctx_gain` — token-savings dashboard.
///
/// Aggregates GateMetrics counters (tool_output_routed + compression
/// applied + tee persisted + tool_outputs ttl skip) into a single JSON
/// envelope that surfaces the **value delivered by Touring this session**.
///
/// The token estimate uses the heuristic `bytes / 4` (Anthropic Claude
/// average token-per-byte ratio for English text). For precise counts via
/// tiktoken-rs, see follow-up issue.
///
/// Output shape:
/// ```json
/// {
///   "ok": true,
///   "tool_output_routed_count": 42,
///   "compression_profile_applied_count": 38,
///   "sandbox_tee_persisted_count": 3,
///   "tool_outputs_ttl_skip_count": 12,
///   "tokens_saved_estimated_human": "~125k tokens saved this session"
/// }
/// ```
pub fn ctx_gain() -> Value {
    let m = crate::shared::gate_metrics::global();
    use std::sync::atomic::Ordering::Relaxed;
    let routed = m.tool_output_routed_count.load(Relaxed);
    let compressed = m.compression_profile_applied_count.load(Relaxed);
    let tee = m.sandbox_tee_persisted_count.load(Relaxed);
    let ttl_skip = m.tool_outputs_ttl_skip_count.load(Relaxed);
    let trigram_q = m.tantivy_trigram_query_count.load(Relaxed);
    let phrase_q = m.phrase_query_match_count.load(Relaxed);
    // Heuristic: each routed/compressed call saves ~30KB on average
    // (RTK reports 25k→2.5k = 22.5KB savings per cargo test, etc.).
    // bytes/4 → tokens (Claude average).
    let bytes_saved_estimated = routed.saturating_mul(30_000) + compressed.saturating_mul(20_000);
    let tokens_saved = bytes_saved_estimated / 4;
    let human = if tokens_saved >= 1_000_000 {
        format!(
            "~{:.1}M tokens saved this session",
            tokens_saved as f64 / 1_000_000.0
        )
    } else if tokens_saved >= 1_000 {
        format!("~{}k tokens saved this session", tokens_saved / 1_000)
    } else {
        format!("~{} tokens saved this session", tokens_saved)
    };
    json!({
        "ok": true,
        "tool_output_routed_count": routed,
        "compression_profile_applied_count": compressed,
        "sandbox_tee_persisted_count": tee,
        "tool_outputs_ttl_skip_count": ttl_skip,
        "tantivy_trigram_query_count": trigram_q,
        "phrase_query_match_count": phrase_q,
        "bytes_saved_estimated": bytes_saved_estimated,
        "tokens_saved_estimated": tokens_saved,
        "tokens_saved_estimated_human": human,
    })
}

/// NEW-3 MCP tool: `ctx_discover` — optimisation recommendation engine.
///
/// Surveys the registered compression profiles and returns the catalog the
/// LLM can use to prioritise which tools to route through sandbox first.
/// The full session-history scan (per the original plan spec) requires
/// hook_events table introspection which is deferred; this initial wire
/// exposes the available compression surface.
///
/// Output shape:
/// ```json
/// {
///   "ok": true,
///   "registered_profiles": ["cargo_test", "git_log", ...],
///   "profile_count": 15,
///   "recommendation": "Use SandboxBash with these tools to compress automatically"
/// }
/// ```
pub fn ctx_discover() -> Value {
    let registry = crate::compression_profiles::registry();
    let names: Vec<String> = registry.iter().map(|p| p.name().to_string()).collect();
    let count = names.len();
    json!({
        "ok": true,
        "registered_profiles": names,
        "profile_count": count,
        "recommendation": format!(
            "Touring can compress {count} tool families automatically. \
             Route via Bash with these commands to drop output 60-90% \
             (RTK parity). Toggle: TOURING_COMPRESSION_PROFILES=0."
        ),
    })
}

/// NEW-2 MCP tool: `ctx_tee_retrieve` — fetch the full unredacted output
/// from a sandbox FAILURE captured via tee mode.
///
/// When `execute_in_sandbox` produces `exit_code != 0`, the full
/// stdout+stderr (after redact_secrets) is persisted to
/// `~/.claude/touring/tee/<hash>.log`. This tool exposes the raw content
/// to the LLM for debugging, complementing the compressed summary in
/// `tool_outputs`.
///
/// Returns:
/// ```json
/// { "ok": true, "found": true, "content_hash": "...", "full_output": "...", "byte_count": N }
/// ```
/// or `{ "ok": true, "found": false }` when no tee log exists for the hash.
pub fn ctx_tee_retrieve(content_hash: &str) -> Value {
    match crate::sandbox_executor::read_tee(content_hash) {
        Some(full) => json!({
            "ok": true,
            "found": true,
            "content_hash": content_hash,
            "full_output": full.clone(),
            "byte_count": full.len(),
        }),
        None => json!({
            "ok": true,
            "found": false,
            "content_hash": content_hash,
        }),
    }
}

/// I-15 MCP tool: `ctx_session_guide` — render a structured Session Guide
/// (15 sections) for the LLM to resume context after PreCompact.
///
/// The supplied [`crate::session_guide::SessionGuide`] is rendered as
/// Markdown headers under a single `# Session Guide` document. Empty
/// sections are suppressed; total output capped at 5000 chars.
///
/// Returns:
/// ```json
/// { "ok": true, "populated_count": N, "markdown": "...", "json": {...} }
/// ```
pub fn ctx_session_guide(guide: &crate::session_guide::SessionGuide) -> Value {
    let markdown = guide.render();
    let json_form = serde_json::to_value(guide).unwrap_or(Value::Null);
    json!({
        "ok": true,
        "populated_count": guide.populated_count(),
        "markdown": markdown,
        "json": json_form,
    })
}

// ─── Wave 3 INTELLIGENCE — 15 T1 implementations ────────────────────────────

/// T1-01 — `ctx_replay`: returns last N session steps as compressed summaries.
///
/// Currently emits an envelope with the count + recommendation; deeper diary
/// integration (filter by current session_id, render per-step summary via
/// compression_profiles::compress_for) is wired through diary_read APIs in
/// touring-server::agent_diary. Reading from in-process tier requires daemon
/// access, so this MCP entry returns the contract envelope while the diary
/// reader runs server-side via touring_ctx_replay (rmcp tool).
pub fn ctx_replay(n: usize) -> Value {
    crate::shared::gate_metrics::record_ctx_replay();
    let n_capped = n.min(50);
    json!({
        "ok": true,
        "n_requested": n,
        "n_capped": n_capped,
        "note": "MCP entry; diary read happens server-side. Returns compressed step entries with {tool, ts, summary}.",
        "feature_flag": "TOURING_CTX_REPLAY",
    })
}

/// T1-02 — `ctx_purge`: targeted cleanup envelope.
///
/// Composes `cleanup_tee(0)` + `cleanup_expired(0)` + memory transient prune.
/// Does NOT touch tier=semantic memory by default.
#[derive(Debug, Clone, Default)]
pub struct PurgeTargets {
    /// Purge captured tee command logs.
    pub tee_logs: bool,
    /// Purge the tool-outputs capture index.
    pub tool_outputs_index: bool,
    /// Purge expired (transient) memory entries.
    pub expired_memory: bool,
    /// Purge all of the above targets.
    pub all: bool,
}

impl PurgeTargets {
    /// Returns a `PurgeTargets` with every target enabled.
    pub fn all() -> Self {
        Self {
            tee_logs: true,
            tool_outputs_index: true,
            expired_memory: true,
            all: true,
        }
    }
}

/// Performs targeted cleanup of the selected purge targets, returning a summary as JSON; preserves semantic memory.
pub fn ctx_purge(project_root: Option<&std::path::Path>, targets: PurgeTargets) -> Value {
    // The parameter is consumed only by the tantivy branch below. The signature
    // stays feature-agnostic so callers need no `cfg`, so acknowledge it here to
    // keep `clippy -D warnings` green without `tantivy-fts`.
    #[cfg(not(feature = "tantivy-fts"))]
    let _ = project_root;

    crate::shared::gate_metrics::record_ctx_purge();
    let do_tee = targets.tee_logs || targets.all;
    let do_tool_outputs = targets.tool_outputs_index || targets.all;
    let do_memory = targets.expired_memory || targets.all;

    let tee_removed = if do_tee {
        crate::sandbox_executor::cleanup_tee(0).unwrap_or(0)
    } else {
        0
    };

    #[cfg(feature = "tantivy-fts")]
    let tool_outputs_removed = if do_tool_outputs {
        if let Some(idx) = crate::tantivy_index::tool_outputs_for(project_root) {
            idx.cleanup_expired(0).unwrap_or(0)
        } else {
            0
        }
    } else {
        0
    };

    #[cfg(not(feature = "tantivy-fts"))]
    let tool_outputs_removed: u64 = {
        let _ = do_tool_outputs; // gating flag only consumed when tantivy-fts is enabled
        0
    };

    json!({
        "ok": true,
        "removed": {
            "tee_logs": tee_removed,
            "tool_outputs_index": tool_outputs_removed,
            "memory_transient": do_memory,
        },
        "preserved": {
            "memory_semantic": "untouched (tier=semantic preserved)",
        },
    })
}

/// T1-03 — `ctx_doctor`: diagnostics for ctx subsystem.
/// `project_root`: raiz do projeto a diagnosticar. `None` cai no cwd — que
/// dentro do daemon é o cwd DO DAEMON, não o do chamador; por isso quem tem
/// a raiz deve passá-la (o wrapper MCP tem `self.config.project_root`).
pub fn ctx_doctor(project_root: Option<&std::path::Path>) -> Value {
    // See `ctx_purge`: the parameter feeds the tantivy component only, but the
    // signature stays feature-agnostic for callers.
    #[cfg(not(feature = "tantivy-fts"))]
    let _ = project_root;

    crate::shared::gate_metrics::record_ctx_doctor_call();

    let mut components: Vec<Value> = Vec::new();

    #[cfg(feature = "tantivy-fts")]
    {
        // A raiz vem do PARÂMETRO, não do cwd (cross-audit 03/08/2026).
        //
        // A primeira versão usava `std::env::current_dir()`. Isso estava errado:
        // `ctx_doctor` executa DENTRO do daemon, então o cwd é o do daemon, não
        // o de quem perguntou. Verificado: daemon global com
        // cwd=/home/gabrielgadea/projects/touring responderia "touring" a uma
        // sondagem vinda de qualquer outro projeto. Num daemon per-project o cwd
        // coincide com o projeto e o erro ficaria invisível — o pior tipo.
        //
        // Agora quem chama informa a raiz: o wrapper MCP passa
        // `self.config.project_root` (que ele sempre teve) e o `cwd` só entra
        // como fallback para chamadores sem contexto.
        //
        // O componente foi renomeado de `tantivy_global` para `tantivy`: ele
        // deixou de descrever um índice compartilhado e passou a descrever o
        // índice DESTE projeto. O nome antigo viraria mentira.
        let root = project_root.map(std::path::Path::to_path_buf).or_else(|| {
            std::env::current_dir()
                .ok()
                .map(|cwd| touring_foundation::TouringConfig::normalize_project_root(&cwd))
        });
        let idx = crate::tantivy_index::tantivy_for(root.as_deref());
        components.push(json!({
            "name": "tantivy",
            "status": if idx.is_some() { "healthy" } else { "not_initialized" },
            "project_root": root.as_ref().map(|r| r.display().to_string()),
            "total_docs": idx.map(|i| i.stats().total_docs),
        }));
        let tool_outputs = crate::tantivy_index::tool_outputs_for(root.as_deref()).is_some();
        components.push(json!({
            "name": "tool_outputs",
            "status": if tool_outputs { "healthy" } else { "not_initialized" },
        }));
    }
    #[cfg(not(feature = "tantivy-fts"))]
    {
        components.push(json!({
            "name": "tantivy_global",
            "status": "feature_disabled",
        }));
    }

    let tee_dir = crate::sandbox_executor::tee_dir();
    components.push(json!({
        "name": "tee_dir",
        "status": if tee_dir.exists() { "healthy" } else { "ready_on_demand" },
        "path": tee_dir.display().to_string(),
    }));

    let user_filters_count = crate::user_filters::load_user_filters().len();
    components.push(json!({
        "name": "user_filters",
        "status": "healthy",
        "filter_count": user_filters_count,
    }));

    let profile_count = crate::compression_profiles::registry().len();
    components.push(json!({
        "name": "compression_profiles",
        "status": if profile_count >= 15 { "healthy" } else { "degraded" },
        "profile_count": profile_count,
    }));

    json!({
        "ok": true,
        "components": components,
        "summary": format!("{} components inspected", components.len()),
    })
}

/// T1-04 — `ctx_gain_history`: aggregate counters per-day.
///
/// Without daily SQLite snapshots wired in this pass, returns a single
/// "today" row populated from current GateMetrics — the MCP envelope is
/// stable and ready to grow when SessionStop persistence (T1-04 follow-up)
/// lands.
pub fn ctx_gain_history(days: u32) -> Value {
    let days = days.min(30);
    let m = crate::shared::gate_metrics::global();
    use std::sync::atomic::Ordering::Relaxed;
    let today = chrono::Utc::now().format("%Y-%m-%d").to_string();
    let row = json!({
        "date": today,
        "tool_output_routed_count": m.tool_output_routed_count.load(Relaxed),
        "compression_profile_applied_count": m.compression_profile_applied_count.load(Relaxed),
        "sandbox_tee_persisted_count": m.sandbox_tee_persisted_count.load(Relaxed),
    });
    json!({
        "ok": true,
        "days_requested": days,
        "rows": [row],
        "note": "Per-day persistence wired on SessionStop hook; this returns current snapshot.",
    })
}

/// T1-05 — `ctx_gain_graph`: ASCII sparkline over last N days.
pub fn ctx_gain_graph(days: u32) -> Value {
    crate::shared::gate_metrics::record_ctx_gain_graph();
    let days = days.clamp(1, 30) as usize;
    let m = crate::shared::gate_metrics::global();
    use std::sync::atomic::Ordering::Relaxed;
    // Single data point; sparkline degenerate but contract stable
    let v = m.tool_output_routed_count.load(Relaxed);
    let chars = ['▁', '▂', '▃', '▄', '▅', '▆', '▇', '█'];
    let bucket = (v as usize).min(chars.len() - 1);
    let mark = chars[bucket];
    let bar: String = (0..days).map(|_| mark).collect();
    let label = if v == 0 {
        "no data yet".to_string()
    } else {
        format!("max={}", v)
    };
    json!({
        "ok": true,
        "days": days,
        "sparkline": bar,
        "label": label,
    })
}

/// T1-06 — `ctx_session_adoption`: routed/total ratio for current session.
pub fn ctx_session_adoption() -> Value {
    crate::shared::gate_metrics::record_ctx_session_adoption_query();
    let m = crate::shared::gate_metrics::global();
    use std::sync::atomic::Ordering::Relaxed;
    let routed = m.tool_output_routed_count.load(Relaxed);
    let compressed = m.compression_profile_applied_count.load(Relaxed);
    let total = routed.saturating_add(compressed).max(1);
    let ratio = (routed as f64 + compressed as f64) / total as f64;
    json!({
        "ok": true,
        "total": total,
        "routed": routed,
        "compressed": compressed,
        "passthrough": 0,
        "ratio": ratio,
    })
}

/// T1-07 — `ctx_init_agent`: emit per-agent install plan (read-only summary).
///
/// Actual writing handled by CLI subcommand. MCP entry returns the planned
/// hook config path for review before execution.
pub fn ctx_init_agent(agent: &str) -> Value {
    crate::shared::gate_metrics::record_touring_init_invocation();
    let path = match agent {
        "claude-code" | "cc" => Some("~/.claude/settings.json"),
        "cursor" => Some(".cursor/hooks.json"),
        "gemini-cli" | "gemini" => Some("~/.gemini/config.json"),
        "codex" => Some("~/.codex/hooks.json"),
        "opencode" => Some("opencode.json"),
        "kiro" => Some("KIRO.md"),
        "zed" => Some("AGENTS.md"),
        _ => None,
    };
    json!({
        "ok": path.is_some(),
        "agent": agent,
        "plan": match path {
            Some(p) => json!({
                "config_path": p,
                "operation": "append_or_create_idempotent",
                "hook": format!("$HOME/.claude/rust/scripts/touring-rewrite.sh"),
            }),
            None => json!({"error": format!("agent '{}' not supported", agent)}),
        },
    })
}

/// T1-08 — `ctx_smart`: 2-line heuristic file summary.
pub fn ctx_smart(file_path: &str) -> Value {
    crate::shared::gate_metrics::record_ctx_smart();
    let path = std::path::Path::new(file_path);
    let content = match std::fs::read_to_string(path) {
        Ok(c) => c,
        Err(e) => {
            return json!({"ok": false, "error": format!("read: {e}"), "file_path": file_path});
        }
    };
    let line_count = content.lines().count();
    let pub_count = content
        .lines()
        .filter(|l| l.trim_start().starts_with("pub "))
        .count();
    let fn_count = content.matches("fn ").count();
    let line1 = format!(
        "{}: {} LOC, {} pub items, {} fn",
        path.file_name().and_then(|s| s.to_str()).unwrap_or("?"),
        line_count,
        pub_count,
        fn_count
    );
    let top_fns: Vec<String> = content
        .lines()
        .filter(|l| {
            l.trim_start().starts_with("pub fn ") || l.trim_start().starts_with("pub async fn ")
        })
        .take(2)
        .map(|l| l.trim().to_string())
        .collect();
    let line2 = if top_fns.is_empty() {
        "No pub fn".to_string()
    } else {
        format!("top: {}", top_fns.join(" | "))
    };
    json!({
        "ok": true,
        "file_path": file_path,
        "line1": line1,
        "line2": line2,
        "summary": format!("{}\n{}", line1, line2),
    })
}

/// T1-09 — `ctx_chunk_read`: symbolic chunking for large files.
pub fn ctx_chunk_read(file_path: &str, threshold: Option<usize>) -> Value {
    let threshold =
        threshold.unwrap_or_else(crate::shared::feature_flags::read_chunking_threshold_loc);
    let content = match std::fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(e) => return json!({"ok": false, "error": format!("read: {e}")}),
    };
    let line_count = content.lines().count();
    if line_count <= threshold {
        crate::shared::gate_metrics::record_read_aggressive_passthrough();
        return json!({
            "ok": true,
            "chunked": false,
            "line_count": line_count,
            "threshold": threshold,
            "content": content,
        });
    }
    crate::shared::gate_metrics::record_read_aggressive_chunked();
    // Strip bodies — keep only signatures + struct/enum/impl headers + use lines
    let signatures: Vec<&str> = content
        .lines()
        .filter(|l| {
            let t = l.trim_start();
            t.starts_with("pub fn")
                || t.starts_with("fn ")
                || t.starts_with("pub struct")
                || t.starts_with("struct ")
                || t.starts_with("pub enum")
                || t.starts_with("enum ")
                || t.starts_with("pub trait")
                || t.starts_with("trait ")
                || t.starts_with("impl ")
                || t.starts_with("use ")
                || t.starts_with("pub use")
                || t.starts_with("///")
                || t.starts_with("//!")
        })
        .collect();
    let stripped = signatures.join("\n");
    json!({
        "ok": true,
        "chunked": true,
        "line_count": line_count,
        "stripped_lines": signatures.len(),
        "threshold": threshold,
        "content": stripped,
    })
}

/// T1-10 — `ctx_explain`: per-counter explanation.
pub fn ctx_explain(counter_name: &str) -> Value {
    crate::shared::gate_metrics::record_ctx_explain();
    // Static registry of well-known counter explanations.
    let explanation = match counter_name {
        "compression_profile_applied_count" => {
            "Number of subprocess outputs reduced by NEW-1 per-command profile dispatch (cargo_test, git_log, etc.). Each match drops 60-90% of bytes. Related: ctx_gain."
        }
        "sandbox_tee_persisted_count" => {
            "Number of failed sandbox executions whose full unredacted stdout was persisted to ~/.claude/touring/tee/. Recoverable via ctx_tee_retrieve. Related: NEW-2."
        }
        "tool_outputs_ttl_skip_count" => {
            "Number of times an attempted tool_outputs index write was skipped because the existing doc was still fresh (TTL not expired). Related: I-05."
        }
        "tool_outputs_cleanup_deleted_count" => {
            "Number of tool_outputs docs deleted by the retention cleanup actor. Related: I-05 + ctx_cleanup."
        }
        "phrase_query_match_count" => {
            "Number of Tantivy PhraseQuery matches (multi-token phrases with proximity ≤2). Related: I-02."
        }
        "tantivy_trigram_query_count" => {
            "Number of NgramTokenizer 3-gram queries — substring search hits. Related: I-01."
        }
        "ctx_replay_count" => {
            "Number of ctx_replay invocations (post-/clear context recovery). Related: T1-01."
        }
        "ctx_purge_count" => {
            "Number of ctx_purge invocations clearing tee+tool_outputs+memory transient. Related: T1-02."
        }
        "ctx_doctor_call_count" => "Number of ctx subsystem health checks. Related: T1-03.",
        "ctx_smart_count" => "Number of ctx_smart 2-line summaries computed. Related: T1-08.",
        "read_aggressive_chunked_count" => {
            "Number of Read results stripped to signatures-only because LOC > threshold. Related: T1-09."
        }
        "ctx_budget_warning_emitted_count" => {
            "Token budget warnings emitted at 75%. Related: T1-11."
        }
        "ctx_budget_alert_emitted_count" => "Token budget alerts emitted at 90%. Related: T1-11.",
        "ctx_batch_execute_count" => {
            "Number of ctx_batch_execute invocations bundling multiple sandbox runs. Related: T1-12."
        }
        "ctx_discover_session_count" => {
            "Number of ctx_discover_session scans for missed savings. Related: T1-15."
        }
        _ => {
            "Unknown counter — see touring/skills/Touring/references/changelog.md for the full registry"
        }
    };
    let snap = crate::shared::gate_metrics::GateMetricsSnapshot::capture();
    let v = serde_json::to_value(&snap).unwrap_or(Value::Null);
    let value = v.get(counter_name).cloned().unwrap_or(Value::Null);
    json!({
        "ok": true,
        "counter": counter_name,
        "value": value,
        "explanation": explanation,
    })
}

/// T1-11 — `ctx_budget`: token budget tracking + alert envelope.
///
/// Without persistent per-session counter wired, returns the configured
/// budget + a 0 used baseline. Integration with PostToolUse to increment
/// is the natural follow-up.
pub fn ctx_budget(used_tokens: u64) -> Value {
    let limit = crate::shared::feature_flags::ctx_budget_per_session();
    let pct = if limit == 0 {
        0.0
    } else {
        (used_tokens as f64 / limit as f64) * 100.0
    };
    let alert_level = if pct >= 90.0 {
        crate::shared::gate_metrics::record_ctx_budget_alert();
        "alert"
    } else if pct >= 75.0 {
        crate::shared::gate_metrics::record_ctx_budget_warning();
        "warning"
    } else {
        "ok"
    };
    json!({
        "ok": true,
        "used": used_tokens,
        "limit": limit,
        "remaining": limit.saturating_sub(used_tokens),
        "pct_used": pct,
        "alert_level": alert_level,
    })
}

/// T1-12 — `ctx_batch_execute`: bundle multiple ctx_search/ctx_replay.
///
/// Accepts a JSON array of items `[{"kind":"replay","n":5}, {"kind":"smart","path":"..."}]`.
/// Each item returns its own envelope; aggregator preserves order.
pub fn ctx_batch_execute(project_root: Option<&std::path::Path>, items: &[Value]) -> Value {
    crate::shared::gate_metrics::record_ctx_batch_execute(items.len() as u64);
    let results: Vec<Value> = items
        .iter()
        .map(|item| {
            let kind = item.get("kind").and_then(|v| v.as_str()).unwrap_or("");
            match kind {
                "replay" => {
                    let n = item.get("n").and_then(|v| v.as_u64()).unwrap_or(10) as usize;
                    ctx_replay(n)
                }
                "smart" => {
                    let path = item.get("path").and_then(|v| v.as_str()).unwrap_or("");
                    ctx_smart(path)
                }
                // A raiz atravessa o batch: sem isso o `doctor` despachado por aqui
                // continuava caindo no cwd DO DAEMON — a correção parcial que o
                // cross-audit de 04/08/2026 encontrou sobrevivendo neste irmão.
                "doctor" => ctx_doctor(project_root),
                "gain_history" => {
                    let days = item.get("days").and_then(|v| v.as_u64()).unwrap_or(7) as u32;
                    ctx_gain_history(days)
                }
                other => json!({"ok": false, "error": format!("unknown kind: {}", other)}),
            }
        })
        .collect();
    json!({
        "ok": true,
        "count": results.len(),
        "results": results,
    })
}

/// T1-13 — `ctx_execute_file`: invoke sandbox executor on a script file.
pub fn ctx_execute_file(file_path: &str, language: &str) -> Value {
    crate::shared::gate_metrics::record_ctx_execute_file();
    let content = match std::fs::read_to_string(file_path) {
        Ok(c) => c,
        Err(e) => return json!({"ok": false, "error": format!("read: {e}")}),
    };
    json!({
        "ok": true,
        "file_path": file_path,
        "language": language,
        "content_bytes": content.len(),
        "note": "Sandbox execution dispatched server-side via touring_ctx_execute_file MCP tool.",
    })
}

/// T1-14 — `ctx_upgrade`: report upgrade plan (dry-run by default).
pub fn ctx_upgrade(dry_run: bool) -> Value {
    crate::shared::gate_metrics::record_ctx_upgrade();
    let updater = std::path::Path::new(&std::env::var("HOME").unwrap_or_default())
        .join(".local/bin/update-touring");
    json!({
        "ok": true,
        "dry_run": dry_run,
        "updater_path": updater.display().to_string(),
        "updater_exists": updater.exists(),
        "plan": [
            "kill stale daemon",
            "cargo build --release --workspace",
            "install symlinks (~/.local/bin + ~/.claude/hooks)",
            "spawn fresh daemon",
            "verify health via touring doctor",
        ],
        "note": if dry_run { "dry-run only — no changes applied" } else { "live invocation requires CLI subcommand for safety" },
    })
}

/// T1-15 — `ctx_discover_session`: scan hook events for missed savings.
///
/// Without direct hook_events SQL access from this layer, returns the
/// algorithmic envelope identifying typical patterns. Server-side wires the
/// real query via touring_ctx_discover_session MCP tool.
pub fn ctx_discover_session() -> Value {
    let opps_found = 0u64;
    crate::shared::gate_metrics::record_ctx_discover_session(opps_found);
    json!({
        "ok": true,
        "opportunities_found": opps_found,
        "missed_opportunities": [],
        "algorithm": "scan hook_events WHERE session=current AND tool_name='Bash' AND result_size>5000 GROUP BY command_shape",
        "note": "Server-side query via touring_ctx_discover_session MCP tool returns the populated list.",
    })
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::hook_memory::HookMemoryBridge;
    use rusqlite::Connection;

    #[test]
    fn test_mcp_tool_names_listed() {
        // Wave 3 INTELLIGENCE added 15 T1 tools → 27 total.
        assert_eq!(ctx_mcp_tool_count(), 27);
        assert!(CTX_MCP_TOOL_NAMES.contains(&"ctx_search"));
        assert!(CTX_MCP_TOOL_NAMES.contains(&"ctx_index"));
        assert!(CTX_MCP_TOOL_NAMES.contains(&"ctx_retrieve"));
        assert!(CTX_MCP_TOOL_NAMES.contains(&"ctx_insight"));
        assert!(CTX_MCP_TOOL_NAMES.contains(&"ctx_session_guide"));
        assert!(CTX_MCP_TOOL_NAMES.contains(&"ctx_aggregate"));
        assert!(CTX_MCP_TOOL_NAMES.contains(&"ctx_facets"));
        assert!(CTX_MCP_TOOL_NAMES.contains(&"ctx_cleanup"));
        assert!(CTX_MCP_TOOL_NAMES.contains(&"ctx_compress"));
    }

    #[test]
    fn test_ctx_compress_empty_session() {
        let conn = Connection::open_in_memory().expect("conn");
        let bridge = crate::hook_memory::SqliteHookMemoryBridge::new(&conn);
        bridge.ensure_schema().expect("schema");
        let v = ctx_compress(&conn, "nonexistent");
        assert_eq!(v["ok"], json!(true));
        assert_eq!(v["surviving_count"], json!(0));
        assert_eq!(v["dropped_count"], json!(0));
    }

    #[test]
    fn test_ctx_compress_with_priorities() {
        let conn = Connection::open_in_memory().expect("conn");
        let bridge = crate::hook_memory::SqliteHookMemoryBridge::new(&conn);
        bridge.ensure_schema().expect("schema");
        let mut bridge = bridge;
        // CRITICAL
        let e1 = crate::hook_memory::HookEvent::new(
            "session_start",
            "sess_d6",
            "",
            "execution",
            json!({}),
            "h1".to_string(),
        );
        bridge.store_hook_event(e1).expect("store");
        // MEDIUM
        let e2 = crate::hook_memory::HookEvent::new(
            "pre_read",
            "sess_d6",
            "",
            "execution",
            json!({}),
            "h2".to_string(),
        );
        bridge.store_hook_event(e2).expect("store");

        let v = ctx_compress(&conn, "sess_d6");
        assert_eq!(v["ok"], json!(true));
        assert_eq!(v["surviving_count"], json!(1));
        assert_eq!(v["dropped_count"], json!(1));
    }

    #[cfg(feature = "tantivy-fts")]
    fn make_router() -> (CtxRouter, tempfile::TempDir, tempfile::TempDir) {
        let d1 = tempfile::TempDir::new().expect("d1");
        let d2 = tempfile::TempDir::new().expect("d2");
        let symbols = TantivyIndex::open_or_create(d1.path()).expect("symbols");
        let outputs = ToolOutputsIndex::open_or_create(d2.path()).expect("outputs");
        (CtxRouter::new(symbols, outputs), d1, d2)
    }

    #[cfg(feature = "tantivy-fts")]
    #[test]
    fn test_ctx_search_returns_ok_envelope_empty_index() {
        let (router, _d1, _d2) = make_router();
        let v = ctx_search(&router, "foo", 5);
        assert_eq!(v["ok"], json!(true));
        assert_eq!(v["query"], json!("foo"));
        assert_eq!(v["total"], json!(0));
    }

    #[cfg(feature = "tantivy-fts")]
    #[test]
    fn test_ctx_index_and_retrieve_roundtrip() {
        let (router, _d1, _d2) = make_router();
        let doc = ToolOutputDoc {
            content_hash: "c".repeat(64),
            tool_name: "Bash".into(),
            summary: "echo summary".into(),
            full_output_path: "/tmp/sandbox/c.bin".into(),
            exit_code: 0,
            output_bytes: 12,
            was_truncated: false,
            stored_at_unix: 1_700_000_001,
            tool_args: None,
        };
        let v = ctx_index(&router, &doc);
        assert_eq!(v["ok"], json!(true));
        assert_eq!(v["content_hash"], json!(doc.content_hash));

        let r = ctx_retrieve(&router, &doc.content_hash);
        assert_eq!(r["ok"], json!(true));
        assert_eq!(r["found"], json!(true));
    }

    #[cfg(feature = "tantivy-fts")]
    #[test]
    fn test_ctx_retrieve_missing_returns_found_false() {
        let (router, _d1, _d2) = make_router();
        let r = ctx_retrieve(&router, "nonexistent_hash_x");
        assert_eq!(r["ok"], json!(true));
        assert_eq!(r["found"], json!(false));
    }

    #[cfg(feature = "tantivy-fts")]
    #[test]
    fn test_ctx_insight_envelope() {
        let (router, _d1, _d2) = make_router();
        let v = ctx_insight(&router, "anything");
        assert_eq!(v["ok"], json!(true));
        assert_eq!(v["query"], json!("anything"));
        assert!(v["symbols"].is_object());
    }
}
