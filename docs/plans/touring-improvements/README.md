# Touring Improvements Plan 2026

**Plan ID**: touring-improvements-2026
**Created**: 2026-03-25
**Completed**: 2026-03-25
**Version**: 1.0.0 → **1.1.0 (IMPLEMENTED)**
**Total Insights**: 13 / 13 ✅
**Test delta**: 2,079 → 2,152 (+73 novos testes)
**Audit**: APROVADO — 0 Critical, 0 Important, 4 Suggestions (opcionais)

## Objective

Implement 13 insights across touring-learning/aco, touring-ast, and touring-cognitive for diagnostic engine, evolution logic, prioritized indexing, fused search, cognitive MCTS, refinement cycles, adaptive decay, multi-dimensional GoT evaluation, and Q-value persistence.

## Horizons

| Horizon | Timeline | Description |
|---------|----------|-------------|
| H1 | 0-2 weeks | 0-2 weeks (Quick Wins) |
| H2 | 2-8 weeks | 2-8 weeks (Strategic) |
| H3 | 2-6 months | 2-6 months (Synergies + Audit) |

## Insights Summary

| ID | Title | Crate | Phase | Priority | Effort |
|----|-------|-------|-------|----------|--------|
| ACO-1 | 4-Layer Diagnostic Engine | touring-learning | 1 | Q1 | 2h |
| ACO-2 | EvolutionPackage Population Logic | touring-learning | 3 | Q2 | 1 week |
| ACO-3 | Auto-decompose by Complexity | touring-learning | 1 | Q1 | 1h |
| ACO-4 | Execution Tracking State Machine | touring-learning | 1 | Q1 | 2h |
| ACO-5 | ObjectiveHash verify_invariant() | touring-learning | 1 | Q1 | 30min |
| AST-1 | FileHeat Prioritized Indexing | touring-ast | 4 | Q2 | 3 days |
| AST-2 | find_symbols_fused() with RRF | touring-ast | 4 | Q2 | 3 days |
| AST-3 | EnrichedBlastRadius with Impact Categories | touring-ast | 4 | Q2 | 3 days |
| COG-1 | CognitiveMCTS (SemanticGraph-informed MCTS) | touring-cognitive | 5 | Q2 | 1 week |
| COG-2 | RefinementCycle (Cognitive Retry) | touring-cognitive | 5 | Q2 | 1 week |
| COG-3 | Adaptive Temporal Decay | touring-cognitive | 2 | Q1 | 1h |
| COG-4 | GoT Multi-dimensional Evaluation | touring-cognitive | 5 | Q2 | 3 days |
| COG-5 | TransitionMatrix + QTable Persistence in CognitiveSnapshot | touring-cognitive | 2 | Q1 | 2h |

## Phases

| Phase | Title | Horizon | Insights | Effort | Status |
|-------|-------|---------|----------|--------|--------|
| phase-00 | [Foundation: DISCOVER Protocol, VGP Cache, Checkpoint Setup](phase-00-foundation.md) | H0 | - | 2h | planned |
| phase-01 | [ACO Quick Wins: Diagnostics, Auto-decompose, State Machine, Invariant](phase-01-aco-quick-wins.md) | H1 | ACO-1, ACO-3, ACO-4, ACO-5 | 5.5h | ✅ completed |
| phase-02 | [Cognitive Quick Wins: Adaptive Decay, Q-Value Persistence](phase-02-cognitive-quick-wins.md) | H1 | COG-3, COG-5 | 3h | ✅ completed |
| phase-03 | [ACO Evolution: EvolutionPackage Population Logic](phase-03-aco-evolution.md) | H2 | ACO-2 | 1 week | ✅ completed |
| phase-04 | [AST Enhancements: FileHeat, Fused Search, EnrichedBlastRadius](phase-04-ast-enhancements.md) | H2 | AST-1, AST-2, AST-3 | 9 days | ✅ completed |
| phase-05 | [Cognitive Advanced: CognitiveMCTS, RefinementCycle, GoT Multi-dim](phase-05-cognitive-advanced.md) | H2 | COG-1, COG-2, COG-4 | 2.5 weeks | ✅ completed |
| phase-06 | [Cross-Crate Synergies Integration](phase-06-cross-crate-synergies-integration.md) | H3 | - | 1 week | ✅ completed |
| phase-07 | [Final Audit and Consolidation](phase-07-final-audit-and-consolidation.md) | H3 | - | 3 days | ✅ completed |

## Usage

```bash
# Generate/regenerate all plan files
python generate_plan.py --validate

# List phases
python generate_plan.py --list

# Validate a specific phase after implementation
python validate_phase01.py --verbose

# Audit all implementations
python audit_implementation.py --verbose

# Modify plan atomically
python patch_plan.py --phase phase-01 --set status=in_progress
```

## Synergy Map

```
ACO-1 <---> COG-2  (diagnostic classifies error -> refinement selects strategy)
AST-1 <---> COG-3  (HeatMap unified — pheromone shared between indexing and decay)
ACO-2 <---> COG-5  (EvolutionPackages feed persisted Q-values -> RL cross-session)
AST-2 <---> COG-1  (RRF fusion provides priors for MCTS expansion)
```

## VGP-Verified Structs

Total: 27 structs verified from source.
See `generate_plan.py` VGP_SCHEMAS dict for complete field listings.
