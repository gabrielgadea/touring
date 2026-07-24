//! Companion cache for `job_registry` terminal states.
//!
//! The `job_registry` stores live `JoinHandle`s inside a `DashMap` because
//! running jobs need exclusive-abort semantics. Once a job transitions to
//! `Completed` or `Failed`, its payload is pure data and can migrate to a
//! moka cache with size + TTL caps.
//!
//! This keeps the DashMap bounded to *active* jobs, while clients that poll
//! a job after it finished still get the result until it ages out.
//!
//! # Lifecycle
//!
//! 1. `spawn_worker` → DashMap (Running)
//! 2. `poll_worker` detects terminal → snapshot JSON is inserted here via
//!    `store_terminal`
//! 3. `drop_job` purges the DashMap entry and calls `forget` to evict the
//!    terminal cache entry eagerly
//! 4. Callers that poll an already-dropped id can still recover the result
//!    via `lookup_terminal` until TTL/size eviction reclaims it

use crate::moka_policies::{MokaCacheStats, build_terminal_job_cache, sample_stats};
use moka::sync::Cache;
use std::sync::{Arc, OnceLock};

/// Snapshot of a terminal job — JSON-serialized shape identical to the
/// live `poll_worker` response. Kept as `Arc<str>` so clones are O(1).
#[derive(Debug)]
pub struct TerminalJobRecord {
    /// Identifier of the job this record snapshots.
    pub job_id: String,
    /// Terminal status, always `"completed"` or `"failed"`.
    pub status: &'static str,
    /// JSON-serialized poll response, shared as `Arc<str>` for O(1) clones.
    pub payload: Arc<str>,
    /// Byte length of `payload`, used by the cache weigher.
    pub bytes: u32,
}

impl TerminalJobRecord {
    /// Build a record from a JSON value. `status` must be `"completed"` or
    /// `"failed"` — other variants are rejected to keep the cache strictly
    /// terminal.
    pub fn from_json(job_id: &str, value: &serde_json::Value) -> Option<Self> {
        let status_str = value.get("status").and_then(|s| s.as_str())?;
        let status: &'static str = match status_str {
            "completed" => "completed",
            "failed" => "failed",
            _ => return None,
        };
        let text = value.to_string();
        let bytes = text.len().min(u32::MAX as usize) as u32;
        Some(Self {
            job_id: job_id.to_string(),
            status,
            payload: Arc::from(text),
            bytes,
        })
    }

    /// Decode the stored payload back into a `serde_json::Value`.
    pub fn to_json(&self) -> serde_json::Value {
        serde_json::from_str::<serde_json::Value>(&self.payload).unwrap_or_else(|_| {
            // Preserve the best-effort shape so downstream code never panics
            // on a malformed payload — we fall back to a minimal object.
            serde_json::json!({
                "job_id": self.job_id,
                "status": self.status,
                "corrupted": true,
            })
        })
    }
}

/// Global singleton so the cache persists for the daemon's lifetime.
static TERMINAL_JOB_CACHE: OnceLock<Cache<String, Arc<TerminalJobRecord>>> = OnceLock::new();

/// Fetch (or lazily initialise) the process-wide cache.
fn cache() -> &'static Cache<String, Arc<TerminalJobRecord>> {
    TERMINAL_JOB_CACHE
        .get_or_init(|| build_terminal_job_cache::<TerminalJobRecord, _>(|rec| rec.bytes))
}

/// Store a terminal job snapshot. Returns `true` if the value parsed
/// correctly and was inserted, `false` otherwise.
pub fn store_terminal(job_id: &str, value: &serde_json::Value) -> bool {
    match TerminalJobRecord::from_json(job_id, value) {
        Some(rec) => {
            cache().insert(job_id.to_string(), Arc::new(rec));
            true
        }
        None => false,
    }
}

/// Lookup a terminal job snapshot, reconstituting the JSON value on hit.
pub fn lookup_terminal(job_id: &str) -> Option<serde_json::Value> {
    cache().get(job_id).map(|rec| rec.to_json())
}

/// Drop a terminal entry — idempotent.
pub fn forget(job_id: &str) {
    cache().invalidate(job_id);
}

/// Snapshot of the cache for observability handlers.
pub fn stats() -> MokaCacheStats {
    sample_stats(cache())
}

/// Clear everything (test-only convenience — production code should rely
/// on TTL + size eviction).
pub fn clear_for_tests() {
    cache().invalidate_all();
    cache().run_pending_tasks();
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_id(prefix: &str) -> String {
        use std::sync::atomic::{AtomicU64, Ordering};
        static N: AtomicU64 = AtomicU64::new(0);
        format!("{prefix}-{}", N.fetch_add(1, Ordering::Relaxed))
    }

    #[test]
    fn from_json_requires_terminal_status() {
        let ok = TerminalJobRecord::from_json(
            "job-1",
            &serde_json::json!({"status": "completed", "result": "hi"}),
        );
        assert!(ok.is_some());

        let running =
            TerminalJobRecord::from_json("job-1", &serde_json::json!({"status": "running"}));
        assert!(running.is_none());
    }

    #[test]
    fn store_and_lookup_round_trip() {
        let id = unique_id("rt");
        let payload = serde_json::json!({
            "job_id": &id,
            "status": "completed",
            "result": "42",
        });
        assert!(store_terminal(&id, &payload));

        let got = lookup_terminal(&id).expect("cached");
        assert_eq!(got["status"], "completed");
        assert_eq!(got["result"], "42");
        forget(&id);
    }

    #[test]
    fn forget_is_idempotent() {
        let id = unique_id("idem");
        forget(&id); // nothing stored — must not panic
        let payload = serde_json::json!({"status": "failed", "error": "boom"});
        store_terminal(&id, &payload);
        forget(&id);
        forget(&id);
        assert!(lookup_terminal(&id).is_none());
    }

    #[test]
    fn stats_reflect_insertions() {
        // Use a dedicated id set so other tests don't race the counter.
        let a = unique_id("stats");
        let b = unique_id("stats");
        let _ = store_terminal(
            &a,
            &serde_json::json!({"status": "completed", "result": "a"}),
        );
        let _ = store_terminal(&b, &serde_json::json!({"status": "failed", "error": "b"}));
        let s = stats();
        assert!(
            s.entry_count >= 2,
            "expected >=2 entries, got {}",
            s.entry_count
        );
        forget(&a);
        forget(&b);
    }

    #[test]
    fn corrupted_payload_falls_back_to_minimal_shape() {
        let rec = TerminalJobRecord {
            job_id: "broken".into(),
            status: "failed",
            payload: Arc::from("not-json-$#@"),
            bytes: 12,
        };
        let value = rec.to_json();
        assert_eq!(value["job_id"], "broken");
        assert_eq!(value["corrupted"], true);
    }
}
