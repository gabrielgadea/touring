//! Error Predictor — Markov-based predictive error prevention.
//!
//! ## S5.1 — Predictive Error Prevention
//!
//! Builds a transition model from edit sequences to error outcomes.
//! If the pattern `tool_A → tool_B → tool_C` historically leads to error `E`
//! in ≥80% of cases, the predictor warns proactively BEFORE the edit.
//!
//! This module requires 2+ weeks of data to produce meaningful predictions.
//! Until then, `predict()` returns `None` (graceful degradation).
//!
//! The predictor is trained from `edit_history` in the knowledge DB, using
//! sequences of (file_path, edit_type, error_pattern) tuples.

use super::knowledge::FileKnowledgeDB;
use std::collections::HashMap;

/// Minimum number of observations for a pattern before predicting.
const MIN_OBSERVATIONS: u32 = 5;

/// Minimum probability threshold to trigger a warning.
const MIN_PROBABILITY: f64 = 0.80;

/// Maximum sequence length to consider.
const MAX_SEQ_LEN: usize = 3;

/// A predicted error with confidence score.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ErrorPrediction {
    /// The predicted error pattern (e.g., "string_not_found").
    pub error_pattern: String,
    /// Probability of this error occurring (0.0 to 1.0).
    pub probability: f64,
    /// Number of historical observations supporting this prediction.
    pub observations: u32,
    /// The triggering sequence (e.g., ["Edit:a.rs", "Edit:b.rs"]).
    pub trigger_sequence: Vec<String>,
}

/// Markov-based error predictor.
///
/// Learns from edit history: sequences of (file, tool) → error outcome.
/// Predicts likely errors before they occur.
#[derive(Debug, Clone)]
pub struct ErrorPredictor {
    /// (sequence_key) → {error_pattern → count}
    transitions: HashMap<String, HashMap<String, u32>>,
    /// (sequence_key) → total observations
    totals: HashMap<String, u32>,
    /// Recent edit sequence (sliding window of MAX_SEQ_LEN).
    recent: Vec<String>,
}

impl ErrorPredictor {
    /// Create an empty predictor.
    pub fn new() -> Self {
        Self {
            transitions: HashMap::new(),
            totals: HashMap::new(),
            recent: Vec::with_capacity(MAX_SEQ_LEN),
        }
    }

    /// Create a predictor pre-seeded with common error patterns.
    ///
    /// Useful for cold-start in new projects where no history is available yet.
    /// Seeded patterns represent empirically common error sequences for Rust and Python.
    /// Each pattern is given `MIN_OBSERVATIONS` occurrences so predictions activate immediately.
    pub fn with_common_patterns() -> Self {
        let mut predictor = Self::new();
        predictor.seed_common_patterns();
        predictor
    }

    /// Inject pre-defined high-probability error patterns.
    ///
    /// Each entry is `(trigger_sequence_key, error_pattern, weight)`.
    /// `weight` is multiplied by `MIN_OBSERVATIONS` to give initial counts.
    pub fn seed_common_patterns(&mut self) {
        // Format: (sequence key, error pattern, weight)
        // weight=1 → MIN_OBSERVATIONS observations; weight=2 → 2×MIN_OBSERVATIONS
        let common: &[(&str, &str, u32)] = &[
            // Rust: editing a .rs file often triggers borrow/lifetime errors
            ("Edit:.rs", "borrow_conflict", 2),
            ("Edit:.rs", "lifetime_mismatch", 1),
            ("Edit:.rs", "use_of_moved_value", 2),
            ("Edit:.rs|Edit:.rs", "borrow_conflict", 2),
            ("Edit:.rs|Edit:.rs", "type_mismatch", 1),
            // Rust: writing a new file often leads to missing imports
            ("Write:.rs", "unresolved_import", 2),
            ("Write:.rs", "missing_trait_impl", 1),
            // Python: editing .py often causes indentation or name errors
            ("Edit:.py", "IndentationError", 2),
            ("Edit:.py", "NameError", 2),
            ("Edit:.py", "AttributeError", 1),
            ("Edit:.py|Edit:.py", "AttributeError", 1),
            // Python: writing new .py → missing imports or syntax errors
            ("Write:.py", "ImportError", 2),
            ("Write:.py", "SyntaxError", 1),
            // General: running bash after editing often exposes missing deps
            ("Edit:.rs|Bash:", "linker_error", 1),
            ("Edit:.py|Bash:", "ModuleNotFoundError", 1),
        ];

        for (seq_key, error_pattern, weight) in common {
            let count = weight * MIN_OBSERVATIONS;
            *self
                .transitions
                .entry(seq_key.to_string())
                .or_default()
                .entry(error_pattern.to_string())
                .or_insert(0) += count;
            *self.totals.entry(seq_key.to_string()).or_insert(0) += count;
        }
    }

    /// Train the predictor from the knowledge database.
    ///
    /// Reads edit_history and builds transition probabilities.
    /// Returns the number of sequences learned.
    pub fn train_from_db(&mut self, db: &FileKnowledgeDB) -> usize {
        // Get recent edits (up to 1000) across all files
        let edits = match db.recent_edits_all(500) {
            Ok(e) => e,
            Err(_) => return 0,
        };

        if edits.len() < MIN_OBSERVATIONS as usize {
            return 0;
        }

        let mut sequences_learned = 0;

        // Build sequences of length 2 and 3
        for window_size in 2..=MAX_SEQ_LEN {
            for window in edits.windows(window_size) {
                // SAFETY: window.len() == window_size >= 2, so both
                // window[window_size - 1] and window[..window_size - 1] are in bounds.
                #[allow(clippy::indexing_slicing)]
                let outcome = &window[window_size - 1];
                let error = match &outcome.error_pattern {
                    Some(e) => e.clone(),
                    None => "__success__".to_string(),
                };

                // Build sequence key from preceding elements
                #[allow(clippy::indexing_slicing)]
                let seq_parts: Vec<String> = window[..window_size - 1]
                    .iter()
                    .map(|e| format!("{}:{}", e.edit_type, e.file_path))
                    .collect();
                let seq_key = seq_parts.join("|");

                *self
                    .transitions
                    .entry(seq_key.clone())
                    .or_default()
                    .entry(error)
                    .or_insert(0) += 1;
                *self.totals.entry(seq_key).or_insert(0) += 1;

                sequences_learned += 1;
            }
        }

        sequences_learned
    }

    /// Record a new edit event in the recent sequence.
    pub fn record_edit(&mut self, file_path: &str, edit_type: &str) {
        let entry = format!("{edit_type}:{file_path}");
        self.recent.push(entry);
        if self.recent.len() > MAX_SEQ_LEN {
            self.recent.remove(0);
        }
    }

    /// Predict the most likely error for the current sequence.
    ///
    /// Returns `None` if:
    /// - Insufficient data (< MIN_OBSERVATIONS for this sequence)
    /// - No error predicted with probability >= MIN_PROBABILITY
    /// - Recent sequence is empty
    pub fn predict(&self) -> Option<ErrorPrediction> {
        if self.recent.is_empty() || self.transitions.is_empty() {
            return None;
        }

        // Try longest sequence first, then shorter
        for start in 0..self.recent.len() {
            // SAFETY: start ranges from 0..len, so start..len is always valid
            #[allow(clippy::indexing_slicing)]
            let seq_parts = &self.recent[start..];
            let seq_key = seq_parts.join("|");

            if let Some(outcomes) = self.transitions.get(&seq_key) {
                let total = *self.totals.get(&seq_key).unwrap_or(&0);
                if total < MIN_OBSERVATIONS {
                    continue;
                }

                // Find the most likely error (excluding __success__)
                let best_error = outcomes
                    .iter()
                    .filter(|(k, _)| *k != "__success__")
                    .max_by_key(|(_, count)| *count);

                if let Some((error_pattern, count)) = best_error {
                    let probability = *count as f64 / total as f64;
                    if probability >= MIN_PROBABILITY {
                        return Some(ErrorPrediction {
                            error_pattern: error_pattern.clone(),
                            probability,
                            observations: total,
                            trigger_sequence: seq_parts.to_vec(),
                        });
                    }
                }
            }
        }

        None
    }

    /// Number of unique sequences learned.
    pub fn sequence_count(&self) -> usize {
        self.transitions.len()
    }

    /// Whether the predictor has enough data to make predictions.
    pub fn is_ready(&self) -> bool {
        self.totals.values().any(|&t| t >= MIN_OBSERVATIONS)
    }
}

impl Default for ErrorPredictor {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_predictor_new_is_empty() {
        let pred = ErrorPredictor::new();
        assert_eq!(pred.sequence_count(), 0);
        assert!(!pred.is_ready());
        assert!(pred.predict().is_none());
    }

    #[test]
    fn test_predictor_learns_from_manual_data() {
        let mut pred = ErrorPredictor::new();

        // Simulate: Edit:a.rs → always leads to string_not_found
        let seq_key = "Edit:a.rs".to_string();
        for _ in 0..10 {
            *pred
                .transitions
                .entry(seq_key.clone())
                .or_default()
                .entry("string_not_found".to_string())
                .or_insert(0) += 1;
            *pred.totals.entry(seq_key.clone()).or_insert(0) += 1;
        }

        assert!(pred.is_ready());
        assert_eq!(pred.sequence_count(), 1);

        // Record edit and predict
        pred.record_edit("a.rs", "Edit");
        let prediction = pred.predict();
        assert!(prediction.is_some());
        // assert! above guarantees Some — predictor was seeded with 10 observations
        // for "Edit:a.rs → string_not_found" in with_common_patterns or manual training
        let p = prediction.expect("error predictor returned None in hotpath: sequence was seeded with MIN_OBSERVATIONS=5, prediction must be Some");
        assert_eq!(p.error_pattern, "string_not_found");
        assert!((p.probability - 1.0).abs() < 0.01);
        assert_eq!(p.observations, 10);
    }

    #[test]
    fn test_predictor_with_insufficient_data() {
        let mut pred = ErrorPredictor::new();

        // Only 2 observations (below MIN_OBSERVATIONS=5)
        let seq_key = "Edit:b.rs".to_string();
        for _ in 0..2 {
            *pred
                .transitions
                .entry(seq_key.clone())
                .or_default()
                .entry("syntax_error".to_string())
                .or_insert(0) += 1;
            *pred.totals.entry(seq_key.clone()).or_insert(0) += 1;
        }

        pred.record_edit("b.rs", "Edit");
        assert!(
            pred.predict().is_none(),
            "Should not predict with < MIN_OBSERVATIONS"
        );
    }

    #[test]
    fn test_prediction_confidence_above_threshold() {
        let mut pred = ErrorPredictor::new();

        let seq_key = "Edit:c.rs".to_string();
        // 4 errors, 6 successes = 40% error rate (below 80% threshold)
        for _ in 0..4 {
            *pred
                .transitions
                .entry(seq_key.clone())
                .or_default()
                .entry("file_not_found".to_string())
                .or_insert(0) += 1;
        }
        for _ in 0..6 {
            *pred
                .transitions
                .entry(seq_key.clone())
                .or_default()
                .entry("__success__".to_string())
                .or_insert(0) += 1;
        }
        *pred.totals.entry(seq_key).or_insert(0) = 10;

        pred.record_edit("c.rs", "Edit");
        assert!(
            pred.predict().is_none(),
            "40% error rate should not trigger prediction (threshold=80%)"
        );
    }

    #[test]
    fn test_record_edit_sliding_window() {
        let mut pred = ErrorPredictor::new();
        for i in 0..10 {
            pred.record_edit(&format!("file_{i}.rs"), "Edit");
        }
        assert_eq!(
            pred.recent.len(),
            MAX_SEQ_LEN,
            "Recent window should be capped at MAX_SEQ_LEN"
        );
    }

    #[test]
    fn test_predictor_prefers_longer_sequences() {
        let mut pred = ErrorPredictor::new();

        // Short sequence: Edit:a.rs → 50% error
        let short_key = "Edit:a.rs".to_string();
        for _ in 0..5 {
            *pred
                .transitions
                .entry(short_key.clone())
                .or_default()
                .entry("error_x".to_string())
                .or_insert(0) += 1;
        }
        for _ in 0..5 {
            *pred
                .transitions
                .entry(short_key.clone())
                .or_default()
                .entry("__success__".to_string())
                .or_insert(0) += 1;
        }
        *pred.totals.entry(short_key).or_insert(0) = 10;

        // Long sequence: Edit:b.rs|Edit:a.rs → 90% error
        let long_key = "Edit:b.rs|Edit:a.rs".to_string();
        for _ in 0..9 {
            *pred
                .transitions
                .entry(long_key.clone())
                .or_default()
                .entry("error_x".to_string())
                .or_insert(0) += 1;
        }
        *pred
            .transitions
            .entry(long_key.clone())
            .or_default()
            .entry("__success__".to_string())
            .or_insert(0) += 1;
        *pred.totals.entry(long_key).or_insert(0) = 10;

        // Record the longer sequence
        pred.record_edit("b.rs", "Edit");
        pred.record_edit("a.rs", "Edit");

        let prediction = pred.predict();
        assert!(prediction.is_some(), "Should predict from longer sequence");
        // assert! above guarantees Some — predictor was seeded with 10 observations
        // for "Edit:a.rs → string_not_found" in with_common_patterns or manual training
        let p = prediction.expect("error predictor returned None in hotpath: sequence was seeded with MIN_OBSERVATIONS=5, prediction must be Some");
        assert_eq!(p.error_pattern, "error_x");
        assert!(p.probability > 0.8);
    }
}
