//! Devrcfile → Touring Tasksfile converter.

use crate::devrc::parser::{
    AuthSpec, DevrcfileRoot, IncludeSpec as DevrcInclude, RetryPolicy, Task,
};
use crate::tasks::{
    GlobalHooks, IncludeSpec, Metadata, NetrcAuth, ParamDefinition, RetryPolicyDef, TaskDefinition,
    TaskHooks, TasksfileRoot, TemplateDefinition,
};
use anyhow::Result;
use std::collections::HashMap;

/// Result of a Devrcfile → Tasksfile conversion.
#[derive(Debug)]
pub struct ConversionResult {
    /// The converted Touring `TasksfileRoot`.
    pub tasksfile: TasksfileRoot,
    /// Non-fatal warnings collected during conversion (e.g. dropped fields).
    pub warnings: Vec<String>,
}

impl ConversionResult {
    /// Create a `ConversionResult` wrapping the given tasksfile with no warnings.
    pub fn new(tasksfile: TasksfileRoot) -> Self {
        Self {
            tasksfile,
            warnings: Vec::new(),
        }
    }

    /// Append a warning and return the updated result (builder style).
    pub fn with_warning(mut self, warning: impl Into<String>) -> Self {
        self.warnings.push(warning.into());
        self
    }
}

/// Convert a DevrcfileRoot into a Touring TasksfileRoot.
pub fn devrcfile_to_tasksfile(devrc: &DevrcfileRoot) -> Result<ConversionResult> {
    let mut warnings = Vec::new();

    // Build metadata
    let metadata = Metadata {
        name: devrc
            .variables
            .get("project_name")
            .and_then(|v| v.as_str().map(String::from)),
        description: None,
        shell: devrc.devrc_config.shell.clone(),
        log_level: devrc.devrc_config.log_level.clone(),
        cache_ttl: devrc.devrc_config.cache_ttl,
    };

    // Global variables → params (for top-level template context)
    let mut templates = HashMap::new();
    for name in devrc.variables.keys() {
        let td = TemplateDefinition {
            timeout: None,
            tags: vec![],
            retry_policy: None,
        };
        templates.insert(name.clone(), td);
    }

    // Convert includes: env_file entries become file includes
    let mut includes: Vec<IncludeSpec> = Vec::new();

    for ef in &devrc.env_file {
        warnings.push(format!("env_file '{}' converted to file include", ef));
        includes.push(IncludeSpec {
            file: Some(ef.clone()),
            url: None,
            path_resolve: "relative".to_string(),
            auth: None,
        });
    }

    for inc in &devrc.include {
        let converted = convert_include(inc);
        includes.push(converted);
    }

    // Convert hooks
    let hooks = GlobalHooks {
        before_all: devrc.before_script.clone(),
        after_all: devrc.after_script.clone(),
        on_failure: vec![],
    };

    // Convert tasks
    let mut tasks: HashMap<String, TaskDefinition> = HashMap::new();

    for (name, task) in &devrc.tasks {
        let converted = convert_task(name, task, &devrc.environment)?;
        tasks.insert(name.clone(), converted);
    }

    let tasksfile = TasksfileRoot {
        version: "1.0".to_string(),
        metadata,
        templates,
        tasks,
        includes,
        hooks,
    };

    Ok(ConversionResult::new(tasksfile).with_warning("Phase 6 adapter — first pass"))
}

fn convert_include(inc: &DevrcInclude) -> IncludeSpec {
    let auth = inc.auth.as_ref().map(convert_auth);
    IncludeSpec {
        file: inc.file.clone(),
        url: inc.url.clone(),
        path_resolve: inc.path_resolve(),
        auth,
    }
}

fn convert_auth(auth: &AuthSpec) -> NetrcAuth {
    NetrcAuth {
        machine: auth.machine.clone(),
        auth_type: auth.auth_type(),
        username: auth.username.clone(),
        password: auth.password.clone(),
    }
}

fn convert_task(
    name: &str,
    task: &Task,
    global_env: &HashMap<String, String>,
) -> Result<TaskDefinition> {
    if task.exec.is_empty() {
        return Err(anyhow::anyhow!("Task '{}' has no exec commands", name));
    }

    // Join exec commands with && for shell execution
    let command = if task.exec.len() == 1 {
        task.exec[0].clone()
    } else {
        task.exec.join(" && ")
    };

    // Merge global environment with task-level environment
    let mut env = global_env.clone();
    for (k, v) in &task.environment {
        env.insert(k.clone(), v.clone());
    }

    // Convert params
    let mut params = HashMap::new();
    for (pname, pdef) in &task.params {
        params.insert(
            pname.clone(),
            ParamDefinition {
                required: pdef.required,
                default: pdef.default.clone(),
                options: pdef.options.clone(),
                description: pdef.description.clone(),
            },
        );
    }

    // Convert retry policy
    let retry_policy = task.retry_policy.as_ref().map(convert_retry_policy);

    // Per-task hooks
    let hooks = TaskHooks {
        before_task: task.before_task.clone(),
        after_task: task.after_task.clone(),
        on_failure: vec![],
    };

    Ok(TaskDefinition {
        desc: task.desc.clone(),
        template: false,
        command: Some(command),
        deps: task.deps.clone(),
        env,
        params,
        timeout: task.timeout.clone(),
        tags: task.tags.clone(),
        complexity_hint: None,
        deadline: None,
        deadline_behavior: None,
        review_required: task.review_required.unwrap_or(false),
        retry_policy,
        env_file: vec![],
        hooks,
    })
}

fn convert_retry_policy(rp: &RetryPolicy) -> RetryPolicyDef {
    RetryPolicyDef {
        max_attempts: rp.max_attempts.unwrap_or(1),
        backoff_ms: rp.backoff_ms.unwrap_or(1000),
        backoff_multiplier: rp.backoff_multiplier.unwrap_or(2.0),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_convert_minimal() {
        let yaml = "tasks:\n  build:\n    exec: [cargo build]";
        let devrc = crate::devrc::parser::parse_devrcfile(yaml).unwrap();
        let result = devrcfile_to_tasksfile(&devrc).unwrap();
        assert!(result.tasksfile.tasks.contains_key("build"));
        assert_eq!(
            result.tasksfile.tasks["build"].command,
            Some("cargo build".to_string())
        );
    }

    #[test]
    fn test_convert_full() {
        let yaml = r#"
devrc_config:
  shell: /bin/bash
  log_level: debug
variables:
  project_name: myapp
env_file:
  - .env.local
environment:
  RUST_BACKTRACE: "1"
before_script:
  - echo "Starting..."
include:
  - file: ./shared.yml
tasks:
  build:
    desc: "Build the project"
    params:
      profile:
        required: false
        default: release
    deps: []
    environment:
      BUILD_PROFILE: release
    exec:
      - cargo build
    tags:
      - ci
    timeout: 300s
"#;
        let devrc = crate::devrc::parser::parse_devrcfile(yaml).unwrap();
        let result = devrcfile_to_tasksfile(&devrc).unwrap();
        assert_eq!(
            result.tasksfile.metadata.shell,
            Some("/bin/bash".to_string())
        );
        assert_eq!(result.tasksfile.includes.len(), 2);
        assert!(result.tasksfile.tasks.contains_key("build"));
        let build = &result.tasksfile.tasks["build"];
        assert_eq!(build.command, Some("cargo build".to_string()));
        assert_eq!(build.env.get("RUST_BACKTRACE"), Some(&"1".to_string()));
        assert_eq!(build.env.get("BUILD_PROFILE"), Some(&"release".to_string()));
        assert!(!result.warnings.is_empty());
    }

    #[test]
    fn test_convert_multi_exec() {
        let yaml = r#"
tasks:
  test:
    exec:
      - cargo fmt
      - cargo clippy
      - cargo test
"#;
        let devrc = crate::devrc::parser::parse_devrcfile(yaml).unwrap();
        let result = devrcfile_to_tasksfile(&devrc).unwrap();
        assert_eq!(
            result.tasksfile.tasks["test"].command,
            Some("cargo fmt && cargo clippy && cargo test".to_string())
        );
    }

    #[test]
    fn test_convert_missing_exec_error() {
        let yaml = "tasks:\n  empty:\n    desc: No exec";
        let devrc = crate::devrc::parser::parse_devrcfile(yaml).unwrap();
        let result = devrcfile_to_tasksfile(&devrc);
        assert!(result.is_err());
    }

    #[test]
    fn test_convert_deps() {
        let yaml = r#"
tasks:
  build:
    exec: [cargo build]
  test:
    exec: [cargo test]
    deps: [build]
"#;
        let devrc = crate::devrc::parser::parse_devrcfile(yaml).unwrap();
        let result = devrcfile_to_tasksfile(&devrc).unwrap();
        assert!(
            result.tasksfile.tasks["test"]
                .deps
                .contains(&"build".to_string())
        );
    }

    #[test]
    fn test_include_with_url_and_auth() {
        let yaml = r#"
include:
  - url: "https://api.github.com/tasks.yml"
    auth:
      machine: api.github.com
      type: bearer
      token: gxY_xxxxx
tasks:
  sync:
    exec: [echo done]
"#;
        let devrc = crate::devrc::parser::parse_devrcfile(yaml).unwrap();
        let result = devrcfile_to_tasksfile(&devrc).unwrap();
        let inc = &result.tasksfile.includes[0];
        assert_eq!(
            inc.url,
            Some("https://api.github.com/tasks.yml".to_string())
        );
        let auth = inc.auth.as_ref().unwrap();
        assert_eq!(auth.machine, "api.github.com");
        assert_eq!(auth.auth_type, "bearer");
    }
}
