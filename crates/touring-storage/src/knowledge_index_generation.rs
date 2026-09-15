//! The seal on a rebuild — `index_generation` rows on `knowledge.db`.
//!
//! A full rebuild is a sequence of per-file transactions, not one transaction: a
//! daemon killed in the middle leaves SQLite intact and the index a MIX of two
//! walks (measured 13/09/2026 on a per-project copy: `kill -9` three seconds into
//! a rebuild left `wiring_map` 85 rows short, integrity `ok`, and the next query
//! answered as if nothing had happened — I16 of the Graft analysis). The seal
//! makes that state explicit: a rebuild opens a generation (`building`), closes
//! it (`complete`), and any reader — `index status`, `index find`, the searches,
//! `doctor` — can see a generation left `building` by a process that is no longer
//! alive, or one marked `aborted`, and say so instead of answering from a partial
//! index. The table is created lazily, so no schema-version bump is needed and a
//! database that predates the seal simply reports `none`.

use rusqlite::{Connection, OptionalExtension, params};

use crate::knowledge::FileKnowledgeDB;

/// The table, created on first use (idempotent).
const INDEX_GENERATION_DDL: &str = "CREATE TABLE IF NOT EXISTS index_generation (
    id INTEGER PRIMARY KEY AUTOINCREMENT,
    state TEXT NOT NULL,
    owner_pid INTEGER NOT NULL,
    started_at TEXT NOT NULL DEFAULT (datetime('now')),
    finished_at TEXT,
    files INTEGER,
    symbols INTEGER,
    note TEXT
)";

/// Generations kept per database; older rows are pruned when a new one opens.
const KEEP_GENERATIONS: i64 = 20;

/// The one command that repairs every non-`complete` state — reached through
/// [`IndexGenerationState::remedy`], so every reader spells it once.
const INDEX_GENERATION_REMEDY: &str = "touring index rebuild";

/// What a reader learns from the latest generation row.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct IndexGenerationState {
    /// `complete` · `building` · `partial` · `none`.
    pub state: &'static str,
    /// Row id of the latest generation, `None` when the table has no row.
    pub generation: Option<i64>,
    /// The daemon that opened the latest generation.
    pub owner_pid: Option<u32>,
    /// When the latest generation opened.
    pub started_at: Option<String>,
    /// When it closed (complete or aborted).
    pub finished_at: Option<String>,
    /// Files walked by the latest complete/aborted generation.
    pub files: Option<i64>,
    /// Symbols written by it.
    pub symbols: Option<i64>,
    /// One sentence a human can act on.
    pub detail: String,
}

impl IndexGenerationState {
    /// `true` when the index may hold a mix of two walks — the state every query
    /// handler flags and `doctor` fails on.
    #[must_use]
    pub fn is_partial(&self) -> bool {
        self.state == "partial"
    }

    /// The command that repairs a `partial` index — carried by every reader that
    /// flags the state, so the remedy is written once.
    #[must_use]
    pub fn remedy(&self) -> &'static str {
        INDEX_GENERATION_REMEDY
    }
}

/// Whether `pid` is a live `touring-daemon` on this machine.
///
/// Read from `/proc/<pid>/comm`; where `/proc` is unavailable the owner is
/// assumed alive, so a `building` row is never branded `partial` without
/// evidence (the doctor prefers a missed partial to a false alarm on a platform
/// it cannot inspect).
#[must_use]
fn owner_alive(pid: u32) -> bool {
    let proc_root = std::path::Path::new("/proc");
    if !proc_root.exists() {
        return true;
    }
    match std::fs::read_to_string(proc_root.join(pid.to_string()).join("comm")) {
        Ok(comm) => comm.trim() == "touring-daemon",
        Err(_) => false,
    }
}

fn table_exists(conn: &Connection) -> Result<bool, rusqlite::Error> {
    conn.query_row(
        "SELECT 1 FROM sqlite_master WHERE type = 'table' AND name = 'index_generation'",
        [],
        |_| Ok(()),
    )
    .optional()
    .map(|r| r.is_some())
}

/// The latest generation, judged: `self_pid` is the reading daemon (a `building`
/// row it owns is its own rebuild in progress), `None` for an external reader such
/// as `touring doctor`, which opens the database read-only.
///
/// # Errors
///
/// Propagates any `rusqlite` failure; a database without the table is not an
/// error, it reports `none`.
pub fn read_index_generation_state(
    conn: &Connection,
    self_pid: Option<u32>,
) -> Result<IndexGenerationState, rusqlite::Error> {
    if !table_exists(conn)? {
        return Ok(IndexGenerationState {
            state: "none",
            generation: None,
            owner_pid: None,
            started_at: None,
            finished_at: None,
            files: None,
            symbols: None,
            detail: format!(
                "no generation recorded yet (the index predates the seal); `{INDEX_GENERATION_REMEDY}` seals one"
            ),
        });
    }
    /// `(id, state, owner_pid, started_at, finished_at, files, symbols, note)`.
    type GenerationRow = (
        i64,
        String,
        i64,
        String,
        Option<String>,
        Option<i64>,
        Option<i64>,
        Option<String>,
    );
    let row: Option<GenerationRow> = conn
        .query_row(
            "SELECT id, state, owner_pid, started_at, finished_at, files, symbols, note
             FROM index_generation ORDER BY id DESC LIMIT 1",
            [],
            |r| {
                Ok((
                    r.get(0)?,
                    r.get(1)?,
                    r.get(2)?,
                    r.get(3)?,
                    r.get(4)?,
                    r.get(5)?,
                    r.get(6)?,
                    r.get(7)?,
                ))
            },
        )
        .optional()?;
    let Some((id, state, owner, started_at, finished_at, files, symbols, note)) = row else {
        return Ok(IndexGenerationState {
            state: "none",
            generation: None,
            owner_pid: None,
            started_at: None,
            finished_at: None,
            files: None,
            symbols: None,
            detail: format!("no generation recorded yet; `{INDEX_GENERATION_REMEDY}` seals one"),
        });
    };
    let owner_pid = u32::try_from(owner).ok();
    let (verdict, detail): (&'static str, String) = match state.as_str() {
        "complete" => (
            "complete",
            format!(
                "generation {id} complete at {} ({} files, {} symbols)",
                finished_at.as_deref().unwrap_or("?"),
                files.unwrap_or(0),
                symbols.unwrap_or(0)
            ),
        ),
        "building" => {
            let own = self_pid.is_some() && self_pid == owner_pid;
            let alive = owner_pid.is_some_and(owner_alive);
            if own || alive {
                (
                    "building",
                    format!(
                        "generation {id} is being built by pid {owner} since {started_at} — answers may mix two walks until it completes"
                    ),
                )
            } else {
                (
                    "partial",
                    format!(
                        "generation {id} was left building by pid {owner} (started {started_at}, process gone): the index holds a mix of two walks — run `{INDEX_GENERATION_REMEDY}`"
                    ),
                )
            }
        }
        _ => (
            "partial",
            format!(
                "generation {id} aborted at {} ({}): the index holds a mix of two walks — run `{INDEX_GENERATION_REMEDY}`",
                finished_at.as_deref().unwrap_or("?"),
                note.as_deref().unwrap_or("no note")
            ),
        ),
    };
    Ok(IndexGenerationState {
        state: verdict,
        generation: Some(id),
        owner_pid,
        started_at: Some(started_at),
        finished_at,
        files,
        symbols,
        detail,
    })
}

impl FileKnowledgeDB {
    /// Open a generation: every generation still `building` is superseded
    /// (`aborted`, noted as such), the table is pruned to the last
    /// `KEEP_GENERATIONS`, and a new `building` row owned by `owner_pid` is
    /// returned.
    ///
    /// # Errors
    ///
    /// Propagates any `rusqlite` failure.
    pub fn begin_index_generation(&self, owner_pid: u32) -> Result<i64, rusqlite::Error> {
        let conn = self.conn_ref();
        conn.execute_batch(INDEX_GENERATION_DDL)?;
        conn.execute(
            "UPDATE index_generation
             SET state = 'aborted', finished_at = datetime('now'),
                 note = 'superseded by a newer rebuild before completing'
             WHERE state = 'building'",
            [],
        )?;
        conn.execute(
            "DELETE FROM index_generation
             WHERE id NOT IN (SELECT id FROM index_generation ORDER BY id DESC LIMIT ?1)",
            params![KEEP_GENERATIONS],
        )?;
        conn.execute(
            "INSERT INTO index_generation (state, owner_pid) VALUES ('building', ?1)",
            params![i64::from(owner_pid)],
        )?;
        Ok(conn.last_insert_rowid())
    }

    /// Close generation `id`: `complete` when the walk finished, `aborted`
    /// otherwise (the note says why — memory pressure, for instance).
    ///
    /// # Errors
    ///
    /// Propagates any `rusqlite` failure.
    pub fn finish_index_generation(
        &self,
        id: i64,
        files: u64,
        symbols: u64,
        complete: bool,
        note: Option<&str>,
    ) -> Result<(), rusqlite::Error> {
        let state = if complete { "complete" } else { "aborted" };
        self.conn_ref().execute(
            "UPDATE index_generation
             SET state = ?2, finished_at = datetime('now'), files = ?3, symbols = ?4, note = ?5
             WHERE id = ?1",
            params![
                id,
                state,
                i64::try_from(files).unwrap_or(i64::MAX),
                i64::try_from(symbols).unwrap_or(i64::MAX),
                note
            ],
        )?;
        Ok(())
    }

    /// The latest generation as seen by the daemon `self_pid`.
    ///
    /// # Errors
    ///
    /// Propagates any `rusqlite` failure.
    pub fn index_generation_state(
        &self,
        self_pid: u32,
    ) -> Result<IndexGenerationState, rusqlite::Error> {
        read_index_generation_state(self.conn_ref(), Some(self_pid))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A pid no process on a Linux box carries (`pid_max` tops out at 2^22).
    const DEAD_PID: u32 = 4_000_000_000;

    fn db() -> (tempfile::TempDir, FileKnowledgeDB) {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = FileKnowledgeDB::new(&dir.path().join("knowledge.db")).expect("open");
        (dir, db)
    }

    #[test]
    fn a_database_without_the_table_reports_none() {
        let (_d, db) = db();
        let s = db.index_generation_state(1).expect("state");
        assert_eq!(s.state, "none");
        assert!(s.detail.contains(INDEX_GENERATION_REMEDY));
    }

    #[test]
    fn open_then_close_is_building_then_complete() {
        let (_d, db) = db();
        let me = std::process::id();
        let id = db.begin_index_generation(me).expect("begin");
        let s = db.index_generation_state(me).expect("state");
        assert_eq!(
            (s.state, s.generation, s.owner_pid),
            ("building", Some(id), Some(me))
        );
        db.finish_index_generation(id, 42, 1000, true, None)
            .expect("finish");
        let s = db.index_generation_state(me).expect("state");
        assert_eq!(s.state, "complete");
        assert_eq!((s.files, s.symbols), (Some(42), Some(1000)));
        assert!(!s.is_partial());
    }

    #[test]
    fn a_generation_left_building_by_a_dead_process_is_partial() {
        let (_d, db) = db();
        db.begin_index_generation(DEAD_PID).expect("begin");
        // Read by a different daemon (or by `doctor`, which passes no pid).
        let s = db
            .index_generation_state(std::process::id())
            .expect("state");
        assert!(s.is_partial(), "{s:?}");
        assert!(s.detail.contains("process gone"));
        assert!(s.detail.contains(INDEX_GENERATION_REMEDY));
        let external = read_index_generation_state(db.conn_ref(), None).expect("state");
        assert!(external.is_partial());
    }

    #[test]
    fn an_aborted_generation_is_partial_and_carries_its_note() {
        let (_d, db) = db();
        let me = std::process::id();
        let id = db.begin_index_generation(me).expect("begin");
        db.finish_index_generation(id, 3, 7, false, Some("memory pressure"))
            .expect("finish");
        let s = db.index_generation_state(me).expect("state");
        assert!(s.is_partial());
        assert!(s.detail.contains("memory pressure"));
    }

    #[test]
    fn a_new_generation_supersedes_one_still_building_and_a_complete_one_heals_it() {
        let (_d, db) = db();
        let me = std::process::id();
        let stale = db.begin_index_generation(DEAD_PID).expect("begin");
        let fresh = db.begin_index_generation(me).expect("begin");
        assert!(fresh > stale);
        let note: String = db
            .conn_ref()
            .query_row(
                "SELECT state || ':' || COALESCE(note, '') FROM index_generation WHERE id = ?1",
                params![stale],
                |r| r.get(0),
            )
            .expect("row");
        assert!(note.starts_with("aborted:superseded"), "{note}");
        db.finish_index_generation(fresh, 1, 1, true, None)
            .expect("finish");
        assert_eq!(
            db.index_generation_state(me).expect("state").state,
            "complete"
        );
    }

    #[test]
    fn the_table_stays_bounded() {
        let (_d, db) = db();
        let me = std::process::id();
        for _ in 0..(KEEP_GENERATIONS + 15) {
            let id = db.begin_index_generation(me).expect("begin");
            db.finish_index_generation(id, 0, 0, true, None)
                .expect("finish");
        }
        let n: i64 = db
            .conn_ref()
            .query_row("SELECT COUNT(*) FROM index_generation", [], |r| r.get(0))
            .expect("count");
        assert!(n <= KEEP_GENERATIONS + 1, "{n}");
    }

    #[test]
    fn a_dead_pid_is_not_alive_and_the_current_process_is_not_a_daemon() {
        assert!(!owner_alive(DEAD_PID));
        // The test binary is not `touring-daemon`, so its own pid reads as not a
        // live daemon — the check is on the comm, not on mere existence.
        if std::path::Path::new("/proc").exists() {
            assert!(!owner_alive(std::process::id()));
        }
    }
}
