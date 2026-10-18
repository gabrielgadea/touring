//! CLI evolution handlers — extracted from cli_handlers.rs (lines 576-878)
//!
//! This module contains the evolution analysis CLI handlers:
//! - `cli_evolution_drift`: Detects drift in edit frequency and bash success rate
//! - `cli_evolution_insights`: Generates insights from SQL + EvolutionAnalyzer
//! - `cli_evolution_tools`: Reports tool effectiveness via Wilson ranking
//!
//! Also includes private helpers:
//! - `populate_drift_from_sql`: Feeds per-day success rates to DriftDetector
//! - `populate_ranker_from_sql`: Populates WilsonRanker from SQL bash_outcomes
//! - `DriftMetric`: Shared drift metric structure

use crate::runtime::HookRuntime;
use rusqlite::params;
use touring_analysis::e2e::schema_guard;
use touring_intelligence::rl::evolution::InsightEngine;

// Shared response types available via crate::cli_handlers::{LearningStatus, GotchaEntry, etc}

// ─────────────────────────────────────────────────────────────────────────────
// Drift analysis helpers
// ─────────────────────────────────────────────────────────────────────────────

/// Feeds per-day success rates for each tool category into the EvolutionAnalyzer's DriftDetector.
fn populate_drift_from_sql(rt: &HookRuntime) {
    if let Some(ref analyzer) = rt.learning.evolution_analyzer {
        let db = &rt.ctx.knowledge;
        // For each tool, compute daily success rates for the last 14 days and record
        let tools = ["cargo", "rustc", "cargo clippy", "touring", "git"];
        for tool in tools {
            for days_ago in 0..14 {
                let total: i64 = db
                    .conn_ref()
                    .query_row(
                        &format!(
                            "SELECT COUNT(*) FROM {}
                         WHERE command_short LIKE ?1
                           AND executed_at > datetime('now', ?2)
                           AND executed_at <= datetime('now', ?3)",
                            schema_guard::TABLE_BASH_OUTCOMES
                        ),
                        params![
                            format!("{tool}%"),
                            format!("-{} days", days_ago + 1),
                            format!("-{} days", days_ago)
                        ],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                if total == 0 {
                    continue;
                }
                let successes: i64 = db
                    .conn_ref()
                    .query_row(
                        &format!(
                            "SELECT COUNT(*) FROM {}
                         WHERE command_short LIKE ?1
                           AND executed_at > datetime('now', ?2)
                           AND executed_at <= datetime('now', ?3)
                           AND success = 1",
                            schema_guard::TABLE_BASH_OUTCOMES
                        ),
                        params![
                            format!("{tool}%"),
                            format!("-{} days", days_ago + 1),
                            format!("-{} days", days_ago)
                        ],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                let rate = successes as f64 / total as f64;
                let metric = format!("{}_success_rate", tool);
                // Record every other day to avoid over-populating (sliding window = 100)
                if days_ago % 2 == 0 {
                    analyzer.record_metric(&metric, rate);
                }
            }
        }
        // Record overall bash success rate per day
        for days_ago in 0..14 {
            let total: i64 = db.conn_ref()
                .query_row(
                    &format!("SELECT COUNT(*) FROM {} WHERE executed_at > datetime('now', ?1) AND executed_at <= datetime('now', ?2)", schema_guard::TABLE_BASH_OUTCOMES),
                    params![format!("-{} days", days_ago + 1), format!("-{} days", days_ago)],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            if total == 0 {
                continue;
            }
            let successes: i64 = db.conn_ref()
                .query_row(
                    &format!("SELECT COUNT(*) FROM {} WHERE executed_at > datetime('now', ?1) AND executed_at <= datetime('now', ?2) AND success = 1", schema_guard::TABLE_BASH_OUTCOMES),
                    params![format!("-{} days", days_ago + 1), format!("-{} days", days_ago)],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            if days_ago % 2 == 0 {
                analyzer.record_metric("bash_success_rate", successes as f64 / total as f64);
            }
        }
    }
}

/// Populate the EvolutionAnalyzer's WilsonRanker from SQL bash_outcomes data.
fn populate_ranker_from_sql(rt: &HookRuntime) {
    if let Some(ref analyzer) = rt.learning.evolution_analyzer {
        let db = &rt.ctx.knowledge;
        let tools = [
            ("cargo", "Cargo build/test"),
            ("rustc", "Rust compiler"),
            ("cargo clippy", "Clippy linter"),
            ("touring", "Touring CLI"),
            ("git", "Git VCS"),
        ];
        for (tool_pat, _name) in tools {
            let total: i64 = db.conn_ref()
                .query_row(
                    &format!("SELECT COUNT(*) FROM {} WHERE command_short LIKE ?1 AND executed_at > datetime('now', '-7 days')", schema_guard::TABLE_BASH_OUTCOMES),
                    [format!("{tool_pat}%")],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            if total == 0 {
                continue;
            }
            let successes: i64 = db.conn_ref()
                .query_row(
                    &format!("SELECT COUNT(*) FROM {} WHERE command_short LIKE ?1 AND executed_at > datetime('now', '-7 days') AND success = 1", schema_guard::TABLE_BASH_OUTCOMES),
                    [format!("{tool_pat}%")],
                    |r| r.get(0),
                )
                .unwrap_or(0);
            // Record N times proportional to trial count (cap at 10 to avoid flooding)
            let record_n = total.min(10) as usize;
            for _ in 0..record_n {
                // Last recorded outcome is what matters for Wilson confidence
                let last_success = successes == total;
                analyzer.record_tool_outcome(tool_pat, last_success);
            }
        }
    }
}

#[derive(serde::Serialize)]
struct DriftMetric {
    pub name: String,
    pub current: f64,
    pub baseline: f64,
    pub trend: String,
    pub drift_detected: bool,
    pub source: String,
}

// ─────────────────────────────────────────────────────────────────────────────
// Evolution CLI handlers
// ─────────────────────────────────────────────────────────────────────────────

/// Detects drift in edit frequency and bash success rate, returning the metrics and alert level as JSON.
pub fn cli_evolution_drift(rt: &mut HookRuntime, _payload: &serde_json::Value) -> String {
    let db = &rt.ctx.knowledge;

    // Build SQL base metrics
    let recent_edits: i64 = db
        .conn_ref()
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM {} WHERE edited_at > datetime('now', '-1 days')",
                schema_guard::TABLE_EDIT_HISTORY
            ),
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let older_edits: i64 = db.conn_ref()
        .query_row(&format!("SELECT COUNT(*) FROM {} WHERE edited_at > datetime('now', '-2 days') AND edited_at <= datetime('now', '-1 days')", schema_guard::TABLE_EDIT_HISTORY), [], |r| r.get(0))
        .unwrap_or(0);
    let edit_trend = if older_edits > 0 {
        (recent_edits as f64 - older_edits as f64) / older_edits as f64
    } else {
        0.0
    };
    let edit_drift = edit_trend.abs() > 0.5;

    let recent_bash_total: i64 = db
        .conn_ref()
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM {} WHERE executed_at > datetime('now', '-1 days')",
                schema_guard::TABLE_BASH_OUTCOMES
            ),
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let recent_bash_success: i64 = db.conn_ref()
        .query_row(&format!("SELECT COUNT(*) FROM {} WHERE executed_at > datetime('now', '-1 days') AND success = 1", schema_guard::TABLE_BASH_OUTCOMES), [], |r| r.get(0))
        .unwrap_or(0);
    let older_bash_total: i64 = db.conn_ref()
        .query_row(&format!("SELECT COUNT(*) FROM {} WHERE executed_at > datetime('now', '-7 days') AND executed_at <= datetime('now', '-1 days')", schema_guard::TABLE_BASH_OUTCOMES), [], |r| r.get(0))
        .unwrap_or(0);
    let older_bash_success: i64 = db.conn_ref()
        .query_row(&format!("SELECT COUNT(*) FROM {} WHERE executed_at > datetime('now', '-7 days') AND executed_at <= datetime('now', '-1 days') AND success = 1", schema_guard::TABLE_BASH_OUTCOMES), [], |r| r.get(0))
        .unwrap_or(0);
    let recent_rate = if recent_bash_total > 0 {
        recent_bash_success as f64 / recent_bash_total as f64
    } else {
        1.0
    };
    let older_rate = if older_bash_total > 0 {
        older_bash_success as f64 / older_bash_total as f64
    } else {
        1.0
    };
    let bash_drift = (recent_rate - older_rate).abs() > 0.2;

    let total_gotchas: i64 = db
        .conn_ref()
        .query_row("SELECT COUNT(*) FROM gotchas", [], |r| r.get(0))
        .unwrap_or(0);
    let gotchas_with_hits: i64 = db
        .conn_ref()
        .query_row(
            "SELECT COUNT(*) FROM gotchas WHERE hit_count > 0",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let gotchas_prevented: i64 = db
        .conn_ref()
        .query_row(
            "SELECT COALESCE(SUM(prevented_errors), 0) FROM gotchas",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    let mut degrading_metrics: Vec<DriftMetric> = Vec::new();

    // SQL-based drift signals
    if edit_drift {
        degrading_metrics.push(DriftMetric {
            name: "edit_frequency".to_string(),
            current: recent_edits as f64,
            baseline: older_edits as f64,
            trend: if edit_trend > 0.0 {
                "increasing"
            } else {
                "decreasing"
            }
            .to_string(),
            drift_detected: true,
            source: format!("sql:{}", schema_guard::TABLE_EDIT_HISTORY),
        });
    }
    if bash_drift {
        let trend = if recent_rate < older_rate {
            "degrading"
        } else {
            "improving"
        };
        degrading_metrics.push(DriftMetric {
            name: "bash_success_rate".to_string(),
            current: recent_rate,
            baseline: older_rate,
            trend: trend.to_string(),
            drift_detected: recent_rate < older_rate - 0.1,
            source: "sql:bash_outcomes".to_string(),
        });
    }

    // Analyzer-based drift — populate from SQL then query
    let analyzer_drifts = if rt.learning.evolution_analyzer.is_some() {
        populate_drift_from_sql(rt);
        if let Some(ref analyzer) = rt.learning.evolution_analyzer {
            let all_results = analyzer.analyze_all();
            all_results
                .into_iter()
                .filter(|r| r.category == "drift_detection" && r.trend.needs_attention())
                .map(|r| DriftMetric {
                    name: r.metric,
                    current: r.value,
                    baseline: 0.0,
                    trend: r.trend.to_string(),
                    drift_detected: true,
                    source: "analyzer:DriftDetector".to_string(),
                })
                .collect::<Vec<_>>()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    let analyzer_drifts_nonempty = !analyzer_drifts.is_empty();
    for ad in analyzer_drifts {
        degrading_metrics.push(ad);
    }

    let detected = edit_drift || bash_drift || analyzer_drifts_nonempty;
    let metric_count = degrading_metrics.len();

    // P4.3: Self-correction loop — inject negative reward when drift is detected.
    // This triggers the OnlineRLEngine's TD learning to penalize the degraded behavior,
    // which propagates to LinUCB bandit and SelfOptimizer hyperparameter adjustment.
    if detected {
        // Compute drift severity: bash success rate drop is most serious.
        let drift_severity = if bash_drift {
            // Bash success rate drop — high severity
            -0.3 * (older_rate - recent_rate).abs().min(0.5)
        } else if edit_drift {
            // Edit frequency volatility — medium severity
            -0.1 * edit_trend.abs().min(1.0)
        } else {
            // Analyzer-based drift — severity depends on how many metrics degrade
            -0.2 * (metric_count as f64).min(3.0) / 3.0
        };

        if drift_severity < 0.0 {
            rt.learning.inject_reward(
                "evolution:drift_detected",
                drift_severity,
                "evolution_drift",
            );
            tracing::debug!("P4.3: injected drift penalty reward={:.4}", drift_severity);
        }

        // P4.3: Alert escalation for structural drift.
        // Triggered when 3+ metrics are degrading simultaneously (systemic problem).
        if metric_count >= 3 {
            tracing::warn!(
                "P4.3 STRUCTURAL DRIFT ALERT: {} degrading metrics detected. \
                Consider running 'touring learning reward edit -1.0 \"structural_drift\"' for full correction.",
                metric_count
            );
        }
    }

    serde_json::json!({
        "status": "ok",
        "note": "drift from SQL + EvolutionAnalyzer::DriftDetector; analyzer enhances SQL when available",
        "detected": detected,
        "metric_count": metric_count,
        "degrading_metrics": degrading_metrics,
        "alert_level": if metric_count >= 3 { "structural" } else if detected { "degraded" } else { "none" },
        "self_correction_applied": detected,
        "summary": {
            "recent_edits_24h": recent_edits,
            "older_edits_24h": older_edits,
            "edit_trend_pct": edit_trend * 100.0,
            "recent_bash_success_rate": recent_rate,
            "older_bash_success_rate": older_rate,
            "total_gotchas": total_gotchas,
            "gotchas_with_hits": gotchas_with_hits,
            "total_prevented_errors": gotchas_prevented,
        }
    }).to_string()
}

/// Generates evolution insights from SQL telemetry combined with the `EvolutionAnalyzer` as JSON.
pub fn cli_evolution_insights(rt: &mut HookRuntime, _payload: &serde_json::Value) -> String {
    let db = &rt.ctx.knowledge;

    let top_files: Vec<(String, i64)> = {
        let mut stmt = match db.conn_ref().prepare(
            &format!("SELECT file_path, COUNT(*) as cnt FROM {} WHERE edited_at > datetime('now', '-7 days') GROUP BY file_path ORDER BY cnt DESC LIMIT 10", schema_guard::TABLE_EDIT_HISTORY),
        ) {
            Ok(s) => s,
            Err(_) => return serde_json::json!({"error": "db query failed"}).to_string(),
        };
        stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    };

    let edit_types: Vec<(String, i64)> = {
        let mut stmt = match db.conn_ref().prepare(
            &format!("SELECT edit_type, COUNT(*) as cnt FROM {} WHERE edited_at > datetime('now', '-7 days') GROUP BY edit_type ORDER BY cnt DESC", schema_guard::TABLE_EDIT_HISTORY),
        ) {
            Ok(s) => s,
            Err(_) => return serde_json::json!({"error": "db query failed"}).to_string(),
        };
        stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    };

    let bash_total: i64 = db
        .conn_ref()
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM {} WHERE executed_at > datetime('now', '-7 days')",
                schema_guard::TABLE_BASH_OUTCOMES
            ),
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let bash_success: i64 = db.conn_ref()
        .query_row(&format!("SELECT COUNT(*) FROM {} WHERE executed_at > datetime('now', '-7 days') AND success = 1", schema_guard::TABLE_BASH_OUTCOMES), [], |r| r.get(0))
        .unwrap_or(0);
    let bash_failed: i64 = db.conn_ref()
        .query_row(&format!("SELECT COUNT(*) FROM {} WHERE executed_at > datetime('now', '-7 days') AND success = 0", schema_guard::TABLE_BASH_OUTCOMES), [], |r| r.get(0))
        .unwrap_or(0);

    let error_patterns: Vec<(String, i64)> = {
        let mut stmt = match db.conn_ref().prepare(
            &format!("SELECT COALESCE(error_pattern, '<unknown>'), COUNT(*) as cnt FROM {} WHERE executed_at > datetime('now', '-7 days') AND success = 0 GROUP BY error_pattern ORDER BY cnt DESC LIMIT 5", schema_guard::TABLE_BASH_OUTCOMES),
        ) {
            Ok(s) => s,
            Err(_) => return serde_json::json!({"error": "db query failed"}).to_string(),
        };
        stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, i64>(1)?)))
            .map(|rows| rows.filter_map(|r| r.ok()).collect())
            .unwrap_or_default()
    };

    let (gotcha_total, gotcha_hits, gotcha_prevented) = db.gotcha_stats();
    let resolved_gotchas: i64 = db
        .conn_ref()
        .query_row(
            "SELECT COUNT(*) FROM gotchas WHERE resolved_at IS NOT NULL",
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    // Analyzer-generated insights (Wilson + Drift + CILA from RlmMemory)
    let analyzer_insights: Vec<serde_json::Value> = if rt.learning.evolution_analyzer.is_some() {
        populate_ranker_from_sql(rt);
        if let Some(ref analyzer) = rt.learning.evolution_analyzer {
            let all_results = analyzer.analyze_all();
            let insights = InsightEngine::generate(&all_results);
            insights
                .into_iter()
                .map(|insight| {
                    serde_json::json!({
                        "axis": insight.axis.to_string(),
                        "category": insight.category,
                        "severity": insight.severity.to_string(),
                        "message": insight.message,
                        "evidence": insight.evidence,
                        "recommendation": insight.recommendation,
                    })
                })
                .collect()
        } else {
            Vec::new()
        }
    } else {
        Vec::new()
    };

    serde_json::json!({
        "status": "ok",
        "note": "insights from SQL (knowledge DB) + EvolutionAnalyzer::InsightEngine when available",
        "insights": {
            "top_edited_files": top_files,
            "edit_type_distribution": edit_types,
            "bash_outcomes": {
                "total": bash_total,
                "success": bash_success,
                "failed": bash_failed,
                "success_rate": if bash_total > 0 { bash_success as f64 / bash_total as f64 } else { 1.0 }
            },
            "top_error_patterns": error_patterns,
            "analyzer_insights": analyzer_insights,
        },
        "gotcha_coverage": {
            "total": gotcha_total,
            "with_hits": gotcha_hits,
            "prevented_errors": gotcha_prevented,
            "resolved": resolved_gotchas,
        }
    }).to_string()
}

/// Reports per-tool effectiveness ranked by Wilson lower-bound score as JSON.
pub fn cli_evolution_tools(rt: &mut HookRuntime, _payload: &serde_json::Value) -> String {
    let db = &rt.ctx.knowledge;
    let recent_edits = db.recent_edits_all(200).unwrap_or_default();

    // SQL-based tool effectiveness
    let sql_tool_effectiveness: Vec<serde_json::Value> = {
        let categories = [
            ("cargo", "Cargo build/test"),
            ("rustc", "Rust compiler"),
            ("cargo clippy", "Clippy linter"),
            ("touring", "Touring CLI"),
            ("git", "Git VCS"),
        ];
        categories
            .iter()
            .filter_map(|(cmd_pattern, name)| {
                let total: i64 = db.conn_ref()
                    .query_row(
                        &format!("SELECT COUNT(*) FROM {} WHERE command_short LIKE ?1 AND executed_at > datetime('now', '-7 days')", schema_guard::TABLE_BASH_OUTCOMES),
                        [format!("{cmd_pattern}%")],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                if total == 0 {
                    return None;
                }
                let successes: i64 = db.conn_ref()
                    .query_row(
                        &format!("SELECT COUNT(*) FROM {} WHERE command_short LIKE ?1 AND executed_at > datetime('now', '-7 days') AND success = 1", schema_guard::TABLE_BASH_OUTCOMES),
                        [format!("{cmd_pattern}%")],
                        |r| r.get(0),
                    )
                    .unwrap_or(0);
                let score = successes as f64 / total as f64;
                let trend = if score > 0.8 { "improving" } else if score > 0.5 { "stable" } else { "degrading" };
                Some(serde_json::json!({
                    "tool": name,
                    "pattern": cmd_pattern,
                    "trials": total,
                    "successes": successes,
                    "failures": total - successes,
                    "score": score,
                    "trend": trend,
                    "source": "sql:bash_outcomes"
                }))
            })
            .collect()
    };

    // Analyzer-based Wilson ranking
    let analyzer_tool_effectiveness: Vec<serde_json::Value> =
        if rt.learning.evolution_analyzer.is_some() {
            populate_ranker_from_sql(rt);
            if let Some(ref analyzer) = rt.learning.evolution_analyzer {
                let ranked = analyzer.rank_tools();
                ranked
                    .into_iter()
                    .map(|item| {
                        serde_json::json!({
                            "tool": item.id,
                            "score": item.score.lower,
                            "score_upper": item.score.upper,
                            "trials": item.trials,
                            "raw_rate": item.raw_rate,
                            "source": "analyzer:WilsonRanker"
                        })
                    })
                    .collect()
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };

    let unknown_subagent_count = recent_edits
        .iter()
        .filter(|e| {
            e.summary
                .as_deref()
                .is_some_and(|s| s.contains("agent") || s.contains("subagent"))
        })
        .count();

    serde_json::json!({
        "status": "ok",
        "note": "tool effectiveness from SQL + WilsonRanker when analyzer available",
        "recent_edit_count": recent_edits.len(),
        "tool_effectiveness": sql_tool_effectiveness,
        "analyzer_tool_effectiveness": analyzer_tool_effectiveness,
        "unknown_subagent_count": unknown_subagent_count,
        "recent_edit_summary": recent_edits.iter().take(5).map(|e| {
            serde_json::json!({
                "file": e.file_path.split('/').next_back().unwrap_or(&e.file_path),
                "type": e.edit_type,
                "summary": e.summary
            })
        }).collect::<Vec<_>>()
    })
    .to_string()
}
