# Architecture Reference

> Touring CLI architecture: 3-layer design, daemon handlers, dispatch table.

## 3-Layer Architecture

| Camada | Local | Responsabilidade |
|--------|-------|-----------------|
| **CLI Client** | `touring-server/src/cli/` | Parsing de args + `daemon_query()` |
| **Daemon Handler** | `touring-hooks/src/cli_handlers.rs` | Lógica via `HookRuntime` |
| **Dispatch Table** | `touring-hooks/src/hook_registry.rs` | Mapeia hook name → handler |

## Daemon Actor Pattern (v30.3.0)

O daemon usa **um actor por projeto**:
- Cada projeto possui thread OS dedicada executando `run_project_actor(runtime, cmd_rx)`
- O actor owns `HookRuntime` e processa commands serialmente
- Panic-safe: cada handler envolve em `catch_unwind`
- Handler budgets: 15s light / **300s heavy**

## Hook Registry

| Metric | Value |
|--------|-------|
| Hook Registry | 143 entries (as of Wave C1.7 — see `hook_registry.rs` `assert_eq!(ALL_DAEMON_HOOK_NAMES.len(), 143)` for authoritative count) |
| Heavy hooks | cli-index-rebuild, cli-ast-blast, cli-ast-blast-cross-feature, cli-mcts-search, cli-session-start, cli-session-assess, cli-tantivy-reindex, cli-wiring-chains, cli-wiring-audit, cli-e2e |

## Global Flags

| Flag | Efeito |
|------|--------|
| `-j`, `--json` | Output JSON puro |
| `-v`, `--verbose` | Verbose tracing para stderr |
| `--timeout <N>` | Timeout do socket daemon em segundos (default: 10) |

## Communication Format

```json
// CLI client envia:
{"hook": "cli-ast-find", "payload": {...}, "project_root": "/path/to/project"}

// Daemon responde:
{"success": true, "output": "{...json result...}"}
```

## Advisory-Mode Hooks Pattern (2026-04-20)

Pattern introduzido na Predictive Wave (D2/D3/D4): hooks `PreToolUse`/`PostToolUse` podem
**mutar o input** da ferramenta ou **enriquecer o additionalContext** via
`HookResponse::ContextWithUpdatedInput`, mas **nunca** bloqueiam — exit 0 sempre.

### 3 Vetores Preditivos

| Vetor | Hook Event | Handler | Budget |
|-------|-----------|---------|--------|
| **D2 Blast Injection** | `PreToolUse[TaskCreate\|TaskUpdate]` | `pre_tool_use::compute_predictive_blast_injection` | 40ms |
| **D3 LinUCB Routing** | `PostToolUse[TaskList]` | `task_list::linucb_routing_hint` | 50ms (try_lock) |
| **D4 MCTS Shadow** | `PreToolUse[EnterPlanMode]` | `plan_mode::mcts_shadow_rollout_hint` | 12s thread + 200ms join |

### Graceful Degradation Hierarchy

```
Budget exceeded?  → retornar resultado parcial (budget_exhausted: true)
try_lock failed?  → retornar "" sem bloquear
thread timeout?   → join_timeout 200ms, continuar sem hint
qualquer erro?    → log via tracing::warn!, retornar HookResponse original
```

### 9 Observability Counters (D5)

Todos os 3 vetores reportam ao `GateMetrics` singleton via AtomicU64:

```bash
touring gate-metrics -j
# Famílias: blast_* | linucb_route_* | mcts_shadow_*
```

Campos: `blast_inject_count`, `blast_timeout_count`, `blast_mutation_count`,
`linucb_route_manual_count`, `linucb_route_generator_count`, `linucb_route_hint_count`,
`mcts_shadow_run_count`, `mcts_shadow_timeout_count`, `mcts_shadow_deadlock_detected_count`.

### Cross-Crate Dependencies

```
touring-hooks → touring-analysis  (BlastRadiusEngine::compute_with_timeout)
touring-hooks → touring-learning  (LinUCBBandit, FEATURE_DIM=25, NUM_ARMS=8)
touring-hooks → touring-cognitive (via shadow_rollout traits — indirect)
```

**Invariante**: mudanças em `FEATURE_DIM` ou `NUM_ARMS` em `touring-learning` requerem
atualização simultânea de `task_features.rs::debug_assert` em `touring-hooks`.

### Documentação Detalhada

- `crates/touring-hooks/touring-hooks-ARCHITECTURE.md` — Seções §D2, §D3, §D4, §D5
- `crates/touring-analysis/touring-analysis-ARCHITECTURE.md` — `compute_with_timeout`
- `crates/touring-learning/touring-learning-ARCHITECTURE.md` — LinUCBBandit cross-crate contract
- `crates/touring-cognitive/touring-cognitive-ARCHITECTURE.md` — Homonymia Resolution
