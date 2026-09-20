//! A1 (14/09/2026): a light command queued behind a heavy hook is served while
//! the heavy hook yields, and every other command still waits for it to finish.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use tokio::sync::{mpsc, oneshot};

use super::{HookHandler, is_heavy_hook, may_run_during_heavy, run_project_actor_with};
use crate::runtime::HookRuntime;
use touring_hook_runtime::actor_yield;
use touring_hook_runtime::daemon_protocol::ProjectCommand;

static HEAVY_STARTED: AtomicBool = AtomicBool::new(false);
static RELEASE_HEAVY: AtomicBool = AtomicBool::new(false);
static ORDER: Mutex<Vec<&'static str>> = Mutex::new(Vec::new());

fn record(event: &'static str) {
    ORDER.lock().expect("order").push(event);
}

/// Stands in for `index rebuild`: yields until the test releases it.
fn heavy(rt: &mut HookRuntime, _: &serde_json::Value) -> String {
    HEAVY_STARTED.store(true, Ordering::SeqCst);
    let deadline = Instant::now() + Duration::from_secs(20);
    while !RELEASE_HEAVY.load(Ordering::SeqCst) && Instant::now() < deadline {
        actor_yield::yield_now(rt);
        std::thread::sleep(Duration::from_millis(2));
    }
    record("heavy-end");
    "heavy".to_string()
}

/// Stands in for `memory store`; a nested yield from here must be refused.
fn light(rt: &mut HookRuntime, _: &serde_json::Value) -> String {
    record(if actor_yield::yield_now(rt) {
        "light-reentered"
    } else {
        "light"
    });
    "light".to_string()
}

/// Stands in for a hook outside the allowlist.
fn other(_: &mut HookRuntime, _: &serde_json::Value) -> String {
    record(if actor_yield::is_installed() {
        "other-under-yield"
    } else {
        "other"
    });
    "other".to_string()
}

fn send(tx: &mpsc::Sender<ProjectCommand>, hook: &str) -> oneshot::Receiver<String> {
    let (response, rx) = oneshot::channel();
    tx.blocking_send(ProjectCommand::RunHook {
        hook_name: hook.to_string(),
        payload: serde_json::json!({}),
        response,
        enqueued_at: Instant::now(),
    })
    .expect("actor alive");
    rx
}

fn wait_reply(rx: &mut oneshot::Receiver<String>, what: &str) -> String {
    let deadline = Instant::now() + Duration::from_secs(20);
    loop {
        match rx.try_recv() {
            Ok(reply) => return reply,
            Err(oneshot::error::TryRecvError::Empty) if Instant::now() < deadline => {
                std::thread::sleep(Duration::from_millis(5));
            }
            Err(e) => panic!("{what}: no reply ({e:?})"),
        }
    }
}

#[test]
fn a_light_command_is_served_while_a_heavy_hook_yields_and_others_wait() {
    let mut table: HashMap<&'static str, HookHandler> = HashMap::new();
    table.insert("cli-index-rebuild", heavy);
    table.insert("cli-memory-store", light);
    table.insert("cli-wiring-orphans", other);
    let table: &'static HashMap<&'static str, HookHandler> = Box::leak(Box::new(table));

    let tmp = tempfile::tempdir().expect("tempdir");
    let runtime = HookRuntime::new(tmp.path()).expect("runtime");
    let (tx, rx) = mpsc::channel(16);
    let actor = std::thread::spawn(move || run_project_actor_with(runtime, rx, table));

    let mut heavy_reply = send(&tx, "cli-index-rebuild");
    let deadline = Instant::now() + Duration::from_secs(20);
    while !HEAVY_STARTED.load(Ordering::SeqCst) {
        assert!(Instant::now() < deadline, "heavy hook never started");
        std::thread::sleep(Duration::from_millis(2));
    }
    // Queued in this order: the hook outside the allowlist first.
    let mut other_reply = send(&tx, "cli-wiring-orphans");
    let mut light_reply = send(&tx, "cli-memory-store");

    assert_eq!(wait_reply(&mut light_reply, "light"), "light");
    assert!(
        heavy_reply.try_recv().is_err(),
        "the heavy hook is still running"
    );
    assert!(
        other_reply.try_recv().is_err(),
        "a hook outside the allowlist waits"
    );

    RELEASE_HEAVY.store(true, Ordering::SeqCst);
    assert_eq!(wait_reply(&mut heavy_reply, "heavy"), "heavy");
    assert_eq!(wait_reply(&mut other_reply, "other"), "other");
    assert_eq!(
        *ORDER.lock().expect("order"),
        ["light", "heavy-end", "other"]
    );

    drop(tx);
    actor.join().expect("actor exits when every sender is gone");
}

static BOUND_HEAVY_STARTED: AtomicBool = AtomicBool::new(false);
static BOUND_RELEASE_HEAVY: AtomicBool = AtomicBool::new(false);
static BOUND_ORDER: Mutex<Vec<u64>> = Mutex::new(Vec::new());

/// The heavy hook of the bound test (its own statics: tests share the binary).
fn bound_heavy(rt: &mut HookRuntime, _: &serde_json::Value) -> String {
    BOUND_HEAVY_STARTED.store(true, Ordering::SeqCst);
    let deadline = Instant::now() + Duration::from_secs(20);
    while !BOUND_RELEASE_HEAVY.load(Ordering::SeqCst) && Instant::now() < deadline {
        actor_yield::yield_now(rt);
        std::thread::sleep(Duration::from_millis(2));
    }
    "heavy".to_string()
}

/// A deferred hook that records the order it finally runs in.
fn bound_other(_: &mut HookRuntime, payload: &serde_json::Value) -> String {
    BOUND_ORDER
        .lock()
        .expect("order")
        .push(payload["i"].as_u64().expect("index"));
    "other".to_string()
}

/// Cross-audit 14/09/2026 (A1): each `try_recv` of the yield frees a channel slot,
/// so an unbounded deferred queue let every sender through during a heavy hook.
/// The queue is bounded by the channel's capacity: with a channel of 4, a sender of
/// 12 commands parks after 8 (4 deferred + 4 in the channel), and once the heavy
/// hook ends all 12 run in the order they were sent.
#[test]
fn deferred_commands_never_outgrow_the_channel_and_keep_their_order() {
    use std::sync::Arc;
    use std::sync::atomic::AtomicUsize;

    let mut table: HashMap<&'static str, HookHandler> = HashMap::new();
    table.insert("cli-index-rebuild", bound_heavy);
    table.insert("cli-wiring-orphans", bound_other);
    let table: &'static HashMap<&'static str, HookHandler> = Box::leak(Box::new(table));

    let tmp = tempfile::tempdir().expect("tempdir");
    let runtime = HookRuntime::new(tmp.path()).expect("runtime");
    let (tx, rx) = mpsc::channel(4);
    let actor = std::thread::spawn(move || run_project_actor_with(runtime, rx, table));

    let mut heavy_reply = send(&tx, "cli-index-rebuild");
    let deadline = Instant::now() + Duration::from_secs(20);
    while !BOUND_HEAVY_STARTED.load(Ordering::SeqCst) {
        assert!(Instant::now() < deadline, "heavy hook never started");
        std::thread::sleep(Duration::from_millis(2));
    }

    let sent = Arc::new(AtomicUsize::new(0));
    let sender = {
        let tx = tx.clone();
        let sent = Arc::clone(&sent);
        std::thread::spawn(move || {
            (0..12u64)
                .map(|i| {
                    let (response, reply) = oneshot::channel();
                    tx.blocking_send(ProjectCommand::RunHook {
                        hook_name: "cli-wiring-orphans".to_string(),
                        payload: serde_json::json!({ "i": i }),
                        response,
                        enqueued_at: Instant::now(),
                    })
                    .expect("actor alive");
                    sent.fetch_add(1, Ordering::SeqCst);
                    reply
                })
                .collect::<Vec<_>>()
        })
    };

    // Let the heavy hook yield a few hundred times with the sender pushing.
    std::thread::sleep(Duration::from_millis(400));
    let accepted = sent.load(Ordering::SeqCst);
    assert!(
        accepted <= 8,
        "the sender is parked at the bound (4 deferred + 4 queued), accepted {accepted} of 12"
    );
    assert!(
        heavy_reply.try_recv().is_err(),
        "the heavy hook is still running"
    );

    BOUND_RELEASE_HEAVY.store(true, Ordering::SeqCst);
    assert_eq!(wait_reply(&mut heavy_reply, "heavy"), "heavy");
    let mut replies = sender.join().expect("sender");
    for (i, reply) in replies.iter_mut().enumerate() {
        assert_eq!(wait_reply(reply, &format!("deferred {i}")), "other");
    }
    assert_eq!(
        *BOUND_ORDER.lock().expect("order"),
        (0..12u64).collect::<Vec<_>>(),
        "deferred commands run in the order they were sent"
    );

    drop(tx);
    actor.join().expect("actor exits when every sender is gone");
}

static CAP_HEAVY_STARTED: AtomicBool = AtomicBool::new(false);
static CAP_QUEUED: AtomicBool = AtomicBool::new(false);
static CAP_SERVED: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static CAP_SERVED_IN_ONE_YIELD: std::sync::atomic::AtomicUsize =
    std::sync::atomic::AtomicUsize::new(usize::MAX);

/// Yields exactly once, after the test has queued every light command, and
/// records how many that single yield served.
fn cap_heavy(rt: &mut HookRuntime, _: &serde_json::Value) -> String {
    CAP_HEAVY_STARTED.store(true, Ordering::SeqCst);
    let deadline = Instant::now() + Duration::from_secs(20);
    while !CAP_QUEUED.load(Ordering::SeqCst) && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(2));
    }
    actor_yield::yield_now(rt);
    CAP_SERVED_IN_ONE_YIELD.store(CAP_SERVED.load(Ordering::SeqCst), Ordering::SeqCst);
    "heavy".to_string()
}

fn cap_light(_: &mut HookRuntime, _: &serde_json::Value) -> String {
    CAP_SERVED.fetch_add(1, Ordering::SeqCst);
    "light".to_string()
}

/// Cross-audit 14/09/2026 (A9): nothing held the per-yield cap. With 40 light
/// commands queued, one yield serves exactly `MAX_SERVED_PER_YIELD` and hands the
/// thread back; the rest run after the heavy hook.
#[test]
fn one_yield_serves_at_most_the_cap_and_the_rest_still_run() {
    use super::MAX_SERVED_PER_YIELD;

    let mut table: HashMap<&'static str, HookHandler> = HashMap::new();
    table.insert("cli-index-rebuild", cap_heavy);
    table.insert("cli-memory-store", cap_light);
    let table: &'static HashMap<&'static str, HookHandler> = Box::leak(Box::new(table));

    let tmp = tempfile::tempdir().expect("tempdir");
    let runtime = HookRuntime::new(tmp.path()).expect("runtime");
    let (tx, rx) = mpsc::channel(64);
    let actor = std::thread::spawn(move || run_project_actor_with(runtime, rx, table));

    let mut heavy_reply = send(&tx, "cli-index-rebuild");
    let deadline = Instant::now() + Duration::from_secs(20);
    while !CAP_HEAVY_STARTED.load(Ordering::SeqCst) {
        assert!(Instant::now() < deadline, "heavy hook never started");
        std::thread::sleep(Duration::from_millis(2));
    }
    let queued = MAX_SERVED_PER_YIELD + 8;
    let mut light_replies: Vec<_> = (0..queued).map(|_| send(&tx, "cli-memory-store")).collect();
    CAP_QUEUED.store(true, Ordering::SeqCst);

    assert_eq!(wait_reply(&mut heavy_reply, "heavy"), "heavy");
    assert_eq!(
        CAP_SERVED_IN_ONE_YIELD.load(Ordering::SeqCst),
        MAX_SERVED_PER_YIELD,
        "one yield serves exactly the cap"
    );
    for (i, reply) in light_replies.iter_mut().enumerate() {
        assert_eq!(wait_reply(reply, &format!("light {i}")), "light");
    }
    assert_eq!(CAP_SERVED.load(Ordering::SeqCst), queued);

    drop(tx);
    actor.join().expect("actor exits when every sender is gone");
}

#[test]
fn only_hooks_that_leave_the_indexes_alone_run_during_a_heavy_hook() {
    for hook in [
        "cli-memory-store",
        "cli-memory-recall",
        "cli-memory-link",
        "cli-hook-memory-store",
        "cli-hook-memory-recall",
        "cli-index-status",
    ] {
        assert!(may_run_during_heavy(hook), "{hook}");
    }
    for hook in [
        "cli-memory-reindex",
        "cli-index-rebuild",
        "cli-tantivy-reindex",
        "post_edit",
        "cli-wiring-orphans",
        "cli-index-find",
        // The decompose family is deliberately OUT, and this is the record of
        // why (measured 20/09/2026, after `decompose update` answered 75 during
        // a rebuild and the question "can it yield too?" came up): the rebuild
        // holds long transactions on the SAME knowledge.db these tables live in
        // (`unchecked_transaction`, `clear_wiring`, `begin_index_generation`).
        // Letting them through would trade a clean, retryable 75 for SQLITE_BUSY
        // contention against a walk mid-flight. Exit 75 with `retryable: true`
        // is the correct answer here, not a wart to remove.
        "cli-decompose-update",
        "cli-decompose-ready",
        "cli-decompose-archive",
        "cli-decompose-reconcile-stages",
    ] {
        assert!(!may_run_during_heavy(hook), "{hook}");
    }
}

#[test]
fn workspace_wide_scans_get_the_heavy_budget() {
    for hook in [
        "cli-index-rebuild",
        "cli-ast-blast",
        "cli-pre-task-scout",
        "cli-memory-reindex",
    ] {
        assert!(is_heavy_hook(hook), "{hook}");
    }
    for hook in ["cli-memory-store", "cli-index-status", "pre_edit"] {
        assert!(!is_heavy_hook(hook), "{hook}");
    }
}
