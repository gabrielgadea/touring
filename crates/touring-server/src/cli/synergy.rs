//! `touring synergy` — Cross-subsystem wiring observability.
//!
//! Wave 8 S6 (Synergy Maximization, 2026-04-26).
//!
//! Reports which Touring subsystems are wired together vs operating in
//! isolation, and lists known synergy opportunities (deferred integrations
//! awaiting drivers).
//!
//! # Output
//!
//! JSON: `{ "wired_pairs": [...], "synergy_opportunities": [...], "version": "..." }`
//!
//! Human: tabular text rendering of both lists with section headers.
//!
//! Wave P3-1.3 W5b (2026-06-11): migrated from manual `has_flag` + `args.get(2)`
//! parsing to clap derive. Dispatch contract unchanged — `run` receives full argv
//! slice; clap parses `args[1..]`. All logic (enrich_pairs_with_metrics, render_human,
//! etc.) is preserved verbatim (G6).

use super::daemon_query;
use clap::{Parser, Subcommand};

/// Static catalogue of cross-subsystem integrations currently active.
///
/// Each tuple is `(producer, consumer, wave, description)`. The version
/// number records the wave that introduced the wiring — useful for
/// archaeology and rollback decisions.
const WIRED_PAIRS: &[(&str, &str, &str, &str)] = &[
    (
        "pre_edit",
        "TDG (touring-analysis)",
        "v4.12.0 S1",
        "Wave S1 — TDG grade signal in compose_quality_evolution",
    ),
    (
        "pre_edit",
        "Q-201/Q-202 (RFC-100)",
        "v4.20.0 S5",
        "Wave 8 S5 — emit Q-2xx via tracing on D/F grade",
    ),
    (
        "pre_edit",
        "memory_recall hint",
        "v4.5.0 W14",
        "Wave W14 — streak hint Signal 13",
    ),
    (
        "pre_edit",
        "blast_radius cache",
        "v4.6.0 D2",
        "Wave D2 — predictive blast injection",
    ),
    (
        "post_edit",
        "health_delta compute",
        "v4.5.0 W10",
        "Wave W10 — compute deltas + RL reward",
    ),
    (
        "post_edit",
        "RL reward",
        "v4.5.0 W19",
        "Wave W19 — generator pipeline integration",
    ),
    (
        "post_edit",
        "query_cache invalidate",
        "v4.5.0 W18",
        "Wave W18 — path-scoped cache eviction",
    ),
    (
        "post_write",
        "query_cache invalidate",
        "v4.5.0 W18",
        "Wave W18 — path-scoped cache eviction",
    ),
    (
        "cli_decompose_create",
        "GranularityBandit advisory",
        "v4.15.0 G1",
        "Wave G1 — bandit_split_factor in response",
    ),
    (
        "cli_memory_recall",
        "MemoryFinding M-5xx",
        "v4.15.0 G3",
        "Wave G3 — RecallEmpty/TfidfActivated/RrfFusion diagnostics",
    ),
    (
        "cli_ast_blast",
        "BlastWarning B-300",
        "v4.16.0 T4",
        "Wave T4 — diagnostics field in JSON when blast > 10",
    ),
    (
        "cli_ast_blast",
        "termtree --tree render",
        "v4.16.0 T2",
        "Wave T2 — ASCII tree visualization",
    ),
    (
        "cli_wiring_audit",
        "termtree --tree render",
        "v4.16.0 T3",
        "Wave T3 — ASCII tree of orphans + cycles",
    ),
    (
        "cli_wiring_audit",
        "F2 cycle detection",
        "v4.12.0 S3",
        "Wave S3 — Tarjan SCC integration",
    ),
    (
        "cli_wiring_audit",
        "RFC-100 W-100/W-103",
        "v4.14.0 N2",
        "Wave N2 — diagnostics in audit output",
    ),
    (
        "cli_wiring_status",
        "hypergraph + multi_import",
        "v4.14.0 N7",
        "Wave N7 — pub orphan fns wired",
    ),
    (
        "cli_session_summary",
        "health_delta per-file",
        "v4.14.0 N5",
        "Wave N5 — streak surfaced in summary",
    ),
    (
        "instructions_loaded",
        "cognitive_runtime status",
        "v4.14.0 N6",
        "Wave N6 — predictor status in context",
    ),
    (
        "Diagnostic",
        "miette::Diagnostic trait",
        "v4.16.0 T1",
        "Wave T1 — miette bridge for fancy rendering",
    ),
    (
        "Diagnostic",
        "syntect source highlight",
        "v4.20.0 S1",
        "Wave 8 S1 — with_source_snippet + miette source_code",
    ),
    (
        "touring status",
        "composite_health_score",
        "v4.20.0 S3",
        "Wave 8 S3 — weighted average of 5 dimensions",
    ),
    (
        "Diagnostic",
        "source_snippet wiring (production)",
        "v4.21.0 S7",
        "Wave 9 S7 — try_attach_source_from_file + read_source_snippet helpers",
    ),
    (
        "cli_ast_blast",
        "source_snippet field",
        "v4.21.0 S7",
        "Wave 9 S7 — diagnostic JSON now carries miette-renderable snippet",
    ),
    (
        "cli_wiring_orphans",
        "source_snippet via WiringFinding",
        "v4.21.0 S7",
        "Wave 9 S7 — orphan diagnostics gain inline source",
    ),
    (
        "instructions_loaded",
        "composite_health_score warning",
        "v4.21.0 S8",
        "Wave 9 S8 — degraded health surfaced at session start",
    ),
    (
        "touring_foundation::health",
        "compute_composite_health_score (shared)",
        "v4.21.0 S8",
        "Wave 9 S8 — moved from cli/status.rs for cross-crate reuse",
    ),
    (
        "touring synergy",
        "gate_metrics counter enrichment",
        "v4.21.0 S9",
        "Wave 9 S9 — --with-metrics flag folds runtime activity into pairs",
    ),
    (
        "touring ast",
        "highlight subcommand",
        "v4.17.0 T3",
        "Wave T3 — syntect ANSI rendering",
    ),
    (
        "touring ast",
        "rust-semantic (syn)",
        "v4.4.0 W4.4",
        "Wave W4.4 — deep Rust semantics",
    ),
    (
        "touring ast",
        "format-rust (prettyplease)",
        "v4.4.0 W4.4",
        "Wave W4.4 — rustfmt-clean output",
    ),
    (
        "touring ast",
        "workspace-info (cargo_metadata)",
        "v4.4.0 W4.4",
        "Wave W4.4 — cross-crate dependents",
    ),
    (
        "touring ast",
        "ast-grep polyglot search",
        "—",
        "touring_code::polyglot integration",
    ),
    (
        "touring ast",
        "TDG grade A+..F",
        "v4.10.0 Q1",
        "Wave Q1 — TDG dimensional report",
    ),
    (
        "touring index find",
        "case_insensitive_find (StringZilla)",
        "v4.11.0 T3.1",
        "Wave T3.1 — SIMD --ignore-case flag",
    ),
    (
        "touring index find",
        "moka query_cache",
        "v4.5.0 W17",
        "Wave W17 — 60s TTL cache",
    ),
    (
        "touring tantivy search",
        "moka query_cache",
        "v4.5.0 W17",
        "Wave W17 — 60s TTL cache",
    ),
    (
        "touring tantivy search",
        "BM25 + cognitive_score rank",
        "—",
        "tantivy production wiring",
    ),
    (
        "daemon socket",
        "rkyv IPC dual-path",
        "v4.5.0 W3",
        "Wave W3 — peek-byte dispatch",
    ),
    (
        "daemon socket",
        "ACP protocol shim",
        "v4.9.0 F3",
        "Wave F3 — opt-in JSON-RPC layer",
    ),
    (
        "LinUCBBandit",
        "GPU UCB shader",
        "v4.6.0 D5",
        "Wave D5 — GPU offload",
    ),
    (
        "PheromoneMCTS",
        "GPU rollout shader",
        "v4.6.0 D4",
        "Wave D4 — GPU dispatch + rayon fallback",
    ),
    (
        "HookRuntime",
        "actor pattern (mpsc)",
        "v4.4.0 actor_refactor",
        "Wave actor refactor — anti-contention",
    ),
    (
        "touring-loom-proofs",
        "loom concurrency proofs",
        "v4.4.0 supply-chain",
        "Wave supply-chain — isolated crate",
    ),
    (
        "cli_mpatch_preview",
        "B-302 PatchExpansion (RFC-100)",
        "v4.24.0 W12",
        "Wave 12 — emit B-302 when mpatch fuzzy expand + confidence < 0.7; b302_diagnostic field in response JSON",
    ),
    (
        "pre_write::emit_b302_if_low_confidence_expansion",
        "PatchComplexityDelta::compute",
        "v4.24.0 W12",
        "Wave 12 — wires the previously-orphaned compute() into a real production diagnostic + gate_metrics counter",
    ),
    (
        "MemoryGuard (pressure ticker)",
        "pre_bash::run_returning",
        "v4.30.0 TRM-1",
        "Wave TRM-1 — W-540 gate: Red pressure pauses cargo/npm/pip in pre_bash; counter: cargo_test_paused_count",
    ),
    (
        "MemoryGuard::snapshot",
        "gate_metrics::record_pressure_tick",
        "v4.30.0 TRM-2",
        "Wave TRM-2 — PSI tick bridge: MemoryGuard ticker updates gate_metrics pressure counters; counter: memory_pressure_total_tick_count",
    ),
    (
        "Pressure (enum)",
        "compute_composite_health_score (status.rs)",
        "v4.30.0 TRM-3",
        "Wave TRM-3 — Red pressure degrades composite health score; counter: memory_pressure_red_count",
    ),
    (
        "CoreScheduler::pin_high_priority",
        "post_edit::run_returning",
        "v4.30.0 TRM-4",
        "Wave TRM-4 — P-core affinity pinning wired to post_edit quality gate; counter: core_pinning_p_count",
    ),
    (
        "MemoryGuard::snapshot",
        "touring status (resource_monitor key)",
        "v4.30.0 TRM-5",
        "Wave TRM-5 — resource_monitor key in touring status consumes MemoryGuard snapshot; counter: memory_pressure_yellow_count",
    ),
    (
        "activity_hook::emit_*",
        "activity.jsonl (EventStore)",
        "v8.0 D1.6",
        "Wave v8 D1.6 — ESAA pattern: 5 hooks emit HookFired events to activity.jsonl via EventStore append-only log",
    ),
    (
        "CEG gateway (X0..X9)",
        "cli_suggester enrichment",
        "v8.0 P6.4",
        "Wave P6.4 — Bash commands carrying executable code are advised to route through `touring exec` (CEG pipeline); workflow_advice_emitted_count counter reflects live activity",
    ),
];

/// Wave 9 S9 (2026-04-26) — mapping from `(producer, consumer)` to the
/// `gate_metrics` counter that reflects the integration's live activity.
///
/// Used by `enrich_pairs_with_metrics` to fold runtime activity into the
/// otherwise-static `WIRED_PAIRS` catalogue, turning `touring synergy`
/// into a true observability tool ("which integrations are firing?").
///
/// Pairs without a 1:1 counter are intentionally absent — their JSON
/// output simply omits the `metrics` field.
const WIRED_PAIR_METRICS: &[(&str, &str, &str)] = &[
    (
        "pre_edit",
        "TDG (touring-analysis)",
        "diagnostic_tdg_emitted_count",
    ),
    (
        "pre_edit",
        "Q-201/Q-202 (RFC-100)",
        "diagnostic_tdg_emitted_count",
    ),
    ("pre_edit", "blast_radius cache", "blast_inject_count"),
    (
        "post_edit",
        "health_delta compute",
        "health_delta_compute_count",
    ),
    (
        "post_edit",
        "query_cache invalidate",
        "query_cache_invalidate_count",
    ),
    (
        "post_write",
        "query_cache invalidate",
        "query_cache_invalidate_count",
    ),
    ("cli_ast_blast", "BlastWarning B-300", "blast_inject_count"),
    (
        "cli_wiring_audit",
        "RFC-100 W-100/W-103",
        "diagnostic_wiring_finding_emitted_count",
    ),
    (
        "touring index find",
        "moka query_cache",
        "query_cache_hit_count",
    ),
    (
        "touring tantivy search",
        "moka query_cache",
        "query_cache_hit_count",
    ),
    (
        "cli_mpatch_preview",
        "B-302 PatchExpansion (RFC-100)",
        "diagnostic_b302_emitted_count",
    ),
    (
        "MemoryGuard (pressure ticker)",
        "pre_bash::run_returning",
        "cargo_test_paused_count",
    ),
    (
        "MemoryGuard::snapshot",
        "gate_metrics::record_pressure_tick",
        "memory_pressure_total_tick_count",
    ),
    (
        "Pressure (enum)",
        "compute_composite_health_score (status.rs)",
        "memory_pressure_red_count",
    ),
    (
        "CoreScheduler::pin_high_priority",
        "post_edit::run_returning",
        "core_pinning_p_count",
    ),
    (
        "MemoryGuard::snapshot",
        "touring status (resource_monitor key)",
        "memory_pressure_yellow_count",
    ),
    (
        "activity_hook::emit_*",
        "activity.jsonl (EventStore)",
        "activity_hook_event_count",
    ),
    (
        "CEG gateway (X0..X9)",
        "cli_suggester enrichment",
        "workflow_advice_emitted_count",
    ),
];

/// Catalogue of known cross-subsystem integrations awaiting a driver.
///
/// Each entry documents an integration that has been DESIGNED but not
/// SHIPPED, with the rationale for the deferral. When a driver emerges
/// (concrete use case), promote the entry to `WIRED_PAIRS`.
const SYNERGY_OPPORTUNITIES: &[(&str, &str, &str)] = &[
    (
        "wiring impact transitive in pre_edit",
        "Signal 15 in compose_edit_context",
        "pre_edit already saturated at 14 signals; adding Signal 15 risks signal noise",
    ),
    (
        "MCTS rollout outcome → GranularityBandit reward",
        "Cross-update bandit from cognitive layer",
        "Requires tightening the actor-pattern boundary; current 200ms join window is conservative",
    ),
    (
        "diary entries → memory recall ingestion",
        "AAAK entries become FTS5-searchable",
        "Schema migration required; no current diary consumer needs cross-tier recall",
    ),
    (
        "plan-speculate → wiring chain advisory",
        "Generator emits would_break_chain hint",
        "Needs canonicalised chain diff representation; touring-generator typestate refactor first",
    ),
    (
        "Generic Trace<K> primitive (Accumulating, Dutch)",
        "Replace hardcoded Replacing in qtable.rs",
        "Wave 7 finding: zero use case driver for trace kind A/B test",
    ),
    (
        "FunctionApproximator trait abstraction",
        "Shared trait across MLP + tiny_transformer + Burn",
        "Wave 7 finding: 3 implementations work without shared interface; refactor cost > benefit",
    ),
    (
        "miette SourceCode ANSI colour rendering",
        "syntect-coloured snippets in fancy diagnostics",
        "Wave 9 S7 wired data path (production source_snippet attachment); colour rendering deferred to terminal-side miette consumer (caller composes Report::handler with GraphicalReportHandler)",
    ),
];

// ─────────────────────────────────────────────────────────────────────────────
// clap derive types (Wave P3-1.3 W5b)
// ─────────────────────────────────────────────────────────────────────────────

#[derive(Parser, Debug)]
#[command(
    name = "touring synergy",
    bin_name = "touring synergy",
    about = "Cross-subsystem wiring observability: report (default), wired, opportunities",
    disable_help_subcommand = true
)]
struct SynergyCli {
    #[command(subcommand)]
    cmd: Option<SynergyCmd>,
    /// Enrich each wired pair with its live gate_metrics counter value (Wave 9 S9).
    /// Degrades silently when the daemon is unreachable.
    #[arg(long, global = true)]
    with_metrics: bool,
    /// Output JSON instead of human-readable text.
    #[arg(short = 'j', long, global = true)]
    json: bool,
}

#[derive(Subcommand, Debug)]
enum SynergyCmd {
    /// Full report: wired pairs + synergy opportunities + summary (default).
    Report,
    /// List only active wired pairs.
    Wired,
    /// List only synergy opportunities awaiting a driver.
    Opportunities,
}

// ─────────────────────────────────────────────────────────────────────────────
// Public entry point
// ─────────────────────────────────────────────────────────────────────────────

/// Run the `touring synergy` subcommand.
///
/// Subcommands:
/// - (none) / `report` — full report (default)
/// - `wired` — list only wired pairs
/// - `opportunities` — list only synergy opportunities
///
/// Flags:
/// - `-j` / `--json` — JSON output
/// - `--with-metrics` (Wave 9 S9) — enrich each wired pair with its
///   live `gate_metrics` counter value via a daemon query. Degrades
///   silently when the daemon is unreachable.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let cli = match SynergyCli::try_parse_from(args.iter().skip(1)) {
        Ok(c) => c,
        Err(e) => e.exit(),
    };

    let mut body = match cli.cmd.unwrap_or(SynergyCmd::Report) {
        SynergyCmd::Report => synergy_json_report(),
        SynergyCmd::Wired => synergy_json_wired(),
        SynergyCmd::Opportunities => synergy_json_opportunities(),
    };

    // --with-metrics is a no-op for the opportunities subcommand.
    if cli.with_metrics {
        // Check if body is opportunities-only (no wired_pairs or pairs key).
        let is_opportunities_only =
            body.get("wired_pairs").is_none() && body.get("pairs").is_none();
        if !is_opportunities_only && let Some(metrics) = fetch_gate_metrics_or_none() {
            enrich_pairs_with_metrics(&mut body, &metrics);
        }
    }

    if cli.json {
        use super::common::json_to_stdout;
        let pretty = serde_json::to_string_pretty(&body)?;
        json_to_stdout(&pretty);
    } else {
        render_human(&body);
    }
    Ok(())
}

// ─────────────────────────────────────────────────────────────────────────────
// Internal helpers (unchanged from pre-migration — G6 payload compat)
// ─────────────────────────────────────────────────────────────────────────────

/// Fetch the live `gate_metrics` snapshot from the daemon. Returns
/// `None` on any I/O or parse error so callers can degrade silently
/// (the daemon may be down, the project may have no daemon yet, etc.).
///
/// Wave 9 S9 (2026-04-26).
fn fetch_gate_metrics_or_none() -> Option<serde_json::Value> {
    let raw = daemon_query("cli-gate-metrics", serde_json::json!({})).ok()?;
    serde_json::from_str::<serde_json::Value>(&raw).ok()
}

/// Walk the body's `wired_pairs` (or `pairs`) array and attach a
/// `metrics` object to each entry whose `(producer, consumer)` tuple
/// has a known counter in [`WIRED_PAIR_METRICS`].
///
/// Pairs without a known counter are left unchanged — synergy still
/// renders them, just without a runtime activity number.
///
/// Wave 9 S9 (2026-04-26).
fn enrich_pairs_with_metrics(body: &mut serde_json::Value, metrics: &serde_json::Value) {
    // The `wired` subcommand stores pairs under "pairs"; the full
    // report uses "wired_pairs". Handle both shapes uniformly.
    let array_key = if body.get("wired_pairs").is_some() {
        "wired_pairs"
    } else {
        "pairs"
    };
    let Some(array) = body.get_mut(array_key).and_then(|v| v.as_array_mut()) else {
        return;
    };
    for entry in array {
        let producer = entry
            .get("producer")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        let consumer = entry
            .get("consumer")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        if let Some((_, _, counter_key)) = WIRED_PAIR_METRICS
            .iter()
            .find(|(p, c, _)| *p == producer && *c == consumer)
        {
            let value = metrics
                .get(*counter_key)
                .cloned()
                .unwrap_or(serde_json::Value::Null);
            if let Some(obj) = entry.as_object_mut() {
                obj.insert(
                    "metrics".to_string(),
                    serde_json::json!({ "counter": counter_key, "value": value }),
                );
            }
        }
    }
    // Stamp the body so consumers know enrichment ran.
    if let Some(obj) = body.as_object_mut() {
        obj.insert(
            "metrics_enriched".to_string(),
            serde_json::Value::Bool(true),
        );
    }
}

/// Build the full JSON report (wired pairs + opportunities + summary).
fn synergy_json_report() -> serde_json::Value {
    serde_json::json!({
        "version": "v4.21.0",
        "wave": "Wave 9 — Synergy Deepening",
        "wired_pairs_count": WIRED_PAIRS.len(),
        "synergy_opportunities_count": SYNERGY_OPPORTUNITIES.len(),
        "wired_pairs": WIRED_PAIRS.iter().map(|(producer, consumer, wave, description)| {
            serde_json::json!({
                "producer": producer,
                "consumer": consumer,
                "wave": wave,
                "description": description,
            })
        }).collect::<Vec<_>>(),
        "synergy_opportunities": SYNERGY_OPPORTUNITIES.iter().map(|(integration, target, deferral_reason)| {
            serde_json::json!({
                "integration": integration,
                "target": target,
                "deferral_reason": deferral_reason,
            })
        }).collect::<Vec<_>>(),
    })
}

/// Build a JSON report listing only wired pairs.
fn synergy_json_wired() -> serde_json::Value {
    serde_json::json!({
        "count": WIRED_PAIRS.len(),
        "pairs": WIRED_PAIRS.iter().map(|(producer, consumer, wave, description)| {
            serde_json::json!({
                "producer": producer,
                "consumer": consumer,
                "wave": wave,
                "description": description,
            })
        }).collect::<Vec<_>>(),
    })
}

/// Build a JSON report listing only synergy opportunities.
fn synergy_json_opportunities() -> serde_json::Value {
    serde_json::json!({
        "count": SYNERGY_OPPORTUNITIES.len(),
        "opportunities": SYNERGY_OPPORTUNITIES.iter().map(|(integration, target, deferral_reason)| {
            serde_json::json!({
                "integration": integration,
                "target": target,
                "deferral_reason": deferral_reason,
            })
        }).collect::<Vec<_>>(),
    })
}

/// Render the JSON report as human-readable tabular text on stderr.
fn render_human(body: &serde_json::Value) {
    use super::common::human_to_stderr;
    if let Some(version) = body.get("version").and_then(|v| v.as_str()) {
        human_to_stderr(&format!("Touring Synergy Report — {version}"));
    }
    if let Some(wave) = body.get("wave").and_then(|v| v.as_str()) {
        human_to_stderr(wave);
    }
    human_to_stderr("");

    if let Some(pairs) = body
        .get("wired_pairs")
        .or_else(|| body.get("pairs"))
        .and_then(|v| v.as_array())
    {
        human_to_stderr(&format!("── Wired Pairs ({} active) ──", pairs.len()));
        for pair in pairs {
            let producer = pair.get("producer").and_then(|v| v.as_str()).unwrap_or("?");
            let consumer = pair.get("consumer").and_then(|v| v.as_str()).unwrap_or("?");
            let wave = pair.get("wave").and_then(|v| v.as_str()).unwrap_or("?");
            human_to_stderr(&format!("  [{wave}] {producer} → {consumer}"));
        }
        human_to_stderr("");
    }

    if let Some(opps) = body
        .get("synergy_opportunities")
        .or_else(|| body.get("opportunities"))
        .and_then(|v| v.as_array())
    {
        use super::common::human_to_stderr;
        human_to_stderr(&format!(
            "── Synergy Opportunities ({} pending) ──",
            opps.len()
        ));
        for opp in opps {
            let integration = opp
                .get("integration")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            let target = opp.get("target").and_then(|v| v.as_str()).unwrap_or("?");
            let reason = opp
                .get("deferral_reason")
                .and_then(|v| v.as_str())
                .unwrap_or("?");
            human_to_stderr(&format!("  • {integration}"));
            human_to_stderr(&format!("    target: {target}"));
            human_to_stderr(&format!("    deferred: {reason}"));
        }
    }
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

/// Expose this handler's clap Command for the completions aggregator (W7).
pub(super) fn command() -> clap::Command {
    use clap::CommandFactory;
    SynergyCli::command()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn s(parts: &[&str]) -> Vec<String> {
        parts.iter().map(|p| p.to_string()).collect()
    }

    // ── clap parse smoke tests (W5b) ─────────────────────────────────────────

    #[test]
    fn defaults_to_report_subcommand() {
        let cli = SynergyCli::try_parse_from(["synergy"]).unwrap();
        assert!(matches!(
            cli.cmd.unwrap_or(SynergyCmd::Report),
            SynergyCmd::Report
        ));
        assert!(!cli.json);
        assert!(!cli.with_metrics);
    }

    #[test]
    fn parses_wired_subcommand() {
        let cli = SynergyCli::try_parse_from(["synergy", "wired"]).unwrap();
        assert!(matches!(cli.cmd, Some(SynergyCmd::Wired)));
    }

    #[test]
    fn parses_opportunities_subcommand() {
        let cli = SynergyCli::try_parse_from(["synergy", "opportunities"]).unwrap();
        assert!(matches!(cli.cmd, Some(SynergyCmd::Opportunities)));
    }

    #[test]
    fn parses_json_short_flag() {
        let cli = SynergyCli::try_parse_from(["synergy", "-j"]).unwrap();
        assert!(cli.json);
    }

    #[test]
    fn parses_json_long_flag() {
        let cli = SynergyCli::try_parse_from(["synergy", "--json"]).unwrap();
        assert!(cli.json);
    }

    #[test]
    fn parses_with_metrics_flag() {
        let cli = SynergyCli::try_parse_from(["synergy", "--with-metrics"]).unwrap();
        assert!(cli.with_metrics);
    }

    #[test]
    fn parses_wired_with_metrics_and_json() {
        let cli =
            SynergyCli::try_parse_from(["synergy", "wired", "--with-metrics", "--json"]).unwrap();
        assert!(matches!(cli.cmd, Some(SynergyCmd::Wired)));
        assert!(cli.with_metrics);
        assert!(cli.json);
    }

    #[test]
    fn unknown_subcommand_errors() {
        let result = SynergyCli::try_parse_from(["synergy", "invalid_sub"]);
        assert!(result.is_err());
    }

    // ── run() routing (no-daemon path) ───────────────────────────────────────

    #[test]
    fn run_report_does_not_panic_without_daemon() {
        // run() renders human output (to stderr) and returns Ok when the
        // daemon is unreachable (--with-metrics degrades silently).
        let args = s(&["synergy"]);
        // Without --json the function writes to stderr; we just verify it
        // does not return an Err for the data-only path (report).
        // NOTE: do NOT call run() directly in tests — e.exit() kills the runner.
        // Instead exercise via try_parse_from + business logic.
        let cli = SynergyCli::try_parse_from(args.iter().skip(1)).unwrap();
        let body = match cli.cmd.unwrap_or(SynergyCmd::Report) {
            SynergyCmd::Report => synergy_json_report(),
            SynergyCmd::Wired => synergy_json_wired(),
            SynergyCmd::Opportunities => synergy_json_opportunities(),
        };
        assert!(body.get("wired_pairs").is_some());
    }

    #[test]
    fn run_wired_produces_pairs_key() {
        let cli = SynergyCli::try_parse_from(["synergy", "wired"]).unwrap();
        let body = match cli.cmd.unwrap_or(SynergyCmd::Report) {
            SynergyCmd::Wired => synergy_json_wired(),
            _ => unreachable!(),
        };
        assert!(body.get("pairs").is_some());
        assert!(body.get("wired_pairs").is_none());
    }

    #[test]
    fn run_opportunities_produces_opportunities_key() {
        let cli = SynergyCli::try_parse_from(["synergy", "opportunities"]).unwrap();
        let body = match cli.cmd.unwrap_or(SynergyCmd::Report) {
            SynergyCmd::Opportunities => synergy_json_opportunities(),
            _ => unreachable!(),
        };
        assert!(body.get("opportunities").is_some());
        assert!(body.get("wired_pairs").is_none());
    }

    // ── Catalogue integrity (pre-existing tests, unchanged) ─────────────────

    #[test]
    fn wired_pairs_has_entries() {
        assert!(
            WIRED_PAIRS.len() >= 30,
            "expected substantial wired pair coverage"
        );
    }

    #[test]
    fn synergy_opportunities_documented() {
        assert!(
            !SYNERGY_OPPORTUNITIES.is_empty(),
            "should document at least one opportunity"
        );
    }

    #[test]
    fn wired_pairs_have_no_empty_fields() {
        for (producer, consumer, wave, description) in WIRED_PAIRS {
            assert!(!producer.is_empty(), "empty producer");
            assert!(!consumer.is_empty(), "empty consumer");
            assert!(!wave.is_empty(), "empty wave");
            assert!(!description.is_empty(), "empty description");
        }
    }

    #[test]
    fn synergy_json_report_includes_both_lists() {
        let body = synergy_json_report();
        assert!(body.get("wired_pairs").is_some());
        assert!(body.get("synergy_opportunities").is_some());
        let wp = body.get("wired_pairs").and_then(|v| v.as_array()).unwrap();
        assert_eq!(wp.len(), WIRED_PAIRS.len());
    }

    #[test]
    fn synergy_json_wired_only_returns_pairs() {
        let body = synergy_json_wired();
        assert!(body.get("pairs").is_some());
        assert!(body.get("synergy_opportunities").is_none());
    }

    #[test]
    fn synergy_json_opportunities_only_returns_opps() {
        let body = synergy_json_opportunities();
        assert!(body.get("opportunities").is_some());
        assert!(body.get("wired_pairs").is_none());
    }

    // ── Wave 9 S9: gate-metrics enrichment for wired pairs ───────────────────

    #[test]
    fn s9_metrics_enrichment_attaches_counter_value_to_known_pairs() {
        let mut body = synergy_json_report();
        let fake_metrics = serde_json::json!({
            "diagnostic_tdg_emitted_count": 17,
            "blast_inject_count": 42,
            "query_cache_hit_count": 1234,
            "query_cache_invalidate_count": 8,
            "diagnostic_wiring_finding_emitted_count": 99,
            "health_delta_compute_count": 56,
            "workflow_advice_emitted_count": 7,
        });
        enrich_pairs_with_metrics(&mut body, &fake_metrics);
        assert_eq!(
            body.get("metrics_enriched").and_then(|v| v.as_bool()),
            Some(true)
        );
        let pairs = body.get("wired_pairs").and_then(|v| v.as_array()).unwrap();
        let mut enriched_count = 0;
        for pair in pairs {
            if pair.get("metrics").is_some() {
                enriched_count += 1;
                let m = pair.get("metrics").and_then(|v| v.as_object()).unwrap();
                assert!(m.contains_key("counter"));
                assert!(m.contains_key("value"));
            }
        }
        assert!(
            enriched_count >= WIRED_PAIR_METRICS.len(),
            "expected at least {} enriched pairs, got {}",
            WIRED_PAIR_METRICS.len(),
            enriched_count
        );
    }

    #[test]
    fn s9_metrics_enrichment_leaves_unmapped_pairs_untouched() {
        let mut body = synergy_json_report();
        let metrics = serde_json::json!({});
        enrich_pairs_with_metrics(&mut body, &metrics);

        let pairs = body.get("wired_pairs").and_then(|v| v.as_array()).unwrap();
        let hook_runtime = pairs
            .iter()
            .find(|p| p.get("producer").and_then(|v| v.as_str()) == Some("HookRuntime"));
        assert!(
            hook_runtime.is_some(),
            "test assumes HookRuntime pair exists in WIRED_PAIRS"
        );
        if let Some(p) = hook_runtime {
            assert!(
                p.get("metrics").is_none(),
                "HookRuntime has no counter mapping; metrics field must be absent"
            );
        }
    }

    #[test]
    fn s9_metrics_enrichment_works_on_wired_subcommand_shape() {
        let mut body = synergy_json_wired();
        let fake_metrics = serde_json::json!({
            "diagnostic_tdg_emitted_count": 5,
            "blast_inject_count": 10,
        });
        enrich_pairs_with_metrics(&mut body, &fake_metrics);
        assert_eq!(
            body.get("metrics_enriched").and_then(|v| v.as_bool()),
            Some(true)
        );
        let pairs = body.get("pairs").and_then(|v| v.as_array()).unwrap();
        let pre_edit_tdg = pairs.iter().find(|p| {
            p.get("producer").and_then(|v| v.as_str()) == Some("pre_edit")
                && p.get("consumer").and_then(|v| v.as_str()) == Some("TDG (touring-analysis)")
        });
        assert!(pre_edit_tdg.is_some(), "pre_edit ↔ TDG pair must exist");
        assert!(
            pre_edit_tdg.unwrap().get("metrics").is_some(),
            "wired-shape: pre_edit ↔ TDG must be enriched"
        );
    }

    #[test]
    fn s9_wired_pair_metrics_catalogue_has_no_dangling_entries() {
        for (producer, consumer, _) in WIRED_PAIR_METRICS {
            let exists = WIRED_PAIRS
                .iter()
                .any(|(p, c, _, _)| p == producer && c == consumer);
            assert!(
                exists,
                "WIRED_PAIR_METRICS entry ({producer}, {consumer}) is not in WIRED_PAIRS"
            );
        }
    }

    // ── P6.4 CEG gateway catalog tests ───────────────────────────────────────

    #[test]
    fn p6_4_wired_pairs_contains_ceg_gateway_entry() {
        let found = WIRED_PAIRS.iter().any(|(producer, consumer, wave, _)| {
            producer.contains("CEG gateway")
                && consumer.contains("cli_suggester")
                && wave.contains("P6.4")
        });
        assert!(
            found,
            "WIRED_PAIRS must contain CEG gateway (X0..X9) <-> cli_suggester enrichment entry (v8.0 P6.4)"
        );
    }

    #[test]
    fn p6_4_ceg_wired_pair_entry_is_well_formed() {
        let entry = WIRED_PAIRS.iter().find(|(producer, consumer, _, _)| {
            producer.contains("CEG gateway") && consumer.contains("cli_suggester")
        });
        let (producer, consumer, wave, description) =
            entry.expect("CEG gateway entry must exist in WIRED_PAIRS");
        assert!(!producer.is_empty(), "CEG producer must be non-empty");
        assert!(!consumer.is_empty(), "CEG consumer must be non-empty");
        assert!(!wave.is_empty(), "CEG wave must be non-empty");
        assert!(!description.is_empty(), "CEG description must be non-empty");
        assert!(
            description.contains("touring exec") || description.contains("CEG"),
            "CEG description must mention `touring exec` or `CEG`: got {description:?}"
        );
    }

    #[test]
    fn p6_4_wired_pair_metrics_contains_ceg_counter() {
        let found = WIRED_PAIR_METRICS
            .iter()
            .any(|(producer, consumer, counter)| {
                producer.contains("CEG gateway")
                    && consumer.contains("cli_suggester")
                    && !counter.is_empty()
            });
        assert!(
            found,
            "WIRED_PAIR_METRICS must contain a counter for the CEG gateway <-> cli_suggester pair"
        );
    }

    // ── with_metrics no-op on opportunities (W5b regression) ────────────────

    #[test]
    fn with_metrics_is_noop_for_opportunities_body() {
        // Opportunities body has no wired_pairs or pairs key — enrichment
        // must be skipped entirely (no metrics_enriched stamp either).
        let mut body = synergy_json_opportunities();
        let metrics = serde_json::json!({"blast_inject_count": 99});
        // Simulate the guard from run():
        let is_opportunities_only =
            body.get("wired_pairs").is_none() && body.get("pairs").is_none();
        if !is_opportunities_only {
            enrich_pairs_with_metrics(&mut body, &metrics);
        }
        assert!(
            body.get("metrics_enriched").is_none(),
            "opportunities body must not be stamped with metrics_enriched"
        );
    }
}
