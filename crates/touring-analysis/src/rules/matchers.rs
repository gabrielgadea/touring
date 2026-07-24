//! Glob matching helpers for `applies_to` patterns.
//!
//! Backed by the `glob` crate (already widely used elsewhere in the
//! workspace). The wrapper in this module keeps callers in
//! [`super::evaluator`] unaware of the underlying engine and gives us a
//! single point to validate the pattern up-front (so a malformed glob
//! produces a typed [`super::error::RulesError::Glob`] rather than a
//! silent no-match at evaluation time).

use super::error::{Result, RulesError};

/// Compile and match `path` against `glob`, returning a typed error when
/// the pattern is malformed.
///
/// `rule_name` is included only to make the resulting error friendly
/// (so users know which rule's pattern was wrong).
///
/// # Errors
///
/// [`RulesError::Glob`] if `glob` fails to compile.
pub fn matches_glob(rule_name: &str, glob: &str, path: &str) -> Result<bool> {
    let pat = ::glob::Pattern::new(glob).map_err(|source| RulesError::Glob {
        rule: rule_name.to_string(),
        glob: glob.to_string(),
        source,
    })?;
    Ok(pat.matches(path))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn star_star_matches_nested() {
        assert!(matches_glob("r", "**/*.rs", "src/foo/bar.rs").unwrap());
        assert!(matches_glob("r", "**/*.rs", "lib.rs").unwrap());
    }

    #[test]
    fn directory_anchored_glob() {
        assert!(matches_glob("r", "src/handlers/**/*.rs", "src/handlers/inner/x.rs").unwrap());
        assert!(!matches_glob("r", "src/handlers/**/*.rs", "src/other/x.rs").unwrap());
    }

    #[test]
    fn malformed_glob_yields_typed_error() {
        let err = matches_glob("r", "[invalid", "anywhere").unwrap_err();
        match err {
            RulesError::Glob { rule, .. } => assert_eq!(rule, "r"),
            other => panic!("unexpected: {other:?}"),
        }
    }

    #[test]
    fn exact_path_match() {
        assert!(matches_glob("r", "Cargo.toml", "Cargo.toml").unwrap());
        assert!(!matches_glob("r", "Cargo.toml", "src/Cargo.toml").unwrap());
    }
}
