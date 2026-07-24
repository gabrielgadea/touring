---
plan_id: touring-improvements-2026
phase: 5
title: 'Cognitive Advanced: CognitiveMCTS, RefinementCycle, GoT Multi-dim'
status: completed
created: '2026-03-25'
horizon: H2
insights_covered:
- COG-1
- COG-2
- COG-4
depends_on_phases:
- 1
- 2
validates_with: validate_phase05.py
estimated_effort: 2.5 weeks
vgp_verified_structs:
- CognitiveMCTS
- CognitiveMCTSConfig
- GotNode
- RefinementConfig
- RefinementCycle
- RefinementOutcome
- ThoughtResult
---

# Cognitive Advanced: CognitiveMCTS, RefinementCycle, GoT Multi-dim

## Objective

Implement 3 strategic improvements to touring-cognitive: graph-informed MCTS, diagnostic-driven refinement cycle, and multi-dimensional GoT evaluation.

## Final Result

New files cognitive_mcts.rs and refinement.rs. Extended ThoughtResult and GotNode with multi-dimensional evaluation.

## Impacts

| Dimension | Assessment |
|-----------|------------|
| performance | CognitiveMCTS: bounded by MCTSConfig.max_rollouts. RefinementCycle: max 3 iterations. GoT: O(n*d) where d=dimensions. |
| maintainability | Each new module is self-contained with clear interfaces. |
| reliability | RefinementCycle adds structured error recovery. CognitiveMCTS improves action selection. |
| daemon_latency | No daemon changes. All cognitive operations are caller-side. |

## Subtasks

- [x]S5.1: [COG-1] Create src/cognitive_mcts.rs with CognitiveMCTS
- [x]S5.2: [COG-1] Implement cognitive_search with graph priors
- [x]S5.3: [COG-1] Write 6 tests
- [x]S5.4: [COG-2] Create src/refinement.rs with RefinementCycle
- [x]S5.5: [COG-2] Implement run_refinement with DiagnosticLayer integration
- [x]S5.6: [COG-2] Write 6 tests
- [x]S5.7: [COG-4] Extend ThoughtResult with relevance, confidence, novelty fields
- [x]S5.8: [COG-4] Add evaluate_multidimensional() to GotNode
- [x]S5.9: [COG-4] Write 5 tests for multi-dimensional evaluation
- [x]S5.10: Register new modules in lib.rs
- [x]S5.11: cargo clippy -p touring-cognitive -- -D warnings
- [x]S5.12: cargo test -p touring-cognitive

## Insight Details

### COG-1: CognitiveMCTS (SemanticGraph-informed MCTS)

**Crate**: `touring-cognitive` | **Module**: `src/cognitive_mcts.rs`
**Priority**: Q2 | **Effort**: 1 week

#### Description

Create CognitiveMCTS that wraps MCTSEngine with SemanticGraph priors. Expand function uses graph neighbors as candidate actions. Reward function weighted by MemoryNode.relevance_score().

#### Implementation

```
New file src/cognitive_mcts.rs:
  struct CognitiveMCTSConfig {
      mcts_config: MCTSConfig,
      relevance_weight: f64,  // 0.0-1.0, how much to weight graph priors
  }
  struct CognitiveMCTS {
      engine: MCTSEngine,
      config: CognitiveMCTSConfig,
  }
  impl CognitiveMCTS:
    pub fn cognitive_search(
        &self,
        root_state: u64,
        graph: &SemanticGraph,
    ) -> Option<MCTSResult>
  Constructs expand_fn from graph neighbors (Direction::Outgoing).
  Constructs reward_fn using MemoryNode.relevance_score(now).
  Delegates to MCTSEngine.search().
```

#### Test Cases

- `test_cognitive_mcts_search_basic`
- `test_cognitive_mcts_uses_graph_priors`
- `test_cognitive_mcts_empty_graph`
- `test_cognitive_mcts_respects_max_depth`
- `test_graph_expand_returns_neighbors`
- `test_graph_reward_uses_relevance`

#### Synergies: AST-2

### COG-2: RefinementCycle (Cognitive Retry)

**Crate**: `touring-cognitive` | **Module**: `src/refinement.rs`
**Priority**: Q2 | **Effort**: 1 week

#### Description

Implement RefinementCycle: a retry loop informed by DiagnosticLayer (from ACO-1). Max 3 iterations, each using a different strategy based on the diagnostic classification of the previous failure.

#### Implementation

```
New file src/refinement.rs:
  struct RefinementConfig { max_iterations: u32, timeout_secs: u64 }
  enum RefinementOutcome { Resolved, Exhausted, Escalated }
  struct RefinementCycle {
      config: RefinementConfig,
      history: Vec<(DiagnosticLayer, String)>,  // layer + error msg
  }
  impl RefinementCycle:
    pub fn run_refinement<F>(
        &mut self,
        execute_fn: F,
    ) -> RefinementOutcome
    where F: FnMut() -> Result<(), String>
  On each failure: classify_error -> select_strategy -> apply_strategy -> retry.
  Architecture errors -> immediate Escalated.
```

#### Test Cases

- `test_refinement_succeeds_first_try`
- `test_refinement_retries_on_syntax_error`
- `test_refinement_halts_on_architecture_error`
- `test_refinement_max_iterations`
- `test_refinement_strategy_selection`
- `test_refinement_outcome_recording`

#### Synergies: ACO-1

### COG-4: GoT Multi-dimensional Evaluation

**Crate**: `touring-cognitive` | **Module**: `src/got.rs`
**Priority**: Q2 | **Effort**: 3 days

#### Description

Enhance GotNode.evaluate() with 3 dimensions: relevance (existing), confidence (based on visit count), novelty (inverse of similarity to previous results). ThoughtResult extended with dimension scores.

#### Implementation

```
Extend ThoughtResult (got.rs line 37) with:
  pub relevance: f64,    // existing score dimension
  pub confidence: f64,   // 1.0 - 1.0/(1.0 + visits as f64)
  pub novelty: f64,      // 1.0 - max_similarity_to_previous
Add to GotNode:
  pub fn evaluate_multidimensional(
      &self,
      msg: &ThoughtMessage,
      visit_count: u64,
      previous_outputs: &[String],
  ) -> ThoughtResult
  Final score = 0.4*relevance + 0.3*confidence + 0.3*novelty.
```

#### Test Cases

- `test_multidim_eval_all_dimensions`
- `test_multidim_confidence_increases_with_visits`
- `test_multidim_novelty_decreases_with_repetition`
- `test_multidim_backward_compatible`
- `test_multidim_score_aggregation`

## DISCOVER Protocol

```
1. Read mcts.rs for MCTSEngine interface (search method signature)
2. Read semantic_graph.rs for SemanticGraph neighbor traversal
3. Read got.rs for GotNode.evaluate() and ThoughtResult
4. touring_graph(blast_radius, MCTSEngine)
5. touring_graph(blast_radius, GotNode)
```

## TDD Plan

### Tests First
17 test cases across 3 insights BEFORE implementation.

### Implementation
1. cognitive_mcts.rs (COG-1) — depends on MCTSEngine + SemanticGraph
2. refinement.rs (COG-2) — depends on ACO-1 DiagnosticLayer
3. Extend got.rs (COG-4)

### E2E Tests
CognitiveMCTS search over populated SemanticGraph -> verify graph priors used.
RefinementCycle with mock execute_fn that fails twice then succeeds.
GoT evaluation with multi-dimensional scoring.

## Checkpoint (.toon)

```
Save: {insights: [COG-1,COG-2,COG-4], new_files: ['cognitive_mcts.rs','refinement.rs'], modified_files: ['got.rs','lib.rs'], test_count_delta: +17}
File: checkpoints/phase-05-cog-advanced.toon
```

## Validation Criteria

- [x]src/cognitive_mcts.rs exists with CognitiveMCTS struct
- [x]cognitive_search method exists
- [x]src/refinement.rs exists with RefinementCycle struct
- [x]run_refinement method exists
- [x]evaluate_multidimensional method exists in got.rs
- [x]ThoughtResult has relevance, confidence, novelty fields
- [x]cargo clippy -p touring-cognitive -- -D warnings exits 0
- [x]cargo test -p touring-cognitive exits 0
- [x]All 17+ new tests pass

## Dependencies: phase-01, phase-02
