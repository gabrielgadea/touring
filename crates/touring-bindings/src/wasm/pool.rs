//! `InferletPool` — async pool of pre-compiled WASM modules.
//!
//! Distributes concurrent evaluations across a fixed set of [`WasmModule`]
//! instances using a semaphore + deque pattern. Each `evaluate` call acquires
//! a permit, borrows a module, executes, and returns it to the pool.

use std::collections::VecDeque;
use std::sync::Arc;

use tokio::sync::{Mutex, Semaphore};
use wasmtime::{Engine, Store};

use crate::wasm::error::WasmBindingsError;
use crate::wasm::plugin::{PluginContext, PluginResult};
use crate::wasm::runner::{MAX_FUEL, WasmModule, WasmRunner};

/// Async pool of pre-compiled WASM modules for concurrent evaluation.
///
/// # Architecture
///
/// ```text
/// evaluate() ──► Semaphore::acquire ──► pop module ──► call_evaluate ──► push module back
/// ```
///
/// The pool holds `N` independently instantiable copies of the same compiled
/// module. The semaphore guarantees at most `N` concurrent evaluations; the
/// deque provides FIFO module reuse.
///
/// # Example
///
/// ```no_run
/// # async fn example() -> Result<(), String> {
/// use touring_bindings::wasm::{WasmRunner, InferletPool, PluginContext};
///
/// let runner = WasmRunner::new()?;
/// let wat = r#"(module (func (export "evaluate") (result i32) i32.const 1))"#;
/// let wasm = wat::parse_str(wat).map_err(|e| format!("{e}"))?;
/// let pool = InferletPool::new(&runner, &wasm, 4).await?;
///
/// let result = pool.evaluate(PluginContext::new("hello")).await?;
/// assert!(result.success);
/// # Ok(())
/// # }
/// ```
pub struct InferletPool {
    modules: Arc<Mutex<VecDeque<WasmModule>>>,
    semaphore: Arc<Semaphore>,
    pool_size: usize,
}

impl std::fmt::Debug for InferletPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("InferletPool")
            .field("pool_size", &self.pool_size)
            .field("idle_count", &self.idle_count())
            .finish()
    }
}

impl InferletPool {
    /// Create a pool with `pool_size` pre-compiled instances of the given WASM binary.
    ///
    /// Each instance is compiled independently via [`WasmRunner::load_module`].
    /// Returns an error if `pool_size` is zero or if compilation fails.
    pub async fn new(
        runner: &WasmRunner,
        wasm_bytes: &[u8],
        pool_size: usize,
    ) -> Result<Self, WasmBindingsError> {
        if pool_size == 0 {
            return Err(WasmBindingsError::Instantiate(
                "pool_size must be > 0".to_string(),
            ));
        }

        let mut deque = VecDeque::with_capacity(pool_size);
        for _ in 0..pool_size {
            deque.push_back(runner.load_module(wasm_bytes)?);
        }

        Ok(Self {
            modules: Arc::new(Mutex::new(deque)),
            semaphore: Arc::new(Semaphore::new(pool_size)),
            pool_size,
        })
    }

    /// Evaluate using an idle module from the pool.
    ///
    /// Blocks (async) until a module becomes available. The module is returned
    /// to the pool after execution regardless of success or failure.
    pub async fn evaluate(&self, ctx: PluginContext) -> Result<PluginResult, WasmBindingsError> {
        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|e| WasmBindingsError::Evaluate(format!("semaphore closed: {e}")))?;

        let module = {
            let mut guard = self.modules.lock().await;
            guard
                .pop_front()
                .ok_or_else(|| WasmBindingsError::Evaluate("pool unexpectedly empty".to_string()))?
        };

        let result = module.call_evaluate(&ctx);

        // Always return module to pool, even on evaluation failure.
        {
            let mut guard = self.modules.lock().await;
            guard.push_back(module);
        }

        result
    }

    /// Total number of modules in the pool (idle + in-use).
    pub fn pool_size(&self) -> usize {
        self.pool_size
    }

    /// Number of currently idle (available) modules.
    ///
    /// This is a snapshot — the value may change immediately after reading.
    pub fn idle_count(&self) -> usize {
        self.semaphore.available_permits()
    }
}

/// Async pool of pre-compiled WASM modules with true async API.
///
/// Uses `InstancePre` for pre-linked modules and `spawn_blocking` for execution.
/// A `Mutex<VecDeque>` provides correct concurrent access: each caller atomically
/// pops one instance, evaluates, then pushes it back.
pub struct AsyncInferletPool {
    instances: Arc<Mutex<std::collections::VecDeque<wasmtime::InstancePre<()>>>>,
    engine: Engine,
    semaphore: Arc<Semaphore>,
    pool_size: usize,
}

impl std::fmt::Debug for AsyncInferletPool {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AsyncInferletPool")
            .field("pool_size", &self.pool_size)
            .field("idle_count", &self.idle_count())
            .finish()
    }
}

impl AsyncInferletPool {
    /// Create a new async pool with `pool_size` pre-linked WASM instances.
    pub async fn new(
        runner: &WasmRunner,
        wasm_bytes: &[u8],
        pool_size: usize,
    ) -> Result<Self, WasmBindingsError> {
        if pool_size == 0 {
            return Err(WasmBindingsError::Instantiate(
                "pool_size must be > 0".to_string(),
            ));
        }

        let module = runner.load_module(wasm_bytes)?;
        let engine = module.engine().clone();
        let linker = wasmtime::Linker::<()>::new(&engine);

        let mut deque = std::collections::VecDeque::with_capacity(pool_size);
        for _ in 0..pool_size {
            let pre = linker
                .instantiate_pre(module.module_ref())
                .map_err(|e| WasmBindingsError::Instantiate(format!("instantiate_pre: {e}")))?;
            deque.push_back(pre);
        }

        Ok(Self {
            instances: Arc::new(Mutex::new(deque)),
            engine,
            semaphore: Arc::new(Semaphore::new(pool_size)),
            pool_size,
        })
    }

    /// Evaluate using an idle instance from the pool.
    ///
    /// Blocks (async) until a slot is available via the semaphore, then atomically
    /// pops one pre-linked instance, executes in `spawn_blocking`, and returns it.
    pub async fn evaluate(&self, ctx: PluginContext) -> Result<PluginResult, WasmBindingsError> {
        let _permit = self
            .semaphore
            .acquire()
            .await
            .map_err(|e| WasmBindingsError::Evaluate(format!("semaphore closed: {e}")))?;

        let instance_pre = {
            let mut guard = self.instances.lock().await;
            guard
                .pop_front()
                .ok_or_else(|| WasmBindingsError::Evaluate("pool unexpectedly empty".to_string()))?
        };

        let engine = self.engine.clone();
        let instance_pre_clone = instance_pre.clone();
        // Capture the input bytes before the blocking task.
        let input_bytes = ctx.input.as_bytes().to_vec();

        let (success, fuel_consumed) = tokio::task::spawn_blocking(move || {
            let mut store = Store::new(&engine, ());
            store
                .set_fuel(MAX_FUEL)
                .map_err(|e| WasmBindingsError::Instantiate(format!("set_fuel: {e}")))?;
            let fuel_before = store
                .get_fuel()
                .map_err(|e| WasmBindingsError::Instantiate(format!("get_fuel: {e}")))?;
            let instance = instance_pre_clone
                .instantiate(&mut store)
                .map_err(|e| WasmBindingsError::Instantiate(format!("instantiate: {e}")))?;

            // Write input to WASM memory and call set_input(ptr, len).
            // The WASM module uses a thread_local buffer; set_input stores the
            // pointer/length so evaluate() can read the bytes.
            let memory_export = instance.get_memory(&mut store, "memory");
            let set_input_fn = instance.get_func(&mut store, "set_input");
            if let (Some(memory), Some(set_input_fn)) = (memory_export, set_input_fn) {
                let offset = 0usize;
                let len = input_bytes.len();
                memory
                    .write(&mut store, offset, &input_bytes)
                    .map_err(|e| WasmBindingsError::Evaluate(format!("write memory: {e}")))?;
                let set_input_typed =
                    set_input_fn.typed::<(i32, i32), i32>(&store).map_err(|e| {
                        WasmBindingsError::Evaluate(format!("set_input signature: {e}"))
                    })?;
                set_input_typed
                    .call(&mut store, (offset as i32, len as i32))
                    .map_err(|e| WasmBindingsError::Evaluate(format!("set_input call: {e}")))?;
            }

            let success = match instance.get_func(&mut store, "evaluate") {
                Some(func) => {
                    let typed = func
                        .typed::<(), i32>(&store)
                        .map_err(|e| WasmBindingsError::Evaluate(format!("evaluate typed: {e}")))?;
                    typed
                        .call(&mut store, ())
                        .map_err(|e| WasmBindingsError::Evaluate(format!("call: {e}")))?
                        != 0
                }
                None => false,
            };
            let fuel_after = store
                .get_fuel()
                .map_err(|e| WasmBindingsError::Instantiate(format!("get_fuel after: {e}")))?;
            Ok::<(bool, u64), WasmBindingsError>((success, fuel_before - fuel_after))
        })
        .await
        .map_err(|e| WasmBindingsError::TaskJoin(format!("task join: {e}")))??;

        // Always return the instance to the pool.
        {
            let mut guard = self.instances.lock().await;
            guard.push_back(instance_pre);
        }

        if success {
            Ok(PluginResult::success(String::new(), fuel_consumed))
        } else {
            Ok(PluginResult::failure(fuel_consumed))
        }
    }

    /// Total pool capacity.
    pub fn pool_size(&self) -> usize {
        self.pool_size
    }

    /// Number of idle slots (available permits).
    pub fn idle_count(&self) -> usize {
        self.semaphore.available_permits()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wasm::plugin::PluginContext;
    use crate::wasm::runner::WasmRunner;

    /// Helper: compile WAT to WASM bytes.
    fn wat_to_wasm(wat: &str) -> Vec<u8> {
        wat::parse_str(wat).expect("test WAT must be valid")
    }

    fn success_wat() -> Vec<u8> {
        wat_to_wasm(r#"(module (func (export "evaluate") (result i32) i32.const 1))"#)
    }

    fn failure_wat() -> Vec<u8> {
        wat_to_wasm(r#"(module (func (export "evaluate") (result i32) i32.const 0))"#)
    }

    #[tokio::test]
    async fn test_pool_new_with_valid_module() {
        let runner = WasmRunner::new().expect("engine");
        let wasm = success_wat();
        let pool = InferletPool::new(&runner, &wasm, 4).await;
        assert!(pool.is_ok(), "pool creation must succeed");
    }

    #[tokio::test]
    async fn test_pool_new_zero_size_returns_error() {
        let runner = WasmRunner::new().expect("engine");
        let wasm = success_wat();
        let result = InferletPool::new(&runner, &wasm, 0).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("pool_size must be > 0"),
            "zero pool_size must be rejected"
        );
    }

    #[tokio::test]
    async fn test_pool_evaluate_returns_result() {
        let runner = WasmRunner::new().expect("engine");
        let wasm = success_wat();
        let pool = InferletPool::new(&runner, &wasm, 2).await.expect("pool");
        let result = pool
            .evaluate(PluginContext::new("test"))
            .await
            .expect("evaluate");
        assert!(result.success, "success module must return success");
    }

    #[tokio::test]
    async fn test_pool_evaluate_failure_module() {
        let runner = WasmRunner::new().expect("engine");
        let wasm = failure_wat();
        let pool = InferletPool::new(&runner, &wasm, 2).await.expect("pool");
        let result = pool
            .evaluate(PluginContext::new("test"))
            .await
            .expect("evaluate");
        assert!(!result.success, "failure module must return failure");
    }

    #[tokio::test]
    async fn test_pool_concurrent_evaluations() {
        // Spawn 4 concurrent tasks on a pool of size 2.
        // All must complete without deadlock.
        let runner = WasmRunner::new().expect("engine");
        let wasm = success_wat();
        let pool = Arc::new(InferletPool::new(&runner, &wasm, 2).await.expect("pool"));

        let mut handles = Vec::new();
        for i in 0..4 {
            let pool = Arc::clone(&pool);
            handles.push(tokio::spawn(async move {
                pool.evaluate(PluginContext::new(format!("concurrent-{i}")))
                    .await
            }));
        }

        for handle in handles {
            let result = handle.await.expect("task must not panic");
            assert!(result.is_ok(), "concurrent evaluate must succeed");
            assert!(result.expect("ok").success);
        }
    }

    #[tokio::test]
    async fn test_pool_size_matches_requested() {
        let runner = WasmRunner::new().expect("engine");
        let wasm = success_wat();
        let pool = InferletPool::new(&runner, &wasm, 3).await.expect("pool");
        assert_eq!(pool.pool_size(), 3);
    }

    #[tokio::test]
    async fn test_pool_module_returned_after_evaluate() {
        let runner = WasmRunner::new().expect("engine");
        let wasm = success_wat();
        let pool = InferletPool::new(&runner, &wasm, 2).await.expect("pool");

        assert_eq!(pool.idle_count(), 2);
        let _ = pool.evaluate(PluginContext::new("x")).await;
        // After evaluate completes, module should be back in pool.
        assert_eq!(pool.idle_count(), 2);
    }

    #[tokio::test]
    async fn test_pool_invalid_wasm_returns_error() {
        let runner = WasmRunner::new().expect("engine");
        let result = InferletPool::new(&runner, b"not valid wasm", 2).await;
        assert!(result.is_err(), "invalid wasm must fail pool creation");
    }

    #[tokio::test]
    async fn test_pool_size_1_sequential() {
        let runner = WasmRunner::new().expect("engine");
        let wasm = success_wat();
        let pool = InferletPool::new(&runner, &wasm, 1).await.expect("pool");

        for _ in 0..3 {
            let result = pool
                .evaluate(PluginContext::new("seq"))
                .await
                .expect("evaluate");
            assert!(result.success);
        }
        assert_eq!(pool.idle_count(), 1);
    }
}

#[cfg(test)]
mod async_tests {
    use super::*;

    fn wat_success() -> Vec<u8> {
        wat::parse_str(r#"(module (func (export "evaluate") (result i32) i32.const 1))"#)
            .expect("valid WAT")
    }

    fn wat_failure() -> Vec<u8> {
        wat::parse_str(r#"(module (func (export "evaluate") (result i32) i32.const 0))"#)
            .expect("valid WAT")
    }

    #[tokio::test]
    async fn test_async_pool_new_valid() {
        let runner = WasmRunner::new().expect("runner");
        let pool = AsyncInferletPool::new(&runner, &wat_success(), 2).await;
        assert!(pool.is_ok());
        let pool = pool.unwrap();
        assert_eq!(pool.pool_size(), 2);
        assert_eq!(pool.idle_count(), 2);
    }

    #[tokio::test]
    async fn test_async_pool_zero_size_error() {
        let runner = WasmRunner::new().expect("runner");
        let result = AsyncInferletPool::new(&runner, &wat_success(), 0).await;
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("pool_size must be > 0")
        );
    }

    #[tokio::test]
    async fn test_async_pool_evaluate_success() {
        let runner = WasmRunner::new().expect("runner");
        let pool = AsyncInferletPool::new(&runner, &wat_success(), 2)
            .await
            .expect("pool");
        let result = pool.evaluate(PluginContext::new("test")).await;
        assert!(result.is_ok());
        assert!(result.unwrap().success);
    }

    #[tokio::test]
    async fn test_async_pool_evaluate_failure() {
        let runner = WasmRunner::new().expect("runner");
        let pool = AsyncInferletPool::new(&runner, &wat_failure(), 2)
            .await
            .expect("pool");
        let result = pool
            .evaluate(PluginContext::new("test"))
            .await
            .expect("eval");
        assert!(!result.success);
    }

    #[tokio::test]
    async fn test_async_pool_concurrent() {
        let runner = WasmRunner::new().expect("runner");
        let pool = Arc::new(
            AsyncInferletPool::new(&runner, &wat_success(), 2)
                .await
                .expect("pool"),
        );

        let mut handles = Vec::new();
        for i in 0..4 {
            let pool = Arc::clone(&pool);
            handles.push(tokio::spawn(async move {
                pool.evaluate(PluginContext::new(format!("concurrent-{}", i)))
                    .await
            }));
        }

        for handle in handles {
            let result = handle.await.expect("task not panicked");
            assert!(result.is_ok());
            assert!(result.unwrap().success);
        }
    }

    #[tokio::test]
    async fn test_async_pool_idle_count_recovers() {
        let runner = WasmRunner::new().expect("runner");
        let pool = AsyncInferletPool::new(&runner, &wat_success(), 2)
            .await
            .expect("pool");
        assert_eq!(pool.idle_count(), 2);
        let _ = pool.evaluate(PluginContext::new("x")).await;
        assert_eq!(pool.idle_count(), 2);
    }

    #[tokio::test]
    async fn test_async_pool_size_1_sequential() {
        let runner = WasmRunner::new().expect("runner");
        let pool = AsyncInferletPool::new(&runner, &wat_success(), 1)
            .await
            .expect("pool");
        for _ in 0..3 {
            let r = pool
                .evaluate(PluginContext::new("seq"))
                .await
                .expect("eval");
            assert!(r.success);
        }
        assert_eq!(pool.idle_count(), 1);
    }
}
