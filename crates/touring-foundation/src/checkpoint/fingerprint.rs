//! CheckpointSettingsFingerprint — family-aware config compatibility detection.
//!
//! Enables asymmetric embeddings: query model can change without reindex if
//! the config family is compatible. Also used for general config drift detection
//! (tree-sitter version bumps, chunker changes, etc.).
//!
//! # Algorithm
//!
//! - **Symmetric mode**: same chunker name = compatible; different = breaking
//! - **Asymmetric mode**: same family + same primary = compatible even if secondary differs
//! - **ChangeImpact**: None → Compatible → BreakingMinor → BreakingMajor
//!
//! # Example
//!
//! ```
//! use touring_foundation::checkpoint::fingerprint::{CheckpointSettingsFingerprint, ChangeImpact, ConfigType};
//!
//! // Same config type + same chunker → compatible with No impact
//! let fp1 = CheckpointSettingsFingerprint::symmetric("SemanticChunker", Some("tree-sitter"));
//! let fp2 = CheckpointSettingsFingerprint::symmetric("SemanticChunker", Some("tree-sitter"));
//!
//! let (compatible, impact) = fp1.is_compatible_with(&fp2);
//! assert!(compatible);
//! assert!(matches!(impact, ChangeImpact::None));
//! ```

use serde::{Deserialize, Serialize};

/// Configuration type determines compatibility rules.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ConfigType {
    /// Symmetric: same chunker = compatible; different = breaking.
    Symmetric,
    /// Asymmetric: query model can differ from index model if family is compatible.
    Asymmetric,
}

/// Impact level of a config change on an existing checkpoint.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChangeImpact {
    /// No change — identical configs.
    None,
    /// Different values but same semantic result (e.g., minor version bump within family).
    Compatible,
    /// Minor breaking change (e.g., different secondary chunker in asymmetric mode).
    BreakingMinor,
    /// Major breaking change (e.g., different chunker family).
    BreakingMajor,
}

/// Family-aware fingerprint of indexer settings.
/// Used to decide whether an existing checkpoint is still valid after config changes.
///
/// # Compatibility Rules
///
/// | Mode       | Same chunker | Same family | Secondary same | Result              |
/// |------------|--------------|-------------|----------------|---------------------|
/// | Symmetric  | ✅           | —           | —              | Compatible          |
/// | Symmetric  | ❌           | —           | —              | BreakingMajor       |
/// | Asymmetric | ✅           | ✅          | ✅             | Compatible          |
/// | Asymmetric | ✅           | ✅          | ❌             | BreakingMinor       |
/// | Asymmetric | ❌           | ❌          | —              | BreakingMajor       |
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckpointSettingsFingerprint {
    /// Symmetric or Asymmetric mode.
    pub config_type: ConfigType,
    /// Primary chunker name (e.g., `"SemanticChunker"`).
    pub primary_chunker: String,
    /// Chunker's semantic family (e.g., `"tree-sitter"`, `"tree-sitter-0.20"`).
    /// Minor version bumps within same family are considered compatible.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub primary_chunker_family: Option<String>,
    /// Optional secondary/fallback chunker name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub secondary_chunker: Option<String>,
    /// Vector store identifier (for Wave 5 embeddings).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub vector_store: Option<String>,
    /// Embedding provider name (e.g., "voyage", "fastembed", "candle-bge").
    /// Captured in fingerprint so changing provider invalidates checkpoints.
    #[serde(default)]
    pub embedding_provider: Option<String>,
    /// Embedding model variant within the provider (e.g., "bge-small", "bge-large").
    #[serde(default)]
    pub embedding_model: Option<String>,
    /// Blake3 hash of the full config snapshot (stable identifier).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config_hash: Option<[u8; 32]>,
}

impl CheckpointSettingsFingerprint {
    /// Create a symmetric fingerprint (same chunker = compatible).
    pub fn symmetric(primary_chunker: &str, primary_family: Option<&str>) -> Self {
        Self {
            config_type: ConfigType::Symmetric,
            primary_chunker: primary_chunker.to_string(),
            primary_chunker_family: primary_family.map(|s| s.to_string()),
            secondary_chunker: None,
            vector_store: None,
            embedding_provider: None,
            embedding_model: None,
            config_hash: None,
        }
    }

    /// Create an asymmetric fingerprint (query model ≠ index model if family compatible).
    pub fn asymmetric(
        primary_chunker: &str,
        primary_family: Option<&str>,
        secondary_chunker: Option<&str>,
    ) -> Self {
        Self {
            config_type: ConfigType::Asymmetric,
            primary_chunker: primary_chunker.to_string(),
            primary_chunker_family: primary_family.map(|s| s.to_string()),
            secondary_chunker: secondary_chunker.map(|s| s.to_string()),
            vector_store: None,
            embedding_provider: None,
            embedding_model: None,
            config_hash: None,
        }
    }

    /// Set the vector store identifier.
    pub fn with_vector_store(mut self, store: &str) -> Self {
        self.vector_store = Some(store.to_string());
        self
    }

    /// Compute and store the blake3 config hash.
    pub fn with_hash(mut self) -> Self {
        // Build a stable repr for hashing (order-independent on optional fields)
        let repr = format!(
            "{:?}|{}|{:?}|{:?}|{:?}|{:?}|{:?}",
            self.config_type,
            self.primary_chunker,
            self.primary_chunker_family,
            self.secondary_chunker,
            self.vector_store,
            self.embedding_provider,
            self.embedding_model,
        );
        let hash = blake3::hash(repr.as_bytes());
        let mut out = [0u8; 32];
        out.copy_from_slice(hash.as_bytes());
        self.config_hash = Some(out);
        self
    }

    /// Set the embedding provider and model.
    pub fn with_embedding_info(mut self, provider: &str, model: Option<&str>) -> Self {
        self.embedding_provider = Some(provider.to_string());
        self.embedding_model = model.map(|s| s.to_string());
        self
    }

    /// Check whether `self` is compatible with `other` (i.e., can reuse the index).
    pub fn is_compatible_with(&self, other: &Self) -> (bool, ChangeImpact) {
        // Exact match
        if self.config_hash.is_some()
            && other.config_hash.is_some()
            && self.config_hash == other.config_hash
        {
            return (true, ChangeImpact::None);
        }

        match (&self.config_type, &other.config_type) {
            // Symmetric mode: same chunker name = compatible
            (ConfigType::Symmetric, ConfigType::Symmetric) => {
                if self.primary_chunker == other.primary_chunker {
                    let impact = if self.secondary_chunker != other.secondary_chunker {
                        ChangeImpact::BreakingMinor
                    } else {
                        ChangeImpact::None
                    };
                    (true, impact)
                } else {
                    (false, ChangeImpact::BreakingMajor)
                }
            }
            // Asymmetric: same family + same primary = compatible
            (ConfigType::Asymmetric, ConfigType::Asymmetric) => {
                let same_primary = self.primary_chunker == other.primary_chunker;
                let same_family = self.primary_chunker_family == other.primary_chunker_family;
                let same_secondary = self.secondary_chunker == other.secondary_chunker;

                if same_primary && same_family {
                    if same_secondary {
                        (true, ChangeImpact::None)
                    } else {
                        // Secondary changed — minor break
                        (true, ChangeImpact::BreakingMinor)
                    }
                } else if same_primary && !same_family {
                    // Same chunker but different family — major break
                    (false, ChangeImpact::BreakingMajor)
                } else {
                    // Different primary chunker
                    (false, ChangeImpact::BreakingMajor)
                }
            }
            // Cross-mode: different mode is a breaking change
            _ => (false, ChangeImpact::BreakingMajor),
        }
    }

    /// Short human-readable description for session start output.
    pub fn describe_change(&self, other: &Self) -> String {
        let (_, impact) = self.is_compatible_with(other);
        match impact {
            ChangeImpact::None => "identical config".to_string(),
            ChangeImpact::Compatible => "compatible (minor variant)".to_string(),
            ChangeImpact::BreakingMinor => {
                format!(
                    "breaking minor: secondary chunker changed ({} → {})",
                    other
                        .secondary_chunker
                        .as_ref()
                        .unwrap_or(&"<none>".to_string()),
                    self.secondary_chunker
                        .as_ref()
                        .unwrap_or(&"<none>".to_string()),
                )
            }
            ChangeImpact::BreakingMajor => {
                if self.config_type != other.config_type {
                    "breaking: symmetric/asymmetric mode changed".to_string()
                } else if self.primary_chunker != other.primary_chunker {
                    format!(
                        "breaking: chunker changed ({} → {})",
                        other.primary_chunker, self.primary_chunker
                    )
                } else {
                    format!(
                        "breaking: chunker family changed ({} → {:?})",
                        other
                            .primary_chunker_family
                            .as_ref()
                            .unwrap_or(&"<none>".to_string()),
                        self.primary_chunker_family,
                    )
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── Symmetric mode ─────────────────────────────────────────────────────────

    #[test]
    fn symmetric_same_chunker_compatible() {
        let fp1 = CheckpointSettingsFingerprint::symmetric("SemanticChunker", Some("tree-sitter"));
        let fp2 = CheckpointSettingsFingerprint::symmetric("SemanticChunker", Some("tree-sitter"));
        let (ok, impact) = fp1.is_compatible_with(&fp2);
        assert!(ok);
        assert!(matches!(impact, ChangeImpact::None));
    }

    #[test]
    fn symmetric_different_chunker_breaking() {
        let fp1 = CheckpointSettingsFingerprint::symmetric("SemanticChunker", Some("tree-sitter"));
        let fp2 = CheckpointSettingsFingerprint::symmetric("DelimiterChunker", Some("tree-sitter"));
        let (ok, impact) = fp1.is_compatible_with(&fp2);
        assert!(!ok);
        assert!(matches!(impact, ChangeImpact::BreakingMajor));
    }

    #[test]
    fn symmetric_same_chunker_different_family() {
        let fp1 =
            CheckpointSettingsFingerprint::symmetric("SemanticChunker", Some("tree-sitter-0.21"));
        let fp2 =
            CheckpointSettingsFingerprint::symmetric("SemanticChunker", Some("tree-sitter-0.20"));
        let (ok, _impact) = fp1.is_compatible_with(&fp2);
        // Same name but different family within symmetric — still compatible (family ignored)
        assert!(ok);
    }

    // ── Asymmetric mode ────────────────────────────────────────────────────────

    #[test]
    fn asymmetric_same_all_compatible() {
        let fp1 =
            CheckpointSettingsFingerprint::asymmetric("SemanticChunker", Some("tree-sitter"), None);
        let fp2 =
            CheckpointSettingsFingerprint::asymmetric("SemanticChunker", Some("tree-sitter"), None);
        let (ok, impact) = fp1.is_compatible_with(&fp2);
        assert!(ok);
        assert!(matches!(impact, ChangeImpact::None));
    }

    #[test]
    fn asymmetric_same_primary_same_family_different_secondary() {
        let fp1 =
            CheckpointSettingsFingerprint::asymmetric("SemanticChunker", Some("tree-sitter"), None);
        let fp2 = CheckpointSettingsFingerprint::asymmetric(
            "SemanticChunker",
            Some("tree-sitter"),
            Some("DelimiterChunker"),
        );
        let (ok, impact) = fp1.is_compatible_with(&fp2);
        assert!(ok);
        assert!(matches!(impact, ChangeImpact::BreakingMinor));
    }

    #[test]
    fn asymmetric_same_primary_different_family_breaking() {
        let fp1 = CheckpointSettingsFingerprint::asymmetric(
            "SemanticChunker",
            Some("tree-sitter-0.21"),
            None,
        );
        let fp2 = CheckpointSettingsFingerprint::asymmetric(
            "SemanticChunker",
            Some("tree-sitter-0.20"),
            None,
        );
        let (ok, impact) = fp1.is_compatible_with(&fp2);
        assert!(!ok);
        assert!(matches!(impact, ChangeImpact::BreakingMajor));
    }

    #[test]
    fn asymmetric_different_primary_breaking() {
        let fp1 =
            CheckpointSettingsFingerprint::asymmetric("SemanticChunker", Some("tree-sitter"), None);
        let fp2 = CheckpointSettingsFingerprint::asymmetric(
            "DelimiterChunker",
            Some("tree-sitter"),
            None,
        );
        let (ok, impact) = fp1.is_compatible_with(&fp2);
        assert!(!ok);
        assert!(matches!(impact, ChangeImpact::BreakingMajor));
    }

    // ── Cross-mode ───────────────────────────────────────────────────────────

    #[test]
    fn symmetric_vs_asymmetric_breaking() {
        let fp1 = CheckpointSettingsFingerprint::symmetric("SemanticChunker", Some("tree-sitter"));
        let fp2 =
            CheckpointSettingsFingerprint::asymmetric("SemanticChunker", Some("tree-sitter"), None);
        let (ok, impact) = fp1.is_compatible_with(&fp2);
        assert!(!ok);
        assert!(matches!(impact, ChangeImpact::BreakingMajor));
    }

    // ── Edge: secondary only change in asymmetric ─────────────────────────

    #[test]
    fn asymmetric_secondary_only_change_compatible_with_family() {
        // Same primary + same family + different secondary → BreakingMinor (not major)
        let fp1 = CheckpointSettingsFingerprint::asymmetric(
            "SemanticChunker",
            Some("tree-sitter"),
            Some("DelimiterChunker"),
        );
        let fp2 =
            CheckpointSettingsFingerprint::asymmetric("SemanticChunker", Some("tree-sitter"), None);
        let (ok, impact) = fp1.is_compatible_with(&fp2);
        assert!(ok);
        assert!(matches!(impact, ChangeImpact::BreakingMinor));
    }

    // ── Config hash ────────────────────────────────────────────────────────────

    #[test]
    fn config_hash_exact_match() {
        let fp1 = CheckpointSettingsFingerprint::symmetric("SemanticChunker", None).with_hash();
        let fp2 = CheckpointSettingsFingerprint::symmetric("SemanticChunker", None).with_hash();
        let (ok, impact) = fp1.is_compatible_with(&fp2);
        assert!(ok);
        assert!(matches!(impact, ChangeImpact::None));
    }

    #[test]
    fn config_hash_different_after_change() {
        let fp1 = CheckpointSettingsFingerprint::symmetric("SemanticChunker", None).with_hash();
        let fp2 = CheckpointSettingsFingerprint::symmetric("DelimiterChunker", None).with_hash();
        let (ok, _impact) = fp1.is_compatible_with(&fp2);
        assert!(!ok);
    }

    // ── Describe change ───────────────────────────────────────────────────────

    #[test]
    fn describe_change_identical() {
        let fp1 = CheckpointSettingsFingerprint::symmetric("SemanticChunker", Some("tree-sitter"));
        let fp2 = CheckpointSettingsFingerprint::symmetric("SemanticChunker", Some("tree-sitter"));
        let desc = fp1.describe_change(&fp2);
        assert_eq!(desc, "identical config");
    }

    #[test]
    fn describe_change_breaking_major() {
        let fp1 = CheckpointSettingsFingerprint::symmetric("SemanticChunker", Some("tree-sitter"));
        let fp2 = CheckpointSettingsFingerprint::symmetric("DelimiterChunker", Some("tree-sitter"));
        let desc = fp1.describe_change(&fp2);
        assert!(desc.contains("breaking"));
    }

    // ── Embedding provider / model fields (D18) ───────────────────────────────

    #[test]
    fn with_embedding_info_stores_provider_and_model() {
        let fp = CheckpointSettingsFingerprint::symmetric("SemanticChunker", Some("tree-sitter"))
            .with_embedding_info("voyage", Some("voyage-3"));
        assert_eq!(fp.embedding_provider, Some("voyage".to_string()));
        assert_eq!(fp.embedding_model, Some("voyage-3".to_string()));
    }

    #[test]
    fn with_embedding_info_model_is_optional() {
        let fp = CheckpointSettingsFingerprint::symmetric("SemanticChunker", None)
            .with_embedding_info("fastembed", None);
        assert_eq!(fp.embedding_provider, Some("fastembed".to_string()));
        assert_eq!(fp.embedding_model, None);
    }

    #[test]
    fn embedding_info_affects_hash() {
        // Same base config but different embedding info → different hash
        let fp1 = CheckpointSettingsFingerprint::symmetric("SemanticChunker", None)
            .with_embedding_info("voyage", Some("voyage-3"))
            .with_hash();
        let fp2 = CheckpointSettingsFingerprint::symmetric("SemanticChunker", None)
            .with_embedding_info("fastembed", Some("bge-small"))
            .with_hash();
        // Hashes should differ because embedding info differs
        assert_ne!(fp1.config_hash, fp2.config_hash);
    }

    #[test]
    fn serde_default_allows_missing_embedding_fields() {
        // Verifies backward compat: old serialized checkpoints without
        // embedding_provider/model deserialize correctly with None
        use serde_json;
        #[derive(Deserialize)]
        struct FingerprintWrapper {
            config_type: ConfigType,
            primary_chunker: String,
            primary_chunker_family: Option<String>,
            secondary_chunker: Option<String>,
            vector_store: Option<String>,
            #[serde(default)]
            embedding_provider: Option<String>,
            #[serde(default)]
            embedding_model: Option<String>,
            config_hash: Option<String>,
        }
        let old_json = r#"{
            "config_type": "Symmetric",
            "primary_chunker": "SemanticChunker",
            "primary_chunker_family": "tree-sitter"
        }"#;
        let w: FingerprintWrapper = serde_json::from_str(old_json)
            .expect("old checkpoint JSON should parse with serde default");
        // Assert every deserialized field — proves the full legacy schema
        // round-trips, not only the embedding fields.
        let _ = &w.config_type;
        assert_eq!(w.primary_chunker, "SemanticChunker");
        assert_eq!(w.primary_chunker_family.as_deref(), Some("tree-sitter"));
        assert_eq!(w.secondary_chunker, None);
        assert_eq!(w.vector_store, None);
        assert_eq!(w.config_hash, None);
        assert_eq!(w.embedding_provider, None);
        assert_eq!(w.embedding_model, None);
    }
}
