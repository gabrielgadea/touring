//! Orphan symbol detection via knowledge DB queries.
//!
//! ## Consumer Type Taxonomy
//!
//! The wiring DB tracks consumer relationships in `wiring_map.consumer_type`:
//! - `'rust_import'`   — Rust crate-to-crate via `use`/`mod` statements
//! - `'daemon_hook'`   — touring-daemon dispatch table (`ALL_DAEMON_HOOK_NAMES`)
//! - `'ipc_socket'`    — Unix socket client connection
//! - `'cli_handler'`  — touring CLI subcommand handler
//!
//! Symbols in daemon crates (e.g. `touring-hooks`) that are served via the
//! touring-daemon dispatch table have `consumer_type = 'daemon_hook'` and
//! are **not orphans** — they have real consumers via the socket protocol.
//!
//! ## Diagnostic Layers
//!
//! | Layer | What it does | Improves orphan precision |
//! |-------|--------------|------------------------------|
//! | **L1** | SQL path exclusions for benches/tests/examples | Filters test/harness false positives |
//! | **L2** | `touring index rebuild --full` to re-parse all source files | Fresh consumer data from imports |
//! | **L3** | Suffix-only dead pattern matching (`_unused`, `_dead`, etc.) | Eliminates substring false positives |
//! | **L4** | IPC consumer enrichment — daemon dispatch table mapped to wiring_map | ✅ DONE — 2352 daemon_hook symbols marked non-orphan |
//!
//! ## Schema Compatibility
//!
//! The orphan queries handle **two DB shapes** at runtime:
//! - **L4 schema**: `wiring_map` has `consumer_type` column — IPC-served symbols
//!   (daemon_hook, ipc_socket, cli_handler) are excluded, only `rust_import` counts.
//! - **Pre-L4 schema**: no `consumer_type` column — `column_exists()` check gates
//!   which SQL variant runs. Legacy test DBs created via `KNOWLEDGE_SCHEMA_V8` have
//!   `consumer_type DEFAULT 'rust_import'` so COALESCE in L4 queries handles them.
//!
//! ## L4 IPC Enrichment (Completed)
//!
//! The daemon dispatch table (`hook_registry::build_dispatch_table`) maps 204
//! hook names → Rust handler modules in touring-hooks. The enrichment:
//! 1. Adds `consumer_type TEXT DEFAULT 'rust_import'` column to wiring_map
//! 2. Populates `consumer_type = 'daemon_hook'` for touring-hooks symbols via
//!    `knowledge.rs` L4.1 enrichment (run at DB migration time)
//! 3. Updates `consumer_file` to `touring-daemon://dispatch` as synthetic URI
//!    This eliminates the ~94% false-positive orphan rate for daemon crates.

use touring_foundation::schema_guard;

/// SQL WHERE fragment that excludes non-production module paths (benches,
/// tests, examples, harness, fixture directories) from orphan queries.
///
/// Shared by all four `count_orphans` SQL variants to eliminate the 6-line
/// verbatim repeat that previously appeared in every format! string.
const SQL_PATH_EXCLUSION: &str = "WHERE NOT module_file LIKE '%/benches/%' \
     AND NOT module_file LIKE '%/tests/%' \
     AND NOT module_file LIKE '%/examples/%' \
     AND NOT module_file LIKE '%/test_harness%' \
     AND NOT module_file LIKE '%/benchmark_harness%' \
     AND NOT module_file LIKE '%/test_fixture%'";

/// Orphan detection result.
#[derive(Debug, Clone)]
pub struct OrphanResult {
    /// Total public symbols in wiring_map.
    pub total_pub: usize,
    /// Symbols with no consumer.
    pub orphan_count: usize,
    /// Total consumer relationships.
    pub total_consumers: usize,
    /// List of orphan (module_file, symbol_name) pairs.
    pub orphans: Vec<(String, String)>,
    /// Count of private symbols with zero callers (dead code candidates).
    pub dead_code_count: usize,
    /// Names of private symbols with zero callers.
    pub dead_code_symbols: Vec<String>,
    /// Symbol names matching dead code patterns (_unused, _dead, _old, etc.).
    pub dead_patterns: Vec<String>,
}

// Compile-time invariant: OrphanResult crosses thread boundaries in the
// rayon-powered wiring pipeline and is cloned into Tantivy enrichment
// closures. Breaking Send/Sync here would silently serialize analysis.
static_assertions::assert_impl_all!(OrphanResult: Send, Sync, Clone);

/// Returns true if the given column exists in the given table.
fn column_exists(conn: &rusqlite::Connection, table: &str, column: &str) -> bool {
    let sql = format!("PRAGMA table_info({})", table);
    match conn.prepare(&sql) {
        Ok(mut stmt) => {
            let mut has_col = false;
            if let Ok(mut rows) = stmt.query([]) {
                while let Ok(Some(row)) = rows.next() {
                    if let Ok(col_name) = row.get::<_, String>(1) {
                        if col_name == column {
                            has_col = true;
                            break;
                        }
                    }
                }
            }
            has_col
        }
        _ => false,
    }
}

/// Count orphan symbols from the wiring_map table.
pub fn count_orphans(conn: &rusqlite::Connection) -> OrphanResult {
    let table = schema_guard::TABLE_WIRING_MAP;

    // Total distinct pub symbols.
    // Use char(0) (NUL) as separator instead of '::' — symbol_name may itself
    // contain '::' (e.g. 'crate::Foo'), which would cause false collisions.
    let total_pub: i64 = conn
        .query_row(
            &format!("SELECT COUNT(DISTINCT module_file || char(0) || symbol_name) FROM {table}"),
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    // Orphans: pub symbols where NO row of the (module_file, symbol_name) pair
    // carries a consumer. IPC-served symbols (daemon_hook, ipc_socket,
    // cli_handler) have consumers via dispatch table / socket / CLI — they are
    // not orphans. Handle two DB shapes: L4 schema (consumer_type present) and
    // pre-L4 schema (absent).
    //
    // Fix 2026-07-01: the producer registration keeps a placeholder row with
    // `consumer_file NULL` even after real consumer rows are inserted for the
    // SAME pair (INSERT OR IGNORE producer + one row per consumer). The old
    // `WHERE consumer_file IS NULL` counted the pair as orphan because of its
    // placeholder — inflating orphan_count by ~48% on the live workspace DB
    // (11917 → 6169 measured; e.g. `save_linucb` with 11 real consumer rows
    // still reported as orphan). A pair is an orphan only when it has ZERO
    // consumer rows AND at least one rust_import-typed placeholder (the L4
    // IPC taxonomy marks IPC-served placeholders with a non-rust_import type).
    let orphan_count = if column_exists(conn, table, "consumer_type") {
        let sql = format!(
            "SELECT COUNT(*) FROM ( \
               SELECT module_file, symbol_name FROM {table} \
               {SQL_PATH_EXCLUSION} \
               GROUP BY module_file, symbol_name \
               HAVING SUM(CASE WHEN consumer_file IS NOT NULL THEN 1 ELSE 0 END) = 0 \
                  AND SUM(CASE WHEN consumer_file IS NULL \
                            AND COALESCE(consumer_type, 'rust_import') = 'rust_import' \
                          THEN 1 ELSE 0 END) > 0 \
             )"
        );
        conn.query_row(&sql, [], |r| r.get::<_, i64>(0))
            .unwrap_or(0)
    } else {
        // Pre-L4 schema: no IPC taxonomy — a pair with zero consumer rows is
        // an orphan (its remaining rows are all NULL placeholders by definition).
        let sql = format!(
            "SELECT COUNT(*) FROM ( \
               SELECT module_file, symbol_name FROM {table} \
               {SQL_PATH_EXCLUSION} \
               GROUP BY module_file, symbol_name \
               HAVING SUM(CASE WHEN consumer_file IS NOT NULL THEN 1 ELSE 0 END) = 0 \
             )"
        );
        conn.query_row(&sql, [], |r| r.get::<_, i64>(0))
            .unwrap_or(0)
    };

    // Total consumer relationships (non-null consumer_file)
    let total_consumers: i64 = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM {table} WHERE consumer_file IS NOT NULL"),
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    // List orphan details — the SAME pair-level predicate as the count above
    // (fix 2026-07-01: a pair with any real consumer row is not listed, even
    // when its producer placeholder row has consumer_file NULL).
    let mut orphans = Vec::new();
    if column_exists(conn, table, "consumer_type") {
        if let Ok(mut stmt) = conn.prepare(&format!(
            "SELECT module_file, symbol_name FROM {table} \
             {SQL_PATH_EXCLUSION} \
             GROUP BY module_file, symbol_name \
             HAVING SUM(CASE WHEN consumer_file IS NOT NULL THEN 1 ELSE 0 END) = 0 \
                AND SUM(CASE WHEN consumer_file IS NULL \
                          AND COALESCE(consumer_type, 'rust_import') = 'rust_import' \
                        THEN 1 ELSE 0 END) > 0 \
             ORDER BY module_file, symbol_name"
        )) {
            if let Ok(rows) = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            }) {
                for row in rows.flatten() {
                    orphans.push(row);
                }
            }
        }
    } else {
        // Pre-L4 schema: no consumer_type column — zero consumer rows ⇒ orphan.
        if let Ok(mut stmt) = conn.prepare(&format!(
            "SELECT module_file, symbol_name FROM {table} \
             {SQL_PATH_EXCLUSION} \
             GROUP BY module_file, symbol_name \
             HAVING SUM(CASE WHEN consumer_file IS NOT NULL THEN 1 ELSE 0 END) = 0 \
             ORDER BY module_file, symbol_name"
        )) {
            if let Ok(rows) = stmt.query_map([], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            }) {
                for row in rows.flatten() {
                    orphans.push(row);
                }
            }
        }
    }

    // Deduplicate orphan list — the count query uses DISTINCT but the
    // detail query may return duplicates when multiple NULL-consumer rows
    // exist for the same (module_file, symbol_name) pair.
    orphans.sort();
    orphans.dedup();

    // Dead code: private symbols with zero callers.
    let (dead_code_count, dead_code_symbols) = dead_code_detect(conn);

    // Dead patterns: orphan symbol names matching suspicious naming patterns.
    let symbol_names: Vec<String> = orphans.iter().map(|(_, n)| n.clone()).collect();
    let dead_patterns = scan_dead_patterns(&symbol_names);

    OrphanResult {
        total_pub: total_pub as usize,
        orphan_count: orphan_count as usize,
        total_consumers: total_consumers as usize,
        orphans,
        dead_code_count,
        dead_code_symbols,
        dead_patterns,
    }
}

/// Detect private symbols with zero callers (dead code candidates).
///
/// Returns `(count, Vec<symbol_name>)` using `wiring_map.visibility = 'private'`.
/// Returns `(0, vec![])` gracefully if `wiring_map` lacks a `visibility` column
/// or if the table does not exist.
pub fn dead_code_detect(conn: &rusqlite::Connection) -> (usize, Vec<String>) {
    let table = schema_guard::TABLE_WIRING_MAP;

    // Dead code: private symbols with zero rust_import consumers.
    // IPC-served symbols (daemon_hook, ipc_socket, cli_handler) are wired — not dead.
    // Handle two DB shapes: L4 schema (consumer_type present) and pre-L4 schema (absent).
    // Fix 2026-07-01 (same placeholder bug as count_orphans): a private symbol
    // whose pair has real consumer rows is called — not dead — even when its
    // producer placeholder row still has consumer_file NULL.
    let sql = if column_exists(conn, table, "consumer_type") {
        format!(
            "SELECT DISTINCT symbol_name FROM ( \
               SELECT module_file, symbol_name FROM {table} \
               WHERE visibility = 'private' \
               GROUP BY module_file, symbol_name \
               HAVING SUM(CASE WHEN consumer_file IS NOT NULL THEN 1 ELSE 0 END) = 0 \
                  AND SUM(CASE WHEN consumer_file IS NULL \
                            AND COALESCE(consumer_type, 'rust_import') = 'rust_import' \
                          THEN 1 ELSE 0 END) > 0 \
             ) ORDER BY symbol_name \
             LIMIT 100"
        )
    } else {
        // Pre-L4 schema: no consumer_type column
        format!(
            "SELECT DISTINCT symbol_name FROM ( \
               SELECT module_file, symbol_name FROM {table} \
               WHERE visibility = 'private' \
               GROUP BY module_file, symbol_name \
               HAVING SUM(CASE WHEN consumer_file IS NOT NULL THEN 1 ELSE 0 END) = 0 \
             ) ORDER BY symbol_name \
             LIMIT 100"
        )
    };

    let mut stmt = match conn.prepare(&sql) {
        Ok(s) => s,
        Err(_) => return (0, vec![]),
    };

    let names: Vec<String> = match stmt.query_map([], |row| row.get::<_, String>(0)) {
        Ok(rows) => rows.flatten().collect(),
        Err(_) => vec![],
    };

    let count = names.len();
    (count, names)
}

/// Scan symbol names for dead code patterns using multi-pattern matching.
///
/// Uses [`aho_corasick::AhoCorasick`] for efficient simultaneous matching
/// of multiple dead-code indicators: `_unused`, `_dead`, `_old`, `_deprecated`,
/// `_legacy`, `_stub`.
///
/// Enabled only when the `simd-wiring` feature is active.
/// Returns an empty vec when the feature is disabled.
///
/// Returns a list of symbol names that match one or more patterns.
pub fn scan_dead_patterns(symbols: &[String]) -> Vec<String> {
    #[cfg(feature = "simd-wiring")]
    {
        let patterns = [
            "_unused",
            "_dead",
            "_old",
            "_deprecated",
            "_legacy",
            "_stub",
        ];
        // Suffix-only matching: only match at END of symbol name.
        // ac.is_match() is substring — we use explicit ends_with() instead.
        // This eliminates false positives like "check_deadlines" (contains "_dead"
        // in "deadlines" but isn't a dead-code indicator) and
        // "cleanup_old_entries" (contains "_old" in "old_entries").
        symbols
            .iter()
            .filter(|s| patterns.iter().any(|p| s.ends_with(p)))
            .cloned()
            .collect()
    }
    #[cfg(not(feature = "simd-wiring"))]
    {
        let _ = symbols;
        Vec::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_db() -> rusqlite::Connection {
        let conn = rusqlite::Connection::open_in_memory().expect("open");
        conn.execute_batch(touring_foundation::schema::knowledge::KNOWLEDGE_SCHEMA_V8)
            .expect("schema");
        conn
    }

    #[test]
    fn test_empty_wiring() {
        let conn = setup_db();
        let result = count_orphans(&conn);
        assert_eq!(result.total_pub, 0);
        assert_eq!(result.orphan_count, 0);
    }

    #[test]
    fn test_orphan_detected() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility) \
             VALUES ('lib.rs', 'MyStruct', 'struct', 'public')",
            [],
        )
        .expect("insert");
        let result = count_orphans(&conn);
        assert_eq!(result.orphan_count, 1);
        assert_eq!(result.orphans.len(), 1);
        assert_eq!(
            result.orphans.first(),
            Some(&("lib.rs".to_string(), "MyStruct".to_string()))
        );
    }

    #[test]
    fn test_consumed_symbol_not_orphan() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, consumer_file) \
             VALUES ('lib.rs', 'MyStruct', 'struct', 'public', 'main.rs')",
            [],
        ).expect("insert");
        let result = count_orphans(&conn);
        assert_eq!(result.orphan_count, 0);
        assert_eq!(result.total_consumers, 1);
    }

    #[test]
    fn test_orphan_count_never_exceeds_total_pub() {
        let conn = setup_db();
        // Same symbol with a NULL consumer row AND a non-NULL consumer row —
        // the producer-placeholder shape the live indexer writes (INSERT OR
        // IGNORE producer + one row per consumer; the unique index allows
        // both because COALESCE(consumer_file, '') differs).
        //
        // Fix 2026-07-01: this test previously ASSERTED the placeholder bug
        // ("the NULL consumer row counts as an orphan"). On the live
        // workspace DB that inflated orphan_count by ~48% — e.g. `save_linucb`
        // with 11 real consumer rows was still reported as orphan. A consumed
        // pair is NOT an orphan; the invariant orphan_count <= total_pub holds.
        conn.execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility) \
             VALUES ('lib.rs', 'Dup', 'struct', 'public')",
            [],
        )
        .expect("insert producer placeholder row");
        conn.execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, consumer_file) \
             VALUES ('lib.rs', 'Dup', 'struct', 'public', 'main.rs')",
            [],
        ).expect("insert consumer row");
        let result = count_orphans(&conn);
        assert_eq!(
            result.total_pub, 1,
            "same symbol in two rows should count as 1 distinct"
        );
        assert_eq!(
            result.orphan_count, 0,
            "a pair with a real consumer row is consumed — its producer \
             placeholder row must not count it as an orphan"
        );
        assert!(
            result.orphans.is_empty(),
            "the orphan list must agree with the count"
        );
        assert!(
            result.orphan_count <= result.total_pub,
            "orphan_count ({}) must never exceed total_pub ({})",
            result.orphan_count,
            result.total_pub
        );
    }

    #[test]
    fn test_placeholder_only_pair_is_still_orphan() {
        let conn = setup_db();
        // Control for the 2026-07-01 fix: a pair with ONLY the producer
        // placeholder row (zero consumer rows) remains a real orphan.
        conn.execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility) \
             VALUES ('lib.rs', 'LonelyFn', 'function', 'public')",
            [],
        )
        .expect("insert producer placeholder row");
        let result = count_orphans(&conn);
        assert_eq!(result.orphan_count, 1, "no consumer rows ⇒ orphan");
        assert_eq!(
            result.orphans.first(),
            Some(&("lib.rs".to_string(), "LonelyFn".to_string()))
        );
    }

    #[test]
    fn test_symbol_with_double_colon_in_name() {
        let conn = setup_db();
        // 'crate::Foo' and 'Foo' in same module must be 2 distinct symbols.
        // With the old '::' separator, 'lib.rs::crate::Foo' could collide.
        // The char(0) separator prevents this.
        conn.execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility) \
             VALUES ('lib.rs', 'crate::Foo', 'struct', 'public')",
            [],
        )
        .expect("insert crate::Foo");
        conn.execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility) \
             VALUES ('lib.rs', 'Foo', 'struct', 'public')",
            [],
        )
        .expect("insert Foo");
        let result = count_orphans(&conn);
        assert_eq!(
            result.total_pub, 2,
            "symbols 'crate::Foo' and 'Foo' should be 2 distinct entries"
        );
        assert_eq!(result.orphan_count, 2);
    }

    #[test]
    fn test_scan_dead_patterns_returns_empty_without_feature() {
        // Without `simd-wiring` feature, scan_dead_patterns always returns empty.
        // This test runs in the default feature set (simd-wiring is NOT in default).
        let symbols = vec![
            "my_unused_fn".to_string(),
            "old_handler".to_string(),
            "clean_symbol".to_string(),
        ];
        let result = scan_dead_patterns(&symbols);
        // simd-wiring is not in default features, so result must be empty.
        #[cfg(not(feature = "simd-wiring"))]
        assert!(
            result.is_empty(),
            "scan_dead_patterns must return empty when simd-wiring is off"
        );
        // When simd-wiring is on, we expect matches — just confirm no panic.
        #[cfg(feature = "simd-wiring")]
        let _ = result;
    }

    #[test]
    #[cfg(feature = "simd-wiring")]
    fn test_scan_dead_patterns_suffix_only_matching() {
        // Suffix-only: patterns must match at END of symbol name, not anywhere.
        // "_dead" in "deadlines" is NOT a match — only suffix position counts.
        let symbols = vec![
            "check_deadlines".to_string(), // contains "_dead" in middle → no match
            "cleanup_old_entries".to_string(), // contains "_old" in middle → no match
            "fn_is_unused".to_string(),    // ends with "_unused" → match
            "thing_is_deprecated".to_string(), // ends with "_deprecated" → match
            "handler_is_old".to_string(),  // ends with "_old" → match
            "bridge_is_legacy".to_string(), // ends with "_legacy" → match
            "impl_is_stub".to_string(),    // ends with "_stub" → match
            "marker_is_dead".to_string(),  // ends with "_dead" → match
            "code_is_dead".to_string(),    // ends with "_dead" → match
            "clean_symbol".to_string(),    // no suffix pattern → no match
        ];
        let result = scan_dead_patterns(&symbols);
        let matched: Vec<&str> = result.iter().map(|s| s.as_str()).collect();
        eprintln!("DEBUG matched: {:?}", matched);
        eprintln!("DEBUG count: {}", matched.len());
        assert!(
            !matched.contains(&"check_deadlines"),
            "check_deadlines contains '_dead' in 'deadlines' but is NOT suffix → must NOT match"
        );
        assert!(
            !matched.contains(&"cleanup_old_entries"),
            "cleanup_old_entries contains '_old' in 'old_entries' but is NOT suffix → must NOT match"
        );
        assert!(
            matched.contains(&"fn_is_unused"),
            "fn_is_unused ends with '_unused' → must match"
        );
        assert!(
            matched.contains(&"thing_is_deprecated"),
            "thing_is_deprecated ends with '_deprecated' → must match"
        );
        assert!(
            matched.contains(&"handler_is_old"),
            "handler_is_old ends with '_old' → must match"
        );
        assert!(
            matched.contains(&"bridge_is_legacy"),
            "bridge_is_legacy ends with '_legacy' → must match"
        );
        assert!(
            matched.contains(&"impl_is_stub"),
            "impl_is_stub ends with '_stub' → must match"
        );
        assert!(
            matched.contains(&"marker_is_dead"),
            "marker_is_dead ends with '_dead' → must match"
        );
        assert!(
            matched.contains(&"code_is_dead"),
            "code_is_dead ends with '_dead' → must match"
        );
        assert!(
            !matched.contains(&"clean_symbol"),
            "clean_symbol has no suffix pattern → must NOT match"
        );
        assert_eq!(
            matched.len(),
            7,
            "expected exactly 7 suffix-matched symbols (2 for _dead)"
        );
    }

    #[test]
    fn test_daemon_hook_symbol_not_orphan() {
        // daemon_hook symbols have consumer_type='daemon_hook' and consumer_file='touring-daemon://dispatch'
        // They are IPC-served and must NOT count as orphans even when consumer_file IS NOT NULL
        // (the daemon IS the consumer via the dispatch table).
        let conn = rusqlite::Connection::open_in_memory().expect("open");
        conn.execute_batch(
            "CREATE TABLE wiring_map ( \
                module_file TEXT, symbol_name TEXT, symbol_kind TEXT, visibility TEXT, \
                consumer_file TEXT, consumer_type TEXT DEFAULT 'rust_import' \
            )",
        )
        .expect("schema");
        // Insert a daemon_hook symbol (IPC-served via dispatch table)
        conn.execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, consumer_file, consumer_type) \
             VALUES ('touring-hooks/src/lib.rs', 'handle_hook_dispatch', 'fn', 'public', \
                     'touring-daemon://dispatch', 'daemon_hook')",
            [],
        )
        .expect("insert daemon_hook");
        let result = count_orphans(&conn);
        assert_eq!(
            result.orphan_count, 0,
            "daemon_hook symbol must NOT be orphan"
        );
        assert_eq!(
            result.total_consumers, 1,
            "daemon_hook has a consumer (the dispatch table)"
        );
    }

    #[test]
    fn test_consumer_type_ipc_socket_not_orphan() {
        // ipc_socket symbols are served via Unix socket connection — not orphans.
        let conn = rusqlite::Connection::open_in_memory().expect("open");
        conn.execute_batch(
            "CREATE TABLE wiring_map ( \
                module_file TEXT, symbol_name TEXT, symbol_kind TEXT, visibility TEXT, \
                consumer_file TEXT, consumer_type TEXT DEFAULT 'rust_import' \
            )",
        )
        .expect("schema");
        conn.execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, consumer_file, consumer_type) \
             VALUES ('touring-daemon/src/ipc.rs', 'socket_handler', 'fn', 'public', \
                     'touring-daemon://socket', 'ipc_socket')",
            [],
        )
        .expect("insert ipc_socket");
        let result = count_orphans(&conn);
        assert_eq!(
            result.orphan_count, 0,
            "ipc_socket symbol must NOT be orphan"
        );
    }

    #[test]
    fn test_consumer_type_cli_handler_not_orphan() {
        // cli_handler symbols are served via CLI subcommand dispatch — not orphans.
        let conn = rusqlite::Connection::open_in_memory().expect("open");
        conn.execute_batch(
            "CREATE TABLE wiring_map ( \
                module_file TEXT, symbol_name TEXT, symbol_kind TEXT, visibility TEXT, \
                consumer_file TEXT, consumer_type TEXT DEFAULT 'rust_import' \
            )",
        )
        .expect("schema");
        conn.execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, consumer_file, consumer_type) \
             VALUES ('touring-hooks/src/cli_handlers.rs', 'cli_decompose_add', 'fn', 'public', \
                     'touring-daemon://cli', 'cli_handler')",
            [],
        )
        .expect("insert cli_handler");
        let result = count_orphans(&conn);
        assert_eq!(
            result.orphan_count, 0,
            "cli_handler symbol must NOT be orphan"
        );
    }

    #[test]
    fn test_only_rust_import_orphans_count() {
        // Only rust_import symbols with no consumers count as orphans.
        // daemon_hook/ipc_socket/cli_handler with NULL consumer_file are NOT orphans.
        let conn = rusqlite::Connection::open_in_memory().expect("open");
        conn.execute_batch(
            "CREATE TABLE wiring_map ( \
                module_file TEXT, symbol_name TEXT, symbol_kind TEXT, visibility TEXT, \
                consumer_file TEXT, consumer_type TEXT DEFAULT 'rust_import' \
            )",
        )
        .expect("schema");
        // rust_import orphan — should count
        conn.execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, consumer_type) \
             VALUES ('lib.rs', 'unused_func', 'fn', 'public', 'rust_import')",
            [],
        )
        .expect("insert rust_import orphan");
        // daemon_hook with NULL consumer_file — should NOT count (IPC-served)
        conn.execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, consumer_type) \
             VALUES ('hooks.rs', 'hook_handler', 'fn', 'public', 'daemon_hook')",
            [],
        )
        .expect("insert daemon_hook no consumer");
        let result = count_orphans(&conn);
        assert_eq!(
            result.orphan_count, 1,
            "only rust_import orphan should count"
        );
        assert_eq!(
            result.orphans.first(),
            Some(&("lib.rs".to_string(), "unused_func".to_string())),
            "the rust_import orphan should be listed, not the daemon_hook"
        );
    }
}
