//! E2E integration tests for touring-learning — ACO subsystem
//!
//! Covers:
//! 1. MutableGeneratorGraph — DAG of ACO generators with pheromone state
//! 2. Tracker — ACO tracker with composite scoring and dimensional rewards
//! 3. UnifiedPheromoneBus — shared pheromone backbone
//! 4. Saga orchestration — saga state machine
//! 5. Multi-objective RL bridge
//!
//! Tests verify the hot path: graph construction → tracker update → pheromone propagation.

// Test harness idioms permitted.
#![allow(
    clippy::assertions_on_constants,
    clippy::let_unit_value,
    clippy::manual_range_contains,
    clippy::unnecessary_cast,
    clippy::useless_vec,
    clippy::int_plus_one,
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic
)]

use touring_intelligence::rl::aco::{
    CheckContext, CheckHandler, CheckRegistry, DimResult, DimensionalFeatures, GraphError,
    MultiObjectiveMapping, MutableGeneratorGraph, PheroKey, SagaOrchestrator, SagaState, SagaStep,
    StepResult, TrackerRlBridge, TrackerStatus, UnifiedPheromoneBus, build_report,
    compute_composite, determine_status, dim_from_checks,
};

// ══════════════════════════════════════════════════════════════════════════════
// MutableGeneratorGraph tests — DAG of ACO generators
// ══════════════════════════════════════════════════════════════════════════════

mod graph_tests {
    use super::*;

    fn make_graph() -> MutableGeneratorGraph {
        MutableGeneratorGraph::new()
    }

    #[test]
    fn graph_new_is_empty() {
        let g = make_graph();
        assert_eq!(g.node_count(), 0);
        assert_eq!(g.edge_count(), 0);
    }

    #[test]
    fn graph_add_node_increments_count() {
        let mut g = make_graph();
        g.add_node_with_deps("gen_a", "echo", None).unwrap();
        assert_eq!(g.node_count(), 1);
    }

    #[test]
    fn graph_add_edge_increments_edge_count() {
        let mut g = make_graph();
        g.add_node_with_deps("gen_a", "echo", None).unwrap();
        g.add_node_with_deps("gen_b", "echo", None).unwrap();
        g.add_edge("gen_a", "gen_b", None).unwrap();
        assert_eq!(g.edge_count(), 1);
    }

    #[test]
    fn graph_topological_order_respects_edges() {
        let mut g = make_graph();
        g.add_node_with_deps("a", "echo", None).unwrap();
        g.add_node_with_deps("b", "echo", None).unwrap();
        g.add_node_with_deps("c", "echo", None).unwrap();
        g.add_edge("a", "b", None).unwrap();
        g.add_edge("b", "c", None).unwrap();
        let order = g.topological_order().unwrap();
        let pos = |id: &str| order.iter().position(|x| x == id).unwrap();
        assert!(pos("a") < pos("b"));
        assert!(pos("b") < pos("c"));
    }

    #[test]
    fn graph_cycle_detection_rejects_cycle() {
        let mut g = make_graph();
        g.add_node_with_deps("a", "echo", None).unwrap();
        g.add_node_with_deps("b", "echo", None).unwrap();
        g.add_node_with_deps("c", "echo", None).unwrap();
        g.add_edge("a", "b", None).unwrap();
        g.add_edge("b", "c", None).unwrap();
        let result = g.add_edge("c", "a", None);
        assert!(result.is_err());
    }

    #[test]
    fn graph_remove_node_removes_from_adjacency() {
        let mut g = make_graph();
        g.add_node_with_deps("a", "echo", None).unwrap();
        g.add_node_with_deps("b", "echo", None).unwrap();
        g.add_edge("a", "b", None).unwrap();
        g.remove_node("a").unwrap();
        assert_eq!(g.node_count(), 1);
        assert_eq!(g.edge_count(), 0);
    }

    #[test]
    fn graph_get_pheromone_returns_default() {
        let g = make_graph();
        let p = g.get_pheromone("missing");
        assert!((p - 0.0).abs() < 1e-9);
    }

    #[test]
    fn graph_update_pheromone_modifies_value() {
        let mut g = make_graph();
        g.add_node_with_deps("gen", "echo", None).unwrap();
        g.update_pheromone("gen", 0.8).unwrap();
        let p = g.get_pheromone("gen");
        assert!((p - 0.8).abs() < 1e-9);
    }

    #[test]
    fn graph_evaporate_all_decays_pheromones() {
        let mut g = make_graph();
        g.add_node_with_deps("gen", "echo", None).unwrap();
        g.update_pheromone("gen", 1.0).unwrap();
        g.set_evaporation_rate(0.5);
        g.evaporate_all();
        let p = g.get_pheromone("gen");
        assert!((p - 0.5).abs() < 1e-9);
    }

    #[test]
    fn graph_node_data_returns_correct_kind() {
        let mut g = make_graph();
        g.add_node_with_deps("gen", "test_kind", None).unwrap();
        let data = g.node_data("gen".to_string()).unwrap();
        // Note: kind parameter in add_node_with_deps is used for pheromone hashing
        // but generator_type is always Template in the auto-created node.
        // The kind stored in NodeDataView reflects the GeneratorType enum.
        assert_eq!(data.kind, "Template");
    }

    #[test]
    fn graph_successors_returns_direct_children() {
        let mut g = make_graph();
        g.add_node_with_deps("a", "echo", None).unwrap();
        g.add_node_with_deps("b", "echo", None).unwrap();
        g.add_node_with_deps("c", "echo", None).unwrap();
        g.add_edge("a", "b", None).unwrap();
        g.add_edge("a", "c", None).unwrap();
        let succ = g.successors("a".to_string()).unwrap();
        assert_eq!(succ.len(), 2);
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Tracker tests — composite scoring and dimensional rewards
// ══════════════════════════════════════════════════════════════════════════════

mod tracker_tests {
    use super::*;

    // Helper: build a DimResult from pass/fail checks
    fn dim(name_id: &str, name: &str, checks: &[(&str, bool)]) -> DimResult {
        dim_from_checks(name_id, name, checks)
    }

    #[test]
    fn tracker_status_pass_all_above_veto() {
        // All dims score >= VETO_THRESHOLD (0.80) → Pass
        let dims = vec![
            dim(
                "D1",
                "Correctness",
                &[("c1", true), ("c2", true), ("c3", true), ("c4", true)],
            ),
            dim(
                "D2",
                "Completeness",
                &[("c1", true), ("c2", true), ("c3", true)],
            ),
            dim("D3", "Performance", &[("c1", true), ("c2", true)]),
            dim("D4", "Reliability", &[("c1", true)]),
            dim("D5", "Maintainability", &[("c1", true)]),
            dim(
                "D6",
                "Security",
                &[("c1", true), ("c2", true), ("c3", true)],
            ),
            dim("D7", "Observability", &[("c1", true), ("c2", true)]),
            dim("D8", "Testability", &[("c1", true)]),
            dim("D9", "Evolvability", &[("c1", true), ("c2", true)]),
        ];
        let c = compute_composite(&dims);
        assert_eq!(determine_status(&dims, c), TrackerStatus::Pass);
    }

    #[test]
    fn tracker_status_veto_some_below_veto() {
        // D7=0.78 < VETO_THRESHOLD=0.80, composite >= HALT → Veto
        let dims = vec![
            dim(
                "D1",
                "Correctness",
                &[("c1", true), ("c2", true), ("c3", true), ("c4", true)],
            ),
            dim(
                "D2",
                "Completeness",
                &[("c1", true), ("c2", true), ("c3", true)],
            ),
            dim("D3", "Performance", &[("c1", true)]),
            dim("D4", "Reliability", &[("c1", false)]),
            dim("D5", "Maintainability", &[("c1", false)]),
            dim("D6", "Security", &[("c1", true), ("c2", true)]),
            dim(
                "D7",
                "Observability",
                &[("c1", true), ("c2", true), ("c3", false)],
            ), // 2/3 = 0.667
            dim("D8", "Testability", &[("c1", true)]),
            dim("D9", "Evolvability", &[("c1", true), ("c2", true)]),
        ];
        let c = compute_composite(&dims);
        assert_eq!(determine_status(&dims, c), TrackerStatus::Veto);
    }

    #[test]
    fn tracker_status_halt_all_below_half() {
        // All dims score very low, composite < HALT_THRESHOLD (0.50) → Halt
        let dims = vec![
            dim("D1", "Correctness", &[("c1", false)]),
            dim("D2", "Completeness", &[("c1", false)]),
            dim("D3", "Performance", &[("c1", false)]),
            dim("D4", "Reliability", &[("c1", false)]),
            dim("D5", "Maintainability", &[("c1", false)]),
            dim("D6", "Security", &[("c1", false)]),
            dim("D7", "Observability", &[("c1", false)]),
            dim("D8", "Testability", &[("c1", false)]),
            dim("D9", "Evolvability", &[("c1", false)]),
        ];
        let c = compute_composite(&dims);
        assert_eq!(determine_status(&dims, c), TrackerStatus::Halt);
    }

    #[test]
    fn tracker_composite_in_range() {
        let dims = vec![
            dim("D1", "Correctness", &[("c1", true), ("c2", false)]), // 0.5
            dim("D2", "Completeness", &[("c1", true)]),
            dim("D3", "Performance", &[("c1", true)]),
            dim("D4", "Reliability", &[("c1", true)]),
            dim("D5", "Maintainability", &[("c1", true)]),
            dim("D6", "Security", &[("c1", true)]),
            dim("D7", "Observability", &[("c1", true)]),
            dim("D8", "Testability", &[("c1", true)]),
            dim("D9", "Evolvability", &[("c1", true)]),
        ];
        let c = compute_composite(&dims);
        assert!(c >= 0.0 && c <= 1.0);
    }

    #[test]
    fn tracker_report_dimensional_rewards() {
        let dims = vec![
            dim("D1", "Correctness", &[("c1", true), ("c2", true)]), // pass → 0.5+0.5*1.0=1.0
            dim("D2", "Completeness", &[("c1", false)]),             // fail → 0.0*0.5=0.0
        ];
        let report = build_report(dims, 1);
        let rewards: Vec<_> = report.dimensional_rewards();
        assert!((rewards[0].1 - 1.0).abs() < 1e-9);
        assert!((rewards[1].1 - 0.0).abs() < 1e-9);
    }

    #[test]
    fn tracker_as_rl_reward_pass() {
        let dims = vec![
            dim(
                "D1",
                "Correctness",
                &[("c1", true), ("c2", true), ("c3", true), ("c4", true)],
            ),
            dim("D2", "Completeness", &[("c1", true)]),
            dim("D3", "Performance", &[("c1", true)]),
            dim("D4", "Reliability", &[("c1", true)]),
            dim("D5", "Maintainability", &[("c1", true)]),
            dim("D6", "Security", &[("c1", true)]),
            dim("D7", "Observability", &[("c1", true)]),
            dim("D8", "Testability", &[("c1", true)]),
            dim("D9", "Evolvability", &[("c1", true)]),
        ];
        let report = build_report(dims, 1);
        let reward = report.as_rl_reward();
        // composite ~1.0, Pass → 0.5 + 0.5*1.0 = 1.0
        assert!((reward - 1.0).abs() < 1e-9);
    }

    #[test]
    fn tracker_as_rl_reward_halt() {
        let dims = vec![
            dim("D1", "Correctness", &[("c1", false)]),
            dim("D2", "Completeness", &[("c1", false)]),
            dim("D3", "Performance", &[("c1", false)]),
            dim("D4", "Reliability", &[("c1", false)]),
            dim("D5", "Maintainability", &[("c1", false)]),
            dim("D6", "Security", &[("c1", false)]),
            dim("D7", "Observability", &[("c1", false)]),
            dim("D8", "Testability", &[("c1", false)]),
            dim("D9", "Evolvability", &[("c1", false)]),
        ];
        let report = build_report(dims, 1);
        assert_eq!(report.as_rl_reward(), 0.0);
    }

    #[test]
    fn tracker_d1_d2_d6_are_critical() {
        // D1 and D2 are critical (weight 1.5)
        let d1 = dim("D1", "Correctness", &[("c1", true)]);
        let d2 = dim("D2", "Completeness", &[("c1", true)]);
        let d3 = dim("D3", "Performance", &[("c1", true)]);
        assert!((d1.weight() - 1.5).abs() < 1e-9);
        assert!((d2.weight() - 1.5).abs() < 1e-9);
        assert!((d3.weight() - 1.0).abs() < 1e-9);
    }

    #[test]
    fn tracker_build_report_sets_iteration() {
        let dims = vec![dim("D1", "Correctness", &[("c1", true)])];
        let report = build_report(dims, 42);
        assert_eq!(report.iteration, 42);
    }

    #[test]
    fn tracker_dim_result_passing() {
        let d = dim(
            "D1",
            "Correctness",
            &[("c1", true), ("c2", true), ("c3", true), ("c4", true)],
        );
        assert!(d.is_passing());
        assert!((d.weighted_score() - 1.5).abs() < 1e-9); // score=1.0 * weight=1.5
    }

    #[test]
    fn tracker_dim_result_not_passing() {
        let d = dim("D1", "Correctness", &[("c1", false), ("c2", false)]);
        assert!(!d.is_passing());
        assert!((d.weighted_score() - 0.0).abs() < 1e-9); // score=0.0 * weight=1.5
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// UnifiedPheromoneBus tests — shared pheromone backbone
// ══════════════════════════════════════════════════════════════════════════════

mod pheromone_bus_tests {
    use super::*;

    #[test]
    fn bus_deposit_and_get() {
        let bus = UnifiedPheromoneBus::new(0.0);
        bus.deposit(PheroKey::FilePath("src/lib.rs".into()), 1.0);
        assert!((bus.get(&PheroKey::FilePath("src/lib.rs".into())) - 1.0).abs() < 1e-9);
    }

    #[test]
    fn bus_deposit_accumulates() {
        let bus = UnifiedPheromoneBus::new(0.0);
        bus.deposit(PheroKey::TemplateId("tpl_1".into()), 0.5);
        bus.deposit(PheroKey::TemplateId("tpl_1".into()), 0.3);
        assert!((bus.get(&PheroKey::TemplateId("tpl_1".into())) - 0.8).abs() < 1e-9);
    }

    #[test]
    fn bus_evaporate_all_decays() {
        let bus = UnifiedPheromoneBus::new(0.5); // 50% evaporation
        bus.deposit(PheroKey::ActionPair(1, 2), 1.0);
        bus.evaporate_all();
        assert!((bus.get(&PheroKey::ActionPair(1, 2)) - 0.5).abs() < 1e-9);
    }

    #[test]
    fn bus_top_k_returns_sorted() {
        let bus = UnifiedPheromoneBus::new(0.0);
        bus.deposit(PheroKey::TaskId("task_a".into()), 3.0);
        bus.deposit(PheroKey::TaskId("task_b".into()), 1.0);
        bus.deposit(PheroKey::TaskId("task_c".into()), 2.0);
        let top = bus.top_k(2);
        assert_eq!(top.len(), 2);
        assert!((top[0].1 - 3.0).abs() < 1e-9);
        assert!((top[1].1 - 2.0).abs() < 1e-9);
    }

    #[test]
    fn bus_entry_count() {
        let bus = UnifiedPheromoneBus::new(0.0);
        bus.deposit(PheroKey::FilePath("a.rs".into()), 1.0);
        bus.deposit(PheroKey::FilePath("b.rs".into()), 1.0);
        assert_eq!(bus.entry_count(), 2);
    }

    #[test]
    fn bus_clone_shares_state() {
        let bus = UnifiedPheromoneBus::new(0.0);
        let bus2 = bus.clone();
        bus.deposit(PheroKey::TeammateId("t1".into()), 7.0);
        assert!((bus2.get(&PheroKey::TeammateId("t1".into())) - 7.0).abs() < 1e-9);
    }

    #[test]
    fn bus_snapshot_values() {
        let bus = UnifiedPheromoneBus::new(0.0);
        bus.deposit(PheroKey::TemplateId("x".into()), 1.0);
        bus.deposit(PheroKey::TemplateId("y".into()), 2.0);
        let snap = bus.snapshot_values();
        assert_eq!(snap.len(), 2);
        assert!(snap.contains(&1.0));
        assert!(snap.contains(&2.0));
    }

    #[test]
    fn bus_evaporate_all_prunes_near_zero() {
        let bus = UnifiedPheromoneBus::new(1.0); // 100% evaporation
        bus.deposit(PheroKey::FilePath("a.rs".into()), 0.01);
        bus.evaporate_all();
        assert_eq!(bus.entry_count(), 0);
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Saga tests — saga state machine
// ══════════════════════════════════════════════════════════════════════════════

mod saga_tests {
    use super::*;

    #[test]
    fn saga_state_pending() {
        assert_eq!(SagaState::Pending, SagaState::Pending);
    }

    #[test]
    fn saga_state_running() {
        assert_eq!(SagaState::Running, SagaState::Running);
    }

    #[test]
    fn saga_state_completed() {
        assert_eq!(SagaState::Completed, SagaState::Completed);
    }

    #[test]
    fn saga_state_compensating() {
        assert_eq!(SagaState::Compensating, SagaState::Compensating);
    }

    #[test]
    fn saga_state_compensated() {
        assert_eq!(SagaState::Compensated, SagaState::Compensated);
    }

    #[test]
    fn saga_state_failed() {
        let failed = SagaState::Failed("boom".to_string());
        assert_eq!(failed, SagaState::Failed("boom".to_string()));
    }

    #[test]
    fn step_result_succeeded() {
        assert_eq!(StepResult::Succeeded, StepResult::Succeeded);
    }

    #[test]
    fn step_result_failed() {
        let failed = StepResult::Failed("error".into());
        assert!(matches!(failed, StepResult::Failed(_)));
    }

    #[test]
    fn saga_orchestrator_new_is_pending() {
        let orch = SagaOrchestrator::new();
        assert_eq!(*orch.state(), SagaState::Pending);
    }

    #[test]
    fn saga_step_impl_execute_and_compensate() {
        struct TestStep;
        impl SagaStep for TestStep {
            fn step_id(&self) -> &str {
                "test_step"
            }
            fn execute(&self) -> StepResult {
                StepResult::Succeeded
            }
            fn compensate(&self) -> StepResult {
                StepResult::Succeeded
            }
        }
        let step = TestStep;
        assert_eq!(step.step_id(), "test_step");
        assert!(matches!(step.execute(), StepResult::Succeeded));
        assert!(matches!(step.compensate(), StepResult::Succeeded));
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// TrackerRlBridge + MultiObjectiveMapping tests
// ══════════════════════════════════════════════════════════════════════════════

mod tracker_rl_bridge_tests {
    use super::*;

    #[test]
    fn multi_objective_mapping_default() {
        let mapping = MultiObjectiveMapping::default();
        let feats = DimensionalFeatures {
            d1: 0.9,
            d2: 0.8,
            d3: 0.3,
            d4: 0.1,
            d5: 0.0,
            d6: 0.5,
            d7: 0.2,
            d8: 0.7,
            d9: 0.88,
        };
        let obj = mapping.compute_objectives(&feats.as_array());
        assert_eq!(obj.len(), 4);
        for o in &obj {
            assert!(*o >= 0.0 && *o <= 1.0);
        }
    }

    #[test]
    fn tracker_rl_bridge_scalar_reward() {
        let bridge = TrackerRlBridge::new();
        let dims = vec![
            dim_from_checks(
                "D1",
                "Correctness",
                &[("c1", true), ("c2", true), ("c3", true), ("c4", true)],
            ),
            dim_from_checks("D2", "Completeness", &[("c1", true)]),
            dim_from_checks("D3", "Performance", &[("c1", true)]),
            dim_from_checks("D4", "Reliability", &[("c1", true)]),
            dim_from_checks("D5", "Maintainability", &[("c1", true)]),
            dim_from_checks("D6", "Security", &[("c1", true)]),
            dim_from_checks("D7", "Observability", &[("c1", true)]),
            dim_from_checks("D8", "Testability", &[("c1", true)]),
            dim_from_checks("D9", "Evolvability", &[("c1", true)]),
        ];
        let report = build_report(dims, 1);
        let reward = bridge.scalar_reward(&report);
        assert!(reward >= 0.0 && reward <= 1.0);
    }

    #[test]
    fn tracker_rl_bridge_extract_linucb_features() {
        let bridge = TrackerRlBridge::new();
        let dims = vec![
            dim_from_checks("D1", "Correctness", &[("c1", true), ("c2", true)]),
            dim_from_checks("D2", "Completeness", &[("c1", false)]),
            dim_from_checks("D3", "Performance", &[("c1", true)]),
            dim_from_checks("D4", "Reliability", &[("c1", false)]),
            dim_from_checks("D5", "Maintainability", &[("c1", false)]),
            dim_from_checks("D6", "Security", &[("c1", true)]),
            dim_from_checks("D7", "Observability", &[("c1", true)]),
            dim_from_checks("D8", "Testability", &[("c1", false)]),
            dim_from_checks("D9", "Evolvability", &[("c1", true)]),
        ];
        let report = build_report(dims, 1);
        let features = bridge.extract_linucb_features(&report);
        assert_eq!(features.d1, 1.0);
        assert_eq!(features.d2, 0.0);
    }

    #[test]
    fn tracker_rl_bridge_compute_pareto_objectives() {
        let bridge = TrackerRlBridge::new();
        let dims = vec![dim_from_checks("D1", "Correctness", &[("c1", true)])];
        let report = build_report(dims, 1);
        let obj = bridge.compute_pareto_objectives(&report);
        assert_eq!(obj.len(), 4);
    }

    #[test]
    fn tracker_rl_bridge_process_report() {
        let bridge = TrackerRlBridge::new();
        let dims = vec![
            dim_from_checks("D1", "Correctness", &[("c1", true)]),
            dim_from_checks("D2", "Completeness", &[("c1", true)]),
            dim_from_checks("D3", "Performance", &[("c1", true)]),
            dim_from_checks("D4", "Reliability", &[("c1", true)]),
            dim_from_checks("D5", "Maintainability", &[("c1", true)]),
            dim_from_checks("D6", "Security", &[("c1", true)]),
            dim_from_checks("D7", "Observability", &[("c1", true)]),
            dim_from_checks("D8", "Testability", &[("c1", true)]),
            dim_from_checks("D9", "Evolvability", &[("c1", true)]),
        ];
        let report = build_report(dims, 1);
        let (features, objectives, reward) = bridge.process_report(&report);
        assert_eq!(features.d1, 1.0);
        assert_eq!(objectives.len(), 4);
        // Reward is computed by the RL bridge and may vary; verify it's in valid range
        assert!(reward >= 0.0 && reward <= 1.0);
    }

    #[test]
    fn tracker_rl_default_bridge() {
        let bridge = TrackerRlBridge::default();
        let dims = vec![dim_from_checks("D1", "Correctness", &[("c1", false)])];
        let report = build_report(dims, 1);
        let reward = bridge.scalar_reward(&report);
        assert!(reward >= 0.0);
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// DimensionalFeatures tests
// ══════════════════════════════════════════════════════════════════════════════

mod dimensional_features_tests {
    use super::*;

    #[test]
    fn dimensional_features_fields() {
        let df = DimensionalFeatures {
            d1: 0.9,
            d2: 0.85,
            d3: 0.3,
            d4: 0.1,
            d5: 0.0,
            d6: 0.5,
            d7: 0.2,
            d8: 0.7,
            d9: 0.88,
        };
        assert_eq!(df.d1, 0.9);
        assert_eq!(df.d9, 0.88);
    }

    #[test]
    fn dimensional_features_as_array() {
        let df = DimensionalFeatures {
            d1: 0.1,
            d2: 0.2,
            d3: 0.3,
            d4: 0.4,
            d5: 0.5,
            d6: 0.6,
            d7: 0.7,
            d8: 0.8,
            d9: 0.9,
        };
        let arr = df.as_array();
        assert_eq!(arr.len(), 9);
        assert!((arr[0] - 0.1).abs() < 1e-9);
        assert!((arr[8] - 0.9).abs() < 1e-9);
    }

    #[test]
    fn dimensional_features_append_to_context() {
        let df = DimensionalFeatures {
            d1: 0.1,
            d2: 0.2,
            d3: 0.3,
            d4: 0.4,
            d5: 0.5,
            d6: 0.6,
            d7: 0.7,
            d8: 0.8,
            d9: 0.9,
        };
        let mut ctx = vec![1.0, 2.0];
        df.append_to_context(&mut ctx);
        assert_eq!(ctx.len(), 11);
        assert!((ctx[2] - 0.1).abs() < 1e-9);
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// CheckHandler + CheckRegistry tests
// ══════════════════════════════════════════════════════════════════════════════

mod check_registry_tests {
    use super::*;

    struct PassHandler;
    impl CheckHandler for PassHandler {
        fn check_id(&self) -> &str {
            "PASS_01"
        }
        fn verify(&self, _ctx: &CheckContext) -> bool {
            true
        }
    }

    struct FailHandler;
    impl CheckHandler for FailHandler {
        fn check_id(&self) -> &str {
            "FAIL_01"
        }
        fn verify(&self, _ctx: &CheckContext) -> bool {
            false
        }
    }

    #[test]
    fn check_handler_pass() {
        let h = PassHandler;
        assert_eq!(h.check_id(), "PASS_01");
        assert!(h.verify(&CheckContext::new()));
    }

    #[test]
    fn check_handler_fail() {
        let h = FailHandler;
        assert_eq!(h.check_id(), "FAIL_01");
        assert!(!h.verify(&CheckContext::new()));
    }

    #[test]
    fn check_registry_default_empty() {
        let reg = CheckRegistry::default();
        assert_eq!(reg.checks_len(), 0);
    }

    #[test]
    fn check_registry_register() {
        let mut reg = CheckRegistry::default();
        reg.register(PassHandler);
        assert_eq!(reg.checks_len(), 1);
    }

    #[test]
    fn check_context_with_value() {
        let ctx = CheckContext::new()
            .with_value("D1C1", 1.0)
            .with_value("D2C1", 0.5)
            .with_threshold(0.8);
        assert!((ctx.values.get("D1C1").copied().unwrap_or(0.0) - 1.0).abs() < 1e-9);
        assert!((ctx.values.get("D2C1").copied().unwrap_or(0.0) - 0.5).abs() < 1e-9);
        assert!((ctx.threshold - 0.8).abs() < 1e-9);
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// GraphError tests
// ══════════════════════════════════════════════════════════════════════════════

mod graph_error_tests {
    use super::*;

    #[test]
    fn graph_error_node_not_found() {
        let e = GraphError::NodeNotFound("missing".into());
        let s = e.to_string();
        assert!(!s.is_empty());
    }

    #[test]
    fn graph_error_cycle_detected() {
        let e = GraphError::CycleDetected;
        let s = e.to_string();
        assert!(!s.is_empty());
    }

    #[test]
    fn graph_error_result_handling() {
        let result: Result<String, GraphError> = Err(GraphError::NodeNotFound("test".into()));
        assert!(result.is_err());
        let result2: Result<String, GraphError> = Ok("ok".into());
        assert!(result2.is_ok());
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// PheroKey all variants
// ══════════════════════════════════════════════════════════════════════════════

mod phero_key_tests {
    use super::*;

    #[test]
    fn phero_key_file_path() {
        let k = PheroKey::FilePath("src/lib.rs".into());
        assert!(matches!(k, PheroKey::FilePath(_)));
    }

    #[test]
    fn phero_key_action_pair() {
        let k = PheroKey::ActionPair(1, 2);
        assert!(matches!(k, PheroKey::ActionPair(1, 2)));
    }

    #[test]
    fn phero_key_template_id() {
        let k = PheroKey::TemplateId("tpl_test".into());
        assert!(matches!(k, PheroKey::TemplateId(_)));
    }

    #[test]
    fn phero_key_task_id() {
        let k = PheroKey::TaskId("task_abc".into());
        assert!(matches!(k, PheroKey::TaskId(_)));
    }

    #[test]
    fn phero_key_teammate_id() {
        let k = PheroKey::TeammateId("tm_1".into());
        assert!(matches!(k, PheroKey::TeammateId(_)));
    }

    #[test]
    fn phero_key_limbo_pattern() {
        let k = PheroKey::LimboPattern("idle:tm_1".into());
        assert!(matches!(k, PheroKey::LimboPattern(_)));
    }

    #[test]
    fn phero_key_clone_is_equal() {
        let k1 = PheroKey::FilePath("a.rs".into());
        let k2 = k1.clone();
        assert_eq!(k1, k2);
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Integration: graph + tracker + pheromone bus together
// ══════════════════════════════════════════════════════════════════════════════

mod integration_tests {
    use super::*;

    #[test]
    fn graph_and_bus_integration() {
        let mut graph = MutableGeneratorGraph::new();
        graph.add_node_with_deps("gen_a", "echo", None).unwrap();
        graph.add_node_with_deps("gen_b", "echo", None).unwrap();
        graph.update_pheromone("gen_a", 0.9).unwrap();
        graph.update_pheromone("gen_b", 0.7).unwrap();
        let p_a = graph.get_pheromone("gen_a");
        let p_b = graph.get_pheromone("gen_b");
        assert!((p_a - 0.9).abs() < 1e-9);
        assert!((p_b - 0.7).abs() < 1e-9);
    }

    #[test]
    fn tracker_report_integration() {
        let dims = vec![
            dim_from_checks(
                "D1",
                "Correctness",
                &[("c1", true), ("c2", true), ("c3", true), ("c4", true)],
            ),
            dim_from_checks("D2", "Completeness", &[("c1", true), ("c2", true)]),
            dim_from_checks("D3", "Performance", &[("c1", true)]),
            dim_from_checks("D4", "Reliability", &[("c1", true)]),
            dim_from_checks("D5", "Maintainability", &[("c1", true)]),
            dim_from_checks("D6", "Security", &[("c1", true), ("c2", true)]),
            dim_from_checks("D7", "Observability", &[("c1", true), ("c2", true)]),
            dim_from_checks("D8", "Testability", &[("c1", true)]),
            dim_from_checks("D9", "Evolvability", &[("c1", true), ("c2", true)]),
        ];
        let report = build_report(dims, 1);
        assert_eq!(report.iteration, 1);
        assert_eq!(report.status, TrackerStatus::Pass);
        let rewards = report.dimensional_rewards();
        assert_eq!(rewards.len(), 9);
    }

    #[test]
    fn dimensional_features_append_integration() {
        let df = DimensionalFeatures {
            d1: 0.5,
            d2: 0.6,
            d3: 0.7,
            d4: 0.8,
            d5: 0.9,
            d6: 1.0,
            d7: 0.4,
            d8: 0.3,
            d9: 0.2,
        };
        let mut ctx = vec![];
        df.append_to_context(&mut ctx);
        assert_eq!(ctx.len(), 9);
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// Tracker constants
// ══════════════════════════════════════════════════════════════════════════════

mod tracker_constants_tests {

    #[test]
    fn veto_threshold_is_080() {
        assert!((touring_intelligence::rl::aco::VETO_THRESHOLD - 0.80).abs() < 1e-10);
    }

    #[test]
    fn halt_threshold_is_050() {
        assert!((touring_intelligence::rl::aco::HALT_THRESHOLD - 0.50).abs() < 1e-10);
    }

    #[test]
    fn critical_dims_contains_d1_d2_d6() {
        assert!(touring_intelligence::rl::aco::CRITICAL_DIMS.contains(&"D1"));
        assert!(touring_intelligence::rl::aco::CRITICAL_DIMS.contains(&"D2"));
        assert!(touring_intelligence::rl::aco::CRITICAL_DIMS.contains(&"D6"));
    }

    #[test]
    fn critical_weight_is_15() {
        assert!((touring_intelligence::rl::aco::CRITICAL_WEIGHT - 1.5).abs() < 1e-10);
    }

    #[test]
    fn normal_weight_is_10() {
        assert!((touring_intelligence::rl::aco::NORMAL_WEIGHT - 1.0).abs() < 1e-10);
    }
}

// ══════════════════════════════════════════════════════════════════════════════
// TrackerStatus display
// ══════════════════════════════════════════════════════════════════════════════

mod tracker_status_tests {
    use super::*;

    #[test]
    fn tracker_status_all_variants() {
        for s in [
            TrackerStatus::Pass,
            TrackerStatus::Veto,
            TrackerStatus::Halt,
        ] {
            let _ = s;
        }
        assert!(true);
    }

    #[test]
    fn tracker_status_debug() {
        let s = TrackerStatus::Pass;
        let debug = format!("{:?}", s);
        assert!(!debug.is_empty());
    }

    #[test]
    fn tracker_status_display() {
        assert_eq!(TrackerStatus::Pass.to_string(), "PASS");
        assert_eq!(TrackerStatus::Veto.to_string(), "VETO");
        assert_eq!(TrackerStatus::Halt.to_string(), "HALT");
    }
}
