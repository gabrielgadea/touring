//! touring-definitions — Semantic classification of code symbols.
//!
//! # Architecture
//!
//! This crate provides 22 `SemanticClass` categories for classifying code symbols
//! using a data-driven pipeline powered by `universal_rules.json`.
//!
//! # Pipeline (in order of precedence)
//!
//! 1. **override** — Per-language TOML overrides take highest precedence
//! 2. **file_detection** — Detect semantic meaning from file path patterns
//! 3. **token_purpose** — Classify based on token syntactic role
//! 4. **universal_exact** — Exact match in `universal_rules.json`
//! 5. **universal_majority** — Majority vote across similar symbols
//! 6. **category** — Category-level heuristics (e.g., `is_test`, `is_benchmark`)
//! 7. **name_heuristic** — Name-based patterns (snake_case, PascalCase, SCREAMING_SNAKE)
//! 8. **unclassified** — Fallback when no rule matches
//!
//! # Example
//!
//! ```ignore
//! use touring_definitions::{SemanticClassifier, SemanticClass};
//!
//! let classifier = SemanticClassifier::default();
//! let result = classifier.classify("MyStruct", "struct MyStruct { }");
//! assert!(matches!(result.primary, SemanticClass::StructDef));
//! ```

mod categories;
mod classifier;
pub mod data;
mod overrides;
pub mod rules;

pub mod cli;

pub use categories::SemanticClass;
pub use classifier::SemanticClassifier;
pub use rules::RuleEngine;

/// Validates the three embedded JSON corpora (universal_rules, categories,
/// scoring) at process start. Memoised via [`std::sync::OnceLock`] so the
/// parse cost is paid exactly once. Called from [`SemanticClassifier::new`]
/// to fail-fast on data corruption rather than at first classification.
pub fn validate_embedded_data() -> Result<(), EmbeddedDataError> {
    use std::sync::OnceLock;
    static OUTCOME: OnceLock<Result<(), EmbeddedDataError>> = OnceLock::new();
    OUTCOME
        .get_or_init(|| {
            data::parse_universal_rules()
                .map_err(|e| EmbeddedDataError(format!("universal_rules: {e}")))?;
            data::parse_categories().map_err(|e| EmbeddedDataError(format!("categories: {e}")))?;
            data::parse_scoring().map_err(|e| EmbeddedDataError(format!("scoring: {e}")))?;
            Ok(())
        })
        .clone()
}

/// Error from [`validate_embedded_data`] (F-8 / RBP-03: typed in place of
/// `String`; `Clone` because the outcome is memoised in a `OnceLock` and cloned
/// to each caller).
#[derive(Debug, Clone, thiserror::Error)]
#[error("embedded data invalid: {0}")]
pub struct EmbeddedDataError(String);
