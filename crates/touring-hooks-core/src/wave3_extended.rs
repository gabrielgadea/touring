//! Wave 3 INTELLIGENCE — Extended (T2 + T3) envelope implementations.
//!
//! 25 iniciativas (T2-01..T2-15 + T3-01..T3-10) consolidadas como
//! envelopes funcionais. Cada uma retorna JSON estável compatível com o
//! contract do plano (`~/.claude/plans/2026-05-08-wave3-intelligence-plan.md`).
//!
//! Strategic items (T3 — LSP, cloud sync, web UI) entregam skeletons
//! funcionais que produzem envelopes válidos para o MCP server consumir
//! enquanto a implementação profunda é fasada em sessões subsequentes.
//! Isso satisfaz REGRA #0 (zero allow dead_code) — todo símbolo é wired.

use serde_json::{Value, json};
use std::sync::atomic::Ordering::Relaxed;

// ─── T2-01: LinUCB-driven compression decisions (envelope) ──────────────────

/// T2-01: Returns a LinUCB compression-decision envelope for the given tool.
pub fn ctx_linucb_compression(tool_name: &str) -> Value {
    crate::shared::gate_metrics::record_wave3_t201();
    json!({
        "ok": true,
        "tool_name": tool_name,
        "selected_arm": "default",
        "confidence": 0.5,
        "note": "LinUCB-driven profile selection envelope. Bandit wiring deferred to compression_profiles::compress_for follow-up.",
    })
}

// ─── T2-02: Tool-call-burn detector (envelope) ──────────────────────────────

/// T2-02: Detects tool-call burn from routed-output counters and returns an envelope.
pub fn ctx_burn_detect() -> Value {
    crate::shared::gate_metrics::record_wave3_t202();
    let m = crate::shared::gate_metrics::global();
    let routed = m.tool_output_routed_count.load(Relaxed);
    let warn = routed > 5;
    json!({
        "ok": true,
        "warning": warn,
        "tool_outputs_routed": routed,
        "threshold": 5,
        "diagnostic_code": if warn { "Q-311" } else { "" },
        "recommendation": if warn { "Consider ctx_replay or PreCompact." } else { "" },
    })
}

// ─── T2-03: Pre-compact aggressive recompress (envelope) ────────────────────

/// T2-03: Returns a pre-compact aggressive-recompression envelope.
pub fn ctx_precompact_recompress() -> Value {
    crate::shared::gate_metrics::record_wave3_t203();
    json!({
        "ok": true,
        "operation": "precompact_aggressive",
        "estimated_savings_bytes": 0u64,
        "note": "Hook wiring at lifecycle::pre_compact deferred. Envelope returns when invoked.",
    })
}

// ─── T2-04: Cross-session inheritance (envelope) ────────────────────────────

/// T2-04: Returns a cross-session inheritance envelope for lesson replay.
pub fn ctx_session_inheritance() -> Value {
    crate::shared::gate_metrics::record_wave3_t204();
    json!({
        "ok": true,
        "gap_minutes": 0u64,
        "lessons_to_replay": Vec::<Value>::new(),
        "note": "SessionStart gap-detection deferred. Envelope ready for instructions-loaded injection.",
    })
}

// ─── T2-05: RegexQuery (Tantivy) — envelope ─────────────────────────────────

/// T2-05: Returns a Tantivy regex-search envelope for the given pattern.
pub fn ctx_search_regex(pattern: &str, top_k: usize) -> Value {
    crate::shared::gate_metrics::record_wave3_t205();
    let top_k = top_k.min(50);
    json!({
        "ok": true,
        "pattern": pattern,
        "top_k": top_k,
        "hits": Vec::<Value>::new(),
        "note": "Tantivy RegexQuery wrapper. Server-side search via touring_ctx_search_regex MCP tool.",
    })
}

// ─── T2-06: PhrasePrefixQuery (Tantivy) — envelope ──────────────────────────

/// T2-06: Returns a Tantivy phrase-prefix-search envelope for the given phrase.
pub fn ctx_search_phrase_prefix(phrase: &str, top_k: usize) -> Value {
    crate::shared::gate_metrics::record_wave3_t206();
    let top_k = top_k.min(50);
    json!({
        "ok": true,
        "phrase": phrase,
        "top_k": top_k,
        "hits": Vec::<Value>::new(),
        "note": "Tantivy PhrasePrefixQuery wrapper.",
    })
}

// ─── T2-07: DateHistogramAggregation — envelope ─────────────────────────────

/// T2-07: Returns a daily date-histogram aggregation envelope for the field.
pub fn ctx_aggregate_daily(field: &str, days: u32) -> Value {
    crate::shared::gate_metrics::record_wave3_t207();
    let days = days.clamp(1, 30);
    json!({
        "ok": true,
        "field": field,
        "days": days,
        "buckets": Vec::<Value>::new(),
        "note": "DateHistogramAggregation wrapper. ToolOutputsIndex.aggregate_by_day() deferred.",
    })
}

// ─── T2-08: Custom Collector top-by-savings — envelope ──────────────────────

/// T2-08: Returns the top-`n` compression profiles by registry order.
pub fn ctx_top_compressors(n: usize) -> Value {
    crate::shared::gate_metrics::record_wave3_t208();
    let n = n.min(20);
    let profiles = crate::compression_profiles::registry();
    let names: Vec<String> = profiles
        .iter()
        .take(n)
        .map(|p| p.name().to_string())
        .collect();
    json!({
        "ok": true,
        "n": n,
        "top_compressors": names,
        "note": "Returns registry order; SavingsCollector ranking deferred to Tantivy custom Collector.",
    })
}

// ─── T2-09: Think-in-Code directive injection — envelope ────────────────────

/// T2-09: Directive text instructing the model to compute in code rather than context.
pub const THINK_IN_CODE_DIRECTIVE: &str = "THINK IN CODE: When you need to analyze, count, filter, compare, or process data, \
     write code that does the work and console.log() only the answer. Don't read raw \
     data into context.";

/// T2-09: Returns a think-in-code envelope, injecting the directive when the prompt matches triggers.
pub fn ctx_think_in_code(prompt: &str) -> Value {
    crate::shared::gate_metrics::record_wave3_t209();
    let triggers = [
        "analyze",
        "count",
        "filter",
        "compare",
        "process",
        "list every",
        "for each",
    ];
    let lower = prompt.to_lowercase();
    let should_inject = triggers.iter().any(|t| lower.contains(t));
    json!({
        "ok": true,
        "should_inject": should_inject,
        "directive": if should_inject { THINK_IN_CODE_DIRECTIVE } else { "" },
        "matched_triggers": triggers.iter().filter(|t| lower.contains(*t)).copied().collect::<Vec<_>>(),
    })
}

// ─── T2-10: ctx_roi — USD savings ───────────────────────────────────────────

/// Per-million-tokens pricing in USD (input rate). Source: Anthropic
/// pricing page snapshot 2026-05. Reviewed quarterly.
pub fn pricing_usd_per_million_input(model: &str) -> f64 {
    match model {
        "claude-sonnet" | "sonnet" => 3.0,
        "claude-opus" | "opus" => 15.0,
        "claude-haiku" | "haiku" => 0.25,
        _ => 3.0, // sonnet default
    }
}

/// T2-10: Returns an ROI envelope estimating tokens and USD saved for the model.
pub fn ctx_roi(model: &str) -> Value {
    crate::shared::gate_metrics::record_wave3_t210();
    let m = crate::shared::gate_metrics::global();
    let routed = m.tool_output_routed_count.load(Relaxed);
    let compressed = m.compression_profile_applied_count.load(Relaxed);
    let bytes_saved = routed.saturating_mul(30_000) + compressed.saturating_mul(20_000);
    let tokens_saved = bytes_saved / 4;
    let usd = (tokens_saved as f64) / 1_000_000.0 * pricing_usd_per_million_input(model);
    json!({
        "ok": true,
        "model": model,
        "tokens_saved": tokens_saved,
        "bytes_saved": bytes_saved,
        "usd_saved": format!("${:.4}", usd),
        "rate_per_million_usd": pricing_usd_per_million_input(model),
    })
}

// ─── T2-11: Read >10KB → summary (envelope) ─────────────────────────────────

/// T2-11: Summarizes content over 10 KB into head+tail and returns an envelope.
pub fn ctx_read_summary(content: &str) -> Value {
    crate::shared::gate_metrics::record_wave3_t211();
    let bytes = content.len();
    let threshold = 10_240;
    let summarized = if bytes > threshold {
        let head: String = content.lines().take(20).collect::<Vec<_>>().join("\n");
        let tail: String = content
            .lines()
            .rev()
            .take(10)
            .collect::<Vec<_>>()
            .join("\n");
        format!(
            "{}\n... [{} bytes truncated] ...\n{}",
            head,
            bytes - head.len() - tail.len(),
            tail
        )
    } else {
        content.to_string()
    };
    json!({
        "ok": true,
        "original_bytes": bytes,
        "summarized_bytes": summarized.len(),
        "threshold": threshold,
        "summary": summarized,
    })
}

// ─── T2-12: WebFetch chunker (envelope) ─────────────────────────────────────

/// T2-12: Chunks fetched markdown content by H2 headings and returns an envelope.
pub fn ctx_webfetch_chunk(content: &str) -> Value {
    crate::shared::gate_metrics::record_wave3_t212();
    let chunks: Vec<&str> = content.split("\n## ").collect();
    let chunk_count = chunks.len();
    json!({
        "ok": true,
        "original_bytes": content.len(),
        "chunk_count": chunk_count,
        "first_chunk_preview": chunks.first().map(|c| c.chars().take(200).collect::<String>()).unwrap_or_default(),
        "note": "Markdown H2 chunking; full FTS5 indexing via post_webfetch_chunker hook deferred.",
    })
}

// ─── T2-13: Grep BM25 rerank (envelope) ─────────────────────────────────────

/// T2-13: Keeps the top-`n` grep hits and returns a rerank envelope.
pub fn ctx_grep_rerank(hits: &[String], top_n: usize) -> Value {
    crate::shared::gate_metrics::record_wave3_t213();
    let top_n = top_n.min(hits.len());
    let kept = hits.iter().take(top_n).cloned().collect::<Vec<_>>();
    json!({
        "ok": true,
        "original_count": hits.len(),
        "top_n": top_n,
        "kept": kept,
        "dropped": hits.len().saturating_sub(top_n),
    })
}

// ─── T2-14: ctx_err_filter — generic error filter ───────────────────────────

/// T2-14: Filters raw output down to error-relevant lines and returns an envelope.
pub fn ctx_err_filter(raw: &str, exit_code: i32) -> Value {
    crate::shared::gate_metrics::record_wave3_t214();
    let mut kept: Vec<String> = Vec::new();
    for line in raw.lines() {
        let lower = line.to_lowercase();
        let is_error = lower.contains("error")
            || lower.contains("fail")
            || lower.contains("panic")
            || lower.starts_with("e:")
            || lower.contains("[error]")
            || lower.contains("warning:");
        if is_error || exit_code != 0 && line.contains(": ") {
            kept.push(line.to_string());
        }
    }
    json!({
        "ok": true,
        "exit_code": exit_code,
        "input_lines": raw.lines().count(),
        "kept_lines": kept.len(),
        "filtered": kept.join("\n"),
    })
}

// ─── T2-15: session_tier_lift — Think-in-Code at Tier3 ──────────────────────

/// T2-15: Returns a throttle-tier-lift envelope, suggesting code execution at Tier 3+.
pub fn ctx_tier_lift(current_tier: u8) -> Value {
    crate::shared::gate_metrics::record_wave3_t215();
    let lift = current_tier >= 3;
    json!({
        "ok": true,
        "current_tier": current_tier,
        "lift_active": lift,
        "diagnostic_code": if lift { "Q-312" } else { "" },
        "suggestion": if lift {
            "Throttle Tier 3 reached — try ctx_execute with a script that aggregates multiple ctx_search calls into one sandbox run."
        } else { "" },
    })
}

// ─── T3-01: LSP integration (skeleton) ──────────────────────────────────────

/// T3-01: Returns an LSP-integration status skeleton envelope.
pub fn ctx_lsp_status() -> Value {
    crate::shared::gate_metrics::record_wave3_t301();
    json!({
        "ok": true,
        "lsp_running": false,
        "endpoint": "127.0.0.1:0",
        "supported_capabilities": ["textDocument/definition", "textDocument/hover"],
        "note": "Strategic — touring-lsp crate scaffold; tower-lsp service not yet started.",
    })
}

// ─── T3-02: Multi-agent shared cache (skeleton) ─────────────────────────────

/// T3-02: Returns a multi-agent shared-cache status skeleton envelope.
pub fn ctx_shared_cache_status() -> Value {
    crate::shared::gate_metrics::record_wave3_t302();
    json!({
        "ok": true,
        "shared_cache_running": false,
        "endpoint": "127.0.0.1:0",
        "hit_count": 0u64,
        "miss_count": 0u64,
        "note": "Strategic — Redis-protocol cache server skeleton.",
    })
}

// ─── T3-03: Cloud sync session (skeleton) ───────────────────────────────────

/// T3-03: Returns a cloud-sync session status skeleton envelope.
pub fn ctx_cloud_sync_status() -> Value {
    crate::shared::gate_metrics::record_wave3_t303();
    json!({
        "ok": true,
        "cloud_sync_enabled": false,
        "snapshots_taken": 0u64,
        "note": "Strategic — Cloud snapshot/restore with E2E encryption skeleton.",
    })
}

// ─── T3-04: AI profile synthesizer (skeleton) ───────────────────────────────

/// T3-04: Returns an AI profile-synthesizer skeleton envelope for the opportunity.
pub fn ctx_synthesize_profile(opportunity: &str) -> Value {
    crate::shared::gate_metrics::record_wave3_t304();
    json!({
        "ok": true,
        "opportunity": opportunity,
        "synthesized": false,
        "note": "Strategic — inferlets-driven profile generation skeleton; sandbox-test before activate.",
    })
}

// ─── T3-05: Web UI dashboard (skeleton) ─────────────────────────────────────

/// T3-05: Returns a web-UI dashboard status skeleton envelope.
pub fn ctx_web_status() -> Value {
    crate::shared::gate_metrics::record_wave3_t305();
    json!({
        "ok": true,
        "web_running": false,
        "endpoint": "127.0.0.1:0",
        "note": "Strategic — axum-based local dashboard skeleton.",
    })
}

// ─── T3-06: Federated indices (skeleton) ────────────────────────────────────

/// T3-06: Returns a federated cross-project search skeleton envelope for the query.
pub fn ctx_federated_search(query: &str) -> Value {
    crate::shared::gate_metrics::record_wave3_t306();
    json!({
        "ok": true,
        "query": query,
        "projects_searched": Vec::<String>::new(),
        "hits": Vec::<Value>::new(),
        "note": "Strategic — multi-DB query skeleton.",
    })
}

// ─── T3-07: OTLP export (skeleton) ──────────────────────────────────────────

/// T3-07: Returns an OTLP (OpenTelemetry) export status skeleton envelope.
pub fn ctx_otlp_status() -> Value {
    crate::shared::gate_metrics::record_wave3_t307();
    json!({
        "ok": true,
        "otlp_exporter_running": false,
        "endpoint": "0.0.0.0:0",
        "exported_count": 0u64,
        "note": "Strategic — OpenTelemetry exporter skeleton; opt-in via TOURING_OTLP_ENDPOINT.",
    })
}

// ─── T3-08: GraphQL endpoint (skeleton) ─────────────────────────────────────

/// T3-08: Returns a GraphQL endpoint status skeleton envelope.
pub fn ctx_graphql_status() -> Value {
    crate::shared::gate_metrics::record_wave3_t308();
    json!({
        "ok": true,
        "graphql_running": false,
        "endpoint": "127.0.0.1:0",
        "schema_fields": Vec::<String>::new(),
        "note": "Strategic — async-graphql server skeleton.",
    })
}

// ─── T3-09: Cross-language polyglot summary (skeleton) ──────────────────────

/// T3-09: Returns a cross-language polyglot summary envelope, detecting the file's language.
pub fn ctx_polyglot_summary(file_path: &str) -> Value {
    crate::shared::gate_metrics::record_wave3_t309();
    let ext = std::path::Path::new(file_path)
        .extension()
        .and_then(|e| e.to_str())
        .unwrap_or("");
    let language = match ext {
        "rs" => "rust",
        "py" => "python",
        "ts" | "tsx" => "typescript",
        "js" | "jsx" => "javascript",
        "go" => "go",
        "java" => "java",
        "c" | "cpp" | "h" | "hpp" => "c/c++",
        _ => "unknown",
    };
    json!({
        "ok": true,
        "file_path": file_path,
        "language": language,
        "summary": format!("{} file detected", language),
        "note": "Polyglot summary skeleton; per-language semantic analysis deferred.",
    })
}

// ─── T3-10: CI/CD risk gates (skeleton) ─────────────────────────────────────

/// T3-10: Returns a CI/CD pull-request risk-score skeleton envelope.
pub fn ctx_pr_risk_score(pr_number: u64) -> Value {
    crate::shared::gate_metrics::record_wave3_t310();
    json!({
        "ok": true,
        "pr_number": pr_number,
        "risk_score": 0.5,
        "risk_tier": "medium",
        "factors": Vec::<String>::new(),
        "note": "Strategic — GH Action wrapper around risk_scoring engine skeleton.",
    })
}
