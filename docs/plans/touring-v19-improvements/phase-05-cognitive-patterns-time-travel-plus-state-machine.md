---
plan_id: touring-v19-improvements
phase: 5
title: 'Cognitive Patterns: Time-Travel + State Machine'
status: planned
created: '2026-03-25'
horizon: H2
insights_covered: [INS-9, INS-10]
depends_on_phases:
- 0
- 3
validates_with: validate_phase05.py
estimated_effort: 1 week
vgp_verified_structs: [MutableGeneratorGraph]
---

# Cognitive Patterns: Time-Travel + State Machine

> **Depends on**: Phase 0, 3
> **Insights**: INS-9 (ROI=1.17), INS-10 (ROI=1.20)

## Objective

Implement INS-9 (Time-Travel Debugging, ROI=1.17) in touring-learning and INS-10 (Agent State Machine Complete, ROI=1.20) in touring-cognitive. INS-9 benefits from CQRS snapshot concept (phase 3). INS-10 is independent.

## Final Result

New time_travel.rs with TimeTravelDebugger. New agent_state_machine.rs in touring-cognitive. 12 new tests.

## Impacts

| Dimension | Assessment |
|-----------|------------|
| performance | TimeTravelDebugger adds O(snapshots) memory. Bounded by max_snapshots. |
| maintainability | Time-travel enables reproducible post-mortem. State machine makes agent flow explicit. |
| reliability | State machine prevents invalid state transitions in cognitive layer. |
| daemon_latency | No daemon changes. |

## Insight INS-9: Time-Travel Debugging

**Crate**: `touring-learning` | **Module**: `src/aco/time_travel.rs`
**Action**: NEW | **Priority**: Q2 | **Effort**: 3 days | **ROI**: 1.17

### Description

Implement time-travel debugging for ACO execution: capture state snapshots at each epoch, query state_at_epoch, diff two epochs, replay execution from any checkpoint. Enables post-mortem analysis of orchestration failures.

### Implementation

```
New file src/aco/time_travel.rs:
  pub struct StateSnapshot {
      pub epoch: u64,
      pub node_statuses: HashMap<String, ExecutionStatus>,
      pub graph_metrics: GraphMetrics,
      pub timestamp_ms: u64,
  }
  pub struct EpochDiff {
      pub from_epoch: u64,
      pub to_epoch: u64,
      pub changed_nodes: Vec<(String, ExecutionStatus, ExecutionStatus)>,
      pub new_nodes: Vec<String>,
      pub removed_nodes: Vec<String>,
  }
  pub struct TimeTravelDebugger {
      snapshots: Vec<StateSnapshot>,
      max_snapshots: usize,
  }
  impl TimeTravelDebugger:
    pub fn new(max_snapshots: usize) -> Self
    pub fn capture_state(&mut self, graph: &MutableGeneratorGraph) -> u64
      → creates StateSnapshot, appends, returns epoch number
      → if at capacity: evict oldest snapshot
    pub fn state_at_epoch(&self, epoch: u64) -> Option<&StateSnapshot>
    pub fn diff_epochs(&self, from: u64, to: u64) -> Option<EpochDiff>
    pub fn replay_from(&self, epoch: u64) -> Option<Vec<StateSnapshot>>
      → returns all snapshots from epoch onward
    pub fn epoch_count(&self) -> usize

```

### VGP-Verified Structs

**MutableGeneratorGraph** (crates/touring-learning/src/aco/graph.rs line 33):
- `graph: DiGraph<String, ()>`
- `index_map: HashMap<String, NodeIndex>`
- `nodes: BTreeMap<String, GeneratorNode>`
- `execution_status: HashMap<String, ExecutionStatus>`
- `dirty: bool`

### Test Cases

- `test_capture_state_returns_epoch`
- `test_state_at_epoch_found`
- `test_state_at_epoch_not_found`
- `test_diff_epochs_detects_changes`
- `test_diff_epochs_new_nodes`
- `test_replay_from_returns_subsequence`

### Synergies

- AcoReadModel: snapshots read model at each epoch

## Insight INS-10: Agent State Machine Complete

**Crate**: `touring-cognitive` | **Module**: `src/agent_state_machine.rs`
**Action**: NEW | **Priority**: Q2 | **Effort**: 2 days | **ROI**: 1.20

### Description

Implement formal agent state machine for touring-cognitive. AgentState enum with 7 states: Idle, Perceiving, Planning, Executing, Evaluating, Refining, Halted. Transition table enforces valid state changes. History tracks full state sequence.

### Implementation

```
New file src/agent_state_machine.rs:
  pub enum AgentState { Idle, Perceiving, Planning, Executing, Evaluating, Refining, Halted }
  pub enum TransitionError { InvalidTransition(AgentState, AgentState), AlreadyHalted }
  pub struct AgentStateMachine {
      current: AgentState,
      history: Vec<AgentState>,
      transition_count: u64,
  }
  const VALID_TRANSITIONS: &[(AgentState, AgentState)] = &[
    (Idle, Perceiving), (Perceiving, Planning), (Planning, Executing),
    (Executing, Evaluating), (Evaluating, Refining), (Refining, Executing),
    (Evaluating, Idle), (Refining, Idle),
    (*, Halted),  // any state -> Halted
  ];
  impl AgentStateMachine:
    pub fn new() -> Self            → starts in Idle
    pub fn current(&self) -> &AgentState
    pub fn transition(&mut self, next: AgentState) -> Result<(), TransitionError>
      → validates against VALID_TRANSITIONS, appends to history, increments count
    pub fn can_transition(&self, next: &AgentState) -> bool
    pub fn history(&self) -> &[AgentState]
    pub fn transition_count(&self) -> u64
    pub fn reset(&mut self)         → returns to Idle, clears history
Register in src/lib.rs as pub mod agent_state_machine

```

### VGP-Verified Structs

No existing structs referenced — all new types.

### Test Cases

- `test_state_machine_starts_idle`
- `test_valid_transition_sequence`
- `test_invalid_transition_rejected`
- `test_any_state_can_halt`
- `test_history_records_transitions`
- `test_reset_clears_history`

### Synergies

- RefinementCycle: drives state transitions during cognitive refinement

## Subtasks

- [ ]S5.1: [INS-9] Write 6 tests for TimeTravelDebugger (TDD first)
- [ ]S5.2: [INS-9] Create src/aco/time_travel.rs with StateSnapshot, EpochDiff, TimeTravelDebugger (after S5.1)
- [ ]S5.3: [INS-9] Register in aco/mod.rs: pub mod time_travel (after S5.2)
- [ ]S5.4: [INS-10] Write 6 tests for AgentStateMachine (TDD first)
- [ ]S5.5: [INS-10] Create src/agent_state_machine.rs in touring-cognitive with AgentState enum (after S5.4)
- [ ]S5.6: [INS-10] Implement VALID_TRANSITIONS const and transition() method (after S5.5)
- [ ]S5.7: [INS-10] Register in touring-cognitive/src/lib.rs: pub mod agent_state_machine (after S5.6)
- [ ]S5.8: cargo clippy --workspace -- -D warnings (after S5.3, S5.7)
- [ ]S5.9: cargo test --workspace --exclude touring-python → expect +12 more tests (after S5.8)

## DISCOVER Protocol

```
touring_ast_find(symbol_name='MutableGeneratorGraph', definitions_only=true) → iter method
touring_ast_overview(file_path='crates/touring-cognitive/src/lib.rs')
touring_ast_find(symbol_name='RefinementCycle', definitions_only=true) → integration point
touring_memory_recall(query='time travel debug state machine cognitive', top_k=5)
```

## TDD Plan

Tests First (12 total):
  - time_travel.rs: 6 tests for TimeTravelDebugger
  - agent_state_machine.rs: 6 tests for AgentStateMachine
Implementation:
  1. time_travel.rs in touring-learning (INS-9)
  2. agent_state_machine.rs in touring-cognitive (INS-10)
  3. mod.rs/lib.rs registrations
E2E:
  Capture 5 epochs → state_at_epoch(3) → diff_epochs(1,4) → replay_from(2)
  AgentStateMachine: full cycle Idle→Perceiving→Planning→Executing→Evaluating→Idle


## Checkpoint (.toon)

```python
# Save: {'insights': ['INS-9', 'INS-10'], 'new_tests': 12, 'new_files': ['time_travel.rs', 'agent_state_machine.rs']}
# File: checkpoints/phase-05-cognitive-patterns.toon
import msgpack, pathlib
pathlib.Path('checkpoints/phase-05-cognitive-patterns.toon').write_bytes(msgpack.packb({'insights': ['INS-9', 'INS-10'], 'new_tests': 12, 'new_files': ['time_travel.rs', 'agent_state_machine.rs']}))
```

## Validation Criteria

- [ ]src/aco/time_travel.rs exists with TimeTravelDebugger (capture_state, state_at_epoch, diff_epochs, replay_from)
- [ ]src/agent_state_machine.rs exists in touring-cognitive
- [ ]AgentState enum has all 7 states
- [ ]VALID_TRANSITIONS enforced — invalid transitions return Err
- [ ]cargo clippy --workspace → 0 warnings
- [ ]cargo test --workspace --exclude touring-python → 12+ new tests

## Dependencies: phase-00, phase-03
