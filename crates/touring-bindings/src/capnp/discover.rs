//! Filesystem walker that mirrors `holon.py::discover_holons` in Rust.
//!
//! Given a holarchy root, enumerates every `.holon/manifest.toml` under
//! it (up to `DEFAULT_DEPTH` levels), parses each TOML file, and returns
//! a list of [`ManifestRecord`] values suitable for projecting into the
//! generated Cap'n Proto builders.
//!
//! Noise directories (`.git`, `node_modules`, `target`, `build`, …) are
//! pruned aggressively so that huge repos (analise = 112G) finish in
//! reasonable time.

use std::path::{Path, PathBuf};

use serde::Deserialize;
use walkdir::{DirEntry, WalkDir};

/// Depth limit mirrors Python's `DEFAULT_DEPTH_LIMIT = 6`.
pub const DEFAULT_DEPTH: usize = 6;

/// Directory base names that should never be descended into.
const NOISY: &[&str] = &[
    ".git",
    "node_modules",
    "target",
    "build",
    "dist",
    "__pycache__",
    ".venv",
    "venv",
    ".tox",
    ".mypy_cache",
    ".pytest_cache",
    ".ruff_cache",
];

// ---------------------------------------------------------------------------
// Record types (one-shot — not retained after we emit a Cap'n Proto reply).
// ---------------------------------------------------------------------------

/// One capability offered by a holon.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct CapabilityRecord {
    /// Capability identifier (the `[holon.offers.<name>]` key).
    pub name: String,
    /// Schema reference describing the capability's payload contract.
    #[serde(default)]
    pub schema: String,
    /// Adapter kind used to invoke the capability (defaults to `cli`).
    #[serde(default = "default_adapter")]
    pub adapter: String,
    /// Shell command line the `cli` adapter executes for this capability.
    #[serde(rename = "adapter_cmd", default)]
    pub adapter_cmd: String,
    /// Unix socket path for the `capnp` adapter transport, when used.
    #[serde(default)]
    pub capnp_socket: String,
    /// WASM component path for the `wasm` adapter transport, when used.
    #[serde(default)]
    pub wasm_component: String,
    /// Semantic version of the offered capability (defaults to `0.1.0`).
    #[serde(default = "default_version")]
    pub version: String,
}

/// One capability required by a holon.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct RequireRecord {
    /// Required capability identifier (the `[holon.requires.<name>]` key).
    pub name: String,
    /// Whether the dependency is optional (defaults to `true`).
    #[serde(default = "default_true")]
    pub optional: bool,
    /// Fallback capability or command used when the requirement is unmet.
    #[serde(default)]
    pub fallback: String,
    /// Minimum acceptable semantic version of the required capability.
    #[serde(default)]
    pub min_version: String,
}

/// A parsed manifest ready to be flattened into a Cap'n Proto struct.
#[derive(Debug, Clone)]
pub struct ManifestRecord {
    /// Holon identity name from `[holon.identity]`.
    pub name: String,
    /// Holon semantic version (defaults to `0.1.0` when absent).
    pub version: String,
    /// Human-readable description of the holon.
    pub description: String,
    /// Filesystem path to the source `.holon/manifest.toml`.
    pub manifest_path: PathBuf,
    /// Capabilities this holon offers, sorted by name for deterministic output.
    pub offers: Vec<CapabilityRecord>,
    /// Capabilities this holon requires, sorted by name for deterministic output.
    pub requires: Vec<RequireRecord>,
    /// Whether the holon asserts an autonomy guarantee (defaults to `true`).
    pub autonomy_guarantee: bool,
}

fn default_adapter() -> String {
    "cli".to_string()
}
fn default_version() -> String {
    "0.1.0".to_string()
}
fn default_true() -> bool {
    true
}

// ---------------------------------------------------------------------------
// TOML deserialization shape — mirrors HolonManifest semantics from holon.py.
// ---------------------------------------------------------------------------

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct RawManifest {
    holon: RawHolon,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct RawHolon {
    identity: RawIdentity,
    #[serde(default)]
    offers: std::collections::BTreeMap<String, toml::Value>,
    #[serde(default)]
    requires: std::collections::BTreeMap<String, toml::Value>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(default)]
struct RawIdentity {
    name: String,
    version: String,
    description: String,
    #[serde(default = "default_true")]
    autonomy_guarantee: bool,
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Walk `root` and return every parseable `.holon/manifest.toml`.
///
/// Malformed TOML is silently skipped (matches the Python behaviour —
/// a single broken manifest must not crash discovery for the whole
/// holarchy).
pub fn discover_holons(root: &Path, depth_limit: usize) -> Vec<ManifestRecord> {
    if !root.exists() {
        return Vec::new();
    }

    let mut out = Vec::new();
    for entry in WalkDir::new(root)
        .max_depth(depth_limit)
        .follow_links(false)
        .into_iter()
        .filter_entry(|e| !is_noisy(e))
    {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };
        if !entry.file_type().is_file() {
            continue;
        }
        let path = entry.path();
        if !ends_with_manifest(path) {
            continue;
        }
        if let Ok(record) = load_manifest(path) {
            out.push(record);
        }
    }
    out
}

/// Find all holons whose `offers` contain `capability`.
pub fn providers_of<'a>(holons: &'a [ManifestRecord], capability: &str) -> Vec<&'a ManifestRecord> {
    holons
        .iter()
        .filter(|m| m.offers.iter().any(|o| o.name == capability))
        .collect()
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn is_noisy(entry: &DirEntry) -> bool {
    if !entry.file_type().is_dir() {
        return false;
    }
    entry
        .file_name()
        .to_str()
        .map(|name| NOISY.iter().any(|n| n == &name))
        .unwrap_or(false)
}

fn ends_with_manifest(path: &Path) -> bool {
    // We want ".../.holon/manifest.toml"
    path.file_name()
        .map(|n| n == "manifest.toml")
        .unwrap_or(false)
        && path
            .parent()
            .and_then(|p| p.file_name())
            .map(|n| n == ".holon")
            .unwrap_or(false)
}

fn load_manifest(path: &Path) -> Result<ManifestRecord, anyhow::Error> {
    let content = std::fs::read_to_string(path)?;
    let raw: RawManifest = toml::from_str(&content)?;

    let mut offers = Vec::with_capacity(raw.holon.offers.len());
    for (name, val) in raw.holon.offers {
        let mut rec: CapabilityRecord = val
            .try_into()
            .unwrap_or_else(|_| CapabilityRecord::default());
        rec.name = name;
        if rec.version.is_empty() {
            rec.version = default_version();
        }
        if rec.adapter.is_empty() {
            rec.adapter = default_adapter();
        }
        offers.push(rec);
    }

    let mut requires = Vec::with_capacity(raw.holon.requires.len());
    for (name, val) in raw.holon.requires {
        let mut rec: RequireRecord = val.try_into().unwrap_or_else(|_| RequireRecord::default());
        rec.name = name;
        requires.push(rec);
    }

    // Sort for deterministic output (consumers rely on stable ordering).
    offers.sort_by(|a, b| a.name.cmp(&b.name));
    requires.sort_by(|a, b| a.name.cmp(&b.name));

    Ok(ManifestRecord {
        name: raw.holon.identity.name,
        version: if raw.holon.identity.version.is_empty() {
            default_version()
        } else {
            raw.holon.identity.version
        },
        description: raw.holon.identity.description,
        manifest_path: path.to_path_buf(),
        offers,
        requires,
        autonomy_guarantee: raw.holon.identity.autonomy_guarantee,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::fs;
    use tempfile::TempDir;

    fn write_manifest(root: &Path, name: &str, body: &str) {
        let holon_dir = root.join(".holon");
        fs::create_dir_all(&holon_dir).unwrap();
        fs::write(holon_dir.join("manifest.toml"), body).unwrap();
        let _ = name; // reserved for future disambiguation if needed
    }

    #[test]
    fn discover_returns_empty_for_missing_root() {
        assert!(discover_holons(Path::new("/nonexistent/xyz"), DEFAULT_DEPTH).is_empty());
    }

    #[test]
    fn discover_finds_single_manifest() {
        let tmp = TempDir::new().unwrap();
        write_manifest(
            tmp.path(),
            "demo",
            r#"
                [holon.identity]
                name = "demo"
                version = "1.2.3"
                description = "demo holon"
                autonomy_guarantee = true

                [holon.offers.foo]
                adapter = "cli"
                adapter_cmd = "echo foo"
                version = "1.0.0"
            "#,
        );
        let found = discover_holons(tmp.path(), DEFAULT_DEPTH);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "demo");
        assert_eq!(found[0].version, "1.2.3");
        assert_eq!(found[0].offers.len(), 1);
        assert_eq!(found[0].offers[0].name, "foo");
        assert_eq!(found[0].offers[0].adapter_cmd, "echo foo");
    }

    #[test]
    fn discover_skips_noisy_dirs() {
        let tmp = TempDir::new().unwrap();
        // Place a manifest inside node_modules — must NOT be discovered.
        let hidden = tmp.path().join("node_modules").join("pkg");
        fs::create_dir_all(&hidden).unwrap();
        write_manifest(
            &hidden,
            "hidden",
            r#"[holon.identity]
name = "hidden"
"#,
        );
        let found = discover_holons(tmp.path(), DEFAULT_DEPTH);
        assert!(found.is_empty(), "got {found:#?}");
    }

    #[test]
    fn discover_skips_malformed_toml() {
        let tmp = TempDir::new().unwrap();
        // Good manifest
        let ok_dir = tmp.path().join("good");
        fs::create_dir_all(&ok_dir).unwrap();
        write_manifest(
            &ok_dir,
            "good",
            r#"
            [holon.identity]
            name = "good"
        "#,
        );
        // Broken manifest
        let bad_dir = tmp.path().join("bad");
        fs::create_dir_all(&bad_dir).unwrap();
        write_manifest(&bad_dir, "bad", "this = is not valid toml {[");

        let found = discover_holons(tmp.path(), DEFAULT_DEPTH);
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].name, "good");
    }

    #[test]
    fn providers_filters_by_capability() {
        let tmp = TempDir::new().unwrap();
        let a = tmp.path().join("a");
        let b = tmp.path().join("b");
        fs::create_dir_all(&a).unwrap();
        fs::create_dir_all(&b).unwrap();
        write_manifest(
            &a,
            "a",
            r#"
                [holon.identity]
                name = "a"
                [holon.offers.shared]
                adapter = "cli"
                adapter_cmd = "true"
            "#,
        );
        write_manifest(
            &b,
            "b",
            r#"
                [holon.identity]
                name = "b"
                [holon.offers.other]
                adapter = "cli"
                adapter_cmd = "true"
            "#,
        );
        let holons = discover_holons(tmp.path(), DEFAULT_DEPTH);
        let matches = providers_of(&holons, "shared");
        assert_eq!(matches.len(), 1);
        assert_eq!(matches[0].name, "a");
    }
}
