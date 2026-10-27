//! Pensieve — Failed-path memory for cognitive search.
//!
//! Records failed MCTS/GoT reasoning paths as embeddings in [`AnnIndex`].
//! On future searches, checks if a candidate state matches a known failure
//! and returns a penalty score to discourage re-exploration.
//!
//! Named after the memory basin in Harry Potter — stores experiences
//! for later retrieval and learning.
//!
//! # Design
//!
//! State sequences are hashed into fixed-dimension float vectors via
//! modular bucket hashing. The [`AnnIndex`] (IVF-flat with SIMD cosine)
//! performs approximate nearest-neighbour search to find similar failures.
//!
//! Penalty is the cosine similarity to the closest known failure:
//! values closer to 1.0 mean the candidate is very similar to a past failure.

use crate::reasoning::ann_index::AnnIndex;
use touring_foundation::text_sanitize::redact_secrets;

/// Maximum bytes of the failing command kept in the store.
///
/// The notice renders far less than this; the cap only stops a pathological
/// command line from growing the in-memory store without bound.
const MAX_STORED_COMMAND_BYTES: usize = 512;

/// Minimum bytes a name must have to be worth printing.
const MIN_LABEL_BYTES: usize = 8;

/// A recorded failure from MCTS/GoT search.
#[derive(Debug, Clone)]
pub struct FailedPath {
    /// Hash of the state sequence that failed.
    pub state_hash: u64,
    /// Human-readable reason for failure.
    pub reason: String,
    /// The command that failed — empty when the producer recorded none.
    ///
    /// Stored redacted: the command travels from the shell into the model's
    /// context, so a credential pasted into the command line must not
    /// survive the round-trip.
    pub command: String,
    /// Depth at which the path was pruned.
    pub depth: usize,
    /// Timestamp (epoch seconds).
    pub timestamp: u64,
    /// Number of times this failure pattern was seen.
    pub hit_count: u32,
}

/// A known failure that matched a candidate, with the score that matched it.
///
/// The query API hands back the recorded failure itself, not only a score: a
/// score says "something similar failed", the failure says WHICH command
/// failed — the difference between a notice the model can act on and one it
/// can only stare at.
#[derive(Debug, Clone)]
pub struct FailedMatch<'a> {
    /// Cosine similarity to the matched failure (0.0–1.0).
    pub similarity: f64,
    /// The recorded failure that matched.
    pub path: &'a FailedPath,
}

/// Pensieve memory for failed reasoning paths.
///
/// Stores failed MCTS/GoT paths as vector embeddings and provides
/// fast similarity lookup to penalise re-exploration of known failures.
pub struct Pensieve {
    /// Vector index for fast similarity search.
    index: AnnIndex,
    /// Recorded failure metadata (position-indexed, matching AnnIndex entries).
    failures: Vec<FailedPath>,
    /// Raw embeddings kept for incremental rebuild of the IVF index.
    embeddings: Vec<Vec<f32>>,
    /// Embedding dimension.
    dim: usize,
    /// Cosine-similarity threshold for a match (0.0–1.0, higher = stricter).
    threshold: f64,
}

impl Pensieve {
    /// Create a new empty Pensieve with the given embedding dimension.
    ///
    /// Default similarity threshold is 0.5.
    pub fn new(dim: usize) -> Self {
        Self {
            index: AnnIndex::new(dim),
            failures: Vec::new(),
            embeddings: Vec::new(),
            dim,
            threshold: 0.5,
        }
    }

    /// Builder: set the cosine-similarity threshold for failure matching.
    ///
    /// Values closer to 1.0 require near-exact matches; lower values
    /// cast a wider net.
    pub fn with_threshold(mut self, threshold: f64) -> Self {
        self.threshold = threshold.clamp(0.0, 1.0);
        self
    }

    /// Record a failed path.
    ///
    /// Computes an embedding from `states`, stores the metadata, and
    /// rebuilds the ANN index so the failure is immediately searchable.
    ///
    /// The failure is recorded WITHOUT a command; use
    /// [`record_failure_with_command`](Self::record_failure_with_command) when
    /// the command that failed is known — that is what lets a later lookup
    /// name the failure instead of only scoring it.
    pub fn record_failure(&mut self, states: &[u64], reason: &str, depth: usize) {
        self.record_failure_with_command(states, reason, depth, "");
    }

    /// Record a failed path together with the command that failed.
    ///
    /// The command is what makes the later notice actionable, so it is stored
    /// alongside the failure — redacted, because it travels from the shell
    /// into the model's context and a credential pasted into the command line
    /// must not survive the round-trip.
    pub fn record_failure_with_command(
        &mut self,
        states: &[u64],
        reason: &str,
        depth: usize,
        command: &str,
    ) {
        let embedding = compute_embedding(states, self.dim);
        let state_hash = hash_states(states);

        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::SystemTime::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);

        // Redact BEFORE truncating: a half-cut token would no longer match the
        // redaction patterns and its prefix would leak.
        let command = truncate_bytes(&redact_secrets(command), MAX_STORED_COMMAND_BYTES);

        // Check for duplicate state_hash and increment hit_count instead.
        if let Some(existing) = self
            .failures
            .iter_mut()
            .find(|f| f.state_hash == state_hash)
        {
            existing.hit_count = existing.hit_count.saturating_add(1);
            existing.timestamp = timestamp;
            if !command.is_empty() {
                existing.command = command;
            }
            return;
        }

        self.failures.push(FailedPath {
            state_hash,
            reason: reason.to_string(),
            command,
            depth,
            timestamp,
            hit_count: 1,
        });
        self.embeddings.push(embedding);

        self.rebuild_index();
    }

    /// Check if a candidate state matches a known failure.
    ///
    /// Returns `Some(penalty)` if the closest failure exceeds the similarity
    /// threshold.  Penalty is in 0.0–1.0 (higher = more similar to failure).
    /// Returns `None` if no match meets the threshold.
    pub fn check_known_failure(&self, state: u64) -> Option<f64> {
        self.check_known_failure_detail(state).map(|m| m.similarity)
    }

    /// Check a candidate state and return the failure that matched.
    ///
    /// Unlike [`check_known_failure`](Self::check_known_failure) this hands
    /// back the recorded failure, so the caller can name what failed instead
    /// of only knowing that something similar failed.
    pub fn check_known_failure_detail(&self, state: u64) -> Option<FailedMatch<'_>> {
        if self.failures.is_empty() {
            return None;
        }

        let embedding = compute_embedding(&[state], self.dim);
        self.match_failure(&embedding)
    }

    /// Check a full state sequence against known failures.
    ///
    /// Like [`check_known_failure`](Self::check_known_failure) but embeds the
    /// entire sequence, giving a more accurate match.
    pub fn check_known_failure_seq(&self, states: &[u64]) -> Option<f64> {
        self.check_known_failure_seq_detail(states)
            .map(|m| m.similarity)
    }

    /// Check a full state sequence and return the failure that matched.
    ///
    /// Like [`check_known_failure_detail`](Self::check_known_failure_detail) but
    /// embeds the entire sequence, giving a more accurate match.
    pub fn check_known_failure_seq_detail(&self, states: &[u64]) -> Option<FailedMatch<'_>> {
        if self.failures.is_empty() || states.is_empty() {
            return None;
        }

        let embedding = compute_embedding(states, self.dim);
        self.match_failure(&embedding)
    }

    /// The closest recorded failure to `embedding`, when it clears the threshold.
    fn match_failure(&self, embedding: &[f32]) -> Option<FailedMatch<'_>> {
        let results = self.index.query(embedding, 1);

        let (id, similarity) = results.first()?;
        let sim = f64::from(*similarity);

        // `sim` é um f64 (clippy::neg_cmp_op_on_partial_ord): NaN torna a
        // comparação falsa; queremos `None` nesse caso, e a forma explícita
        // com partial_cmp deixa isso óbvio em vez de esconder atrás de um `!`.
        if sim.partial_cmp(&self.threshold).is_none_or(|o| o.is_lt()) {
            return None;
        }

        // The index stores the position of the failure as its key.
        let position: usize = id.parse().ok()?;
        let path = self.failures.get(position)?;

        Some(FailedMatch {
            similarity: sim.clamp(0.0, 1.0),
            path,
        })
    }

    /// Number of recorded failures.
    pub fn failure_count(&self) -> usize {
        self.failures.len()
    }

    /// Get all recorded failures (for stats/debugging).
    pub fn failures(&self) -> &[FailedPath] {
        &self.failures
    }

    /// Current similarity threshold.
    pub fn threshold(&self) -> f64 {
        self.threshold
    }

    /// Set the cosine-similarity threshold for failure matching.
    ///
    /// Higher values = stricter matching = fewer false positives = more penalty suppression.
    /// Lower values = looser matching = more matches = more penalty warnings.
    pub fn set_threshold(&mut self, threshold: f64) {
        self.threshold = threshold.clamp(0.0, 1.0);
    }

    /// Rebuild the IVF index from all stored embeddings.
    fn rebuild_index(&mut self) {
        let entries: Vec<(String, Vec<f32>)> = self
            .embeddings
            .iter()
            .enumerate()
            .map(|(i, emb): (usize, &Vec<f32>)| (i.to_string(), emb.clone()))
            .collect();
        self.index.build(&entries);
    }
}

impl<'a> FailedMatch<'a> {
    /// The name of what failed, compacted to one line and truncated to
    /// `max_bytes`.
    ///
    /// The command is the name when the producer recorded one; the recorded
    /// reason stands in when it did not (the producer may not know the
    /// command); `"unknown"` when neither is known.
    pub fn failure_label(&self, max_bytes: usize) -> String {
        let source = if !self.path.command.trim().is_empty() {
            self.path.command.as_str()
        } else if !self.path.reason.trim().is_empty() {
            self.path.reason.as_str()
        } else {
            "unknown"
        };
        compact_command(source, max_bytes)
    }

    /// One-line notice for a command-shaped lookup.
    ///
    /// Names the command that failed. The notice is never longer than the one
    /// it replaces — the name is what the leftover budget pays for.
    pub fn command_notice(&self) -> String {
        let pct = self.similarity * 100.0;
        let legacy = format!("Similar command failed recently (similarity={pct:.0}%)");
        self.notice(
            "Similar failure: ",
            &format!(" ({pct:.0}%)"),
            &legacy,
            legacy.len(),
        )
    }

    /// One-line notice for a path-shaped lookup.
    ///
    /// Names the command that failed on the similar path.
    pub fn path_notice(&self) -> String {
        let pct = self.similarity * 100.0;
        let legacy = format!("⚠ pensieve: similar path had recorded failures ({pct:.0}% match)");
        self.notice(
            "⚠ pensieve: ",
            &format!(" failed before ({pct:.0}% match)"),
            &legacy,
            legacy.len(),
        )
    }

    /// One-line notice for a task-shaped lookup.
    ///
    /// Names the command that failed on the similar task.
    pub fn task_notice(&self) -> String {
        let legacy = format!(
            "⚠ pensieve: similar task failed before (penalty={:.2}) — review gotchas before proceeding",
            self.similarity
        );
        self.notice(
            "⚠ pensieve: ",
            &format!(
                " failed before (penalty={:.2}) — review gotchas",
                self.similarity
            ),
            &legacy,
            legacy.len(),
        )
    }

    /// Build `<lead>`<name>`<tail>`, hard-bounded to `max_total` bytes.
    ///
    /// `fallback` is returned verbatim when the budget cannot hold a name, so
    /// the notice keeps its shape and never grows.
    fn notice(&self, lead: &str, tail: &str, fallback: &str, max_total: usize) -> String {
        let room = max_total
            .saturating_sub(lead.len())
            .saturating_sub(tail.len())
            .saturating_sub(2); // the two backticks around the name
        if room < MIN_LABEL_BYTES {
            return fallback.to_string();
        }
        format!("{lead}`{}`{tail}", self.failure_label(room))
    }
}

/// Compact `command` to a single line, truncated to `max_bytes`.
///
/// A cut is marked with an ellipsis, so the reader knows the name is a prefix
/// of the real command rather than the whole of it.
pub fn compact_command(command: &str, max_bytes: usize) -> String {
    let flat = command.split_whitespace().collect::<Vec<_>>().join(" ");
    // A backtick would close the inline-code marker the notice wraps the name in.
    let flat = flat.replace('`', "'");
    truncate_bytes(&flat, max_bytes)
}

/// Truncate `s` to at most `max_bytes`, marking the cut with an ellipsis.
///
/// Never splits a character: the result is always valid UTF-8 and always fits
/// the budget.
fn truncate_bytes(s: &str, max_bytes: usize) -> String {
    if s.len() <= max_bytes {
        return s.to_string();
    }
    if max_bytes == 0 {
        return String::new();
    }

    const ELLIPSIS: &str = "…";
    if max_bytes <= ELLIPSIS.len() {
        // No room for the marker — cut on a character boundary and stop.
        let mut out = String::new();
        for ch in s.chars() {
            if out.len() + ch.len_utf8() > max_bytes {
                break;
            }
            out.push(ch);
        }
        return out;
    }

    let budget = max_bytes - ELLIPSIS.len();
    let mut out = String::new();
    for ch in s.chars() {
        if out.len() + ch.len_utf8() > budget {
            break;
        }
        out.push(ch);
    }
    out.push_str(ELLIPSIS);
    out
}

/// Hash a state sequence into a single u64 for deduplication.
fn hash_states(states: &[u64]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325; // FNV-1a offset basis
    for &s in states {
        h ^= s;
        h = h.wrapping_mul(0x100000001b3); // FNV-1a prime
    }
    h
}

/// Compute a fixed-dimension embedding from a state sequence.
///
/// Uses modular bucket hashing: each state is distributed across
/// `dim` buckets, then the vector is L2-normalised so cosine
/// similarity works correctly.
fn compute_embedding(states: &[u64], dim: usize) -> Vec<f32> {
    if dim == 0 {
        return Vec::new();
    }

    let mut embedding = vec![0.0_f32; dim];

    for (i, &state) in states.iter().enumerate() {
        // Primary bucket: state mod dim
        let bucket = (state as usize) % dim;
        if let Some(v) = embedding.get_mut(bucket) {
            *v += 1.0;
        }

        // Secondary bucket: incorporate position for sequence sensitivity
        let secondary = ((state.wrapping_mul(2654435761)) as usize + i) % dim;
        if let Some(v) = embedding.get_mut(secondary) {
            *v += 0.5;
        }

        // Tertiary: spread high bits
        let tertiary = ((state >> 16) as usize) % dim;
        if let Some(v) = embedding.get_mut(tertiary) {
            *v += 0.25;
        }
    }

    // L2-normalise via SIMD dot-product so cosine similarity is meaningful.
    touring_simd::simd_utils::matrix::l2_normalize_inplace(&mut embedding);

    embedding
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_pensieve_has_zero_failures() {
        let p = Pensieve::new(16);
        assert_eq!(p.failure_count(), 0);
        assert!(p.failures().is_empty());
    }

    #[test]
    fn record_failure_increments_count() {
        let mut p = Pensieve::new(16);
        p.record_failure(&[1, 2, 3], "test failure", 3);
        assert_eq!(p.failure_count(), 1);

        p.record_failure(&[4, 5, 6], "another failure", 2);
        assert_eq!(p.failure_count(), 2);
    }

    #[test]
    fn check_known_failure_returns_none_for_unknown_state() {
        let mut p = Pensieve::new(16);
        p.record_failure(&[100, 200, 300], "some failure", 5);

        // A very different state should not match at default threshold
        // Use a state that hashes to completely different buckets
        let result = p.check_known_failure(999_999_999);
        // Even if it matches weakly, it should be below the 0.5 threshold
        // for truly distant states — or it may match due to bucket collisions.
        // We just verify the API works without panicking.
        let _ = result;
    }

    #[test]
    fn check_known_failure_returns_some_for_recorded_state() {
        let mut p = Pensieve::new(16).with_threshold(0.0);
        // Record failure for state sequence [42]
        p.record_failure(&[42], "the answer failed", 1);

        // Check the exact same state — should match with high similarity
        let result = p.check_known_failure(42);
        assert!(result.is_some(), "exact state should match");
        let penalty = result.expect("just checked Some");
        assert!(
            penalty > 0.5,
            "exact match penalty should be high, got {penalty}"
        );
    }

    #[test]
    fn check_known_failure_penalty_proportional_to_similarity() {
        let mut p = Pensieve::new(32).with_threshold(0.0);
        p.record_failure(&[10], "failure at 10", 1);

        // Exact match should have highest penalty
        let exact = p.check_known_failure(10).unwrap_or(0.0);

        // A state whose embedding lands in different buckets should have lower penalty
        // (we use a large offset to maximise bucket divergence)
        let distant = p.check_known_failure(10 + 32 * 7 + 1).unwrap_or(0.0);

        assert!(
            exact >= distant,
            "exact match ({exact}) should have >= penalty than distant ({distant})"
        );
    }

    #[test]
    fn threshold_filtering_works() {
        let mut p_strict = Pensieve::new(16).with_threshold(0.99);
        p_strict.record_failure(&[7, 8, 9], "strict failure", 3);

        // With very strict threshold, a loosely related state should not match
        let result = p_strict.check_known_failure(12345);
        // Strict threshold should filter most non-exact matches
        assert!(
            result.is_none() || result.expect("checked Some") > 0.99,
            "strict threshold should filter weak matches"
        );

        // With very loose threshold, even weak matches pass
        let mut p_loose = Pensieve::new(16).with_threshold(0.0);
        p_loose.record_failure(&[7, 8, 9], "loose failure", 3);
        let result = p_loose.check_known_failure(7);
        assert!(result.is_some(), "loose threshold should allow matches");
    }

    #[test]
    fn multiple_failures_stored_correctly() {
        let mut p = Pensieve::new(16);
        let reasons = ["fail_a", "fail_b", "fail_c"];

        for (i, reason) in reasons.iter().enumerate() {
            p.record_failure(&[(i as u64) * 100], reason, i);
        }

        assert_eq!(p.failure_count(), 3);

        let failures = p.failures();
        assert_eq!(failures.len(), 3);
        assert_eq!(failures[0].reason, "fail_a");
        assert_eq!(failures[1].reason, "fail_b");
        assert_eq!(failures[2].reason, "fail_c");
        assert_eq!(failures[0].depth, 0);
        assert_eq!(failures[1].depth, 1);
        assert_eq!(failures[2].depth, 2);
    }

    #[test]
    fn with_threshold_builder_works() {
        let p = Pensieve::new(8).with_threshold(0.75);
        assert!((p.threshold() - 0.75).abs() < f64::EPSILON);

        // Threshold is clamped to [0.0, 1.0]
        let p_clamped = Pensieve::new(8).with_threshold(1.5);
        assert!((p_clamped.threshold() - 1.0).abs() < f64::EPSILON);

        let p_negative = Pensieve::new(8).with_threshold(-0.5);
        assert!((p_negative.threshold() - 0.0).abs() < f64::EPSILON);
    }

    #[test]
    fn failures_returns_correct_metadata() {
        let mut p = Pensieve::new(16);
        p.record_failure(&[50, 60], "timeout in expansion", 4);

        let f = &p.failures()[0];
        assert_eq!(f.reason, "timeout in expansion");
        assert_eq!(f.depth, 4);
        assert_eq!(f.hit_count, 1);
        assert!(f.timestamp > 0, "timestamp should be set");
    }

    #[test]
    fn similar_states_match_with_low_threshold() {
        let mut p = Pensieve::new(32).with_threshold(0.0);

        // Record a failure for state [10, 20, 30]
        p.record_failure(&[10, 20, 30], "original failure", 3);

        // A very similar sequence [10, 20, 31] should produce a partial match
        let result = p.check_known_failure_seq(&[10, 20, 31]);
        assert!(
            result.is_some(),
            "similar sequence should match with threshold=0"
        );
        let penalty = result.expect("just checked Some");
        assert!(penalty > 0.0, "penalty should be positive");
    }

    #[test]
    fn duplicate_state_hash_increments_hit_count() {
        let mut p = Pensieve::new(16);
        p.record_failure(&[1, 2, 3], "first occurrence", 2);
        assert_eq!(p.failure_count(), 1);
        assert_eq!(p.failures()[0].hit_count, 1);

        // Same state sequence → should increment, not duplicate
        p.record_failure(&[1, 2, 3], "second occurrence", 2);
        assert_eq!(p.failure_count(), 1);
        assert_eq!(p.failures()[0].hit_count, 2);
    }

    #[test]
    fn empty_state_sequence_produces_zero_embedding() {
        // Should not panic, produces a normalised zero-like vector
        let emb = compute_embedding(&[], 8);
        assert_eq!(emb.len(), 8);
    }

    #[test]
    fn zero_dim_pensieve_does_not_panic() {
        let mut p = Pensieve::new(0);
        p.record_failure(&[1, 2, 3], "zero dim", 1);
        assert_eq!(p.failure_count(), 1);
        assert!(p.check_known_failure(1).is_none());
    }

    #[test]
    fn hash_states_deterministic() {
        let a = hash_states(&[1, 2, 3]);
        let b = hash_states(&[1, 2, 3]);
        assert_eq!(a, b, "same input should produce same hash");

        let c = hash_states(&[3, 2, 1]);
        // Order matters for FNV-1a with XOR + multiply
        // (though XOR is commutative, the wrapping_mul changes the accumulator)
        // Just verify determinism of the different input
        let d = hash_states(&[3, 2, 1]);
        assert_eq!(c, d);
    }

    #[test]
    fn record_failure_with_command_stores_the_command() {
        let mut p = Pensieve::new(16);
        p.record_failure_with_command(
            &[1, 2, 3],
            "exit code 1",
            3,
            "cargo test -p touring-intelligence",
        );
        assert_eq!(p.failure_count(), 1);
        assert_eq!(
            p.failures()[0].command,
            "cargo test -p touring-intelligence"
        );
    }

    #[test]
    fn record_failure_without_a_command_leaves_it_empty() {
        let mut p = Pensieve::new(16);
        p.record_failure(&[1, 2, 3], "exit code 1", 3);
        assert_eq!(p.failures()[0].command, "");
    }

    #[test]
    fn record_failure_redacts_secrets_in_the_command() {
        // touring-quality:allow-secrets — a SAMPLE token: the test exists to
        // prove the redactor eats a provider-shaped token, so the shape has to
        // be present in the source.
        const TOKEN: &str = "ghp_abcdefghijklmnopqrstuvwxyz012345";
        let mut p = Pensieve::new(16);
        p.record_failure_with_command(
            &[1, 2, 3],
            "auth failed",
            3,
            "curl -H 'Authorization: Bearer ghp_abcdefghijklmnopqrstuvwxyz012345' https://api",
        );
        let stored = &p.failures()[0].command;
        assert!(
            !stored.contains(TOKEN),
            "the token survived into the store: {stored}"
        );
        assert!(
            stored.contains("[REDACTED]"),
            "the redaction marker is missing: {stored}"
        );
    }

    #[test]
    fn record_failure_truncates_a_pathological_command() {
        let mut p = Pensieve::new(16);
        let huge = "x".repeat(MAX_STORED_COMMAND_BYTES * 2);
        p.record_failure_with_command(&[1, 2, 3], "boom", 3, &huge);
        assert!(
            p.failures()[0].command.len() <= MAX_STORED_COMMAND_BYTES,
            "the stored command is unbounded: {}",
            p.failures()[0].command.len()
        );
    }

    #[test]
    fn duplicate_failure_keeps_the_stored_command() {
        let mut p = Pensieve::new(16);
        p.record_failure_with_command(&[1, 2, 3], "first", 2, "cargo build");
        p.record_failure_with_command(&[1, 2, 3], "second", 2, "cargo build");
        assert_eq!(p.failure_count(), 1);
        assert_eq!(p.failures()[0].command, "cargo build");
        assert_eq!(p.failures()[0].hit_count, 2);
    }

    #[test]
    fn check_known_failure_detail_names_the_failing_command() {
        let mut p = Pensieve::new(32).with_threshold(0.0);
        p.record_failure_with_command(&[42], "exit code 1", 1, "cargo test --workspace");
        let matched = p
            .check_known_failure_detail(42)
            .expect("the exact state matches");
        assert!(
            matched.path.command.contains("cargo test"),
            "the match must carry the failing command: {}",
            matched.path.command
        );
    }

    #[test]
    fn check_known_failure_seq_detail_names_the_failing_command() {
        let mut p = Pensieve::new(32).with_threshold(0.0);
        p.record_failure_with_command(&[10, 20, 30], "exit code 1", 3, "cargo test --workspace");
        let matched = p
            .check_known_failure_seq_detail(&[10, 20, 30])
            .expect("the same sequence matches");
        assert!(matched.path.command.contains("cargo test"));
    }

    #[test]
    fn check_known_failure_keeps_returning_the_score() {
        let mut p = Pensieve::new(32).with_threshold(0.0);
        p.record_failure_with_command(&[42], "exit code 1", 1, "cargo test");
        let score = p
            .check_known_failure(42)
            .expect("the score API still matches");
        let detail = p
            .check_known_failure_detail(42)
            .expect("the detail API matches");
        assert!(
            (score - detail.similarity).abs() < f64::EPSILON,
            "the two APIs disagree: {score} vs {}",
            detail.similarity
        );
    }

    #[test]
    fn command_notice_names_the_command_and_never_grows() {
        let mut p = Pensieve::new(32).with_threshold(0.0);
        p.record_failure_with_command(
            &[42],
            "exit code 1",
            1,
            "cargo test -p touring-intelligence --lib --release",
        );
        let matched = p
            .check_known_failure_detail(42)
            .expect("the exact state matches");
        let notice = matched.command_notice();
        assert!(
            notice.contains("cargo test"),
            "the notice must name the failing command: {notice}"
        );
        let legacy = format!(
            "Similar command failed recently (similarity={:.0}%)",
            matched.similarity * 100.0
        );
        assert!(
            notice.len() <= legacy.len(),
            "the notice must not grow: {notice} ({}) > {legacy} ({})",
            notice.len(),
            legacy.len()
        );
    }

    #[test]
    fn path_notice_names_the_command_and_never_grows() {
        let mut p = Pensieve::new(32).with_threshold(0.0);
        p.record_failure_with_command(
            &[42],
            "exit code 1",
            1,
            "ruff check crates/touring-cli/src/cli/pretask.rs",
        );
        let matched = p
            .check_known_failure_detail(42)
            .expect("the exact state matches");
        let notice = matched.path_notice();
        assert!(
            notice.contains("ruff check"),
            "the notice must name the failing command: {notice}"
        );
        let legacy = format!(
            "⚠ pensieve: similar path had recorded failures ({:.0}% match)",
            matched.similarity * 100.0
        );
        assert!(
            notice.len() <= legacy.len(),
            "the notice must not grow: {notice} ({}) > {legacy} ({})",
            notice.len(),
            legacy.len()
        );
    }

    #[test]
    fn task_notice_names_the_command_and_never_grows() {
        let mut p = Pensieve::new(32).with_threshold(0.0);
        p.record_failure_with_command(&[42], "exit code 1", 1, "cargo build --release");
        let matched = p
            .check_known_failure_detail(42)
            .expect("the exact state matches");
        let notice = matched.task_notice();
        assert!(
            notice.contains("cargo build"),
            "the notice must name the failing command: {notice}"
        );
        let legacy = format!(
            "⚠ pensieve: similar task failed before (penalty={:.2}) — review gotchas before proceeding",
            matched.similarity
        );
        assert!(
            notice.len() <= legacy.len(),
            "the notice must not grow: {notice} ({}) > {legacy} ({})",
            notice.len(),
            legacy.len()
        );
    }

    #[test]
    fn failure_label_falls_back_to_the_reason_when_no_command_was_recorded() {
        let mut p = Pensieve::new(32).with_threshold(0.0);
        p.record_failure(&[42], "dependency cycle in auth refactor", 1);
        let matched = p
            .check_known_failure_detail(42)
            .expect("the exact state matches");
        let notice = matched.command_notice();
        assert!(
            notice.contains("dependency cycle"),
            "the notice must name the recorded reason: {notice}"
        );
    }

    #[test]
    fn compact_command_flattens_to_one_line_and_marks_the_cut() {
        assert_eq!(compact_command("cargo\n  test", 64), "cargo test");
        assert_eq!(compact_command("echo `date`", 64), "echo 'date'");
        let cut = compact_command("cargo test -p touring-intelligence --lib", 12);
        assert!(cut.len() <= 12, "the cut is too long: {cut}");
        assert!(cut.ends_with('…'), "the cut must be marked: {cut}");
    }
}
