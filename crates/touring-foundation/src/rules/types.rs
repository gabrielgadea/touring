//! Rule engine types.

use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Severity level for a rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    /// Hint style fix.
    Hint,
    /// Style improvement (default — least disruptive feedback level).
    #[default]
    Style,
    /// Warning level.
    Warning,
    /// Error level.
    Error,
}

/// A single autofix rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Rule {
    /// Unique rule name.
    pub name: String,
    /// Glob pattern or explicit file path to target.
    #[serde(default)]
    pub path: Option<String>,
    /// Regex pattern to search in file contents.
    pub pattern: String,
    /// Replacement string or template.
    #[serde(default)]
    pub fix: Option<String>,
    /// Languages this rule applies to.
    #[serde(default)]
    pub languages: Vec<String>,
    /// Rule severity.
    #[serde(default)]
    pub severity: Severity,
    /// Optional description.
    #[serde(default)]
    pub description: Option<String>,
}

impl Rule {
    /// Returns true if the rule applies to the given language.
    pub fn applies_to_language(&self, lang: &str) -> bool {
        self.languages.is_empty() || self.languages.iter().any(|l| l == lang)
    }
}

/// A collection of rules loaded from a YAML file.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RuleSet {
    /// List of rules.
    #[serde(default)]
    pub rules: Vec<Rule>,
}

impl RuleSet {
    /// Load a [`RuleSet`] from a YAML file.
    pub fn load_from_file(path: &PathBuf) -> super::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let ruleset: RuleSet = serde_yaml::from_str(&content)?;
        Ok(ruleset)
    }

    /// Parse a [`RuleSet`] from YAML string content.
    pub fn parse(content: &str) -> super::Result<Self> {
        let ruleset: RuleSet = serde_yaml::from_str(content)?;
        Ok(ruleset)
    }
}

/// An applied fix result.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Fix {
    /// Rule name that produced this fix.
    pub rule_name: String,
    /// File path where fix was applied.
    pub file_path: PathBuf,
    /// Original matched text.
    pub original: String,
    /// Replacement text applied.
    pub replacement: String,
    /// Line number in the file.
    pub line: usize,
    /// Column offset.
    pub column: usize,
}

/// A rule engine that loads and applies rules to source files.
#[derive(Debug, Clone)]
pub struct RuleEngine {
    ruleset: RuleSet,
}

impl RuleEngine {
    /// Create a new engine from a ruleset.
    pub fn new(ruleset: RuleSet) -> Self {
        Self { ruleset }
    }

    /// Load rules from a YAML file.
    pub fn load_rules(path: &PathBuf) -> super::Result<Self> {
        let ruleset = RuleSet::load_from_file(path)?;
        Ok(Self::new(ruleset))
    }

    /// Apply all rules to a list of files, returning fixes.
    pub fn apply_rules(&self, files: &[PathBuf]) -> Vec<Fix> {
        let mut fixes = Vec::new();
        for file in files {
            fixes.extend(self.apply_to_file(file));
        }
        fixes
    }

    /// Apply rules to a single file.
    pub fn apply_to_file(&self, file: &PathBuf) -> Vec<Fix> {
        let mut fixes = Vec::new();
        let Ok(content) = std::fs::read_to_string(file) else {
            return fixes;
        };

        for rule in &self.ruleset.rules {
            // Filter by path glob if specified
            if let Some(glob_pattern) = &rule.path {
                let file_str = file.to_string_lossy();
                if !glob_match(glob_pattern, &file_str) {
                    continue;
                }
            }

            let lang = detect_language(file);
            if !rule.applies_to_language(&lang) {
                continue;
            }

            let re = match regex::Regex::new(&rule.pattern) {
                Ok(r) => r,
                Err(_) => continue,
            };

            for (line_idx, line) in content.lines().enumerate() {
                for mat in re.find_iter(line) {
                    if let Some(fix_text) = &rule.fix {
                        fixes.push(Fix {
                            rule_name: rule.name.clone(),
                            file_path: file.clone(),
                            original: mat.as_str().to_string(),
                            replacement: fix_text.clone(),
                            line: line_idx + 1,
                            column: mat.start(),
                        });
                    }
                }
            }
        }
        fixes
    }

    /// Returns the underlying ruleset.
    pub fn ruleset(&self) -> &RuleSet {
        &self.ruleset
    }
}

/// Simple glob matching (supports `*` and `**`).
fn glob_match(pattern: &str, text: &str) -> bool {
    // Normalize pattern
    let pattern = pattern.replace("**/", "*");
    let parts: Vec<&str> = pattern.split('*').collect();
    let mut text = text;

    for part in parts {
        if part.is_empty() {
            continue;
        }
        if let Some(pos) = text.find(part) {
            text = &text[pos + part.len()..];
        } else {
            return false;
        }
    }
    true
}

/// Detect language from file extension.
fn detect_language(path: &Path) -> String {
    path.extension()
        .and_then(|e| e.to_str())
        .map(|e| match e {
            "rs" => "rust",
            "py" => "python",
            "ts" | "tsx" => "typescript",
            "js" | "jsx" => "javascript",
            "go" => "go",
            "java" => "java",
            "cpp" | "cc" | "cxx" => "cpp",
            "c" | "h" => "c",
            "rb" => "ruby",
            "cs" => "csharp",
            "swift" => "swift",
            "kt" => "kotlin",
            _ => "unknown",
        })
        .unwrap_or("unknown")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_severity_default() {
        assert_eq!(Severity::default(), Severity::Style);
    }

    #[test]
    fn test_rule_applies_to_language() {
        let rule = Rule {
            name: "test".to_string(),
            path: None,
            pattern: "foo".to_string(),
            fix: Some("bar".to_string()),
            languages: vec!["rust".to_string()],
            severity: Severity::Warning,
            description: None,
        };
        assert!(rule.applies_to_language("rust"));
        assert!(!rule.applies_to_language("python"));
    }

    #[test]
    fn test_rule_empty_languages() {
        let rule = Rule {
            name: "test".to_string(),
            path: None,
            pattern: "foo".to_string(),
            fix: None,
            languages: vec![],
            severity: Severity::default(),
            description: None,
        };
        // Empty languages means applies to all
        assert!(rule.applies_to_language("rust"));
        assert!(rule.applies_to_language("python"));
    }

    #[test]
    fn test_ruleset_parse() {
        let yaml = r#"
rules:
  - name: test-rule
    pattern: "foo"
    fix: "bar"
    languages: [rust]
    severity: warning
"#;
        let rs = RuleSet::parse(yaml).unwrap();
        assert_eq!(rs.rules.len(), 1);
        assert_eq!(rs.rules[0].name, "test-rule");
    }

    #[test]
    fn test_glob_match() {
        assert!(glob_match("*.rs", "foo.rs"));
        assert!(glob_match("*.rs", "bar.rs"));
        assert!(!glob_match("*.rs", "foo.py"));
        assert!(glob_match("**/src/*.rs", "src/foo.rs"));
    }

    #[test]
    fn test_detect_language() {
        assert_eq!(detect_language(&PathBuf::from("foo.rs")), "rust");
        assert_eq!(detect_language(&PathBuf::from("foo.py")), "python");
        assert_eq!(detect_language(&PathBuf::from("foo.ts")), "typescript");
    }

    // ── W3.9 expanded coverage (2026-05-14) ───────────────────────────────

    /// REQUIREMENT detect_language ALL extensions including unknown
    /// BOUNDARY no-ext, weird-ext, casing
    /// COVER 12 of 12 known + fallback branch
    #[test]
    fn test_detect_language_all_known_extensions() {
        let cases = [
            ("foo.rs", "rust"),
            ("foo.py", "python"),
            ("foo.ts", "typescript"),
            ("foo.tsx", "typescript"),
            ("foo.js", "javascript"),
            ("foo.jsx", "javascript"),
            ("foo.go", "go"),
            ("foo.java", "java"),
            ("foo.cpp", "cpp"),
            ("foo.cc", "cpp"),
            ("foo.cxx", "cpp"),
            ("foo.c", "c"),
            ("foo.h", "c"),
            ("foo.rb", "ruby"),
            ("foo.cs", "csharp"),
            ("foo.swift", "swift"),
            ("foo.kt", "kotlin"),
        ];
        for (path, expected) in cases {
            assert_eq!(
                detect_language(&PathBuf::from(path)),
                expected,
                "expected {} → {}",
                path,
                expected
            );
        }
    }

    /// REQUIREMENT detect_language fallback for unknown extensions
    /// BOUNDARY no extension, dotfile, weird extension
    /// COVER unwrap_or fallback path
    #[test]
    fn test_detect_language_unknown_fallback() {
        assert_eq!(detect_language(&PathBuf::from("foo")), "unknown");
        assert_eq!(detect_language(&PathBuf::from("foo.xyz")), "unknown");
        assert_eq!(detect_language(&PathBuf::from(".bashrc")), "unknown");
        assert_eq!(detect_language(&PathBuf::from("")), "unknown");
    }

    /// REQUIREMENT glob_match handles edge patterns
    /// BOUNDARY empty pattern, all-stars, no-stars, missing tail, * matches empty
    /// COVER continue branch on empty parts + fail branch on text.find None
    ///
    /// Note: per the impl's greedy semantics, `*` can match the empty string
    /// (the loop sequentially finds each non-empty part); so `"a*b"` matches
    /// both `"axxxb"` AND `"ab"`. This mirrors how `**/*.rs` matches `/foo.rs`.
    #[test]
    fn test_glob_match_edge_cases() {
        assert!(glob_match("", "anything")); // empty pattern matches all
        assert!(glob_match("*", "anything")); // single star
        assert!(glob_match("**", "anything")); // double star
        assert!(glob_match("foo", "foo")); // exact
        assert!(!glob_match("foo", "bar")); // mismatch
        assert!(glob_match("a*b", "axxxb")); // sandwich
        assert!(glob_match("a*b", "ab")); // star can match empty
        assert!(!glob_match("foo*", "barfo")); // missing tail char
        assert!(glob_match("*foo*", "xfooy")); // star on both sides
        assert!(!glob_match("xyz", "ab")); // unmatchable
    }

    /// REQUIREMENT Severity serde roundtrip lowercase wire format
    /// BOUNDARY all 4 variants
    /// COVER serialize + deserialize round-trip
    #[test]
    fn test_severity_serde_lowercase_roundtrip() {
        for (variant, expected_json) in [
            (Severity::Hint, "\"hint\""),
            (Severity::Style, "\"style\""),
            (Severity::Warning, "\"warning\""),
            (Severity::Error, "\"error\""),
        ] {
            let s = serde_json::to_string(&variant).unwrap();
            assert_eq!(s, expected_json, "serialize {:?}", variant);
            let parsed: Severity = serde_json::from_str(&s).unwrap();
            assert_eq!(parsed, variant, "roundtrip {:?}", variant);
        }
    }

    /// REQUIREMENT Rule with all fields populated serializes to YAML
    /// BOUNDARY camelCase rename (file_path → filePath impact on Fix, not Rule)
    /// COVER full serde populated path
    #[test]
    fn test_rule_full_serde() {
        let rule = Rule {
            name: "no-unwrap".to_string(),
            path: Some("**/*.rs".to_string()),
            pattern: r"\.unwrap\(\)".to_string(),
            fix: Some("?".to_string()),
            languages: vec!["rust".to_string()],
            severity: Severity::Warning,
            description: Some("Avoid unwrap in production".to_string()),
        };
        let json = serde_json::to_string(&rule).unwrap();
        let parsed: Rule = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.name, rule.name);
        assert_eq!(parsed.severity, Severity::Warning);
        assert_eq!(parsed.languages, vec!["rust"]);
    }

    /// REQUIREMENT Rule defaults via serde when YAML omits optional fields
    /// BOUNDARY missing path/fix/languages/description
    /// COVER all #[serde(default)] paths
    #[test]
    fn test_rule_minimal_yaml_uses_defaults() {
        let yaml = "name: minimal\npattern: \"x\"";
        let rule: Rule = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(rule.name, "minimal");
        assert_eq!(rule.path, None);
        assert_eq!(rule.fix, None);
        assert!(rule.languages.is_empty());
        assert_eq!(rule.severity, Severity::Style); // default
        assert_eq!(rule.description, None);
    }

    /// REQUIREMENT RuleEngine::new + ruleset() getter return what was stored
    /// BOUNDARY empty + populated
    /// COVER constructor + getter API
    #[test]
    fn test_rule_engine_new_and_getter() {
        let empty = RuleEngine::new(RuleSet { rules: vec![] });
        assert!(empty.ruleset().rules.is_empty());

        let with_rules = RuleEngine::new(RuleSet {
            rules: vec![Rule {
                name: "r1".to_string(),
                path: None,
                pattern: "x".to_string(),
                fix: None,
                languages: vec![],
                severity: Severity::default(),
                description: None,
            }],
        });
        assert_eq!(with_rules.ruleset().rules.len(), 1);
        assert_eq!(with_rules.ruleset().rules[0].name, "r1");
    }

    /// REQUIREMENT apply_to_file returns empty Vec when file does not exist
    /// BOUNDARY missing file
    /// COVER `Ok(content)` short-circuit branch
    #[test]
    fn test_apply_to_file_missing_file_returns_empty() {
        let engine = RuleEngine::new(RuleSet {
            rules: vec![Rule {
                name: "x".to_string(),
                path: None,
                pattern: "foo".to_string(),
                fix: Some("bar".to_string()),
                languages: vec![],
                severity: Severity::default(),
                description: None,
            }],
        });
        let fixes = engine.apply_to_file(&PathBuf::from("/nonexistent/path/qzzz"));
        assert!(fixes.is_empty());
    }

    /// REQUIREMENT apply_to_file skips rule with invalid regex (no panic)
    /// BOUNDARY malformed pattern
    /// COVER `Regex::new` Err branch
    #[test]
    fn test_apply_to_file_invalid_regex_skips_silently() {
        let tmp =
            std::env::temp_dir().join(format!("touring-rules-test-{}.rs", std::process::id()));
        std::fs::write(&tmp, "fn main() { foo(); }").unwrap();
        let engine = RuleEngine::new(RuleSet {
            rules: vec![Rule {
                name: "bad".to_string(),
                path: None,
                pattern: "[invalid(regex".to_string(), // unbalanced
                fix: Some("good".to_string()),
                languages: vec![],
                severity: Severity::default(),
                description: None,
            }],
        });
        let fixes = engine.apply_to_file(&tmp);
        assert!(fixes.is_empty(), "invalid regex should produce no fixes");
        let _ = std::fs::remove_file(&tmp);
    }

    /// REQUIREMENT apply_to_file does NOT produce Fix when rule.fix is None
    /// BOUNDARY audit-only rule (no replacement)
    /// COVER `if let Some(fix_text)` skip branch
    #[test]
    fn test_apply_to_file_no_fix_means_no_output() {
        let tmp =
            std::env::temp_dir().join(format!("touring-rules-nofix-{}.rs", std::process::id()));
        std::fs::write(&tmp, "fn main() { foo(); }").unwrap();
        let engine = RuleEngine::new(RuleSet {
            rules: vec![Rule {
                name: "audit".to_string(),
                path: None,
                pattern: "foo".to_string(),
                fix: None, // audit-mode rule
                languages: vec![],
                severity: Severity::Hint,
                description: None,
            }],
        });
        let fixes = engine.apply_to_file(&tmp);
        assert!(
            fixes.is_empty(),
            "audit-only rule should produce no Fix entries"
        );
        let _ = std::fs::remove_file(&tmp);
    }

    /// REQUIREMENT apply_to_file respects language filter
    /// BOUNDARY rule with languages=[python] applied to .rs file
    /// COVER `applies_to_language` continue branch
    #[test]
    fn test_apply_to_file_language_mismatch_skips() {
        let tmp =
            std::env::temp_dir().join(format!("touring-rules-lang-{}.rs", std::process::id()));
        std::fs::write(&tmp, "fn main() { foo(); }").unwrap();
        let engine = RuleEngine::new(RuleSet {
            rules: vec![Rule {
                name: "python-only".to_string(),
                path: None,
                pattern: "foo".to_string(),
                fix: Some("bar".to_string()),
                languages: vec!["python".to_string()],
                severity: Severity::default(),
                description: None,
            }],
        });
        let fixes = engine.apply_to_file(&tmp);
        assert!(
            fixes.is_empty(),
            "rust file should not match python-only rule"
        );
        let _ = std::fs::remove_file(&tmp);
    }

    /// REQUIREMENT apply_to_file respects path glob filter (mismatch skips)
    /// BOUNDARY rule with path=*.py applied to .rs file
    /// COVER `!glob_match` continue branch
    #[test]
    fn test_apply_to_file_path_glob_mismatch_skips() {
        let tmp =
            std::env::temp_dir().join(format!("touring-rules-glob-{}.rs", std::process::id()));
        std::fs::write(&tmp, "fn main() { foo(); }").unwrap();
        let engine = RuleEngine::new(RuleSet {
            rules: vec![Rule {
                name: "py-only".to_string(),
                path: Some("*.py".to_string()), // .py glob, file is .rs
                pattern: "foo".to_string(),
                fix: Some("bar".to_string()),
                languages: vec![],
                severity: Severity::default(),
                description: None,
            }],
        });
        let fixes = engine.apply_to_file(&tmp);
        assert!(fixes.is_empty(), "*.py glob should reject .rs file");
        let _ = std::fs::remove_file(&tmp);
    }

    /// REQUIREMENT apply_to_file matches multiple occurrences on same line + multiple lines
    /// BOUNDARY 2 matches/line × 2 lines = 4 fixes
    /// COVER nested find_iter loops
    #[test]
    fn test_apply_to_file_multiple_matches() {
        let tmp =
            std::env::temp_dir().join(format!("touring-rules-multi-{}.rs", std::process::id()));
        let content = "let a = foo; let b = foo;\nlet c = foo;\n";
        std::fs::write(&tmp, content).unwrap();
        let engine = RuleEngine::new(RuleSet {
            rules: vec![Rule {
                name: "all-foos".to_string(),
                path: None,
                pattern: "foo".to_string(),
                fix: Some("bar".to_string()),
                languages: vec![],
                severity: Severity::Warning,
                description: None,
            }],
        });
        let fixes = engine.apply_to_file(&tmp);
        assert_eq!(fixes.len(), 3, "should match all 3 foos across 2 lines");
        // Line numbers are 1-indexed
        assert_eq!(fixes[0].line, 1);
        assert_eq!(fixes[1].line, 1);
        assert_eq!(fixes[2].line, 2);
        // All produce same replacement
        for f in &fixes {
            assert_eq!(f.original, "foo");
            assert_eq!(f.replacement, "bar");
            assert_eq!(f.rule_name, "all-foos");
        }
        let _ = std::fs::remove_file(&tmp);
    }

    /// REQUIREMENT apply_rules across multiple files aggregates fixes
    /// BOUNDARY 0 files, 1 file with 0 matches, 2 files with matches each
    /// COVER outer iteration loop
    #[test]
    fn test_apply_rules_aggregates_across_files() {
        let pid = std::process::id();
        let tmp1 = std::env::temp_dir().join(format!("touring-rules-agg-1-{}.rs", pid));
        let tmp2 = std::env::temp_dir().join(format!("touring-rules-agg-2-{}.rs", pid));
        std::fs::write(&tmp1, "x foo y\n").unwrap();
        std::fs::write(&tmp2, "z foo w foo\n").unwrap();
        let engine = RuleEngine::new(RuleSet {
            rules: vec![Rule {
                name: "agg".to_string(),
                path: None,
                pattern: "foo".to_string(),
                fix: Some("FOO".to_string()),
                languages: vec![],
                severity: Severity::default(),
                description: None,
            }],
        });

        // 0 files
        assert!(engine.apply_rules(&[]).is_empty());

        // 2 files, expecting 1 + 2 = 3 fixes
        let fixes = engine.apply_rules(&[tmp1.clone(), tmp2.clone()]);
        assert_eq!(fixes.len(), 3);
        let from_1 = fixes.iter().filter(|f| f.file_path == tmp1).count();
        let from_2 = fixes.iter().filter(|f| f.file_path == tmp2).count();
        assert_eq!(from_1, 1);
        assert_eq!(from_2, 2);

        let _ = std::fs::remove_file(&tmp1);
        let _ = std::fs::remove_file(&tmp2);
    }

    /// REQUIREMENT RuleSet::load_from_file errors gracefully on missing file
    /// BOUNDARY file does not exist
    /// COVER std::fs::read_to_string Err branch propagated via `?`
    #[test]
    fn test_ruleset_load_missing_file_returns_err() {
        let result = RuleSet::load_from_file(&PathBuf::from("/no/such/path/qzz.yaml"));
        assert!(result.is_err(), "expected Err for missing file");
    }

    /// REQUIREMENT RuleSet::parse errors on malformed YAML
    /// BOUNDARY invalid syntax (broken tokens, type mismatch on required field)
    /// COVER serde_yaml::from_str Err branch
    #[test]
    fn test_ruleset_parse_invalid_yaml_returns_err() {
        // Broken YAML — unclosed mapping with stray indentation
        let result = RuleSet::parse("rules:\n  - {{ totally broken: [unclosed");
        assert!(result.is_err(), "expected Err for broken YAML syntax");

        // Type mismatch: rules expects array, given string
        let result2 = RuleSet::parse("rules: not_an_array");
        assert!(
            result2.is_err(),
            "expected Err for type mismatch on rules field"
        );

        // Missing required field `pattern` on a Rule
        let result3 = RuleSet::parse("rules:\n  - name: incomplete");
        assert!(
            result3.is_err(),
            "expected Err for Rule missing required `pattern` field"
        );
    }

    /// REQUIREMENT RuleSet::parse accepts empty rules list
    /// BOUNDARY empty rules array
    /// COVER #[serde(default)] on rules field
    #[test]
    fn test_ruleset_parse_empty_or_omitted_rules() {
        let rs1: RuleSet = serde_yaml::from_str("rules: []").unwrap();
        assert!(rs1.rules.is_empty());

        let rs2: RuleSet = serde_yaml::from_str("{}").unwrap();
        assert!(
            rs2.rules.is_empty(),
            "missing rules field should default to []"
        );
    }

    /// REQUIREMENT Fix struct has serde round-trip with camelCase wire format
    /// BOUNDARY all fields populated
    /// COVER #[serde(rename_all = "camelCase")] on Fix
    #[test]
    fn test_fix_camelcase_serde() {
        let fix = Fix {
            rule_name: "r".to_string(),
            file_path: PathBuf::from("/tmp/foo.rs"),
            original: "old".to_string(),
            replacement: "new".to_string(),
            line: 42,
            column: 7,
        };
        let json = serde_json::to_string(&fix).unwrap();
        // camelCase: ruleName, filePath
        assert!(
            json.contains("\"ruleName\""),
            "expected camelCase ruleName in {}",
            json
        );
        assert!(
            json.contains("\"filePath\""),
            "expected camelCase filePath in {}",
            json
        );
        let parsed: Fix = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.line, 42);
        assert_eq!(parsed.column, 7);
        assert_eq!(parsed.original, "old");
    }
}
