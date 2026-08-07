//! **S-11 / R10 — the learned action-outcome model.**
//!
//! [`ExecutionOutcomePredictor`] (X4 PREDICT) is a Laplace-Beta point estimate
//! keyed by the *exact* [`ActionSignature`]: an unseen signature predicts the
//! bare prior `0.5`, even when thousands of *similar* actions (same tool + intent
//! class) have a strong empirical success rate. The 11k+ `bash_outcomes` + 9k+
//! edit records carry that signal, unused.
//!
//! [`LearnedOutcomeModel`] closes the cold-start gap. It generalizes across
//! signatures by keying on **features** — `(tool_class, intent_class, context)`
//! extracted from the [`ActionSignature`] — so a brand-new signature inherits the
//! success rate of its feature class. It keeps the Beta estimator as the
//! **fallback prior**: an unseen feature tuple still predicts `0.5`, never a
//! `NaN`, so the model is strictly additive over the existing predictor.
//!
//! # Why this beats the bare Beta baseline
//!
//! For a never-before-seen signature whose *feature class* is well-observed
//! (e.g. `bash / cargo / plain` with 90% historical success), the bare predictor
//! returns `0.5` (no per-signature data) while the learned model returns ~`0.9`
//! — a calibrated, evidence-backed estimate. Where the feature class is also
//! unseen, both return `0.5`: the learned model never does worse than the prior.

use super::predict::{
    ExecutionOutcomePredictor, OutcomeStats, PredictionConfidence, PredictionReport,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use touring_hooks_shared::action_signature::ActionSignature;

/// The feature tuple an outcome is generalized over — the coarse equivalence
/// class of an [`ActionSignature`]. Two signatures with the same features share
/// learned evidence.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ActionFeatures {
    /// Tool class (`bash`, `edit`, `read`, `write`, `search`, `web`, `mcp`, `task`, …).
    pub tool_class: String,
    /// Intent class (`cargo`, `rs`, `symbol`, `python`, `unknown`, …).
    pub intent_class: String,
    /// Context qualifier string (`plain`, `hi-blast`, `hi-complexity`, …).
    pub context: String,
}

impl ActionFeatures {
    /// Extract the feature tuple from an [`ActionSignature`].
    #[must_use]
    pub fn from_signature(sig: &ActionSignature) -> Self {
        Self {
            tool_class: sig.tool_class.clone(),
            intent_class: sig.intent_class.clone(),
            context: sig.context_qualifier.as_str().to_owned(),
        }
    }

    /// Construct from raw parts (test ergonomics + non-signature callers).
    #[must_use]
    pub fn from_parts(
        tool_class: impl Into<String>,
        intent_class: impl Into<String>,
        context: impl Into<String>,
    ) -> Self {
        Self {
            tool_class: tool_class.into(),
            intent_class: intent_class.into(),
            context: context.into(),
        }
    }

    /// Parse a signature key (`outcome:<tool>:<intent>:<ctx>` — the format of
    /// [`ActionSignature::to_key`](touring_hooks_shared::action_signature::ActionSignature::to_key))
    /// back into features. Returns `None` for a malformed key (fail-open), so the
    /// X4 closure falls back to the neutral prior.
    #[must_use]
    pub fn from_key(key: &str) -> Option<Self> {
        let rest = key.strip_prefix("outcome:")?;
        let mut parts = rest.splitn(3, ':');
        let tool_class = parts.next()?;
        let intent_class = parts.next()?;
        let context = parts.next()?;
        if tool_class.is_empty() || intent_class.is_empty() || context.is_empty() {
            return None;
        }
        Some(Self::from_parts(tool_class, intent_class, context))
    }

    fn key(&self) -> (String, String, String) {
        (
            self.tool_class.clone(),
            self.intent_class.clone(),
            self.context.clone(),
        )
    }
}

/// One training example — a feature tuple and the observed outcome.
#[derive(Debug, Clone)]
pub struct OutcomeExample {
    /// The action's feature tuple.
    pub features: ActionFeatures,
    /// `true` if the action succeeded.
    pub success: bool,
}

impl OutcomeExample {
    /// Construct an example.
    #[must_use]
    pub fn new(features: ActionFeatures, success: bool) -> Self {
        Self { features, success }
    }
}

/// A learned per-feature-tuple outcome model — the calibrated upgrade to the
/// bare [`ExecutionOutcomePredictor`] cold-start prior (S-11 / R10).
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LearnedOutcomeModel {
    counts: HashMap<(String, String, String), OutcomeStats>,
    total_examples: u32,
}

impl LearnedOutcomeModel {
    /// Train a fresh model from a batch of examples (the `bash_outcomes` /
    /// edit-history substrate).
    #[must_use]
    pub fn train_from_examples<I>(examples: I) -> Self
    where
        I: IntoIterator<Item = OutcomeExample>,
    {
        let mut model = Self::default();
        for ex in examples {
            model.observe(&ex.features, ex.success);
        }
        model
    }

    /// Fold a single observation into the model.
    pub fn observe(&mut self, features: &ActionFeatures, success: bool) {
        let stats = self.counts.entry(features.key()).or_default();
        if success {
            stats.successes += 1;
        } else {
            stats.failures += 1;
        }
        self.total_examples = self.total_examples.saturating_add(1);
    }

    /// Total training examples folded in.
    #[must_use]
    pub fn total_examples(&self) -> u32 {
        self.total_examples
    }

    /// Number of distinct feature tuples learned.
    #[must_use]
    pub fn distinct_features(&self) -> usize {
        self.counts.len()
    }

    /// The learned counts for a feature tuple, if observed.
    #[must_use]
    pub fn stats_for(&self, features: &ActionFeatures) -> Option<OutcomeStats> {
        self.counts.get(&features.key()).copied()
    }

    /// ES4 P2 — iterate over all distinct `(ActionFeatures, OutcomeStats)`
    /// pairs in this model. Used by `merge_into_global` to replay the
    /// distilled substrate into the process-global online model. The
    /// returned Vec is small (one entry per distinct feature key) so the
    /// allocation is bounded by the model's distinct-feature count.
    #[must_use]
    pub fn snapshot_distinct_features(&self) -> Vec<(ActionFeatures, OutcomeStats)> {
        self.counts
            .iter()
            .map(|(key, stats)| {
                let features =
                    ActionFeatures::from_parts(key.0.clone(), key.1.clone(), key.2.clone());
                (features, *stats)
            })
            .collect()
    }

    /// ES4 P2 — replay every observation in this (typically the
    /// distilled-from-`bash_outcomes`) model into the process-global online
    /// model. Idempotent at the model level: re-distilling the same data
    /// reinforces the running Laplace-smoothed mean (per-row influence
    /// halves as n grows). Returns the number of (success, failure)
    /// observations applied.
    pub fn merge_into_global(&self) -> usize {
        let mut applied = 0;
        for (features, stats) in self.snapshot_distinct_features() {
            for _ in 0..stats.successes {
                observe_global_outcome(&features, true);
                applied += 1;
            }
            for _ in 0..stats.failures {
                observe_global_outcome(&features, false);
                applied += 1;
            }
        }
        touring_hooks_shared::gate_metrics::record_outcome_learner_distill(applied);
        applied
    }

    /// Predict `(probability, confidence, matched_observations)` for a feature
    /// tuple. Uses the learned counts smoothed by `fallback`'s Beta prior when
    /// the tuple was observed; otherwise returns the bare prior (`0.5` for the
    /// default predictor) with `None` confidence — never worse than the baseline.
    #[must_use]
    pub fn predict_from_features(
        &self,
        features: &ActionFeatures,
        fallback: &ExecutionOutcomePredictor,
    ) -> (f64, PredictionConfidence, u32) {
        match self.counts.get(&features.key()) {
            Some(stats) if stats.total() > 0 => (
                fallback.success_probability(stats),
                PredictionConfidence::from_total(stats.total()),
                stats.total(),
            ),
            _ => {
                let neutral = OutcomeStats::default();
                (
                    fallback.success_probability(&neutral),
                    PredictionConfidence::None,
                    0,
                )
            }
        }
    }

    /// Build a backward-compatible [`PredictionReport`] for a signature, blending
    /// the learned feature-class evidence with the per-signature observations.
    ///
    /// Resolution order: per-signature observations win when present (most
    /// specific); otherwise the learned feature-class generalizes the cold-start;
    /// otherwise the bare Beta prior. The `signature` field carries the exact key
    /// so X9 LEARN still records against it.
    #[must_use]
    pub fn prediction_report(
        &self,
        sig: &ActionSignature,
        per_signature: &OutcomeStats,
        prior: &ExecutionOutcomePredictor,
    ) -> PredictionReport {
        let features = ActionFeatures::from_signature(sig);
        // Per-signature evidence is the most specific — prefer it when present.
        if per_signature.total() > 0 {
            return PredictionReport {
                signature: sig.to_key(),
                success_probability: prior.success_probability(per_signature),
                observed: *per_signature,
                confidence: PredictionConfidence::from_total(per_signature.total()),
            };
        }
        // Cold-start for this signature → generalize over the feature class.
        let (probability, confidence, _matched) = self.predict_from_features(&features, prior);
        let observed = self.stats_for(&features).unwrap_or_default();
        PredictionReport {
            signature: sig.to_key(),
            success_probability: probability,
            observed,
            confidence,
        }
    }
}

// ── S-11 — process-global online outcome model ──────────────────────────────────

/// The process-global online [`LearnedOutcomeModel`] (S-11 / R10).
///
/// `HookRuntime` is per-client (see `daemon.rs`), so the learned model lives in a
/// process-wide singleton instead: the PostToolUse hook
/// [`observe_global_outcome`]s every real outcome into it, and the X4 PREDICT
/// closure [`global_model_outcome_history`] reads it — a closed online-learning
/// loop that runs for the daemon's whole life. `RwLock`: many concurrent readers
/// (every gated action), brief exclusive writers (one per PostToolUse).
fn global_model() -> &'static std::sync::RwLock<LearnedOutcomeModel> {
    static GLOBAL_MODEL: std::sync::OnceLock<std::sync::RwLock<LearnedOutcomeModel>> =
        std::sync::OnceLock::new();
    GLOBAL_MODEL.get_or_init(|| std::sync::RwLock::new(LearnedOutcomeModel::default()))
}

/// **S-11** — record one observed outcome into the global online model.
///
/// Called from the PostToolUse hook (`post_tool_rl`) on every tool result, so the
/// model accumulates real `(tool_class, intent_class, context) → success/failure`
/// counts as the session runs — the correct online data source (not the
/// presence-only `outcome:*` memory ledger). Fail-open: a poisoned lock is
/// recovered rather than panicking.
pub fn observe_global_outcome(features: &ActionFeatures, success: bool) {
    {
        let mut model = global_model().write().unwrap_or_else(|p| p.into_inner());
        model.observe(features, success);
    }
    // ES4 P1: debounced durability — every PERSIST_EVERY observations, atomically
    // snapshot to disk so a hard restart (no clean session-stop) loses at most
    // PERSIST_EVERY-1 outcomes. Best-effort + fail-open; no-op until a warm-load
    // has configured the snapshot path.
    let folded = dirty_counter().fetch_add(1, Ordering::Relaxed) + 1;
    if folded >= PERSIST_EVERY {
        // Reset first so a missing path (persist no-op) doesn't retry every call.
        dirty_counter().store(0, Ordering::Relaxed);
        let _ = persist_global_model();
    }
}

/// **S-11** — the X4 PREDICT outcome-history closure backed by the global model.
///
/// Given an [`ActionSignature`] key (`outcome:<tool>:<intent>:<ctx>`), returns the
/// model's accumulated [`OutcomeStats`] for that feature class, or the neutral
/// default when the class is unseen or the key is malformed. Wired into
/// `gate_hook_input` / `observe` in place of the always-neutral
/// `neutral_outcome_history`, so X4 stops predicting a flat `0.5` once a feature
/// class has history (beating the Beta cold-start).
#[must_use]
pub fn global_model_outcome_history(key: &str) -> OutcomeStats {
    match ActionFeatures::from_key(key) {
        Some(features) => {
            let model = global_model().read().unwrap_or_else(|p| p.into_inner());
            model.stats_for(&features).unwrap_or_default()
        }
        None => OutcomeStats::default(),
    }
}

/// **S-11** — snapshot the global model (clone) for callers needing an owned
/// copy, e.g. [`rank_by_predicted`](super::speculative::rank_by_predicted) in the
/// speculative driver (S-12). Cheap relative to a gateway run, and avoids holding
/// the read lock across an `await`.
#[must_use]
pub fn global_model_snapshot() -> LearnedOutcomeModel {
    global_model()
        .read()
        .unwrap_or_else(|p| p.into_inner())
        .clone()
}

// ── ES4 P1 — durable persistence + warm-load of the global world model ──────────
//
// The online loop (observe → key-based read) already runs for the daemon's life,
// but the model is RAM-only: a fresh daemon predicts a flat `0.5` until the
// session re-accumulates, even though 64+ outcomes sit durably in `bash_outcomes`.
// This module persists the distilled model to a JSON snapshot and warm-loads it
// once per process, so X4 PREDICT survives restart with calibrated history.

/// Persist after every Nth online observation, so a hard daemon restart (e.g.
/// `update-touring`) loses at most `PERSIST_EVERY - 1` observations even without
/// a clean session-stop flush.
const PERSIST_EVERY: u32 = 16;

/// One feature-class entry in a [`WorldModelSnapshot`] — the JSON-safe flattening
/// of the in-RAM `(tool, intent, ctx) → OutcomeStats` map entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldModelEntry {
    /// Tool class (`bash`, `edit`, …).
    pub tool_class: String,
    /// Intent class (`cargo`, `rs`, …).
    pub intent_class: String,
    /// Context qualifier (`plain`, `hi-blast`, …).
    pub context: String,
    /// Observed successes for this class.
    pub successes: u32,
    /// Observed failures for this class.
    pub failures: u32,
}

/// JSON-safe on-disk snapshot of a [`LearnedOutcomeModel`].
///
/// The in-RAM model keys its counts by a `(tool, intent, ctx)` **tuple**, which
/// `serde_json` cannot emit as an object key (JSON keys must be strings). This
/// flattened entry-list form is the durable representation: stable, human-
/// inspectable, and round-trippable through [`LearnedOutcomeModel::to_snapshot`]
/// / [`LearnedOutcomeModel::from_snapshot`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldModelSnapshot {
    /// One entry per observed feature class (sorted for deterministic bytes).
    pub entries: Vec<WorldModelEntry>,
    /// Total observations folded across all classes (parity check).
    pub total_examples: u32,
}

/// Durable-model status payload for the `world-model-status` CLI / liveness probe
/// (ES4 P1 observability — proves the model survived restart).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldModelStatus {
    /// Total observations the live model has folded in.
    pub total_examples: u32,
    /// Distinct feature classes the live model knows.
    pub distinct_features: usize,
    /// Entries merged in by this process's one-time warm-load (0 ⇒ cold start).
    pub warm_loaded_entries: u64,
    /// Configured on-disk snapshot path (`None` until the first warm-load runs).
    pub snapshot_path: Option<String>,
    /// Whether the snapshot file currently exists on disk.
    pub snapshot_exists: bool,
}

impl LearnedOutcomeModel {
    /// Project the model into its JSON-safe [`WorldModelSnapshot`] form. Entries
    /// are sorted by `(tool, intent, ctx)` so the serialized bytes are stable
    /// (smaller diffs, reproducible test assertions).
    #[must_use]
    pub fn to_snapshot(&self) -> WorldModelSnapshot {
        let mut entries: Vec<WorldModelEntry> = self
            .counts
            .iter()
            .map(
                |((tool_class, intent_class, context), stats)| WorldModelEntry {
                    tool_class: tool_class.clone(),
                    intent_class: intent_class.clone(),
                    context: context.clone(),
                    successes: stats.successes,
                    failures: stats.failures,
                },
            )
            .collect();
        entries.sort_by(|a, b| {
            (&a.tool_class, &a.intent_class, &a.context).cmp(&(
                &b.tool_class,
                &b.intent_class,
                &b.context,
            ))
        });
        WorldModelSnapshot {
            entries,
            total_examples: self.total_examples,
        }
    }

    /// Reconstruct a model from a [`WorldModelSnapshot`] (the warm-load inverse of
    /// [`to_snapshot`](Self::to_snapshot)).
    #[must_use]
    pub fn from_snapshot(snapshot: &WorldModelSnapshot) -> Self {
        let mut counts = HashMap::with_capacity(snapshot.entries.len());
        for e in &snapshot.entries {
            counts.insert(
                (
                    e.tool_class.clone(),
                    e.intent_class.clone(),
                    e.context.clone(),
                ),
                OutcomeStats {
                    successes: e.successes,
                    failures: e.failures,
                },
            );
        }
        Self {
            counts,
            total_examples: snapshot.total_examples,
        }
    }

    /// Fold another model's counts into this one. **Order-independent**: warm-load
    /// merges the on-disk snapshot into the (usually empty) live model without
    /// losing observations recorded in the startup window before the load fired.
    pub fn merge(&mut self, other: &Self) {
        for (key, stats) in &other.counts {
            let entry = self.counts.entry(key.clone()).or_default();
            entry.successes = entry.successes.saturating_add(stats.successes);
            entry.failures = entry.failures.saturating_add(stats.failures);
            self.total_examples = self.total_examples.saturating_add(stats.total());
        }
    }
}

/// Configured on-disk snapshot path for the process-global model. Set once at the
/// first session-start warm-load; read by the debounced + shutdown persists.
fn world_model_path_cell() -> &'static std::sync::RwLock<Option<PathBuf>> {
    static CELL: std::sync::OnceLock<std::sync::RwLock<Option<PathBuf>>> =
        std::sync::OnceLock::new();
    CELL.get_or_init(|| std::sync::RwLock::new(None))
}

/// Observations folded since the last successful persist (debounce counter).
fn dirty_counter() -> &'static AtomicU32 {
    static C: AtomicU32 = AtomicU32::new(0);
    &C
}

/// Feature-class entries merged in by this process's one-time warm-load.
fn warm_loaded_cell() -> &'static AtomicU64 {
    static C: AtomicU64 = AtomicU64::new(0);
    &C
}

/// Once-per-process guard: the durable model is loaded into RAM exactly once;
/// thereafter the live model is the source of truth. A singleton daemon serves
/// many CC sessions — re-merging the snapshot each session-start would double-count.
fn warm_loaded_guard() -> &'static AtomicBool {
    static G: AtomicBool = AtomicBool::new(false);
    &G
}

/// **ES4 P1** — warm-load the durable world model from `project_root`'s snapshot.
///
/// Resolves and remembers the canonical snapshot path (so later debounced and
/// shutdown persists know where to write), then — **exactly once per process** —
/// merges the on-disk snapshot into the live global model. Returns the number of
/// feature-class entries loaded (`0` on cold start, missing file, parse error, or
/// any call after the first). Fail-open: errors log and leave the live model
/// untouched (the pre-ES4 cold behavior), never panicking.
pub fn warm_load_global_model(project_root: &Path) -> usize {
    // Load + configure exactly once per process — subsequent session-starts and
    // sibling project actors are no-ops. The FIRST caller (typically the daemon's
    // primary workspace) owns the durable snapshot path; later callers must NOT
    // overwrite it, else a transient sibling project (e.g. HOME) would redirect
    // every persist to its own file and the model would migrate across restarts.
    if warm_loaded_guard().swap(true, Ordering::SeqCst) {
        return 0;
    }
    let path = touring_foundation::TouringConfig::world_model_canonical(project_root);
    // Remember the path for debounced + shutdown persists (set only by the loader).
    if let Ok(mut cell) = world_model_path_cell().write() {
        *cell = Some(path.clone());
    }
    let data = match std::fs::read_to_string(&path) {
        Ok(d) => d,
        Err(_) => {
            tracing::debug!(?path, "ES4: world model snapshot absent — cold start");
            return 0;
        }
    };
    let snapshot: WorldModelSnapshot = match serde_json::from_str(&data) {
        Ok(s) => s,
        Err(e) => {
            tracing::warn!("ES4: world model snapshot parse failed: {e} — cold start");
            return 0;
        }
    };
    let loaded = LearnedOutcomeModel::from_snapshot(&snapshot);
    let n = loaded.distinct_features();
    {
        let mut model = global_model().write().unwrap_or_else(|p| p.into_inner());
        model.merge(&loaded);
    }
    warm_loaded_cell().store(n as u64, Ordering::Relaxed);
    tracing::info!(
        entries = n,
        total = snapshot.total_examples,
        "ES4: warm-loaded durable action world model"
    );
    n
}

/// **ES4 P1** — atomically persist the global model to the configured snapshot
/// path. No-op (`false`) when no path has been configured (no warm-load ran yet).
/// Best-effort, fail-open.
pub fn persist_global_model() -> bool {
    let path = world_model_path_cell().read().ok().and_then(|c| c.clone());
    match path {
        Some(p) => persist_global_model_to(&p),
        None => false,
    }
}

/// **ES4 P1** — persist the global model to an explicit `path` (test + CLI seam).
///
/// Atomic: serializes to a sibling `.tmp` then `rename`s, so a concurrent reader
/// never observes a torn file. Returns `false` (fail-open) on any I/O error.
pub fn persist_global_model_to(path: &Path) -> bool {
    let snapshot = global_model_snapshot().to_snapshot();
    let json = match serde_json::to_string_pretty(&snapshot) {
        Ok(j) => j,
        Err(e) => {
            tracing::warn!("ES4: world model serialize failed: {e}");
            return false;
        }
    };
    if let Some(parent) = path.parent()
        && let Err(e) = std::fs::create_dir_all(parent)
    {
        tracing::warn!("ES4: world model dir create failed: {e}");
        return false;
    }
    let tmp = path.with_extension("json.tmp");
    if let Err(e) = std::fs::write(&tmp, json.as_bytes()) {
        tracing::warn!("ES4: world model tmp write failed: {e}");
        return false;
    }
    if let Err(e) = std::fs::rename(&tmp, path) {
        tracing::warn!("ES4: world model rename failed: {e}");
        let _ = std::fs::remove_file(&tmp);
        return false;
    }
    dirty_counter().store(0, Ordering::Relaxed);
    true
}

/// **ES4 P1** — durable-model status for the `world-model-status` CLI / liveness
/// probe. Reads the live global model + the configured snapshot path.
#[must_use]
pub fn world_model_status() -> WorldModelStatus {
    let (total_examples, distinct_features) = {
        let model = global_model().read().unwrap_or_else(|p| p.into_inner());
        (model.total_examples(), model.distinct_features())
    };
    let path = world_model_path_cell().read().ok().and_then(|c| c.clone());
    let snapshot_exists = path.as_ref().is_some_and(|p| p.exists());
    WorldModelStatus {
        total_examples,
        distinct_features,
        warm_loaded_entries: warm_loaded_cell().load(Ordering::Relaxed),
        snapshot_path: path.map(|p| p.display().to_string()),
        snapshot_exists,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ex(tool: &str, intent: &str, ctx: &str, success: bool) -> OutcomeExample {
        OutcomeExample::new(ActionFeatures::from_parts(tool, intent, ctx), success)
    }

    #[test]
    fn train_folds_examples_and_counts() {
        let model = LearnedOutcomeModel::train_from_examples(vec![
            ex("bash", "cargo", "plain", true),
            ex("bash", "cargo", "plain", true),
            ex("bash", "cargo", "plain", false),
        ]);
        assert_eq!(model.total_examples(), 3);
        assert_eq!(model.distinct_features(), 1);
        let stats = model
            .stats_for(&ActionFeatures::from_parts("bash", "cargo", "plain"))
            .unwrap();
        assert_eq!(stats.successes, 2);
        assert_eq!(stats.failures, 1);
    }

    #[test]
    fn unseen_feature_falls_back_to_beta_prior() {
        let model = LearnedOutcomeModel::default();
        let prior = ExecutionOutcomePredictor::new();
        let (p, conf, matched) = model.predict_from_features(
            &ActionFeatures::from_parts("bash", "novel", "plain"),
            &prior,
        );
        assert!(
            (p - 0.5).abs() < 1e-9,
            "unseen tuple must predict the 0.5 prior"
        );
        assert_eq!(conf, PredictionConfidence::None);
        assert_eq!(matched, 0);
    }

    #[test]
    fn predictor_beats_beta_baseline_on_well_observed_class() {
        // 18 successes / 2 failures for bash/cargo/plain → strongly successful class.
        let mut examples = Vec::new();
        for _ in 0..18 {
            examples.push(ex("bash", "cargo", "plain", true));
        }
        for _ in 0..2 {
            examples.push(ex("bash", "cargo", "plain", false));
        }
        let model = LearnedOutcomeModel::train_from_examples(examples);
        let prior = ExecutionOutcomePredictor::new();
        let feats = ActionFeatures::from_parts("bash", "cargo", "plain");
        let (learned_p, conf, matched) = model.predict_from_features(&feats, &prior);
        // Bare baseline for a NEW signature in this class has no per-signature data
        // → 0.5. The learned model discriminates: ~ (18+1)/(20+2) = 0.8636.
        let baseline_p = prior.success_probability(&OutcomeStats::default());
        assert!((baseline_p - 0.5).abs() < 1e-9);
        assert!(
            learned_p > 0.8,
            "learned model must reflect the 90% class success: got {learned_p}"
        );
        assert!(learned_p > baseline_p, "learned must beat the bare prior");
        assert_eq!(conf, PredictionConfidence::High);
        assert_eq!(matched, 20);
    }

    #[test]
    fn prediction_report_prefers_per_signature_then_class_then_prior() {
        let model = LearnedOutcomeModel::train_from_examples(vec![
            ex("bash", "cargo", "plain", true),
            ex("bash", "cargo", "plain", true),
        ]);
        let prior = ExecutionOutcomePredictor::new();
        let sig = ActionSignature {
            tool_class: "bash".to_owned(),
            intent_class: "cargo".to_owned(),
            context_qualifier: touring_hooks_shared::action_signature::ContextQualifier::Plain,
        };

        // (a) per-signature evidence present → wins (most specific).
        let per_sig = OutcomeStats {
            successes: 0,
            failures: 6,
        };
        let report = model.prediction_report(&sig, &per_sig, &prior);
        assert!(
            report.success_probability < 0.2,
            "per-signature failures must dominate: {}",
            report.success_probability
        );
        assert_eq!(report.observed, per_sig);

        // (b) cold-start signature (no per-signature data) → feature class generalizes.
        let cold = OutcomeStats::default();
        let report2 = model.prediction_report(&sig, &cold, &prior);
        assert!(
            report2.success_probability > 0.5,
            "feature class (2 successes) must lift the cold-start estimate: {}",
            report2.success_probability
        );
    }

    // ── S-11 — global online loop (observe → key-based read) ────────────────

    #[test]
    fn global_loop_observe_then_read_via_signature_key() {
        // A unique feature class so the process-global model is pollution-free
        // even under parallel test execution.
        let features = ActionFeatures::from_parts("s11tool", "s11intent", "s11ctx");
        observe_global_outcome(&features, true);
        observe_global_outcome(&features, true);
        observe_global_outcome(&features, false);

        // The X4 closure reads the accumulated counts back via the signature key.
        let stats = global_model_outcome_history("outcome:s11tool:s11intent:s11ctx");
        assert_eq!(
            stats.total(),
            3,
            "all three observed outcomes must be readable"
        );
        assert_eq!(stats.successes, 2);
        assert_eq!(stats.failures, 1);
    }

    #[test]
    fn from_key_roundtrips_and_rejects_garbage() {
        assert_eq!(
            ActionFeatures::from_key("outcome:bash:cargo:plain"),
            Some(ActionFeatures::from_parts("bash", "cargo", "plain"))
        );
        // Malformed keys → None (fail-open), so the closure falls back to neutral.
        assert!(ActionFeatures::from_key("garbage").is_none());
        assert!(ActionFeatures::from_key("outcome:only:two").is_none());
        assert!(
            ActionFeatures::from_key("outcome:a::c").is_none(),
            "empty field rejected"
        );
        // An unseen class reads as the neutral default (total 0).
        assert_eq!(
            global_model_outcome_history("outcome:never:seen:here").total(),
            0
        );
    }

    // ── ES4 P1 — durable persistence + warm-load ────────────────────────────

    #[test]
    fn snapshot_roundtrips_through_json() {
        // The in-RAM model keys counts by a `(tool, intent, ctx)` tuple, which
        // serde_json cannot emit as an object key. to_snapshot/from_snapshot must
        // survive a real JSON round-trip — this is the regression guard for the
        // "derives Serialize but can't actually serialize to JSON" trap.
        let model = LearnedOutcomeModel::train_from_examples(vec![
            ex("bash", "cargo", "plain", true),
            ex("bash", "cargo", "plain", true),
            ex("bash", "cargo", "plain", false),
            ex("edit", "rs", "hi-blast", true),
        ]);
        let snap = model.to_snapshot();
        let json = serde_json::to_string(&snap).expect("snapshot must be JSON-serializable");
        assert!(json.contains("bash"));
        assert!(json.contains("total_examples"));
        let parsed: WorldModelSnapshot = serde_json::from_str(&json).expect("round-trip parse");
        let restored = LearnedOutcomeModel::from_snapshot(&parsed);
        assert_eq!(restored.total_examples(), model.total_examples());
        assert_eq!(restored.distinct_features(), model.distinct_features());
        let s = restored
            .stats_for(&ActionFeatures::from_parts("bash", "cargo", "plain"))
            .expect("class must reload");
        assert_eq!(s.successes, 2);
        assert_eq!(s.failures, 1);
    }

    #[test]
    fn snapshot_entries_sorted_for_stable_bytes() {
        let model = LearnedOutcomeModel::train_from_examples(vec![
            ex("zzz", "z", "z", true),
            ex("aaa", "a", "a", true),
        ]);
        let snap = model.to_snapshot();
        assert_eq!(snap.entries[0].tool_class, "aaa");
        assert_eq!(snap.entries[1].tool_class, "zzz");
    }

    #[test]
    fn merge_sums_counts_across_classes() {
        let mut a = LearnedOutcomeModel::train_from_examples(vec![
            ex("bash", "cargo", "plain", true),
            ex("bash", "cargo", "plain", false),
        ]);
        let b = LearnedOutcomeModel::train_from_examples(vec![
            ex("bash", "cargo", "plain", true),
            ex("edit", "rs", "plain", true),
        ]);
        a.merge(&b);
        assert_eq!(a.total_examples(), 4);
        let s = a
            .stats_for(&ActionFeatures::from_parts("bash", "cargo", "plain"))
            .unwrap();
        assert_eq!(s.successes, 2);
        assert_eq!(s.failures, 1);
        assert!(
            a.stats_for(&ActionFeatures::from_parts("edit", "rs", "plain"))
                .is_some()
        );
    }

    #[test]
    #[serial_test::serial]
    fn persist_global_to_file_then_reload() {
        // Observe a uniquely-named class into the global model, persist it to a
        // temp file, then reload the file at the model level — proving the on-disk
        // snapshot is valid JSON and round-trips the durable counts.
        let feats = ActionFeatures::from_parts("persisttool", "persistintent", "persistctx");
        observe_global_outcome(&feats, true);
        observe_global_outcome(&feats, true);
        observe_global_outcome(&feats, false);

        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("wm.json");
        assert!(persist_global_model_to(&path), "persist must succeed");
        assert!(path.exists(), "snapshot file must be written");

        let data = std::fs::read_to_string(&path).unwrap();
        let snap: WorldModelSnapshot = serde_json::from_str(&data).unwrap();
        let reloaded = LearnedOutcomeModel::from_snapshot(&snap);
        let s = reloaded
            .stats_for(&feats)
            .expect("persisted class must reload");
        assert_eq!(s.successes, 2);
        assert_eq!(s.failures, 1);
    }

    #[test]
    #[serial_test::serial]
    fn warm_load_configures_path_and_loads_once() {
        // A project layout with a pre-written snapshot — the restart scenario.
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let snap = WorldModelSnapshot {
            entries: vec![WorldModelEntry {
                tool_class: "wltool".to_owned(),
                intent_class: "wlintent".to_owned(),
                context: "wlctx".to_owned(),
                successes: 7,
                failures: 1,
            }],
            total_examples: 8,
        };
        let path = touring_foundation::TouringConfig::world_model_canonical(root);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, serde_json::to_string(&snap).unwrap()).unwrap();

        // First warm-load in the process merges the snapshot in.
        warm_load_global_model(root);

        let status = world_model_status();
        assert_eq!(
            status.snapshot_path.as_deref(),
            Some(path.display().to_string().as_str()),
            "warm-load must configure the snapshot path"
        );
        assert!(
            status.snapshot_exists,
            "snapshot file must be reported present"
        );

        // The class is readable through the X4 PREDICT key path post-load.
        let stats = global_model_outcome_history("outcome:wltool:wlintent:wlctx");
        assert_eq!(
            stats.total(),
            8,
            "warm-loaded class must be readable via X4 key"
        );
        assert_eq!(stats.successes, 7);

        // Second warm-load is a no-op (once-per-process guard prevents double-count).
        assert_eq!(
            warm_load_global_model(root),
            0,
            "second warm-load must be a no-op"
        );
    }
}
