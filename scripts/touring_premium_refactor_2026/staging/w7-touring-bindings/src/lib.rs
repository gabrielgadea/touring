//! `touring-bindings` — Language bindings for Touring (W7).
//!
//! Build with EXACTLY ONE language feature: `bind-py`, `bind-ts-napi`,
//! or `bind-wasm`. The default feature set is empty by design — callers
//! must opt in to the target language to avoid pulling unnecessary deps.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

#[cfg(feature = "common")]
pub mod common;

#[cfg(feature = "bind-py")]
pub mod py;

#[cfg(feature = "bind-ts-napi")]
pub mod ts;

#[cfg(feature = "bind-wasm")]
pub mod wasm;

/// Bindings crate version (semver-compatible with workspace).
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
