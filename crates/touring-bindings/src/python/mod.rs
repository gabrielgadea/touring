//! touring-python — PyO3 bindings for the Touring workspace.
//!
//! Exposes all workspace crates to Python as `claude_learning_kernel`.
//! Preserves backward compatibility with existing `scripts/aco/rust_bridge.py` imports.
//!
//! # Backward Compatibility Contract (CRITICAL)
//!
//! `rust_bridge.py` imports these symbols from `claude_learning_kernel.claude_learning_kernel`:
//!
//! ```python
//! # ACO core:
//! CRITICAL_WEIGHT, HALT_ITERATIONS, HALT_THRESHOLD, NORMAL_WEIGHT, VETO_THRESHOLD
//! AcoGraph, DimResult, TrackerReport
//! py_build_report, py_compute_composite, py_determine_status
//!
//! # ESAA:
//! EventProjector, verify_chain_parallel
//! ```
//!
//! ALL of these are registered in the top-level `claude_learning_kernel` module.

use pyo3::prelude::*;

mod aco_bindings;
mod ast_bindings;
mod ast_rl_bridge;
mod cognitive_bindings;
mod exceptions;
mod financial_bindings;
mod nlp_bindings;
mod rl_bindings;
mod rust_semantic_bindings;
mod simd_bindings;

/// Python module: claude_learning_kernel
///
/// High-performance Rust acceleration for the Learning System.
/// Preserves full backward compatibility with rust_bridge.py.
#[pymodule]
fn claude_learning_kernel(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add("__version__", "4.0.0")?;

    // ACO bindings — backward-compatible symbols + ESAA QueryCache/EventBuffer
    aco_bindings::register(m)?;

    // AST bindings — symbol extraction, complexity, syntax validation
    ast_bindings::register(m)?;

    // NLP bindings — tokenization, keyword extraction
    nlp_bindings::register(m)?;

    // SIMD bindings — cosine similarity, Wilson ranking, drift detection
    simd_bindings::register(m)?;

    // RL bindings — QTable, LinUCB bandit, state computation
    rl_bindings::register(m)?;

    // AST-RL bridge — compute RL state from file features
    ast_rl_bridge::register(m)?;

    // Cognitive bindings — MCTS search (v4.0)
    cognitive_bindings::register(m)?;

    // Financial bindings — NPV, IRR, concession analysis (v4.0)
    financial_bindings::register(m)?;

    // Wave 5 (2026-04-18) — Deep Rust semantic analysis via syn +
    // prettyplease + cargo_metadata + public-API extraction.
    rust_semantic_bindings::register(m)?;

    // Custom exceptions
    m.add(
        "AcoGraphError",
        m.py().get_type::<exceptions::AcoGraphError>(),
    )?;
    m.add(
        "AcoValidationError",
        m.py().get_type::<exceptions::AcoValidationError>(),
    )?;
    m.add(
        "AstParseError",
        m.py().get_type::<exceptions::AstParseError>(),
    )?;
    m.add(
        "AstSurgeryError",
        m.py().get_type::<exceptions::AstSurgeryError>(),
    )?;
    m.add(
        "SerializationError",
        m.py().get_type::<exceptions::SerializationError>(),
    )?;

    Ok(())
}
