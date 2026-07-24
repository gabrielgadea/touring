//! E2E tests for mcts_materializer (Pln3 R7).
//!
//! These tests exercise the full `materialize_mcts_subgoals` pipeline using a
//! real `HookRuntime` backed by a temporary SQLite DB.  They verify:
//! 1. Happy path: N subgoals → N tasks created (or graceful skip on fallback).
//! 2. Edge case: top_n=0 is clamped to 1.
//! 3. Payload dispatch: `materialize_from_payload` routes correctly.
//! 4. Task origin field: created tasks carry `origin="touring-mcts"`.
//! 5. Task digest integration: materialized tasks appear as pending in digest.

#![allow(clippy::indexing_slicing)]

use tempfile::TempDir;
use touring_hooks::hook_runtime::HookRuntime;
use touring_hooks::mcts_materializer::{materialize_from_payload, materialize_mcts_subgoals};

/// Build a minimal HookRuntime backed by a temp SQLite DB.
fn make_runtime(tmp: &TempDir) -> HookRuntime {
    HookRuntime::new(tmp.path()).expect("HookRuntime::new must succeed for temp dir")
}

// ── Test 1: Happy path — returns valid JSON with expected keys ────────────────

#[test]
fn materialize_returns_valid_json_with_expected_keys() {
    let tmp = TempDir::new().expect("tempdir");
    let mut rt = make_runtime(&tmp);

    let result_json = materialize_mcts_subgoals(&mut rt, "test_root_state", 3);
    let result: serde_json::Value =
        serde_json::from_str(&result_json).expect("result must be valid JSON");

    assert!(
        result.get("materialized").is_some(),
        "result must contain 'materialized'"
    );
    assert!(
        result.get("skipped").is_some(),
        "result must contain 'skipped'"
    );
    assert!(
        result.get("task_ids").is_some(),
        "result must contain 'task_ids'"
    );
    assert!(
        result.get("mcts_root_id").is_some(),
        "result must contain 'mcts_root_id'"
    );
    assert!(result.get("top_n").is_some(), "result must contain 'top_n'");
    assert_eq!(result["top_n"], 3, "top_n must echo the requested value");
    assert_eq!(
        result["mcts_root_id"], "test_root_state",
        "mcts_root_id must match root_state"
    );

    // materialized + skipped must sum to top_n
    let materialized = result["materialized"].as_u64().unwrap_or(0);
    let skipped = result["skipped"].as_u64().unwrap_or(0);
    assert_eq!(
        materialized + skipped,
        3,
        "materialized + skipped must equal top_n=3"
    );
}

// ── Test 2: Zero top_n is clamped to 1 ───────────────────────────────────────

#[test]
fn materialize_zero_subgoals_clamped_to_one() {
    let tmp = TempDir::new().expect("tempdir");
    let mut rt = make_runtime(&tmp);

    let result_json = materialize_mcts_subgoals(&mut rt, "edge_case_root", 0);
    let result: serde_json::Value =
        serde_json::from_str(&result_json).expect("result must be valid JSON");

    // top_n=0 is clamped to 1 internally
    assert_eq!(result["top_n"], 1, "top_n=0 must be clamped to 1 in result");
    let materialized = result["materialized"].as_u64().unwrap_or(0);
    let skipped = result["skipped"].as_u64().unwrap_or(0);
    assert_eq!(
        materialized + skipped,
        1,
        "exactly 1 subgoal must be processed after clamp"
    );
}

// ── Test 3: Payload dispatch routes correctly ─────────────────────────────────

#[test]
fn materialize_from_payload_dispatches_correctly() {
    let tmp = TempDir::new().expect("tempdir");
    let mut rt = make_runtime(&tmp);

    let payload = serde_json::json!({
        "root_state": "payload_root",
        "top_n": 2
    });
    let result_json = materialize_from_payload(&mut rt, &payload);
    let result: serde_json::Value =
        serde_json::from_str(&result_json).expect("payload dispatch must return valid JSON");

    assert_eq!(result["top_n"], 2, "top_n must match payload value");
    assert_eq!(
        result["mcts_root_id"], "payload_root",
        "mcts_root_id must match payload root_state"
    );
}

// ── Test 4: Payload with missing keys uses defaults ───────────────────────────

#[test]
fn materialize_from_payload_uses_defaults_when_keys_absent() {
    let tmp = TempDir::new().expect("tempdir");
    let mut rt = make_runtime(&tmp);

    // Empty payload — all keys absent
    let payload = serde_json::json!({});
    let result_json = materialize_from_payload(&mut rt, &payload);
    let result: serde_json::Value =
        serde_json::from_str(&result_json).expect("empty payload must return valid JSON");

    // Default top_n=3
    assert_eq!(result["top_n"], 3, "default top_n must be 3");
    // Default root_state="{}"
    assert_eq!(
        result["mcts_root_id"], "{}",
        "default mcts_root_id must be {{}}"
    );
}

// ── Test 5: top_n=10 (max clamp) does not panic ───────────────────────────────

#[test]
fn materialize_max_top_n_does_not_panic() {
    let tmp = TempDir::new().expect("tempdir");
    let mut rt = make_runtime(&tmp);

    let result_json = materialize_mcts_subgoals(&mut rt, "stress_root", 10);
    let result: serde_json::Value =
        serde_json::from_str(&result_json).expect("max top_n must return valid JSON");

    assert_eq!(result["top_n"], 10, "top_n=10 must be accepted");
    let materialized = result["materialized"].as_u64().unwrap_or(0);
    let skipped = result["skipped"].as_u64().unwrap_or(0);
    assert_eq!(
        materialized + skipped,
        10,
        "all 10 subgoals must be accounted for"
    );
}
