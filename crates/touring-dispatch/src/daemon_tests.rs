use super::*;
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
    let dir = tempfile::tempdir().expect("tempdir");
    let lock_path = &dir.path().join("daemon-flock.lock");
    let socket_path = &dir.path().join("daemon-flock.sock");
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
    let dir = tempfile::tempdir().expect("tempdir");
    let socket_path = &dir.path().join("already-alive-probe.sock");
    let lock_path = &dir.path().join("already-alive-probe.lock");
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

/// Cross-audit 14/09/2026 (A2): dropping the handle the daemon keeps per project
/// is what evicts it, so nothing the actor owns may hold a strong sender to its
/// own channel. Before the fix the runtime stored one, the channel never closed,
/// and every actor the LRU "evicted" kept running with its databases open.
#[test]
fn dropping_a_project_runtime_closes_its_actor_channel() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let runtime = HookRuntime::new(tmp.path()).expect("runtime");
    let project = ProjectRuntime::new(runtime);
    let weak = project.cmd_sender().downgrade();
    assert!(
        weak.upgrade().is_some(),
        "the channel is open while the handle lives"
    );
    drop(project);
    let deadline = Instant::now() + Duration::from_secs(5);
    while weak.upgrade().is_some() {
        assert!(
            Instant::now() < deadline,
            "a strong sender outlived the ProjectRuntime: the actor can never exit"
        );
        thread::sleep(Duration::from_millis(10));
    }
}

/// The handler-facing accessor follows the channel: absent before the actor
/// spawns, present while a dispatch-side sender lives, absent after.
#[test]
fn the_runtime_sender_never_outlives_the_dispatch_side() {
    let tmp = tempfile::tempdir().expect("tempdir");
    let runtime = HookRuntime::new(tmp.path()).expect("runtime");
    assert!(runtime.ctx.cmd_tx().is_none(), "no actor yet");
    let (tx, _rx) = mpsc::channel::<ProjectCommand>(1);
    runtime.ctx.set_cmd_tx(&tx);
    assert!(
        runtime.ctx.cmd_tx().is_some(),
        "the actor's channel is reachable"
    );
    drop(tx);
    assert!(runtime.ctx.cmd_tx().is_none(), "the dispatch side is gone");
}

#[test]
// `collect` into a Vec is required to force eager spawn — without materialising
// the handles, the thread-spawning iterator would be consumed lazily and the
// concurrent-startup scenario under test would never actually race.
#[allow(clippy::needless_collect)]
fn test_concurrent_daemon_startup() {
    // A private dir per run: fixed /tmp names collided across concurrent test
    // runs, which is why this test used to be #[ignore]d (cross-audit 14/09, O3).
    let dir = tempfile::tempdir().expect("tempdir");
    let lock_path = &dir.path().join("concurrent.lock");
    let socket_path = &dir.path().join("concurrent.sock");
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

// ── client/server budget symmetry (19/08/2026) ───────────────────────────
//
// The client had learned that a full rebuild outlives the default read timeout
// and raised its floor to 1800 s. The server kept its own `300`. So the client
// waited 30 minutes under a server that quit after 5, and the operator was told
// the rebuild FAILED while the actor was still writing to `symbols.db`.
//
// Both sides now read `HEAVY_OP_BUDGET_SECS`. These tests hold the invariant
// that made the constant necessary — a shared constant that one side quietly
// stops using is back to two numbers.

/// The client must still be listening when the server's typed budget reply goes
/// out. Cross-audit 14/09/2026 (A3): the old version of this test compared the
/// constant with itself; the floor equalled the budget and started earlier, so
/// the socket read gave up first and the reply that says "still running" was lost.
#[test]
fn the_client_outwaits_the_heavy_budget_and_every_wait_before_it() {
    let server_budget = touring_foundation::HEAVY_OP_BUDGET_SECS;
    let client_floor = touring_foundation::HEAVY_OP_CLIENT_FLOOR_SECS;
    // Global slot, project slot and channel send each wait up to REQUEST_TIMEOUT
    // before `handler_budget` starts counting.
    let pre_budget_waits = 3 * REQUEST_TIMEOUT.as_secs();
    assert!(
        client_floor > server_budget + pre_budget_waits,
        "client floor {client_floor}s must exceed the server budget {server_budget}s plus \
         {pre_budget_waits}s of queueing, or the typed budget reply never reaches the client"
    );
    assert!(
        server_budget >= 300,
        "the budget must not regress below the old hard-coded 300s, which `analise` \
         already exceeded on a healthy rebuild"
    );
}

/// The one call site that decides the server's heavy budget must read the
/// shared constant. A literal here is how the two numbers drifted apart the
/// first time — and a grep is the only thing that notices a re-introduced one.
#[test]
fn the_server_budget_reads_the_shared_constant_not_a_literal() {
    let source = include_str!("daemon.rs");
    assert!(
        source.contains("Duration::from_secs(touring_foundation::HEAVY_OP_BUDGET_SECS)"),
        "the heavy-op budget must come from the shared constant"
    );
    assert!(
        !source.contains("Duration::from_secs(300)"),
        "a bare 300s budget is the literal that disagreed with the client's 1800s floor"
    );
}

/// A cargo-mutants run over one crate is ~19 min measured (134 mutants,
/// touring-identity). Outside the heavy class the light 15s budget kills every
/// real run in transport, so the KPI's cache could never be populated through
/// the canonical route (rodada 4, 2026-08-20 — fixed 2026-08-28).
#[test]
fn mutation_test_is_classified_heavy() {
    assert!(
        is_heavy_hook("cli-mutation-test"),
        "cli-mutation-test must be in the is_heavy_hook class — under the light \
         budget a real mutation run dies in transport and the kill_rate KPI \
         can only ever see a cache_miss"
    );
}

/// The three tests above prove the HELPER carries a reason. None of them proved
/// that the failure BRANCHES call it — and on 09/08/2026 only the saturation
/// branch was converted, while four others kept returning the empty payload.
///
/// That gap cost two diagnostic rounds on 19/08/2026: a dead project actor in
/// the `analise` daemon answered `index rebuild` AND a trivial `gate-metrics`
/// with the same "empty response payload", so the reindex looked guilty for
/// four minutes of wall-clock and two restarts. `gate-metrics` cannot take five
/// minutes; had the message named the actor, the first response would have
/// pointed at the daemon.
///
/// So the invariant is enforced over the WHOLE file, not one branch: a quality
/// rule tested on a single instance recurs in the next uncovered one.
#[test]
fn no_dispatch_branch_answers_with_the_empty_payload() {
    let source = include_str!("daemon.rs");
    let mut offenders = Vec::new();
    // `DaemonResponse { output: String::new(), success: false }` in any spelling
    // the formatter may produce — the pair is what makes it undiagnosable.
    let normalized: String = source.split_whitespace().collect::<Vec<_>>().join(" ");
    let needle = "DaemonResponse { output: String::new(), success: false }";
    if normalized.contains(needle) {
        for (lineno, line) in source.lines().enumerate() {
            if line.contains("output: String::new()") {
                offenders.push(format!("{}: {}", lineno + 1, line.trim()));
            }
        }
    }
    assert!(
        offenders.is_empty(),
        "{} dispatch branch(es) still answer with an empty payload, which reaches the operator \
         as \"Daemon returned success=false (empty response payload)\" — indistinguishable from \
         a failure in the work they asked for. Use `protocol_failure(<what happened and what to \
         do>)`:\n{}",
        offenders.len(),
        offenders.join("\n")
    );
}

/// A guard that stopped matching would report a clean file forever. Prove the
/// detector still sees the shape it forbids.
#[test]
fn the_empty_payload_detector_still_recognizes_the_shape() {
    let offending = "return DaemonResponse { output: String::new(), success: false };";
    let normalized: String = offending.split_whitespace().collect::<Vec<_>>().join(" ");
    assert!(
        normalized.contains("DaemonResponse { output: String::new(), success: false }"),
        "the detector must still match the exact shape the guard forbids"
    );
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

// ── peer_label (21/08/2026 — the heavy-op line now names its sender) ──────

#[test]
fn peer_label_names_own_pid_and_comm_from_proc() {
    let me = std::process::id() as i32;
    let label = super::peer_label(Some(me));
    let comm = std::fs::read_to_string("/proc/self/comm")
        .map(|s| s.trim().to_string())
        .expect("/proc/self/comm readable on Linux");
    assert_eq!(label, format!("pid={me} comm={comm}"));
    assert!(!comm.is_empty(), "comm must be non-empty: {label}");
}

#[test]
fn peer_label_without_credentials_says_unknown_not_a_fake_pid() {
    assert_eq!(super::peer_label(None), "unknown");
}

#[test]
fn peer_label_for_a_dead_pid_keeps_the_pid_and_marks_comm_unknown() {
    // pid 2^22+1 is above the default pid_max; no such process exists.
    let label = super::peer_label(Some(4_194_305));
    assert_eq!(label, "pid=4194305 comm=?");
}

/// Decision 3-A (14/09/2026): `index status` is answered before the runtime map
/// is touched. A project whose databases exist gets its status with NO project
/// actor created — the property that lets it answer while that actor seals a
/// rebuild (a status call timed out at 15 s in the analise at 09:08:32).
#[test]
fn index_status_is_served_without_the_project_actor() {
    let tmp = tempfile::tempdir().expect("tmp");
    let root = tmp.path().to_path_buf();
    // A project marker, so the dispatch's root normalization stays on the tmpdir.
    std::fs::create_dir(root.join(".git")).expect(".git");
    drop(crate::HookRuntime::new(&root).expect("create the project databases"));

    let map: RuntimeMap = tokio::sync::RwLock::new(HashMap::new());
    let tokio_rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("tokio");
    let req = DaemonRequest {
        peer_pid: None,
        hook: "cli-index-status".to_string(),
        payload: serde_json::json!({}),
        project_root: root.display().to_string(),
        session_id: None,
        priority: 0,
        origin: None,
    };
    let dispatched = || {
        touring_foundation::gate_metrics::hook_dispatch_by_name()
            .get("cli-index-status")
            .copied()
            .unwrap_or(0)
    };
    let before = dispatched();
    let resp = tokio_rt.block_on(dispatch_request_async(req, &map));

    assert!(resp.success, "{}", resp.output);
    assert!(
        dispatched() > before,
        "the off-actor route counts in the per-hook dispatch counter too (A8)"
    );
    let status: serde_json::Value = serde_json::from_str(&resp.output).expect("status json");
    assert_eq!(status["initialized"], true, "{status}");
    assert_eq!(
        status["symbol_count"], 0,
        "the tmp project's own empty store answered: {status}"
    );
    assert_eq!(status["index_generation"]["state"], "none", "{status}");
    assert!(
        tokio_rt.block_on(map.read()).is_empty(),
        "no project runtime was created to answer the status"
    );
}

/// Cross-audit 14/09/2026 (C12): the spawner is read before the boot waits on
/// anything (lock, bind), so a launcher that exits meanwhile is still the one
/// named — not the subreaper that adopted the daemon.
#[test]
fn the_spawner_is_read_before_the_lock_and_the_bind() {
    let source = include_str!("daemon.rs");
    let body = source
        .split("pub async fn run_daemon_async()")
        .nth(1)
        .expect("run_daemon_async");
    let read = body.find("parent_id()").expect("the spawner is read");
    let lock = body.find("acquire_lock(&lock_path").expect("the lock");
    assert!(
        read < lock,
        "the parent pid must be read before acquiring the lock"
    );
}
