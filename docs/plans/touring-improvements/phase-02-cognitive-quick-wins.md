---
plan_id: touring-improvements-2026
phase: 2
title: 'Cognitive Quick Wins: Adaptive Decay, Q-Value Persistence'
status: completed
created: '2026-03-25'
horizon: H1
insights_covered:
- COG-3
- COG-5
depends_on_phases:
- 0
validates_with: validate_phase02.py
estimated_effort: 3h
vgp_verified_structs:
- GraphSnapshot
- MemoryNode
---

# Cognitive Quick Wins: Adaptive Decay, Q-Value Persistence

## Objective

Implement 2 quick-win improvements to touring-cognitive: adaptive temporal decay (COG-3) and TransitionMatrix/QTable persistence in GraphSnapshot (COG-5).

## Final Result

MemoryNode.relevance_score() uses adaptive half-life based on access_count. GraphSnapshot includes transition_matrix and q_values with backward-compatible serde defaults.

## Impacts

| Dimension | Assessment |
|-----------|------------|
| performance | Negligible — ln() call in relevance_score, HashMap serde in snapshot. |
| maintainability | Adaptive decay is self-tuning; Q-value persistence reduces cold-start. |
| reliability | Backward-compatible serde(default) ensures old snapshots still load. |
| daemon_latency | No daemon changes (persistence is done by caller, not daemon). |

## Subtasks

- [x]S2.1: [COG-3] Add adaptive_half_life() method to MemoryNode
- [x]S2.2: [COG-3] Modify relevance_score() to use adaptive half-life
- [x]S2.3: [COG-3] Rename constant to BASE_DECAY_HALF_LIFE_SECS
- [x]S2.4: [COG-3] Write 4 tests for adaptive decay
- [x]S2.5: [COG-5] Add transition_matrix and q_values fields to GraphSnapshot
- [x]S2.6: [COG-5] Add #[serde(default)] for backward compatibility
- [x]S2.7: [COG-5] Write 5 tests for snapshot roundtrip with new fields
- [x]S2.8: cargo clippy -p touring-cognitive -- -D warnings
- [x]S2.9: cargo test -p touring-cognitive

## Insight Details

### COG-3: Adaptive Temporal Decay

**Crate**: `touring-cognitive` | **Module**: `src/semantic_graph.rs`
**Priority**: Q1 | **Effort**: 1h

#### Description

Replace fixed DECAY_HALF_LIFE_SECS (7 days) with adaptive decay: half_life = base_half_life * ln(access_count + 1). Frequently accessed nodes decay slower. Modify MemoryNode.relevance_score() to use adaptive half-life.

#### Implementation

```
Modify MemoryNode impl in semantic_graph.rs:
  fn adaptive_half_life(&self) -> f64 {
      DECAY_HALF_LIFE_SECS * (1.0 + self.access_count as f64).ln().max(1.0)
  }
  Update relevance_score() to use adaptive_half_life() instead of
  the fixed DECAY_HALF_LIFE_SECS constant.
  The constant remains as BASE_DECAY_HALF_LIFE_SECS for reference.
```

#### Test Cases

- `test_adaptive_decay_increases_with_access`
- `test_adaptive_decay_minimum_half_life`
- `test_relevance_score_adaptive_vs_fixed`
- `test_zero_access_uses_base_half_life`

#### Synergies: AST-1

### COG-5: TransitionMatrix + QTable Persistence in CognitiveSnapshot

**Crate**: `touring-cognitive` | **Module**: `src/persistence.rs`
**Priority**: Q1 | **Effort**: 2h

#### Description

Extend GraphSnapshot with TransitionMatrix and QTable from SessionPredictor to enable cross-session RL learning. Save/load preserves accumulated Q-values and transition counts.

#### Implementation

```
Extend GraphSnapshot (persistence.rs line 10):
  #[serde(default)] pub transition_matrix: HashMap<String, HashMap<String, u64>>,
  #[serde(default)] pub q_values: HashMap<String, f64>,
  The #[serde(default)] ensures backward compatibility:
  loading old snapshots without these fields uses empty defaults.
  Add to GraphPersistence:
  pub fn save_with_predictor(&self, snapshot: &GraphSnapshot) -> CognitiveResult<usize>
  pub fn load_with_predictor(&self) -> CognitiveResult<Option<GraphSnapshot>>
  (These can reuse save/load since serde handles the new fields.)
```

#### Test Cases

- `test_snapshot_with_transitions_roundtrip`
- `test_snapshot_with_q_values_roundtrip`
- `test_backward_compatible_load`
- `test_empty_predictor_serializes`
- `test_predictor_data_integrity_after_roundtrip`

#### Synergies: ACO-2

## DISCOVER Protocol

```
1. Read semantic_graph.rs DECAY_HALF_LIFE_SECS constant (line 16)
2. Read MemoryNode.relevance_score() implementation
3. Read persistence.rs GraphSnapshot struct (line 10)
4. Check existing tests in persistence.rs for test patterns
5. touring_graph(blast_radius, MemoryNode) for impact
```

## TDD Plan

### Tests First
Write 9 test cases for COG-3 and COG-5 BEFORE modifying code.

### Implementation
1. Modify MemoryNode in semantic_graph.rs (COG-3)
2. Extend GraphSnapshot in persistence.rs (COG-5)

### E2E Tests
Save a GraphSnapshot with transition_matrix and q_values, load it, verify data integrity. Also load an old snapshot (without the fields) to verify backward compatibility.

## Checkpoint (.toon)

```
Save: {insights: [COG-3,COG-5], modified_files: ['semantic_graph.rs','persistence.rs'], test_count_delta: +9}
File: checkpoints/phase-02-cog-quick-wins.toon
```

## Validation Criteria

- [x]adaptive_half_life method exists in semantic_graph.rs
- [x]relevance_score uses adaptive half-life (not fixed constant)
- [x]GraphSnapshot has transition_matrix field with serde(default)
- [x]GraphSnapshot has q_values field with serde(default)
- [x]cargo clippy -p touring-cognitive -- -D warnings exits 0
- [x]cargo test -p touring-cognitive exits 0
- [x]Backward-compatible load test passes (old snapshot format)

## Dependencies: phase-00
