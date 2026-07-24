//! File-level wiring fingerprints for incremental analysis.
//!
//! Tracks a content hash per file so wiring analysis can skip files
//! whose wiring rows have not changed since the last full analysis.

use std::collections::HashMap;

/// Per-file wiring fingerprint: a hash of the file's wiring_map rows.
///
/// Used to skip re-analysis of files whose wiring hasn't changed.
#[derive(Debug, Clone, Default)]
pub struct WiringFingerprintStore {
    /// Maps module_file → FNV-1a hash of its wiring_map rows.
    pub fingerprints: HashMap<String, u64>,
}

impl WiringFingerprintStore {
    /// Create an empty fingerprint store with no tracked files.
    pub fn new() -> Self {
        Self::default()
    }

    /// Compute fingerprint for a file from its wiring_map rows.
    ///
    /// Hash covers: symbol_name + consumer_file + visibility for each row,
    /// ordered by symbol_name then consumer_file for determinism.
    /// Returns 0 if the table doesn't exist or the file has no rows.
    pub fn compute(conn: &rusqlite::Connection, module_file: &str) -> u64 {
        let sql = "SELECT symbol_name, COALESCE(consumer_file,''), visibility \
                   FROM wiring_map WHERE module_file = ?1 ORDER BY symbol_name, consumer_file";
        let mut stmt = match conn.prepare(sql) {
            Ok(s) => s,
            Err(_) => return 0,
        };
        let rows: Vec<(String, String, String)> = stmt
            .query_map([module_file], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
            .map(|iter| iter.flatten().collect())
            .unwrap_or_default();

        if rows.is_empty() {
            return 0;
        }

        // FNV-1a hash over all rows
        let mut hash: u64 = 14_695_981_039_346_656_037;
        for (sym, consumer, vis) in &rows {
            for byte in sym
                .bytes()
                .chain(b"|".iter().copied())
                .chain(consumer.bytes())
                .chain(b"|".iter().copied())
                .chain(vis.bytes())
                .chain(b"\n".iter().copied())
            {
                hash ^= byte as u64;
                hash = hash.wrapping_mul(1_099_511_628_211);
            }
        }
        hash
    }

    /// Returns true if the stored fingerprint for `module_file` matches the current DB state.
    pub fn is_unchanged(&self, conn: &rusqlite::Connection, module_file: &str) -> bool {
        match self.fingerprints.get(module_file) {
            Some(&stored) => stored == Self::compute(conn, module_file),
            None => false,
        }
    }

    /// Update the stored fingerprint for `module_file` from current DB state.
    pub fn refresh(&mut self, conn: &rusqlite::Connection, module_file: &str) {
        let hash = Self::compute(conn, module_file);
        self.fingerprints.insert(module_file.to_string(), hash);
    }

    /// Refresh fingerprints for all files in `module_files`.
    pub fn refresh_batch(&mut self, conn: &rusqlite::Connection, module_files: &[String]) {
        for path in module_files {
            self.refresh(conn, path);
        }
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
    fn test_compute_returns_zero_for_unknown_file() {
        let conn = setup_db();
        let hash = WiringFingerprintStore::compute(&conn, "does_not_exist.rs");
        assert_eq!(hash, 0, "no rows → hash should be 0");
    }

    #[test]
    fn test_is_unchanged_returns_false_for_unknown_file() {
        let conn = setup_db();
        let store = WiringFingerprintStore::new();
        assert!(
            !store.is_unchanged(&conn, "unknown.rs"),
            "unknown file should not be considered unchanged"
        );
    }

    #[test]
    fn test_compute_nonzero_when_rows_exist() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility) \
             VALUES ('a.rs', 'Foo', 'struct', 'public')",
            [],
        )
        .expect("insert");
        let hash = WiringFingerprintStore::compute(&conn, "a.rs");
        assert_ne!(hash, 0, "existing rows → hash should be non-zero");
    }

    #[test]
    fn test_refresh_then_is_unchanged() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility) \
             VALUES ('b.rs', 'Bar', 'fn', 'public')",
            [],
        )
        .expect("insert");
        let mut store = WiringFingerprintStore::new();
        store.refresh(&conn, "b.rs");
        assert!(
            store.is_unchanged(&conn, "b.rs"),
            "after refresh, file should be unchanged"
        );
    }

    #[test]
    fn test_changed_after_insert() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility) \
             VALUES ('c.rs', 'Baz', 'struct', 'public')",
            [],
        )
        .expect("insert");
        let mut store = WiringFingerprintStore::new();
        store.refresh(&conn, "c.rs");
        // Insert another row to change the fingerprint
        conn.execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, consumer_file) \
             VALUES ('c.rs', 'Baz', 'struct', 'public', 'd.rs')",
            [],
        )
        .expect("insert consumer");
        assert!(
            !store.is_unchanged(&conn, "c.rs"),
            "after new row insert, file should be detected as changed"
        );
    }

    #[test]
    fn test_refresh_batch() {
        let conn = setup_db();
        for name in &["X", "Y"] {
            conn.execute(
                &format!(
                    "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility) \
                     VALUES ('{name}.rs', '{name}Sym', 'struct', 'public')"
                ),
                [],
            )
            .expect("insert");
        }
        let mut store = WiringFingerprintStore::new();
        let files = vec!["X.rs".to_string(), "Y.rs".to_string()];
        store.refresh_batch(&conn, &files);
        assert!(store.is_unchanged(&conn, "X.rs"));
        assert!(store.is_unchanged(&conn, "Y.rs"));
    }
}
