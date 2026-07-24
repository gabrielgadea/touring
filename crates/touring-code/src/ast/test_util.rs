//! Shared test utilities for `touring-ast` unit and integration tests.
//!
//! Tier A (2026-04-19): thin prelude that re-exports `pretty_assertions`
//! shadow-macros so every `#[cfg(test)] mod tests` can pull the whole set
//! via `use crate::test_util::*;`.
//!
//! Gated behind `#[cfg(test)]` at the declaration site in `lib.rs`.

pub use pretty_assertions::{assert_eq, assert_ne};
