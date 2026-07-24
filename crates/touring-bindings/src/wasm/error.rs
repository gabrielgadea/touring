//! Typed error for the WASM plugin runtime (RBP-03).

use thiserror::Error;

/// Error returned by the WASM plugin runtime ([`super::runner::WasmRunner`],
/// [`super::pool`], typed evaluation). Replaces the previous stringly-typed
/// `Result<_, String>`; `Display` is preserved byte-for-byte via the phase
/// message carried in each variant so existing logs read identically.
#[derive(Debug, Error)]
pub enum WasmBindingsError {
    /// WASM engine creation failed.
    #[error("{0}")]
    Engine(String),
    /// WASM module / WAT failed to load, validate, or declared a forbidden import.
    #[error("{0}")]
    ModuleLoad(String),
    /// Plugin instantiation, fuel, or memory setup failed.
    #[error("{0}")]
    Instantiate(String),
    /// Plugin evaluation call failed.
    #[error("{0}")]
    Evaluate(String),
    /// Async task join failed.
    #[error("{0}")]
    TaskJoin(String),
}

impl From<WasmBindingsError> for String {
    fn from(e: WasmBindingsError) -> Self {
        e.to_string()
    }
}
