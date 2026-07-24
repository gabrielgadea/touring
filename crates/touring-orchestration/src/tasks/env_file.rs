//! Environment file loader — parses `.env` files with hierarchy support.
//!
//! ## Priority order (highest → lowest)
//!
//! | File | When |
//! |------|------|
//! | `.env.local` | Local overrides — never committed |
//! | `.env.<NODE_ENV>` | e.g., `.env.production` |
//! | `.env` | Base defaults |
//!
//! Files are merged with later entries overriding earlier ones.

use crate::tasks::error::Result;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Parse a `.env` file into a map of key-value pairs.
///
/// Supports formats:
/// - `KEY=value`
/// - `KEY="value"` (double-quoted)
/// - `KEY='value'` (single-quoted)
/// - `# comments` (ignored)
/// - `KEY=` (empty value)
/// - `export KEY=value` (export prefix, ignored)
pub fn parse_env_file(path: impl AsRef<Path>) -> Result<HashMap<String, String>> {
    let content = fs::read_to_string(path.as_ref())?;
    parse_env_content(&content)
}

/// Parse `.env` content string into key-value map.
pub fn parse_env_content(content: &str) -> Result<HashMap<String, String>> {
    let mut vars = HashMap::new();

    for line in content.lines() {
        let line = line.trim();

        // Skip empty lines and comments
        if line.is_empty() || line.starts_with('#') {
            continue;
        }

        // Strip `export ` prefix if present
        let line = line.strip_prefix("export ").unwrap_or(line);

        // Split on first `=` to get key/value
        if let Some(eq_pos) = line.find('=') {
            let key = line[..eq_pos].trim().to_string();
            let mut value = line[eq_pos + 1..].trim();

            // Strip surrounding quotes
            if (value.starts_with('"') && value.ends_with('"'))
                || (value.starts_with('\'') && value.ends_with('\''))
            {
                value = &value[1..value.len() - 1];
            }

            vars.insert(key, value.to_string());
        }
    }

    Ok(vars)
}

/// Load environment files with hierarchy merging.
///
/// `paths` are evaluated in order — later files override earlier ones.
pub fn load_env_hierarchy(paths: &[String]) -> Result<HashMap<String, String>> {
    let mut merged = HashMap::new();

    for path in paths {
        if let Ok(vars) = parse_env_file(path) {
            merged.extend(vars);
        }
        // Silently skip missing files — allows optional env files
    }

    Ok(merged)
}

/// Resolve an env file path relative to a base directory.
pub fn resolve_env_path(base_dir: &Path, env_file: &str) -> PathBuf {
    if env_file.starts_with('/') {
        PathBuf::from(env_file)
    } else {
        base_dir.join(env_file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_basic() {
        let content = r#"
            # Comment line
            RUST_LOG=info
            DATABASE_URL=postgres://localhost/db
            "#;
        let vars = parse_env_content(content).unwrap();
        assert_eq!(vars.get("RUST_LOG"), Some(&"info".to_string()));
        assert_eq!(
            vars.get("DATABASE_URL"),
            Some(&"postgres://localhost/db".to_string())
        );
    }

    #[test]
    fn test_parse_quoted_values() {
        let content = r#"
            SAMPLE_NAME="placeholder value"
            KEY='single quoted'
            PLAIN=noquotes
        "#;
        let vars = parse_env_content(content).unwrap();
        // double-quoted (content was secret-redacted SECRET→SAMPLE_NAME; assert follows)
        assert_eq!(
            vars.get("SAMPLE_NAME"),
            Some(&"placeholder value".to_string())
        );
        assert_eq!(vars.get("KEY"), Some(&"single quoted".to_string()));
        assert_eq!(vars.get("PLAIN"), Some(&"noquotes".to_string()));
    }

    #[test]
    fn test_parse_export_prefix() {
        let content = "export RUST_LOG=debug\nexport FOO=bar";
        let vars = parse_env_content(content).unwrap();
        assert_eq!(vars.get("RUST_LOG"), Some(&"debug".to_string()));
        assert_eq!(vars.get("FOO"), Some(&"bar".to_string()));
    }

    #[test]
    fn test_empty_value() {
        let content = "EMPTY_KEY=\nALSO_EMPTY=";
        let vars = parse_env_content(content).unwrap();
        assert_eq!(vars.get("EMPTY_KEY"), Some(&"".to_string()));
        assert_eq!(vars.get("ALSO_EMPTY"), Some(&"".to_string()));
    }

    #[test]
    fn test_hierarchy_merge() {
        let base = parse_env_content("A=1\nB=2").unwrap();
        let override_ = parse_env_content("B=3\nC=4").unwrap();

        let mut merged = base;
        merged.extend(override_);

        assert_eq!(merged.get("A"), Some(&"1".to_string())); // from base
        assert_eq!(merged.get("B"), Some(&"3".to_string())); // overridden
        assert_eq!(merged.get("C"), Some(&"4".to_string())); // from override
    }
}
