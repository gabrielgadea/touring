//! Shared test utilities for `touring-learning` unit and integration tests.
//!
//! Tier A (2026-04-19): thin prelude that re-exports `pretty_assertions`
//! shadow-macros so every `#[cfg(test)] mod tests` can pull the whole set
//! via `use crate::test_util::*;`.
//!
//! Gated behind `#[cfg(test)]` at the declaration site in `lib.rs`.

// `assert_ne` re-exported alongside `assert_eq` for symmetry — callers may
// need either without importing two paths. `#[allow(unused_imports)]` covers
// the case where a specific test module only uses `assert_eq`.
#[allow(unused_imports)]
pub use pretty_assertions::{assert_eq, assert_ne};
