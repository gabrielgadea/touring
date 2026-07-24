---
plan_id: touring-improvements-2026
phase: 3
title: 'ACO Evolution: EvolutionPackage Population Logic'
status: completed
created: '2026-03-25'
horizon: H2
insights_covered:
- ACO-2
depends_on_phases:
- 1
validates_with: validate_phase03.py
estimated_effort: 1 week
vgp_verified_structs:
- EvolutionPackage
---

# ACO Evolution: EvolutionPackage Population Logic

## Objective

Implement full population logic for EvolutionPackage. After graph execution, extract patterns, anti-patterns, and propose upgrades.

## Final Result

New file evolution.rs with populate_evolution_package() and helpers. Integrates with MutableGeneratorGraph.execution_status and TrackerReport.

## Impacts

| Dimension | Assessment |
|-----------|------------|
| performance | O(n) over graph nodes — linear, acceptable for typical graph sizes (<100 nodes). |
| maintainability | Centralizes learning extraction in one module. |
| reliability | Explicit extraction better than ad-hoc pattern recognition. |
| daemon_latency | No daemon changes. Evolution runs post-execution. |

## Subtasks

- [x]S3.1: Create src/aco/evolution.rs with populate_evolution_package()
- [x]S3.2: Implement extract_learned_patterns() from Success nodes
- [x]S3.3: Implement discover_anti_patterns() from failed nodes
- [x]S3.4: Implement propose_system_upgrades() from quality_metrics
- [x]S3.5: Wire evolution.rs into mod.rs
- [x]S3.6: Write 6 tests for all extraction functions
- [x]S3.7: Integration test: full graph execution -> evolution package
- [x]S3.8: cargo clippy + cargo test

## Insight Details

### ACO-2: EvolutionPackage Population Logic

**Crate**: `touring-learning` | **Module**: `src/aco/evolution.rs`
**Priority**: Q2 | **Effort**: 1 week

#### Description

Implement the full population logic for EvolutionPackage (models.rs line 266). After a graph execution completes, extract learned_patterns from successful nodes, discover anti_patterns from failed nodes, propose system_upgrades based on quality_metrics deltas, and serialize to persistence_actions.

#### Implementation

```
New file src/aco/evolution.rs:
  fn populate_evolution_package(
      graph: &MutableGeneratorGraph,
      session_id: &str,
      objective_hash: &str,
      execution_report: &str,
  ) -> EvolutionPackage
  Iterates graph.iter() to extract patterns from Success nodes,
  anti-patterns from ExecutionFailed/ValidationFailed nodes.
  Uses execution_status HashMap from MutableGeneratorGraph.
  quality_metrics computed from TrackerReport.
```

#### Test Cases

- `test_populate_from_successful_execution`
- `test_populate_from_failed_execution`
- `test_extract_patterns_from_nodes`
- `test_discover_anti_patterns`
- `test_propose_upgrades_from_metrics`
- `test_empty_graph_produces_empty_package`

#### Synergies: COG-5

## DISCOVER Protocol

```
1. touring_ast_find(EvolutionPackage) for field list (verified: 8 fields)
2. Read graph.rs execution_status usage patterns
3. Read tracker.rs for TrackerReport integration
4. touring_graph(blast_radius, EvolutionPackage)
```

## TDD Plan

### Tests First
6 unit tests + 1 integration test BEFORE implementation.

### Implementation
evolution.rs: populate_evolution_package + 3 helper functions.

### E2E Tests
Build a graph, execute all nodes (some fail), extract EvolutionPackage, verify it contains both patterns and anti-patterns.

## Checkpoint (.toon)

```
Save: {insights: [ACO-2], new_files: ['evolution.rs'], test_count_delta: +7}
File: checkpoints/phase-03-aco-evolution.toon
```

## Validation Criteria

- [x]src/aco/evolution.rs exists
- [x]populate_evolution_package function exists
- [x]extract_learned_patterns function exists
- [x]discover_anti_patterns function exists
- [x]propose_system_upgrades function exists
- [x]cargo clippy -p touring-learning -- -D warnings exits 0
- [x]cargo test -p touring-learning exits 0

## Dependencies: phase-01
