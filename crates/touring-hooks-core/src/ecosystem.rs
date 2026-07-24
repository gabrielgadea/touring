//! Module Ecosystem Scanner — builds a map of the project's module structure.
//!
//! Scans the project directory on session-start to identify:
//! - Entry points (main.rs, lib.rs, tests/, benches/)
//! - Module tree (mod declarations, directory structure)
//! - Re-export chains (pub use)
//! - External dependencies (Cargo.toml/pyproject.toml)

use crate::knowledge::FileKnowledgeDB;

/// Role of a module in the project.
#[derive(Debug, Clone, PartialEq)]
pub enum ModuleRole {
    /// A binary entry point (`main.rs` or a `[[bin]]` target).
    EntryPoint,
    /// The crate's public library root (`lib.rs`).
    Library,
    /// An internal (non-exported) implementation module.
    Internal,
    /// A test module or file under `tests/`.
    Test,
    /// A benchmark module or file under `benches/`.
    Bench,
    /// A Cargo build script (`build.rs`).
    BuildScript,
}

impl ModuleRole {
    /// Returns the canonical lowercase string name for this role.
    pub fn as_str(&self) -> &str {
        match self {
            Self::EntryPoint => "entry_point",
            Self::Library => "library",
            Self::Internal => "internal",
            Self::Test => "test",
            Self::Bench => "bench",
            Self::BuildScript => "build_script",
        }
    }

    /// Parses a role from its string name, defaulting to `Internal` if unknown.
    pub fn parse_role(s: &str) -> Self {
        match s {
            "entry_point" => Self::EntryPoint,
            "library" => Self::Library,
            "test" => Self::Test,
            "bench" => Self::Bench,
            "build_script" => Self::BuildScript,
            _ => Self::Internal,
        }
    }
}

/// Classify a file's role based on its path.
///
/// Order matters: test/bench checks come first so that files like
/// `tests/test_main.rs` are correctly classified as Test, not EntryPoint.
pub fn classify_module_role(rel_path: &str) -> ModuleRole {
    // Test and bench paths take priority over filename-based classification
    if rel_path.starts_with("tests/") || rel_path.contains("/tests/") {
        ModuleRole::Test
    } else if rel_path.starts_with("benches/") || rel_path.contains("/benches/") {
        ModuleRole::Bench
    } else if rel_path == "src/main.rs"
        || rel_path.ends_with("/main.rs")
        || rel_path.contains("src/bin/")
    {
        ModuleRole::EntryPoint
    } else if rel_path == "src/lib.rs" || rel_path.ends_with("/lib.rs") {
        ModuleRole::Library
    } else if rel_path == "build.rs" || rel_path.ends_with("/build.rs") {
        ModuleRole::BuildScript
    } else {
        ModuleRole::Internal
    }
}

/// Scan and register a file in the module ecosystem.
pub fn register_module(
    db: &FileKnowledgeDB,
    rel_path: &str,
    pub_symbol_count: i64,
    import_count: i64,
    re_export_count: i64,
) {
    let role = classify_module_role(rel_path);
    let score = db.integration_score(rel_path).unwrap_or(1.0);
    let now = chrono::Utc::now().to_rfc3339();

    let _ = db.conn_ref().execute(
        "INSERT OR REPLACE INTO module_ecosystem
         (file_path, module_role, pub_symbol_count, import_count, re_export_count, integration_score, last_scanned_at)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7)",
        rusqlite::params![rel_path, role.as_str(), pub_symbol_count, import_count, re_export_count, score, now],
    );
}

/// Get all modules with low integration score.
pub fn low_integration_modules(db: &FileKnowledgeDB, threshold: f64) -> Vec<(String, f64)> {
    let mut stmt = match db.conn_ref().prepare(
        "SELECT file_path, integration_score FROM module_ecosystem
         WHERE integration_score < ?1 AND module_role NOT IN ('test', 'bench')
         ORDER BY integration_score ASC",
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    stmt.query_map(rusqlite::params![threshold], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, f64>(1)?))
    })
    .map(|rows| rows.filter_map(|r| r.ok()).collect())
    .unwrap_or_default()
}

/// Get all entry points in the project.
pub fn entry_points(db: &FileKnowledgeDB) -> Vec<String> {
    let mut stmt = match db.conn_ref().prepare(
        "SELECT file_path FROM module_ecosystem
         WHERE module_role IN ('entry_point', 'library')
         ORDER BY file_path",
    ) {
        Ok(s) => s,
        Err(_) => return Vec::new(),
    };

    stmt.query_map([], |row| row.get::<_, String>(0))
        .map(|rows| rows.filter_map(|r| r.ok()).collect())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn test_db() -> (TempDir, FileKnowledgeDB) {
        let tmp = TempDir::new().unwrap();
        let db_path = tmp.path().join("test.db");
        let db = FileKnowledgeDB::new(&db_path).unwrap();
        (tmp, db)
    }

    #[test]
    fn test_classify_module_role() {
        assert_eq!(classify_module_role("src/main.rs"), ModuleRole::EntryPoint);
        assert_eq!(
            classify_module_role("src/bin/cli.rs"),
            ModuleRole::EntryPoint
        );
        assert_eq!(classify_module_role("src/lib.rs"), ModuleRole::Library);
        assert_eq!(
            classify_module_role("tests/integration.rs"),
            ModuleRole::Test
        );
        assert_eq!(classify_module_role("benches/perf.rs"), ModuleRole::Bench);
        assert_eq!(classify_module_role("build.rs"), ModuleRole::BuildScript);
        assert_eq!(classify_module_role("src/utils.rs"), ModuleRole::Internal);
        assert_eq!(
            classify_module_role("src/deep/nested/module.rs"),
            ModuleRole::Internal
        );
    }

    #[test]
    fn test_module_role_roundtrip() {
        for role in [
            ModuleRole::EntryPoint,
            ModuleRole::Library,
            ModuleRole::Internal,
            ModuleRole::Test,
            ModuleRole::Bench,
            ModuleRole::BuildScript,
        ] {
            assert_eq!(ModuleRole::parse_role(role.as_str()), role);
        }
    }

    #[test]
    fn test_register_module_and_query() {
        let (_tmp, db) = test_db();
        register_module(&db, "src/main.rs", 3, 5, 0);
        register_module(&db, "src/lib.rs", 10, 2, 4);
        register_module(&db, "tests/test_main.rs", 0, 8, 0);

        let entries = entry_points(&db);
        assert_eq!(entries.len(), 2);
        assert!(entries.contains(&"src/main.rs".to_string()));
        assert!(entries.contains(&"src/lib.rs".to_string()));
    }

    #[test]
    fn test_low_integration_modules() {
        let (_tmp, db) = test_db();
        // Register a pub symbol with no consumer (score=0.0)
        db.register_pub_symbol("src/orphan.rs", "OrphanStruct", "struct", "public")
            .unwrap();
        register_module(&db, "src/orphan.rs", 1, 0, 0);

        // Register a fully wired module (no pub symbols = score 1.0)
        register_module(&db, "src/wired.rs", 0, 3, 0);

        let low = low_integration_modules(&db, 0.5);
        assert_eq!(low.len(), 1);
        assert_eq!(low[0].0, "src/orphan.rs");
        assert!(low[0].1 < 0.5);
    }

    #[test]
    fn test_test_modules_excluded_from_low_integration() {
        let (_tmp, db) = test_db();
        // Test modules should be excluded even with low scores
        db.register_pub_symbol("tests/helpers.rs", "TestHelper", "struct", "public")
            .unwrap();
        register_module(&db, "tests/helpers.rs", 1, 0, 0);

        let low = low_integration_modules(&db, 0.5);
        assert!(
            low.is_empty(),
            "test modules should be excluded from low integration report"
        );
    }
}
