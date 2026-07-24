//! Tests for the embedding provider abstraction.

use touring_storage::embeddings::{EmbeddingModel, EmbeddingResult, ModelFamily};

#[test]
fn test_embedding_model_variants() {
    let models = [
        EmbeddingModel::BgeLarge,
        EmbeddingModel::BgeSmall,
        EmbeddingModel::FastEmbedBgeLarge,
        EmbeddingModel::FastEmbedBgeSmall,
        EmbeddingModel::Voyage2,
        EmbeddingModel::VoyageLarge,
    ];

    for model in models {
        let id = model.id();
        assert!(!id.is_empty());
        let dims = model.dimensions();
        assert!(dims > 0);
        let family = model.family();
        assert!(!family.name.is_empty());
    }
}

#[test]
fn test_embedding_result_construction() {
    let vectors = vec![vec![0.0; 1024]; 5];
    let result = EmbeddingResult::new(vectors.clone(), EmbeddingModel::BgeLarge, Some(100));
    assert_eq!(result.len(), 5);
    assert_eq!(result.dimension, 1024);
    assert_eq!(result.token_count, Some(100));
    assert!(!result.is_empty());
}

#[test]
fn test_model_family_equality() {
    let f1 = ModelFamily::new("bge", "large");
    let f2 = ModelFamily::new("bge", "large");
    assert_eq!(f1, f2);
}

#[test]
fn test_model_family_inequality() {
    let bge = ModelFamily::new("bge", "large");
    let voyage = ModelFamily::new("voyage", "large");
    assert_ne!(bge, voyage);
}
