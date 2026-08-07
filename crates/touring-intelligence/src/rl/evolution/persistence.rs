//! Learning state persistence to SQLite.
//!
//! Saves and loads QTable, WilsonRanker, and DriftDetector state.
//! Unified from touring/src/evolution/persistence.rs (446 LOC)

use crate::rl::bandit::linucb::LinUCBBandit;
use crate::rl::error::LearningError;
use crate::rl::ranking::wilson::{DriftDetector, WilsonRanker};
use crate::rl::rl::qtable::QTable;
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

/// Persists learning state to SQLite tables.
#[derive(Debug)]
pub struct LearningPersistence {
    db_path: std::path::PathBuf,
}

impl LearningPersistence {
    /// Construct a `LearningPersistence` targeting the SQLite database at `db_path`.
    pub fn new(db_path: impl Into<std::path::PathBuf>) -> Self {
        Self {
            db_path: db_path.into(),
        }
    }

    /// Create the learning state tables (Wilson, drift, QTable, LinUCB) if absent.
    pub fn ensure_tables(&self) -> Result<(), LearningError> {
        let conn = self.open_conn()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS learning_wilson (
                item_id TEXT PRIMARY KEY,
                successes INTEGER NOT NULL,
                trials INTEGER NOT NULL
            );
            CREATE TABLE IF NOT EXISTS learning_drift (
                metric TEXT PRIMARY KEY,
                values_json TEXT NOT NULL
            );
            CREATE TABLE IF NOT EXISTS learning_qtable (
                state_action TEXT PRIMARY KEY,
                q_value REAL NOT NULL,
                trace REAL NOT NULL DEFAULT 0.0
            );
            CREATE TABLE IF NOT EXISTS learning_linucb (
                arm_id INTEGER PRIMARY KEY,
                pulls INTEGER NOT NULL,
                cumulative_reward REAL NOT NULL,
                a_inv_flat TEXT NOT NULL,
                b_vec TEXT NOT NULL,
                total_pulls INTEGER NOT NULL DEFAULT 0
            );",
        )?;
        Ok(())
    }

    /// Persist all `WilsonRanker` statistics to SQLite; returns rows written.
    pub fn save_wilson(&self, ranker: &WilsonRanker) -> Result<usize, LearningError> {
        let conn = self.open_conn()?;
        let stats = ranker.all_stats();
        let mut count = 0;
        for (item_id, successes, trials) in &stats {
            conn.execute(
                "INSERT OR REPLACE INTO learning_wilson (item_id, successes, trials) VALUES (?1, ?2, ?3)",
                rusqlite::params![item_id, *successes as i64, *trials as i64],
            )?;
            count += 1;
        }
        Ok(count)
    }

    /// Load persisted statistics into a `WilsonRanker`; returns rows read.
    pub fn load_wilson(&self, ranker: &mut WilsonRanker) -> Result<usize, LearningError> {
        let conn = self.open_conn()?;
        let mut stmt = conn.prepare("SELECT item_id, successes, trials FROM learning_wilson")?;

        let mut count = 0;
        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, String>(0)?,
                row.get::<_, i64>(1)? as u64,
                row.get::<_, i64>(2)? as u64,
            ))
        })?;

        for row in rows {
            let (item_id, successes, trials) = row?;
            ranker.load_stats(&item_id, successes, trials);
            count += 1;
        }
        Ok(count)
    }

    /// Persist all `DriftDetector` metric histories to SQLite; returns rows written.
    pub fn save_drift(&self, detector: &DriftDetector) -> Result<usize, LearningError> {
        let conn = self.open_conn()?;
        let histories = detector.all_histories();
        let mut count = 0;
        for (metric, values) in &histories {
            let json = serde_json::to_string(values)?;
            conn.execute(
                "INSERT OR REPLACE INTO learning_drift (metric, values_json) VALUES (?1, ?2)",
                rusqlite::params![metric, json],
            )?;
            count += 1;
        }
        Ok(count)
    }

    /// Load persisted metric histories into a `DriftDetector`; returns rows read.
    pub fn load_drift(&self, detector: &mut DriftDetector) -> Result<usize, LearningError> {
        let conn = self.open_conn()?;
        let mut stmt = conn.prepare("SELECT metric, values_json FROM learning_drift")?;

        let mut count = 0;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
        })?;

        for row in rows {
            let (metric, json) = row?;
            let values: Vec<f64> = serde_json::from_str(&json)?;
            detector.load_history(&metric, values);
            count += 1;
        }
        Ok(count)
    }

    /// Persist all `QTable` Q-values to SQLite; returns rows written.
    pub fn save_qtable(&self, qtable: &QTable) -> Result<usize, LearningError> {
        let conn = self.open_conn()?;
        let entries = qtable.all_q_values();
        let mut count = 0;
        for (state, action, q_value) in &entries {
            let key = format!("{}:{}", state, action);
            conn.execute(
                "INSERT OR REPLACE INTO learning_qtable (state_action, q_value) VALUES (?1, ?2)",
                rusqlite::params![key, q_value],
            )?;
            count += 1;
        }
        Ok(count)
    }

    /// Load persisted Q-values into a `QTable`; returns rows read.
    pub fn load_qtable(&self, qtable: &mut QTable) -> Result<usize, LearningError> {
        let conn = self.open_conn()?;
        let mut stmt = conn.prepare("SELECT state_action, q_value FROM learning_qtable")?;

        let mut count = 0;
        let rows = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
        })?;

        for row in rows {
            let (key, q_value) = row?;
            if let Some((state_str, action_str)) = key.split_once(':')
                && let (Ok(state), Ok(action)) =
                    (state_str.parse::<u64>(), action_str.parse::<u64>())
            {
                qtable.load_q_value(state, action, q_value);
                count += 1;
            }
        }
        Ok(count)
    }

    /// Persist both ranker and drift state, returning `(wilson_rows, drift_rows)`.
    pub fn save_all(
        &self,
        ranker: &Arc<Mutex<WilsonRanker>>,
        drift: &Arc<Mutex<DriftDetector>>,
    ) -> Result<(usize, usize), LearningError> {
        self.ensure_tables()?;
        let wilson_count = {
            let r = ranker
                .lock()
                .map_err(|e| LearningError::Persistence(format!("mutex poisoned: {e}")))?;
            self.save_wilson(&r)?
        };
        let drift_count = {
            let d = drift
                .lock()
                .map_err(|e| LearningError::Persistence(format!("mutex poisoned: {e}")))?;
            self.save_drift(&d)?
        };
        Ok((wilson_count, drift_count))
    }

    /// Load both ranker and drift state, returning `(wilson_rows, drift_rows)`.
    pub fn load_all(
        &self,
        ranker: &Arc<Mutex<WilsonRanker>>,
        drift: &Arc<Mutex<DriftDetector>>,
    ) -> Result<(usize, usize), LearningError> {
        self.ensure_tables()?;
        let wilson_count = {
            let mut r = ranker
                .lock()
                .map_err(|e| LearningError::Persistence(format!("mutex poisoned: {e}")))?;
            self.load_wilson(&mut r)?
        };
        let drift_count = {
            let mut d = drift
                .lock()
                .map_err(|e| LearningError::Persistence(format!("mutex poisoned: {e}")))?;
            self.load_drift(&mut d)?
        };
        Ok((wilson_count, drift_count))
    }

    /// Save LinUCB bandit state to SQLite.
    ///
    /// Persists all 8 arm models (A_inv matrix, b vector, pulls, cumulative_reward)
    /// plus total_pulls. Uses JSON serialization for the \<Vec\>\<f64\> fields.
    pub fn save_linucb(&self, bandit: &LinUCBBandit) -> Result<usize, LearningError> {
        let conn = self.open_conn()?;
        let exported = bandit.export();
        let total_pulls = bandit.export_total_pulls();

        // Clear previous state and insert fresh
        conn.execute("DELETE FROM learning_linucb", [])?;

        let mut count = 0;
        for (arm_id, (pulls, cumulative_reward, a_inv_flat, b_vec)) in exported.iter().enumerate() {
            let a_inv_json = serde_json::to_string(a_inv_flat)?;
            let b_json = serde_json::to_string(b_vec)?;

            conn.execute(
                "INSERT INTO learning_linucb (arm_id, pulls, cumulative_reward, a_inv_flat, b_vec, total_pulls) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
                rusqlite::params![arm_id as i64, *pulls as i64, cumulative_reward, a_inv_json, b_json, total_pulls as i64],
            )?;
            count += 1;
        }
        Ok(count)
    }

    /// Load LinUCB bandit state from SQLite.
    ///
    /// Returns `Ok(None)` if no persisted state exists (empty table).
    /// Returns `Ok(Some(bandit))` with restored arm models and total_pulls.
    pub fn load_linucb(&self) -> Result<Option<LinUCBBandit>, LearningError> {
        let conn = self.open_conn()?;

        // Check if table has any rows
        let row_count: i64 =
            conn.query_row("SELECT COUNT(*) FROM learning_linucb", [], |row| row.get(0))?;

        if row_count == 0 {
            return Ok(None);
        }

        let mut stmt = conn
            .prepare("SELECT arm_id, pulls, cumulative_reward, a_inv_flat, b_vec, total_pulls FROM learning_linucb ORDER BY arm_id")?;

        let mut data: Vec<(u64, f64, Vec<f64>, Vec<f64>)> = Vec::new();
        let mut total_pulls: u64 = 0;

        let rows = stmt.query_map([], |row| {
            Ok((
                row.get::<_, i64>(0)?,    // arm_id
                row.get::<_, i64>(1)?,    // pulls
                row.get::<_, f64>(2)?,    // cumulative_reward
                row.get::<_, String>(3)?, // a_inv_flat JSON
                row.get::<_, String>(4)?, // b_vec JSON
                row.get::<_, i64>(5)?,    // total_pulls
            ))
        })?;

        for row in rows {
            let (_arm_id, pulls, cumulative_reward, a_inv_json, b_json, tp) = row?;

            let a_inv_flat: Vec<f64> = serde_json::from_str(&a_inv_json)?;
            let b_vec: Vec<f64> = serde_json::from_str(&b_json)?;

            data.push((pulls as u64, cumulative_reward, a_inv_flat, b_vec));
            total_pulls = tp as u64;
        }

        let mut bandit = LinUCBBandit::new();
        bandit.import(&data);
        bandit.import_total_pulls(total_pulls);

        Ok(Some(bandit))
    }

    /// Load hook events from `touring_hook_events` table with `id > since_id`.
    ///
    /// Returns `Vec<(id, event_type, tool_name, reward)>` ordered by id ASC,
    /// limited to `batch_size` rows (capped at 10,000). Returns empty vec on
    /// any DB error (table missing, connection failure, etc.) -- never panics.
    ///
    /// The `touring_hook_events` table is created by Python hooks (`_bridge.py`)
    /// with schema: `id INTEGER PK AUTOINCREMENT, event_type TEXT NOT NULL,
    /// tool_name TEXT, outcome TEXT, reward REAL NOT NULL DEFAULT 0.0,
    /// created_at INTEGER NOT NULL`.
    pub fn load_hook_events_since(
        &self,
        since_id: i64,
        batch_size: usize,
    ) -> Vec<(i64, String, String, f64)> {
        let conn = match self.open_conn() {
            Ok(c) => c,
            Err(_) => return Vec::new(),
        };

        // Check if table exists before querying (it's created by Python hooks)
        let table_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='touring_hook_events'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap_or(false);

        if !table_exists {
            return Vec::new();
        }

        let mut stmt = match conn.prepare(
            "SELECT id, event_type, COALESCE(tool_name, 'unknown'), reward
             FROM touring_hook_events
             WHERE id > ?1 AND reward IS NOT NULL
             ORDER BY id ASC
             LIMIT ?2",
        ) {
            Ok(s) => s,
            Err(_) => return Vec::new(),
        };

        let limit = batch_size.min(10_000) as i64;

        let rows = match stmt.query_map(rusqlite::params![since_id, limit], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, f64>(3)?,
            ))
        }) {
            Ok(r) => r,
            Err(_) => return Vec::new(),
        };

        rows.filter_map(|r| r.ok()).collect()
    }

    // ── Convenience methods for Cortex handlers (Sprint 0) ──────────

    /// Log a hook event to `touring_hook_events` table.
    /// Creates the table if it doesn't exist (idempotent).
    pub fn log_hook_event(
        &self,
        event_type: &str,
        tool_name: &str,
        outcome: &str,
        reward: f64,
    ) -> Result<(), LearningError> {
        let conn = self.open_conn()?;
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS touring_hook_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                event_type TEXT NOT NULL,
                tool_name TEXT,
                outcome TEXT,
                reward REAL NOT NULL DEFAULT 0.0,
                created_at INTEGER NOT NULL DEFAULT (strftime('%s','now')),
                accessed_at INTEGER NOT NULL DEFAULT (strftime('%s','now'))
            )",
        )?;

        conn.execute(
            "INSERT INTO touring_hook_events (event_type, tool_name, outcome, reward, created_at)
             VALUES (?1, ?2, ?3, ?4, strftime('%s','now'))",
            rusqlite::params![event_type, tool_name, outcome, reward],
        )?;
        Ok(())
    }

    /// Update Wilson score for an item (upsert: increment successes/trials).
    pub fn wilson_update(&self, item_id: &str, success: bool) -> Result<(), LearningError> {
        let conn = self.open_conn()?;
        let (s_inc, t_inc) = if success { (1, 1) } else { (0, 1) };
        conn.execute(
            "INSERT INTO learning_wilson (item_id, successes, trials)
             VALUES (?1, ?2, ?3)
             ON CONFLICT(item_id) DO UPDATE SET
                successes = successes + ?2,
                trials = trials + ?3",
            rusqlite::params![item_id, s_inc, t_inc],
        )?;
        Ok(())
    }

    /// Get Wilson top-k items by lower confidence bound.
    pub fn wilson_top_k(&self, k: usize) -> Result<Vec<(String, f64, u64, u64)>, LearningError> {
        let conn = self.open_conn()?;
        let mut stmt = conn.prepare(
            "SELECT item_id, successes, trials FROM learning_wilson
                 WHERE trials > 0 ORDER BY
                 (successes + 1.0) / (trials + 2.0) DESC LIMIT ?1",
        )?;

        let rows = stmt.query_map(rusqlite::params![k as i64], |row| {
            let id: String = row.get(0)?;
            let s: i64 = row.get(1)?;
            let t: i64 = row.get(2)?;
            let score = (s as f64 + 1.0) / (t as f64 + 2.0);
            Ok((id, score, s as u64, t as u64))
        })?;

        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    /// Record a drift metric value.
    pub fn drift_record(&self, metric: &str, value: f64) -> Result<(), LearningError> {
        let conn = self.open_conn()?;
        // Load existing values, append, keep last 100
        let existing: String = conn
            .query_row(
                "SELECT values_json FROM learning_drift WHERE metric = ?1",
                rusqlite::params![metric],
                |row| row.get(0),
            )
            .unwrap_or_else(|_| "[]".to_string());

        let mut values: Vec<f64> = serde_json::from_str(&existing).unwrap_or_default();
        values.push(value);
        if values.len() > 100 {
            values.drain(..values.len() - 100);
        }

        let json = serde_json::to_string(&values)?;

        conn.execute(
            "INSERT OR REPLACE INTO learning_drift (metric, values_json)
             VALUES (?1, ?2)",
            rusqlite::params![metric, json],
        )?;
        Ok(())
    }

    /// Detect drift for a metric. Returns (trend, mean, stddev).
    pub fn drift_detect(&self, metric: &str) -> Result<Option<(f64, f64, f64)>, LearningError> {
        let conn = self.open_conn()?;
        let json: String = conn.query_row(
            "SELECT values_json FROM learning_drift WHERE metric = ?1",
            rusqlite::params![metric],
            |row| row.get(0),
        )?;

        let values: Vec<f64> = serde_json::from_str(&json).unwrap_or_default();

        if values.len() < 5 {
            return Ok(None);
        }

        let n = values.len() as f64;
        let mean = values.iter().sum::<f64>() / n;
        let variance = values.iter().map(|v| (v - mean).powi(2)).sum::<f64>() / n;
        let stddev = variance.sqrt();

        // Simple trend: compare last 25% average vs first 25%
        let quarter = values.len() / 4;
        if quarter == 0 {
            return Ok(Some((0.0, mean, stddev)));
        }
        // SAFETY: quarter = len/4 and we early-return when quarter == 0 above,
        // so quarter <= len. Also len - quarter >= 0, making both slices valid.
        #[allow(clippy::indexing_slicing)]
        let first_avg = values[..quarter].iter().sum::<f64>() / quarter as f64;
        #[allow(clippy::indexing_slicing)]
        let last_avg = values[values.len() - quarter..].iter().sum::<f64>() / quarter as f64;
        let trend = last_avg - first_avg;

        Ok(Some((trend, mean, stddev)))
    }

    /// Get recent hook events within a time window (seconds from now).
    pub fn recent_hook_events(
        &self,
        window_secs: i64,
    ) -> Result<Vec<(i64, String, String, f64)>, LearningError> {
        let conn = self.open_conn()?;
        let table_exists: bool = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name='touring_hook_events'",
                [],
                |row| row.get::<_, i64>(0),
            )
            .map(|c| c > 0)
            .unwrap_or(false);

        if !table_exists {
            return Ok(Vec::new());
        }

        let mut stmt = conn.prepare(
            "SELECT id, event_type, COALESCE(tool_name, 'unknown'), reward
                 FROM touring_hook_events
                 WHERE created_at > (strftime('%s','now') - ?1)
                 ORDER BY id DESC LIMIT 1000",
        )?;

        let rows = stmt.query_map(rusqlite::params![window_secs], |row| {
            Ok((
                row.get::<_, i64>(0)?,
                row.get::<_, String>(1)?,
                row.get::<_, String>(2)?,
                row.get::<_, f64>(3)?,
            ))
        })?;

        Ok(rows.filter_map(|r| r.ok()).collect())
    }

    fn open_conn(&self) -> Result<Connection, LearningError> {
        Connection::open(&self.db_path).map_err(|e| {
            LearningError::Persistence(format!("failed to open {}: {}", self.db_path.display(), e))
        })
    }
}

#[cfg(test)]
mod tests {
    #![allow(clippy::indexing_slicing, unused_must_use)] // vecs asserted non-empty; QLearning::update TD error intentionally discarded in setup
    use super::*;
    use crate::rl::rl::qtable::QLearning;
    use tempfile::TempDir;

    #[test]
    fn test_ensure_tables() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("learning.db");
        let persistence = LearningPersistence::new(&db_path);
        persistence.ensure_tables().unwrap();

        let conn = Connection::open(&db_path).unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT COUNT(*) FROM sqlite_master WHERE type='table' AND name LIKE 'learning_%'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(count, 4);
    }

    #[test]
    fn test_wilson_roundtrip() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("learning.db");
        let persistence = LearningPersistence::new(&db_path);
        persistence.ensure_tables().unwrap();

        let mut ranker = WilsonRanker::new();
        for _ in 0..10 {
            ranker.record("tool_a", true);
        }
        for _ in 0..3 {
            ranker.record("tool_a", false);
        }

        let saved = persistence.save_wilson(&ranker).unwrap();
        assert_eq!(saved, 1);

        let mut loaded = WilsonRanker::new();
        let count = persistence.load_wilson(&mut loaded).unwrap();
        assert_eq!(count, 1);

        let orig_score = ranker.score("tool_a");
        let loaded_score = loaded.score("tool_a");
        assert!((orig_score - loaded_score).abs() < 0.01);
    }

    #[test]
    fn test_qtable_roundtrip() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("learning.db");
        let persistence = LearningPersistence::new(&db_path);
        persistence.ensure_tables().unwrap();

        let mut qtable = QTable::new();
        qtable.update(0, 0, 1.0, 1, None, true);
        qtable.update(0, 1, 2.0, 1, None, true);

        let saved = persistence.save_qtable(&qtable).unwrap();
        assert_eq!(saved, 2);

        let mut loaded = QTable::new();
        let count = persistence.load_qtable(&mut loaded).unwrap();
        assert_eq!(count, 2);
        assert!((qtable.get_q(0, 0) - loaded.get_q(0, 0)).abs() < 1e-10);
    }

    #[test]
    fn test_save_load_linucb_roundtrip() {
        use crate::rl::bandit::linucb::{NUM_ARMS, extract_features};

        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("learning.db");
        let persistence = LearningPersistence::new(&db_path);
        persistence.ensure_tables().unwrap();

        // Create bandit and train it
        let mut bandit = LinUCBBandit::new();
        let features = extract_features("python", 50, 5, 0, 2);
        bandit.update(0, &features, 1.0);
        bandit.update(3, &features, 0.5);
        bandit.update(0, &features, 0.8);

        // Save
        let saved = persistence.save_linucb(&bandit).unwrap();
        assert_eq!(saved, NUM_ARMS);

        // Load
        let loaded = persistence.load_linucb().unwrap();
        assert!(loaded.is_some(), "should load persisted bandit");
        let mut loaded_bandit = loaded.unwrap();

        // Verify total_pulls preserved
        assert_eq!(
            loaded_bandit.total_pulls(),
            bandit.total_pulls(),
            "total_pulls must match"
        );

        // Verify same selection behavior
        let (arm1, score1) = bandit.select_arm(&features);
        let (arm2, score2) = loaded_bandit.select_arm(&features);
        assert_eq!(arm1, arm2, "same arm should be selected after load");
        assert!(
            (score1 - score2).abs() < 1e-10,
            "scores should match: {} vs {}",
            score1,
            score2
        );

        // Verify arm stats match
        let stats1 = bandit.arm_stats();
        let stats2 = loaded_bandit.arm_stats();
        for i in 0..NUM_ARMS {
            assert_eq!(stats1[i].1, stats2[i].1, "pulls should match for arm {}", i);
            assert!(
                (stats1[i].2 - stats2[i].2).abs() < 1e-10,
                "avg reward should match for arm {}: {} vs {}",
                i,
                stats1[i].2,
                stats2[i].2
            );
        }
    }

    #[test]
    fn test_load_linucb_empty_db() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("learning.db");
        let persistence = LearningPersistence::new(&db_path);
        persistence.ensure_tables().unwrap();

        // Load from empty table should return None
        let result = persistence.load_linucb().unwrap();
        assert!(result.is_none(), "empty DB should return None");
    }

    #[test]
    fn test_linucb_persistence_preserves_arm_stats() {
        use crate::rl::bandit::linucb::{NUM_ARMS, extract_features};

        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("learning.db");
        let persistence = LearningPersistence::new(&db_path);
        persistence.ensure_tables().unwrap();

        // Create bandit with varied training per arm
        let mut bandit = LinUCBBandit::new();
        let feat_py = extract_features("python", 50, 5, 0, 2);
        let feat_rs = extract_features("rust", 500, 30, 1, 4);

        // Arm 0: 5 pulls, high reward
        for _ in 0..5 {
            bandit.update(0, &feat_py, 1.0);
        }
        // Arm 3: 3 pulls, medium reward
        for _ in 0..3 {
            bandit.update(3, &feat_rs, 0.5);
        }
        // Arm 7: 2 pulls, low reward
        for _ in 0..2 {
            bandit.update(7, &feat_py, 0.1);
        }

        // Save and load
        persistence.save_linucb(&bandit).unwrap();
        let loaded = persistence.load_linucb().unwrap().unwrap();

        // Verify per-arm pull counts
        let orig_stats = bandit.arm_stats();
        let loaded_stats = loaded.arm_stats();

        assert_eq!(orig_stats[0].1, 5, "arm 0 should have 5 pulls");
        assert_eq!(orig_stats[3].1, 3, "arm 3 should have 3 pulls");
        assert_eq!(orig_stats[7].1, 2, "arm 7 should have 2 pulls");

        for i in 0..NUM_ARMS {
            assert_eq!(
                orig_stats[i].1, loaded_stats[i].1,
                "arm {} pull count mismatch: {} vs {}",
                i, orig_stats[i].1, loaded_stats[i].1
            );
            assert!(
                (orig_stats[i].2 - loaded_stats[i].2).abs() < 1e-10,
                "arm {} avg reward mismatch: {} vs {}",
                i,
                orig_stats[i].2,
                loaded_stats[i].2
            );
        }

        // Verify total pulls
        assert_eq!(loaded.total_pulls(), 10, "total pulls should be 10");

        // Untouched arms should have 0 pulls
        for i in [1, 2, 4, 5, 6] {
            assert_eq!(
                loaded_stats[i].1, 0,
                "untouched arm {} should have 0 pulls",
                i
            );
        }
    }

    // ── load_hook_events_since tests ────────────────────────────────────────

    /// Helper: create the touring_hook_events table (mirrors Python _bridge.py schema)
    fn create_hook_events_table(conn: &Connection) {
        conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS touring_hook_events (
                id INTEGER PRIMARY KEY AUTOINCREMENT,
                event_type TEXT NOT NULL,
                tool_name TEXT,
                outcome TEXT,
                reward REAL NOT NULL DEFAULT 0.0,
                created_at INTEGER NOT NULL,
                accessed_at INTEGER NOT NULL DEFAULT (strftime('%s','now'))
            );",
        )
        .unwrap();
    }

    /// Helper: insert a hook event row, returns the inserted row id
    fn insert_hook_event(
        conn: &Connection,
        event_type: &str,
        tool_name: Option<&str>,
        reward: f64,
        created_at: i64,
    ) -> i64 {
        conn.execute(
            "INSERT INTO touring_hook_events (event_type, tool_name, outcome, reward, created_at)
             VALUES (?1, ?2, 'ok', ?3, ?4)",
            rusqlite::params![event_type, tool_name, reward, created_at],
        )
        .unwrap();
        conn.last_insert_rowid()
    }

    #[test]
    fn test_load_hook_events_since_returns_events() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("learning.db");
        let persistence = LearningPersistence::new(&db_path);

        let conn = Connection::open(&db_path).unwrap();
        create_hook_events_table(&conn);
        insert_hook_event(&conn, "PostToolUse", Some("Read"), 1.0, 1000);
        insert_hook_event(&conn, "PreToolUse", Some("Write"), 0.5, 1001);
        insert_hook_event(&conn, "PostToolUse", Some("Bash"), -1.0, 1002);
        drop(conn);

        let events = persistence.load_hook_events_since(0, 100);
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].1, "PostToolUse");
        assert_eq!(events[0].2, "Read");
        assert!((events[0].3 - 1.0).abs() < 1e-10);
        assert_eq!(events[2].2, "Bash");
    }

    #[test]
    fn test_load_hook_events_since_empty_db() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("learning.db");
        let persistence = LearningPersistence::new(&db_path);

        // DB exists but no touring_hook_events table at all
        let conn = Connection::open(&db_path).unwrap();
        conn.execute_batch("CREATE TABLE IF NOT EXISTS dummy (id INTEGER);")
            .unwrap();
        drop(conn);

        let events = persistence.load_hook_events_since(0, 100);
        assert!(events.is_empty());
    }

    #[test]
    fn test_load_hook_events_since_filters_by_id() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("learning.db");
        let persistence = LearningPersistence::new(&db_path);

        let conn = Connection::open(&db_path).unwrap();
        create_hook_events_table(&conn);
        let id1 = insert_hook_event(&conn, "PostToolUse", Some("Read"), 1.0, 1000);
        let _id2 = insert_hook_event(&conn, "PreToolUse", Some("Write"), 0.5, 1001);
        let _id3 = insert_hook_event(&conn, "PostToolUse", Some("Edit"), 0.8, 1002);
        drop(conn);

        // Only events with id > id1 should be returned
        let events = persistence.load_hook_events_since(id1, 100);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].2, "Write");
        assert_eq!(events[1].2, "Edit");
    }

    #[test]
    fn test_load_hook_events_since_respects_batch_size() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("learning.db");
        let persistence = LearningPersistence::new(&db_path);

        let conn = Connection::open(&db_path).unwrap();
        create_hook_events_table(&conn);
        for i in 0..10 {
            insert_hook_event(&conn, "PostToolUse", Some("Read"), 1.0, 1000 + i);
        }
        drop(conn);

        let events = persistence.load_hook_events_since(0, 3);
        assert_eq!(events.len(), 3);
    }

    #[test]
    fn test_load_hook_events_since_null_tool_name_coalesced() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("learning.db");
        let persistence = LearningPersistence::new(&db_path);

        let conn = Connection::open(&db_path).unwrap();
        create_hook_events_table(&conn);
        insert_hook_event(&conn, "SessionStart", None, 0.0, 1000);
        drop(conn);

        let events = persistence.load_hook_events_since(0, 100);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].2, "unknown");
    }

    #[test]
    fn test_load_hook_events_since_nonexistent_db() {
        let dir = TempDir::new().unwrap();
        let db_path = dir.path().join("nonexistent_subdir/learning.db");
        let persistence = LearningPersistence::new(&db_path);

        // Should return empty vec, not panic
        let events = persistence.load_hook_events_since(0, 100);
        assert!(events.is_empty());
    }
}
