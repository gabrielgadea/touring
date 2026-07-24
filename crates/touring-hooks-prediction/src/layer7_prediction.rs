//! Layer 7: Prediction — ACO Wiring Enhancement for anticipatory intelligence.
//!
//! Extends the 6-layer ACO Wiring system with a 7th prediction layer.
//! Delegates to PredictiveFocusCache (E12) from touring-cognitive for
//! ACO pheromone-based co-edit prediction.
//!
//! `session_files()` exposes PFC candidate files for integration.
//!
//! Queried by `pre_read` and `pre_edit` hooks to anticipate next files.

use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex, RwLock};

use moka::sync::Cache;

/// A predicted file with confidence score.
#[derive(Debug, Clone)]
pub struct PredictedFile {
    /// File path.
    pub path: String,
    /// Prediction confidence (0.0 to 1.0).
    pub confidence: f64,
    /// Reason for prediction.
    pub reason: PredictionReason,
}

/// Why a file was predicted.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PredictionReason {
    /// File was recently edited together with current file.
    CoEdit,
    /// High pheromone trail intensity.
    PheromoneHot,
    /// Follows session edit sequence pattern.
    SessionSequence,
    /// Combined score from multiple sources.
    Combined,
}

/// Layer 7: Prediction engine for anticipatory context injection.
///
/// Predicts which files will likely be needed next based on:
/// - Co-edit graph (files edited together historically)
/// - Pheromone intensity (recent edit frequency)
/// - Session sequence (order of recent edits)
#[derive(Debug)]
pub struct PredictionLayer {
    /// Co-edit graph: file → files frequently edited together.
    ///
    /// Migrated 2026-04-16 from `RwLock<HashMap<String, Vec<CoEditEntry>>>` to
    /// `moka::sync::Cache<String, Arc<Mutex<Vec<CoEditEntry>>>>`. The key-level
    /// `Arc<Mutex<Vec<_>>>` keeps RMW atomicity for `record_co_edit` while the
    /// outer moka cache eliminates the global RwLock — unrelated keys (distinct
    /// source files) now mutate without contending each other. Reads are
    /// sharded lock-free.
    co_edit_graph: Cache<String, Arc<Mutex<Vec<CoEditEntry>>>>,
    /// Recent edit sequence in current session.
    ///
    /// Kept as `RwLock<VecDeque>` — this is a bounded rolling buffer (FIFO,
    /// capacity 100), not a cache. moka's eviction model is access-frequency
    /// based, which would violate the FIFO-by-position invariant callers
    /// rely on in `predict_by_session_sequence` and `session_files`.
    session_sequence: RwLock<VecDeque<String>>,
    /// Pheromone-based file heat (from ACO bus).
    ///
    /// Migrated 2026-04-16 to `moka::sync::Cache<String, f64>`. f64 is `Copy`
    /// so values are returned by value (no Arc needed), and writes are
    /// lock-free. Under write-heavy load from the ACO bus the old RwLock
    /// serialized every heat update behind a single writer; moka's sharded
    /// internal maps parallelize disjoint-key updates.
    file_heat: Cache<String, f64>,
}

impl Default for PredictionLayer {
    fn default() -> Self {
        Self::new()
    }
}

/// A co-edit entry with frequency count.
#[derive(Debug, Clone)]
struct CoEditEntry {
    file: String,
    frequency: u32,
}

impl PredictionLayer {
    /// Create a new prediction layer.
    pub fn new() -> Self {
        Self {
            co_edit_graph: Cache::builder().max_capacity(4096).build(),
            session_sequence: RwLock::new(VecDeque::new()),
            file_heat: Cache::builder().max_capacity(8192).build(),
        }
    }

    /// Record that two files were edited together.
    ///
    /// Called after each edit to build co-edit graph.
    pub fn record_co_edit(&self, file1: &str, file2: &str) {
        if file1 == file2 {
            return;
        }

        // `get_with` atomically inserts a fresh `Arc<Mutex<Vec<_>>>` on miss,
        // or returns the existing one on hit — both paths give us the same
        // `Arc` that any concurrent writer would see.
        Self::increment_co_edit(&self.co_edit_graph, file1, file2);
        Self::increment_co_edit(&self.co_edit_graph, file2, file1);
    }

    fn increment_co_edit(
        cache: &Cache<String, Arc<Mutex<Vec<CoEditEntry>>>>,
        from: &str,
        to: &str,
    ) {
        let entries_arc = cache.get_with(from.to_string(), || Arc::new(Mutex::new(Vec::new())));
        let mut entries = entries_arc.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(entry) = entries.iter_mut().find(|e| e.file == to) {
            entry.frequency += 1;
        } else {
            entries.push(CoEditEntry {
                file: to.to_string(),
                frequency: 1,
            });
        }
    }

    /// Record a file edit in the session sequence.
    pub fn record_edit(&self, file_path: &str) {
        let mut seq = self
            .session_sequence
            .write()
            .unwrap_or_else(|e| e.into_inner());
        seq.push_back(file_path.to_string());
        // Keep last 100 edits
        if seq.len() > 100 {
            seq.pop_front();
        }
    }

    /// Update file heat from pheromone system.
    pub fn update_file_heat(&self, file_path: &str, heat: f64) {
        // moka `insert` is lock-free; under write-heavy load from the ACO
        // bus this replaces the global RwLock writer with sharded atomics.
        self.file_heat.insert(file_path.to_string(), heat);
    }

    /// Predict next files to edit based on current file.
    ///
    /// Returns top-K predicted files with confidence scores.
    pub fn predict_next(&self, current_file: &str, k: usize) -> Vec<PredictedFile> {
        let mut predictions: Vec<PredictedFile> = Vec::new();

        // 1. Co-edit based predictions
        predictions.extend(self.predict_by_co_edit(current_file, k));

        // 2. Pheromone hot paths
        predictions.extend(self.predict_by_pheromones(k));

        // 3. Session sequence patterns
        predictions.extend(self.predict_by_session_sequence(current_file, k));

        // Deduplicate by path, keeping highest confidence per path
        let mut seen: HashMap<String, PredictedFile> = HashMap::new();
        for pred in predictions {
            seen.entry(pred.path.clone())
                .and_modify(|existing| {
                    if pred.confidence > existing.confidence {
                        *existing = pred.clone();
                    }
                })
                .or_insert(pred);
        }
        let mut deduped: Vec<PredictedFile> = seen.into_values().collect();
        deduped.sort_by(|a, b| {
            b.confidence
                .partial_cmp(&a.confidence)
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        deduped.into_iter().take(k).collect()
    }

    fn predict_by_co_edit(&self, current_file: &str, k: usize) -> Vec<PredictedFile> {
        let Some(entries_arc) = self.co_edit_graph.get(current_file) else {
            return Vec::new();
        };
        let entries = entries_arc.lock().unwrap_or_else(|e| e.into_inner());

        // Sort by frequency descending to return highest co-edit partners first.
        // Materialize `(file, frequency)` tuples so we can drop the mutex guard
        // before the map/collect (keeps the critical section small).
        let mut sorted: Vec<(String, u32)> = entries
            .iter()
            .map(|e| (e.file.clone(), e.frequency))
            .collect();
        drop(entries);
        sorted.sort_by_key(|b| std::cmp::Reverse(b.1));

        // Compute max_freq once O(1) from the sorted head
        let max_freq = sorted.first().map(|e| e.1 as f64).unwrap_or(1.0);

        sorted
            .into_iter()
            .take(k)
            .map(|(file, frequency)| PredictedFile {
                path: file,
                confidence: frequency as f64 / max_freq,
                reason: PredictionReason::CoEdit,
            })
            .collect()
    }

    fn predict_by_pheromones(&self, k: usize) -> Vec<PredictedFile> {
        // moka `iter` is lock-free; collect into an owned Vec so we can
        // sort by the Copy f64 without tying the borrow to the iterator.
        let mut hot: Vec<(String, f64)> = self
            .file_heat
            .iter()
            .map(|(k, v)| ((*k).clone(), v))
            .collect();
        hot.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));

        hot.into_iter()
            .take(k)
            .filter(|(_path, heat)| *heat > 0.5) // Only hot paths
            .map(|(path, heat)| PredictedFile {
                path,
                confidence: heat,
                reason: PredictionReason::PheromoneHot,
            })
            .collect()
    }

    fn predict_by_session_sequence(&self, current_file: &str, k: usize) -> Vec<PredictedFile> {
        let seq = self
            .session_sequence
            .read()
            .unwrap_or_else(|e| e.into_inner());

        // Find current_file in sequence and predict next
        let Some(pos) = seq.iter().position(|f| f == current_file) else {
            return Vec::new();
        };

        // Predict next k files after current position
        seq.iter()
            .skip(pos + 1)
            .take(k)
            .enumerate()
            .map(|(i, path)| {
                let decay = 1.0 / (i as f64 + 1.0); // Further = lower confidence
                PredictedFile {
                    path: path.clone(),
                    confidence: decay,
                    reason: PredictionReason::SessionSequence,
                }
            })
            .collect()
    }

    /// Get prediction confidence for a specific file.
    pub fn prediction_confidence(&self, file_path: &str) -> f64 {
        // Combine all prediction sources
        let co_edit = self.co_edit_confidence(file_path);
        let pheromone = self.pheromone_confidence(file_path);
        let sequence = self.sequence_confidence(file_path);

        // Weighted average
        co_edit * 0.4 + pheromone * 0.3 + sequence * 0.3
    }

    fn co_edit_confidence(&self, file_path: &str) -> f64 {
        // Sum total frequencies across all buckets — each bucket is locked
        // briefly then released to keep critical sections short.
        let mut total: u64 = 0;
        for (_, entries_arc) in self.co_edit_graph.iter() {
            let entries = entries_arc.lock().unwrap_or_else(|e| e.into_inner());
            total = total.saturating_add(entries.iter().map(|e| e.frequency as u64).sum::<u64>());
        }
        if total == 0 {
            return 0.0;
        }

        let relevant: u64 = self
            .co_edit_graph
            .get(file_path)
            .map(|arc| {
                let v = arc.lock().unwrap_or_else(|e| e.into_inner());
                v.iter().map(|e| e.frequency as u64).sum()
            })
            .unwrap_or(0);

        relevant as f64 / total as f64
    }

    fn pheromone_confidence(&self, file_path: &str) -> f64 {
        self.file_heat.get(file_path).unwrap_or(0.0).min(1.0)
    }

    fn sequence_confidence(&self, file_path: &str) -> f64 {
        let seq = self
            .session_sequence
            .read()
            .unwrap_or_else(|e| e.into_inner());
        if seq.is_empty() {
            return 0.0;
        }

        let occurrences = seq.iter().filter(|f| *f == file_path).count();
        occurrences as f64 / seq.len() as f64
    }

    /// Return unique file paths from the recent session sequence.
    ///
    /// Used by E12 integration: provides the candidate pool for
    /// `PredictiveFocusCache::prefetch_likely()` when L7 predictions
    /// are empty or insufficient.
    pub fn session_files(&self) -> Vec<String> {
        let seq = self
            .session_sequence
            .read()
            .unwrap_or_else(|e| e.into_inner());
        let mut seen = std::collections::HashSet::new();
        seq.iter().filter(|f| seen.insert(*f)).cloned().collect()
    }

    /// Clear all prediction data.
    pub fn clear(&self) {
        // moka invalidate_all is async/lazy; follow with run_pending_tasks
        // to force synchronous eviction so tests observe empty caches
        // immediately.
        self.co_edit_graph.invalidate_all();
        self.co_edit_graph.run_pending_tasks();
        self.session_sequence
            .write()
            .unwrap_or_else(|e| e.into_inner())
            .clear();
        self.file_heat.invalidate_all();
        self.file_heat.run_pending_tasks();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_prediction_layer_new() {
        let layer = PredictionLayer::new();
        let predictions = layer.predict_next("src/lib.rs", 5);
        assert!(predictions.is_empty());
    }

    #[test]
    fn test_record_co_edit() {
        let layer = PredictionLayer::new();
        layer.record_co_edit("src/lib.rs", "src/main.rs");

        let predictions = layer.predict_next("src/lib.rs", 5);
        assert!(!predictions.is_empty());
        assert_eq!(predictions[0].path, "src/main.rs");
        assert_eq!(predictions[0].reason, PredictionReason::CoEdit);
    }

    #[test]
    fn test_record_edit_sequence() {
        let layer = PredictionLayer::new();

        layer.record_edit("a.rs");
        layer.record_edit("b.rs");
        layer.record_edit("c.rs");

        let predictions = layer.predict_next("a.rs", 2);
        assert!(!predictions.is_empty());
        // b.rs should be predicted after a.rs
        assert!(predictions.iter().any(|p| p.path == "b.rs"));
    }

    #[test]
    fn test_update_file_heat() {
        let layer = PredictionLayer::new();
        layer.update_file_heat("hot.rs", 0.9);

        let predictions = layer.predict_by_pheromones(5);
        assert!(!predictions.is_empty());
        assert_eq!(predictions[0].path, "hot.rs");
    }

    #[test]
    fn test_clear() {
        let layer = PredictionLayer::new();
        layer.record_co_edit("a.rs", "b.rs");
        layer.record_edit("c.rs");
        layer.update_file_heat("d.rs", 0.8);

        layer.clear();

        assert!(layer.predict_next("a.rs", 5).is_empty());
        assert!(layer.predict_by_pheromones(5).is_empty());
    }

    #[test]
    fn test_co_edit_bidirectional() {
        let layer = PredictionLayer::new();
        layer.record_co_edit("a.rs", "b.rs");

        // Both directions should have the co-edit
        let from_a = layer.predict_next("a.rs", 5);
        let from_b = layer.predict_next("b.rs", 5);

        assert!(from_a.iter().any(|p| p.path == "b.rs"));
        assert!(from_b.iter().any(|p| p.path == "a.rs"));
    }

    #[test]
    fn test_prediction_confidence() {
        let layer = PredictionLayer::new();
        layer.record_co_edit("a.rs", "b.rs");
        layer.record_co_edit("a.rs", "b.rs"); // Twice
        layer.update_file_heat("a.rs", 0.8);

        let conf = layer.prediction_confidence("a.rs");
        assert!(conf > 0.0);
    }

    // ── E12: session_files tests ──────────────────────────────────────────

    #[test]
    fn test_session_files_empty() {
        let layer = PredictionLayer::new();
        assert!(layer.session_files().is_empty());
    }

    #[test]
    fn test_session_files_deduplicates() {
        let layer = PredictionLayer::new();
        layer.record_edit("a.rs");
        layer.record_edit("b.rs");
        layer.record_edit("a.rs"); // duplicate

        let files = layer.session_files();
        assert_eq!(files.len(), 2);
        assert_eq!(files[0], "a.rs");
        assert_eq!(files[1], "b.rs");
    }

    #[test]
    fn test_session_files_preserves_order() {
        let layer = PredictionLayer::new();
        layer.record_edit("c.rs");
        layer.record_edit("a.rs");
        layer.record_edit("b.rs");

        let files = layer.session_files();
        assert_eq!(files, vec!["c.rs", "a.rs", "b.rs"]);
    }
}
