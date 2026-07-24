//! TOML parser for [`super::types::MetricRuleSet`].
//!
//! A canonical rules file looks like:
//!
//! ```toml
//! version = "1.0"
//!
//! [[rule]]
//! name = "no-god-files"
//! applies_to = "**/*.rs"
//! metric = "file_lines"
//! op = "lt"
//! threshold = 1000
//! severity = "warn"
//! message = "files should be < 1000 LOC for cognitive limits"
//!
//! [[rule]]
//! name = "no-cycles"
//! metric = "cycle_count"
//! op = "eq"
//! threshold = 0
//! severity = "deny"
//! ```
//!
//! Rule names must be unique within a single set. The schema currently
//! supports only `version = "1.0"`; everything else triggers
//! [`super::error::RulesError::UnsupportedVersion`].

use std::collections::HashSet;
use std::fs;
use std::path::Path;

use super::error::{Result, RulesError};
use super::types::MetricRuleSet;

const SUPPORTED_VERSION: &str = "1.0";

/// Parse a [`MetricRuleSet`] from a TOML string.
///
/// # Errors
///
/// * [`RulesError::Parse`] for malformed TOML.
/// * [`RulesError::UnsupportedVersion`] if `version` is not supported.
/// * [`RulesError::Invalid`] for duplicate rule names or per-entity
///   metrics missing the `applies_to` field, etc.
pub fn parse_str(content: &str) -> Result<MetricRuleSet> {
    let raw: MetricRuleSet = toml::from_str(content)?;
    validate(&raw)?;
    Ok(raw)
}

/// Read and parse a TOML rules file from disk.
///
/// # Errors
///
/// [`RulesError::Read`] when the file cannot be read; otherwise the
/// errors documented for [`parse_str`].
pub fn parse_path(path: &Path) -> Result<MetricRuleSet> {
    let body = fs::read_to_string(path).map_err(|source| RulesError::Read {
        path: path.to_path_buf(),
        source,
    })?;
    parse_str(&body)
}

fn validate(set: &MetricRuleSet) -> Result<()> {
    if set.version != SUPPORTED_VERSION {
        return Err(RulesError::UnsupportedVersion {
            version: set.version.clone(),
        });
    }

    let mut seen: HashSet<&str> = HashSet::new();
    for rule in &set.rules {
        if !seen.insert(&rule.name) {
            return Err(RulesError::Invalid {
                rule: rule.name.clone(),
                reason: "duplicate rule name in this rule set".into(),
            });
        }
        if rule.metric.is_per_entity() && rule.applies_to.is_none() {
            return Err(RulesError::Invalid {
                rule: rule.name.clone(),
                reason: format!(
                    "metric `{}` is per-entity and requires an `applies_to` glob",
                    rule.metric.as_str()
                ),
            });
        }
        if rule.threshold.is_nan() {
            return Err(RulesError::Invalid {
                rule: rule.name.clone(),
                reason: "threshold must be a finite number, not NaN".into(),
            });
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rules::types::{MetricKind, Op, Severity};

    const GOOD: &str = r#"
version = "1.0"

[[rule]]
name = "no-god-files"
applies_to = "**/*.rs"
metric = "file_lines"
op = "lt"
threshold = 1000
severity = "warn"
message = "files should be < 1000 LOC"

[[rule]]
name = "no-cycles"
metric = "cycle_count"
op = "eq"
threshold = 0
severity = "deny"
"#;

    #[test]
    fn parses_valid_toml() {
        let rs = parse_str(GOOD).expect("parse");
        assert_eq!(rs.rules.len(), 2);
        assert_eq!(rs.rules[0].name, "no-god-files");
        assert!(matches!(rs.rules[0].metric, MetricKind::FileLines));
        assert!(matches!(rs.rules[0].op, Op::Lt));
        assert!(matches!(rs.rules[0].severity, Severity::Warn));
    }

    #[test]
    fn rejects_unsupported_version() {
        let bad = r#"version = "9.9""#;
        let err = parse_str(bad).unwrap_err();
        assert!(matches!(err, RulesError::UnsupportedVersion { .. }));
    }

    #[test]
    fn rejects_duplicate_rule_names() {
        let bad = r#"
version = "1.0"

[[rule]]
name = "x"
metric = "cycle_count"
op = "eq"
threshold = 0

[[rule]]
name = "x"
metric = "max_depth"
op = "lt"
threshold = 10
"#;
        let err = parse_str(bad).unwrap_err();
        match err {
            RulesError::Invalid { rule, reason } => {
                assert_eq!(rule, "x");
                assert!(reason.contains("duplicate"));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn rejects_per_entity_without_applies_to() {
        let bad = r#"
version = "1.0"

[[rule]]
name = "no-applies-to"
metric = "file_lines"
op = "lt"
threshold = 100
"#;
        let err = parse_str(bad).unwrap_err();
        match err {
            RulesError::Invalid { reason, .. } => {
                assert!(reason.contains("applies_to"));
            }
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn workspace_metric_does_not_require_applies_to() {
        let ok = r#"
version = "1.0"

[[rule]]
name = "no-cycles"
metric = "cycle_count"
op = "eq"
threshold = 0
"#;
        assert!(parse_str(ok).is_ok());
    }

    #[test]
    fn malformed_toml_yields_parse_error() {
        let bad = r#"version = "1.0
this is not valid toml"#;
        let err = parse_str(bad).unwrap_err();
        assert!(matches!(err, RulesError::Parse(_)));
    }

    #[test]
    fn empty_ruleset_parses() {
        let rs = parse_str(r#"version = "1.0""#).expect("parse");
        assert!(rs.rules.is_empty());
    }
}
