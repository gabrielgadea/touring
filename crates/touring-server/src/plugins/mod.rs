//! WASM Plugin System -- hot-loadable NLP extractors with security bounds.
//!
//! Security model:
//! - Fuel limits: 10M instructions per execution (prevents infinite loops)
//! - Stack limits: 1MB per WASM instance (prevents stack overflow)
//! - Import whitelist: only approved host functions can be called
//! - No filesystem, network, or system call access

#[cfg(feature = "wasm-plugins")]
pub mod runner;

#[cfg(feature = "wasm-plugins")]
pub use runner::WasmPluginRunner;
