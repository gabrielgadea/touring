---
plan_id: touring-v19-improvements
phase: 3
title: 'Persistence Patterns: CQRS + Saga'
status: planned
created: '2026-03-25'
horizon: H2
insights_covered: [INS-5, INS-6]
depends_on_phases:
- 0
- 1
- 2
validates_with: validate_phase03.py
estimated_effort: 1 week
vgp_verified_structs: [MutableGeneratorGraph]
---

# Persistence Patterns: CQRS + Saga

> **Depends on**: Phase 0, 1, 2
> **Insights**: INS-5 (ROI=1.40), INS-6 (ROI=1.40)

## Objective

Implement INS-5 (CQRS Read Model, ROI=1.40) and INS-6 (Saga Pattern, ROI=1.40). CQRS first as Saga depends on clean read/write separation. Both are independent of phase 1-2 outputs but benefit from GraphMetrics (phase 2).

## Final Result

New read_model.rs with AcoReadModel. New saga.rs with SagaOrchestrator. 11 new tests.

## Impacts

| Dimension | Assessment |
|-----------|------------|
| performance | CQRS cache avoids repeated graph traversal on reads. Amortized O(1) status lookup. |
| maintainability | Saga makes multi-step rollback declarative and testable. |
| reliability | CQRS version prevents stale reads. Saga ensures compensation on failure. |
| daemon_latency | No daemon changes. Both are caller-side patterns. |

## Insight INS-5: CQRS Read Model

**Crate**: `touring-learning` | **Module**: `src/aco/read_model.rs`
**Action**: NEW | **Priority**: Q2 | **Effort**: 2 days | **ROI**: 1.40

### Description

Implement CQRS read model for ACO: separate read projections from write commands. AcoReadModel caches ExecutionStatus per node with version-based cache invalidation. Commands (execute, rollback) update write side; reads go through cached projection.

### Implementation

```
New file src/aco/read_model.rs:
  pub struct AcoSnapshot { pub status: HashMap<String, ExecutionStatus>, pub version: u64 }
  pub struct AcoReadModel {
      snapshot: AcoSnapshot,
      cache_valid: bool,
      rebuild_count: u64,
  }
  impl AcoReadModel:
    pub fn new() -> Self
    pub fn apply_execution(&mut self, node_id: &str, status: ExecutionStatus)
      → updates snapshot.status, increments version, sets cache_valid=true
    pub fn invalidate(&mut self)
      → cache_valid = false, increments rebuild_count
    pub fn get_status(&self, node_id: &str) -> Option<&ExecutionStatus>
      → returns None if !cache_valid (caller must rebuild)
    pub fn rebuild_from_graph(&mut self, graph: &MutableGeneratorGraph)
      → iterates graph.iter(), populates snapshot from execution_status
    pub fn stats(&self) -> (u64, u64)
      → (version, rebuild_count)

```

### VGP-Verified Structs

**MutableGeneratorGraph** (crates/touring-learning/src/aco/graph.rs line 33):
- `graph: DiGraph<String, ()>`
- `index_map: HashMap<String, NodeIndex>`
- `nodes: BTreeMap<String, GeneratorNode>`
- `execution_status: HashMap<String, ExecutionStatus>`
- `dirty: bool`

### Test Cases

- `test_read_model_apply_and_get`
- `test_read_model_invalidate_clears_access`
- `test_read_model_rebuild_from_graph`
- `test_read_model_version_increments`
- `test_read_model_rebuild_count_tracked`

### Synergies

- TimeTravelDebugger: snapshots the read model at each epoch

## Insight INS-6: Saga Pattern with Compensating Transactions

**Crate**: `touring-learning` | **Module**: `src/aco/saga.rs`
**Action**: NEW | **Priority**: Q2 | **Effort**: 3 days | **ROI**: 1.40

### Description

Implement Saga pattern for multi-step ACO operations with compensating transactions. Each step has execute + compensate. On failure, all previous steps are rolled back in reverse order. Enables safe multi-step phase execution.

### Implementation

```
New file src/aco/saga.rs:
  pub struct SagaStep {
      pub name: String,
      pub execute: Box<dyn Fn() -> Result<(), String> + Send + Sync>,
      pub compensate: Box<dyn Fn() -> Result<(), String> + Send + Sync>,
  }
  pub enum SagaOutcome { Completed, RolledBack { failed_step: String, reason: String } }
  pub struct SagaOrchestrator {
      steps: Vec<SagaStep>,
      history: Vec<String>,   // executed step names (for rollback tracking)
  }
  impl SagaOrchestrator:
    pub fn new() -> Self
    pub fn add_step(&mut self, step: SagaStep)
    pub fn run(&mut self) -> SagaOutcome
      → executes steps in order; on Err: calls compensate in reverse, returns RolledBack
    pub fn history(&self) -> &[String]

```

### VGP-Verified Structs

No existing structs referenced — all new types.

### Test Cases

- `test_saga_all_steps_succeed`
- `test_saga_rollback_on_step_failure`
- `test_saga_compensation_order_reversed`
- `test_saga_history_records_executed_steps`
- `test_saga_empty_steps_completes`
- `test_saga_first_step_failure_no_compensation`

### Synergies

- PhaseRegistry: each phase wrapped in SagaStep for safe rollback

## Subtasks

- [ ]S3.1: [INS-5] Write 5 tests for AcoReadModel (TDD first)
- [ ]S3.2: [INS-5] Create src/aco/read_model.rs with AcoSnapshot + AcoReadModel (after S3.1)
- [ ]S3.3: [INS-5] Register in mod.rs: pub mod read_model (after S3.2)
- [ ]S3.4: [INS-6] Write 6 tests for SagaOrchestrator (TDD first)
- [ ]S3.5: [INS-6] Create src/aco/saga.rs with SagaStep + SagaOutcome + SagaOrchestrator (after S3.4)
- [ ]S3.6: [INS-6] Register in mod.rs: pub mod saga (after S3.5)
- [ ]S3.7: cargo clippy -p touring-learning -- -D warnings (after S3.3, S3.6)
- [ ]S3.8: cargo test -p touring-learning → expect +11 more tests (after S3.7)

## DISCOVER Protocol

```
touring_ast_find(symbol_name='MutableGeneratorGraph', definitions_only=true) → iter() method
touring_ast_find(symbol_name='ExecutionStatus', definitions_only=true) → variants
touring_graph(action='blast_radius', symbol='ExecutionStatus') → impact
touring_memory_recall(query='CQRS read model saga compensating transaction rust', top_k=5)
```

## TDD Plan

Tests First (11 total):
  - read_model.rs: 5 tests before AcoReadModel
  - saga.rs: 6 tests before SagaOrchestrator
Implementation:
  1. read_model.rs (INS-5)
  2. saga.rs (INS-6)
  3. mod.rs registrations
E2E:
  CQRS: execute 3 nodes → read model → invalidate → rebuild → compare
  Saga: 3-step saga with step 2 failing → verify compensation called for step 1 in reverse


## Checkpoint (.toon)

```python
# Save: {'insights': ['INS-5', 'INS-6'], 'new_tests': 11, 'new_files': ['read_model.rs', 'saga.rs']}
# File: checkpoints/phase-03-persistence-patterns.toon
import msgpack, pathlib
pathlib.Path('checkpoints/phase-03-persistence-patterns.toon').write_bytes(msgpack.packb({'insights': ['INS-5', 'INS-6'], 'new_tests': 11, 'new_files': ['read_model.rs', 'saga.rs']}))
```

## Validation Criteria

- [ ]src/aco/read_model.rs exists with AcoReadModel (apply_execution, invalidate, get_status, rebuild_from_graph)
- [ ]src/aco/saga.rs exists with SagaOrchestrator (add_step, run, history)
- [ ]SagaOutcome::RolledBack includes failed_step and reason
- [ ]cargo clippy -p touring-learning → 0 warnings
- [ ]cargo test -p touring-learning → 11+ new tests

## Dependencies: phase-00, phase-01, phase-02
