---
plan_id: touring-improvements-2026
phase: 1
title: 'ACO Quick Wins: Diagnostics, Auto-decompose, State Machine, Invariant'
status: completed
created: '2026-03-25'
horizon: H1
insights_covered:
- ACO-1
- ACO-3
- ACO-4
- ACO-5
depends_on_phases:
- 0
validates_with: validate_phase01.py
estimated_effort: 5.5h
vgp_verified_structs:
- DiagnosticLayer
- DiagnosticResult
- MutableGeneratorGraph
---

# ACO Quick Wins: Diagnostics, Auto-decompose, State Machine, Invariant

## Objective

Implement 4 quick-win improvements to touring-learning/aco: diagnostic engine (ACO-1), auto-decompose (ACO-3), execution state machine (ACO-4), and objective hash verification (ACO-5).

## Final Result

New file diagnostics.rs with 4-layer classification. MutableGeneratorGraph gains auto_decompose(), transition_status(), ready_to_execute(), verify_invariant() methods. All with tests.

## Impacts

| Dimension | Assessment |
|-----------|------------|
| performance | Minimal — new methods are O(n) over graph nodes. |
| maintainability | Diagnostic engine enables structured error handling across the crate. |
| reliability | State machine prevents invalid execution transitions. Invariant detects scope creep. |
| daemon_latency | No daemon changes required. |

## Subtasks

- [x] S1.1: [ACO-1] Create src/aco/diagnostics.rs with DiagnosticLayer enum + classify_error()
- [x] S1.2: [ACO-1] Add RetryStrategy enum and suggest_retry_strategy()
- [x] S1.3: [ACO-1] Write 6 tests for diagnostic classification
- [x] S1.4: [ACO-3] Add auto_decompose() to MutableGeneratorGraph
- [x] S1.5: [ACO-3] Write 6 tests for auto-decompose
- [x] S1.6: [ACO-4] Add transition_status(), ready_to_execute(), execution_report()
- [x] S1.7: [ACO-4] Write 7 tests for state machine transitions
- [x] S1.8: [ACO-5] Add compute_objective_hash() and verify_invariant()
- [x] S1.9: [ACO-5] Write 4 tests for invariant verification
- [x] S1.10: Register diagnostics.rs in mod.rs
- [x] S1.11: cargo clippy --workspace -- -D warnings
- [x] S1.12: cargo test -p touring-learning

## Insight Details

### ACO-1: 4-Layer Diagnostic Engine

**Crate**: `touring-learning` | **Module**: `src/aco/diagnostics.rs`
**Priority**: Q1 | **Effort**: 2h

#### Description

Create a DiagnosticLayer enum (Syntax, Logic, Contract, Architecture) and a classify_error() function that categorizes errors into layers. Each layer maps to a retry strategy: Syntax -> auto-fix, Logic -> re-plan, Contract -> escalate to user, Architecture -> halt.

#### Implementation

```
New file src/aco/diagnostics.rs with:
  enum DiagnosticLayer { Syntax, Logic, Contract, Architecture }
  struct DiagnosticResult { layer: DiagnosticLayer, confidence: f64, message: String, retry_strategy: RetryStrategy }
  fn classify_error(error_msg: &str, context: &str) -> DiagnosticResult
  fn suggest_retry_strategy(layer: &DiagnosticLayer) -> RetryStrategy
Uses keyword matching + regex patterns for classification.
```

#### Test Cases

- `test_classify_syntax_error`
- `test_classify_logic_error`
- `test_classify_contract_error`
- `test_classify_architecture_error`
- `test_suggest_retry_for_each_layer`
- `test_unknown_error_defaults_to_logic`

#### Synergies: COG-2

### ACO-3: Auto-decompose by Complexity

**Crate**: `touring-learning` | **Module**: `src/aco/graph.rs`
**Priority**: Q1 | **Effort**: 1h

#### Description

Add auto_decompose() method to MutableGeneratorGraph. Based on Complexity enum (Trivial, Moderate, High, Critical), decompose a task node into 1, 2, 4, or 6 sub-nodes respectively.

#### Implementation

```
Add to MutableGeneratorGraph impl:
  pub fn auto_decompose(
      &mut self,
      node_id: &str,
      complexity: Complexity,
  ) -> Result<Vec<String>, GraphError>
  Maps Complexity variants to sub-node counts:
  Trivial=1, Moderate=2, High=4, Critical=6.
  Creates sub-nodes with id format: '{node_id}_sub_{i}'.
  Sub-nodes inherit depends_on from parent.
  Parent node is replaced by sub-node DAG.
```

#### Test Cases

- `test_auto_decompose_trivial_1_node`
- `test_auto_decompose_moderate_2_nodes`
- `test_auto_decompose_high_4_nodes`
- `test_auto_decompose_critical_6_nodes`
- `test_decompose_preserves_dependencies`
- `test_decompose_nonexistent_node_errors`

### ACO-4: Execution Tracking State Machine

**Crate**: `touring-learning` | **Module**: `src/aco/graph.rs`
**Priority**: Q1 | **Effort**: 2h

#### Description

Enrich MutableGeneratorGraph.execution_status with a state machine: Pending -> Running -> Success/Failed -> Rollback. Add methods to transition states, query ready-to-execute nodes, and get execution report.

#### Implementation

```
Add to MutableGeneratorGraph impl:
  pub fn transition_status(&mut self, node_id: &str, new_status: ExecutionStatus) -> Result<(), GraphError>
  Validates transition legality (e.g., can't go from Success to Running).
  pub fn ready_to_execute(&self) -> Vec<&str>
  Returns nodes whose deps are all Success and own status is pending.
  pub fn execution_report(&self) -> String
  Formatted summary of all node statuses.
```

#### Test Cases

- `test_initial_status_is_pending`
- `test_transition_pending_to_running`
- `test_transition_running_to_success`
- `test_transition_running_to_failed`
- `test_invalid_transition_errors`
- `test_ready_to_execute_respects_deps`
- `test_execution_report_format`

### ACO-5: ObjectiveHash verify_invariant()

**Crate**: `touring-learning` | **Module**: `src/aco/graph.rs`
**Priority**: Q1 | **Effort**: 30min

#### Description

Add verify_invariant() to MutableGeneratorGraph that recomputes objective_hash from current node contents and compares to stored hash. Detects scope creep by identifying when the graph has drifted.

#### Implementation

```
Add to MutableGeneratorGraph impl:
  pub fn compute_objective_hash(&self) -> String
  SHA-256 over sorted node IDs + descriptions (deterministic).
  Uses sha2::Sha256 (already imported in graph.rs).
  pub fn verify_invariant(&self, original_hash: &str) -> bool
  Returns compute_objective_hash() == original_hash.
```

#### Test Cases

- `test_verify_invariant_passes_unchanged`
- `test_verify_invariant_detects_drift`
- `test_compute_hash_deterministic`
- `test_compute_hash_changes_on_node_add`

## DISCOVER Protocol

```
1. Read src/aco/mod.rs to see current module exports
2. touring_ast_find(MutableGeneratorGraph) for exact impl location
3. touring_graph(blast_radius, MutableGeneratorGraph) for impact
4. Verify sha2::Sha256 import exists in graph.rs
```

## TDD Plan

### Tests First
Write all 23 test cases BEFORE implementation.
Tests reference the new types/functions with #[should_panic] or expected errors.

### Implementation
1. Create diagnostics.rs (ACO-1)
2. Extend graph.rs (ACO-3, ACO-4, ACO-5)
3. Register in mod.rs

### E2E Tests
Integration test: create graph -> add nodes -> auto_decompose -> transition states -> verify invariant -> classify error on failure.

## Checkpoint (.toon)

```
Save: {insights: [ACO-1,ACO-3,ACO-4,ACO-5], new_files: ['diagnostics.rs'], modified_files: ['graph.rs','mod.rs'], test_count_delta: +23}
File: checkpoints/phase-01-aco-quick-wins.toon
```

## Validation Criteria

- [x] src/aco/diagnostics.rs exists with DiagnosticLayer enum
- [x] classify_error function exists in diagnostics.rs
- [x]auto_decompose method exists in graph.rs
- [x]transition_status method exists in graph.rs
- [x]verify_invariant method exists in graph.rs
- [x]cargo clippy -p touring-learning -- -D warnings exits 0
- [x]cargo test -p touring-learning exits 0
- [x]All 23+ new tests pass

## Dependencies: phase-00
