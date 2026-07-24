# Action-Suggestion Bidirectional Channel — Pln3

> **Date**: 2026-04-13 | **Phase**: Pln3-P5 | **Status**: Shipped

---

## §1 Motivação

Pln2 introduziu o primeiro canal bidirecional: Touring cria tasks e Claude Code as adota via
`external_ref` (fluxo de cima para baixo). Pln3 adiciona um **segundo canal independente**:
Touring *observa* o estado de execução (subtasks presas, falhas repetidas, complexidade L4+)
e *sugere ações* para CC reagir (fluxo de baixo para cima).

| Canal | Semântica | Tabela | Flag de controle |
|---|---|---|---|
| **Pln2** (origin) | "Touring cria task, CC adota" | `task_decompositions` | `origin` / `mirrored_to_cc` |
| **Pln3** (action) | "Touring observa, CC reage" | `cc_action_suggestions` | `action_type` / `consumed` |

O canal Pln3 resolve o problema do **loop infinito de sugestões**: uma vez que CC consome
uma sugestão (via `TaskUpdate/TaskStop/PlanMode` com `suggestion_ref`), o campo `consumed=1`
impede que o digest a resurface na próxima sessão.

---

## §2 Arquitetura

```
instructions_loaded
    │
    ├── detect_and_suggest_stuck_subtasks()   ─── StuckSubtaskSuggester
    ├── detect_and_suggest_failing_tasks()    ─── FailureThresholdSuggester
    └── detect_and_suggest_plan_mode()        ─── PlanModeSuggester
                │
                └── run_suggester()  ← bidirectional::suggester (trait driver)
                        │
                        ├── Suggester::detect()  (read-only, pure)
                        ├── storage::has_pending_suggestion()  (dedup/rate-limit)
                        └── storage::insert_suggestion()  (write)

session-start / instructions_loaded
    └── digest_pending_action_suggestions(rt, action_type)
            └── cc_action_suggestions WHERE consumed=0 → additionalContext

task-sync-post-update / task-sync-post-stop / enter-plan-mode (CC hooks)
    └── cli_suggestion_mark_consumed(suggestion_id)
            └── consumed=1  ← LOOP BROKEN
```

---

## §3 Schema delta (`cc_action_suggestions`)

```sql
CREATE TABLE IF NOT EXISTS cc_action_suggestions (
    suggestion_id     TEXT PRIMARY KEY,          -- "sugg_<nanos>"
    action_type       TEXT NOT NULL,             -- "update" | "stop" | "plan_mode"
    target_task_id    TEXT NOT NULL,             -- FK to task_decompositions.task_id
    target_subtask_id TEXT,                      -- NULL = task-level suggestion
    reason            TEXT NOT NULL,             -- human-readable explanation
    evidence_json     TEXT NOT NULL DEFAULT '{}',-- structured JSON evidence
    suggested_at      TEXT NOT NULL,             -- RFC-3339 (chrono::Utc::now())
    consumed          INTEGER NOT NULL DEFAULT 0,-- 0=pending, 1=consumed
    consumed_at       TEXT,                      -- set when consumed
    consumed_action   TEXT                       -- e.g. "TaskUpdate", "TaskStop"
);
```

The table is **lazy-created** by `ensure_decompose_tables()` (called inside CLI handlers),
so no migration is required on existing DBs.

---

## §4 Suggester trait API

```rust
// crates/touring-hooks/src/bidirectional/suggester.rs

pub struct PendingSuggestion {
    pub target_task_id: String,
    pub target_subtask_id: Option<String>,
    pub reason: String,
    pub evidence: serde_json::Value,
}

pub trait Suggester {
    /// The constant action type emitted by this detector.
    /// Must be one of: "update" | "stop" | "plan_mode"
    fn action_type(&self) -> &'static str;

    /// Pure detection — no writes. Return vec![] on any error.
    fn detect(&self, rt: &HookRuntime) -> Vec<PendingSuggestion>;
}

/// Framework driver: detect → dedup → persist → return count inserted.
/// De-dup key: (action_type, target_task_id, target_subtask_id).
pub fn run_suggester<S: Suggester>(suggester: &S, rt: &HookRuntime) -> usize;
```

**Contract**:
- `detect` MUST be pure (no DB writes).
- `detect` MUST NOT panic — return `vec![]` on any error.
- De-duplication and persistence are handled by `run_suggester` so each concrete
  implementor only needs `action_type` + `detect`.

---

## §5 3 Concretos

### StuckSubtaskSuggester (`suggesters/stuck_subtask.rs`)

| Property | Value |
|---|---|
| `action_type` | `"update"` |
| Trigger | subtask `status IN ('pending','ready')` AND `updated_at < now - 30 min` |
| Evidence | `{"subtask_id", "status", "minutes_stuck"}` |
| Consumption hook | `handle_task_sync_post_update` |

```rust
pub fn detect_and_suggest_stuck_subtasks(runtime: &HookRuntime) -> usize;
```

### FailureThresholdSuggester (`suggesters/failure_threshold.rs`)

| Property | Value |
|---|---|
| `action_type` | `"stop"` |
| Trigger | subtask `status = 'failed'` OR `attempts > 3` |
| Evidence | `{"subtask_id", "status", "attempts"}` |
| Consumption hook | `handle_task_sync_post_stop` |

```rust
pub fn detect_and_suggest_failing_tasks(runtime: &HookRuntime) -> usize;
```

Note: in non-test builds, also emits a sentinel suggestion when the circuit breaker
is open (`#[cfg(not(test))]` branch in `detect_and_suggest_failing_tasks`).

### PlanModeSuggester (`suggesters/plan_mode_complexity.rs`)

| Property | Value |
|---|---|
| `action_type` | `"plan_mode"` |
| Trigger | task `cila_level >= 4` OR keyword density ≥ 2 in description, created within 5 min |
| Evidence | `{"task_id", "cila_level", "keyword_matches"}` |
| Consumption hook | `handle_enter_plan_mode` |

```rust
pub fn detect_and_suggest_plan_mode(runtime: &HookRuntime) -> usize;
```

Keyword set: `architecture`, `refactor major`, `migration`, `multi-service`,
`distributed`, `schema migration`, `breaking change`, `multi-step`, ...

---

## §6 Anti-loop invariantes

Three invariants prevent infinite suggestion loops:

1. **Rate-limit (dedup)**: `has_pending_suggestion(action_type, task_id, subtask_id)`
   checks for an existing unconsumed row before inserting. A second detector run for
   the same key inserts nothing — `run_suggester` returns 0.

2. **Consumed flag**: `cli_suggestion_mark_consumed(suggestion_id)` flips `consumed=1`.
   The digest query (`WHERE consumed = 0`) will never resurface the row.

3. **`digest_pending_action_suggestions` guard**: if the table doesn't exist yet,
   the function returns `None` immediately — no error propagates to the session start hook.

Together: `suggest → digest → CC acts with suggestion_ref → mark consumed → digest gone`.

---

## §7 Arquivos afetados

| Path | Mudança | LOC delta |
|---|---|---|
| `crates/touring-hooks/src/bidirectional/suggester.rs` | Trait `Suggester` + `PendingSuggestion` + `run_suggester` driver | +180 |
| `crates/touring-hooks/src/bidirectional/storage.rs` | `has_pending_suggestion` + `insert_suggestion` helpers | +90 |
| `crates/touring-hooks/src/bidirectional/mod.rs` | Re-exports: `PendingSuggestion`, `Suggester` | +5 |
| `crates/touring-hooks/src/suggesters/stuck_subtask.rs` | `StuckSubtaskSuggester` impl + `detect_and_suggest_stuck_subtasks` | +220 |
| `crates/touring-hooks/src/suggesters/failure_threshold.rs` | `FailureThresholdSuggester` impl + `detect_and_suggest_failing_tasks` | +215 |
| `crates/touring-hooks/src/suggesters/plan_mode_complexity.rs` | `PlanModeSuggester` impl + `detect_and_suggest_plan_mode` | +200 |
| `crates/touring-hooks/src/suggesters/mod.rs` | Module declarations | +15 |
| `crates/touring-hooks/src/cli_handlers.rs` | `cli_suggest_action`, `cli_suggestion_mark_consumed`, `cli_suggestion_list_pending` | +130 |
| `crates/touring-hooks/src/task_digest.rs` | `digest_pending_action_suggestions(rt, action_type)` | +60 |
| `crates/touring-hooks/src/lib.rs` | `pub mod suggesters`, `pub mod bidirectional` | +2 |
| `crates/touring-hooks/tests/bidirectional_suggestions_e2e.rs` | **This file — 13 E2E tests** | +550 |

---

## §8 Testes E2E (`bidirectional_suggestions_e2e.rs`)

**13 tests, 0 failed** — added 2026-04-13 (Pln3-P5).

Storage layer:

1. `cli_suggest_action_inserts_row_with_defaults` — happy path, verifies `inserted=true` + `sugg_` prefix + `suggested_at`
2. `cli_suggest_action_rejects_empty_fields` — 4 cases: missing action_type, target_task_id, reason, empty payload
3. `cli_suggestion_mark_consumed_flips_flag` — insert → mark consumed → verify `rows_updated=1` + absent from pending list
4. `cli_suggestion_mark_consumed_unknown_id_returns_zero` — `rows_updated=0`, `marked=false`
5. `cli_suggestion_list_pending_filters_by_action_type` — 3 update + 2 stop inserted; filter by type returns exact count
6. `cli_suggestion_list_pending_omits_consumed` — 3 inserted, 2 consumed, 1 remaining in list

Detectors:

7. `stuck_subtask_detector_finds_old_pending_tasks` — seed subtask `updated_at - 31 min`, assert count ≥ 1
8. `stuck_subtask_detector_respects_rate_limit` — second run returns 0 (dedup by key)
9. `failure_threshold_detector_finds_failed_tasks` — `status=failed` + `attempts=4` → stop suggestion
10. `plan_mode_detector_classifies_complex_description` — description with ≥ 2 keywords + `cila_level=4` → plan_mode suggestion

Digest + loop-break:

11. `digest_action_suggestions_returns_only_requested_type` — update/stop inserted; filter returns only requested type; plan_mode returns None
12. `digest_action_suggestions_returns_none_when_empty` — empty DB returns None for all three action types
13. `full_suggestion_to_consumption_cycle_breaks_loop` — full E2E: create task → suggest → digest shows → mark consumed → digest None → pending list empty

---

## §9 Próximos consumers

| Consumer | Trigger | action_type | Notes |
|---|---|---|---|
| ANTT CILA adaptive | CILA level change mid-task | `update` | Re-estimate complexity on scope change |
| Evolution drift | `touring evolution drift` = structural | `stop` | Halt task execution during structural degradation |
| MCTS subgoals | MCTS search reveals better subtask split | `update` | Suggest subtask restructure |
| Session quality gate | Composite score < 0.7 at session assess | `plan_mode` | Force re-planning on low quality sessions |
| Tantivy signal | Symbol referenced in task not found in index | `update` | VGP-style pre-implementation check |
