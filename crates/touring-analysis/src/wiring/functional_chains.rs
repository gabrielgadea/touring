//! Functional chain analysis — detect broken and healthy chains.

use touring_foundation::schema_guard;

/// Functional chain analysis result.
#[derive(Debug, Clone)]
pub struct ChainResult {
    /// Total chains detected.
    pub total_chains: usize,
    /// Broken chains (low confidence or missing endpoints).
    pub broken_count: usize,
    /// Chain types breakdown: (chain_type, count).
    pub by_type: Vec<(String, usize)>,
    /// Average confidence across all chains.
    pub avg_confidence: f64,
}

/// Analyze functional chains from the knowledge DB.
pub fn analyze_chains(conn: &rusqlite::Connection) -> ChainResult {
    let table = schema_guard::TABLE_FUNCTIONAL_CHAINS;

    // Total chains
    let total_chains: i64 = conn
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap_or(0);

    if total_chains <= 0 {
        return ChainResult {
            total_chains: 0,
            broken_count: 0,
            by_type: vec![],
            avg_confidence: 0.0,
        };
    }

    // Broken chains: confidence < 0.3 or chain_type = 'Broken'
    let broken_count: i64 = conn
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM {table} WHERE confidence < 0.3 OR chain_type = 'Broken'"
            ),
            [],
            |r| r.get(0),
        )
        .unwrap_or(0);

    // Average confidence
    let avg_confidence: f64 = conn
        .query_row(&format!("SELECT AVG(confidence) FROM {table}"), [], |r| {
            r.get(0)
        })
        .unwrap_or(0.0);

    // Breakdown by chain type
    let mut by_type = Vec::new();
    if let Ok(mut stmt) = conn.prepare(&format!(
        "SELECT chain_type, COUNT(*) FROM {table} GROUP BY chain_type ORDER BY COUNT(*) DESC"
    )) {
        if let Ok(rows) = stmt.query_map([], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, i64>(1)? as usize))
        }) {
            for row in rows.flatten() {
                by_type.push(row);
            }
        }
    }

    ChainResult {
        total_chains: total_chains as usize,
        broken_count: broken_count as usize,
        by_type,
        avg_confidence,
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
    fn test_no_chains() {
        let conn = setup_db();
        let result = analyze_chains(&conn);
        assert_eq!(result.total_chains, 0);
        assert_eq!(result.broken_count, 0);
    }

    #[test]
    fn test_healthy_chain() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO functional_chains (source_module, source_symbol, sink_module, sink_symbol, chain_type, confidence) \
             VALUES ('a.rs', 'process', 'b.rs', 'validate', 'Sequential', 0.9)",
            [],
        ).expect("insert");
        let result = analyze_chains(&conn);
        assert_eq!(result.total_chains, 1);
        assert_eq!(result.broken_count, 0);
        assert!(result.avg_confidence > 0.8);
    }

    #[test]
    fn test_broken_chain() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO functional_chains (source_module, source_symbol, sink_module, sink_symbol, chain_type, confidence) \
             VALUES ('a.rs', 'process', 'b.rs', 'validate', 'Broken', 0.1)",
            [],
        ).expect("insert");
        let result = analyze_chains(&conn);
        assert_eq!(result.total_chains, 1);
        assert_eq!(result.broken_count, 1);
    }

    #[test]
    fn test_chain_type_breakdown() {
        let conn = setup_db();
        conn.execute_batch(
            "INSERT INTO functional_chains (source_module, source_symbol, sink_module, sink_symbol, chain_type, confidence) \
             VALUES ('a.rs', 'f1', 'b.rs', 'f2', 'Sequential', 0.8);
             INSERT INTO functional_chains (source_module, source_symbol, sink_module, sink_symbol, chain_type, confidence) \
             VALUES ('c.rs', 'f3', 'd.rs', 'f4', 'Sequential', 0.7);
             INSERT INTO functional_chains (source_module, source_symbol, sink_module, sink_symbol, chain_type, confidence) \
             VALUES ('e.rs', 'f5', 'f.rs', 'f6', 'Hierarchical', 0.9);",
        ).expect("insert");
        let result = analyze_chains(&conn);
        assert_eq!(result.total_chains, 3);
        assert_eq!(result.by_type.len(), 2);
    }
}
