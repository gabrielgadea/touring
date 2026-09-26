//! Synthetic sessions — the ids test harnesses mint, which must never teach the learner.
//!
//! One source for every consumer (2026-09-26). The S73 KPI already kept the analise
//! hook-contract suite's `hooks-contract-*` sessions out of daemon availability — with a
//! private constant — while session-start and session-stop kept persisting insights,
//! rewards, snapshots and bandit state for those same sessions: 69 synthetic
//! `session_insights_*.json` accumulated in that project, and the next real session loaded
//! one as its "prior". A filter declared in two places drifts; this is the only place.

/// Id prefixes of synthetic sessions. The analise contract suite mints
/// `hooks-contract-<uuid4>` (`tests/hooks_contract/_contract.py`).
pub const SYNTHETIC_SESSION_PREFIXES: &[&str] = &["hooks-contract-"];

/// `true` when `session_id` was minted by a test harness rather than by a real
/// Claude Code session — its events are observed, never learned from.
#[must_use]
pub fn is_synthetic_session_id(session_id: &str) -> bool {
    SYNTHETIC_SESSION_PREFIXES
        .iter()
        .any(|prefix| session_id.starts_with(prefix))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_contract_suite_session_is_synthetic() {
        assert!(is_synthetic_session_id("hooks-contract-4f1c2d"));
    }

    /// Negative controls: a real uuid, an empty id and a prefix that only appears
    /// in the middle are real sessions.
    #[test]
    fn real_and_lookalike_ids_are_not_synthetic() {
        for id in [
            "0b4d1f2e-9a3c-4e5f-8a7b-1c2d3e4f5a6b",
            "",
            "x-hooks-contract-1",
        ] {
            assert!(!is_synthetic_session_id(id), "{id:?}");
        }
    }
}
