//! Workspace metric rules — TOML budget DSL for the Sentrux signal.
//!
//! Sentrux Master Plan Wave 2 P3 (2026-05-09). This sub-module is the
//! metric-budget peer of the YAML+regex autofix engine that lives in
//! `touring_foundation::rules`; the two are
//! **complementary** (not duplicates) — names are deliberately
//! different (`MetricRule` here vs. `Rule` there) to avoid homonimia
//! per VP-Scout Cadeia 4.
//!
//! # Quick start
//!
//! ```ignore
//! use std::path::Path;
//! use touring_analysis::quality::{build_workspace_from_path, compute_quality_signal};
//! use touring_analysis::rules::{parse_path, evaluate};
//!
//! let ws     = build_workspace_from_path(Path::new("./crates/foo")).unwrap();
//! let signal = compute_quality_signal(&ws);
//! let rules  = parse_path(Path::new(".touring/quality.toml")).unwrap();
//! let violations = evaluate(&rules, &ws, &signal).unwrap();
//! ```
//!
//! # Schema (`v1.0`)
//!
//! ```toml
//! version = "1.0"
//!
//! [[rule]]
//! name        = "no-god-files"
//! applies_to  = "**/*.rs"     # required for per-entity metrics
//! metric      = "file_lines"  # see MetricKind
//! op          = "lt"
//! threshold   = 1000
//! severity    = "warn"        # info | warn | deny
//! message     = "files should be < 1000 LOC"
//! ```

pub mod diff;
pub mod error;
pub mod evaluator;
pub mod matchers;
pub mod parser;
pub mod types;

pub use diff::{PersistingViolation, ViolationsDiff, diff_violations};
pub use error::{Result, RulesError};
pub use evaluator::{count_by_severity, evaluate};
pub use matchers::matches_glob;
pub use parser::{parse_path, parse_str};
pub use types::{MetricKind, MetricRule, MetricRuleSet, MetricViolation, Op, Severity};
