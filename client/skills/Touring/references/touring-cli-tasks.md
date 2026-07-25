# Touring CLI — Task Lifecycle

> **Module**: 4/7 | **Version**: v4.9 | **Touring**: v30.3.0
> **Series**: Touring CLI Reference (consulta sob demanda) — `~/.claude/skills/Touring/references/touring-cli-*.md`
> **Index** (auto-load): `~/.claude/rules/touring-cli-index.md` (CLI RANKS Tier 4, 9)

Lifecycle de trabalho: session, decompose (DAG + workflow B3-B6 + bidirectional suggestions), diary (agent memory W6), memory (knowledge graph), tantivy FTS (BM25 sobre symbols).

---

## 4. Session / Checkpoint

| Comando | Hook Handler | Descrição |
|---------|-------------|-----------|
| `touring session start [id] [type] [objective]` | `cli-session-start` | Inicia nova sessão |
| `touring session checkpoint <id> [data]` | `cli-session-checkpoint` | Salva checkpoint |
| `touring session list` | `cli-session-list` | Lista sessões ativas |
| `touring session assess [id]` | `cli-session-assess` | Avalia qualidade da sessão |

## 5. Decompose (DAG de tarefas)

| Comando | Hook Handler | Descrição |
|---------|-------------|-----------|
| `touring decompose create <type> [desc] [--origin=<val>] [--cila-level=N]` | `cli-decompose-create` | Cria DAG de tarefa. `--origin` define provenance (default `"touring-cli"` via CLI, `"claude-code"` via hook). `--cila-level=N` propaga CILA complexity level (R2, Pln3). |
| `touring decompose add <task_id> <subtask_id> [desc]` | `cli-decompose-add` | Adiciona subtarefa |
| `touring decompose get <task_id>` | `cli-decompose-get` | Obtém status do DAG |
| `touring decompose update <task_id> <status>` | `cli-decompose-update` | Atualiza status |
| `touring decompose validate <task_id>` | `cli-decompose-validate` | Valida ciclos no DAG |
| `touring decompose status` | `cli-decompose-status` | Status geral do decompose |
| `touring decompose finalize <task_id> [quality_threshold]` | `cli-decompose-finalize` | Verifica se todos os subtasks são terminais, arquiva a task, injeta RL reward (1.0) via fire-and-forget. Retorna `{ready, archived, completion_pct, total_subtasks, completed, failed, skipped, cancelled, pending, in_progress, blocking, rl_reward_injected}`. Não arquiva se houver subtasks em `pending` ou `in_progress` (lista em `blocking`). |
| `touring decompose ready [task_id]` | `cli-decompose-ready` | Lista subtasks com status `pending` cujos todos os deps estão `completed`. `task_id` é filtro opcional. Retorna `{ready_count, ready_subtasks: [{task_id, subtask_id, description}]}`. Usa query parametrizada `(task_id = ?1 OR ?1 = '')` para unificar filtered/unfiltered. |

### 5b. Workflow CLI (Cronflow-Inspired — 2026-04-24)

Workflow commands para execução visualizada de tasks com streaming events e ANSI colors.

| Comando | Hook Handler | Descrição |
|---------|-------------|-----------|
| `touring workflow run <task_id> [--color]` | `cli-workflow-run` | Executa task com eventos de streaming (B3). Quando `--color` é passado, inclui campo `summary.colored` com ANSI escapes. Retorna `{event, task, subtasks, events, summary}` onde `events` = `[task_start, ...subtask_start, task_complete]`. |
| `touring workflow status <task_id>` | `cli-workflow-status` | Poll status de task. Retorna contadores agregados (completed/failed/pending/in_progress) + lista de subtasks com IDs escopados `task_id::subtask_id`. |
| `touring workflow resume <task_id>` | `cli-workflow-resume` | Retoma task após crash/interrupção. IDs de subtask usam formato `task_id::subtask_id` para JOINs corretos. |
| `touring workflow stats <task_id>` | `cli-workflow-stats` | Estatísticas de execução: `total_duration_ms`, `avg_duration_ms`, `cache_hit_rate`. |
| `touring workflow slowest <task_id> [--top N]` | `cli-workflow-slowest` | Top N subtasks mais lentos com duração e cache_hit status. |
| `touring workflow compare <task_id_a> <task_id_b>` | `cli-workflow-compare` | Compara métricas entre duas tasks. |

**B3 Streaming Events Response** (`cli-workflow-run`):
```json
{
  "event": "workflow_start",
  "task": { "task_id": "...", "description": "..." },
  "subtasks": [...],
  "events": [
    {"event": "task_start", "task_id": "task_123", "timestamp": "2026-04-24T10:30:00Z"},
    {"event": "subtask_start", "subtask_id": "s1", "description": "...", "timestamp": "..."},
    {"event": "subtask_start", "subtask_id": "s2", "description": "...", "timestamp": "..."},
    {"event": "task_complete", "task_id": "task_123", "timestamp": "..."}
  ],
  "summary": {
    "colored": "\x1b[1;32m▶\x1b[0m task_123 \x1b[90m(workflow run)\x1b[0m",
    "raw": "task_123"
  }
}
```

**B6 ANSI Color Rendering** (`cli-workflow-run --color`):
- Campo `summary.colored` contém ANSI escape codes crus (`\x1b[...]`)
- CLI client rendering: `\x1b[1;32m` = bold green, `\x1b[0m` = reset
- Simbologia: `▶` (play) = running, `[✓]` = complete, `[✗]` = failed

**B5 Resume Scoped IDs**: Subtask IDs usam `task_id::subtask_id` (ex: `task_123::sub_1`) para evitar colisão em JOINs SQL.

**MCP Actions (`touring_decompose`)**: além das ações CRUD existentes, agora suporta:

| Action | Params | Descrição |
|--------|--------|-----------|
| `validate_completion` | `task_id`, `subtask_id`, `quality_threshold?` | Gate pré-conclusão: verifica se todos os deps do subtask estão `completed`. Retorna `{ready_to_complete, blocking_reasons, pending_deps}`. Advisory para L3+ tasks. |
| `finalize` | `task_id`, `quality_threshold?` | Arquiva task via MCP (usa in-memory `TaskDecomposer`): itera subtasks, conta por bucket, deleta task se pronta, injeta RL reward via `tokio::spawn`. |

### 5c. Bidirectional Action Suggestions (Pln2+Pln3, 2026-04-13)

Internal API in `cli_handlers.rs` — not yet exposed as CLI subcommands. Callable via daemon hook payload.

| Handler (internal) | Hook Handler | Descrição |
|---------------------|-------------|-----------|
| `cli_suggest_action` | inline in `cli_handlers` | Insere sugestão em `cc_action_suggestions`. Payload: `{action_type, target_task_id, target_subtask_id?, reason, evidence_json?}`. Retorna `{inserted, suggestion_id}`. De-dup por `(action_type, task_id, subtask_id)` — idempotente. |
| `cli_suggestion_mark_consumed` | inline in `cli_handlers` | Flip `consumed=1` para fechar loop. Payload: `{suggestion_id, consumed_action?}`. Retorna `{marked, rows_updated}`. |
| `cli_suggestion_list_pending` | inline in `cli_handlers` | Lista sugestões `consumed=0`, filtro por `action_type`. Retorna `{count, suggestions: [{suggestion_id, action_type, target_task_id, reason, suggested_at}]}`. |
| `cli_suggestions_gc` | inline in `cli_handlers` | Remove rows com `suggested_at < now - 30d` (R4). Retorna `{deleted_count}`. |
| `cli_decompose_mark_mirrored` | inline in `cli_handlers` | UPDATE `mirrored_to_cc=1` para task adotada por CC (Pln2). Payload: `{task_id}`. |

**Schema tables** (lazy-created by `ensure_decompose_tables`):

`cc_action_suggestions`: `suggestion_id PK`, `action_type`, `target_task_id`, `target_subtask_id?`, `reason`, `evidence_json`, `suggested_at`, `consumed`, `consumed_at`, `consumed_action`

`action_type_deactivation`: per-key `surface_count`; after 3× surface without consume → 24h pause via `deactivated_until` (R5)

**Trait Suggester** (`crates/touring-hooks/src/bidirectional/suggester.rs`):
- `StuckSubtaskSuggester` — pending/ready > 30min → `"update"` suggestion
- `FailureThresholdSuggester` — attempts > 3 OR circuit open → `"stop"` suggestion
- `PlanModeSuggester` — `cila_level >= 4` OR keyword density >= 2 → `"plan_mode"` suggestion

**Integration point**: hook `instructions-loaded` runs all 3 detectors then surfaces pending suggestions in `additionalContext`. Digest ranking: `stop > update > plan_mode` (R3).

**Anti-loop**: dedup on insert + `consumed=1` after CC acts + `WHERE consumed=0` in digest query.

### 5b. Diary (Agent Memory — PLN2 P4.1)

| Comando | Handler | Descrição |
|---------|---------|-----------|
| `touring diary write <agent> <entry> [--topic <topic>] [--aaak]` | `cli-diary` (direct) | Escreve entry no diary do agent |
| `touring diary write <agent> <entry> --project <p> [--task <id>] [--subtask <id>]` | `cli-diary` (direct) | W6: entry com escopo de projeto/task/subtask |
| `touring diary read <agent> [--last N] [--topic <topic>]` | `cli-diary` (direct) | Lê entries do diary |
| `touring diary read <agent> --project <p> [--task <id>]` | `cli-diary` (direct) | W6: filtra entries por projeto ou projeto+task |
| `touring diary list` | `cli-diary` (direct) | Lista agents com diaries (probes known patterns) |
| `touring diary meta <agent>` | `cli-diary` (direct) | Mostra metadata do diary |
| `touring diary projects <agent>` | `cli-diary` (direct) | W6: lista todos os projetos do agente |

**Key Hierarchy (W6)**: `wing_{agent}/diary/{meta,entries/{ts},topics/{topic},projects/{p}/entries/{ts},projects/_index}`

**AAAK Format**: `#[P:phase] #[R:0.85] #[L:lesson] #[W:warn] #[E:error]`

**CLI Pattern**: Direct `MemoryStore::new()` — sem daemon socket (isola schema `memory.db` do `rlm_memory.db`)

## 15. Memory (Knowledge Graph)

| Comando | Hook Handler | Descrição |
|---------|-------------|-----------|
| `touring memory stats` | `cli-memory-stats` | Estatísticas do BD |
| `touring memory recall <query>` | `cli-memory-recall` | Consulta entries |
| `touring memory store <key> <value> [--tier T] [--type T]` | `cli-memory-store` | Persiste entry (tier: semantic/local, type: lesson/pattern/insight/gotcha) |
| `touring memory list [--limit N] [--sort F]` | `cli-memory-list` | Lista entries (default: limit=20, sort=access_count) |

### 15b. Tantivy Full-Text Search (Wave 2-4, 2026-04-12)

| Comando | Hook Handler | Descrição |
|---------|-------------|-----------|
| `touring tantivy search <query> [top_k]` | `cli-tantivy-search` | BM25 full-text search over symbols. Returns ranked hits. |
| `touring tantivy fuzzy <query> [distance] [top_k]` | `cli-tantivy-fuzzy` | Fuzzy search with edit distance tolerance (default: 2). |
| `touring tantivy stats` | `cli-tantivy-stats` | Index health: total_docs, index_size_bytes, pending_ops, total_commits, total_upserts. |
| `touring tantivy suggest <prefix> [top_k]` | `cli-tantivy-suggest` | Prefix-based autocomplete for symbol names. |
| `touring tantivy reindex` | `cli-tantivy-reindex` | Rebuild Tantivy index from symbol store. |

**Schema v2** (15 campos): symbol_name, file_path, symbol_kind, module_path, docstring (FTS), line_number (fast u64), language, visibility, crate_name, blake3_hash, import_count, export_count, cognitive_score_x1000, functional_signature (FTS).

**Index dir**: `~/.claude/touring/tantivy/`

**Feature gate**: `tantivy-fts` (default ON in touring-hooks).

**Memory recall enrichment** (U17): `touring memory recall` agora inclui `symbol_context` com Tantivy BM25 hits enriquecendo os resultados de text search.

## 19. Tantivy FTS (5 subcommands — detalhado)

Implementado em `touring-server/src/cli/tantivy.rs`. BM25 full-text search sobre symbol index.

| Comando | Descrição |
|---------|-----------|
| `touring tantivy search "<query>" [-j]` | BM25 ranked search |
| `touring tantivy fuzzy "<query>" [distance] [-j]` | Levenshtein fuzzy match |
| `touring tantivy stats [-j]` | Index health metrics |
| `touring tantivy suggest "<prefix>" [-j]` | Prefix autocomplete |
| `touring tantivy reindex [flags]` | **Batched** client-side reindex. Flags: `--batch-size N` (default 25000), `--resume N` (offset, skip clear), `--full` (legacy single-call). CLI loops across batches of 25k rows, each a bounded daemon call (<10s). Full 1.1M-symbol reindex completes in ~2m14s. Emits per-batch progress to stderr + final JSON summary `{batches, total_upserted, final_offset, elapsed_secs, final_stats}` to stdout. |

**Payload schema** (`cli-tantivy-reindex` hook):
```json
{"mode": "batch", "offset": 0, "limit": 25000, "clear": true}
```
Response: `{reindexed, done, mode, upserted, next_offset, stats}`. Setting
`mode: "full"` (or omitting `mode`) preserves legacy single-call semantics
but may exceed handler budget on >500k-symbol workspaces.

**Composite dedup key** (`upsert_symbol`): `blake3(symbol_name | file_path
| line_number)` stored in `blake3_hash` field. Fixes prior data loss where
`delete_term(symbol_name)` clobbered all homonymous siblings — 1.1M upserts
previously produced 75,696 `total_docs`, now produces 1,097,892.

---

**Outros módulos**: [overview](touring-cli-overview.md) | [hooks](touring-cli-hooks.md) | [intelligence](touring-cli-intelligence.md) | [rl-quality](touring-cli-rl-quality.md) | [generate](touring-cli-generate.md) | [meta](touring-cli-meta.md)
