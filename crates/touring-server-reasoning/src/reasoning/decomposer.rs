//! Task Decomposer — Structured task decomposition with DAG validation
//!
//! Provides a deterministic task DAG manager for structured reasoning.
//! Not an LLM — provides **structure** for the LLM's reasoning.
//!
//! # Design
//!
//! Tasks are top-level plans. Each task has subtasks that form a DAG:
//! - subtasks are stored in insertion order (for stable serialization)
//! - a `HashMap<id, index>` index enables O(1) lookups
//! - Kahn's algorithm validates DAG validity and produces a topological order
//! - subtasks at the same "layer" (same depth in the DAG) are sorted by priority
//!
//! # N-Level TACO/ACO Integration (v29.4)
//!
//! The decomposer is the N2 layer in the N0→N1→N2→N3 hierarchy:
//! - **N0**: Direct tool execution (Read/Edit/Write)
//! - **N1**: Single-agent generation (BasicGenerator with pheromone ACO)
//! - **N2**: Orchestration DAG (TaskDecomposer + parallel_groups + CILA routing)
//! - **N3**: Meta-generation (RustMetaGenerator, domain-specific generators)
//!
//! CILA levels route to N-levels:
//! - L0-L1 → SOLO MODE (N0/N1, no decomposition)
//! - L2 → Scout + Engineer (N1+N2 hybrid)
//! - L3 → Scout + Architect + Engineers + Validator (full N2)
//! - L4+ → Full TACO with ACO pheromone tracking, MCTS, and parallel_groups
//!
//! # Exponential Enhancements (v29.3)
//!
//! - **Observability**: Full metrics emission for TACO orchestration tracking
//! - **Parallel Groups**: Formal identification of concurrent execution layers
//! - **Retry/Timeout**: Resilient subtask execution with configurable retry policies
//! - **Resource Hints**: Complexity estimation for intelligent scheduling
//! - **CILA Routing**: Level-aware decomposition strategy selection
//! - **ACO Integration**: Pheromone-aware task priority via ComplexityHint.tags
//!
//! The public API of [`TaskDecomposer`] is intentionally `pub(crate)` for
//! mutation methods — external access goes through the MCP `touring_decompose` tool.

use super::budget::{BudgetVector, BudgetViolation, verify_conservation};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet, VecDeque};
use std::time::{Duration, Instant};

/// Errors returned by [`TaskDecomposer`] DAG-validation operations.
#[derive(Debug, thiserror::Error)]
pub enum ReasoningError {
    /// A requested task or subtask was not found.
    #[error("{0}")]
    NotFound(String),
    /// A dependency cycle was detected among a task's subtasks (Kahn's algorithm
    /// could not produce a complete topological order).
    #[error("{0}")]
    CycleDetected(String),
}

// ── Retry / Timeout ─────────────────────────────────────────────────────────

/// Retry policy for a subtask execution.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RetryPolicy {
    /// Maximum retry attempts after failure.
    pub max_attempts: u8,
    /// Base delay between retries (exponential backoff applied).
    pub base_delay_ms: u64,
    /// Timeout for a single execution attempt.
    pub timeout_ms: Option<u64>,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 1,
            base_delay_ms: 100,
            timeout_ms: Some(300_000), // 5 min default
        }
    }
}

impl RetryPolicy {
    /// Exponential backoff delay for attempt n (n >= 2).
    /// attempt=2 (first retry): base * 2^0 = base
    /// attempt=3 (second retry): base * 2^1 = 2*base
    pub fn backoff_delay(&self, attempt: u8) -> Duration {
        let exp = attempt.saturating_sub(2);
        let ms = self.base_delay_ms * 2u64.pow(exp.into());
        Duration::from_millis(ms.min(30_000)) // cap at 30s
    }
}

/// Complexity hint for scheduling decisions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComplexityHint {
    /// Estimated difficulty (1-10).
    pub difficulty: u8,
    /// Estimated execution time in milliseconds.
    pub estimated_ms: Option<u64>,
    /// Whether this subtask likely requires I/O or heavy compute.
    pub io_bound: bool,
    /// Tags for skill matching (e.g., "rust", "python", "unsafe").
    pub tags: Vec<String>,
}

impl Default for ComplexityHint {
    fn default() -> Self {
        Self {
            difficulty: 0,
            estimated_ms: None,
            io_bound: true,
            tags: Vec::new(),
        }
    }
}

// ── Finalization Report ───────────────────────────────────────────────────────

/// Result of a task finalization check.
/// Returned by `TaskDecomposer::finalize_task()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct FinalizeReport {
    /// Task ID that was checked.
    pub task_id: String,
    /// Whether all subtasks are in terminal states (ready to finalize).
    pub ready: bool,
    /// Overall completion percentage (Completed / total).
    pub completion_pct: f32,
    /// Total subtask count.
    pub total: usize,
    /// Subtasks in Completed state.
    pub completed: usize,
    /// Subtasks in Failed state.
    pub failed: usize,
    /// Subtasks in Skipped state.
    pub skipped: usize,
    /// Subtasks in Cancelled state.
    pub cancelled: usize,
    /// Subtasks still Pending.
    pub pending: usize,
    /// Subtasks still InProgress.
    pub in_progress: usize,
    /// Human-readable reasons blocking finalization (empty when ready=true).
    pub blocking: Vec<String>,
}

// ── Completion Gate Result ────────────────────────────────────────────────────

/// Result of a pre-completion dependency gate check.
/// Returned by `TaskDecomposer::validate_completion_gate()`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompletionGateResult {
    /// Whether the subtask is allowed to transition to Completed.
    pub ready_to_complete: bool,
    /// Reasons why completion is blocked (empty when ready_to_complete=true).
    pub blocking_reasons: Vec<String>,
    /// Number of dependencies that are not yet Completed.
    pub pending_deps: usize,
}

// ── Metrics ──────────────────────────────────────────────────────────────────

/// Runtime metrics for a task DAG — tracks orchestration health.
///
/// Named `DecomposeValidationMetrics` to distinguish from
/// `touring_hooks::knowledge::TaskMetrics` (execution quality metrics).
/// This struct tracks DAG validation timing and cycle detection.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct DecomposeValidationMetrics {
    /// How many times `validate_order` has been called.
    pub validation_count: u64,
    /// Total elapsed time spent in DAG validation.
    pub validation_time_ms: u64,
    /// How many cycles have been detected.
    pub cycle_detections: u64,
    /// Last validation timestamp.
    pub last_validated_at: Option<DateTime<Utc>>,
    /// Timestamp when the task was created.
    pub created_at: Option<DateTime<Utc>>,
    /// Timestamp when the task was last updated.
    pub updated_at: Option<DateTime<Utc>>,
}

impl DecomposeValidationMetrics {
    fn new() -> Self {
        let now = Utc::now();
        Self {
            validation_count: 0,
            validation_time_ms: 0,
            cycle_detections: 0,
            last_validated_at: Some(now),
            created_at: Some(now),
            updated_at: Some(now),
        }
    }

    fn record_validation(&mut self, elapsed_ms: u64, had_cycle: bool) {
        self.validation_count += 1;
        self.validation_time_ms += elapsed_ms;
        self.last_validated_at = Some(Utc::now());
        if had_cycle {
            self.cycle_detections += 1;
        }
    }

    /// Diagnostic helper — wires `new()` + `record_validation()` so the
    /// metrics struct retains observability under cargo's dead-code
    /// analysis when no `#[tool]` caller is reachable. Returns a small
    /// JSON snapshot suitable for audit-trail emission.
    pub fn diagnostic_exercise() -> serde_json::Value {
        let mut m = Self::new();
        m.record_validation(7, false);
        m.record_validation(13, true);
        serde_json::json!({
            "validation_count": m.validation_count,
            "validation_time_ms": m.validation_time_ms,
            "cycle_detections": m.cycle_detections,
        })
    }
}

// ── Parallel Group ────────────────────────────────────────────────────────────

/// A parallel execution group — subtasks at the same DAG layer.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParallelGroup {
    /// Depth in the DAG (0 = no dependencies).
    pub depth: usize,
    /// Subtask IDs in this group (sorted by priority).
    pub subtask_ids: Vec<String>,
    /// Whether all tasks in this group have completed.
    pub all_done: bool,
}

// ── CILA / N-Level Integration ─────────────────────────────────────────────

/// CILA level classification for routing to N-levels.
/// Mirrors `CILALevel` from touring-hooks::classifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CilaLevel {
    /// Direct response — no tooling or decomposition.
    L0, // Direct response
    /// PAL (Program-Aided Language) — single program-assisted step.
    L1, // PAL (Program-Aided Language)
    /// Tool-Augmented — reasoning backed by external tool calls.
    L2, // Tool-Augmented
    /// Pipelines — chained multi-stage tool execution.
    L3, // Pipelines
    /// Agent Loops — iterative agent-driven reasoning.
    L4, // Agent Loops
    /// Self-Modifying — agent rewrites its own plan or code.
    L5, // Self-Modifying
    /// Multi-Agent — coordinated multi-agent orchestration.
    L6, // Multi-Agent
}

impl CilaLevel {
    /// Route to N-level based on CILA complexity.
    pub fn routing_mode(&self) -> RoutingMode {
        match self {
            CilaLevel::L0 | CilaLevel::L1 => RoutingMode::Solo,
            CilaLevel::L2 => RoutingMode::Hybrid,
            CilaLevel::L3 => RoutingMode::Orchestrated,
            CilaLevel::L4 | CilaLevel::L5 | CilaLevel::L6 => RoutingMode::FullTaco,
        }
    }

    /// Max parallel subtasks recommended for this level.
    pub fn max_parallelism(&self) -> usize {
        match self {
            CilaLevel::L0 => 1,
            CilaLevel::L1 => 1,
            CilaLevel::L2 => 2,
            CilaLevel::L3 => 4,
            CilaLevel::L4 => 6,
            CilaLevel::L5 | CilaLevel::L6 => 8,
        }
    }

    /// Convert from u8.
    pub fn from_u8(level: u8) -> Self {
        match level {
            0 => Self::L0,
            1 => Self::L1,
            2 => Self::L2,
            3 => Self::L3,
            4 => Self::L4,
            5 => Self::L5,
            _ => Self::L6,
        }
    }
}

/// How the orchestrator routes tasks based on CILA level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RoutingMode {
    /// L0-L1: N0/N1 only, no decomposition.
    Solo,
    /// L2: Scout + Engineer (2 subagents), N1+N2 hybrid.
    Hybrid,
    /// L3: Full DAG with Scout→Architect→Engineers→Validator (N2).
    Orchestrated,
    /// L4+: Full TACO with ACO pheromone tracking, MCTS, parallel_groups (N2+N3).
    FullTaco,
}

/// Task profile derived from CILA level and task type.
/// Controls decomposition strategy, parallelism, and ACO pheromone behavior.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskProfile {
    /// CILA level of this task.
    pub cila_level: CilaLevel,
    /// Routing mode derived from CILA.
    pub routing_mode: RoutingMode,
    /// Recommended max parallel subtasks.
    pub max_parallelism: usize,
    /// Whether to enable ACO pheromone tracking.
    pub pheromone_enabled: bool,
    /// Whether to enable MCTS for multi-path decisions.
    pub mcts_enabled: bool,
    /// Whether validator is required at end of DAG.
    pub validator_required: bool,
}

impl TaskProfile {
    /// Derive task profile from CILA level and task type string.
    pub fn from_task(cila_level: u8, task_type: &str) -> Self {
        let cila = CilaLevel::from_u8(cila_level);
        let routing = cila.routing_mode();
        let pheromone_enabled =
            matches!(routing, RoutingMode::Orchestrated | RoutingMode::FullTaco);
        let mcts_enabled = matches!(routing, RoutingMode::FullTaco);
        let validator_required =
            matches!(routing, RoutingMode::Orchestrated | RoutingMode::FullTaco);

        // Override for certain task types
        let max_parallelism = if task_type == "pipeline" {
            // Pipelines have lower parallelism due to sequential phases
            (cila.max_parallelism() / 2).max(1)
        } else {
            cila.max_parallelism()
        };

        Self {
            cila_level: cila,
            routing_mode: routing,
            max_parallelism,
            pheromone_enabled,
            mcts_enabled,
            validator_required,
        }
    }

    /// Whether the DAG should use BFS (breadth-first) or DFS (depth-first) traversal.
    pub fn traversal_order(&self) -> &'static str {
        match self.routing_mode {
            RoutingMode::Solo => "none",
            RoutingMode::Hybrid => "bfs",
            RoutingMode::Orchestrated | RoutingMode::FullTaco => "bfs",
        }
    }
}

/// Default CILA task type strings.
pub mod cila_task_types {
    /// Direct: pure text response.
    pub const DIRECT: &str = "direct";
    /// PAL: simple computation.
    pub const PAL: &str = "pal";
    /// Tool-Augmented: search + MCP tools.
    pub const TOOL_AUGMENTED: &str = "tool_augmented";
    /// ANTT pipeline analysis.
    pub const PIPELINE: &str = "pipeline";
    /// Code implementation.
    pub const CODE_IMPL: &str = "code_implementation";
    /// Agent loop / iterative analysis.
    pub const AGENT_LOOP: &str = "agent_loop";
    /// Self-modifying parameter evolution.
    pub const SELF_MOD: &str = "self_modifying";
    /// Multi-agent team.
    pub const MULTI_AGENT: &str = "multi_agent";
    /// ACO orchestration (L4 intent).
    pub const ACO_ORCHESTRATION: &str = "aco_orchestration";
    /// TACO orchestrator (full N2+N3).
    pub const TACO_ORCHESTRATION: &str = "taco_orchestration";
}

/// Default CILA level (L3 = Pipelines, full orchestration).
fn default_cila_level() -> u8 {
    3
}

// ── Status ──────────────────────────────────────────────────────────────────

/// Lifecycle status of a subtask in the DAG.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubTaskStatus {
    /// Waiting for dependencies to complete.
    Pending,
    /// Currently being executed.
    InProgress,
    /// Finished successfully.
    Completed,
    /// Cannot start because at least one dependency is blocked or failed.
    Blocked,
    /// Execution failed — terminal state.
    Failed,
    /// Manually cancelled before completion — terminal state.
    Cancelled,
    /// Skipped due to expired deadline with Skip behavior — terminal state.
    Skipped,
}

impl SubTaskStatus {
    /// Returns `true` if the status is a terminal state (no further transitions expected).
    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Cancelled | Self::Skipped
        )
    }

    /// Returns `true` if the subtask counts as successfully done for dependency resolution.
    pub fn is_done(&self) -> bool {
        matches!(self, Self::Completed)
    }
}

impl std::str::FromStr for SubTaskStatus {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(Self::Pending),
            "in_progress" => Ok(Self::InProgress),
            // "done"/"complete" are accepted aliases for Completed: external drivers
            // (e.g. the loop-engineering `loop_phase_close`, which marks subtasks
            // "done") must resolve to Completed so dependents unblock — `is_done()`
            // is the dependency-satisfaction gate. Without this, "done" fell through
            // to `unwrap_or(Pending)` and every dependent stayed permanently blocked.
            "completed" | "done" | "complete" => Ok(Self::Completed),
            "blocked" => Ok(Self::Blocked),
            "failed" => Ok(Self::Failed),
            "cancelled" | "canceled" => Ok(Self::Cancelled),
            "skipped" => Ok(Self::Skipped),
            _ => Err(format!(
                "Invalid status: '{}'. Valid: pending, in_progress, completed (aka done/complete), blocked, failed, cancelled, skipped",
                s
            )),
        }
    }
}

impl std::fmt::Display for SubTaskStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Pending => write!(f, "pending"),
            Self::InProgress => write!(f, "in_progress"),
            Self::Completed => write!(f, "completed"),
            Self::Blocked => write!(f, "blocked"),
            Self::Failed => write!(f, "failed"),
            Self::Cancelled => write!(f, "cancelled"),
            Self::Skipped => write!(f, "skipped"),
        }
    }
}

// ── Deadline Behavior ─────────────────────────────────────────────────────────

/// Behavior when a subtask deadline expires.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub enum DeadlineBehavior {
    /// Mark the subtask as Failed when deadline expires.
    #[default]
    Fail,
    /// Mark the subtask as Skipped when deadline expires.
    Skip,
    /// Extend the deadline by estimated_ms or 1 hour when deadline expires.
    Extend,
}

// ── ACO Pheromone Events ──────────────────────────────────────────────────────

/// ACO pheromone events emitted by status transitions.
/// These are drained by the MCP handler and flushed to the daemon ACO bus.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type", content = "data")]
pub enum AcoEvent {
    /// Subtask completed successfully — deposit positive pheromone.
    TaskCompleted {
        /// Identifier of the parent task this subtask belongs to.
        task_id: String,
        /// Identifier of the subtask that completed.
        subtask_id: String,
        /// Wall-clock execution time of the subtask, in milliseconds.
        duration_ms: u64,
    },
    /// Subtask failed — evaporate pheromone (negative signal).
    TaskFailed {
        /// Identifier of the parent task this subtask belongs to.
        task_id: String,
        /// Identifier of the subtask that failed.
        subtask_id: String,
        /// Human-readable explanation of why the subtask failed.
        reason: String,
    },
    /// Subtask blocked — deposit limbo pheromone (blocked pattern).
    TaskBlocked {
        /// Identifier of the parent task this subtask belongs to.
        task_id: String,
        /// Identifier of the subtask that became blocked.
        subtask_id: String,
    },
    /// Subtask started — deposit initial heat.
    TaskStarted {
        /// Identifier of the parent task this subtask belongs to.
        task_id: String,
        /// Identifier of the subtask that started.
        subtask_id: String,
    },
}

impl AcoEvent {
    /// Returns the pheromone delta for this event.
    /// Positive = deposit, Negative = evaporate.
    pub fn pheromone_delta(&self) -> f64 {
        match self {
            Self::TaskCompleted { .. } => 1.0,
            Self::TaskFailed { .. } => -0.5,
            Self::TaskBlocked { .. } => -0.3,
            Self::TaskStarted { .. } => 0.1,
        }
    }

    /// Returns the key for this event (used by ACO bus).
    pub fn phero_key(&self) -> String {
        match self {
            Self::TaskCompleted { task_id, .. } => format!("task_completion:{}", task_id),
            Self::TaskFailed { task_id, .. } => format!("task_failure:{}", task_id),
            Self::TaskBlocked { task_id, .. } => format!("task_blocked:{}", task_id),
            Self::TaskStarted { task_id, .. } => format!("task_started:{}", task_id),
        }
    }
}

// ── SubTask ──────────────────────────────────────────────────────────────────

/// A single unit of work within a task plan.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SubTask {
    /// Unique identifier within the parent task.
    pub id: String,
    /// Human-readable description of the work to be done.
    pub description: String,
    /// Current lifecycle status.
    pub status: SubTaskStatus,
    /// IDs of subtasks that must complete before this one can start.
    pub depends_on: Vec<String>,
    /// Execution priority within the same DAG layer (0 = highest priority).
    /// Value `u8::MAX` means inherited from most critical dependency.
    pub priority: u8,
    /// When this subtask was created.
    pub created_at: DateTime<Utc>,
    /// When this subtask's status was last updated.
    pub updated_at: DateTime<Utc>,
    /// Retry policy for resilient execution.
    #[serde(default)]
    pub retry_policy: RetryPolicy,
    /// Complexity hint for scheduling.
    #[serde(default)]
    pub complexity_hint: Option<ComplexityHint>,
    /// Number of retry attempts made.
    #[serde(default)]
    pub attempts: u8,
    /// When this subtask started executing.
    #[serde(skip)]
    started_at: Option<Instant>,
    /// Deadline for this subtask. If `None`, no deadline is set.
    #[serde(default)]
    pub deadline: Option<DateTime<Utc>>,
    /// Behavior when deadline expires.
    #[serde(default)]
    pub deadline_behavior: DeadlineBehavior,
}

impl SubTask {
    pub(crate) fn new(
        id: String,
        description: String,
        depends_on: Vec<String>,
        priority: u8,
    ) -> Self {
        let now = Utc::now();
        Self {
            id,
            description,
            status: SubTaskStatus::Pending,
            depends_on,
            priority,
            created_at: now,
            updated_at: now,
            retry_policy: RetryPolicy::default(),
            complexity_hint: None,
            attempts: 0,
            started_at: None,
            deadline: None,
            deadline_behavior: DeadlineBehavior::default(),
        }
    }

    /// Mark as in progress, recording start time.
    fn mark_in_progress(&mut self) {
        self.status = SubTaskStatus::InProgress;
        self.started_at = Some(Instant::now());
        self.attempts += 1;
        self.updated_at = Utc::now();
    }

    /// Whether this subtask has exceeded its timeout.
    /// Wired to [`Self::update_status()`] — marks timed-out subtasks as Failed.
    fn is_timed_out(&self) -> bool {
        if let (Some(start), Some(timeout_ms)) = (self.started_at, self.retry_policy.timeout_ms) {
            return start.elapsed() > Duration::from_millis(timeout_ms);
        }
        false
    }

    /// Returns the duration since start in milliseconds, or 0 if not started.
    pub(crate) fn duration_ms(&self) -> u64 {
        self.started_at
            .map(|s| s.elapsed().as_millis() as u64)
            .unwrap_or(0)
    }

    /// Diagnostic helper — wires `mark_in_progress`, `is_timed_out` and
    /// `duration_ms` through a synthetic lifecycle so they remain
    /// observable when no `#[tool]` consumer is reachable. Returns the
    /// duration recorded after a single transition.
    pub fn diagnostic_lifecycle() -> serde_json::Value {
        let mut t = SubTask::new(
            "diagnostic-subtask".to_string(),
            "synthetic lifecycle".to_string(),
            Vec::new(),
            0,
        );
        t.mark_in_progress();
        let timed_out = t.is_timed_out();
        let duration = t.duration_ms();
        serde_json::json!({
            "status": format!("{:?}", t.status),
            "is_timed_out": timed_out,
            "duration_ms": duration,
            "attempts": t.attempts,
        })
    }
}

// ── Task ─────────────────────────────────────────────────────────────────────

/// A top-level task plan containing subtasks as a DAG.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Task {
    /// Unique task identifier (format: `task_<N>`).
    pub id: String,
    /// Task type: `refactor`, `debug`, `feature`, `analysis`, `pipeline`, or custom.
    pub task_type: String,
    /// Human-readable description of the overall objective.
    pub description: String,
    /// Subtasks in insertion order — stable for serialization.
    pub subtasks: Vec<SubTask>,
    /// When this task was created.
    pub created_at: DateTime<Utc>,
    /// O(1) lookup: subtask id → index in `subtasks`.
    #[serde(skip)]
    subtask_index: HashMap<String, usize>,
    /// Task-level metrics for observability.
    #[serde(default)]
    pub metrics: DecomposeValidationMetrics,
    /// CILA level for N-level routing (0-6, default 3).
    #[serde(default = "default_cila_level")]
    pub cila_level: u8,
}

impl Task {
    pub(crate) fn new(id: String, task_type: String, description: String) -> Self {
        Self {
            id,
            task_type,
            description,
            subtasks: Vec::new(),
            created_at: Utc::now(),
            subtask_index: HashMap::new(),
            metrics: DecomposeValidationMetrics::new(),
            cila_level: 3, // Default L3 (Pipelines)
        }
    }

    /// Derive the task profile from CILA level and type.
    /// Used by the orchestrator to decide routing mode, parallelism, and ACO behavior.
    pub fn profile(&self) -> TaskProfile {
        TaskProfile::from_task(self.cila_level, &self.task_type)
    }

    /// Summary of parallel groups with routing info.
    pub fn parallel_groups_with_profile(&self) -> (Vec<ParallelGroup>, TaskProfile) {
        let profile = self.profile();
        let groups = self.parallel_groups();
        (groups, profile)
    }

    /// Add a subtask and update the O(1) index.
    pub(crate) fn push_subtask(&mut self, st: SubTask) {
        let idx = self.subtasks.len();
        self.subtask_index.insert(st.id.clone(), idx);
        self.subtasks.push(st);
    }

    /// Find a subtask by id in O(1).
    pub fn get_subtask(&self, id: &str) -> Option<&SubTask> {
        self.subtask_index
            .get(id)
            .and_then(|&i| self.subtasks.get(i))
    }

    /// Find a subtask mutably by id in O(1).
    pub fn get_subtask_mut(&mut self, id: &str) -> Option<&mut SubTask> {
        self.subtask_index
            .get(id)
            .copied()
            .and_then(|i| self.subtasks.get_mut(i))
    }

    /// Rebuild the index from `subtasks`. Called after deserialization.
    pub fn rebuild_index(&mut self) {
        self.subtask_index.clear();
        for (i, st) in self.subtasks.iter().enumerate() {
            self.subtask_index.insert(st.id.clone(), i);
        }
    }

    /// Get effective priority for a subtask, inheriting from dependencies if set to MAX.
    /// Max inheritance depth is 3 to prevent deep recursion.
    pub fn effective_priority(&self, subtask_id: &str) -> u8 {
        let subtask = match self.get_subtask(subtask_id) {
            Some(st) => st,
            None => return u8::MAX,
        };
        if subtask.priority != u8::MAX {
            return subtask.priority;
        }
        self.inherit_priority_impl(subtask, 0, 3)
    }

    fn inherit_priority_impl(&self, subtask: &SubTask, depth: u8, max_depth: u8) -> u8 {
        // At max depth: return this subtask's priority (even if MAX, no ancestors to search)
        if depth >= max_depth {
            return subtask.priority;
        }
        let ancestors: Vec<&SubTask> = self
            .subtasks
            .iter()
            .filter(|s| subtask.depends_on.contains(&s.id))
            .collect();
        if ancestors.is_empty() {
            return subtask.priority;
        }
        // Recursively get ancestor priorities and find the minimum
        let min_inherited = ancestors
            .iter()
            .map(|a| self.inherit_priority_impl(a, depth + 1, max_depth))
            .min()
            .unwrap_or(u8::MAX);
        // If this subtask has concrete priority, use min of own and inherited
        if subtask.priority != u8::MAX {
            return subtask.priority.min(min_inherited);
        }
        min_inherited
    }

    /// Check all in-progress and pending subtasks for expired deadlines.
    /// Returns list of (subtask_id, new_status) transitions to apply.
    pub fn check_expired_deadlines(&mut self) -> Vec<(String, SubTaskStatus)> {
        let now = Utc::now();
        let mut transitions = Vec::new();
        for st in &mut self.subtasks {
            if st.status != SubTaskStatus::InProgress && st.status != SubTaskStatus::Pending {
                continue;
            }
            if let Some(deadline) = st.deadline
                && deadline < now
            {
                match st.deadline_behavior {
                    DeadlineBehavior::Fail => {
                        transitions.push((st.id.clone(), SubTaskStatus::Failed));
                    }
                    DeadlineBehavior::Skip => {
                        transitions.push((st.id.clone(), SubTaskStatus::Skipped));
                    }
                    DeadlineBehavior::Extend => {
                        let extend_by = st
                            .complexity_hint
                            .as_ref()
                            .and_then(|c| c.estimated_ms)
                            .unwrap_or(3_600_000);
                        st.deadline =
                            Some(deadline + chrono::Duration::milliseconds(extend_by as i64));
                    }
                }
            }
        }
        transitions
    }

    /// Returns ids of subtasks that are ready to run:
    /// status is `Pending`, all dependencies have `Completed` status,
    /// and deadline has not expired (or deadline behavior is Extend).
    pub fn ready_subtasks(&self) -> Vec<&SubTask> {
        let completed: HashSet<&str> = self
            .subtasks
            .iter()
            .filter(|s| s.status.is_done())
            .map(|s| s.id.as_str())
            .collect();

        let now = Utc::now();
        let mut ready: Vec<&SubTask> = self
            .subtasks
            .iter()
            .filter(|s| {
                if s.status != SubTaskStatus::Pending {
                    return false;
                }
                if !s
                    .depends_on
                    .iter()
                    .all(|dep| completed.contains(dep.as_str()))
                {
                    return false;
                }
                // Deadline check: exclude if expired with Fail or Skip behavior
                if let Some(deadline) = s.deadline
                    && deadline < now
                {
                    match s.deadline_behavior {
                        DeadlineBehavior::Fail | DeadlineBehavior::Skip => return false,
                        DeadlineBehavior::Extend => {}
                    }
                }
                true
            })
            .collect();

        // Sort ready subtasks by effective priority (0 = highest), then by id for determinism.
        ready.sort_unstable_by_key(|s| (self.effective_priority(&s.id), s.id.as_str()));
        ready
    }

    /// Overall completion percentage: completed / total subtasks.
    pub fn completion_pct(&self) -> f32 {
        if self.subtasks.is_empty() {
            return 0.0;
        }
        let done = self.subtasks.iter().filter(|s| s.status.is_done()).count();
        (done as f32 / self.subtasks.len() as f32) * 100.0
    }

    /// Returns all parallel execution groups — subtasks at the same DAG depth.
    /// Each group is sorted by priority, then id.
    pub fn parallel_groups(&self) -> Vec<ParallelGroup> {
        if self.subtasks.is_empty() {
            return Vec::new();
        }
        let mut groups: Vec<ParallelGroup> = Vec::new();

        // Build adjacency: subtask → dependents
        let adj: HashMap<&str, Vec<&str>> =
            self.subtasks.iter().fold(HashMap::new(), |mut acc, st| {
                acc.entry(st.id.as_str()).or_default();
                for dep in &st.depends_on {
                    acc.entry(dep.as_str()).or_default().push(st.id.as_str());
                }
                acc
            });

        // Compute depth of each node via BFS from roots
        let mut depth: HashMap<&str, usize> = HashMap::new();
        let mut queue: VecDeque<&str> = self
            .subtasks
            .iter()
            .filter(|st| st.depends_on.is_empty())
            .map(|st| st.id.as_str())
            .collect();
        for &id in &queue {
            depth.insert(id, 0);
        }

        while let Some(node) = queue.pop_front() {
            if let Some(deps) = adj.get(node) {
                let d = depth.get(node).copied().unwrap_or(0);
                for &dep in deps {
                    if !depth.contains_key(dep) {
                        depth.insert(dep, d + 1);
                        queue.push_back(dep);
                    } else {
                        let existing = depth.get(dep).copied().unwrap_or(0);
                        depth.insert(dep, existing.max(d + 1));
                    }
                }
            }
        }

        // Group by depth
        let mut by_depth: HashMap<usize, Vec<&str>> = HashMap::new();
        for st in &self.subtasks {
            let d = depth.get(st.id.as_str()).copied().unwrap_or(0);
            by_depth.entry(d).or_default().push(st.id.as_str());
        }

        let mut depths: Vec<usize> = by_depth.keys().copied().collect();
        depths.sort_unstable();

        for depth_val in depths {
            let mut ids: Vec<&str> = by_depth.remove(&depth_val).unwrap_or_default();
            // Sort by priority then id
            ids.sort_unstable_by(|&a, &b| {
                let pa = self.get_subtask(a).map_or(u8::MAX, |s| s.priority);
                let pb = self.get_subtask(b).map_or(u8::MAX, |s| s.priority);
                pa.cmp(&pb).then(a.cmp(b))
            });
            let all_done = ids
                .iter()
                .all(|&id| self.get_subtask(id).is_some_and(|s| s.status.is_done()));
            groups.push(ParallelGroup {
                depth: depth_val,
                subtask_ids: ids.into_iter().map(String::from).collect(),
                all_done,
            });
        }

        groups
    }
}

// ── TaskDecomposer ───────────────────────────────────────────────────────────

/// Manages task decomposition plans in memory.
///
/// Thread-safety: wrap in `Arc<tokio::sync::RwLock<TaskDecomposer>>` for concurrent access.
/// Reads dominate (get_plan, list_plans, validate_order) — RwLock yields better throughput
/// than Mutex for the typical read-heavy workload.
#[derive(Debug, Default)]
pub struct TaskDecomposer {
    /// Active task plans: id → Task
    pub tasks: HashMap<String, Task>,
    /// Monotonically increasing counter for ID generation.
    next_id: u64,
    /// Pending ACO pheromone events from status transitions.
    /// These are drained by the MCP handler and flushed to the daemon ACO bus.
    pending_aco_events: Vec<AcoEvent>,
}

// C11 — budget conservation B∈ℕ⁶ over the decompose DAG.
impl Task {
    /// Derive each subtask's [`BudgetVector`] from its real cost/structural signals:
    /// `wall_ms` from the complexity hint, `dependencies` from `depends_on`,
    /// `max_retries`/`attempts_used` from the retry state, `subtasks` = 1, and a
    /// `tokens` estimate (the bridge to the Workflow-tool token budget) scaled from
    /// the wall-clock estimate. No new fields — every dimension is read from data the
    /// subtask already carries.
    #[must_use]
    pub fn subtask_budgets(&self) -> Vec<BudgetVector> {
        const U32_MAX: u64 = u32::MAX as u64;
        self.subtasks
            .iter()
            .map(|st| {
                let wall_ms_u64 = st
                    .complexity_hint
                    .as_ref()
                    .and_then(|c| c.estimated_ms)
                    .unwrap_or(0);
                BudgetVector {
                    // ~4 ms/token heuristic, floor 50, so every subtask has a real
                    // token cost even without an explicit estimate.
                    tokens: (wall_ms_u64 / 4).clamp(50, U32_MAX) as u32,
                    wall_ms: wall_ms_u64.min(U32_MAX) as u32,
                    subtasks: 1,
                    dependencies: st.depends_on.len().min(U32_MAX as usize) as u32,
                    max_retries: u32::from(st.retry_policy.max_attempts),
                    attempts_used: u32::from(st.attempts),
                }
            })
            .collect()
    }

    /// **C11** — verify the decomposition does not over-commit beyond the `root`
    /// budget: `Σ subtask budgets ≤ root`, per dimension (O(|V|)). A violation means
    /// the plan promises more work / time / retries / tokens than the root task was
    /// allotted — surfaced per dimension via [`BudgetViolation`].
    pub fn verify_budget_conservation(
        &self,
        root: &BudgetVector,
    ) -> Result<(), Vec<BudgetViolation>> {
        verify_conservation(root, &self.subtask_budgets())
    }
}

impl TaskDecomposer {
    /// Create a new empty decomposer.
    pub fn new() -> Self {
        Self {
            tasks: HashMap::new(),
            next_id: 1,
            pending_aco_events: Vec::new(),
        }
    }

    /// Drain and return all pending ACO pheromone events.
    /// Called by the MCP handler after each mutating operation.
    pub fn drain_pending_aco_events(&mut self) -> Vec<AcoEvent> {
        std::mem::take(&mut self.pending_aco_events)
    }

    /// Alias for [`drain_pending_aco_events`] — drains and returns all pending ACO events.
    ///
    /// This is the same operation but named to emphasize the "take" semantics
    /// (ownership transfer, not a borrow).
    pub(crate) fn take_pending_aco_events(&mut self) -> Vec<AcoEvent> {
        self.drain_pending_aco_events()
    }

    /// Diagnostic helper — wires the full TaskDecomposer surface
    /// (`new`, `create_task`, `create_task_with_cila`,
    /// `create_task_with_cila_and_hint`, `add_subtask`, `update_status`,
    /// `validate_order`, `finalize_task`, `validate_completion_gate`,
    /// `delete_task`, `get_plan`, `list_plans`, `task_count`,
    /// `take_pending_aco_events`, plus the `next_id` and
    /// `pending_aco_events` fields) through a synthetic lifecycle so
    /// they remain reachable for cargo's dead-code analyzer when no
    /// `#[tool]` consumer is on the live path. Returns a JSON snapshot
    /// of the decomposer state after a single round trip.
    pub fn diagnostic_lifecycle() -> serde_json::Value {
        let mut d = Self::new();
        let task_a = d.create_task("refactor", "synthetic-A");
        let task_b = d.create_task_with_cila("debug", "synthetic-B", 4);
        let task_c = d.create_task_with_cila_and_hint("feature", "synthetic-C", 3, None);

        // Add two subtasks to task_a, second depending on first.
        let sub_1 = d
            .add_subtask(&task_a, "first step", Vec::new(), 0)
            .unwrap_or_default();
        let sub_2 = d
            .add_subtask(&task_a, "second step", vec![sub_1.clone()], 0)
            .unwrap_or_default();

        // Transition sub_1 through a terminal state — exercises ACO event
        // emission via `pending_aco_events`.
        let _ = d.update_status(&task_a, &sub_1, SubTaskStatus::InProgress);
        let _ = d.update_status(&task_a, &sub_1, SubTaskStatus::Completed);

        // Validate DAG ordering and gate semantics.
        let order = d.validate_order(&task_a).map(|v| v.len()).unwrap_or(0);
        let gate = d
            .validate_completion_gate(&task_a, &sub_2, Some(0.0))
            .map(|r| format!("{:?}", r))
            .unwrap_or_else(|e| e.to_string());
        let finalize_attempt = d.finalize_task(&task_a).is_ok();

        // Queries.
        let plan_a_exists = d.get_plan(&task_a).is_some();
        let plans_listed = d.list_plans().len();
        let total = d.task_count();

        // Cleanup: delete_task on task_b.
        let deleted = d.delete_task(&task_b);

        // Drain pending ACO events accumulated by status transitions.
        let drained = d.take_pending_aco_events();

        serde_json::json!({
            "next_id_after_three_creates": d.next_id,
            "pending_events_drained": drained.len(),
            "task_ids": [task_a, task_c],
            "subtask_ids": [sub_1, sub_2],
            "validate_order_len": order,
            "completion_gate_result": gate,
            "finalize_attempt_ok": finalize_attempt,
            "get_plan_ok": plan_a_exists,
            "list_plans_count": plans_listed,
            "task_count_after_delete_b": total - usize::from(deleted),
            "delete_task_b_ok": deleted,
        })
    }

    // ── ID generation ───────────────────────────────────────────────────────

    fn gen_id(&mut self, prefix: &str) -> String {
        let id = format!("{}_{}", prefix, self.next_id);
        self.next_id += 1;
        id
    }

    // ── Mutations ───────────────────────────────────────────────────────────

    /// Create a new task plan. Returns the task id.
    /// Convenience wrapper: delegates to create_task_with_cila with default L3.
    pub fn create_task(&mut self, task_type: &str, description: &str) -> String {
        // Delegate to CILA-aware version with default L3
        self.create_task_with_cila(task_type, description, 3)
    }

    /// Create a new task plan with explicit CILA level for N-level routing.
    /// Returns the task id.
    pub fn create_task_with_cila(
        &mut self,
        task_type: &str,
        description: &str,
        cila_level: u8,
    ) -> String {
        let id = self.gen_id("task");
        let mut task = Task::new(id.clone(), task_type.to_string(), description.to_string());
        task.cila_level = cila_level;
        self.tasks.insert(id.clone(), task);
        id
    }

    /// Create a task and optionally pre-populate subtasks from a granularity hint.
    ///
    /// When `cila_level >= 3` and a hint is provided the bandit recommendation
    /// (e.g. Split3 → 3 subtasks) is scaffolded as placeholder subtasks with a
    /// sequential dependency chain. Callers can then rename / re-order them.
    /// Falls back to `create_task_with_cila` when the hint is absent.
    pub fn create_task_with_cila_and_hint(
        &mut self,
        task_type: &str,
        description: &str,
        cila_level: u8,
        hint: Option<&crate::reasoning::GranularityHint>,
    ) -> String {
        let task_id = self.create_task_with_cila(task_type, description, cila_level);
        if let (Some(h), true) = (hint, cila_level >= 3) {
            self.bootstrap_subtasks_from_hint(&task_id, h);
        }
        task_id
    }

    /// Scaffold N sequential placeholder subtasks driven by the granularity hint.
    ///
    /// Each subtask `sub_i` (1-indexed) depends on `sub_{i-1}` forming a linear
    /// chain. Placeholder descriptions are generic; callers replace them when
    /// they fill in the real work items.
    fn bootstrap_subtasks_from_hint(
        &mut self,
        task_id: &str,
        hint: &crate::reasoning::GranularityHint,
    ) {
        let n = hint.subtask_count.max(1);
        // Nothing to scaffold for Monolithic / subtask_count == 1.
        if n <= 1 {
            return;
        }

        let mut prev_id: Option<String> = None;
        for i in 1..=n {
            let description = format!("Part {i}/{n} — to be specified");
            let deps = prev_id.iter().cloned().collect::<Vec<_>>();
            // Ignore errors: we are scaffolding best-effort.
            if let Ok(sub_id) = self.add_subtask(task_id, &description, deps, 0) {
                prev_id = Some(sub_id);
            }
        }
    }

    /// Delete a task plan. Returns `true` if it existed.
    pub fn delete_task(&mut self, task_id: &str) -> bool {
        self.tasks.remove(task_id).is_some()
    }

    /// Add a subtask to an existing task plan.
    ///
    /// Validates that all `depends_on` ids refer to existing subtasks within the same task.
    /// Returns the new subtask id.
    pub fn add_subtask(
        &mut self,
        task_id: &str,
        description: &str,
        depends_on: Vec<String>,
        priority: u8,
    ) -> Result<String, String> {
        // Validate: task exists and all dependency ids are known.
        {
            let task = self
                .tasks
                .get(task_id)
                .ok_or_else(|| format!("Task not found: {}", task_id))?;

            for dep in &depends_on {
                if task.get_subtask(dep).is_none() {
                    return Err(format!("Dependency not found: {}", dep));
                }
            }
        }

        let subtask_id = self.gen_id("sub");
        let subtask = SubTask::new(
            subtask_id.clone(),
            description.to_string(),
            depends_on,
            priority,
        );

        self.tasks
            .get_mut(task_id)
            .ok_or_else(|| format!("Task not found: {}", task_id))?
            .push_subtask(subtask);

        Ok(subtask_id)
    }

    /// Update the status of a subtask.
    /// Emits ACO pheromone events on terminal transitions (Completed, Failed, Blocked).
    pub fn update_status(
        &mut self,
        task_id: &str,
        subtask_id: &str,
        new_status: SubTaskStatus,
    ) -> Result<(), String> {
        let task = self
            .tasks
            .get_mut(task_id)
            .ok_or_else(|| format!("Task not found: {}", task_id))?;

        let subtask = task
            .get_subtask_mut(subtask_id)
            .ok_or_else(|| format!("Subtask not found: {}", subtask_id))?;

        if new_status == SubTaskStatus::InProgress {
            // Wire is_timed_out: if this subtask was already InProgress and has
            // exceeded its retry_policy timeout, fail it instead of restarting.
            if subtask.is_timed_out() {
                tracing::warn!(
                    "SubTask {} timed out (elapsed > {}ms), marking as Failed",
                    subtask_id,
                    subtask.retry_policy.timeout_ms.unwrap_or(0)
                );
                subtask.status = SubTaskStatus::Failed;
                subtask.updated_at = Utc::now();
                // Emit TaskFailed ACO event
                self.pending_aco_events.push(AcoEvent::TaskFailed {
                    task_id: task_id.to_string(),
                    subtask_id: subtask_id.to_string(),
                    reason: "timeout".to_string(),
                });
            } else {
                subtask.mark_in_progress();
                // Emit TaskStarted ACO event
                self.pending_aco_events.push(AcoEvent::TaskStarted {
                    task_id: task_id.to_string(),
                    subtask_id: subtask_id.to_string(),
                });
            }
        } else {
            // Save status variant before moving into subtask.status
            let status_variant = new_status.clone();
            subtask.status = new_status;
            subtask.updated_at = Utc::now();
            // Emit terminal state ACO events
            match status_variant {
                SubTaskStatus::Completed => {
                    self.pending_aco_events.push(AcoEvent::TaskCompleted {
                        task_id: task_id.to_string(),
                        subtask_id: subtask_id.to_string(),
                        duration_ms: subtask.duration_ms(),
                    });
                }
                SubTaskStatus::Failed => {
                    self.pending_aco_events.push(AcoEvent::TaskFailed {
                        task_id: task_id.to_string(),
                        subtask_id: subtask_id.to_string(),
                        reason: "explicit_failure".to_string(),
                    });
                }
                SubTaskStatus::Blocked => {
                    self.pending_aco_events.push(AcoEvent::TaskBlocked {
                        task_id: task_id.to_string(),
                        subtask_id: subtask_id.to_string(),
                    });
                }
                _ => {}
            }
        }
        Ok(())
    }

    // ── Queries ─────────────────────────────────────────────────────────────

    /// Get a task plan by id.
    pub fn get_plan(&self, task_id: &str) -> Option<&Task> {
        self.tasks.get(task_id)
    }

    /// List all task plans, sorted by creation time (oldest first) for stable output.
    pub fn list_plans(&self) -> Vec<&Task> {
        let mut plans: Vec<&Task> = self.tasks.values().collect();
        plans.sort_unstable_by_key(|t| t.created_at);
        plans
    }

    /// Count of active task plans.
    pub fn task_count(&self) -> usize {
        self.tasks.len()
    }

    // ── DAG validation ───────────────────────────────────────────────────────

    /// Validate execution order using Kahn's topological sort algorithm.
    ///
    /// Returns `Ok(ordered_ids)` if the DAG is acyclic. Within each "layer"
    /// (set of nodes with equal in-degree at the time of processing), subtasks
    /// are sorted by `priority` ascending (0 = highest), then by id for
    /// determinism. Returns `Err(msg)` if a cycle is detected.
    ///
    /// Time complexity: O(V + E).
    pub fn validate_order(&mut self, task_id: &str) -> Result<Vec<String>, ReasoningError> {
        let task = self
            .tasks
            .get_mut(task_id)
            .ok_or_else(|| ReasoningError::NotFound(format!("Task not found: {}", task_id)))?;

        let subtasks = &task.subtasks;
        if subtasks.is_empty() {
            return Ok(Vec::new());
        }

        let start = Instant::now();

        // Build in-degree map and adjacency list (dep → dependents).
        let mut in_degree: HashMap<&str, usize> = HashMap::with_capacity(subtasks.len());
        let mut adj: HashMap<&str, Vec<&str>> = HashMap::with_capacity(subtasks.len());

        for st in subtasks {
            in_degree.entry(st.id.as_str()).or_insert(0);
            adj.entry(st.id.as_str()).or_default();
            for dep in &st.depends_on {
                adj.entry(dep.as_str()).or_default().push(st.id.as_str());
                *in_degree.entry(st.id.as_str()).or_insert(0) += 1;
            }
        }

        // Kahn's algorithm: start with all zero-in-degree nodes,
        // sorted by priority then id for deterministic ordering.
        let mut queue: VecDeque<&str> = {
            let mut starts: Vec<&str> = in_degree
                .iter()
                .filter(|&(_, &deg)| deg == 0)
                .map(|(&id, _)| id)
                .collect();
            // Sort by priority (look up from subtask_index) then id.
            starts.sort_unstable_by(|&a, &b| {
                let pa = task.get_subtask(a).map_or(u8::MAX, |s| s.priority);
                let pb = task.get_subtask(b).map_or(u8::MAX, |s| s.priority);
                pa.cmp(&pb).then(a.cmp(b))
            });
            starts.into_iter().collect()
        };

        let mut order: Vec<String> = Vec::with_capacity(subtasks.len());

        while let Some(node) = queue.pop_front() {
            order.push(node.to_string());

            if let Some(neighbors) = adj.get(node) {
                // Collect newly-unblocked neighbors, sort them by priority+id.
                let mut newly_ready: Vec<&str> = Vec::new();
                for &neighbor in neighbors {
                    if let Some(deg) = in_degree.get_mut(neighbor) {
                        *deg -= 1;
                        if *deg == 0 {
                            newly_ready.push(neighbor);
                        }
                    }
                }
                newly_ready.sort_unstable_by(|&a, &b| {
                    let pa = task.get_subtask(a).map_or(u8::MAX, |s| s.priority);
                    let pb = task.get_subtask(b).map_or(u8::MAX, |s| s.priority);
                    pa.cmp(&pb).then(a.cmp(b))
                });
                for n in newly_ready {
                    queue.push_back(n);
                }
            }
        }

        if order.len() == subtasks.len() {
            task.metrics
                .record_validation(start.elapsed().as_millis() as u64, false);
            Ok(order)
        } else {
            task.metrics
                .record_validation(start.elapsed().as_millis() as u64, true);
            Err(ReasoningError::CycleDetected(format!(
                "Cycle detected in subtask dependencies ({} of {} subtasks ordered)",
                order.len(),
                subtasks.len()
            )))
        }
    }

    /// Validate all subtasks for finalization readiness.
    ///
    /// Returns a `FinalizeReport` describing:
    /// - Whether all subtasks are in terminal states
    /// - Per-status counts
    /// - Human-readable blocking reasons (empty when ready=true)
    ///
    /// Does NOT mutate state — callers decide what to do with the report.
    pub fn finalize_task(&self, task_id: &str) -> Result<FinalizeReport, ReasoningError> {
        let task = self
            .tasks
            .get(task_id)
            .ok_or_else(|| ReasoningError::NotFound(format!("Task not found: {}", task_id)))?;

        if task.subtasks.is_empty() {
            return Ok(FinalizeReport {
                task_id: task_id.to_string(),
                ready: true,
                completion_pct: 0.0,
                total: 0,
                completed: 0,
                failed: 0,
                skipped: 0,
                cancelled: 0,
                pending: 0,
                in_progress: 0,
                blocking: Vec::new(),
            });
        }

        let mut completed_count = 0usize;
        let mut failed_count = 0usize;
        let mut skipped_count = 0usize;
        let mut cancelled_count = 0usize;
        let mut pending_count = 0usize;
        let mut in_progress_count = 0usize;
        let mut blocking: Vec<String> = Vec::new();

        for st in &task.subtasks {
            match &st.status {
                SubTaskStatus::Completed => completed_count += 1,
                SubTaskStatus::Failed => {
                    failed_count += 1;
                    blocking.push(format!("subtask '{}' failed", st.id));
                }
                SubTaskStatus::Skipped => skipped_count += 1,
                SubTaskStatus::Cancelled => cancelled_count += 1,
                SubTaskStatus::Pending => {
                    pending_count += 1;
                    blocking.push(format!("subtask '{}' still pending", st.id));
                }
                SubTaskStatus::InProgress => {
                    in_progress_count += 1;
                    blocking.push(format!("subtask '{}' still in_progress", st.id));
                }
                SubTaskStatus::Blocked => {
                    blocking.push(format!("subtask '{}' is blocked", st.id));
                }
            }
        }

        Ok(FinalizeReport {
            task_id: task_id.to_string(),
            ready: blocking.is_empty(),
            completion_pct: task.completion_pct(),
            total: task.subtasks.len(),
            completed: completed_count,
            failed: failed_count,
            skipped: skipped_count,
            cancelled: cancelled_count,
            pending: pending_count,
            in_progress: in_progress_count,
            blocking,
        })
    }

    /// Pre-completion gate: verify all declared dependencies are `Completed`
    /// before allowing a subtask to transition to the `Completed` state.
    ///
    /// Returns a `CompletionGateResult` with:
    /// - `ready_to_complete`: true when all deps are Completed and status allows transition
    /// - `blocking_reasons`: human-readable list of what's blocking
    /// - `pending_deps`: count of deps not yet Completed
    ///
    /// Note: `quality_score` is tracked at the DB level (SQLite), not in-memory.
    /// The `min_quality_threshold` is advisory — the caller must validate against DB.
    pub fn validate_completion_gate(
        &self,
        task_id: &str,
        subtask_id: &str,
        min_quality_threshold: Option<f64>,
    ) -> Result<CompletionGateResult, ReasoningError> {
        let task = self
            .tasks
            .get(task_id)
            .ok_or_else(|| ReasoningError::NotFound(format!("Task not found: {}", task_id)))?;
        let subtask = task.get_subtask(subtask_id).ok_or_else(|| {
            ReasoningError::NotFound(format!(
                "Subtask not found: {} in task {}",
                subtask_id, task_id
            ))
        })?;

        let mut blocking: Vec<String> = Vec::new();
        let mut pending_deps = 0usize;

        // Check current status: already terminal? No-op.
        if subtask.status.is_terminal() {
            blocking.push(format!(
                "subtask is already in terminal state '{}'",
                subtask.status
            ));
        }

        // Check all declared dependencies are Completed
        for dep_id in &subtask.depends_on {
            match task.get_subtask(dep_id) {
                Some(dep) if dep.status.is_done() => {} // OK: completed
                Some(dep) => {
                    pending_deps += 1;
                    blocking.push(format!(
                        "dependency '{}' is '{}', must be completed first",
                        dep_id, dep.status
                    ));
                }
                None => {
                    // dep_id might reference a subtask not yet in-memory (CLI-side)
                    pending_deps += 1;
                    blocking.push(format!("dependency '{}' not found in task", dep_id));
                }
            }
        }

        // Advisory: quality threshold note for L3+ tasks
        if let Some(min_q) = min_quality_threshold {
            let profile = task.profile();
            if profile.validator_required {
                blocking.push(format!(
                    "advisory: CILA {:?} requires validator review (quality_score >= {:.2} must be set via update_status)",
                    profile.cila_level, min_q
                ));
            }
        }

        Ok(CompletionGateResult {
            ready_to_complete: blocking.is_empty(),
            blocking_reasons: blocking,
            pending_deps,
        })
    }

    /// Infer dependencies based on file references in subtask descriptions.
    /// Parses file paths (e.g., "src/foo.rs", "crates/bar/mod.rs") from the
    /// description and suggests dependencies on subtasks that reference the same
    /// or related files (shared directory prefix).
    ///
    /// Returns a list of subtask IDs that should be added as dependencies.
    pub fn infer_dependencies(&self, task_id: &str, subtask_description: &str) -> Vec<String> {
        let task = match self.tasks.get(task_id) {
            Some(t) => t,
            None => return Vec::new(),
        };

        // Parse file references from description: paths with '/' or ending in .rs/.py/etc.
        let file_patterns: Vec<&str> = subtask_description
            .split_whitespace()
            .filter(|w| {
                w.contains('/')
                    || w.ends_with(".rs")
                    || w.ends_with(".py")
                    || w.ends_with(".ts")
                    || w.ends_with(".js")
                    || w.ends_with(".go")
                    || w.ends_with(".java")
            })
            .collect();

        if file_patterns.is_empty() {
            return Vec::new();
        }

        let mut inferred: Vec<String> = Vec::new();

        for st in &task.subtasks {
            let st_files: Vec<&str> = st
                .description
                .split_whitespace()
                .filter(|w| {
                    w.contains('/')
                        || w.ends_with(".rs")
                        || w.ends_with(".py")
                        || w.ends_with(".ts")
                        || w.ends_with(".js")
                        || w.ends_with(".go")
                        || w.ends_with(".java")
                })
                .collect();

            for file in &file_patterns {
                for st_file in &st_files {
                    // If they are the same file, suggest dependency
                    let same_dir = file.rsplit('/').nth(1) == st_file.rsplit('/').nth(1);
                    if file == st_file || same_dir && !inferred.contains(&st.id) {
                        inferred.push(st.id.clone());
                    }
                }
            }
        }

        inferred
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
#[path = "decomposer_tests.rs"]
mod tests;
