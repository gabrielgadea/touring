//! Gotcha YAML loader — Wave Q3 (waves-Q-R-M-A-T-P plan, 2026-04-25).
//!
//! Source-of-truth for gotchas is now a YAML rule library under
//! `~/.claude/rust/docs/gotchas/`. SQLite acts as a derived cache populated
//! via [`sync_to_sqlite`].
//!
//! # Design
//!
//! - YAML files describe gotchas with stable IDs, language tags, patterns,
//!   severity, and resolution text (see `_schema.json` for the formal shape).
//! - [`load_yaml_gotchas`] walks the directory tree (skipping hidden dirs)
//!   and parses every `*.yaml` / `*.yml` file. Parse failures are returned
//!   as `(error, file)` tuples so callers can surface them — never panics.
//! - [`sync_to_sqlite`] iterates loaded gotchas and calls
//!   `HookRuntime.knowledge.add_gotcha_with_language()` for each. Existing
//!   entries (same `pattern + language`) get their `hit_count` incremented
//!   per the dedup invariant in `add_gotcha_with_language`.
//! - [`yaml_dir_hash`] computes a blake3 hash of all YAML contents
//!   (sorted by path) for cache-invalidation decisions.

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::runtime::HookRuntime;

/// On-disk YAML shape — single gotcha rule.
///
/// Mirrors `~/.claude/rust/docs/gotchas/_schema.json`. Optional metadata
/// is preserved as a free-form JSON object so future fields don't break
/// the loader.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct YamlGotcha {
    /// Stable identifier in the form `<lang>:<name>` (e.g. `rust:unwrap-in-prod`).
    pub id: String,
    /// Language tag — `rust|python|typescript|javascript|go|multi-lang`.
    pub language: String,
    /// Substring or regex used by `gotcha_match`.
    pub pattern: String,
    /// Human-readable summary (max 280 chars per schema).
    pub description: String,
    /// Severity bucket — `low|medium|high`.
    pub severity: String,
    /// Optional multi-line suggested fix.
    #[serde(default)]
    pub resolution: Option<String>,
    /// Free-form metadata (introduced date, references, etc).
    #[serde(default)]
    pub metadata: Option<serde_yaml::Value>,
}

/// Result of [`sync_to_sqlite`] — counts and per-rule outcomes.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SyncReport {
    /// Number of gotcha rules parsed from the YAML directory.
    pub total_yaml_loaded: usize,
    /// Number of rules successfully written to the SQLite gotcha table.
    pub synced_to_sqlite: usize,
    /// Identifiers of rules that failed to sync, with their error messages.
    pub failed: Vec<String>,
    /// Content hash of the YAML directory, used to skip re-syncing unchanged sources.
    pub yaml_dir_hash: String,
}

/// Walk a directory recursively and load every `*.yaml` / `*.yml` file as
/// a `YamlGotcha`.
///
/// Returns `(loaded, errors)`:
/// - `loaded` — successfully parsed gotchas in directory order
/// - `errors` — `(file_path, parse_error_message)` for each failure
///
/// Hidden dirs (`.git`, `.idea`, etc) are skipped. The `_schema.json` file
/// is ignored even if it has a `.json` extension.
#[must_use]
pub fn load_yaml_gotchas(dir: &Path) -> (Vec<YamlGotcha>, Vec<(PathBuf, String)>) {
    let mut loaded = Vec::new();
    let mut errors = Vec::new();
    let yaml_files = walk_yaml_files(dir);
    for path in yaml_files {
        let content = match std::fs::read_to_string(&path) {
            Ok(s) => s,
            Err(e) => {
                errors.push((path, format!("read failed: {e}")));
                continue;
            }
        };
        match serde_yaml::from_str::<YamlGotcha>(&content) {
            Ok(g) => loaded.push(g),
            Err(e) => errors.push((path, format!("parse failed: {e}"))),
        }
    }
    (loaded, errors)
}

/// Sync loaded gotchas into the SQLite cache via `add_gotcha_with_language`.
///
/// Returns a [`SyncReport`] with counts. Each `add_gotcha_with_language`
/// failure is captured as a string in `failed` (rule id + error) — the
/// sync continues for remaining rules.
pub fn sync_to_sqlite(rt: &mut HookRuntime, dir: &Path) -> SyncReport {
    let (loaded, parse_errors) = load_yaml_gotchas(dir);
    let yaml_dir_hash = compute_yaml_dir_hash(dir);
    let mut synced = 0usize;
    let mut failed: Vec<String> = parse_errors
        .iter()
        .map(|(p, e)| format!("{}: {e}", p.display()))
        .collect();

    for g in &loaded {
        match rt.ctx.knowledge.add_gotcha_with_language(
            &g.pattern,
            &g.description,
            &g.severity,
            None, // symbol_name — gotchas are pattern-based, not symbol-bound
            Some(&g.language),
        ) {
            Ok(_) => synced += 1,
            Err(e) => failed.push(format!("{}: {e}", g.id)),
        }
    }

    SyncReport {
        total_yaml_loaded: loaded.len(),
        synced_to_sqlite: synced,
        failed,
        yaml_dir_hash,
    }
}

/// Compute a blake3 hash over all YAML files in `dir` (sorted by path).
/// Used by the loader to detect when the on-disk corpus has changed and
/// the SQLite cache needs re-sync.
#[must_use]
pub fn yaml_dir_hash(dir: &Path) -> String {
    compute_yaml_dir_hash(dir)
}

fn compute_yaml_dir_hash(dir: &Path) -> String {
    let mut paths = walk_yaml_files(dir);
    paths.sort();
    let mut hasher = blake3::Hasher::new();
    for p in &paths {
        if let Some(s) = p.to_str() {
            hasher.update(s.as_bytes());
        }
        if let Ok(content) = std::fs::read(p) {
            hasher.update(&content);
        }
    }
    hasher.finalize().to_hex().to_string()
}

fn walk_yaml_files(root: &Path) -> Vec<PathBuf> {
    fn is_skipped_dir(name: &std::ffi::OsStr) -> bool {
        let n = name.to_string_lossy();
        n.starts_with('.') || n == "target" || n == "node_modules"
    }
    let mut out = Vec::new();
    let mut stack = vec![root.to_path_buf()];
    while let Some(dir) = stack.pop() {
        let entries = match std::fs::read_dir(&dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let path = entry.path();
            let file_type = match entry.file_type() {
                Ok(t) => t,
                Err(_) => continue,
            };
            if file_type.is_dir() {
                if let Some(name) = path.file_name() {
                    if !is_skipped_dir(name) {
                        stack.push(path);
                    }
                }
            } else if file_type.is_file() {
                let is_yaml = path
                    .extension()
                    .and_then(|e| e.to_str())
                    .is_some_and(|e| e == "yaml" || e == "yml");
                if is_yaml {
                    out.push(path);
                }
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    fn temp_dir(name: &str) -> PathBuf {
        let mut d = std::env::temp_dir();
        d.push(format!("touring-gotcha-test-{}-{name}", std::process::id()));
        let _ = std::fs::create_dir_all(&d);
        d
    }

    fn write_yaml(dir: &Path, name: &str, content: &str) -> PathBuf {
        let p = dir.join(name);
        let mut f = std::fs::File::create(&p).expect("create yaml");
        f.write_all(content.as_bytes()).expect("write yaml");
        p
    }

    const VALID_YAML: &str = "id: test:foo
language: rust
pattern: foo
description: test gotcha
severity: high
";

    #[test]
    fn load_yaml_gotchas_empty_dir_returns_empty() {
        let dir = temp_dir("empty");
        let (loaded, errors) = load_yaml_gotchas(&dir);
        assert!(loaded.is_empty());
        assert!(errors.is_empty());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_yaml_gotchas_parses_single_valid_file() {
        let dir = temp_dir("single");
        write_yaml(&dir, "g1.yaml", VALID_YAML);
        let (loaded, errors) = load_yaml_gotchas(&dir);
        assert_eq!(loaded.len(), 1);
        assert!(errors.is_empty());
        assert_eq!(loaded[0].id, "test:foo");
        assert_eq!(loaded[0].language, "rust");
        assert_eq!(loaded[0].severity, "high");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_yaml_gotchas_walks_subdirs() {
        let dir = temp_dir("nested");
        let sub = dir.join("rust");
        let _ = std::fs::create_dir_all(&sub);
        write_yaml(&sub, "g1.yaml", VALID_YAML);
        let (loaded, _) = load_yaml_gotchas(&dir);
        assert_eq!(loaded.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_yaml_gotchas_handles_yml_extension() {
        let dir = temp_dir("yml-ext");
        write_yaml(&dir, "g1.yml", VALID_YAML);
        let (loaded, _) = load_yaml_gotchas(&dir);
        assert_eq!(loaded.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_yaml_gotchas_records_parse_errors() {
        let dir = temp_dir("parse-err");
        write_yaml(&dir, "bad.yaml", "this is :: not :: valid :: yaml ::: ::");
        let (loaded, errors) = load_yaml_gotchas(&dir);
        assert!(loaded.is_empty(), "should not load malformed yaml");
        assert_eq!(errors.len(), 1);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_yaml_gotchas_skips_hidden_dirs() {
        let dir = temp_dir("hidden");
        let hidden = dir.join(".git");
        let _ = std::fs::create_dir_all(&hidden);
        write_yaml(&hidden, "leaked.yaml", VALID_YAML);
        let (loaded, _) = load_yaml_gotchas(&dir);
        assert!(loaded.is_empty(), "should skip .git/ contents");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_yaml_gotchas_ignores_non_yaml_files() {
        let dir = temp_dir("mixed");
        write_yaml(&dir, "g1.yaml", VALID_YAML);
        write_yaml(&dir, "README.md", "# not yaml");
        write_yaml(&dir, "_schema.json", "{}");
        let (loaded, _) = load_yaml_gotchas(&dir);
        assert_eq!(loaded.len(), 1, "should only load .yaml/.yml");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn yaml_dir_hash_stable_for_same_content() {
        let dir = temp_dir("hash-stable");
        write_yaml(&dir, "g1.yaml", VALID_YAML);
        let h1 = yaml_dir_hash(&dir);
        let h2 = yaml_dir_hash(&dir);
        assert_eq!(h1, h2, "same content must hash identically");
        assert_eq!(h1.len(), 64, "blake3 hex hash is 64 chars");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn yaml_dir_hash_changes_on_content_change() {
        let dir = temp_dir("hash-change");
        write_yaml(&dir, "g1.yaml", VALID_YAML);
        let h1 = yaml_dir_hash(&dir);
        write_yaml(&dir, "g1.yaml", &VALID_YAML.replace("foo", "bar"));
        let h2 = yaml_dir_hash(&dir);
        assert_ne!(h1, h2, "different content must hash differently");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn yaml_dir_hash_empty_dir_returns_well_formed() {
        let dir = temp_dir("hash-empty");
        let h = yaml_dir_hash(&dir);
        assert_eq!(h.len(), 64);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn load_real_bundle_at_canonical_path() {
        let canonical = Path::new("/home/gabrielgadea/.claude/rust/docs/gotchas");
        if !canonical.is_dir() {
            // Skip if bundle not present (CI sandboxes etc).
            return;
        }
        let (loaded, errors) = load_yaml_gotchas(canonical);
        assert!(
            loaded.len() >= 6,
            "canonical bundle must contain >=6 gotchas, got {}",
            loaded.len()
        );
        assert!(
            errors.is_empty(),
            "canonical bundle should not have parse errors: {errors:?}"
        );
        // Sanity: every loaded gotcha must have non-empty pattern and description
        for g in &loaded {
            assert!(!g.pattern.is_empty(), "{}: empty pattern", g.id);
            assert!(!g.description.is_empty(), "{}: empty description", g.id);
            assert!(
                ["low", "medium", "high"].contains(&g.severity.as_str()),
                "{}: invalid severity {}",
                g.id,
                g.severity
            );
        }
    }
}
