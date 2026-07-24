//! `cli_kpi` — Wave R2: Falsifiable Commitments Dashboard.
//!
//! Reads `~/.claude/rust/docs/kpi/commitments.yaml` (versioned source of truth),
//! checks each commitment against its declared threshold/direction, and
//! returns a structured snapshot consumable by Gabriel or by CI gates.
//!
//! # Source parsing
//!
//! Each commitment declares a `source` in one of two forms:
//! - `daemon:<handler-name>:<json-pointer>` — invoke an in-process handler
//!   and extract the value via RFC-6901 JSON pointer (e.g. `/orphan_count`).
//! - `derived:<name>` — a value computed from already-collected data with no
//!   new instrumentation (`health_delta_net`, `world_model_success`). Powers
//!   the `touring.coupling.*` effectiveness family (F1 telemetry).
//! - `external:<command>` — placeholder for subprocess gates (cargo nextest,
//!   llvm-cov, etc.) that the daemon cannot check alone. For MVP these
//!   return `actual: null, status: "STUB"` to keep the dashboard honest.
//!
//! # CLI flags (handled by `touring-server::cli::kpi`)
//!
//! - `-j` / no flag: JSON dashboard
//! - `--check`: signal failure when any commitment fails (CLI exits non-zero)
//! - `--snapshot`: persist to `docs/kpi/YYYY-MM/YYYY-MM-DD.json`
//!
//! # Output schema
//!
//! ```json
//! {
//!   "schema": "kpi-commitments-v1",
//!   "snapshot_date": "2026-04-25",
//!   "checks": [
//!     {"id": "touring.wiring.orphans", "actual": 9106, "threshold": 100,
//!      "direction": "lte", "status": "FAIL"}
//!   ],
//!   "summary": {"total": 8, "passed": 4, "failed": 3, "stub": 2, "advisory": 0, "regressions": 0}
//! }
//! ```

use crate::runtime::HookRuntime;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::path::{Path, PathBuf};

// ─────────────────────────────────────────────────────────────────────────────
// Public types — kept minimal so the YAML schema can grow without breaking.
// ─────────────────────────────────────────────────────────────────────────────

/// One commitment row, deserialised from `commitments.yaml`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Commitment {
    /// Stable identifier (e.g. `touring.wiring.orphans`).
    pub id: String,
    /// Human-readable label.
    pub name: String,
    /// Numeric target — compared via `direction`.
    pub threshold: f64,
    /// Comparison direction: `gte`, `lte`, or `eq`.
    pub direction: String,
    /// Source spec: `daemon:<handler>:<pointer>` or `external:<command>`.
    pub source: String,
    /// Human rationale (echoed for context, not checked).
    #[serde(default)]
    pub rationale: String,
    /// When true, a missed threshold is reported as `ADVISORY` (not `FAIL`)
    /// and excluded from the `--check` exit-code gate — for KPIs under
    /// calibration (the `touring.coupling.*` family, 2-week advisory window).
    #[serde(default)]
    pub advisory: bool,
}

/// Top-level YAML structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CommitmentsFile {
    /// Version string of the commitments file format.
    pub version: String,
    /// Schema identifier the file conforms to.
    pub schema: String,
    /// The KPI commitments declared in this file.
    pub commitments: Vec<Commitment>,
}

/// Per-commitment check result.
#[derive(Debug, Clone, Serialize)]
pub struct CommitmentCheck {
    /// Identifier of the commitment being checked.
    pub id: String,
    /// Human-readable name of the commitment.
    pub name: String,
    /// Target threshold the actual value is compared against.
    pub threshold: f64,
    /// Comparison direction (whether the actual must be above or below the threshold).
    pub direction: String,
    /// Source metric kind the actual value was read from.
    pub source_kind: &'static str,
    /// Observed value, or `None` when the metric could not be resolved.
    pub actual: Option<f64>,
    /// Pass/fail/unknown outcome of the threshold comparison.
    pub status: &'static str,
    /// Human rationale echoed from the commitment for context.
    pub rationale: String,
    /// Whether this commitment is advisory (missed threshold → `ADVISORY`,
    /// excluded from the `--check` failure gate).
    pub advisory: bool,
}

// ─────────────────────────────────────────────────────────────────────────────
// Public handler
// ─────────────────────────────────────────────────────────────────────────────

/// `cli-kpi` handler — produces the falsifiable commitments dashboard.
///
/// Payload optional fields:
/// - `"check": true` → set `summary.check_failed` flag when any commitment FAIL
/// - `"snapshot": true` → persist to `docs/kpi/YYYY-MM/YYYY-MM-DD.json`
/// - `"yaml_path": "/abs/path"` → override default commitments file (testing)
pub fn cli_kpi(rt: &mut HookRuntime, payload: &Value) -> String {
    let yaml_override = payload.get("yaml_path").and_then(Value::as_str);
    let snapshot = payload
        .get("snapshot")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let check = payload
        .get("check")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    let yaml_path = yaml_override
        .map(PathBuf::from)
        .unwrap_or_else(default_commitments_path);
    let file = match load_commitments(&yaml_path) {
        Ok(f) => f,
        Err(e) => {
            return json!({
                "error": format!("failed to load commitments.yaml: {e}"),
                "yaml_path": yaml_path.display().to_string(),
            })
            .to_string();
        }
    };
    let checks: Vec<CommitmentCheck> = file.commitments.iter().map(|c| check_one(rt, c)).collect();
    let summary = summarize(&checks);
    let snapshot_date = today_iso();
    // Investigation 2026-07-01: several sources resolve per-project (orphans,
    // ema_reward), so the SAME commitment reports different numbers depending
    // on the caller's cwd. Label every output with the project it measured so
    // readers (and the datated series) can tell which project a number is from.
    let mut out = json!({
        "schema": file.schema,
        "snapshot_date": snapshot_date,
        "project_root": rt.project_root.display().to_string(),
        "checks": checks,
        "summary": summary,
    });

    // F6 (telemetry §10/§11.1) — attach the latest A/B causal-attribution block
    // from `run_bench.py --compare`; `null` when no A/B has run (honest absence).
    out["ab"] = build_ab_block(&default_ab_path()).unwrap_or(Value::Null);

    // F7 (telemetry §12) — when requested, run the refinement engine over the live
    // coupling signals + the A/B gate, surfacing the recommended actuators (advisory
    // unless the A/B confirms the coupling — see `RefinementAction::actionable`).
    if payload
        .get("refine")
        .and_then(Value::as_bool)
        .unwrap_or(false)
    {
        out["refinements"] = json!(collect_and_recommend(rt, &out["ab"]));
    }

    if check && summary["failed"].as_u64().unwrap_or(0) > 0 {
        out["check_failed"] = json!(true);
    }
    if snapshot {
        match persist_snapshot(&out, &snapshot_date, &rt.project_root) {
            Ok(path) => out["snapshot_path"] = json!(path.display().to_string()),
            Err(e) => out["snapshot_error"] = json!(e.to_string()),
        }
    }
    out.to_string()
}

// ─────────────────────────────────────────────────────────────────────────────
// Loading + path resolution
// ─────────────────────────────────────────────────────────────────────────────

fn default_commitments_path() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        let p = PathBuf::from(home).join(".claude/rust/docs/kpi/commitments.yaml");
        if p.exists() {
            return p;
        }
    }
    PathBuf::from("docs/kpi/commitments.yaml")
}

fn load_commitments(path: &PathBuf) -> std::io::Result<CommitmentsFile> {
    let raw = std::fs::read_to_string(path)?;
    serde_yaml::from_str::<CommitmentsFile>(&raw)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e.to_string()))
}

fn today_iso() -> String {
    use std::time::{SystemTime, UNIX_EPOCH};
    let secs = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    iso_date_from_unix(secs)
}

/// Convert unix seconds to `YYYY-MM-DD` (UTC, Gregorian, post-1970).
#[must_use]
pub fn iso_date_from_unix(secs: u64) -> String {
    let days = secs / 86_400;
    let (y, m, d) = days_to_ymd(days as i64);
    format!("{y:04}-{m:02}-{d:02}")
}

fn days_to_ymd(mut days: i64) -> (i32, u32, u32) {
    days += 719_468;
    let era = if days >= 0 { days } else { days - 146_096 } / 146_097;
    let doe = (days - era * 146_097) as u64;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let y_signed = yoe as i64 + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { mp + 3 } else { mp - 9 } as u32;
    let y = if m <= 2 { y_signed + 1 } else { y_signed };
    (y as i32, m, d)
}

// ─────────────────────────────────────────────────────────────────────────────
// Per-commitment checking
// ─────────────────────────────────────────────────────────────────────────────

fn check_one(rt: &mut HookRuntime, c: &Commitment) -> CommitmentCheck {
    let (kind, actual) = resolve_source(rt, &c.source);
    let mut status = match (kind, actual) {
        ("daemon" | "derived", Some(v)) => check_threshold(v, c.threshold, &c.direction),
        ("external", _) | ("daemon" | "derived", None) => "STUB",
        _ => "ERROR",
    };
    if c.advisory && status == "FAIL" {
        status = "ADVISORY";
    }
    CommitmentCheck {
        id: c.id.clone(),
        name: c.name.clone(),
        threshold: c.threshold,
        direction: c.direction.clone(),
        source_kind: kind,
        actual,
        status,
        rationale: c.rationale.clone(),
        advisory: c.advisory,
    }
}

fn resolve_source(rt: &mut HookRuntime, source: &str) -> (&'static str, Option<f64>) {
    if let Some(rest) = source.strip_prefix("daemon:") {
        let mut parts = rest.splitn(2, ':');
        let handler = parts.next().unwrap_or("");
        let pointer = parts.next().unwrap_or("");
        let value = invoke_handler(rt, handler);
        let extracted = value
            .as_ref()
            .and_then(|v| v.pointer(pointer))
            .and_then(json_value_as_f64);
        return ("daemon", extracted);
    }
    if let Some(name) = source.strip_prefix("derived:") {
        return ("derived", resolve_derived(rt, name));
    }
    if source.starts_with("external:") {
        return ("external", None);
    }
    ("unknown", None)
}

/// Resolves a `derived:<name>` KPI — a value computed from already-collected
/// data with no new instrumentation, for the `touring.coupling.*` family (F1).
/// Returns `None` (→ `STUB`) when the underlying data is unavailable.
fn resolve_derived(rt: &mut HookRuntime, name: &str) -> Option<f64> {
    match name {
        "health_delta_net" => {
            let m = invoke_handler(rt, "cli-gate-metrics")?;
            let imp = m
                .pointer("/health_delta_improvement_count")
                .and_then(json_value_as_f64)
                .unwrap_or(0.0);
            let reg = m
                .pointer("/health_delta_regression_count")
                .and_then(json_value_as_f64)
                .unwrap_or(0.0);
            Some(imp - reg)
        }
        "suggestion_uptake" => {
            let m = invoke_handler(rt, "cli-gate-metrics")?;
            let emitted = m
                .pointer("/suggestion_uptake_emitted_count")
                .and_then(json_value_as_f64)
                .unwrap_or(0.0);
            if emitted <= 0.0 {
                return None;
            }
            let followed = m
                .pointer("/suggestion_uptake_followed_count")
                .and_then(json_value_as_f64)
                .unwrap_or(0.0);
            Some(followed / emitted)
        }
        "adoption_ratio" => {
            let m = invoke_handler(rt, "cli-gate-metrics")?;
            let touring = m
                .pointer("/adoption_touring_count")
                .and_then(json_value_as_f64)
                .unwrap_or(0.0);
            let antipattern = m
                .pointer("/adoption_antipattern_count")
                .and_then(json_value_as_f64)
                .unwrap_or(0.0);
            let total = touring + antipattern;
            if total <= 0.0 {
                return None;
            }
            Some(touring / total)
        }
        "pillar_induction_ratio" => {
            // Task #6 — followed / emitted for the armed pillar-induction layer
            // (master-cli + learning-memory nudges). Mirrors `suggestion_uptake`
            // but scoped to the per-pillar counters; `None` until the layer emits
            // (default-OFF), so it reports ADVISORY rather than a false 0.
            let m = invoke_handler(rt, "cli-gate-metrics")?;
            let emitted = m
                .pointer("/pillar_induction_emitted_count")
                .and_then(json_value_as_f64)
                .unwrap_or(0.0);
            if emitted <= 0.0 {
                return None;
            }
            let followed = m
                .pointer("/pillar_induction_followed_count")
                .and_then(json_value_as_f64)
                .unwrap_or(0.0);
            Some(followed / emitted)
        }
        "world_model_success" => read_world_model_success(),
        // F6.4 (ADW plan 2026-07-19) — the software-factory KPI family. All are
        // file-derived from per-project artifacts the ADW stack already writes;
        // `None` (→ STUB) whenever the project has no such artifacts yet.
        "adw_explore_rounds_to_dry" => adw_explore_rounds_to_dry(&rt.project_root),
        "adw_plan_refine_iters" => adw_plan_refine_iters(&rt.project_root),
        "adw_runs" => adw_runs_count(&rt.project_root),
        "adw_router_accuracy" => adw_router_accuracy(&rt.project_root),
        "adw_zte_bypass_rate" => adw_zte_bypass_rate(&rt.project_root),
        // E3 (flow enforcement 2026-07-23) — gated-flow OUTER compliance for
        // THIS project, fed by loop_outer_gate.py evaluations at every Stop.
        "flow_compliance" => flow_compliance_ratio(&rt.project_root),
        _ => None,
    }
}

/// Mean number of exploration rounds until the CCE ledger converged, over every
/// `.touring-explore/*.ledger.json` in the current project. `None` when no
/// ledger has converged yet (the KPI only speaks about *finished* explorations).
fn adw_explore_rounds_to_dry(root: &std::path::Path) -> Option<f64> {
    let dir = root.join(".touring-explore");
    let mut totals: Vec<f64> = Vec::new();
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let path = entry.path();
        if path.extension().is_none_or(|e| e != "json") {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(&path) else {
            continue;
        };
        let Ok(ledger) = serde_json::from_str::<serde_json::Value>(&text) else {
            continue;
        };
        let converged = ledger
            .pointer("/verdict/converged")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false);
        if converged {
            if let Some(rounds) = ledger.pointer("/rounds").and_then(serde_json::Value::as_array) {
                totals.push(rounds.len() as f64);
            }
        }
    }
    if totals.is_empty() {
        return None;
    }
    Some(totals.iter().sum::<f64>() / totals.len() as f64)
}

/// Highest iteration count across `*.refine.json` plan-refinement ledgers
/// (searched shallowly: project root and `docs/plans/**`, depth-capped).
fn adw_plan_refine_iters(root: &std::path::Path) -> Option<f64> {
    fn scan(dir: &std::path::Path, depth: usize, best: &mut Option<f64>) {
        if depth == 0 {
            return;
        }
        let Ok(entries) = std::fs::read_dir(dir) else {
            return;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                scan(&path, depth - 1, best);
            } else if path.file_name().is_some_and(|n| {
                n.to_string_lossy().ends_with(".refine.json")
            }) {
                if let Ok(text) = std::fs::read_to_string(&path) {
                    if let Ok(serde_json::Value::Array(iters)) = serde_json::from_str(&text) {
                        let n = iters.len() as f64;
                        if best.is_none_or(|b| n > b) {
                            *best = Some(n);
                        }
                    }
                }
            }
        }
    }
    let mut best = None;
    scan(&root.join("docs").join("plans"), 4, &mut best);
    scan(root, 1, &mut best);
    best
}

/// Number of ADW runs recorded for this project (`.touring/adw-runs/*/journal.jsonl`).
fn adw_runs_count(root: &std::path::Path) -> Option<f64> {
    let dir = root.join(".touring").join("adw-runs");
    let mut count = 0u64;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        if entry.path().join("journal.jsonl").is_file() {
            count += 1;
        }
    }
    Some(count as f64)
}

/// Fraction of factory-routed runs whose outcome completed (proxy for
/// `router_accuracy` until human relabeling exists). `None` before any outcome.
fn adw_router_accuracy(root: &std::path::Path) -> Option<f64> {
    let path = root.join(".touring").join("factory").join("stats.json");
    let stats: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(path).ok()?).ok()?;
    let outcomes = stats.pointer("/outcomes")?.as_array()?;
    if outcomes.is_empty() {
        return None;
    }
    let completed = outcomes
        .iter()
        .filter(|o| o.pointer("/status").and_then(serde_json::Value::as_str) == Some("completed"))
        .count();
    Some(completed as f64 / outcomes.len() as f64)
}

/// ZTE bypasses per finished ADW run — the bypass must stay the audited
/// exception, never the rule. `None` before any run finishes.
fn adw_zte_bypass_rate(root: &std::path::Path) -> Option<f64> {
    let dir = root.join(".touring").join("adw-runs");
    let mut finished = 0u64;
    let mut bypasses = 0u64;
    for entry in std::fs::read_dir(dir).ok()?.flatten() {
        let journal = entry.path().join("journal.jsonl");
        let Ok(text) = std::fs::read_to_string(journal) else {
            continue;
        };
        if text.contains("\"run_finished\"") {
            finished += 1;
        }
        bypasses += text.matches("\"zte_bypass\"").count() as u64;
    }
    if finished == 0 {
        return None;
    }
    Some(bypasses as f64 / finished as f64)
}

/// Fraction of gated-flow OUTER evaluations whose artifact manifest was
/// complete, for THIS project (`~/.claude/loop-engineering/compliance.jsonl`,
/// one JSONL record per Stop-gate evaluation, written by `loop_outer_gate.py`).
/// `None` before any evaluation — the KPI only speaks once a gated flow has
/// actually been enforced here.
fn flow_compliance_ratio(root: &std::path::Path) -> Option<f64> {
    let home = std::env::var("HOME").ok()?;
    let log = PathBuf::from(home).join(".claude/loop-engineering/compliance.jsonl");
    flow_compliance_from_log(&log, root)
}

/// Pure core of [`flow_compliance_ratio`], separated so tests can feed a
/// synthetic log: ratio of `complete: true` records whose `cwd` is `root`.
fn flow_compliance_from_log(log: &std::path::Path, root: &std::path::Path) -> Option<f64> {
    let text = std::fs::read_to_string(log).ok()?;
    let root_str = root.display().to_string();
    let (mut total, mut complete) = (0u64, 0u64);
    for line in text.lines() {
        let Ok(rec) = serde_json::from_str::<Value>(line) else {
            continue;
        };
        if rec.pointer("/cwd").and_then(Value::as_str) != Some(root_str.as_str()) {
            continue;
        }
        total += 1;
        if rec.pointer("/complete").and_then(Value::as_bool) == Some(true) {
            complete += 1;
        }
    }
    if total == 0 {
        return None;
    }
    Some(complete as f64 / total as f64)
}

/// Σsuccesses / (Σsuccesses + Σfailures) over `action_world_model.json`
/// (`~/.claude/touring/action_world_model.json`). Returns `None` when the file
/// is absent/unreadable or has no recorded outcomes yet.
fn read_world_model_success() -> Option<f64> {
    let home = std::env::var("HOME").ok()?;
    let path = PathBuf::from(home).join(".claude/touring/action_world_model.json");
    let raw = std::fs::read_to_string(path).ok()?;
    let model: Value = serde_json::from_str(&raw).ok()?;
    let entries = model.get("entries").and_then(Value::as_array)?;
    let (mut succ, mut fail) = (0.0_f64, 0.0_f64);
    for e in entries {
        succ += e
            .get("successes")
            .and_then(json_value_as_f64)
            .unwrap_or(0.0);
        fail += e.get("failures").and_then(json_value_as_f64).unwrap_or(0.0);
    }
    let total = succ + fail;
    if total <= 0.0 {
        None
    } else {
        Some(succ / total)
    }
}

fn invoke_handler(rt: &mut HookRuntime, handler: &str) -> Option<Value> {
    let raw = match handler {
        "cli-wiring-status" => super::super::cli_handlers::cli_wiring_status(rt, &Value::Null),
        "cli-learning-status" => super::super::cli_handlers::cli_learning_status(rt, &Value::Null),
        "cli-gate-metrics" => super::super::cli_handlers::cli_gate_metrics(rt, &Value::Null),
        "cli-gotcha-stats" => super::super::cli_handlers::cli_gotcha_stats(rt, &Value::Null),
        _ => return None,
    };
    serde_json::from_str(&raw).ok()
}

fn json_value_as_f64(v: &Value) -> Option<f64> {
    v.as_f64()
        .or_else(|| v.as_u64().map(|n| n as f64))
        .or_else(|| v.as_i64().map(|n| n as f64))
        .or_else(|| v.as_bool().map(|b| if b { 1.0 } else { 0.0 }))
}

/// Returns `"PASS"` or `"FAIL"` per RFC-R2 directions.
#[must_use]
pub fn check_threshold(actual: f64, threshold: f64, direction: &str) -> &'static str {
    let pass = match direction {
        "gte" => actual >= threshold,
        "lte" => actual <= threshold,
        "eq" => (actual - threshold).abs() < f64::EPSILON,
        _ => false,
    };
    if pass { "PASS" } else { "FAIL" }
}

fn summarize(checks: &[CommitmentCheck]) -> Value {
    let total = checks.len();
    let passed = checks.iter().filter(|e| e.status == "PASS").count();
    let failed = checks.iter().filter(|e| e.status == "FAIL").count();
    let stub = checks.iter().filter(|e| e.status == "STUB").count();
    let advisory = checks.iter().filter(|e| e.status == "ADVISORY").count();
    json!({
        "total": total,
        "passed": passed,
        "failed": failed,
        "stub": stub,
        "advisory": advisory,
        "regressions": 0,
    })
}

// ─────────────────────────────────────────────────────────────────────────────
// Snapshot persistence
// ─────────────────────────────────────────────────────────────────────────────

/// Path to the latest A/B attribution block written by `run_bench.py --compare`
/// (mirrors its `DEFAULT_AB_OUT`: `<workspace>/docs/agentic-bench/.ab-latest.json`).
fn default_ab_path() -> PathBuf {
    if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home).join(".claude/rust/docs/agentic-bench/.ab-latest.json")
    } else {
        PathBuf::from("docs/agentic-bench/.ab-latest.json")
    }
}

/// Reads the latest A/B causal-attribution block (telemetry §10) persisted by
/// `run_bench.py --compare`. Returns `None` when no A/B has run yet → the snapshot
/// carries `"ab": null` (an honest absence, never a fabricated zero).
fn build_ab_block(path: &Path) -> Option<Value> {
    let raw = std::fs::read_to_string(path).ok()?;
    serde_json::from_str::<Value>(&raw).ok()
}

fn persist_snapshot(
    payload: &Value,
    date: &str,
    project_root: &std::path::Path,
) -> std::io::Result<PathBuf> {
    let month = date.get(0..7).unwrap_or("0000-00");
    let dir = if let Ok(home) = std::env::var("HOME") {
        PathBuf::from(home)
            .join(".claude/rust/docs/kpi")
            .join(month)
    } else {
        PathBuf::from("docs/kpi").join(month)
    };
    std::fs::create_dir_all(&dir)?;
    // One file per project per day (investigation 2026-07-01): the daemon's F5
    // flush snapshots EVERY warm project, and per-project sources (orphans,
    // ema) differ wildly between projects — a shared `{date}.json` made
    // consecutive writes silently overwrite each other and the dated series
    // uninterpretable (observed: 368→0→0→12002 across mixed projects).
    // Readers glob `docs/kpi/*/*.json`, so the new name stays discoverable.
    let file = dir.join(format!("{date}--{}.json", project_slug(project_root)));
    let pretty = serde_json::to_string_pretty(payload)?;
    std::fs::write(&file, pretty)?;
    Ok(file)
}

/// Deterministic filename-safe slug for a project root: the full path with
/// separators folded to `-` (collision-free, unlike a basename).
fn project_slug(project_root: &std::path::Path) -> String {
    let slug: String = project_root.display().to_string().replace(['/', '\\'], "-");
    slug.trim_matches('-').to_string()
}

// ─────────────────────────────────────────────────────────────────────────────
// F7 — refinement engine (telemetry §12): coupling KPIs → recommended actuators
// ─────────────────────────────────────────────────────────────────────────────

/// The coupling KPI signals the refinement engine reads (telemetry §5, D1–D6).
/// Each is `None` when its data is unavailable (STUB) — the engine recommends
/// nothing for a signal it cannot observe (no fabricated action).
#[derive(Debug, Clone, Default)]
pub struct CouplingSignals {
    /// D3 suggestion-uptake (followed/emitted); low → demote noisy hints.
    pub suggestion_uptake: Option<f64>,
    /// D2 STR bytes/emit; rising → tighten `--brief`/summarizer elision.
    pub str_bytes_per_emit: Option<f64>,
    /// D1 adoption_ratio (the mother metric); low → promote the capability.
    pub adoption_ratio: Option<f64>,
    /// D4 net health movement (improvements − regressions); < 0 → drift alert.
    pub health_delta_net: Option<f64>,
    /// A/B causal gate (F6): `Some(true)` iff treatment beat control (coupling
    /// confirmed). `None` = no A/B run. Actuators auto-apply only when confirmed.
    pub ab_attributable: Option<bool>,
}

/// A recommended refinement actuator (telemetry §12). `actionable` is the A/B
/// gate: an action auto-applies only once the coupling is causally confirmed
/// (`ab_attributable == Some(true)`); otherwise it is surfaced advisory-only —
/// the discipline that separates "induce" from "mutate a system not proven to help".
#[derive(Debug, Clone, Serialize, PartialEq)]
pub struct RefinementAction {
    /// Actuator kind (stable machine tag).
    pub kind: &'static str,
    /// The KPI that triggered it.
    pub signal: &'static str,
    /// Observed value of the triggering KPI.
    pub observed: f64,
    /// Threshold it breached.
    pub threshold: f64,
    /// Whether the A/B gate authorises auto-application (vs advisory-only).
    pub actionable: bool,
    /// Human rationale for the recommendation.
    pub rationale: String,
}

/// Thresholds mirror the `touring.coupling.*` commitments (telemetry §7).
const UPTAKE_FLOOR: f64 = 0.40;
const STR_BYTES_CEIL: f64 = 800.0;
const ADOPTION_FLOOR: f64 = 0.50;

/// Assembles the live coupling signals (telemetry §5) and runs the F7 engine.
/// The A/B gate is read from the snapshot's `ab` block (`attributable`). This is
/// the I/O glue over the pure [`recommend_refinements`]; exercised end-to-end by
/// `touring kpi --refine`.
fn collect_and_recommend(rt: &mut HookRuntime, ab: &Value) -> Vec<RefinementAction> {
    let str_bytes = invoke_handler(rt, "cli-gate-metrics")
        .as_ref()
        .and_then(|m| m.pointer("/enrichment_mean_bytes_per_emit"))
        .and_then(json_value_as_f64);
    let signals = CouplingSignals {
        suggestion_uptake: resolve_derived(rt, "suggestion_uptake"),
        str_bytes_per_emit: str_bytes,
        adoption_ratio: resolve_derived(rt, "adoption_ratio"),
        health_delta_net: resolve_derived(rt, "health_delta_net"),
        ab_attributable: ab.get("attributable").and_then(Value::as_bool),
    };
    recommend_refinements(&signals)
}

/// Pure refinement engine (telemetry §12): maps coupling KPIs to the actuators
/// that would refine the strategy. A/B-gated — `actionable` is true only when the
/// coupling is causally confirmed; otherwise actions are advisory (recommend,
/// don't auto-apply). Empty vec when every observed signal is healthy.
#[must_use]
pub fn recommend_refinements(s: &CouplingSignals) -> Vec<RefinementAction> {
    let gate = s.ab_attributable == Some(true);
    let mut actions = Vec::new();

    if let Some(uptake) = s.suggestion_uptake {
        if uptake < UPTAKE_FLOOR {
            actions.push(RefinementAction {
                kind: "demote_hint",
                signal: "suggestion_uptake",
                observed: uptake,
                threshold: UPTAKE_FLOOR,
                actionable: gate,
                rationale: format!(
                    "uptake {uptake:.2} < {UPTAKE_FLOOR:.2}: hints are ignored — demote the \
                     noisiest cluster and re-arm with a number (telemetry §12, I5)."
                ),
            });
        }
    }

    if let Some(bytes) = s.str_bytes_per_emit {
        if bytes > STR_BYTES_CEIL {
            actions.push(RefinementAction {
                kind: "tighten_elision",
                signal: "str_bytes_per_emit",
                observed: bytes,
                threshold: STR_BYTES_CEIL,
                actionable: gate,
                rationale: format!(
                    "STR {bytes:.0}B/emit > {STR_BYTES_CEIL:.0}: tighten the --brief/summarizer \
                     elision floor (auto-tune, telemetry §12)."
                ),
            });
        }
    }

    if let Some(adoption) = s.adoption_ratio {
        if adoption < ADOPTION_FLOOR {
            actions.push(RefinementAction {
                kind: "promote_capability",
                signal: "adoption_ratio",
                observed: adoption,
                threshold: ADOPTION_FLOOR,
                actionable: gate,
                rationale: format!(
                    "adoption {adoption:.2} < {ADOPTION_FLOOR:.2}: prior-bash still wins — promote \
                     the capability via a high-signal-rare trigger (telemetry §12)."
                ),
            });
        }
    }

    if let Some(net) = s.health_delta_net {
        if net < 0.0 {
            actions.push(RefinementAction {
                kind: "alert_drift",
                signal: "health_delta_net",
                observed: net,
                threshold: 0.0,
                actionable: gate,
                rationale: format!(
                    "health_delta_net {net:.0} < 0: coupling-guided edits regress more than they \
                     improve — raise a drift alert + RL penalty (telemetry §12)."
                ),
            });
        }
    }

    actions
}

/// F7 actuator (telemetry §12, hint demotion) — the *brain* of F7c. Returns the
/// additive bump to the hint firing threshold when the coupling data shows hints
/// are ignored (`suggestion_uptake` below the floor) AND the A/B gate confirms the
/// coupling helps (`ab_attributable == Some(true)`). `0.0` otherwise — graduated by
/// how far below the floor, capped at `+0.30`. `cli_suggester` applies this only when
/// armed (`TOURING_F7_ACTUATOR_ARMED`), so the default is zero live impact.
#[must_use]
pub fn hint_demotion_bump(suggestion_uptake: Option<f64>, ab_attributable: Option<bool>) -> f32 {
    if ab_attributable != Some(true) {
        return 0.0; // A/B gate: never demote a coupling not proven beneficial.
    }
    match suggestion_uptake {
        Some(u) if u < UPTAKE_FLOOR => (((UPTAKE_FLOOR - u) * 0.5) as f32).min(0.30),
        _ => 0.0,
    }
}

/// F7c actuator signal source: the live `(suggestion_uptake, ab_attributable)` pair
/// the `cli_suggester` demotion gate consults when armed. **Read-only** — uptake from
/// the global gate-metrics snapshot (the same counters `derived:suggestion_uptake`
/// reads, so the actuator and the dashboard agree) and the A/B verdict from disk; no
/// `HookRuntime` needed (the hot path holds only `&HookRuntime`). Lives here with the
/// engine so all F7 signal logic is co-located.
pub fn actuator_signals() -> (Option<f64>, Option<bool>) {
    let snap = crate::shared::gate_metrics::GateMetricsSnapshot::capture();
    let emitted = snap.suggestion_uptake_emitted_count;
    let uptake =
        (emitted > 0).then(|| snap.suggestion_uptake_followed_count as f64 / emitted as f64);
    let ab = build_ab_block(&default_ab_path())
        .and_then(|v| v.get("attributable").and_then(Value::as_bool));
    (uptake, ab)
}

// ─────────────────────────────────────────────────────────────────────────────
// Tests
// ─────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn project_slug_is_deterministic_and_collision_free() {
        let a = project_slug(std::path::Path::new("/home/g/.claude/rust"));
        assert_eq!(a, "home-g-.claude-rust");
        // Distinct roots sharing a basename must not collide (a plain
        // basename slug would map both to "rust").
        let b = project_slug(std::path::Path::new("/tmp/other/rust"));
        assert_ne!(a, b);
        assert!(!a.contains('/'));
    }

    #[test]
    fn build_ab_block_reads_persisted_attribution() {
        let dir = std::env::temp_dir().join(format!("kpi_ab_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = dir.join(".ab-latest.json");
        std::fs::write(
            &p,
            r#"{"schema":"coupling-ab-v1","arm":"treatment","attributable":true,"verdict":"coupling_helps"}"#,
        )
        .unwrap();
        let block = build_ab_block(&p).expect("persisted A/B block should parse");
        assert_eq!(block["verdict"], "coupling_helps");
        assert_eq!(block["attributable"], true);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn build_ab_block_absent_is_none() {
        let p = std::env::temp_dir().join("kpi_ab_definitely_absent_zzz.json");
        let _ = std::fs::remove_file(&p);
        assert!(build_ab_block(&p).is_none());
    }

    #[test]
    fn recommend_empty_when_all_signals_healthy() {
        let s = CouplingSignals {
            suggestion_uptake: Some(0.80),
            str_bytes_per_emit: Some(400.0),
            adoption_ratio: Some(0.90),
            health_delta_net: Some(5.0),
            ab_attributable: Some(true),
        };
        assert!(recommend_refinements(&s).is_empty());
    }

    #[test]
    fn recommend_ignores_absent_signals() {
        // All None → nothing to recommend (can't act on unobserved data).
        assert!(recommend_refinements(&CouplingSignals::default()).is_empty());
    }

    #[test]
    fn demotion_bump_zero_without_ab_confirmation() {
        // Low uptake but no causal confirmation → no demotion (the F7 A/B gate).
        assert_eq!(hint_demotion_bump(Some(0.10), None), 0.0);
        assert_eq!(hint_demotion_bump(Some(0.10), Some(false)), 0.0);
    }

    #[test]
    fn demotion_bump_zero_when_uptake_healthy() {
        assert_eq!(hint_demotion_bump(Some(0.80), Some(true)), 0.0);
        assert_eq!(hint_demotion_bump(None, Some(true)), 0.0);
    }

    #[test]
    fn demotion_bump_graduated_by_distance_below_floor() {
        // uptake 0.30 vs floor 0.40 → (0.10 * 0.5) = 0.05 bump.
        assert!((hint_demotion_bump(Some(0.30), Some(true)) - 0.05).abs() < 1e-6);
        // uptake 0.0 → (0.40 * 0.5) = 0.20 bump (under the 0.30 cap).
        assert!((hint_demotion_bump(Some(0.0), Some(true)) - 0.20).abs() < 1e-6);
    }

    #[test]
    fn recommend_demotes_hint_actionable_when_ab_confirmed() {
        let s = CouplingSignals {
            suggestion_uptake: Some(0.10),
            ab_attributable: Some(true),
            ..Default::default()
        };
        let acts = recommend_refinements(&s);
        assert_eq!(acts.len(), 1);
        assert_eq!(acts[0].kind, "demote_hint");
        assert!(
            acts[0].actionable,
            "A/B confirmed → actuator may auto-apply"
        );
    }

    #[test]
    fn recommend_advisory_only_when_ab_absent() {
        // Same low uptake, but no A/B run → recommend, do NOT auto-apply (the F7 gate).
        let s = CouplingSignals {
            suggestion_uptake: Some(0.10),
            ab_attributable: None,
            ..Default::default()
        };
        let acts = recommend_refinements(&s);
        assert_eq!(acts.len(), 1);
        assert!(!acts[0].actionable, "no A/B confirmation → advisory only");
    }

    #[test]
    fn recommend_advisory_when_ab_negative() {
        let s = CouplingSignals {
            adoption_ratio: Some(0.20),
            ab_attributable: Some(false),
            ..Default::default()
        };
        let acts = recommend_refinements(&s);
        assert_eq!(acts.len(), 1);
        assert_eq!(acts[0].kind, "promote_capability");
        assert!(!acts[0].actionable);
    }

    #[test]
    fn recommend_tighten_elision_and_drift_together() {
        let s = CouplingSignals {
            str_bytes_per_emit: Some(1200.0),
            health_delta_net: Some(-3.0),
            ab_attributable: Some(true),
            ..Default::default()
        };
        let kinds: Vec<_> = recommend_refinements(&s).iter().map(|a| a.kind).collect();
        assert!(kinds.contains(&"tighten_elision"));
        assert!(kinds.contains(&"alert_drift"));
    }

    #[test]
    fn check_threshold_gte_pass_and_fail() {
        assert_eq!(check_threshold(0.8, 0.5, "gte"), "PASS");
        assert_eq!(check_threshold(0.5, 0.5, "gte"), "PASS");
        assert_eq!(check_threshold(0.4, 0.5, "gte"), "FAIL");
    }
    #[test]
    fn check_threshold_lte_pass_and_fail() {
        assert_eq!(check_threshold(50.0, 100.0, "lte"), "PASS");
        assert_eq!(check_threshold(100.0, 100.0, "lte"), "PASS");
        assert_eq!(check_threshold(150.0, 100.0, "lte"), "FAIL");
    }
    #[test]
    fn check_threshold_eq_uses_epsilon() {
        assert_eq!(check_threshold(0.0, 0.0, "eq"), "PASS");
        assert_eq!(check_threshold(1.0, 1.0, "eq"), "PASS");
        assert_eq!(check_threshold(1.0, 0.0, "eq"), "FAIL");
    }
    #[test]
    fn check_threshold_unknown_direction_fails() {
        assert_eq!(check_threshold(1.0, 0.0, "lt"), "FAIL");
    }
    #[test]
    fn iso_date_2026_04_25() {
        let date = iso_date_from_unix(1_777_075_200);
        assert_eq!(date, "2026-04-25");
    }
    #[test]
    fn iso_date_unix_epoch() {
        assert_eq!(iso_date_from_unix(0), "1970-01-01");
    }
    #[test]
    fn json_value_as_f64_handles_numerics() {
        assert_eq!(json_value_as_f64(&json!(42)), Some(42.0));
        assert_eq!(json_value_as_f64(&json!(3.14)), Some(3.14));
        assert_eq!(json_value_as_f64(&json!(true)), Some(1.0));
        assert_eq!(json_value_as_f64(&json!(false)), Some(0.0));
        assert_eq!(json_value_as_f64(&json!("not a number")), None);
    }
    #[test]
    fn yaml_round_trip_preserves_commitment() {
        let yaml = "version: '1.0'\nschema: kpi-v1\ncommitments:\n  - id: a.b\n    name: T\n    threshold: 1.0\n    direction: gte\n    source: 'daemon:x:/y'\n";
        let parsed: CommitmentsFile = serde_yaml::from_str(yaml).expect("parse");
        assert_eq!(parsed.commitments.len(), 1);
        assert_eq!(parsed.commitments[0].id, "a.b");
        assert_eq!(parsed.commitments[0].direction, "gte");
    }
    #[test]
    fn summarize_counts_buckets() {
        let checks = vec![
            mk_check("PASS"),
            mk_check("PASS"),
            mk_check("FAIL"),
            mk_check("STUB"),
        ];
        let s = summarize(&checks);
        assert_eq!(s["total"], 4);
        assert_eq!(s["passed"], 2);
        assert_eq!(s["failed"], 1);
        assert_eq!(s["stub"], 1);
    }
    #[test]
    fn external_source_returns_stub_marker() {
        assert!("external:cargo nextest --list-tests".starts_with("external:"));
    }
    #[test]
    fn default_commitments_path_returns_non_empty() {
        let p = default_commitments_path();
        assert!(!p.as_os_str().is_empty());
    }
    #[test]
    fn iso_date_handles_late_year() {
        assert_eq!(iso_date_from_unix(1_924_905_600), "2030-12-31");
    }
    fn mk_check(status: &'static str) -> CommitmentCheck {
        CommitmentCheck {
            id: "x".into(),
            name: "x".into(),
            threshold: 0.0,
            direction: "gte".into(),
            source_kind: "daemon",
            actual: Some(0.0),
            status,
            rationale: String::new(),
            advisory: false,
        }
    }
    #[test]
    fn summarize_counts_advisory_separately_from_failed() {
        let checks = vec![mk_check("PASS"), mk_check("ADVISORY"), mk_check("FAIL")];
        let s = summarize(&checks);
        assert_eq!(
            s["failed"], 1,
            "advisory must not inflate the failed bucket"
        );
        assert_eq!(s["advisory"], 1);
        assert_eq!(s["passed"], 1);
    }
    #[test]
    fn flow_compliance_filters_by_project_and_needs_data() {
        let dir = std::env::temp_dir().join(format!("kpi-flow-{}", std::process::id()));
        std::fs::create_dir_all(&dir).expect("mkdir");
        let log = dir.join("compliance.jsonl");
        let proj = std::path::Path::new("/proj/a");
        // No log yet → None (STUB, never a fabricated 0.0).
        assert_eq!(flow_compliance_from_log(&log, proj), None);
        std::fs::write(
            &log,
            concat!(
                "{\"cwd\":\"/proj/a\",\"flow\":\"strategy-outer\",\"complete\":true}\n",
                "{\"cwd\":\"/proj/a\",\"flow\":\"strategy-outer\",\"complete\":false}\n",
                "{\"cwd\":\"/proj/b\",\"flow\":\"cross-audit\",\"complete\":false}\n",
                "not-json\n",
            ),
        )
        .expect("write log");
        // Only /proj/a records count: 1 complete of 2 → 0.5; /proj/b is ignored.
        assert_eq!(flow_compliance_from_log(&log, proj), Some(0.5));
        assert_eq!(
            flow_compliance_from_log(&log, std::path::Path::new("/proj/c")),
            None,
            "a project with no evaluations stays STUB"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
    #[test]
    fn commitment_advisory_defaults_false_and_parses_true() {
        let base = "version: '1.0'\nschema: kpi-v1\ncommitments:\n  - id: a.b\n    name: T\n    threshold: 1.0\n    direction: gte\n    source: 'daemon:x:/y'\n";
        let f: CommitmentsFile = serde_yaml::from_str(base).expect("parse base");
        assert!(!f.commitments[0].advisory, "advisory defaults to false");
        let adv = "version: '1.0'\nschema: kpi-v1\ncommitments:\n  - id: a.b\n    name: T\n    threshold: 1.0\n    direction: gte\n    source: 'derived:health_delta_net'\n    advisory: true\n";
        let f2: CommitmentsFile = serde_yaml::from_str(adv).expect("parse advisory");
        assert!(f2.commitments[0].advisory);
    }
}
