//! `inferlets_assets` — Compile-time embedded WASM inferlet binary.
//!
//! This module embeds the compiled inferlet WASM binary via `include_bytes!()`.
//! The binary is built by `cargo build --target wasm32-wasip1 --release -p inferlets`
//! and copied to `wasm_bytes/` by the build.rs script.
//!
//! # Architecture
//!
//! All four inferlets (always_success, memory, pattern, classifier) are
//! compiled into a SINGLE WASM module with a single `evaluate()` entry point
//! that dispatches internally based on the `__inferlet__` key in the input JSON.
//!
//! # Input Format
//!
//! ```json
//! { "__inferlet__": "memory", "data": { ... } }
//! ```
//!
//! # Building WASM inferlets
//!
//! ```bash
//! cargo build --target wasm32-wasip1 --release -p inferlets
//! ```
//!
//! This produces `libinferlets.wasm` in `inferlets/wasm_bytes/`.
//!
//! # Design
//!
//! The WASM binary is embedded at compile time so the daemon binary is
//! self-contained. If not yet built, the module compiles with empty bytes
//! and loading fails at runtime with a clear error.

/// Embedded unified inferlet WASM binary.
///
/// Contains all 4 inferlets (always_success, memory, pattern, classifier)
/// with internal dispatch via `__inferlet__` key.
pub const INFERLET_WASM: &[u8] = include_bytes!("../../inferlets/wasm_bytes/libinferlets.wasm");

use crate::inferlets::{InferletKind, InferletService};

/// Load all compiled inferlets into an `InferletService`.
///
/// Uses the embedded WASM binary which dispatches internally to the
/// appropriate inferlet based on the `__inferlet__` key in the input.
///
/// Each inferlet kind is mapped to the unified WASM binary and loaded
/// into its own `AsyncInferletPool` with the given `pool_size`.
pub async fn load_all_inferlets(service: &InferletService, pool_size: usize) -> Result<(), String> {
    let wasm_bytes = INFERLET_WASM;

    if wasm_bytes.is_empty() {
        return Err("inferlets WASM binary is empty — run `cargo build --target wasm32-wasip1 --release -p inferlets` first".to_string());
    }

    // All 4 kinds share the same WASM binary (dispatched internally)
    service
        .load_inferlet(InferletKind::AlwaysSuccess, wasm_bytes, pool_size)
        .await
        .map_err(|e| format!("always_success: {e}"))?;

    service
        .load_inferlet(InferletKind::Memory, wasm_bytes, pool_size)
        .await
        .map_err(|e| format!("memory: {e}"))?;

    service
        .load_inferlet(InferletKind::Pattern, wasm_bytes, pool_size)
        .await
        .map_err(|e| format!("pattern: {e}"))?;

    service
        .load_inferlet(InferletKind::Classifier, wasm_bytes, pool_size)
        .await
        .map_err(|e| format!("classifier: {e}"))?;

    Ok(())
}
