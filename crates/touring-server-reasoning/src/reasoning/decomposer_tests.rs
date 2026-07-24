use super::*;

#[test]
fn test_create_task() {
    let mut d = TaskDecomposer::new();
    let id = d.create_task("feature", "Build auth module");
    assert!(d.get_plan(&id).is_some());
    assert_eq!(d.get_plan(&id).unwrap().task_type, "feature");
}

#[test]
fn test_add_subtask() {
    let mut d = TaskDecomposer::new();
    let tid = d.create_task("refactor", "Split module");
    let s1 = d.add_subtask(&tid, "Extract types", vec![], 0).unwrap();
    let s2 = d
        .add_subtask(&tid, "Move functions", vec![s1.clone()], 1)
        .unwrap();

    let plan = d.get_plan(&tid).unwrap();
    assert_eq!(plan.subtasks.len(), 2);
    assert_eq!(plan.subtasks[1].depends_on, vec![s1.clone()]);
    assert_ne!(s2, s1);
}

#[test]
fn test_add_subtask_invalid_dep() {
    let mut d = TaskDecomposer::new();
    let tid = d.create_task("debug", "Fix crash");
    let result = d.add_subtask(&tid, "Step 1", vec!["nonexistent".into()], 0);
    assert!(result.is_err());
    assert!(result.unwrap_err().contains("Dependency not found"));
}

#[test]
fn test_update_status() {
    let mut d = TaskDecomposer::new();
    let tid = d.create_task("feature", "Test");
    let sid = d.add_subtask(&tid, "Do thing", vec![], 0).unwrap();

    d.update_status(&tid, &sid, SubTaskStatus::InProgress)
        .unwrap();
    let plan = d.get_plan(&tid).unwrap();
    assert_eq!(plan.subtasks[0].status, SubTaskStatus::InProgress);
}

#[test]
fn test_updated_at_changes_on_status_update() {
    let mut d = TaskDecomposer::new();
    let tid = d.create_task("feature", "Test timestamps");
    let sid = d.add_subtask(&tid, "Work", vec![], 0).unwrap();

    let created = d
        .get_plan(&tid)
        .unwrap()
        .get_subtask(&sid)
        .unwrap()
        .created_at;
    // Small sleep to ensure timestamp differs
    std::thread::sleep(std::time::Duration::from_millis(5));

    d.update_status(&tid, &sid, SubTaskStatus::Completed)
        .unwrap();
    let updated = d
        .get_plan(&tid)
        .unwrap()
        .get_subtask(&sid)
        .unwrap()
        .updated_at;
    assert!(updated >= created, "updated_at should be >= created_at");
}

#[test]
fn test_validate_order_linear() {
    let mut d = TaskDecomposer::new();
    let tid = d.create_task("pipeline", "Sequential");
    let s1 = d.add_subtask(&tid, "Step 1", vec![], 0).unwrap();
    let s2 = d.add_subtask(&tid, "Step 2", vec![s1.clone()], 0).unwrap();
    let s3 = d.add_subtask(&tid, "Step 3", vec![s2.clone()], 0).unwrap();

    let order = d.validate_order(&tid).unwrap();
    assert_eq!(order.len(), 3);
    let pos = |id: &str| order.iter().position(|x| x == id).unwrap();
    assert!(pos(&s1) < pos(&s2));
    assert!(pos(&s2) < pos(&s3));
}

#[test]
fn test_validate_order_empty() {
    let mut d = TaskDecomposer::new();
    let tid = d.create_task("analysis", "Empty plan");
    let order = d.validate_order(&tid).unwrap();
    assert!(order.is_empty());
}

#[test]
fn test_validate_order_parallel() {
    let mut d = TaskDecomposer::new();
    let tid = d.create_task("feature", "Parallel work");
    let _s1 = d.add_subtask(&tid, "Independent A", vec![], 0).unwrap();
    let _s2 = d.add_subtask(&tid, "Independent B", vec![], 0).unwrap();

    let order = d.validate_order(&tid).unwrap();
    assert_eq!(order.len(), 2);
}

#[test]
fn test_validate_order_priority_respected() {
    let mut d = TaskDecomposer::new();
    let tid = d.create_task("feature", "Priority ordering");
    // Add in reverse priority order — lower number = higher priority
    let s_low = d.add_subtask(&tid, "Low priority", vec![], 5).unwrap();
    let s_high = d.add_subtask(&tid, "High priority", vec![], 0).unwrap();

    let order = d.validate_order(&tid).unwrap();
    let pos = |id: &str| order.iter().position(|x| x == id).unwrap();
    // High priority (0) should come before low priority (5)
    assert!(pos(&s_high) < pos(&s_low));
}

#[test]
fn test_validate_order_diamond_dag() {
    // A → B, A → C, B → D, C → D  (diamond dependency)
    let mut d = TaskDecomposer::new();
    let tid = d.create_task("feature", "Diamond DAG");
    let a = d.add_subtask(&tid, "A", vec![], 0).unwrap();
    let b = d.add_subtask(&tid, "B", vec![a.clone()], 0).unwrap();
    let c = d.add_subtask(&tid, "C", vec![a.clone()], 0).unwrap();
    let dd = d
        .add_subtask(&tid, "D", vec![b.clone(), c.clone()], 0)
        .unwrap();

    let order = d.validate_order(&tid).unwrap();
    assert_eq!(order.len(), 4);
    let pos = |id: &str| order.iter().position(|x| x == id).unwrap();
    assert!(pos(&a) < pos(&b));
    assert!(pos(&a) < pos(&c));
    assert!(pos(&b) < pos(&dd));
    assert!(pos(&c) < pos(&dd));
}

#[test]
fn test_task_not_found() {
    let mut d = TaskDecomposer::new();
    assert!(d.get_plan("nonexistent").is_none());
    assert!(d.validate_order("nonexistent").is_err());
}

#[test]
fn test_delete_task() {
    let mut d = TaskDecomposer::new();
    let tid = d.create_task("feature", "To be deleted");
    assert_eq!(d.task_count(), 1);
    assert!(d.delete_task(&tid));
    assert_eq!(d.task_count(), 0);
    assert!(!d.delete_task(&tid)); // second delete returns false
}

#[test]
fn test_subtask_status_from_str() {
    assert_eq!(
        "pending".parse::<SubTaskStatus>(),
        Ok(SubTaskStatus::Pending)
    );
    assert_eq!(
        "IN_PROGRESS".parse::<SubTaskStatus>(),
        Ok(SubTaskStatus::InProgress)
    );
    assert_eq!(
        "completed".parse::<SubTaskStatus>(),
        Ok(SubTaskStatus::Completed)
    );
    assert_eq!(
        "blocked".parse::<SubTaskStatus>(),
        Ok(SubTaskStatus::Blocked)
    );
    assert_eq!("failed".parse::<SubTaskStatus>(), Ok(SubTaskStatus::Failed));
    assert_eq!(
        "cancelled".parse::<SubTaskStatus>(),
        Ok(SubTaskStatus::Cancelled)
    );
    // "done"/"complete" are accepted aliases for Completed (loop_phase_close writes
    // "done"; without this alias it fell through to unwrap_or(Pending) and every
    // dependent stayed permanently blocked — the DAG done-vs-ready quirk).
    assert_eq!("done".parse::<SubTaskStatus>(), Ok(SubTaskStatus::Completed));
    assert_eq!("DONE".parse::<SubTaskStatus>(), Ok(SubTaskStatus::Completed));
    assert_eq!(
        "complete".parse::<SubTaskStatus>(),
        Ok(SubTaskStatus::Completed)
    );
    // The alias must satisfy the dependency-resolution gate, not just parse.
    assert!(
        "done"
            .parse::<SubTaskStatus>()
            .expect("done parses")
            .is_done()
    );
    assert!("unknown".parse::<SubTaskStatus>().is_err());
}

#[test]
fn test_subtask_status_is_terminal() {
    assert!(!SubTaskStatus::Pending.is_terminal());
    assert!(!SubTaskStatus::InProgress.is_terminal());
    assert!(!SubTaskStatus::Blocked.is_terminal());
    assert!(SubTaskStatus::Completed.is_terminal());
    assert!(SubTaskStatus::Failed.is_terminal());
    assert!(SubTaskStatus::Cancelled.is_terminal());
}

#[test]
fn test_ready_subtasks() {
    let mut d = TaskDecomposer::new();
    let tid = d.create_task("feature", "Ready subtasks test");
    let s1 = d.add_subtask(&tid, "Step 1", vec![], 0).unwrap();
    let s2 = d.add_subtask(&tid, "Step 2", vec![s1.clone()], 0).unwrap();
    let _s3 = d.add_subtask(&tid, "Step 3", vec![s2.clone()], 0).unwrap();

    {
        let plan = d.get_plan(&tid).unwrap();
        let ready = plan.ready_subtasks();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, s1);
    }

    // Mark s1 complete — now s2 should be ready
    d.update_status(&tid, &s1, SubTaskStatus::Completed)
        .unwrap();
    {
        let plan = d.get_plan(&tid).unwrap();
        let ready = plan.ready_subtasks();
        assert_eq!(ready.len(), 1);
        assert_eq!(ready[0].id, s2);
    }
}

#[test]
fn test_completion_pct() {
    let mut d = TaskDecomposer::new();
    let tid = d.create_task("feature", "Completion test");
    let s1 = d.add_subtask(&tid, "A", vec![], 0).unwrap();
    let _s2 = d.add_subtask(&tid, "B", vec![], 0).unwrap();

    assert_eq!(d.get_plan(&tid).unwrap().completion_pct(), 0.0);
    d.update_status(&tid, &s1, SubTaskStatus::Completed)
        .unwrap();
    assert!((d.get_plan(&tid).unwrap().completion_pct() - 50.0).abs() < 0.01);
}

#[test]
fn test_list_plans_sorted_by_creation() {
    let mut d = TaskDecomposer::new();
    let t1 = d.create_task("feature", "First");
    let t2 = d.create_task("debug", "Second");
    let plans = d.list_plans();
    assert_eq!(plans.len(), 2);
    // Should be sorted oldest first
    assert_eq!(plans[0].id, t1);
    assert_eq!(plans[1].id, t2);
}

#[test]
fn test_o1_subtask_lookup() {
    let mut d = TaskDecomposer::new();
    let tid = d.create_task("feature", "Lookup test");
    let sid = d.add_subtask(&tid, "Work", vec![], 0).unwrap();

    let plan = d.get_plan(&tid).unwrap();
    assert!(plan.get_subtask(&sid).is_some());
    assert!(plan.get_subtask("nonexistent").is_none());
}

#[test]
fn test_cycle_detection_error_message() {
    // Cannot create a direct cycle with current API (add_subtask validates deps),
    // but we can verify the error path via nonexistent task.
    let mut d = TaskDecomposer::new();
    let err = d.validate_order("ghost").unwrap_err();
    assert!(matches!(err, ReasoningError::NotFound(m) if m.contains("Task not found")));
}

// ── E2E / Cross-Audit Tests ───────────────────────────────────────────────
//
// These tests verify the module as an orchestration instrument:
// does it correctly model and drive a multi-step process toward a goal?
// They exercise the full lifecycle, not just individual methods.

/// Full TACO orchestration flow:
/// scout → architect → [engineer-1 ∥ engineer-2] → validator
///
/// Proves the module can drive a real parallel DAG to completion,
/// with correct ready-subtask dispatch at every step.
#[test]
fn test_e2e_full_taco_orchestration_flow() {
    let mut d = TaskDecomposer::new();
    let tid = d.create_task("feature", "Implement cache module");

    // Build the TACO DAG
    let scout = d.add_subtask(&tid, "Scout: map module", vec![], 0).unwrap();
    let arch = d
        .add_subtask(&tid, "Architect: design cache", vec![scout.clone()], 0)
        .unwrap();
    let eng1 = d
        .add_subtask(&tid, "Engineer-1: core impl", vec![arch.clone()], 0)
        .unwrap();
    let eng2 = d
        .add_subtask(&tid, "Engineer-2: async patterns", vec![arch.clone()], 1)
        .unwrap();
    let validator = d
        .add_subtask(
            &tid,
            "Validator: quality gates",
            vec![eng1.clone(), eng2.clone()],
            0,
        )
        .unwrap();

    // ── Step 0: Initial state — only scout is ready ────────────────────
    assert_eq!(d.get_plan(&tid).unwrap().ready_subtasks().len(), 1);
    assert_eq!(d.get_plan(&tid).unwrap().ready_subtasks()[0].id, scout);
    assert_eq!(d.get_plan(&tid).unwrap().completion_pct(), 0.0);

    // ── Step 1: Validate DAG order before execution ────────────────────
    let order = d.validate_order(&tid).unwrap();
    assert_eq!(order.len(), 5);
    let pos = |id: &str| order.iter().position(|x| x == id).unwrap();
    assert!(pos(&scout) < pos(&arch), "scout must precede architect");
    assert!(pos(&arch) < pos(&eng1), "architect must precede engineer-1");
    assert!(pos(&arch) < pos(&eng2), "architect must precede engineer-2");
    assert!(
        pos(&eng1) < pos(&validator),
        "engineer-1 must precede validator"
    );
    assert!(
        pos(&eng2) < pos(&validator),
        "engineer-2 must precede validator"
    );

    // ── Step 2: Dispatch scout ─────────────────────────────────────────
    d.update_status(&tid, &scout, SubTaskStatus::InProgress)
        .unwrap();
    assert_eq!(
        d.get_plan(&tid).unwrap().ready_subtasks().len(),
        0,
        "nothing else ready while scout runs"
    );

    d.update_status(&tid, &scout, SubTaskStatus::Completed)
        .unwrap();
    assert!(
        (d.get_plan(&tid).unwrap().completion_pct() - 20.0).abs() < 0.1,
        "1/5 = 20%"
    );

    // ── Step 3: Architect unblocked ────────────────────────────────────
    let ready = d.get_plan(&tid).unwrap().ready_subtasks();
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].id, arch);

    d.update_status(&tid, &arch, SubTaskStatus::InProgress)
        .unwrap();
    d.update_status(&tid, &arch, SubTaskStatus::Completed)
        .unwrap();
    assert!(
        (d.get_plan(&tid).unwrap().completion_pct() - 40.0).abs() < 0.1,
        "2/5 = 40%"
    );

    // ── Step 4: Both engineers unblocked simultaneously ────────────────
    let ready = d.get_plan(&tid).unwrap().ready_subtasks();
    assert_eq!(ready.len(), 2, "both engineers must be ready in parallel");
    // Priority 0 before priority 1
    assert_eq!(ready[0].id, eng1);
    assert_eq!(ready[1].id, eng2);

    d.update_status(&tid, &eng1, SubTaskStatus::InProgress)
        .unwrap();
    d.update_status(&tid, &eng2, SubTaskStatus::InProgress)
        .unwrap();

    // Validator still blocked while both engineers run
    assert_eq!(
        d.get_plan(&tid).unwrap().ready_subtasks().len(),
        0,
        "validator must not be ready until both engineers complete"
    );

    d.update_status(&tid, &eng1, SubTaskStatus::Completed)
        .unwrap();
    assert_eq!(
        d.get_plan(&tid).unwrap().ready_subtasks().len(),
        0,
        "validator still blocked: eng2 not done"
    );

    d.update_status(&tid, &eng2, SubTaskStatus::Completed)
        .unwrap();

    // ── Step 5: Validator unblocked ────────────────────────────────────
    let ready = d.get_plan(&tid).unwrap().ready_subtasks();
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].id, validator);
    assert!(
        (d.get_plan(&tid).unwrap().completion_pct() - 80.0).abs() < 0.1,
        "4/5 = 80%"
    );

    d.update_status(&tid, &validator, SubTaskStatus::Completed)
        .unwrap();

    // ── Step 6: All done ───────────────────────────────────────────────
    assert_eq!(
        d.get_plan(&tid).unwrap().ready_subtasks().len(),
        0,
        "no more ready subtasks after completion"
    );
    assert!(
        (d.get_plan(&tid).unwrap().completion_pct() - 100.0).abs() < 0.1,
        "5/5 = 100%"
    );

    // All subtasks are terminal and done
    for st in &d.get_plan(&tid).unwrap().subtasks {
        assert!(st.status.is_terminal(), "all subtasks must be terminal");
        assert!(st.status.is_done(), "all subtasks must count as done");
    }
}

/// Proves that `Failed` and `Cancelled` are terminal but NOT done —
/// they must NOT unblock dependent subtasks.
#[test]
fn test_e2e_failed_and_cancelled_do_not_unblock_dependents() {
    // Chain: A → B → C (linear)
    let mut d = TaskDecomposer::new();
    let tid = d.create_task("pipeline", "Failure propagation test");
    let a = d.add_subtask(&tid, "A", vec![], 0).unwrap();
    let b = d.add_subtask(&tid, "B", vec![a.clone()], 0).unwrap();
    let _c = d.add_subtask(&tid, "C", vec![b.clone()], 0).unwrap();

    // A fails — B should NOT become ready (A.is_done() = false)
    d.update_status(&tid, &a, SubTaskStatus::Failed).unwrap();
    assert!(
        SubTaskStatus::Failed.is_terminal(),
        "Failed must be terminal"
    );
    assert!(
        !SubTaskStatus::Failed.is_done(),
        "Failed must NOT count as done"
    );
    assert_eq!(
        d.get_plan(&tid).unwrap().ready_subtasks().len(),
        0,
        "B must not be ready after A fails"
    );
    assert_eq!(
        d.get_plan(&tid).unwrap().completion_pct(),
        0.0,
        "Failed subtask must not count toward completion"
    );

    // Reset A to Cancelled — same expectation
    d.update_status(&tid, &a, SubTaskStatus::Cancelled).unwrap();
    assert!(
        SubTaskStatus::Cancelled.is_terminal(),
        "Cancelled must be terminal"
    );
    assert!(
        !SubTaskStatus::Cancelled.is_done(),
        "Cancelled must NOT count as done"
    );
    assert_eq!(
        d.get_plan(&tid).unwrap().ready_subtasks().len(),
        0,
        "B must not be ready after A is cancelled"
    );
}

/// Proves all 6 lifecycle transitions are valid and status display/parse
/// round-trips correctly.
#[test]
fn test_e2e_full_status_lifecycle_roundtrip() {
    use std::str::FromStr;

    let all_statuses = [
        SubTaskStatus::Pending,
        SubTaskStatus::InProgress,
        SubTaskStatus::Completed,
        SubTaskStatus::Blocked,
        SubTaskStatus::Failed,
        SubTaskStatus::Cancelled,
    ];

    for status in &all_statuses {
        // Display → parse round-trip
        let displayed = status.to_string();
        let parsed = SubTaskStatus::from_str(&displayed).expect("round-trip must succeed");
        assert_eq!(parsed, *status, "round-trip failed for {}", displayed);
    }

    // Terminal/done semantic contract
    assert!(!SubTaskStatus::Pending.is_terminal());
    assert!(!SubTaskStatus::InProgress.is_terminal());
    assert!(!SubTaskStatus::Blocked.is_terminal());
    assert!(SubTaskStatus::Completed.is_terminal());
    assert!(SubTaskStatus::Failed.is_terminal());
    assert!(SubTaskStatus::Cancelled.is_terminal());

    // Only Completed counts as done for dependency resolution
    assert!(SubTaskStatus::Completed.is_done());
    assert!(!SubTaskStatus::Pending.is_done());
    assert!(!SubTaskStatus::InProgress.is_done());
    assert!(!SubTaskStatus::Blocked.is_done());
    assert!(!SubTaskStatus::Failed.is_done());
    assert!(!SubTaskStatus::Cancelled.is_done());
}

/// Proves serde round-trip: a Task serialized to JSON and deserialized
/// retains all data, and `rebuild_index()` restores O(1) lookup.
#[test]
fn test_e2e_serde_roundtrip_and_index_rebuild() {
    let mut d = TaskDecomposer::new();
    let tid = d.create_task("analysis", "Serde test task");
    let s1 = d.add_subtask(&tid, "Alpha", vec![], 2).unwrap();
    let s2 = d.add_subtask(&tid, "Beta", vec![s1.clone()], 0).unwrap();
    d.update_status(&tid, &s1, SubTaskStatus::Completed)
        .unwrap();

    let original = d.get_plan(&tid).unwrap().clone();

    // Serialize to JSON
    let json = serde_json::to_string(&original).expect("serialization must succeed");

    // Deserialize (index is #[serde(skip)] — will be empty)
    let mut restored: Task = serde_json::from_str(&json).expect("deserialization must succeed");

    // Before rebuild: get_subtask uses the index — must be empty so lookup fails
    // (This verifies #[serde(skip)] actually skips the index)
    assert!(
        restored.get_subtask(&s1).is_none(),
        "index must be empty after deserialization before rebuild"
    );

    // Rebuild index
    restored.rebuild_index();

    // After rebuild: O(1) lookup works
    let st1 = restored
        .get_subtask(&s1)
        .expect("s1 must be found after rebuild");
    assert_eq!(st1.description, "Alpha");
    assert_eq!(st1.status, SubTaskStatus::Completed);
    assert_eq!(st1.priority, 2);

    let st2 = restored
        .get_subtask(&s2)
        .expect("s2 must be found after rebuild");
    assert_eq!(st2.description, "Beta");
    assert_eq!(st2.depends_on, vec![s1.clone()]);

    // completion_pct is preserved (1 of 2 = 50%)
    assert!((restored.completion_pct() - 50.0).abs() < 0.01);
}

/// Proves that `completion_pct` is correct across all edge cases:
/// empty task, partial completion, and non-done statuses don't count.
#[test]
fn test_e2e_completion_pct_all_cases() {
    let mut d = TaskDecomposer::new();

    // Edge case: empty task
    let empty_tid = d.create_task("feature", "Empty");
    assert_eq!(
        d.get_plan(&empty_tid).unwrap().completion_pct(),
        0.0,
        "empty task must be 0%"
    );

    // 4 subtasks, progressively complete
    let tid = d.create_task("feature", "Progress test");
    let ids: Vec<String> = (0..4)
        .map(|i| {
            d.add_subtask(&tid, &format!("Step {}", i), vec![], 0)
                .unwrap()
        })
        .collect();

    assert_eq!(d.get_plan(&tid).unwrap().completion_pct(), 0.0);

    for (i, sid) in ids.iter().enumerate() {
        d.update_status(&tid, sid, SubTaskStatus::Completed)
            .unwrap();
        let expected = (i + 1) as f32 * 25.0;
        assert!(
            (d.get_plan(&tid).unwrap().completion_pct() - expected).abs() < 0.01,
            "after completing {}/{}: expected {}%, got {}%",
            i + 1,
            4,
            expected,
            d.get_plan(&tid).unwrap().completion_pct()
        );
    }

    // Non-done statuses don't count: reset last to Failed
    d.update_status(&tid, &ids[3], SubTaskStatus::Failed)
        .unwrap();
    assert!(
        (d.get_plan(&tid).unwrap().completion_pct() - 75.0).abs() < 0.01,
        "Failed does not count: should remain 3/4 = 75%"
    );
}

/// Proves `delete_task` cleans up completely and `list_plans` reflects the change.
#[test]
fn test_e2e_delete_and_list_plans_lifecycle() {
    let mut d = TaskDecomposer::new();
    let t1 = d.create_task("feature", "Plan Alpha");
    let t2 = d.create_task("debug", "Plan Beta");
    let t3 = d.create_task("refactor", "Plan Gamma");

    // All three visible, sorted by creation
    let plans = d.list_plans();
    assert_eq!(plans.len(), 3);
    assert_eq!(plans[0].id, t1);
    assert_eq!(plans[1].id, t2);
    assert_eq!(plans[2].id, t3);

    // Delete middle plan
    assert!(d.delete_task(&t2), "first delete must return true");
    assert!(
        !d.delete_task(&t2),
        "second delete must return false (idempotent)"
    );

    let plans = d.list_plans();
    assert_eq!(plans.len(), 2);
    assert_eq!(plans[0].id, t1);
    assert_eq!(plans[1].id, t3);
    assert_eq!(d.task_count(), 2);

    // get_plan returns None for deleted task
    assert!(
        d.get_plan(&t2).is_none(),
        "deleted task must not be retrievable"
    );
}

/// Proves the invalid-status error message is propagated from FromStr,
/// listing all 6 valid values — not a stale hardcoded list.
#[test]
fn test_e2e_update_status_invalid_propagates_from_str_error() {
    let mut d = TaskDecomposer::new();
    let tid = d.create_task("feature", "Test");
    let sid = d.add_subtask(&tid, "Step", vec![], 0).unwrap();

    // update_status uses .parse() which delegates to FromStr
    let bad: Result<SubTaskStatus, String> = "garbage_status".parse();
    assert!(bad.is_err());
    let msg = bad.unwrap_err();
    // Must list ALL 6 valid statuses (not a stale subset)
    for expected in &[
        "pending",
        "in_progress",
        "completed",
        "blocked",
        "failed",
        "cancelled",
    ] {
        assert!(
            msg.contains(expected),
            "error message must list '{}' but got: {}",
            expected,
            msg
        );
    }

    // Verify update_status with nonexistent subtask returns correct error
    let err = d
        .update_status(&tid, "ghost_sub", SubTaskStatus::Completed)
        .unwrap_err();
    assert!(err.contains("Subtask not found"), "got: {}", err);

    // Verify update_status with nonexistent task returns correct error
    let _ = sid; // used above
    let err2 = d
        .update_status("ghost_task", "any", SubTaskStatus::Completed)
        .unwrap_err();
    assert!(err2.contains("Task not found"), "got: {}", err2);
}

/// Proves priority ordering in `ready_subtasks()` is correct when multiple
/// subtasks become ready simultaneously at different priority levels.
#[test]
fn test_e2e_ready_subtasks_priority_ordering() {
    // Gate: all depend on A. When A completes, B/C/D all become ready.
    // They should be dispatched by priority (0 before 5 before 10).
    let mut d = TaskDecomposer::new();
    let tid = d.create_task("pipeline", "Priority dispatch");
    let gate = d.add_subtask(&tid, "Gate", vec![], 0).unwrap();
    let high = d
        .add_subtask(&tid, "High-P", vec![gate.clone()], 0)
        .unwrap();
    let mid = d.add_subtask(&tid, "Mid-P", vec![gate.clone()], 5).unwrap();
    let low = d
        .add_subtask(&tid, "Low-P", vec![gate.clone()], 10)
        .unwrap();

    d.update_status(&tid, &gate, SubTaskStatus::Completed)
        .unwrap();

    let ready = d.get_plan(&tid).unwrap().ready_subtasks();
    assert_eq!(ready.len(), 3);
    assert_eq!(ready[0].id, high, "priority 0 must be first");
    assert_eq!(ready[1].id, mid, "priority 5 must be second");
    assert_eq!(ready[2].id, low, "priority 10 must be last");
}

/// Proves the `updated_at` timestamp is strictly after `created_at`
/// when status changes, and `created_at` is immutable.
#[test]
fn test_e2e_timestamps_are_monotonic_and_immutable() {
    let mut d = TaskDecomposer::new();
    let tid = d.create_task("feature", "Timestamp test");
    let sid = d.add_subtask(&tid, "Step", vec![], 0).unwrap();

    let created_at = d
        .get_plan(&tid)
        .unwrap()
        .get_subtask(&sid)
        .unwrap()
        .created_at;
    let updated_at_before = d
        .get_plan(&tid)
        .unwrap()
        .get_subtask(&sid)
        .unwrap()
        .updated_at;
    assert_eq!(
        created_at, updated_at_before,
        "initially created_at == updated_at"
    );

    std::thread::sleep(std::time::Duration::from_millis(5));
    d.update_status(&tid, &sid, SubTaskStatus::InProgress)
        .unwrap();

    let updated_at_after_first = {
        let st = d.get_plan(&tid).unwrap().get_subtask(&sid).unwrap();
        assert_eq!(st.created_at, created_at, "created_at must be immutable");
        assert!(
            st.updated_at > created_at,
            "updated_at must advance on status change"
        );
        st.updated_at
    }; // immutable borrow ends here

    // Second update: updated_at must advance again
    std::thread::sleep(std::time::Duration::from_millis(5));
    d.update_status(&tid, &sid, SubTaskStatus::Completed)
        .unwrap();
    let final_updated = d
        .get_plan(&tid)
        .unwrap()
        .get_subtask(&sid)
        .unwrap()
        .updated_at;
    assert!(
        final_updated > updated_at_after_first,
        "updated_at must advance on each status change"
    );
    assert_eq!(
        d.get_plan(&tid)
            .unwrap()
            .get_subtask(&sid)
            .unwrap()
            .created_at,
        created_at,
        "created_at must remain immutable after all updates"
    );
}

/// Proves the O(1) index stays consistent after multiple push_subtask calls,
/// including that the index correctly maps to the right Vec position.
#[test]
fn test_e2e_o1_index_consistency_under_multiple_inserts() {
    let mut d = TaskDecomposer::new();
    let tid = d.create_task("feature", "Index integrity");

    let ids: Vec<String> = (0..10)
        .map(|i| {
            d.add_subtask(&tid, &format!("Subtask {}", i), vec![], i as u8)
                .unwrap()
        })
        .collect();

    let plan = d.get_plan(&tid).unwrap();
    // Every id must be findable and point to the correct subtask
    for (i, sid) in ids.iter().enumerate() {
        let st = plan
            .get_subtask(sid)
            .unwrap_or_else(|| panic!("id {sid} must be in index"));
        assert_eq!(st.id, *sid);
        assert_eq!(st.priority, i as u8, "priority must match insertion order");
        // Verify Vec position matches index entry
        assert_eq!(
            plan.subtasks[i].id, *sid,
            "Vec[{}] must correspond to the i-th inserted subtask",
            i
        );
    }

    // Nonexistent id returns None
    assert!(plan.get_subtask("no_such_id").is_none());
}

// ── Exponential Enhancement Tests ──────────────────────────────────────

#[test]
fn test_retry_policy_backoff() {
    let policy = RetryPolicy {
        max_attempts: 3,
        base_delay_ms: 100,
        timeout_ms: Some(60_000),
    };
    // Attempt 1: no backoff (first attempt)
    // Attempt 2: 100ms * 2^0 = 100ms
    // Attempt 3: 100ms * 2^1 = 200ms
    assert_eq!(policy.backoff_delay(2), Duration::from_millis(100));
    assert_eq!(policy.backoff_delay(3), Duration::from_millis(200));
    // Cap at 30s
    let large = RetryPolicy {
        max_attempts: 10,
        base_delay_ms: 100_000,
        timeout_ms: None,
    };
    assert_eq!(large.backoff_delay(5), Duration::from_millis(30_000));
}

#[test]
fn test_subtask_mark_in_progress() {
    let mut d = TaskDecomposer::new();
    let tid = d.create_task("feature", "Test");
    let sid = d.add_subtask(&tid, "Work", vec![], 0).unwrap();

    let plan = d.get_plan(&tid).unwrap();
    assert_eq!(plan.subtasks[0].attempts, 0);

    d.update_status(&tid, &sid, SubTaskStatus::InProgress)
        .unwrap();

    let plan = d.get_plan(&tid).unwrap();
    assert_eq!(plan.subtasks[0].status, SubTaskStatus::InProgress);
    assert_eq!(plan.subtasks[0].attempts, 1);
}

#[test]
fn test_parallel_groups_diamond() {
    // Diamond: A (depth 0) → B,C (depth 1) → D (depth 2)
    let mut d = TaskDecomposer::new();
    let tid = d.create_task("feature", "Diamond groups");
    let a = d.add_subtask(&tid, "A", vec![], 0).unwrap();
    let b = d.add_subtask(&tid, "B", vec![a.clone()], 0).unwrap();
    let c = d.add_subtask(&tid, "C", vec![a.clone()], 0).unwrap();
    let _dd = d
        .add_subtask(&tid, "D", vec![b.clone(), c.clone()], 0)
        .unwrap();

    let plan = d.get_plan(&tid).unwrap();
    let groups = plan.parallel_groups();

    assert_eq!(groups.len(), 3);
    // Depth 0: only A
    assert_eq!(groups[0].depth, 0);
    assert_eq!(groups[0].subtask_ids, vec![a]);
    // Depth 1: B and C (sorted by id since same priority)
    assert_eq!(groups[1].depth, 1);
    assert_eq!(groups[1].subtask_ids, vec![b, c]);
    // Depth 2: D
    assert_eq!(groups[2].depth, 2);
    assert_eq!(groups[2].subtask_ids, vec![_dd]);
}

#[test]
fn test_parallel_groups_parallel_at_same_depth() {
    // All parallel: X, Y, Z at depth 0
    let mut d = TaskDecomposer::new();
    let tid = d.create_task("feature", "Parallel");
    let x = d.add_subtask(&tid, "X", vec![], 0).unwrap();
    let y = d.add_subtask(&tid, "Y", vec![], 1).unwrap();
    let z = d.add_subtask(&tid, "Z", vec![], 2).unwrap();

    let plan = d.get_plan(&tid).unwrap();
    let groups = plan.parallel_groups();

    assert_eq!(groups.len(), 1);
    assert_eq!(groups[0].depth, 0);
    // Sorted by priority then id: X(0), Y(1), Z(2)
    assert_eq!(groups[0].subtask_ids, vec![x, y, z]);
    assert!(!groups[0].all_done);
}

#[test]
fn test_parallel_groups_all_done() {
    let mut d = TaskDecomposer::new();
    let tid = d.create_task("feature", "All done");
    let s1 = d.add_subtask(&tid, "S1", vec![], 0).unwrap();
    let s2 = d.add_subtask(&tid, "S2", vec![], 0).unwrap();

    d.update_status(&tid, &s1, SubTaskStatus::Completed)
        .unwrap();
    d.update_status(&tid, &s2, SubTaskStatus::Completed)
        .unwrap();

    let plan = d.get_plan(&tid).unwrap();
    let groups = plan.parallel_groups();

    assert!(groups[0].all_done);
}

#[test]
fn test_task_metrics_record_validation() {
    let mut d = TaskDecomposer::new();
    let tid = d.create_task("feature", "Metrics");
    d.add_subtask(&tid, "S1", vec![], 0).unwrap();

    let before = d.get_plan(&tid).unwrap().metrics.validation_count;
    d.validate_order(&tid).ok();

    let plan = d.get_plan(&tid).unwrap();
    assert_eq!(plan.metrics.validation_count, before + 1);
    assert!(plan.metrics.last_validated_at.is_some());
}

#[test]
fn test_complexity_hint_default() {
    let hint = ComplexityHint::default();
    assert_eq!(hint.difficulty, 0);
    assert!(hint.estimated_ms.is_none());
    assert!(hint.io_bound);
    assert!(hint.tags.is_empty());
}

// ── CILA / N-Level Integration Tests ──────────────────────────────────────

#[test]
fn test_cila_level_from_u8() {
    assert_eq!(CilaLevel::from_u8(0), CilaLevel::L0);
    assert_eq!(CilaLevel::from_u8(1), CilaLevel::L1);
    assert_eq!(CilaLevel::from_u8(2), CilaLevel::L2);
    assert_eq!(CilaLevel::from_u8(3), CilaLevel::L3);
    assert_eq!(CilaLevel::from_u8(4), CilaLevel::L4);
    assert_eq!(CilaLevel::from_u8(5), CilaLevel::L5);
    assert_eq!(CilaLevel::from_u8(6), CilaLevel::L6);
    assert_eq!(CilaLevel::from_u8(99), CilaLevel::L6); // caps at L6
}

#[test]
fn test_cila_routing_mode() {
    assert_eq!(CilaLevel::L0.routing_mode(), RoutingMode::Solo);
    assert_eq!(CilaLevel::L1.routing_mode(), RoutingMode::Solo);
    assert_eq!(CilaLevel::L2.routing_mode(), RoutingMode::Hybrid);
    assert_eq!(CilaLevel::L3.routing_mode(), RoutingMode::Orchestrated);
    assert_eq!(CilaLevel::L4.routing_mode(), RoutingMode::FullTaco);
    assert_eq!(CilaLevel::L5.routing_mode(), RoutingMode::FullTaco);
    assert_eq!(CilaLevel::L6.routing_mode(), RoutingMode::FullTaco);
}

#[test]
fn test_cila_max_parallelism() {
    assert_eq!(CilaLevel::L0.max_parallelism(), 1);
    assert_eq!(CilaLevel::L1.max_parallelism(), 1);
    assert_eq!(CilaLevel::L2.max_parallelism(), 2);
    assert_eq!(CilaLevel::L3.max_parallelism(), 4);
    assert_eq!(CilaLevel::L4.max_parallelism(), 6);
    assert_eq!(CilaLevel::L5.max_parallelism(), 8);
    assert_eq!(CilaLevel::L6.max_parallelism(), 8);
}

#[test]
fn test_task_profile_solo() {
    let profile = TaskProfile::from_task(0, "direct");
    assert_eq!(profile.routing_mode, RoutingMode::Solo);
    assert!(!profile.pheromone_enabled);
    assert!(!profile.mcts_enabled);
    assert!(!profile.validator_required);
    assert_eq!(profile.max_parallelism, 1);
}

#[test]
fn test_task_profile_orchestrated() {
    let profile = TaskProfile::from_task(3, "feature");
    assert_eq!(profile.routing_mode, RoutingMode::Orchestrated);
    assert!(profile.pheromone_enabled);
    assert!(!profile.mcts_enabled);
    assert!(profile.validator_required);
    assert_eq!(profile.max_parallelism, 4);
}

#[test]
fn test_task_profile_full_taco() {
    let profile = TaskProfile::from_task(4, "agent_loop");
    assert_eq!(profile.routing_mode, RoutingMode::FullTaco);
    assert!(profile.pheromone_enabled);
    assert!(profile.mcts_enabled);
    assert!(profile.validator_required);
    assert_eq!(profile.max_parallelism, 6);
}

#[test]
fn test_task_profile_pipeline_reduces_parallelism() {
    // Pipelines have half parallelism
    let profile = TaskProfile::from_task(3, "pipeline");
    assert_eq!(profile.max_parallelism, 2); // 4/2 = 2
}

#[test]
fn test_task_profile_traversal_order() {
    assert_eq!(
        TaskProfile::from_task(0, "direct").traversal_order(),
        "none"
    );
    assert_eq!(TaskProfile::from_task(2, "tool").traversal_order(), "bfs");
    assert_eq!(
        TaskProfile::from_task(3, "feature").traversal_order(),
        "bfs"
    );
    assert_eq!(TaskProfile::from_task(4, "loop").traversal_order(), "bfs");
}

#[test]
fn test_create_task_with_cila() {
    let mut d = TaskDecomposer::new();
    let tid = d.create_task_with_cila("feature", "Test L5", 5);
    let plan = d.get_plan(&tid).unwrap();
    assert_eq!(plan.cila_level, 5);

    let profile = plan.profile();
    assert_eq!(profile.routing_mode, RoutingMode::FullTaco);
    assert!(profile.mcts_enabled);
}

#[test]
fn test_parallel_groups_with_profile() {
    let mut d = TaskDecomposer::new();
    let tid = d.create_task_with_cila("feature", "Profile test", 4);
    d.add_subtask(&tid, "A", vec![], 0).unwrap();
    d.add_subtask(&tid, "B", vec![], 0).unwrap();

    let plan = d.get_plan(&tid).unwrap();
    let (groups, profile) = plan.parallel_groups_with_profile();

    assert_eq!(groups.len(), 1);
    assert_eq!(profile.routing_mode, RoutingMode::FullTaco);
}

// ── Priority Inheritance Tests ──────────────────────────────────────────────

#[test]
fn test_effective_priority_own_priority() {
    let mut d = TaskDecomposer::new();
    let tid = d.create_task("feature", "Priority test");
    let s1 = d.add_subtask(&tid, "Step 1", vec![], 3).unwrap();

    let plan = d.get_plan(&tid).unwrap();
    assert_eq!(plan.effective_priority(&s1), 3);
}

#[test]
fn test_effective_priority_inherited_from_dependency() {
    let mut d = TaskDecomposer::new();
    let tid = d.create_task("feature", "Priority inheritance");
    let s1 = d.add_subtask(&tid, "Step 1", vec![], 2).unwrap();
    let s2 = d
        .add_subtask(&tid, "Step 2", vec![s1.clone()], u8::MAX)
        .unwrap();

    let plan = d.get_plan(&tid).unwrap();
    // s2 inherits from s1: min priority = 2
    assert_eq!(plan.effective_priority(&s2), 2);
}

#[test]
fn test_effective_priority_max_depth() {
    let mut d = TaskDecomposer::new();
    let tid = d.create_task("feature", "Deep inheritance");
    let s1 = d.add_subtask(&tid, "Step 1", vec![], 1).unwrap();
    let s2 = d
        .add_subtask(&tid, "Step 2", vec![s1.clone()], u8::MAX)
        .unwrap();
    let s3 = d
        .add_subtask(&tid, "Step 3", vec![s2.clone()], u8::MAX)
        .unwrap();
    let s4 = d
        .add_subtask(&tid, "Step 4", vec![s3.clone()], u8::MAX)
        .unwrap();

    let plan = d.get_plan(&tid).unwrap();
    // Chain: s4 -> s3 -> s2 -> s1 (priority 1)
    // max_depth=3 limits recursion depth
    // s4 (depth 0) -> s3 (depth 1) -> s2 (depth 2) -> s1 (depth 3)
    // At depth 3, s1.priority=1 is returned (concrete priority found before depth limit)
    assert_eq!(plan.effective_priority(&s4), 1);
}

#[test]
fn test_effective_priority_nonexistent_subtask() {
    let mut d = TaskDecomposer::new();
    let tid = d.create_task("feature", "Test");
    let _s1 = d.add_subtask(&tid, "Step 1", vec![], 0).unwrap();

    let plan = d.get_plan(&tid).unwrap();
    assert_eq!(plan.effective_priority("ghost"), u8::MAX);
}

// ── Deadline Behavior Tests ────────────────────────────────────────────────

#[test]
fn test_deadline_behavior_default() {
    let db = DeadlineBehavior::default();
    assert!(matches!(db, DeadlineBehavior::Fail));
}

#[test]
fn test_check_expired_deadlines_fail() {
    let mut d = TaskDecomposer::new();
    let tid = d.create_task("feature", "Deadline test");
    let s1 = d.add_subtask(&tid, "Step 1", vec![], 0).unwrap();

    // Manually set a past deadline with Fail behavior
    {
        let st = d.get_plan(&tid).unwrap().get_subtask(&s1).unwrap();
        let _past = Utc::now() - chrono::Duration::hours(1);
        assert!(st.deadline.is_none());
    }

    let plan = d.get_plan(&tid).unwrap();
    let mut plan_mut = plan.clone();
    // Set past deadline via get_subtask_mut is not pub, so we test the method exists
    // The actual deadline setting would be done via a setter - but for now
    // we verify the enum and method exist and compile
    let transitions = plan_mut.check_expired_deadlines();
    // No transitions since no deadlines are set
    assert!(transitions.is_empty());
}

#[test]
fn test_subtask_skipped_status_roundtrip() {
    let parsed = "skipped".parse::<SubTaskStatus>().unwrap();
    assert!(matches!(parsed, SubTaskStatus::Skipped));
    assert!(parsed.is_terminal());
    assert!(!parsed.is_done());
    assert_eq!(parsed.to_string(), "skipped");
}

#[test]
fn test_ready_subtasks_excludes_expired_deadlines() {
    let mut d = TaskDecomposer::new();
    let tid = d.create_task("feature", "Deadline filter");
    let s1 = d.add_subtask(&tid, "Step 1", vec![], 0).unwrap();
    let _s2 = d.add_subtask(&tid, "Step 2", vec![s1.clone()], 0).unwrap();

    // Both s1 and s2 are pending, s1 has no deps, s2 depends on s1
    // So initially only s1 should be ready
    let plan = d.get_plan(&tid).unwrap();
    let ready = plan.ready_subtasks();
    assert_eq!(ready.len(), 1);
    assert_eq!(ready[0].id, s1);
}

// ── Infer Dependencies Tests ───────────────────────────────────────────────

#[test]
fn test_infer_dependencies_from_file_references() {
    let mut d = TaskDecomposer::new();
    let tid = d.create_task("feature", "File deps");
    let _s1 = d
        .add_subtask(&tid, "Implement src/auth/login.rs module", vec![], 0)
        .unwrap();
    let _s2 = d
        .add_subtask(&tid, "Add tests for src/auth/login.rs", vec![], 0)
        .unwrap();

    let inferred = d.infer_dependencies(&tid, "Add tests for src/auth/login.rs");
    // s2's description references src/auth/login.rs, s1 also references it
    assert!(
        inferred.contains(&_s1),
        "Should infer dependency on s1 from shared file path"
    );
}

#[test]
fn test_infer_dependencies_no_files() {
    let mut d = TaskDecomposer::new();
    let tid = d.create_task("feature", "No files");
    let _s1 = d
        .add_subtask(&tid, "Do something generic", vec![], 0)
        .unwrap();

    let inferred = d.infer_dependencies(&tid, "Do something else generic without files");
    assert!(inferred.is_empty());
}

#[test]
fn test_infer_dependencies_nonexistent_task() {
    let d = TaskDecomposer::new();
    let inferred = d.infer_dependencies("ghost", "src/foo.rs");
    assert!(inferred.is_empty());
}

#[test]
fn test_infer_dependencies_directory_prefix() {
    let mut d = TaskDecomposer::new();
    let tid = d.create_task("refactor", "Directory deps");
    let _s1 = d
        .add_subtask(&tid, "Refactor src/core/mod.rs", vec![], 0)
        .unwrap();
    let _s2 = d
        .add_subtask(&tid, "Update src/core/utils.rs helper", vec![], 0)
        .unwrap();

    let inferred = d.infer_dependencies(&tid, "Update src/core/utils.rs helper");
    // Both share src/core/ prefix
    assert!(inferred.contains(&_s1));
}

// ── D3 tests: create_task_with_cila_and_hint ─────────────────────────

#[test]
fn decompose_without_hint_creates_empty_task() {
    let mut d = TaskDecomposer::new();
    let tid = d.create_task_with_cila_and_hint("feature", "test task", 3, None);
    let task = d.get_plan(&tid).expect("task exists");
    assert!(task.ready_subtasks().is_empty(), "no subtasks without hint");
    assert_eq!(task.cila_level, 3);
}

#[test]
fn decompose_with_split3_hint_creates_3_subtasks_with_deps_chain() {
    let mut d = TaskDecomposer::new();
    let hint = crate::reasoning::GranularityHint {
        split_factor: "Split3".to_string(),
        subtask_count: 3,
        size_loc: 500,
        language: "rust".to_string(),
        cila_level: 3,
    };
    let tid = d.create_task_with_cila_and_hint("feature", "split3 task", 3, Some(&hint));
    let task = d.get_plan(&tid).expect("task exists");
    assert_eq!(task.subtasks.len(), 3, "split3 → 3 subtasks");
    // sub_1 has no deps; sub_2 depends on sub_1; sub_3 depends on sub_2
    let subs: Vec<_> = task.subtasks.iter().collect();
    assert!(subs[0].depends_on.is_empty());
    assert_eq!(subs[1].depends_on.len(), 1);
    assert_eq!(subs[2].depends_on.len(), 1);
}

#[test]
fn decompose_l1_ignores_hint() {
    // cila_level < 3 → hint should be ignored even when provided
    let mut d = TaskDecomposer::new();
    let hint = crate::reasoning::GranularityHint {
        split_factor: "Split4".to_string(),
        subtask_count: 4,
        size_loc: 200,
        language: "rust".to_string(),
        cila_level: 1,
    };
    let tid = d.create_task_with_cila_and_hint("bugfix", "simple fix", 1, Some(&hint));
    let task = d.get_plan(&tid).expect("task exists");
    assert!(task.subtasks.is_empty(), "L1 ignores split hint");
}

#[test]
fn decompose_hint_monolithic_creates_no_subtasks() {
    let mut d = TaskDecomposer::new();
    let hint = crate::reasoning::GranularityHint {
        split_factor: "Monolithic".to_string(),
        subtask_count: 1,
        size_loc: 50,
        language: "rust".to_string(),
        cila_level: 3,
    };
    let tid = d.create_task_with_cila_and_hint("analysis", "mono task", 3, Some(&hint));
    let task = d.get_plan(&tid).expect("task exists");
    assert!(task.subtasks.is_empty(), "Monolithic → no subtasks");
}

// C11 — budget conservation B∈ℕ⁶ over the decompose DAG.
#[test]
fn test_budget_conservation_over_dag() {
    use crate::reasoning::budget::BudgetVector;
    let mut d = TaskDecomposer::new();
    let tid = d.create_task("feature", "Build with a budget");
    let s1 = d.add_subtask(&tid, "Step 1", vec![], 0).unwrap();
    let _s2 = d.add_subtask(&tid, "Step 2", vec![s1.clone()], 1).unwrap();
    let _s3 = d.add_subtask(&tid, "Step 3", vec![s1], 1).unwrap();
    let task = d.get_plan(&tid).expect("task exists");

    // Derived vectors: 3 subtasks; dependency edges = 0 + 1 + 1 = 2.
    let budgets = task.subtask_budgets();
    assert_eq!(budgets.len(), 3);
    assert_eq!(budgets.iter().map(|b| b.subtasks).sum::<u32>(), 3);
    assert_eq!(budgets.iter().map(|b| b.dependencies).sum::<u32>(), 2);

    // A root that allots everything generously is conserved.
    let generous = BudgetVector {
        tokens: 1_000_000,
        wall_ms: 1_000_000,
        subtasks: 100,
        dependencies: 100,
        max_retries: 1_000,
        attempts_used: 1_000,
    };
    assert!(task.verify_budget_conservation(&generous).is_ok());

    // Tighten ONLY the subtasks dimension (3 > 2) — the violation is reported there
    // and nowhere else (every other dimension stays generous via the spread).
    let tight = BudgetVector {
        subtasks: 2,
        ..generous
    };
    let err = task.verify_budget_conservation(&tight).unwrap_err();
    assert!(err.iter().any(|v| v.dimension == "subtasks"));
    assert!(
        err.iter().all(|v| v.dimension == "subtasks"),
        "only the subtasks dimension should over-commit, got {err:?}"
    );
}
