//! touring-hooks-prediction — predictive/classification layer extracted from touring-hooks (W8 pragmatic split).
//!
//! Internal workspace crate. Depends only on `touring-hooks-shared` (no SCC back-edge).
//! Re-exported verbatim by the `touring-hooks` facade so the external API is unchanged.

#![deny(missing_docs)]
// RBP-01 elite-lint ratchet (2026-06-16): prod-unwrap-free — lock against future
// bare unwrap in non-test code (`.expect("…")` stays the sanctioned escape).
#![cfg_attr(not(test), deny(clippy::unwrap_used))]

pub mod ann_memory;
pub mod classifier;
pub mod layer7_prediction;
pub mod llm_judge;
pub mod pii;
pub mod semantic_classifier;
pub mod tfidf_retriever;
