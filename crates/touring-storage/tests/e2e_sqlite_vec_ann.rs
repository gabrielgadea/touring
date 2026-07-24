//! E2E ANN recall test for SqliteVecStore.
//!
//! Creates a temp SQLite DB with 1000 random vectors (dim=128),
//! then verifies that each vector can be found in its own top-10 ANN
//! results with recall >= 0.8 using cosine similarity.
use rand::rngs::StdRng;
use rand::{Rng, SeedableRng};
use tempfile::NamedTempFile;
use touring_storage::vec::backends::sqlite_vec::SqliteVecStore;
use touring_storage::vec::{CollectionSchema, DistanceMetric, Point, SearchQuery, VectorStore};
/// Deterministic random vector generator (seeded by index so results are reproducible).
fn generate_random_vector(dim: usize, seed: usize) -> Vec<f32> {
    let mut rng = StdRng::seed_from_u64(seed as u64);
    (0..dim).map(|_| rng.r#gen::<f32>()).collect()
}
#[tokio::test]
async fn e2e_sqlite_vec_ann_recall() -> Result<(), Box<dyn std::error::Error>> {
    const DIM: usize = 128;
    const N_VECTORS: usize = 1000;
    const TOP_K: usize = 10;
    const MIN_RECALL: f32 = 0.8;
    let temp_db = NamedTempFile::new()?;
    let db_path = temp_db.path().to_str().unwrap();
    let store = SqliteVecStore::new(db_path)?;
    let schema = CollectionSchema {
        name: "test_recall".to_string(),
        dimension: DIM,
        distance: DistanceMetric::Cosine,
    };
    store.create_collection(schema.clone()).await?;
    let vectors: Vec<Vec<f32>> = (0..N_VECTORS)
        .map(|i| generate_random_vector(DIM, i))
        .collect();
    for (i, v) in vectors.iter().enumerate() {
        store
            .upsert(
                "test_recall",
                vec![Point {
                    id: format!("vec_{}", i),
                    vector: v.clone(),
                    metadata: serde_json::json!({}),
                }],
            )
            .await?;
    }
    let mut found = 0usize;
    for (i, v) in vectors.iter().enumerate() {
        let hits = store
            .search(
                "test_recall",
                SearchQuery {
                    vector: v.clone(),
                    top_k: TOP_K,
                    with_metadata: false,
                    filter: None,
                },
            )
            .await?;
        if hits.iter().any(|h| h.id == format!("vec_{}", i)) {
            found += 1;
        }
    }
    let recall = found as f32 / N_VECTORS as f32;
    assert!(
        recall >= MIN_RECALL,
        "recall {:.3} < {:.1} (found {}/{})",
        recall,
        MIN_RECALL,
        found,
        N_VECTORS
    );
    Ok(())
}
