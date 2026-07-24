---
plan_id: touring-improvements-2026
phase: 6
title: Cross-Crate Synergies Integration
status: completed
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

## Objective

Wire up the 4 identified synergy pairs: (1) ACO-1 <-> COG-2: diagnostic drives refinement strategy, (2) AST-1 <-> COG-3: unified HeatMap for indexing and decay, (3) ACO-2 <-> COG-5: EvolutionPackages feed persisted Q-values, (4) AST-2 <-> COG-1: RRF provides MCTS expansion priors.

## Final Result

Integration modules/traits connecting ACO diagnostics with cognitive refinement, shared heat signals, evolution-to-persistence pipeline, and search-to-MCTS bridge.

## Impacts

| Dimension | Assessment |
|-----------|------------|
| performance | Cross-crate calls add indirection. Measure before optimizing. |
| maintainability | Clean trait boundaries prevent tight coupling. |
| reliability | Integration tests catch cross-crate regressions. |
| daemon_latency | Possible increase if synergies add to hook processing. Benchmark. |

## Subtasks

- [x]S6.1: Synergy 1 — ACO-1 <-> COG-2: Export DiagnosticLayer from touring-learning, import in touring-cognitive/refinement.rs
- [x]S6.2: Synergy 2 — AST-1 <-> COG-3: Create shared HeatSignal trait consumed by both file_heat.rs and semantic_graph.rs
- [x]S6.3: Synergy 3 — ACO-2 <-> COG-5: EvolutionPackage.quality_metrics feeds into GraphSnapshot.q_values
- [x]S6.4: Synergy 4 — AST-2 <-> COG-1: find_symbols_fused() results as MCTS expand priors
- [x]S6.5: Integration tests for all 4 synergies
- [x]S6.6: cargo clippy --workspace -- -D warnings
- [x]S6.7: cargo test --workspace --exclude touring-python
- [x]S6.8: Measure daemon latency before/after with benchmark

## DISCOVER Protocol

```
1. Check Cargo.toml dependencies between crates
2. touring_graph(blast_radius) for each synergy endpoint
3. Verify no circular dependencies would be created
4. Read lib.rs of each crate for public export structure
```

## TDD Plan

### Tests First
Integration tests for all 4 synergies BEFORE wiring.

### Implementation
1. Dependency additions in Cargo.toml (if needed)
2. Trait definitions for cross-crate interfaces
3. Bridge implementations

### E2E Tests
Full pipeline: execute graph -> diagnose failure -> refinement cycle -> persist Q-values -> use for next MCTS search.

## Checkpoint (.toon)

```
Save: {synergies: 4, modified_crates: ['touring-learning','touring-ast','touring-cognitive'], new_deps: [...], test_count_delta: +N}
File: checkpoints/phase-06-synergies.toon
```

## Validation Criteria

- [x]DiagnosticLayer is importable from touring-cognitive
- [x]No circular dependency in cargo check
- [x]All 4 synergy integration tests pass
- [x]cargo clippy --workspace -- -D warnings exits 0
- [x]cargo test --workspace --exclude touring-python exits 0
- [x]Daemon latency benchmark: P50 < 2ms, P99 < 10ms

## Dependencies: phase-01, phase-02, phase-03, phase-04, phase-05
