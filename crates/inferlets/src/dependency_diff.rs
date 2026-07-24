//! Dependency diff inferlet.
//!
//! Compares two Cargo.lock files to identify added, removed, and changed
//! crate versions. Useful for impact analysis in multi-crate workspaces.
//!
//! # Input JSON
//!
//! ```json
//! {
//!   "__inferlet__": "dependency_diff",
//!   "base_lock": "/path/to/base/Cargo.lock",
//!   "head_lock": "/path/to/head/Cargo.lock"
//! }
//! ```
//!
//! # Output JSON
//!
//! ```json
//! {
//!   "added": [{"crate": "foo", "version": "1.0"}],
//!   "removed": [{"crate": "bar", "version": "2.0"}],
//!   "changed": [{"crate": "baz", "old_version": "1.0", "new_version": "2.0"}]
//! }
//! ```

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// Thread-local error buffer for error propagation. (Doc comment kept as
// inner-line because thread_local! macro does not attach `///` to its
// generated `static`.)
thread_local! {
    static LAST_ERROR: RefCell<Option<String>> = const { RefCell::new(None) };
}

use std::cell::RefCell;

/// Input structure for dependency_diff inferlet.
#[derive(Debug, Deserialize)]
pub struct Input {
    /// Path to the baseline `Cargo.lock` (the "before" side of the diff).
    pub base_lock: String,
    /// Path to the head `Cargo.lock` (the "after" side of the diff).
    pub head_lock: String,
}

/// A crate entry with name and version.
#[derive(Debug, Serialize, Clone)]
pub struct CrateEntry {
    /// Name of the crate.
    pub crate_: String,
    /// Locked version of the crate.
    pub version: String,
}

/// A changed crate with old and new versions.
#[derive(Debug, Serialize)]
pub struct ChangedCrate {
    /// Name of the crate whose version changed.
    pub crate_: String,
    /// Version recorded in the base lock file.
    pub old_version: String,
    /// Version recorded in the head lock file.
    pub new_version: String,
}

/// Output structure for dependency_diff inferlet.
#[derive(Debug, Serialize)]
pub struct Output {
    /// Crates present in head but absent in base.
    pub added: Vec<CrateEntry>,
    /// Crates present in base but absent in head.
    pub removed: Vec<CrateEntry>,
    /// Crates present in both lock files with differing versions.
    pub changed: Vec<ChangedCrate>,
}

/// Parse a Cargo.lock file into a map of crate name -> version.
/// Returns None on malformed TOML.
fn parse_lock(lock_path: &str) -> Option<HashMap<String, String>> {
    let content = std::fs::read_to_string(lock_path).ok()?;
    let value: toml::Value = content.parse().ok()?;

    let packages = value.get("package")?.as_array()?;

    let mut result: HashMap<String, String> = HashMap::new();
    for pkg in packages {
        let name = pkg.get("name")?.as_str()?.to_string();
        let version = pkg.get("version")?.as_str()?.to_string();
        result.insert(name, version);
    }

    Some(result)
}

/// Compare two lock file maps and compute added/removed/changed.
fn diff_maps(
    base: &HashMap<String, String>,
    head: &HashMap<String, String>,
) -> (Vec<CrateEntry>, Vec<CrateEntry>, Vec<ChangedCrate>) {
    let mut added: Vec<CrateEntry> = Vec::new();
    let mut removed: Vec<CrateEntry> = Vec::new();
    let mut changed: Vec<ChangedCrate> = Vec::new();

    // Check added and changed in head
    for (name, new_version) in head {
        if let Some(old_version) = base.get(name) {
            if old_version != new_version {
                changed.push(ChangedCrate {
                    crate_: name.clone(),
                    old_version: old_version.clone(),
                    new_version: new_version.clone(),
                });
            }
        } else {
            added.push(CrateEntry {
                crate_: name.clone(),
                version: new_version.clone(),
            });
        }
    }

    // Check removed (in base but not in head)
    for (name, old_version) in base {
        if !head.contains_key(name) {
            removed.push(CrateEntry {
                crate_: name.clone(),
                version: old_version.clone(),
            });
        }
    }

    (added, removed, changed)
}

/// Raw evaluate — returns 1 if any differences found, 0 otherwise.
pub(crate) fn evaluate_raw(input: &str) -> i32 {
    let input = input.trim();
    let inp: Input = match serde_json::from_str(input) {
        Ok(v) => v,
        Err(_) => {
            LAST_ERROR.with(|cell| *cell.borrow_mut() = Some("invalid JSON input".to_string()));
            return 0;
        }
    };

    let base = match parse_lock(&inp.base_lock) {
        Some(m) => m,
        None => {
            LAST_ERROR.with(|cell| {
                *cell.borrow_mut() = Some(format!("failed to parse base_lock: {}", inp.base_lock))
            });
            return 0;
        }
    };

    let head = match parse_lock(&inp.head_lock) {
        Some(m) => m,
        None => {
            LAST_ERROR.with(|cell| {
                *cell.borrow_mut() = Some(format!("failed to parse head_lock: {}", inp.head_lock))
            });
            return 0;
        }
    };

    let (added, removed, changed) = diff_maps(&base, &head);

    if added.is_empty() && removed.is_empty() && changed.is_empty() {
        return 0;
    }

    let output = Output {
        added,
        removed,
        changed,
    };
    if let Ok(json) = serde_json::to_string(&output) {
        LAST_ERROR.with(|cell| *cell.borrow_mut() = Some(json));
    }

    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_lock_valid() {
        let base_map: HashMap<String, String> = vec![
            ("foo".to_string(), "1.0.0".to_string()),
            ("bar".to_string(), "2.1.0".to_string()),
        ]
        .into_iter()
        .collect();
        let head_map: HashMap<String, String> = vec![
            ("foo".to_string(), "1.0.0".to_string()),
            ("baz".to_string(), "3.0.0".to_string()),
        ]
        .into_iter()
        .collect();

        let (added, removed, changed) = diff_maps(&base_map, &head_map);

        // baz is new
        assert_eq!(added.len(), 1);
        assert_eq!(added[0].crate_, "baz");

        // bar is gone
        assert_eq!(removed.len(), 1);
        assert_eq!(removed[0].crate_, "bar");

        // nothing changed (foo same version)
        assert!(changed.is_empty());
    }

    #[test]
    fn test_diff_maps_changed() {
        let base_map: HashMap<String, String> = vec![("pkg".to_string(), "1.0.0".to_string())]
            .into_iter()
            .collect();
        let head_map: HashMap<String, String> = vec![("pkg".to_string(), "2.0.0".to_string())]
            .into_iter()
            .collect();

        let (_, _, changed) = diff_maps(&base_map, &head_map);

        assert_eq!(changed.len(), 1);
        assert_eq!(changed[0].crate_, "pkg");
        assert_eq!(changed[0].old_version, "1.0.0");
        assert_eq!(changed[0].new_version, "2.0.0");
    }

    #[test]
    fn test_diff_maps_no_changes() {
        let base_map: HashMap<String, String> = HashMap::new();
        let head_map: HashMap<String, String> = HashMap::new();

        let (added, removed, changed) = diff_maps(&base_map, &head_map);

        assert!(added.is_empty());
        assert!(removed.is_empty());
        assert!(changed.is_empty());
    }

    #[test]
    fn test_evaluate_raw_malformed_json() {
        let result = evaluate_raw("{ invalid");
        assert_eq!(result, 0);
    }
}
