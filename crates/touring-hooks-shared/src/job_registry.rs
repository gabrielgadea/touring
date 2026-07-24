//! L7-B Gamma: Spawn & Poll async primitives for long-running MCP workers.
//!
//! Implements the `JobRegistry` pattern proposed in the L7-B monograph as the
//! "MCP Grip Cognitivo" — the async counterpart to synchronous tool calls.
//!
//! # Design
//!
//! A process-global `JobRegistry` holds a `DashMap<String, JobState>` that
//! tracks spawned background tasks. Each task runs on the tokio runtime and
//! writes its result into the map on completion.
//!
//! # Workflow
//!
//! 1. Caller invokes `spawn_worker(tool_name, command, args)` → returns `job_id`
//! 2. Background tokio task executes the command and records outcome
//! 3. Caller later invokes `poll_worker(job_id)` → returns status + result
//!
//! # Why DashMap (and NOT moka)?
//!
//! - Lock-sharded: concurrent spawn/poll don't contend on the same lock
//! - Insertion O(1) amortized
//! - Read during poll O(1) — no global read lock
//!
//! **Important**: the 2026-04-16 moka-expansion wave intentionally left this
//! map on DashMap because `JobState::Running` owns a `tokio::task::JoinHandle`
//! that must be `.abort()`-able by exactly one caller (`drop_job`). Any
//! automatic eviction (TTL or capacity) would drop the handle without aborting
//! the task, leaking a detached future. The complementary cache for *terminal*
//! outcomes lives in `shared::terminal_job_cache` and is moka-backed.
//!
//! # Lifecycle
//!
//! Jobs remain in the registry indefinitely until `poll_worker` retrieves
//! a terminal state (Completed/Failed) AND the caller invokes `drop_job`.
//! A `gc` helper is provided to sweep jobs older than a threshold.

use crate::terminal_job_cache;
use dashmap::DashMap;
use std::sync::Arc;
use std::sync::OnceLock;
use std::time::{SystemTime, UNIX_EPOCH};
use tokio::task::JoinHandle;

// Compile-time invariants for the JobRegistry.
//
// These assertions fail the build the moment someone accidentally adds a
// non-Send field to `JobState` (e.g. a raw pointer or a `Cell<_>`). The
// registry stores `JobState` in `DashMap<String, JobState>` and is shared
// across the tokio multi-thread runtime, so `Send` is load-bearing. Without
// these asserts, a violation would only surface at the use site with a
// confusing trait-bound error pointing at DashMap internals.
#[cfg(test)]
mod _job_state_invariants {
    use super::JobState;
    static_assertions::assert_impl_all!(JobState: Send);
    // Note: JobState is intentionally NOT Sync — the JoinHandle inside Running
    // requires exclusive access for `.abort()`. Storing inside DashMap is safe
    // because DashMap shards provide the required synchronization.
}

/// Job state variants — Running is the only non-terminal state.
#[derive(Debug)]
pub enum JobState {
    /// Job is still executing on the tokio runtime.
    ///
    /// The `JoinHandle` allows cancellation via `abort()` from `drop_job`.
    Running {
        /// Join handle for the spawned tokio task; awaited on poll, aborted on drop.
        handle: JoinHandle<Result<String, String>>,
        /// Unix timestamp (seconds) when the worker was spawned.
        started_at_secs: u64,
    },
    /// Job completed successfully. `result` is the stdout/output string.
    Completed {
        /// Captured stdout produced by the finished worker.
        result: String,
        /// Unix timestamp (seconds) when the worker was spawned.
        started_at_secs: u64,
        /// Unix timestamp (seconds) when the worker finished.
        finished_at_secs: u64,
    },
    /// Job failed with an error message.
    Failed {
        /// Human-readable failure reason (spawn error, non-zero exit, or join error).
        error: String,
        /// Unix timestamp (seconds) when the worker was spawned.
        started_at_secs: u64,
        /// Unix timestamp (seconds) when the failure was recorded.
        finished_at_secs: u64,
    },
}

impl JobState {
    /// Returns the start timestamp for any state variant.
    pub fn started_at(&self) -> u64 {
        match self {
            JobState::Running {
                started_at_secs, ..
            } => *started_at_secs,
            JobState::Completed {
                started_at_secs, ..
            } => *started_at_secs,
            JobState::Failed {
                started_at_secs, ..
            } => *started_at_secs,
        }
    }

    /// Returns `true` if the job is in a terminal state (Completed or Failed).
    pub fn is_terminal(&self) -> bool {
        matches!(self, JobState::Completed { .. } | JobState::Failed { .. })
    }

    /// Returns a short status string for CLI display.
    pub fn status_str(&self) -> &'static str {
        match self {
            JobState::Running { .. } => "running",
            JobState::Completed { .. } => "completed",
            JobState::Failed { .. } => "failed",
        }
    }
}

/// Global JobRegistry singleton — shared across daemon + hooks.
///
/// Uses `OnceLock<Arc<DashMap>>` so the registry persists for the daemon's
/// lifetime and can be cloned cheaply via `Arc::clone`.
static JOB_REGISTRY: OnceLock<Arc<DashMap<String, JobState>>> = OnceLock::new();

/// Get the global job registry, initializing on first access.
#[inline]
pub fn registry() -> Arc<DashMap<String, JobState>> {
    JOB_REGISTRY
        .get_or_init(|| Arc::new(DashMap::new()))
        .clone()
}

/// Retention window (seconds) for terminal jobs before [`gc`] evicts them.
pub const JOB_RETENTION_SECS: u64 = 3600;

/// Soft capacity: when the registry grows past this, [`spawn_worker`] runs an
/// opportunistic [`gc`] so the process-global map can't grow unbounded.
pub const JOB_REGISTRY_SOFT_CAP: usize = 256;

/// Evict **terminal** (`Completed`/`Failed`) jobs whose start time is older than
/// `max_age_secs`, returning the number of entries removed.
///
/// `Running` jobs are never evicted: dropping a live `JoinHandle` without
/// aborting it would leak the underlying tokio task (see the module docs on why
/// naive TTL/capacity eviction is unsafe). Terminal jobs hold no live task, so
/// removing them only frees their stored stdout/error string — the actual fix
/// for the slow `JOB_REGISTRY` leak (F-6).
pub fn gc(max_age_secs: u64) -> usize {
    let reg = registry();
    let cutoff = now_secs().saturating_sub(max_age_secs);
    // Collect keys first so the DashMap iterator's shard locks are released
    // before `remove` re-acquires them (avoids self-deadlock).
    let stale: Vec<String> = reg
        .iter()
        .filter(|entry| entry.value().is_terminal() && entry.value().started_at() <= cutoff)
        .map(|entry| entry.key().clone())
        .collect();
    let removed = stale.len();
    for key in stale {
        reg.remove(&key);
    }
    if removed > 0 {
        tracing::debug!(removed, "job_registry: gc evicted stale terminal jobs");
    }
    removed
}

/// Count jobs whose id starts with `prefix` that are still `Running`
/// (non-terminal). Used to cap concurrent spawns of a heavyweight tool (e.g.
/// `cargo mutants`) so rapid edits can't pile up unbounded subprocesses (F-7).
pub fn count_running(prefix: &str) -> usize {
    registry()
        .iter()
        .filter(|entry| entry.key().starts_with(prefix) && !entry.value().is_terminal())
        .count()
}

/// Generate a unique job id based on tool_name + current timestamp.
///
/// Format: `{tool_name}-{epoch_ms}-{counter}`. The counter is a local atomic
/// incremented per call to avoid collisions for rapid spawns in the same ms.
fn generate_job_id(tool_name: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    format!("{tool_name}-{now_ms}-{n}")
}

/// Current unix time in seconds.
fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

/// Spawn a background worker executing a program with arguments and return the job id.
///
/// Uses `tokio::process::Command::new(program).args(args)` which invokes `execve`
/// directly — **no shell interpolation**, safe from command injection even if
/// callers pass user-provided arguments.
///
/// # Arguments
///
/// * `tool_name` — logical tool name, used in the generated job_id prefix
/// * `program` — executable name or path (e.g., "cargo", "/usr/bin/touring")
/// * `args` — individual argument strings; each is passed as a distinct argv entry
///
/// # Returns
///
/// The generated `job_id` (format: `{tool_name}-{epoch_ms}-{counter}`).
///
/// # Security
///
/// Because no shell is invoked, arguments like `"foo; rm -rf /"` are passed
/// as a single literal argument, not interpreted as shell metacharacters.
///
/// # No-Runtime Safety
///
/// When called outside a Tokio runtime (e.g., some test contexts), inserts a
/// `Failed` state instead of panicking. The daemon always runs inside a Tokio
/// runtime, so this path is only hit in tests or during early init.
pub fn spawn_worker(tool_name: &str, program: &str, args: &[String]) -> String {
    let job_id = generate_job_id(tool_name);
    let started_at_secs = now_secs();

    // Try to get the current Tokio runtime handle. If none, insert Failed and
    // return early rather than panicking — ensures tests without #[tokio::test]
    // still execute cleanly.
    let rt_handle = match tokio::runtime::Handle::try_current() {
        Ok(h) => h,
        Err(_) => {
            tracing::warn!(job_id = %job_id, tool_name, "spawn_worker called outside Tokio runtime — marking job as failed");
            registry().insert(
                job_id.clone(),
                JobState::Failed {
                    error: "no tokio runtime".to_string(),
                    started_at_secs,
                    finished_at_secs: started_at_secs,
                },
            );
            return job_id;
        }
    };

    let program_owned = program.to_string();
    let args_owned: Vec<String> = args.to_vec();

    let handle: JoinHandle<Result<String, String>> = rt_handle.spawn(async move {
        let output = tokio::process::Command::new(&program_owned)
            .args(&args_owned)
            .output()
            .await
            .map_err(|e| format!("failed to spawn {program_owned}: {e}"))?;

        if output.status.success() {
            Ok(String::from_utf8_lossy(&output.stdout).to_string())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).to_string();
            let exit = output.status.code().unwrap_or(-1);
            Err(format!("exit={exit} stderr={stderr}"))
        }
    });

    registry().insert(
        job_id.clone(),
        JobState::Running {
            handle,
            started_at_secs,
        },
    );

    // Opportunistic bounded-registry gc (F-6): when the registry grows past the
    // soft cap, evict terminal jobs older than the retention window so the
    // process-global map can't leak when jobs are spawned but never polled.
    if registry().len() > JOB_REGISTRY_SOFT_CAP {
        let evicted = gc(JOB_RETENTION_SECS);
        if evicted > 0 {
            tracing::debug!(evicted, "job_registry: opportunistic gc on spawn");
        }
    }

    tracing::info!(job_id = %job_id, tool_name, "job: spawned");
    job_id
}

/// Placeholder state used during the Running→Terminal transition.
///
/// `poll_worker` must temporarily replace the `Running` entry with a
/// placeholder so it can take ownership of the `JoinHandle`. The placeholder
/// is immediately overwritten with the real terminal state before returning.
#[inline]
fn placeholder_state() -> JobState {
    JobState::Failed {
        error: "transitioning".to_string(),
        started_at_secs: 0,
        finished_at_secs: 0,
    }
}

/// Build a terminal `JobState` from the outcome of a finished `JoinHandle`.
///
/// Maps the three possible join outcomes:
/// - `Ok(Ok(stdout))`         → `Completed { result: stdout, ... }`
/// - `Ok(Err(err_string))`    → `Failed { error: err_string, ... }` (worker error)
/// - `Err(join_error)`        → `Failed { error: "join error: ...", ... }` (panic/abort)
#[inline]
fn terminal_from_join(
    join_result: Result<Result<String, String>, tokio::task::JoinError>,
    started_at_secs: u64,
    finished_at_secs: u64,
) -> JobState {
    match join_result {
        Ok(Ok(result)) => JobState::Completed {
            result,
            started_at_secs,
            finished_at_secs,
        },
        Ok(Err(err)) => JobState::Failed {
            error: err,
            started_at_secs,
            finished_at_secs,
        },
        Err(join_err) => JobState::Failed {
            error: format!("join error: {join_err}"),
            started_at_secs,
            finished_at_secs,
        },
    }
}

/// Try to transition a `Running` entry to a terminal state if its handle is finished.
///
/// Does nothing if the entry is not `Running` or the handle is still executing.
/// Requires an active tokio runtime for `block_on` — returns `Err(string)` with
/// a descriptive message if unavailable. On success, the entry is mutated in-place.
fn try_transition_to_terminal(
    entry: &mut dashmap::mapref::one::RefMut<String, JobState>,
) -> Result<(), String> {
    // Short-circuit if not Running or handle still working.
    let should_transition = match entry.value() {
        JobState::Running { handle, .. } => handle.is_finished(),
        _ => false,
    };
    if !should_transition {
        return Ok(());
    }

    // Temporarily swap out the Running state so we can .await the handle.
    let old_state = std::mem::replace(entry.value_mut(), placeholder_state());
    let JobState::Running {
        handle,
        started_at_secs,
    } = old_state
    else {
        // Impossible: should_transition was true only for Running variants.
        return Ok(());
    };

    let rt_handle = match tokio::runtime::Handle::try_current() {
        Ok(h) => h,
        Err(_) => {
            // Restore some reasonable state and signal the error.
            *entry.value_mut() = JobState::Failed {
                error: "no tokio runtime for join".to_string(),
                started_at_secs,
                finished_at_secs: now_secs(),
            };
            return Err("no tokio runtime for join".to_string());
        }
    };

    // block_in_place requires a multi-threaded runtime. The daemon and all tests
    // (#[tokio::test(flavor = "multi_threaded")]) use multi-threaded, so this branch
    // is the only one that executes in practice.
    let join_result = if rt_handle.runtime_flavor() == tokio::runtime::RuntimeFlavor::MultiThread {
        tokio::task::block_in_place(|| rt_handle.block_on(handle))
    } else {
        // Dead code path: multi-threaded runtime is always used in production and tests.
        // Kept for completeness in case a single-threaded runtime is used.
        drop(handle);
        unreachable!("single-threaded runtime not supported for job registry polling")
    };
    *entry.value_mut() = terminal_from_join(join_result, started_at_secs, now_secs());
    Ok(())
}

/// Serialize a `JobState` reference as a JSON value for CLI/MCP consumers.
fn state_to_json(job_id: &str, state: &JobState) -> serde_json::Value {
    match state {
        JobState::Running {
            started_at_secs, ..
        } => serde_json::json!({
            "job_id": job_id,
            "status": "running",
            "started_at_secs": started_at_secs,
        }),
        JobState::Completed {
            result,
            started_at_secs,
            finished_at_secs,
        } => serde_json::json!({
            "job_id": job_id,
            "status": "completed",
            "result": result,
            "started_at_secs": started_at_secs,
            "finished_at_secs": finished_at_secs,
            "duration_secs": finished_at_secs.saturating_sub(*started_at_secs),
        }),
        JobState::Failed {
            error,
            started_at_secs,
            finished_at_secs,
        } => serde_json::json!({
            "job_id": job_id,
            "status": "failed",
            "error": error,
            "started_at_secs": started_at_secs,
            "finished_at_secs": finished_at_secs,
        }),
    }
}

/// Poll a job by id, transitioning Running → Completed/Failed when the
/// background task has finished, and returning a JSON-serializable status.
///
/// # Returns
///
/// A JSON object with shape:
/// ```json
/// { "job_id": "...", "status": "running|completed|failed", "result": "..."|null,
///   "error": "..."|null, "started_at_secs": N, "finished_at_secs": N|null }
/// ```
///
/// Returns `{"job_id": id, "status": "not_found"}` if id is unknown.
///
/// # Complexity
///
/// Delegates to `try_transition_to_terminal` and `state_to_json` helpers to
/// keep ciclomatic complexity ≤ 8 in this function.
pub fn poll_worker(job_id: &str) -> serde_json::Value {
    let reg = registry();
    let mut entry = match reg.get_mut(job_id) {
        Some(e) => e,
        None => {
            // Fallback to the terminal cache: a client polling after
            // `drop_job` (or after the live entry was reaped) still gets
            // the recorded outcome until TTL/size eviction reclaims it.
            if let Some(cached) = terminal_job_cache::lookup_terminal(job_id) {
                return cached;
            }
            return serde_json::json!({
                "job_id": job_id,
                "status": "not_found"
            });
        }
    };

    // Transition Running → terminal if the handle has finished. On runtime
    // errors, the helper already wrote a `Failed` state into the entry, so
    // the subsequent `state_to_json` call will report it correctly.
    let _ = try_transition_to_terminal(&mut entry);

    let snapshot = state_to_json(job_id, entry.value());

    // Warm the terminal cache exactly once per transition so the Arc-backed
    // copy outlives subsequent `drop_job` calls. Running jobs are ignored
    // (store_terminal rejects non-terminal status).
    if entry.value().is_terminal() {
        let _ = terminal_job_cache::store_terminal(job_id, &snapshot);
    }

    snapshot
}

/// List all jobs currently in the registry with their status.
///
/// Returns an array of `{job_id, status, started_at_secs}` objects.
pub fn list_jobs() -> serde_json::Value {
    let reg = registry();
    let items: Vec<serde_json::Value> = reg
        .iter()
        .map(|entry| {
            let state = entry.value();
            serde_json::json!({
                "job_id": entry.key(),
                "status": state.status_str(),
                "started_at_secs": state.started_at(),
                "is_terminal": state.is_terminal(),
            })
        })
        .collect();

    serde_json::json!({
        "job_count": items.len(),
        "jobs": items,
    })
}

/// Remove a job from the registry. Aborts the handle if still running.
///
/// Useful for explicit cleanup after polling a terminal state.
pub fn drop_job(job_id: &str) -> bool {
    let reg = registry();
    let removed = match reg.remove(job_id) {
        Some((_k, state)) => {
            if let JobState::Running { handle, .. } = state {
                handle.abort();
            }
            true
        }
        _ => false,
    };
    // Terminal cache entry — if any — is always purged, independent of
    // whether the live registry had the job. This keeps the two stores
    // consistent under concurrent poll/drop sequences.
    terminal_job_cache::forget(job_id);
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_job_id_format() {
        let id = generate_job_id("test");
        assert!(id.starts_with("test-"));
        // Format: test-{ms}-{counter} → 3 dash-separated parts
        assert_eq!(id.matches('-').count(), 2);
    }

    #[test]
    fn test_generate_job_id_unique() {
        let a = generate_job_id("x");
        let b = generate_job_id("x");
        assert_ne!(a, b, "job ids must be unique even in same ms");
    }

    #[test]
    fn test_job_state_is_terminal() {
        let running = JobState::Completed {
            result: "ok".to_string(),
            started_at_secs: 0,
            finished_at_secs: 1,
        };
        assert!(running.is_terminal());

        let failed = JobState::Failed {
            error: "boom".to_string(),
            started_at_secs: 0,
            finished_at_secs: 1,
        };
        assert!(failed.is_terminal());
    }

    #[test]
    fn test_job_state_status_str() {
        let completed = JobState::Completed {
            result: "ok".to_string(),
            started_at_secs: 0,
            finished_at_secs: 1,
        };
        assert_eq!(completed.status_str(), "completed");

        let failed = JobState::Failed {
            error: "err".to_string(),
            started_at_secs: 0,
            finished_at_secs: 1,
        };
        assert_eq!(failed.status_str(), "failed");
    }

    #[test]
    fn test_poll_not_found() {
        let result = poll_worker("nonexistent-job-12345");
        assert_eq!(result["status"], "not_found");
    }

    #[test]
    fn test_list_jobs_empty_or_valid() {
        // The registry may be shared with other tests, so we assert structure not size.
        let result = list_jobs();
        assert!(result.get("job_count").is_some());
        assert!(result.get("jobs").is_some());
        assert!(result["jobs"].is_array());
    }

    #[test]
    fn test_gc_evicts_old_terminal_keeps_fresh() {
        // Registry is process-global/shared with other tests → use unique keys and
        // assert on THOSE specifically (not on total counts).
        let reg = registry();
        let old = now_secs().saturating_sub(JOB_RETENTION_SECS + 100);
        let fresh = now_secs();
        let k_old = format!("gc-old-{}", generate_job_id("gc"));
        let k_fresh = format!("gc-fresh-{}", generate_job_id("gc"));
        reg.insert(
            k_old.clone(),
            JobState::Completed {
                result: "r".to_string(),
                started_at_secs: old,
                finished_at_secs: old,
            },
        );
        reg.insert(
            k_fresh.clone(),
            JobState::Failed {
                error: "e".to_string(),
                started_at_secs: fresh,
                finished_at_secs: fresh,
            },
        );

        let removed = gc(JOB_RETENTION_SECS);
        assert!(removed >= 1, "gc must evict at least the old terminal job");
        assert!(
            !reg.contains_key(&k_old),
            "old terminal job must be evicted"
        );
        assert!(
            reg.contains_key(&k_fresh),
            "fresh terminal job must be retained"
        );

        drop_job(&k_fresh); // cleanup
    }

    #[tokio::test(flavor = "multi_thread")]
    async fn test_spawn_and_poll_echo_command() {
        let job_id = spawn_worker("test-echo", "echo", &["hello_l7b".to_string()]);
        assert!(job_id.starts_with("test-echo-"));

        // Poll loop — job should complete within 1s for echo
        let mut attempts = 0;
        let poll = loop {
            let p = poll_worker(&job_id);
            if p["status"] != "running" {
                break p;
            }
            attempts += 1;
            if attempts > 50 {
                panic!("echo did not complete after 50 attempts");
            }
            tokio::time::sleep(std::time::Duration::from_millis(20)).await;
        };

        assert_eq!(poll["status"], "completed", "echo should succeed");
        assert!(
            poll["result"].as_str().unwrap().contains("hello_l7b"),
            "echo output should contain stdin: {poll}"
        );

        drop_job(&job_id);
    }
}
