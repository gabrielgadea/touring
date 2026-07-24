//! Tasksfile YAML schema — types representing the Tasksfile format.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Top-level Tasksfile root.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TasksfileRoot {
    /// Tasksfile schema version string.
    pub version: String,
    /// File-level metadata (name, description, shell, etc.).
    #[serde(default)]
    pub metadata: Metadata,
    /// Reusable templates keyed by template name.
    #[serde(default)]
    pub templates: HashMap<String, TemplateDefinition>,
    /// Task definitions keyed by task name.
    pub tasks: HashMap<String, TaskDefinition>,
    /// External files or URLs included into this Tasksfile.
    #[serde(default)]
    pub includes: Vec<IncludeSpec>,
    /// Global hooks run around the whole task run.
    #[serde(default)]
    pub hooks: GlobalHooks,
}

/// File-level metadata.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Metadata {
    /// Display name of the Tasksfile.
    pub name: Option<String>,
    /// Human-readable description of the Tasksfile.
    pub description: Option<String>,
    /// Default shell used to execute task commands.
    pub shell: Option<String>,
    /// Default logging verbosity level.
    pub log_level: Option<String>,
    /// Cache time-to-live in seconds for cached task results.
    pub cache_ttl: Option<u64>,
}

/// Named template (e.g., `ci_job`) applied to tasks via `template: true`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TemplateDefinition {
    /// Default timeout inherited by tasks using this template.
    pub timeout: Option<String>,
    /// Default tags inherited by tasks using this template.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Default retry policy inherited by tasks using this template.
    #[serde(default)]
    pub retry_policy: Option<RetryPolicyDef>,
}

/// Retry policy shared across tasks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RetryPolicyDef {
    /// Maximum number of attempts before giving up.
    pub max_attempts: usize,
    /// Initial backoff delay in milliseconds between attempts.
    #[serde(default = "default_backoff_ms")]
    pub backoff_ms: u64,
    /// Multiplier applied to the backoff delay after each retry.
    #[serde(default = "default_backoff_multiplier")]
    pub backoff_multiplier: f64,
}

fn default_backoff_ms() -> u64 {
    1000
}
fn default_backoff_multiplier() -> f64 {
    2.0
}

/// Individual task definition within a Tasksfile.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskDefinition {
    /// Human-readable task description.
    pub desc: Option<String>,
    /// Whether this entry is a template rather than a runnable task.
    #[serde(default)]
    pub template: bool,
    /// Shell command to execute for the task.
    pub command: Option<String>,
    /// Names of tasks this task depends on.
    #[serde(default)]
    pub deps: Vec<String>,
    /// Environment variables scoped to the task.
    #[serde(default)]
    pub env: HashMap<String, String>,
    /// Parameters accepted by the task, keyed by parameter name.
    #[serde(default)]
    pub params: HashMap<String, ParamDefinition>,
    /// Maximum execution duration before the task is aborted.
    pub timeout: Option<String>,
    /// Free-form tags used for filtering and grouping.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Estimated complexity hint used for scheduling.
    pub complexity_hint: Option<String>,
    /// Deadline by which the task should complete.
    pub deadline: Option<String>,
    /// Action taken when the deadline is missed.
    #[serde(default)]
    pub deadline_behavior: Option<String>,
    /// Whether the task requires manual review before running.
    #[serde(default)]
    pub review_required: bool,
    /// Retry behavior applied when the task fails.
    #[serde(default)]
    pub retry_policy: Option<RetryPolicyDef>,
    /// Paths to `.env` files loaded for the task.
    #[serde(default)]
    pub env_file: Vec<String>,
    /// Per-task hooks run before and after the task.
    #[serde(default)]
    pub hooks: TaskHooks,
}

/// Parameter definition with optional default and allowed options.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ParamDefinition {
    /// Whether the parameter must be supplied by the caller.
    #[serde(default)]
    pub required: bool,
    /// Default value used when the parameter is omitted.
    #[serde(default)]
    pub default: Option<serde_json::Value>,
    /// Allowed values; supplying anything outside this set is rejected.
    #[serde(default)]
    pub options: Option<Vec<String>>,
    /// Human-readable description of the parameter.
    pub description: Option<String>,
}

/// External file or URL include.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IncludeSpec {
    /// Local file path to include, if this is a file include.
    pub file: Option<String>,
    /// Remote URL to fetch, if this is a URL include.
    pub url: Option<String>,
    /// Path resolution strategy (`relative`, `root`, or `absolute`).
    #[serde(default = "default_path_resolve")]
    pub path_resolve: String,
    /// `.netrc`-style authentication used when fetching a URL include.
    #[serde(default)]
    pub auth: Option<NetrcAuth>,
}

fn default_path_resolve() -> String {
    "relative".to_string()
}

/// .netrc authentication for URL includes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetrcAuth {
    /// Host machine the credentials apply to.
    pub machine: String,
    /// Authentication scheme (e.g. `basic`); defaults to `basic`.
    #[serde(rename = "type", default = "default_auth_type")]
    pub auth_type: String,
    /// Username for authentication.
    pub username: Option<String>,
    /// Password for authentication.
    pub password: Option<String>,
}

fn default_auth_type() -> String {
    "basic".to_string()
}

/// Global hooks (before_all, after_all, on_failure).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GlobalHooks {
    /// Commands run once before any task executes.
    #[serde(default)]
    pub before_all: Vec<String>,
    /// Commands run once after all tasks complete.
    #[serde(default)]
    pub after_all: Vec<String>,
    /// Commands run when any task fails.
    #[serde(default)]
    pub on_failure: Vec<String>,
}

/// Per-task hooks (before_task, after_task, on_failure).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TaskHooks {
    /// Commands run before this task executes.
    #[serde(default)]
    pub before_task: Vec<String>,
    /// Commands run after this task executes.
    #[serde(default)]
    pub after_task: Vec<String>,
    /// Commands run when this task fails.
    #[serde(default)]
    pub on_failure: Vec<String>,
}
