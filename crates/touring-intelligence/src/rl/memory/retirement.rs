//! Retired memory entries — one rule for every read that serves an agent.
//!
//! `memory store --supersedes <old>` retires `<old>`: the row stays in
//! `memory_entries` for audit, `superseded_by` names its successor, and it
//! must never surface again. Until 18/09/2026 only the SQL arm of `memory
//! recall` honoured that. Its ANN and TF-IDF arms, `memory query`, the MOC
//! corpus and the lessons the hooks inject read the table (or a store built
//! from it) without the rule, and a superseded probe came back 6th in a recall
//! in the analise project. Every such read now appends [`live_predicate`] to
//! its SQL or asks [`retirement_of`] about the keys it gathered elsewhere.
//! Exact reads by key (`get`, the audit listing) still see retired rows on
//! purpose: retirement is not deletion.

use rusqlite::{Connection, OptionalExtension, params};
use std::collections::HashSet;

/// Whether this database has the retirement column. Federated reads reach
/// other projects' databases, which may predate it; nothing is retired there.
fn has_retirement(conn: &Connection) -> bool {
    conn.query_row(
        "SELECT COUNT(*) FROM pragma_table_info('memory_entries') \
         WHERE name = 'superseded_by'",
        [],
        |row| row.get::<_, i64>(0),
    )
    .map(|count| count > 0)
    .unwrap_or(false)
}

/// `AND` clause keeping only live rows of `memory_entries`, with `alias` as
/// the table prefix (`""`, `"e."`, `"m."`). Empty when the database predates
/// retirement, so the clause can be appended to any `WHERE`.
pub fn live_predicate(conn: &Connection, alias: &str) -> String {
    if has_retirement(conn) {
        format!(" AND {alias}superseded_by IS NULL")
    } else {
        String::new()
    }
}

/// Every retired key in this database, in one query. Retired entries are few,
/// so a read over a large key set (a tag universe of up to 100 000 keys)
/// filters against this set instead of asking key by key.
pub fn retired_keys(conn: &Connection) -> HashSet<String> {
    if !has_retirement(conn) {
        return HashSet::new();
    }
    let Ok(mut stmt) =
        conn.prepare("SELECT DISTINCT key FROM memory_entries WHERE superseded_by IS NOT NULL")
    else {
        return HashSet::new();
    };
    stmt.query_map([], |row| row.get::<_, String>(0))
        .map(|rows| rows.filter_map(Result::ok).collect())
        .unwrap_or_default()
}

/// What this database says about `keys`: `(held, retired)`, with
/// `retired ⊆ held`. A key the database does not hold is in neither set, so a
/// caller reading several databases can let the first one that holds a key
/// speak for it — the precedence the federated SQL recall already uses.
pub fn retirement_of(conn: &Connection, keys: &[&str]) -> (HashSet<String>, HashSet<String>) {
    let mut held = HashSet::new();
    let mut retired = HashSet::new();
    let sql = if has_retirement(conn) {
        "SELECT superseded_by IS NOT NULL FROM memory_entries WHERE key = ?1 LIMIT 1"
    } else {
        "SELECT 0 FROM memory_entries WHERE key = ?1 LIMIT 1"
    };
    let Ok(mut stmt) = conn.prepare(sql) else {
        return (held, retired);
    };
    for key in keys {
        let state = stmt
            .query_row(params![key], |row| row.get::<_, bool>(0))
            .optional()
            .ok()
            .flatten();
        if let Some(is_retired) = state {
            held.insert((*key).to_string());
            if is_retired {
                retired.insert((*key).to_string());
            }
        }
    }
    (held, retired)
}

#[cfg(test)]
mod tests {
    #![allow(clippy::unwrap_used)]
    use super::*;

    fn db(with_column: bool) -> Connection {
        let conn = Connection::open_in_memory().unwrap();
        let retirement = if with_column {
            ", superseded_by TEXT"
        } else {
            ""
        };
        conn.execute_batch(&format!(
            "CREATE TABLE memory_entries (key TEXT, tier TEXT, value TEXT{retirement});"
        ))
        .unwrap();
        conn
    }

    #[test]
    fn a_database_without_the_column_has_nothing_retired() {
        let conn = db(false);
        conn.execute(
            "INSERT INTO memory_entries VALUES ('a', 'semantic', 'x')",
            [],
        )
        .unwrap();
        assert_eq!(live_predicate(&conn, "e."), "");
        let (held, retired) = retirement_of(&conn, &["a", "absent"]);
        assert_eq!(held, HashSet::from(["a".to_string()]));
        assert!(retired.is_empty());
    }

    #[test]
    fn the_predicate_keeps_live_rows_and_drops_retired_ones() {
        let conn = db(true);
        conn.execute_batch(
            "INSERT INTO memory_entries VALUES ('old', 'semantic', 'v1', 'new');
             INSERT INTO memory_entries VALUES ('new', 'semantic', 'v2', NULL);",
        )
        .unwrap();
        let predicate = live_predicate(&conn, "e.");
        assert_eq!(predicate, " AND e.superseded_by IS NULL");
        let live: Vec<String> = conn
            .prepare(&format!(
                "SELECT e.key FROM memory_entries e WHERE 1 = 1{predicate}"
            ))
            .unwrap()
            .query_map([], |row| row.get(0))
            .unwrap()
            .collect::<Result<_, _>>()
            .unwrap();
        assert_eq!(live, vec!["new".to_string()]);
    }

    #[test]
    fn retirement_of_separates_held_retired_and_absent_keys() {
        let conn = db(true);
        conn.execute_batch(
            "INSERT INTO memory_entries VALUES ('old', 'semantic', 'v1', 'new');
             INSERT INTO memory_entries VALUES ('new', 'semantic', 'v2', NULL);",
        )
        .unwrap();
        let (held, retired) = retirement_of(&conn, &["old", "new", "absent"]);
        assert_eq!(held, HashSet::from(["old".to_string(), "new".to_string()]));
        assert_eq!(retired, HashSet::from(["old".to_string()]));
        assert_eq!(retired_keys(&conn), HashSet::from(["old".to_string()]));
        assert!(retired_keys(&db(false)).is_empty());
    }
}
