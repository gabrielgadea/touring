//! Rules engine — YAML-driven autofix rules absorbed from `touring-rule-engine`.
//!
//! Absorbed into `touring-foundation` in W3.4 (Wave 2026-05-12). The crate
//! `touring-rule-engine` was anemic (443 LOC, single internal consumer:
//! `touring-analysis`), so its content lives here under `touring_foundation::rules`.
//!
//! # Module structure
//!
//! - [`error`]: typed errors via `thiserror`.
//! - [`types`]: public data types — `Rule`, `RuleSet`, `Fix`, `RuleEngine`.
//! - `cli`: CLI commands (check, apply, list).
//!
//! # Migration note
//!
//! Before W3.4: `use touring_rule_engine::{Fix, Rule, RuleEngine};`
//! After  W3.4: `use touring_foundation::rules::{Fix, Rule, RuleEngine};`

pub mod cli;
pub mod error;
pub mod types;

pub use error::{Error, Result};
pub use types::{Fix, Rule, RuleEngine, RuleSet, Severity};
