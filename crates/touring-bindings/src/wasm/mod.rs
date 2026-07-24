//! `touring-wasm` — WebAssembly plugin runtime PoC.
//!
//! Provides a sandboxed execution environment for WASM plugins via [`wasmtime`].
//!
//! # Architecture
//!
//! ```text
//! WasmRunner ─── load_wat / load_module ──► WasmModule
//!                                               │
//!                                         call_evaluate(PluginContext)
//!                                               │
//!                                         PluginResult { success, fuel_consumed, output }
//!
//! KvCacheManager ─── InMemoryCacheManager ──► WasmCacheManager (pooling)
//!      │                                           │
//!  get / insert                              fast_instantiation_config()
//!  invalidate / exists                       (wasmtime pooling + CoW memory)
//! ```
//!
//! # Security guarantees
//! - **Fuel limit**: max [`runner::MAX_FUEL`] instructions per call (no infinite loops).
//! - **Stack limit**: max [`runner::MAX_STACK_SIZE`] bytes per instance.
//! - **Import allowlist**: plugins may only declare imports explicitly whitelisted.
//! - **Sandbox**: wasmtime provides no host filesystem / network / syscall access.
//!
//! # Quick start
//! ```
//! use touring_bindings::wasm::{WasmRunner, PluginContext};
//!
//! let runner = WasmRunner::new().expect("engine creation failed");
//! let module = runner.load_wat(r#"(module
//!     (func (export "evaluate") (result i32) i32.const 1)
//! )"#).expect("load failed");
//!
//! let ctx = PluginContext::new("hello world");
//! let result = module.call_evaluate(&ctx).expect("execution failed");
//! assert!(result.success);
//! ```

pub mod cache_manager;
pub mod error;
pub mod plugin;
pub mod pool;
pub mod runner;
pub mod selector;
pub mod simd_embedding;
pub mod typed;

pub use error::WasmBindingsError;

// D35: Cloudflare Workers edge functions (wasm32-unknown-unknown target)
pub mod cloudflare;

// Re-exports
pub use cloudflare::{SymbolHit, find_symbols, render_graph_svg};

/// Serialized inferlet for persistent caching.
pub use inferlets::SerializedInferlet;

/// Cache management for WASM module execution.
///
/// # Re-exports
/// - [`AsyncInMemoryCacheManager`] — Async in-memory cache manager
/// - [`AsyncKvCacheManager`] — Async key-value cache manager
/// - [`CacheEntry`] — Individual cache entry with metadata
/// - [`InMemoryCacheManager`] — Synchronous in-memory cache
/// - [`KvCacheManager`] — Key-value cache manager trait
/// - [`WasmCacheManager`] — WASM-specific cache manager
pub use cache_manager::{
    AsyncInMemoryCacheManager, AsyncKvCacheManager, CacheEntry, InMemoryCacheManager,
    KvCacheManager, WasmCacheManager,
};

/// Fast instantiation configuration for pooling allocator.
#[cfg(feature = "pooling-allocator")]
pub use cache_manager::fast_instantiation_config;

/// Plugin execution context and results.
///
/// # Re-exports
/// - [`PluginContext`] — Execution context for WASM plugins
/// - [`PluginResult`] — Result of plugin execution
pub use plugin::{PluginContext, PluginResult};

/// WASM inferlet pool for efficient plugin reuse.
///
/// # Re-exports
/// - [`AsyncInferletPool`] — Async pool of pre-compiled WASM modules
/// - [`InferletPool`] — Synchronous inferlet pool
pub use pool::{AsyncInferletPool, InferletPool};

/// WASM module runner and execution limits.
///
/// # Re-exports
/// - [`WasmModule`] — Compiled WASM module wrapper
/// - [`WasmRunner`] — Runtime for executing WASM modules
/// - [`MAX_FUEL`] — Maximum fuel/instruction count for execution
/// - [`MAX_STACK_SIZE`] — Maximum stack size for WASM modules
pub use runner::{MAX_FUEL, MAX_STACK_SIZE, WasmModule, WasmRunner};

/// Context-aware plugin selection for dynamic routing.
///
/// # Re-exports
/// - [`ContextualPluginSelector`] — Selects plugins based on execution context
/// - [`SelectionContext`] — Context used for plugin selection
pub use selector::{ContextualPluginSelector, SelectionContext};

/// SIMD-accelerated embedding computation for WASM plugins.
///
/// # Re-exports
/// - [`compute`] — SIMD embedding computation function
/// - [`EmbeddingDoc`] — Document for embedding storage
/// - [`EmbeddingSearch`] — Similarity search over embeddings
/// - [`SearchHit`] — Single search result with score
pub use simd_embedding::{EmbeddingDoc, EmbeddingSearch, SearchHit, compute};

/// Typed plugin contexts with structured inputs/outputs.
///
/// # Re-exports
/// - [`TypedPluginContext`] — Plugin context with typed metadata
/// - [`TypedPluginResult`] — Result with typed output data
pub use typed::{TypedPluginContext, TypedPluginResult};
