//! I-10 — Progressive throttling for ctx_* MCP tools.
//!
//! Replicates context-mode's call-tier model: tier 1 (1-3 calls/session)
//! passes through; tier 2 (4-8) reduces top_k + emits warning; tier 3 (9+)
//! redirects to batch mode. Per-session counters live in a moka::sync::Cache
//! with TTL 1h. Configurable via env: `TOURING_THROTTLE_TIER1_MAX` (default 3),
//! `TOURING_THROTTLE_TIER2_MAX` (default 8).
//!
//! Used by `touring_hooks::cli_handlers_mcp` to throttle ctx_search/ctx_index/etc.

use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use moka::sync::Cache;

/// Tier classification for a session's ctx_* call count.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ThrottleTier {
    /// 1-3 calls — pass-through (default top_k respected).
    Tier1,
    /// 4-8 calls — reduce top_k to <= 3 and emit warn.
    Tier2,
    /// 9+ calls — block and redirect to batch mode.
    Tier3,
}

impl ThrottleTier {
    /// Friendly label for JSON envelopes.
    pub fn label(self) -> &'static str {
        match self {
            ThrottleTier::Tier1 => "TIER1_NORMAL",
            ThrottleTier::Tier2 => "TIER2_REDUCED",
            ThrottleTier::Tier3 => "TIER3_BLOCKED",
        }
    }
}

fn tier1_max() -> u32 {
    std::env::var("TOURING_THROTTLE_TIER1_MAX")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(3)
}

fn tier2_max() -> u32 {
    std::env::var("TOURING_THROTTLE_TIER2_MAX")
        .ok()
        .and_then(|s| s.parse().ok())
        .unwrap_or(8)
}

/// Maps a count value to its tier. Pure function for testability.
pub fn tier_for(count: u32) -> ThrottleTier {
    if count <= tier1_max() {
        ThrottleTier::Tier1
    } else if count <= tier2_max() {
        ThrottleTier::Tier2
    } else {
        ThrottleTier::Tier3
    }
}

/// Per-session call counter with TTL.
pub struct ThrottleState {
    counters: Cache<String, Arc<AtomicU32>>,
}

impl ThrottleState {
    /// Build a state with capacity 10k sessions and TTL 1h.
    pub fn new() -> Self {
        Self {
            counters: Cache::builder()
                .max_capacity(10_000)
                .time_to_live(std::time::Duration::from_secs(3_600))
                .build(),
        }
    }

    /// Increment the call count for `session_id` and return the new tier.
    /// Atomic: all increments within one cache entry use the same AtomicU32.
    pub fn check_and_record(&self, session_id: &str) -> (u32, ThrottleTier) {
        let counter = self
            .counters
            .get_with(session_id.to_string(), || Arc::new(AtomicU32::new(0)));
        let prev = counter.fetch_add(1, Ordering::Relaxed);
        let count = prev + 1;
        (count, tier_for(count))
    }

    /// Reset count for one session (or all when `session_id` is None).
    pub fn reset(&self, session_id: Option<&str>) {
        match session_id {
            Some(id) => self.counters.invalidate(id),
            None => self.counters.invalidate_all(),
        }
    }

    /// Inspect the current count without incrementing. Returns 0 when absent.
    pub fn peek(&self, session_id: &str) -> u32 {
        self.counters
            .get(session_id)
            .map(|c| c.load(Ordering::Relaxed))
            .unwrap_or(0)
    }
}

impl Default for ThrottleState {
    fn default() -> Self {
        Self::new()
    }
}

/// Lazy global ThrottleState instance.
pub fn global() -> &'static ThrottleState {
    use std::sync::OnceLock;
    static INSTANCE: OnceLock<ThrottleState> = OnceLock::new();
    INSTANCE.get_or_init(ThrottleState::new)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tier_for_default_thresholds() {
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("TOURING_THROTTLE_TIER1_MAX") };
        // TODO: Audit that the environment access only happens in single-threaded code.
        unsafe { std::env::remove_var("TOURING_THROTTLE_TIER2_MAX") };
        assert_eq!(tier_for(1), ThrottleTier::Tier1);
        assert_eq!(tier_for(3), ThrottleTier::Tier1);
        assert_eq!(tier_for(4), ThrottleTier::Tier2);
        assert_eq!(tier_for(8), ThrottleTier::Tier2);
        assert_eq!(tier_for(9), ThrottleTier::Tier3);
        assert_eq!(tier_for(100), ThrottleTier::Tier3);
    }

    #[test]
    fn test_check_and_record_advances_count() {
        let s = ThrottleState::new();
        let sid = "test_session_advance";
        let (c1, t1) = s.check_and_record(sid);
        let (c2, t2) = s.check_and_record(sid);
        assert_eq!(c1, 1);
        assert_eq!(c2, 2);
        assert_eq!(t1, ThrottleTier::Tier1);
        assert_eq!(t2, ThrottleTier::Tier1);
    }

    #[test]
    fn test_per_session_isolation() {
        let s = ThrottleState::new();
        for _ in 0..5 {
            s.check_and_record("session_a");
        }
        let (count_b, tier_b) = s.check_and_record("session_b");
        assert_eq!(count_b, 1);
        assert_eq!(tier_b, ThrottleTier::Tier1);
    }

    #[test]
    fn test_reset_clears_count() {
        let s = ThrottleState::new();
        let sid = "test_reset";
        for _ in 0..3 {
            s.check_and_record(sid);
        }
        assert_eq!(s.peek(sid), 3);
        s.reset(Some(sid));
        assert_eq!(s.peek(sid), 0);
    }

    #[test]
    fn test_tier3_after_9_calls() {
        let s = ThrottleState::new();
        let sid = "test_tier3";
        let mut last_tier = ThrottleTier::Tier1;
        for _ in 0..9 {
            let (_c, t) = s.check_and_record(sid);
            last_tier = t;
        }
        assert_eq!(last_tier, ThrottleTier::Tier3);
    }

    #[test]
    fn test_tier_label_strings() {
        assert_eq!(ThrottleTier::Tier1.label(), "TIER1_NORMAL");
        assert_eq!(ThrottleTier::Tier2.label(), "TIER2_REDUCED");
        assert_eq!(ThrottleTier::Tier3.label(), "TIER3_BLOCKED");
    }
}
