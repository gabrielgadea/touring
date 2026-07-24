//! Tests for the vector store backends.

use touring_storage::vec::{
    CollectionSchema, DistanceMetric, InMemoryVectorStore, Point, SearchQuery, VectorStore,
};

#[tokio::test]
async fn test_in_memory_backend_basic() {
    let store = InMemoryVectorStore::new();

    let schema = CollectionSchema {
        name: "test".to_string(),
        dimension: 3,
        distance: DistanceMetric::Cosine,
    };

    store
        .create_collection(schema)
        .await
        .expect("create collection should succeed");
    let exists = store
        .collection_exists("test")
        .await
        .expect("collection_exists should succeed");
    assert!(exists, "collection should exist after creation");

    let point = Point {
        id: "p1".to_string(),
        vector: vec![1.0, 0.0, 0.0],
        metadata: serde_json::json!({}),
    };

    store
        .upsert("test", vec![point])
        .await
        .expect("upsert should succeed");

    let hits = store
        .search(
            "test",
            SearchQuery {
                vector: vec![1.0, 0.0, 0.0],
                top_k: 5,
                with_metadata: true,
                filter: None,
            },
        )
        .await
        .expect("search should succeed");

    assert_eq!(hits.len(), 1, "should return 1 hit");
    assert_eq!(hits[0].id, "p1", "hit id should match");
    assert!(
        hits[0].score > 0.99,
        "identical vectors should score near 1.0"
    );
}
