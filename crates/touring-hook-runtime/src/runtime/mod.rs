//! HookRuntime — public traits for extensibility.
//!
//! This module re-exports all traits from the concrete HookRuntime implementation
//! in the parent module (hook_runtime.rs).

pub mod traits;

// ── Trait implementations (split by domain for maintainability) ─────────
mod impls_aco;
mod impls_cognitive;
mod impls_context;
mod impls_hook;
mod impls_knowledge;
mod impls_rl;
mod impls_symbols;

pub use traits::*;

// Re-export concrete types from hook_runtime for backward compatibility
pub use crate::hook_runtime::{HookResponse, HookRuntime, HookTimer, hash_str, make_relative};
