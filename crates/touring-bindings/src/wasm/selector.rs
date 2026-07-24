//! ContextualPluginSelector — selects WASM plugins based on file/hook context.
//!
//! Provides rule-based plugin selection without requiring touring-rules dependency,
//! keeping touring-wasm isolated and dependency-free.

/// Context for plugin selection decision.
#[derive(Debug, Clone, Default)]
pub struct SelectionContext {
    /// File extension (e.g., "rs", "py", "ts")
    pub file_type: String,
    /// Hook type (e.g., "pre_edit", "post_edit", "post_read")
    pub hook_type: String,
}

impl SelectionContext {
    /// Create a new selection context.
    pub fn new(file_type: impl Into<String>, hook_type: impl Into<String>) -> Self {
        Self {
            file_type: file_type.into(),
            hook_type: hook_type.into(),
        }
    }
}

/// Rule-based plugin selector — no external dependencies.
///
/// Matches file_type + hook_type against a priority-ordered rule table
/// and returns the best plugin name.
#[derive(Debug, Clone)]
pub struct ContextualPluginSelector {
    rules: Vec<SelectionRule>,
}

/// A single selection rule mapping context to plugin.
#[derive(Debug, Clone)]
struct SelectionRule {
    /// File type to match (empty = any)
    file_type: &'static str,
    /// Hook type to match (empty = any)
    hook_type: &'static str,
    /// Plugin name to use
    plugin_name: &'static str,
    /// Priority (lower = higher priority)
    priority: u8,
}

impl Default for ContextualPluginSelector {
    fn default() -> Self {
        Self::new()
    }
}

impl ContextualPluginSelector {
    /// Create a new selector with builtin rules.
    pub fn new() -> Self {
        Self {
            rules: vec![
                SelectionRule {
                    file_type: "rs",
                    hook_type: "pre_edit",
                    plugin_name: "rust_analyzer",
                    priority: 1,
                },
                SelectionRule {
                    file_type: "py",
                    hook_type: "pre_edit",
                    plugin_name: "python_linter",
                    priority: 1,
                },
                SelectionRule {
                    file_type: "rs",
                    hook_type: "post_edit",
                    plugin_name: "clippy_gate",
                    priority: 2,
                },
                SelectionRule {
                    file_type: "ts",
                    hook_type: "pre_edit",
                    plugin_name: "ts_checker",
                    priority: 1,
                },
                SelectionRule {
                    file_type: "",
                    hook_type: "",
                    plugin_name: "default",
                    priority: 5,
                },
            ],
        }
    }

    /// Select the best plugin for the given context.
    ///
    /// Returns the plugin_name of the highest-priority matching rule.
    pub fn select(&self, ctx: &SelectionContext) -> &str {
        let mut best: Option<&SelectionRule> = None;

        for rule in &self.rules {
            let file_match = rule.file_type.is_empty() || rule.file_type == ctx.file_type;
            let hook_match = rule.hook_type.is_empty() || rule.hook_type == ctx.hook_type;

            if file_match && hook_match {
                let is_better = best.map_or(true, |b| rule.priority < b.priority);
                if is_better {
                    best = Some(rule);
                }
            }
        }

        best.map_or("default", |r| r.plugin_name)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_select_rust_pre_edit() {
        let sel = ContextualPluginSelector::new();
        let ctx = SelectionContext::new("rs", "pre_edit");
        assert_eq!(sel.select(&ctx), "rust_analyzer");
    }

    #[test]
    fn test_select_rust_post_edit() {
        let sel = ContextualPluginSelector::new();
        let ctx = SelectionContext::new("rs", "post_edit");
        assert_eq!(sel.select(&ctx), "clippy_gate");
    }

    #[test]
    fn test_select_python_pre_edit() {
        let sel = ContextualPluginSelector::new();
        let ctx = SelectionContext::new("py", "pre_edit");
        assert_eq!(sel.select(&ctx), "python_linter");
    }

    #[test]
    fn test_select_typescript() {
        let sel = ContextualPluginSelector::new();
        let ctx = SelectionContext::new("ts", "pre_edit");
        assert_eq!(sel.select(&ctx), "ts_checker");
    }

    #[test]
    fn test_select_default_fallback() {
        let sel = ContextualPluginSelector::new();
        let ctx = SelectionContext::new("md", "post_read");
        assert_eq!(sel.select(&ctx), "default");
    }

    #[test]
    fn test_select_empty_context() {
        let sel = ContextualPluginSelector::new();
        let ctx = SelectionContext::default();
        assert_eq!(sel.select(&ctx), "default");
    }

    #[test]
    fn test_selection_context_new() {
        let ctx = SelectionContext::new("rs", "pre_edit");
        assert_eq!(ctx.file_type, "rs");
        assert_eq!(ctx.hook_type, "pre_edit");
    }
}
