//! Inferlet manifest schema for install/list/validate lifecycle.
//!
//! Manifest format: TOML
//! ```toml
//! [inferlet]
//! name = "pattern"
//! version = "1.0.0"
//! description = "Pattern matching inferlet"
//! wasm_uri = "https://example.com/pattern.wasm"
//! wasm_sha256 = "abc123..."
//! validate_input = true
//! input_json_schema = '''{ "type": "object", ... }'''
//! max_fuel = 10_000_000
//! max_stack_size = 1_048_576
//!
//! [[inferlet.dependencies]]
//! name = "touring-wasm"
//! version = ">=1.0.0"
//!
//! [inferlet.metadata]
//! tags = ["pattern", "nlp"]
//! author = "Kazuba"
//! license = "MIT"
//! ```

use sha2::{Digest, Sha256};
use std::path::Path;

/// A single inferlet dependency declared in the manifest.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct InferletDep {
    /// Name of the depended-upon inferlet or library.
    pub name: String,
    /// Required version (a semver requirement string).
    pub version: String,
}

/// Optional descriptive metadata attached to an inferlet manifest.
#[derive(Debug, Clone, PartialEq, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct InferletMetadata {
    /// Free-form classification tags.
    pub tags: Vec<String>,
    /// Author or maintainer of the inferlet.
    pub author: Option<String>,
    /// SPDX license identifier.
    pub license: Option<String>,
}

impl InferletMetadata {
    /// Construct empty metadata (no tags, author, or license).
    pub fn new() -> Self {
        Self {
            tags: Vec::new(),
            author: None,
            license: None,
        }
    }
}

/// Full manifest describing one installable WASM inferlet.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct InferletManifest {
    /// Unique inferlet name.
    pub name: String,
    /// Semantic version of the inferlet.
    pub version: String,
    /// Human-readable description of what the inferlet does.
    pub description: String,
    /// Optional URI from which the WASM module is fetched.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub wasm_uri: Option<String>,
    /// Expected SHA-256 hex digest of the WASM module (integrity check).
    pub wasm_sha256: String,
    /// Optional SHA-256 hex digest of the manifest itself.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub manifest_sha256: Option<String>,
    /// Maximum wasmtime fuel budget per evaluation.
    #[serde(default)]
    pub max_fuel: Option<u64>,
    /// Maximum WASM stack size in bytes.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub max_stack_size: Option<usize>,
    /// Whether input should be validated against `input_json_schema`.
    #[serde(default)]
    pub validate_input: bool,
    /// Optional JSON Schema used to validate input when `validate_input` is set.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub input_json_schema: Option<String>,
    /// Other inferlets this one depends on.
    #[serde(default)]
    pub dependencies: Vec<InferletDep>,
    /// Descriptive metadata (tags, author, license).
    #[serde(default)]
    pub metadata: InferletMetadata,
}

/// Errors that can occur while loading or validating a manifest.
#[derive(Debug)]
pub enum InferletManifestLoadError {
    /// Failure reading the manifest file from disk.
    Io(std::io::Error),
    /// Failure parsing the manifest TOML.
    Toml(toml::de::Error),
    /// The `wasm_sha256` field is not a valid 64-char hex digest.
    InvalidSha256(String),
}

impl std::fmt::Display for InferletManifestLoadError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(e) => write!(f, "IO error: {}", e),
            Self::Toml(e) => write!(f, "TOML parse error: {}", e),
            Self::InvalidSha256(s) => write!(f, "Invalid sha256: {}", s),
        }
    }
}

impl std::error::Error for InferletManifestLoadError {}

impl From<std::io::Error> for InferletManifestLoadError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e)
    }
}

impl From<toml::de::Error> for InferletManifestLoadError {
    fn from(e: toml::de::Error) -> Self {
        Self::Toml(e)
    }
}

impl InferletManifest {
    /// Read and validate a manifest from a TOML file at `path`.
    pub fn load_from_path(path: &Path) -> Result<Self, InferletManifestLoadError> {
        let content = std::fs::read_to_string(path)?;
        let manifest: InferletManifest = toml::from_str(&content)?;
        manifest.validate_format()?;
        Ok(manifest)
    }

    /// Parse and validate a manifest from a TOML string.
    pub fn load_from_str(s: &str) -> Result<Self, InferletManifestLoadError> {
        let manifest: InferletManifest = toml::from_str(s)?;
        manifest.validate_format()?;
        Ok(manifest)
    }

    /// Check that `wasm_sha256` is a well-formed 64-char hex digest.
    pub fn validate_format(&self) -> Result<(), InferletManifestLoadError> {
        if self.wasm_sha256.len() != 64 || !self.wasm_sha256.chars().all(|c| c.is_ascii_hexdigit())
        {
            return Err(InferletManifestLoadError::InvalidSha256(
                self.wasm_sha256.clone(),
            ));
        }
        Ok(())
    }

    /// Verify that `wasm_bytes` hashes to the manifest's `wasm_sha256`.
    pub fn validate_wasm(&self, wasm_bytes: &[u8]) -> bool {
        let computed = Sha256::digest(wasm_bytes);
        let computed_hex = format!("{:x}", computed);
        computed_hex == self.wasm_sha256
    }

    /// Default wasmtime fuel budget when a manifest omits `max_fuel`.
    pub fn wasm_default_fuel() -> u64 {
        10_000_000
    }
    /// Default WASM stack size (1 MiB) when a manifest omits `max_stack_size`.
    pub fn wasm_default_stack() -> usize {
        1024 * 1024
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const VALID_SHA256: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

    fn minimal_toml() -> String {
        format!(
            r#"name = "test"
version = "1.0.0"
description = "A test inferlet"
wasm_sha256 = "{}"
"#,
            VALID_SHA256
        )
    }

    fn full_toml() -> String {
        format!(
            r#"name = "pattern"
version = "1.0.0"
description = "Pattern matching inferlet"
wasm_uri = "https://example.com/pattern.wasm"
wasm_sha256 = "{}"
validate_input = true
input_json_schema = "{{ \"type\": \"object\" }}"
max_fuel = 10_000_000
max_stack_size = 1_048_576

[[dependencies]]
name = "touring-wasm"
version = ">=1.0.0"

[metadata]
tags = ["pattern", "nlp"]
author = "Kazuba"
license = "MIT"
"#,
            VALID_SHA256
        )
    }

    #[test]
    fn test_minimal_manifest() {
        let toml_str = minimal_toml();
        let manifest = InferletManifest::load_from_str(&toml_str).unwrap();
        assert_eq!(manifest.name, "test");
        assert_eq!(manifest.version, "1.0.0");
        assert!(manifest.dependencies.is_empty());
        assert!(manifest.metadata.tags.is_empty());
    }

    #[test]
    fn test_full_manifest() {
        let toml_str = full_toml();
        let manifest = InferletManifest::load_from_str(&toml_str).unwrap();
        assert_eq!(manifest.name, "pattern");
        assert_eq!(manifest.version, "1.0.0");
        assert_eq!(
            manifest.wasm_uri,
            Some("https://example.com/pattern.wasm".to_string())
        );
        assert_eq!(manifest.max_fuel, Some(10_000_000));
        assert_eq!(manifest.max_stack_size, Some(1_048_576));
        assert!(manifest.validate_input);
        assert_eq!(manifest.dependencies.len(), 1);
        assert_eq!(manifest.dependencies[0].name, "touring-wasm");
        assert_eq!(manifest.metadata.tags, vec!["pattern", "nlp"]);
        assert_eq!(manifest.metadata.author, Some("Kazuba".to_string()));
        assert_eq!(manifest.metadata.license, Some("MIT".to_string()));
    }

    #[test]
    fn test_invalid_sha256() {
        let toml_str = r#"name = "test"
version = "1.0.0"
description = "A test inferlet"
wasm_sha256 = "too_short"
"#
        .to_string();
        let err = InferletManifest::load_from_str(&toml_str).unwrap_err();
        assert!(matches!(err, InferletManifestLoadError::InvalidSha256(_)));
    }

    #[test]
    fn test_invalid_sha256_not_hex() {
        let toml_str = format!(
            r#"name = "test"
version = "1.0.0"
description = "A test inferlet"
wasm_sha256 = "{}"
"#,
            "g".repeat(64) // 'g' is not hex
        );
        let err = InferletManifest::load_from_str(&toml_str).unwrap_err();
        assert!(matches!(err, InferletManifestLoadError::InvalidSha256(_)));
    }

    #[test]
    fn test_validate_wasm_correct() {
        let wasm_bytes = b"\0asm";
        let digest = Sha256::digest(wasm_bytes);
        let digest_hex = format!("{:x}", digest);

        let toml_str = format!(
            r#"name = "test"
version = "1.0.0"
description = "A test inferlet"
wasm_sha256 = "{}"
"#,
            digest_hex
        );
        let manifest = InferletManifest::load_from_str(&toml_str).unwrap();
        assert!(manifest.validate_wasm(wasm_bytes));
    }

    #[test]
    fn test_validate_wasm_incorrect() {
        let wasm_bytes = b"\0asm";
        let wrong_bytes = b"\0x86\0no";

        let digest = Sha256::digest(wasm_bytes);
        let digest_hex = format!("{:x}", digest);

        let toml_str = format!(
            r#"name = "test"
version = "1.0.0"
description = "A test inferlet"
wasm_sha256 = "{}"
"#,
            digest_hex
        );
        let manifest = InferletManifest::load_from_str(&toml_str).unwrap();
        assert!(!manifest.validate_wasm(wrong_bytes));
    }

    #[test]
    fn test_roundtrip() {
        let toml_str = full_toml();
        let manifest = InferletManifest::load_from_str(&toml_str).unwrap();
        let serialized = toml::to_string(&manifest).unwrap();
        let manifest2 = InferletManifest::load_from_str(&serialized).unwrap();
        assert_eq!(manifest.name, manifest2.name);
        assert_eq!(manifest.version, manifest2.version);
        assert_eq!(manifest.wasm_sha256, manifest2.wasm_sha256);
        assert_eq!(manifest.dependencies, manifest2.dependencies);
        assert_eq!(manifest.metadata.tags, manifest2.metadata.tags);
    }

    #[test]
    fn test_load_from_str() {
        let toml_str = minimal_toml();
        let manifest = InferletManifest::load_from_str(&toml_str).unwrap();
        assert_eq!(manifest.name, "test");
    }

    #[test]
    fn test_default_values() {
        let toml_str = minimal_toml();
        let manifest = InferletManifest::load_from_str(&toml_str).unwrap();
        assert!(manifest.metadata.tags.is_empty());
        assert!(manifest.metadata.author.is_none());
        assert!(manifest.metadata.license.is_none());
        assert_eq!(manifest.max_fuel, None);
        assert_eq!(manifest.max_stack_size, None);
        assert!(!manifest.validate_input);
        assert!(manifest.dependencies.is_empty());
    }
}
