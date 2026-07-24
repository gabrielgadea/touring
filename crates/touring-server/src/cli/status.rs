//! `touring status` — Unified dashboard: daemon health + index + wiring + sessions.
//!
//! Aggregates multiple daemon queries into a single consolidated view.
//!
//! Wave 8 S3 (Synergy Maximization, 2026-04-26): JSON output now includes
//! a `composite_health_score` ∈ [0.0, 1.0] computed as a weighted average
//! across 5 health dimensions. See [`compute_composite_health_score`] for
//! the weighting rationale.

use super::common::{
    flag_value, human_to_stderr, json_to_stdout, parse_global_flags, run_daemon_cmd,
};
use super::daemon_query;

/// Queries to run for the unified status dashboard.
///
/// L7-B Delta: added `gate_metrics` entry exposing enrichment gate
/// observability counters under the `gate_metrics` key.
///
/// Wave 16 (2026-04-18): added `health_delta` entry surfacing the
/// aggregate snapshot from `cli-health-delta-status` (when no
/// `file_path` payload is supplied, the handler returns the same
/// counters as `gate_metrics` but in a flatter shape — useful as a
/// quick at-a-glance for the streak-tracking subsystem).
const STATUS_QUERIES: &[(&str, &str)] = &[
    ("daemon_health", "cli-flywheel-status"),
    ("index", "cli-index-status"),
    ("wiring", "cli-wiring-status"),
    ("sessions", "cli-session-list"),
    ("learning", "cli-learning-status"),
    ("incremental", "cli-incremental-status"),
    ("gate_metrics", "cli-gate-metrics"),
    ("health_delta", "cli-health-delta-status"),
    // Wave 2026-05-02: PSI memory pressure + CPU P/E core topology status.
    // Handler `cli-resource-monitor-status` returns a JSON snapshot of the
    // MemoryGuard singleton: pressure tier, available_mb, psi_some_avg10,
    // swap_used_pct, and the 8 gate_metrics resource-monitor counters.
    // Gracefully degrades to `{"error": "..."}` when the feature is absent
    // or the guard has not yet emitted its first tick.
    ("resource_monitor", "cli-resource-monitor-status"),
];

/// Entry point for the `touring status` CLI handler — runs every entry in
/// `STATUS_QUERIES` against the daemon and renders a unified dashboard. In JSON
/// mode (`-j`) it aggregates all sections into one object, injects the
/// `quality_signal` snapshot, an optional `federation` summary (`--federate
/// <paths>`), and the top-line `composite_health_score`; otherwise it prints
/// each section to stderr with a header.
pub fn run(args: &[String]) -> anyhow::Result<()> {
    let (flags, filtered) = parse_global_flags(args);
    let federate_arg: Option<String> = flag_value(&filtered, "--federate").map(String::from);

    if flags.json {
        // JSON mode: aggregate all into a single JSON object
        let mut combined = serde_json::Map::new();
        for (key, hook) in STATUS_QUERIES {
            let result = daemon_query(hook, serde_json::json!({}))
                .unwrap_or_else(|e| format!("{{\"error\": \"{e}\"}}"));
            let val: serde_json::Value =
                serde_json::from_str(&result).unwrap_or(serde_json::Value::String(result));
            combined.insert((*key).to_string(), val);
        }
        // Drop the verbose PPO policy weights (~95 KB of the ~166 KB `-j`
        // output) from the learning snapshot — they bloat the at-a-glance
        // dashboard and belong in a dedicated `touring learning export`,
        // not in `status`. Nothing downstream consumes `policy`.
        if let Some(rl) = combined
            .get_mut("learning")
            .and_then(serde_json::Value::as_object_mut)
            .and_then(|l| l.get_mut("agentic_rl_state"))
            .and_then(serde_json::Value::as_object_mut)
        {
            rl.remove("policy");
        }
        // Slim the wiring snapshot before it lands in the dashboard. The
        // `cli-wiring-status` handler embeds `hypergraph_cycles.detail` — a
        // ~923-entry, ~58 KB array (99% of the 59 KB `wiring` block) — which has
        // no at-a-glance value. `slim_large_arrays` elides large arrays to a
        // count while preserving every scalar (orphan_count, total_pub_symbols,
        // module_count — the composite_health_score inputs). The handler itself
        // is untouched, so `touring wiring status` / snapshot / eval keep the
        // full detail. Applied to `combined` here so BOTH the full dashboard and
        // the `--brief` subset (which reads `combined.get("wiring")`) stay lean.
        if let Some(w) = combined.get_mut("wiring") {
            let slim = super::common::slim_large_arrays(w);
            *w = slim;
        }
        // Wave 1 P2 (Sentrux master plan, 2026-05-09): inject the
        // workspace quality signal for the current working directory.
        // Computed in-process (no daemon round-trip) so an external
        // workspace can still report a signal even when its symbol_store
        // is uninitialised.
        combined.insert(
            "quality_signal".to_string(),
            compute_quality_signal_snapshot(),
        );
        // Wave 3 P7 (Sentrux master plan, 2026-05-09): when --federate
        // <comma-separated paths> is supplied, also embed a federated
        // summary across the listed workspaces. Disabled by default
        // (zero overhead when the flag is absent).
        if let Some(spec) = &federate_arg {
            combined.insert("federation".to_string(), compute_federation_snapshot(spec));
        }
        // Wave 8 S3 (synergy maximization): compute composite health score
        // from the aggregated subsystem snapshots. Surfaces a single
        // top-line number for at-a-glance "is the system healthy?" decisions.
        let score = compute_composite_health_score(&combined);
        combined.insert(
            "composite_health_score".to_string(),
            serde_json::json!(score),
        );
        // `--brief`: emit only the at-a-glance essentials (no large arrays),
        // collapsing the dashboard to a ~1 KB summary for the LLM context.
        // Default mode keeps every section (minus the dropped PPO policy).
        let value = if flags.brief {
            serde_json::json!({
                "composite_health_score": score,
                "daemon_health": combined.get("daemon_health"),
                "index": combined.get("index"),
                "wiring": combined.get("wiring"),
                "health_delta": combined.get("health_delta"),
                "quality_signal": combined.get("quality_signal"),
            })
        } else {
            serde_json::Value::Object(combined)
        };
        let output = serde_json::to_string_pretty(&value)?;
        json_to_stdout(&output);
    } else {
        // Human-readable mode: print each section with headers
        for (key, hook) in STATUS_QUERIES {
            let label = key.replace('_', " ");
            human_to_stderr(&format!("── {label} ──"));
            if let Err(e) = run_daemon_cmd(hook, serde_json::json!({}), &flags) {
                human_to_stderr(&format!("  (unavailable: {e})"));
            }
            human_to_stderr("");
        }
    }
    Ok(())
}

/// Snapshot a federated Sentrux summary across the workspaces listed in
/// `spec` (comma-separated paths).
///
/// Wave 3 P7 (Sentrux master plan, 2026-05-09). Each path is scanned
/// in-process, its quality signal is computed, and the resulting
/// `FederationEntry` list is folded by
/// [`touring_hooks::shared::federation::aggregate`] into a single
/// `FederationSummary`. Per-workspace failures (unreadable root, etc.)
/// are recorded in the returned `errors` array — the aggregator still
/// runs over the entries that succeeded, so partial federations remain
/// useful.
///
/// `workspace_id` defaults to the path leaf (`PathBuf::file_name`).
/// Empty input yields a structured `error` field; the caller's caller
/// can still parse the JSON without special-casing.
fn compute_federation_snapshot(spec: &str) -> serde_json::Value {
    let now_ms = unix_millis_now();
    let mut entries: Vec<touring_hooks::shared::federation::FederationEntry> = Vec::new();
    let mut errors: Vec<serde_json::Value> = Vec::new();

    let roots: Vec<&str> = spec
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if roots.is_empty() {
        return serde_json::json!({
            "error": "--federate must list at least one path",
            "spec": spec,
        });
    }

    let roots_count = roots.len();
    for raw in &roots {
        let root = std::path::PathBuf::from(raw);
        let workspace_id = root
            .file_name()
            .and_then(|s| s.to_str())
            .map_or_else(|| (*raw).to_string(), str::to_string);
        match touring_analysis::quality::build_workspace_from_path(&root) {
            Ok(workspace) => {
                let signal = touring_analysis::quality::compute_quality_signal(&workspace);
                entries.push(touring_hooks::shared::federation::FederationEntry {
                    workspace_id,
                    workspace_root: root,
                    signal,
                    timestamp_ms: now_ms,
                });
            }
            Err(err) => {
                errors.push(serde_json::json!({
                    "workspace_id": workspace_id,
                    "root": raw,
                    "error": err.to_string(),
                }));
            }
        }
    }

    let summary = touring_hooks::shared::federation::aggregate(&entries);
    serde_json::json!({
        "entries_input": roots_count,
        "entries_aggregated": entries.len(),
        "errors": errors,
        "summary": summary,
    })
}

fn unix_millis_now() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0)
}

/// Snapshot the workspace quality signal for the current working directory.
///
/// Wave 1 P2 (Sentrux master plan, 2026-05-09). Returns a compact JSON
/// object containing `signal_0_10000`, `signal_normalized`, `bottleneck`,
/// and the five `root_causes` scores — designed to be embedded inside
/// `touring status -j` without the bulk of `diagnostics`.
///
/// On failure (unreadable root, etc.), the returned JSON carries an
/// `error` field and a neutral `signal_0_10000 = 0` so downstream
/// callers can still parse without special-casing.
fn compute_quality_signal_snapshot() -> serde_json::Value {
    let root = std::env::current_dir().unwrap_or_else(|_| std::path::PathBuf::from("."));
    match touring_analysis::quality::build_workspace_from_path(&root) {
        Ok(ws) => {
            let signal = touring_analysis::quality::compute_quality_signal(&ws);
            serde_json::json!({
                "root": root.to_string_lossy().into_owned(),
                "signal_0_10000": signal.signal_0_10000,
                "signal_normalized": signal.signal_normalized,
                "bottleneck": signal.bottleneck,
                "root_causes": signal.root_causes,
                "raw": signal.raw,
            })
        }
        Err(err) => serde_json::json!({
            "root": root.to_string_lossy().into_owned(),
            "signal_0_10000": 0,
            "signal_normalized": 0.0,
            "bottleneck": "Tied",
            "error": err.to_string(),
        }),
    }
}

/// Compute a composite health score ∈ [0.0, 1.0] from the aggregated
/// subsystem snapshots returned by `STATUS_QUERIES`.
///
/// Weighted average across 5 dimensions:
/// - **daemon_health** (30%): `healthy_count / total_count` from `daemon_health.components`
/// - **orphan_ratio** (20%): `1.0 - clamp(orphan_count / total_pub_symbols, 0.0, 1.0)`
/// - **regression_streak** (20%): `1.0 / (1.0 + outstanding)` from `health_delta.outstanding`
/// - **cache_hit_ratio** (15%): from `gate_metrics.query_cache_hit_ratio` (or 0.5 if absent)
/// - **ema_reward** (15%): clamp `learning.ema_reward` to `[0, 1]`
///
/// Missing fields contribute neutral 0.5 for that dimension. Score = 1.0
/// means all subsystems are at healthy baseline; < 0.5 indicates degradation.
///
/// Wave 9 S8 (2026-04-26): the implementation moved to
/// [`touring_foundation::health::compute_composite_health_score`] so the same
/// scoring logic is available to non-CLI callers (e.g. the
/// `instructions_loaded` hook). This wrapper preserves the historical
/// public path under `cli::status`.
#[must_use]
pub fn compute_composite_health_score(
    combined: &serde_json::Map<String, serde_json::Value>,
) -> f64 {
    touring_foundation::health::compute_composite_health_score(combined)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_queries_has_entries() {
        assert!(STATUS_QUERIES.len() >= 5);
    }

    #[test]
    fn status_queries_keys_are_unique() {
        let mut keys: Vec<&str> = STATUS_QUERIES.iter().map(|(k, _)| *k).collect();
        let len = keys.len();
        keys.sort();
        keys.dedup();
        assert_eq!(keys.len(), len, "duplicate keys in STATUS_QUERIES");
    }

    #[test]
    fn composite_score_empty_input_is_neutral() {
        let map = serde_json::Map::new();
        let score = compute_composite_health_score(&map);
        // 5 dimensions × 0.5 neutral × respective weights = 0.5 exactly.
        assert!((score - 0.5).abs() < 1e-6, "expected 0.5, got {score}");
    }

    #[test]
    fn composite_score_all_perfect_is_one() {
        let mut map = serde_json::Map::new();
        map.insert(
            "daemon_health".to_string(),
            serde_json::json!({"healthy_count": 8, "total_count": 8}),
        );
        map.insert(
            "wiring".to_string(),
            serde_json::json!({"orphan_count": 0, "total_pub_symbols": 1000}),
        );
        map.insert(
            "health_delta".to_string(),
            serde_json::json!({"outstanding": 0}),
        );
        map.insert(
            "gate_metrics".to_string(),
            serde_json::json!({"query_cache_hit_ratio": 1.0}),
        );
        map.insert(
            "learning".to_string(),
            serde_json::json!({"ema_reward": 1.0}),
        );
        let score = compute_composite_health_score(&map);
        assert!((score - 1.0).abs() < 1e-6, "expected 1.0, got {score}");
    }

    #[test]
    fn composite_score_degraded_daemon_drops_score() {
        let mut map = serde_json::Map::new();
        map.insert(
            "daemon_health".to_string(),
            serde_json::json!({"healthy_count": 4, "total_count": 8}),
        );
        let score = compute_composite_health_score(&map);
        // 0.30 * 0.5 (daemon at half) + (0.20+0.20+0.15+0.15) * 0.5 (others neutral)
        // = 0.15 + 0.35 = 0.50
        assert!((score - 0.5).abs() < 1e-6, "expected 0.5, got {score}");
    }

    #[test]
    fn composite_score_high_orphan_count_drops_score() {
        let mut map = serde_json::Map::new();
        map.insert(
            "wiring".to_string(),
            serde_json::json!({"orphan_count": 9000, "total_pub_symbols": 10000}),
        );
        let score = compute_composite_health_score(&map);
        // orphan_score = 1 - 0.9 = 0.1; others neutral 0.5.
        // 0.30*0.5 + 0.20*0.1 + 0.20*0.5 + 0.15*0.5 + 0.15*0.5 = 0.42
        assert!((score - 0.42).abs() < 1e-6, "expected 0.42, got {score}");
    }
}
