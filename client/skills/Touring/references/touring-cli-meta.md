# Touring CLI — Meta-comandos & TACO Workflow

> **Module**: 7/7 | **Version**: v4.27 | **Touring**: v30.3.0
> **Series**: Touring CLI Reference (consulta sob demanda) — `~/.claude/skills/Touring/references/touring-cli-*.md`
> **Index** (auto-load): `~/.claude/rules/touring-cli-index.md` (CLI RANKS Tier 2)
>
> **Last update**: Wave C (v4.27.0) — `touring assist` (10 handlers), `touring ssr`, `touring skip`, `touring source-change`, `touring profile`. RFC-100 codes Q-220/Q-310/W-115/S-100..S-102/G-200/SC-100..SC-102/F-200/A-100..A-109.

Meta-comandos (status, doctor, synergy, --help, --version), tabela resumo agregada (~123 comandos), uso no TACO workflow (Phase 0-4), integridade e testes do hook registry, referências internas.

---

## 20. Meta-comandos (v3.0 + Wave 8 expansão)

| Comando | Descrição |
|---------|-----------|
| `touring status [-j]` | Dashboard unificado: daemon health + index + wiring + sessions + learning + incremental + **composite_health_score** (W8 S3, weighted 5-dim ∈ [0,1]) |
| `touring doctor [-j]` | Diagnósticos: binary version, daemon socket, daemon health, circuit breaker, project DB |
| `touring synergy [report\|wired\|opportunities] [-j] [--with-metrics]` | **Wave 8 S6 + Wave 9 S9** (2026-04-26) — cross-subsystem wiring observability. `report` (default) full output; `wired` lista 43 active integrations (era 37; +6 Wave 9 S7+S8+S9); `opportunities` lista 7 deferred designs (com deferral_reason). `--with-metrics` (W9 S9) consulta daemon `cli-gate-metrics` e anexa `metrics: {counter, value}` a cada wired_pair com mapping em WIRED_PAIR_METRICS (10 entries). Graceful degrade quando daemon unreachable. |
| `touring --help` | Help auto-gerado da command_table() (zero drift) |
| `touring --version` | Versão do binário |

---

## Tabela Resumo

| Subapp | Comandos | Hook Handlers |
|--------|----------|---------------|
| **Hooks** | serve, pre-*, post-*, session-*, cortex, prompt-enhance, post-tool-failure, post-compact, instructions-loaded, hook-memory-*, decompose-event, pre-task-scout, task-created, task-completed, post-tool-rl | 24 hooks |
| **classify/pii** | classify-intent, scan-pii | 2 handlers |
| **index** | status, search, find, files, rebuild | 5 handlers |
| **ast** | find, overview, blast, blast-cross-feature, **highlight** (W5 v4.17), rust-semantic, format-rust, workspace-info, grep, tdg, scan, meta, skeleton | 4 daemon handlers + 8 pure-library subcommands (`highlight`/`rust-semantic`/`format-rust`/`workspace-info` are pure-library, no daemon hop) |
| **session** | start, checkpoint, list, assess | 4 handlers |
| **decompose** | create, add, get, update, validate, status, finalize, ready | 8 handlers (cli-decompose-finalize + cli-decompose-ready adicionados 2026-04-12) |
| **diary** | write, read, list, meta | 4 commands (direct) |
| **mcts** | search | 1 handler |
| **shadow** | validate | 1 handler |
| **suggest** | next, skill | 2 handlers |
| **learning** | status, reward | 2 handlers |
| **tantivy** | search, fuzzy, stats, suggest, reindex | 5 handlers (cli-tantivy-*) |
| **wiring** | status, orphans, modules, score, audit, suggest, purpose, community, chains, impact, cycles | 9 handlers (cli-wiring-chains + cli-wiring-impact + cli-wiring-cycles added Wave cross-audit) |
| **assist** | list-kinds, applicable, apply | 3 pure-library handlers (Wave C) |
| **ssr** | --pattern, --replacement, --dry-run | 1 pure-library handler (Wave B) |
| **skip** | list, validate | 2 pure-library handlers (Wave A) |
| **source-change** | apply | 1 pure-library handler (Wave B) |
| **profile** | query, dump, heap-dump, flamegraph | 1 handler (Wave A) |
| **file-knowledge** | extended | 1 handler (cli-file-knowledge-extended, Wave cross-audit) |
| **cognitive** | metrics, engines | 2 handlers |
| **flywheel** | status | 1 handler |
| **incremental** | status | 1 handler |
| **gotcha** | list, add, match, stats | 4 handlers |
| **memory** | stats, recall, store, list | 4 handlers |
| **evolution** | drift, insights, tools | 3 handlers |
| **e2e** | e2e [--depth quick\|standard\|deep] | 1 handler (cli-e2e) |
| **gate-metrics** (L7-B) | gate-metrics | 1 handler (cli-gate-metrics) |
| **inferlets** (L7-B) | list, run | 2 handlers (cli-inferlets-list, cli-inferlets-run) |
| **jobs** (L7-B) | spawn, poll, list, drop | 4 handlers (cli-jobs-spawn/poll/list/drop) |
| **generate** | list-kinds, render, plan, verify, plan-submit, plan-validate, plan-verify, plan-render, plan-speculate, plan-commit, plan-status, plan-export, plan-diff, plan-critique, plan-suggest, plan-recall, plan-history, plan-replay, plan-rollback, template-list, template-validate, template-test, schema-dump, capacity | 24 subcommands (in touring-server) |
| **meta** | status, doctor, **synergy** (W8 v4.20 + W9 v4.21 `--with-metrics` enrichment), --help, --version | 5 commands |
| **TOTAL** | **~123 comandos** | **63+ handlers + 24 generate subcommands. Hook Registry: 176 (W8 +2 tasksfile + W9 +2 devrcfile)** |

---

## Uso no TACO Workflow

### Phase 0: Perception

```bash
# Classificar intent
touring classify-intent

# Session start
touring session start taco-$(date +%s) "decomposition" "implementar feature X"
```

### Phase 0.5: System Pre-flight (novo v3.0)

```bash
# Dashboard rápido do sistema
touring status -j | jq '{index: .index.symbol_count, wiring: .wiring.orphan_count, rl: .learning.ema_reward}'

# Diagnósticos de saúde
touring doctor -j | jq '.[] | select(.status != "ok")'
```

### Phase 1: DAG Decomposition

```bash
# Criar DAG de tarefa
touring decompose create intent "implementar feature X"

# Adicionar subtarefas
touring decompose add task_abc sub_1 "research APIs"
touring decompose add task_abc sub_2 "implementar backend"
touring decompose add task_abc sub_3 "implementar frontend" sub_1,sub_2

# Validar DAG (detecta ciclos)
touring decompose validate task_abc
```

### Phase 2: Scout (Discovery)

```bash
# Buscar símbolo existente
touring index find MCTSEngine
touring ast find SymbolStore

# Overview de arquivo
touring ast overview src/cli/mod.rs

# Blast radius
touring ast blast src/bridge.rs
```

### Phase 3: Session/Cognitive

```bash
# Session assessment
touring session assess taco-123

# Cognitive metrics
touring cognitive metrics
touring cognitive engines
```

### Phase 4: Knowledge Capture

```bash
# Memory recall
touring memory recall "pattern: error handling"

# Evolution insights + drift monitoring
touring evolution insights
touring evolution tools
touring evolution drift   # P4.3: structural degradation detection
```

---

## Integridade e Testes

```bash
# Hook registry validation
cargo test --package touring-hooks -- registry

# Esperado: ALL_DAEMON_HOOK_NAMES.len() == 138
```

---

## Referencias

- **CLI source**: `crates/touring-server/src/cli/{module}.rs`
- **Handlers**: `crates/touring-hooks/src/cli_handlers.rs`
- **Registry**: `crates/touring-hooks/src/hook_registry.rs`
- **Skill master** (CLI ranks Tier 1-9, best practices): `~/.claude/skills/Touring/SKILL.md`
- **Index** (auto-load): `~/.claude/rules/touring-cli-index.md`

---

**Outros módulos**: [overview](touring-cli-overview.md) | [hooks](touring-cli-hooks.md) | [intelligence](touring-cli-intelligence.md) | [tasks](touring-cli-tasks.md) | [rl-quality](touring-cli-rl-quality.md) | [generate](touring-cli-generate.md)
