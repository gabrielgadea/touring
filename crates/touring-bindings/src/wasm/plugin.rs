//! Plugin context and result types for the WASM plugin system.

/// Context passed to a WASM plugin for evaluation.
///
/// Serialized as a JSON string when crossing the WASM boundary
/// (future: shared memory ABI for zero-copy).
#[derive(Debug, Clone, PartialEq)]
pub struct PluginContext {
    /// Arbitrary input text for the plugin to process.
    pub input: String,
    /// Optional configuration key-value pairs.
    pub config: Vec<(String, String)>,
}

impl PluginContext {
    /// Create a new context with the given input and no config.
    pub fn new(input: impl Into<String>) -> Self {
        Self {
            input: input.into(),
            config: Vec::new(),
        }
    }

    /// Add a config entry and return `self` for chaining.
    pub fn with_config(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.config.push((key.into(), value.into()));
        self
    }
}

/// Result produced by running a WASM plugin.
#[derive(Debug, Clone, PartialEq)]
pub struct PluginResult {
    /// Output produced by the plugin.
    pub output: String,
    /// Number of WASM fuel units consumed during execution.
    pub fuel_consumed: u64,
    /// Whether the plugin reported success (via its return value).
    pub success: bool,
}

impl PluginResult {
    /// Construct a successful result.
    pub fn success(output: impl Into<String>, fuel_consumed: u64) -> Self {
        Self {
            output: output.into(),
            fuel_consumed,
            success: true,
        }
    }

    /// Construct a failed result (plugin ran but indicated failure).
    pub fn failure(fuel_consumed: u64) -> Self {
        Self {
            output: String::new(),
            fuel_consumed,
            success: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_plugin_context_new() {
        let ctx = PluginContext::new("hello");
        assert_eq!(ctx.input, "hello");
        assert!(ctx.config.is_empty());
    }

    #[test]
    fn test_plugin_context_with_config() {
        let ctx = PluginContext::new("input")
            .with_config("key1", "val1")
            .with_config("key2", "val2");
        assert_eq!(ctx.config.len(), 2);
        assert_eq!(ctx.config[0], ("key1".to_string(), "val1".to_string()));
        assert_eq!(ctx.config[1], ("key2".to_string(), "val2".to_string()));
    }

    #[test]
    fn test_plugin_result_success() {
        let r = PluginResult::success("output data", 42);
        assert_eq!(r.output, "output data");
        assert_eq!(r.fuel_consumed, 42);
        assert!(r.success);
    }

    #[test]
    fn test_plugin_result_failure() {
        let r = PluginResult::failure(100);
        assert!(r.output.is_empty());
        assert_eq!(r.fuel_consumed, 100);
        assert!(!r.success);
    }
}
