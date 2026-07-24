//! Typed evaluate interface for WASM plugins.
//!
//! Extends the basic `evaluate() -> i32` (0/1) interface with:
//! - `evaluate_scored() -> i32` (0..=100 score)
//! - Structured parameters via [`TypedPluginContext`]
//! - Rich results via [`TypedPluginResult`]
//!
//! Falls back gracefully: if a module only exports `evaluate`, the score is
//! mapped to 0 (failure) or 100 (success).

use crate::wasm::error::WasmBindingsError;
use crate::wasm::runner::{MAX_FUEL, WasmModule};

/// Extended context with structured parameters for typed evaluation.
///
/// # Example
/// ```
/// use touring_bindings::wasm::TypedPluginContext;
///
/// let ctx = TypedPluginContext::new("analyze this")
///     .with_params(serde_json::json!({"threshold": 0.8}))
///     .with_max_fuel(5_000_000);
/// ```
#[derive(Debug, Clone)]
pub struct TypedPluginContext {
    /// Input text for the plugin to process.
    pub input: String,
    /// Structured parameters (JSON value).
    pub params: serde_json::Value,
    /// Maximum fuel for this evaluation (overrides default [`MAX_FUEL`]).
    pub max_fuel: u64,
}

impl TypedPluginContext {
    /// Create a new typed context with default fuel and no params.
    pub fn new(input: impl Into<String>) -> Self {
        Self {
            input: input.into(),
            params: serde_json::Value::Null,
            max_fuel: MAX_FUEL,
        }
    }

    /// Set structured parameters.
    #[must_use]
    pub fn with_params(mut self, params: serde_json::Value) -> Self {
        self.params = params;
        self
    }

    /// Override the default fuel limit.
    #[must_use]
    pub fn with_max_fuel(mut self, fuel: u64) -> Self {
        self.max_fuel = fuel;
        self
    }
}

/// Extended result from typed evaluation.
///
/// The `score` field provides a 0..=100 relevance score instead of a simple
/// boolean. `success` is derived as `score >= 50`.
#[derive(Debug, Clone)]
pub struct TypedPluginResult {
    /// Whether the plugin reported success (score >= 50).
    pub success: bool,
    /// Relevance score from 0 (worst) to 100 (best).
    pub score: u8,
    /// Structured output (JSON value, Null if module provides none).
    pub output: serde_json::Value,
    /// WASM fuel units consumed during execution.
    pub fuel_consumed: u64,
    /// Error message if the plugin reported failure.
    pub error: Option<String>,
}

impl WasmModule {
    /// Execute the module with typed context and scored result.
    ///
    /// Resolution order:
    /// 1. If module exports `evaluate_scored() -> i32`: use raw score clamped to 0..=100.
    /// 2. If module exports `evaluate() -> i32`: map non-zero to 100, zero to 0.
    /// 3. If neither exists: return score=0, success=false.
    ///
    /// `success` is derived as `score >= 50`.
    pub fn call_evaluate_typed(
        &self,
        ctx: &TypedPluginContext,
    ) -> Result<TypedPluginResult, WasmBindingsError> {
        use wasmtime::{Instance, Store};

        tracing::debug!(input = %ctx.input, "WasmModule::call_evaluate_typed");

        let mut store = Store::new(self.engine(), ());
        store
            .set_fuel(ctx.max_fuel)
            .map_err(|e| WasmBindingsError::Instantiate(format!("set fuel: {e}")))?;

        let instance = Instance::new(&mut store, self.module_ref(), &[])
            .map_err(|e| WasmBindingsError::Instantiate(format!("instantiate: {e}")))?;

        let fuel_before = store
            .get_fuel()
            .map_err(|e| WasmBindingsError::Instantiate(format!("get fuel: {e}")))?;

        let score = match instance.get_func(&mut store, "evaluate_scored") {
            Some(func) => {
                let typed = func.typed::<(), i32>(&store).map_err(|e| {
                    WasmBindingsError::Evaluate(format!("evaluate_scored signature: {e}"))
                })?;
                let raw = typed.call(&mut store, ()).map_err(|e| {
                    WasmBindingsError::Evaluate(format!("evaluate_scored call: {e}"))
                })?;
                raw.clamp(0, 100) as u8
            }
            None => match instance.get_func(&mut store, "evaluate") {
                Some(func) => {
                    let typed = func.typed::<(), i32>(&store).map_err(|e| {
                        WasmBindingsError::Evaluate(format!("evaluate signature: {e}"))
                    })?;
                    let raw = typed
                        .call(&mut store, ())
                        .map_err(|e| WasmBindingsError::Evaluate(format!("evaluate call: {e}")))?;
                    if raw != 0 { 100 } else { 0 }
                }
                None => 0,
            },
        };

        let fuel_after = store
            .get_fuel()
            .map_err(|e| WasmBindingsError::Instantiate(format!("get fuel after: {e}")))?;
        let fuel_consumed = fuel_before - fuel_after;

        Ok(TypedPluginResult {
            success: score >= 50,
            score,
            output: serde_json::Value::Null,
            fuel_consumed,
            error: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wasm::runner::WasmRunner;

    #[test]
    fn test_typed_context_new() {
        let ctx = TypedPluginContext::new("hello");
        assert_eq!(ctx.input, "hello");
        assert_eq!(ctx.params, serde_json::Value::Null);
        assert_eq!(ctx.max_fuel, MAX_FUEL);
    }

    #[test]
    fn test_typed_context_with_params() {
        let params = serde_json::json!({"key": "value", "n": 42});
        let ctx = TypedPluginContext::new("input").with_params(params.clone());
        assert_eq!(ctx.params, params);
    }

    #[test]
    fn test_typed_context_with_max_fuel() {
        let ctx = TypedPluginContext::new("x").with_max_fuel(5_000_000);
        assert_eq!(ctx.max_fuel, 5_000_000);
    }

    #[test]
    fn test_typed_context_chaining() {
        let ctx = TypedPluginContext::new("chain")
            .with_params(serde_json::json!({"a": 1}))
            .with_max_fuel(1_000);
        assert_eq!(ctx.input, "chain");
        assert_eq!(ctx.max_fuel, 1_000);
        assert_eq!(ctx.params, serde_json::json!({"a": 1}));
    }

    #[test]
    fn test_call_evaluate_typed_with_evaluate_export() {
        let runner = WasmRunner::new().expect("engine");
        let module = runner
            .load_wat(r#"(module (func (export "evaluate") (result i32) i32.const 1))"#)
            .expect("module");
        let ctx = TypedPluginContext::new("test");
        let result = module.call_evaluate_typed(&ctx).expect("typed eval");
        assert!(result.success);
        assert_eq!(result.score, 100);
        assert!(result.fuel_consumed > 0);
        assert!(result.error.is_none());
    }

    #[test]
    fn test_call_evaluate_typed_with_evaluate_zero() {
        let runner = WasmRunner::new().expect("engine");
        let module = runner
            .load_wat(r#"(module (func (export "evaluate") (result i32) i32.const 0))"#)
            .expect("module");
        let ctx = TypedPluginContext::new("test");
        let result = module.call_evaluate_typed(&ctx).expect("typed eval");
        assert!(!result.success);
        assert_eq!(result.score, 0);
    }

    #[test]
    fn test_call_evaluate_typed_with_evaluate_scored_export() {
        let runner = WasmRunner::new().expect("engine");
        // Module exports evaluate_scored returning 75.
        let module = runner
            .load_wat(r#"(module (func (export "evaluate_scored") (result i32) i32.const 75))"#)
            .expect("module");
        let ctx = TypedPluginContext::new("scored");
        let result = module.call_evaluate_typed(&ctx).expect("typed eval");
        assert!(result.success, "score 75 >= 50 must be success");
        assert_eq!(result.score, 75);
    }

    #[test]
    fn test_call_evaluate_typed_scored_below_threshold() {
        let runner = WasmRunner::new().expect("engine");
        // Score 30 < 50 → failure.
        let module = runner
            .load_wat(r#"(module (func (export "evaluate_scored") (result i32) i32.const 30))"#)
            .expect("module");
        let ctx = TypedPluginContext::new("low");
        let result = module.call_evaluate_typed(&ctx).expect("typed eval");
        assert!(!result.success, "score 30 < 50 must be failure");
        assert_eq!(result.score, 30);
    }

    #[test]
    fn test_call_evaluate_typed_scored_clamped_high() {
        let runner = WasmRunner::new().expect("engine");
        // Score 200 must be clamped to 100.
        let module = runner
            .load_wat(r#"(module (func (export "evaluate_scored") (result i32) i32.const 200))"#)
            .expect("module");
        let ctx = TypedPluginContext::new("high");
        let result = module.call_evaluate_typed(&ctx).expect("typed eval");
        assert_eq!(result.score, 100, "score must be clamped to 100");
        assert!(result.success);
    }

    #[test]
    fn test_call_evaluate_typed_scored_clamped_negative() {
        let runner = WasmRunner::new().expect("engine");
        // Negative score must be clamped to 0.
        // In WAT, i32.const -5 is represented as a large unsigned value that wraps,
        // but clamp(0, 100) on i32 handles it: -5_i32.clamp(0, 100) == 0.
        let module = runner
            .load_wat(r#"(module (func (export "evaluate_scored") (result i32) i32.const -5))"#)
            .expect("module");
        let ctx = TypedPluginContext::new("neg");
        let result = module.call_evaluate_typed(&ctx).expect("typed eval");
        assert_eq!(result.score, 0, "negative score must clamp to 0");
        assert!(!result.success);
    }

    #[test]
    fn test_call_evaluate_typed_no_export_returns_zero() {
        let runner = WasmRunner::new().expect("engine");
        let module = runner.load_wat("(module)").expect("module");
        let ctx = TypedPluginContext::new("empty");
        let result = module.call_evaluate_typed(&ctx).expect("typed eval");
        assert!(!result.success);
        assert_eq!(result.score, 0);
        assert_eq!(result.output, serde_json::Value::Null);
    }

    #[test]
    fn test_call_evaluate_typed_prefers_scored_over_evaluate() {
        let runner = WasmRunner::new().expect("engine");
        // Module has both exports — evaluate_scored should take precedence.
        let module = runner
            .load_wat(
                r#"(module
                    (func (export "evaluate") (result i32) i32.const 1)
                    (func (export "evaluate_scored") (result i32) i32.const 42)
                )"#,
            )
            .expect("module");
        let ctx = TypedPluginContext::new("both");
        let result = module.call_evaluate_typed(&ctx).expect("typed eval");
        assert_eq!(
            result.score, 42,
            "evaluate_scored must take priority over evaluate"
        );
        assert!(!result.success, "score 42 < 50 must be failure");
    }

    #[test]
    fn test_typed_result_success_threshold_boundary() {
        // Score exactly 50 should be success.
        let runner = WasmRunner::new().expect("engine");
        let module = runner
            .load_wat(r#"(module (func (export "evaluate_scored") (result i32) i32.const 50))"#)
            .expect("module");
        let ctx = TypedPluginContext::new("boundary");
        let result = module.call_evaluate_typed(&ctx).expect("typed eval");
        assert!(result.success, "score == 50 must be success (>= 50)");
        assert_eq!(result.score, 50);
    }

    #[test]
    fn test_typed_result_success_threshold_below() {
        // Score 49 should be failure.
        let runner = WasmRunner::new().expect("engine");
        let module = runner
            .load_wat(r#"(module (func (export "evaluate_scored") (result i32) i32.const 49))"#)
            .expect("module");
        let ctx = TypedPluginContext::new("below");
        let result = module.call_evaluate_typed(&ctx).expect("typed eval");
        assert!(!result.success, "score 49 < 50 must be failure");
        assert_eq!(result.score, 49);
    }
}
