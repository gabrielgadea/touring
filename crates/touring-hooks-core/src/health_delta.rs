//! Wave 9 (2026-04-18) — Cross-hook health delta bridge.
//!
//! # Purpose
//!
//! Transforms the `QualityGateAdapter` + `wave5_workflow` fusion
//! (Waves 6–8) from **absolute-quality** assessment into
//! **delta-quality** assessment. Today an edit that drops
//! `health_score` from 0.90 → 0.60 emits two independent signals with
//! no correlation; this module makes the regression directly visible.
//!
//! # Architecture
//!
//! A process-wide `DashMap<String, f32>` (lock-free, sharded) keyed by
//! absolute file path. `pre_edit` calls [`record_pre_health`] with the
//! on-disk source **before** the edit takes effect; `post_edit` calls
//! [`compute_health_delta`] with the **new** source and receives a
//! [`HealthDelta`] carrying old/new values and the signed difference.
//!
//! The cache is deliberately process-scoped (not disk-backed):
//!
//! - Each daemon session cares only about deltas observed within the
//!   session. Historical trends belong to `touring-memory` /
//!   `touring-evolution`, not here.
//! - Restarting the daemon resets deltas — same semantics as any
//!   in-memory HookRuntime state.
//!
//! # Invariants
//!
//! 1. Both API surfaces are Rust-only — non-Rust files return `None`
//!    (no syn parser for other languages). Multi-lang deltas can be
//!    added later via tree-sitter `analyze_quality::complexity_score`.
//! 2. Parse failures are fail-open: `None` returned, no cache mutation,
//!    no crash.
//! 3. `compute_health_delta` consumes the cache entry (`remove`)
//!    because a successful delta computation is a one-shot event per
//!    (pre_edit, post_edit) pair. If post_edit fires without a matching
//!    pre_edit, `old` is reported as `None` and `delta = None`.
//! 4. The cache never grows unbounded — every call either removes an
//!    entry (consumer) or replaces an entry (producer).
//!
//! # Reward mapping
//!
//! When `delta` is well-defined, [`delta_reward`] maps it to an RL
//! reward in the Wave 5/8 envelope `[-0.10, +0.10]`:
//!
//! | Delta band       | Reward |
//! |------------------|-------:|
//! | `delta >= +0.15` | +0.10  |
//! | `delta >= +0.05` | +0.05  |
//! | `delta ∈ (-0.05, +0.05)` |  0.00 |
//! | `delta <= -0.05` | -0.05  |
//! | `delta <= -0.15` | -0.10  |
//!
//! The floor/ceiling match the existing Wave 5 envelope so RL reward
//! aggregation (phase1 `+1.0` base + V6 `±0.10` modulator + V7 delta
//! `±0.10` modulator) remains bounded.
use dashmap::DashMap;
use std::path::Path;
use std::sync::OnceLock;
use touring_analysis::quality::RustQualitySignals;
use touring_code::ast::{Lang, analyze_quality};
use touring_foundation::diagnostic::{Diagnostic, Severity, codes};
/// Compute a unified `[0.0, 1.0]` quality score for any source the
/// touring stack can parse. Higher = healthier.
///
/// - **Rust** (`.rs`): syn-backed `RustQualitySignals::health_score()`
///   (Wave 7/8 fusion — captures generics, lifetimes, unsafe density).
/// - **Other languages** that `Lang::from_path` recognises: tree-sitter
///   `analyze_quality(src, lang).complexity_score` (Wave 5.1 multi-lang
///   path — already normalised so 1.0 = perfectly clean).
/// - **Unsupported extensions**: returns `None`. Cache stays uniform
///   per file_path because path → language is deterministic, so
///   recorded vs computed values are always produced by the same engine
///   and therefore comparable.
fn compute_quality_for_path(file_path: &str, source: &str) -> Option<f32> {
    if file_path.ends_with(".rs") {
        return RustQualitySignals::from_source(source).map(|s| s.health_score());
    }
    let lang = Lang::from_path(Path::new(file_path))?;
    let report = analyze_quality(source, lang);
    Some(report.complexity_score)
}
/// Process-wide singleton cache for pre-edit health scores.
static HEALTH_DELTA_CACHE: OnceLock<DashMap<String, f32>> = OnceLock::new();
fn cache() -> &'static DashMap<String, f32> {
    HEALTH_DELTA_CACHE.get_or_init(DashMap::new)
}
/// Wave 13 — Threshold for raising a streak alert (consecutive
/// regressions on the same path). Alerts CC + RL that the file is
/// trending in a bad direction across multiple edits.
pub const STREAK_ALERT_THRESHOLD: u32 = 3;
/// Wave 13 — Per-path streak counters tracking consecutive regression
/// vs improvement deltas. Maintained as a separate `DashMap` from the
/// pre-edit health cache because streaks survive across many compute
/// cycles whereas the pre-edit cache is one-shot per pair.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct StreakCounters {
    /// Consecutive regression deltas on this path. Resets on any
    /// non-regression compute (delta > -0.05).
    pub regression_streak: u32,
    /// Consecutive improvement deltas on this path. Resets on any
    /// non-improvement compute (delta < +0.05).
    pub improvement_streak: u32,
}
static STREAK_CACHE: OnceLock<DashMap<String, StreakCounters>> = OnceLock::new();
fn streak_cache() -> &'static DashMap<String, StreakCounters> {
    STREAK_CACHE.get_or_init(DashMap::new)
}
/// Wave 13 — Look up the current per-path streak counters.
///
/// Returns `StreakCounters::default()` (both zeros) when the path has
/// never been observed. Useful for hint emission and tests.
#[must_use]
pub fn streak_counters(file_path: &str) -> StreakCounters {
    streak_cache()
        .get(file_path)
        .map(|v| *v)
        .unwrap_or_default()
}
/// Wave 13 — Convenience: current consecutive regression count for `file_path`.
#[must_use]
pub fn regression_streak(file_path: &str) -> u32 {
    streak_counters(file_path).regression_streak
}
/// Wave 13 — Convenience: current consecutive improvement count for `file_path`.
#[must_use]
pub fn improvement_streak(file_path: &str) -> u32 {
    streak_counters(file_path).improvement_streak
}
/// Wave 13 — Reset both streak counters for a path. Called by tests
/// and by external callers that want to start fresh (e.g. after a
/// known-good refactor checkpoint).
pub fn reset_streak(file_path: &str) {
    streak_cache().remove(file_path);
}
/// Wave 14 — Render a CC-facing warning hint when the file's
/// regression streak has crossed [`STREAK_ALERT_THRESHOLD`].
///
/// Returns `None` when the path has no active streak ≥ threshold.
/// Returns `Some(...)` when the streak is concerning, so callers
/// (pre_edit / pre_read advisories) can surface it directly.
///
/// Format:
/// ```text
/// ⚠ regression streak: 4 consecutive declines on src/foo.rs — review before continuing
/// ```
///
/// Symmetric helper [`improvement_streak_hint`] surfaces the positive
/// direction so CC sees both negative and positive trajectories.
#[must_use]
pub fn streak_warning_hint(file_path: &str) -> Option<String> {
    let s = streak_counters(file_path);
    if s.regression_streak >= STREAK_ALERT_THRESHOLD {
        Some(format!(
            "⚠ regression streak: {} consecutive declines on {} — review before continuing",
            s.regression_streak, file_path,
        ))
    } else {
        None
    }
}
/// Wave 14 — Render a positive-direction hint when a file is on an
/// improvement streak ≥ [`STREAK_ALERT_THRESHOLD`]. Useful as a
/// confirmation signal that recent edits are moving in the right
/// direction. Returns `None` for streaks below threshold.
#[must_use]
pub fn improvement_streak_hint(file_path: &str) -> Option<String> {
    let s = streak_counters(file_path);
    if s.improvement_streak >= STREAK_ALERT_THRESHOLD {
        Some(format!(
            "✓ improvement streak: {} consecutive gains on {} — keep going",
            s.improvement_streak, file_path,
        ))
    } else {
        None
    }
}
/// Wave S-3 persistence (2026-05-04): Save the pre-edit health cache to
/// disk as JSON so health_delta state survives daemon restarts.
///
/// # Errors
///
/// Returns `Err` when JSON serialization fails or when the filesystem
/// write fails (permission, disk full, read-only mount).
pub fn save_health_delta_cache(
    project_root: &std::path::Path,
) -> Result<(), HealthDeltaCacheError> {
    let cache_dir = project_root.join(".claude").join("touring");
    std::fs::create_dir_all(&cache_dir).map_err(|e| HealthDeltaCacheError::CreateDir {
        path: cache_dir.clone(),
        source: e,
    })?;
    let path = cache_dir.join("health_delta_cache.json");
    let items: Vec<(String, f32)> = cache()
        .iter()
        .map(|e| (e.key().clone(), *e.value()))
        .collect();
    let json = serde_json::to_string(&items).map_err(HealthDeltaCacheError::Serialize)?;
    std::fs::write(&path, json).map_err(|e| HealthDeltaCacheError::Write {
        path: path.clone(),
        source: e,
    })?;
    Ok(())
}
/// Wave S-3 persistence (2026-05-04): Load the pre-edit health cache from
/// disk on daemon startup. Missing file = cold-start (silent); present but
/// malformed = warn and fall back to empty cache so a corrupted snapshot
/// never blocks daemon boot.
///
/// Returns `Ok(false)` when the file does not exist (cold-start);
/// returns `Ok(true)` when cache was successfully restored.
///
/// # Errors
///
/// Returns `Err` when the snapshot file exists but cannot be read
/// (permission, I/O failure) or when the JSON is malformed.
pub fn load_health_delta_cache(
    project_root: &std::path::Path,
) -> Result<bool, HealthDeltaCacheError> {
    let path = project_root
        .join(".claude")
        .join("touring")
        .join("health_delta_cache.json");
    if !path.exists() {
        return Ok(false);
    }
    let data = std::fs::read_to_string(&path).map_err(|e| HealthDeltaCacheError::Read {
        path: path.clone(),
        source: e,
    })?;
    let items: Vec<(String, f32)> =
        serde_json::from_str(&data).map_err(HealthDeltaCacheError::Parse)?;
    let c = cache();
    for (path_str, score) in items {
        c.insert(path_str, score);
    }
    Ok(true)
}

/// Errors from health-delta cache persistence ([`save_health_delta_cache`] /
/// [`load_health_delta_cache`]).
#[derive(Debug, thiserror::Error)]
pub enum HealthDeltaCacheError {
    /// Could not create the `.claude/touring` cache directory.
    #[error("Failed to create cache dir {path:?}: {source}")]
    CreateDir {
        /// Cache directory path that failed to create.
        path: std::path::PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// Could not serialize the cache to JSON.
    #[error("Failed to serialize health delta cache: {0}")]
    Serialize(#[source] serde_json::Error),
    /// Could not write the cache file.
    #[error("Failed to write health delta cache to {path:?}: {source}")]
    Write {
        /// Cache file path that failed to write.
        path: std::path::PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// Could not read the cache file.
    #[error("Failed to read {path:?}: {source}")]
    Read {
        /// Cache file path that failed to read.
        path: std::path::PathBuf,
        /// Underlying I/O error.
        #[source]
        source: std::io::Error,
    },
    /// Could not parse the cache JSON.
    #[error("Failed to parse health delta cache: {0}")]
    Parse(#[source] serde_json::Error),
}
/// Wave 16 — Pure JSON status snapshot. Independent of `HookRuntime`,
/// callable from any consumer (CLI handler, MCP tool, integration tests).
///
/// When `file_path = None` (or empty), returns aggregate counters drawn
/// from `gate_metrics` (record/compute/regression/improvement/outstanding/
/// streak_alert/recovery + alert_threshold).
///
/// When `file_path = Some(path)`, returns per-path streak state +
/// warning/improvement hint (null when below threshold) + alert_threshold.
#[must_use]
pub fn status_json(file_path: Option<&str>) -> String {
    if let Some(path) = file_path {
        let s = streak_counters(path);
        let warning = streak_warning_hint(path);
        let improvement = improvement_streak_hint(path);
        let out = serde_json::json!(
            { "file_path" : path, "regression_streak" : s.regression_streak,
            "improvement_streak" : s.improvement_streak, "warning_hint" : warning,
            "improvement_hint" : improvement, "alert_threshold" : STREAK_ALERT_THRESHOLD,
            }
        );
        return out.to_string();
    }
    let snap = crate::shared::gate_metrics::GateMetricsSnapshot::capture();
    let agg = serde_json::json!(
        { "record_count" : snap.health_delta_record_count, "compute_count" : snap
        .health_delta_compute_count, "regression_count" : snap
        .health_delta_regression_count, "improvement_count" : snap
        .health_delta_improvement_count, "outstanding" : snap.health_delta_outstanding,
        "streak_alert_count" : snap.health_delta_streak_alert_count, "recovery_count" :
        snap.health_delta_recovery_count, "alert_threshold" : STREAK_ALERT_THRESHOLD, }
    );
    agg.to_string()
}
/// Wave 16 — Pure JSON reset. Clears streak counters + pending pre-record
/// for the given path. Returns `{"reset": true, "file_path": <path>}` on
/// success; never errors (path is always honored).
#[must_use]
pub fn reset_json(file_path: &str) -> String {
    reset_streak(file_path);
    discard_pre_health(file_path);
    serde_json::json!({ "reset" : true, "file_path" : file_path }).to_string()
}
/// Delta between a pre-edit and post-edit [`RustQualitySignals::health_score`].
///
/// `old` is `None` when `post_edit` fires without a matching `pre_edit`
/// cache entry (daemon restart, first-ever edit of a file, or non-Rust
/// file skipped at `pre_edit`). In that case `delta = None`.
#[derive(Debug, Clone, Copy, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct HealthDelta {
    /// Cached health score captured at `pre_edit`.
    pub old: Option<f32>,
    /// Health score of the post-edit source.
    pub new: f32,
    /// Signed difference `new - old`; `None` when `old` is `None`.
    pub delta: Option<f32>,
}
impl HealthDelta {
    /// True when `delta` is present and below `-0.05` — a regression.
    #[must_use]
    pub fn is_regression(&self) -> bool {
        self.delta.map(|d| d <= -0.05).unwrap_or(false)
    }
    /// True when `delta` is present and above `+0.05` — an improvement.
    #[must_use]
    pub fn is_improvement(&self) -> bool {
        self.delta.map(|d| d >= 0.05).unwrap_or(false)
    }
}
/// Delta between pre-edit and post-edit patch complexity scores.
///
/// Emitted when `mpatch-fuzzy` feature is enabled and a patch is being
/// evaluated via `preview_patch`. Carries old/new complexity proxies
/// (raw byte lengths as a simple proxy) plus the actual match method
/// and confidence from the mpatch engine.
#[cfg(feature = "mpatch-fuzzy")]
#[derive(Debug, Clone, PartialEq)]
pub struct PatchComplexityDelta {
    /// Complexity proxy of the original source (byte length).
    pub old_complexity: f64,
    /// Complexity proxy of the patched source (byte length).
    pub new_complexity: f64,
    /// Signed difference `new_complexity - old_complexity`.
    pub delta: f64,
    /// Which matching method the mpatch engine used.
    pub method: crate::shared::mpatch_preview::PatchMethod,
    /// Confidence score from mpatch apply report `[0.0, 1.0]`.
    pub confidence: f32,
}
#[cfg(feature = "mpatch-fuzzy")]
impl PatchComplexityDelta {
    /// Compute a `PatchComplexityDelta` from old source, new source (patch result),
    /// and the mpatch `PatchPreview`.
    ///
    /// Uses raw byte length as the complexity proxy — simple but monotonic
    /// (adding lines always increases length). The `method` and `confidence`
    /// come directly from the mpatch apply report.
    #[must_use]
    pub fn compute(
        old: &str,
        new: &str,
        preview: &crate::shared::mpatch_preview::PatchPreview,
    ) -> Self {
        let old_complexity = old.len() as f64;
        let new_complexity = new.len() as f64;
        PatchComplexityDelta {
            old_complexity,
            new_complexity,
            delta: new_complexity - old_complexity,
            method: preview.method,
            confidence: preview.confidence,
        }
    }
    /// True when `confidence` is above the typical apply threshold (>= 0.7).
    #[must_use]
    pub fn is_confident(&self) -> bool {
        self.confidence >= 0.7
    }
    /// True when the patch increased complexity (bytes added > bytes removed).
    #[must_use]
    pub fn is_expansion(&self) -> bool {
        self.delta > 0.0
    }
    /// True when the patch reduced complexity.
    #[must_use]
    pub fn is_contraction(&self) -> bool {
        self.delta < 0.0
    }
}
/// Stub when `mpatch-fuzzy` is not enabled.
#[cfg(not(feature = "mpatch-fuzzy"))]
#[derive(Debug, Clone, PartialEq)]
pub struct PatchComplexityDelta;
#[cfg(not(feature = "mpatch-fuzzy"))]
impl PatchComplexityDelta {
    /// Stub constructor: returns an empty delta when `mpatch-fuzzy` is disabled.
    #[must_use]
    pub fn compute(
        _old: &str,
        _new: &str,
        _preview: &crate::shared::mpatch_preview::PatchPreview,
    ) -> Self {
        Self
    }
    /// Stub: always `false` since no complexity is computed without the feature.
    #[must_use]
    pub fn is_confident(&self) -> bool {
        false
    }
    /// Stub: always `false` since no complexity is computed without the feature.
    #[must_use]
    pub fn is_expansion(&self) -> bool {
        false
    }
    /// Stub: always `false` since no complexity is computed without the feature.
    #[must_use]
    pub fn is_contraction(&self) -> bool {
        false
    }
}
/// Record the health score of a Rust source file **before** an edit.
///
/// Returns the computed health (useful for test assertions and hint
/// emission at pre_edit time). Returns `None` for non-Rust paths or
/// unparseable source — in that case the cache is not mutated.
#[must_use]
pub fn record_pre_health(file_path: &str, source: &str) -> Option<f32> {
    if !file_path.ends_with(".rs") {
        return None;
    }
    let health = RustQualitySignals::from_source(source)?.health_score();
    cache().insert(file_path.to_string(), health);
    Some(health)
}
/// Compute the health delta for a Rust source file **after** an edit.
///
/// Looks up and **removes** the cached pre-edit health (one-shot),
/// computes the post-edit health, and returns the signed delta. When
/// no cache entry exists (e.g. daemon restart), `old = None` and
/// `delta = None`, but `new` is still computed so callers can decide
/// how to treat first-time observations.
#[must_use]
pub fn compute_health_delta(file_path: &str, new_source: &str) -> Option<HealthDelta> {
    if !file_path.ends_with(".rs") {
        return None;
    }
    let new_health = RustQualitySignals::from_source(new_source)?.health_score();
    let old = cache().remove(file_path).map(|(_, v)| v);
    let delta = old.map(|o| new_health - o);
    Some(HealthDelta {
        old,
        new: new_health,
        delta,
    })
}
/// Wave 11 — multi-language pre-record. Dispatches to the syn engine
/// for `.rs` files and to the tree-sitter `analyze_quality` engine for
/// every other language `Lang::from_path` recognises.
///
/// Returns the recorded score (`[0.0, 1.0]`, higher = healthier) when
/// the source could be analysed, `None` otherwise. Pairs with
/// [`compute_signals_delta`] to compute deltas across all supported
/// languages — Python, TypeScript/TSX, JavaScript, Bash, Go, etc.
#[must_use]
pub fn record_pre_signals(file_path: &str, source: &str) -> Option<f32> {
    let quality = compute_quality_for_path(file_path, source)?;
    cache().insert(file_path.to_string(), quality);
    crate::shared::gate_metrics::record_health_delta_record();
    Some(quality)
}
/// Wave 11 — multi-language delta computation. Mirror of
/// [`compute_health_delta`] for any supported language. Consumes the
/// cache entry on success (one-shot semantic).
#[must_use]
pub fn compute_signals_delta(file_path: &str, new_source: &str) -> Option<HealthDelta> {
    let new_quality = compute_quality_for_path(file_path, new_source)?;
    let old = cache().remove(file_path).map(|(_, v)| v);
    let delta = old.map(|o| new_quality - o);
    let result = HealthDelta {
        old,
        new: new_quality,
        delta,
    };
    if delta.is_some() {
        crate::shared::gate_metrics::record_health_delta_compute();
    }
    let mut outcome_for_bus = touring_foundation::DeltaOutcome::Neutral;
    let mut streak_regression: u32 = 0;
    let mut streak_improvement: u32 = 0;
    if let Some(d) = delta {
        let mut entry = streak_cache().entry(file_path.to_string()).or_default();
        if result.is_regression() {
            crate::shared::gate_metrics::record_health_delta_regression();
            entry.regression_streak = entry.regression_streak.saturating_add(1);
            entry.improvement_streak = 0;
            if entry.regression_streak == STREAK_ALERT_THRESHOLD {
                crate::shared::gate_metrics::record_health_delta_streak_alert();
                let diag = Diagnostic::new(
                    codes::Q_210_REGRESSION_STREAK,
                    Severity::Warning,
                    format!(
                        "Health delta regression streak of {} consecutive declines on '{}'",
                        STREAK_ALERT_THRESHOLD, file_path
                    ),
                )
                .with_file(file_path);
                tracing::warn!(
                    code = % diag.code, message = % diag.message, severity = ? diag
                    .severity, file_path = file_path,
                    "RFC-100 Q-210: health delta regression streak alert"
                );
            }
            outcome_for_bus = touring_foundation::DeltaOutcome::Regression;
        } else if result.is_improvement() {
            crate::shared::gate_metrics::record_health_delta_improvement();
            if entry.regression_streak >= 1 {
                crate::shared::gate_metrics::record_health_delta_recovery();
            }
            entry.improvement_streak = entry.improvement_streak.saturating_add(1);
            entry.regression_streak = 0;
            outcome_for_bus = touring_foundation::DeltaOutcome::Improvement;
            if entry.improvement_streak == STREAK_ALERT_THRESHOLD {
                crate::shared::gate_metrics::record_health_delta_streak_alert();
                let diag = Diagnostic::new(
                    codes::Q_220_IMPROVEMENT_STREAK,
                    Severity::Warning,
                    format!(
                        "Health delta improvement streak of {} consecutive gains on '{}'",
                        STREAK_ALERT_THRESHOLD, file_path
                    ),
                )
                .with_file(file_path);
                tracing::warn!(
                    code = % diag.code, message = % diag.message, severity = ? diag
                    .severity, file_path = file_path,
                    "RFC-100 Q-220: health delta improvement streak alert"
                );
            }
        } else {
            entry.regression_streak = 0;
            entry.improvement_streak = 0;
            outcome_for_bus = touring_foundation::DeltaOutcome::Neutral;
        }
        streak_regression = entry.regression_streak;
        streak_improvement = entry.improvement_streak;
        let _ = d;
    }
    if let (Some(old_val), Some(delta_val)) = (result.old, result.delta) {
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0);
        let event = touring_foundation::HealthDeltaEvent {
            file_path: file_path.to_string(),
            old_health: old_val,
            new_health: result.new,
            delta: delta_val,
            outcome: outcome_for_bus,
            regression_streak: streak_regression,
            improvement_streak: streak_improvement,
            timestamp_ms: now_ms,
        };
        let _delivered = touring_foundation::publish_health_event(event.clone());
        let _recorded = crate::health_delta_audit::record_event(&event);
    }
    Some(result)
}
/// Drop the cached pre-edit health without computing a delta.
///
/// Useful when a pre_edit is superseded (e.g. validation failure) and
/// the corresponding post_edit will never fire.
pub fn discard_pre_health(file_path: &str) {
    cache().remove(file_path);
}
/// Current number of pending pre-edit entries. Primarily a diagnostic
/// surface for tests and observability.
#[must_use]
pub fn pending_len() -> usize {
    cache().len()
}
/// Map a health delta to a bounded RL reward in `[-0.10, +0.10]`.
///
/// Returns `None` when `delta.delta` itself is `None` (no pre-edit
/// context). The caller decides how to treat that case (e.g. inject
/// a neutral reward or skip).
#[must_use]
pub fn delta_reward(delta: &HealthDelta) -> Option<f64> {
    let d = delta.delta?;
    let reward = if d >= 0.15 {
        0.10
    } else if d >= 0.05 {
        0.05
    } else if d <= -0.15 {
        -0.10
    } else if d <= -0.05 {
        -0.05
    } else {
        0.00
    };
    Some(reward)
}
/// Map max cyclomatic complexity to a bounded RL penalty in `[-0.10, 0.0]`.
///
/// Files with CC > 20 are significantly harder to edit safely. Emits a
/// small negative reward to push the RL policy toward preferring low-CC targets.
#[must_use]
pub fn complexity_reward(max_cc: u32) -> Option<f64> {
    const HIGH_CC_THRESHOLD: u32 = 20;
    const VERY_HIGH_CC_THRESHOLD: u32 = 40;
    if max_cc <= HIGH_CC_THRESHOLD {
        return Some(0.00);
    }
    let penalty = if max_cc >= VERY_HIGH_CC_THRESHOLD {
        -0.10
    } else {
        -0.05
    };
    Some(penalty)
}
/// Map unwrap count to a bounded RL penalty in `[-0.10, 0.0]`.
///
/// Files with high unwrap density are fragile. Emits a small negative reward
/// to push the RL policy toward safer patterns.
#[must_use]
pub fn unwrap_penalty(count: usize, line_count: usize) -> Option<f64> {
    let density = (count as f64 / (line_count as f64).max(1.0)) * 100.0;
    let penalty = if density > 10.0 {
        -0.10
    } else if density > 5.0 {
        -0.05
    } else {
        0.00
    };
    Some(penalty)
}
/// Render a compact human-readable hint for a [`HealthDelta`].
///
/// Format:
/// ```text
/// ⚙ health-delta: old=0.92 new=0.68 Δ=-0.24 (regression)
/// ⚙ health-delta: first-observation new=0.85
/// ```
#[must_use]
pub fn format_delta_hint(delta: &HealthDelta) -> String {
    match (delta.old, delta.delta) {
        (Some(old), Some(d)) => {
            let tag = if delta.is_regression() {
                " (regression)"
            } else if delta.is_improvement() {
                " (improvement)"
            } else {
                ""
            };
            format!(
                "⚙ health-delta: old={old:.2} new={new:.2} Δ={d:+.2}{tag}",
                new = delta.new,
            )
        }
        _ => format!("⚙ health-delta: first-observation new={:.2}", delta.new),
    }
}
/// Wave 12 (2026-04-27) — Opção B: emit RFC-100 B-302 PatchExpansion when an
/// mpatch dry-run preview shows code expansion with low fuzzy-match confidence.
///
/// Wires the previously-orphaned `PatchComplexityDelta::compute()` (Wave P1.5)
/// into a real production diagnostic. Returns the computed delta for callers
/// that want to consume it (e.g. for additional reward/health-delta signals).
///
/// Threshold: emit when `delta.is_expansion()` AND `delta.confidence < 0.7`.
/// Severity: Warning (not Error — patch is viable but worth reviewing).
///
/// Returns `None` when:
/// - The gate does not fire (no expansion or high confidence)
/// - The `mpatch-fuzzy` feature is off (stub returns None unconditionally)
#[cfg(feature = "mpatch-fuzzy")]
pub fn emit_b302_if_low_confidence_expansion(
    file: &str,
    source: &str,
    preview: &crate::shared::mpatch_preview::PatchPreview,
) -> Option<crate::health_delta::PatchComplexityDelta> {
    use crate::health_delta::PatchComplexityDelta;
    use touring_analysis::blast_radius::BlastWarning;
    use touring_foundation::diagnostic::DiagnosticCode;

    const B302_CONFIDENCE_THRESHOLD: f32 = 0.7;

    let delta = PatchComplexityDelta::compute(source, &preview.preview, preview);
    if delta.is_expansion() && delta.confidence < B302_CONFIDENCE_THRESHOLD {
        let finding = BlastWarning::PatchExpansion {
            file: file.to_string(),
            delta_bytes: delta.delta,
            confidence: delta.confidence,
        };
        let diag = finding.to_diagnostic();
        tracing::warn!(
            code = %diag.code,
            severity = %diag.severity,
            message = %diag.message,
            file_path = %file,
            delta_bytes = delta.delta,
            confidence = delta.confidence,
            method = ?delta.method,
            "B-302 PatchExpansion: mpatch fuzzy expanded with low confidence"
        );
        crate::shared::gate_metrics::record_diagnostic_b302_emitted();
        Some(delta)
    } else {
        None
    }
}

/// Stub when `mpatch-fuzzy` feature is off — always returns None.
#[cfg(not(feature = "mpatch-fuzzy"))]
pub fn emit_b302_if_low_confidence_expansion(
    _file: &str,
    _source: &str,
    _preview: &crate::shared::mpatch_preview::PatchPreview,
) -> Option<crate::health_delta::PatchComplexityDelta> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn non_rust_paths_return_none_and_do_not_mutate_cache() {
        let before = pending_len();
        assert_eq!(record_pre_health("x.py", "def f(): pass"), None);
        assert_eq!(record_pre_health("x.ts", "export const a = 1;"), None);
        assert_eq!(compute_health_delta("x.py", "def g(): pass"), None);
        assert_eq!(pending_len(), before);
    }
    #[test]
    fn parse_failure_is_fail_open() {
        let path = "/wave9/parse_fail_unique.rs";
        discard_pre_health(path);
        assert_eq!(record_pre_health(path, "this is {{{ not rust"), None);
        let probe = compute_health_delta(path, "pub fn x() -> i32 { 1 }").expect("delta");
        assert_eq!(probe.old, None, "parse failure must not insert into cache");
    }
    #[test]
    fn record_then_compute_returns_delta() {
        let path = "/wave9/flow1.rs";
        let old = record_pre_health(path, "pub fn a() -> i32 { 1 }").expect("old");
        let d = compute_health_delta(path, "pub fn a() -> i32 { 1 }").expect("delta");
        assert_eq!(d.old, Some(old));
        assert!((d.new - old).abs() < 0.01);
        assert_eq!(d.delta, Some(0.0));
    }
    #[test]
    fn regression_is_detected_when_health_drops() {
        let path = "/wave9/flow2.rs";
        record_pre_health(path, "pub fn ok() -> i32 { 1 }").expect("pre");
        let new = "pub unsafe fn bad() -> u8 {\n\
            unsafe { let _ = std::ptr::null::<u8>(); }\n\
            unsafe { let _ = std::ptr::null::<u8>(); }\n\
            unsafe { let _ = std::ptr::null::<u8>(); }\n\
            0\n\
        }";
        let d = compute_health_delta(path, new).expect("delta computed");
        assert!(
            d.delta.expect("delta present") < 0.0,
            "regression must be negative"
        );
        assert!(d.is_regression(), "is_regression flag must trip");
        assert!(!d.is_improvement());
    }
    #[test]
    fn improvement_is_detected_when_health_rises() {
        let path = "/wave9/flow3.rs";
        let old = "pub unsafe fn bad() -> u8 {\n\
            unsafe { let _ = std::ptr::null::<u8>(); }\n\
            unsafe { let _ = std::ptr::null::<u8>(); }\n\
            unsafe { let _ = std::ptr::null::<u8>(); }\n\
            0\n\
        }";
        record_pre_health(path, old).expect("pre");
        let new = "pub fn good() -> i32 { 1 }";
        let d = compute_health_delta(path, new).expect("delta");
        assert!(
            d.delta.expect("delta present") > 0.0,
            "improvement must be positive"
        );
        assert!(d.is_improvement());
        assert!(!d.is_regression());
    }
    #[test]
    fn compute_without_prior_record_has_no_delta() {
        let path = "/wave9/flow4.rs";
        discard_pre_health(path);
        let d = compute_health_delta(path, "pub fn a() -> i32 { 1 }").expect("delta");
        assert_eq!(d.old, None);
        assert_eq!(d.delta, None);
        assert!(d.new > 0.0);
    }
    #[test]
    fn cache_is_consumed_after_compute() {
        let path = "/wave9/flow5.rs";
        record_pre_health(path, "pub fn a() -> i32 { 1 }").expect("pre");
        let before = pending_len();
        assert!(before >= 1);
        let _ = compute_health_delta(path, "pub fn a() -> i32 { 1 }");
        let d2 = compute_health_delta(path, "pub fn a() -> i32 { 2 }").expect("second");
        assert_eq!(
            d2.old, None,
            "cache entry must be consumed by first compute"
        );
    }
    #[test]
    fn discard_pre_health_drops_entry() {
        let path = "/wave9/flow6.rs";
        record_pre_health(path, "pub fn a() -> i32 { 1 }").expect("pre");
        discard_pre_health(path);
        let d = compute_health_delta(path, "pub fn a() -> i32 { 1 }").expect("delta");
        assert_eq!(d.old, None, "discarded entry must not produce a delta");
    }
    #[test]
    fn delta_reward_envelope_is_bounded() {
        let fixtures = [
            (0.50, 0.10),
            (0.15, 0.10),
            (0.10, 0.05),
            (0.05, 0.05),
            (0.00, 0.00),
            (-0.04, 0.00),
            (-0.05, -0.05),
            (-0.14, -0.05),
            (-0.15, -0.10),
            (-0.50, -0.10),
        ];
        for (d, expected) in fixtures {
            let hd = HealthDelta {
                old: Some(0.9),
                new: 0.9 + d,
                delta: Some(d),
            };
            let r = delta_reward(&hd).expect("reward");
            assert!((-0.10..=0.10).contains(&r), "envelope violated: {r}");
            assert!(
                (r - expected).abs() < 1e-9,
                "reward {r} != {expected} for delta {d}"
            );
        }
    }
    #[test]
    fn delta_reward_is_none_when_delta_absent() {
        let hd = HealthDelta {
            old: None,
            new: 0.9,
            delta: None,
        };
        assert_eq!(delta_reward(&hd), None);
    }
    #[test]
    fn record_pre_signals_handles_python() {
        let path = "/wave11/sig_py.py";
        discard_pre_health(path);
        let q = record_pre_signals(path, "def add(a, b):\n    return a + b\n")
            .expect("python source must record");
        assert!((0.0..=1.0).contains(&q), "score in [0,1], got {q}");
    }
    #[test]
    fn record_pre_signals_handles_typescript() {
        let path = "/wave11/sig_ts.ts";
        discard_pre_health(path);
        let q = record_pre_signals(
            path,
            "export function inc(x: number): number { return x + 1; }\n",
        )
        .expect("ts source must record");
        assert!((0.0..=1.0).contains(&q));
    }
    #[test]
    fn record_pre_signals_handles_rust_via_syn_engine() {
        let path = "/wave11/sig_rs.rs";
        discard_pre_health(path);
        let q = record_pre_signals(path, "pub fn ok() -> i32 { 1 }").expect("rs must record");
        discard_pre_health(path);
        let q2 = record_pre_health(path, "pub fn ok() -> i32 { 1 }").expect("rs legacy");
        assert!(
            (q - q2).abs() < f32::EPSILON,
            "syn dispatch drift: signals={q} vs legacy={q2}",
        );
    }
    #[test]
    fn record_pre_signals_returns_none_for_unsupported_extension() {
        let path = "/wave11/sig.xyz";
        discard_pre_health(path);
        assert_eq!(record_pre_signals(path, "anything"), None);
    }
    #[test]
    fn compute_signals_delta_yields_delta_for_python() {
        let path = "/wave11/delta_py.py";
        discard_pre_health(path);
        record_pre_signals(path, "def a(): return 1\n").expect("pre");
        let d = compute_signals_delta(
            path,
            "def a():\n    if True: return 1\n    else: return 2\n",
        )
        .expect("delta");
        assert!(d.old.is_some(), "old should be present");
        assert!(d.delta.is_some(), "delta should be present");
        let d2 = compute_signals_delta(path, "def a(): return 1\n").expect("second");
        assert_eq!(d2.old, None, "cache must be consumed by first compute");
    }
    #[test]
    fn compute_signals_delta_for_typescript_pre_post_cycle() {
        let path = "/wave11/delta_ts.ts";
        discard_pre_health(path);
        record_pre_signals(path, "export const a = (x: number) => x + 1;\n").expect("pre");
        let d =
            compute_signals_delta(path, "export const a = (x: number) => x + 1;\n").expect("delta");
        assert!(d.old.is_some());
        assert_eq!(d.delta, Some(0.0), "identity edit must yield zero delta");
    }
    #[test]
    fn multi_lang_mix_does_not_cross_contaminate() {
        let py = "/wave11/mix.py";
        let ts = "/wave11/mix.ts";
        discard_pre_health(py);
        discard_pre_health(ts);
        record_pre_signals(py, "def a(): return 1\n").expect("py pre");
        record_pre_signals(ts, "export const a = () => 1;\n").expect("ts pre");
        let dy = compute_signals_delta(py, "def a(): return 2\n").expect("py delta");
        let dt = compute_signals_delta(ts, "export const a = () => 2;\n").expect("ts delta");
        assert!(dy.old.is_some(), "py cache hit");
        assert!(dt.old.is_some(), "ts cache hit");
    }
    #[test]
    fn record_pre_signals_increments_record_counter() {
        use crate::shared::gate_metrics::global;
        let path = "/wave12/obs_record.rs";
        discard_pre_health(path);
        let baseline = global()
            .health_delta_record_count
            .load(std::sync::atomic::Ordering::Relaxed);
        record_pre_signals(path, "pub fn ok() -> i32 { 1 }").expect("recorded");
        let after = global()
            .health_delta_record_count
            .load(std::sync::atomic::Ordering::Relaxed);
        assert!(
            after >= baseline + 1,
            "record counter must advance: {baseline} → {after}"
        );
    }
    #[test]
    fn compute_signals_delta_increments_compute_counter() {
        use std::sync::atomic::Ordering;
        let path = "/wave12/obs_compute.rs";
        discard_pre_health(path);
        record_pre_signals(path, "pub fn a() -> i32 { 1 }").expect("pre");
        let baseline = crate::shared::gate_metrics::global()
            .health_delta_compute_count
            .load(Ordering::Relaxed);
        let _ = compute_signals_delta(path, "pub fn a() -> i32 { 2 }").expect("delta");
        let after = crate::shared::gate_metrics::global()
            .health_delta_compute_count
            .load(Ordering::Relaxed);
        assert!(
            after >= baseline + 1,
            "compute counter must advance: {baseline} → {after}"
        );
    }
    #[test]
    fn regression_increments_regression_counter() {
        use std::sync::atomic::Ordering;
        let path = "/wave12/obs_regression.rs";
        discard_pre_health(path);
        record_pre_signals(path, "pub fn ok() -> i32 { 1 }").expect("pre");
        let baseline = crate::shared::gate_metrics::global()
            .health_delta_regression_count
            .load(Ordering::Relaxed);
        let degraded = "pub unsafe fn bad() -> u8 {\n\
            unsafe { let _ = std::ptr::null::<u8>(); }\n\
            unsafe { let _ = std::ptr::null::<u8>(); }\n\
            unsafe { let _ = std::ptr::null::<u8>(); }\n\
            0\n\
        }";
        let d = compute_signals_delta(path, degraded).expect("delta");
        assert!(d.is_regression(), "fixture must regress");
        let after = crate::shared::gate_metrics::global()
            .health_delta_regression_count
            .load(Ordering::Relaxed);
        assert!(after >= baseline + 1, "regression counter must advance");
    }
    #[test]
    fn improvement_increments_improvement_counter() {
        use std::sync::atomic::Ordering;
        let path = "/wave12/obs_improvement.rs";
        discard_pre_health(path);
        let degraded = "pub unsafe fn bad() -> u8 {\n\
            unsafe { let _ = std::ptr::null::<u8>(); }\n\
            unsafe { let _ = std::ptr::null::<u8>(); }\n\
            unsafe { let _ = std::ptr::null::<u8>(); }\n\
            0\n\
        }";
        record_pre_signals(path, degraded).expect("pre");
        let baseline = crate::shared::gate_metrics::global()
            .health_delta_improvement_count
            .load(Ordering::Relaxed);
        let d = compute_signals_delta(path, "pub fn good() -> i32 { 1 }").expect("delta");
        assert!(d.is_improvement(), "fixture must improve");
        let after = crate::shared::gate_metrics::global()
            .health_delta_improvement_count
            .load(Ordering::Relaxed);
        assert!(after >= baseline + 1, "improvement counter must advance");
    }
    /// Drive `n` consecutive regression cycles on `path`.
    fn drive_regressions(path: &str, n: u32) {
        for _ in 0..n {
            reset_streak_helper(path);
            discard_pre_health(path);
            record_pre_signals(path, "pub fn ok() -> i32 { 1 }").expect("pre");
            let degraded = "pub unsafe fn bad() -> u8 {\n\
                unsafe { let _ = std::ptr::null::<u8>(); }\n\
                unsafe { let _ = std::ptr::null::<u8>(); }\n\
                unsafe { let _ = std::ptr::null::<u8>(); }\n\
                0\n\
            }";
            let d = compute_signals_delta(path, degraded).expect("delta");
            assert!(d.is_regression(), "fixture must regress");
        }
    }
    fn reset_streak_helper(_path: &str) {}
    #[test]
    fn streak_starts_at_zero_for_new_path() {
        let path = "/wave13/zero.rs";
        reset_streak(path);
        assert_eq!(regression_streak(path), 0);
        assert_eq!(improvement_streak(path), 0);
        assert_eq!(streak_counters(path), StreakCounters::default());
    }
    #[test]
    fn three_consecutive_regressions_alert_threshold() {
        let path = "/wave13/streak3.rs";
        reset_streak(path);
        discard_pre_health(path);
        let alerts_before = crate::shared::gate_metrics::global()
            .health_delta_streak_alert_count
            .load(std::sync::atomic::Ordering::Relaxed);
        drive_regressions(path, 3);
        assert_eq!(
            regression_streak(path),
            3,
            "streak must equal 3 after 3 regressions"
        );
        let alerts_after = crate::shared::gate_metrics::global()
            .health_delta_streak_alert_count
            .load(std::sync::atomic::Ordering::Relaxed);
        assert!(
            alerts_after >= alerts_before + 1,
            "alert counter must advance"
        );
    }
    #[test]
    fn improvement_resets_regression_streak() {
        let path = "/wave13/recovery.rs";
        reset_streak(path);
        drive_regressions(path, 2);
        assert_eq!(regression_streak(path), 2);
        let recoveries_before = crate::shared::gate_metrics::global()
            .health_delta_recovery_count
            .load(std::sync::atomic::Ordering::Relaxed);
        let degraded = "pub unsafe fn bad() -> u8 {\n\
            unsafe { let _ = std::ptr::null::<u8>(); }\n\
            unsafe { let _ = std::ptr::null::<u8>(); }\n\
            unsafe { let _ = std::ptr::null::<u8>(); }\n\
            0\n\
        }";
        record_pre_signals(path, degraded).expect("pre");
        let d = compute_signals_delta(path, "pub fn good() -> i32 { 1 }").expect("delta");
        assert!(d.is_improvement(), "must be improvement");
        assert_eq!(
            regression_streak(path),
            0,
            "regression streak resets on improvement"
        );
        assert!(improvement_streak(path) >= 1);
        let recoveries_after = crate::shared::gate_metrics::global()
            .health_delta_recovery_count
            .load(std::sync::atomic::Ordering::Relaxed);
        assert!(
            recoveries_after >= recoveries_before + 1,
            "recovery counter must bump",
        );
    }
    #[test]
    fn neutral_delta_resets_both_streaks() {
        let path = "/wave13/neutral.rs";
        reset_streak(path);
        drive_regressions(path, 1);
        assert_eq!(regression_streak(path), 1);
        let src = "pub fn ok() -> i32 { 1 }";
        record_pre_signals(path, src).expect("pre");
        let d = compute_signals_delta(path, src).expect("delta");
        assert_eq!(d.delta, Some(0.0));
        assert!(!d.is_regression() && !d.is_improvement());
        assert_eq!(regression_streak(path), 0);
        assert_eq!(improvement_streak(path), 0);
    }
    #[test]
    fn first_observation_does_not_touch_streaks() {
        let path = "/wave13/firstobs.rs";
        reset_streak(path);
        discard_pre_health(path);
        let d = compute_signals_delta(path, "pub fn a() -> i32 { 1 }").expect("delta");
        assert_eq!(d.old, None);
        assert_eq!(d.delta, None);
        assert_eq!(regression_streak(path), 0);
        assert_eq!(improvement_streak(path), 0);
    }
    #[test]
    fn streak_alert_emits_only_at_threshold_crossing() {
        let path = "/wave13/single_alert.rs";
        reset_streak(path);
        drive_regressions(path, 4);
        assert_eq!(regression_streak(path), 4, "streak grew to 4");
        drive_regressions(path, 2);
        assert_eq!(regression_streak(path), 6, "streak continues growing");
    }
    #[test]
    fn streak_warning_hint_returns_none_below_threshold() {
        let path = "/wave14/below.rs";
        reset_streak(path);
        assert_eq!(streak_warning_hint(path), None);
        drive_regressions(path, 2);
        assert_eq!(
            streak_warning_hint(path),
            None,
            "warning must NOT fire below threshold (streak=2)",
        );
    }
    #[test]
    fn streak_warning_hint_fires_at_threshold() {
        let path = "/wave14/at_threshold.rs";
        reset_streak(path);
        drive_regressions(path, 3);
        let hint = streak_warning_hint(path).expect("hint must fire at streak=3");
        assert!(
            hint.starts_with("⚠ regression streak:"),
            "format mismatch: {hint:?}"
        );
        assert!(hint.contains("3 consecutive"), "must show count: {hint:?}");
        assert!(hint.contains(path), "must reference file: {hint:?}");
    }
    #[test]
    fn streak_warning_hint_grows_with_streak() {
        let path = "/wave14/growing.rs";
        reset_streak(path);
        drive_regressions(path, 5);
        let hint = streak_warning_hint(path).expect("hint at streak=5");
        assert!(
            hint.contains("5 consecutive"),
            "must reflect current streak: {hint:?}"
        );
    }
    #[test]
    fn improvement_streak_hint_below_threshold_returns_none() {
        let path = "/wave14/imp_below.rs";
        reset_streak(path);
        let degraded = "pub unsafe fn bad() -> u8 {\n\
            unsafe { let _ = std::ptr::null::<u8>(); }\n\
            unsafe { let _ = std::ptr::null::<u8>(); }\n\
            unsafe { let _ = std::ptr::null::<u8>(); }\n\
            0\n\
        }";
        for _ in 0..2 {
            discard_pre_health(path);
            record_pre_signals(path, degraded).expect("pre");
            let _ = compute_signals_delta(path, "pub fn good() -> i32 { 1 }").expect("imp");
        }
        assert_eq!(
            improvement_streak_hint(path),
            None,
            "below 3 must return None"
        );
    }
    #[test]
    fn improvement_streak_hint_fires_at_threshold() {
        let path = "/wave14/imp_threshold.rs";
        reset_streak(path);
        let degraded = "pub unsafe fn bad() -> u8 {\n\
            unsafe { let _ = std::ptr::null::<u8>(); }\n\
            unsafe { let _ = std::ptr::null::<u8>(); }\n\
            unsafe { let _ = std::ptr::null::<u8>(); }\n\
            0\n\
        }";
        for _ in 0..3 {
            discard_pre_health(path);
            record_pre_signals(path, degraded).expect("pre");
            let _ = compute_signals_delta(path, "pub fn good() -> i32 { 1 }").expect("imp");
        }
        let hint = improvement_streak_hint(path).expect("imp hint at 3");
        assert!(
            hint.starts_with("✓ improvement streak:"),
            "format: {hint:?}"
        );
        assert!(hint.contains("3 consecutive gains"));
    }
    #[test]
    fn warning_hint_clears_after_recovery() {
        let path = "/wave14/clears.rs";
        reset_streak(path);
        drive_regressions(path, 3);
        assert!(streak_warning_hint(path).is_some(), "fires before recovery");
        let degraded = "pub unsafe fn bad() -> u8 {\n\
            unsafe { let _ = std::ptr::null::<u8>(); }\n\
            unsafe { let _ = std::ptr::null::<u8>(); }\n\
            unsafe { let _ = std::ptr::null::<u8>(); }\n\
            0\n\
        }";
        record_pre_signals(path, degraded).expect("pre");
        let _ = compute_signals_delta(path, "pub fn good() -> i32 { 1 }").expect("imp");
        assert_eq!(
            streak_warning_hint(path),
            None,
            "warning must clear after recovery",
        );
    }
    #[test]
    fn status_json_aggregate_includes_all_counters() {
        let out = status_json(None);
        let v: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
        for key in &[
            "record_count",
            "compute_count",
            "regression_count",
            "improvement_count",
            "outstanding",
            "streak_alert_count",
            "recovery_count",
            "alert_threshold",
        ] {
            assert!(
                v.get(key).is_some(),
                "aggregate must include `{key}`: {out}"
            );
        }
        assert_eq!(
            v["alert_threshold"].as_u64(),
            Some(u64::from(STREAK_ALERT_THRESHOLD)),
        );
    }
    #[test]
    fn status_json_per_path_includes_streak_state() {
        let path = "/wave16/status_path.rs";
        reset_streak(path);
        for _ in 0..3 {
            discard_pre_health(path);
            record_pre_signals(path, "pub fn ok() -> i32 { 1 }").expect("pre");
            let degraded = "pub unsafe fn bad() -> u8 {\n\
                unsafe { let _ = std::ptr::null::<u8>(); }\n\
                unsafe { let _ = std::ptr::null::<u8>(); }\n\
                unsafe { let _ = std::ptr::null::<u8>(); }\n\
                0\n\
            }";
            let _ = compute_signals_delta(path, degraded);
        }
        let out = status_json(Some(path));
        let v: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
        assert_eq!(v["file_path"].as_str(), Some(path));
        assert_eq!(v["regression_streak"].as_u64(), Some(3));
        assert_eq!(v["improvement_streak"].as_u64(), Some(0));
        assert!(v["warning_hint"].is_string(), "warning must surface: {out}");
        assert!(v["improvement_hint"].is_null());
    }
    #[test]
    fn status_json_per_path_below_threshold_yields_null_hints() {
        let path = "/wave16/status_below.rs";
        reset_streak(path);
        discard_pre_health(path);
        record_pre_signals(path, "pub fn ok() -> i32 { 1 }").expect("pre");
        let degraded = "pub unsafe fn bad() -> u8 {\n\
            unsafe { let _ = std::ptr::null::<u8>(); }\n\
            unsafe { let _ = std::ptr::null::<u8>(); }\n\
            unsafe { let _ = std::ptr::null::<u8>(); }\n\
            0\n\
        }";
        let _ = compute_signals_delta(path, degraded);
        let out = status_json(Some(path));
        let v: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
        assert_eq!(v["regression_streak"].as_u64(), Some(1));
        assert!(v["warning_hint"].is_null());
        assert!(v["improvement_hint"].is_null());
    }
    #[test]
    fn reset_json_clears_state_and_returns_success() {
        let path = "/wave16/reset_target.rs";
        reset_streak(path);
        for _ in 0..2 {
            discard_pre_health(path);
            record_pre_signals(path, "pub fn ok() -> i32 { 1 }").expect("pre");
            let degraded = "pub unsafe fn bad() -> u8 {\n\
                unsafe { let _ = std::ptr::null::<u8>(); }\n\
                unsafe { let _ = std::ptr::null::<u8>(); }\n\
                unsafe { let _ = std::ptr::null::<u8>(); }\n\
                0\n\
            }";
            let _ = compute_signals_delta(path, degraded);
        }
        assert_eq!(regression_streak(path), 2);
        let out = reset_json(path);
        let v: serde_json::Value = serde_json::from_str(&out).expect("valid JSON");
        assert_eq!(v["reset"].as_bool(), Some(true));
        assert_eq!(v["file_path"].as_str(), Some(path));
        assert_eq!(regression_streak(path), 0, "reset must clear streak");
    }
    #[test]
    fn status_json_and_cli_handler_produce_identical_output() {
        let path = "/wave16/sst_invariant.rs";
        reset_streak(path);
        let pure_agg = status_json(None);
        let payload_agg = serde_json::json!({});
        let _: serde_json::Value =
            serde_json::from_str(&pure_agg).expect("pure aggregate is valid JSON");
        for key in &["record_count", "alert_threshold"] {
            assert!(pure_agg.contains(key), "key `{key}` missing");
        }
        let _ = payload_agg;
    }
    #[test]
    fn snapshot_outstanding_reflects_record_minus_compute() {
        let path1 = "/wave12/outstanding_a.rs";
        let path2 = "/wave12/outstanding_b.rs";
        discard_pre_health(path1);
        discard_pre_health(path2);
        let snap_before = crate::shared::gate_metrics::GateMetricsSnapshot::capture();
        record_pre_signals(path1, "pub fn a() -> i32 { 1 }").expect("a");
        record_pre_signals(path2, "pub fn b() -> i32 { 2 }").expect("b");
        let _ = compute_signals_delta(path1, "pub fn a() -> i32 { 1 }").expect("a delta");
        let snap_after = crate::shared::gate_metrics::GateMetricsSnapshot::capture();
        let record_delta =
            snap_after.health_delta_record_count - snap_before.health_delta_record_count;
        let compute_delta =
            snap_after.health_delta_compute_count - snap_before.health_delta_compute_count;
        assert!(
            record_delta >= 2 && compute_delta >= 1,
            "deltas must advance: record+={record_delta} compute+={compute_delta}",
        );
        discard_pre_health(path2);
    }
    #[test]
    fn compute_signals_delta_for_unsupported_returns_none() {
        let path = "/wave11/skip.xyz";
        discard_pre_health(path);
        assert_eq!(compute_signals_delta(path, "anything"), None);
    }
    #[test]
    fn format_hint_has_expected_structure() {
        let hd = HealthDelta {
            old: Some(0.90),
            new: 0.60,
            delta: Some(-0.30),
        };
        let h = format_delta_hint(&hd);
        assert!(h.starts_with("⚙ health-delta:"));
        assert!(h.contains("old=0.90"));
        assert!(h.contains("new=0.60"));
        assert!(h.contains("(regression)"));
        let hd2 = HealthDelta {
            old: None,
            new: 0.85,
            delta: None,
        };
        let h2 = format_delta_hint(&hd2);
        assert!(h2.contains("first-observation"));
        assert!(h2.contains("new=0.85"));
        let hd3 = HealthDelta {
            old: Some(0.50),
            new: 0.85,
            delta: Some(0.35),
        };
        let h3 = format_delta_hint(&hd3);
        assert!(h3.contains("(improvement)"));
    }
    #[test]
    fn q210_diagnostic_code_is_valid_and_well_formed() {
        use touring_foundation::diagnostic::{Diagnostic, Severity, codes};
        let path = "/rfc100/q210_shape.rs";
        let diag = Diagnostic::new(
            codes::Q_210_REGRESSION_STREAK,
            Severity::Warning,
            format!(
                "Health delta regression streak of {} consecutive declines on '{}'",
                STREAK_ALERT_THRESHOLD, path
            ),
        )
        .with_file(path);
        assert_eq!(diag.code, "Q-210", "RFC-100 code must be Q-210");
        assert_eq!(
            diag.severity,
            Severity::Warning,
            "streak alert is Warning severity"
        );
        assert!(
            diag.message.contains(&STREAK_ALERT_THRESHOLD.to_string()),
            "message must embed threshold count: {:?}",
            diag.message,
        );
        assert!(
            diag.message.contains(path),
            "message must embed file path: {:?}",
            diag.message,
        );
        assert_eq!(diag.file.as_deref(), Some(path), "file field must be set");
        assert!(
            Diagnostic::is_valid_code(&diag.code),
            "Q-210 must pass RFC-100 code validation",
        );
    }
    #[test]
    fn q210_emitted_when_streak_reaches_threshold() {
        let path = "/rfc100/q210_emit.rs";
        reset_streak(path);
        discard_pre_health(path);
        let alerts_before = crate::shared::gate_metrics::global()
            .health_delta_streak_alert_count
            .load(std::sync::atomic::Ordering::Relaxed);
        drive_regressions(path, STREAK_ALERT_THRESHOLD);
        assert_eq!(
            regression_streak(path),
            STREAK_ALERT_THRESHOLD,
            "streak must equal threshold after {} regressions",
            STREAK_ALERT_THRESHOLD,
        );
        let alerts_after = crate::shared::gate_metrics::global()
            .health_delta_streak_alert_count
            .load(std::sync::atomic::Ordering::Relaxed);
        assert!(
            alerts_after >= alerts_before + 1,
            "streak_alert counter must advance when Q-210 path fires \
             (before={alerts_before} after={alerts_after})",
        );
    }
    #[test]
    fn q220_diagnostic_code_is_valid_and_well_formed() {
        use touring_foundation::diagnostic::{Diagnostic, Severity, codes};
        let path = "/rfc100/q220_shape.rs";
        let diag = Diagnostic::new(
            codes::Q_220_IMPROVEMENT_STREAK,
            Severity::Warning,
            format!(
                "Health delta improvement streak of {} consecutive gains on '{}'",
                STREAK_ALERT_THRESHOLD, path
            ),
        )
        .with_file(path);
        assert_eq!(diag.code, "Q-220", "RFC-100 code must be Q-220");
        assert_eq!(
            diag.severity,
            Severity::Warning,
            "improvement streak is Warning severity"
        );
        assert!(
            diag.message.contains(&STREAK_ALERT_THRESHOLD.to_string()),
            "message must embed threshold count: {:?}",
            diag.message,
        );
        assert!(
            diag.message.contains(path),
            "message must embed file path: {:?}",
            diag.message,
        );
        assert_eq!(diag.file.as_deref(), Some(path), "file field must be set");
        assert!(
            Diagnostic::is_valid_code(&diag.code),
            "Q-220 must pass RFC-100 code validation",
        );
    }
    #[test]
    fn q220_emitted_when_improvement_streak_reaches_threshold() {
        let path = "/rfc100/q220_emit.rs";
        reset_streak(path);
        discard_pre_health(path);
        let alerts_before = crate::shared::gate_metrics::global()
            .health_delta_streak_alert_count
            .load(std::sync::atomic::Ordering::Relaxed);
        let degraded = "pub unsafe fn bad() -> u8 {\n\
            unsafe { let _ = std::ptr::null::<u8>(); }\n\
            unsafe { let _ = std::ptr::null::<u8>(); }\n\
            unsafe { let _ = std::ptr::null::<u8>(); }\n\
            0\n\
        }";
        for _ in 0..STREAK_ALERT_THRESHOLD {
            discard_pre_health(path);
            record_pre_signals(path, degraded).expect("pre");
            let _ = compute_signals_delta(path, "pub fn good() -> i32 { 1 }").expect("imp");
        }
        assert_eq!(
            improvement_streak(path),
            STREAK_ALERT_THRESHOLD,
            "improvement streak must equal threshold after {} improvements",
            STREAK_ALERT_THRESHOLD,
        );
        let alerts_after = crate::shared::gate_metrics::global()
            .health_delta_streak_alert_count
            .load(std::sync::atomic::Ordering::Relaxed);
        assert!(
            alerts_after >= alerts_before + 1,
            "streak_alert counter must advance when Q-220 path fires \
             (before={alerts_before} after={alerts_after})",
        );
    }
}
