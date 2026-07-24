//! Learning/RL health dimension: Wilson scores, Q-table richness, LinUCB arms.
//!
//! Queries `graph.db` tables (`learning_wilson`, `learning_qtable`, `learning_linucb`)
//! to derive an RL health score. Higher scores indicate a well-trained, active RL system.

use serde::{Deserialize, Serialize};
use touring_foundation::schema_guard;

/// RL reward trend direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RewardTrend {
    /// Average Wilson score increasing.
    Improving,
    /// Average Wilson score stable.
    Stable,
    /// Average Wilson score decreasing.
    Degrading,
    /// Not enough data to determine trend.
    Insufficient,
}

/// Learning dimension analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct LearningReport {
    /// Tools with Wilson score entries.
    pub wilson_tool_count: usize,
    /// Mean Wilson lower-bound score across tools.
    pub avg_wilson_score: f64,
    /// Total Q-table entries (richness proxy).
    pub qtable_entry_count: usize,
    /// LinUCB bandit arms loaded.
    pub linucb_arm_count: usize,
    /// Total pulls across all LinUCB arms.
    pub linucb_total_pulls: usize,
    /// Whether any RL updates have been recorded.
    pub rl_active: bool,
    /// Reward trend direction.
    pub reward_trend: RewardTrend,
    /// Composite learning score (0.0–1.0).
    pub score: f64,
}

/// Compute a normalised reward signal for the LinUCB `BlastRadius` arm.
///
/// Translates a raw symbol-index edge count into a `[0.1, 1.0]` reward
/// suitable for direct injection into the LinUCB bandit via
/// `touring_learning reward blast_radius <value>`.
///
/// The saturation point is 10 000 edges — a well-indexed medium-to-large
/// codebase. Below that the reward scales linearly; above it, it clamps at 1.0.
/// A floor of 0.1 ensures the arm is never starved even on cold start.
///
/// # Usage in touring-hooks
///
/// `touring-hooks/src/cli_e2e.rs::phase_learning()` calls this after computing
/// the blast radius to inject feedback into the RL system:
///
/// ```
/// use touring_analysis::compute_blast_reward;
///
/// let edge_count = 4_200_usize; // from symbol_store.stats().edge_count
/// let reward = compute_blast_reward(edge_count);
/// assert!(reward >= 0.1 && reward <= 1.0);
/// ```
pub fn compute_blast_reward(edge_count: usize) -> f64 {
    const SATURATION: f64 = 10_000.0;
    (edge_count as f64 / SATURATION).clamp(0.1, 1.0)
}

/// Compute an RL reward signal from an overall analysis quality report.
///
/// Maps `composite_score` linearly to `[0.1, 1.0]`.  A healthy codebase
/// (score ≥ 0.9) yields a near-maximum reward; a degraded codebase
/// (score = 0.0) yields the floor value 0.1 to keep the bandit exploring.
///
/// # Usage (G2 — bidirectional RL flywheel)
///
/// The reward is derived directly from `composite_score`:
/// a score of `0.82` yields a reward of `0.82`, clamped to `[0.1, 1.0]`.
/// Construct a full `CodeHealthReport` via `AnalysisPipeline::run()` in practice.
pub fn analysis_reward_from_report(report: &crate::health::CodeHealthReport) -> f64 {
    report.composite_score.clamp(0.1, 1.0)
}

/// Analyze RL/learning health from graph.db.
///
/// Queries Wilson scores, Q-table entry count, and LinUCB arm state.
/// Returns neutral scores (0.5) when tables are empty or missing.
pub fn analyze_learning(graph_conn: &rusqlite::Connection) -> LearningReport {
    let wilson_table = schema_guard::GRAPH_TABLE_LEARNING_WILSON;
    let qtable_table = schema_guard::GRAPH_TABLE_LEARNING_QTABLE;
    let linucb_table = schema_guard::GRAPH_TABLE_LEARNING_LINUCB;

    // Wilson tool count + average score
    let wilson_tool_count: i64 = graph_conn
        .query_row(&format!("SELECT COUNT(*) FROM {wilson_table}"), [], |r| {
            r.get(0)
        })
        .unwrap_or(0);

    let avg_wilson_score: f64 = graph_conn
        .query_row(
            &format!("SELECT COALESCE(AVG(wilson_score), 0.0) FROM {wilson_table}"),
            [],
            |r| r.get(0),
        )
        .unwrap_or(0.0);

    // Q-table entry count
    let qtable_entry_count: i64 = graph_conn
        .query_row(&format!("SELECT COUNT(*) FROM {qtable_table}"), [], |r| {
            r.get(0)
        })
        .unwrap_or(0);

    // LinUCB arms
    let linucb_arm_count: i64 = graph_conn
        .query_row(&format!("SELECT COUNT(*) FROM {linucb_table}"), [], |r| {
            r.get(0)
        })
        .unwrap_or(0);

    let linucb_total_pulls: i64 = graph_conn
        .query_row(
            &format!("SELECT COALESCE(SUM(pulls), 0) FROM {linucb_table}"),
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    // RL active: any data in Wilson or Q-table
    let rl_active = wilson_tool_count > 0 || qtable_entry_count > 0;

    // Reward trend: compare top-half vs bottom-half Wilson scores by ID ordering.
    // With sorted IDs, newer entries have higher IDs. This is a rough proxy.
    let reward_trend = compute_reward_trend(graph_conn, wilson_table, wilson_tool_count);

    // Score computation
    let qtable_richness = ((qtable_entry_count as f64) / 100.0).clamp(0.0, 1.0);
    let wilson_component = avg_wilson_score.clamp(0.0, 1.0);
    let linucb_component = if linucb_arm_count > 0 { 1.0 } else { 0.0 };
    let active_component = if rl_active { 1.0 } else { 0.0 };

    let score = if !rl_active {
        // Neutral when no RL data
        0.5
    } else {
        (0.4 * qtable_richness
            + 0.3 * wilson_component
            + 0.2 * linucb_component
            + 0.1 * active_component)
            .clamp(0.0, 1.0)
    };

    LearningReport {
        wilson_tool_count: wilson_tool_count as usize,
        avg_wilson_score,
        qtable_entry_count: qtable_entry_count as usize,
        linucb_arm_count: linucb_arm_count as usize,
        linucb_total_pulls: linucb_total_pulls as usize,
        rl_active,
        reward_trend,
        score,
    }
}

/// Compute reward trend by comparing older vs newer Wilson entries.
fn compute_reward_trend(conn: &rusqlite::Connection, table: &str, total: i64) -> RewardTrend {
    if total < 4 {
        return RewardTrend::Insufficient;
    }

    let half = total / 2;

    // Older half average
    let older_avg: f64 = conn
        .query_row(
            &format!(
                "SELECT COALESCE(AVG(wilson_score), 0.0) FROM \
                 (SELECT wilson_score FROM {table} ORDER BY id ASC LIMIT ?1)"
            ),
            [half],
            |r| r.get(0),
        )
        .unwrap_or(0.0);

    // Newer half average
    let newer_avg: f64 = conn
        .query_row(
            &format!(
                "SELECT COALESCE(AVG(wilson_score), 0.0) FROM \
                 (SELECT wilson_score FROM {table} ORDER BY id DESC LIMIT ?1)"
            ),
            [half],
            |r| r.get(0),
        )
        .unwrap_or(0.0);

    let delta = newer_avg - older_avg;
    if delta > 0.05 {
        RewardTrend::Improving
    } else if delta < -0.05 {
        RewardTrend::Degrading
    } else {
        RewardTrend::Stable
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_graph_db() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().expect("open");
        conn.execute_batch(touring_foundation::schema::graph::GRAPH_SCHEMA_V8)
            .expect("schema");
        conn
    }

    #[test]
    fn test_empty_graph_db() {
        let conn = setup_graph_db();
        let report = analyze_learning(&conn);
        assert!(!report.rl_active);
        assert_eq!(report.wilson_tool_count, 0);
        assert_eq!(report.qtable_entry_count, 0);
        assert_eq!(report.linucb_arm_count, 0);
        assert_eq!(report.reward_trend, RewardTrend::Insufficient);
        // Neutral score when no data
        assert!(
            (report.score - 0.5).abs() < f64::EPSILON,
            "empty graph should return 0.5, got {}",
            report.score
        );
    }

    #[test]
    fn test_learning_with_wilson_data() {
        let conn = setup_graph_db();
        conn.execute_batch(
            "INSERT INTO learning_wilson (tool_name, successes, trials, wilson_score) \
             VALUES ('edit', 80, 100, 0.72);
             INSERT INTO learning_wilson (tool_name, successes, trials, wilson_score) \
             VALUES ('bash', 60, 100, 0.51);",
        )
        .expect("insert");
        let report = analyze_learning(&conn);
        assert_eq!(report.wilson_tool_count, 2);
        assert!(report.avg_wilson_score > 0.5);
        assert!(report.rl_active);
    }

    #[test]
    fn test_learning_with_qtable() {
        let conn = setup_graph_db();
        for i in 0..50 {
            conn.execute(
                "INSERT INTO learning_qtable (state_action, q_value) VALUES (?1, ?2)",
                rusqlite::params![format!("s{i}:a"), 0.5],
            )
            .expect("insert");
        }
        let report = analyze_learning(&conn);
        assert_eq!(report.qtable_entry_count, 50);
        assert!(report.rl_active);
        // qtable_richness = 50/100 = 0.5 → contributes 0.4*0.5 = 0.2
        assert!(report.score > 0.2);
    }

    #[test]
    fn test_learning_with_linucb() {
        let conn = setup_graph_db();
        conn.execute(
            "INSERT INTO learning_linucb (arm_id, pulls, cumulative_reward) \
             VALUES ('arm_1', 50, 40.0)",
            [],
        )
        .expect("insert");
        let report = analyze_learning(&conn);
        assert_eq!(report.linucb_arm_count, 1);
        assert_eq!(report.linucb_total_pulls, 50);
    }

    #[test]
    fn test_reward_trend_improving() {
        let conn = setup_graph_db();
        // Older entries: low scores; newer entries: high scores
        for i in 0..8 {
            let score = if i < 4 { 0.3 } else { 0.8 };
            conn.execute(
                "INSERT INTO learning_wilson (tool_name, successes, trials, wilson_score) \
                 VALUES (?1, 70, 100, ?2)",
                rusqlite::params![format!("tool_{i}"), score],
            )
            .expect("insert");
        }
        let report = analyze_learning(&conn);
        assert_eq!(report.reward_trend, RewardTrend::Improving);
    }

    #[test]
    fn test_reward_trend_degrading() {
        let conn = setup_graph_db();
        for i in 0..8 {
            let score = if i < 4 { 0.8 } else { 0.3 };
            conn.execute(
                "INSERT INTO learning_wilson (tool_name, successes, trials, wilson_score) \
                 VALUES (?1, 70, 100, ?2)",
                rusqlite::params![format!("tool_{i}"), score],
            )
            .expect("insert");
        }
        let report = analyze_learning(&conn);
        assert_eq!(report.reward_trend, RewardTrend::Degrading);
    }

    #[test]
    fn test_reward_trend_insufficient_with_few_tools() {
        let conn = setup_graph_db();
        conn.execute(
            "INSERT INTO learning_wilson (tool_name, successes, trials, wilson_score) \
             VALUES ('edit', 80, 100, 0.7)",
            [],
        )
        .expect("insert");
        let report = analyze_learning(&conn);
        assert_eq!(report.reward_trend, RewardTrend::Insufficient);
    }

    #[test]
    fn test_learning_score_bounded() {
        let conn = setup_graph_db();
        let report = analyze_learning(&conn);
        assert!(report.score >= 0.0 && report.score <= 1.0);
    }

    #[test]
    fn test_full_rl_system_scores_high() {
        let conn = setup_graph_db();
        // Rich Q-table
        for i in 0..150 {
            conn.execute(
                "INSERT INTO learning_qtable (state_action, q_value) VALUES (?1, 0.7)",
                [format!("s{i}:a")],
            )
            .expect("insert");
        }
        // Good Wilson scores
        conn.execute_batch(
            "INSERT INTO learning_wilson (tool_name, successes, trials, wilson_score) \
             VALUES ('edit', 90, 100, 0.85);
             INSERT INTO learning_wilson (tool_name, successes, trials, wilson_score) \
             VALUES ('bash', 80, 100, 0.72);",
        )
        .expect("insert");
        // LinUCB arms
        conn.execute(
            "INSERT INTO learning_linucb (arm_id, pulls, cumulative_reward) VALUES ('a1', 100, 80.0)",
            [],
        ).expect("insert");

        let report = analyze_learning(&conn);
        assert!(report.rl_active);
        // qtable=1.0*0.4 + wilson=0.785*0.3 + linucb=1.0*0.2 + active=1.0*0.1 = 0.4+0.236+0.2+0.1 = 0.936
        assert!(
            report.score > 0.8,
            "full RL system should score > 0.8, got {}",
            report.score
        );
    }
}
