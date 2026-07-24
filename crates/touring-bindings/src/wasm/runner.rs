//! `WasmRunner` — sandboxed WebAssembly execution engine.
//!
//! Provides a PoC for loading and running WASM plugins with:
//! - Fuel metering (10M instruction cap — prevents infinite loops)
//! - Stack size limit (1MB — prevents stack overflow)
//! - Import whitelist validation (rejects unauthorized host calls)
//! - No filesystem, network, or system-call access (wasmtime default sandbox)

use std::collections::HashSet;

use wasmtime::{Config, Engine, Instance, InstanceAllocationStrategy, Module, Store};

use crate::wasm::error::WasmBindingsError;
use crate::wasm::plugin::{PluginContext, PluginResult};

/// Maximum fuel (instruction units) per plugin execution.
pub const MAX_FUEL: u64 = 10_000_000;

/// Maximum WASM stack size in bytes (1 MB).
pub const MAX_STACK_SIZE: usize = 1024 * 1024;

/// Sandboxed WebAssembly plugin runner.
///
/// Each [`WasmRunner`] owns a single `wasmtime` [`Engine`] and an import
/// whitelist. Modules are compiled on-demand via `load_module` and executed
/// via [`WasmModule::call_evaluate`].
///
/// # Security model
/// - Fuel: execution halts after [`MAX_FUEL`] instructions (no runaway code).
/// - Stack: limited to [`MAX_STACK_SIZE`] bytes.
/// - Imports: only functions explicitly added via `allow_import` may be
///   declared by a plugin; any other import causes an immediate error.
/// - Sandbox: wasmtime provides no host filesystem/network/syscall access by default.
///
/// # Example
/// ```
/// use touring_bindings::wasm::WasmRunner;
///
/// let runner = WasmRunner::new().expect("engine creation failed");
/// let _module = runner.load_wat(r#"(module)"#).expect("load failed");
/// ```
pub struct WasmRunner {
    engine: Engine,
    allowed_imports: HashSet<String>,
}

/// A compiled WASM module ready for execution.
///
/// Obtained via [`WasmRunner::load_module`] or [`WasmRunner::load_wat`].
#[derive(Debug)]
pub struct WasmModule {
    module: Module,
    engine: Engine,
    allowed_imports: HashSet<String>,
}

impl WasmRunner {
    /// Create a new runner with security defaults.
    ///
    /// Default import whitelist: `env::log`, `env::get_config`.
    pub fn new() -> Result<Self, WasmBindingsError> {
        let mut config = Config::new();
        config.consume_fuel(true);
        config.max_wasm_stack(MAX_STACK_SIZE);
        config.allocation_strategy(InstanceAllocationStrategy::Pooling(Default::default()));

        let engine = Engine::new(&config)
            .map_err(|e| WasmBindingsError::Engine(format!("Failed to create WASM engine: {e}")))?;

        let mut allowed_imports = HashSet::new();
        allowed_imports.insert("env::log".to_string());
        allowed_imports.insert("env::get_config".to_string());

        Ok(Self {
            engine,
            allowed_imports,
        })
    }

    /// Create a new runner without pooling allocator (on-demand allocation).
    ///
    /// Identical to [`WasmRunner::new`] but skips `InstanceAllocationStrategy::Pooling`,
    /// which can fail when the system pooling allocator is unavailable (e.g., in some
    /// CI environments or when memory limits prevent pool pre-allocation).
    ///
    /// Prefer this as a fallback when `new()` fails.
    pub fn new_on_demand() -> Result<Self, WasmBindingsError> {
        let mut config = Config::new();
        config.consume_fuel(true);
        config.max_wasm_stack(MAX_STACK_SIZE);
        // No pooling strategy — wasmtime defaults to on-demand allocation.

        let engine = Engine::new(&config).map_err(|e| {
            WasmBindingsError::Engine(format!("Failed to create WASM engine (on-demand): {e}"))
        })?;

        let mut allowed_imports = HashSet::new();
        allowed_imports.insert("env::log".to_string());
        allowed_imports.insert("env::get_config".to_string());

        Ok(Self {
            engine,
            allowed_imports,
        })
    }

    /// Create a sentinel runner that always succeeds using only `Engine::default()`.
    ///
    /// This is a last-resort fallback when both [`WasmRunner::new`] and
    /// [`WasmRunner::new_on_demand`] fail (e.g., wasmtime unavailable in a
    /// sandboxed environment). The sentinel runner will fail gracefully at
    /// module-load time rather than panicking at service initialization.
    pub fn new_sentinel() -> Self {
        let mut allowed_imports = HashSet::new();
        allowed_imports.insert("env::log".to_string());
        allowed_imports.insert("env::get_config".to_string());
        Self {
            engine: Engine::default(),
            allowed_imports,
        }
    }

    /// Add an import to the allowlist (format: `"module::function"`).
    pub fn allow_import(&mut self, import_name: &str) {
        self.allowed_imports.insert(import_name.to_string());
    }

    /// Return the current import allowlist.
    pub fn allowed_imports(&self) -> &HashSet<String> {
        &self.allowed_imports
    }

    /// Compile a WASM module from raw bytes.
    ///
    /// Returns a [`WasmModule`] ready for execution.
    /// The module's imports are validated against the allowlist immediately.
    pub fn load_module(&self, wasm_bytes: &[u8]) -> Result<WasmModule, WasmBindingsError> {
        let module = Module::from_binary(&self.engine, wasm_bytes)
            .map_err(|e| WasmBindingsError::ModuleLoad(format!("Invalid WASM module: {e}")))?;
        self.check_imports(&module)?;
        Ok(WasmModule {
            module,
            engine: self.engine.clone(),
            allowed_imports: self.allowed_imports.clone(),
        })
    }

    /// Compile a WASM module from WAT (WebAssembly Text Format) source.
    ///
    /// Parses WAT to binary and delegates to [`WasmRunner::load_module`].
    /// Primarily useful for tests and prototyping.
    pub fn load_wat(&self, wat_source: &str) -> Result<WasmModule, WasmBindingsError> {
        let wasm_bytes = wat::parse_str(wat_source)
            .map_err(|e| WasmBindingsError::ModuleLoad(format!("Invalid WAT: {e}")))?;
        self.load_module(&wasm_bytes)
    }

    /// Validate that every import in `module` is present in the allowlist.
    fn check_imports(&self, module: &Module) -> Result<(), WasmBindingsError> {
        for import in module.imports() {
            let full_name = format!("{}::{}", import.module(), import.name());
            if !self.allowed_imports.contains(&full_name) {
                return Err(WasmBindingsError::ModuleLoad(format!(
                    "WASM import '{}' not in allowlist. Allowed: {:?}",
                    full_name, self.allowed_imports
                )));
            }
        }
        Ok(())
    }
}

impl WasmModule {
    /// Execute the module's `evaluate` export with the given context.
    ///
    /// The WASM module is expected to export:
    /// - `set_input(ptr: i32, len: i32) -> i32` — stores input in thread_local
    /// - `evaluate() -> i32` — reads stored input and dispatches to inferlet
    ///
    /// If the WASM module does not export memory, the input is silently dropped
    /// and `evaluate()` is called with no input (backward compat for simple modules).
    ///
    /// Fuel is reset to [`MAX_FUEL`] on each call.
    pub fn call_evaluate(&self, ctx: &PluginContext) -> Result<PluginResult, WasmBindingsError> {
        tracing::debug!(input = %ctx.input, "WasmModule::call_evaluate");

        let mut store = Store::new(&self.engine, ());
        store
            .set_fuel(MAX_FUEL)
            .map_err(|e| WasmBindingsError::Instantiate(format!("Failed to set fuel: {e}")))?;

        let instance = Instance::new(&mut store, &self.module, &[]).map_err(|e| {
            WasmBindingsError::Instantiate(format!("Failed to instantiate WASM: {e}"))
        })?;

        let fuel_before = store
            .get_fuel()
            .map_err(|e| WasmBindingsError::Instantiate(format!("Failed to get fuel: {e}")))?;

        // Write ctx.input to WASM memory and call set_input(ptr, len).
        let input_bytes = ctx.input.as_bytes();
        if let (Some(memory), Some(set_input_fn)) = (
            instance.get_memory(&mut store, "memory"),
            instance.get_func(&mut store, "set_input"),
        ) {
            let offset = 0usize;
            let len = input_bytes.len();
            memory
                .write(&mut store, offset, input_bytes)
                .map_err(|e| WasmBindingsError::Instantiate(format!("write memory: {e}")))?;
            let set_input_typed = set_input_fn
                .typed::<(i32, i32), i32>(&store)
                .map_err(|e| WasmBindingsError::Instantiate(format!("set_input signature: {e}")))?;
            set_input_typed
                .call(&mut store, (offset as i32, len as i32))
                .map_err(|e| WasmBindingsError::Instantiate(format!("set_input call: {e}")))?;
        }

        let success = match instance.get_func(&mut store, "evaluate") {
            Some(func) => {
                let typed = func.typed::<(), i32>(&store).map_err(|e| {
                    WasmBindingsError::Evaluate(format!("evaluate has wrong signature: {e}"))
                })?;
                let result = typed.call(&mut store, ()).map_err(|e| {
                    WasmBindingsError::Evaluate(format!("evaluate call failed: {e}"))
                })?;
                result != 0
            }
            None => false,
        };

        let fuel_after = store.get_fuel().map_err(|e| {
            WasmBindingsError::Instantiate(format!("Failed to get fuel after: {e}"))
        })?;
        let fuel_consumed = fuel_before - fuel_after;

        if success {
            Ok(PluginResult::success(String::new(), fuel_consumed))
        } else {
            Ok(PluginResult::failure(fuel_consumed))
        }
    }

    /// Return whether the module exports a function with the given name.
    ///
    /// Uses a temporary store for introspection (no execution side effects).
    pub fn has_export(&self, name: &str) -> bool {
        let mut store = Store::new(&self.engine, ());
        match Instance::new(&mut store, &self.module, &[]) {
            Ok(instance) => instance.get_func(&mut store, name).is_some(),
            Err(_) => false,
        }
    }

    /// Return the current import allowlist inherited from the runner.
    pub fn allowed_imports(&self) -> &HashSet<String> {
        &self.allowed_imports
    }

    /// Return a reference to the underlying `wasmtime::Engine`.
    pub fn engine(&self) -> &Engine {
        &self.engine
    }

    /// Return a reference to the compiled `wasmtime::Module`.
    pub fn module_ref(&self) -> &Module {
        &self.module
    }

    /// Execute the module's `evaluate` export asynchronously.
    ///
    /// Uses spawn_blocking for CPU-bound WASM execution within async context.
    /// Fuel is reset to [`MAX_FUEL`] on each call.
    ///
    /// Writes ctx.input to WASM memory and calls set_input before evaluate.
    pub async fn call_evaluate_async(
        &self,
        ctx: &PluginContext,
    ) -> Result<PluginResult, WasmBindingsError> {
        let span = tracing::debug_span!("wasm_evaluate_async", input = %ctx.input);
        let _guard = span.enter();

        let engine = self.engine.clone();
        let module = self.module.clone();
        let input_bytes = ctx.input.as_bytes().to_vec();

        tokio::task::spawn_blocking(move || {
            let mut store = Store::new(&engine, ());
            store
                .set_fuel(MAX_FUEL)
                .map_err(|e| WasmBindingsError::Instantiate(format!("set_fuel: {e}")))?;

            let fuel_before = store
                .get_fuel()
                .map_err(|e| WasmBindingsError::Instantiate(format!("get_fuel: {e}")))?;

            let instance = Instance::new(&mut store, &module, &[])
                .map_err(|e| WasmBindingsError::Instantiate(format!("instantiate: {e}")))?;

            // Write input to WASM memory and call set_input(ptr, len).
            if let (Some(memory), Some(set_input_fn)) = (
                instance.get_memory(&mut store, "memory"),
                instance.get_func(&mut store, "set_input"),
            ) {
                let offset = 0usize;
                let len = input_bytes.len();
                memory
                    .write(&mut store, offset, &input_bytes)
                    .map_err(|e| WasmBindingsError::Instantiate(format!("write memory: {e}")))?;
                let set_input_typed =
                    set_input_fn.typed::<(i32, i32), i32>(&store).map_err(|e| {
                        WasmBindingsError::Instantiate(format!("set_input signature: {e}"))
                    })?;
                set_input_typed
                    .call(&mut store, (offset as i32, len as i32))
                    .map_err(|e| WasmBindingsError::Instantiate(format!("set_input call: {e}")))?;
            }

            let success = match instance.get_func(&mut store, "evaluate") {
                Some(func) => {
                    let typed = func.typed::<(), i32>(&store).map_err(|e| {
                        WasmBindingsError::Evaluate(format!("evaluate signature: {e}"))
                    })?;
                    let result = typed
                        .call(&mut store, ())
                        .map_err(|e| WasmBindingsError::Evaluate(format!("evaluate call: {e}")))?;
                    result != 0
                }
                None => false,
            };

            let fuel_after = store
                .get_fuel()
                .map_err(|e| WasmBindingsError::Instantiate(format!("get_fuel after: {e}")))?;
            let fuel_consumed = fuel_before - fuel_after;

            if success {
                Ok(PluginResult::success(String::new(), fuel_consumed))
            } else {
                Ok(PluginResult::failure(fuel_consumed))
            }
        })
        .await
        .map_err(|e| WasmBindingsError::TaskJoin(format!("task join: {e}")))?
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wasm::plugin::PluginContext;

    // ── Construction ─────────────────────────────────────────────────────────

    #[test]
    fn test_wasm_runner_new() {
        let runner = WasmRunner::new();
        assert!(runner.is_ok(), "WasmRunner::new() must succeed");
    }

    #[test]
    fn test_pooling_allocator_functional() {
        // Pooling allocator must be active — engine creation and module execution must succeed.
        let runner = WasmRunner::new().expect("engine with pooling allocator must succeed");
        let module = runner
            .load_wat(r#"(module (func (export "evaluate") (result i32) i32.const 1))"#)
            .expect("module must load with pooling allocator");
        let ctx = PluginContext::new("pooling test");
        let result = module.call_evaluate(&ctx).expect("evaluate must succeed");
        assert!(
            result.success,
            "pooling allocator must not break module execution"
        );
    }

    #[test]
    fn test_default_allowlist_contains_env_log() {
        let runner = WasmRunner::new().unwrap();
        assert!(runner.allowed_imports().contains("env::log"));
        assert!(runner.allowed_imports().contains("env::get_config"));
        assert!(!runner.allowed_imports().contains("evil::hack"));
    }

    #[test]
    fn test_allow_import_adds_entry() {
        let mut runner = WasmRunner::new().unwrap();
        assert!(!runner.allowed_imports().contains("custom::func"));
        runner.allow_import("custom::func");
        assert!(runner.allowed_imports().contains("custom::func"));
    }

    // ── WAT module loading ────────────────────────────────────────────────────

    #[test]
    fn test_load_wat_empty_module() {
        let runner = WasmRunner::new().unwrap();
        let result = runner.load_wat("(module)");
        assert!(result.is_ok(), "Empty module must load without error");
    }

    #[test]
    fn test_load_wat_invalid_source_returns_error() {
        let runner = WasmRunner::new().unwrap();
        let result = runner.load_wat("(not valid wat !!!");
        assert!(result.is_err(), "Invalid WAT must return an error");
    }

    #[test]
    fn test_load_module_invalid_bytes_returns_error() {
        let runner = WasmRunner::new().unwrap();
        let result = runner.load_module(b"not wasm bytes");
        assert!(result.is_err(), "Invalid bytes must return an error");
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("Invalid WASM module"),
            "Expected 'Invalid WASM module', got: {err}"
        );
    }

    // ── Import allowlist ──────────────────────────────────────────────────────

    #[test]
    fn test_import_not_in_allowlist_is_rejected() {
        let runner = WasmRunner::new().unwrap();
        let wat = r#"(module (import "evil" "hack" (func)))"#;
        let result = runner.load_wat(wat);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("not in allowlist"),
            "Expected allowlist error, got: {msg}"
        );
    }

    #[test]
    fn test_sandbox_no_filesystem_access() {
        // A module that imports WASI filesystem function must be rejected.
        let runner = WasmRunner::new().unwrap();
        let wat = r#"(module
            (import "wasi_snapshot_preview1" "fd_write"
                (func (param i32 i32 i32 i32) (result i32)))
        )"#;
        let result = runner.load_wat(wat);
        assert!(result.is_err());
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("not in allowlist"),
            "Filesystem import must be blocked: {msg}"
        );
    }

    // ── call_evaluate ─────────────────────────────────────────────────────────

    #[test]
    fn test_call_evaluate_returns_true_for_always_true_module() {
        let runner = WasmRunner::new().unwrap();
        let wat = r#"(module
            (func (export "evaluate") (result i32)
                i32.const 1
            )
        )"#;
        let module = runner.load_wat(wat).unwrap();
        let ctx = PluginContext::new("test input");
        let result = module.call_evaluate(&ctx).unwrap();
        assert!(result.success, "evaluate() returning 1 must be success");
    }

    #[test]
    fn test_call_evaluate_returns_false_for_always_false_module() {
        let runner = WasmRunner::new().unwrap();
        let wat = r#"(module
            (func (export "evaluate") (result i32)
                i32.const 0
            )
        )"#;
        let module = runner.load_wat(wat).unwrap();
        let ctx = PluginContext::new("test input");
        let result = module.call_evaluate(&ctx).unwrap();
        assert!(!result.success, "evaluate() returning 0 must be failure");
    }

    #[test]
    fn test_call_evaluate_no_export_returns_failure() {
        let runner = WasmRunner::new().unwrap();
        // Module has no `evaluate` export — must return failure gracefully
        let module = runner.load_wat("(module)").unwrap();
        let ctx = PluginContext::new("input");
        let result = module.call_evaluate(&ctx).unwrap();
        assert!(!result.success, "Missing evaluate export must be failure");
    }

    // ── Multiple independent modules ──────────────────────────────────────────

    #[test]
    fn test_multiple_modules_are_independent() {
        let runner = WasmRunner::new().unwrap();

        let mod_true = runner
            .load_wat(r#"(module (func (export "evaluate") (result i32) i32.const 1))"#)
            .unwrap();
        let mod_false = runner
            .load_wat(r#"(module (func (export "evaluate") (result i32) i32.const 0))"#)
            .unwrap();

        let ctx = PluginContext::new("x");
        assert!(mod_true.call_evaluate(&ctx).unwrap().success);
        assert!(!mod_false.call_evaluate(&ctx).unwrap().success);
    }

    // ── Fuel metering ─────────────────────────────────────────────────────────

    #[test]
    fn test_fuel_consumed_tracked() {
        let runner = WasmRunner::new().unwrap();
        // A module that does arithmetic — fuel should be > 0
        let wat = r#"(module
            (func (export "evaluate") (result i32)
                i32.const 42
                i32.const 58
                i32.add
                drop
                i32.const 1
            )
        )"#;
        let module = runner.load_wat(wat).unwrap();
        let ctx = PluginContext::new("");
        let result = module.call_evaluate(&ctx).unwrap();
        assert!(
            result.fuel_consumed > 0,
            "Fuel must be consumed by execution"
        );
    }

    // ── has_export ────────────────────────────────────────────────────────────

    #[test]
    fn test_has_export_true_for_present_function() {
        let runner = WasmRunner::new().unwrap();
        let module = runner
            .load_wat(r#"(module (func (export "evaluate") (result i32) i32.const 1))"#)
            .unwrap();
        assert!(module.has_export("evaluate"));
        assert!(!module.has_export("nonexistent"));
    }

    // ── Module with memory ────────────────────────────────────────────────────

    #[test]
    fn test_module_with_memory_loads_ok() {
        let runner = WasmRunner::new().unwrap();
        let module = runner.load_wat(r#"(module (memory (export "memory") 1))"#);
        assert!(module.is_ok(), "Module with memory must load successfully");
    }
}
