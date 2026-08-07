//! Contextual Bandit — LinUCB for adaptive context injection.
//!
//! Selects which type of context to inject for each Claude Code operation,
//! based on features of the current state (file type, session turn, CILA level, etc.).
//!
//! # Modules
//!
//! - [`linucb`] — Base 19-dim LinUCB bandit (stable, backward-compatible).
//! - [`ast_features`] — AST-derived feature extraction (16 features from touring-ast).
//! - [`ast_enriched`] — 35-dim LinUCB bandit combining base + AST features.
//! - [`transfer`] — Soft parameter transfer between LinUCB bandits (TransferLinUCB).
//! - [`reminder_bandit`] — 6-dim LinUCB bandit for system reminder selection (VGP, memory recall, etc.).

pub mod adaptive_alpha;
pub mod ast_enriched;
pub mod ast_features;
pub mod decision_ledger;
pub mod granularity;
pub mod linucb;
pub mod reminder_bandit;
pub mod transfer;

pub use linucb::{
    ArmKind, FEATURE_DIM, LinUCBArm, LinUCBBandit, LinUcbError, NUM_ARMS, extract_features,
    extract_features_rich,
};

pub use adaptive_alpha::AdaptiveAlpha;
pub use ast_enriched::AstEnrichedBandit;
pub use ast_features::{AST_FEATURE_COUNT, FEATURE_DIM_AST, enrich_features, extract_ast_features};
pub use decision_ledger::{
    ArmChoice, CaseLedger, DecisionLedger, Ledger, PendingDecision, blend_case_value,
};
pub use granularity::{
    GRANULARITY_FEATURE_DIM, GRANULARITY_NUM_ARMS, GranularityArmState, GranularityBandit,
    GranularitySnapshot, SplitFactor, features_for_task, reward_from_quality,
};
pub use reminder_bandit::{ReminderBandit, ReminderContext, ReminderKind};
pub use transfer::TransferLinUCB;

// ── Unified ContextualBandit trait ──────────────────────────────────────

/// Unified interface for contextual bandits.
/// Enables `Box<dyn ContextualBandit>` in HookRuntime for polymorphic bandit selection.
///
/// Uses `&[f64]` for features (not `&Array1<f64>`) for ergonomics — conversion is internal.
/// Note: `select_arm` and `update` share names with inherent methods on concrete types
/// but differ in parameter types (`&[f64]` vs `&Array1<f64>`), so Rust resolves them
/// correctly. `total_pulls` has an identical signature — inherent method takes priority
/// on concrete types; use UFCS or `dyn ContextualBandit` for the trait version.
pub trait ContextualBandit: Send + Sync {
    /// Select the best arm given features. Returns (arm_index, confidence_score).
    fn select_arm(&mut self, features: &[f64]) -> (usize, f64);

    /// Update model after observing reward for selected arm.
    fn update(&mut self, arm: usize, features: &[f64], reward: f64);

    /// Total pulls across all arms.
    fn total_pulls(&self) -> u64;

    /// Number of arms.
    fn num_arms(&self) -> usize;

    /// Export model state for persistence.
    fn export_snapshot(&self) -> BanditSnapshot;

    /// Import model state from snapshot.
    fn import_snapshot(&mut self, snapshot: &BanditSnapshot) -> Result<(), String>;
}

/// Serializable model snapshot for persistence and transfer.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct BanditSnapshot {
    /// Bandit type identifier: "linucb", "ast_enriched", "transfer"
    pub bandit_type: String,
    /// Feature dimensionality
    pub feature_dim: usize,
    /// Number of arms
    pub num_arms: usize,
    /// Total pulls at time of snapshot
    pub total_pulls: u64,
    /// Serialized model data (JSON)
    pub model_data: String,
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod contextual_bandit_tests {
    use super::*;

    /// Helper: exercise the ContextualBandit trait through a `Box<dyn>`.
    fn exercise_bandit(bandit: &mut Box<dyn ContextualBandit>, features: &[f64]) {
        assert_eq!(bandit.num_arms(), NUM_ARMS);
        assert_eq!(bandit.total_pulls(), 0);

        // Select and update cycle
        let (arm, score) = bandit.select_arm(features);
        assert!(arm < NUM_ARMS);
        assert!(score.is_finite());

        bandit.update(arm, features, 0.7);
        assert_eq!(bandit.total_pulls(), 1);

        // Snapshot roundtrip
        let snapshot = bandit.export_snapshot();
        assert_eq!(snapshot.num_arms, NUM_ARMS);
        assert_eq!(snapshot.total_pulls, 1);
        assert!(!snapshot.model_data.is_empty());

        // Import into a fresh bandit and verify state
        let snapshot_json = serde_json::to_string(&snapshot).expect("snapshot serializes");
        let snapshot_restored: BanditSnapshot =
            serde_json::from_str(&snapshot_json).expect("snapshot deserializes");
        bandit
            .import_snapshot(&snapshot_restored)
            .expect("import succeeds");
        assert_eq!(bandit.total_pulls(), 1);
    }

    #[test]
    fn test_linucb_as_dyn_contextual_bandit() {
        let mut bandit: Box<dyn ContextualBandit> = Box::new(LinUCBBandit::new());
        let features = vec![0.0f64; FEATURE_DIM];
        exercise_bandit(&mut bandit, &features);
        assert_eq!(bandit.export_snapshot().bandit_type, "linucb");
    }

    #[test]
    fn test_ast_enriched_as_dyn_contextual_bandit() {
        let mut bandit: Box<dyn ContextualBandit> = Box::new(AstEnrichedBandit::new(1.0));
        let features = vec![0.0f64; FEATURE_DIM_AST];
        exercise_bandit(&mut bandit, &features);
        assert_eq!(bandit.export_snapshot().bandit_type, "ast_enriched");
    }

    #[test]
    fn test_ast_enriched_accepts_short_features() {
        let mut bandit: Box<dyn ContextualBandit> = Box::new(AstEnrichedBandit::new(1.0));
        // 19-dim features should be zero-padded to 35
        let features = vec![0.1f64; FEATURE_DIM];
        let (arm, score) = bandit.select_arm(&features);
        assert!(arm < NUM_ARMS);
        assert!(score.is_finite());
    }

    #[test]
    fn test_transfer_as_dyn_contextual_bandit() {
        let mut bandit: Box<dyn ContextualBandit> = Box::new(TransferLinUCB::new(0.8));
        let features = vec![0.0f64; FEATURE_DIM];
        exercise_bandit(&mut bandit, &features);
        assert_eq!(bandit.export_snapshot().bandit_type, "transfer");
    }

    #[test]
    fn test_polymorphic_vec_of_bandits() {
        let mut bandits: Vec<Box<dyn ContextualBandit>> = vec![
            Box::new(LinUCBBandit::new()),
            Box::new(AstEnrichedBandit::new(1.0)),
            Box::new(TransferLinUCB::new(0.5)),
        ];
        let features_19 = vec![0.0f64; FEATURE_DIM];
        let features_35 = vec![0.0f64; FEATURE_DIM_AST];

        let feature_sets = [&features_19[..], &features_35[..], &features_19[..]];

        for (bandit, features) in bandits.iter_mut().zip(feature_sets.iter()) {
            let (arm, _) = bandit.select_arm(features);
            assert!(arm < NUM_ARMS);
            bandit.update(arm, features, 1.0);
            assert_eq!(bandit.total_pulls(), 1);
        }
    }

    #[test]
    fn test_snapshot_type_mismatch_returns_error() {
        let mut linucb: Box<dyn ContextualBandit> = Box::new(LinUCBBandit::new());
        let features = vec![0.0f64; FEATURE_DIM];
        linucb.update(0, &features, 0.5);

        let snapshot = linucb.export_snapshot();

        // Try importing a linucb snapshot into an AstEnrichedBandit
        let mut ast_bandit = AstEnrichedBandit::new(1.0);
        let result =
            <AstEnrichedBandit as ContextualBandit>::import_snapshot(&mut ast_bandit, &snapshot);
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("Expected ast_enriched"));
    }

    #[test]
    fn test_transfer_accepts_linucb_snapshot() {
        // TransferLinUCB should accept both "transfer" and "linucb" snapshots
        let mut linucb = LinUCBBandit::new();
        let features = vec![0.0f64; FEATURE_DIM];
        <LinUCBBandit as ContextualBandit>::update(&mut linucb, 0, &features, 0.5);

        let snapshot = <LinUCBBandit as ContextualBandit>::export_snapshot(&linucb);
        assert_eq!(snapshot.bandit_type, "linucb");

        let mut transfer = TransferLinUCB::new(0.7);
        let result =
            <TransferLinUCB as ContextualBandit>::import_snapshot(&mut transfer, &snapshot);
        assert!(result.is_ok());
        assert_eq!(
            <TransferLinUCB as ContextualBandit>::total_pulls(&transfer),
            1
        );
    }
}
