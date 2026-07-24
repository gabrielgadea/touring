//! E2E integration proof: local candle embedding -> Qdrant storage -> search.
//!
//! Wires the two storage backends end to end — a real all-MiniLM BERT model
//! embeds documents, a real Qdrant server stores the vectors, and a semantic
//! query retrieves the right document. This proves the whole `embeddings` +
//! `vec` pipeline working together, not the units in isolation.
//!
//! Skips cleanly when either the BERT model or the Qdrant server is absent;
//! it never fails for a missing external resource.
#![cfg(all(feature = "candle-bge", feature = "qdrant"))]

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use touring_storage::embeddings::{BgeModelVariant, CandleBgeProvider, EmbeddingProvider};
use touring_storage::vec::backends::qdrant::QdrantStore;
use touring_storage::vec::{CollectionSchema, DistanceMetric, Point, SearchQuery, VectorStore};

/// True when `dir` holds the three files a `BertModel` checkpoint needs.
fn model_dir_complete(dir: &Path) -> bool {
    dir.join("config.json").is_file()
        && dir.join("model.safetensors").is_file()
        && dir.join("tokenizer.json").is_file()
}

/// True when `<dir>/config.json` declares the BERT architecture.
fn config_is_bert(dir: &Path) -> bool {
    std::fs::read_to_string(dir.join("config.json"))
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .and_then(|json| {
            json.get("model_type")
                .and_then(|v| v.as_str())
                .map(str::to_string)
        })
        .as_deref()
        == Some("bert")
}

/// Locates a BERT-architecture model directory usable by `CandleBgeProvider`.
fn locate_bert_model() -> Option<PathBuf> {
    if let Ok(dir) = std::env::var("TOURING_BGE_MODEL_DIR") {
        let explicit = PathBuf::from(dir);
        if model_dir_complete(&explicit) && config_is_bert(&explicit) {
            return Some(explicit);
        }
    }
    let home = std::env::var("HOME").ok()?;
    let hub = Path::new(&home).join(".cache/huggingface/hub");
    for model in std::fs::read_dir(&hub).ok()?.flatten() {
        let Ok(snapshots) = std::fs::read_dir(model.path().join("snapshots")) else {
            continue;
        };
        for snapshot in snapshots.flatten() {
            let dir = snapshot.path();
            if model_dir_complete(&dir) && config_is_bert(&dir) {
                return Some(dir);
            }
        }
    }
    None
}

/// The Qdrant URL under test (`TOURING_QDRANT_URL` or the local default).
///
/// Defaults to the gRPC port (6334) — `qdrant-client` speaks gRPC, not the
/// REST API on 6333.
fn qdrant_url() -> String {
    std::env::var("TOURING_QDRANT_URL").unwrap_or_else(|_| "http://localhost:6334".to_string())
}

#[tokio::test]
async fn e2e_embed_documents_then_qdrant_semantic_search() {
    // --- resource detection -------------------------------------------------
    let Some(model_dir) = locate_bert_model() else {
        eprintln!("SKIP e2e_embed_to_qdrant: no BERT model in the HF cache");
        return;
    };
    let Ok(store) = QdrantStore::new(&qdrant_url()) else {
        eprintln!("SKIP e2e_embed_to_qdrant: cannot build the Qdrant client");
        return;
    };
    let collection = format!(
        "touring_storage_embed_e2e_{}",
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos())
            .unwrap_or(0)
    );
    if store.collection_exists(&collection).await.is_err() {
        eprintln!("SKIP e2e_embed_to_qdrant: Qdrant server unreachable");
        return;
    }

    // --- embed real documents with the local BERT model --------------------
    let model_path = model_dir.to_str().expect("model dir path is valid UTF-8");
    let provider = CandleBgeProvider::new(model_path, BgeModelVariant::BgeSmall)
        .expect("CandleBgeProvider must load the BERT checkpoint");
    let docs = [
        "Rust ownership and the borrow checker prevent data races at compile time",
        "Photosynthesis converts sunlight into chemical energy stored as glucose",
        "A sourdough starter ferments flour and water with wild airborne yeast",
    ];
    let embedded = provider
        .embed(docs.iter().map(|&s| s.to_string()).collect())
        .await
        .expect("embedding the document batch");
    let dimension = provider.dimensions();
    assert_eq!(
        embedded.vectors.len(),
        3,
        "three documents -> three vectors"
    );
    assert_eq!(
        embedded.dimension, dimension,
        "result dimension must equal the model's real width"
    );

    // --- store the embeddings in a fresh Qdrant collection -----------------
    store
        .create_collection(CollectionSchema {
            name: collection.clone(),
            dimension,
            distance: DistanceMetric::Cosine,
        })
        .await
        .expect("creating the embedding collection");
    let points: Vec<Point> = embedded
        .vectors
        .iter()
        .zip(docs.iter())
        .enumerate()
        .map(|(index, (vector, doc))| Point {
            id: (index as u64 + 1).to_string(),
            vector: vector.clone(),
            metadata: serde_json::json!({ "doc": doc }),
        })
        .collect();
    store
        .upsert(&collection, points)
        .await
        .expect("upserting the embedded documents");

    // --- query with a fresh embedding of a Rust-related question -----------
    let query = provider
        .embed_query("How does Rust stop concurrent memory errors?".to_string())
        .await
        .expect("embedding the query");
    let query_vector = query
        .vectors
        .into_iter()
        .next()
        .expect("embed_query yields one vector");
    let hits = store
        .search(
            &collection,
            SearchQuery {
                vector: query_vector,
                top_k: 3,
                with_metadata: true,
                filter: None,
            },
        )
        .await
        .expect("semantic search over the embedded documents");

    // --- proof: the Rust document (id 1) ranks first -----------------------
    let top = hits
        .first()
        .expect("search must return the stored documents");
    assert_eq!(
        top.id, "1",
        "a Rust-about query must rank the Rust document (id 1) first; got id {}",
        top.id
    );

    // --- cleanup ------------------------------------------------------------
    store
        .delete_collection(&collection)
        .await
        .expect("dropping the throwaway collection");
}
