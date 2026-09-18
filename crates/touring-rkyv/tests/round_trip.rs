//! Round-trip serialization tests for touring-rkyv templates.
//!
//! RKYV-4b: Verifies every Archived template (the 5 with a production consumer
//! since 18/09/2026; the 8 only this file used were removed) can:
//! 1. Serialize via `touring_rkyv::to_bytes`
//! 2. Deserialize to owned type via `touring_rkyv::deserialize`
//!
//! These tests verify the core serialize→deserialize pipeline. They go through
//! the FAÇADE on purpose (`touring_rkyv::*`, never `rkyv::*` directly): since the
//! 0.7→0.8 migration the façade's adapters are what preserve the old call shape,
//! so routing the round-trip through them makes this suite the regression guard
//! for the adapters themselves — a test that bypassed them would keep passing
//! while every consumer broke.

use touring_rkyv::templates::*;

/// Verify: serialize + deserialize all succeed for a type.
macro_rules! round_trip_test {
    ($name:ident, $type:ty, $value:expr_2021) => {
        #[test]
        fn $name() {
            let original: $type = $value;
            // Serialize
            let bytes = touring_rkyv::to_bytes::<$type, 8192>(&original).unwrap();
            // Zero-copy access — safe here because the bytes were just produced
            // by `to_bytes` above, which is exactly the contract `archived_root`
            // documents.
            let archived = unsafe { touring_rkyv::archived_root::<$type>(&bytes) };
            let _deserialized: $type = touring_rkyv::deserialize::<$type>(archived).unwrap();
        }
    };
}

// ── Hook Event Templates ──────────────────────────────────────────────────────

round_trip_test!(
    test_archived_event_record_round_trip,
    ArchivedEventRecord,
    ArchivedEventRecord {
        session_id: "sess_abc123".to_string(),
        tool_name: "grep".to_string(),
        outcome: "success".to_string(),
        latency_ms: 42,
        cila_level: 3,
        timestamp_ms: 9876543210,
    }
);

round_trip_test!(
    test_archived_event_record_error_outcome,
    ArchivedEventRecord,
    ArchivedEventRecord {
        session_id: "sess_error".to_string(),
        tool_name: "bash".to_string(),
        outcome: "error: segfault".to_string(),
        latency_ms: 5000,
        cila_level: 5,
        timestamp_ms: 1111111111,
    }
);

// ── Symbol & Index Templates ─────────────────────────────────────────────────

round_trip_test!(
    test_archived_index_snapshot_round_trip,
    ArchivedIndexSnapshot,
    ArchivedIndexSnapshot {
        edges: vec![
            (
                "touring_hooks::pre_edit".to_string(),
                "touring_hooks::HookRuntime::process".to_string()
            ),
            (
                "touring_ast::find".to_string(),
                "touring_index::SymbolStore::get".to_string()
            ),
        ],
        node_count: 150,
        schema_version: 1,
    }
);

round_trip_test!(
    test_archived_index_snapshot_empty,
    ArchivedIndexSnapshot,
    ArchivedIndexSnapshot {
        edges: vec![],
        node_count: 0,
        schema_version: 1,
    }
);

// ── RL Learning Templates ─────────────────────────────────────────────────────

// ── CRDT Graph Templates ───────────────────────────────────────────────────

round_trip_test!(
    test_archived_crdt_edge_round_trip,
    ArchivedCrdtEdge,
    ArchivedCrdtEdge {
        from: 100,
        to: 200,
        label: "calls".to_string(),
    }
);

round_trip_test!(
    test_archived_node_weight_round_trip,
    ArchivedNodeWeight,
    ArchivedNodeWeight {
        label: "pre_edit".to_string(),
        score: 0.95,
        updated_at: 1234567890,
    }
);

round_trip_test!(
    test_archived_graph_snapshot_round_trip,
    ArchivedGraphSnapshot,
    ArchivedGraphSnapshot {
        nodes: vec![1, 2, 3, 4, 5],
        edges: vec![
            ArchivedCrdtEdge {
                from: 1,
                to: 2,
                label: "calls".to_string(),
            },
            ArchivedCrdtEdge {
                from: 2,
                to: 3,
                label: "imports".to_string(),
            },
            ArchivedCrdtEdge {
                from: 3,
                to: 4,
                label: "uses".to_string(),
            },
        ],
        weights: vec![
            (
                1,
                ArchivedNodeWeight {
                    label: "HookRuntime".to_string(),
                    score: 0.9,
                    updated_at: 1000,
                },
            ),
            (
                2,
                ArchivedNodeWeight {
                    label: "process_hook".to_string(),
                    score: 0.8,
                    updated_at: 2000,
                },
            ),
        ],
    }
);

round_trip_test!(
    test_archived_graph_snapshot_empty,
    ArchivedGraphSnapshot,
    ArchivedGraphSnapshot {
        nodes: vec![],
        edges: vec![],
        weights: vec![],
    }
);

// ── Cognitive / GoT Templates ───────────────────────────────────────────────
