//! E2E proof for `CandleBgeProvider` — loads a real BERT-architecture
//! embedding model from disk and runs genuine candle inference.
//!
//! Proves `CandleBgeProvider::{new, run_inference, embed}` against a real
//! `model.safetensors` checkpoint. `run_inference` is private and is exercised
//! through `embed`, which calls it. The provider's `BertModel` path accepts
//! any BERT-architecture sentence-embedding model; the test auto-discovers one
//! (e.g. all-MiniLM-L6-v2 — `model_type: "bert"`) from the HuggingFace hub
//! cache.
//!
//! Model resolution order:
//!  1. `TOURING_BGE_MODEL_DIR` — an explicit model directory.
//!  2. Auto-scan of `~/.cache/huggingface/hub` for a snapshot carrying
//!     `config.json` (`model_type: "bert"`) + `model.safetensors` +
//!     `tokenizer.json`.
//!
//! When no compatible model is found a test prints a skip notice and returns
//! — it never fails for a missing external resource.
#![cfg(feature = "candle-bge")]

use std::path::{Path, PathBuf};

use touring_storage::embeddings::{BgeModelVariant, CandleBgeProvider, EmbeddingProvider};

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

/// Cosine similarity between two equal-length vectors.
fn cosine(a: &[f32], b: &[f32]) -> f32 {
    let dot: f32 = a.iter().zip(b).map(|(x, y)| x * y).sum();
    let norm_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let norm_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm_a == 0.0 || norm_b == 0.0 {
        0.0
    } else {
        dot / (norm_a * norm_b)
    }
}

#[tokio::test]
async fn e2e_candle_new_loads_real_model_and_embeds() {
    let Some(model_dir) = locate_bert_model() else {
        eprintln!("SKIP e2e_candle_new_loads_real_model_and_embeds: no BERT model in HF cache");
        return;
    };
    eprintln!("model directory: {}", model_dir.display());

    // `new` — loads config.json + model.safetensors + tokenizer.json.
    let model_path = model_dir.to_str().expect("model dir path is valid UTF-8");
    let provider = CandleBgeProvider::new(model_path, BgeModelVariant::BgeSmall)
        .expect("CandleBgeProvider::new must load a complete BERT checkpoint");

    // `dimensions` reflects the model's real config.hidden_size.
    let dim = provider.dimensions();
    assert!(dim > 0, "model dimension must be positive, got {dim}");

    // `embed` — a real candle BERT forward pass through `run_inference`.
    let result = provider
        .embed(vec!["touring orchestrates code intelligence".to_string()])
        .await
        .expect("embed must run a forward pass");

    assert_eq!(result.vectors.len(), 1, "one input text yields one vector");
    let embedding = result.vectors.first().expect("the single embedding");
    assert_eq!(
        embedding.len(),
        dim,
        "the produced vector width must equal dimensions()"
    );
    assert_eq!(
        result.dimension, dim,
        "EmbeddingResult.dimension must equal the real vector width"
    );
    assert!(
        embedding.iter().all(|x| x.is_finite()),
        "every embedding component must be finite"
    );
    assert!(
        embedding.iter().any(|&x| x != 0.0),
        "a genuine inference result is not the zero vector"
    );
}

#[tokio::test]
async fn e2e_candle_run_inference_over_a_batch() {
    let Some(model_dir) = locate_bert_model() else {
        eprintln!("SKIP e2e_candle_run_inference_over_a_batch: no BERT model in HF cache");
        return;
    };
    let model_path = model_dir.to_str().expect("model dir path is valid UTF-8");
    let provider =
        CandleBgeProvider::new(model_path, BgeModelVariant::BgeSmall).expect("provider must load");

    // `run_inference` embeds each text in its own forward pass; drive it with
    // a three-text batch through `embed`.
    let texts = vec![
        "first document".to_string(),
        "second document".to_string(),
        "third document".to_string(),
    ];
    let result = provider.embed(texts).await.expect("batch embed must run");

    assert_eq!(result.vectors.len(), 3, "three inputs yield three vectors");
    let dim = provider.dimensions();
    for (index, vector) in result.vectors.iter().enumerate() {
        assert_eq!(vector.len(), dim, "vector {index} width must be uniform");
        assert!(
            vector.iter().all(|x| x.is_finite()),
            "vector {index} must be finite"
        );
    }
}

#[tokio::test]
async fn e2e_candle_embeddings_are_semantic() {
    let Some(model_dir) = locate_bert_model() else {
        eprintln!("SKIP e2e_candle_embeddings_are_semantic: no BERT model in HF cache");
        return;
    };
    let model_path = model_dir.to_str().expect("model dir path is valid UTF-8");
    let provider =
        CandleBgeProvider::new(model_path, BgeModelVariant::BgeSmall).expect("provider must load");

    // Purpose proof: the embeddings must be *semantic*, not noise — two related
    // sentences must sit closer than an unrelated one.
    let result = provider
        .embed(vec![
            "the cat sat on the warm mat".to_string(),
            "a kitten rested on the soft rug".to_string(),
            "quarterly corporate tax filing deadlines".to_string(),
        ])
        .await
        .expect("embed must run");

    let [feline_a, feline_b, taxes] = &result.vectors[..] else {
        panic!("expected exactly three embedding vectors");
    };
    let related = cosine(feline_a, feline_b);
    let unrelated = cosine(feline_a, taxes);
    eprintln!("cosine(related)={related:.4}  cosine(unrelated)={unrelated:.4}");
    assert!(
        related > unrelated,
        "semantically close texts must embed closer: related {related:.4} \
         must exceed unrelated {unrelated:.4}"
    );
}

#[tokio::test]
async fn e2e_candle_embed_empty_batch() {
    let Some(model_dir) = locate_bert_model() else {
        eprintln!("SKIP e2e_candle_embed_empty_batch: no BERT model in HF cache");
        return;
    };
    let model_path = model_dir.to_str().expect("model dir path is valid UTF-8");
    let provider =
        CandleBgeProvider::new(model_path, BgeModelVariant::BgeSmall).expect("provider must load");

    // Edge case: an empty batch yields an empty, well-formed result.
    let result = provider.embed(vec![]).await.expect("empty embed is Ok");
    assert!(result.is_empty(), "empty input yields an empty result");
    assert_eq!(result.len(), 0);
}

#[test]
fn e2e_candle_new_rejects_missing_model() {
    // Contract: `new` on a non-existent directory is a clean error, not a
    // panic. This needs no external model, so it always runs.
    let outcome = CandleBgeProvider::new(
        "/nonexistent/touring/bge/model/directory",
        BgeModelVariant::BgeLarge,
    );
    let Err(error) = outcome else {
        panic!("new must fail for a missing model directory");
    };
    assert!(
        !format!("{error}").is_empty(),
        "the load error must carry a diagnostic message"
    );
}
