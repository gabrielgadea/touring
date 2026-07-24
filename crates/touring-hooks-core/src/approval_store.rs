//! **S-15 / R14 — durable cross-session HITL approvals.**
//!
//! The `permission_request` hook decides allow/deny/ask in real time, but the
//! decision is ephemeral — a daemon restart forgets every pending approval, so a
//! human who approved a risky action last session is asked again. R14 makes the
//! approval state durable: a `pending_approvals` table in the knowledge DB keyed
//! by [`ActionSignature::to_key`](crate::action_signature::ActionSignature::to_key),
//! so an approval *survives a restart* and the next session resumes it.
//!
//! [`ApprovalStore`] is a thin, fail-explicit wrapper over a `rusqlite`
//! connection — it owns no schema beyond its one table, so it composes with the
//! existing knowledge DB (`ensure_table` is idempotent `CREATE TABLE IF NOT
//! EXISTS`). Keying by the signature (not a raw command) means semantically
//! equivalent actions share one approval, exactly as the conformal-threshold
//! routing (S-08) expects.

use rusqlite::{Connection, OptionalExtension, params};

/// The durable state of a human-in-the-loop approval.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApprovalStatus {
    /// Awaiting a human decision — carried across sessions until resolved.
    Pending,
    /// A human approved the action class.
    Approved,
    /// A human denied the action class.
    Denied,
}

impl ApprovalStatus {
    /// The stable string stored in the DB.
    #[must_use]
    pub fn as_str(&self) -> &'static str {
        match self {
            ApprovalStatus::Pending => "pending",
            ApprovalStatus::Approved => "approved",
            ApprovalStatus::Denied => "denied",
        }
    }

    /// Parse from the stored string; unknown values map to `None`.
    #[must_use]
    pub fn from_db(s: &str) -> Option<Self> {
        match s {
            "pending" => Some(ApprovalStatus::Pending),
            "approved" => Some(ApprovalStatus::Approved),
            "denied" => Some(ApprovalStatus::Denied),
            _ => None,
        }
    }
}

/// One persisted approval row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalRecord {
    /// `ActionSignature::to_key()` — the durable key.
    pub action_sig_key: String,
    /// The approval state.
    pub status: ApprovalStatus,
    /// Free-text reason / context for the decision.
    pub reason: String,
    /// Last-updated unix timestamp (caller-supplied; deterministic in tests).
    pub updated_ts: i64,
}

/// A durable store of HITL approvals over a `rusqlite` connection.
pub struct ApprovalStore<'a> {
    conn: &'a Connection,
}

impl<'a> ApprovalStore<'a> {
    /// Wrap a connection (the knowledge DB, or `:memory:` / a temp file in tests).
    #[must_use]
    pub fn new(conn: &'a Connection) -> Self {
        Self { conn }
    }

    /// Idempotently create the `pending_approvals` table. Safe to call every
    /// startup — `CREATE TABLE IF NOT EXISTS`.
    pub fn ensure_table(&self) -> rusqlite::Result<()> {
        self.conn.execute_batch(
            "CREATE TABLE IF NOT EXISTS pending_approvals (
                 action_sig_key TEXT PRIMARY KEY,
                 status         TEXT NOT NULL,
                 reason         TEXT NOT NULL DEFAULT '',
                 updated_ts     INTEGER NOT NULL DEFAULT 0
             );",
        )
    }

    /// Insert or update an approval keyed by the action signature.
    pub fn upsert(
        &self,
        action_sig_key: &str,
        status: ApprovalStatus,
        reason: &str,
        updated_ts: i64,
    ) -> rusqlite::Result<()> {
        self.conn.execute(
            "INSERT INTO pending_approvals (action_sig_key, status, reason, updated_ts)
             VALUES (?1, ?2, ?3, ?4)
             ON CONFLICT(action_sig_key) DO UPDATE SET
                 status = excluded.status,
                 reason = excluded.reason,
                 updated_ts = excluded.updated_ts;",
            params![action_sig_key, status.as_str(), reason, updated_ts],
        )?;
        Ok(())
    }

    /// Fetch the approval for a signature, if any.
    pub fn get(&self, action_sig_key: &str) -> rusqlite::Result<Option<ApprovalRecord>> {
        self.conn
            .query_row(
                "SELECT action_sig_key, status, reason, updated_ts
                   FROM pending_approvals WHERE action_sig_key = ?1;",
                params![action_sig_key],
                |row| {
                    let status_str: String = row.get(1)?;
                    Ok(ApprovalRecord {
                        action_sig_key: row.get(0)?,
                        status: ApprovalStatus::from_db(&status_str)
                            .unwrap_or(ApprovalStatus::Pending),
                        reason: row.get(2)?,
                        updated_ts: row.get(3)?,
                    })
                },
            )
            .optional()
    }

    /// All rows still `pending` — what the next session must resume.
    pub fn list_pending(&self) -> rusqlite::Result<Vec<ApprovalRecord>> {
        let mut stmt = self.conn.prepare(
            "SELECT action_sig_key, status, reason, updated_ts
               FROM pending_approvals WHERE status = 'pending'
               ORDER BY updated_ts ASC;",
        )?;
        let rows = stmt.query_map([], |row| {
            let status_str: String = row.get(1)?;
            Ok(ApprovalRecord {
                action_sig_key: row.get(0)?,
                status: ApprovalStatus::from_db(&status_str).unwrap_or(ApprovalStatus::Pending),
                reason: row.get(2)?,
                updated_ts: row.get(3)?,
            })
        })?;
        rows.collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upsert_then_get_roundtrips_in_memory() {
        let conn = Connection::open_in_memory().unwrap();
        let store = ApprovalStore::new(&conn);
        store.ensure_table().unwrap();
        store
            .upsert(
                "outcome:bash:cargo:plain",
                ApprovalStatus::Approved,
                "human ok",
                1000,
            )
            .unwrap();
        let got = store.get("outcome:bash:cargo:plain").unwrap().unwrap();
        assert_eq!(got.status, ApprovalStatus::Approved);
        assert_eq!(got.reason, "human ok");
        assert_eq!(got.updated_ts, 1000);
        assert!(store.get("unknown:key").unwrap().is_none());
    }

    #[test]
    fn upsert_updates_existing_status() {
        let conn = Connection::open_in_memory().unwrap();
        let store = ApprovalStore::new(&conn);
        store.ensure_table().unwrap();
        store
            .upsert("k", ApprovalStatus::Pending, "asked", 1)
            .unwrap();
        store
            .upsert("k", ApprovalStatus::Denied, "human nack", 2)
            .unwrap();
        let got = store.get("k").unwrap().unwrap();
        assert_eq!(got.status, ApprovalStatus::Denied);
        assert_eq!(got.updated_ts, 2);
    }

    #[test]
    fn list_pending_returns_only_unresolved() {
        let conn = Connection::open_in_memory().unwrap();
        let store = ApprovalStore::new(&conn);
        store.ensure_table().unwrap();
        store.upsert("a", ApprovalStatus::Pending, "", 1).unwrap();
        store.upsert("b", ApprovalStatus::Approved, "", 2).unwrap();
        store.upsert("c", ApprovalStatus::Pending, "", 3).unwrap();
        let pending = store.list_pending().unwrap();
        assert_eq!(pending.len(), 2);
        assert_eq!(pending[0].action_sig_key, "a");
        assert_eq!(pending[1].action_sig_key, "c");
    }

    #[test]
    fn approval_survives_restart_on_disk() {
        // Durability proof: write through one connection, drop it (≈ daemon stop),
        // reopen the SAME file (≈ next session), and confirm the approval persists.
        let dir = tempfile::tempdir().unwrap();
        let db_path = dir.path().join("approvals.db");

        {
            let conn = Connection::open(&db_path).unwrap();
            let store = ApprovalStore::new(&conn);
            store.ensure_table().unwrap();
            store
                .upsert(
                    "outcome:bash:cargo:plain",
                    ApprovalStatus::Approved,
                    "session-1 human ok",
                    42,
                )
                .unwrap();
        } // conn dropped — simulates daemon shutdown.

        let conn2 = Connection::open(&db_path).unwrap();
        let store2 = ApprovalStore::new(&conn2);
        let resumed = store2.get("outcome:bash:cargo:plain").unwrap().unwrap();
        assert_eq!(resumed.status, ApprovalStatus::Approved);
        assert_eq!(resumed.reason, "session-1 human ok");
        assert_eq!(resumed.updated_ts, 42, "approval must survive the restart");
    }
}
