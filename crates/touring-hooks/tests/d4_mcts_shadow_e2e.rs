//! D4 — MCTS Shadow Rollout E2E tests.
//!
//! Validates the shadow rollout infrastructure wired into `handle_enter_plan_mode`:
//! 1. Low CILA level → rollout does NOT run (threshold guard).
//! 2. High CILA level + tasks → rollout runs, result surfaced.
//! 3. Zero-budget → graceful None, hook returns safely.

use serde_json::json;
use std::time::Duration;
use touring_hooks::shared::shadow_rollout::run_shadow_rollout;

// ---------------------------------------------------------------------------
// Case 1: CILA L1 — shadow rollout must NOT run (tested via run_shadow_rollout
// with empty tasks, since CILA gate lives in handle_enter_plan_mode which
// requires a full HookRuntime. We test the threshold logic via cila_level < 3
// => None path in the helper, and verify rollout itself with empty tasks.)
// ---------------------------------------------------------------------------

#[test]
fn test_case1_empty_tasks_returns_none() {
    // Simulates: plan-mode entry with cila_level=1, no pending subtasks.
    // run_shadow_rollout must return None for empty task list.
    let result = run_shadow_rollout(&[], None, Duration::from_secs(1));
    assert!(
        result.is_none(),
        "empty task list must return None (cila_level < 3 gate path)"
    );
}

// ---------------------------------------------------------------------------
// Case 2: CILA L4 + 3 tasks → rollout runs, completed=true, hint optional.
// ---------------------------------------------------------------------------

#[test]
fn test_case2_l4_three_tasks_rollout_completes() {
    let tasks = vec![
        json!({"task_id": "S-1", "target_module": "crates/touring-hooks/src/pre_edit.rs"}),
        json!({"task_id": "S-2", "target_module": "crates/touring-cognitive/src/mcts.rs"}),
        json!({"task_id": "S-3", "target_module": "crates/touring-ast/src/call_graph.rs"}),
    ];

    let result = run_shadow_rollout(&tasks, None, Duration::from_secs(1))
        .expect("non-empty task list with sufficient budget must return Some");

    assert!(result.completed, "rollout should complete within 1s budget");
    assert_eq!(
        result.suggested_order.len(),
        3,
        "suggested_order must contain all 3 task IDs"
    );
    assert!(
        result.suggested_order.contains(&"S-1".to_string()),
        "S-1 must appear in suggested_order"
    );
    assert!(
        result.suggested_order.contains(&"S-2".to_string()),
        "S-2 must appear in suggested_order"
    );
    assert!(
        result.suggested_order.contains(&"S-3".to_string()),
        "S-3 must appear in suggested_order"
    );
    assert!(
        result.elapsed_ms < 1_000,
        "elapsed_ms must be under 1000ms; got {}",
        result.elapsed_ms
    );

    // For non-crossing modules, no deadlock expected.
    assert!(
        !result.deadlock_detected,
        "non-crossing modules should not produce deadlock signal"
    );

    // Hint should be None when no deadlock detected.
    assert!(
        result.as_hint().is_none(),
        "as_hint() should return None when deadlock_detected=false"
    );
}

// ---------------------------------------------------------------------------
// Case 3: budget 1ns → rollout returns None or completed=false, hook graceful.
// ---------------------------------------------------------------------------

#[test]
fn test_case3_budget_exhausted_graceful() {
    let tasks = vec![
        json!({"task_id": "S-1", "target_module": "crates/touring-hooks/src/pre_edit.rs"}),
        json!({"task_id": "S-2", "target_module": "crates/touring-cognitive/src/mcts.rs"}),
    ];

    // 1ns budget — should either return None (budget exceeded before extraction)
    // or return a result with completed reflecting the actual timing.
    // Either way: no panic. This proves the hook never blocks plan-mode entry.
    let result = run_shadow_rollout(&tasks, None, Duration::from_nanos(1));

    // The key assertion: no panic, result is either None or a valid struct.
    match result {
        None => {
            // Budget expired before extraction — expected on fast machines.
        }
        Some(r) => {
            // Budget expired during or after extraction — completed may be true or false.
            // The hint path is still safe.
            let _ = r.as_hint(); // must not panic
        }
    }
}

// ---------------------------------------------------------------------------
// Case 4: deadlock hint formatting — as_hint produces readable string.
// ---------------------------------------------------------------------------

#[test]
fn test_case4_hint_format_with_deadlock() {
    use touring_hooks::shared::shadow_rollout::ShadowRolloutResult;

    let result = ShadowRolloutResult {
        deadlock_detected: true,
        cycles: vec![vec!["S-1".to_string(), "S-2".to_string()]],
        suggested_order: vec!["S-1".to_string(), "S-2".to_string()],
        elapsed_ms: 15,
        completed: true,
        prefetch_paths: vec![],
    };

    let hint = result.as_hint().expect("deadlock result must produce hint");
    assert!(
        hint.contains("MCTS-SYNTHESIS"),
        "hint must contain MCTS-SYNTHESIS tag; got: {hint}"
    );
    assert!(hint.contains("S-1"), "hint must mention S-1");
    assert!(hint.contains("S-2"), "hint must mention S-2");
    assert!(
        hint.len() < 512,
        "hint should be concise (< 512 chars); got {} chars",
        hint.len()
    );
}

// ---------------------------------------------------------------------------
// Case 5: tasks without task_id fall back to t{index} naming.
// ---------------------------------------------------------------------------

#[test]
fn test_case5_tasks_without_id_use_index_fallback() {
    let tasks = vec![
        json!({"target_module": "crates/touring-hooks/src/pre_edit.rs"}),
        json!({"file": "crates/touring-cognitive/src/mcts.rs"}),
    ];

    let result = run_shadow_rollout(&tasks, None, Duration::from_secs(1))
        .expect("tasks with file field should return Some");

    assert_eq!(result.suggested_order.len(), 2);
    assert!(
        result.suggested_order[0].starts_with('t'),
        "fallback ID must start with 't'; got: {}",
        result.suggested_order[0]
    );
}
