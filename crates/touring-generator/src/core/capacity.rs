//! `CapacityLimits` and `PlanPriority` — resource budget enforcement for concurrent plans.

use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

/// Resource limits governing a single plan execution.
#[derive(Debug, Clone, Serialize, Deserialize, JsonSchema)]
pub struct CapacityLimits {
    /// Maximum number of plans executing concurrently (Semaphore permits).
    pub max_concurrent_plans: u16,
    /// Maximum serialized plan size in bytes.
    pub max_plan_size_bytes: u32,
    /// Maximum number of output files per plan.
    pub max_files_per_plan: u16,
    /// Maximum replan iterations before escalation.
    pub max_iterations: u8,
    /// Timeout for VGP batch verification in milliseconds.
    pub vgp_timeout_ms: u32,
    /// Timeout for template rendering in milliseconds.
    pub render_timeout_ms: u32,
    /// Timeout for speculative validation in milliseconds.
    pub speculate_timeout_ms: u32,
    /// Timeout for atomic commit in milliseconds.
    pub commit_timeout_ms: u32,
    /// Minimum speculate score required to proceed to commit (default 0.8).
    pub speculate_threshold: f64,
}

impl Default for CapacityLimits {
    fn default() -> Self {
        Self {
            max_concurrent_plans: 8,
            max_plan_size_bytes: 1_048_576, // 1 MiB
            max_files_per_plan: 64,
            max_iterations: 5,
            vgp_timeout_ms: 5_000,
            render_timeout_ms: 10_000,
            speculate_timeout_ms: 50_000,
            commit_timeout_ms: 100_000,
            speculate_threshold: 0.8,
        }
    }
}

/// Scheduling priority for a plan in the backpressure queue.
#[derive(
    Debug, Clone, Copy, Serialize, Deserialize, JsonSchema, PartialEq, Eq, PartialOrd, Ord, Default,
)]
pub enum PlanPriority {
    /// Lowest priority — scheduled only when no higher-priority plan is queued.
    Low,
    /// Default priority for ordinary plans.
    #[default]
    Normal,
    /// Elevated priority — preempts `Normal` and `Low` plans in the queue.
    High,
    /// Highest priority — scheduled ahead of all other plans.
    Critical,
}
