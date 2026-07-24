//! PluginRuntime — Extensibility framework for 3rd-party Touring plugins.
//!
//! Allows external developers to extend Touring functionality through WASM plugins.

use std::collections::HashMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// Plugin manifest metadata.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct PluginManifest {
    /// Plugin name, used together with `version` to form the plugin id.
    pub name: String,
    /// Plugin version string, used together with `name` to form the plugin id.
    pub version: String,
    /// Lifecycle hooks this plugin subscribes to.
    pub hooks: Vec<HookName>,
    /// Filesystem, command, and network permissions the plugin requests.
    #[serde(default)]
    pub permissions: PluginPermissions,
}

/// Hook names that plugins can implement.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum HookName {
    /// Fires before a file Read.
    PreRead,
    /// Fires after a file Read.
    PostRead,
    /// Fires before an Edit.
    PreEdit,
    /// Fires after an Edit.
    PostEdit,
    /// Fires before a Bash command.
    PreBash,
    /// Fires after a Bash command.
    PostBash,
    /// Fires before a Write.
    PreWrite,
    /// Fires after a Write.
    PostWrite,
}

impl std::fmt::Display for HookName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            HookName::PreRead => write!(f, "pre_read"),
            HookName::PostRead => write!(f, "post_read"),
            HookName::PreEdit => write!(f, "pre_edit"),
            HookName::PostEdit => write!(f, "post_edit"),
            HookName::PreBash => write!(f, "pre_bash"),
            HookName::PostBash => write!(f, "post_bash"),
            HookName::PreWrite => write!(f, "pre_write"),
            HookName::PostWrite => write!(f, "post_write"),
        }
    }
}

/// Permissions requested by a plugin.
#[derive(Debug, Clone, Default, Deserialize, Serialize)]
#[serde(default)]
pub struct PluginPermissions {
    /// Path patterns the plugin is allowed to read.
    pub read_files: Vec<String>,
    /// Path patterns the plugin is allowed to write.
    pub write_files: Vec<String>,
    /// Command names the plugin is allowed to execute.
    pub execute_commands: Vec<String>,
    /// Whether the plugin is granted outbound network access.
    pub network: bool,
}

/// A path pattern for permission matching.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PathPattern {
    pattern: String,
}

impl PathPattern {
    /// Builds a path pattern from a glob-like string (`*` single-segment, `**` recursive).
    pub fn new(pattern: &str) -> Result<Self, PluginError> {
        Ok(Self {
            pattern: pattern.to_string(),
        })
    }

    /// Returns `true` if `path` matches this pattern.
    pub fn matches(&self, path: &Path) -> bool {
        let path_str = path.to_string_lossy();
        let p = &self.pattern;

        // Simple wildcard matching: ** matches anything, * matches single segment
        if p.contains("**") {
            let prefix = p.split("**").next().unwrap_or("");
            return path_str.starts_with(prefix);
        }

        if p.contains('*') {
            let glob_parts: Vec<&str> = p.split('*').collect();
            // glob_parts like ["", ".rs"] or ["src/", ".rs"]
            if let [prefix, suffix] = glob_parts.as_slice() {
                // Handle patterns like "*.rs" (prefix="", suffix=".rs")
                // or "src/*.rs" (prefix="src/", suffix=".rs")
                if prefix.is_empty() {
                    return path_str.ends_with(suffix);
                }
                if suffix.is_empty() {
                    return path_str.starts_with(prefix);
                }
                return path_str.starts_with(prefix) && path_str.ends_with(suffix);
            }
            true
        } else {
            path_str == *p || path_str.ends_with(p)
        }
    }
}

/// A loaded plugin instance.
#[derive(Debug)]
pub struct LoadedPlugin {
    /// Unique plugin id, formatted as `{name}-{version}`.
    pub id: String,
    /// Parsed manifest declaring the plugin's hooks and permissions.
    pub manifest: PluginManifest,
    wasm_bytes: Vec<u8>,
    enabled: bool,
}

/// Plugin loading error.
#[derive(Debug, thiserror::Error)]
pub enum PluginError {
    /// I/O failure while reading the plugin directory or files.
    #[error("failed to read plugin directory: {0}")]
    Io(#[from] std::io::Error),
    /// The `plugin.yaml` manifest could not be parsed.
    #[error("failed to parse manifest: {0}")]
    ManifestParse(#[from] serde_yaml::Error),
    /// No plugin with the requested id is loaded.
    #[error("plugin not found: {0}")]
    NotFound(String),
    /// A permission path pattern was malformed.
    #[error("invalid pattern: {0}")]
    InvalidPattern(String),
}

/// Result of executing a plugin hook.
#[derive(Debug, Clone)]
pub struct PluginHookResult {
    /// Id of the plugin that produced this result.
    pub plugin_id: String,
    /// Whether the plugin requested a modification to the hooked action.
    pub modified: bool,
    /// Free-form output the plugin emitted for the hook.
    pub output: String,
}

/// PluginRuntime manages loading and execution of 3rd-party plugins.
#[derive(Debug, Default)]
pub struct PluginRuntime {
    plugins: HashMap<String, LoadedPlugin>,
}

impl PluginRuntime {
    /// Creates an empty runtime with no plugins loaded.
    pub fn new() -> Self {
        Self {
            plugins: HashMap::new(),
        }
    }

    /// Load a plugin from a directory.
    pub fn load_plugin(&mut self, dir: &Path) -> Result<String, PluginError> {
        let manifest_path = dir.join("plugin.yaml");
        let manifest_content = std::fs::read_to_string(&manifest_path)?;
        let manifest: PluginManifest = serde_yaml::from_str(&manifest_content)?;

        let wasm_path = dir.join("plugin.wasm");
        let wasm_bytes = std::fs::read(&wasm_path).unwrap_or_default();

        let id = format!("{}-{}", manifest.name, manifest.version);
        let plugin = LoadedPlugin {
            id: id.clone(),
            manifest,
            wasm_bytes,
            enabled: true,
        };

        self.plugins.insert(id.clone(), plugin);
        Ok(id)
    }

    /// Removes a loaded plugin by id; errors if no such plugin exists.
    pub fn unload_plugin(&mut self, plugin_id: &str) -> Result<(), PluginError> {
        self.plugins
            .remove(plugin_id)
            .ok_or_else(|| PluginError::NotFound(plugin_id.to_string()))?;
        Ok(())
    }

    /// Enables or disables a loaded plugin by id; errors if no such plugin exists.
    pub fn set_enabled(&mut self, plugin_id: &str, enabled: bool) -> Result<(), PluginError> {
        let plugin = self
            .plugins
            .get_mut(plugin_id)
            .ok_or_else(|| PluginError::NotFound(plugin_id.to_string()))?;
        plugin.enabled = enabled;
        Ok(())
    }

    /// Returns the loaded plugin registered under `plugin_id`, or `None` if absent.
    #[must_use]
    pub fn get_plugin(&self, plugin_id: &str) -> Option<&LoadedPlugin> {
        self.plugins.get(plugin_id)
    }

    /// Returns references to every currently loaded plugin (enabled or not).
    #[must_use]
    pub fn list_plugins(&self) -> Vec<&LoadedPlugin> {
        self.plugins.values().collect()
    }

    /// Runs the given `hook` on every enabled plugin that subscribes to it, collecting results.
    pub async fn execute_hook(&self, hook: HookName) -> Vec<PluginHookResult> {
        let mut results = Vec::new();

        for plugin in self.plugins.values() {
            if !plugin.enabled {
                continue;
            }
            if !plugin.manifest.hooks.contains(&hook) {
                continue;
            }

            // EC62: wire wasm_bytes — surface WASM size in skeleton output for
            // diagnostics. When WASM execution is implemented, this path will
            // use wasm_bytes to instantiate and call the plugin module.
            let result = PluginHookResult {
                plugin_id: plugin.id.clone(),
                modified: false,
                output: format!(
                    "[skeleton] {} hook executed (wasm_size={})",
                    hook,
                    plugin.wasm_bytes.len()
                ),
            };
            results.push(result);
        }

        results
    }

    /// Returns the number of loaded plugins.
    #[must_use]
    pub fn len(&self) -> usize {
        self.plugins.len()
    }

    /// Returns `true` if no plugins are loaded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.plugins.is_empty()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path_pattern_exact() {
        let p = PathPattern::new("src/lib.rs").expect("valid pattern");
        assert!(p.matches(Path::new("src/lib.rs")));
        assert!(!p.matches(Path::new("src/main.rs")));
    }

    #[test]
    fn test_path_pattern_wildcard() {
        let p = PathPattern::new("src/**/*.rs").expect("valid pattern");
        assert!(p.matches(Path::new("src/lib.rs")));
        assert!(p.matches(Path::new("src/foo/bar.rs")));
        assert!(!p.matches(Path::new("tests/lib.rs")));
    }

    #[test]
    fn test_path_pattern_suffix() {
        let p = PathPattern::new("*.rs").expect("valid pattern");
        // *.rs matches any .rs file in any directory (suffix matching)
        assert!(p.matches(Path::new("lib.rs")));
        assert!(p.matches(Path::new("src/lib.rs")));
    }

    #[test]
    fn test_plugin_runtime_empty() {
        let runtime = PluginRuntime::new();
        assert!(runtime.is_empty());
        assert_eq!(runtime.len(), 0);
    }

    #[test]
    fn test_hook_name_display() {
        assert_eq!(HookName::PreRead.to_string(), "pre_read");
        assert_eq!(HookName::PostEdit.to_string(), "post_edit");
    }
}
