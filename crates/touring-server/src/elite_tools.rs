//! Elite harness tools — migrated 2026-06-25 from `touring-harness-mcp`.
//!
//! Five MCP-callable tools that wrap the 17-gate composite scorer from
//! `touring-quality::runner` + `touring-quality::gate`. Originally exposed
//! by the standalone `touring-harness-mcp` binary (rmcp-stdio), now
//! available in the unified `touring` server via the `elite` subcommand
//! (`touring elite <tool> --args '{...}'`) and the `tools::elite_*`
//! helper module exposed to other consumers (CLI, gateway extension).
//!
//! # Tool reference (semantics unchanged from touring-harness-mcp)
//!
//! | Tool                | Args                                       | Returns |
//! |---------------------|--------------------------------------------|---------|
//! | `elite_check`       | `file?`, `agent_id?`, `model?`             | composite score + gates JSON |
//! | `elite_gate`        | `gate_id`                                  | single-gate outcome |
//! | `elite_badge`       | (none)                                     | tier badge |
//! | `elite_register`    | `agent_id` (required), `model?`            | history entry for agent registration |
//! | `elite_history`     | `limit?` (default 10), `agent_filter?`     | recent history entries |
//!
//! All functions are pure (no I/O side effects beyond the JSONL history
//! append under `~/.claude/touring/elite-history.jsonl`), deterministic,
//! and cheap (single 17-gate run is sub-millisecond on warm caches).

use std::path::PathBuf;

use serde_json::{Value, json};
use touring_quality::builtin_default_gates;
use touring_quality::change::{Change, FileKind, ProposedFile};
use touring_quality::gate::{GateId, GateOutcome, GateSeverity};
use touring_quality::history::ScoreHistory;
use touring_quality::runner::{HarnessConfig, run_harness, should_block};

/// Dispatch a tool call by name (`elite_*`) with JSON params.
///
/// Returns the tool's JSON result. Unknown tool names return an error
/// envelope (`{"error": "..."}`) — never panics. Mirrors the behaviour
/// of the original `touring-harness-mcp::handle_request` dispatcher.
pub fn dispatch(tool: &str, params: &Value) -> Value {
    match tool {
        "elite_check" => tool_check(params),
        "elite_gate" => tool_gate(params),
        "elite_badge" => tool_badge(),
        "elite_register" => tool_register(params),
        "elite_history" => tool_history(params),
        other => json!({
            "error": format!(
                "unknown elite tool '{other}'; valid: elite_check, elite_gate, elite_badge, elite_register, elite_history"
            )
        }),
    }
}

/// Construct a synthetic `Change` for harness scoring. When `file` is
/// provided, reads the file contents into `after`. Otherwise uses a
/// scratch change (create `/scratch.rs`) — gives the default 17-gate set
/// a meaningful composite to report.
fn build_change(file: Option<&str>) -> Change {
    match file {
        Some(p) => {
            let path = PathBuf::from(p);
            let content = std::fs::read_to_string(&path).unwrap_or_default();
            Change::new().with_file(ProposedFile {
                path,
                before: Some(String::new()),
                after: Some(content),
                kind: FileKind::Modify,
            })
        }
        None => Change::new().with_file(ProposedFile {
            path: PathBuf::from("scratch.rs"),
            before: None,
            after: Some("// scratch\n".into()),
            kind: FileKind::Create,
        }),
    }
}

/// `elite_check` — composite score for a synthetic change built from
/// `file` (or scratch). Appends one entry to the score history. Returns
/// composite + tier + per-gate breakdown + block/warn reasons.
fn tool_check(params: &Value) -> Value {
    let file = params.get("file").and_then(|v| v.as_str());
    let agent_id = params
        .get("agent_id")
        .and_then(|v| v.as_str())
        .map(String::from);
    let model = params
        .get("model")
        .and_then(|v| v.as_str())
        .map(String::from);

    let mut change = build_change(file);
    if let Some(a) = agent_id {
        change.agent_id = Some(a);
    }
    if let Some(m) = model {
        change.model = Some(m);
    }
    if change.agent_id.is_none() {
        change.agent_id = Some("touring-elite".to_string());
    }
    if change.model.is_none() {
        change.model = Some("unknown".to_string());
    }

    let gates = builtin_default_gates();
    let cfg = HarnessConfig::default();
    let (score, _block) = should_block(&change, &gates, &cfg);

    let history = ScoreHistory::new(ScoreHistory::default_path());
    let entry = ScoreHistory::entry_from(
        &score,
        change.agent_id.as_deref().unwrap_or("touring"),
        change.model.as_deref().unwrap_or("unknown"),
        "touring_elite_check via server",
        change.files.len(),
        change.total_added(),
    );
    let _ = history.append(&entry);

    let gates_json: Vec<Value> = score
        .gates
        .iter()
        .map(|g| {
            json!({
                "gate_id": g.gate_id.slug(),
                "severity": format!("{:?}", g.severity),
                "status": format!("{:?}", g.status),
                "score": g.score,
                "weight": g.weight,
                "message": g.message,
                "elapsed_ms": g.elapsed_ms,
            })
        })
        .collect();

    json!({
        "composite": score.composite,
        "tier": format!("{:?}", score.tier),
        "badge": score.badge(),
        "is_release_ready": score.is_release_ready(),
        "block_reasons": score.block_reasons().iter().map(|g| g.gate_id.slug()).collect::<Vec<_>>(),
        "warn_reasons": score.warn_reasons().iter().map(|g| g.gate_id.slug()).collect::<Vec<_>>(),
        "total_weight": score.total_weight,
        "weighted_sum": score.weighted_sum,
        "gates": gates_json,
    })
}

/// `elite_gate` — single-gate outcome by `gate_id` slug.
fn tool_gate(params: &Value) -> Value {
    let gate_id = params.get("gate_id").and_then(|v| v.as_str()).unwrap_or("");
    let id = match GateId::from_slug(gate_id) {
        Some(id) => id,
        None => {
            return json!({
                "error": format!(
                    "unknown gate id '{gate_id}'; valid: {}",
                    GateId::ALL.iter().map(|g| g.slug()).collect::<Vec<_>>().join(", ")
                )
            });
        }
    };
    let outcome: GateOutcome = GateOutcome::external(id, GateSeverity::Block).with_payload(
        serde_json::json!({"hint": "single-gate inspection; run elite_check for composite"}),
    );
    json!({
        "gate_id": outcome.gate_id.slug(),
        "severity": format!("{:?}", outcome.severity),
        "status": format!("{:?}", outcome.status),
        "score": outcome.score,
        "weight": outcome.weight,
        "message": outcome.message,
    })
}

/// `elite_badge` — current synthetic-change tier badge.
fn tool_badge() -> Value {
    let score = run_harness(
        &Change::new(),
        &builtin_default_gates(),
        &HarnessConfig::default(),
    );
    json!({
        "badge": score.badge(),
        "tier": format!("{:?}", score.tier),
        "composite": score.composite,
    })
}

/// `elite_register` — append a history entry recording that an agent
/// `agent_id` (required) using `model` (optional, default "unknown")
/// is now tracked by the harness.
fn tool_register(params: &Value) -> Value {
    let agent_id = params
        .get("agent_id")
        .and_then(|v| v.as_str())
        .unwrap_or("");
    if agent_id.is_empty() {
        return json!({"error": "agent_id is required"});
    }
    let model = params
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown");
    let history = ScoreHistory::new(ScoreHistory::default_path());
    let score = run_harness(
        &Change::new(),
        &builtin_default_gates(),
        &HarnessConfig::default(),
    );
    let entry = ScoreHistory::entry_from(
        &score,
        agent_id,
        model,
        "agent registered via touring_elite_register",
        0,
        0,
    );
    match history.append(&entry) {
        Ok(()) => {
            json!({"registered": true, "agent_id": agent_id, "model": model, "tier": format!("{:?}", score.tier)})
        }
        Err(e) => json!({"error": format!("history append failed: {e}")}),
    }
}

/// `elite_history` — read the last `limit` entries (default 10) from
/// the JSONL score history, optionally filtered by `agent_id`.
fn tool_history(params: &Value) -> Value {
    let limit = params
        .get("limit")
        .and_then(|v| v.as_u64())
        .map(|n| n as usize)
        .unwrap_or(10);
    let agent_filter = params
        .get("agent_id")
        .and_then(|v| v.as_str())
        .map(String::from);
    let history = ScoreHistory::new(ScoreHistory::default_path());
    match history.last_n(limit) {
        Ok(entries) => {
            let filtered: Vec<_> = entries
                .into_iter()
                .filter(|e| agent_filter.as_ref().is_none_or(|f| e.agent_id == *f))
                .map(|e| {
                    json!({
                        "timestamp": e.timestamp.to_rfc3339(),
                        "agent_id": e.agent_id,
                        "model": e.model,
                        "composite": e.composite,
                        "tier": format!("{:?}", e.tier),
                        "change_summary": e.change_summary,
                        "file_count": e.file_count,
                        "added_loc": e.added_loc,
                    })
                })
                .collect();
            json!({"count": filtered.len(), "entries": filtered})
        }
        Err(e) => json!({"error": format!("history read failed: {e}")}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use touring_quality::gate::GateStatus;

    #[test]
    fn dispatch_unknown_tool_returns_error_envelope() {
        let v = dispatch("nonexistent", &json!({}));
        assert!(v.get("error").is_some());
    }

    #[test]
    fn dispatch_badge_works() {
        let v = dispatch("elite_badge", &json!({}));
        assert!(v.get("badge").is_some(), "missing badge field: {v}");
    }

    #[test]
    fn dispatch_gate_with_valid_slug() {
        let v = dispatch("elite_gate", &json!({"gate_id": "security"}));
        assert!(v.get("error").is_none(), "unexpected error: {v}");
        assert_eq!(v.get("gate_id").and_then(|x| x.as_str()), Some("security"));
    }

    #[test]
    fn dispatch_gate_with_invalid_slug_returns_error_with_valid_list() {
        let v = dispatch("elite_gate", &json!({"gate_id": "nope"}));
        let err = v.get("error").and_then(|x| x.as_str()).unwrap_or("");
        assert!(err.starts_with("unknown gate id 'nope'"));
        assert!(err.contains("security")); // contains valid list
    }

    #[test]
    fn dispatch_register_requires_agent_id() {
        let v = dispatch("elite_register", &json!({}));
        assert!(v.get("error").is_some());
    }

    #[test]
    fn gate_outcome_builders_available() {
        // Guard against accidental removal of GateOutcome builders used
        // by tool_check and tool_gate.
        let _ = GateOutcome::external(GateId::Security, GateSeverity::Block);
        let _ = GateStatus::Pass;
    }
}
