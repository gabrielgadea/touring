//! Skill clustering by usage patterns and community detection.
//!
//! - `cosine`: Cosine similarity-based skill clustering (vector space model)
//! - `leiden`: Leiden algorithm for graph-based community detection

pub mod cosine;
pub mod leiden;

pub use cosine::{SkillCluster, SkillClusterer, SkillUsage};
pub use leiden::{Community, Graph, LeidenAlgorithmConfig, LeidenCommunityDetector, LeidenResult};
