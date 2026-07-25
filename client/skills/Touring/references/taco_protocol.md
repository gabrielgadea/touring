# TACO Phase Protocol Reference

> Complete reference for TACO orchestration phases and workflow.

## Phase Overview

| Nível | Fases Executadas |
|-------|------------------|
| **L0-L1** | Solo mode — orchestrator resolve direto |
| **L2** | Scout → Engineer → Validate |
| **L3** | Scout → Architect → Engineer → Audit → Validate |
| **L4+** | FASE 0 → 1 → 2 → 3 → 4 → 4.5 → 5 → 6 → 7 |

## FASE 0 — System Health Gate (CRÍTICO — BLOQUEIA TUDO)

**Executado ANTES de qualquer fase. Se falhar, NENHUMA fase posterior roda.**

### Commands

```bash
cd <workspace_root>
cargo check --workspace 2>&1 | tail -5
touring doctor -j
```

### Decision Table

| Condition | Action | Blocking |
|-----------|--------|----------|
| `cargo check` exit != 0 | Reportar errors, BLOQUEIA todas as fases | 🔴 YES |
| `touring doctor` daemon_socket = error | Reportar, fallback mode ativa | 🟡 DEGRADED |
| daemon degraded + ema_reward = 0.0 | BLOQUEIA | 🔴 YES |

## FASE 1 — Scout (Discovery)

```bash
touring index find <symbol>
touring ast blast <file>
touring wiring orphans -j
```

## FASE 2 — Architect (Planning)

```bash
touring mcts search [root_state]
touring_decompose create
```

## FASE 3 — Context7 Best Practices

Consultar Context7 para best practices antes da decisão de implementação.

## FASE 4 — Decompose (DAG Creation)

```bash
touring decompose create <type> <desc>
touring decompose add <task_id> <subtask_id> "<desc>"
touring decompose validate <task_id>
```

## FASE 4.5 — Pre-Implementation Audit

**Auditor pode REJECT tasks marcadas como FALSE_POSITIVE ANTES de Engineers.**

### FALSE_POSITIVE Detection Patterns

| Pattern | Detection | Action |
|---------|-----------|--------|
| Task diz "unwrap em production" mas todos unwraps estão em tests | Grep test modules | REJECT |
| Task diz "símbolo X não existe" mas `touring index find` retorna resultado | touring index find X | REJECT |
| Task diz "compilation error" mas `cargo check` exit = 0 | cargo check | REJECT |

## FASE 5 — Engineers (Implementation)

Engineers executam implementação seguindo o DAG.

## FASE 6 — Post-Implementation Audit

```bash
touring wiring audit -j
cargo check --workspace
```

## FASE 7 — Documentation

Toda documentação relacionada deve ser atualizada.

## Task Lifecycle

```
┌─────────────────────────────────────────────────────────────────────┐
│ 1. CREATE ──► touring decompose create ──► DAG decomposto           │
│ 2. TRACK  ──► touring decompose add (subtasks com depends_on)      │
│ 3. VALIDATE ─► touring decompose validate ──► sem ciclos           │
│ 4. EXECUTE ──► tasks executadas + touring session checkpoint      │
│ 5. ASSESS ──► touring session assess ──► qualidade avaliada        │
│ 6. LEARN  ──► touring memory store + touring learning reward       │
└─────────────────────────────────────────────────────────────────────┘
```
