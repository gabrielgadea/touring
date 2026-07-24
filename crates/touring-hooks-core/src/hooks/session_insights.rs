//! Cross-session knowledge transfer via session insights.
//!
//! Extracts valuable learnings at session end (Stop event) and
//! persists them as JSON for warm-starting the next session (SessionStart).
//!
//! Flow: Stop → extract_session_insights() → save() → [file]
//!       SessionStart → load_latest() → inject as context

use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::FileKnowledgeDB;

// A5 (2026-06-15): ErrorPatternInsight + EditedFileInsight relocated to
// touring-foundation (kernel) to break the knowledge↔session_insights cycle —
// the knowledge data layer (gotchas/analytics) returns these value types, while
// this module depends on FileKnowledgeDB. Re-exported here so every historical
// `crate::session_insights::{ErrorPatternInsight, EditedFileInsight}` path (and
// this module's own use) keeps resolving unchanged.
pub use touring_foundation::insights::{EditedFileInsight, ErrorPatternInsight};

/// Extracted insights from a session, serialized to JSON between sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionInsights {
    /// When insights were extracted (RFC 3339).
    pub timestamp: String,
    /// Session identifier.
    pub session_id: String,
    /// Top gotchas by hit count (most triggered first).
    pub top_gotchas: Vec<GotchaInsight>,
    /// Top error patterns observed in failed edits.
    pub top_error_patterns: Vec<ErrorPatternInsight>,
    /// Files most frequently edited during the session.
    pub top_edited_files: Vec<EditedFileInsight>,
    /// Total number of edits tracked.
    pub total_edits: u64,
    /// Total number of bash commands tracked.
    pub total_bash_commands: u64,
    /// Bash command success rate (0.0..=1.0).
    pub success_rate: f64,

    /// R14: Evolution-derived patterns (from touring-learning InsightEngine).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evolution_patterns: Vec<String>,

    /// R14: Tool effectiveness insights (from touring-learning InsightEngine).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tool_effectiveness: Vec<ToolEffectivenessInsight>,

    /// R14: RL convergence status at session end.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rl_convergence: Option<RlConvergenceInsight>,
}

/// A gotcha that fired during the session.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GotchaInsight {
    /// File/path pattern that triggered the gotcha.
    pub pattern: String,
    /// The gotcha warning message.
    pub gotcha: String,
    /// Number of times the gotcha fired in the session.
    pub hit_count: i64,
}

// ErrorPatternInsight + EditedFileInsight definitions relocated to
// touring-foundation::insights (A5, 2026-06-15); re-exported above.

/// R14: Insight about tool usage effectiveness (from touring-learning InsightEngine).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ToolEffectivenessInsight {
    /// Name of the tool.
    pub tool_name: String,
    /// Number of times the tool was used.
    pub usage_count: u32,
    /// Fraction of uses that succeeded (0.0–1.0).
    pub success_rate: f64,
    /// Mean latency per use, in milliseconds.
    pub avg_latency_ms: f64,
}

/// R14: RL convergence status snapshot (from touring-learning QLearningMetrics).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RlConvergenceInsight {
    /// Exponential moving average of TD error.
    pub td_error_ema: f64,
    /// Average reward observed.
    pub avg_reward: f64,
    /// Total number of RL updates applied.
    pub total_updates: u64,
    /// Whether the RL process is considered converging.
    pub is_converging: bool,
}

impl SessionInsights {
    /// Save insights to a timestamped JSON file in `data_dir`.
    ///
    /// Returns the path of the written file.
    pub fn save(&self, data_dir: &Path) -> Result<PathBuf, std::io::Error> {
        let filename = format!(
            "session_insights_{}.json",
            chrono::Utc::now().format("%Y%m%d_%H%M%S")
        );
        let path = data_dir.join(filename);
        let json = serde_json::to_string_pretty(self).map_err(std::io::Error::other)?;
        std::fs::write(&path, json)?;
        Ok(path)
    }

    /// Load the most recent insights file from `data_dir`.
    ///
    /// Finds all `session_insights_*.json` files, sorts by name (which
    /// embeds a timestamp), and deserializes the latest one.
    pub fn load_latest(data_dir: &Path) -> Option<Self> {
        let mut entries: Vec<_> = std::fs::read_dir(data_dir)
            .ok()?
            .filter_map(|e| e.ok())
            .filter(|e| {
                let name = e.file_name();
                let s = name.to_string_lossy();
                s.starts_with("session_insights_") && s.ends_with(".json")
            })
            .collect();

        // Sort by filename — timestamp suffix ensures chronological order.
        entries.sort_by_key(|e| e.file_name());

        let latest = entries.last()?;
        let content = std::fs::read_to_string(latest.path()).ok()?;
        serde_json::from_str(&content).ok()
    }

    /// Format a compact one-line summary for injection into session context.
    pub fn summary_line(&self) -> String {
        let gotcha_part = if self.top_gotchas.is_empty() {
            String::new()
        } else {
            let names: Vec<&str> = self
                .top_gotchas
                .iter()
                .take(3)
                .map(|g| g.pattern.as_str())
                .collect();
            format!(" | gotchas: {}", names.join(", "))
        };

        let error_part = if self.top_error_patterns.is_empty() {
            String::new()
        } else {
            let patterns: Vec<&str> = self
                .top_error_patterns
                .iter()
                .take(3)
                .map(|e| e.pattern.as_str())
                .collect();
            format!(" | errors: {}", patterns.join(", "))
        };

        format!(
            "Prior session: {} edits, {} cmds, {:.0}% success{}{}",
            self.total_edits,
            self.total_bash_commands,
            self.success_rate * 100.0,
            gotcha_part,
            error_part,
        )
    }
}

/// Trend comparison between two sessions.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SessionTrend {
    /// Change in bash success rate (positive = improvement).
    pub success_rate_delta: f64,
    /// Change in edit count (positive = more edits this session).
    pub edit_count_delta: i64,
    /// Gotcha patterns present in current session but not in prior.
    pub new_gotchas: Vec<String>,
    /// Overall direction: "improving", "degrading", or "stable".
    pub trend_direction: String,
}

/// Compare two session insights to compute a trend.
///
/// The trend direction is determined by `success_rate_delta`:
/// - `> 0.05`: "improving"
/// - `< -0.05`: "degrading"
/// - otherwise: "stable"
pub fn compute_trend(current: &SessionInsights, prior: &SessionInsights) -> SessionTrend {
    let success_rate_delta = current.success_rate - prior.success_rate;
    let edit_count_delta = current.total_edits as i64 - prior.total_edits as i64;

    // Gotcha patterns in current that were not present in prior
    let prior_patterns: std::collections::HashSet<&str> = prior
        .top_gotchas
        .iter()
        .map(|g| g.pattern.as_str())
        .collect();
    let new_gotchas: Vec<String> = current
        .top_gotchas
        .iter()
        .filter(|g| !prior_patterns.contains(g.pattern.as_str()))
        .map(|g| g.pattern.clone())
        .collect();

    let trend_direction = if success_rate_delta > 0.05 {
        "improving".to_string()
    } else if success_rate_delta < -0.05 {
        "degrading".to_string()
    } else {
        "stable".to_string()
    };

    SessionTrend {
        success_rate_delta,
        edit_count_delta,
        new_gotchas,
        trend_direction,
    }
}

/// Extract insights from the knowledge database at session end.
///
/// Queries the SQLite knowledge DB for:
/// - Top gotchas by hit count
/// - Top error patterns from edit history
/// - Most frequently edited files
/// - Aggregate stats (edits, bash commands, success rate)
pub fn extract_session_insights(db: &FileKnowledgeDB, session_id: &str) -> SessionInsights {
    let top_gotchas = extract_top_gotchas(db);
    let top_error_patterns = extract_top_error_patterns(db);
    let top_edited_files = extract_top_edited_files(db);

    let stats = db.stats().unwrap_or_default();

    let success_rate = compute_bash_success_rate(db);

    SessionInsights {
        timestamp: chrono::Utc::now().to_rfc3339(),
        session_id: session_id.to_string(),
        top_gotchas,
        top_error_patterns,
        top_edited_files,
        total_edits: stats.edit_count as u64,
        total_bash_commands: stats.bash_count as u64,
        success_rate,
        evolution_patterns: Vec::new(),
        tool_effectiveness: Vec::new(),
        rl_convergence: None,
    }
}

/// R14: Extract evolution-based insights from the RL system.
///
/// Called optionally when `QLearningMetrics` data is available.
/// Populates `rl_convergence` on the given `SessionInsights`.
pub fn extract_evolution_insights(
    insights: &mut SessionInsights,
    rl_metrics: Option<&touring_intelligence::rl::QLearningMetrics>,
) {
    if let Some(metrics) = rl_metrics {
        insights.rl_convergence = Some(RlConvergenceInsight {
            td_error_ema: metrics.td_error_ema(),
            avg_reward: metrics.avg_reward(),
            total_updates: metrics.total_updates(),
            is_converging: metrics.is_converging(),
        });
    }
}

// ── Internal extraction helpers ──────────────────────────────────────────

/// Top 10 gotchas ordered by hit_count descending.
fn extract_top_gotchas(db: &FileKnowledgeDB) -> Vec<GotchaInsight> {
    let all = db.list_gotchas();
    let mut with_hits: Vec<_> = all.into_iter().filter(|g| g.hit_count > 0).collect();
    with_hits.sort_by_key(|b| std::cmp::Reverse(b.hit_count));
    with_hits
        .into_iter()
        .take(10)
        .map(|g| GotchaInsight {
            pattern: g.pattern,
            gotcha: g.gotcha,
            hit_count: g.hit_count,
        })
        .collect()
}

/// Top error patterns from edit_history, grouped by error_pattern.
///
/// Uses direct SQL via the knowledge DB's connection (accessed through
/// a public helper we add here via the existing `count_edit_error_pattern` pattern).
fn extract_top_error_patterns(db: &FileKnowledgeDB) -> Vec<ErrorPatternInsight> {
    db.top_error_patterns(10)
}

/// Top edited files from edit_history, grouped by file_path.
fn extract_top_edited_files(db: &FileKnowledgeDB) -> Vec<EditedFileInsight> {
    db.top_edited_files(10)
}

/// Compute bash success rate from bash_outcomes table.
fn compute_bash_success_rate(db: &FileKnowledgeDB) -> f64 {
    db.bash_success_rate()
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing, clippy::len_zero)]
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_save_and_load_insights() {
        let temp = TempDir::new().expect("temp dir");
        let insights = SessionInsights {
            timestamp: "2026-03-19T00:00:00Z".to_string(),
            session_id: "test-session".to_string(),
            top_gotchas: vec![GotchaInsight {
                pattern: "rust_bridge".into(),
                gotcha: "Use matched_text not text".into(),
                hit_count: 5,
            }],
            top_error_patterns: vec![ErrorPatternInsight {
                pattern: "string_not_found".into(),
                occurrences: 3,
            }],
            top_edited_files: vec![EditedFileInsight {
                file_path: "src/main.rs".into(),
                edit_count: 7,
            }],
            total_edits: 10,
            total_bash_commands: 20,
            success_rate: 0.95,
            evolution_patterns: Vec::new(),
            tool_effectiveness: Vec::new(),
            rl_convergence: None,
        };

        let path = insights.save(temp.path()).expect("save");
        assert!(path.exists());

        let loaded = SessionInsights::load_latest(temp.path());
        assert!(loaded.is_some());
        let loaded = loaded.unwrap();
        assert_eq!(loaded.session_id, "test-session");
        assert_eq!(loaded.top_gotchas.len(), 1);
        assert_eq!(loaded.top_gotchas[0].hit_count, 5);
        assert_eq!(loaded.top_error_patterns.len(), 1);
        assert_eq!(loaded.top_edited_files.len(), 1);
        assert_eq!(loaded.top_edited_files[0].edit_count, 7);
    }

    #[test]
    fn test_load_latest_empty_dir() {
        let temp = TempDir::new().expect("temp dir");
        assert!(SessionInsights::load_latest(temp.path()).is_none());
    }

    #[test]
    fn test_load_latest_picks_newest() {
        let temp = TempDir::new().expect("temp dir");

        let older = SessionInsights {
            timestamp: "2026-01-01T00:00:00Z".into(),
            session_id: "old".into(),
            top_gotchas: vec![],
            top_error_patterns: vec![],
            top_edited_files: vec![],
            total_edits: 1,
            total_bash_commands: 1,
            success_rate: 0.5,
            evolution_patterns: Vec::new(),
            tool_effectiveness: Vec::new(),
            rl_convergence: None,
        };
        let newer = SessionInsights {
            timestamp: "2026-03-19T00:00:00Z".into(),
            session_id: "new".into(),
            top_gotchas: vec![],
            top_error_patterns: vec![],
            top_edited_files: vec![],
            total_edits: 5,
            total_bash_commands: 10,
            success_rate: 0.9,
            evolution_patterns: Vec::new(),
            tool_effectiveness: Vec::new(),
            rl_convergence: None,
        };

        // Write with explicit filenames to control ordering
        std::fs::write(
            temp.path().join("session_insights_20260101_000000.json"),
            serde_json::to_string(&older).unwrap(),
        )
        .unwrap();
        std::fs::write(
            temp.path().join("session_insights_20260319_000000.json"),
            serde_json::to_string(&newer).unwrap(),
        )
        .unwrap();

        let loaded = SessionInsights::load_latest(temp.path()).unwrap();
        assert_eq!(loaded.session_id, "new");
        assert_eq!(loaded.total_edits, 5);
    }

    #[test]
    fn test_load_ignores_non_insight_files() {
        let temp = TempDir::new().expect("temp dir");

        // Write a non-insight JSON file that should be ignored
        std::fs::write(temp.path().join("other_data.json"), r#"{"key": "value"}"#).unwrap();

        assert!(SessionInsights::load_latest(temp.path()).is_none());
    }

    #[test]
    fn test_summary_line_with_data() {
        let insights = SessionInsights {
            timestamp: "2026-03-19T00:00:00Z".into(),
            session_id: "s1".into(),
            top_gotchas: vec![
                GotchaInsight {
                    pattern: "bridge".into(),
                    gotcha: "g1".into(),
                    hit_count: 3,
                },
                GotchaInsight {
                    pattern: "compute".into(),
                    gotcha: "g2".into(),
                    hit_count: 1,
                },
            ],
            top_error_patterns: vec![ErrorPatternInsight {
                pattern: "string_not_found".into(),
                occurrences: 2,
            }],
            top_edited_files: vec![],
            total_edits: 15,
            total_bash_commands: 40,
            success_rate: 0.93,
            evolution_patterns: Vec::new(),
            tool_effectiveness: Vec::new(),
            rl_convergence: None,
        };

        let line = insights.summary_line();
        assert!(line.contains("15 edits"));
        assert!(line.contains("40 cmds"));
        assert!(line.contains("93% success"));
        assert!(line.contains("bridge"));
        assert!(line.contains("string_not_found"));
    }

    #[test]
    fn test_summary_line_empty() {
        let insights = SessionInsights {
            timestamp: "2026-03-19T00:00:00Z".into(),
            session_id: "s1".into(),
            top_gotchas: vec![],
            top_error_patterns: vec![],
            top_edited_files: vec![],
            total_edits: 0,
            total_bash_commands: 0,
            success_rate: 1.0,
            evolution_patterns: Vec::new(),
            tool_effectiveness: Vec::new(),
            rl_convergence: None,
        };

        let line = insights.summary_line();
        assert_eq!(line, "Prior session: 0 edits, 0 cmds, 100% success");
    }

    // ── S4.3: SessionTrend tests ───────────────────────────────────

    fn make_insights(
        success_rate: f64,
        total_edits: u64,
        gotcha_patterns: &[&str],
    ) -> SessionInsights {
        SessionInsights {
            timestamp: "2026-03-19T00:00:00Z".into(),
            session_id: "test".into(),
            top_gotchas: gotcha_patterns
                .iter()
                .map(|p| GotchaInsight {
                    pattern: p.to_string(),
                    gotcha: format!("desc for {p}"),
                    hit_count: 1,
                })
                .collect(),
            top_error_patterns: vec![],
            top_edited_files: vec![],
            total_edits,
            total_bash_commands: 10,
            success_rate,
            evolution_patterns: Vec::new(),
            tool_effectiveness: Vec::new(),
            rl_convergence: None,
        }
    }

    #[test]
    fn test_trend_improving() {
        let prior = make_insights(0.70, 10, &["bridge"]);
        let current = make_insights(0.90, 15, &["bridge"]);

        let trend = compute_trend(&current, &prior);

        assert_eq!(trend.trend_direction, "improving");
        assert!((trend.success_rate_delta - 0.20).abs() < 1e-10);
        assert_eq!(trend.edit_count_delta, 5);
        assert!(trend.new_gotchas.is_empty()); // "bridge" was in prior too
    }

    #[test]
    fn test_trend_degrading() {
        let prior = make_insights(0.95, 20, &[]);
        let current = make_insights(0.80, 5, &["new_gotcha"]);

        let trend = compute_trend(&current, &prior);

        assert_eq!(trend.trend_direction, "degrading");
        assert!(trend.success_rate_delta < -0.05);
        assert_eq!(trend.edit_count_delta, -15);
        assert_eq!(trend.new_gotchas, vec!["new_gotcha".to_string()]);
    }

    #[test]
    fn test_trend_stable() {
        let prior = make_insights(0.90, 10, &["a", "b"]);
        let current = make_insights(0.92, 12, &["a", "b"]);

        let trend = compute_trend(&current, &prior);

        assert_eq!(trend.trend_direction, "stable");
        assert!((trend.success_rate_delta - 0.02).abs() < 1e-10);
        assert_eq!(trend.edit_count_delta, 2);
        assert!(trend.new_gotchas.is_empty());
    }

    #[test]
    fn test_trend_new_gotchas_detected() {
        let prior = make_insights(0.85, 10, &["old_pattern"]);
        let current = make_insights(0.87, 12, &["old_pattern", "new_one", "another_new"]);

        let trend = compute_trend(&current, &prior);

        assert_eq!(trend.new_gotchas.len(), 2);
        assert!(trend.new_gotchas.contains(&"new_one".to_string()));
        assert!(trend.new_gotchas.contains(&"another_new".to_string()));
    }

    #[test]
    fn test_trend_boundary_just_within_stable() {
        // Delta of +0.04 — within stable range
        let prior = make_insights(0.80, 10, &[]);
        let current = make_insights(0.84, 10, &[]);

        let trend = compute_trend(&current, &prior);
        assert_eq!(trend.trend_direction, "stable");

        // Delta of -0.04 — within stable range
        let current2 = make_insights(0.76, 10, &[]);
        let trend2 = compute_trend(&current2, &prior);
        assert_eq!(trend2.trend_direction, "stable");
    }

    #[test]
    fn test_trend_boundary_just_outside_stable() {
        // Delta of +0.10 — clearly improving
        let prior = make_insights(0.80, 10, &[]);
        let current = make_insights(0.90, 10, &[]);

        let trend = compute_trend(&current, &prior);
        assert_eq!(trend.trend_direction, "improving");

        // Delta of -0.10 — clearly degrading
        let current2 = make_insights(0.70, 10, &[]);
        let trend2 = compute_trend(&current2, &prior);
        assert_eq!(trend2.trend_direction, "degrading");
    }

    // ── Existing knowledge DB tests ─────────────────────────────────

    #[test]
    fn test_extract_session_insights_empty_db() {
        let temp = TempDir::new().expect("temp dir");
        let db_path = temp.path().join("test.db");
        let db = FileKnowledgeDB::new(&db_path).unwrap();

        let insights = extract_session_insights(&db, "test-session");
        assert_eq!(insights.session_id, "test-session");
        assert!(insights.top_gotchas.is_empty());
        assert!(insights.top_error_patterns.is_empty());
        assert!(insights.top_edited_files.is_empty());
        assert_eq!(insights.total_edits, 0);
        assert_eq!(insights.total_bash_commands, 0);
        // No commands = 1.0 success rate (vacuous truth)
        assert!((insights.success_rate - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn test_extract_session_insights_with_data() {
        let temp = TempDir::new().expect("temp dir");
        let db_path = temp.path().join("test.db");
        let db = FileKnowledgeDB::new(&db_path).unwrap();

        // Add gotchas with hits
        let id1 = db
            .add_gotcha("bridge", "Use matched_text", "error", None)
            .unwrap();
        db.increment_gotcha_hit(id1);
        db.increment_gotcha_hit(id1);
        let _id2 = db
            .add_gotcha("compute", "Check Decimal", "warning", None)
            .unwrap();
        // id2 has 0 hits — should not appear in top_gotchas

        // Add edits
        db.record_edit("src/main.rs", "Edit", Some("fix import"))
            .unwrap();
        db.record_edit("src/main.rs", "Edit", Some("fix typo"))
            .unwrap();
        db.record_edit("src/lib.rs", "Edit", Some("add fn"))
            .unwrap();

        // Add edits with error patterns
        db.record_edit_with_error("src/main.rs", "Edit", None, Some("string_not_found"))
            .unwrap();
        db.record_edit_with_error("src/lib.rs", "Edit", None, Some("string_not_found"))
            .unwrap();

        // Add bash outcomes
        db.record_bash_outcome(&crate::BashOutcome {
            command: "cargo test".into(),
            command_short: "cargo".into(),
            command_hash: String::new(),
            exit_code: 0,
            success: true,
            error_pattern: None,
            file_context: None,
            executed_at: String::new(),
        })
        .unwrap();
        db.record_bash_outcome(&crate::BashOutcome {
            command: "ruff check".into(),
            command_short: "ruff".into(),
            command_hash: String::new(),
            exit_code: 1,
            success: false,
            error_pattern: Some("E501".into()),
            file_context: None,
            executed_at: String::new(),
        })
        .unwrap();

        let insights = extract_session_insights(&db, "rich-session");

        assert_eq!(insights.session_id, "rich-session");

        // Only gotchas with hits > 0
        assert_eq!(insights.top_gotchas.len(), 1);
        assert_eq!(insights.top_gotchas[0].pattern, "bridge");
        assert_eq!(insights.top_gotchas[0].hit_count, 2);

        // Error patterns
        assert!(!insights.top_error_patterns.is_empty());
        assert_eq!(insights.top_error_patterns[0].pattern, "string_not_found");
        assert_eq!(insights.top_error_patterns[0].occurrences, 2);

        // Top edited files (main.rs has 3 edits, lib.rs has 2)
        assert!(!insights.top_edited_files.is_empty());
        assert_eq!(insights.top_edited_files[0].file_path, "src/main.rs");
        assert_eq!(insights.top_edited_files[0].edit_count, 3);

        // Stats
        assert_eq!(insights.total_edits, 5); // 3 + 2
        assert_eq!(insights.total_bash_commands, 2);

        // Success rate: 1 success out of 2 = 0.5
        assert!((insights.success_rate - 0.5).abs() < f64::EPSILON);
    }
}
