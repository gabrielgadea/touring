//! data — Embedded data files for the classifier.
//!
//! This module re-exports data from `universal_rules.json` and other
//! data files. These are embedded at compile time via `include_str!`.

/// Universal rules table, embedded at compile time from
/// `data/universal_rules.json`. The full Erickson argument-mining
/// taxonomy used by the pattern classifier.
pub const UNIVERSAL_RULES_JSON: &str = include_str!("data/universal_rules.json");
/// Category metadata (display names, group hierarchies), embedded
/// at compile time from `data/categories.json`.
pub const CATEGORIES_JSON: &str = include_str!("data/categories.json");
/// Scoring weights and tie-breaker rules, embedded at compile
/// time from `data/scoring.json`.
pub const SCORING_JSON: &str = include_str!("data/scoring.json");

/// Parse the embedded universal rules JSON. Returns the raw
/// `serde_json::Value` so callers can navigate the shape
/// without forcing a struct.
pub fn parse_universal_rules() -> Result<serde_json::Value, serde_json::Error> {
    serde_json::from_str(UNIVERSAL_RULES_JSON)
}

/// Parse the categories JSON. Returns the raw `serde_json::Value`
/// so callers can navigate the shape without forcing a struct.
pub fn parse_categories() -> Result<serde_json::Value, serde_json::Error> {
    serde_json::from_str(CATEGORIES_JSON)
}

/// Parse the scoring JSON. Returns the raw `serde_json::Value`
/// so callers can navigate the shape without forcing a struct.
pub fn parse_scoring() -> Result<serde_json::Value, serde_json::Error> {
    serde_json::from_str(SCORING_JSON)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_universal_rules_json_const_non_empty() {
        assert!(!UNIVERSAL_RULES_JSON.is_empty());
        assert!(!CATEGORIES_JSON.is_empty());
        assert!(!SCORING_JSON.is_empty());
    }

    #[test]
    fn test_parse_universal_rules_ok() {
        let v = parse_universal_rules().expect("universal_rules.json must parse");
        assert!(v.is_object() || v.is_array());
    }

    #[test]
    fn test_parse_categories_ok() {
        let v = parse_categories().expect("categories.json must parse");
        assert!(v.is_object() || v.is_array());
    }

    #[test]
    fn test_parse_scoring_ok() {
        let v = parse_scoring().expect("scoring.json must parse");
        assert!(v.is_object() || v.is_array());
    }
}
