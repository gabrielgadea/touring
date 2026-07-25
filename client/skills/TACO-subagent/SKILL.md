---
name: TACO-subagent
description: >
  TACO — Touring Agentic Code Orchestrator v6.0. Orchestration sequencial por fases,
  spawns pure subagents (no Agent Teams), valida resultados, e persiste padrões aprendidos.
  MANDATORY: Every subagent must be bound to the TACO rule.
version: 6.0
author: Gabriel Gadea
category: meta-orchestration
tags:
  - orchestrator
  - subagent
  - sequential-phases
  - touring
  - taco
triggers:
  - taco
  - touring-orchestrator
  - orchestrator
  - decompor em DAG
  - spawning subagents
mcp_servers:
  - touring
rules:
  - taco-orchestrator
---

# TACO — Touring Agentic Code Orchestrator v6.0

**MANDATORY**: Every subagent MUST be bound to the TACO rule.

---

## SEQUENTIAL PHASE PROTOCOL v6.0 (OBRIGATÓRIO)

**Fases são SEQUENCIAIS. Dentro de cada fase, agentes podem ser paralelos.**

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ FASE 1 ──► SCOUT (paralelo) ──► AGUARDA resultado ──► sequential-thinking   │
│                                        PROCESSA                               │
├──────────────────────────────────────────────────────────────────────────────┤
│ FASE 2 ──► ARCHITECT (paralelo) ──► AGUARDA ──► sequential-thinking         │
│                                        PROCESSA                               │
├──────────────────────────────────────────────────────────────────────────────┤
│ FASE 3 ──► CONTEXT7 best practices ──► DECISÃO de implementação           │
├──────────────────────────────────────────────────────────────────────────────┤
│ FASE 4 ──► DECOMPOSE (sequential-thinking) ──► subtasks especificadas      │
├──────────────────────────────────────────────────────────────────────────────┤
│ FASE 5 ──► ENGINEERS (paralelo/sequencial conforme DAG)                     │
├──────────────────────────────────────────────────────────────────────────────┤
│ FASE 6 ──► CROSS-AUDIT (paralelo)                                          │
├──────────────────────────────────────────────────────────────────────────────┤
│ FASE 7 ──► DOCUMENTAÇÃO completa de TUDO implementado                      │
└──────────────────────────────────────────────────────────────────────────────┘
```

**REGRAS CRÍTICAS:**
1. **AGUARDAR resultado** de cada fase ANTES de prosseguir para próxima
2. **NUNCA pular fases** ou fundir fases adjacentes
3. **sequential-thinking** usado para PROCESSAR resultados entre fases
4. **Context7** consultado APÓS architects e ANTES da decisão
5. **Decompose é LUNGA** com subtasks bem especificadas
6. **Engineers** podem ser paralelos ou sequenciais conforme DAG
7. **Cross-audit** pode ter auditores paralelos
8. **Documentação FINAL** atualiza TODA documentação relacionada

---

## ORCHESTRATOR EXECUTION FLOW

### PHASE 0 — PERCEPTION (executado diretamente pelo orchestrator)

```bash
# Intent Classification
mcp__touring__touring_classify_intent(text="<user_prompt>")

# Memory Recall
mcp__touring__touring_memory_recall(query="<user_prompt>", top_k=10)

# Session Start
mcp__touring__touring_session(action="start", task_type="intent", objective="<user_prompt>")

# System Pre-flight
touring doctor -j | jq '.[] | select(.status != "ok")'
touring status -j | jq '{idx: .index.symbol_count, orphans: .wiring.orphan_count, rl: .learning.ema_reward}'
```

### PHASE 1 — SCOUT (paralelo, aguardado)

```bash
# Scouters podem ser múltiplos, todos em paralelo
# AGUARDAR todos os resultados antes de prosseguir

# Exemplo: 3 scouters em paralelo
result_scout_1 = Agent(
  prompt="@/home/gabrielgadea/.claude/rules/TACO-subagent.md\n\n# TACO SUBAGENT\n\n## ROLE: scout\n## TASK: [descricao da tarefa de scouting 1]\n## ORCHESTRATOR: TACO\n\n[Include full context]\n\n## RETURN: ONLY JSON, NO PROSE",
  subagent_type="feature-dev:code-explorer",
  run_in_background=True
)

result_scout_2 = Agent(
  prompt="@/home/gabrielgadea/.claude/rules/TACO-subagent.md\n\n# TACO SUBAGENT\n\n## ROLE: scout\n## TASK: [descricao da tarefa de scouting 2]\n## ORCHESTRATOR: TACO\n\n[Include full context]\n\n## RETURN: ONLY JSON, NO PROSE",
  subagent_type="feature-dev:code-explorer",
  run_in_background=True
)

result_scout_3 = Agent(
  prompt="@/home/gabrielgadea/.claude/rules/TACO-subagent.md\n\n# TACO SUBAGENT\n\n## ROLE: scout\n## TASK: [descricao da tarefa de scouting 3]\n## ORCHESTRATOR: TACO\n\n[Include full context]\n\n## RETURN: ONLY JSON, NO PROSE",
  subagent_type="feature-dev:code-explorer",
  run_in_background=True
)

# AGUARDAR resultados...
# Aguardar notifications de conclusão dos scouters
```

**PROCESSAMENTO** (após aguardAR):
```bash
# Usar sequential-thinking para processar resultados dos scouts
mcp__sequential-thinking__sequentialthinking(
  thought="Resultados dos scouters: [result_scout_1, result_scout_2, result_scout_3]. Analisando padrões, conflitos, e recomendações...",
  nextThoughtNeeded=true,
  thoughtNumber=1,
  totalThoughts=3
)
```

### PHASE 2 — ARCHITECT (paralelo, aguardado)

```bash
# Architects podem ser múltiplos, todos em paralelo
# AGUARDAR todos os resultados antes de prosseguir

result_architect_1 = Agent(
  prompt="@/home/gabrielgadea/.claude/rules/TACO-subagent.md\n\n# TACO SUBAGENT\n\n## ROLE: architect\n## TASK: [descricao da tarefa arquitetural 1]\n## ORCHESTRATOR: TACO\n\n[Include scout results + context]\n\n## RETURN: ONLY JSON, NO PROSE",
  subagent_type="touring-architect",
  run_in_background=True
)

result_architect_2 = Agent(
  prompt="@/home/gabrielgadea/.claude/rules/TACO-subagent.md\n\n# TACO SUBAGENT\n\n## ROLE: architect\n## TASK: [descricao da tarefa arquitetural 2]\n## ORCHESTRATOR: TACO\n\n[Include scout results + context]\n\n## RETURN: ONLY JSON, NO PROSE",
  subagent_type="touring-architect",
  run_in_background=True
)

# AGUARDAR resultados...
```

**PROCESSAMENTO** (após aguardar):
```bash
# Usar sequential-thinking para processar resultados dos architects
mcp__sequential-thinking__sequentialthinking(
  thought="Resultados dos architects: [result_architect_1, result_architect_2]. Arquitetura final, DAG, contratos, riscos...",
  nextThoughtNeeded=true,
  thoughtNumber=1,
  totalThoughts=3
)
```

### PHASE 3 — CONTEXT7 + DECISÃO

```bash
# Consultar Context7 para melhores práticas
mcp__context7__query-docs(
  libraryId="/anthropic/claude-code",
  query="best practices for multi-agent orchestration, sequential processing"
)

# DECISÃO de implementação
mcp__sequential-thinking__sequentialthinking(
  thought="Após Context7, tomando decisão: o que será implementado?",
  nextThoughtNeeded=false,
  thoughtNumber=1,
  totalThoughts=1
)
```

### PHASE 4 — DECOMPOSE (longa decomposição)

```bash
# Criar DAG com touring decompose
mcp__touring__touring_decompose(
  action="create",
  task_type="intent",
  description="[descricao completa da tarefa]"
)

# Adicionar subtasks com depends_on
mcp__touring__touring_decompose(
  action="add_subtask",
  task_id="[task_id]",
  subtask_id="S-1",
  description="[subtask 1 details]",
  depends_on=[],
  priority=5
)

mcp__touring__touring_decompose(
  action="add_subtask",
  task_id="[task_id]",
  subtask_id="S-2",
  description="[subtask 2 details]",
  depends_on=["S-1"],
  priority=4
)

# ... mais subtasks

# Validar ordem topológica
mcp__touring__touring_decompose(
  action="validate_order",
  task_id="[task_id]"
)

# Decompose LUNGA com sequential-thinking
mcp__sequential-thinking__sequentialthinking(
  thought="Decompondo tarefa em subtasks específicas: [lista completa]. Validando dependências, identificando parallel groups...",
  nextThoughtNeeded=true,
  thoughtNumber=1,
  totalThoughts=10
)
```

### PHASE 5 — ENGINEERS (paralelo/sequencial conforme DAG)

```bash
# Engineers executam conforme DAG:
# - Subtasks sem dependências: podem executar em PARALELO
# - Subtasks com dependências: executam SEQUENCIALMENTE após dependências concluídas

# GRUPO PARALELO 1 (sem dependências)
result_engineer_1 = Agent(
  prompt="@/home/gabrielgadea/.claude/rules/TACO-subagent.md\n\n# TACO SUBAGENT\n\n## ROLE: engineer\n## TASK: [implementar modulo A]\n## ORCHESTRATOR: TACO\n\n[Include full context + DAG]\n\n## RETURN: ONLY JSON, NO PROSE",
  subagent_type="general-purpose",
  run_in_background=True
)

result_engineer_2 = Agent(
  prompt="@/home/gabrielgadea/.claude/rules/TACO-subagent.md\n\n# TACO SUBAGENT\n\n## ROLE: engineer\n## TASK: [implementar modulo B]\n## ORCHESTRATOR: TACO\n\n[Include full context + DAG]\n\n## RETURN: ONLY JSON, NO PROSE",
  subagent_type="general-purpose",
  run_in_background=True
)

# AGUARDAR resultados do grupo 1...

# GRUPO SEQUENCIAL (depende do grupo 1)
result_engineer_3 = Agent(
  prompt="@/home/gabrielgadea/.claude/rules/TACO-subagent.md\n\n# TACO SUBAGENT\n\n## ROLE: engineer\n## TASK: [implementar modulo C que depende de A e B]\n## ORCHESTRATOR: TACO\n\n[Include results from A and B + full context]\n\n## RETURN: ONLY JSON, NO PROSE",
  subagent_type="general-purpose",
  run_in_background=False  # SEQUENCIAL após grupo 1
)
```

### PHASE 6 — CROSS-AUDIT (paralelo)

```bash
# Auditors podem executar em paralelo para cobrir mais código
result_auditor_1 = Agent(
  prompt="@/home/gabrielgadea/.claude/rules/TACO-subagent.md\n\n# TACO SUBAGENT\n\n## ROLE: validator\n## TASK: [auditar modulo A - code review completo]\n## ORCHESTRATOR: TACO\n\n[Include implementation results]\n\n## RETURN: ONLY JSON, NO PROSE",
  subagent_type="feature-dev:code-reviewer",
  run_in_background=True
)

result_auditor_2 = Agent(
  prompt="@/home/gabrielgadea/.claude/rules/TACO-subagent.md\n\n# TACO SUBAGENT\n\n## ROLE: validator\n## TASK: [auditar modulo B - code review completo]\n## ORCHESTRATOR: TACO\n\n[Include implementation results]\n\n## RETURN: ONLY JSON, NO PROSE",
  subagent_type="feature-dev:code-reviewer",
  run_in_background=True
)

# AGUARDAR resultados...
```

### PHASE 6.5 — ELITE 50-DIM QUALITY GATE (Premium de Elite de Mercado)

Antes de declarar a entrega completa, rodar o gate 50-dim (motor real `touring-quality`). Floor de entrega = **Gold (0.80)**; release = **Diamond (0.95)**.

```bash
# Score do workspace/arquivos tocados nas 50 dimensões (F1.1–F4.12)
touring-quality score <DIR_OR_FILE> --workspace --fail-below 0.80 --format json

# 6 BLOCK dims (P0, fail-closed) — DEVEM passar antes do delivery:
for dim in F2.1 F2.4 F2.5 F2.6 F4.3 F4.5; do
  touring-quality check --gate "$dim" --target <FILE> --format json
done

# Release composite (13 gates agregados → touring-elite)
python3 ~/projects/touring/docs/elite_aggregate.py --check     # alvo ≥ 0.95 Diamond
```

Qualquer dim abaixo do tier-alvo → consultar a D-rule (`~/.claude/skills/touring-elite/references/quality/D{nn}.md`), remediar via `Edit tool`, re-score. ⚠ NÃO existe `touring quality` (subcommand), `score --gate`, `--enforce`, nem `generator de qualidade dedicado (inexistente)` (PLANNED W7). Catálogo + dim→agent owner: `~/.claude/rules/elite-50-quality.md`. Cada subagent inclui no JSON o campo `quality_dimensions` com os scores das suas dims primárias.

### PHASE 7 — DOCUMENTAÇÃO COMPLETA

```bash
# Atualizar TODA documentação relacionada
# - SKILL.md se habilidades foram adicionadas/modificadas
# - CLAUDE.md se diretivas mudaram
# - README.md se features foram adicionadas
# - Docs de API se contratos mudaram

# Memory store para lessons aprendidas
touring memory store "lesson:taco:v6:sequential-phases" "[descricao da lesson]" --tier semantic --type lesson

# RL reward
touring learning reward orchestrate 1.0 "sequential phases completed successfully"
```

---

## Subagent Invocation (MANDATORY)

Every subagent MUST start with:

```
@/home/gabrielgadea/.claude/rules/TACO-subagent.md
```

This directive **BONDS** the subagent to the TACO rule. Without it, the subagent is NOT operating under TACO protocol.

---

## Subagent Prompt Template

```markdown
@/home/gabrielgadea/.claude/rules/TACO-subagent.md

# TACO SUBAGENT — BOUND TO RULE

## ROLE: [scout|engineer|architect|validator]
## TASK: [specific task description]
## ORCHESTRATOR: TACO

## PHASE: [1-7] — Which phase this subagent is executing in

## Context from Previous Phases:
[If phase > 1, include results from previous phases]

## Sua Tarefa Específica:
[Descrição completa e específica do que fazer]

## Mandatory: Touring CLI Discovery (VGP)

Before ANY code generation, you MUST:

1. touring index find <symbol>     # VGP: Verify symbol exists
2. touring ast blast <file>        # Blast radius check
3. touring memory recall "<query>" # Check for past patterns

## Mandatory: Speculative Validation

Before applying ANY edit:

1. touring shadow validate         # Validate in shadow branch
2. Check score >= 0.8

## Quality Gates

**RETURN ONLY JSON — NO PROSE, NO MARKDOWN, NO EXPLANATIONS**

Your response MUST be ONLY valid JSON. The orchestrator parses your response as JSON. Any text outside the JSON structure = FAILURE.

```json
{
  "role": "[scout|architect|engineer|validator]",
  "status": "completed|failed|partial",
  "result": { ... },
  "quality_gates": {
    "functional": true,
    "robust": true,
    "readable": true,
    "documented": true,
    "secure": true,
    "no_regression": true
  },
  "composite_score": 1.0,
  "issues": [],
  "next_recommendations": []
}
```

---

## CILA Routing Strategy (determines which phases execute)

| CILA Level | Phases Executed |
|------------|----------------|
| L0-L1 | SOLO MODE — resolver direto, sem subagents |
| L2 | FASE 1 (scout) + FASE 5 (engineer) |
| L3 | FASE 1 (scout) + FASE 2 (architect) + FASE 5 (engineers) + FASE 6 (auditor) |
| L4+ | TODAS AS 7 FASES |

---

## ACO Patterns (Pheromone-Based Routing)

- **AcoWiringState**: Estado de feromônio para roteamento adaptativo
- **Deposit pheromone**: Após sucesso de uma sequência de fases
- **Evaporate**: Após falhas em sequência
- **sequential-thinking**: Usa os resultados de pheromone para decidir próximo passo

---

## Touring CLI Commands

```bash
# VGP (before code gen)
touring index find <symbol>
touring ast blast <file>

# Session
touring session start <id> type "<objective>"
touring session assess <id>

# Decompose
touring decompose create <type> <desc>
touring decompose add <task_id> <subtask_id> [deps]
touring decompose validate <task_id>

# Memory
touring memory recall "<query>"
touring memory store "<key>" "<value>" --tier semantic --type lesson

# Wiring
touring wiring status
touring wiring orphans

# Evolution
touring evolution insights
touring learning reward <tool> <value>
```

---

## Quality Gate

**composite_score >= 1.0** → PASS
**composite_score < 1.0** → REJECT and respawn (max 3 attempts)

| Gate | Criteria |
|------|----------|
| Functional | Tests pass |
| Robust | Error handling |
| Readable | Clear names |
| Documented | Docstrings |
| Secure | No secrets |
| No Regression | Existing tests green |

---

## Hard Rules

1. **BOUND to rule**: `@/home/gabrielgadea/.claude/rules/TACO-subagent.md` as FIRST LINE
2. **VGP**: Verify symbols via `touring index find` before code gen
3. **Speculate**: `touring shadow validate` before applying changes
4. **Exit 0**: Never block user operations
5. **No unwrap**: Use `?`, `.expect()`, `.unwrap_or_default()`
6. **Never reduce scope**: Solve problems completely
7. **AGUARDAR**: Sempre aguarde resultado de cada fase antes de prosseguir

---

*TACO v6.0 — Sequential Phase Protocol | 7 FASES OBRIGATÓRIAS | CILA-adaptive routing | ACO pheromone patterns*
