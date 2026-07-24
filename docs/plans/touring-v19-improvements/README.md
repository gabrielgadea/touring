# touring-v19-improvements — Touring v19 Improvements

**Plan ID**: touring-v19-improvements
**Created**: 2026-03-25
**Version**: 1.0.0
**Total Insights**: 10
**Baseline Tests**: 2152
**Expected New Tests**: 84
**Expected Final Tests**: 2236

## Objective

Implement 10 insights across touring-learning/aco, touring-ast, and touring-cognitive: TemplateLibrary with learning, GoalTracker 9x9 computational, PhaseRegistry dynamic, deterministic graph analytics, CQRS read model, Saga pattern, parallel generator engine, ESAA 24 subsystems, time-travel debugging, and agent state machine.

## Horizons

| Horizon | Description |
|---------|-------------|
| H0 | Setup (2h) |
| H1 | Quick Wins (0-2 weeks) |
| H2 | Strategic (2-8 weeks) |
| H3 | Synergies + Audit (2-6 months) |

## Insights by ROI

| ID | Title | Crate | Phase | ROI | Effort |
|----|-------|-------|-------|-----|--------|
| INS-1 | TemplateLibrary with Learning | touring-learning | 1 | 2.00 | 3 days |
| INS-4 | Deterministic Topological Sort + Graph Analytics | touring-learning | 2 | 1.67 | 3 days |
| INS-2 | GoalTracker 9×9 Computational | touring-learning | 1 | 1.60 | 2 days |
| INS-3 | Plugin/Phase Registry Dynamic | touring-learning | 2 | 1.50 | 2 days |
| INS-5 | CQRS Read Model | touring-learning | 3 | 1.40 | 2 days |
| INS-6 | Saga Pattern with Compensating Transactions | touring-learning | 3 | 1.40 | 3 days |
| INS-7 | Parallel Generator Engine | touring-learning | 4 | 1.33 | 3 days |
| INS-10 | Agent State Machine Complete | touring-cognitive | 5 | 1.20 | 2 days |
| INS-9 | Time-Travel Debugging | touring-learning | 5 | 1.17 | 3 days |
| INS-8 | ESAA Complete 24 Subsystems | touring-learning | 4 | 1.13 | 1.5 weeks |

## Phases

| Phase | Title | Horizon | Insights | Status |
|-------|-------|---------|----------|--------|
| 0 | [Foundation: DISCOVER, VGP Cache, Checkpoints](phase-00-foundation-discover-vgp-cache-checkpoints.md) | H0 | — | planned |
| 1 | [Quick Wins: TemplateLibrary + GoalTracker 9×9](phase-01-quick-wins-templatelibrary-plus-goaltracker-99.md) | H1 | INS-1, INS-2 | planned |
| 2 | [Graph Analytics + Phase Registry](phase-02-graph-analytics-plus-phase-registry.md) | H1 | INS-3, INS-4 | planned |
| 3 | [Persistence Patterns: CQRS + Saga](phase-03-persistence-patterns-cqrs-plus-saga.md) | H2 | INS-5, INS-6 | planned |
| 4 | [Performance: Parallel Engine + ESAA 24](phase-04-performance-parallel-engine-plus-esaa-24.md) | H2 | INS-7, INS-8 | planned |
| 5 | [Cognitive Patterns: Time-Travel + State Machine](phase-05-cognitive-patterns-time-travel-plus-state-machine.md) | H2 | INS-9, INS-10 | planned |
| 6 | [Cross-Crate Synergies Integration](phase-06-cross-crate-synergies-integration.md) | H3 | — | planned |
| 7 | [Final Audit and Cross-Validation](phase-07-final-audit-and-cross-validation.md) | H3 | — | planned |

## VGP-Verified Structs

| Struct | File | Line | Kind |
|--------|------|------|------|
| EvolutionPackage | crates/touring-learning/src/aco/models.rs | 272 | struct |
| LearnedPattern | crates/touring-learning/src/aco/models.rs | 254 | struct |
| GoalTrackerState | crates/touring-learning/src/aco/models.rs | 240 | struct |
| DimensionScore | crates/touring-learning/src/aco/models.rs | 230 | struct |
| GeneratorGraphModel | crates/touring-learning/src/aco/models.rs | 219 | struct |
| MutableGeneratorGraph | crates/touring-learning/src/aco/graph.rs | 33 | struct |
| DiagnosticResult | crates/touring-learning/src/aco/diagnostics.rs | 59 | struct |
| DiagnosticLayer | crates/touring-learning/src/aco/diagnostics.rs | 10 | enum |
| TrackerReport | crates/touring-learning/src/aco/tracker.rs | 65 | struct |
| DimResult | crates/touring-learning/src/aco/tracker.rs | 36 | struct |

## Usage

```bash
# Regenerate all plan files
python generate_plan.py

# Validate idempotency
python generate_plan.py --validate

# Validate a specific phase
python validate_phase01.py --verbose

# Cross-audit all 10 insights
python audit_v19.py --verbose
```
