//! touring-python — the `claude_learning_kernel` PyO3 module (the cdylib).
//!
//! All implementation lives in `touring-bindings` (feature `bind-python`);
//! the `#[pymodule]` invocation lives HERE, because a cdylib only exports
//! symbols of its own crate: while the invocation stayed in the dependency's
//! rlib and this shim referenced nothing, the linker dropped the whole rlib
//! and the `.so` shipped with ZERO `PyInit` symbols (measured 24/09/2026 —
//! every import died with "dynamic module does not define module export
//! function", and the analise lost Wilson, drift and py_parse_monetary since
//! 23/08).

#![deny(missing_docs)]
// RBP-01 elite-lint ratchet (2026-06-16): prod-unwrap-free — lock against future
// bare unwrap in non-test code (`.expect("…")` stays the sanctioned escape).
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

use pyo3::prelude::*;

/// Python module: `claude_learning_kernel` — high-performance Rust
/// acceleration for the Learning System. The registration contract
/// (backward compatibility with `scripts/aco/rust_bridge.py`) lives in
/// [`touring_bindings::python::register_all`].
#[pymodule]
fn claude_learning_kernel(m: &Bound<'_, PyModule>) -> PyResult<()> {
    touring_bindings::python::register_all(m)
}
