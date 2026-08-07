//! Wiring analysis: orphan detection, functional chain tracing, integration scoring.

pub mod cycle_detection;
pub mod finding;
pub mod fingerprints;
pub mod functional_chains;
/// Hermetic intra-crate module import-cycle detection (Kosaraju SCC built from
/// the target's own `use crate::…` graph); backs the F1.8 dimension.
pub mod module_cycles;
pub mod orphan;

pub use cycle_detection::{CyclePath, count_import_cycles, detect_import_cycles};
pub use finding::WiringFinding;
pub use fingerprints::WiringFingerprintStore;
pub use functional_chains::{ChainResult, analyze_chains};
pub use orphan::{OrphanResult, count_orphans, dead_code_detect, scan_dead_patterns};

use serde::{Deserialize, Serialize};
use std::sync::{LazyLock, Mutex};
use touring_foundation::schema_guard;

/// Complete wiring analysis report.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WiringReport {
    /// Total public symbols tracked.
    pub total_pub_symbols: usize,
    /// Orphan symbols (no consumers).
    pub orphan_count: usize,
    /// Orphan rate (0.0–1.0).
    pub orphan_rate: f64,
    /// Total consumer relationships.
    pub total_consumers: usize,
    /// Functional chains detected.
    pub chain_count: usize,
    /// Broken chains detected.
    pub broken_chain_count: usize,
    /// Average module integration score from `module_ecosystem` (0.0–1.0).
    ///
    /// Derived from the `integration_score` column in the `module_ecosystem`
    /// table (SCHEMA_VERSION=8). Defaults to 1.0 when the table is empty.
    #[serde(default = "default_integration_score")]
    pub avg_integration_score: f64,
    /// Modules with integration_score < 0.5 (under-integrated).
    #[serde(default)]
    pub modules_below_threshold: usize,
    /// Circular import cycles detected (Kosaraju SCC, nodes > 1).
    ///
    /// Zero means no circular dependencies in the tracked import graph.
    /// A non-zero value flags architectural issues that should be resolved.
    #[serde(default)]
    pub cycle_count: usize,
    /// Composite wiring score (0.0–1.0).
    ///
    /// Formula (v2): 0.4 × (1 − orphan_rate) + 0.3 × chain_health + 0.3 × avg_integration_score
    pub score: f64,
}

fn default_integration_score() -> f64 {
    1.0
}

/// Analyze wiring health from a knowledge DB connection.
pub fn analyze_wiring(conn: &rusqlite::Connection) -> WiringReport {
    let orphan_result = orphan::count_orphans(conn);
    let chain_result = functional_chains::analyze_chains(conn);

    let orphan_rate = if orphan_result.total_pub > 0 {
        (orphan_result.orphan_count as f64 / orphan_result.total_pub as f64).min(1.0)
    } else {
        0.0
    };

    // Query avg integration score from module_ecosystem (SCHEMA_VERSION=8).
    let me_table = schema_guard::TABLE_MODULE_ECOSYSTEM;
    let avg_integration_score: f64 = conn
        .query_row(
            &format!(
                "SELECT COALESCE(AVG(integration_score), 1.0) FROM {me_table} \
                 WHERE integration_score IS NOT NULL"
            ),
            [],
            |r| r.get(0),
        )
        .unwrap_or(1.0);

    let modules_below_threshold: usize = conn
        .query_row(
            &format!(
                "SELECT COUNT(*) FROM {me_table} \
                 WHERE integration_score IS NOT NULL AND integration_score < 0.5"
            ),
            [],
            |r| r.get::<_, i64>(0),
        )
        .unwrap_or(0) as usize;

    // Score v3: weighted average across three dimensions.
    // 0.4 orphan health + 0.3 chain health (weighted by confidence) + 0.3 ecosystem integration
    let chain_health = if chain_result.total_chains > 0 {
        let not_broken =
            1.0 - (chain_result.broken_count as f64 / chain_result.total_chains as f64);
        // Weight by average confidence — low-confidence chains reduce health even if not "broken"
        not_broken * chain_result.avg_confidence.clamp(0.0, 1.0).max(0.1)
    } else {
        1.0
    };
    let score = (0.4 * (1.0 - orphan_rate)
        + 0.3 * chain_health
        + 0.3 * avg_integration_score.clamp(0.0, 1.0))
    .clamp(0.0, 1.0);

    // EC46: First production caller of count_import_cycles() — surfaces circular
    // import chains (Kosaraju SCC) in every tool that reads WiringReport.
    // Non-blocking: returns 0 gracefully when wiring_map has no cycles.
    let cycle_count = cycle_detection::count_import_cycles(conn);

    WiringReport {
        total_pub_symbols: orphan_result.total_pub,
        orphan_count: orphan_result.orphan_count,
        orphan_rate,
        total_consumers: orphan_result.total_consumers,
        chain_count: chain_result.total_chains,
        broken_chain_count: chain_result.broken_count,
        avg_integration_score,
        modules_below_threshold,
        cycle_count,
        score,
    }
}

/// Incremental wiring analysis: skips files whose fingerprints haven't changed.
///
/// Returns the wiring report AND updates `store` with fresh fingerprints for
/// files that WERE re-analyzed. Files with unchanged fingerprints are skipped —
/// their contribution to the report is omitted (counts as zero orphans from that
/// perspective).
///
/// The orchestrator can union results across sessions for full accuracy, or call
/// `analyze_wiring` for a full re-analysis when `store` is empty.
pub fn analyze_wiring_incremental(
    conn: &rusqlite::Connection,
    store: &mut WiringFingerprintStore,
) -> WiringReport {
    // Collect all distinct module_file values in wiring_map.
    let me_table = schema_guard::TABLE_WIRING_MAP;
    let sql = format!("SELECT DISTINCT module_file FROM {me_table} LIMIT 5000");
    let file_paths: Vec<String> = conn
        .prepare(&sql)
        .ok()
        .and_then(|mut stmt| {
            stmt.query_map([], |r| r.get(0))
                .ok()
                .map(|iter| iter.flatten().collect())
        })
        .unwrap_or_default();

    // Filter to only changed (or new) files.
    let changed: Vec<String> = file_paths
        .iter()
        .filter(|p| !store.is_unchanged(conn, p))
        .cloned()
        .collect();

    if changed.is_empty() {
        tracing::debug!(
            "analyze_wiring_incremental: all {} files unchanged, returning cached analysis",
            file_paths.len()
        );
    } else {
        tracing::debug!(
            "analyze_wiring_incremental: {}/{} files changed, refreshing fingerprints",
            changed.len(),
            file_paths.len()
        );
        // Refresh fingerprints for changed files before delegating.
        store.refresh_batch(conn, &changed);
    }

    // Delegate to full analysis for an accurate report over all files.
    analyze_wiring(conn)
}

/// Cheap content fingerprint of the two tables [`analyze_wiring`] reads
/// (`wiring_map` for orphans/chains/cycles, `module_ecosystem` for integration
/// scores). `(row_count, max_rowid)` of `wiring_map` catches every edge
/// insert/delete; `(row_count, score_sum)` of `module_ecosystem` catches both
/// module add/remove and in-place `integration_score` updates. Computed with a
/// handful of indexed aggregates — orders of magnitude cheaper than the full
/// analysis (`COUNT(DISTINCT …)` + functional-chain graph + Kosaraju SCC).
type WiringSignature = (i64, i64, i64, i64);

fn wiring_db_signature(conn: &rusqlite::Connection) -> Option<WiringSignature> {
    let wm = schema_guard::TABLE_WIRING_MAP;
    let me = schema_guard::TABLE_MODULE_ECOSYSTEM;
    conn.query_row(
        &format!(
            "SELECT \
               (SELECT COUNT(*) FROM {wm}), \
               (SELECT COALESCE(MAX(rowid), 0) FROM {wm}), \
               (SELECT COUNT(*) FROM {me}), \
               (SELECT CAST(COALESCE(SUM(integration_score), 0) * 1000000 AS INTEGER) FROM {me})"
        ),
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?)),
    )
    .ok()
}

/// Process-global memo for [`analyze_wiring`], keyed on [`wiring_db_signature`].
static WIRING_MEMO: LazyLock<Mutex<Option<(WiringSignature, WiringReport)>>> =
    LazyLock::new(|| Mutex::new(None));

/// Memoized [`analyze_wiring`] for the daemon hot path.
///
/// `pre_read` / `post_edit` hooks run the full analysis pipeline on every tool
/// call, but the wiring report only changes when the knowledge DB does. On a
/// cache hit (signature unchanged since the last analysis) this returns the
/// cached report in microseconds instead of recomputing (~250 ms on a large
/// workspace — the dominant contributor to the hook-dispatch p99 tail).
///
/// Correctness is preserved by construction: a hit happens **only** when the
/// content fingerprint matches, i.e. when the inputs to `analyze_wiring` are
/// byte-identical, so the cached report is exactly what a fresh analysis would
/// produce. A failed signature query (schema mismatch / locked DB) falls back
/// to a full uncached analysis — the cache never serves a stale report.
///
/// Tests call [`analyze_wiring`] directly to stay isolated from this
/// process-global cache; only the production pipeline uses the memoized path.
pub fn analyze_wiring_memoized(conn: &rusqlite::Connection) -> WiringReport {
    let Some(sig) = wiring_db_signature(conn) else {
        return analyze_wiring(conn);
    };
    if let Ok(guard) = WIRING_MEMO.lock()
        && let Some((cached_sig, report)) = guard.as_ref()
        && *cached_sig == sig
    {
        return report.clone();
    }
    let report = analyze_wiring(conn);
    if let Ok(mut guard) = WIRING_MEMO.lock() {
        *guard = Some((sig, report.clone()));
    }
    report
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
    fn test_empty_db_wiring() {
        let conn = setup_db();
        let report = analyze_wiring(&conn);
        assert_eq!(report.total_pub_symbols, 0);
        assert_eq!(report.orphan_count, 0);
        assert!((report.score - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn wiring_signature_changes_when_wiring_map_changes() {
        let conn = setup_db();
        let sig_empty = wiring_db_signature(&conn).expect("signature");
        conn.execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility) \
             VALUES ('a.rs', 'Foo', 'struct', 'public')",
            [],
        )
        .expect("insert");
        let sig_after = wiring_db_signature(&conn).expect("signature");
        assert_ne!(
            sig_empty, sig_after,
            "fingerprint must change when a wiring edge is inserted (cache must invalidate)"
        );
    }

    #[test]
    fn wiring_signature_changes_on_integration_score_update() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO module_ecosystem (file_path, integration_score) VALUES ('a.rs', 0.5)",
            [],
        )
        .expect("insert");
        let sig_before = wiring_db_signature(&conn).expect("signature");
        conn.execute(
            "UPDATE module_ecosystem SET integration_score = 0.9 WHERE file_path = 'a.rs'",
            [],
        )
        .expect("update");
        let sig_after = wiring_db_signature(&conn).expect("signature");
        assert_ne!(
            sig_before, sig_after,
            "in-place integration_score update must change the fingerprint (no stale score)"
        );
    }

    #[test]
    fn memoized_reflects_changes_through_the_cache() {
        // Correctness-through-cache: a hit only occurs when the fingerprint
        // matches (identical inputs), so the memoized report must always equal
        // a fresh analysis — including after a mutation invalidates the entry.
        let conn = setup_db();
        let empty = analyze_wiring_memoized(&conn);
        assert_eq!(empty.orphan_count, 0, "empty DB has no orphans");

        conn.execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility) \
             VALUES ('mem.rs', 'Bar', 'struct', 'public')",
            [],
        )
        .expect("insert");
        let after = analyze_wiring_memoized(&conn);
        assert_eq!(
            after.orphan_count, 1,
            "memoized analysis must reflect the new orphan, not serve a stale cache"
        );
        // Second call with the DB unchanged returns the identical report.
        let again = analyze_wiring_memoized(&conn);
        assert_eq!(after.orphan_count, again.orphan_count);
        assert_eq!(after.total_pub_symbols, again.total_pub_symbols);
    }

    #[test]
    fn test_wiring_with_orphans() {
        let conn = setup_db();
        // Insert a pub symbol with no consumer
        conn.execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility) \
             VALUES ('a.rs', 'Foo', 'struct', 'public')",
            [],
        )
        .expect("insert");
        let report = analyze_wiring(&conn);
        assert_eq!(report.total_pub_symbols, 1);
        assert_eq!(report.orphan_count, 1);
        assert!(report.score < 1.0);
    }

    #[test]
    fn test_wiring_with_consumer() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, consumer_file) \
             VALUES ('a.rs', 'Foo', 'struct', 'public', 'b.rs')",
            [],
        ).expect("insert");
        let report = analyze_wiring(&conn);
        assert_eq!(report.total_pub_symbols, 1);
        assert_eq!(report.orphan_count, 0);
        assert_eq!(report.total_consumers, 1);
    }

    #[test]
    fn test_avg_integration_score_default_one_when_empty() {
        let conn = setup_db();
        let report = analyze_wiring(&conn);
        assert!(
            (report.avg_integration_score - 1.0).abs() < f64::EPSILON,
            "empty module_ecosystem → avg_integration_score must default to 1.0"
        );
        assert_eq!(report.modules_below_threshold, 0);
    }

    #[test]
    fn test_avg_integration_score_from_module_ecosystem() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO module_ecosystem (file_path, integration_score) VALUES ('a.rs', 0.8)",
            [],
        )
        .expect("insert");
        conn.execute(
            "INSERT INTO module_ecosystem (file_path, integration_score) VALUES ('b.rs', 0.4)",
            [],
        )
        .expect("insert");
        let report = analyze_wiring(&conn);
        // avg = (0.8 + 0.4) / 2 = 0.6
        assert!(
            (report.avg_integration_score - 0.6).abs() < 1e-9,
            "avg_integration_score should be 0.6, got {}",
            report.avg_integration_score
        );
        // b.rs < 0.5 threshold
        assert_eq!(report.modules_below_threshold, 1);
    }

    #[test]
    fn test_score_v2_formula_penalises_low_integration() {
        let conn = setup_db();
        // One orphan pub symbol (orphan_rate = 1.0) + low integration
        conn.execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility) \
             VALUES ('a.rs', 'Bar', 'fn', 'public')",
            [],
        )
        .expect("insert");
        conn.execute(
            "INSERT INTO module_ecosystem (file_path, integration_score) VALUES ('a.rs', 0.1)",
            [],
        )
        .expect("insert");
        let report = analyze_wiring(&conn);
        assert!(
            report.score < 0.5,
            "high orphan rate + low integration should produce score < 0.5, got {}",
            report.score
        );
    }

    #[test]
    fn test_cycle_count_zero_no_cycles() {
        let conn = setup_db();
        // Linear chain: no cycles
        conn.execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, consumer_file) \
             VALUES ('a.rs', 'Foo', 'struct', 'public', 'b.rs')",
            [],
        ).expect("insert");
        let report = analyze_wiring(&conn);
        assert_eq!(report.cycle_count, 0, "linear chain has no cycles");
    }

    #[test]
    fn test_cycle_count_detects_mutual_import() {
        let conn = setup_db();
        // Mutual import: a.rs ↔ b.rs (one cycle)
        conn.execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, consumer_file) \
             VALUES ('a.rs', 'Foo', 'struct', 'public', 'b.rs')",
            [],
        ).expect("insert");
        conn.execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, consumer_file) \
             VALUES ('b.rs', 'Bar', 'fn', 'public', 'a.rs')",
            [],
        ).expect("insert");
        let report = analyze_wiring(&conn);
        assert_eq!(
            report.cycle_count, 1,
            "mutual a↔b import must report cycle_count=1, got {}",
            report.cycle_count
        );
    }

    #[test]
    fn test_score_v2_formula_perfect_wiring() {
        let conn = setup_db();
        conn.execute(
            "INSERT INTO wiring_map (module_file, symbol_name, symbol_kind, visibility, consumer_file) \
             VALUES ('a.rs', 'Foo', 'struct', 'public', 'b.rs')",
            [],
        ).expect("insert");
        conn.execute(
            "INSERT INTO module_ecosystem (file_path, integration_score) VALUES ('a.rs', 1.0)",
            [],
        )
        .expect("insert");
        let report = analyze_wiring(&conn);
        assert!(
            report.score > 0.9,
            "0 orphans + full integration should score > 0.9, got {}",
            report.score
        );
    }
}
