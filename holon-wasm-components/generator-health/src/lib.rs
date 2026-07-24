//! THSF Phase 5 Wave H — `generator-health` capability component.
//!
//! Pure-function formatter for `health_delta` snapshots. Consumes a
//! JSON payload with aggregate counters + per-path streak state and
//! produces an analysis JSON with:
//!
//! - a one-line `summary` for dashboards
//! - an `alerts` array (severity + path + message)
//! - a `metrics` block with derived ratios + a composite `health_score`
//!
//! The component is sandbox-isolated (WASI 0.2) — it does NOT call
//! `touring gate-metrics`, read the filesystem, or do any I/O. The
//! HOST fetches the raw snapshot (typically via `touring gate-metrics -j`
//! + `touring health-delta status <path>`) and passes it as the invoke
//! args. This separation makes the component usable from any
//! wasmtime-embedding language (Go/Zig/JS) without Rust-specific IPC.
//!
//! # Input JSON
//!
//! ```json
//! {
//!   "counters": {
//!     "record_count": 12, "compute_count": 10,
//!     "regression_count": 2, "improvement_count": 7,
//!     "streak_alert_count": 1, "recovery_count": 1,
//!     "alert_threshold": 3
//!   },
//!   "per_path": [
//!     {"file_path": "src/foo.rs", "regression_streak": 3, "improvement_streak": 0},
//!     {"file_path": "src/bar.rs", "regression_streak": 0, "improvement_streak": 5}
//!   ]
//! }
//! ```
//!
//! # Output JSON
//!
//! ```json
//! {
//!   "summary": "10 computes • 2 regressions (20.0%) • 7 improvements • 1 recovery",
//!   "alerts": [
//!     {"severity": "critical", "file_path": "src/foo.rs",
//!      "message": "regression streak 3 reached alert threshold"},
//!     {"severity": "info", "file_path": "src/bar.rs",
//!      "message": "improvement streak 5"}
//!   ],
//!   "metrics": {
//!     "total_computes": 10,
//!     "regression_ratio": 0.2,
//!     "improvement_ratio": 0.7,
//!     "alert_count": 2,
//!     "health_score": 0.9
//!   }
//! }
//! ```
//!
//! # Error surface
//!
//! - `InvokeError::UnknownCapability` — capability name != `"generator-health"`
//! - `InvokeError::InvalidArgs` — args bytes are not UTF-8 JSON or
//!   required fields missing / wrong type

use serde::{Deserialize, Serialize};

// ---------------------------------------------------------------------------
// Bindings
// ---------------------------------------------------------------------------

wit_bindgen::generate!({
    path: "crates/touring-wasm/wit/holon-core.wit",
    world: "holon-component",
});

use exports::holon::core::capabilities::{Guest, InvokeError, InvokeRequest, InvokeResponse};

const CAPABILITY: &str = "generator-health";

// ---------------------------------------------------------------------------
// Input / output types
// ---------------------------------------------------------------------------

/// Aggregate counter snapshot mirroring `touring-hooks::shared::gate_metrics`
/// `health_delta_*` fields. Missing fields default to 0; this is a forward-
/// compatibility hedge in case the host predates or postdates a counter
/// rename.
#[derive(Debug, Default, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
struct Counters {
    #[serde(default)]
    compute_count: u64,
    #[serde(default)]
    regression_count: u64,
    #[serde(default)]
    improvement_count: u64,
    #[serde(default)]
    streak_alert_count: u64,
    #[serde(default)]
    recovery_count: u64,
    #[serde(default = "default_alert_threshold")]
    alert_threshold: u32,
}

fn default_alert_threshold() -> u32 {
    3
}

/// One entry of the `per_path` array — current streak state for a file.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
struct PerPathEntry {
    file_path: String,
    #[serde(default)]
    regression_streak: u32,
    #[serde(default)]
    improvement_streak: u32,
}

/// Full input envelope consumed by the component.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "snake_case")]
struct Input {
    #[serde(default)]
    counters: Counters,
    #[serde(default)]
    per_path: Vec<PerPathEntry>,
}

/// Severity tier for a single alert.
#[derive(Debug, Clone, Copy, Serialize)]
#[serde(rename_all = "lowercase")]
enum Severity {
    /// Positive signal — improvement streak worth highlighting.
    Info,
    /// Approaching the alert threshold; investigate next edit.
    Warning,
    /// At or above the alert threshold; review recommended.
    Critical,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct Alert {
    severity: Severity,
    file_path: String,
    message: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct Metrics {
    total_computes: u64,
    regression_ratio: f32,
    improvement_ratio: f32,
    alert_count: u32,
    /// Composite health score in `[0.0, 1.2]`. Values below 0.75 indicate
    /// sustained regressions; 1.0 is a healthy baseline; values above 1.0
    /// reflect an above-baseline improvement streak.
    health_score: f32,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "snake_case")]
struct Output {
    summary: String,
    alerts: Vec<Alert>,
    metrics: Metrics,
}

// ---------------------------------------------------------------------------
// Pure analysis
// ---------------------------------------------------------------------------

/// Compute `regression_ratio` and `improvement_ratio` with safe division.
///
/// Returns `(regression_ratio, improvement_ratio)` each clamped to `[0, 1]`.
/// When `compute_count == 0` both ratios are zero (nothing to assess).
fn compute_ratios(c: &Counters) -> (f32, f32) {
    let total = c.compute_count as f32;
    if total == 0.0 {
        return (0.0, 0.0);
    }
    let reg = (c.regression_count as f32 / total).clamp(0.0, 1.0);
    let imp = (c.improvement_count as f32 / total).clamp(0.0, 1.0);
    (reg, imp)
}

/// Composite health score.
///
/// Formula: `(1 - regression_ratio) + recovery_bonus`, with
/// `recovery_bonus = min(0.2, recovery_count / max(1, regression_count))`.
///
/// Produces values in `[0.0, 1.2]`. A file with zero regressions and a
/// recent recovery scores 1.2; a file with 50% regression ratio scores
/// 0.5. Meaningfully above 0.75 = healthy; below = review.
fn compute_health_score(c: &Counters, regression_ratio: f32) -> f32 {
    let recovery_bonus = if c.regression_count == 0 {
        0.0
    } else {
        (c.recovery_count as f32 / c.regression_count as f32).min(0.2)
    };
    ((1.0 - regression_ratio) + recovery_bonus).clamp(0.0, 1.2)
}

/// Classify a single `PerPathEntry` into an alert, or `None` when neutral.
fn classify_path(entry: &PerPathEntry, alert_threshold: u32) -> Option<Alert> {
    if alert_threshold == 0 {
        return None;
    }
    if entry.regression_streak >= alert_threshold {
        return Some(Alert {
            severity: Severity::Critical,
            file_path: entry.file_path.clone(),
            message: format!(
                "regression streak {} reached alert threshold",
                entry.regression_streak
            ),
        });
    }
    if entry.regression_streak + 1 == alert_threshold && entry.regression_streak > 0 {
        return Some(Alert {
            severity: Severity::Warning,
            file_path: entry.file_path.clone(),
            message: format!(
                "regression streak {} approaching threshold {}",
                entry.regression_streak, alert_threshold
            ),
        });
    }
    if entry.improvement_streak >= 3 {
        return Some(Alert {
            severity: Severity::Info,
            file_path: entry.file_path.clone(),
            message: format!("improvement streak {}", entry.improvement_streak),
        });
    }
    None
}

/// Build the single-line summary string.
fn build_summary(c: &Counters, regression_ratio: f32, improvement_ratio: f32) -> String {
    format!(
        "{} computes \u{2022} {} regressions ({:.1}%) \u{2022} {} improvements ({:.1}%) \u{2022} {} {}",
        c.compute_count,
        c.regression_count,
        regression_ratio * 100.0,
        c.improvement_count,
        improvement_ratio * 100.0,
        c.recovery_count,
        if c.recovery_count == 1 { "recovery" } else { "recoveries" },
    )
}

/// Core analysis pipeline — pure and deterministic. Returns the rendered
/// [`Output`] ready for JSON serialization.
fn analyze(input: Input) -> Output {
    let (regression_ratio, improvement_ratio) = compute_ratios(&input.counters);
    let health_score = compute_health_score(&input.counters, regression_ratio);

    let mut alerts: Vec<Alert> = input
        .per_path
        .iter()
        .filter_map(|e| classify_path(e, input.counters.alert_threshold))
        .collect();

    // Aggregate-level alerts appended after per-path alerts to preserve
    // source order readability.
    if input.counters.compute_count >= 10 && regression_ratio > 0.3 {
        alerts.push(Alert {
            severity: Severity::Critical,
            file_path: "<aggregate>".to_string(),
            message: format!(
                "aggregate regression ratio {:.1}% exceeds 30%",
                regression_ratio * 100.0
            ),
        });
    }
    if input.counters.streak_alert_count > 0
        && !alerts
            .iter()
            .any(|a| matches!(a.severity, Severity::Critical))
    {
        alerts.push(Alert {
            severity: Severity::Warning,
            file_path: "<aggregate>".to_string(),
            message: format!(
                "{} streak alert(s) fired in this window",
                input.counters.streak_alert_count
            ),
        });
    }

    let metrics = Metrics {
        total_computes: input.counters.compute_count,
        regression_ratio,
        improvement_ratio,
        alert_count: alerts.len() as u32,
        health_score,
    };

    Output {
        summary: build_summary(&input.counters, regression_ratio, improvement_ratio),
        alerts,
        metrics,
    }
}

// ---------------------------------------------------------------------------
// Guest trait implementation
// ---------------------------------------------------------------------------

struct Component;

impl Guest for Component {
    fn list_capabilities() -> Vec<String> {
        vec![CAPABILITY.to_string()]
    }

    fn invoke(request: InvokeRequest) -> Result<InvokeResponse, InvokeError> {
        if request.capability != CAPABILITY {
            return Err(InvokeError::UnknownCapability(request.capability));
        }

        let args_str = match core::str::from_utf8(&request.args) {
            Ok(s) => s,
            Err(e) => {
                return Err(InvokeError::InvalidArgs(format!(
                    "args not UTF-8: {e}"
                )));
            }
        };

        let input: Input = match serde_json::from_str(args_str) {
            Ok(v) => v,
            Err(e) => {
                return Err(InvokeError::InvalidArgs(format!(
                    "args not valid JSON per Input schema: {e}"
                )));
            }
        };

        let output = analyze(input);
        let body = match serde_json::to_vec(&output) {
            Ok(b) => b,
            Err(e) => {
                return Err(InvokeError::Internal(format!(
                    "failed to serialize output: {e}"
                )));
            }
        };

        Ok(InvokeResponse {
            exit_code: 0,
            stdout: body,
            stderr: Vec::new(),
            duration_ms: 0,
            logged: false,
        })
    }
}

export!(Component);

// ---------------------------------------------------------------------------
// Host-target unit tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    fn counters(
        compute: u64,
        regression: u64,
        improvement: u64,
        recovery: u64,
        streak_alert: u64,
    ) -> Counters {
        Counters {
            compute_count: compute,
            regression_count: regression,
            improvement_count: improvement,
            streak_alert_count: streak_alert,
            recovery_count: recovery,
            alert_threshold: 3,
        }
    }

    #[test]
    fn empty_counters_yield_zero_ratios() {
        let c = counters(0, 0, 0, 0, 0);
        let (r, i) = compute_ratios(&c);
        assert_eq!(r, 0.0);
        assert_eq!(i, 0.0);
    }

    #[test]
    fn ratios_are_clamped_between_zero_and_one() {
        let c = counters(10, 5, 10, 0, 0); // improvement > compute is pathological but possible
        let (r, i) = compute_ratios(&c);
        assert_eq!(r, 0.5);
        assert_eq!(i, 1.0);
    }

    #[test]
    fn health_score_perfect_when_no_regressions() {
        let c = counters(10, 0, 10, 0, 0);
        let (r, _) = compute_ratios(&c);
        let hs = compute_health_score(&c, r);
        assert!((hs - 1.0).abs() < 1e-6);
    }

    #[test]
    fn health_score_rewards_recoveries() {
        let c = counters(10, 2, 5, 2, 0);
        let (r, _) = compute_ratios(&c);
        let hs = compute_health_score(&c, r);
        // 1 - 0.2 = 0.8; + recovery_bonus = min(0.2, 2/2=1.0) = 0.2 → 1.0
        assert!((hs - 1.0).abs() < 1e-6);
    }

    #[test]
    fn health_score_recovery_bonus_capped_at_020() {
        let c = counters(10, 1, 5, 100, 0); // absurd recovery count
        let (r, _) = compute_ratios(&c);
        let hs = compute_health_score(&c, r);
        // 1 - 0.1 = 0.9; + min(0.2, 100/1=100) = 0.2 → 1.1
        assert!((hs - 1.1).abs() < 1e-6);
    }

    #[test]
    fn classify_path_critical_on_threshold() {
        let e = PerPathEntry {
            file_path: "/a.rs".into(),
            regression_streak: 3,
            improvement_streak: 0,
        };
        let a = classify_path(&e, 3).expect("alert");
        assert!(matches!(a.severity, Severity::Critical));
        assert!(a.message.contains("alert threshold"));
    }

    #[test]
    fn classify_path_warning_one_step_before_threshold() {
        let e = PerPathEntry {
            file_path: "/b.rs".into(),
            regression_streak: 2,
            improvement_streak: 0,
        };
        let a = classify_path(&e, 3).expect("alert");
        assert!(matches!(a.severity, Severity::Warning));
    }

    #[test]
    fn classify_path_info_on_improvement_streak() {
        let e = PerPathEntry {
            file_path: "/c.rs".into(),
            regression_streak: 0,
            improvement_streak: 5,
        };
        let a = classify_path(&e, 3).expect("alert");
        assert!(matches!(a.severity, Severity::Info));
    }

    #[test]
    fn classify_path_neutral_is_none() {
        let e = PerPathEntry {
            file_path: "/d.rs".into(),
            regression_streak: 0,
            improvement_streak: 1,
        };
        assert!(classify_path(&e, 3).is_none());
    }

    #[test]
    fn aggregate_alert_fires_above_30_percent_regression_with_10_plus_computes() {
        let input = Input {
            counters: counters(10, 4, 3, 0, 0),
            per_path: vec![],
        };
        let out = analyze(input);
        assert!(out
            .alerts
            .iter()
            .any(|a| matches!(a.severity, Severity::Critical) && a.file_path == "<aggregate>"));
    }

    #[test]
    fn aggregate_warning_on_streak_alerts_without_critical() {
        let input = Input {
            counters: counters(5, 1, 3, 0, 2),
            per_path: vec![],
        };
        let out = analyze(input);
        assert!(out
            .alerts
            .iter()
            .any(|a| matches!(a.severity, Severity::Warning) && a.file_path == "<aggregate>"));
    }

    #[test]
    fn analyze_output_is_deterministic() {
        let input = Input {
            counters: counters(10, 2, 7, 1, 1),
            per_path: vec![PerPathEntry {
                file_path: "/x.rs".into(),
                regression_streak: 2,
                improvement_streak: 0,
            }],
        };
        let a = analyze(input.clone());
        let b = analyze(input);
        assert_eq!(
            serde_json::to_string(&a).expect("serialize a"),
            serde_json::to_string(&b).expect("serialize b"),
        );
    }

    #[test]
    fn summary_uses_singular_for_one_recovery() {
        let c = counters(10, 2, 7, 1, 0);
        let summary = build_summary(&c, 0.2, 0.7);
        assert!(summary.contains("1 recovery"));
        assert!(!summary.contains("recoveries"));
    }

    #[test]
    fn summary_uses_plural_for_multiple_recoveries() {
        let c = counters(10, 2, 5, 3, 0);
        let summary = build_summary(&c, 0.2, 0.5);
        assert!(summary.contains("3 recoveries"));
    }
}
