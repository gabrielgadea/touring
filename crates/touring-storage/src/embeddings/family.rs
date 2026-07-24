//! Model family definitions for embedding compatibility checks.

use serde::{Deserialize, Serialize};

/// Represents a family/generation of embedding models.
/// Used for compatibility checking between different model versions.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelFamily {
    /// The model family name (e.g., "bge", "fastembed", "voyage").
    pub name: String,
    /// The generation/version (e.g., "large", "small", "v2").
    pub generation: String,
}

impl ModelFamily {
    /// Creates a new model family.
    pub fn new(name: impl Into<String>, generation: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            generation: generation.into(),
        }
    }

    /// Checks if this family is compatible with another.
    /// Compatibility means same family name and overlapping generations.
    pub fn is_compatible_with(&self, other: &ModelFamily) -> bool {
        self.name == other.name && self.generation == other.generation
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_family_compatibility_same() {
        let bge_large = ModelFamily::new("bge", "large");
        let bge_large_2 = ModelFamily::new("bge", "large");
        assert!(bge_large.is_compatible_with(&bge_large_2));
    }

    #[test]
    fn test_model_family_incompatibility_different_name() {
        let bge = ModelFamily::new("bge", "large");
        let voyage = ModelFamily::new("voyage", "large");
        assert!(!bge.is_compatible_with(&voyage));
    }

    #[test]
    fn test_model_family_incompatibility_different_generation() {
        let large = ModelFamily::new("bge", "large");
        let small = ModelFamily::new("bge", "small");
        assert!(!large.is_compatible_with(&small));
    }
}
