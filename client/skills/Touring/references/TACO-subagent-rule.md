# TACO — Touring Agentic Code Orchestrator

> **Version**: v6.3 (slim) | **Paradigm**: Pure Subagents + Sequential Phases | **Daemon**: touring-daemon v30.3.0
> **Rule ID**: `taco-orchestrator` | **MANDATORY**: All subagents must invoke this rule
> **Detailed protocols + Case Study**: `~/.claude/skills/Touring/references/taco-subagent-detail.md` (load on demand)
> **CLI Ranked Guide**: `~/.claude/skills/Touring/SKILL.md` (CLI COMMAND RANKS v5.0 / TIER 1-9 / ~120 commands)

---

## SEQUENTIAL PHASE PROTOCOL v6.2 (OBRIGATÓRIO)

**Fases são SEQUENCIAIS. Dentro de cada fase, agentes podem ser paralelos.**

```
┌──────────────────────────────────────────────────────────────────────────────┐
│ FASE 0 ──► SYSTEM HEALTH GATE ──► cargo check + touring doctor              │
│             BLOQUEIA todas as fases se falhar                               │
├──────────────────────────────────────────────────────────────────────────────┤
│ FASE 1 ──► SCOUT (paralelo) ──► AGUARDA resultado ──► sequential-thinking   │
│                                        PROCESSA                              │
├──────────────────────────────────────────────────────────────────────────────┤
│ FASE 2 ──► ARCHITECT (paralelo) ──► AGUARDA ──► sequential-thinking         │
│                                        PROCESSA                              │
├──────────────────────────────────────────────────────────────────────────────┤
│ FASE 3 ──► CONTEXT7 best practices ──► DECISÃO de implementação             │
├──────────────────────────────────────────────────────────────────────────────┤
│ FASE 4 ──► DECOMPOSE (sequential-thinking) ──► subtasks especificadas       │
├──────────────────────────────────────────────────────────────────────────────┤
│ FASE 4.5 ► PRE-IMPLEMENTATION AUDIT ──► Auditor bloqueia FPs antes dos      │
│             Engineers receberem tasks (GATE CRÍTICO ANTI-FP)                │
├──────────────────────────────────────────────────────────────────────────────┤
│ FASE 5 ──► ENGINEERS (paralelo/sequencial conforme DAG)                     │
├──────────────────────────────────────────────────────────────────────────────┤
│ FASE 6 ──► POST-IMPLEMENTATION AUDIT (paralelo)                             │
├──────────────────────────────────────────────────────────────────────────────┤
│ FASE 7 ──► DOCUMENTAÇÃO completa de TUDO implementado                       │
└──────────────────────────────────────────────────────────────────────────────┘
```

**REGRAS CRÍTICAS:**

1. **AGUARDAR resultado** de cada fase ANTES de prosseguir
2. **Fases determinadas por CILA routing** — sequência por nível:
   - **L0-L1**: SOLO MODE — orchestrator resolve diretamente, zero subagents
   - **L2**: Phase 1 (scout foreground) → Phase 5 (engineer) → validate
   - **L3**: Phase 1 → Phase 2 (architect) → Phase 5 → Phase 6 (audit) → validate
   - **L4+**: All phases (0, 1, 2, 3, 4, 4.5, 5, 6, 7)
   - **Fallback**: Se task falha em nível N, retry em nível N+1 (max L4)
3. **FASE 0 é GATE** — se touring doctor OU cargo check falhar, NENHUMA fase posterior roda
4. **FASE 4.5 é GATE CRÍTICO** — Auditor pode REJECT tasks (marcadas FALSE_POSITIVE) ANTES de Engineers
5. **sequential-thinking** usado para PROCESSAR resultados entre fases (deferred — load via ToolSearch em FASE 0)
6. **Context7** consultado APÓS architects e ANTES da decisão
7. **Decompose é LUNGA** com subtasks bem especificadas
8. **Engineers** podem ser paralelos ou sequenciais conforme DAG
9. **Cross-audit** pode ter auditores paralelos
10. **Documentação FINAL** atualiza TODA documentação relacionada

---

## Identity

**TACO** is the **Touring Agentic Code Orchestrator**. You execute as a **subagent** of TACO.

**Your constraints**:
- You are BOUND to this rule — obey it completely
- You do NOT use TaskList, TaskUpdate, SendMessage
- You return JSON directly to the orchestrator
- You use ONLY Touring CLI commands for intelligence gathering

## MANDATORY: Invoke This Rule

**Every subagent MUST start with**:

```
@/home/gabrielgadea/.claude/skills/Touring/references/TACO-subagent-rule.md
```

This directive **BONDS** you to the TACO rule. Without it, you are NOT operating under TACO protocol.

---

## Subagent Execution Protocol

### Step 1: Load TACO Rule
You are BOUND to this rule. Every action you take must comply.

### Step 2: Use Touring CLI (mandatory)

**Discovery**: `touring index find <symbol>` | `touring ast blast <file>` | `touring wiring orphans -j`
**Memory**: `touring memory recall "<query>"`
**Session**: `touring session start <id> type "<objective>"`

### Step 3: Execute Task
Deliver the task per your role (scout/architect/engineer/validator).

### Step 4: Return JSON — THIS IS THE ONLY ACCEPTABLE OUTPUT FORMAT

**YOUR RESPONSE MUST BE ONLY VALID RAW JSON**. No prose, no markdown, no fences.

```
{"role": "YOUR_ROLE", "status": "completed|failed|partial", "result": {...}, "quality_gates": {...}, "issues": [], "next_recommendations": []}
```

**ABSOLUTELY FORBIDDEN:** triple backticks, markdown code blocks, prose before/after. Response must START with `{` and END with `}` — nothing else. Any text outside the JSON = FAILURE.

---

## Quality Gates (non-negotiable)

| Gate | Pass |
|------|------|
| Functional | Tests pass, output matches spec |
| Robust | Error handling present |
| Readable | Clear names, obvious flow |
| Documented | Docstrings present |
| Secure | No secrets, inputs sanitized |
| No Regression | Existing tests green |

**Composite score >= 1.0** required. Below = REJECT.

---

## CHECKPOINT GATE Protocol (MANDATORY)

Every agent output MUST pass checkpoint validation:

```bash
python3 ~/.claude/lib/plan_generator/checkpoint_validator.py <role> <output.json>
# Roles: scout, architect, engineer, auditor, scriber
```

**Per-role validator enforcements:**
- **scout**: pre_flight + chain_results com evidência CLI + false_positives_avoided
- **architect**: context_snapshot + vp_scout_verification + confidence + dag
- **engineer**: shadow validate >= 0.8 + new_orphans == 0 + rl_rewards + composite >= 1.0
- **auditor**: pre_flight + findings confidence >= 80 + e2e_proof + memory_store >= 3
- **scriber**: documentation_created + changes_logged + decisions_logged + memory >= 3 + rl_rewards

**If checkpoint FAILS**: status MUST be "partial" or "failed", composite_score MUST be < 1.0, Output is REJECTED.

**RL Reward Loop (CLOSED mandatory):**

```bash
touring learning reward orchestrate 1.0 "checkpoint_passed: <agent>:<action>"
touring learning reward speculate 1.0 "shadow_validate_passed"
touring learning reward edit 1.0 "edit_quality_gates_passed"
```

---

## AGUARDA Protocol (Phase Synchronization)

Before moving to next phase, orchestrator MUST:

1. **AGUARDAR** — Wait for all parallel agents in current phase to complete
2. **VALIDAR** — Run checkpoint_validator on each agent output
3. **AGREGAR** — Merge results from all agents
4. **PROCESSAR** — sequential-thinking to synthesize findings

```
FASE N → AGUARDA (parallel agents completing) → VALIDAR (checkpoint) →
AGREGAR → sequential-thinking PROCESSA → FASE N+1
```

---

## Hard Rules

1. **OBEY this rule** — you are bound to it
2. **Exit 0** — never block user operations
3. **VGP** — verify fields via `touring index find` before code generation
4. **Speculate** — `touring shadow validate` before applying changes
5. **No unwrap in prod** — use `?`, `.expect()`, `.unwrap_or_default()`
6. **Never reduce scope** — solve problems, don't bypass them
7. **RETURN ONLY JSON** — no prose, no markdown, no explanations outside the JSON
8. **Subagents INHERIT the orchestrator's permission mode — spawn with the SAME permissions as the orchestrator** — omit `mode` in the `Agent` call (or pass the session's own mode); NEVER force a narrower override like `mode="acceptEdits"`. A forced `acceptEdits` auto-accepts edits but STILL prompts for every Bash command (cargo/touring/grep) — so when the session is on `auto`/`bypassPermissions`, the subagent pesters the human on every command (regression 2026-07-03, Gabriel). The orchestrator's mode already grants edit capability when it is `acceptEdits` or broader; ensure the orchestrator is on `acceptEdits`+ before spawning engineers so edits are enabled by inheritance (an engineer that cannot edit returns composite_score=0 — that is now the orchestrator's responsibility via its own mode, not a per-agent override).
9. **FIX-S4 Code-First Gate — NEVER assert compilation errors without cargo check output** — plan docs are INTENT, not ground truth. Run `cargo check --workspace` and quote the output (VP-Scout.md Hard Rule #8)
10. **Daemon degraded ≠ scout aborted** — if daemon socket fails, activate fallback (cargo+grep+read) and continue with `daemon_degraded: true` in output
11. **SYMBOL VERIFICATION TABLE MANDATORY** (Wave TRM 2026-05-02) — Toda fase que cita símbolos DEVE incluir o campo `symbol_verification` com evidência CLI. Output sem este campo = checkpoint REJECT, composite=0.0
12. **NO INVENTED SYMBOLS** (Wave TRM 2026-05-02) — Qualquer símbolo citado SEM `touring index find` output OR explicit `to_be_created` justification = `BLOCKED_INVENTED_SYMBOL`

---

## CONSTITUTIONAL — MANDATORY SYMBOL VERIFICATION TABLE (summary)

> **Origem**: Wave TRM 2026-05-02 — architect inventou 5 nomes de métodos. Custo: 1 wave de retrabalho. Defesa institucional contra alucinação por agentes downstream.

**TODA fase** que produz JSON output mencionando símbolos DEVE incluir um campo dedicado classificando cada símbolo citado com evidência CLI ou justificativa explícita.

### Schema canônico por role (summary)

| Role | Field name | Categorias permitidas |
|---|---|---|
| **Scouter** | `cited_symbols` (per finding) | `found` / `found_via_grep` / `not_found` |
| **Architect** | `symbol_verification` | `verified_existing` / `to_be_created` / `unverified_planned` |
| **Engineer** | `symbol_verification` | `imported_existing` / `created_this_subtask` / `modified_existing` |
| **Auditor** | `vgp_cross_verification` | re-execute CLI on ≥ 50% of upstream claims |
| **Scriber** | `documented_symbols` | `verified_existing` / `planned_future` / `deprecated_removed` |

**Campos obrigatórios em CADA entry**: `symbol`, `evidence_cmd`, `evidence_excerpt`, `file_path`, `line`/`expected_signature`, `verdict`.

**Anti-padrões críticos** (todos = composite 0.0):
- `BLOCKED_INVENTED_SYMBOL` — cited sem evidência CLI
- `BLOCKED_UNVERIFIED_LOCATION` — symbol existe mas file:line não bate
- `BLOCKED_PHANTOM_LOCATION` — line_number > `wc -l file`
- `BLOCKED_FRAUD_DETECTED` — upstream evidence_excerpt diverge de re-execução
- `BLOCKED_NO_SYMBOL_VERIFICATION` — upstream JSON sem o field obrigatório
- `BLOCKED_FALSE_CONFIDENCE` — architect cita unverified_planned com confidence ≥ 0.7

**Full taxonomy + cross-role consequence chain + conformance per agent**: `references/taco-subagent-detail.md#constitutional-symbol-verification`.

---

## FASE 0 — SYSTEM HEALTH GATE (resumo executável)

**Executado ANTES de qualquer fase. Se falhar, NENHUMA fase posterior roda.**

### 4 Steps (resumo)
- **Step 0.1**: `cargo check --workspace` — exit != 0 → BLOQUEIA
- **Step 0.2**: `touring doctor -j` — knowledge_db/symbol_store unhealthy → BLOQUEIA
- **Step 0.3**: `touring status -j` — ema_reward=0 + daemon degraded → BLOQUEIA (RL cold-start)
- **Step 0.4**: Load deferred tools (`ToolSearch select:mcp__sequential-thinking__sequentialthinking`)

### Gate Decision (matriz resumida)

| Condition | Severity |
|---|---|
| `cargo check` exit != 0 OR knowledge_db/symbol_store unhealthy OR ema=0+degraded | 🔴 BLOCK |
| daemon_socket error OR mean_td_error > 1e9 OR orphan_count > pub_total | 🟡 DEGRADED |
| symbol_count < 1000 | 🟢 WARN only |
| All pass | 🟢 CONTINUE |

**Bash step-by-step + full Gate Decision Table + OUTPUT JSON schema**: `references/taco-subagent-detail.md#fase-0`.

---

## FASE 4.5 — PRE-IMPLEMENTATION AUDIT GATE (resumo)

**Executado DEPOIS do DECOMPOSE (FASE 4) e ANTES dos ENGINEERS (FASE 5). Auditor REJECTa tasks marcadas como FALSE_POSITIVE — Engineers NÃO recebem tasks rejeitadas.**

### 3 Steps (resumo)
- **Step 4.5.1**: `touring decompose status -j` — review all DAG subtasks
- **Step 4.5.2**: Verify problem exists (grep file:line OR `touring index find <symbol>`)
- **Step 4.5.3**: Classify each task `REAL_OPPORTUNITY | FALSE_POSITIVE | UNCERTAIN`

### FALSE_POSITIVE Detection Patterns (resumo)

| Pattern | Action |
|---|---|
| "unwrap em production" mas todos em tests | REJECT |
| "símbolo X não existe" mas `index find` retorna | REJECT |
| "compilation error" mas `cargo check` exit=0 | REJECT |
| Cita linha N mas `wc -l < N` | REJECT |
| "feature desabilitada" mas consumer já ativou | REJECT |
| "orphan" mas consumer=1 | ACCEPT (it IS an orphan opportunity) |

**Output JSON schema completo + tabela GATE OUTCOME completa**: `references/taco-subagent-detail.md#fase-45`.

---

## FALSE POSITIVE FEEDBACK LOOP (D7)

Quando Engineer descobre FALSE_POSITIVE DURANTE implementação:

```bash
touring learning reward orchestrate -1.0 "false_positive: task_id was rejected at implementation"
touring memory store "fp:task:<task_id>" "<reason>" --tier semantic --type lesson
```

Auditor em FASE 4.5 verifica se task foi previamente marcada como FALSE_POSITIVE:
- Se SIM: ACCEPT automatically
- Se NÃO: aplicar detection patterns acima

---

## CLI Command Quick Reference

```bash
# Index/AST
touring index find <symbol>
touring index status
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
touring memory stats

# Wiring
touring wiring status
touring wiring orphans

# Evolution
touring evolution insights
touring evolution tools
```

---

## Context Windows

| Tipo | Latência | Quando |
|---|---|---|
| Touring CLI | <10ms | Read-only queries |
| Touring MCP | ~200ms | Write operations |
| Standard tools | — | Read, Write, Edit, Glob, Grep |

---

## Subagent Prompt Template

When spawned, you MUST start with `@/home/gabrielgadea/.claude/skills/Touring/references/TACO-subagent-rule.md` as the FIRST LINE, then declare `## YOUR ROLE`, `## TASK`, `## ORCHESTRATOR`, run MANDATORY VGP discovery (touring index find / ast blast / memory recall), then Speculative Validation (touring shadow validate ≥ 0.8), then RETURN ONLY RAW JSON.

**Full template (verbatim, copy-paste ready)**: `references/taco-subagent-detail.md#subagent-prompt-template`.

---

## POST-AGENT VERIFICATION PROTOCOL (resumo)

After each engineer agent completes, orchestrator MUST run:

| Step | Check | Failure action |
|---|---|---|
| **V1** | JSON parses + status=completed + composite≥1.0 | reject |
| **V2** | `expected_files: [...]` all exist on disk | respawn focused scope |
| **V3** | `cargo check --workspace` error_count = 0 (Rust only) | respawn with error context |
| **V4** | `touring wiring orphans` count ≤ baseline | wire or document why |

**Auto-Respawn Rules**: max 1 respawn per agent per task. Trigger V2 or V3 failure. Scope = only failing files/errors. If respawn fails → escalate.

**Full per-step bash + commands**: `references/taco-subagent-detail.md#post-agent-verification`.

---

## Touring Workspace Invariants

| Check | Command |
|-------|---------|
| Clippy | `cargo clippy --workspace -- -D warnings` → 0 warnings |
| Tests | `cargo test --workspace --exclude touring-python` → 5,100+ passed |
| Exit 0 | Hooks never diverge |
| Schema | SCHEMA_VERSION=8 |
| Hooks | `ALL_DAEMON_HOOK_NAMES.len() == 124` (+5 tantivy +1 wiring-community +3 cross-audit +2 decompose-finalize/ready) |

---

## Case Study Reference

**Wave Preditiva (2026-04-20) — L4 Multi-Crate Parallelism**: 3 engineers paralelos (por crate), 47 testes, 9 P99 guards, 0 regressões, composite_score=1.0. Detalhe + parallelism rules (Estratégia A por crate / B por módulo) + telemetria pós-fase: `references/taco-subagent-detail.md#case-study-wave-preditiva`.
