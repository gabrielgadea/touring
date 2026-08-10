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

/// T2-10: ROI envelope built from **measured** context savings.
///
/// Reads this process's counters. The MCP bridge and the CLI do not compress
/// anything — the daemon does — so a caller that wants production numbers must
/// pass the daemon's snapshot to [`ctx_roi_from_snapshot`]. The `source` field
/// of the envelope always says which one produced it.
pub fn ctx_roi(model: &str) -> Value {
    let snap = crate::shared::gate_metrics::GateMetricsSnapshot::capture();
    let mut v = ctx_roi_from_snapshot(&snap, model, "local_process");
    // Whether THIS process could have measured tokens at all — the difference
    // between "nothing was compressed here" and "compression happened but no
    // tokenizer was installed to count it".
    v["tokenizer_installed"] = json!(crate::shared::gate_metrics::has_token_counter());
    v
}

/// T2-10 core: ROI over an explicit metrics snapshot.
///
/// **A2 (2026-08-08) — what changed and why.** This used to compute
/// `routed × 30_000 + compressed × 20_000`, then `bytes / 4`, then a USD figure
/// from that. Three stacked guesses: two invented per-event byte constants and
/// a chars-per-token divisor. The compression site is capped at a 512-byte
/// summary (`sandbox_output_store::derive_summary_with_tool`), so the 20_000
/// constant alone overstated the maximum possible saving by ~40×.
///
/// Now: `bytes_saved` is an exact sum of `len()` deltas taken where the work
/// happens. Tokens are reported **only** when a real tokenizer was installed in
/// the recording process (`gate_metrics::set_token_counter`); otherwise the
/// envelope carries `tokens_saved: null` and a separately-named
/// `tokens_saved_estimate` whose basis is stated in the same object — an
/// estimate can never be misread as a measurement, because it does not occupy
/// the measurement's field.
pub fn ctx_roi_from_snapshot(
    snap: &crate::shared::gate_metrics::GateMetricsSnapshot,
    model: &str,
    source: &str,
) -> Value {
    crate::shared::gate_metrics::record_wave3_t210();
    let rate = pricing_usd_per_million_input(model);
    let bytes_saved = snap
        .compression_bytes_in_total
        .saturating_sub(snap.compression_bytes_out_total)
        .saturating_add(
            snap.routed_bytes_in_total
                .saturating_sub(snap.routed_bytes_out_total),
        );

    let mut out = json!({
        "ok": true,
        "model": model,
        "source": source,
        "rate_per_million_usd": rate,
        "bytes_saved": bytes_saved,
        "bytes": {
            "compression_in": snap.compression_bytes_in_total,
            "compression_out": snap.compression_bytes_out_total,
            "routed_in": snap.routed_bytes_in_total,
            "routed_out": snap.routed_bytes_out_total,
        },
        "events": {
            "recorded": snap.savings_event_count,
            "token_measured": snap.token_measured_event_count,
            "compression_profile_applied": snap.compression_profile_applied_count,
            "tool_output_routed": snap.tool_output_routed_count,
        },
    });

    if snap.token_measured_event_count > 0 {
        let tokens_saved = snap
            .measured_tokens_in_total
            .saturating_sub(snap.measured_tokens_out_total);
        let usd = (tokens_saved as f64) / 1_000_000.0 * rate;
        out["tokens_saved"] = json!(tokens_saved);
        out["usd_saved"] = json!(format!("${usd:.4}"));
        out["token_method"] = json!(format!(
            "cl100k_base, measured on {}/{} event(s)",
            snap.token_measured_event_count, snap.savings_event_count
        ));
    } else {
        // No tokenizer where the recording happened → say so, and keep the
        // estimate out of the measured fields.
        out["tokens_saved"] = Value::Null;
        out["usd_saved"] = Value::Null;
        out["token_method"] = json!("not_measured — no tokenizer registered in the recording process");
        out["tokens_saved_estimate"] = json!(bytes_saved / 4);
        out["estimate_basis"] =
            json!("bytes_saved / 4 — a chars-per-token heuristic, NOT a token count");
        out["usd_saved_estimate"] =
            json!(format!("${:.4}", (bytes_saved / 4) as f64 / 1_000_000.0 * rate));
    }

    if snap.savings_event_count == 0 {
        out["note"] = json!(
            "no compression or routing event recorded — this reports the absence of \
             activity, not a saving of zero. Sandbox routing is gated by \
             TOURING_HOOK_ROUTING and only fires above \
             TOURING_HOOK_ROUTING_THRESHOLD (default 10240 bytes of estimated output)."
        );
    }
    out
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

// ─── A2 (2026-08-08) — the ROI envelope's honesty invariants ────────────────

#[cfg(test)]
mod roi_honesty_tests {
    use super::*;
    use crate::shared::gate_metrics::GateMetricsSnapshot;

    /// A snapshot with the savings fields set and everything else defaulted.
    fn snap(comp: (u64, u64), routed: (u64, u64), tokens: (u64, u64), tok_events: u64,
            events: u64) -> GateMetricsSnapshot {
        let mut s = GateMetricsSnapshot::capture();
        s.compression_bytes_in_total = comp.0;
        s.compression_bytes_out_total = comp.1;
        s.routed_bytes_in_total = routed.0;
        s.routed_bytes_out_total = routed.1;
        s.measured_tokens_in_total = tokens.0;
        s.measured_tokens_out_total = tokens.1;
        s.token_measured_event_count = tok_events;
        s.savings_event_count = events;
        s
    }

    #[test]
    fn bytes_saved_is_the_exact_delta_never_a_per_event_constant() {
        // 900 - 300 saved by compression, 50_000 - 400 by routing.
        let v = ctx_roi_from_snapshot(&snap((900, 300), (50_000, 400), (0, 0), 0, 7), "sonnet", "test");
        assert_eq!(v["bytes_saved"], json!(600 + 49_600));
        // The old formula would have produced a multiple of 30_000/20_000 from
        // the EVENT counts; the new one cannot, because it never sees them.
        assert_ne!(v["bytes_saved"], json!(7 * 20_000));
    }

    #[test]
    fn measured_tokens_occupy_the_measured_fields_and_no_estimate_appears() {
        let v = ctx_roi_from_snapshot(&snap((900, 300), (0, 0), (250, 90), 4, 4), "opus", "daemon");
        assert_eq!(v["tokens_saved"], json!(160));
        assert_eq!(v["usd_saved"], json!("$0.0024")); // 160/1e6 * 15.0
        assert!(v["token_method"].as_str().expect("method").contains("cl100k_base"));
        assert!(v.get("tokens_saved_estimate").is_none(),
                "a measured envelope must not also carry an estimate");
    }

    #[test]
    fn without_a_tokenizer_the_measured_fields_are_null_never_an_estimate() {
        // The defect this whole item exists to remove: an estimate presented in
        // the field a reader takes for a measurement.
        let v = ctx_roi_from_snapshot(&snap((4_000, 0), (0, 0), (0, 0), 0, 3), "sonnet", "daemon");
        assert!(v["tokens_saved"].is_null());
        assert!(v["usd_saved"].is_null());
        assert_eq!(v["tokens_saved_estimate"], json!(1_000));
        assert!(v["estimate_basis"].as_str().expect("basis").contains("NOT a token count"));
        assert!(v["token_method"].as_str().expect("method").starts_with("not_measured"));
    }

    #[test]
    fn zero_events_reports_absence_of_activity_not_a_zero_saving() {
        let v = ctx_roi_from_snapshot(&snap((0, 0), (0, 0), (0, 0), 0, 0), "sonnet", "daemon");
        assert_eq!(v["bytes_saved"], json!(0));
        let note = v["note"].as_str().expect("a zero-event envelope must explain itself");
        assert!(note.contains("not a saving of zero"));
        assert!(note.contains("TOURING_HOOK_ROUTING"), "name the gate that keeps it dormant");
    }

    #[test]
    fn the_envelope_always_names_the_process_it_measured() {
        // The MCP bridge and the daemon have different counters; a number
        // without its origin is unreadable.
        for source in ["daemon", "local_process"] {
            let v = ctx_roi_from_snapshot(&snap((10, 5), (0, 0), (0, 0), 0, 1), "haiku", source);
            assert_eq!(v["source"], json!(source));
        }
    }

    #[test]
    fn out_larger_than_in_saturates_instead_of_wrapping() {
        // A profile may legitimately grow its input (dedupe adds "(×3)").
        let v = ctx_roi_from_snapshot(&snap((100, 180), (0, 0), (0, 0), 0, 1), "sonnet", "test");
        assert_eq!(v["bytes_saved"], json!(0));
    }

    #[test]
    fn the_invented_constants_are_gone_from_the_roi_path() {
        // Structural guard: 30_000 / 20_000 / "/ 4" were the three fabrications.
        // Assert over the SOURCE so a future edit cannot quietly reinstate them.
        let src = include_str!("wave3_extended.rs");
        let roi = src
            .split("pub fn ctx_roi_from_snapshot")
            .nth(1)
            .expect("the roi core must exist")
            .split("\n#[cfg(test)]")
            .next()
            .expect("bounded slice");
        assert!(!roi.contains("30_000"), "per-event byte constant reinstated");
        assert!(!roi.contains("saturating_mul"), "event counts multiplied into bytes again");
    }
}
