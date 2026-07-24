//! E2E integration tests for generator-hooks synergy.
//!
//! Validates the wiring between touring-generator commit pipeline and the
//! touring-hooks task lifecycle system:
//! - `build_consumer_generator_plans` JSON shape contract
//! - `touring generate consumer-wiring` CLI exit-0 contract
//! - `touring cortex task-completed` fire-and-forget exit-0 contract for task-created events
//! - `touring cortex task-completed` fire-and-forget exit-0 contract for task-metrics events
//! - `touring cortex task-completed` fire-and-forget exit-0 contract for task-validation events
//! - limit parameter is respected by `build_consumer_generator_plans`
//!
//! TC-B3/B4/B5 verify the `emit_cortex_event` path used by the three `inject_task_*_hook`
//! functions. All route through `touring cortex task-completed` (HookSilent — always exits 0)
//! with JSON piped via stdin.

use std::process::Command;
use touring_server::tools::generator_tools::build_consumer_generator_plans;

/// Helper: run the `touring` binary with given args and capture output.
/// Mirrors the pattern in `binary_e2e.rs`.
fn run_touring(args: &[&str], stdin_data: &str) -> (i32, String, String) {
    let binary = std::env::var("TOURING_BINARY").unwrap_or_else(|_| {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        // Navigate: touring-server/ -> crates/ -> workspace root
        let crates_dir = std::path::Path::new(manifest_dir)
            .parent()
            .expect("touring-server parent must exist");
        let workspace = crates_dir
            .parent()
            .expect("crates parent (workspace) must exist");
        let debug = workspace.join("target/debug/touring");
        let release = workspace.join("target/release/touring");
        if release.exists() {
            release.to_string_lossy().to_string()
        } else {
            debug.to_string_lossy().to_string()
        }
    });

    let mut cmd = Command::new(&binary);
    cmd.args(args);
    cmd.stdin(std::process::Stdio::piped());
    cmd.stdout(std::process::Stdio::piped());
    cmd.stderr(std::process::Stdio::piped());

    let mut child = cmd
        .spawn()
        .expect("failed to spawn touring binary — ensure it is built");

    if !stdin_data.is_empty() {
        use std::io::Write;
        let stdin = child.stdin.as_mut().expect("stdin pipe must be open");
        stdin
            .write_all(stdin_data.as_bytes())
            .expect("write to stdin must succeed");
    }
    drop(child.stdin.take());

    let output = child
        .wait_with_output()
        .expect("wait_with_output must succeed");
    let code = output.status.code().unwrap_or(-1);
    let stdout = String::from_utf8_lossy(&output.stdout).to_string();
    let stderr = String::from_utf8_lossy(&output.stderr).to_string();
    (code, stdout, stderr)
}

// ── TC-B1: build_consumer_generator_plans JSON shape contract ────────────────

/// TC-B1: `build_consumer_generator_plans` always returns a JSON object with the
/// shape `{ok, count, plans}`. The `ok` field must be boolean; `plans` must
/// be an array. count must equal plans.len(). No missing fields.
#[test]
fn test_build_consumer_generator_plans_returns_valid_shape() {
    let result = build_consumer_generator_plans(5);

    assert!(
        result.is_object(),
        "build_consumer_generator_plans must return a JSON object, got: {result}"
    );
    assert!(
        result.get("ok").is_some(),
        "result must have 'ok' field, got: {result}"
    );
    assert!(
        result["ok"].is_boolean(),
        "result['ok'] must be boolean, got: {}",
        result["ok"]
    );

    if result["ok"].as_bool().unwrap_or(false) {
        // Success path: count + plans are required.
        assert!(
            result["count"].is_number(),
            "result['count'] must be a number, got: {}",
            result["count"]
        );
        assert!(
            result["plans"].is_array(),
            "result['plans'] must be an array, got: {}",
            result["plans"]
        );

        let plans = result["plans"]
            .as_array()
            .expect("plans is_array() already asserted above");
        let count = result["count"].as_u64().unwrap_or(u64::MAX);
        assert_eq!(
            count,
            plans.len() as u64,
            "result['count'] ({count}) must equal plans.len() ({})",
            plans.len()
        );

        // Verify per-plan schema.
        for (i, plan) in plans.iter().enumerate() {
            assert!(
                plan.get("plan_id").is_some(),
                "plan[{i}] must have 'plan_id', got: {plan}"
            );
            assert!(
                plan.get("kind").and_then(|v| v.as_str()) == Some("ConsumerGenerator"),
                "plan[{i}]['kind'] must be 'ConsumerGenerator', got: {plan}"
            );
            assert!(
                plan.get("intent").is_some(),
                "plan[{i}] must have 'intent', got: {plan}"
            );
            assert!(
                plan.get("contracts").is_some(),
                "plan[{i}] must have 'contracts', got: {plan}"
            );
        }
    } else {
        // Error path: must have `error` field.
        assert!(
            result.get("error").is_some(),
            "failed result must have 'error' field, got: {result}"
        );
    }
}

// ── TC-B2: consumer-wiring CLI exits 0 ───────────────────────────────────────

/// TC-B2: `touring generate consumer-wiring --limit 1` exits 0 and returns JSON
/// with the shape `{ok, count, plans}`. The CLI never exits non-zero regardless
/// of wiring orphan state.
#[test]
fn test_consumer_wiring_cli_exits_0() {
    let (code, stdout, stderr) =
        run_touring(&["generate", "consumer-wiring", "--limit", "1", "-j"], "");

    assert_eq!(
        code, 0,
        "touring generate consumer-wiring must exit 0, stderr: {stderr}"
    );
    assert!(
        !stdout.is_empty(),
        "consumer-wiring must produce JSON output"
    );

    let json: serde_json::Value =
        serde_json::from_str(&stdout).expect("consumer-wiring output must be valid JSON");

    assert!(
        json.get("ok").is_some(),
        "consumer-wiring JSON must have 'ok' field, got: {json}"
    );
    assert!(
        json["plans"].is_array(),
        "consumer-wiring JSON 'plans' must be an array, got: {json}"
    );
}

// ── TC-B3: cortex task-completed accepts task-created payload and exits 0 ──────

/// TC-B3: `touring cortex task-completed` exits 0 when piped a task-created payload.
///
/// Matches the `emit_cortex_event` path used by `inject_task_created_hook`.
/// `touring cortex` uses `ErrorPolicy::HookSilent` — it exits 0 even when the
/// daemon is unavailable. The payload is piped via stdin (not as a CLI argument).
#[test]
fn test_hook_task_created_accepts_payload_exits_0() {
    let payload = serde_json::json!({
        "task_id": "test-gen-plan-12345",
        "task_subject": "E2E test: wire orphan symbol",
        "session_id": "gen-test-gen-plan-12345",
        "teammate_name": "touring-generator",
        "team_name": "taco-generator",
        "hook_event_name": "TaskCreated",
        "status": "CREATED",
    })
    .to_string();

    let (code, _stdout, stderr) = run_touring(&["cortex", "task-completed"], &payload);

    assert_eq!(
        code, 0,
        "touring cortex task-completed (task-created payload) must exit 0 (HookSilent contract), stderr: {stderr}"
    );
}

// ── TC-B4: cortex task-completed accepts task-metrics payload and exits 0 ──────

/// TC-B4: `touring cortex task-completed` exits 0 when piped a task-metrics payload.
///
/// Matches the `emit_cortex_event` path used by `inject_task_metrics_hook`.
#[test]
fn test_hook_task_metrics_accepts_payload_exits_0() {
    let payload = serde_json::json!({
        "task_id": "test-gen-plan-12345",
        "completion_time": 1.5_f64,
        "subtask_count": 3_i64,
        "success_rate": 1.0_f64,
        "quality_score": 1.0_f64,
        "status": "METRICS",
    })
    .to_string();

    let (code, _stdout, stderr) = run_touring(&["cortex", "task-completed"], &payload);

    assert_eq!(
        code, 0,
        "touring cortex task-completed (task-metrics payload) must exit 0 (HookSilent contract), stderr: {stderr}"
    );
}

// ── TC-B5: cortex task-completed accepts task-validation payload and exits 0 ───

/// TC-B5: `touring cortex task-completed` exits 0 when piped a task-validation payload.
///
/// Matches the `emit_cortex_event` path used by `inject_task_validation_hook`.
#[test]
fn test_hook_task_validation_accepts_payload_exits_0() {
    let payload = serde_json::json!({
        "task_id": "test-gen-plan-12345",
        "subtask_count": 3_i64,
        "status": "VALIDATED",
    })
    .to_string();

    let (code, _stdout, stderr) = run_touring(&["cortex", "task-completed"], &payload);

    assert_eq!(
        code, 0,
        "touring cortex task-completed (task-validation payload) must exit 0 (HookSilent contract), stderr: {stderr}"
    );
}

// ── TC-B6: limit parameter is respected ──────────────────────────────────────

/// TC-B6: `build_consumer_generator_plans(limit)` never returns more plans than
/// `limit`, regardless of the number of wiring orphans present in the index.
#[test]
fn test_generator_tools_build_consumer_plans_limit_respected() {
    for limit in [0_usize, 1, 3, 5] {
        let result = build_consumer_generator_plans(limit);

        assert!(
            result.is_object(),
            "build_consumer_generator_plans({limit}) must return a JSON object"
        );

        if result["ok"].as_bool().unwrap_or(false) {
            let plans = result["plans"]
                .as_array()
                .expect("on success, plans must be an array");
            assert!(
                plans.len() <= limit,
                "build_consumer_generator_plans({limit}) returned {} plans, expected <= {limit}",
                plans.len()
            );

            let reported = result["count"].as_u64().unwrap_or(u64::MAX) as usize;
            assert_eq!(
                reported,
                plans.len(),
                "count ({reported}) must equal plans.len() ({}) for limit={limit}",
                plans.len()
            );
        }
        // Error path (ok=false): limit enforcement not applicable; shape already verified above.
    }
}
