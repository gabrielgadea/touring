//! Integration tests for the touring-analysis + touring-ast pipeline.
//!
//! Tests in `tests/` verify end-to-end wiring between:
//! - `AnalysisPipeline` (touring-analysis) consuming `SymbolIndex` (touring-ast)
//! - `adaptive_pool_size` sizing behaviour
//! - Pipeline builder contract: build → run returns a valid `CodeHealthReport`

#![deny(missing_docs)]
// RBP-01 elite-lint ratchet (2026-06-16): prod-unwrap-free — lock against future
// bare unwrap in non-test code (`.expect("…")` stays the sanctioned escape).
#![cfg_attr(not(test), deny(clippy::unwrap_used))]
