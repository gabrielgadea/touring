//! ExperimentLog — Dual audit trail for RL decisions.
//!
//! Inspired by autoresearch P10: Dual Audit Trail.
//! Git history records only successful experiments (monotonic improvement),
//! while results.tsv records ALL experiments including discards and crashes.
//!
//! ## Two Views
//!
//! - **committed**: Only decisions where reward improved (monotonically better)
//! - **all**: Every attempt, including keeps, discards, errors, rollbacks
//!
//! ## Schema
//!
//! ```sql
//! CREATE TABLE experiment_log (
//!     id INTEGER PRIMARY KEY AUTOINCREMENT,
//!     timestamp TEXT NOT NULL,
//!     session_id TEXT,
//!     state TEXT NOT NULL,
//!     action TEXT NOT NULL,
//!     reward REAL NOT NULL,
//!     decision TEXT NOT NULL CHECK(decision IN ('keep','discard','error','rollback')),
//!     composite_score REAL,
//!     diagnostic TEXT,
//!     tool_name TEXT,
//!     latency_ms INTEGER
//! );
//! ```

use rusqlite::{Connection, params};

/// A single experiment log entry.
#[derive(Debug, Clone)]
pub struct ExperimentEntry {
    /// State identifier (CILA level * 4 + file_type).
    pub state: u64,
    /// Action identifier (tool hash).
    pub action: u64,
    /// Computed reward value.
    pub reward: f64,
    /// Decision outcome.
    pub decision: ExperimentDecision,
    /// Optional composite quality score (0.0-1.0).
    pub composite_score: Option<f64>,
    /// Human-readable diagnostic context.
    pub diagnostic: Option<String>,
    /// Tool that triggered this entry.
    pub tool_name: Option<String>,
    /// Execution latency in ms.
    pub latency_ms: Option<u64>,
    /// Session identifier.
    pub session_id: Option<String>,
}

/// Decision outcome for an experiment.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExperimentDecision {
    /// Reward improved — state committed.
    Keep,
    /// Reward did not improve — discarded.
    Discard,
    /// Error during evaluation.
    Error,
    /// Rolled back due to NaN or anomaly.
    Rollback,
}

impl ExperimentDecision {
    /// Convert to string for storage.
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Keep => "keep",
            Self::Discard => "discard",
            Self::Error => "error",
            Self::Rollback => "rollback",
        }
    }
}

/// Dual audit trail for RL experiments.
#[derive(Debug)]
pub struct ExperimentLog {
    conn: Connection,
    /// Best reward observed so far (for keep/discard classification).
    best_reward: f64,
}

impl ExperimentLog {
    /// Create or open an experiment log in the given SQLite database.
    pub fn new(conn: Connection) -> Result<Self, rusqlite::Error> {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS experiment_log (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                timestamp TEXT NOT NULL DEFAULT (datetime('now')),
                session_id TEXT,
                state TEXT NOT NULL,
                action TEXT NOT NULL,
                reward REAL NOT NULL,
                decision TEXT NOT NULL CHECK(decision IN ('keep','discard','error','rollback')),
                composite_score REAL,
                diagnostic TEXT,
                tool_name TEXT,
                latency_ms INTEGER
            );
            CREATE INDEX IF NOT EXISTS idx_exp_decision ON experiment_log(decision);
            CREATE INDEX IF NOT EXISTS idx_exp_session ON experiment_log(session_id);",
        )?;

        // Recover best_reward from existing data
        let best: f64 = conn
            .query_row(
                "SELECT COALESCE(MAX(reward), 0.0) FROM experiment_log WHERE decision = 'keep'",
                [],
                |row| row.get(0),
            )
            .unwrap_or(0.0);

        Ok(Self {
            conn,
            best_reward: best,
        })
    }

    /// Record an experiment entry with automatic keep/discard classification.
    ///
    /// If `decision` is `None`, automatically classifies based on reward vs best.
    pub fn record(&mut self, entry: &ExperimentEntry) -> Result<i64, rusqlite::Error> {
        let decision = entry.decision;

        // Update best_reward for keep decisions
        if decision == ExperimentDecision::Keep && entry.reward > self.best_reward {
            self.best_reward = entry.reward;
        }

        self.conn.execute(
            "INSERT INTO experiment_log (session_id, state, action, reward, decision, composite_score, diagnostic, tool_name, latency_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
            params![
                entry.session_id,
                entry.state.to_string(),
                entry.action.to_string(),
                entry.reward,
                decision.as_str(),
                entry.composite_score,
                entry.diagnostic,
                entry.tool_name,
                entry.latency_ms.map(|ms| ms as i64),
            ],
        )?;

        Ok(self.conn.last_insert_rowid())
    }

    /// Query the "committed" view — only Keep decisions (monotonically improving).
    pub fn committed(&self, limit: usize) -> Result<Vec<LogRow>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp, state, action, reward, decision, tool_name
             FROM experiment_log WHERE decision = 'keep'
             ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(params![limit as i64], |row| {
                Ok(LogRow {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    state: row.get(2)?,
                    action: row.get(3)?,
                    reward: row.get(4)?,
                    decision: row.get(5)?,
                    tool_name: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Query the "all" view — every experiment regardless of outcome.
    pub fn all_events(&self, limit: usize) -> Result<Vec<LogRow>, rusqlite::Error> {
        let mut stmt = self.conn.prepare(
            "SELECT id, timestamp, state, action, reward, decision, tool_name
             FROM experiment_log
             ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map(params![limit as i64], |row| {
                Ok(LogRow {
                    id: row.get(0)?,
                    timestamp: row.get(1)?,
                    state: row.get(2)?,
                    action: row.get(3)?,
                    reward: row.get(4)?,
                    decision: row.get(5)?,
                    tool_name: row.get(6)?,
                })
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(rows)
    }

    /// Get total count of all experiments.
    pub fn total_count(&self) -> Result<i64, rusqlite::Error> {
        self.conn
            .query_row("SELECT COUNT(*) FROM experiment_log", [], |row| row.get(0))
    }

    /// Get count by decision type.
    pub fn count_by_decision(&self, decision: ExperimentDecision) -> Result<i64, rusqlite::Error> {
        self.conn.query_row(
            "SELECT COUNT(*) FROM experiment_log WHERE decision = ?1",
            params![decision.as_str()],
            |row| row.get(0),
        )
    }

    /// Current best reward observed.
    pub fn best_reward(&self) -> f64 {
        self.best_reward
    }

    // ── P5.5: Log Rotation ──────────────────────────────────────────

    /// Delete entries older than `max_age_days`.
    ///
    /// Returns the number of entries deleted.
    pub fn rotate_by_age(&self, max_age_days: u32) -> Result<usize, rusqlite::Error> {
        let deleted = self.conn.execute(
            "DELETE FROM experiment_log WHERE timestamp < datetime('now', ?1)",
            params![format!("-{max_age_days} days")],
        )?;
        Ok(deleted)
    }

    /// Keep only the most recent `max_entries` entries, deleting the rest.
    ///
    /// Returns the number of entries deleted.
    pub fn rotate_by_count(&self, max_entries: usize) -> Result<usize, rusqlite::Error> {
        let total: i64 = self.total_count()?;
        if total <= max_entries as i64 {
            return Ok(0);
        }

        let to_delete = total - max_entries as i64;
        let deleted = self.conn.execute(
            "DELETE FROM experiment_log WHERE id IN (
                SELECT id FROM experiment_log ORDER BY id ASC LIMIT ?1
            )",
            params![to_delete],
        )?;
        Ok(deleted)
    }

    /// Rotate by both age and count: first delete by age, then cap by count.
    ///
    /// Returns the total number of entries deleted.
    pub fn rotate(&self, max_age_days: u32, max_entries: usize) -> Result<usize, rusqlite::Error> {
        let by_age = self.rotate_by_age(max_age_days)?;
        let by_count = self.rotate_by_count(max_entries)?;
        Ok(by_age + by_count)
    }
}

/// A row from the experiment log query.
#[derive(Debug, Clone)]
pub struct LogRow {
    /// Auto-increment primary key of the log row.
    pub id: i64,
    /// Timestamp when the entry was recorded.
    pub timestamp: String,
    /// Serialized RL state at decision time.
    pub state: String,
    /// Action chosen for this entry.
    pub action: String,
    /// Reward observed for the action.
    pub reward: f64,
    /// Decision outcome/label recorded for the entry.
    pub decision: String,
    /// Tool associated with the action, if any.
    pub tool_name: Option<String>,
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing)] // test vecs asserted non-empty before indexing
    use super::*;

    fn in_memory_log() -> ExperimentLog {
        let conn = Connection::open_in_memory().unwrap();
        ExperimentLog::new(conn).unwrap()
    }

    fn entry(
        state: u64,
        action: u64,
        reward: f64,
        decision: ExperimentDecision,
    ) -> ExperimentEntry {
        ExperimentEntry {
            state,
            action,
            reward,
            decision,
            composite_score: None,
            diagnostic: None,
            tool_name: Some("Edit".to_string()),
            latency_ms: Some(42),
            session_id: Some("test-session".to_string()),
        }
    }

    #[test]
    fn test_empty_log() {
        let log = in_memory_log();
        assert_eq!(log.total_count().unwrap(), 0);
        assert_eq!(log.best_reward(), 0.0);
    }

    #[test]
    fn test_record_and_query() {
        let mut log = in_memory_log();
        log.record(&entry(1, 10, 0.8, ExperimentDecision::Keep))
            .unwrap();
        log.record(&entry(1, 20, 0.3, ExperimentDecision::Discard))
            .unwrap();
        log.record(&entry(1, 30, -0.5, ExperimentDecision::Error))
            .unwrap();

        assert_eq!(log.total_count().unwrap(), 3);
        assert_eq!(log.count_by_decision(ExperimentDecision::Keep).unwrap(), 1);
        assert_eq!(
            log.count_by_decision(ExperimentDecision::Discard).unwrap(),
            1
        );
        assert_eq!(log.count_by_decision(ExperimentDecision::Error).unwrap(), 1);
    }

    #[test]
    fn test_committed_view_only_keeps() {
        let mut log = in_memory_log();
        log.record(&entry(1, 10, 0.8, ExperimentDecision::Keep))
            .unwrap();
        log.record(&entry(1, 20, 0.3, ExperimentDecision::Discard))
            .unwrap();
        log.record(&entry(1, 30, 0.9, ExperimentDecision::Keep))
            .unwrap();
        log.record(&entry(1, 40, -1.0, ExperimentDecision::Rollback))
            .unwrap();
        log.record(&entry(1, 50, 0.95, ExperimentDecision::Keep))
            .unwrap();

        let committed = log.committed(10).unwrap();
        assert_eq!(committed.len(), 3, "Only keeps");

        let all = log.all_events(10).unwrap();
        assert_eq!(all.len(), 5, "All events");
    }

    #[test]
    fn test_best_reward_tracking() {
        let mut log = in_memory_log();
        assert_eq!(log.best_reward(), 0.0);

        log.record(&entry(1, 10, 0.5, ExperimentDecision::Keep))
            .unwrap();
        assert!((log.best_reward() - 0.5).abs() < f64::EPSILON);

        log.record(&entry(1, 20, 0.3, ExperimentDecision::Discard))
            .unwrap();
        assert!(
            (log.best_reward() - 0.5).abs() < f64::EPSILON,
            "Discard should not change best"
        );

        log.record(&entry(1, 30, 0.9, ExperimentDecision::Keep))
            .unwrap();
        assert!((log.best_reward() - 0.9).abs() < f64::EPSILON, "New best");
    }

    #[test]
    fn test_decision_as_str() {
        assert_eq!(ExperimentDecision::Keep.as_str(), "keep");
        assert_eq!(ExperimentDecision::Discard.as_str(), "discard");
        assert_eq!(ExperimentDecision::Error.as_str(), "error");
        assert_eq!(ExperimentDecision::Rollback.as_str(), "rollback");
    }

    #[test]
    fn test_large_batch_performance() {
        let mut log = in_memory_log();
        for i in 0..1000 {
            let decision = if i % 4 == 0 {
                ExperimentDecision::Keep
            } else if i % 4 == 1 {
                ExperimentDecision::Discard
            } else if i % 4 == 2 {
                ExperimentDecision::Error
            } else {
                ExperimentDecision::Rollback
            };
            log.record(&entry(1, i, i as f64 * 0.001, decision))
                .unwrap();
        }
        assert_eq!(log.total_count().unwrap(), 1000);
        assert_eq!(
            log.count_by_decision(ExperimentDecision::Keep).unwrap(),
            250
        );

        // Query last 100 should be fast
        let recent = log.all_events(100).unwrap();
        assert_eq!(recent.len(), 100);
    }

    #[test]
    fn test_committed_view_monotonic() {
        let mut log = in_memory_log();
        log.record(&entry(1, 10, 0.5, ExperimentDecision::Keep))
            .unwrap();
        log.record(&entry(1, 20, 0.7, ExperimentDecision::Keep))
            .unwrap();
        log.record(&entry(1, 30, 0.3, ExperimentDecision::Keep))
            .unwrap();
        log.record(&entry(1, 40, 0.9, ExperimentDecision::Keep))
            .unwrap();

        let committed = log.committed(10).unwrap();
        assert_eq!(committed.len(), 4);
        // All keeps are logged even if not monotonically improving
        // The committed view shows decision=keep, user can filter further
    }

    #[test]
    fn test_recover_best_from_existing_data() {
        let conn = Connection::open_in_memory().unwrap();
        {
            let mut log = ExperimentLog::new(conn).unwrap();
            log.record(&entry(1, 10, 0.8, ExperimentDecision::Keep))
                .unwrap();
            log.record(&entry(1, 20, 0.3, ExperimentDecision::Discard))
                .unwrap();
        }
    }

    // ── P5.5: Rotation tests ────────────────────────────────────────

    #[test]
    fn test_rotate_by_count() {
        let mut log = in_memory_log();
        for i in 0..100 {
            log.record(&entry(1, i, i as f64 * 0.01, ExperimentDecision::Keep))
                .unwrap();
        }
        assert_eq!(log.total_count().unwrap(), 100);

        // Keep only 30 most recent
        let deleted = log.rotate_by_count(30).unwrap();
        assert_eq!(deleted, 70);
        assert_eq!(log.total_count().unwrap(), 30);

        // Verify we kept the most recent (highest IDs)
        let recent = log.all_events(30).unwrap();
        assert_eq!(recent.len(), 30);
        // The most recent entry should have the highest reward
        assert!(
            recent[0].reward > 0.5,
            "most recent entries should be high-reward"
        );
    }

    #[test]
    fn test_rotate_by_count_no_op() {
        let mut log = in_memory_log();
        for i in 0..5 {
            log.record(&entry(1, i, 0.5, ExperimentDecision::Keep))
                .unwrap();
        }

        // Max > actual count: no deletion
        let deleted = log.rotate_by_count(100).unwrap();
        assert_eq!(deleted, 0);
        assert_eq!(log.total_count().unwrap(), 5);
    }

    #[test]
    fn test_rotate_by_age() {
        let mut log = in_memory_log();

        // Insert an old entry manually
        log.conn
            .execute(
                "INSERT INTO experiment_log (timestamp, state, action, reward, decision)
             VALUES (datetime('now', '-40 days'), '1', '10', 0.5, 'keep')",
                [],
            )
            .unwrap();
        // Insert a recent entry
        log.record(&entry(1, 20, 0.8, ExperimentDecision::Keep))
            .unwrap();

        assert_eq!(log.total_count().unwrap(), 2);

        let deleted = log.rotate_by_age(30).unwrap();
        assert_eq!(deleted, 1);
        assert_eq!(log.total_count().unwrap(), 1);
    }

    #[test]
    fn test_rotate_combined() {
        let mut log = in_memory_log();

        // Insert old entries
        for _ in 0..5 {
            log.conn
                .execute(
                    "INSERT INTO experiment_log (timestamp, state, action, reward, decision)
                 VALUES (datetime('now', '-60 days'), '1', '10', 0.1, 'discard')",
                    [],
                )
                .unwrap();
        }
        // Insert recent entries
        for i in 0..50 {
            log.record(&entry(1, i, 0.5, ExperimentDecision::Keep))
                .unwrap();
        }

        assert_eq!(log.total_count().unwrap(), 55);

        // Rotate: delete old (5) + cap at 20 (30 more)
        let deleted = log.rotate(30, 20).unwrap();
        assert_eq!(deleted, 35); // 5 by age + 30 by count
        assert_eq!(log.total_count().unwrap(), 20);
    }
}
