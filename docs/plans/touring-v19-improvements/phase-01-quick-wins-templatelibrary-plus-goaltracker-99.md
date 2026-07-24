---
plan_id: touring-v19-improvements
phase: 1
title: 'Quick Wins: TemplateLibrary + GoalTracker 9×9'
status: planned
created: '2026-03-25'
horizon: H1
insights_covered: [INS-1, INS-2]
depends_on_phases:
- 0
validates_with: validate_phase01.py
estimated_effort: 1 week
vgp_verified_structs: [DimResult, EvolutionPackage, LearnedPattern, TrackerReport]
---

# Quick Wins: TemplateLibrary + GoalTracker 9×9

> **Depends on**: Phase 0
> **Insights**: INS-1 (ROI=2.00), INS-2 (ROI=1.60)

## Objective

Implement INS-1 (TemplateLibrary with Learning, ROI=2.00) and INS-2 (GoalTracker 9×9 Computational, ROI=1.60). Highest ROI insights — ship first for early value. Both are self-contained in touring-learning.

## Final Result

New template_library.rs with TemplateLibrary. Modified tracker.rs with 81-check computational verification. 14 new tests.

## Impacts

| Dimension | Assessment |
|-----------|------------|
| performance | TemplateLibrary adds O(1) lookup amortized. GoalTracker adds 81 checks but batched. |
| maintainability | Templates persist cross-session — reduces repeated pattern extraction. |
| reliability | GoalTracker now has real evidence for scores, not math approximations. |
| daemon_latency | No daemon changes. Caller-side only. |

## Insight INS-1: TemplateLibrary with Learning

**Crate**: `touring-learning` | **Module**: `src/aco/template_library.rs`
**Action**: NEW | **Priority**: Q1 | **Effort**: 3 days | **ROI**: 2.00

### Description

Create TemplateLibrary that persists LearnedPatterns extracted by evolution.rs. Currently evolution.rs extracts templates per-session but discards them. TemplateLibrary stores them in SQLite via Touring knowledge graph, with similarity search, usage_count tracking, and version-controlled persistence.

### Implementation

```
New file src/aco/template_library.rs:
  struct TemplateEntry { pattern: LearnedPattern, usage_count: u32, last_used: u64 }
  struct TemplateLibrary { entries: HashMap<String, TemplateEntry>, version: u32 }
  impl TemplateLibrary:
    pub fn record_template(&mut self, pattern: LearnedPattern)
      → dedup by pattern_id, increment usage_count if exists
    pub fn find_similar(&self, domain: &str, tags: &[String]) -> Vec<&LearnedPattern>
      → filter by domain match + tag intersection score >= 0.5
    pub fn top_k(&self, k: usize) -> Vec<&LearnedPattern>
      → sort by usage_count desc, return first k
    pub fn persist(&self, store: &mut SymbolStore) -> Result<()>
      → serialize to JSON, store via touring_memory_store key "template::{pattern_id}"
    pub fn load(store: &SymbolStore) -> Result<Self>
      → recall all entries with prefix "template::"
  Integrate with evolution.rs:
    → In extract_learned_patterns(), call library.record_template(p) for each pattern
    → Library stored in EvolutionPackage.persistence_actions as metadata note

```

### VGP-Verified Structs

**LearnedPattern** (crates/touring-learning/src/aco/models.rs line 254):
- `pattern_id: String`
- `description: String`
- `generator_template: String`
- `domain: String`
- `tags: Vec<String>`
**EvolutionPackage** (crates/touring-learning/src/aco/models.rs line 272):
- `session_id: String`
- `objective_hash: String`
- `execution_report_json: String`
- `learned_patterns: Vec<LearnedPattern>`
- `anti_patterns_discovered: Vec<String>`
- `system_upgrades: Vec<SystemUpgrade>`
- `quality_metrics: HashMap<String, f64>`
- `persistence_actions: Vec<String>`

### Test Cases

- `test_record_template_new`
- `test_record_template_dedup_increments_usage`
- `test_find_similar_by_domain`
- `test_find_similar_by_tags`
- `test_top_k_returns_most_used`
- `test_top_k_empty_library`
- `test_persist_and_load_roundtrip`
- `test_version_bump_on_mutation`

### Synergies

- COG-MCTS: shared pattern store for MCTS expansion priors

## Insight INS-2: GoalTracker 9×9 Computational

**Crate**: `touring-learning` | **Module**: `src/aco/tracker.rs`
**Action**: MODIFY | **Priority**: Q1 | **Effort**: 2 days | **ROI**: 1.60

### Description

Replace scoring math approximations in GoalTracker with real 9×9 computational verification. Each of the 9 dimensions has 9 sub-checks (81 total). Currently tracker.rs uses hardcoded score formulas. New version performs actual verification via DimResult.checks_passed / checks_total with traceable details per check.

### Implementation

```
Modify src/aco/tracker.rs:
  Add struct CheckSpec { id: String, description: String, weight: f64 }
  Add const DIM_CHECKS: [(dim_id, [CheckSpec; 9]); 9]
    → 9 dimensions × 9 checks each = 81 specs
  Add fn verify_check(spec: &CheckSpec, context: &CheckContext) -> bool
    → real computation: file exists, symbol count >= threshold, etc.
  Add pub fn compute_dimensional_score_real(
      &mut self,
      context: &CheckContext,
  ) -> TrackerReport
    → iterates all 81 checks, fills DimResult.checks_passed/checks_total/details
  Add pub fn verify_9x9_matrix(&self, report: &TrackerReport) -> bool
    → asserts all 9 dims present, each with exactly 9 checks

```

### VGP-Verified Structs

**TrackerReport** (crates/touring-learning/src/aco/tracker.rs line 65):
- `dims: Vec<DimResult>`
- `composite: f64`
- `status: TrackerStatus`
- `iteration: u32`
**DimResult** (crates/touring-learning/src/aco/tracker.rs line 36):
- `dim_id: String`
- `name: String`
- `score: f64`
- `checks_passed: u32`
- `checks_total: u32`
- `details: Vec<String>`

### Test Cases

- `test_9x9_matrix_completeness`
- `test_compute_dimensional_score_passes_on_valid`
- `test_compute_dimensional_score_fails_on_missing_context`
- `test_verify_9x9_matrix_structure`
- `test_dim_result_checks_tracked`
- `test_tracker_report_composite_from_real_checks`

### Synergies

- AST FileHeat: heat-informed dimensional scoring context

## Subtasks

- [ ]S1.1: [INS-1] Write 8 tests for TemplateLibrary (TDD first)
- [ ]S1.2: [INS-1] Create src/aco/template_library.rs with TemplateLibrary struct (after S1.1)
- [ ]S1.3: [INS-1] Integrate with evolution.rs: call library.record_template() (after S1.2)
- [ ]S1.4: [INS-1] Register in src/aco/mod.rs: pub mod template_library (after S1.2)
- [ ]S1.5: [INS-2] Write 6 tests for GoalTracker 9×9 (TDD first)
- [ ]S1.6: [INS-2] Implement DIM_CHECKS const and verify_check() in tracker.rs (after S1.5)
- [ ]S1.7: [INS-2] Implement compute_dimensional_score_real() in tracker.rs (after S1.6)
- [ ]S1.8: [INS-2] Implement verify_9x9_matrix() in tracker.rs (after S1.7)
- [ ]S1.9: cargo clippy -p touring-learning -- -D warnings (after S1.4, S1.8)
- [ ]S1.10: cargo test -p touring-learning → expect 2152+14 = 2166 passed (after S1.9)

## DISCOVER Protocol

```
touring_ast_find(symbol_name='LearnedPattern', definitions_only=true) → verify fields
touring_ast_find(symbol_name='EvolutionPackage', definitions_only=true) → fields
touring_ast_find(symbol_name='TrackerReport', definitions_only=true) → fields
touring_ast_overview(file_path='crates/touring-learning/src/aco/tracker.rs')
touring_graph(action='blast_radius', symbol='DimResult') → impact scope
touring_memory_recall(query='goaltracker 9x9 computational template library', top_k=5)
```

## TDD Plan

Tests First (14 total):
  - template_library.rs: 8 tests before implementation
  - tracker.rs: 6 tests before compute_dimensional_score_real()
Implementation:
  1. template_library.rs (INS-1) — new file
  2. tracker.rs modifications (INS-2)
  3. mod.rs registration
  4. evolution.rs integration
E2E:
  Create EvolutionPackage → extract_learned_patterns → TemplateLibrary.record_template
  → find_similar → top_k → persist → load → assert roundtrip


## Checkpoint (.toon)

```python
# Save: {'insights': ['INS-1', 'INS-2'], 'new_tests': 14, 'new_files': ['template_library.rs']}
# File: checkpoints/phase-01-quick-wins.toon
import msgpack, pathlib
pathlib.Path('checkpoints/phase-01-quick-wins.toon').write_bytes(msgpack.packb({'insights': ['INS-1', 'INS-2'], 'new_tests': 14, 'new_files': ['template_library.rs']}))
```

## Validation Criteria

- [ ]src/aco/template_library.rs exists
- [ ]TemplateLibrary struct has record_template, find_similar, top_k, persist, load
- [ ]Evolution.rs calls library.record_template() in extract_learned_patterns
- [ ]tracker.rs has compute_dimensional_score_real() and verify_9x9_matrix()
- [ ]cargo clippy -p touring-learning → 0 warnings
- [ ]cargo test -p touring-learning → 14+ new tests passing

## Dependencies: phase-00
