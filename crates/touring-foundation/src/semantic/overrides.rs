//! overrides — Per-language TOML overrides for classification.

use super::categories::SemanticClass;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Language-specific override rules loaded from TOML.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LanguageOverrides {
    /// Language identifier (e.g., "rust", "python", "typescript")
    pub language: String,
    /// Override rules for this language
    pub rules: Vec<OverrideRule>,
}

/// A single override rule for a specific language.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OverrideRule {
    /// Glob pattern for symbol names
    pub name_pattern: String,
    /// File path pattern (optional)
    pub file_pattern: Option<String>,
    /// The semantic class to assign
    pub class: String,
    /// Reason for the override
    pub reason: String,
}

impl LanguageOverrides {
    /// Parse overrides from TOML string.
    pub fn from_toml(toml_str: &str) -> Result<Vec<Self>, toml::de::Error> {
        toml::from_str(toml_str)
    }
}

/// OverrideEngine — Applies per-language override rules.
#[derive(Debug, Clone)]
pub struct OverrideEngine {
    overrides: HashMap<String, Vec<OverrideRule>>,
}

impl OverrideEngine {
    /// Create a new engine with built-in overrides for common languages.
    pub fn new() -> Self {
        Self::with_builtins()
    }

    /// Build engine that merges built-in overrides with a user-supplied
    /// TOML file. Languages declared in the TOML override the built-ins
    /// for the same key. Used by [`crate::SemanticClassifier::from_toml`].
    pub fn with_builtins_and_toml(toml_str: &str) -> Result<Self, toml::de::Error> {
        let mut engine = Self::with_builtins();
        let lang_overrides = LanguageOverrides::from_toml(toml_str)?;
        for lo in lang_overrides {
            engine.overrides.insert(lo.language.clone(), lo.rules);
        }
        Ok(engine)
    }

    /// Create engine with custom TOML overrides only (no built-ins).
    pub fn from_toml(toml_str: &str) -> Result<Self, toml::de::Error> {
        let lang_overrides = LanguageOverrides::from_toml(toml_str)?;
        let mut overrides = HashMap::new();
        for lo in lang_overrides {
            overrides.insert(lo.language.clone(), lo.rules);
        }
        Ok(Self { overrides })
    }

    /// Apply override rules for a specific language.
    pub fn apply(&self, language: &str, name: &str, path: &str) -> Option<SemanticClass> {
        let rules = self.overrides.get(language)?;
        for rule in rules {
            if let Ok(glob) = glob::Pattern::new(&rule.name_pattern) {
                if glob.matches(name) {
                    if let Some(ref file_pattern) = rule.file_pattern {
                        if let Ok(fp) = glob::Pattern::new(file_pattern) {
                            if !fp.matches(path) {
                                continue;
                            }
                        }
                    }
                    return str_to_class(&rule.class);
                }
            }
        }
        None
    }

    /// Get list of supported languages.
    pub fn supported_languages(&self) -> Vec<&str> {
        self.overrides.keys().map(|s| s.as_str()).collect()
    }

    fn with_builtins() -> Self {
        let mut overrides = HashMap::new();

        // Rust-specific overrides
        overrides.insert(
            "rust".to_string(),
            vec![
                OverrideRule {
                    name_pattern: "*".to_string(),
                    file_pattern: Some("**/tests/**/*.rs".to_string()),
                    class: "FunctionDef".to_string(),
                    reason: "Rust test functions in tests/ dir".to_string(),
                },
                OverrideRule {
                    name_pattern: "test_*".to_string(),
                    file_pattern: None,
                    class: "FunctionDef".to_string(),
                    reason: "Rust test function convention".to_string(),
                },
                OverrideRule {
                    name_pattern: "*_test".to_string(),
                    file_pattern: None,
                    class: "FunctionDef".to_string(),
                    reason: "Rust unit test convention".to_string(),
                },
            ],
        );

        // Python-specific overrides
        overrides.insert(
            "python".to_string(),
            vec![
                OverrideRule {
                    name_pattern: "test_*".to_string(),
                    file_pattern: None,
                    class: "FunctionDef".to_string(),
                    reason: "Python test function convention".to_string(),
                },
                OverrideRule {
                    name_pattern: "*_test".to_string(),
                    file_pattern: None,
                    class: "FunctionDef".to_string(),
                    reason: "Python unittest convention".to_string(),
                },
                OverrideRule {
                    name_pattern: "Test*".to_string(),
                    file_pattern: None,
                    class: "StructDef".to_string(),
                    reason: "Python test class convention".to_string(),
                },
            ],
        );

        // TypeScript/JavaScript overrides
        overrides.insert(
            "typescript".to_string(),
            vec![
                OverrideRule {
                    name_pattern: "*.test.ts".to_string(),
                    file_pattern: None,
                    class: "FunctionDef".to_string(),
                    reason: "Jest test convention".to_string(),
                },
                OverrideRule {
                    name_pattern: "*.spec.ts".to_string(),
                    file_pattern: None,
                    class: "FunctionDef".to_string(),
                    reason: "Jasmine test convention".to_string(),
                },
            ],
        );

        Self { overrides }
    }
}

impl Default for OverrideEngine {
    fn default() -> Self {
        Self::new()
    }
}

fn str_to_class(s: &str) -> Option<SemanticClass> {
    match s {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rust_test_override() {
        let engine = OverrideEngine::new();
        let class = engine.apply("rust", "test_something", "src/tests/foo.rs");
        assert_eq!(class, Some(SemanticClass::FunctionDef));
    }

    #[test]
    fn test_python_test_override() {
        let engine = OverrideEngine::new();
        let class = engine.apply("python", "test_my_func", "test_file.py");
        assert_eq!(class, Some(SemanticClass::FunctionDef));
    }
}
