//! TierManager — tier promotion and demotion for RLM memory entries.
//!
//! Memory entries live in one of four tiers (Ephemeral → Working → Reference → Core).
//! [`TierManager`] decides when an entry should move up (promotion) or down (demotion)
//! based on access frequency and recency.
//!
//! # Tier Ladder
//!
//! ```text
//! Ephemeral  →  Working  →  Reference  →  Core
//!    ↑ promote on high access_count         ↑
//!    ↓ demote on long absence               ↓
//! ```
//!
//! # Usage
//!
//! ```rust
//! use touring_intelligence::rl::memory::tier_manager::{TierManager, TierLevel};
//!
//! let mgr = TierManager::default();
//! let entry_tier = TierLevel::Working;
//!
//! if mgr.should_promote(entry_tier, /*access_count=*/ 15) {
//!     // move entry to Reference tier
//! }
//! if mgr.should_demote(entry_tier, /*days_idle=*/ 35) {
//!     // move entry back to Ephemeral tier
//! }
//! ```

/// The four tiers of the Touring memory ladder.
///
/// Maps 1:1 onto `crate::memory::rlm::MemoryTier` but is expressed
/// as an ordered enum so the promotion/demotion direction is unambiguous.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum TierLevel {
    /// Transient entries — not worth persisting long-term.
    Ephemeral = 0,
    /// Active working set — frequently accessed during a session.
    Working = 1,
    /// Durable reference material — accessed across sessions.
    Reference = 2,
    /// Core knowledge — canonical, almost never evicted.
    Core = 3,
}

impl TierLevel {
    /// Return the next tier up (`None` if already at `Core`).
    pub fn promoted(self) -> Option<Self> {
        match self {
            Self::Ephemeral => Some(Self::Working),
            Self::Working => Some(Self::Reference),
            Self::Reference => Some(Self::Core),
            Self::Core => None,
        }
    }

    /// Return the next tier down (`None` if already at `Ephemeral`).
    pub fn demoted(self) -> Option<Self> {
        match self {
            Self::Core => Some(Self::Reference),
            Self::Reference => Some(Self::Working),
            Self::Working => Some(Self::Ephemeral),
            Self::Ephemeral => None,
        }
    }

    /// Human-readable label.
    pub fn label(self) -> &'static str {
        match self {
            Self::Ephemeral => "ephemeral",
            Self::Working => "working",
            Self::Reference => "reference",
            Self::Core => "core",
        }
    }
}

/// Decision returned by [`TierManager::evaluate`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TierDecision {
    /// Entry should move to a higher tier.
    Promote(TierLevel),
    /// Entry should move to a lower tier.
    Demote(TierLevel),
    /// No change needed.
    Keep,
}

/// Configuration for tier promotion and demotion thresholds.
///
/// | Threshold | Default | Meaning |
/// |-----------|---------|---------|
/// | `promotion_access` | 10 | `access_count` needed to promote |
/// | `demotion_days_idle` | 30 | Days without access before demotion |
#[derive(Debug, Clone)]
pub struct TierManager {
    /// Minimum access count required to promote an entry.
    pub promotion_access: u32,
    /// Number of idle days before an entry is demoted.
    pub demotion_days_idle: u32,
}

impl Default for TierManager {
    fn default() -> Self {
        Self {
            promotion_access: 10,
            demotion_days_idle: 30,
        }
    }
}

impl TierManager {
    /// Create a manager with custom thresholds.
    pub fn new(promotion_access: u32, demotion_days_idle: u32) -> Self {
        Self {
            promotion_access,
            demotion_days_idle,
        }
    }

    /// Returns `true` if `access_count` qualifies `tier` for promotion.
    ///
    /// `Core` entries are never promoted (already at the top).
    pub fn should_promote(&self, tier: TierLevel, access_count: u32) -> bool {
        tier != TierLevel::Core && access_count >= self.promotion_access
    }

    /// Returns `true` if `days_idle` qualifies `tier` for demotion.
    ///
    /// `Ephemeral` entries are never demoted (already at the bottom).
    /// `Core` entries are never demoted — they are canonical.
    pub fn should_demote(&self, tier: TierLevel, days_idle: u32) -> bool {
        matches!(tier, TierLevel::Working | TierLevel::Reference)
            && days_idle >= self.demotion_days_idle
    }

    /// Evaluate an entry and return the recommended [`TierDecision`].
    ///
    /// Promotion takes priority over demotion when both conditions are met.
    pub fn evaluate(&self, tier: TierLevel, access_count: u32, days_idle: u32) -> TierDecision {
        if self.should_promote(tier, access_count) {
            if let Some(next) = tier.promoted() {
                return TierDecision::Promote(next);
            }
        }
        if self.should_demote(tier, days_idle) {
            if let Some(prev) = tier.demoted() {
                return TierDecision::Demote(prev);
            }
        }
        TierDecision::Keep
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn promotion_ladder_is_ordered() {
        assert_eq!(TierLevel::Ephemeral.promoted(), Some(TierLevel::Working));
        assert_eq!(TierLevel::Working.promoted(), Some(TierLevel::Reference));
        assert_eq!(TierLevel::Reference.promoted(), Some(TierLevel::Core));
        assert_eq!(TierLevel::Core.promoted(), None);
    }

    #[test]
    fn demotion_ladder_is_ordered() {
        assert_eq!(TierLevel::Core.demoted(), Some(TierLevel::Reference));
        assert_eq!(TierLevel::Reference.demoted(), Some(TierLevel::Working));
        assert_eq!(TierLevel::Working.demoted(), Some(TierLevel::Ephemeral));
        assert_eq!(TierLevel::Ephemeral.demoted(), None);
    }

    #[test]
    fn should_promote_at_threshold() {
        let mgr = TierManager::new(5, 30);
        assert!(!mgr.should_promote(TierLevel::Working, 4));
        assert!(mgr.should_promote(TierLevel::Working, 5));
        assert!(mgr.should_promote(TierLevel::Working, 100));
    }

    #[test]
    fn core_never_promoted() {
        let mgr = TierManager::default();
        assert!(!mgr.should_promote(TierLevel::Core, 9999));
    }

    #[test]
    fn should_demote_after_idle() {
        let mgr = TierManager::new(10, 30);
        assert!(!mgr.should_demote(TierLevel::Working, 29));
        assert!(mgr.should_demote(TierLevel::Working, 30));
        assert!(mgr.should_demote(TierLevel::Reference, 31));
    }

    #[test]
    fn ephemeral_and_core_not_demoted() {
        let mgr = TierManager::default();
        assert!(!mgr.should_demote(TierLevel::Ephemeral, 9999));
        assert!(!mgr.should_demote(TierLevel::Core, 9999));
    }

    #[test]
    fn evaluate_returns_promote_when_eligible() {
        let mgr = TierManager::new(5, 30);
        let decision = mgr.evaluate(TierLevel::Working, 10, 5);
        assert_eq!(decision, TierDecision::Promote(TierLevel::Reference));
    }

    #[test]
    fn evaluate_returns_demote_when_idle() {
        let mgr = TierManager::new(5, 30);
        let decision = mgr.evaluate(TierLevel::Reference, 1, 45);
        assert_eq!(decision, TierDecision::Demote(TierLevel::Working));
    }

    #[test]
    fn evaluate_promote_takes_priority_over_demote() {
        // access_count >= threshold AND days_idle >= threshold simultaneously
        let mgr = TierManager::new(5, 30);
        let decision = mgr.evaluate(TierLevel::Working, 10, 45);
        // Promote wins
        assert_eq!(decision, TierDecision::Promote(TierLevel::Reference));
    }

    #[test]
    fn evaluate_keep_when_nothing_triggers() {
        let mgr = TierManager::new(10, 30);
        let decision = mgr.evaluate(TierLevel::Working, 3, 5);
        assert_eq!(decision, TierDecision::Keep);
    }

    #[test]
    fn tier_labels_are_non_empty() {
        for t in [
            TierLevel::Ephemeral,
            TierLevel::Working,
            TierLevel::Reference,
            TierLevel::Core,
        ] {
            assert!(!t.label().is_empty());
        }
    }
}
