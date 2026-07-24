//! RuleEngine — Data-driven classification rule evaluation.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// A single classification rule from `universal_rules.json`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Rule {
    /// Glob pattern for file paths (e.g., "**/tests/**/*.rs")
    pub file_pattern: Option<String>,
    /// Exact name match (e.g., "main", "new", "default")
    pub name_exact: Option<String>,
    /// Prefix match (e.g., "test_" for test functions)
    pub name_prefix: Option<String>,
    /// Suffix match (e.g., "_test" for test functions)
    pub name_suffix: Option<String>,
    /// Regex pattern on the full symbol name
    pub name_pattern: Option<String>,
    /// Target semantic class for this rule
    pub class: String,
    /// Rule priority (higher = evaluated first)
    pub priority: u32,
    /// Confidence score [0.0, 1.0]
    pub confidence: f32,
}

impl Rule {
    /// Checks if a symbol name matches this rule.
    pub fn matches_name(&self, name: &str) -> bool {
        if let Some(ref exact) = self.name_exact {
            if name == exact {
                return true;
            }
        }
        if let Some(ref prefix) = self.name_prefix {
            if name.starts_with(prefix) {
                return true;
            }
        }
        if let Some(ref suffix) = self.name_suffix {
            if name.ends_with(suffix) {
                return true;
            }
        }
        if let Some(ref pattern) = self.name_pattern {
            if let Ok(re) = regex::Regex::new(pattern) {
                if re.is_match(name) {
                    return true;
                }
            }
        }
        false
    }

    /// Checks if a file path matches this rule's file pattern.
    pub fn matches_file(&self, path: &str) -> bool {
        if let Some(ref pattern) = self.file_pattern {
            if let Ok(glob) = glob::Pattern::new(pattern) {
                return glob.matches(path);
            }
        }
        true // No file pattern means match all
    }
}

/// RuleEngine — Evaluates classification rules from `universal_rules.json`.
#[derive(Debug, Clone)]
pub struct RuleEngine {
    rules: Vec<Rule>,
    rules_by_class: HashMap<String, Vec<Rule>>,
}

impl RuleEngine {
    /// Load rules from embedded JSON data.
    pub fn new() -> Self {
        Self::default()
    }

    /// Load rules from a custom JSON string (for testing).
    pub fn from_json(json: &str) -> Result<Self, serde_json::Error> {
        let rules: Vec<Rule> = serde_json::from_str(json)?;
        Ok(Self::new_with_rules(rules))
    }

    fn new_with_rules(rules: Vec<Rule>) -> Self {
        let mut rules_by_class: HashMap<String, Vec<Rule>> = HashMap::new();
        for rule in &rules {
            rules_by_class
                .entry(rule.class.clone())
                .or_default()
                .push(rule.clone());
        }
        Self {
            rules,
            rules_by_class,
        }
    }

    /// Find the best matching rule for a symbol.
    pub fn find_rule(&self, name: &str, path: &str) -> Option<&Rule> {
        self.rules
            .iter()
            .filter(|r| r.matches_name(name) && r.matches_file(path))
            .max_by(|a, b| {
                a.priority.cmp(&b.priority).then_with(|| {
                    a.confidence
                        .partial_cmp(&b.confidence)
                        .unwrap_or(std::cmp::Ordering::Equal)
                })
            })
    }

    /// Get all rules for a specific semantic class.
    pub fn rules_for(&self, class: &str) -> &[Rule] {
        self.rules_by_class.get(class).map_or(&[], |v| v)
    }

    /// Count of total rules.
    pub fn rule_count(&self) -> usize {
        self.rules.len()
    }
}

impl Default for RuleEngine {
    fn default() -> Self {
        let json = include_str!("data/universal_rules.json");
        match serde_json::from_str(json) {
            Ok(rules) => Self::new_with_rules(rules),
            Err(e) => {
                tracing::warn!("Failed to load universal_rules.json: {e}, using empty ruleset");
                Self::new_with_rules(Vec::new())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rule_exact_match() {
        let rule = Rule {
            file_pattern: None,
            name_exact: Some("main".to_string()),
            name_prefix: None,
            name_suffix: None,
            name_pattern: None,
            class: "FunctionDef".to_string(),
            priority: 10,
            confidence: 1.0,
        };
        assert!(rule.matches_name("main"));
        assert!(!rule.matches_name("other"));
    }

    #[test]
    fn test_rule_prefix_match() {
        let rule = Rule {
            file_pattern: None,
            name_exact: None,
            name_prefix: Some("test_".to_string()),
            name_suffix: None,
            name_pattern: None,
            class: "FunctionDef".to_string(),
            priority: 5,
            confidence: 0.8,
        };
        assert!(rule.matches_name("test_foo"));
        assert!(!rule.matches_name("foo_test"));
    }

    #[test]
    fn test_rule_engine_priority() {
        let json = r#"[
            {"name_exact": "main", "class": "FunctionDef", "priority": 10, "confidence": 1.0},
            {"name_prefix": "m", "class": "FunctionDef", "priority": 5, "confidence": 0.5}
        ]"#;
        let engine = RuleEngine::from_json(json).unwrap();
        let rule = engine.find_rule("main", "main.rs").unwrap();
        assert_eq!(rule.priority, 10);
    }
}
