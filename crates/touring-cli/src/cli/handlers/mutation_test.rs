//! Wave T1 — `cli-mutation-test` hook handler.
//!
//! Thin daemon-side adapter on top of [`crate::mutation_test`]. Reads
//! payload flags, executes the mutation test (or returns cached result),
//! and emits a stable JSON envelope downstream consumers can parse:
//!
//! ```jsonc
//! {
//!   "ok": true,
//!   "cached": false,                // true when served from disk cache
//!   "package": "touring-ast",       // null when whole-workspace
//!   "mutants_total": 100,
//!   "mutants_killed": 80,
//!   "mutants_survived": 15,
//!   "mutants_timeout": 3,
//!   "mutants_unviable": 2,
//!   "kill_rate": 84.69,
//!   "elapsed_secs": 42,
//!   "passed_threshold": true,
//!   "threshold": 80.0,
//!   "cargo_mutants_version": "26.1.2"
//! }
//! ```
//!
//! Failure envelope (cargo-mutants missing, parse error, etc.):
//!
//! ```jsonc
//! { "ok": false, "error": "cargo-mutants binary not found ...", "kind": "binary_not_found" }
//! ```

use std::path::PathBuf;

use serde_json::{Value, json};

use crate::mutation_test::{
    MutationConfig, MutationError, MutationReport, cache_load, cache_path, cache_store,
    run_mutation_test,
};
use crate::runtime::HookRuntime;

/// `cli-mutation-test` handler.
///
/// Payload (all optional):
/// - `package`: string — restrict to a single cargo package
/// - `threshold`: number — kill-rate threshold in percent (default 80.0)
/// - `timeout_secs`: u32 — per-mutant timeout (default 60)
/// - `jobs`: u32 — parallel jobs (default = physical cores via lib)
/// - `workspace`: string — workspace path override (default = HookRuntime project root)
/// - `force`: bool — bypass cache and re-run (default false)
/// - `cache_only`: bool — return cached report or `{cached: false, ok: false}` (no run)
pub fn cli_mutation_test(rt: &mut HookRuntime, payload: &Value) -> String {
    let request = parse_payload(payload, &rt.project_root);
    let cache_root = touring_cache_root(&request.workspace);

    if !request.force
        && let Some(cached) = lookup_cache(&cache_root, request.package.as_deref())
    {
        return success_envelope(&cached, true);
    }

    if request.cache_only {
        return cache_miss_envelope(&cache_root, request.package.as_deref());
    }

    execute_and_cache(&request, &cache_root)
}

/// Resolved payload — flat struct keeps `cli_mutation_test` linear.
struct MutationRequest {
    workspace: PathBuf,
    package: Option<String>,
    threshold: f32,
    timeout_secs: u32,
    jobs_override: Option<u32>,
    force: bool,
    cache_only: bool,
}

fn parse_payload(payload: &Value, default_workspace: &std::path::Path) -> MutationRequest {
    MutationRequest {
        workspace: payload
            .get("workspace")
            .and_then(Value::as_str)
            .map(PathBuf::from)
            .unwrap_or_else(|| default_workspace.to_path_buf()),
        package: payload
            .get("package")
            .and_then(Value::as_str)
            .map(String::from),
        threshold: payload
            .get("threshold")
            .and_then(Value::as_f64)
            .map(|v| v as f32)
            .unwrap_or(80.0),
        timeout_secs: payload
            .get("timeout_secs")
            .and_then(Value::as_u64)
            .unwrap_or(60) as u32,
        jobs_override: payload
            .get("jobs")
            .and_then(Value::as_u64)
            .map(|v| v as u32),
        force: payload
            .get("force")
            .and_then(Value::as_bool)
            .unwrap_or(false),
        cache_only: payload
            .get("cache_only")
            .and_then(Value::as_bool)
            .unwrap_or(false),
    }
}

/// Soft cache lookup — corrupt entries log a warning and behave as miss.
fn lookup_cache(cache_root: &std::path::Path, package: Option<&str>) -> Option<MutationReport> {
    match cache_load(cache_root, package) {
        Ok(report) => report,
        Err(e) => {
            tracing::warn!(target: "touring::mutation_test", "cache load error: {e}");
            None
        }
    }
}

fn cache_miss_envelope(cache_root: &std::path::Path, package: Option<&str>) -> String {
    json!({
        "ok": false,
        "cached": false,
        "kind": "cache_miss",
        "error": "no fresh cache entry; pass force:true or run without cache_only",
        "cache_path": cache_path(cache_root, package).display().to_string(),
    })
    .to_string()
}

fn execute_and_cache(req: &MutationRequest, cache_root: &std::path::Path) -> String {
    let mut config = MutationConfig::new(req.workspace.clone());
    config.package = req.package.clone();
    config.threshold = req.threshold;
    config.timeout_secs = req.timeout_secs;
    if let Some(j) = req.jobs_override {
        config.jobs = j.max(1);
    }

    match run_mutation_test(&config) {
        Ok(report) => {
            if let Err(e) = cache_store(cache_root, req.package.as_deref(), &report) {
                tracing::warn!(target: "touring::mutation_test", "cache store error: {e}");
            }
            success_envelope(&report, false)
        }
        Err(e) => failure_envelope(&e),
    }
}

/// Resolve the touring cache root for mutation reports.
/// Mirrors the on-disk layout used by other waves (`.touring-cache/`
/// at the workspace root).
fn touring_cache_root(workspace: &std::path::Path) -> PathBuf {
    workspace.join(".touring-cache")
}

fn success_envelope(r: &MutationReport, cached: bool) -> String {
    json!({
        "ok": true,
        "cached": cached,
        "package": r.package,
        "mutants_total": r.mutants_total,
        "mutants_killed": r.mutants_killed,
        "mutants_survived": r.mutants_survived,
        "mutants_timeout": r.mutants_timeout,
        "mutants_unviable": r.mutants_unviable,
        "kill_rate": r.kill_rate,
        "elapsed_secs": r.elapsed_secs,
        "passed_threshold": r.passed_threshold,
        "threshold": r.threshold,
        "cargo_mutants_version": r.cargo_mutants_version,
    })
    .to_string()
}

fn failure_envelope(e: &MutationError) -> String {
    let kind = match e {
        MutationError::BinaryNotFound => "binary_not_found",
        MutationError::ExitFailed { .. } => "exit_failed",
        MutationError::OutcomesMissing(_) => "outcomes_missing",
        MutationError::OutcomesParse { .. } => "outcomes_parse",
        MutationError::Io(_) => "io",
    };
    json!({
        "ok": false,
        "error": e.to_string(),
        "kind": kind,
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn failure_envelope_binary_not_found_shape() {
        let s = failure_envelope(&MutationError::BinaryNotFound);
        let v: Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["ok"], false);
        assert_eq!(v["kind"], "binary_not_found");
        assert!(v["error"].as_str().unwrap().contains("cargo-mutants"));
    }

    #[test]
    fn success_envelope_round_trip() {
        let r = MutationReport {
            mutants_total: 10,
            mutants_killed: 8,
            mutants_survived: 2,
            mutants_timeout: 0,
            mutants_unviable: 0,
            kill_rate: 80.0,
            elapsed_secs: 5,
            passed_threshold: true,
            threshold: 80.0,
            package: Some("touring-ast".into()),
            cargo_mutants_version: "26.1.2".into(),
        };
        let s = success_envelope(&r, false);
        let v: Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["ok"], true);
        assert_eq!(v["cached"], false);
        assert_eq!(v["package"], "touring-ast");
        assert_eq!(v["passed_threshold"], true);
    }

    #[test]
    fn success_envelope_marks_cached_true() {
        let r = MutationReport {
            mutants_total: 10,
            mutants_killed: 8,
            mutants_survived: 2,
            mutants_timeout: 0,
            mutants_unviable: 0,
            kill_rate: 80.0,
            elapsed_secs: 5,
            passed_threshold: true,
            threshold: 80.0,
            package: None,
            cargo_mutants_version: "26.1.2".into(),
        };
        let s = success_envelope(&r, true);
        let v: Value = serde_json::from_str(&s).unwrap();
        assert_eq!(v["cached"], true);
        assert!(v["package"].is_null());
    }
}
