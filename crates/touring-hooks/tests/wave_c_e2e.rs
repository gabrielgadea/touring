//! Wave C E2E integration test (D6, 2026-04-20).
//!
//! Validates the full Wave C pipeline:
//!   - `CascadeQueue` push/drain lifecycle
//!   - Hook registry contains the two Wave C D5 handlers (146 total)
//!   - Low-severity proposals are filtered out at the queue layer

use std::path::Path;

use touring_code::ast::api_cascade::{CallerRef, CascadePlan, Severity, SubtaskProposal};
use touring_code::ast::rust_semantic::ApiChangeKind;
use touring_hooks::hook_registry::all_daemon_hook_names;
use touring_hooks::shared::cascade_queue::CascadeQueue;

// ─── Helpers ─────────────────────────────────────────────────────────────────

/// Build a `CascadePlan` with a single proposal of the given severity.
///
/// - `Severity::High` is produced by `ApiChangeKind::Removed` with ≥1 caller.
/// - `Severity::Low`  is produced by `ApiChangeKind::Removed` with 0 callers.
fn make_plan(severity: Severity) -> CascadePlan {
    let callers = match severity {
        Severity::High => vec![CallerRef {
            caller: "some_caller_fn".to_string(),
            line: 42,
        }],
        // Low/Medium: no callers means Removed → Low
        _ => vec![],
    };

    let kind = match severity {
        Severity::Medium => ApiChangeKind::Added,
        _ => ApiChangeKind::Removed,
    };

    CascadePlan {
        proposals: vec![SubtaskProposal {
            api_item: "pub fn important_fn() -> u32".to_string(),
            symbol: "important_fn".to_string(),
            kind,
            callers,
            reason: "public API change detected".to_string(),
            severity,
        }],
    }
}

// ─── Tests ────────────────────────────────────────────────────────────────────

/// T1: High-severity proposals are enqueued and recoverable via `drain_fresh`.
#[test]
fn cascade_queue_push_and_drain() {
    let queue = CascadeQueue::new();
    assert!(queue.is_empty(), "new queue must be empty");

    let plan = make_plan(Severity::High);
    queue.push(Path::new("src/lib.rs"), &plan);

    assert_eq!(queue.len(), 1, "queue must hold 1 item after push");

    let drained = queue.drain_fresh();
    assert_eq!(drained.len(), 1, "drain_fresh must return 1 PendingCascade");
    assert_eq!(
        drained[0].proposals.len(),
        1,
        "PendingCascade must contain 1 proposal"
    );
    assert_eq!(
        drained[0].proposals[0].severity,
        Severity::High,
        "proposal severity must be High"
    );

    assert!(
        queue.is_empty(),
        "queue must be empty after drain_fresh consumes all items"
    );
}

/// T2: Hook registry contains the two Wave C D5 cascade queue handlers and
///     the total count equals 208 (Wave cross-audit 2026-05-05).
#[test]
fn hook_registry_has_cascade_queue_handlers() {
    let names = all_daemon_hook_names();

    assert!(
        names.contains(&"cli-cascade-queue-status"),
        "hook registry must contain cli-cascade-queue-status"
    );
    assert!(
        names.contains(&"cli-cascade-queue-drain"),
        "hook registry must contain cli-cascade-queue-drain"
    );
    assert_eq!(
        names.len(),
        222,
        "hook registry must have exactly 222 entries (sync with touring-dispatch hook_registry test), got {}",
        names.len()
    );
}

/// T3: Low-severity proposals are not enqueued — the queue filters them out.
#[test]
fn cascade_queue_low_severity_not_enqueued() {
    let queue = CascadeQueue::new();
    let plan = make_plan(Severity::Low);

    queue.push(Path::new("src/lib.rs"), &plan);

    assert!(
        queue.is_empty(),
        "low-severity proposals must not be enqueued (filtered at push)"
    );
}
