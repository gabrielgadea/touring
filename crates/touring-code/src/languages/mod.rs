//! touring-language — Tier-based language support disclosure.
//!
//! ## Tier Hierarchy
//!
//! | Tier | Languages | Capabilities |
//! |------|------------|---------------|
//! | 1 | Rust, TypeScript | Full AST, symbols, quality, wiring, cognitive |
//! | 2 | Python, Go, C | AST, symbols, quality; partial wiring |
//! | 3 | Kotlin, Swift, Java | AST, partial symbols |
//! | 4 | Ruby, PHP | Basic tokens only |
//!
//! ## CLI Commands
//!
//! - `touring language list` — show all languages with tiers and capabilities
//! - `touring language <lang>` — detailed capability dump for one language

#![warn(missing_docs)]
#![forbid(unsafe_code)]

pub mod error;
// W4 fusion: nested `languages` submodule (ex-touring-language crate root was
// `touring_language`, so `touring_language::languages` had no inception).
#[allow(clippy::module_inception)]
pub mod languages;
pub mod matrix;
pub mod tiers;
pub mod types;

pub use error::{Error, Result};
pub use matrix::{Capability, LanguageSupport, SupportLevel};
pub use tiers::{Language, Tier};
pub use types::Item;
