//! Devrcfile parser — parses Devrcfile YAML format into Rust structs.

use anyhow::Result;
use serde::Deserialize;
use std::collections::HashMap;

/// Devrcfile top-level root.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct DevrcfileRoot {
    /// Global devrc configuration block (`devrc_config`).
    #[serde(default)]
    pub devrc_config: DevrcConfig,
    /// User-defined variables available for substitution.
    #[serde(default)]
    pub variables: HashMap<String, serde_json::Value>,
    /// Paths to `.env` files loaded for all tasks.
    #[serde(default)]
    pub env_file: Vec<String>,
    /// Environment variables applied to all tasks.
    #[serde(default)]
    pub environment: HashMap<String, String>,
    /// Commands run once before the whole run begins.
    #[serde(default)]
    pub before_script: Vec<String>,
    /// Commands run once after the whole run finishes.
    #[serde(default)]
    pub after_script: Vec<String>,
    /// Commands run before each individual task.
    #[serde(default)]
    pub before_task: Vec<String>,
    /// Commands run after each individual task.
    #[serde(default)]
    pub after_task: Vec<String>,
    /// External Devrcfiles to include (local files or remote URLs).
    #[serde(default)]
    pub include: Vec<IncludeSpec>,
    /// Named tasks keyed by task name.
    #[serde(default)]
    pub tasks: HashMap<String, Task>,
}

/// Global devrc configuration.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct DevrcConfig {
    /// Shell used to execute task commands (e.g. `bash`).
    #[serde(default)]
    pub shell: Option<String>,
    /// Logging verbosity level.
    #[serde(default)]
    pub log_level: Option<String>,
    /// Cache time-to-live in seconds for cached task results.
    #[serde(default)]
    pub cache_ttl: Option<u64>,
    /// Plugin configuration keyed by plugin name.
    #[serde(default)]
    pub plugins: HashMap<String, serde_json::Value>,
    /// Interpreter (Deno-style runtime) configuration.
    #[serde(default)]
    pub interpreter: InterpreterConfig,
}

/// Deno/ interpreter configuration.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct InterpreterConfig {
    /// Runtime binary used to execute scripts (e.g. `deno`).
    #[serde(default)]
    pub runtime: Option<String>,
    /// Runtime permission grants keyed by permission name.
    #[serde(default)]
    pub permissions: HashMap<String, String>,
}

/// Devrcfile include spec.
#[derive(Debug, Clone, Deserialize)]
pub struct IncludeSpec {
    /// Local file path to include, if this is a file include.
    pub file: Option<String>,
    /// Remote URL to fetch, if this is a URL include.
    pub url: Option<String>,
    /// Path resolution strategy (`relative`, `root`, or `absolute`).
    #[serde(default)]
    pub path_resolve: Option<String>,
    /// Authentication used when fetching a URL include.
    #[serde(default)]
    pub auth: Option<AuthSpec>,
}

impl IncludeSpec {
    /// Returns the effective path resolution strategy, defaulting to `relative`.
    pub fn path_resolve(&self) -> String {
        self.path_resolve
            .clone()
            .unwrap_or_else(|| "relative".to_string())
    }
}

/// Authentication spec for URL includes.
#[derive(Debug, Clone, Deserialize)]
pub struct AuthSpec {
    /// Host machine the credentials apply to.
    pub machine: String,
    /// Authentication scheme (e.g. `basic`, `bearer`); defaults to `basic`.
    #[serde(rename = "type")]
    pub auth_type: Option<String>,
    /// Username for basic authentication.
    pub username: Option<String>,
    /// Password for basic authentication.
    pub password: Option<String>,
    /// Bearer token for token-based authentication.
    pub token: Option<String>,
}

impl AuthSpec {
    /// Returns the effective auth type, defaulting to "basic".
    pub fn auth_type(&self) -> String {
        self.auth_type
            .clone()
            .unwrap_or_else(|| "basic".to_string())
    }
}

/// Individual task definition in Devrcfile.
#[derive(Debug, Clone, Deserialize, Default)]
pub struct Task {
    /// Human-readable task description.
    pub desc: Option<String>,
    /// Parameters accepted by the task, keyed by parameter name.
    #[serde(default)]
    pub params: HashMap<String, Param>,
    /// Names of tasks this task depends on.
    #[serde(default)]
    pub deps: Vec<String>,
    /// Environment variables scoped to this task.
    #[serde(default)]
    pub environment: HashMap<String, String>,
    /// Commands executed for this task.
    #[serde(default)]
    pub exec: Vec<String>,
    /// Maximum execution duration before the task is aborted.
    #[serde(default)]
    pub timeout: Option<String>,
    /// Free-form tags used for filtering and grouping.
    #[serde(default)]
    pub tags: Vec<String>,
    /// Retry behavior applied when the task fails.
    #[serde(default)]
    pub retry_policy: Option<RetryPolicy>,
    /// Whether the task requires manual review before running.
    #[serde(default)]
    pub review_required: Option<bool>,
    /// Commands run before this task executes.
    #[serde(default)]
    pub before_task: Vec<String>,
    /// Commands run after this task executes.
    #[serde(default)]
    pub after_task: Vec<String>,
}

/// Task parameter definition.
#[derive(Debug, Clone, Deserialize)]
pub struct Param {
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
    #[serde(default)]
    pub description: Option<String>,
}

/// Retry policy definition.
#[derive(Debug, Clone, Deserialize)]
pub struct RetryPolicy {
    /// Maximum number of attempts before giving up.
    pub max_attempts: Option<usize>,
    /// Initial backoff delay in milliseconds between attempts.
    #[serde(default)]
    pub backoff_ms: Option<u64>,
    /// Multiplier applied to the backoff delay after each retry.
    #[serde(default)]
    pub backoff_multiplier: Option<f64>,
}

/// Parse a Devrcfile YAML string into a DevrcfileRoot.
pub fn parse_devrcfile(content: &str) -> Result<DevrcfileRoot> {
    let root: DevrcfileRoot = serde_yaml::from_str(content)?;
    Ok(root)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal() {
        let yaml = "tasks:\n  build:\n    exec: [cargo build]";
        let root = parse_devrcfile(yaml).unwrap();
        assert!(root.tasks.contains_key("build"));
        assert_eq!(root.tasks["build"].exec, vec!["cargo build"]);
    }

    #[test]
    fn test_parse_full() {
        let yaml = r#"
devrc_config:
  shell: /bin/bash
  log_level: debug
  cache_ttl: 3600

variables:
  profile: release

env_file:
  - .env.local
  - .env.production

environment:
  RUST_BACKTRACE: "1"

before_script:
  - echo "Starting..."

include:
  - file: ./shared.yml
    path_resolve: relative

tasks:
  build:
    desc: "Build the project"
    params:
      profile:
        required: false
        default: release
    environment:
      BUILD_PROFILE: release
    exec:
      - cargo build --{{ profile }}
    tags:
      - ci
    timeout: 300s
"#;
        let root = parse_devrcfile(yaml).unwrap();
        assert_eq!(root.devrc_config.shell, Some("/bin/bash".to_string()));
        assert_eq!(
            root.variables.get("profile"),
            Some(&serde_json::Value::String("release".to_string()))
        );
        assert_eq!(root.env_file.len(), 2);
        assert_eq!(root.before_script, vec!["echo \"Starting...\""]);
        assert_eq!(root.include.len(), 1);
        assert!(root.tasks.contains_key("build"));
        assert_eq!(root.tasks["build"].exec[0], "cargo build --{{ profile }}");
    }

    #[test]
    fn test_parse_include_with_url() {
        let yaml = r#"
include:
  - url: "https://example.com/tasks.yml"
    auth:
      machine: example.com
      type: bearer
      token: supersecret
tasks:
  test:
    exec: [cargo test]
"#;
        let root = parse_devrcfile(yaml).unwrap();
        assert_eq!(
            root.include[0].url,
            Some("https://example.com/tasks.yml".to_string())
        );
        assert_eq!(
            root.include[0].auth.as_ref().map(|a| a.auth_type()),
            Some("bearer".to_string())
        );
    }

    #[test]
    fn test_parse_retry_policy() {
        let yaml = r#"
tasks:
  flaky:
    exec: [cargo test]
    retry_policy:
      max_attempts: 3
      backoff_ms: 500
      backoff_multiplier: 2.0
"#;
        let root = parse_devrcfile(yaml).unwrap();
        let rp = root.tasks["flaky"].retry_policy.as_ref().unwrap();
        assert_eq!(rp.max_attempts, Some(3));
        assert_eq!(rp.backoff_ms, Some(500));
        assert_eq!(rp.backoff_multiplier, Some(2.0));
    }
}
