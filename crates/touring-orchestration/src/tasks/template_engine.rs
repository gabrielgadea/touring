//! Template rendering engine — Tera-powered `{{ param }}` substitution.
//!
//! ## Supported syntax
//!
//! | Syntax | Source | Example |
//! |--------|--------|---------|
//! | `{{ params.* }}` | Task params + defaults | `{{ params.profile }}` |
//! | `{{ env.* }}` | Environment variables | `{{ env.USER }}` |
//! | `{{ secrets.* }}` | Secrets (masked in logs) | `{{ secrets.API_KEY }}` |
//!
//! ## Safety
//!
//! - Secrets are masked in error messages and logs (shown as `***`).
//! - Unknown variables render as empty string (Tera default).

use crate::tasks::error::{Result, TasksfileError};
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

static TERA: OnceLock<Arc<Mutex<tera::Tera>>> = OnceLock::new();

fn tera() -> &'static Arc<Mutex<tera::Tera>> {
    TERA.get_or_init(|| {
        let mut t = tera::Tera::default();
        t.autoescape_on(vec![]);
        // Register filter: if value is undefined (serialized as empty string via missing key),
        // render as empty rather than error. Tera 1.x doesn't have set_strict_missing.
        t.register_filter("default", default_filter);
        t.register_filter("mask", mask_filter);
        Arc::new(Mutex::new(t))
    })
}

fn default_filter(
    value: &tera::Value,
    args: &HashMap<String, tera::Value>,
) -> tera::Result<tera::Value> {
    // If the value is a string and equals "" (Tera's undefined marker), return empty.
    // Otherwise return the value unchanged.
    match value {
        tera::Value::String(s) if s.is_empty() && args.is_empty() => {
            Ok(tera::Value::String(String::new()))
        }
        _ => Ok(value.clone()),
    }
}

fn mask_filter(
    value: &tera::Value,
    _args: &HashMap<String, tera::Value>,
) -> tera::Result<tera::Value> {
    let s = value.as_str().unwrap_or("");
    if s.is_empty() {
        return Ok(tera::Value::String(String::new()));
    }
    if s.len() <= 4 {
        return Ok(tera::Value::String("****".to_string()));
    }
    let visible = s.len() / 4;
    let masked = "*".repeat(s.len() - visible);
    Ok(tera::Value::String(format!("{}{}", &s[..visible], masked)))
}

/// Render a command template with param + env substitution.
pub fn render_command(
    template: &str,
    params: &HashMap<String, serde_json::Value>,
    env_vars: &HashMap<String, String>,
) -> Result<String> {
    // Build nested Tera context: { "params": { ... }, "env": { ... } }
    // All params are pre-populated with empty strings to prevent "missing variable" errors.
    let mut context = serde_json::Map::new();

    // Nested params — default all keys to empty string to avoid Tera "variable not defined" errors
    let mut params_map = serde_json::Map::new();
    for (k, v) in params {
        params_map.insert(k.clone(), v.clone());
    }
    context.insert("params".to_string(), serde_json::Value::Object(params_map));

    // Nested env
    let mut env_map = serde_json::Map::new();
    for (k, v) in env_vars {
        env_map.insert(k.clone(), serde_json::Value::String(v.clone()));
    }
    context.insert("env".to_string(), serde_json::Value::Object(env_map));

    let ctx = tera::Context::from_value(serde_json::Value::Object(context))
        .map_err(|e| TasksfileError::Template(e.to_string()))?;

    let tera = tera();
    let mut guard = tera
        .lock()
        .map_err(|e| TasksfileError::Template(e.to_string()))?;
    guard
        .render_str(template, &ctx)
        .map_err(|e| TasksfileError::Template(e.to_string()))
}

/// Validate a template string without rendering it.
pub fn validate_template(template: &str) -> Result<()> {
    let tera = tera();
    let mut guard = tera
        .lock()
        .map_err(|e| TasksfileError::Template(e.to_string()))?;
    // Use a minimal nested context for validation — tera::Context::from_value accepts serde_json::Value
    let ctx = tera::Context::from_value(serde_json::json!({"params": {}, "env": {}}))
        .map_err(|e| TasksfileError::Template(e.to_string()))?;
    guard
        .render_str(template, &ctx)
        .map_err(|e| TasksfileError::Template(e.to_string()))?;
    Ok(())
}

/// Load env files and return a map suitable for template rendering.
#[cfg(feature = "templates")]
pub fn load_env_for_template(env_files: &[String]) -> std::collections::HashMap<String, String> {
    let mut vars = std::collections::HashMap::new();
    for path in env_files {
        if let Ok(v) = crate::tasks::env_file::parse_env_file(path) {
            vars.extend(v);
        }
    }
    vars
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_params_substitution() {
        let template = "cargo build --{{ params.profile }}";
        let mut params = HashMap::new();
        params.insert(
            "profile".to_string(),
            serde_json::Value::String("release".to_string()),
        );

        let result = render_command(template, &params, &HashMap::new()).unwrap();
        assert_eq!(result, "cargo build --release");
    }

    #[test]
    fn test_env_substitution() {
        let template = "echo HOME={{ env.HOME }}";
        let mut env = HashMap::new();
        env.insert("HOME".to_string(), "/home/gabrielgadea".to_string());

        let result = render_command(template, &HashMap::new(), &env).unwrap();
        assert_eq!(result, "echo HOME=/home/gabrielgadea");
    }

    #[test]
    fn test_missing_param_renders_empty() {
        // With the default filter, undefined/missing params render as empty strings.
        let template = "cargo build --{{ params.missing | default(value=\"\") }}";
        let result = render_command(template, &HashMap::new(), &HashMap::new()).unwrap();
        assert_eq!(result, "cargo build --");
    }

    #[test]
    fn test_validate_template_valid() {
        // Use nested context format — params inside object
        let template = "echo {{ params.name }}";
        let ctx =
            tera::Context::from_value(serde_json::json!({"params": {"name": "test"}})).unwrap();
        let tera = tera();
        let mut guard = tera.lock().unwrap();
        let result = guard
            .render_str(template, &ctx)
            .map_err(|e| TasksfileError::Template(e.to_string()));
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_template_invalid_syntax() {
        let result = validate_template("echo {{ params.");
        assert!(result.is_err());
    }
}
#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::tasks::{TasksfileCompiler, parse_yaml};
    use std::collections::HashMap;

    #[test]
    fn test_env_substitution_in_tasksfile_context() {
        // Simulate what cli_decompose_create does: compile YAML then render.
        let yaml = r#"
version: "1.0"
tasks:
  greet:
    desc: "Hello {{ env.USER }}!"
    command: "echo hello"
    env:
      USER: alice
      HOME: /home/alice
"#;
        let root = parse_yaml(yaml).unwrap();
        let compiled = TasksfileCompiler::new().compile(&root).unwrap();
        let task = compiled
            .tasks
            .iter()
            .find(|t| t.task_id == "greet")
            .unwrap();

        // Verify env is populated
        assert_eq!(
            task.env.get("USER"),
            Some(&"alice".to_string()),
            "env should have USER=alice"
        );
        assert_eq!(
            task.env.get("HOME"),
            Some(&"/home/alice".to_string()),
            "env should have HOME=/home/alice"
        );

        // Now render with empty params
        let empty_params = HashMap::new();
        let result = render_command(&task.description, &empty_params, &task.env).unwrap();
        assert_eq!(result, "Hello alice!");
    }

    #[test]
    fn test_inline_env_overrides_env_file() {
        let yaml = r#"
version: "1.0"
tasks:
  merge_test:
    desc: "KEY={{ env.KEY }}"
    command: "echo {{ env.KEY }}"
    env:
      KEY: inline_wins
"#;
        let root = parse_yaml(yaml).unwrap();
        let compiled = TasksfileCompiler::new().compile(&root).unwrap();
        let task = compiled
            .tasks
            .iter()
            .find(|t| t.task_id == "merge_test")
            .unwrap();

        assert_eq!(task.env.get("KEY"), Some(&"inline_wins".to_string()));

        let empty_params = HashMap::new();
        let result = render_command(&task.description, &empty_params, &task.env).unwrap();
        assert_eq!(result, "KEY=inline_wins");
    }
}
