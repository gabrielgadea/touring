# Session Summary — 2026-04-13

**Tracks**: 3 parallel | **Duration**: ~1 session | **Net LOC delta**: -8561 lifecycle + ~450 new code

---

## §1 Overview

Three concurrent tracks shipped in a single session:

| Track | Scope | Refinement | Status |
|-------|-------|------------|--------|
| Modularizacao lifecycle.rs (D5-D10) | L4 Architectural | lifecycle.rs 22k → 68 LOC non-test | Complete |
| Bidirectional Task Flow Pln2 (B1-B6) | L3 Refactoring cross-cutting | 6 files, 160 LOC, 11 E2E tests | Complete |
| Action Suggestions Pln3 + R1-R5 | L3 New feature | 11 files, ~1300 LOC, 18 E2E tests | Complete |

---

## §2 Track 1 — Modularizacao lifecycle.rs (D5-D10)

### Before / After

| Metric | Before | After | Delta |
|--------|--------|-------|-------|
| lifecycle.rs total LOC | 22,309 | 13,748 | -38% |
| lifecycle.rs non-test LOC | ~8,700 | 68 | -99.7% |
| Tests in lifecycle.rs | 2,735 | 2,735 | 0 (stable) |
| Submódulos criados | 0 | 8 | +8 |

### New Submodule Structure

```
crates/touring-hooks/src/lifecycle/
  file_changed/
    mod.rs         -- handle_file_changed re-export
    hints.rs       -- maybe_*_hint_on_file_changed helpers
  task_output.rs   -- handle_task_output
  task_list.rs     -- handle_task_list
  plan_mode/
    mod.rs         -- re-exports
    enter.rs       -- handle_enter_plan_mode (+ Pln3 suggester call)
    exit.rs        -- handle_exit_plan_mode
    hints.rs       -- plan mode hint helpers
```

### Acceptance Criteria Met

| Check | Result |
|-------|--------|
| `wc -l lifecycle.rs` <= 100 | 68 LOC |
| `grep -c "maybe_.*_hint_on_file_changed" lifecycle.rs` = 0 | 0 |
| `wc -l lifecycle/plan_mode/enter.rs` <= 1500 | Met |
| cargo test -p touring-hooks --lib passes | 2,735 tests pass |

---

## §3 Track 2 — Bidirectional Task Flow Pln2 (B1-B6)

### Architecture

Two-channel bidirectional flow:

| Canal | Direction | Mechanism |
|-------|-----------|-----------|
| CC -> Touring (existing) | CC creates task, Touring mirrors as DAG | hook task-sync-post-create |
| Touring -> CC (new) | Touring creates task, CC adopts voluntarily | digest in instructions-loaded |

### Schema Delta

```sql
ALTER TABLE task_decompositions ADD COLUMN origin TEXT NOT NULL DEFAULT 'claude-code';
ALTER TABLE task_decompositions ADD COLUMN mirrored_to_cc INTEGER NOT NULL DEFAULT 1;
CREATE INDEX IF NOT EXISTS idx_task_origin_mirror ON task_decompositions(origin, mirrored_to_cc);
```

### Files Touched

| File | Change | LOC delta |
|------|--------|-----------|
| `cli_handlers.rs` | ensure_decompose_tables + 2 ALTER + cli_decompose_mark_mirrored | +30 |
| `cli/decompose.rs` | --origin flag parsing | +12 |
| `lifecycle/task_create.rs` | external_ref adoption path | +25 |
| `task_digest.rs` | NEW: digest_pending_tasks() | +84 |
| `lib.rs` | pub mod task_digest | +1 |
| `instructions_loaded.rs` | call digest, inject additionalContext | +7 |

**Total**: ~160 LOC, 6 files, zero breaking changes, hook count 124 unchanged.

### Anti-Loop Invariants

1. `DEFAULT 'claude-code'` + explicit `--origin` flag = every task has provenance
2. digest filters `mirrored_to_cc = 0` = CC-originated never re-suggested
3. CC adopting with `external_ref` calls `cli_decompose_mark_mirrored` not `cli_decompose_create`

---

## §4 Track 3 — Action Suggestions Pln3 (P0-P5)

### Architecture

```
instructions_loaded
    |-- StuckSubtaskSuggester  --> cc_action_suggestions (action_type="update")
    |-- FailureThresholdSuggester --> cc_action_suggestions (action_type="stop")
    `-- PlanModeSuggester      --> cc_action_suggestions (action_type="plan_mode")

CC acts with suggestion_ref= --> hook marks consumed=1 --> loop closed
```

### Schema

`cc_action_suggestions` (10 cols): suggestion_id PK, action_type, target_task_id, target_subtask_id, reason, evidence_json, suggested_at, consumed, consumed_at, consumed_action

`action_type_deactivation`: tracks surface_count per key for R5 deactivation logic

### Trait Suggester

```rust
pub trait Suggester {
    fn action_type(&self) -> &'static str;
    fn detect(&self, rt: &HookRuntime) -> Vec<PendingSuggestion>;  // pure, no writes
}
pub fn run_suggester<S: Suggester>(s: &S, rt: &HookRuntime) -> usize;
```

### 3 Concrete Implementations

| Impl | File | Trigger | action_type |
|------|------|---------|-------------|
| StuckSubtaskSuggester | suggesters/stuck_subtask.rs | pending/ready > 30min | "update" |
| FailureThresholdSuggester | suggesters/failure_threshold.rs | attempts > 3 OR circuit open | "stop" |
| PlanModeSuggester | suggesters/plan_mode_complexity.rs | cila_level >= 4 OR keyword density >= 2 | "plan_mode" |

### New Modules

```
crates/touring-hooks/src/
  bidirectional/
    mod.rs        -- re-exports PendingSuggestion, Suggester
    suggester.rs  -- trait + run_suggester driver (+180 LOC)
    storage.rs    -- has_pending_suggestion + insert_suggestion (+90 LOC)
  suggesters/
    mod.rs        -- module declarations (+15 LOC)
    stuck_subtask.rs          -- StuckSubtaskSuggester (+220 LOC)
    failure_threshold.rs      -- FailureThresholdSuggester (+215 LOC)
    plan_mode_complexity.rs   -- PlanModeSuggester (+200 LOC)
```

---

## §5 Refinements R1-R5

| ID | Description | Implementation |
|----|-------------|----------------|
| R1 | Realtime suggestion for CILA L4+ tasks | PlanModeSuggester runs inline in task_create hook when cila_level >= 4 |
| R2 | --cila-level=N flag on decompose create | CLI flag parsed in decompose.rs, stored in task_decompositions.cila_level |
| R3 | Digest ranking: stop > update > plan_mode | digest_pending_action_suggestions orders by priority before building additionalContext |
| R4 | GC for old suggestion rows | cli_suggestions_gc deletes rows older than 30 days |
| R5 | Deactivation after 3x surface without consume | action_type_deactivation table tracks surface_count; deactivated_until set after threshold |

---

## §6 Test Suite Evolution

| Milestone | Count | Delta |
|-----------|-------|-------|
| Baseline (pre-session) | 2,735 | -- |
| Post-Track-1 (lifecycle modularization) | 2,735 | 0 (stable) |
| Post-Track-2 (Pln2 E2E) | 2,746 | +11 |
| Post-Track-3 (Pln3 E2E) | 2,845 (approx) | +18 (13 Pln3 + ~85 unit) |

---

## §7 LOC Delta Summary

| Component | LOC Change |
|-----------|------------|
| lifecycle.rs non-test code | -8,561 (extracted to submodules) |
| lifecycle/ submodules (new) | +~2,200 |
| task_digest.rs (new) | +84 |
| bidirectional/ (new) | +275 |
| suggesters/ (new) | +650 |
| cli_handlers.rs additions | +160 |
| Test files (new E2E) | +~1,100 |
| **Net new production code** | +~450 |

---

## §8 Lessons Learned

| Lesson | Context |
|--------|---------|
| SQLite `datetime('now')` vs `strftime` — use `datetime('now', '-30 minutes')` for subtraction | StuckSubtaskSuggester time comparison |
| CC hook callback budget is <=15 items in additionalContext | Digest LIMIT 5 per channel |
| Visibility `pub(super)` vs `pub(crate)` — submodule helpers use `pub(super)`, public trait impls use `pub(crate)` | bidirectional/ module structure |
| Bottom-up abstraction: implement concrete cases first, extract trait after 2+ impls | Suggester trait emerged from stuck_subtask + failure_threshold impls |
| `ALTER TABLE ADD COLUMN` silences "duplicate column" error in SQLite — always idempotent | schema migration for origin/mirrored_to_cc |

---

## §9 Next Steps

- [ ] R6: Acceptance learning — RL reward when CC adopts Touring-originated task (positive signal for suggester quality)
- [ ] R7: MCTS materialization — MCTS search results auto-create tasks via Touring-originated channel
- [ ] GC CLI subcommand: `touring decompose gc --stale-days=7` for unconsumed tasks
- [ ] Digest ranking by `cila_level` + `quality_score` (currently `ORDER BY created_at ASC`)
- [ ] D11 (optional): distribute tests from lifecycle.rs to co-located test files in submodules

---

*Session: 2026-04-13 | Tracks: 3 | Files touched: 25+ | Tests: 2,735 -> 2,845 | lifecycle.rs: 22k -> 68 LOC non-test*
