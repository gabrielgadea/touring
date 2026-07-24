//! Transfer learning for LinUCB bandits.
//!
//! [`TransferLinUCB`] wraps a [`LinUCBBandit`] and allows soft parameter transfer
//! from a donor bandit. The transfer blends the donor's `A_inv` and `b` vectors
//! into the recipient, weighted by task similarity.
//!
//! # Blend formula
//!
//! For each arm `i`, given `blend_weight = similarity * 0.3` (capped at 30%):
//!
//! ```text
//! a_inv_new[i] = (1 - w) * a_inv_self[i] + w * a_inv_donor[i]
//! b_new[i]     = (1 - w) * b_self[i]     + w * b_donor[i]
//! ```
//!
//! The 30% cap ensures the recipient's own learning signal dominates. Transfer
//! is most useful when the recipient is cold (few pulls) and the donor is warm.
//!
//! # Example
//!
//! ```rust,no_run
//! use touring_intelligence::rl::bandit::{LinUCBBandit, FEATURE_DIM};
//! use touring_intelligence::rl::bandit::transfer::TransferLinUCB;
//! use ndarray::Array1;
//!
//! let donor = LinUCBBandit::new(); // warm bandit from a related task
//! let mut transfer = TransferLinUCB::new(0.8); // 80% task similarity
//! transfer.transfer_from(&donor, 0.8);
//!
//! let features = Array1::zeros(FEATURE_DIM);
//! let (arm, score) = transfer.bandit_mut().select_arm(&mut features.clone());
//! ```

use crate::rl::bandit::linucb::{FEATURE_DIM, LinUCBBandit, NUM_ARMS};

/// Maximum fraction of donor parameters to blend in (30%).
const MAX_BLEND_WEIGHT: f64 = 0.3;

/// LinUCB bandit with soft parameter transfer from a donor bandit.
#[derive(Debug, Clone)]
pub struct TransferLinUCB {
    /// Underlying bandit (recipient of transferred knowledge).
    bandit: LinUCBBandit,
    /// Similarity between source and target contexts (0.0–1.0).
    /// Higher similarity → more weight given to the donor's parameters.
    context_similarity: f64,
}

impl TransferLinUCB {
    /// Create a new TransferLinUCB with a fresh underlying bandit.
    ///
    /// `context_similarity` should be in [0.0, 1.0] and represents how similar
    /// the donor's task context is to the recipient's. It is clamped internally.
    pub fn new(context_similarity: f64) -> Self {
        Self {
            bandit: LinUCBBandit::new(),
            context_similarity: context_similarity.clamp(0.0, 1.0),
        }
    }

    /// Create a TransferLinUCB wrapping an existing bandit.
    pub fn with_bandit(bandit: LinUCBBandit, context_similarity: f64) -> Self {
        Self {
            bandit,
            context_similarity: context_similarity.clamp(0.0, 1.0),
        }
    }

    /// Perform soft parameter transfer from the donor bandit.
    ///
    /// `similarity` overrides `self.context_similarity` for this call,
    /// allowing callers to use dynamic similarity estimates.
    ///
    /// The blend weight is `min(similarity, self.context_similarity) * MAX_BLEND_WEIGHT`,
    /// ensuring at most 30% of donor parameters are incorporated.
    ///
    /// Uses the public `export`/`import` API of [`LinUCBBandit`] to access
    /// arm parameters without requiring access to private fields.
    pub fn transfer_from(&mut self, donor: &LinUCBBandit, similarity: f64) {
        let similarity = similarity.clamp(0.0, 1.0);
        let blend_weight = similarity * MAX_BLEND_WEIGHT;

        if blend_weight < f64::EPSILON {
            // Nothing to transfer
            return;
        }

        let donor_data = donor.export();
        let self_data = self.bandit.export();

        if donor_data.len() != NUM_ARMS || self_data.len() != NUM_ARMS {
            // Mismatched bandit sizes — skip transfer
            return;
        }

        let mut blended: Vec<(u64, f64, Vec<f64>, Vec<f64>)> = Vec::with_capacity(NUM_ARMS);

        for arm_idx in 0..NUM_ARMS {
            // SAFETY: arm_idx < NUM_ARMS and both vecs verified to have len == NUM_ARMS above
            #[allow(clippy::indexing_slicing)]
            let (self_pulls, self_cum_reward, self_a_inv, self_b) = &self_data[arm_idx];
            #[allow(clippy::indexing_slicing)]
            let (_donor_pulls, _donor_cum_reward, donor_a_inv, donor_b) = &donor_data[arm_idx];

            // Validate dimensions
            let expected_a_len = FEATURE_DIM * FEATURE_DIM;
            let expected_b_len = FEATURE_DIM;

            if self_a_inv.len() != expected_a_len
                || donor_a_inv.len() != expected_a_len
                || self_b.len() != expected_b_len
                || donor_b.len() != expected_b_len
            {
                // Dimension mismatch — keep self unchanged for this arm
                blended.push((
                    *self_pulls,
                    *self_cum_reward,
                    self_a_inv.clone(),
                    self_b.clone(),
                ));
                continue;
            }

            // Blend A_inv: (1 - w) * self + w * donor (element-wise)
            let blended_a_inv: Vec<f64> = self_a_inv
                .iter()
                .zip(donor_a_inv.iter())
                .map(|(s, d)| (1.0 - blend_weight) * s + blend_weight * d)
                .collect();

            // Blend b: (1 - w) * self + w * donor (element-wise)
            let blended_b: Vec<f64> = self_b
                .iter()
                .zip(donor_b.iter())
                .map(|(s, d)| (1.0 - blend_weight) * s + blend_weight * d)
                .collect();

            // Preserve self's pull statistics (not blended — only parameters)
            blended.push((*self_pulls, *self_cum_reward, blended_a_inv, blended_b));
        }

        // Import blended parameters back into the underlying bandit
        self.bandit.import(&blended);
    }

    /// Current context similarity used for transfer.
    pub fn context_similarity(&self) -> f64 {
        self.context_similarity
    }

    /// Update context similarity (e.g., after a new similarity estimate).
    pub fn set_context_similarity(&mut self, similarity: f64) {
        self.context_similarity = similarity.clamp(0.0, 1.0);
    }

    /// Immutable reference to the underlying bandit.
    pub fn bandit(&self) -> &LinUCBBandit {
        &self.bandit
    }

    /// Mutable reference to the underlying bandit (for select_arm, update, etc.).
    pub fn bandit_mut(&mut self) -> &mut LinUCBBandit {
        &mut self.bandit
    }

    /// Consume self and return the underlying bandit.
    pub fn into_bandit(self) -> LinUCBBandit {
        self.bandit
    }
}

// ── ContextualBandit trait impl ──────────────────────────────────────────

/// Helper for serializing TransferLinUCB state (wraps inner bandit data + similarity).
#[derive(serde::Serialize, serde::Deserialize)]
struct TransferSnapshotData {
    context_similarity: f64,
    inner: super::linucb::LinUCBSnapshotData,
}

impl super::ContextualBandit for TransferLinUCB {
    fn select_arm(&mut self, features: &[f64]) -> (usize, f64) {
        // aview1 zero-copy borrows the slice; to_owned() makes a compact owned copy
        // (stack-to-stack, no syscalls). Small array is negligible.
        let arr = ndarray::aview1(features).to_owned();
        self.bandit_mut().select_arm(&arr)
    }

    fn update(&mut self, arm: usize, features: &[f64], reward: f64) {
        // Same: compact owned copy of the borrowed slice.
        let arr = ndarray::aview1(features).to_owned();
        self.bandit_mut().update(arm, &arr, reward);
    }

    fn total_pulls(&self) -> u64 {
        self.bandit().total_pulls()
    }

    fn num_arms(&self) -> usize {
        NUM_ARMS
    }

    fn export_snapshot(&self) -> super::BanditSnapshot {
        let inner = super::linucb::LinUCBSnapshotData {
            alpha: self.bandit().alpha(),
            total_pulls: self.bandit().total_pulls(),
            arms: self.bandit().export(),
        };
        let data = TransferSnapshotData {
            context_similarity: self.context_similarity(),
            inner,
        };
        super::BanditSnapshot {
            bandit_type: "transfer".to_string(),
            feature_dim: FEATURE_DIM,
            num_arms: NUM_ARMS,
            total_pulls: self.bandit().total_pulls(),
            model_data: serde_json::to_string(&data).unwrap_or_default(),
        }
    }

    fn import_snapshot(&mut self, snapshot: &super::BanditSnapshot) -> Result<(), String> {
        if snapshot.bandit_type != "transfer" && snapshot.bandit_type != "linucb" {
            return Err(format!(
                "TransferLinUCB expects transfer or linucb snapshot, got {}",
                snapshot.bandit_type
            ));
        }
        if snapshot.bandit_type == "transfer" {
            let data: TransferSnapshotData = serde_json::from_str(&snapshot.model_data)
                .map_err(|e| format!("Failed to deserialize TransferLinUCB: {e}"))?;
            let mut bandit = LinUCBBandit::new();
            bandit.import(&data.inner.arms);
            bandit.import_total_pulls(data.inner.total_pulls);
            bandit.set_alpha(data.inner.alpha);
            *self = TransferLinUCB::with_bandit(bandit, data.context_similarity);
        } else {
            // "linucb" snapshot — import as inner bandit, keep current similarity
            let data: super::linucb::LinUCBSnapshotData =
                serde_json::from_str(&snapshot.model_data)
                    .map_err(|e| format!("Failed to deserialize linucb for transfer: {e}"))?;
            let mut bandit = LinUCBBandit::new();
            bandit.import(&data.arms);
            bandit.import_total_pulls(data.total_pulls);
            bandit.set_alpha(data.alpha);
            *self = TransferLinUCB::with_bandit(bandit, self.context_similarity());
        }
        Ok(())
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)] // test vecs asserted non-empty before indexing
    use super::*;
    use ndarray::Array1;
    // Tier A (2026-04-19): named import of the pretty_assertions
    // shadow-macros from the crate-wide test prelude.
    use crate::rl::test_util::assert_eq;

    fn warm_donor() -> LinUCBBandit {
        use ndarray::Array1;
        let mut donor = LinUCBBandit::new();
        // Use non-zero features so b accumulates real reward signal
        // feature vector: all 0.5 (uniform, non-zero)
        let features = Array1::from_elem(FEATURE_DIM, 0.5f64);
        for arm in 0..NUM_ARMS {
            // Multiple updates per arm to build up b and modify a_inv significantly
            for _ in 0..5 {
                donor.update(arm, &features, 1.0);
            }
        }
        donor
    }

    #[test]
    fn test_transfer_changes_parameters() {
        let donor = warm_donor();
        let donor_data = donor.export();

        let mut transfer = TransferLinUCB::new(1.0);
        let before_data = transfer.bandit().export();

        transfer.transfer_from(&donor, 1.0);
        let after_data = transfer.bandit().export();

        // At least one arm's parameters should have changed
        let changed = after_data.iter().zip(before_data.iter()).any(
            |((_, _, a_after, b_after), (_, _, a_before, b_before))| {
                a_after != a_before || b_after != b_before
            },
        );
        assert!(changed, "Transfer should modify bandit parameters");

        // Verify blend: after should be between before and donor
        // For b[0]: after = (1 - 0.3) * before + 0.3 * donor
        let arm0_before = &before_data[0];
        let arm0_donor = &donor_data[0];
        let arm0_after = &after_data[0];

        let expected_b0 = (1.0 - 0.3) * arm0_before.3[0] + 0.3 * arm0_donor.3[0];
        assert!(
            (arm0_after.3[0] - expected_b0).abs() < 1e-10,
            "Blended b[0] should follow formula: expected {}, got {}",
            expected_b0,
            arm0_after.3[0]
        );
    }

    #[test]
    fn test_zero_similarity_no_transfer() {
        let donor = warm_donor();
        let mut transfer = TransferLinUCB::new(0.0);
        let before_data = transfer.bandit().export();

        transfer.transfer_from(&donor, 0.0);
        let after_data = transfer.bandit().export();

        // Zero similarity: no change expected
        for (before, after) in before_data.iter().zip(after_data.iter()) {
            assert_eq!(
                before.2, after.2,
                "A_inv should not change with zero similarity"
            );
            assert_eq!(
                before.3, after.3,
                "b should not change with zero similarity"
            );
        }
    }

    #[test]
    fn test_blend_weight_capped_at_30_percent() {
        let donor = warm_donor();
        let donor_data = donor.export();

        let mut transfer_high = TransferLinUCB::new(1.0);
        let before = transfer_high.bandit().export();
        transfer_high.transfer_from(&donor, 1.0); // similarity=1.0 → blend_weight=0.3
        let after = transfer_high.bandit().export();

        // Verify max 30% blend applied
        for arm_idx in 0..NUM_ARMS {
            let expected_b0 = (1.0 - 0.3) * before[arm_idx].3[0] + 0.3 * donor_data[arm_idx].3[0];
            assert!(
                (after[arm_idx].3[0] - expected_b0).abs() < 1e-10,
                "Arm {}: blend weight should be capped at 30%",
                arm_idx
            );
        }
    }

    #[test]
    fn test_context_similarity_clamped() {
        let t1 = TransferLinUCB::new(-0.5);
        assert_eq!(t1.context_similarity(), 0.0);

        let t2 = TransferLinUCB::new(1.5);
        assert_eq!(t2.context_similarity(), 1.0);

        let t3 = TransferLinUCB::new(0.7);
        assert!((t3.context_similarity() - 0.7).abs() < 1e-10);
    }

    #[test]
    fn test_select_arm_works_after_transfer() {
        let donor = warm_donor();
        let mut transfer = TransferLinUCB::new(0.8);
        transfer.transfer_from(&donor, 0.8);

        // Should be able to select and update after transfer
        let features = Array1::zeros(FEATURE_DIM);
        let (arm_idx, score) = transfer.bandit_mut().select_arm(&features);
        assert!(arm_idx < NUM_ARMS, "Arm index should be valid");
        assert!(score.is_finite(), "UCB score should be finite");
    }

    #[test]
    fn test_into_bandit() {
        let transfer = TransferLinUCB::new(0.5);
        let bandit = transfer.into_bandit();
        assert_eq!(bandit.total_pulls(), 0);
    }
}
