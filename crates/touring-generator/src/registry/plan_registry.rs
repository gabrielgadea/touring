//! `PlanRegistry` — concurrent map of in-flight plan executors.

use crate::plan::result::ExecutionStatus;
use moka::sync::Cache;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use uuid::Uuid;

/// Handle stored in the registry for an in-flight plan.
#[derive(Debug, Clone)]
pub struct PlanExecutorHandle {
    /// Unique identifier of the in-flight plan.
    pub plan_id: Uuid,
    /// Current execution status of the plan.
    pub status: ExecutionStatus,
    /// Truncated preview of the plan's intent, for listing/diagnostics.
    pub intent_preview: String,
}

/// Registry of all in-flight plans keyed by `plan_id`.
///
/// Uses `moka` W-TinyLFU cache for lock-free concurrent access with admission policy.
pub struct PlanRegistry {
    map: Cache<Uuid, PlanExecutorHandle>,
    count: AtomicUsize,
}

impl PlanRegistry {
    /// Construct an empty registry.
    #[must_use]
    pub fn new() -> Self {
        Self {
            map: Cache::new(10_000),
            count: AtomicUsize::new(0),
        }
    }

    /// Register a new plan.
    pub fn register(&self, handle: PlanExecutorHandle) {
        self.map.insert(handle.plan_id, handle);
        self.count.fetch_add(1, Ordering::Relaxed);
    }

    /// Update the status of an existing plan.
    pub fn update_status(&self, plan_id: Uuid, status: ExecutionStatus) {
        // moka get() returns Option<V> directly, not a ref with fields
        // We need to remove and re-insert for update semantics
        if let Some(mut handle) = self.map.remove(&plan_id) {
            handle.status = status;
            self.map.insert(plan_id, handle);
        }
    }

    /// Remove a completed or failed plan from the registry.
    #[must_use]
    pub fn remove(&self, plan_id: Uuid) -> Option<PlanExecutorHandle> {
        let result = self.map.remove(&plan_id);
        if result.is_some() {
            self.count.fetch_sub(1, Ordering::Relaxed);
        }
        result
    }

    /// Returns the current status of a plan, if registered.
    #[must_use]
    pub fn status(&self, plan_id: Uuid) -> Option<ExecutionStatus> {
        self.map.get(&plan_id).map(|e| e.status.clone())
    }

    /// Returns the number of in-flight plans.
    #[must_use]
    pub fn len(&self) -> usize {
        self.count.load(Ordering::Relaxed)
    }

    /// Returns true if no plans are currently in flight.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns a snapshot of all in-flight plans as a list.
    ///
    /// Each entry is `(plan_id_string, intent_preview, status)`.
    /// Clones are cheap — `ExecutionStatus` is `Clone`.
    #[must_use]
    pub fn list(&self) -> Vec<(String, String, ExecutionStatus)> {
        self.map
            .iter()
            .map(|(plan_id, handle)| {
                (
                    plan_id.to_string(),
                    handle.intent_preview.clone(),
                    handle.status.clone(),
                )
            })
            .collect()
    }
}

impl Default for PlanRegistry {
    fn default() -> Self {
        Self::new()
    }
}

/// Shared reference to the plan registry.
pub type SharedPlanRegistry = Arc<PlanRegistry>;
