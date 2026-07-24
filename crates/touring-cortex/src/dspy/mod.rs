//! DSPy-inspired prompt compilation API for Rust.
//!
//! This module provides a DSPy-compatible interface for prompt optimization:
//! - **Signature**: Declarative input/output field definitions
//! - **Module**: Program transformation with signature-aware reasoning
//! - **Teleprompter**: Automatic prompt optimization from demonstrations
//!
//! ## Feature Gating
//!
//! When `dspy` feature is enabled, full DSPy compilation is available.
//! When disabled, heuristic optimization provides best-effort results.

// Re-export DSPy types for external use.
pub use dspy_compiler::DspyCompiler;
pub use dspy_module::{DspyModule, ModuleResult};
pub use dspy_signature::{
    DspySignature, SignatureField, code_generation_sig, code_reflection_sig, test_generation_sig,
};
pub use dspy_teleprompter::{
    BootstrapFewShot, CompiledPrompt, Demo, MCTSTeleprompter, Teleprompter,
};

mod dspy_compiler;
mod dspy_module;
mod dspy_signature;
mod dspy_teleprompter;
#[cfg(test)]
mod e2e_audit;
