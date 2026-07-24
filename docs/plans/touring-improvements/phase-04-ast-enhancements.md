---
plan_id: touring-improvements-2026
phase: 4
title: 'AST Enhancements: FileHeat, Fused Search, EnrichedBlastRadius'
status: completed
created: '2026-03-25'
horizon: H2
insights_covered:
- AST-1
- AST-2
- AST-3
depends_on_phases:
- 0
validates_with: validate_phase04.py
estimated_effort: 9 days
vgp_verified_structs:
- EnrichedBlastRadius
- FileHeat
- HeatMap
- ImpactCategory
- SemanticSymbolIndex
---

# AST Enhancements: FileHeat, Fused Search, EnrichedBlastRadius

## Objective

Implement 3 strategic improvements to touring-ast: FileHeat prioritized indexing, RRF-based fused symbol search, and enriched blast radius with impact categories.

## Final Result

New file file_heat.rs with HeatMap. SemanticSymbolIndex gains find_symbols_fused(). New EnrichedBlastRadius struct in graph.rs.

## Impacts

| Dimension | Assessment |
|-----------|------------|
| performance | FileHeat: O(n log n) sort for priority order. RRF: 3x ranking. Enriched: one extra BFS. |
| maintainability | HeatMap is self-contained. RRF fusion is composable (add signals later). |
| reliability | FileHeat prevents stale index. EnrichedBlastRadius gives better impact estimates. |
| daemon_latency | HeatMap may be used by daemon for indexing priority — measure after integration. |

## Subtasks

- [x]S4.1: [AST-1] Create src/file_heat.rs with FileHeat and HeatMap
- [x]S4.2: [AST-1] Implement record_edit, record_access, get_priority_order, decay_all
- [x]S4.3: [AST-1] Write 5 tests for FileHeat
- [x]S4.4: [AST-2] Add find_symbols_fused() to SemanticSymbolIndex
- [x]S4.5: [AST-2] Implement RRF formula with 3 signals
- [x]S4.6: [AST-2] Write 5 tests for fused search
- [x]S4.7: [AST-3] Add EnrichedBlastRadius and ImpactCategory to graph.rs
- [x]S4.8: [AST-3] Implement compute_enriched_blast_radius()
- [x]S4.9: [AST-3] Write 5 tests for enriched blast radius
- [x]S4.10: Register file_heat.rs in lib.rs
- [x]S4.11: cargo clippy -p touring-ast -- -D warnings
- [x]S4.12: cargo test -p touring-ast

## Insight Details

### AST-1: FileHeat Prioritized Indexing

**Crate**: `touring-ast` | **Module**: `src/file_heat.rs`
**Priority**: Q2 | **Effort**: 3 days

#### Description

Create a FileHeat system (digital pheromone) that tracks edit frequency, recency, and blast_radius.file_count per file. IncrementalPipeline uses FileHeat to prioritize re-indexing: hot files get indexed first after changes.

#### Implementation

```
New file src/file_heat.rs:
  struct FileHeat { edits: u32, last_edit_epoch: f64, access_count: u32, blast_radius_weight: f64 }
  struct HeatMap { entries: HashMap<String, FileHeat>, capacity: usize }
  impl HeatMap:
    fn record_edit(&mut self, file: &str, now: f64)
    fn record_access(&mut self, file: &str, now: f64)
    fn get_priority_order(&self, now: f64) -> Vec<(&str, f64)>
    fn decay_all(&mut self, now: f64, half_life_secs: f64)
  Heat score = edits * recency_decay * (1 + blast_radius_weight).
```

#### Test Cases

- `test_heat_increases_on_edit`
- `test_heat_decays_over_time`
- `test_priority_order_respects_heat`
- `test_blast_radius_boosts_heat`
- `test_heat_map_capacity_limit`

#### Synergies: COG-3

### AST-2: find_symbols_fused() with RRF

**Crate**: `touring-ast` | **Module**: `src/semantic_search.rs`
**Priority**: Q2 | **Effort**: 3 days

#### Description

Add find_symbols_fused() to SemanticSymbolIndex that combines 3 signals using Reciprocal Rank Fusion (RRF): (1) cosine similarity from existing find_similar_symbols, (2) co-edit frequency (from HeatMap), (3) blast radius file_count. RRF formula: score = sum(1/(k+rank_i)).

#### Implementation

```
Add to SemanticSymbolIndex:
  pub fn find_symbols_fused(
      &self,
      query: &Symbol,
      co_edit_scores: &HashMap<String, f64>,
      blast_radius_scores: &HashMap<String, f64>,
      k: f64,   // RRF constant, typically 60.0
      limit: usize,
  ) -> Vec<(String, f64)>
  Ranks by similarity, co-edit, blast_radius independently.
  Fuses with RRF: score = 1/(k+rank_sim) + 1/(k+rank_coedit) + 1/(k+rank_blast).
  Sorted descending by fused score.
```

#### Test Cases

- `test_fused_search_combines_signals`
- `test_fused_search_rrf_formula`
- `test_fused_search_empty_index`
- `test_fused_search_single_signal`
- `test_fused_search_respects_limit`

#### Synergies: COG-1

### AST-3: EnrichedBlastRadius with Impact Categories

**Crate**: `touring-ast` | **Module**: `src/graph.rs`
**Priority**: Q2 | **Effort**: 3 days

#### Description

Extend BlastRadius (graph.rs line 432) with categorized impact: direct_dependents, transitive_dependents, co_edited_files. Add a severity score based on weighted categories.

#### Implementation

```
Add to src/graph.rs:
  enum ImpactCategory { DirectDependents, TransitiveDependents, CoEdited }
  struct EnrichedBlastRadius {
      base: BlastRadius,  // reuse existing (start_file, affected_files, affected_symbols, max_distance, file_count)
      direct_dependents: Vec<String>,
      transitive_dependents: Vec<String>,
      co_edited_files: Vec<String>,
      severity: f64,  // 0.0-1.0
  }
  fn compute_enriched_blast_radius(
      index: &SymbolIndex,
      file: &str,
      co_edit_data: &HashMap<String, Vec<String>>,
  ) -> EnrichedBlastRadius
  severity = 0.5*direct_count/total + 0.3*transitive_count/total + 0.2*coedit_count/total.
```

#### Test Cases

- `test_enriched_blast_radius_direct_deps`
- `test_enriched_blast_radius_transitive_deps`
- `test_enriched_blast_radius_severity_score`
- `test_enriched_empty_graph`
- `test_category_weights_sum_to_one`

## DISCOVER Protocol

```
1. Read semantic_search.rs for SemanticSymbolIndex interface
2. Read graph.rs BlastRadius (line 432) for existing fields
3. Read lib.rs for module registration pattern
4. touring_graph(blast_radius, SemanticSymbolIndex)
5. Check SymbolIndex reverse_deps for transitive computation
```

## TDD Plan

### Tests First
15 test cases across 3 insights BEFORE implementation.

### Implementation
1. file_heat.rs (AST-1)
2. Extend semantic_search.rs (AST-2)
3. Extend graph.rs (AST-3)

### E2E Tests
Record edits -> get priority order -> search with fused signals -> compute enriched blast radius -> verify categories.

## Checkpoint (.toon)

```
Save: {insights: [AST-1,AST-2,AST-3], new_files: ['file_heat.rs'], modified_files: ['semantic_search.rs','graph.rs','lib.rs'], test_count_delta: +15}
File: checkpoints/phase-04-ast-enhancements.toon
```

## Validation Criteria

- [x]src/file_heat.rs exists with FileHeat and HeatMap structs
- [x]find_symbols_fused method exists in semantic_search.rs
- [x]EnrichedBlastRadius struct exists in graph.rs
- [x]ImpactCategory enum exists in graph.rs
- [x]compute_enriched_blast_radius function exists in graph.rs
- [x]cargo clippy -p touring-ast -- -D warnings exits 0
- [x]cargo test -p touring-ast exits 0
- [x]All 15+ new tests pass

## Dependencies: phase-00
