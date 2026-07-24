//! Tier classification — 6-tier mapping from composite score.
//!
//! 0.95+ = 💎 Diamond
//! 0.90+ = 🥇 Platinum
//! 0.80+ = 🥈 Gold
//! 0.70+ = 🥉 Silver
//! 0.60+ = ⚪ Bronze
//! <0.60 = ⚫ Unranked

use serde::{Deserialize, Serialize};
use std::fmt;

/// Composite-score tier classification.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum Tier {
    /// 0.95..=1.0 — elite, BLOCK below on critical dims
    Diamond,
    /// 0.90..0.95 — strong, WARN below
    Platinum,
    /// 0.80..0.90 — adequate, default target
    Gold,
    /// 0.70..0.80 — work in progress
    Silver,
    /// 0.60..0.70 — needs attention
    Bronze,
    /// <0.60 — critical
    Unranked,
}

impl Tier {
    /// Lower bound of this tier's score range.
    pub fn lower_bound(&self) -> f32 {
        match self {
            Tier::Diamond => 0.95,
            Tier::Platinum => 0.90,
            Tier::Gold => 0.80,
            Tier::Silver => 0.70,
            Tier::Bronze => 0.60,
            Tier::Unranked => 0.0,
        }
    }

    /// Symbol used in compact output and badge.
    pub fn symbol(&self) -> &'static str {
        match self {
            Tier::Diamond => "💎",
            Tier::Platinum => "🥇",
            Tier::Gold => "🥈",
            Tier::Silver => "🥉",
            Tier::Bronze => "⚪",
            Tier::Unranked => "⚫",
        }
    }

    /// Short label (e.g. "Diamond", "Gold").
    pub fn label(&self) -> &'static str {
        match self {
            Tier::Diamond => "Diamond",
            Tier::Platinum => "Platinum",
            Tier::Gold => "Gold",
            Tier::Silver => "Silver",
            Tier::Bronze => "Bronze",
            Tier::Unranked => "Unranked",
        }
    }
}

impl fmt::Display for Tier {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.label())
    }
}

/// Map a composite score to a Tier.
pub fn tier_from_composite(composite: f32) -> Tier {
    let c = composite.clamp(0.0, 1.0);
    if c >= 0.95 {
        Tier::Diamond
    } else if c >= 0.90 {
        Tier::Platinum
    } else if c >= 0.80 {
        Tier::Gold
    } else if c >= 0.70 {
        Tier::Silver
    } else if c >= 0.60 {
        Tier::Bronze
    } else {
        Tier::Unranked
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tier_boundaries() {
        assert_eq!(tier_from_composite(1.0), Tier::Diamond);
        assert_eq!(tier_from_composite(0.95), Tier::Diamond);
        assert_eq!(tier_from_composite(0.949), Tier::Platinum);
        assert_eq!(tier_from_composite(0.90), Tier::Platinum);
        assert_eq!(tier_from_composite(0.899), Tier::Gold);
        assert_eq!(tier_from_composite(0.80), Tier::Gold);
        assert_eq!(tier_from_composite(0.799), Tier::Silver);
        assert_eq!(tier_from_composite(0.70), Tier::Silver);
        assert_eq!(tier_from_composite(0.699), Tier::Bronze);
        assert_eq!(tier_from_composite(0.60), Tier::Bronze);
        assert_eq!(tier_from_composite(0.599), Tier::Unranked);
        assert_eq!(tier_from_composite(0.0), Tier::Unranked);
    }

    #[test]
    fn test_tier_clamp() {
        assert_eq!(tier_from_composite(1.5), Tier::Diamond);
        assert_eq!(tier_from_composite(-0.5), Tier::Unranked);
    }

    #[test]
    fn test_tier_lower_bound() {
        assert!((Tier::Diamond.lower_bound() - 0.95).abs() < 1e-6);
        assert!((Tier::Unranked.lower_bound() - 0.0).abs() < 1e-6);
    }
}
