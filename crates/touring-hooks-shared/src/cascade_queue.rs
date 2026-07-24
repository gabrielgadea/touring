//! In-memory queue of pending cascade proposals from post_edit analysis.
//!
//! `CascadeQueue` accumulates high-severity API surface change proposals
//! detected by `api_cascade_bridge::analyze_rust_edit`. The queue is drained
//! by the MCP `touring_decompose drain_cascades` action, which converts pending
//! proposals into real subtasks in an active decomposer DAG.
//!
//! Design constraints:
//! - `Mutex<VecDeque>` — simple, no async needed (post_edit is sync)
//! - `max_len = 256` — bounded to prevent unbounded growth under edit storms
//! - `ttl = 1h` — stale proposals evicted on drain

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, SystemTime};

use touring_code::ast::api_cascade::SubtaskProposal;

// ─── Constants ───────────────────────────────────────────────────────────────

const DEFAULT_MAX_LEN: usize = 256;
const DEFAULT_TTL: Duration = Duration::from_secs(3600);

// ─── Types ───────────────────────────────────────────────────────────────────

/// A single pending cascade item — the file path + proposals queued at a point in time.
#[derive(Debug)]
pub struct PendingCascade {
    /// File that triggered the cascade analysis.
    pub path: PathBuf,
    /// High-severity proposals from `CascadePlan::high_severity()`.
    pub proposals: Vec<SubtaskProposal>,
    /// When this item was enqueued (for TTL eviction).
    pub queued_at: SystemTime,
}

/// Bounded, TTL-evicting queue of cascade proposals.
///
/// Thread-safe via an internal `Mutex`. All operations are non-async and
/// short-lived, so lock contention is negligible.
pub struct CascadeQueue {
    inner: Mutex<VecDeque<PendingCascade>>,
    max_len: usize,
    ttl: Duration,
}

impl std::fmt::Debug for CascadeQueue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let len = self.inner.lock().map(|g| g.len()).unwrap_or(0);
        f.debug_struct("CascadeQueue")
            .field("len", &len)
            .field("max_len", &self.max_len)
            .finish()
    }
}

// ─── Implementation ──────────────────────────────────────────────────────────

impl CascadeQueue {
    /// Create a new queue with default limits (256 items, 1h TTL).
    pub fn new() -> Self {
        Self {
            inner: Mutex::new(VecDeque::new()),
            max_len: DEFAULT_MAX_LEN,
            ttl: DEFAULT_TTL,
        }
    }

    /// Override defaults — useful in tests.
    #[cfg(test)]
    pub fn with_limits(max_len: usize, ttl: Duration) -> Self {
        Self {
            inner: Mutex::new(VecDeque::new()),
            max_len,
            ttl,
        }
    }

    /// Enqueue high-severity proposals from a cascade plan.
    ///
    /// Only proposals where `severity == Severity::High` are kept (filtered by
    /// `CascadePlan::high_severity()`). Does nothing when `proposals` is empty.
    /// Silently drops the oldest item when `max_len` would be exceeded.
    pub fn push(&self, path: &std::path::Path, plan: &touring_code::ast::api_cascade::CascadePlan) {
        let proposals: Vec<SubtaskProposal> = plan.high_severity().into_iter().cloned().collect();

        if proposals.is_empty() {
            return;
        }

        let item = PendingCascade {
            path: path.to_path_buf(),
            proposals,
            queued_at: SystemTime::now(),
        };

        let Ok(mut guard) = self.inner.lock() else {
            return;
        };

        if guard.len() >= self.max_len {
            guard.pop_front(); // evict oldest
        }
        guard.push_back(item);
    }

    /// Drain and return all items whose `queued_at` is within TTL.
    ///
    /// Stale items (age > TTL) are discarded without being returned.
    /// The queue is left empty after this call.
    pub fn drain_fresh(&self) -> Vec<PendingCascade> {
        let Ok(mut guard) = self.inner.lock() else {
            return Vec::new();
        };

        let now = SystemTime::now();
        guard
            .drain(..)
            .filter(|item| {
                now.duration_since(item.queued_at)
                    .map(|age| age < self.ttl)
                    .unwrap_or(false)
            })
            .collect()
    }

    /// Remove stale items from the queue without draining fresh ones.
    ///
    /// Idempotent — safe to call at any time as a housekeeping step.
    pub fn evict_stale(&self) {
        let Ok(mut guard) = self.inner.lock() else {
            return;
        };
        let now = SystemTime::now();
        guard.retain(|item| {
            now.duration_since(item.queued_at)
                .map(|age| age < self.ttl)
                .unwrap_or(false)
        });
    }

    /// Current number of pending items (fresh + stale).
    pub fn len(&self) -> usize {
        self.inner.lock().map(|g| g.len()).unwrap_or(0)
    }

    /// `true` when no pending items.
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

impl Default for CascadeQueue {
    fn default() -> Self {
        Self::new()
    }
}

// ─── Tests ───────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use touring_code::ast::api_cascade::{CascadePlan, Severity, SubtaskProposal};
    use touring_code::ast::rust_semantic::ApiChangeKind;

    fn make_plan_with_severity(severity: Severity) -> CascadePlan {
        CascadePlan {
            proposals: vec![SubtaskProposal {
                api_item: "pub fn foo()".to_string(),
                symbol: "foo".to_string(),
                kind: ApiChangeKind::Removed,
                callers: vec![],
                reason: "pub fn foo removed".to_string(),
                severity,
            }],
        }
    }

    #[test]
    fn queue_push_then_drain_roundtrip() {
        let q = CascadeQueue::new();
        assert!(q.is_empty());

        let plan = make_plan_with_severity(Severity::High);
        q.push(Path::new("src/lib.rs"), &plan);
        assert_eq!(q.len(), 1);

        let drained = q.drain_fresh();
        assert_eq!(drained.len(), 1);
        assert_eq!(drained[0].proposals.len(), 1);
        assert!(q.is_empty(), "queue must be empty after drain");
    }

    #[test]
    fn low_severity_proposals_not_enqueued() {
        let q = CascadeQueue::new();
        let plan = make_plan_with_severity(Severity::Low);
        q.push(Path::new("src/lib.rs"), &plan);
        assert!(q.is_empty(), "low-severity proposals must not be enqueued");
    }

    #[test]
    fn queue_respects_max_len_capacity() {
        let q = CascadeQueue::with_limits(3, DEFAULT_TTL);
        let plan = make_plan_with_severity(Severity::High);

        for i in 0..5 {
            let path = format!("src/file{i}.rs");
            q.push(Path::new(&path), &plan);
        }
        // max_len=3, so only 3 items remain (oldest evicted)
        assert_eq!(q.len(), 3);
    }

    #[test]
    fn stale_cascades_are_evicted_by_ttl() {
        // TTL of 0 means everything is immediately stale
        let q = CascadeQueue::with_limits(256, Duration::from_secs(0));
        let plan = make_plan_with_severity(Severity::High);
        q.push(Path::new("src/lib.rs"), &plan);

        // drain_fresh should return nothing (all items stale)
        let drained = q.drain_fresh();
        assert!(
            drained.is_empty(),
            "stale items must not be returned by drain_fresh"
        );
        assert!(q.is_empty(), "queue must be empty after drain");
    }

    #[test]
    fn evict_stale_removes_stale_items() {
        let q = CascadeQueue::with_limits(256, Duration::from_secs(0));
        let plan = make_plan_with_severity(Severity::High);
        q.push(Path::new("src/lib.rs"), &plan);
        assert_eq!(q.len(), 1);

        q.evict_stale();
        assert!(q.is_empty(), "evict_stale must remove stale items");
    }

    #[test]
    fn drain_empties_the_queue() {
        let q = CascadeQueue::new();
        let plan = make_plan_with_severity(Severity::High);
        q.push(Path::new("src/a.rs"), &plan);
        q.push(Path::new("src/b.rs"), &plan);
        assert_eq!(q.len(), 2);

        let _ = q.drain_fresh();
        assert!(q.is_empty());
    }
}
