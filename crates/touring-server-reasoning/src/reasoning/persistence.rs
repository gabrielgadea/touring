//! Persistence layer for TaskDecomposer via SQLite.
//! CheckpointManager provides WAL-backed persistence with event sourcing.

use chrono::{DateTime, Utc};
use rusqlite::{Connection, params};
use serde::{Deserialize, Serialize};
use std::path::Path;
use std::sync::Mutex;
use std::sync::atomic::{AtomicI64, AtomicU64, Ordering};

use super::decomposer::{
    ComplexityHint, DecomposeValidationMetrics, RetryPolicy, SubTask, SubTaskStatus, Task,
    TaskDecomposer,
};

/// Manages persistence of TaskDecomposer state to SQLite.
/// Checkpoints every N mutations or T seconds.
#[derive(Debug)]
pub struct CheckpointManager {
    conn: Mutex<Connection>,
    mutation_count: AtomicU64,
    last_checkpoint: AtomicI64,
    checkpoint_interval_mutations: u64,
    checkpoint_interval_secs: i64,
}

/// Serializable row representation of a `Task` as stored in SQLite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedTask {
    /// Unique identifier of the task.
    pub task_id: String,
    /// Category of the task (drives decomposition strategy).
    pub task_type: String,
    /// Human-readable description of the task.
    pub description: String,
    /// CILA complexity level (0-6) the task was classified at.
    pub cila_level: u8,
    /// RFC3339 timestamp of when the task was created.
    pub created_at: String,
    /// RFC3339 timestamp of the task's last update.
    pub updated_at: String,
    /// RFC3339 timestamp of when the task was archived, if archived.
    pub archived_at: Option<String>,
    /// Current lifecycle status of the task.
    pub status: String,
    /// Validation metrics collected for the task, if any.
    pub metrics: Option<DecomposeValidationMetrics>,
}

/// Serializable row representation of a `SubTask` as stored in SQLite.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PersistedSubTask {
    /// Unique identifier of the subtask.
    pub subtask_id: String,
    /// Identifier of the parent task this subtask belongs to.
    pub task_id: String,
    /// Human-readable description of the subtask.
    pub description: String,
    /// Subtask IDs that must complete before this one can run.
    pub depends_on: Vec<String>,
    /// Scheduling priority (higher runs first within a DAG layer).
    pub priority: u8,
    /// Current lifecycle status of the subtask.
    pub status: String,
    /// RFC3339 deadline by which the subtask should finish, if set.
    pub deadline: Option<String>,
    /// Action taken when the deadline is exceeded.
    pub deadline_behavior: String,
    // NOTE: review_required column removed from persistence (not in SubTask domain model)
    /// Complexity hint guiding decomposition and parallelism, if set.
    pub complexity_hint: Option<ComplexityHint>,
    /// Retry policy governing re-execution on failure, if set.
    pub retry_policy: Option<RetryPolicy>,
    /// Number of execution attempts made so far.
    pub attempts: u8,
    /// RFC3339 timestamp of when the subtask was created.
    pub created_at: String,
    /// RFC3339 timestamp of the subtask's last update.
    pub updated_at: String,
}

/// Errors returned by [`CheckpointManager`] persistence operations.
#[derive(Debug, thiserror::Error)]
pub enum CheckpointError {
    /// A SQLite operation failed (open, pragma, schema, transaction, query, or write).
    #[error("{context}: {source}")]
    Sqlite {
        /// Human-readable name of the SQLite operation that failed.
        context: &'static str,
        /// The underlying `rusqlite` error.
        #[source]
        source: rusqlite::Error,
    },
    /// The connection mutex was poisoned (a holder thread panicked).
    #[error("Lock failed: {0}")]
    Lock(String),
    /// Snapshot (de)serialization failed.
    #[error("{context}: {source}")]
    Serde {
        /// Human-readable name of the (de)serialization step that failed.
        context: &'static str,
        /// The underlying `serde_json` error.
        #[source]
        source: serde_json::Error,
    },
    /// A requested task or snapshot was not found.
    #[error("{0}")]
    NotFound(String),
}

impl CheckpointManager {
    /// Create a new CheckpointManager, creating the DB file if it doesn't exist.
    pub fn new(db_path: &Path) -> Result<Self, CheckpointError> {
        let conn = Connection::open(db_path).map_err(|source| CheckpointError::Sqlite {
            context: "Failed to open DB",
            source,
        })?;

        // Enable WAL mode for better concurrency
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA busy_timeout=2000;")
            .map_err(|source| CheckpointError::Sqlite {
                context: "Failed to set WAL mode",
                source,
            })?;

        let mgr = Self {
            conn: Mutex::new(conn),
            mutation_count: AtomicU64::new(0),
            last_checkpoint: AtomicI64::new(Utc::now().timestamp()),
            checkpoint_interval_mutations: 5,
            checkpoint_interval_secs: 300,
        };

        mgr.init_schema()?;
        Ok(mgr)
    }

    fn init_schema(&self) -> Result<(), CheckpointError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CheckpointError::Lock(e.to_string()))?;
        conn.execute_batch(
            r#"
            CREATE TABLE IF NOT EXISTS task_decompositions (
                task_id TEXT PRIMARY KEY,
                task_type TEXT NOT NULL,
                description TEXT NOT NULL,
                cila_level INTEGER NOT NULL DEFAULT 3,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                archived_at TEXT,
                status TEXT NOT NULL DEFAULT 'active',
                metrics TEXT
            );

            CREATE TABLE IF NOT EXISTS decomposition_subtasks (
                subtask_id TEXT PRIMARY KEY,
                task_id TEXT NOT NULL,
                description TEXT NOT NULL,
                depends_on TEXT NOT NULL DEFAULT '[]',
                priority INTEGER NOT NULL DEFAULT 255,
                status TEXT NOT NULL,
                deadline TEXT,
                deadline_behavior TEXT DEFAULT 'Fail',
                review_required INTEGER NOT NULL DEFAULT 0,
                complexity_hint TEXT,
                retry_policy TEXT,
                attempts INTEGER NOT NULL DEFAULT 0,
                created_at TEXT NOT NULL,
                updated_at TEXT NOT NULL,
                FOREIGN KEY (task_id) REFERENCES task_decompositions(task_id)
            );

            CREATE TABLE IF NOT EXISTS decomposition_events (
                event_id INTEGER PRIMARY KEY AUTOINCREMENT,
                task_id TEXT NOT NULL,
                subtask_id TEXT,
                event_type TEXT NOT NULL,
                payload TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE TABLE IF NOT EXISTS decomposition_snapshots (
                snapshot_id TEXT PRIMARY KEY,
                task_id TEXT NOT NULL,
                subtasks_snapshot TEXT NOT NULL,
                metrics_snapshot TEXT NOT NULL,
                created_at TEXT NOT NULL DEFAULT (datetime('now'))
            );

            CREATE INDEX IF NOT EXISTS idx_task_status ON task_decompositions(status);
            CREATE INDEX IF NOT EXISTS idx_task_archived ON task_decompositions(archived_at);
            CREATE INDEX IF NOT EXISTS idx_subtasks_task ON decomposition_subtasks(task_id);
            CREATE INDEX IF NOT EXISTS idx_events_task ON decomposition_events(task_id);
            CREATE INDEX IF NOT EXISTS idx_snapshots_task ON decomposition_snapshots(task_id, created_at DESC);
            "#
        ).map_err(|source| CheckpointError::Sqlite { context: "Schema init failed", source })
    }

    /// Record a mutation for checkpoint tracking.
    pub fn record_mutation(&self) {
        self.mutation_count.fetch_add(1, Ordering::SeqCst);
    }

    /// Check if a checkpoint is needed based on mutation count or time interval.
    pub fn needs_checkpoint(&self) -> bool {
        let mutations = self.mutation_count.load(Ordering::SeqCst);
        let last = self.last_checkpoint.load(Ordering::SeqCst);
        let now = Utc::now().timestamp();

        mutations >= self.checkpoint_interval_mutations
            || (now - last) >= self.checkpoint_interval_secs
    }

    /// Save entire TaskDecomposer state to SQLite.
    pub fn checkpoint(&self, decomposer: &TaskDecomposer) -> Result<(), CheckpointError> {
        let mut conn = self
            .conn
            .lock()
            .map_err(|e| CheckpointError::Lock(e.to_string()))?;
        let tx = conn
            .transaction()
            .map_err(|source| CheckpointError::Sqlite {
                context: "Transaction failed",
                source,
            })?;

        // Clear existing active tasks
        tx.execute(
            "DELETE FROM decomposition_subtasks WHERE task_id IN (SELECT task_id FROM task_decompositions WHERE archived_at IS NULL)",
            []
        ).map_err(|source| CheckpointError::Sqlite { context: "Clear subtasks failed", source })?;
        tx.execute(
            "DELETE FROM task_decompositions WHERE archived_at IS NULL",
            [],
        )
        .map_err(|source| CheckpointError::Sqlite {
            context: "Clear tasks failed",
            source,
        })?;

        // Insert current state
        for task in decomposer.tasks.values() {
            let metrics_json = serde_json::to_string(&task.metrics).ok();
            tx.execute(
                "INSERT INTO task_decompositions (task_id, task_type, description, cila_level, created_at, updated_at, archived_at, status, metrics) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)",
                params![
                    task.id,
                    task.task_type,
                    task.description,
                    task.cila_level,
                    task.created_at.to_rfc3339(),
                    task.metrics.updated_at.map(|dt| dt.to_rfc3339()),
                    Option::<String>::None,
                    "active",
                    metrics_json
                ]
            ).map_err(|source| CheckpointError::Sqlite { context: "Insert task failed", source })?;

            for st in &task.subtasks {
                let deadline = st.deadline.map(|d| d.to_rfc3339());
                let complexity_hint = serde_json::to_string(&st.complexity_hint).ok();
                let retry_policy = serde_json::to_string(&st.retry_policy).ok();

                tx.execute(
                    "INSERT INTO decomposition_subtasks (subtask_id, task_id, description, depends_on, priority, status, deadline, deadline_behavior, review_required, complexity_hint, retry_policy, attempts, created_at, updated_at) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)",
                    params![
                        st.id,
                        task.id,
                        st.description,
                        serde_json::to_string(&st.depends_on).unwrap_or_else(|_| "[]".to_string()),
                        st.priority,
                        st.status.to_string(),
                        deadline,
                        serde_json::to_string(&st.deadline_behavior).unwrap_or_else(|_| "{}".to_string()),
                        0, // review_required removed - SubTask has no such field
                        complexity_hint,
                        retry_policy,
                        st.attempts,
                        st.created_at.to_rfc3339(),
                        st.updated_at.to_rfc3339()
                    ]
                ).map_err(|source| CheckpointError::Sqlite { context: "Insert subtask failed", source })?;
            }
        }

        tx.commit().map_err(|source| CheckpointError::Sqlite {
            context: "Commit failed",
            source,
        })?;

        // Reset counters after successful checkpoint
        self.mutation_count.store(0, Ordering::SeqCst);
        self.last_checkpoint
            .store(Utc::now().timestamp(), Ordering::SeqCst);

        Ok(())
    }

    /// Load TaskDecomposer from checkpoint.
    pub fn load(&self) -> Result<TaskDecomposer, CheckpointError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CheckpointError::Lock(e.to_string()))?;
        let mut decomposer = TaskDecomposer::new();

        let mut stmt = conn.prepare(
            "SELECT task_id, task_type, description, cila_level, created_at, updated_at, metrics FROM task_decompositions WHERE archived_at IS NULL"
        ).map_err(|source| CheckpointError::Sqlite { context: "Prepare failed", source })?;

        let task_rows = stmt
            .query_map([], |row| {
                let metrics_str: Option<String> = row.get(6)?;
                Ok(PersistedTask {
                    task_id: row.get(0)?,
                    task_type: row.get(1)?,
                    description: row.get(2)?,
                    cila_level: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                    archived_at: None,
                    status: "active".to_string(),
                    metrics: metrics_str.and_then(|s| serde_json::from_str(&s).ok()),
                })
            })
            .map_err(|source| CheckpointError::Sqlite {
                context: "Query failed",
                source,
            })?;

        for task_result in task_rows {
            let ptask = task_result.map_err(|source| CheckpointError::Sqlite {
                context: "Row failed",
                source,
            })?;

            let mut task = Task::new(
                ptask.task_id.clone(),
                ptask.task_type.clone(),
                ptask.description.clone(),
            );
            task.cila_level = ptask.cila_level;
            if let Some(metrics) = ptask.metrics {
                task.metrics = metrics;
            }

            // Load subtasks for this task
            let mut sub_stmt = conn.prepare(
                "SELECT subtask_id, description, depends_on, priority, status, deadline, deadline_behavior, complexity_hint, retry_policy, attempts, created_at, updated_at FROM decomposition_subtasks WHERE task_id = ?1"
            ).map_err(|source| CheckpointError::Sqlite { context: "Subtask prepare failed", source })?;

            let sub_rows = sub_stmt
                .query_map(params![ptask.task_id], |row| {
                    let depends_on_str: String = row.get(2)?;
                    let depends_on: Vec<String> =
                        serde_json::from_str(&depends_on_str).unwrap_or_default();
                    let deadline_behavior_str: String = row.get(6)?;
                    let complexity_hint_str: Option<String> = row.get(7)?;
                    let retry_policy_str: Option<String> = row.get(8)?;

                    Ok(PersistedSubTask {
                        subtask_id: row.get(0)?,
                        task_id: ptask.task_id.clone(),
                        description: row.get(1)?,
                        depends_on,
                        priority: row.get(3)?,
                        status: row.get(4)?,
                        deadline: row.get(5)?,
                        deadline_behavior: deadline_behavior_str,
                        // NOTE: review_required column removed from persistence (not in SubTask domain model)
                        complexity_hint: complexity_hint_str
                            .and_then(|s| serde_json::from_str(&s).ok()),
                        retry_policy: retry_policy_str.and_then(|s| serde_json::from_str(&s).ok()),
                        attempts: row.get(9)?,
                        created_at: row.get(10)?,
                        updated_at: row.get(11)?,
                    })
                })
                .map_err(|source| CheckpointError::Sqlite {
                    context: "Subtask query failed",
                    source,
                })?;

            for sub_result in sub_rows {
                let pst = sub_result.map_err(|source| CheckpointError::Sqlite {
                    context: "Subtask row failed",
                    source,
                })?;
                let status: SubTaskStatus = pst.status.parse().unwrap_or(SubTaskStatus::Pending);

                let mut subtask = SubTask::new(
                    pst.subtask_id.clone(),
                    pst.description.clone(),
                    pst.depends_on.clone(),
                    pst.priority,
                );
                subtask.status = status;
                subtask.attempts = pst.attempts;
                subtask.deadline_behavior =
                    serde_json::from_str(&pst.deadline_behavior).unwrap_or_default();
                if let Some(deadline_str) = pst.deadline {
                    subtask.deadline = DateTime::parse_from_rfc3339(&deadline_str)
                        .ok()
                        .map(|d| d.with_timezone(&Utc));
                }
                subtask.retry_policy = pst.retry_policy.unwrap_or_default();
                subtask.complexity_hint = pst.complexity_hint;

                task.push_subtask(subtask);
            }

            decomposer.tasks.insert(ptask.task_id.clone(), task);
        }

        Ok(decomposer)
    }

    /// Record a decomposition event for event sourcing.
    pub fn record_event(
        &self,
        task_id: &str,
        subtask_id: Option<&str>,
        event_type: &str,
        payload: &str,
    ) -> Result<(), CheckpointError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CheckpointError::Lock(e.to_string()))?;
        conn.execute(
            "INSERT INTO decomposition_events (task_id, subtask_id, event_type, payload) VALUES (?1, ?2, ?3, ?4)",
            params![task_id, subtask_id, event_type, payload]
        ).map_err(|source| CheckpointError::Sqlite { context: "Record event failed", source })?;
        Ok(())
    }

    /// Create a snapshot of current state.
    pub fn create_snapshot(
        &self,
        task_id: &str,
        decomposer: &TaskDecomposer,
    ) -> Result<String, CheckpointError> {
        let snapshot_id = format!(
            "snap_{}_{}",
            task_id,
            Utc::now().timestamp_nanos_opt().unwrap_or(0)
        );

        let task = decomposer
            .get_plan(task_id)
            .ok_or_else(|| CheckpointError::NotFound(format!("Task not found: {}", task_id)))?;

        let subtasks_snapshot =
            serde_json::to_string(task).map_err(|source| CheckpointError::Serde {
                context: "Serialize task snapshot failed",
                source,
            })?;
        let metrics_snapshot =
            serde_json::to_string(&task.metrics).map_err(|source| CheckpointError::Serde {
                context: "Serialize metrics failed",
                source,
            })?;

        let conn = self
            .conn
            .lock()
            .map_err(|e| CheckpointError::Lock(e.to_string()))?;
        conn.execute(
            "INSERT INTO decomposition_snapshots (snapshot_id, task_id, subtasks_snapshot, metrics_snapshot) VALUES (?1, ?2, ?3, ?4)",
            params![snapshot_id, task_id, subtasks_snapshot, metrics_snapshot]
        ).map_err(|source| CheckpointError::Sqlite { context: "Create snapshot failed", source })?;

        Ok(snapshot_id)
    }

    /// Recover task state from snapshot plus event replay.
    #[allow(clippy::cognitive_complexity)]
    pub fn recover(&self, task_id: &str) -> Result<Task, CheckpointError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CheckpointError::Lock(e.to_string()))?;

        // Load latest snapshot
        let mut snap_stmt = conn.prepare(
            "SELECT snapshot_id, subtasks_snapshot, metrics_snapshot, created_at FROM decomposition_snapshots WHERE task_id = ?1 ORDER BY created_at DESC LIMIT 1"
        ).map_err(|source| CheckpointError::Sqlite { context: "Snapshot prepare failed", source })?;

        let snapshot_result: Option<(String, String, String, String)> = snap_stmt
            .query_row(params![task_id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .ok();

        let (subtasks_snapshot, metrics_snapshot) =
            if let Some((_, subs, metrics, _)) = snapshot_result {
                (subs, metrics)
            } else {
                return Err(CheckpointError::NotFound(format!(
                    "No snapshot found for task: {}",
                    task_id
                )));
            };

        // Load events newer than snapshot
        let mut evt_stmt = conn.prepare(
            "SELECT event_type, payload, created_at FROM decomposition_events WHERE task_id = ?1 ORDER BY created_at ASC"
        ).map_err(|source| CheckpointError::Sqlite { context: "Events prepare failed", source })?;

        let events: Vec<(String, String, String)> = evt_stmt
            .query_map(params![task_id], |row| {
                Ok((row.get(0)?, row.get(1)?, row.get(2)?))
            })
            .map_err(|source| CheckpointError::Sqlite {
                context: "Events query failed",
                source,
            })?
            .filter_map(|r| r.ok())
            .collect();

        // Reconstruct task from snapshot; rebuild O(1) index skipped by serde
        let mut task: Task =
            serde_json::from_str(&subtasks_snapshot).map_err(|source| CheckpointError::Serde {
                context: "Deserialize subtasks failed",
                source,
            })?;
        task.rebuild_index();

        let _ = serde_json::from_str::<DecomposeValidationMetrics>(&metrics_snapshot)
            .map(|m| task.metrics = m);

        // Replay events (simplified - real implementation would apply changes)
        for (event_type, payload, _) in events {
            if event_type == "StatusChanged"
                && let Ok(update) = serde_json::from_str::<StatusChangePayload>(&payload)
                && let Some(st) = task.get_subtask_mut(&update.subtask_id)
            {
                st.status = update.new_status;
            }
        }

        Ok(task)
    }

    /// Archive completed tasks older than age_threshold.
    pub fn archive_completed_tasks(
        &self,
        age_threshold_secs: i64,
    ) -> Result<usize, CheckpointError> {
        let now = Utc::now().timestamp();
        let cutoff = now - age_threshold_secs;
        let cutoff_str = DateTime::from_timestamp(cutoff, 0)
            .map(|dt| dt.to_rfc3339())
            .unwrap_or_default();

        let conn = self
            .conn
            .lock()
            .map_err(|e| CheckpointError::Lock(e.to_string()))?;
        let count = conn.execute(
            "UPDATE task_decompositions SET archived_at = datetime('now') WHERE status = 'completed' AND updated_at < ?1",
            params![cutoff_str]
        ).map_err(|source| CheckpointError::Sqlite { context: "Archive failed", source })?;

        Ok(count)
    }

    /// List archived tasks.
    pub fn list_archived(&self, limit: usize) -> Result<Vec<PersistedTask>, CheckpointError> {
        let conn = self
            .conn
            .lock()
            .map_err(|e| CheckpointError::Lock(e.to_string()))?;
        let mut stmt = conn.prepare(
            "SELECT task_id, task_type, description, cila_level, created_at, updated_at, archived_at, status, metrics FROM task_decompositions WHERE archived_at IS NOT NULL ORDER BY archived_at DESC LIMIT ?1"
        ).map_err(|source| CheckpointError::Sqlite { context: "Prepare failed", source })?;

        let rows = stmt
            .query_map(params![limit as i64], |row| {
                let metrics_str: Option<String> = row.get(8)?;
                Ok(PersistedTask {
                    task_id: row.get(0)?,
                    task_type: row.get(1)?,
                    description: row.get(2)?,
                    cila_level: row.get(3)?,
                    created_at: row.get(4)?,
                    updated_at: row.get(5)?,
                    archived_at: row.get(6)?,
                    status: row.get(7)?,
                    metrics: metrics_str.and_then(|s| serde_json::from_str(&s).ok()),
                })
            })
            .map_err(|source| CheckpointError::Sqlite {
                context: "Query failed",
                source,
            })?;

        let mut tasks = Vec::new();
        for row in rows {
            tasks.push(row.map_err(|source| CheckpointError::Sqlite {
                context: "Row failed",
                source,
            })?);
        }
        Ok(tasks)
    }
}

/// Payload for status change events.
#[derive(Debug, Serialize, Deserialize)]
struct StatusChangePayload {
    subtask_id: String,
    old_status: String,
    new_status: SubTaskStatus,
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::NamedTempFile;

    fn tmp_manager() -> (CheckpointManager, NamedTempFile) {
        let f = NamedTempFile::new().unwrap();
        let mgr = CheckpointManager::new(f.path()).unwrap();
        (mgr, f)
    }

    fn make_task(id: &str) -> Task {
        Task::new(id.to_string(), "intent".to_string(), format!("desc-{}", id))
    }

    fn make_subtask(id: &str) -> SubTask {
        SubTask::new(id.to_string(), format!("sub-{}", id), vec![], 128)
    }

    fn make_decomposer_with_task(task_id: &str) -> TaskDecomposer {
        let mut d = TaskDecomposer::new();
        d.tasks.insert(task_id.to_string(), make_task(task_id));
        d
    }

    // ── Schema ────────────────────────────────────────────────────────────────

    #[test]
    fn test_new_creates_schema() {
        let (mgr, _f) = tmp_manager();
        let conn = mgr.conn.lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT count(*) FROM sqlite_master WHERE type='table' AND name IN ('task_decompositions','decomposition_subtasks','decomposition_events','decomposition_snapshots')",
            [], |r| r.get(0)
        ).unwrap();
        assert_eq!(count, 4, "all 4 tables must exist after init");
    }

    #[test]
    fn test_wal_mode_enabled() {
        let (mgr, _f) = tmp_manager();
        let conn = mgr.conn.lock().unwrap();
        let mode: String = conn
            .query_row("PRAGMA journal_mode", [], |r| r.get(0))
            .unwrap();
        assert_eq!(mode, "wal");
    }

    // ── Mutation counting ─────────────────────────────────────────────────────

    #[test]
    fn test_record_mutation_increments() {
        let (mgr, _f) = tmp_manager();
        for _ in 0..4 {
            mgr.record_mutation();
        }
        assert_eq!(mgr.mutation_count.load(Ordering::SeqCst), 4);
    }

    #[test]
    fn test_needs_checkpoint_false_before_threshold() {
        let (mgr, _f) = tmp_manager();
        for _ in 0..4 {
            mgr.record_mutation();
        }
        // Store a recent last_checkpoint so time branch doesn't fire
        mgr.last_checkpoint
            .store(Utc::now().timestamp(), Ordering::SeqCst);
        assert!(!mgr.needs_checkpoint());
    }

    #[test]
    fn test_needs_checkpoint_true_at_threshold() {
        let (mgr, _f) = tmp_manager();
        mgr.last_checkpoint
            .store(Utc::now().timestamp(), Ordering::SeqCst);
        for _ in 0..5 {
            mgr.record_mutation();
        }
        assert!(mgr.needs_checkpoint());
    }

    #[test]
    fn test_needs_checkpoint_true_by_time() {
        let (mgr, _f) = tmp_manager();
        // Set last checkpoint far in the past
        mgr.last_checkpoint.store(0, Ordering::SeqCst);
        assert!(mgr.needs_checkpoint());
    }

    // ── Checkpoint + Load ─────────────────────────────────────────────────────

    #[test]
    fn test_checkpoint_persists_task() {
        let (mgr, _f) = tmp_manager();
        let decomposer = make_decomposer_with_task("t1");
        mgr.checkpoint(&decomposer).unwrap();

        let loaded = mgr.load().unwrap();
        assert!(loaded.tasks.contains_key("t1"));
        assert_eq!(loaded.tasks["t1"].description, "desc-t1");
    }

    #[test]
    fn test_checkpoint_persists_subtasks() {
        let (mgr, _f) = tmp_manager();
        let mut decomposer = make_decomposer_with_task("t2");
        decomposer
            .tasks
            .get_mut("t2")
            .unwrap()
            .push_subtask(make_subtask("s1"));
        decomposer
            .tasks
            .get_mut("t2")
            .unwrap()
            .push_subtask(make_subtask("s2"));
        mgr.checkpoint(&decomposer).unwrap();

        let loaded = mgr.load().unwrap();
        assert_eq!(loaded.tasks["t2"].subtasks.len(), 2);
        assert_eq!(loaded.tasks["t2"].subtasks[0].description, "sub-s1");
    }

    #[test]
    fn test_checkpoint_resets_mutation_count() {
        let (mgr, _f) = tmp_manager();
        for _ in 0..5 {
            mgr.record_mutation();
        }
        let decomposer = make_decomposer_with_task("t3");
        mgr.checkpoint(&decomposer).unwrap();
        assert_eq!(mgr.mutation_count.load(Ordering::SeqCst), 0);
        mgr.last_checkpoint
            .store(Utc::now().timestamp(), Ordering::SeqCst);
        assert!(!mgr.needs_checkpoint());
    }

    #[test]
    fn test_load_empty_when_no_data() {
        let (mgr, _f) = tmp_manager();
        let loaded = mgr.load().unwrap();
        assert!(loaded.tasks.is_empty());
    }

    #[test]
    fn test_checkpoint_multiple_tasks_roundtrip() {
        let (mgr, _f) = tmp_manager();
        let mut decomposer = TaskDecomposer::new();
        decomposer.tasks.insert("a".to_string(), make_task("a"));
        decomposer.tasks.insert("b".to_string(), make_task("b"));
        decomposer.tasks.insert("c".to_string(), make_task("c"));
        mgr.checkpoint(&decomposer).unwrap();

        let loaded = mgr.load().unwrap();
        assert_eq!(loaded.tasks.len(), 3);
        assert!(loaded.tasks.contains_key("a"));
        assert!(loaded.tasks.contains_key("b"));
        assert!(loaded.tasks.contains_key("c"));
    }

    #[test]
    fn test_checkpoint_idempotent_overwrites_prior() {
        let (mgr, _f) = tmp_manager();
        let d1 = make_decomposer_with_task("orig");
        mgr.checkpoint(&d1).unwrap();

        let d2 = make_decomposer_with_task("replacement");
        mgr.checkpoint(&d2).unwrap();

        let loaded = mgr.load().unwrap();
        assert!(
            !loaded.tasks.contains_key("orig"),
            "old active task should be gone"
        );
        assert!(loaded.tasks.contains_key("replacement"));
    }

    #[test]
    fn test_checkpoint_preserves_cila_level() {
        let (mgr, _f) = tmp_manager();
        let mut decomposer = TaskDecomposer::new();
        let mut t = make_task("cx");
        t.cila_level = 4;
        decomposer.tasks.insert("cx".to_string(), t);
        mgr.checkpoint(&decomposer).unwrap();

        let loaded = mgr.load().unwrap();
        assert_eq!(loaded.tasks["cx"].cila_level, 4);
    }

    #[test]
    fn test_checkpoint_preserves_subtask_status() {
        let (mgr, _f) = tmp_manager();
        let mut decomposer = make_decomposer_with_task("ts");
        let mut st = make_subtask("s1");
        st.status = SubTaskStatus::Completed;
        decomposer.tasks.get_mut("ts").unwrap().push_subtask(st);
        mgr.checkpoint(&decomposer).unwrap();

        let loaded = mgr.load().unwrap();
        let subtask = &loaded.tasks["ts"].subtasks[0];
        assert_eq!(subtask.status, SubTaskStatus::Completed);
    }

    // ── record_event ──────────────────────────────────────────────────────────

    #[test]
    fn test_record_event_no_subtask() {
        let (mgr, _f) = tmp_manager();
        mgr.record_event("t1", None, "TaskCreated", r#"{"info":"ok"}"#)
            .unwrap();

        let conn = mgr.conn.lock().unwrap();
        let count: i64 = conn.query_row(
            "SELECT count(*) FROM decomposition_events WHERE task_id='t1' AND event_type='TaskCreated'",
            [], |r| r.get(0)
        ).unwrap();
        assert_eq!(count, 1);
    }

    #[test]
    fn test_record_event_with_subtask_id() {
        let (mgr, _f) = tmp_manager();
        mgr.record_event("t1", Some("s1"), "StatusChanged", r#"{"s":"pending"}"#)
            .unwrap();

        let conn = mgr.conn.lock().unwrap();
        let subtask_id: String = conn
            .query_row(
                "SELECT subtask_id FROM decomposition_events WHERE task_id='t1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(subtask_id, "s1");
    }

    #[test]
    fn test_record_event_multiple_accumulate() {
        let (mgr, _f) = tmp_manager();
        for i in 0..5 {
            mgr.record_event("t1", None, "Ping", &format!(r#"{{"i":{}}}"#, i))
                .unwrap();
        }
        let conn = mgr.conn.lock().unwrap();
        let count: i64 = conn
            .query_row(
                "SELECT count(*) FROM decomposition_events WHERE task_id='t1'",
                [],
                |r| r.get(0),
            )
            .unwrap();
        assert_eq!(count, 5);
    }

    // ── create_snapshot + recover ─────────────────────────────────────────────

    #[test]
    fn test_create_snapshot_returns_prefixed_id() {
        let (mgr, _f) = tmp_manager();
        let decomposer = make_decomposer_with_task("snap_task");
        let snap_id = mgr.create_snapshot("snap_task", &decomposer).unwrap();
        assert!(
            snap_id.starts_with("snap_snap_task_"),
            "id must be snap_<task_id>_<ts>"
        );
    }

    #[test]
    fn test_create_snapshot_task_not_found_errors() {
        let (mgr, _f) = tmp_manager();
        let decomposer = TaskDecomposer::new();
        let result = mgr.create_snapshot("missing", &decomposer);
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CheckpointError::NotFound(_)));
    }

    #[test]
    fn test_recover_no_snapshot_returns_error() {
        let (mgr, _f) = tmp_manager();
        let result = mgr.recover("nonexistent");
        assert!(result.is_err());
        assert!(matches!(result.unwrap_err(), CheckpointError::NotFound(_)));
    }

    #[test]
    fn test_recover_from_snapshot_roundtrip() {
        let (mgr, _f) = tmp_manager();
        let mut decomposer = make_decomposer_with_task("rtask");
        decomposer
            .tasks
            .get_mut("rtask")
            .unwrap()
            .push_subtask(make_subtask("rs1"));
        mgr.create_snapshot("rtask", &decomposer).unwrap();

        let recovered = mgr.recover("rtask").unwrap();
        assert_eq!(recovered.id, "rtask");
        assert_eq!(recovered.subtasks.len(), 1);
        assert_eq!(recovered.subtasks[0].id, "rs1");
    }

    #[test]
    fn test_recover_replays_status_change_event() {
        let (mgr, _f) = tmp_manager();
        let mut decomposer = make_decomposer_with_task("evtask");
        decomposer
            .tasks
            .get_mut("evtask")
            .unwrap()
            .push_subtask(make_subtask("ev_sub"));
        mgr.create_snapshot("evtask", &decomposer).unwrap();

        // Record a StatusChanged event post-snapshot
        let payload = serde_json::to_string(&StatusChangePayload {
            subtask_id: "ev_sub".to_string(),
            old_status: "pending".to_string(),
            new_status: SubTaskStatus::Completed,
        })
        .unwrap();
        mgr.record_event("evtask", Some("ev_sub"), "StatusChanged", &payload)
            .unwrap();

        let recovered = mgr.recover("evtask").unwrap();
        let sub = recovered.get_subtask("ev_sub").unwrap();
        assert_eq!(
            sub.status,
            SubTaskStatus::Completed,
            "status event must be replayed"
        );
    }

    // ── archive_completed_tasks + list_archived ───────────────────────────────

    #[test]
    fn test_archive_completed_tasks_archives_old_row() {
        let (mgr, _f) = tmp_manager();
        // Directly insert a completed task with old timestamp
        {
            let conn = mgr.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO task_decompositions (task_id, task_type, description, cila_level, created_at, updated_at, status) VALUES ('old_completed','intent','desc',3,'2020-01-01T00:00:00Z','2020-01-01T00:00:00Z','completed')",
                []
            ).unwrap();
        }
        let archived = mgr.archive_completed_tasks(1).unwrap();
        assert_eq!(archived, 1);
    }

    #[test]
    fn test_archive_does_not_archive_active_tasks() {
        let (mgr, _f) = tmp_manager();
        {
            let conn = mgr.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO task_decompositions (task_id, task_type, description, cila_level, created_at, updated_at, status) VALUES ('active_task','intent','desc',3,'2020-01-01T00:00:00Z','2020-01-01T00:00:00Z','active')",
                []
            ).unwrap();
        }
        let archived = mgr.archive_completed_tasks(1).unwrap();
        assert_eq!(archived, 0, "active tasks must not be archived");
    }

    #[test]
    fn test_list_archived_returns_archived_tasks() {
        let (mgr, _f) = tmp_manager();
        {
            let conn = mgr.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO task_decompositions (task_id, task_type, description, cila_level, created_at, updated_at, archived_at, status) VALUES ('arch1','intent','desc',3,'2020-01-01T00:00:00Z','2020-01-01T00:00:00Z','2020-06-01T00:00:00Z','completed')",
                []
            ).unwrap();
        }
        let list = mgr.list_archived(10).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].task_id, "arch1");
        assert!(list[0].archived_at.is_some());
    }

    #[test]
    fn test_list_archived_respects_limit() {
        let (mgr, _f) = tmp_manager();
        {
            let conn = mgr.conn.lock().unwrap();
            for i in 0..5 {
                conn.execute(
                    &format!("INSERT INTO task_decompositions (task_id, task_type, description, cila_level, created_at, updated_at, archived_at, status) VALUES ('a{}','intent','d',3,'2020-01-01T00:00:00Z','2020-01-01T00:00:00Z','2020-06-0{}T00:00:00Z','completed')", i, i + 1),
                    []
                ).unwrap();
            }
        }
        let list = mgr.list_archived(3).unwrap();
        assert_eq!(list.len(), 3, "limit=3 must return at most 3");
    }

    #[test]
    fn test_list_archived_excludes_active() {
        let (mgr, _f) = tmp_manager();
        let decomposer = make_decomposer_with_task("active_one");
        mgr.checkpoint(&decomposer).unwrap();

        let list = mgr.list_archived(100).unwrap();
        assert!(
            list.is_empty(),
            "checkpoint inserts as active, not archived"
        );
    }

    #[test]
    fn test_checkpoint_preserves_archived_rows() {
        let (mgr, _f) = tmp_manager();
        {
            let conn = mgr.conn.lock().unwrap();
            conn.execute(
                "INSERT INTO task_decompositions (task_id, task_type, description, cila_level, created_at, updated_at, archived_at, status) VALUES ('archived_task','intent','d',3,'2020-01-01T00:00:00Z','2020-01-01T00:00:00Z','2020-06-01T00:00:00Z','completed')",
                []
            ).unwrap();
        }
        // Checkpoint only deletes WHERE archived_at IS NULL
        let d = make_decomposer_with_task("new_task");
        mgr.checkpoint(&d).unwrap();

        let list = mgr.list_archived(10).unwrap();
        assert_eq!(list.len(), 1, "archived task must survive checkpoint");
        assert_eq!(list[0].task_id, "archived_task");
    }
}
