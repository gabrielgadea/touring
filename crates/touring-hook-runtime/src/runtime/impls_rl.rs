//! LinUCB, PolymorphicBandit, OnlineRL, and Session implementations for HookRuntime.
//!
//! Provides RL operations for context strategy selection, polymorphic bandit management,
//! online reinforcement learning, and session turn tracking.

use touring_intelligence::rl::bandit::ContextualBandit;
use touring_intelligence::rl::bandit::linucb::ArmKind;
use touring_intelligence::rl::streaming_hook_integration::{HookQualitySummary, HookStatsConsumer};
use touring_intelligence::rl::{LinUCBBandit, OnlineRLEngine};

use super::traits::{LinUCBBanditOps, OnlineRLOps, PolymorphicBandit, Session};
use crate::runtime::HookRuntime;

/// Warm-load the LinUCB bandit from the per-project rkyv snapshot written at
/// actor shutdown (`save_linucb` → `.claude/data/linucb.rkyv`), falling back
/// to a fresh bandit when no snapshot exists or it fails to parse (fail-open).
///
/// Investigation 2026-07-01: the shutdown save ran on every graceful exit but
/// nothing ever loaded the file back — every daemon restart silently reset the
/// bandit to zero pulls. This is the load half of that pair (REGRA #0).
fn warm_or_new_linucb(project_root: &std::path::Path) -> LinUCBBandit {
    let path = project_root.join(".claude/data/linucb.rkyv");
    match LinUCBBandit::load_rkyv(&path) {
        Ok(bandit) => {
            tracing::debug!(path = %path.display(), "LinUCB warm-loaded from rkyv snapshot");
            bandit
        }
        Err(_) => LinUCBBandit::new(),
    }
}

impl LinUCBBanditOps for HookRuntime {
    fn linucb_bandit_mut(&mut self) -> &mut LinUCBBandit {
        if self.learning.linucb.is_none() {
            self.learning.linucb = Some(warm_or_new_linucb(&self.project_root));
        }
        // SAFETY: we just created it above
        self.learning
            .linucb
            .as_mut()
            .expect("linucb initialized above")
    }

    fn select_context_strategy(
        &mut self,
        file_type: &str,
        file_size: usize,
        session_turn: usize,
        recent_errors: usize,
        cila_level: usize,
    ) -> (ArmKind, f64) {
        let cila_u8 = (cila_level.min(255)) as u8;
        let features = touring_intelligence::rl::bandit::linucb::extract_features(
            file_type,
            file_size,
            session_turn,
            recent_errors,
            cila_u8,
        );
        let bandit = self.linucb_bandit_mut();
        let (arm_kind, score) = bandit.select_arm_kind(&features);
        // S4: Track last selected arm for SessionBus outcome correlation.
        self.learning.last_arm_selected = Some(arm_kind as u8);
        (arm_kind, score)
    }

    fn record_context_reward(
        &mut self,
        arm: usize,
        file_type: &str,
        file_size: usize,
        session_turn: usize,
        recent_errors: usize,
        cila_level: usize,
        reward: f64,
    ) {
        let cila_u8 = (cila_level.min(255)) as u8;
        let features = touring_intelligence::rl::bandit::linucb::extract_features(
            file_type,
            file_size,
            session_turn,
            recent_errors,
            cila_u8,
        );
        let bandit = self.linucb_bandit_mut();
        bandit.update(arm, &features, reward);
    }

    #[allow(clippy::too_many_arguments)]
    fn suggest_context_level(
        &mut self,
        file_type: &str,
        file_size: usize,
        session_turn: usize,
        recent_errors: usize,
        cila_level: usize,
    ) -> u8 {
        let (arm, _score) = self.select_context_strategy(
            file_type,
            file_size,
            session_turn,
            recent_errors,
            cila_level,
        );
        match arm {
            ArmKind::None => 0,
            ArmKind::Overview | ArmKind::Gotcha => 1,
            ArmKind::BlastRadius | ArmKind::Relations | ArmKind::OverviewGotcha => 2,
            ArmKind::OverviewBlastRadius | ArmKind::FullEnrichment => 3,
        }
    }

    fn save_linucb(&self) -> Result<(), String> {
        if let Some(ref bandit) = self.learning.linucb {
            let path = self.project_root.join(".claude/data/linucb.rkyv");
            bandit
                .save_rkyv(&path)
                .map_err(|e| format!("Failed to save linucb: {e}"))?;
        }
        Ok(())
    }
}

impl PolymorphicBandit for HookRuntime {
    fn get_bandit_mut(&mut self) -> &mut Box<dyn ContextualBandit> {
        if self.learning.bandit.is_none() {
            let linucb = self
                .learning
                .linucb
                .take()
                .unwrap_or_else(|| warm_or_new_linucb(&self.project_root));
            self.learning.bandit = Some(Box::new(linucb));
        }
        // SAFETY: we just created it above
        self.learning
            .bandit
            .as_mut()
            .expect("bandit initialized above")
    }

    fn save_bandit(&self) -> Result<(), String> {
        if let Some(ref bandit) = self.learning.bandit {
            let snapshot = bandit.export_snapshot();
            let path = self.project_root.join(".claude/data/bandit_snapshot.json");
            let json = serde_json::to_string(&snapshot)
                .map_err(|e| format!("Failed to serialize bandit snapshot: {e}"))?;
            std::fs::write(&path, json)
                .map_err(|e| format!("Failed to write bandit snapshot: {e}"))?;
        }
        Ok(())
    }
}

impl OnlineRLOps for HookRuntime {
    fn process_immediate_reward(&mut self, reward: &touring_intelligence::rl::ImmediateReward) {
        if self.learning.linucb.is_none() {
            self.learning.linucb = Some(warm_or_new_linucb(&self.project_root));
        }
        if let Some(mut engine) = self.learning.online_rl.take() {
            if let Some(ref mut linucb) = self.learning.linucb {
                let mut qtable = self.learning.qtable_cache.take().unwrap_or_default();
                engine.process_reward(reward, &mut qtable, linucb);
                self.learning.qtable_cache = Some(qtable);
            }
            self.learning.online_rl = Some(engine);
        }
    }

    fn online_rl_engine(&self) -> Option<&OnlineRLEngine> {
        self.learning.online_rl.as_ref()
    }
}

impl Session for HookRuntime {
    fn session_turn(&self) -> usize {
        self.session_turn.load(std::sync::atomic::Ordering::Relaxed)
    }

    fn advance_session_turn(&self) -> usize {
        self.session_turn
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed)
            + 1
    }
}

impl HookStatsConsumer for HookRuntime {
    fn consume_hook_quality(&mut self, summary: HookQualitySummary) {
        // Compute reward from composite_score (0.0-1.0 scale)
        let reward = summary.composite_score;
        // success_rate > 0.5 indicates overall hook quality is acceptable
        let accepted = summary.success_rate > 0.5;
        let error_count = if accepted { 0 } else { 1 };

        let q = touring_intelligence::rl::ImmediateReward {
            tool_name: "HookQuality".to_string(),
            accepted,
            latency_ms: summary.avg_latency_ms as u64,
            error_count,
            cila_level: 0, // HookQualitySummary doesn't expose cila_level
            file_type: Self::detect_file_type_from_ext(&summary),
            quality_score: Some(reward),
        };
        // Process via existing OnlineRLOps implementation
        // Use explicit trait call to avoid resolution to HookRuntime's inherent 2-arg method
        OnlineRLOps::process_immediate_reward(self, &q);
        // Increment counter
        self.learning.hook_quality_assessments_consumed += 1;
    }

    fn assessments_consumed(&self) -> u64 {
        self.learning.hook_quality_assessments_consumed
    }
}

impl HookRuntime {
    /// Detect file type index from HookQualitySummary fields (best-effort).
    /// Returns: 0=python, 1=rust, 2=typescript, 3=other.
    ///
    /// NOTE: HookQualitySummary does not carry a file_type field, so this
    /// heuristic uses dimension scores as a proxy. High dimension scores
    /// (precision, coverage, reliability, security) suggest well-structured
    /// languages (rust/typescript) → return 3 (other/high-quality).
    /// Low scores → default to 3 since we cannot reliably distinguish.
    fn detect_file_type_from_ext(summary: &HookQualitySummary) -> u8 {
        let avg_dim_score = (summary.precision_score
            + summary.coverage_score
            + summary.latency_score
            + summary.knowledge_score
            + summary.context_score
            + summary.reliability_score
            + summary.integration_score
            + summary.security_score
            + summary.evolution_score)
            / 9.0;
        // Without an explicit file_type field in HookQualitySummary, we cannot
        // accurately map dimension scores to language types. Return 3 (other)
        // as the conservative default; the LinUCB bandit will still learn
        // from the reward signal even without correct file-type context.
        let _ = avg_dim_score;
        3
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Investigation 2026-07-01 (the load half of the shutdown-save pair,
    /// REGRA #0): the bandit saved at actor shutdown must be restored — not
    /// silently replaced by a fresh one — on the next runtime boot; a
    /// missing or corrupt snapshot must fail open to a fresh bandit.
    #[test]
    fn warm_or_new_linucb_restores_pulls_and_fails_open() {
        let root = std::env::temp_dir().join(format!("linucb-warm-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&root);

        // (a) No snapshot → fresh bandit (fail-open).
        let fresh = warm_or_new_linucb(&root);
        assert_eq!(
            fresh.total_pulls(),
            0,
            "no snapshot must yield a fresh bandit"
        );

        // (b) Round-trip via the SAME path convention the shutdown save uses:
        // a trained bandit is restored with its pulls intact.
        let mut trained = LinUCBBandit::new();
        let features =
            touring_intelligence::rl::bandit::linucb::extract_features("rs", 1000, 1, 0, 2);
        trained.update(0, &features, 1.0);
        assert!(trained.total_pulls() > 0, "training must register a pull");
        let path = root.join(".claude/data/linucb.rkyv");
        std::fs::create_dir_all(path.parent().expect("snapshot parent dir")).expect("mkdir");
        trained.save_rkyv(&path).expect("save_rkyv");
        let restored = warm_or_new_linucb(&root);
        assert_eq!(
            restored.total_pulls(),
            trained.total_pulls(),
            "snapshot pulls must survive the reload"
        );

        // (c) Corrupt snapshot → fresh bandit, no panic (fail-open).
        std::fs::write(&path, b"not-an-rkyv-snapshot").expect("write garbage");
        let fallback = warm_or_new_linucb(&root);
        assert_eq!(fallback.total_pulls(), 0, "corrupt snapshot must fail open");

        let _ = std::fs::remove_dir_all(&root);
    }
}
