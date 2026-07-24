//! Typed error enum for `touring-server` public APIs.
//!
//! Replaces stringly-typed `Result<_, String>` in the four public functions
//! that were previously using raw strings as error carriers.  All variants
//! carry the original human-readable message string so that [`std::fmt::Display`]
//! output is identical to the previous string — callers that call `.to_string()`
//! or embed the error in JSON see the same text.

/// Crate-level typed error for `touring-server` public APIs.
///
/// Each variant corresponds to one logical failure domain.  The inner
/// `String` preserves the original human-readable message so `Display`
/// output is identical to the previous raw-string errors.
#[derive(Debug, thiserror::Error)]
pub enum ServerError {
    /// Failed to parse a generator plan (JSON / schema).
    #[error("{0}")]
    PlanParse(String),
    /// Clone-detection analysis failed.
    #[error("{0}")]
    CloneDetect(String),
    /// Filesystem watcher / ingest poll failed.
    #[error("{0}")]
    Watcher(String),
    /// WASM plugin runner initialization failed.
    #[error("{0}")]
    WasmInit(String),
}
