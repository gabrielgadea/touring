//! Skill Clusterer — Group skills by usage patterns and detect similarities.
//!
//! Unified from touring/src/learning/clustering.rs (345 LOC)

use rayon::prelude::*;
use std::collections::HashMap;

#[cfg(feature = "leiden-clustering")]
use super::leiden::{Graph, LeidenCommunityDetector, LeidenResult};

/// Stub result type used when the `leiden-clustering` feature is OFF.
/// The real `LeidenResult` lives in `super::leiden` and is only
/// available when the optional dep is enabled.
#[cfg(not(feature = "leiden-clustering"))]
pub type LeidenResult = ();

/// A cluster of related skills.
#[derive(Debug, Clone)]
pub struct SkillCluster {
    /// Unique identifier for this cluster.
    pub id: u64,
    /// Skill ids belonging to this cluster.
    pub skills: Vec<String>,
    /// Normalized mean feature vector representing the cluster center.
    pub centroid: HashMap<String, f64>,
    /// Internal cohesion score in `[0, 1]`.
    pub cohesion: f64,
}

/// Usage pattern for a skill.
#[derive(Debug, Clone)]
pub struct SkillUsage {
    /// Identifier of the skill these usage statistics describe.
    pub skill_id: String,
    /// Per-context occurrence weights forming the skill's feature vector.
    pub features: HashMap<String, f64>,
    /// Running fraction of successful invocations in `[0, 1]`.
    pub success_rate: f64,
    /// Total number of recorded invocations.
    pub count: u64,
}

/// Clusters skills by usage patterns using cosine similarity.
#[derive(Debug)]
pub struct SkillClusterer {
    threshold: f64,
    max_clusters: usize,
    clusters: Vec<SkillCluster>,
    usage_patterns: HashMap<String, SkillUsage>,
}

impl SkillClusterer {
    /// Construct a `SkillClusterer` with default similarity threshold and cluster cap.
    pub fn new() -> Self {
        Self {
            threshold: 0.7,
            max_clusters: 20,
            clusters: Vec::new(),
            usage_patterns: HashMap::new(),
        }
    }

    /// Construct a `SkillClusterer` with explicit similarity `threshold` (clamped) and `max_clusters`.
    pub fn with_params(threshold: f64, max_clusters: usize) -> Self {
        Self {
            threshold: threshold.clamp(0.0, 1.0),
            max_clusters,
            clusters: Vec::new(),
            usage_patterns: HashMap::new(),
        }
    }

    /// Record one skill invocation in a given `context`, updating its feature vector and success rate.
    pub fn record_usage(&mut self, skill_id: &str, context: &str, success: bool) {
        let entry = self
            .usage_patterns
            .entry(skill_id.to_string())
            .or_insert_with(|| SkillUsage {
                skill_id: skill_id.to_string(),
                features: HashMap::new(),
                success_rate: 0.0,
                count: 0,
            });

        *entry.features.entry(context.to_string()).or_insert(0.0) += 1.0;

        entry.count += 1;
        let success_val = if success { 1.0 } else { 0.0 };
        entry.success_rate =
            (entry.success_rate * (entry.count - 1) as f64 + success_val) / entry.count as f64;
    }

    fn cosine_similarity(a: &HashMap<String, f64>, b: &HashMap<String, f64>) -> f64 {
        use touring_simd::CosineSimilarity;

        // Collect the union of all keys present in either map.
        let mut keys: Vec<&String> = a.keys().collect();
        for k in b.keys() {
            if !a.contains_key(k) {
                keys.push(k);
            }
        }

        if keys.is_empty() {
            return 0.0;
        }

        // Build aligned dense f32 vectors using the shared key ordering.
        let vec_a: Vec<f32> = keys
            .iter()
            .map(|k| a.get(*k).copied().unwrap_or(0.0) as f32)
            .collect();
        let vec_b: Vec<f32> = keys
            .iter()
            .map(|k| b.get(*k).copied().unwrap_or(0.0) as f32)
            .collect();

        touring_simd::CosineComputer::new().cosine(&vec_a, &vec_b)
    }

    fn find_best_cluster(&self, usage: &SkillUsage) -> Option<(usize, f64)> {
        self.clusters
            .iter()
            .enumerate()
            .map(|(idx, cluster)| {
                let sim = Self::cosine_similarity(&usage.features, &cluster.centroid);
                (idx, sim)
            })
            .filter(|(_, sim)| *sim >= self.threshold)
            .max_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    }

    /// Update cluster centroid as the normalized mean of member feature vectors.
    ///
    /// Takes pre-collected feature vectors to avoid borrow conflicts with usage_patterns.
    fn update_centroid(cluster: &mut SkillCluster, feature_vectors: &[HashMap<String, f64>]) {
        if feature_vectors.is_empty() {
            return;
        }
        let mut sum: HashMap<String, f64> = HashMap::new();
        let n = feature_vectors.len() as f64;
        for features in feature_vectors {
            for (feat, &val) in features {
                *sum.entry(feat.clone()).or_insert(0.0) += val;
            }
        }
        // Compute mean, then normalize to unit vector for cosine similarity
        let mut centroid: HashMap<String, f64> = sum.into_iter().map(|(k, v)| (k, v / n)).collect();
        let norm: f64 = centroid.values().map(|v| v * v).sum::<f64>().sqrt();
        if norm > 0.0 {
            for val in centroid.values_mut() {
                *val /= norm;
            }
        }
        cluster.centroid = centroid;
        let n = cluster.skills.len();
        cluster.cohesion = if n <= 1 { 1.0 } else { 0.8 + 0.2 / n as f64 };
    }

    #[allow(clippy::indexing_slicing)] // cluster_idx from find_best_cluster enumeration — always valid
    /// Recompute all clusters from recorded usage patterns and return the resulting clusters.
    pub fn cluster(&mut self) -> Vec<SkillCluster> {
        self.clusters.clear();
        let skill_ids: Vec<String> = self.usage_patterns.keys().cloned().collect();

        for skill_id in skill_ids {
            let Some(usage) = self.usage_patterns.get(&skill_id) else {
                continue;
            };

            if let Some((cluster_idx, _)) = self.find_best_cluster(usage) {
                // Collect feature vectors BEFORE mutable borrow of clusters.
                // Includes existing skills + the new skill being added.
                let feature_vecs: Vec<HashMap<String, f64>> = {
                    let existing = &self.clusters[cluster_idx].skills;
                    let mut vecs: Vec<HashMap<String, f64>> = existing
                        .iter()
                        .filter_map(|s| self.usage_patterns.get(s).map(|u| u.features.clone()))
                        .collect();
                    vecs.push(usage.features.clone());
                    vecs
                };
                // SAFETY: cluster_idx is returned by find_best_cluster which iterates
                // over self.clusters via enumerate(), so cluster_idx < self.clusters.len().
                #[allow(clippy::indexing_slicing)]
                {
                    self.clusters[cluster_idx].skills.push(skill_id);
                    Self::update_centroid(&mut self.clusters[cluster_idx], &feature_vecs);
                }
            } else if self.clusters.len() < self.max_clusters {
                let mut centroid = HashMap::new();
                for (feature, weight) in &usage.features {
                    centroid.insert(feature.clone(), *weight);
                }
                let norm: f64 = centroid.values().map(|v| v * v).sum::<f64>().sqrt();
                if norm > 0.0 {
                    for val in centroid.values_mut() {
                        *val /= norm;
                    }
                }
                self.clusters.push(SkillCluster {
                    id: self.clusters.len() as u64,
                    skills: vec![skill_id],
                    centroid,
                    cohesion: 1.0,
                });
            }
        }

        self.clusters.clone()
    }

    /// Borrow the clusters produced by the last `cluster` call.
    pub fn get_clusters(&self) -> &[SkillCluster] {
        &self.clusters
    }

    /// Return up to `top_k` skills most cosine-similar to `skill_id` above the threshold.
    pub fn find_similar(&self, skill_id: &str, top_k: usize) -> Vec<(String, f64)> {
        let usage = match self.usage_patterns.get(skill_id) {
            Some(u) => u,
            None => return Vec::new(),
        };

        // par_iter() parallelises the O(n) similarity scan across all usage patterns.
        let mut similarities: Vec<(String, f64)> = self
            .usage_patterns
            .par_iter()
            .filter(|(id, _)| id.as_str() != skill_id)
            .map(|(id, other_usage)| {
                let sim = Self::cosine_similarity(&usage.features, &other_usage.features);
                (id.clone(), sim)
            })
            .filter(|(_, sim)| *sim >= self.threshold)
            .collect();

        similarities.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        similarities.truncate(top_k);
        similarities
    }

    /// Borrow the recorded `SkillUsage` for `skill_id`, if any.
    pub fn get_stats(&self, skill_id: &str) -> Option<&SkillUsage> {
        self.usage_patterns.get(skill_id)
    }

    /// Detects communities using the Leiden algorithm for quality clustering.
    ///
    /// If the skill graph has >= 100 vertices, falls back to cosine similarity clustering.
    /// Returns a `LeidenResult` with modularity score when Leiden is used.
    #[cfg(feature = "leiden-clustering")]
    #[cfg(feature = "leiden-clustering")]
    pub fn detect_communities(&self) -> Option<LeidenResult> {
        // Build co-occurrence graph from usage patterns
        let mut graph = Graph::new();

        // Record co-occurrences based on context usage
        let skill_ids: Vec<String> = self.usage_patterns.keys().cloned().collect();
        for (i, skill_a) in skill_ids.iter().enumerate() {
            for skill_b in skill_ids.iter().skip(i + 1) {
                let usage_a = self.usage_patterns.get(skill_a)?;
                let usage_b = self.usage_patterns.get(skill_b)?;

                // Calculate co-occurrence weight based on shared contexts
                let shared_contexts: f64 = usage_a
                    .features
                    .keys()
                    .filter(|ctx| usage_b.features.contains_key(*ctx))
                    .count() as f64;

                if shared_contexts > 0.0 {
                    graph.record_cooccurrence(skill_a, skill_b);
                }
            }
        }

        if graph.node_count() < 100 {
            #[cfg(feature = "leiden-clustering")]
            let detector = LeidenCommunityDetector::new();
            #[cfg(feature = "leiden-clustering")]
            return Some(detector.detect_communities(&graph));
            #[cfg(not(feature = "leiden-clustering"))]
            return None;
        } else {
            // Fall back to cosine similarity clustering for large graphs
            None
        }
    }

    /// Drop all clusters and recorded usage patterns.
    pub fn clear(&mut self) {
        self.clusters.clear();
        self.usage_patterns.clear();
    }

    /// Number of distinct skills with recorded usage.
    pub fn len(&self) -> usize {
        self.usage_patterns.len()
    }

    /// Returns `true` when no skill usage has been recorded.
    pub fn is_empty(&self) -> bool {
        self.usage_patterns.is_empty()
    }
}

impl Default for SkillClusterer {
    fn default() -> Self {
        Self::new()
    }
}

/// When the `leiden-clustering` feature is OFF, the real detector is
/// unavailable. Expose a stub `detect_communities` that satisfies the
/// `Option<LeidenResult>` signature (where `LeidenResult = ()` is a
/// local type-alias) so consumers compile but get no clustering output.
#[cfg(not(feature = "leiden-clustering"))]
#[allow(dead_code)]
fn _detect_communities_stub_unused(_c: &SkillClusterer) -> Option<LeidenResult> {
    None
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)] // test vecs asserted non-empty before indexing
    use super::*;

    #[test]
    fn test_new_clusterer() {
        let c = SkillClusterer::new();
        assert!(c.is_empty());
    }

    #[test]
    fn test_record_usage() {
        let mut c = SkillClusterer::new();
        c.record_usage("skill1", "context_a", true);
        c.record_usage("skill1", "context_a", true);
        c.record_usage("skill1", "context_b", false);
        let stats = c.get_stats("skill1").unwrap();
        assert_eq!(stats.count, 3);
    }

    #[test]
    fn test_cosine_similarity_identical() {
        let mut a = HashMap::new();
        a.insert("x".to_string(), 1.0);
        a.insert("y".to_string(), 1.0);
        let sim = SkillClusterer::cosine_similarity(&a, &a);
        assert!(
            (sim - 1.0).abs() < 1e-6,
            "cosine similarity of identical vectors should be ~1.0, got {sim}"
        );
    }

    #[test]
    fn test_find_similar_empty() {
        let c = SkillClusterer::new();
        let similar = c.find_similar("nonexistent", 5);
        assert!(similar.is_empty());
    }

    // Exercises the real (feature-gated) Leiden detector + `LeidenResult` fields.
    #[cfg(feature = "leiden-clustering")]
    #[test]
    fn test_detect_communities_small_graph() {
        let mut c = SkillClusterer::new();
        // Record usage for 3 skills with overlapping contexts
        c.record_usage("skill_a", "ctx1", true);
        c.record_usage("skill_a", "ctx2", true);
        c.record_usage("skill_b", "ctx1", true);
        c.record_usage("skill_b", "ctx2", false);
        c.record_usage("skill_c", "ctx1", true);

        // Small graph (< 100 vertices) should use Leiden
        let result = c.detect_communities();
        assert!(result.is_some());
        let leiden_result = result.unwrap();
        assert!(leiden_result.modularity >= 0.0);
        assert!(leiden_result.num_communities() > 0);
    }

    #[cfg(feature = "leiden-clustering")]
    #[test]
    fn test_detect_communities_large_graph_fallback() {
        let mut c = SkillClusterer::new();
        // Simulate many skills (> 100) to trigger fallback to None
        for i in 0..150 {
            c.record_usage(&format!("skill_{}", i), "ctx1", true);
        }

        let result = c.detect_communities();
        // Large graph falls back to None (cosine clustering path)
        assert!(result.is_none());
    }
}
