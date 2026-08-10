//! Round-trip serialization tests for touring-rkyv templates.
//!
//! RKYV-4b: Verifies all 13 Archived types can:
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
    test_archived_hook_event_round_trip,
    ArchivedHookEvent,
    ArchivedHookEvent {
        hook_name: "pre_edit".to_string(),
        timestamp_ms: 1234567890,
        payload: vec![1, 2, 3, 4, 5],
        cila_level: 5,
    }
);

round_trip_test!(
    test_archived_hook_event_empty_payload,
    ArchivedHookEvent,
    ArchivedHookEvent {
        hook_name: "post_write".to_string(),
        timestamp_ms: 0,
        payload: vec![],
        cila_level: 0,
    }
);

round_trip_test!(
    test_archived_hook_event_large_payload,
    ArchivedHookEvent,
    ArchivedHookEvent {
        hook_name: "pre_read".to_string(),
        timestamp_ms: u64::MAX,
        payload: (0..256).map(|i| i as u8).collect(),
        cila_level: 7,
    }
);

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
    test_archived_symbol_round_trip,
    ArchivedSymbol,
    ArchivedSymbol {
        name: "process_hook_event".to_string(),
        module_path: "touring_hooks::runtime::HookRuntime".to_string(),
        line: 42,
    }
);

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

round_trip_test!(
    test_archived_learning_params_snapshot_round_trip,
    ArchivedLearningParamsSnapshot,
    ArchivedLearningParamsSnapshot {
        alpha: 0.1,
        gamma: 0.9,
        lambda: 0.8,
        initial_q: 0.0,
        epsilon: 0.5,
        epsilon_decay: 0.99,
        epsilon_min: 0.01,
    }
);

round_trip_test!(
    test_archived_learning_params_edge_cases,
    ArchivedLearningParamsSnapshot,
    ArchivedLearningParamsSnapshot {
        alpha: 1.0,
        gamma: 1.0,
        lambda: 1.0,
        initial_q: f64::MAX,
        epsilon: 1.0,
        epsilon_decay: 1.0,
        epsilon_min: f64::MIN_POSITIVE,
    }
);

round_trip_test!(
    test_archived_qtable_snapshot_round_trip,
    ArchivedQTableSnapshot,
    ArchivedQTableSnapshot {
        q_values: vec![(0, 0, 0.5), (0, 1, 0.3), (1, 0, 0.8), (1, 1, 0.2)],
        params: ArchivedLearningParamsSnapshot {
            alpha: 0.1,
            gamma: 0.9,
            lambda: 0.8,
            initial_q: 0.0,
            epsilon: 0.5,
            epsilon_decay: 0.99,
            epsilon_min: 0.01,
        },
        revision: 42,
        granular_update_count: 100,
        reward_sums: [0.1, 0.2, 0.3, 0.4, 0.5],
    }
);

round_trip_test!(
    test_archived_linucb_arm_snapshot_round_trip,
    ArchivedLinUCBArmSnapshot,
    ArchivedLinUCBArmSnapshot {
        a_inv_flat: vec![1.0, 0.0, 0.0, 1.0],
        b: vec![0.5, 0.3],
        pulls: 10,
        cumulative_reward: 5.5,
    }
);

round_trip_test!(
    test_archived_linucb_snapshot_round_trip,
    ArchivedLinUCBSnapshot,
    ArchivedLinUCBSnapshot {
        arms: vec![
            ArchivedLinUCBArmSnapshot {
                a_inv_flat: vec![1.0, 0.0, 0.0, 1.0],
                b: vec![0.5, 0.3],
                pulls: 10,
                cumulative_reward: 5.5,
            },
            ArchivedLinUCBArmSnapshot {
                a_inv_flat: vec![2.0, 1.0, 1.0, 3.0],
                b: vec![0.8, 0.6],
                pulls: 20,
                cumulative_reward: 12.0,
            },
        ],
        alpha: 1.5,
        d: 2,
    }
);

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

round_trip_test!(
    test_archived_got_node_snapshot_round_trip,
    ArchivedGotNodeSnapshot,
    ArchivedGotNodeSnapshot {
        id: 1,
        label: "think".to_string(),
        weight: 0.75,
        child_ids: vec![2, 3, 4],
    }
);

round_trip_test!(
    test_archived_got_node_snapshot_no_children,
    ArchivedGotNodeSnapshot,
    ArchivedGotNodeSnapshot {
        id: 99,
        label: "leaf_node".to_string(),
        weight: 0.1,
        child_ids: vec![],
    }
);

round_trip_test!(
    test_archived_got_snapshot_round_trip,
    ArchivedGoTSnapshot,
    ArchivedGoTSnapshot {
        nodes: vec![
            ArchivedGotNodeSnapshot {
                id: 1,
                label: "root".to_string(),
                weight: 1.0,
                child_ids: vec![2, 3],
            },
            ArchivedGotNodeSnapshot {
                id: 2,
                label: "branch_a".to_string(),
                weight: 0.6,
                child_ids: vec![4],
            },
            ArchivedGotNodeSnapshot {
                id: 3,
                label: "branch_b".to_string(),
                weight: 0.4,
                child_ids: vec![4],
            },
            ArchivedGotNodeSnapshot {
                id: 4,
                label: "leaf".to_string(),
                weight: 0.3,
                child_ids: vec![],
            },
        ],
        edges: vec![(1, 2), (1, 3), (2, 4), (3, 4)],
        session_id: "got_session_abc".to_string(),
        schema_version: 1,
    }
);

round_trip_test!(
    test_archived_got_snapshot_empty,
    ArchivedGoTSnapshot,
    ArchivedGoTSnapshot {
        nodes: vec![],
        edges: vec![],
        session_id: "empty_session".to_string(),
        schema_version: 1,
    }
);
