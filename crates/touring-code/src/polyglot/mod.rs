//! Polyglot AST search + rewrite for Touring.
//!
//! Extends `touring-ast` (Rust-only via syn) to JavaScript, TypeScript, Python,
//! Go, Ruby, and ~20 other languages via the ast-grep + tree-sitter stack.
//!
//! # Surface
//!
//! ```no_run
//! use touring_code::polyglot::{Lang, search, rewrite};
//!
//! let src = "console.log('a'); console.log('b');";
//! let hits = search(Lang::JavaScript, src, "console.log($X)").unwrap();
//! assert_eq!(hits.len(), 2);
//!
//! let out = rewrite(Lang::JavaScript, src, "console.log($X)", "logger.info($X)").unwrap();
//! assert!(out.contains("logger.info"));
//! ```

mod error;
/// Language detection and the polyglot `Lang` enum.
pub mod lang;
/// Structural rewrite of source code via ast-grep patterns.
pub mod rewrite;
/// Directory walking and rule-based scanning across many files.
pub mod scan;
/// Structural search returning matches for an ast-grep pattern.
pub mod search;

pub use error::{Error, Result};
pub use lang::{Lang, detect_lang};
pub use rewrite::rewrite;
pub use scan::{Rule, RuleMatch, ScanReport, Severity, scan_files, walk_files};
pub use search::{Match, search};
