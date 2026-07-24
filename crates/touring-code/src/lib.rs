//! touring-code — Unified code intelligence crate
//!
//! Fuses 4 historically-separate crates under one namespace:
//!
//! | Module                | Origin crate              | Status |
//! |-----------------------|---------------------------|--------|
//! | [`ast`]               | `touring-ast`             | W4.2   |
//! | [`polyglot`]          | `touring-ast-polyglot`    | W4.3   |
//! | [`languages`]         | `touring-language`        | W4.4   |
//! | [`semantics`]         | `touring-semantics`       | W4.5   |
//!
//! ## Features
//!
//! Language gates: `lang-rust` (default), `lang-typescript`, `lang-python`,
//! `lang-go`, `lang-ruby`, `lang-java`, `lang-cpp`.
//!
//! Parser engines: `parser-tree-sitter` (default), `parser-ast-grep`,
//! `parser-syn`.
//!
//! Higher-level toggles: `semantic-search`, `incremental-salsa`.
//!
//! ## Migration
//!
//! Source crates remain for two minor versions as shim crates that re-export
//! from this canonical home. Consumers should migrate imports:
//!
//! ```text
//! use touring_ast::X         → use touring_code::ast::X
//! use touring_ast_polyglot::X → use touring_code::polyglot::X
//! use touring_language::X     → use touring_code::languages::X
//! use touring_semantics::X    → use touring_code::semantics::X
//! ```
//!
//! W4 of `touring-premium-refactor-2026`.

// NOTE: `unsafe` is permitted — the fused `ast` module (ex-touring-ast) carries
// `unsafe impl Send/Sync` for its pipeline types. `forbid(unsafe_code)` would
// reject the W4.2 fusion.
//
// `missing_docs` is now denied for non-test builds. `cfg_attr(not(test), ...)`
// is used because several modules carry `pub` test fixtures (e.g. `pub struct
// Foo`) inside `#[cfg(test)]` blocks that are intentionally undocumented.
#![cfg_attr(not(test), deny(missing_docs))]
// RBP-01 elite-lint ratchet (2026-06-16): prod-unwrap-free (3 `wilson_trials().lock()
// .unwrap()` → `.expect(..)`; remaining `.unwrap()` tokens are detector pattern-strings
// in ast/quality.rs) — lock against future bare unwrap in non-test code.
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

pub mod ast;
pub mod error;
pub mod languages;
pub mod polyglot;
pub mod semantics;
pub mod types;

pub use error::{Error, Result};
pub use types::*;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn smoke_re_exports_compile() {
        let _err: Result<()> = Ok(());
    }

    #[test]
    fn languages_module_exposed() {
        // W4.4: touring_code::languages::Lang must resolve.
        // The `languages` submodule mirrors the original touring-language crate.
        let _: Option<crate::languages::tiers::Tier> = None;
    }
}
