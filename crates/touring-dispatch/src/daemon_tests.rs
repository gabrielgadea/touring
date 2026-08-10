use super::*;
use std::path::Path;
use std::thread;
use std::time::Duration;

/// Helper — pattern-match for Acquired/AlreadyAlive/Failed in tests
/// without binding the inner File (which would hold the lock).
fn is_acquired(o: &LockOutcome) -> bool {
    matches!(o, LockOutcome::Acquired(_))
}
fn is_already_alive(o: &LockOutcome) -> bool {
    matches!(o, LockOutcome::AlreadyAlive)
}
fn is_failed(o: &LockOutcome) -> bool {
    matches!(o, LockOutcome::Failed(_))
}

// ── perf F1/F2/T-03: inline E2E scan debounce contract ────────────────
// Proves the post-edit/post-write maintenance scan can run at most once per
// window, so an edit burst cannot convoy the per-project actor on every hook.

#[test]
fn e2e_scan_due_when_never_run() {
    assert!(e2e_scan_due(None, Instant::now(), E2E_SCAN_DEBOUNCE));
}

#[test]
fn e2e_scan_skipped_within_window() {
    let now = Instant::now();
    // Just ran → not due (the burst case the fix bounds).
    assert!(!e2e_scan_due(Some(now), now, E2E_SCAN_DEBOUNCE));
    assert!(!e2e_scan_due(
        Some(now),
        now + Duration::from_secs(1),
        E2E_SCAN_DEBOUNCE
    ));
}

#[test]
fn e2e_scan_due_after_window() {
    let prev = Instant::now();
    let later = prev + E2E_SCAN_DEBOUNCE + Duration::from_millis(1);
    assert!(e2e_scan_due(Some(prev), later, E2E_SCAN_DEBOUNCE));
}

#[test]
fn test_flock_acquire_and_hold() {
    let lock_path = Path::new("/tmp/test-touring-daemon-flock.lock");
    let socket_path = Path::new("/tmp/test-touring-daemon-flock.sock");
    let _ = std::fs::remove_file(lock_path);
    let _ = std::fs::remove_file(socket_path);

    // First acquire should succeed (no daemon running)
    let guard = acquire_lock(lock_path, socket_path);
    assert!(
        is_acquired(&guard),
        "first acquire should succeed: {:?}",
        match &guard {
            LockOutcome::Failed(r) => r.clone(),
            _ => "(non-Failed)".into(),
        }
    );

    // Second acquire while first holds flock: the test process IS NOT
    // a "touring-daemon" per /proc/self/comm (it's a cargo test runner),
    // so PC-2 classifies the holder as Failed (PID reuse signature),
    // NOT AlreadyAlive. This is the correct behavior — only a real
    // daemon process triggers the idempotent silent exit.
    let second = acquire_lock(lock_path, socket_path);
    assert!(
        is_failed(&second),
        "second acquire must classify the (non-daemon) holder as Failed"
    );

    // Drop the first guard — releases flock
    drop(guard);

    // Now a third acquire should succeed again
    let third = acquire_lock(lock_path, socket_path);
    assert!(
        is_acquired(&third),
        "third acquire should succeed after drop"
    );

    // Clean up
    drop(third);
    let _ = std::fs::remove_file(lock_path);
    let _ = std::fs::remove_file(socket_path);
}

#[test]
fn test_pid_is_live_touring_daemon_rejects_pid_one() {
    // PID 1 is init/systemd — comm is "systemd" or similar, never touring-daemon
    assert!(!pid_is_live_touring_daemon(1));
}

#[test]
fn test_pid_is_live_touring_daemon_rejects_invalid_pid() {
    assert!(!pid_is_live_touring_daemon(0));
    assert!(!pid_is_live_touring_daemon(-1));
    // Very high PID that should not exist on any sane host
    assert!(!pid_is_live_touring_daemon(4_194_303));
}

#[test]
fn test_already_alive_via_socket_probe() {
    // When socket probe says "daemon running", acquire_lock short-circuits
    // to AlreadyAlive before even touching the lock file. Simulate this by
    // creating a fake socket that accepts connections. We use a temporary
    // path so we don't race with the real daemon.
    let socket_path = Path::new("/tmp/test-touring-already-alive-probe.sock");
    let lock_path = Path::new("/tmp/test-touring-already-alive-probe.lock");
    let _ = std::fs::remove_file(socket_path);
    let _ = std::fs::remove_file(lock_path);

    // Bind a fake server on the socket
    let _server = std::os::unix::net::UnixListener::bind(socket_path).unwrap();

    let outcome = acquire_lock(lock_path, socket_path);
    assert!(
        is_already_alive(&outcome),
        "socket-probe-positive path should classify as AlreadyAlive"
    );

    // Cleanup
    drop(_server);
    let _ = std::fs::remove_file(socket_path);
    let _ = std::fs::remove_file(lock_path);
}

#[test]
#[ignore]
// Ignored as it requires manual verification (spawns real process)
// `collect` into a Vec is required to force eager spawn — without materialising
// the handles, the thread-spawning iterator would be consumed lazily and the
// concurrent-startup scenario under test would never actually race.
#[allow(clippy::needless_collect)]
fn test_concurrent_daemon_startup() {
    let lock_path = Path::new("/tmp/test-touring-concurrent.lock");
    let socket_path = Path::new("/tmp/test-touring-concurrent.sock");
    let _ = std::fs::remove_file(lock_path);
    let _ = std::fs::remove_file(socket_path);

    // Spawn 5 threads trying to acquire the lock simultaneously.
    // Each thread holds its guard alive until join — only one should succeed.
    let handles: Vec<_> = (0..5)
        .map(|_| {
            let lock_path = lock_path.to_path_buf();
            let socket_path = socket_path.to_path_buf();
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(10)); // Stagger
                acquire_lock(&lock_path, &socket_path)
            })
        })
        .collect();

    let results: Vec<LockOutcome> = handles
        .into_iter()
        .map(|h| h.join().expect("thread panicked"))
        .collect();

    // Exactly one should successfully Acquire. The other four are
    // classified as Failed because the lock holder is the same cargo
    // test process (comm != "touring-daemon"), not the AlreadyAlive
    // arm — which is reserved for real daemons. The plan's "5x parallel
    // start" idempotency scenario fires AlreadyAlive in production
    // only because the daemon binaries call set_process_name first.
    assert_eq!(
        results
            .iter()
            .filter(|o| matches!(o, LockOutcome::Acquired(_)))
            .count(),
        1,
        "exactly one of 5 racers must Acquire"
    );

    let _ = std::fs::remove_file(lock_path);
    let _ = std::fs::remove_file(socket_path);
}

// ── F5 — KPI snapshot scheduler request shape ─────────────────────────
// The scheduler reuses `dispatch_request_async`; the only F5-specific logic is
// the synthetic request it feeds in. Prove its shape so the daemon dispatches
// the snapshot writer (not some other hook), routed through a LIVE project root.

#[test]
fn kpi_snapshot_request_targets_cli_kpi_snapshot() {
    let req = kpi_snapshot_request("/home/u/proj".to_string());
    assert_eq!(req.hook, "cli-kpi", "must dispatch the kpi handler");
    assert_eq!(
        req.payload.get("snapshot").and_then(|v| v.as_bool()),
        Some(true),
        "must request snapshot persistence, not just a read"
    );
    assert_eq!(
        req.project_root, "/home/u/proj",
        "must route through the given LIVE project runtime — an empty root forces \
         HookRuntime::new(\"\"), which never lands the snapshot (F5 flush fix)"
    );
    assert!(req.session_id.is_none());
}

// ── protocol failures must be diagnosable (09/08/2026) ────────────────
// `daemon_client::daemon_failure_message` prints the payload's `error` field,
// and falls back to "(empty response payload)" when there is none. Every
// protocol-level refusal therefore has to CARRY a reason: the saturation
// branch used to send an empty payload, so a shed `memory recall` was
// indistinguishable from a wedged memory subsystem.

#[test]
fn a_protocol_failure_carries_a_reason_the_client_can_print() {
    let resp = protocol_failure("project saturated: no handler slot within 30s");
    assert!(!resp.success);
    let parsed: serde_json::Value =
        serde_json::from_str(&resp.output).expect("payload must be JSON");
    assert_eq!(
        parsed.get("error").and_then(serde_json::Value::as_str),
        Some("project saturated: no handler slot within 30s"),
        "the client reads `error`; anything else reaches the operator as an empty payload"
    );
}

#[test]
fn a_protocol_failure_never_echoes_an_unbounded_payload() {
    let resp = protocol_failure("x".repeat(10_000));
    let parsed: serde_json::Value = serde_json::from_str(&resp.output).expect("JSON");
    let err = parsed["error"].as_str().expect("error string");
    assert!(
        err.chars().count() <= 301,
        "a malformed request must not echo itself back in full, got {} chars",
        err.chars().count()
    );
    assert!(err.ends_with('…'), "truncation must be visible, not silent");
}

#[test]
fn a_protocol_failure_is_never_the_empty_payload_it_replaced() {
    for reason in ["", "   "] {
        let resp = protocol_failure(reason);
        let parsed: serde_json::Value = serde_json::from_str(&resp.output).expect("JSON");
        assert!(
            parsed.get("error").is_some(),
            "even an empty reason keeps the `error` key, so the client never falls \
             back to the (empty response payload) dead end"
        );
    }
}
