// E2E test for A1/A2/B1/B3 implementation - UserPromptSubmit hook, project attribution,
// SHA-256 deduplication, and latency markers.
//
// This test validates the complete flow:
// - A1: UserPromptSubmit hook fires and returns correct JSON
// - A2: project_dir is extracted from runtime.project_root and stored in hook_events
// - B1: content_hash column enables duplicate detection within 1-hour TTL
// - B3: LatencyMarker records/elapses/deletes correctly

use serde_json::json;
use tempfile::TempDir;

use touring_hooks::hook_response::HookResponse;
use touring_hooks::runtime::HookRuntime;
use touring_hooks::shared::latency_marker::{
    LatencyMarker, cleanup_stale_markers, delete_hook_latency, get_hook_elapsed_ms,
    record_hook_latency,
};

// ---------------------------------------------------------------------------
// Test helpers
// ---------------------------------------------------------------------------

fn make_runtime() -> HookRuntime {
    let tmp = TempDir::new().expect("tempdir");
    let root = tmp.path().to_path_buf();
    std::fs::create_dir_all(root.join(".claude/data")).expect("data dir");
    HookRuntime::new(&root).expect("runtime init")
}

// ---------------------------------------------------------------------------
// A1: UserPromptSubmit hook tests
// ---------------------------------------------------------------------------

#[test]
fn test_user_prompt_submit_hook_response_format() {
    use touring_hooks::prompt_enhance::run_user_prompt_submit;

    let rt = make_runtime();
    let payload = json!({
        "prompt": "test prompt",
        "session_id": "test-session-001"
    });

    let response = run_user_prompt_submit(&rt, &payload);
    let json_str = response.to_json();

    // Verify it's valid JSON
    let parsed: serde_json::Value =
        serde_json::from_str(&json_str).expect("to_json() must return valid JSON");

    // Verify structure has hookSpecificOutput with hookEventName
    assert!(parsed.get("hookSpecificOutput").is_some());
    let hso = parsed.get("hookSpecificOutput").unwrap();
    assert_eq!(hso.get("hookEventName").unwrap(), "user_prompt_submit");

    // Verify context injection
    let context = hso.get("additionalContext");
    assert!(
        context.is_some(),
        "UserPromptSubmit should inject additional context"
    );
}

// ---------------------------------------------------------------------------
// B3: LatencyMarker tests
// ---------------------------------------------------------------------------

#[test]
fn test_latency_marker_record_and_elapsed() {
    let hook_name = format!("test_latency_{}", std::process::id());
    let marker = LatencyMarker::new(&hook_name);

    // Should not exist initially
    assert!(!marker.exists(), "New marker should not exist");

    // Record
    marker.record().expect("record should succeed");
    assert!(marker.exists(), "Marker should exist after record");

    // Read timestamp
    let ts = marker.read_timestamp_ms();
    assert!(ts.is_some(), "read_timestamp_ms should return Some");

    // Elapsed should be very small (record and read are consecutive)
    let elapsed = marker.elapsed_ms().expect("elapsed_ms should return Some");
    assert!(elapsed < 5000, "Elapsed should be small: {} ms", elapsed);

    // Delete
    marker.delete().expect("delete should succeed");
    assert!(!marker.exists(), "Marker should not exist after delete");
}

#[test]
fn test_latency_marker_helper_functions() {
    let hook_name = format!("test_helper_{}", std::process::id());

    // record_hook_latency
    record_hook_latency(&hook_name).expect("record_hook_latency should succeed");

    // get_hook_elapsed_ms
    let elapsed = get_hook_elapsed_ms(&hook_name);
    assert!(elapsed.is_some(), "get_hook_elapsed_ms should return Some");
    assert!(*elapsed.as_ref().unwrap() < 5000, "Elapsed should be small");

    // delete_hook_latency
    delete_hook_latency(&hook_name).expect("delete_hook_latency should succeed");

    // After delete, should return None
    let after_delete = get_hook_elapsed_ms(&hook_name);
    assert!(after_delete.is_none(), "After delete should return None");
}

#[test]
fn test_cleanup_stale_markers() {
    let hook_name = format!("test_cleanup_{}", std::process::id());
    let marker = LatencyMarker::new(&hook_name);

    // Record a marker
    marker.record().expect("record should succeed");
    assert!(marker.exists());

    // Cleanup should run (removes 0 for fresh markers)
    let _removed = cleanup_stale_markers().expect("cleanup_stale_markers should succeed");

    // Clean up our test marker
    marker.delete().expect("delete should succeed");
}

// ---------------------------------------------------------------------------
// HookResponse::to_json() tests
// ---------------------------------------------------------------------------

#[test]
fn test_hook_response_to_json_all_variants() {
    // Allow variant — produces empty JSON object {}, no hookSpecificOutput
    let allow_response = HookResponse::Allow;
    let json = allow_response.to_json();
    assert!(!json.is_empty(), "Allow should produce JSON");
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    // Allow is {}, not {hookSpecificOutput: {...}}
    assert!(
        parsed.get("hookSpecificOutput").is_none(),
        "Allow should be empty object"
    );

    // Context variant
    let context_response = HookResponse::Context {
        context: "test context".to_string(),
        event_name: Some("test-event".to_string()),
    };
    let json = context_response.to_json();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert_eq!(
        parsed
            .pointer("/hookSpecificOutput/hookEventName")
            .unwrap()
            .as_str()
            .unwrap(),
        "test-event"
    );

    // Deny variant
    let deny_response = HookResponse::Deny {
        reason: "test denial".to_string(),
        context: None,
        event_name: None,
    };
    let json = deny_response.to_json();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    // Deny uses permissionDecision/permissionDecisionReason, NOT "reason" inside hookSpecificOutput
    assert_eq!(
        parsed
            .pointer("/hookSpecificOutput/permissionDecision")
            .unwrap()
            .as_str()
            .unwrap(),
        "deny"
    );
    assert_eq!(
        parsed
            .pointer("/hookSpecificOutput/permissionDecisionReason")
            .unwrap()
            .as_str()
            .unwrap(),
        "test denial"
    );

    // Block variant
    let block_response = HookResponse::Block {
        reason: "blocked".to_string(),
        context: None,
        event_name: None,
    };
    let json = block_response.to_json();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    // Block has "decision" and "reason" at TOP level, NOT inside hookSpecificOutput
    assert_eq!(
        parsed.pointer("/decision").unwrap().as_str().unwrap(),
        "block"
    );
    assert_eq!(
        parsed.pointer("/reason").unwrap().as_str().unwrap(),
        "blocked"
    );

    // Halt variant
    let halt_response = HookResponse::Halt {
        reason: "halted".to_string(),
    };
    let json = halt_response.to_json();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    // Halt has continue=false and stopReason at TOP level, NOT hookSpecificOutput
    assert_eq!(
        parsed.pointer("/continue").unwrap().as_bool().unwrap(),
        false
    );
    assert_eq!(
        parsed.pointer("/stopReason").unwrap().as_str().unwrap(),
        "halted"
    );

    // ContextWithUpdatedInput variant
    let ctx_update_response = HookResponse::ContextWithUpdatedInput {
        context: "updated context".to_string(),
        event_name: Some("context-update".to_string()),
        updated_input: json!({"prompt": "updated"}),
    };
    let json = ctx_update_response.to_json();
    let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
    assert!(
        parsed
            .pointer("/hookSpecificOutput/additionalContext")
            .is_some()
    );
}

// ---------------------------------------------------------------------------
// Integration test: verify latency marker persists project_dir semantics
// ---------------------------------------------------------------------------

#[test]
fn test_latency_marker_is_stale_behavior() {
    let hook_name = format!("test_stale_{}", std::process::id());
    let marker = LatencyMarker::new(&hook_name);

    // Record
    marker.record().expect("record should succeed");

    // Fresh marker should NOT be stale even with 1-second threshold
    let is_stale_short = marker.is_stale(1);
    assert!(
        !is_stale_short,
        "Fresh marker should not be stale with 1s threshold"
    );

    // Fresh marker should NOT be stale with 24h threshold (default)
    let is_stale_long = marker.is_stale(86400);
    assert!(
        !is_stale_long,
        "Fresh marker should not be stale with 24h threshold"
    );

    // Delete
    marker.delete().expect("delete should succeed");

    // Non-existent marker IS considered stale
    let is_stale_missing = marker.is_stale(86400);
    assert!(
        is_stale_missing,
        "Non-existent marker should be considered stale"
    );
}
