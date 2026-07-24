//! Velocity, churn, and quality delta trend analysis.

use serde::{Deserialize, Serialize};
use touring_foundation::schema_guard;

/// Temporal trend report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TrendReport {
    /// Edit velocity: edits per day in the time window.
    pub edit_velocity: f64,
    /// Bash success rate in the time window (0.0–1.0).
    pub bash_success_rate: f64,
    /// Recent edits in the 1-day window.
    pub edits_1d: usize,
    /// Recent edits in the 7-day window.
    pub edits_7d: usize,
    /// Trend direction: improving, stable, or degrading.
    pub trend: TrendDirection,
    /// Composite temporal score (0.0–1.0).
    pub score: f64,
    /// Churn rate: fraction of files edited more than once in the last 7 days.
    ///
    /// A high churn rate (> 0.5) indicates instability or rework.
    #[serde(default)]
    pub churn_rate: f64,
    /// Error rate in the last 7 days: `1.0 − bash_success_rate`.
    #[serde(default)]
    pub error_rate_7d: f64,
    /// Daily edit distribution for the last 7 days.
    ///
    /// Index 0 = 6 days ago, index 6 = today.
    /// Useful for visualising activity trends and as input to drift detection.
    #[serde(default)]
    pub edit_distribution: Vec<f64>,
    /// KS statistic measuring distribution drift between current and previous week.
    ///
    /// Range [0.0, 1.0] — values above 0.3 indicate notable drift.
    /// `Some(v)` when computed via `touring_simd::DriftDetector` with the
    /// `simd-temporal` feature enabled; `None` when the feature is disabled
    /// (drift unmeasured, not zero).
    #[serde(default)]
    pub quality_drift: Option<f64>,
}

/// Direction of the trend.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum TrendDirection {
    /// The metric is trending upward (quality or activity getting better).
    Improving,
    /// The metric is holding steady within the noise threshold.
    Stable,
    /// The metric is trending downward (quality or activity getting worse).
    Degrading,
}

/// Analyze temporal trends from the knowledge DB.
pub fn analyze_trends(conn: &rusqlite::Connection) -> TrendReport {
    let edit_table = schema_guard::TABLE_EDIT_HISTORY;
    let bash_table = schema_guard::TABLE_BASH_OUTCOMES;

    // Edits in last 1 day
    let edits_1d: i64 = conn
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM {edit_table} WHERE edited_at > datetime('now', '-1 day')"
            ),
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    // Edits in last 7 days
    let edits_7d: i64 = conn
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM {edit_table} WHERE edited_at > datetime('now', '-7 days')"
            ),
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let edits_1d = edits_1d as usize;
    let edits_7d = edits_7d as usize;
    let edit_velocity = edits_7d as f64 / 7.0;

    // Bash success rate (7 days)
    let bash_total: i64 = conn
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM {bash_table} WHERE executed_at > datetime('now', '-7 days')"
            ),
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let bash_success: i64 = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM {bash_table} WHERE executed_at > datetime('now', '-7 days') AND exit_code = 0"),
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let bash_total = bash_total as usize;
    let bash_success = bash_success as usize;
    let bash_success_rate = if bash_total > 0 {
        bash_success as f64 / bash_total as f64
    } else {
        1.0 // No data = assume healthy
    };

    // Trend: compare 1d rate to 7d average
    let daily_avg_7d = edits_7d as f64 / 7.0;
    let trend = if daily_avg_7d < 0.1 {
        TrendDirection::Stable // Not enough data
    } else if edits_1d as f64 > daily_avg_7d * 1.5 {
        TrendDirection::Improving // Activity increasing
    } else if (edits_1d as f64) < daily_avg_7d * 0.5 {
        TrendDirection::Degrading // Activity decreasing
    } else {
        TrendDirection::Stable
    };

    // Churn rate: files edited > 1x in last 7 days / unique files edited.
    let churn_count: i64 = conn
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM (\
                    SELECT file_path FROM {edit_table} \
                    WHERE edited_at > datetime('now', '-7 days') \
                    GROUP BY file_path HAVING COUNT(*) > 1\
                )"
            ),
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let unique_files_7d: i64 = conn
        .query_row(
            &format!(
                "SELECT COUNT(DISTINCT file_path) FROM {edit_table} \
                 WHERE edited_at > datetime('now', '-7 days')"
            ),
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let churn_rate = if unique_files_7d > 0 {
        (churn_count as f64 / unique_files_7d as f64).clamp(0.0, 1.0)
    } else {
        0.0
    };

    let error_rate_7d = (1.0 - bash_success_rate).clamp(0.0, 1.0);

    // Daily edit distribution: 7 slots, index 0 = 6 days ago, index 6 = today.
    let mut edit_distribution = vec![0.0f64; 7];
    {
        let sql = format!(
            "SELECT CAST((julianday('now') - julianday(edited_at)) AS INTEGER) AS days_ago, \
             COUNT(*) FROM {edit_table} \
             WHERE edited_at > datetime('now', '-7 days') \
             GROUP BY days_ago"
        );
        if let Ok(mut stmt) = conn.prepare(&sql) {
            let rows = stmt.query_map([], |r| {
                let days_ago: i64 = r.get(0)?;
                let count: i64 = r.get(1)?;
                Ok((days_ago, count))
            });
            if let Ok(iter) = rows {
                for row in iter.flatten() {
                    let (days_ago, count) = row;
                    // days_ago=0 → today (index 6); days_ago=6 → index 0.
                    if (0..7).contains(&days_ago) {
                        let idx = (6 - days_ago) as usize;
                        if let Some(slot) = edit_distribution.get_mut(idx) {
                            *slot = count as f64;
                        }
                    }
                }
            }
        }
    }

    // Previous-week distribution for drift detection (days 7–13).
    let mut prev_distribution: [f64; 7] = [0.0; 7];
    {
        let sql = format!(
            "SELECT CAST((julianday('now') - julianday(edited_at)) AS INTEGER) - 7 AS days_ago, \
             COUNT(*) FROM {edit_table} \
             WHERE edited_at BETWEEN datetime('now', '-14 days') AND datetime('now', '-7 days') \
             GROUP BY days_ago"
        );
        if let Ok(mut stmt) = conn.prepare(&sql) {
            let rows = stmt.query_map([], |r| {
                let days_ago: i64 = r.get(0)?;
                let count: i64 = r.get(1)?;
                Ok((days_ago, count))
            });
            if let Ok(iter) = rows {
                for row in iter.flatten() {
                    let (days_ago, count) = row;
                    if (0..7).contains(&days_ago) {
                        let idx = (6 - days_ago) as usize;
                        if let Some(slot) = prev_distribution.get_mut(idx) {
                            *slot = count as f64;
                        }
                    }
                }
            }
        }
    }

    // KS drift statistic between current and previous week edit distributions.
    // Some(v) when simd-temporal feature is enabled, None when unavailable.
    #[cfg(feature = "simd-temporal")]
    let quality_drift: Option<f64> = {
        use touring_simd::{DriftDetection, DriftDetector};
        Some(DriftDetector::new().ks_statistic(&edit_distribution, &prev_distribution))
    };
    #[cfg(not(feature = "simd-temporal"))]
    let quality_drift: Option<f64> = None;

    // Score weighted heavily toward bash success (quality signal).
    // Edit velocity is a weak positive signal capped at 15% contribution
    // to avoid rewarding churn over stability.
    let velocity_bonus = (edit_velocity.min(5.0) / 5.0) * 0.15;
    let score = bash_success_rate * 0.85 + velocity_bonus;

    TrendReport {
        edit_velocity,
        bash_success_rate,
        edits_1d,
        edits_7d,
        trend,
        score: score.clamp(0.0, 1.0),
        churn_rate,
        error_rate_7d,
        edit_distribution,
        quality_drift,
    }
}

/// Compute the direction of quality trend over the last `n_sessions` daily windows.
///
/// Uses the bash command success rate as a quality proxy. Each window spans one calendar
/// day. A least-squares linear regression is applied over the `n_sessions` data points;
/// the sign of the slope determines the trend direction:
///
/// - slope > 0.01  → [`TrendDirection::Improving`]
/// - slope < -0.01 → [`TrendDirection::Degrading`]
/// - otherwise     → [`TrendDirection::Stable`]
///
/// A `tracing::warn!` is emitted when three or more consecutive data points fall below
/// the mean success rate by more than 5 percentage points (sustained quality decline).
///
/// Returns [`TrendDirection::Stable`] when fewer than two data points are available.
pub fn quality_trend(conn: &rusqlite::Connection, n_sessions: usize) -> TrendDirection {
    let bash_table = schema_guard::TABLE_BASH_OUTCOMES;

    // Collect daily bash success rates for the last n_sessions days.
    // day_offset 0 = today, day_offset n_sessions-1 = oldest window.
    let mut rates: Vec<f64> = Vec::with_capacity(n_sessions);
    for i in 0..n_sessions {
        let day_start = (n_sessions - i) as i64;
        let day_end = day_start - 1;

        let total: i64 = conn
            .query_row(
                &format!(
                    "SELECT COUNT(*) FROM {bash_table} \
                     WHERE executed_at BETWEEN datetime('now', '-{day_start} days') \
                     AND datetime('now', '-{day_end} days')"
                ),
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);

        if total == 0 {
            // No data for this window — skip rather than push a misleading 0.
            continue;
        }

        let success: i64 = conn
            .query_row(
                &format!(
                    "SELECT COUNT(*) FROM {bash_table} \
                     WHERE executed_at BETWEEN datetime('now', '-{day_start} days') \
                     AND datetime('now', '-{day_end} days') AND exit_code = 0"
                ),
                [],
                |r| r.get(0),
            )
            .unwrap_or(0);

        rates.push(success as f64 / total as f64);
    }

    // Need at least two points to compute a slope.
    if rates.len() < 2 {
        return TrendDirection::Stable;
    }

    // Least-squares linear regression slope.
    let n = rates.len() as f64;
    let xs: Vec<f64> = (0..rates.len()).map(|i| i as f64).collect();
    let x_mean = xs.iter().sum::<f64>() / n;
    let y_mean = rates.iter().sum::<f64>() / n;

    let numerator: f64 = xs
        .iter()
        .zip(rates.iter())
        .map(|(x, y)| (x - x_mean) * (y - y_mean))
        .sum();
    let denominator: f64 = xs.iter().map(|x| (x - x_mean).powi(2)).sum();

    let slope = if denominator.abs() < 1e-9 {
        0.0
    } else {
        numerator / denominator
    };

    // Warn when 3+ consecutive sessions fall below mean − 5%.
    let threshold = y_mean - 0.05;
    let mut consecutive_low = 0usize;
    for &r in &rates {
        if r < threshold {
            consecutive_low += 1;
            if consecutive_low >= 3 {
                tracing::warn!(
                    "quality_trend: declining for 3+ consecutive sessions (mean={y_mean:.3}, threshold={threshold:.3})"
                );
                break;
            }
        } else {
            consecutive_low = 0;
        }
    }

    if slope > 0.01 {
        TrendDirection::Improving
    } else if slope < -0.01 {
        TrendDirection::Degrading
    } else {
        TrendDirection::Stable
    }
}

/// Detect file paths with churn-indicating patterns using aho-corasick matching.
///
/// Matches patterns: `tmp/`, `_bak`, `.bak`, `_old`, `_backup`.
/// When the `simd-temporal-ac` feature is disabled, returns an empty vec.
pub fn detect_churn_patterns(file_paths: &[String]) -> Vec<String> {
    #[cfg(feature = "simd-temporal-ac")]
    {
        let patterns = ["tmp/", "_bak", ".bak", "_old", "_backup"];
        let Ok(ac) = aho_corasick::AhoCorasick::new(patterns) else {
            return Vec::new();
        };
        file_paths
            .iter()
            .filter(|p| ac.is_match(p.as_str()))
            .cloned()
            .collect()
    }
    #[cfg(not(feature = "simd-temporal-ac"))]
    {
        let _ = file_paths;
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_db() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().expect("open");
        conn.execute_batch(touring_foundation::schema::knowledge::KNOWLEDGE_SCHEMA_V8)
            .expect("schema");
        conn
    }

    #[test]
    fn test_empty_db_trends() {
        let conn = setup_db();
        let report = analyze_trends(&conn);
        assert_eq!(report.edits_1d, 0);
        assert_eq!(report.edits_7d, 0);
        assert_eq!(report.trend, TrendDirection::Stable);
    }

    #[test]
    fn test_with_recent_edits() {
        let conn = setup_db();
        for i in 0..5 {
            conn.execute(
                "INSERT INTO edit_history (file_path, edit_type, edited_at) VALUES (?1, 'edit', datetime('now'))",
                [format!("file_{i}.rs")],
            ).expect("insert");
        }
        let report = analyze_trends(&conn);
        assert_eq!(report.edits_1d, 5);
        assert_eq!(report.edits_7d, 5);
        assert!(report.edit_velocity > 0.0);
    }

    #[test]
    fn test_bash_success_rate() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO bash_outcomes (command, command_short, exit_code, executed_at) VALUES ('cargo test', 'cargo', 0, datetime('now'))",
            [],
        ).expect("insert");
        conn.execute(
            "INSERT INTO bash_outcomes (command, command_short, exit_code, executed_at) VALUES ('cargo test', 'cargo', 1, datetime('now'))",
            [],
        ).expect("insert");
        let report = analyze_trends(&conn);
        assert!((report.bash_success_rate - 0.5).abs() < f64::EPSILON);
    }

    #[test]
    fn test_error_rate_complements_success_rate() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO bash_outcomes (command, command_short, exit_code, executed_at) VALUES ('cargo build', 'cargo', 0, datetime('now'))",
            [],
        ).expect("insert");
        conn.execute(
            "INSERT INTO bash_outcomes (command, command_short, exit_code, executed_at) VALUES ('cargo build', 'cargo', 1, datetime('now'))",
            [],
        ).expect("insert");
        let report = analyze_trends(&conn);
        assert!(
            (report.error_rate_7d + report.bash_success_rate - 1.0).abs() < 1e-9,
            "error_rate_7d + bash_success_rate must equal 1.0"
        );
    }

    #[test]
    fn test_churn_rate_with_repeated_edits() {
        let conn = setup_db();
        // Edit same file twice → churned
        for _ in 0..2 {
            conn.execute(
                "INSERT INTO edit_history (file_path, edit_type, edited_at) VALUES ('hot.rs', 'edit', datetime('now'))",
                [],
            ).expect("insert");
        }
        // Edit unique file once → not churned
        conn.execute(
            "INSERT INTO edit_history (file_path, edit_type, edited_at) VALUES ('cold.rs', 'edit', datetime('now'))",
            [],
        ).expect("insert");
        let report = analyze_trends(&conn);
        // 1 churned file out of 2 unique files = 0.5
        assert!(
            (report.churn_rate - 0.5).abs() < 1e-9,
            "churn_rate should be 0.5, got {}",
            report.churn_rate
        );
    }

    #[test]
    fn test_churn_rate_zero_with_no_repeats() {
        let conn = setup_db();
        for i in 0..5 {
            conn.execute(
                &format!("INSERT INTO edit_history (file_path, edit_type, edited_at) VALUES ('file_{i}.rs', 'edit', datetime('now'))"),
                [],
            ).expect("insert");
        }
        let report = analyze_trends(&conn);
        assert_eq!(
            report.churn_rate, 0.0,
            "no repeated edits → churn_rate must be 0"
        );
    }

    #[test]
    fn test_edit_distribution_length() {
        let conn = setup_db();
        let report = analyze_trends(&conn);
        assert_eq!(
            report.edit_distribution.len(),
            7,
            "edit_distribution must always have exactly 7 elements"
        );
    }

    #[test]
    fn test_edit_distribution_today_slot() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO edit_history (file_path, edit_type, edited_at) VALUES ('a.rs', 'edit', datetime('now'))",
            [],
        ).expect("insert");
        let report = analyze_trends(&conn);
        assert_eq!(report.edit_distribution.len(), 7);
        // Index 6 = today
        let today = report
            .edit_distribution
            .get(6)
            .copied()
            .expect("7-bin distribution");
        assert!(
            today >= 1.0,
            "today's edit should appear at index 6, got {:?}",
            report.edit_distribution
        );
    }

    #[test]
    fn test_quality_drift_default_none() {
        let conn = setup_db();
        let report = analyze_trends(&conn);
        // When simd-temporal feature is disabled, drift is None (unmeasured, not zero).
        // When enabled, drift is Some(v) in [0.0, 1.0].
        if let Some(v) = report.quality_drift {
            assert!(
                v >= 0.0 && v <= 1.0,
                "quality_drift Some(v) must be in [0.0, 1.0], got {v}"
            );
        }
        // None is the expected value when simd-temporal is not enabled — that is correct.
    }

    // ── quality_trend tests ───────────────────────────────────────────────────

    #[test]
    fn test_quality_trend_empty_db_returns_stable() {
        let conn = setup_db();
        // No bash outcomes → no data points → Stable graceful default.
        let direction = quality_trend(&conn, 7);
        assert_eq!(
            direction,
            TrendDirection::Stable,
            "empty DB must return Stable"
        );
    }

    #[test]
    fn test_quality_trend_single_window_returns_stable() {
        let conn = setup_db();
        // Only one data point available → cannot compute slope → Stable.
        conn.execute(
            "INSERT INTO bash_outcomes (command, command_short, exit_code, executed_at) \
             VALUES ('cargo test', 'cargo', 0, datetime('now', '-1 days'))",
            [],
        )
        .expect("insert");
        let direction = quality_trend(&conn, 7);
        // One non-empty window produces one data point — need >= 2 for slope.
        assert_eq!(
            direction,
            TrendDirection::Stable,
            "single data point must return Stable"
        );
    }

    #[test]
    fn test_quality_trend_improving_slope() {
        let conn = setup_db();
        // Populate windows with improving success rates:
        // window 7 (oldest): 1 success / 4 total = 0.25
        // window 6:           2/4 = 0.50
        // window 5:           3/4 = 0.75
        // window 4:           4/4 = 1.00
        // Positive slope — should be Improving.
        let data: &[(i64, i64, i64)] = &[
            (7, 1, 3), // 1 success, 3 failures
            (6, 2, 2), // 2 success, 2 failures
            (5, 3, 1), // 3 success, 1 failure
            (4, 4, 0), // 4 success, 0 failures
        ];
        for (days_ago, successes, failures) in data {
            for _ in 0..*successes {
                conn.execute(
                    &format!(
                        "INSERT INTO bash_outcomes (command, command_short, exit_code, executed_at) \
                         VALUES ('cmd', 'cmd', 0, datetime('now', '-{days_ago} days'))"
                    ),
                    [],
                )
                .expect("insert success");
            }
            for _ in 0..*failures {
                conn.execute(
                    &format!(
                        "INSERT INTO bash_outcomes (command, command_short, exit_code, executed_at) \
                         VALUES ('cmd', 'cmd', 1, datetime('now', '-{days_ago} days'))"
                    ),
                    [],
                )
                .expect("insert failure");
            }
        }
        let direction = quality_trend(&conn, 8);
        assert_eq!(
            direction,
            TrendDirection::Improving,
            "rising success rate must produce Improving"
        );
    }

    #[test]
    fn test_quality_trend_degrading_slope() {
        let conn = setup_db();
        // Populate with declining success rates.
        let data: &[(i64, i64, i64)] = &[
            (7, 4, 0), // 1.00
            (6, 3, 1), // 0.75
            (5, 2, 2), // 0.50
            (4, 1, 3), // 0.25
        ];
        for (days_ago, successes, failures) in data {
            for _ in 0..*successes {
                conn.execute(
                    &format!(
                        "INSERT INTO bash_outcomes (command, command_short, exit_code, executed_at) \
                         VALUES ('cmd', 'cmd', 0, datetime('now', '-{days_ago} days'))"
                    ),
                    [],
                )
                .expect("insert success");
            }
            for _ in 0..*failures {
                conn.execute(
                    &format!(
                        "INSERT INTO bash_outcomes (command, command_short, exit_code, executed_at) \
                         VALUES ('cmd', 'cmd', 1, datetime('now', '-{days_ago} days'))"
                    ),
                    [],
                )
                .expect("insert failure");
            }
        }
        let direction = quality_trend(&conn, 8);
        assert_eq!(
            direction,
            TrendDirection::Degrading,
            "falling success rate must produce Degrading"
        );
    }

    #[test]
    fn test_quality_trend_flat_returns_stable() {
        let conn = setup_db();
        // Constant 50% success rate across 5 days → slope ≈ 0 → Stable.
        for days_ago in 1i64..=5 {
            for exit_code in [0i64, 1] {
                conn.execute(
                    &format!(
                        "INSERT INTO bash_outcomes (command, command_short, exit_code, executed_at) \
                         VALUES ('cmd', 'cmd', {exit_code}, datetime('now', '-{days_ago} days'))"
                    ),
                    [],
                )
                .expect("insert");
            }
        }
        let direction = quality_trend(&conn, 7);
        assert_eq!(
            direction,
            TrendDirection::Stable,
            "flat success rate must produce Stable"
        );
    }

    #[test]
    fn test_detect_churn_patterns_returns_empty_without_feature() {
        // Without `simd-temporal-ac` feature, detect_churn_patterns always returns empty.
        // simd-temporal-ac is NOT in the default feature set.
        let paths = vec![
            "src/tmp/scratch.rs".to_string(),
            "src/lib_bak.rs".to_string(),
            "src/clean.rs".to_string(),
        ];
        let result = detect_churn_patterns(&paths);
        #[cfg(not(feature = "simd-temporal-ac"))]
        assert!(
            result.is_empty(),
            "detect_churn_patterns must return empty when simd-temporal-ac is off"
        );
        // When simd-temporal-ac is on, confirm no panic and matches are returned.
        #[cfg(feature = "simd-temporal-ac")]
        assert!(
            result.len() >= 2,
            "tmp/ and _bak patterns should match at least 2 paths"
        );
    }
}
