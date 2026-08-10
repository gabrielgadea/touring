//! touring-search-fusion: Intent classification + semantic weighting for search.

pub mod hybrid;
pub mod intent;

pub use hybrid::pipeline::{
    BackendStatus, ConfidenceTier, KeywordSearch, QueryIntent as HybridQueryIntent, SearchStats,
};
pub use hybrid::{
    HybridConfig, HybridQuery, HybridScorer, RrfFusion, SearchPipeline, SearchResult,
};
pub use intent::QueryIntent; // Re-export intent QueryIntent
pub use intent::{
    IntentResult, QueryIntent as IntentQueryIntent, apply_semantic_weighting, detect_intent,
};
