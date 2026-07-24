//! THSF Phase 5 Wave I — E2E test for the grow-only SQLite audit trail.
//!
//! Single consolidated test because the audit-trail singleton
//! (`OnceLock<Mutex<Option<Connection>>>`) is process-wide: we can't
//! point it at a different DB per test, and once its host `TempDir`
//! drops, follow-up inserts fail silently. Consolidation avoids this
//! race by (a) keeping the tempdir alive for the whole test binary via
//! a `OnceLock<TempDir>`, (b) setting the env override BEFORE the
//! first singleton touch, and (c) exercising all assertions linearly
//! inside one `#[test]`.

use std::path::PathBuf;
use std::sync::OnceLock;

use tempfile::TempDir;
use touring_foundation::{DeltaOutcome, HealthDeltaEvent};
use touring_hooks::health_delta_audit::{
    self, HistoryQuery, count_events, query_history, record_event,
};

/// Process-wide tempdir whose lifetime covers every `record_event` call.
/// Initialized by [`init_once`] on first access.
static TEST_TMP: OnceLock<TempDir> = OnceLock::new();

fn init_once() {
    TEST_TMP.get_or_init(|| {
        let dir = tempfile::tempdir().expect("tempdir");
        let db = dir.path().join("audit.db");
        // SAFETY: set BEFORE the audit singleton is touched. The
        // singleton reads this env var exactly once, so a racing
        // setter is not a concern here (single-threaded test body).
        unsafe {
            std::env::set_var("TOURING_HEALTH_AUDIT_DB", &db);
        }
        dir
    });
}

fn sample(path: &str, delta: f32, ts: u64, outcome: DeltaOutcome) -> HealthDeltaEvent {
    HealthDeltaEvent {
        file_path: path.to_string(),
        old_health: 0.8,
        new_health: 0.8 + delta,
        delta,
        outcome,
        regression_streak: if delta < 0.0 { 1 } else { 0 },
        improvement_streak: if delta > 0.0 { 1 } else { 0 },
        timestamp_ms: ts,
    }
}

#[test]
fn wave_i_full_pipeline() {
    init_once();

    // ---- env override wired
    let env_value = std::env::var("TOURING_HEALTH_AUDIT_DB").expect("env set");
    let resolved = health_delta_audit::resolve_db_path();
    assert_eq!(
        resolved,
        PathBuf::from(&env_value),
        "resolve_db_path must honor env override",
    );

    // ---- baseline: may be non-zero if a previous invocation of the
    //      singleton left rows in the persistent file. Record it and
    //      assert relative increments below.
    let baseline = count_events();

    // ---- write path: three diverse events
    let e1 = sample("/src/a.rs", 0.10, 10_000, DeltaOutcome::Improvement);
    let e2 = sample("/src/b.rs", -0.15, 20_000, DeltaOutcome::Regression);
    let e3 = sample("/docs/z.md", 0.02, 30_000, DeltaOutcome::Neutral);

    assert!(record_event(&e1), "record e1");
    assert!(record_event(&e2), "record e2");
    assert!(record_event(&e3), "record e3");

    let after = count_events();
    assert_eq!(after, baseline + 3, "grow-only: +3 rows after 3 inserts");

    // ---- filter: path_prefix
    let src_only = query_history(&HistoryQuery {
        path_prefix: Some("/src/".to_string()),
        ..Default::default()
    })
    .expect("query /src/");
    assert!(
        src_only.iter().all(|e| e.file_path.starts_with("/src/")),
        "path_prefix must constrain results"
    );
    assert!(
        src_only.iter().any(|e| e.file_path == "/src/a.rs"),
        "a.rs should be present"
    );
    assert!(
        src_only.iter().any(|e| e.file_path == "/src/b.rs"),
        "b.rs should be present"
    );

    // ---- filter: since_ms (only e3 + any prior events newer than 25_000)
    let recent = query_history(&HistoryQuery {
        since_ms: Some(25_000),
        ..Default::default()
    })
    .expect("query since");
    assert!(
        recent.iter().all(|ev| ev.timestamp_ms >= 25_000),
        "all rows must be >= 25_000"
    );
    assert!(
        recent.iter().any(|ev| ev.file_path == "/docs/z.md"),
        "e3 (/docs/z.md @ 30_000) must satisfy the since filter"
    );

    // ---- filter: limit
    let limited = query_history(&HistoryQuery {
        limit: Some(1),
        ..Default::default()
    })
    .expect("query limit");
    assert_eq!(limited.len(), 1, "limit=1 must return exactly one row");

    // ---- outcome round-trip (regression event must preserve its tag)
    let all = query_history(&HistoryQuery::default()).expect("query all");
    let reg = all
        .iter()
        .find(|ev| ev.file_path == "/src/b.rs")
        .expect("b.rs present");
    assert_eq!(reg.outcome, DeltaOutcome::Regression);
    assert!((reg.delta - (-0.15)).abs() < 1e-6);

    // ---- CLI envelope shape (mirrors what cli_health_delta_history returns)
    let events = query_history(&HistoryQuery::default()).expect("query all");
    let envelope = serde_json::json!({
        "count": events.len(),
        "db_row_count": count_events(),
        "events": events.iter().map(|ev| serde_json::json!({
            "file_path": ev.file_path,
            "outcome": match ev.outcome {
                DeltaOutcome::Improvement => "improvement",
                DeltaOutcome::Regression => "regression",
                DeltaOutcome::Neutral => "neutral",
            },
        })).collect::<Vec<_>>(),
    })
    .to_string();
    assert!(envelope.contains("/src/a.rs"));
    assert!(envelope.contains("\"improvement\""));
    assert!(envelope.contains("\"regression\""));

    let parsed: serde_json::Value =
        serde_json::from_str(&envelope).expect("envelope is valid JSON");
    assert!(
        parsed.get("count").and_then(|v| v.as_u64()).unwrap_or(0) >= 3,
        "count must reflect at least our 3 inserts"
    );
    assert!(parsed.get("events").and_then(|v| v.as_array()).is_some());
}
