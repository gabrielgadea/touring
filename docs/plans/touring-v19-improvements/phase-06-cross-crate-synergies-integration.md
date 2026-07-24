---
plan_id: touring-v19-improvements
phase: 6
title: 'Cross-Crate Synergies Integration'
status: planned
created: '2026-03-25'
horizon: H3
insights_covered: []
depends_on_phases:
- 1
- 2
- 3
- 4
- 5
validates_with: validate_phase06.py
estimated_effort: 1 week
vgp_verified_structs: []
---

# Cross-Crate Synergies Integration

> **Depends on**: Phase 1, 2, 3, 4, 5

## Objective

Wire up 4 key synergies: (1) TemplateLibrary ↔ CognitiveMCTS shared patterns, (2) GoalTracker ↔ AST FileHeat heat-informed dimensional scoring, (3) TimeTravelDebugger ↔ AcoReadModel snapshot at epoch, (4) ESAA subsystems ↔ PhaseRegistry plugin-as-phase.

## Final Result

Integration bridges connecting touring-learning ↔ touring-cognitive ↔ touring-ast. 8 integration tests. No circular dependencies.

## Impacts

| Dimension | Assessment |
|-----------|------------|
| performance | Cross-crate calls add indirection. Benchmark before optimizing. |
| maintainability | Clean trait boundaries prevent tight coupling. |
| reliability | Integration tests catch cross-crate regressions. |
| daemon_latency | Possible increase if synergies add to hook processing. Benchmark. |

## Subtasks

- [ ]S6.1: Synergy 1: Export TemplateLibrary from touring-learning; use in CognitiveMCTS as expansion prior
- [ ]S6.2: Synergy 2: GoalTracker passes HeatMap from touring-ast as dimensional check context
- [ ]S6.3: Synergy 3: TimeTravelDebugger.capture_state uses AcoReadModel snapshot
- [ ]S6.4: Synergy 4: Register ESAA subsystems via PhaseRegistry.register()
- [ ]S6.5: Write 2 integration tests per synergy (8 total)
- [ ]S6.6: cargo check --workspace → 0 errors (verify no circular deps)
- [ ]S6.7: cargo clippy --workspace -- -D warnings
- [ ]S6.8: cargo test --workspace --exclude touring-python

## DISCOVER Protocol

```
Check Cargo.toml deps: touring-learning → touring-ast? touring-cognitive → touring-learning?
touring_graph(action='blast_radius', symbol='TemplateLibrary') → who imports
touring_ast_find(symbol_name='CognitiveMCTS', definitions_only=true) → expand_fn signature
touring_memory_recall(query='cross crate synergy integration circular dependency', top_k=5)
```

## TDD Plan

Tests First (8 integration tests):
  - Synergy 1: 2 tests for TemplateLibrary ↔ CognitiveMCTS
  - Synergy 2: 2 tests for GoalTracker ↔ FileHeat
  - Synergy 3: 2 tests for TimeTravelDebugger ↔ AcoReadModel
  - Synergy 4: 2 tests for ESAA ↔ PhaseRegistry
Implementation:
  1. Dependency additions in Cargo.toml (verify no cycles: cargo check)
  2. Trait definitions for cross-crate interfaces
  3. Bridge implementations


## Checkpoint (.toon)

```python
# Save: {'synergies': 4, 'new_tests': 8, 'modified_crates': ['touring-learning', 'touring-cognitive', 'touring-ast']}
# File: checkpoints/phase-06-synergies.toon
import msgpack, pathlib
pathlib.Path('checkpoints/phase-06-synergies.toon').write_bytes(msgpack.packb({'synergies': 4, 'new_tests': 8, 'modified_crates': ['touring-learning', 'touring-cognitive', 'touring-ast']}))
```

## Validation Criteria

- [ ]TemplateLibrary importable from touring-cognitive
- [ ]No circular dependency (cargo check --workspace exits 0)
- [ ]All 4 synergy integration tests pass
- [ ]cargo clippy --workspace → 0 warnings
- [ ]cargo test --workspace --exclude touring-python → 8+ new integration tests

## Dependencies: phase-01, phase-02, phase-03, phase-04, phase-05
