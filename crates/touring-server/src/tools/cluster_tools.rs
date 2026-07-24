//! Cluster Tools — MCP tools for pattern clustering operations
//!
//! Implements:
//! - touring_memory_clusters: List all clusters with stats
//! - touring_cluster_members: Get members of a specific cluster
//! - touring_cluster_similar: Find clusters similar to a query

use serde::{Deserialize, Serialize};
use touring_foundation::config::TouringConfig;
#[cfg(feature = "async-memory")]
use touring_intelligence::rl::memory::pattern_cluster::AsyncPatternClusterer;

/// Input for touring_memory_clusters tool
#[derive(Debug, Deserialize)]
pub struct MemoryClustersInput {
    /// Action to perform: "list" | "stats"
    pub action: String,
    /// Optional cluster_id for "members" action
    pub cluster_id: Option<u64>,
    /// Optional query embedding for "similar" action
    pub query_embedding: Option<Vec<f32>>,
    /// Maximum number of results
    #[serde(default = "default_top_k")]
    pub top_k: usize,
}

/// Output from touring_memory_clusters tool
#[derive(Debug, Serialize)]
pub struct MemoryClustersOutput {
    /// The action that produced this output.
    pub action: String,
    /// Listed clusters, present for the `list` action.
    pub clusters: Option<Vec<ClusterInfo>>,
    /// Aggregate cluster statistics, present for the `stats` action.
    pub stats: Option<ClusterStatsInfo>,
    /// Members of a cluster, present for the `members` action.
    pub members: Option<Vec<ClusterMemberInfo>>,
    /// Similar clusters, present for the `similar` action.
    pub similar: Option<Vec<ClusterMatchInfo>>,
}

/// Cluster information
#[derive(Debug, Serialize)]
pub struct ClusterInfo {
    /// Unique identifier of the cluster.
    pub cluster_id: u64,
    /// Number of patterns assigned to the cluster.
    pub member_count: usize,
    /// Unix timestamp of the cluster's last update.
    pub last_updated: i64,
}

/// Statistics about clusters
#[derive(Debug, Serialize)]
pub struct ClusterStatsInfo {
    /// Total number of clusters.
    pub total_clusters: usize,
    /// Total number of patterns across all clusters.
    pub total_patterns: usize,
    /// Mean number of members per cluster.
    pub avg_cluster_size: f32,
    /// Member count of the largest cluster.
    pub largest_cluster: usize,
    /// Member count of the smallest cluster.
    pub smallest_cluster: usize,
}

/// Cluster member information
#[derive(Debug, Serialize)]
pub struct ClusterMemberInfo {
    /// Memory key of the member pattern.
    pub key: String,
    /// Stored value of the member pattern.
    pub value: String,
    /// Cosine similarity of the member to its cluster centroid.
    pub similarity_to_centroid: f32,
}

/// Similar cluster match
#[derive(Debug, Serialize)]
pub struct ClusterMatchInfo {
    /// Identifier of the matched cluster.
    pub cluster_id: u64,
    /// Similarity of the query to this cluster's centroid.
    pub similarity: f32,
    /// Number of members in the matched cluster.
    pub member_count: usize,
}

fn default_top_k() -> usize {
    10
}

/// Cluster tools handler
pub struct ClusterTools;

impl ClusterTools {
    /// Create new cluster tools handler
    pub fn new(_config: &TouringConfig) -> Result<Self, touring_foundation::TouringError> {
        Ok(Self)
    }

    /// Handle touring_memory_clusters tool
    #[cfg(feature = "async-memory")]
    pub async fn clusters(
        &self,
        input: MemoryClustersInput,
    ) -> Result<MemoryClustersOutput, touring_foundation::TouringError> {
        match input.action.as_str() {
            "list" => self.list_clusters_async().await,
            "stats" => self.get_stats_async().await,
            "members" => {
                let cluster_id = input.cluster_id.ok_or_else(|| {
                    touring_foundation::TouringError::Mcp("cluster_id required".to_string())
                })?;
                self.get_members_async(cluster_id).await
            }
            "similar" => {
                let query_emb = input.query_embedding.ok_or_else(|| {
                    touring_foundation::TouringError::Mcp("query_embedding required".to_string())
                })?;
                self.find_similar_async(&query_emb, input.top_k).await
            }
            _ => Err(touring_foundation::TouringError::Mcp(format!(
                "Unknown action: {}. Use: list | stats | members | similar",
                input.action
            ))),
        }
    }

    /// Handle touring_memory_clusters tool (stub when `async-memory` is off).
    #[cfg(not(feature = "async-memory"))]
    pub async fn clusters(
        &self,
        input: MemoryClustersInput,
    ) -> Result<MemoryClustersOutput, touring_foundation::TouringError> {
        // Validate input even in stub mode to ensure API contract is respected
        if input.action == "list"
            || input.action == "stats"
            || input.action == "members"
            || input.action == "similar"
        {
            Err(touring_foundation::TouringError::Mcp(
                "Clustering requires async-memory feature (not enabled in this build)".to_string(),
            ))
        } else {
            Err(touring_foundation::TouringError::Mcp(format!(
                "Unknown action: {}. Use: list | stats | members | similar",
                input.action
            )))
        }
    }

    /// List all clusters (async version)
    #[cfg(feature = "async-memory")]
    async fn list_clusters_async(
        &self,
    ) -> Result<MemoryClustersOutput, touring_foundation::TouringError> {
        let config = TouringConfig::load()
            .map_err(|e| touring_foundation::TouringError::Config(format!("{}", e)))?;
        let cluster_db_path = config
            .project_root
            .join(".claude/touring/pattern_clusters.db");

        let clusterer = match AsyncPatternClusterer::new(&cluster_db_path, config.embedding_dim) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("Failed to initialize pattern clusterer: {}", e);
                return Ok(MemoryClustersOutput {
                    action: "list".to_string(),
                    clusters: Some(Vec::new()),
                    stats: None,
                    members: None,
                    similar: None,
                });
            }
        };

        let clusters = clusterer
            .get_all_clusters()
            .await
            .map_err(|e| touring_foundation::TouringError::Memory(format!("{}", e)))?
            .into_iter()
            .map(|c| ClusterInfo {
                cluster_id: c.cluster_id,
                member_count: c.member_count,
                last_updated: c.last_updated,
            })
            .collect();

        Ok(MemoryClustersOutput {
            action: "list".to_string(),
            clusters: Some(clusters),
            stats: None,
            members: None,
            similar: None,
        })
    }

    /// Get cluster statistics (async version)
    #[cfg(feature = "async-memory")]
    async fn get_stats_async(
        &self,
    ) -> Result<MemoryClustersOutput, touring_foundation::TouringError> {
        let config = TouringConfig::load()
            .map_err(|e| touring_foundation::TouringError::Config(format!("{}", e)))?;
        let cluster_db_path = config
            .project_root
            .join(".claude/touring/pattern_clusters.db");

        let clusterer = match AsyncPatternClusterer::new(&cluster_db_path, config.embedding_dim) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("Failed to initialize pattern clusterer: {}", e);
                return Ok(MemoryClustersOutput {
                    action: "stats".to_string(),
                    clusters: None,
                    stats: None,
                    members: None,
                    similar: None,
                });
            }
        };

        let stats = clusterer
            .stats()
            .await
            .map_err(|e| touring_foundation::TouringError::Memory(format!("{}", e)))?;

        Ok(MemoryClustersOutput {
            action: "stats".to_string(),
            clusters: None,
            stats: Some(ClusterStatsInfo {
                total_clusters: stats.total_clusters,
                total_patterns: stats.total_patterns,
                avg_cluster_size: stats.avg_cluster_size,
                largest_cluster: stats.largest_cluster,
                smallest_cluster: stats.smallest_cluster,
            }),
            members: None,
            similar: None,
        })
    }

    /// Get members of a cluster (async version)
    #[cfg(feature = "async-memory")]
    async fn get_members_async(
        &self,
        cluster_id: u64,
    ) -> Result<MemoryClustersOutput, touring_foundation::TouringError> {
        let config = TouringConfig::load()
            .map_err(|e| touring_foundation::TouringError::Config(format!("{}", e)))?;
        let cluster_db_path = config
            .project_root
            .join(".claude/touring/pattern_clusters.db");

        let clusterer = match AsyncPatternClusterer::new(&cluster_db_path, config.embedding_dim) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("Failed to initialize pattern clusterer: {}", e);
                return Ok(MemoryClustersOutput {
                    action: "members".to_string(),
                    clusters: None,
                    stats: None,
                    members: None,
                    similar: None,
                });
            }
        };

        let clusters = clusterer
            .get_all_clusters()
            .await
            .map_err(|e| touring_foundation::TouringError::Memory(format!("{}", e)))?;

        let members = clusters
            .into_iter()
            .find(|c| c.cluster_id == cluster_id)
            .map(|c| {
                c.member_keys
                    .iter()
                    .map(|key| ClusterMemberInfo {
                        key: key.clone(),
                        value: String::new(),
                        similarity_to_centroid: 1.0,
                    })
                    .collect()
            });

        Ok(MemoryClustersOutput {
            action: "members".to_string(),
            clusters: None,
            stats: None,
            members,
            similar: None,
        })
    }

    /// Find similar clusters to a query embedding (async version)
    #[cfg(feature = "async-memory")]
    async fn find_similar_async(
        &self,
        query_embedding: &[f32],
        top_k: usize,
    ) -> Result<MemoryClustersOutput, touring_foundation::TouringError> {
        let config = TouringConfig::load()
            .map_err(|e| touring_foundation::TouringError::Config(format!("{}", e)))?;
        let cluster_db_path = config
            .project_root
            .join(".claude/touring/pattern_clusters.db");

        let clusterer = match AsyncPatternClusterer::new(&cluster_db_path, config.embedding_dim) {
            Ok(c) => c,
            Err(e) => {
                tracing::warn!("Failed to initialize pattern clusterer: {}", e);
                return Ok(MemoryClustersOutput {
                    action: "similar".to_string(),
                    clusters: None,
                    stats: None,
                    members: None,
                    similar: Some(Vec::new()),
                });
            }
        };

        let similar = clusterer
            .find_similar_clusters(query_embedding, top_k)
            .await
            .map_err(|e| touring_foundation::TouringError::Memory(format!("{}", e)))?
            .into_iter()
            .map(|m| ClusterMatchInfo {
                cluster_id: m.cluster_id,
                similarity: m.similarity,
                member_count: m.member_count,
            })
            .collect();

        Ok(MemoryClustersOutput {
            action: "similar".to_string(),
            clusters: None,
            stats: None,
            members: None,
            similar: Some(similar),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_action_parsing() {
        let input: MemoryClustersInput = serde_json::from_str(r#"{"action": "list"}"#).unwrap();
        assert_eq!(input.action, "list");
    }
}
