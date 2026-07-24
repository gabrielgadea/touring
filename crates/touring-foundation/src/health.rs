//! Composite system-health scoring — single shared implementation
//! consumed by both `touring status` (CLI dashboard) and the
//! `instructions_loaded` hook (session-start health surfacing).
//!
//! Wave 9 S8 (2026-04-26) — moved from `touring-server/src/cli/status.rs`
//! to `touring-foundation` so the same logic can be invoked from any crate
//! in the workspace without circular deps. The CLI status path
//! re-exports this function to preserve backward compatibility with
//! existing callers and tests.
//!
//! # Score model
//!
//! Weighted average ∈ [0.0, 1.0] across 5 dimensions:
//!
//! - **daemon_health** (30%): `healthy_count / total_count`
//! - **orphan_ratio** (20%): `1.0 - clamp(orphan_count / total_pub_symbols, 0, 1)`
//! - **regression_streak** (20%): `1.0 / (1.0 + outstanding)`
//! - **cache_hit_ratio** (15%): from `gate_metrics.query_cache_hit_ratio`
//! - **ema_reward** (15%): clamp `learning.ema_reward` to `[0, 1]`
//!
//! Missing fields contribute neutral 0.5 for that dimension. A score
//! of 1.0 indicates all subsystems at healthy baseline; below 0.5
//! indicates degradation that warrants caller attention.

use serde_json::{Map, Value};

/// Compute the composite health score from an aggregated subsystem map.
///
/// The input shape mirrors the JSON produced by `touring status -j`:
///
/// ```json
/// {
///   "daemon_health":  { "healthy_count": N, "total_count": M },
///   "wiring":         { "orphan_count": N, "total_pub_symbols": M },
///   "health_delta":   { "outstanding": N },
///   "gate_metrics":   { "query_cache_hit_ratio": F },
///   "learning":       { "ema_reward": F }
/// }
/// ```
///
/// Any missing field contributes a neutral 0.5 to its dimension. The
/// final score is rounded to 4 decimal places for stable JSON output.
#[must_use]
pub fn compute_composite_health_score(combined: &Map<String, Value>) -> f64 {
    fn extract_f64(v: &Value, path: &[&str]) -> Option<f64> {
        let mut cur = v;
        for key in path {
            cur = cur.get(key)?;
        }
        cur.as_f64()
    }
    fn extract_u64(v: &Value, path: &[&str]) -> Option<u64> {
        let mut cur = v;
        for key in path {
            cur = cur.get(key)?;
        }
        cur.as_u64()
    }

    let neutral = 0.5_f64;

    // Dimension 1: daemon health (30%)
    let daemon_score = combined
        .get("daemon_health")
        .and_then(|v| {
            let healthy = extract_u64(v, &["healthy_count"])?;
            let total = extract_u64(v, &["total_count"]).filter(|n| *n > 0)?;
            #[allow(clippy::cast_precision_loss)]
            Some((healthy as f64) / (total as f64))
        })
        .unwrap_or(neutral);

    // Dimension 2: orphan ratio (20%) — lower is better.
    let orphan_score = combined
        .get("wiring")
        .and_then(|v| {
            let orphans = extract_u64(v, &["orphan_count"])?;
            let total = extract_u64(v, &["total_pub_symbols"]).filter(|n| *n > 0)?;
            #[allow(clippy::cast_precision_loss)]
            let ratio = (orphans as f64) / (total as f64);
            Some((1.0 - ratio).clamp(0.0, 1.0))
        })
        .unwrap_or(neutral);

    // Dimension 3: regression streak (20%) — lower outstanding is better.
    let streak_score = combined
        .get("health_delta")
        .and_then(|v| {
            let outstanding = extract_u64(v, &["outstanding"])?;
            #[allow(clippy::cast_precision_loss)]
            Some(1.0 / (1.0 + outstanding as f64))
        })
        .unwrap_or(neutral);

    // Dimension 4: cache hit ratio (15%).
    let cache_score = combined
        .get("gate_metrics")
        .and_then(|v| extract_f64(v, &["query_cache_hit_ratio"]))
        .map(|r| r.clamp(0.0, 1.0))
        .unwrap_or(neutral);

    // Dimension 5: EMA reward (15%) — already in [0, 1] by construction.
    let reward_score = combined
        .get("learning")
        .and_then(|v| extract_f64(v, &["ema_reward"]))
        .map(|r| r.clamp(0.0, 1.0))
        .unwrap_or(neutral);

    let composite = 0.30 * daemon_score
        + 0.20 * orphan_score
        + 0.20 * streak_score
        + 0.15 * cache_score
        + 0.15 * reward_score;

    // Round to 4 decimal places for stable JSON output.
    (composite * 10_000.0).round() / 10_000.0
}

/// Threshold at which the score is considered degraded enough to
/// surface a warning to humans. Below this value, callers (e.g.
/// `instructions_loaded`) should inject a hint advising the operator
/// to review `touring status -j` before performing risky edits.
pub const DEGRADED_SCORE_THRESHOLD: f64 = 0.5;

/// If `score` is below [`DEGRADED_SCORE_THRESHOLD`], return a one-line
/// human-readable warning suitable for prompt injection. Otherwise
/// return `None` so callers can skip the surface.
///
/// Wave 9 S8 (2026-04-26) — used by the `instructions_loaded` hook
/// to make `touring status -j composite_health_score` actionable at
/// session start without forcing the operator to run a separate CLI
/// command.
#[must_use]
pub fn compose_degraded_warning(score: f64) -> Option<String> {
    if score >= DEGRADED_SCORE_THRESHOLD || !score.is_finite() {
        return None;
    }
    Some(format!(
        "⚠ system health degraded (score={score:.2}); review `touring status -j` \
         before risky edits — Wave 8 S3 composite signal indicates regression"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn empty_map_returns_neutral_score() {
        let map = Map::new();
        let score = compute_composite_health_score(&map);
        // All dimensions neutral 0.5 → weighted total 0.5.
        assert!((score - 0.5).abs() < 1e-9);
    }

    #[test]
    fn perfect_health_returns_one() {
        let mut map = Map::new();
        map.insert(
            "daemon_health".to_string(),
            json!({"healthy_count": 8, "total_count": 8}),
        );
        map.insert(
            "wiring".to_string(),
            json!({"orphan_count": 0, "total_pub_symbols": 100}),
        );
        map.insert("health_delta".to_string(), json!({"outstanding": 0}));
        map.insert(
            "gate_metrics".to_string(),
            json!({"query_cache_hit_ratio": 1.0}),
        );
        map.insert("learning".to_string(), json!({"ema_reward": 1.0}));
        let score = compute_composite_health_score(&map);
        assert!((score - 1.0).abs() < 1e-9, "got {score}, expected ~1.0");
    }

    #[test]
    fn degraded_daemon_lowers_score() {
        let mut map = Map::new();
        map.insert(
            "daemon_health".to_string(),
            json!({"healthy_count": 4, "total_count": 8}),
        );
        let score = compute_composite_health_score(&map);
        // daemon=0.5*0.30=0.15; rest neutral 0.5*0.70=0.35; total 0.5.
        assert!((score - 0.5).abs() < 1e-9);
    }

    #[test]
    fn high_orphan_ratio_lowers_score() {
        let mut map = Map::new();
        map.insert(
            "wiring".to_string(),
            json!({"orphan_count": 90, "total_pub_symbols": 100}),
        );
        // orphan_score = 1.0 - 0.9 = 0.1; weighted 0.1*0.20=0.02
        // others neutral 0.5*0.80=0.40; total = 0.42
        let score = compute_composite_health_score(&map);
        assert!((score - 0.42).abs() < 1e-9, "got {score}, expected 0.42");
    }

    #[test]
    fn compose_degraded_warning_returns_none_for_healthy() {
        // Wave 9 S8 — at-or-above threshold = no warning.
        assert!(compose_degraded_warning(0.5).is_none());
        assert!(compose_degraded_warning(0.75).is_none());
        assert!(compose_degraded_warning(1.0).is_none());
    }

    #[test]
    fn compose_degraded_warning_returns_message_below_threshold() {
        // Wave 9 S8 — below threshold = single-line hint with score formatted.
        let warn = compose_degraded_warning(0.42).expect("expected warning below threshold");
        assert!(warn.contains("0.42"), "score must be formatted: {warn}");
        assert!(warn.contains("degraded"), "must mention degraded: {warn}");
        assert!(
            warn.contains("touring status"),
            "must reference status command: {warn}"
        );
    }

    #[test]
    fn compose_degraded_warning_handles_non_finite_gracefully() {
        // Wave 9 S8 — NaN / infinity must not panic; treat as no warning.
        assert!(compose_degraded_warning(f64::NAN).is_none());
        assert!(compose_degraded_warning(f64::INFINITY).is_none());
        assert!(compose_degraded_warning(f64::NEG_INFINITY).is_none());
    }
}
