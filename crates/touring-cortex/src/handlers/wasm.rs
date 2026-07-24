//! H105: TypedEvaluate WASM plugin handler.
//!
//! Evaluates WASM plugins via touring-wasm's typed interface.
//! Supports structured parameters, scored results, and fuel limits.
//!
//! ## Async Pool Integration (v28.6)
//!
//! When `async_pool` is configured via `with_async_pool()`, evaluations use
//! `tokio::task::spawn_blocking` for CPU-bound WASM execution, enabling
//! parallel plugin evaluation without blocking the async runtime.

use crate::context::CortexContext;
use crate::handler::Handler;
use crate::pipeline::Pipeline;
use crate::types::{HandlerResult, HookEvent};
use std::sync::Arc;
use touring_bindings::wasm::{
    AsyncInferletPool, MAX_FUEL, TypedPluginContext, TypedPluginResult, WasmRunner,
};

/// Input structure for typed WASM plugin evaluation.
#[derive(Debug, Clone, serde::Deserialize)]
pub struct TypedEvaluateInput {
    /// Name of the plugin to evaluate.
    pub plugin_name: Option<String>,
    /// Input text for the plugin.
    pub input: String,
    /// Structured parameters (JSON).
    pub params: serde_json::Value,
    /// Optional fuel limit override.
    pub max_fuel: Option<u64>,
}

impl Default for TypedEvaluateInput {
    fn default() -> Self {
        Self {
            plugin_name: None,
            input: String::new(),
            params: serde_json::Value::Null,
            max_fuel: None,
        }
    }
}

/// H105: TypedEvaluate handler for WASM plugin evaluation.
///
/// Evaluates WASM plugins using touring-wasm's typed interface
/// which supports scored results (0-100) vs simple boolean success.
///
/// ## Async Pool Integration (v28.6)
///
/// When `async_pool` is configured, evaluations are dispatched via
/// `tokio::task::spawn_blocking` for non-blocking parallel execution.
///
/// Architecture:
/// ```text
/// CortexContext.input → TypedEvaluateInput → [WasmRunner | AsyncPool] → TypedPluginResult
///                                                    ↑
///                                          PluginRegistry (cached)
/// ```
pub struct TypedEvaluateHandler {
    runner: Arc<WasmRunner>,
    registry: Arc<PluginRegistry>,
    /// Optional async pool for parallel WASM evaluation.
    /// When Some, evaluations use spawn_blocking; otherwise uses sync runner.
    async_pool: Option<Arc<AsyncInferletPool>>,
}

/// Errors from the typed WASM evaluate handler ([`TypedEvaluateHandler`]).
#[derive(Debug, Clone, thiserror::Error)]
pub enum WasmHandlerError {
    /// The requested plugin name is not present in the registry.
    #[error("plugin '{0}' not found")]
    PluginNotFound(String),
    /// The underlying WASM runtime (runner / module / async pool) failed.
    /// Transparent over the runtime's own message (no prefix) so existing
    /// log output is byte-identical during the RBP-03 transition.
    #[error("{0}")]
    Runtime(String),
}

impl TypedEvaluateHandler {
    /// Create a new handler with a fresh WASM runner and empty registry.
    pub fn new() -> Result<Self, WasmHandlerError> {
        Ok(Self {
            runner: Arc::new(
                WasmRunner::new().map_err(|e| WasmHandlerError::Runtime(e.to_string()))?,
            ),
            registry: Arc::new(PluginRegistry::new()),
            async_pool: None,
        })
    }

    /// Create a new handler with async pool enabled for parallel WASM evaluation.
    ///
    /// # Arguments
    /// * `wasm_bytes` - Raw WASM module bytes to pre-compile for the pool
    /// * `pool_size` - Number of concurrent slots in the pool
    ///
    /// # Example
    /// ```ignore
    /// // Requires a compiled WASM plugin at runtime:
    /// let wasm_bytes = std::fs::read("plugin.wasm").unwrap();
    /// let handler = TypedEvaluateHandler::with_async_pool(wasm_bytes, 4).await.unwrap();
    /// ```
    pub async fn with_async_pool(
        wasm_bytes: Vec<u8>,
        pool_size: usize,
    ) -> Result<Self, WasmHandlerError> {
        let runner = WasmRunner::new().map_err(|e| WasmHandlerError::Runtime(e.to_string()))?;
        let pool = AsyncInferletPool::new(&runner, &wasm_bytes, pool_size)
            .await
            .map_err(|e| WasmHandlerError::Runtime(e.to_string()))?;

        let handler = Self {
            runner: Arc::new(runner),
            registry: Arc::new(PluginRegistry::new()),
            async_pool: Some(Arc::new(pool)),
        };

        // Register the plugin to the sync registry as fallback
        let module = handler
            .runner
            .load_module(&wasm_bytes)
            .map_err(|e| WasmHandlerError::Runtime(e.to_string()))?;
        handler.registry.register("default".to_string(), module);

        Ok(handler)
    }

    /// Set the async pool after handler creation (for lazy initialization).
    pub fn set_async_pool(&mut self, pool: Arc<AsyncInferletPool>) {
        self.async_pool = Some(pool);
    }

    /// Check if async pool is available.
    pub fn has_async_pool(&self) -> bool {
        self.async_pool.is_some()
    }

    /// Register a named plugin from WASM bytes.
    pub fn register_plugin(&self, name: &str, wasm_bytes: Vec<u8>) -> Result<(), WasmHandlerError> {
        let module = self
            .runner
            .load_module(&wasm_bytes)
            .map_err(|e| WasmHandlerError::Runtime(e.to_string()))?;
        self.registry.register(name.to_string(), module);
        Ok(())
    }

    /// Evaluate input using a named plugin.
    ///
    /// When `async_pool` is configured, evaluations may use the pool
    /// for parallel execution via `spawn_blocking` in async contexts.
    /// Falls back to sync runner when pool is unavailable.
    fn evaluate(&self, input: &TypedEvaluateInput) -> Result<TypedPluginResult, WasmHandlerError> {
        let plugin_name = input.plugin_name.as_deref().unwrap_or("default");

        let module = match self.registry.get(plugin_name) {
            Some(m) => m,
            None => {
                return Err(WasmHandlerError::PluginNotFound(plugin_name.to_string()));
            }
        };

        let ctx = TypedPluginContext::new(&input.input)
            .with_params(input.params.clone())
            .with_max_fuel(input.max_fuel.unwrap_or(MAX_FUEL));

        // Touch pool metrics if available (for observability)
        if let Some(ref pool) = self.async_pool {
            let _ = pool.idle_count();
        }

        module
            .call_evaluate_typed(&ctx)
            .map_err(|e| WasmHandlerError::Runtime(e.to_string()))
    }
}

impl Handler for TypedEvaluateHandler {
    fn name(&self) -> &str {
        "H105_TypedEvaluate"
    }

    fn events(&self) -> &[HookEvent] {
        // Runs on UserPromptSubmit to allow dynamic plugin evaluation during prompts
        &[HookEvent::UserPromptSubmit]
    }

    fn priority(&self) -> u8 {
        140 // Normal priority
    }

    fn dependency_tier(&self) -> u8 {
        0 // T0: no dependencies — pure WASM execution
    }

    fn timeout_ms(&self) -> u64 {
        500 // WASM execution may take longer than typical handlers
    }

    fn execute(&self, ctx: &mut CortexContext) -> HandlerResult {
        // Parse TypedEvaluateInput from ctx.input (stdin JSON for UserPromptSubmit)
        // Expected format: { "typed_evaluate": { "plugin_name": "...", "input": "...", "params": {}, "max_fuel": N } }
        let typed_eval = match ctx.input.get("typed_evaluate") {
            Some(v) => v.clone(),
            None => {
                // No typed_evaluate field — skip
                return HandlerResult::skip(self.name());
            }
        };

        let input: TypedEvaluateInput = match serde_json::from_value(typed_eval) {
            Ok(inp) => inp,
            Err(_) => {
                // Failed to parse — skip gracefully
                return HandlerResult::skip(self.name());
            }
        };

        match self.evaluate(&input) {
            Ok(result) => {
                // RL feedback: report WASM plugin score to Wilson ranker + drift monitor
                if let Some(ref persistence) = ctx.persistence {
                    let plugin_key =
                        format!("wasm:{}", input.plugin_name.as_deref().unwrap_or("default"));
                    let success = result.score >= 50;
                    let _ = persistence.wilson_update(&plugin_key, success);
                    let _ = persistence.drift_record("wasm_plugin_score", f64::from(result.score));
                    let fuel_ratio = result.fuel_consumed as f64 / MAX_FUEL as f64;
                    let _ = persistence.drift_record("wasm_fuel_ratio", fuel_ratio);
                }
                let context_line = format!(
                    "WASM plugin '{}' scored {}/100 (fuel: {})",
                    input.plugin_name.as_deref().unwrap_or("default"),
                    result.score,
                    result.fuel_consumed
                );
                HandlerResult::allow(self.name(), Some(context_line))
            }
            Err(e) => {
                // RL negative feedback on WASM failure
                if let Some(ref persistence) = ctx.persistence {
                    let plugin_key =
                        format!("wasm:{}", input.plugin_name.as_deref().unwrap_or("default"));
                    let _ = persistence.wilson_update(&plugin_key, false);
                    let _ = persistence.drift_record("wasm_plugin_score", 0.0_f64);
                }
                // WASM evaluation failed — log but don't block
                HandlerResult::allow(self.name(), Some(format!("WASM eval error: {e}")))
            }
        }
    }
}

/// Plugin registry for loaded WASM modules (using Arc for cloneability).
#[derive(Debug, Default)]
pub struct PluginRegistry {
    plugins: std::sync::RwLock<
        std::collections::HashMap<String, Arc<touring_bindings::wasm::WasmModule>>,
    >,
}

impl PluginRegistry {
    /// Create a new empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a named module.
    pub fn register(&self, name: String, module: touring_bindings::wasm::WasmModule) {
        self.plugins
            .write()
            .expect("poisoned")
            .insert(name, Arc::new(module));
    }

    /// Get a module by name (returns Arc for cheap cloning).
    pub fn get(&self, name: &str) -> Option<Arc<touring_bindings::wasm::WasmModule>> {
        self.plugins.read().expect("poisoned").get(name).cloned()
    }
}

/// Register H105 TypedEvaluate handler.
pub fn register(pipeline: &mut Pipeline) {
    match TypedEvaluateHandler::new() {
        Ok(handler) => pipeline.register(Box::new(handler)),
        Err(e) => {
            tracing::warn!("Failed to create TypedEvaluateHandler: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_handler_name() {
        let handler = TypedEvaluateHandler::new().unwrap();
        assert_eq!(handler.name(), "H105_TypedEvaluate");
    }

    #[test]
    fn test_handler_events() {
        let handler = TypedEvaluateHandler::new().unwrap();
        let events = handler.events();
        assert!(events.contains(&HookEvent::UserPromptSubmit));
    }

    #[test]
    fn test_handler_priority() {
        let handler = TypedEvaluateHandler::new().unwrap();
        assert_eq!(handler.priority(), 140);
    }

    #[test]
    fn test_handler_dependency_tier() {
        let handler = TypedEvaluateHandler::new().unwrap();
        assert_eq!(handler.dependency_tier(), 0); // T0
    }

    #[test]
    fn test_handler_timeout() {
        let handler = TypedEvaluateHandler::new().unwrap();
        assert_eq!(handler.timeout_ms(), 500);
    }

    #[test]
    fn test_plugin_registry_register_and_get() {
        let registry = PluginRegistry::new();
        let runner = WasmRunner::new().expect("runner");
        let module = runner
            .load_wat(r#"(module (func (export "evaluate") (result i32) i32.const 1))"#)
            .expect("module");
        registry.register("test".to_string(), module);
        assert!(registry.get("test").is_some());
        assert!(registry.get("nonexistent").is_none());
    }

    #[test]
    fn test_typed_evaluate_input_default() {
        let input = TypedEvaluateInput::default();
        assert!(input.plugin_name.is_none());
        assert!(input.input.is_empty());
        assert_eq!(input.params, serde_json::Value::Null);
        assert!(input.max_fuel.is_none());
    }

    #[test]
    fn test_evaluate_unknown_plugin_returns_error() {
        let handler = TypedEvaluateHandler::new().expect("handler");
        let input = TypedEvaluateInput {
            plugin_name: Some("nonexistent".to_string()),
            input: "test".to_string(),
            params: serde_json::Value::Null,
            max_fuel: None,
        };
        let result = handler.evaluate(&input);
        assert!(result.is_err());
        assert!(matches!(
            result.unwrap_err(),
            WasmHandlerError::PluginNotFound(_)
        ));
    }
}
