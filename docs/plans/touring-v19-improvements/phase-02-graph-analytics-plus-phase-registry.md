---
plan_id: touring-v19-improvements
phase: 2
title: 'Graph Analytics + Phase Registry'
status: planned
created: '2026-03-25'
horizon: H1
insights_covered: [INS-3, INS-4]
depends_on_phases:
- 0
validates_with: validate_phase02.py
estimated_effort: 1 week
vgp_verified_structs: [GeneratorGraphModel, MutableGeneratorGraph]
---

# Graph Analytics + Phase Registry

> **Depends on**: Phase 0
> **Insights**: INS-3 (ROI=1.50), INS-4 (ROI=1.67)

## Objective

Implement INS-4 (Deterministic Topological Sort + GraphMetrics, ROI=1.67) and INS-3 (Plugin/Phase Registry, ROI=1.50). Graph analytics first as PhaseRegistry depends on stable MutableGeneratorGraph API.

## Final Result

MutableGeneratorGraph extended with topological_sort_deterministic, compute_graph_metrics, execute_parallel_nodes. New registry.rs with PhaseRegistry. 11 new tests.

## Impacts

| Dimension | Assessment |
|-----------|------------|
| performance | Deterministic sort eliminates non-deterministic iteration. BTreeMap = O(n log n). |
| maintainability | GraphMetrics makes graph health observable. Registry decouples phases from core. |
| reliability | Stable sort prevents flaky tests that depend on HashMap ordering. |
| daemon_latency | No daemon changes. |

## Insight INS-3: Plugin/Phase Registry Dynamic

**Crate**: `touring-learning` | **Module**: `src/aco/registry.rs`
**Action**: NEW | **Priority**: Q1 | **Effort**: 2 days | **ROI**: 1.50

### Description

Create PhaseRegistry that allows phases to be registered at runtime rather than hardcoded. Current ACO has fixed phase list. Registry enables plugin-style extension: new phases can be added without modifying core graph.rs.

### Implementation

```
New file src/aco/registry.rs:
  pub trait PhaseHandler: Send + Sync {
      fn phase_id(&self) -> &str;
      fn execute(&self, graph: &mut MutableGeneratorGraph) -> Result<(), GraphError>;
      fn rollback(&self, graph: &mut MutableGeneratorGraph) -> Result<(), GraphError>;
  }
  pub struct PhaseRegistry {
      phases: IndexMap<String, Box<dyn PhaseHandler>>,
  }
  impl PhaseRegistry:
    pub fn new() -> Self
    pub fn register<H: PhaseHandler + 'static>(&mut self, handler: H) -> Result<()>
      → returns Err if phase_id already registered
    pub fn execute_phase(&self, id: &str, graph: &mut MutableGeneratorGraph) -> Result<()>
    pub fn rollback_phase(&self, id: &str, graph: &mut MutableGeneratorGraph) -> Result<()>
    pub fn phase_ids(&self) -> Vec<&str>
    pub fn len(&self) -> usize
    pub fn is_empty(&self) -> bool
Add indexmap = "2" to touring-learning Cargo.toml

```

### VGP-Verified Structs

**MutableGeneratorGraph** (crates/touring-learning/src/aco/graph.rs line 33):
- `graph: DiGraph<String, ()>`
- `index_map: HashMap<String, NodeIndex>`
- `nodes: BTreeMap<String, GeneratorNode>`
- `execution_status: HashMap<String, ExecutionStatus>`
- `dirty: bool`

### Test Cases

- `test_registry_register_and_execute`
- `test_registry_duplicate_registration_fails`
- `test_registry_execute_unknown_phase_fails`
- `test_registry_rollback_called_on_failure`
- `test_registry_phase_ids_in_insertion_order`

### Synergies

- ESAA: each of 24 subsystems registered as PhaseHandler

## Insight INS-4: Deterministic Topological Sort + Graph Analytics

**Crate**: `touring-learning` | **Module**: `src/aco/graph.rs`
**Action**: MODIFY | **Priority**: Q1 | **Effort**: 3 days | **ROI**: 1.67

### Description

Add deterministic topological sort (BTreeMap-based stable ordering) and graph analytics metrics to MutableGeneratorGraph. Current sort is non-deterministic (HashMap iteration). Add GraphMetrics struct with density, critical_path_length, max_fan_out, max_fan_in.

### Implementation

```
Modify src/aco/graph.rs:
  Add pub struct GraphMetrics {
      pub node_count: usize,
      pub edge_count: usize,
      pub density: f64,       // edges / (n*(n-1))
      pub critical_path_length: usize,
      pub max_fan_out: usize,
      pub max_fan_in: usize,
  }
  Add pub fn topological_sort_deterministic(&self) -> Result<Vec<String>, GraphError>
    → uses BTreeSet for tie-breaking (ensures same order across runs)
    → returns Err(GraphError::CycleDetected) if cycle found
  Add pub fn compute_graph_metrics(&self) -> GraphMetrics
    → counts nodes, edges, computes density, walks critical path,
       finds max fan_out/fan_in using BTreeMap iteration for stability
  Update existing topological_sort() to call topological_sort_deterministic()

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

- `test_topo_sort_deterministic_simple`
- `test_topo_sort_deterministic_stable_across_insertions`
- `test_topo_sort_detects_cycle`
- `test_graph_metrics_empty`
- `test_graph_metrics_linear_chain`
- `test_graph_metrics_diamond_dag`

### Synergies

- PAR-1: parallelizable groups detected via fan-out metrics

## Subtasks

- [ ]S2.1: [INS-4] Write 6 tests for graph analytics (TDD first)
- [ ]S2.2: [INS-4] Add GraphMetrics struct to graph.rs (after S2.1)
- [ ]S2.3: [INS-4] Implement topological_sort_deterministic() in graph.rs (after S2.2)
- [ ]S2.4: [INS-4] Implement compute_graph_metrics() in graph.rs (after S2.3)
- [ ]S2.5: [INS-3] Write 5 tests for PhaseRegistry (TDD first)
- [ ]S2.6: [INS-3] Add indexmap = '2' to touring-learning Cargo.toml (after S2.5)
- [ ]S2.7: [INS-3] Create src/aco/registry.rs with PhaseHandler trait + PhaseRegistry (after S2.6)
- [ ]S2.8: [INS-3] Register in src/aco/mod.rs: pub mod registry (after S2.7)
- [ ]S2.9: cargo clippy -p touring-learning -- -D warnings (after S2.4, S2.8)
- [ ]S2.10: cargo test -p touring-learning → expect +11 more tests (after S2.9)

## DISCOVER Protocol

```
touring_ast_find(symbol_name='MutableGeneratorGraph', definitions_only=true)
touring_ast_overview(file_path='crates/touring-learning/src/aco/graph.rs')
touring_graph(action='blast_radius', symbol='MutableGeneratorGraph') → impact
touring_graph(action='blast_radius', symbol='topological_sort') → dependents
touring_memory_recall(query='topological sort deterministic graph analytics', top_k=5)
```

## TDD Plan

Tests First (11 total):
  - graph.rs: 6 tests for sort + metrics before implementation
  - registry.rs: 5 tests before PhaseRegistry
Implementation:
  1. graph.rs: GraphMetrics struct + topological_sort_deterministic + compute_graph_metrics
  2. Cargo.toml: indexmap dependency
  3. registry.rs: PhaseHandler trait + PhaseRegistry
  4. mod.rs registration
E2E:
  Build DAG → topological_sort_deterministic twice → assert same order
  Register 3 phases → execute_phase in order → verify side effects


## Checkpoint (.toon)

```python
# Save: {'insights': ['INS-4', 'INS-3'], 'new_tests': 11, 'new_files': ['registry.rs']}
# File: checkpoints/phase-02-graph-registry.toon
import msgpack, pathlib
pathlib.Path('checkpoints/phase-02-graph-registry.toon').write_bytes(msgpack.packb({'insights': ['INS-4', 'INS-3'], 'new_tests': 11, 'new_files': ['registry.rs']}))
```

## Validation Criteria

- [ ]GraphMetrics struct exists in graph.rs with node_count, edge_count, density, critical_path_length, max_fan_out, max_fan_in
- [ ]topological_sort_deterministic() returns same order for same DAG always
- [ ]src/aco/registry.rs exists with PhaseHandler trait + PhaseRegistry
- [ ]cargo check --workspace → 0 errors
- [ ]cargo clippy -p touring-learning → 0 warnings
- [ ]cargo test -p touring-learning → 11+ new tests

## Dependencies: phase-00
