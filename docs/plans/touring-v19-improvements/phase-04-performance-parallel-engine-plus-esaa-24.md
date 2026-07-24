---
plan_id: touring-v19-improvements
phase: 4
title: 'Performance: Parallel Engine + ESAA 24'
status: planned
created: '2026-03-25'
horizon: H2
insights_covered: [INS-7, INS-8]
depends_on_phases:
- 0
- 2
validates_with: validate_phase04.py
estimated_effort: 2.5 weeks
vgp_verified_structs: [GeneratorGraphModel, MutableGeneratorGraph]
---

# Performance: Parallel Engine + ESAA 24

> **Depends on**: Phase 0, 2
> **Insights**: INS-7 (ROI=1.33), INS-8 (ROI=1.13)

## Objective

Implement INS-7 (Parallel Generator Engine, ROI=1.33) and INS-8 (ESAA 24 subsystems, ROI=1.13). Parallel engine needs INS-4 (topological_sort_deterministic). ESAA needs PhaseRegistry from INS-3.

## Final Result

MutableGeneratorGraph with parallel execution. ESAA with 24 subsystems + EsaaCoordinator + routing. 28 new tests.

## Impacts

| Dimension | Assessment |
|-----------|------------|
| performance | Rayon parallelism reduces execution time for independent nodes. Speedup = parallelizable_groups.len(). |
| maintainability | ESAA 24 subsystems makes the event-driven architecture extensible via plugin pattern. |
| reliability | Parallel errors collected cleanly — no partial state corruption. |
| daemon_latency | No daemon changes. |

## Insight INS-7: Parallel Generator Engine

**Crate**: `touring-learning` | **Module**: `src/aco/graph.rs`
**Action**: MODIFY | **Priority**: Q2 | **Effort**: 3 days | **ROI**: 1.33

### Description

Add parallel execution engine using Rayon for parallelizable node groups in MutableGeneratorGraph. GeneratorGraphModel.parallelizable already identifies parallel groups — this adds execution using rayon::par_iter for those groups.

### Implementation

```
Modify src/aco/graph.rs:
  Add rayon = "1" to touring-learning Cargo.toml
  Add pub fn execute_parallel_nodes<F>(
      &self,
      parallel_groups: &[Vec<String>],
      execute_fn: F,
  ) -> Vec<(String, Result<(), String>)>
  where F: Fn(&str) -> Result<(), String> + Send + Sync
    → uses rayon::par_iter on each group
    → collects (node_id, result) for all nodes in group
    → sequential between groups (group N+1 only after group N completes)
  Add pub fn parallelizable_groups(&self) -> Vec<Vec<String>>
    → detects nodes with no mutual dependency via adjacency check
    → returns groups as Vec<Vec<String>> in topological order

```

### VGP-Verified Structs

**MutableGeneratorGraph** (crates/touring-learning/src/aco/graph.rs line 33):
- `graph: DiGraph<String, ()>`
- `index_map: HashMap<String, NodeIndex>`
- `nodes: BTreeMap<String, GeneratorNode>`
- `execution_status: HashMap<String, ExecutionStatus>`
- `dirty: bool`
**GeneratorGraphModel** (crates/touring-learning/src/aco/models.rs line 219):
- `nodes: Vec<GeneratorNode>`
- `critical_path: Vec<String>`
- `parallelizable: Vec<Vec<String>>`
- `objective_hash: String`

### Test Cases

- `test_parallel_execute_independent_nodes`
- `test_parallel_groups_sequential_constraint`
- `test_parallelizable_groups_detection`
- `test_parallel_execute_collects_errors`

### Synergies

- TOPO-1: uses topological_sort_deterministic for group ordering

## Insight INS-8: ESAA Complete 24 Subsystems

**Crate**: `touring-learning` | **Module**: `src/aco/esaa.rs`
**Action**: MODIFY | **Priority**: Q2 | **Effort**: 1.5 weeks | **ROI**: 1.13

### Description

Expand ESAA from current 2 subsystems (QueryCache + EventBuffer) to the full 24 subsystems identified in the analise project: router, planner, executor, validator, monitor, coordinator, scheduler, notifier, aggregator, transformer, filter, dispatcher, collector, analyzer, reporter, auditor, archiver, indexer, searcher, learner, predictor, optimizer, balancer, observer.

### Implementation

```
Modify src/aco/esaa.rs:
  Add trait EsaaSubsystem: Send + Sync {
      fn subsystem_id(&self) -> &str;
      fn process(&self, input: &EsaaInput) -> EsaaOutput;
      fn health_check(&self) -> bool;
  }
  Add struct EsaaInput { pub event_type: String, pub payload: Vec<u8>, pub metadata: HashMap<String,String> }
  Add struct EsaaOutput { pub success: bool, pub result: Vec<u8>, pub latency_us: u64 }
  Add struct EsaaCoordinator {
      subsystems: IndexMap<String, Box<dyn EsaaSubsystem>>,
      routing_table: HashMap<String, Vec<String>>,  // event_type -> [subsystem_ids]
  }
  impl EsaaCoordinator:
    pub fn new() -> Self
    pub fn register(&mut self, subsystem: Box<dyn EsaaSubsystem>)
    pub fn route(&self, input: EsaaInput) -> Vec<EsaaOutput>
    pub fn health_report(&self) -> HashMap<String, bool>
  Implement 24 subsystem structs (each: minimal impl satisfying trait):
    Router, Planner, Executor, Validator, Monitor, Coordinator,
    Scheduler, Notifier, Aggregator, Transformer, Filter, Dispatcher,
    Collector, Analyzer, Reporter, Auditor, Archiver, Indexer,
    Searcher, Learner, Predictor, Optimizer, Balancer, Observer

```

### VGP-Verified Structs

No existing structs referenced — all new types.

### Test Cases

- `test_esaa_coordinator_register_all_24`
- `test_esaa_router_routes_correctly`
- `test_esaa_planner_processes_input`
- `test_esaa_executor_runs`
- `test_esaa_validator_validates`
- `test_esaa_monitor_health_check`
- `test_esaa_coordinator_route_dispatches`
- `test_esaa_health_report_all_healthy`
- `test_esaa_scheduler_schedule`
- `test_esaa_notifier_notify`
- `test_esaa_aggregator_aggregate`
- `test_esaa_transformer_transform`
- `test_esaa_filter_filter`
- `test_esaa_dispatcher_dispatch`
- `test_esaa_collector_collect`
- `test_esaa_analyzer_analyze`
- `test_esaa_reporter_report`
- `test_esaa_auditor_audit`
- `test_esaa_archiver_archive`
- `test_esaa_indexer_index`
- `test_esaa_searcher_search`
- `test_esaa_learner_learn`
- `test_esaa_predictor_predict`
- `test_esaa_balancer_balance`

### Synergies

- PhaseRegistry: each subsystem registered as PhaseHandler

## Subtasks

- [ ]S4.1: [INS-7] Write 4 tests for parallel execution (TDD first)
- [ ]S4.2: [INS-7] Add rayon = '1' to touring-learning Cargo.toml (after S4.1)
- [ ]S4.3: [INS-7] Implement execute_parallel_nodes() in graph.rs (after S4.2)
- [ ]S4.4: [INS-7] Implement parallelizable_groups() in graph.rs (after S4.3)
- [ ]S4.5: [INS-8] Write 24 tests for ESAA subsystems (TDD first — 1 per subsystem)
- [ ]S4.6: [INS-8] Add EsaaSubsystem trait + EsaaInput/EsaaOutput + EsaaCoordinator (after S4.5)
- [ ]S4.7: [INS-8] Implement all 24 subsystem structs (after S4.6)
- [ ]S4.8: [INS-8] Implement routing_table and EsaaCoordinator.route() (after S4.7)
- [ ]S4.9: cargo clippy -p touring-learning -- -D warnings (after S4.4, S4.8)
- [ ]S4.10: cargo test -p touring-learning → expect +28 more tests (after S4.9)

## DISCOVER Protocol

```
touring_ast_find(symbol_name='GeneratorGraphModel', definitions_only=true) → parallelizable field
touring_ast_overview(file_path='crates/touring-learning/src/aco/esaa.rs')
touring_graph(action='blast_radius', symbol='MutableGeneratorGraph') → rayon impact
touring_memory_recall(query='rayon parallel esaa subsystem coordinator', top_k=5)
```

## TDD Plan

Tests First (28 total):
  - graph.rs: 4 tests for parallel execution
  - esaa.rs: 24 tests (1 per subsystem + coordinator)
Implementation:
  1. Cargo.toml: rayon dependency
  2. graph.rs: execute_parallel_nodes + parallelizable_groups (INS-7)
  3. esaa.rs: EsaaSubsystem trait + 24 impls + EsaaCoordinator (INS-8)
E2E:
  Build 6-node DAG with 2 parallelizable groups → execute_parallel_nodes → verify all succeed
  Register all 24 subsystems → route event → health_report → all healthy


## Checkpoint (.toon)

```python
# Save: {'insights': ['INS-7', 'INS-8'], 'new_tests': 28, 'subsystems_implemented': 24}
# File: checkpoints/phase-04-performance-esaa.toon
import msgpack, pathlib
pathlib.Path('checkpoints/phase-04-performance-esaa.toon').write_bytes(msgpack.packb({'insights': ['INS-7', 'INS-8'], 'new_tests': 28, 'subsystems_implemented': 24}))
```

## Validation Criteria

- [ ]execute_parallel_nodes exists in graph.rs using rayon::par_iter
- [ ]parallelizable_groups() correctly identifies independent node groups
- [ ]EsaaSubsystem trait exists with process() and health_check()
- [ ]EsaaCoordinator has all 24 subsystems registered
- [ ]cargo clippy -p touring-learning → 0 warnings
- [ ]cargo test -p touring-learning → 28+ new tests

## Dependencies: phase-00, phase-02
