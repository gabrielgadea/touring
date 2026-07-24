//! Tasksfile YAML parser — loads and validates Tasksfile format.

use crate::tasks::error::{Result, TasksfileError};
use crate::tasks::schema::TasksfileRoot;
use std::fs;
use std::path::Path;

/// Parse a Tasksfile from a YAML string.
pub fn parse_yaml(yaml: &str) -> Result<TasksfileRoot> {
    let root: TasksfileRoot = serde_yaml::from_str(yaml)?;
    validate(&root)?;
    Ok(root)
}

/// Load and parse a Tasksfile from a file path.
pub fn load_file(path: impl AsRef<Path>) -> Result<TasksfileRoot> {
    let content = fs::read_to_string(path.as_ref())?;
    parse_yaml(&content)
}

/// Validate a TasksfileRoot for structural correctness.
fn validate(root: &TasksfileRoot) -> Result<()> {
    if root.version != "1.0" {
        return Err(TasksfileError::Validation(format!(
            "Unsupported Tasksfile version: {}",
            root.version
        )));
    }

    for (name, task) in &root.tasks {
        if task.template && task.command.is_none() {
            return Err(TasksfileError::Validation(format!(
                "Task '{}' has template:true but no command",
                name
            )));
        }

        // Validate params
        for (param_name, param) in &task.params {
            if let Some(ref options) = param.options {
                if let Some(ref default) = param.default {
                    let default_str = match default {
                        serde_json::Value::String(s) => s.clone(),
                        serde_json::Value::Number(n) => n.to_string(),
                        _ => continue,
                    };
                    if !options.iter().any(|o| o == &default_str) {
                        return Err(TasksfileError::InvalidOption(format!(
                            "Task '{}' param '{}' default '{}' not in options {:?}",
                            name, param_name, default_str, options
                        )));
                    }
                }
            }
        }
    }

    // Validate includes
    for (i, include) in root.includes.iter().enumerate() {
        if include.file.is_none() && include.url.is_none() {
            return Err(TasksfileError::Validation(format!(
                "Include #{} has neither file nor url",
                i + 1
            )));
        }
    }

    Ok(())
}

/// Validate that all deps reference existing tasks.
pub fn validate_deps(root: &TasksfileRoot) -> Result<()> {
    let task_names: std::collections::HashSet<_> = root.tasks.keys().collect();

    for (task_name, task) in &root.tasks {
        for dep in &task.deps {
            if !task_names.contains(dep) {
                return Err(TasksfileError::Validation(format!(
                    "Task '{}' depends on unknown task '{}'",
                    task_name, dep
                )));
            }
        }
    }

    Ok(())
}
