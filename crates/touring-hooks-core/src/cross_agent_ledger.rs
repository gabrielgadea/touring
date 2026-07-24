//! Cross-agent outcome ledger — durable event log shared by N concurrent
//! agent processes (ES3 P4, CAH OP4 §5.2.5).
//!
//! ## Schema
//!
//! - `events(id INTEGER PK, actor_id TEXT NOT NULL, event_type TEXT NOT NULL,
//!          payload BLOB NOT NULL, ts INTEGER NOT NULL)`
//! - `actors(actor_id TEXT PK, registered_at INTEGER NOT NULL)`
//! - Index on `(event_type, ts)` for typed time-range queries
//!
//! ## Concurrency
//!
//! - SQLite WAL mode (one writer, many readers, no blocking).
//! - Per-instance `Mutex<Connection>` serializes writes (caller's responsibility
//!   for the global `Arc<CrossAgentLedger>` is handled by `HookRuntime`).
//! - Consumers poll with `since_seq: i64` cursors — no missed events.
//!
//! ## Honest scope (CAH roadmap §3)
//!
//! N>1 concurrent agents on one workspace is **not yet a demonstrated production
//! need**. This module ships the **substrate** (producers + persistence + read
//! API) for capability-readiness; the consumer-side `LedgerConsumer` poll loop
//! is deferred to a followup wave (out of P4 budget). Honest carve-out: failure
//! to open the ledger degrades fail-open to solo mode (no cross-agent contract).
//!
//! ## Entity identity
//!
//! [`ActorId`] is **deterministic** (RFC-004) — derived from
//! `(session_id, agent_role)` via blake3 hash. Same inputs ALWAYS produce the
//! same `ActorId` across sessions, processes, and machines.

use std::path::Path;
use std::sync::Mutex;

use rusqlite::{Connection, params};

/// File name of the cross-agent ledger inside the data directory.
pub const LEDGER_FILENAME: &str = "cross_agent_events.db";

/// Deterministic actor identity for a `(session_id, agent_role)` pair.
///
/// The 8-byte blake3-derived hex form gives 2^64 distinct values, which is
/// astronomically more than enough for any realistic agent fleet.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ActorId(pub String);

impl ActorId {
    /// Derive a deterministic `ActorId` from a session id and an agent role
    /// (e.g. "primary", "worker-1", "reviewer"). Pure function — same inputs
    /// ALWAYS produce the same `ActorId` (RFC-004).
    #[must_use]
    pub fn derive(session_id: &str, agent_role: &str) -> Self {
        let mut hasher = blake3::Hasher::new();
        hasher.update(b"actor:");
        hasher.update(session_id.as_bytes());
        hasher.update(b":");
        hasher.update(agent_role.as_bytes());
        let digest = hasher.finalize();
        let bytes = digest.as_bytes();
        // 8 bytes = 16 hex chars
        let mut hex_buf = [0u8; 16];
        let hex_chars = b"0123456789abcdef";
        for (i, &b) in bytes[..8].iter().enumerate() {
            hex_buf[i * 2] = hex_chars[(b >> 4) as usize];
            hex_buf[i * 2 + 1] = hex_chars[(b & 0x0f) as usize];
        }
        Self(String::from_utf8_lossy(&hex_buf).into_owned())
    }
}

impl std::fmt::Display for ActorId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

/// A single event row from the cross-agent ledger.
#[derive(Debug, Clone)]
pub struct CrossAgentEvent {
    /// Monotonic, dense event id (gapless within a single ledger file).
    pub id: i64,
    /// The actor that produced this event.
    pub actor_id: ActorId,
    /// Free-form event type (e.g. `"tool_outcome"`, `"exec_start"`, `"exec_end"`).
    pub event_type: String,
    /// Opaque, caller-serialized payload (typically `serde_json::to_vec(&T)`).
    pub payload: Vec<u8>,
    /// Wall-clock timestamp in milliseconds since the Unix epoch.
    pub ts_ms: i64,
}

/// The cross-agent ledger — SQLite WAL-backed event log shared by N agent
/// processes.
///
/// Use [`CrossAgentLedger::open`] to open or create the file. All write
/// methods take `&self`; an internal `Mutex<Connection>` serializes
/// per-actor writes (acceptable because per-actor write rate is low:
/// one event per tool outcome).
pub struct CrossAgentLedger {
    conn: Mutex<Connection>,
}

impl CrossAgentLedger {
    /// Open or create the ledger at `{data_dir}/{LEDGER_FILENAME}`. Enables
    /// WAL mode, `busy_timeout=5000ms`, and `foreign_keys`.
    pub fn open(data_dir: &Path) -> Result<Self, rusqlite::Error> {
        std::fs::create_dir_all(data_dir).map_err(|e| {
            rusqlite::Error::ToSqlConversionFailure(Box::new(std::io::Error::new(
                e.kind(),
                format!("cross_agent_ledger: create_dir_all failed: {e}"),
            )))
        })?;
        let db_path = data_dir.join(LEDGER_FILENAME);
        let conn = Connection::open(&db_path)?;

        // WAL mode — one writer, many readers, no blocking.
        conn.pragma_update(None, "journal_mode", "WAL")?;
        // Wait up to 5s for the writer to release the lock under contention.
        conn.busy_timeout(std::time::Duration::from_millis(5_000))?;
        // Enforce FK constraints.
        conn.pragma_update(None, "foreign_keys", "ON")?;

        // Schema (idempotent).
        conn.execute_batch(
            r"
            CREATE TABLE IF NOT EXISTS events (
                id         INTEGER PRIMARY KEY AUTOINCREMENT,
                actor_id   TEXT    NOT NULL,
                event_type TEXT    NOT NULL,
                payload    BLOB    NOT NULL,
                ts         INTEGER NOT NULL
            );
            CREATE INDEX IF NOT EXISTS idx_events_type_ts ON events(event_type, ts);
            CREATE TABLE IF NOT EXISTS actors (
                actor_id      TEXT PRIMARY KEY,
                registered_at INTEGER NOT NULL
            );
            ",
        )?;

        Ok(Self {
            conn: Mutex::new(conn),
        })
    }

    /// Register an actor. Idempotent — INSERT OR IGNORE; no error on duplicate.
    pub fn register_actor(&self, actor: &ActorId) -> Result<(), rusqlite::Error> {
        let conn = self
            .conn
            .lock()
            .expect("cross_agent_ledger: mutex poisoned");
        let now_ms = now_ms();
        conn.execute(
            "INSERT OR IGNORE INTO actors (actor_id, registered_at) VALUES (?1, ?2)",
            params![actor.0, now_ms],
        )?;
        Ok(())
    }

    /// Append an event. Returns the assigned (monotonic) event id.
    pub fn write_event(
        &self,
        actor: &ActorId,
        event_type: &str,
        payload: &[u8],
    ) -> Result<i64, rusqlite::Error> {
        let conn = self
            .conn
            .lock()
            .expect("cross_agent_ledger: mutex poisoned");
        let now_ms = now_ms();
        conn.execute(
            "INSERT INTO events (actor_id, event_type, payload, ts) VALUES (?1, ?2, ?3, ?4)",
            params![actor.0, event_type, payload, now_ms],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Read events with `id > since_seq`, ordered by `id` ASC.
    /// `event_type_filter = None` returns all event types; `Some(t)` filters.
    /// `limit` caps the number of rows returned (callers can re-call to page).
    pub fn read_since(
        &self,
        since_seq: i64,
        event_type_filter: Option<&str>,
        limit: usize,
    ) -> Result<Vec<CrossAgentEvent>, rusqlite::Error> {
        let conn = self
            .conn
            .lock()
            .expect("cross_agent_ledger: mutex poisoned");
        let (sql, type_filter_active) = match event_type_filter {
            Some(_) => (
                "SELECT id, actor_id, event_type, payload, ts \
                 FROM events WHERE id > ?1 AND event_type = ?2 ORDER BY id ASC LIMIT ?3",
                true,
            ),
            None => (
                "SELECT id, actor_id, event_type, payload, ts \
                 FROM events WHERE id > ?1 ORDER BY id ASC LIMIT ?2",
                false,
            ),
        };
        let mut stmt = conn.prepare(sql)?;
        let rows = if type_filter_active {
            stmt.query_map(
                params![since_seq, event_type_filter.unwrap_or(""), limit as i64],
                map_row,
            )?
            .collect::<Result<Vec<_>, _>>()?
        } else {
            stmt.query_map(params![since_seq, limit as i64], map_row)?
                .collect::<Result<Vec<_>, _>>()?
        };
        Ok(rows)
    }

    /// Total event count (observability counter helper).
    pub fn count(&self) -> Result<i64, rusqlite::Error> {
        let conn = self
            .conn
            .lock()
            .expect("cross_agent_ledger: mutex poisoned");
        conn.query_row("SELECT COUNT(*) FROM events", [], |row| {
            row.get::<_, i64>(0)
        })
    }
}

fn map_row(row: &rusqlite::Row<'_>) -> rusqlite::Result<CrossAgentEvent> {
    let actor_id_str: String = row.get(1)?;
    Ok(CrossAgentEvent {
        id: row.get(0)?,
        actor_id: ActorId(actor_id_str),
        event_type: row.get(2)?,
        payload: row.get(3)?,
        ts_ms: row.get(4)?,
    })
}

fn now_ms() -> i64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

use serde::{Deserialize, Serialize};

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn fresh_ledger() -> (TempDir, CrossAgentLedger) {
        let tmp = TempDir::new().expect("tempdir");
        let ledger = CrossAgentLedger::open(tmp.path()).expect("open");
        (tmp, ledger)
    }

    #[test]
    fn actor_id_derive_is_deterministic() {
        let a = ActorId::derive("session-abc", "primary");
        let b = ActorId::derive("session-abc", "primary");
        assert_eq!(a, b, "same inputs must produce same ActorId");
        assert!(!a.0.is_empty());
        assert_eq!(a.0.len(), 16, "ActorId hex is 16 chars (8 bytes)");
    }

    #[test]
    fn actor_id_derive_distinguishes_roles() {
        let primary = ActorId::derive("session-x", "primary");
        let worker = ActorId::derive("session-x", "worker-1");
        let reviewer = ActorId::derive("session-x", "reviewer");
        assert_ne!(primary, worker);
        assert_ne!(primary, reviewer);
        assert_ne!(worker, reviewer);
    }

    #[test]
    fn actor_id_derive_distinguishes_sessions() {
        let s1 = ActorId::derive("session-1", "primary");
        let s2 = ActorId::derive("session-2", "primary");
        assert_ne!(s1, s2);
    }

    #[test]
    fn register_actor_is_idempotent() {
        let (_tmp, ledger) = fresh_ledger();
        let a = ActorId::derive("s1", "primary");
        ledger.register_actor(&a).expect("first register");
        ledger
            .register_actor(&a)
            .expect("second register (idempotent)");
        ledger
            .register_actor(&a)
            .expect("third register (idempotent)");
    }

    #[test]
    fn write_and_read_roundtrips() {
        let (_tmp, ledger) = fresh_ledger();
        let a = ActorId::derive("s1", "primary");
        ledger.register_actor(&a).unwrap();

        let id1 = ledger
            .write_event(&a, "tool_outcome", b"payload-1")
            .unwrap();
        let id2 = ledger
            .write_event(&a, "tool_outcome", b"payload-2")
            .unwrap();
        let id3 = ledger.write_event(&a, "exec_end", b"payload-3").unwrap();

        assert!(id1 < id2 && id2 < id3, "ids must be monotonic");

        let events = ledger.read_since(0, None, 100).unwrap();
        assert_eq!(events.len(), 3);
        assert_eq!(events[0].id, id1);
        assert_eq!(events[0].actor_id, a);
        assert_eq!(events[0].event_type, "tool_outcome");
        assert_eq!(events[0].payload, b"payload-1");
        assert_eq!(events[2].event_type, "exec_end");
    }

    #[test]
    fn read_since_filters_by_event_type() {
        let (_tmp, ledger) = fresh_ledger();
        let a = ActorId::derive("s1", "primary");
        ledger.register_actor(&a).unwrap();
        ledger.write_event(&a, "tool_outcome", b"a").unwrap();
        ledger.write_event(&a, "exec_start", b"b").unwrap();
        ledger.write_event(&a, "tool_outcome", b"c").unwrap();
        ledger.write_event(&a, "exec_end", b"d").unwrap();
        ledger.write_event(&a, "tool_outcome", b"e").unwrap();

        let tool_events = ledger.read_since(0, Some("tool_outcome"), 100).unwrap();
        assert_eq!(tool_events.len(), 3);
        assert!(tool_events.iter().all(|e| e.event_type == "tool_outcome"));

        let exec_events = ledger.read_since(0, Some("exec_end"), 100).unwrap();
        assert_eq!(exec_events.len(), 1);
    }

    #[test]
    fn read_since_respects_since_cursor() {
        let (_tmp, ledger) = fresh_ledger();
        let a = ActorId::derive("s1", "primary");
        ledger.register_actor(&a).unwrap();
        ledger.write_event(&a, "x", b"1").unwrap();
        let mid = ledger.write_event(&a, "x", b"2").unwrap();
        ledger.write_event(&a, "x", b"3").unwrap();

        let after = ledger.read_since(mid, None, 100).unwrap();
        assert_eq!(after.len(), 1);
        assert_eq!(after[0].payload, b"3");
    }

    #[test]
    fn count_returns_total_events() {
        let (_tmp, ledger) = fresh_ledger();
        let a = ActorId::derive("s1", "primary");
        ledger.register_actor(&a).unwrap();
        assert_eq!(ledger.count().unwrap(), 0);
        ledger.write_event(&a, "x", b"1").unwrap();
        ledger.write_event(&a, "x", b"2").unwrap();
        assert_eq!(ledger.count().unwrap(), 2);
    }
}
