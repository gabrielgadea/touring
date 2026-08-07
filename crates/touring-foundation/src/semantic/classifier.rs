//! SemanticClassifier — 8-stage classification pipeline.
//!
//! Pipeline order (highest to lowest precedence):
//! 1. override       — Per-language TOML overrides
//! 2. file_detection — File path pattern matching
//! 3. token_purpose  — Syntactic token role
//! 4. universal_exact — Exact name match in rules
//! 5. universal_majority — Majority vote across similar symbols
//! 6. category       — Category-level heuristics
//! 7. name_heuristic — Name pattern heuristics (snake_case, PascalCase, etc.)
//! 8. unclassified   — Fallback

use super::categories::SemanticClass;
use super::overrides::OverrideEngine;
use super::rules::RuleEngine;
use serde::{Deserialize, Serialize};

/// Classification result with confidence scores.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClassificationResult {
    /// Primary semantic class
    pub primary: SemanticClass,
    /// Confidence of primary classification [0.0, 1.0]
    pub confidence: f32,
    /// All candidate classes with scores
    pub candidates: Vec<(SemanticClass, f32)>,
    /// Which pipeline stage produced the result
    pub stage: &'static str,
}

impl ClassificationResult {
    /// Creates a result from a single class (100% confidence).
    fn exact(class: SemanticClass, stage: &'static str) -> Self {
        Self {
            primary: class,
            confidence: 1.0,
            candidates: vec![(class, 1.0)],
            stage,
        }
    }

    /// Creates a result with a rule-matched confidence.
    fn from_rule(class: SemanticClass, confidence: f32, stage: &'static str) -> Self {
        Self {
            primary: class,
            confidence,
            candidates: vec![(class, confidence)],
            stage,
        }
    }
}

/// Configuration for the classifier.
#[derive(Debug, Clone)]
pub struct ClassifierConfig {
    /// Enable per-language overrides
    pub use_overrides: bool,
    /// Enable name heuristics
    pub use_name_heuristics: bool,
    /// Minimum confidence threshold
    pub min_confidence: f32,
}

impl Default for ClassifierConfig {
    fn default() -> Self {
        Self {
            use_overrides: true,
            use_name_heuristics: true,
            min_confidence: 0.5,
        }
    }
}

/// SemanticClassifier — Classifies code symbols using an 8-stage pipeline.
#[derive(Debug, Clone)]
pub struct SemanticClassifier {
    config: ClassifierConfig,
    rule_engine: RuleEngine,
    override_engine: OverrideEngine,
}

impl SemanticClassifier {
    /// Create a new classifier with default configuration.
    pub fn new() -> Self {
        Self::with_config(ClassifierConfig::default())
    }

    /// Create a classifier with custom configuration.
    pub fn with_config(config: ClassifierConfig) -> Self {
        // Fail-fast on corrupt embedded JSON; memoised via OnceLock so cost
        // is paid once per process. Logs at warn level if validation fails
        // rather than panicking, since an invalid corpus is recoverable
        // (rules engine still works with empty/partial data).
        warn_if_embedded_data_invalid();
        Self {
            config,
            rule_engine: RuleEngine::new(),
            override_engine: OverrideEngine::new(),
        }
    }

    /// Build a classifier whose `OverrideEngine` merges built-in rules
    /// with a user-supplied TOML payload.
    ///
    /// The TOML must follow the `[[language]] / [[language.rules]]` schema
    /// consumed by `super::overrides::LanguageOverrides::from_toml`. Use
    /// when the host project ships per-language overrides not covered by
    /// the built-in set (e.g. domain-specific naming conventions).
    pub fn from_toml(toml_str: &str) -> Result<Self, toml::de::Error> {
        let override_engine = OverrideEngine::with_builtins_and_toml(toml_str)?;
        warn_if_embedded_data_invalid();
        Ok(Self {
            config: ClassifierConfig::default(),
            rule_engine: RuleEngine::new(),
            override_engine,
        })
    }

    /// Languages currently supported by this classifier's override engine.
    ///
    /// Delegates to `OverrideEngine::supported_languages` and exposes
    /// the inventory at the public crate boundary so call-sites can
    /// validate language hints without reaching into the engine directly.
    pub fn supported_languages(&self) -> Vec<&str> {
        self.override_engine.supported_languages()
    }

    /// Build a classifier whose `OverrideEngine` contains **only** the
    /// TOML-supplied rules (no built-ins).
    ///
    /// Use when the host environment ships an exhaustive override TOML
    /// and wants to opt out of the embedded defaults — e.g. air-gapped
    /// deployments with curated rule sets. Built-in JSON validation
    /// still runs as a sanity check.
    pub fn from_toml_only(toml_str: &str) -> Result<Self, toml::de::Error> {
        let override_engine = OverrideEngine::from_toml(toml_str)?;
        warn_if_embedded_data_invalid();
        Ok(Self {
            config: ClassifierConfig::default(),
            rule_engine: RuleEngine::new(),
            override_engine,
        })
    }

    /// Classify a symbol by name and optional source code.
    ///
    /// Path-less entry: the override (stage 1) and file-detection (stage 2)
    /// stages need path context, so this runs only the shared
    /// universal_exact → name_heuristic → unclassified tail.
    pub fn classify(&self, name: &str, _source: &str) -> ClassificationResult {
        self.classify_tail(name, "")
    }

    /// Classify with file path context.
    pub fn classify_with_path(&self, name: &str, path: &str) -> ClassificationResult {
        // Stage 1: override — per-language TOML overrides (highest precedence)
        if self.config.use_overrides {
            // Detect language from path
            let language = if path.ends_with(".rs") {
                "rust"
            } else if path.ends_with(".py") {
                "python"
            } else if path.ends_with(".ts") || path.ends_with(".tsx") {
                "typescript"
            } else {
                ""
            };
            if !language.is_empty()
                && let Some(class) = self.override_engine.apply(language, name, path)
            {
                return ClassificationResult::exact(class, "override");
            }
        }

        // Stage 2: file_detection — check for test files
        if self.is_test_file(path) {
            return ClassificationResult::exact(SemanticClass::FunctionDef, "file_detection");
        }
        if self.is_impl_file(path) {
            return ClassificationResult::exact(SemanticClass::ImplBlock, "file_detection");
        }

        // Stages 4 (universal_exact) → 7 (name_heuristic) → 8 (unclassified).
        self.classify_tail(name, path)
    }

    /// The shared classification tail both entry points converge on: stage 4
    /// (universal_exact rule match) → stage 7 (name_heuristic) → stage 8
    /// (unclassified fallback). `path` is `""` for the path-less [`classify`]
    /// API and the file path for [`classify_with_path`].
    fn classify_tail(&self, name: &str, path: &str) -> ClassificationResult {
        // Stage 4: universal_exact — exact match in rules.
        if let Some(rule) = self.rule_engine.find_rule(name, path)
            && let Some(class) = self.name_to_class(&rule.class)
        {
            return ClassificationResult::from_rule(class, rule.confidence, "universal_exact");
        }

        // Stage 7: name_heuristic — pattern-based heuristics.
        if self.config.use_name_heuristics
            && let Some(class) = self.name_heuristic(name)
        {
            return ClassificationResult::exact(class, "name_heuristic");
        }

        // Stage 8: unclassified — fallback.
        ClassificationResult::exact(SemanticClass::Unclassified, "unclassified")
    }

    fn name_to_class(&self, class_name: &str) -> Option<SemanticClass> {
        match class_name {
            "FunctionDef" => Some(SemanticClass::FunctionDef),
            "StructDef" => Some(SemanticClass::StructDef),
            "EnumDef" => Some(SemanticClass::EnumDef),
            "TraitDef" => Some(SemanticClass::TraitDef),
            "ImplBlock" => Some(SemanticClass::ImplBlock),
            "TypeDef" => Some(SemanticClass::TypeDef),
            "Module" => Some(SemanticClass::Module),
            "UseStatement" => Some(SemanticClass::UseStatement),
            "ConstDef" => Some(SemanticClass::ConstDef),
            "StaticDef" => Some(SemanticClass::StaticDef),
            "FnParam" => Some(SemanticClass::FnParam),
            "StructField" => Some(SemanticClass::StructField),
            "EnumVariant" => Some(SemanticClass::EnumVariant),
            "Attribute" => Some(SemanticClass::Attribute),
            "DocComment" => Some(SemanticClass::DocComment),
            "MacroDef" => Some(SemanticClass::MacroDef),
            "Closure" => Some(SemanticClass::Closure),
            "ClosureParam" => Some(SemanticClass::ClosureParam),
            "TypeAnnotation" => Some(SemanticClass::TypeAnnotation),
            "GenericParam" => Some(SemanticClass::GenericParam),
            "WhereClause" => Some(SemanticClass::WhereClause),
            "ImportStatement" => Some(SemanticClass::ImportStatement),
            _ => None,
        }
    }

    fn is_test_file(&self, path: &str) -> bool {
        path.contains("/tests/")
            || path.ends_with("_test.rs")
            || path.ends_with(".test.ts")
            || path.ends_with("_test.py")
            || path.contains("/test/")
    }

    fn is_impl_file(&self, path: &str) -> bool {
        path.contains("/impl/") || path.ends_with("_impl.rs")
    }

    /// Name-based heuristics: snake_case, PascalCase, SCREAMING_SNAKE detection.
    fn name_heuristic(&self, name: &str) -> Option<SemanticClass> {
        if name.is_empty() {
            return None;
        }

        // SCREAMING_SNAKE → constant
        if name
            .chars()
            .all(|c| c.is_uppercase() || c.is_numeric() || c == '_')
            && name.contains('_')
        {
            return Some(SemanticClass::ConstDef);
        }

        // PascalCase → type definition
        if name
            .chars()
            .next()
            .map(|c| c.is_uppercase())
            .unwrap_or(false)
            && !name.contains('_')
            && !name.chars().any(|c| c.is_lowercase())
        {
            return Some(SemanticClass::TypeDef);
        }

        // camelCase → function or parameter
        if name
            .chars()
            .next()
            .map(|c| c.is_lowercase())
            .unwrap_or(false)
            && name.contains('_')
        {
            return Some(SemanticClass::FunctionDef);
        }

        // snake_case → various (heuristic)
        if name.contains('_') {
            let lower = name.to_lowercase();
            if lower.starts_with("test_") || lower.ends_with("_test") {
                return Some(SemanticClass::FunctionDef);
            }
            if lower.starts_with("is_") || lower.starts_with("has_") || lower.starts_with("are_") {
                return Some(SemanticClass::FunctionDef);
            }
            if lower.ends_with("_new") || lower.ends_with("_default") {
                return Some(SemanticClass::FunctionDef);
            }
        }

        None
    }

    /// Access the underlying rule engine.
    pub fn rule_engine(&self) -> &RuleEngine {
        &self.rule_engine
    }
}

impl Default for SemanticClassifier {
    fn default() -> Self {
        Self::new()
    }
}

/// Log a warning if the embedded classification corpus fails validation.
///
/// Non-fatal by design: an invalid corpus is recoverable (the rule engine still
/// works with empty/partial data), so every constructor calls this instead of
/// panicking. Extracted from the three byte-identical copies the constructors
/// used to carry.
fn warn_if_embedded_data_invalid() {
    if let Err(err) = super::validate_embedded_data() {
        tracing::warn!(
            target: "touring_definitions",
            "embedded_data_validation_failed: {err}"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_function() {
        let classifier = SemanticClassifier::new();
        let result = classifier.classify("my_function", "");
        assert!(matches!(
            result.primary,
            SemanticClass::FunctionDef | SemanticClass::Unclassified
        ));
    }

    #[test]
    fn test_classify_constant() {
        let classifier = SemanticClassifier::new();
        let result = classifier.classify("MY_CONSTANT", "");
        assert_eq!(result.primary, SemanticClass::ConstDef);
    }

    #[test]
    fn test_classify_test_file() {
        let classifier = SemanticClassifier::new();
        // test_something matches test_* override pattern for Rust
        let result = classifier.classify_with_path("test_something", "src/tests/foo_test.rs");
        assert_eq!(result.primary, SemanticClass::FunctionDef);
        // Stage 1 (override) has highest precedence, so it fires before Stage 2 (file_detection)
        assert_eq!(result.stage, "override");
    }
}
