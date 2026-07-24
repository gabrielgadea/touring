//! THSF Phase 5 Wave I — grow-only SQLite audit trail for `HealthDeltaEvent`.
//!
//! # Design
//!
//! The broadcast channel in `touring-core::health_events` delivers
//! events to live subscribers (`GeneratorHealthImpl` fan-out) but has
//! **no persistence** — once all receivers have consumed an event, it
//! is gone. Wave I adds a parallel SQLite writer: every time
//! `compute_signals_delta` emits a broadcast event it also performs an
//! `INSERT` into `health_delta_events`. The table is strictly
//! append-only (grow-only semantics) so the audit trail can be queried
//! cross-session without concern for prior state being modified.
//!
//! # Invariants
//!
//! 1. **INSERT-only**. No `UPDATE` or `DELETE` statements. Retention
//!    (if ever needed) would be a scheduled VACUUM job — out of scope
//!    for MVP.
//! 2. **Best-effort persistence**. Insert failures are logged via
//!    `tracing::warn!` and swallowed. A broken disk MUST NOT break the
//!    hot path (compute_signals_delta) or the broadcast channel.
//! 3. **Lazy init**. The singleton `Mutex<Option<Connection>>` is
//!    created on first call. When init itself fails (read-only mount,
//!    permissions), the slot stays `None` forever — queries return
//!    empty, inserts no-op.
//! 4. **Thread safety**. `rusqlite::Connection` is `!Sync`. A
//!    process-wide `Mutex` serializes writes + reads. INSERT budget is
//!    O(1/s) so contention is negligible.
//!
//! # Storage path resolution
//!
//! Priority order (first writable wins):
//! 1. `TOURING_HEALTH_AUDIT_DB` env override
//! 2. `$XDG_STATE_HOME/touring/health_delta_events.db`
//! 3. `$HOME/.local/state/touring/health_delta_events.db`
//! 4. `/tmp/touring-health-events.db`

use std::path::PathBuf;
use std::sync::{Mutex, OnceLock};

use rusqlite::{Connection, params};

use touring_foundation::{DeltaOutcome, HealthDeltaEvent};

/// Resolved singleton connection. `None` when init failed — callers
/// silently no-op in that case (best-effort persistence).
type SharedConn = OnceLock<Mutex<Option<Connection>>>;

static CONN: SharedConn = OnceLock::new();

/// SQL DDL applied by `ensure_schema`. Idempotent — every statement uses
/// `IF NOT EXISTS`.
const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS health_delta_events (
    event_id           INTEGER PRIMARY KEY AUTOINCREMENT,
    file_path          TEXT    NOT NULL,
    old_health         REAL,
    new_health         REAL    NOT NULL,
    delta              REAL,
    outcome            TEXT    NOT NULL CHECK(outcome IN ('improvement','regression','neutral')),
    regression_streak  INTEGER NOT NULL DEFAULT 0,
    improvement_streak INTEGER NOT NULL DEFAULT 0,
    timestamp_ms       INTEGER NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_health_events_ts
    ON health_delta_events(timestamp_ms);
CREATE INDEX IF NOT EXISTS idx_health_events_path_ts
    ON health_delta_events(file_path, timestamp_ms);
"#;

/// Resolve the SQLite file path. See module docs for priority order.
///
/// This function only picks a candidate; it does NOT attempt to create
/// the parent directory. `init` handles that.
pub fn resolve_db_path() -> PathBuf {
    if let Ok(p) = std::env::var("TOURING_HEALTH_AUDIT_DB") {
        return PathBuf::from(p);
    }
    if let Ok(xdg) = std::env::var("XDG_STATE_HOME") {
        return PathBuf::from(xdg)
            .join("touring")
            .join("health_delta_events.db");
    }
    if let Ok(home) = std::env::var("HOME") {
        return PathBuf::from(home)
            .join(".local/state/touring")
            .join("health_delta_events.db");
    }
    PathBuf::from("/tmp/touring-health-events.db")
}

fn outcome_label(o: DeltaOutcome) -> &'static str {
    match o {
        DeltaOutcome::Improvement => "improvement",
        DeltaOutcome::Regression => "regression",
        DeltaOutcome::Neutral => "neutral",
    }
}

fn parse_outcome(s: &str) -> DeltaOutcome {
    match s {
        "improvement" => DeltaOutcome::Improvement,
        "regression" => DeltaOutcome::Regression,
        _ => DeltaOutcome::Neutral,
    }
}

/// Open the connection, ensure parent dir exists, apply schema.
/// On any failure, returns `None` so the caller sees a no-op singleton.
fn try_open(db_path: &std::path::Path) -> Option<Connection> {
    if let Some(parent) = db_path.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            tracing::warn!(
                error = %e,
                path = %parent.display(),
                "health-delta-audit: create_dir_all failed — audit trail disabled"
            );
            return None;
        }
    }
    let conn = match Connection::open(db_path) {
        Ok(c) => c,
        Err(e) => {
            tracing::warn!(
                error = %e,
                path = %db_path.display(),
                "health-delta-audit: Connection::open failed — audit trail disabled"
            );
            return None;
        }
    };
    if let Err(e) = conn.execute_batch(SCHEMA_SQL) {
        tracing::warn!(
            error = %e,
            "health-delta-audit: schema application failed — audit trail disabled"
        );
        return None;
    }
    Some(conn)
}

fn singleton() -> &'static Mutex<Option<Connection>> {
    CONN.get_or_init(|| {
        let path = resolve_db_path();
        Mutex::new(try_open(&path))
    })
}

/// Persist a single event. Returns `true` when the row was inserted,
/// `false` when the audit trail is disabled OR the insert failed.
///
/// This function never panics and never propagates errors — callers
/// (hot path) should invoke and discard the return value unless they
/// need the liveness signal for tests.
pub fn record_event(event: &HealthDeltaEvent) -> bool {
    let slot = singleton();
    let mut guard = match slot.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    let conn = match guard.as_mut() {
        Some(c) => c,
        None => return false,
    };
    let result = conn.execute(
        "INSERT INTO health_delta_events
            (file_path, old_health, new_health, delta, outcome,
             regression_streak, improvement_streak, timestamp_ms)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![
            event.file_path,
            event.old_health as f64,
            event.new_health as f64,
            event.delta as f64,
            outcome_label(event.outcome),
            event.regression_streak,
            event.improvement_streak,
            event.timestamp_ms as i64,
        ],
    );
    match result {
        Ok(_) => true,
        Err(e) => {
            tracing::warn!(error = %e, "health-delta-audit: INSERT failed");
            false
        }
    }
}

/// Query parameters for `query_history`. Each filter is AND-ed.
#[derive(Debug, Clone, Default)]
pub struct HistoryQuery {
    /// Only events at or after this UNIX-epoch millisecond timestamp.
    pub since_ms: Option<u64>,
    /// Only events whose `file_path` starts with this prefix.
    pub path_prefix: Option<String>,
    /// Max number of rows to return. Defaults to 100 when `None`.
    pub limit: Option<u32>,
}

/// Execute a read query against the audit trail.
///
/// Returns `Ok(events)` on success (possibly empty when audit trail is
/// disabled or no rows match) or `Err(String)` when a query-level error
/// occurred. Open failures never surface here — they result in an empty
/// `Ok(Vec::new())`.
pub fn query_history(q: &HistoryQuery) -> Result<Vec<HealthDeltaEvent>, HistoryQueryError> {
    let slot = singleton();
    let guard = match slot.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    let conn = match guard.as_ref() {
        Some(c) => c,
        None => return Ok(Vec::new()),
    };

    let mut sql = String::from(
        "SELECT file_path, old_health, new_health, delta, outcome,
                regression_streak, improvement_streak, timestamp_ms
         FROM health_delta_events
         WHERE 1=1",
    );
    let mut args: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();
    if let Some(since) = q.since_ms {
        sql.push_str(" AND timestamp_ms >= ?");
        args.push(Box::new(since as i64));
    }
    if let Some(pref) = &q.path_prefix {
        sql.push_str(" AND file_path LIKE ?");
        args.push(Box::new(format!("{pref}%")));
    }
    sql.push_str(" ORDER BY timestamp_ms DESC, event_id DESC");
    let limit = q.limit.unwrap_or(100).max(1);
    sql.push_str(" LIMIT ?");
    args.push(Box::new(limit));

    let mut stmt = conn
        .prepare(&sql)
        .map_err(|e| format!("prepare failed: {e}"))?;
    let params_refs: Vec<&dyn rusqlite::ToSql> = args.iter().map(|b| b.as_ref()).collect();

    let rows = stmt
        .query_map(params_refs.as_slice(), |row| {
            let outcome_str: String = row.get::<_, String>(4)?;
            Ok(HealthDeltaEvent {
                file_path: row.get::<_, String>(0)?,
                old_health: row.get::<_, f64>(1)? as f32,
                new_health: row.get::<_, f64>(2)? as f32,
                delta: row.get::<_, f64>(3)? as f32,
                outcome: parse_outcome(&outcome_str),
                regression_streak: row.get::<_, u32>(5)?,
                improvement_streak: row.get::<_, u32>(6)?,
                timestamp_ms: row.get::<_, i64>(7)? as u64,
            })
        })
        .map_err(|e| format!("query_map failed: {e}"))?;

    let mut out = Vec::new();
    for r in rows {
        match r {
            Ok(e) => out.push(e),
            Err(e) => return Err(format!("row decode: {e}").into()),
        }
    }
    Ok(out)
}

/// Error from [`query_history`] (F-8 / RBP-03: typed in place of `String`).
#[derive(Debug, thiserror::Error)]
#[error("{0}")]
pub struct HistoryQueryError(pub String);

impl From<String> for HistoryQueryError {
    fn from(message: String) -> Self {
        Self(message)
    }
}

/// Count total rows in the audit table. Diagnostic — returns `0` when
/// audit trail is disabled.
pub fn count_events() -> u64 {
    let slot = singleton();
    let guard = match slot.lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    let conn = match guard.as_ref() {
        Some(c) => c,
        None => return 0,
    };
    conn.query_row("SELECT COUNT(*) FROM health_delta_events", [], |r| {
        r.get::<_, i64>(0).map(|v| v as u64)
    })
    .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex as StdMutex;

    // The audit singleton is process-wide. To keep tests hermetic, we:
    //  1. Redirect TOURING_HEALTH_AUDIT_DB to a tempfile BEFORE first access
    //  2. Serialize tests through a local mutex
    //  3. Accept that we can't rebuild the singleton mid-process — tests
    //     validate the MECHANICS via a side-channel (direct rusqlite
    //     connection) while the singleton remains primed from whichever
    //     test ran first.

    static SERIAL: StdMutex<()> = StdMutex::new(());

    fn fresh_tempdb_conn() -> (tempfile::TempDir, Connection) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("test_audit.db");
        let conn = Connection::open(&db).expect("open");
        conn.execute_batch(SCHEMA_SQL).expect("schema");
        (dir, conn)
    }

    #[test]
    fn resolve_db_path_honors_env_override() {
        let _g = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
        let prev = std::env::var_os("TOURING_HEALTH_AUDIT_DB");
        unsafe {
            std::env::set_var("TOURING_HEALTH_AUDIT_DB", "/tmp/custom.db");
        }
        assert_eq!(resolve_db_path(), PathBuf::from("/tmp/custom.db"));
        unsafe {
            match prev {
                Some(v) => std::env::set_var("TOURING_HEALTH_AUDIT_DB", v),
                None => std::env::remove_var("TOURING_HEALTH_AUDIT_DB"),
            }
        }
    }

    #[test]
    fn schema_is_idempotent() {
        let (_dir, conn) = fresh_tempdb_conn();
        conn.execute_batch(SCHEMA_SQL).expect("re-apply schema");
        conn.execute_batch(SCHEMA_SQL)
            .expect("re-apply schema again");
    }

    #[test]
    fn insert_and_read_back_single_event() {
        let (_dir, conn) = fresh_tempdb_conn();
        conn.execute(
            "INSERT INTO health_delta_events
                (file_path, old_health, new_health, delta, outcome,
                 regression_streak, improvement_streak, timestamp_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params!["/test.rs", 0.9, 0.7, -0.2, "regression", 1, 0, 1000_i64],
        )
        .expect("insert");

        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM health_delta_events", [], |r| r.get(0))
            .expect("count");
        assert_eq!(count, 1);

        let outcome: String = conn
            .query_row(
                "SELECT outcome FROM health_delta_events WHERE file_path = ?1",
                ["/test.rs"],
                |r| r.get(0),
            )
            .expect("outcome");
        assert_eq!(outcome, "regression");
    }

    #[test]
    fn outcome_check_constraint_rejects_unknown_value() {
        let (_dir, conn) = fresh_tempdb_conn();
        let err = conn.execute(
            "INSERT INTO health_delta_events
                (file_path, new_health, outcome, timestamp_ms)
             VALUES (?1, ?2, ?3, ?4)",
            params!["/x.rs", 0.5, "bogus", 0_i64],
        );
        assert!(err.is_err(), "CHECK constraint should reject 'bogus'");
    }

    #[test]
    fn outcome_round_trip() {
        assert_eq!(outcome_label(DeltaOutcome::Improvement), "improvement");
        assert_eq!(outcome_label(DeltaOutcome::Regression), "regression");
        assert_eq!(outcome_label(DeltaOutcome::Neutral), "neutral");
        assert_eq!(parse_outcome("improvement"), DeltaOutcome::Improvement);
        assert_eq!(parse_outcome("regression"), DeltaOutcome::Regression);
        assert_eq!(parse_outcome("neutral"), DeltaOutcome::Neutral);
        assert_eq!(parse_outcome("anything-else"), DeltaOutcome::Neutral);
    }

    #[test]
    fn history_query_filters_since_path_limit() {
        let (_dir, conn) = fresh_tempdb_conn();
        for (i, ts) in [100_i64, 200, 300, 400, 500].iter().enumerate() {
            let path = if i % 2 == 0 {
                "/src/a.rs"
            } else {
                "/docs/z.md"
            };
            conn.execute(
                "INSERT INTO health_delta_events
                    (file_path, new_health, outcome, timestamp_ms)
                 VALUES (?1, ?2, ?3, ?4)",
                params![path, 0.5, "neutral", ts],
            )
            .expect("insert");
        }

        // since=250 → 3 rows; path=/src/ → 2 of those; limit=1 → 1 row
        let rows: Vec<String> = conn
            .prepare(
                "SELECT file_path FROM health_delta_events
                 WHERE timestamp_ms >= ?1 AND file_path LIKE ?2
                 ORDER BY timestamp_ms DESC LIMIT ?3",
            )
            .expect("prepare")
            .query_map(params![250_i64, "/src/%", 1_i64], |r| r.get::<_, String>(0))
            .expect("qm")
            .collect::<Result<Vec<_>, _>>()
            .expect("collect");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0], "/src/a.rs");
    }

    #[test]
    fn grow_only_invariant_inserts_accumulate() {
        let (_dir, conn) = fresh_tempdb_conn();
        for i in 0..10 {
            conn.execute(
                "INSERT INTO health_delta_events
                    (file_path, new_health, outcome, timestamp_ms)
                 VALUES (?1, ?2, ?3, ?4)",
                params![format!("/f{i}.rs"), 0.5, "neutral", i as i64],
            )
            .expect("insert");
        }
        let count: i64 = conn
            .query_row("SELECT COUNT(*) FROM health_delta_events", [], |r| r.get(0))
            .expect("count");
        assert_eq!(count, 10);
    }
}
