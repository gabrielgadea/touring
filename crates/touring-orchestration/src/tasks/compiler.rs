//! Tasksfile compiler — converts TasksfileRoot into decompose subtasks.
//!
//! ## Compilation Strategy
//!
//! Each task in the Tasksfile becomes one decompose subtask.
//! The `deps` field maps directly to `depends_on` JSON array.
//! Templates are inherited by copying their fields into the task.
//! Parameter substitution is deferred to task execution time (Tera).

use crate::tasks::error::{Result, TasksfileError};
use crate::tasks::include::{ResolvedInclude, resolve_includes};
use crate::tasks::schema::{RetryPolicyDef, TaskDefinition, TasksfileRoot, TemplateDefinition};
#[cfg(feature = "templates")]
use crate::tasks::template_engine;
use std::collections::HashMap;
use std::path::Path;

/// Compiled task ready for decompose insertion.
#[derive(Debug)]
pub struct CompiledTask {
    /// Full scoped subtask identifier (the task name).
    pub task_id: String, // full scoped subtask_id: "{task_name}"
    /// Human-readable task description.
    pub description: String,
    /// Identifiers of tasks that must complete before this one.
    pub depends_on: Vec<String>,
    /// Scheduling priority (lower runs earlier).
    pub priority: i64,
    /// Deadline by which the task should complete, if any.
    pub deadline: Option<String>,
    /// Action taken when the deadline is missed.
    pub deadline_behavior: Option<String>,
    /// Group label marking tasks that may run in parallel.
    pub parallel_group: Option<String>,
    /// Whether the task requires manual review before running.
    pub review_required: bool,
    /// Estimated complexity hint used for scheduling.
    pub complexity_hint: Option<String>,
    /// Retry behavior applied when the task fails.
    pub retry_policy: Option<RetryPolicyDef>,
    /// Shell command to execute for the task.
    pub command: String,
    /// Environment variables scoped to the task.
    pub env: HashMap<String, String>,
    /// Paths to `.env` files loaded for the task.
    pub env_file: Vec<String>,
    /// Maximum execution duration before the task is aborted.
    pub timeout: Option<String>,
    /// Free-form tags used for filtering and grouping.
    pub tags: Vec<String>,
}

impl CompiledTask {
    /// Render the command with `{{ params.* }}` and `{{ env.* }}` substitution.
    ///
    /// Returns the rendered command string, or the original if templates are disabled.
    #[cfg(feature = "templates")]
    pub fn render(&self, resolved_params: &HashMap<String, serde_json::Value>) -> Result<String> {
        template_engine::render_command(&self.command, resolved_params, &self.env)
    }

    /// Non-template version — returns the command as-is.
    #[cfg(not(feature = "templates"))]
    pub fn render(&self, _resolved_params: &HashMap<String, serde_json::Value>) -> Result<String> {
        Ok(self.command.clone())
    }
}

/// Result of compiling a full Tasksfile.
#[derive(Debug)]
pub struct CompiledTasksfile {
    /// Name from the Tasksfile metadata, if present.
    pub metadata_name: Option<String>,
    /// Tasks compiled into decompose-ready form.
    pub tasks: Vec<CompiledTask>,
    /// Global hooks declared at the Tasksfile level.
    pub hooks: super::schema::GlobalHooks,
    /// Include specs declared in the Tasksfile.
    pub includes: Vec<super::schema::IncludeSpec>,
    /// Resolved include content — populated during compilation.
    /// Empty when `templates` feature is disabled (resolution skipped).
    pub resolved_includes: Vec<ResolvedInclude>,
}

/// Compiler that converts TasksfileRoot into decompose-ready subtasks.
pub struct TasksfileCompiler {
    default_priority: i64,
    priority_labels: HashMap<String, i64>,
}

impl TasksfileCompiler {
    /// Create a compiler with the default priority and named priority labels.
    pub fn new() -> Self {
        Self {
            default_priority: 255,
            priority_labels: HashMap::from([
                ("critical".to_string(), 1),
                ("high".to_string(), 64),
                ("normal".to_string(), 128),
                ("low".to_string(), 192),
                ("backburner".to_string(), 220),
            ]),
        }
    }

    /// Compile a TasksfileRoot into a list of CompiledTask.
    ///
    /// `base_dir` is used to resolve relative include paths.
    /// Defaults to the current directory if not provided.
    pub fn compile(&self, root: &TasksfileRoot) -> Result<CompiledTasksfile> {
        self.compile_with_base_dir(root, std::path::Path::new("."))
    }

    /// Compile with explicit base directory for include resolution.
    pub fn compile_with_base_dir(
        &self,
        root: &TasksfileRoot,
        base_dir: &Path,
    ) -> Result<CompiledTasksfile> {
        let mut tasks = Vec::new();

        for (name, task_def) in &root.tasks {
            let compiled = self.compile_task(name, task_def, root)?;
            tasks.push(compiled);
        }

        // Resolve includes (local files only — URL resolution requires http-client feature)
        let resolved_includes = resolve_includes(
            &root.includes,
            base_dir,
            &mut std::collections::HashSet::new(),
        )?;

        Ok(CompiledTasksfile {
            metadata_name: root.metadata.name.clone(),
            tasks,
            hooks: root.hooks.clone(),
            includes: root.includes.clone(),
            resolved_includes,
        })
    }

    /// Render a task's command with resolved params.
    ///
    /// Looks up the compiled task by name and applies `{{ param }}` substitution.
    /// Returns `None` if the task is not found.
    #[cfg(feature = "templates")]
    pub fn render_task(
        &self,
        compiled: &CompiledTasksfile,
        task_name: &str,
        resolved_params: &HashMap<String, serde_json::Value>,
    ) -> Result<Option<String>> {
        let task = compiled.tasks.iter().find(|t| t.task_id == task_name);
        match task {
            Some(t) => Ok(Some(t.render(resolved_params)?)),
            None => Ok(None),
        }
    }

    /// Non-template version — returns `Ok(None)` since templates are disabled.
    #[cfg(not(feature = "templates"))]
    pub fn render_task(
        &self,
        _compiled: &CompiledTasksfile,
        _task_name: &str,
        _resolved_params: &HashMap<String, serde_json::Value>,
    ) -> Result<Option<String>> {
        Ok(None)
    }

    fn compile_task(
        &self,
        name: &str,
        task_def: &TaskDefinition,
        root: &TasksfileRoot,
    ) -> Result<CompiledTask> {
        // Inherit from template if template: true
        let effective = if task_def.template {
            if let Some(template_name) = task_def.command.as_ref() {
                // `command` field holds the template name when template: true
                if let Some(template) = root.templates.get(template_name) {
                    self.merge_with_template(task_def, template)
                } else {
                    return Err(TasksfileError::Validation(format!(
                        "Task '{}' references unknown template '{}'",
                        name, template_name
                    )));
                }
            } else {
                return Err(TasksfileError::Validation(format!(
                    "Task '{}' has template:true but no template name in command",
                    name
                )));
            }
        } else {
            task_def.clone()
        };

        let priority = self.parse_priority(&effective);
        let retry_policy = effective.retry_policy.clone();

        Ok(CompiledTask {
            task_id: name.to_string(),
            description: effective.desc.clone().unwrap_or_else(|| name.to_string()),
            depends_on: effective.deps.clone(),
            priority,
            deadline: effective.deadline.clone(),
            deadline_behavior: effective.deadline_behavior.clone(),
            parallel_group: effective
                .tags
                .iter()
                .find(|t| t.starts_with("parallel:"))
                .map(|t| t.trim_start_matches("parallel:").to_string()),
            review_required: effective.review_required,
            complexity_hint: effective.complexity_hint.clone(),
            retry_policy,
            command: effective.command.clone().unwrap_or_default(),
            env: effective.env.clone(),
            env_file: effective.env_file.clone(),
            timeout: effective.timeout.clone(),
            tags: effective.tags.clone(),
        })
    }

    fn merge_with_template(
        &self,
        task: &TaskDefinition,
        template: &TemplateDefinition,
    ) -> TaskDefinition {
        let mut merged = task.clone();
        if merged.timeout.is_none() {
            merged.timeout = template.timeout.clone();
        }
        if merged.tags.is_empty() {
            merged.tags = template.tags.clone();
        }
        if merged.retry_policy.is_none() {
            merged.retry_policy = template.retry_policy.clone();
        }
        merged
    }

    fn parse_priority(&self, task: &TaskDefinition) -> i64 {
        // Look for a tag like "priority:high" or just return default
        for tag in &task.tags {
            if let Some(priority) = self.priority_labels.get(tag) {
                return *priority;
            }
            if tag.starts_with("priority:") {
                let name = tag.trim_start_matches("priority:");
                return *self
                    .priority_labels
                    .get(name)
                    .unwrap_or(&self.default_priority);
            }
        }
        self.default_priority
    }
}

impl Default for TasksfileCompiler {
    fn default() -> Self {
        Self::new()
    }
}
