//! Co-edit predictor: predicts which files should be edited next
//! based on co-edit history, import graph, and blast radius.
//!
//! Uses Reciprocal Rank Fusion (RRF) to combine 3 ranked signals into
//! a single ranking. RRF is robust to score scale differences between
//! signals and produces good results without per-signal tuning.
//!
//! # Signals
//!
//! 1. **Co-edit weights**: files historically edited together (from `file_coedits`)
//! 2. **Import graph**: files connected via imports (from `file_relations`)
//! 3. **Blast radius**: files affected by changes (from AST analysis)
//!
//! # Reference
//!
//! Cormack, Clarke & Butt (2009). "Reciprocal Rank Fusion outperforms
//! Condorcet and individual Rank Learning Methods". SIGIR 2009.

use std::collections::HashMap;

/// Default RRF smoothing constant (k). The original paper uses k=60.
/// Higher k reduces the influence of top-ranked documents.
const DEFAULT_RRF_K: f64 = 60.0;

/// Default half-life for co-edit temporal decay: 30 days in fractional days.
const DEFAULT_HALF_LIFE_DAYS: f64 = 30.0;

// ---------------------------------------------------------------------------
// INS-C5: CoEditRecord — co-edit entry with temporal decay support
// ---------------------------------------------------------------------------

/// INS-C5: A co-edit observation with count and timestamp for temporal decay.
#[derive(Debug, Clone)]
pub struct CoEditRecord {
    /// Accumulated co-edit count (decays over time).
    pub count: f64,
    /// Epoch of the last co-edit observation (fractional days since session start).
    pub last_epoch: f64,
}

impl CoEditRecord {
    /// Create a new record with a single observation at `epoch`.
    pub fn new(epoch: f64) -> Self {
        Self {
            count: 1.0,
            last_epoch: epoch,
        }
    }

    /// Observe another co-edit at `epoch` and update the record.
    pub fn observe(&mut self, epoch: f64) {
        self.count += 1.0;
        if epoch > self.last_epoch {
            self.last_epoch = epoch;
        }
    }
}

/// Predicts which files should be edited next based on multi-signal fusion.
///
/// The `rrf_k` parameter controls the smoothing constant in Reciprocal Rank
/// Fusion. Default is 60 (from the original paper). Lower values (20-40)
/// give more weight to top-ranked files.
/// INS-C5: Maintains `co_edits` map with temporal decay via `decayed_score`.
#[derive(Debug)]
pub struct CoEditPredictor {
    rrf_k: f64,
    /// INS-C5: co-edit observations keyed by (file_a, file_b) pair.
    co_edits: HashMap<(String, String), CoEditRecord>,
}

impl Default for CoEditPredictor {
    fn default() -> Self {
        Self::new()
    }
}

impl CoEditPredictor {
    /// Create a new predictor with default RRF_K=60.
    pub fn new() -> Self {
        Self {
            rrf_k: DEFAULT_RRF_K,
            co_edits: HashMap::new(),
        }
    }

    /// Create a new predictor with a custom RRF_K smoothing constant.
    pub fn with_rrf_k(rrf_k: f64) -> Self {
        Self {
            rrf_k: rrf_k.max(1.0),
            co_edits: HashMap::new(),
        }
    }

    /// INS-C5: Record a co-edit observation between `file_a` and `file_b` at `epoch`.
    ///
    /// Pairs are stored canonically (sorted) so (a,b) and (b,a) map to the same entry.
    pub fn record_co_edit(&mut self, file_a: &str, file_b: &str, epoch: f64) {
        let key = if file_a <= file_b {
            (file_a.to_string(), file_b.to_string())
        } else {
            (file_b.to_string(), file_a.to_string())
        };
        self.co_edits
            .entry(key)
            .and_modify(|r| r.observe(epoch))
            .or_insert_with(|| CoEditRecord::new(epoch));
    }

    /// INS-C5: Compute the temporally-decayed score for a co-edit record.
    ///
    /// `score = count × 2^(-(now_epoch - last_epoch) / half_life_days)`
    pub fn decayed_score(&self, record: &CoEditRecord, now_epoch: f64, half_life_days: f64) -> f64 {
        let elapsed = (now_epoch - record.last_epoch).max(0.0);
        record.count * 2.0_f64.powf(-elapsed / half_life_days)
    }

    /// INS-C5: Build a ranked co-edit list for `file` using temporal decay.
    ///
    /// Returns `(peer_file, decayed_score)` pairs sorted by score descending.
    pub fn ranked_co_edits(&self, file: &str, now_epoch: f64) -> Vec<(String, f64)> {
        let mut results: Vec<(String, f64)> = self
            .co_edits
            .iter()
            .filter_map(|((a, b), record)| {
                let peer = if a == file {
                    Some(b.clone())
                } else if b == file {
                    Some(a.clone())
                } else {
                    None
                };
                peer.map(|p| {
                    (
                        p,
                        self.decayed_score(record, now_epoch, DEFAULT_HALF_LIFE_DAYS),
                    )
                })
            })
            .collect();
        results.sort_by(|x, y| y.1.partial_cmp(&x.1).unwrap_or(std::cmp::Ordering::Equal));
        results
    }

    /// Predict top-K files likely to need editing after `current_file`.
    ///
    /// Fuses 3 ranked signals via Reciprocal Rank Fusion:
    /// - `coedits`: files co-edited with current file, ranked by co-edit weight
    /// - `imports`: files imported by or importing current file, ranked by relevance
    /// - `blast_radius`: files in the blast radius, ranked by impact
    ///
    /// Each input is a ranked list of `(file_path, score)` pairs, pre-sorted
    /// by score descending. The absolute score values are ignored; only rank
    /// order matters for RRF.
    ///
    /// Returns `(file_path, rrf_score)` pairs sorted by RRF score descending.
    pub fn predict_next_files(
        coedits: &[(String, f64)],
        imports: &[(String, f64)],
        blast_radius: &[(String, f64)],
        top_k: usize,
    ) -> Vec<(String, f64)> {
        Self::rrf_fuse(&[coedits, imports, blast_radius], DEFAULT_RRF_K, top_k)
    }

    /// Instance method using the configured rrf_k.
    pub fn predict(
        &self,
        coedits: &[(String, f64)],
        imports: &[(String, f64)],
        blast_radius: &[(String, f64)],
        top_k: usize,
    ) -> Vec<(String, f64)> {
        Self::rrf_fuse(&[coedits, imports, blast_radius], self.rrf_k, top_k)
    }

    /// Reciprocal Rank Fusion over multiple ranked lists.
    ///
    /// For each document appearing in any list:
    ///   `score(d) = sum_i( 1 / (k + rank_i(d)) )`
    ///
    /// where `rank_i(d)` is the 1-based rank of document `d` in list `i`,
    /// and `k` is the smoothing constant.
    fn rrf_fuse(ranked_lists: &[&[(String, f64)]], rrf_k: f64, top_k: usize) -> Vec<(String, f64)> {
        let mut scores: HashMap<&str, f64> = HashMap::new();

        for list in ranked_lists {
            for (rank, (file, _weight)) in list.iter().enumerate() {
                // RRF uses 1-based ranking
                let rrf_score = 1.0 / (rrf_k + (rank + 1) as f64);
                *scores.entry(file.as_str()).or_insert(0.0) += rrf_score;
            }
        }

        let mut results: Vec<(String, f64)> = scores
            .into_iter()
            .map(|(k, v)| (k.to_string(), v))
            .collect();
        results.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
        results.truncate(top_k);
        results
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_rrf_fuse_single_list() {
        let list = vec![
            ("a.rs".to_string(), 10.0),
            ("b.rs".to_string(), 5.0),
            ("c.rs".to_string(), 1.0),
        ];
        let result = CoEditPredictor::rrf_fuse(&[&list], DEFAULT_RRF_K, 10);

        assert_eq!(result.len(), 3);
        // a.rs is rank 1 → score = 1/(60+1)
        assert_eq!(result[0].0, "a.rs");
        assert!((result[0].1 - 1.0 / 61.0).abs() < 1e-10);
        // b.rs is rank 2 → score = 1/(60+2)
        assert_eq!(result[1].0, "b.rs");
        assert!((result[1].1 - 1.0 / 62.0).abs() < 1e-10);
        // c.rs is rank 3 → score = 1/(60+3)
        assert_eq!(result[2].0, "c.rs");
        assert!((result[2].1 - 1.0 / 63.0).abs() < 1e-10);
    }

    #[test]
    fn test_rrf_fuse_combines_three_signals() {
        let coedits = vec![("a.rs".to_string(), 10.0), ("b.rs".to_string(), 5.0)];
        let imports = vec![("b.rs".to_string(), 3.0), ("c.rs".to_string(), 1.0)];
        let blast = vec![("c.rs".to_string(), 8.0), ("a.rs".to_string(), 2.0)];

        let result = CoEditPredictor::predict_next_files(&coedits, &imports, &blast, 10);

        assert_eq!(result.len(), 3);

        // Expected scores:
        // a.rs: coedits rank 1 (1/61) + blast rank 2 (1/62) = 0.01639 + 0.01613 = 0.03252
        // b.rs: coedits rank 2 (1/62) + imports rank 1 (1/61) = 0.01613 + 0.01639 = 0.03252
        // c.rs: imports rank 2 (1/62) + blast rank 1 (1/61) = 0.01613 + 0.01639 = 0.03252
        //
        // All three have the same RRF score (rank 1 + rank 2 from different lists)
        // so they should all be present with approximately equal scores.
        let file_names: Vec<&str> = result.iter().map(|(f, _)| f.as_str()).collect();
        assert!(file_names.contains(&"a.rs"));
        assert!(file_names.contains(&"b.rs"));
        assert!(file_names.contains(&"c.rs"));

        // All scores should be approximately equal
        let scores: Vec<f64> = result.iter().map(|(_, s)| *s).collect();
        let expected = 1.0 / 61.0 + 1.0 / 62.0;
        for s in &scores {
            assert!(
                (s - expected).abs() < 1e-10,
                "score {s} should be approximately {expected}"
            );
        }
    }

    #[test]
    fn test_predict_empty_inputs() {
        let result = CoEditPredictor::predict_next_files(&[], &[], &[], 10);
        assert!(result.is_empty());
    }

    #[test]
    fn test_predict_excludes_duplicates() {
        // Same file appears in all 3 lists → it should appear ONCE with highest score
        let coedits = vec![("shared.rs".to_string(), 10.0)];
        let imports = vec![("shared.rs".to_string(), 5.0)];
        let blast = vec![("shared.rs".to_string(), 3.0)];

        let result = CoEditPredictor::predict_next_files(&coedits, &imports, &blast, 10);

        assert_eq!(result.len(), 1, "duplicate file should appear only once");
        assert_eq!(result[0].0, "shared.rs");

        // Score should be 3 * 1/(60+1) since it's rank 1 in all 3 lists
        let expected = 3.0 / 61.0;
        assert!(
            (result[0].1 - expected).abs() < 1e-10,
            "score {} should be {expected}",
            result[0].1,
        );
    }

    #[test]
    fn test_predict_top_k_limit() {
        let coedits: Vec<(String, f64)> = (0..20)
            .map(|i| (format!("file_{i}.rs"), (20 - i) as f64))
            .collect();

        let result = CoEditPredictor::predict_next_files(&coedits, &[], &[], 5);

        assert_eq!(result.len(), 5, "should return at most top_k results");
        // First result should be the highest-ranked file
        assert_eq!(result[0].0, "file_0.rs");
    }

    #[test]
    fn test_rrf_ranking_with_overlapping_lists() {
        // File appears at rank 1 in two lists vs rank 1 in one list
        let list_a = vec![
            ("popular.rs".to_string(), 10.0),
            ("rare.rs".to_string(), 1.0),
        ];
        let list_b = vec![
            ("popular.rs".to_string(), 8.0),
            ("other.rs".to_string(), 2.0),
        ];
        let list_c = vec![("other.rs".to_string(), 5.0)];

        let result = CoEditPredictor::rrf_fuse(&[&list_a, &list_b, &list_c], DEFAULT_RRF_K, 10);

        // popular.rs: rank 1 in A (1/61) + rank 1 in B (1/61) = 2/61 ≈ 0.03279
        // other.rs: rank 2 in B (1/62) + rank 1 in C (1/61) ≈ 0.03252
        // rare.rs: rank 2 in A (1/62) ≈ 0.01613
        assert_eq!(
            result[0].0, "popular.rs",
            "most-fused file should rank first"
        );

        let popular_score = 2.0 / 61.0;
        assert!(
            (result[0].1 - popular_score).abs() < 1e-10,
            "popular.rs score should be 2/61"
        );
    }
}
