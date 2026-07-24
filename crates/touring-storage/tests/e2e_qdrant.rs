//! E2E proof for `QdrantStore` — exercises the Qdrant backend against a real
//! running Qdrant server.
//!
//! Proves `QdrantStore::{new, collection_exists, create_collection,
//! collection_schema, upsert, search, delete, delete_collection}`, including
//! the `json_to_filter` path through a live filtered `search`.
//!
//! Server resolution: `TOURING_QDRANT_URL`, else `http://localhost:6333`.
//! When the server is unreachable a test prints a skip notice and returns —
//! it never fails for a missing external resource.
//!
//! Each test creates a uniquely-named throwaway collection and drops it on
//! completion, so it never collides with real collections on the server.
#![cfg(feature = "qdrant")]

use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use touring_storage::vec::backends::qdrant::QdrantStore;
use touring_storage::vec::{CollectionSchema, DistanceMetric, Point, SearchQuery, VectorStore};

/// Monotonic counter so collections created in one run never clash.
static COLLECTION_SEQ: AtomicU64 = AtomicU64::new(0);

/// The Qdrant URL under test (`TOURING_QDRANT_URL` or the local default).
///
/// Defaults to the gRPC port (6334) — `qdrant-client` speaks gRPC, not the
/// REST API on 6333.
fn qdrant_url() -> String {
    std::env::var("TOURING_QDRANT_URL").unwrap_or_else(|_| "http://localhost:6334".to_string())
}

/// A throwaway collection name unique to this process and call.
fn unique_collection(tag: &str) -> String {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let seq = COLLECTION_SEQ.fetch_add(1, Ordering::Relaxed);
    format!("touring_storage_e2e_{tag}_{nanos}_{seq}")
}

/// Connects to Qdrant and confirms it answers; `None` means it is unreachable.
async fn connect() -> Option<QdrantStore> {
    let store = QdrantStore::new(&qdrant_url()).ok()?;
    // Liveness probe — `collection_exists` round-trips to the server.
    match store
        .collection_exists("___touring_liveness_probe___")
        .await
    {
        Ok(_) => Some(store),
        Err(_) => None,
    }
}

#[tokio::test]
async fn e2e_qdrant_collection_lifecycle() {
    let Some(store) = connect().await else {
        eprintln!("SKIP e2e_qdrant_collection_lifecycle: Qdrant unreachable");
        return;
    };
    let name = unique_collection("lifecycle");

    // `collection_exists` is false for a fresh unique name.
    assert!(
        !store
            .collection_exists(&name)
            .await
            .expect("collection_exists query"),
        "a fresh unique collection name must not exist yet"
    );

    // `create_collection` then makes `collection_exists` true.
    store
        .create_collection(CollectionSchema {
            name: name.clone(),
            dimension: 8,
            distance: DistanceMetric::Cosine,
        })
        .await
        .expect("create_collection must succeed");
    assert!(
        store
            .collection_exists(&name)
            .await
            .expect("collection_exists query"),
        "collection must exist after create_collection"
    );

    // `collection_schema` reads back the dimension and metric we created.
    let (dimension, metric) = store
        .collection_schema(&name)
        .await
        .expect("collection_schema must succeed");
    assert_eq!(dimension, 8, "schema dimension must round-trip");
    assert_eq!(
        metric,
        DistanceMetric::Cosine,
        "schema distance metric must round-trip"
    );

    // `delete_collection` then makes `collection_exists` false again.
    store
        .delete_collection(&name)
        .await
        .expect("delete_collection must succeed");
    assert!(
        !store
            .collection_exists(&name)
            .await
            .expect("collection_exists query"),
        "collection must be gone after delete_collection"
    );
}

#[tokio::test]
async fn e2e_qdrant_upsert_search_delete() {
    let Some(store) = connect().await else {
        eprintln!("SKIP e2e_qdrant_upsert_search_delete: Qdrant unreachable");
        return;
    };
    let name = unique_collection("upsert");
    store
        .create_collection(CollectionSchema {
            name: name.clone(),
            dimension: 4,
            distance: DistanceMetric::Cosine,
        })
        .await
        .expect("create_collection");

    // `upsert` three orthogonal points (numeric ids map to Qdrant Num ids).
    let points = vec![
        Point {
            id: "1".to_string(),
            vector: vec![1.0, 0.0, 0.0, 0.0],
            metadata: serde_json::json!({ "axis": "x" }),
        },
        Point {
            id: "2".to_string(),
            vector: vec![0.0, 1.0, 0.0, 0.0],
            metadata: serde_json::json!({ "axis": "y" }),
        },
        Point {
            id: "3".to_string(),
            vector: vec![0.0, 0.0, 1.0, 0.0],
            metadata: serde_json::json!({ "axis": "z" }),
        },
    ];
    store
        .upsert(&name, points)
        .await
        .expect("upsert must succeed");

    // `search` — the nearest neighbour of [1,0,0,0] must be point "1".
    let hits = store
        .search(
            &name,
            SearchQuery {
                vector: vec![1.0, 0.0, 0.0, 0.0],
                top_k: 3,
                with_metadata: false,
                filter: None,
            },
        )
        .await
        .expect("search must succeed");
    let top = hits.first().expect("search must return at least one hit");
    assert_eq!(
        top.id, "1",
        "the x-axis query must rank the x-axis point first"
    );

    // `delete` point "1"; it must no longer appear in search results.
    store
        .delete(&name, vec!["1".to_string()])
        .await
        .expect("delete must succeed");
    let hits = store
        .search(
            &name,
            SearchQuery {
                vector: vec![1.0, 0.0, 0.0, 0.0],
                top_k: 3,
                with_metadata: false,
                filter: None,
            },
        )
        .await
        .expect("search after delete");
    assert!(
        hits.iter().all(|hit| hit.id != "1"),
        "the deleted point must not appear in search results"
    );

    store
        .delete_collection(&name)
        .await
        .expect("cleanup delete_collection");
}

#[tokio::test]
async fn e2e_qdrant_filtered_search_keeps_only_matching_payload() {
    let Some(store) = connect().await else {
        eprintln!("SKIP e2e_qdrant_filtered_search: Qdrant unreachable");
        return;
    };
    let name = unique_collection("filter");
    store
        .create_collection(CollectionSchema {
            name: name.clone(),
            dimension: 4,
            distance: DistanceMetric::Cosine,
        })
        .await
        .expect("create_collection");

    store
        .upsert(
            &name,
            vec![
                Point {
                    id: "10".to_string(),
                    vector: vec![1.0, 0.0, 0.0, 0.0],
                    metadata: serde_json::json!({ "lang": "rust" }),
                },
                Point {
                    id: "11".to_string(),
                    vector: vec![0.9, 0.1, 0.0, 0.0],
                    metadata: serde_json::json!({ "lang": "python" }),
                },
            ],
        )
        .await
        .expect("upsert");

    // `search` with a JSON filter — `json_to_filter` keeps only lang=rust.
    let hits = store
        .search(
            &name,
            SearchQuery {
                vector: vec![1.0, 0.0, 0.0, 0.0],
                top_k: 10,
                with_metadata: true,
                filter: Some(serde_json::json!({ "lang": "rust" })),
            },
        )
        .await
        .expect("filtered search must succeed");
    assert_eq!(
        hits.len(),
        1,
        "the lang=rust filter must select exactly one of the two points"
    );
    let hit = hits.first().expect("the single filtered hit");
    assert_eq!(hit.id, "10", "the rust-tagged point");
    assert!(
        !hit.metadata.is_null(),
        "with_metadata=true must populate the hit payload"
    );

    store
        .delete_collection(&name)
        .await
        .expect("cleanup delete_collection");
}
