//! Knowledge DB dimension: file stats, language distribution, hot files, gotcha rate.
//!
//! Queries `file_knowledge`, `edit_history`, `gotchas` to derive a
//! knowledge health score. Higher scores indicate a well-indexed, stable codebase.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use touring_foundation::schema_guard;

/// Knowledge dimension analysis result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeReport {
    /// Total indexed files.
    pub total_files: usize,
    /// Language distribution: language → file count.
    pub language_distribution: HashMap<String, usize>,
    /// Average line count per file.
    pub avg_line_count: f64,
    /// Symbol density: symbols per line across all files.
    pub avg_symbol_density: f64,
    /// Files edited 3+ times in the last 7 days (instability signal).
    pub hot_files: usize,
    /// Active (unresolved) gotchas.
    pub active_gotchas: usize,
    /// Import graph health: edges / (nodes + 1), clamped to [0, 1].
    pub import_graph_health: f64,
    /// Composite knowledge score (0.0–1.0).
    pub score: f64,
}

/// Analyze knowledge DB health.
///
/// Queries the knowledge DB for file stats, language distribution, hot files,
/// gotcha rate, and import graph density. Returns a `KnowledgeReport` with
/// a composite score.
pub fn analyze_knowledge(conn: &rusqlite::Connection) -> KnowledgeReport {
    let file_table = schema_guard::TABLE_FILE_KNOWLEDGE;
    let edit_table = schema_guard::TABLE_EDIT_HISTORY;
    let gotcha_table = schema_guard::TABLE_GOTCHAS;
    let relations_table = schema_guard::TABLE_FILE_RELATIONS;

    // Total files
    let total_files: i64 = conn
        .query_row(&format!("SELECT COUNT(*) FROM {file_table}"), [], |r| {
            r.get(0)
        })
        .unwrap_or(0);
    let total_files = total_files as usize;

    // Language distribution
    let mut language_distribution: HashMap<String, usize> = HashMap::new();
    if let Ok(mut stmt) = conn.prepare(&format!(
        "SELECT language, COUNT(*) FROM {file_table} \
         WHERE language IS NOT NULL GROUP BY language ORDER BY COUNT(*) DESC"
    )) && let Ok(rows) = stmt.query_map([], |row| {
        Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
    }) {
        for row in rows.flatten() {
            language_distribution.insert(row.0, row.1);
        }
    }

    // Average line count
    let avg_line_count: f64 = conn
        .query_row(
            &format!("SELECT COALESCE(AVG(line_count), 0.0) FROM {file_table}"),
            [],
            |r| r.get(0),
        )
        .unwrap_or(0.0);

    // Symbol density: total symbols / total lines
    let (total_symbols, total_lines): (i64, i64) = conn
        .query_row(
            &format!(
                "SELECT COALESCE(SUM(symbol_count), 0), COALESCE(SUM(line_count), 1) FROM {file_table}"
            ),
            [],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .unwrap_or((0, 1));
    let avg_symbol_density = if total_lines > 0 {
        total_symbols as f64 / total_lines as f64
    } else {
        0.0
    };

    // Hot files: edited 3+ times in 7 days
    let hot_files: i64 = conn
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM (\
                    SELECT file_path FROM {edit_table} \
                    WHERE edited_at > datetime('now', '-7 days') \
                    GROUP BY file_path HAVING COUNT(*) >= 3\
                )"
            ),
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let hot_files = hot_files as usize;

    // Active gotchas (decay_score > 0.3 means still relevant)
    let active_gotchas: i64 = conn
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM {gotcha_table} \
                 WHERE resolved_at IS NULL AND decay_score > 0.3"
            ),
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let active_gotchas = active_gotchas as usize;

    // Import graph health: edges / (nodes + 1)
    let edge_count: i64 = conn
        .query_row(
            &format!("SELECT COUNT(*) FROM {relations_table}"),
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);
    let import_graph_health = if total_files > 0 {
        ((edge_count as f64) / (total_files as f64 + 1.0)).min(1.0)
    } else {
        0.0
    };

    // Score computation
    let file_coverage = if total_files > 0 { 1.0 } else { 0.0 };
    let density_ok = if (0.01..=0.5).contains(&avg_symbol_density) {
        1.0
    } else if avg_symbol_density > 0.0 {
        0.5
    } else {
        0.0
    };
    let hot_rate = if total_files > 0 {
        (hot_files as f64 / total_files as f64).min(1.0)
    } else {
        0.0
    };
    let gotcha_rate = if total_files > 0 {
        (active_gotchas as f64 / total_files as f64).min(1.0)
    } else {
        0.0
    };

    let score = (0.3 * file_coverage
        + 0.3 * density_ok
        + 0.2 * (1.0 - hot_rate)
        + 0.2 * (1.0 - gotcha_rate))
        .clamp(0.0, 1.0);

    KnowledgeReport {
        total_files,
        language_distribution,
        avg_line_count,
        avg_symbol_density,
        hot_files,
        active_gotchas,
        import_graph_health,
        score,
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
    fn test_empty_db_knowledge() {
        let conn = setup_db();
        let report = analyze_knowledge(&conn);
        assert_eq!(report.total_files, 0);
        assert!(report.language_distribution.is_empty());
        assert_eq!(
            report.score, 0.4,
            "empty DB: 0 files, 0 density → 0.3*0 + 0.3*0 + 0.2*1 + 0.2*1 = 0.4"
        );
    }

    #[test]
    fn test_knowledge_with_files() {
        let conn = setup_db();
        conn.execute_batch(
            "INSERT INTO file_knowledge (file_path, language, line_count, symbol_count, content_hash) \
             VALUES ('a.rs', 'rust', 100, 10, 'h1');
             INSERT INTO file_knowledge (file_path, language, line_count, symbol_count, content_hash) \
             VALUES ('b.py', 'python', 50, 5, 'h2');
             INSERT INTO file_knowledge (file_path, language, line_count, symbol_count, content_hash) \
             VALUES ('c.rs', 'rust', 200, 20, 'h3');",
        ).expect("insert");
        let report = analyze_knowledge(&conn);
        assert_eq!(report.total_files, 3);
        assert_eq!(*report.language_distribution.get("rust").unwrap_or(&0), 2);
        assert_eq!(*report.language_distribution.get("python").unwrap_or(&0), 1);
        assert!(report.avg_line_count > 0.0);
        // density = 35 / 350 = 0.1 → in [0.01, 0.5] → density_ok=1.0
        assert!(
            report.score > 0.5,
            "indexed files should score well: {}",
            report.score
        );
    }

    #[test]
    fn test_knowledge_hot_files_detected() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO file_knowledge (file_path, language, line_count, symbol_count, content_hash) \
             VALUES ('hot.rs', 'rust', 100, 10, 'h1')",
            [],
        ).expect("insert file");
        // 4 edits in last 7 days → hot
        for _ in 0..4 {
            conn.execute(
                "INSERT INTO edit_history (file_path, edit_type, edited_at) \
                 VALUES ('hot.rs', 'edit', datetime('now'))",
                [],
            )
            .expect("insert edit");
        }
        let report = analyze_knowledge(&conn);
        assert_eq!(report.hot_files, 1);
    }

    #[test]
    fn test_knowledge_active_gotchas() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO file_knowledge (file_path, language, line_count, symbol_count, content_hash) \
             VALUES ('a.rs', 'rust', 100, 10, 'h1')",
            [],
        ).expect("insert file");
        conn.execute(
            "INSERT INTO gotchas (pattern, gotcha, severity, decay_score) \
             VALUES ('test_pattern', 'test gotcha', 'high', 0.9)",
            [],
        )
        .expect("insert gotcha");
        let report = analyze_knowledge(&conn);
        assert_eq!(report.active_gotchas, 1);
        // gotcha_rate = 1/1 = 1.0 → penalty = 0.2
        assert!(report.score < 1.0);
    }

    #[test]
    fn test_knowledge_import_graph_health() {
        let conn = setup_db();
        conn.execute_batch(
            "INSERT INTO file_knowledge (file_path, language, line_count, symbol_count, content_hash) \
             VALUES ('a.rs', 'rust', 100, 10, 'h1');
             INSERT INTO file_knowledge (file_path, language, line_count, symbol_count, content_hash) \
             VALUES ('b.rs', 'rust', 100, 10, 'h2');
             INSERT INTO file_relations (source_path, target_path, relation_type) \
             VALUES ('a.rs', 'b.rs', 'imports');",
        ).expect("insert");
        let report = analyze_knowledge(&conn);
        // 1 edge / (2 files + 1) = 0.333
        assert!(
            report.import_graph_health > 0.3 && report.import_graph_health < 0.4,
            "import_graph_health: {}",
            report.import_graph_health
        );
    }

    #[test]
    fn test_knowledge_score_bounded() {
        let conn = setup_db();
        let report = analyze_knowledge(&conn);
        assert!(report.score >= 0.0 && report.score <= 1.0);
    }
}
