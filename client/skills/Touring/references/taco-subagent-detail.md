# TACO-subagent — Detailed Protocols & Case Study

Companion reference for `~/.claude/rules/TACO-subagent.md`. The rule keeps the phase protocol + Hard Rules + summary tables. This file holds the per-step bash, the full symbol-verification taxonomy, the prompt template, post-agent verification deep, and the historical case study. Load when actually executing a TACO phase or auditing per-role compliance.

## CONSTITUTIONAL — Symbol Verification (full taxonomy)

> **Origem**: Wave TRM 2026-05-02 — architect inventou 5 nomes de métodos. Custo: 1 wave de retrabalho. Esta seção é a defesa institucional contra alucinação de símbolos por agentes downstream. Aplica-se a TODOS os 5 roles (scouter, architect, engineer, auditor, scriber).

**TODA fase** que produz JSON output mencionando símbolos (function/struct/method/type) DEVE incluir um campo dedicado classificando cada símbolo citado em uma das categorias canônicas, com evidência CLI ou justificativa explícita.

### Schema canônico por role

| Role | Field name | Categorias permitidas | Anti-padrão |
|---|---|---|---|
| **Scouter** | `cited_symbols` (per finding) | `found` / `found_via_grep` / `not_found` (Chain 8) | Cite sem `touring index find` output |
| **Architect** | `symbol_verification` | `verified_existing` / `to_be_created` / `unverified_planned` | Inventar API "razoável" sem CLI cite |
| **Engineer** | `symbol_verification` | `imported_existing` / `created_this_subtask` / `modified_existing` (NO `unverified_planned`) | Edit referenciando símbolo não verificado |
| **Auditor** | `vgp_cross_verification` | re-execute CLI on ≥ 50% of upstream claims | Aceitar JSON shape sem re-verificar evidence |
| **Scriber** | `documented_symbols` | `verified_existing` / `planned_future` / `deprecated_removed` | Citar como "implemented" item que `index find` retorna 0 |

### Campos obrigatórios em CADA entry

| Campo | Significado | Mandatório? |
|---|---|---|
| `symbol` | Nome canônico (FQN preferred) | YES |
| `evidence_cmd` | Comando CLI executado | YES (categorias verified/imported/found) |
| `evidence_excerpt` | Trecho do output JSON do comando | YES |
| `file_path` ou `expected_file` | Localização (real ou planejada) | YES |
| `line` ou `expected_signature` | Linha real ou assinatura planejada | YES |
| `verdict` | `VERIFIED` / `INDEX_STALE` / `BLOCKED_*` | YES (scouter, auditor) |

### Anti-padrão central — BLOCKED_INVENTED_SYMBOL

Em qualquer role, citar um símbolo SEM evidência CLI (ou explicit `to_be_created` / `unverified_planned` justification) = anti-padrão automático. Auditor detecta via:

```bash
# Auditor cross-verification (Phase 0.6 do auditor)
SYMBOL=$(jq -r '.symbol_verification.verified_existing[0].symbol' /tmp/upstream.json)
CLI_HITS=$(touring index find "$SYMBOL" -j | jq 'length')
if [ "$CLI_HITS" -eq 0 ]; then
  echo "BLOCKED_INVENTED_SYMBOL: upstream claimed verified_existing but CLI returns 0"
fi
```

### Outras anti-padrões críticos

| Anti-padrão | Detecção | Veredicto |
|---|---|---|
| `BLOCKED_UNVERIFIED_LOCATION` | symbol existe mas file:line citado não bate | composite=0.0 |
| `BLOCKED_PHANTOM_LOCATION` | line_number > `wc -l file` | composite=0.0 |
| `BLOCKED_FRAUD_DETECTED` | upstream evidence_excerpt diverge de re-execução | composite=0.0, status=failed |
| `BLOCKED_NO_SYMBOL_VERIFICATION` | upstream JSON sem o field obrigatório | composite=0.0 |
| `BLOCKED_FALSE_CONFIDENCE` | architect cita unverified_planned com confidence ≥ 0.7 | partial |

### Como cada agente entra em conformidade

- **Scouter**: implementa Chain 8 (Wave TRM 2026-05-02) — `cited_symbols` por finding
- **Architect**: Phase 5.0 VGP SYMBOL VERIFICATION GATE — 3 categories
- **Engineer**: Phase 4.5 SYMBOL VERIFICATION TABLE — 3 categories (sem unverified)
- **Auditor**: Phase 0.6 VGP CROSS-VERIFICATION — re-run CLI ≥ 50% sample
- **Scriber**: Phase 0.5 VGP FOR DOCUMENTATION — verify cite before write

### Cross-role consequence chain

```
Scouter Chain 8 fails → finding excluded → architect doesn't see it
Architect Phase 5.0 fails → blueprint blocked → engineer doesn't get DAG
Engineer Phase 4.5 fails → subtask blocked → wiring audit doesn't pass
Auditor Phase 0.6 fails → upstream agent's output composite=0.0
Scriber Phase 0.5 fails → doc rewritten as PLANNED|PROPOSED or removed
```

> **Operational principle**: a invenção de símbolos é um bug semântico que escala rápido — uma vez que entra no blueprint do architect, propaga para engineer (que tenta importar), para o doc (que documenta), para o próximo session (que cita do doc). Esta seção corta a propagação na origem.

---

## FASE 0 — System Health Gate (detalhe step-by-step)

### Step 0.1: Compilation Ground Truth

```bash
cd <workspace_root>
cargo check --workspace 2>&1 | tail -5
# Exit code 0 = PASS. Se != 0 → BLOQUEIA todas as fases
cargo check --workspace 2>&1 | grep "^error\[" | wc -l
# Count errors. Se > 0 → BLOQUEIA.
```

### Step 0.2: Touring Daemon Health

```bash
touring doctor -j | jq '.[] | select(.status != "ok")'
# Se qualquer .status != "ok" → REPORTAR health issue
# Daemon com daemon_socket Error (111) = DEGRADED
# Se component degraded E é CRÍTICO (knowledge_db, symbol_store) → BLOQUEIA
```

### Step 0.3: Touring CLI Signals Baseline

```bash
touring status -j | jq '{idx: .index.symbol_count, orphans: .wiring.orphan_count, rl: .learning.ema_reward}'
# Se index.symbol_count < 1000 → índice possibly stale, continuar com warning
# Se wiring.orphan_count > 20000 → reportar mas não bloqueia
# Se learning.ema_reward == 0.0 E daemon degraded → RL cold-start, BLOQUEIA
```

### Step 0.4: Load Deferred Tools (MANDATORY before inter-phase processing)

**Sequential-thinking é um deferred tool** — deve ser carregado via ToolSearch ANTES de qualquer uso entre fases. Sem isso, a ferramenta nunca é chamada e o protocolo TACO falha silenciosamente.

```
# MANDATORY: carregar sequential-thinking antes de qualquer fase que o usa
ToolSearch(query="select:mcp__sequential-thinking__sequentialthinking")
# Depois de carregado, usar entre fases para processar resultados:
# mcp__sequential-thinking__sequentialthinking(thought="...processar resultado da FASE N...")
```

| Deferred Tool | Quando Carregar | ToolSearch Query |
|---|---|---|
| `mcp__sequential-thinking__sequentialthinking` | Phase 0, ANTES de FASE 1 | `"select:mcp__sequential-thinking__sequentialthinking"` |

**Se ToolSearch falhar**: Continuar sem sequential-thinking, documentar como degraded.
**NUNCA pular**: sequential-thinking deve processar cada transição de fase (FASE 1→2, FASE 2→3, etc).

### GATE DECISION TABLE

| Condition | Action | Blocking |
|-----------|--------|----------|
| `cargo check` exit != 0 | Reportar errors, BLOQUEIA todas as fases | 🔴 YES |
| `touring doctor` daemon_socket = error | Reportar, fallback mode ativa | 🟡 DEGRADED |
| `touring doctor` knowledge_db = unhealthy | BLOQUEIA | 🔴 YES |
| `touring doctor` symbol_store = unhealthy | BLOQUEIA | 🔴 YES |
| `ema_reward = 0.0` + daemon degraded | RL cold-start issue, BLOQUEIA | 🔴 YES |
| `mean_td_error` magnitude > 1e9 | RL overflow detectado — `touring suggest` não-confiável | 🟡 DEGRADED |
| `orphan_count > total_pub_symbols` | WIRING_DB_ANOMALY — todas claims de orphan requerem Chain 7 (grep) | 🟡 DEGRADED |
| `index.symbol_count < 1000` | Warning, não bloqueia | 🟢 NO |
| Todos checks PASS | Continua para FASE 1 | 🟢 PASS |

### FASE 0 OUTPUT FORMAT

```json
{
  "phase": 0,
  "status": "PASS|DEGRADED|BLOCKED",
  "compilation": {"exit_code": 0, "error_count": 0},
  "daemon": {"healthy": true, "degraded_components": [], "blocking_components": []},
  "signals": {"index_symbols": 38815, "orphans": 18957, "ema_reward": 0.06},
  "gate_decision": "CONTINUE|BLOCK",
  "blocking_reasons": [],
  "warnings": []
}
```

Se `gate_decision = BLOCK`: Orchestrator NÃO deve iniciar FASE 1. Reportar blocking_reasons ao usuário.

---

## FASE 4.5 — Pre-Implementation Audit Gate (detalhe)

**Executado DEPOIS do DECOMPOSE (FASE 4) e ANTES dos ENGINEERS (FASE 5). Auditor pode REJECT tasks marcadas como FALSE_POSITIVE — Engineers NÃO recebem tasks rejeitadas.**

### Auditor Pre-Implementation Protocol

#### Step 4.5.1: Review All DAG Subtasks

```bash
touring decompose status -j
# Para cada task no DAG: verificar se task é baseada em problema REAL
```

#### Step 4.5.2: Verify Problem Exists (PARA CADA TASK)

```bash
# Se task menciona arquivo X + linha N:
grep -n "pattern" <file_path> | head -5
# cargo check --workspace | grep "error"

# Se task menciona símbolo Y:
touring index find "Y" -j | jq '.[].file_path'
# touring ast find "Y" -j | jq '.[].module_path'
```

#### Step 4.5.3: FALSE_POSITIVE Classification

```json
{
  "task_id": "S-1",
  "verdict": "REAL_OPPORTUNITY|FALSE_POSITIVE",
  "evidence": "grep output ou touring index output",
  "blocking_reason": "se FALSE_POSITIVE: por quê?",
  "recommendation": "aceitar|modificar|rejeitar"
}
```

### FALSE_POSITIVE Detection Patterns

| Pattern | Detection | Action |
|---------|-----------|--------|
| Task diz "unwrap em production" mas todos unwraps estão em tests | Grep test modules | REJECT |
| Task diz "símbolo X não existe" mas `touring index find` retorna resultado | touring index find X | REJECT |
| Task diz "compilation error" mas `cargo check` exit = 0 | cargo check | REJECT |
| Task cita linha N mas arquivo tem < N linhas | wc -l file | REJECT |
| Task diz "feature desabilitada" mas consumer já ativou | touring wiring modules | REJECT |
| Task diz "orphan" mas símbolo tem consumer=1 | touring wiring orphans | ACCEPT |

### GATE OUTCOME

| Verdict | Action |
|---------|--------|
| REAL_OPPORTUNITY | Task entra no pool para FASE 5 (ENGINEERS) |
| FALSE_POSITIVE | Task REMOVIDA do DAG, não vai para Engineers |
| UNCERTAIN | Task fica suspensa, orchestrator decide |

### FASE 4.5 OUTPUT FORMAT

```json
{
  "phase": 4.5,
  "status": "COMPLETED",
  "tasks_reviewed": 9,
  "accepted": 6,
  "rejected": 3,
  "rejected_tasks": [
    {
      "task_id": "S-2",
      "original_description": "...",
      "verdict": "FALSE_POSITIVE",
      "evidence": "grep output ou CLI output",
      "blocking_reason": "Todos unwraps estão em test modules, não production"
    }
  ],
  "accepted_tasks": ["S-1", "S-3", "S-4", "S-5", "S-6", "S-7"],
  "gate_decision": "CONTINUE_TO_ENGINEERS",
  "engineers_receive": ["S-1", "S-3", "S-4", "S-5", "S-6", "S-7"]
}
```

Se `engineers_receive` está VAZIO: Orchestrator deve REPORTAR ao usuário antes de continuar.

---

## Subagent Prompt Template (full)

When spawned, you MUST start with the rule directive as FIRST LINE:

```
@/home/gabrielgadea/.claude/rules/TACO-subagent.md

# TACO SUBAGENT — BOUND TO RULE

## YOUR ROLE: [scout|engineer|validator]
## TASK: [specific description]
## ORCHESTRATOR: [orchestrator name]

## MANDATORY: Touring CLI Discovery (VGP)
Before ANY code generation:
1. touring index find <symbol>    # Verify symbol exists
2. touring ast blast <file>      # Blast radius check
3. touring memory recall <query> # Check past patterns

## MANDATORY: Speculative Validation
Before applying ANY edit:
1. touring shadow validate         # Validate in shadow
2. Check score >= 0.8

## RETURN FORMAT — ONLY RAW JSON, NO MARKDOWN FENCES

Your output MUST be ONLY valid JSON without any markdown formatting:

{"role": "YOUR_ROLE", "status": "completed|failed", "result": {...}, "quality_gates": {...}, "composite_score": 1.0}

**ABSOLUTELY FORBIDDEN:**
- NO triple backticks (` ```json ` or ` `` ` )
- NO markdown code blocks
- NO prose before or after the JSON
- NO explanatory text
- The FIRST character of your response must be `{`
- The LAST character of your response must be `}`

If you output anything except raw JSON, the orchestrator CANNOT parse your result.
```

---

## POST-AGENT VERIFICATION PROTOCOL (After FASE 5) — full

After each engineer agent completes, the orchestrator MUST run verification before accepting:

### Step V1: Output Parse
- Agent JSON must parse cleanly
- `status` must be "completed" (not "failed" or "partial")
- `composite_score` must be >= 1.0

### Step V2: Expected Files Check
- Agent output SHOULD include `expected_files: [...]` in result
- Each file in the list must exist on disk
- Missing files → respawn with focused scope
- Validate: `python3 ~/.claude/lib/plan_generator/checkpoint_validator.py <role> <output.json>`

### Step V3: Compilation Check
- Run `cargo check --workspace 2>&1 | grep "^error\[" | wc -l`
- If error count > 0 → respawn with compilation errors as context
- Only for Rust engineer agents (skip for Python/protocol agents)

### Step V4: Wiring Orphan Check
- Run `touring wiring orphans -j | python3 -c "import json,sys; print(json.load(sys.stdin).get('count', 0))"`
- Compare with baseline orphan count before agent ran
- New orphans > 0 → agent SHOULD wire them or document why

### Auto-Respawn Rules
- **Maximum**: 1 respawn per agent per task
- **Trigger**: V2 or V3 failure
- **Scope**: Respawn with ONLY the failing files/errors as context
- **Prompt**: Include specific error messages, not full task
- If respawn also fails → escalate to orchestrator with evidence

---

## Case Study: Wave Preditiva (2026-04-20) — L4 Multi-Crate Parallelism

Sessão de referência documentada em `~/projects/touring/docs/2026-04-20-predictive-wave.md`.

### Phase layout executado

| Fase | Agentes | Modo | Resultado |
|------|---------|------|-----------|
| FASE 0 | orchestrator (solo) | cargo check + doctor | PASS — 0 errors, daemon healthy |
| FASE 1 | touring-scouter x1 | VP-Scout Cadeias 1-7 | 3 FPs bloqueados (wiring stale, homonimia intra-crate, orphan falso) |
| FASE 4.5 | touring-auditor x1 | pre-implementation gate | 2 tasks rejeitadas como FALSE_POSITIVE |
| FASE 5 | **3 engineers paralelos** | acceptEdits, escopo por crate | D2+D3+D4 sem conflito de arquivo |
| FASE 6 | auditor + code-reviewer paralelos | leitura apenas | 0 conflito — escopo disjunto |
| FASE 7 | touring-scriber x1 | docs only | 3 rule files + session report |

**Outcome**: 47 novos testes, 9 P99 guards, 0 regressões, composite_score = 1.0.

### Regra: Parallelism por CRATE ou MÓDULO

Quando FASE 5 tem 3+ engineers simultâneos, distribuir escopo para minimizar conflito de file-level edits:

```
ESTRATEGIA A — por CRATE (preferida para multi-crate):
  Engineer-1: touring-analysis  (crates/touring-analysis/src/)
  Engineer-2: touring-hooks     (crates/touring-hooks/src/)
  Engineer-3: touring-cognitive (crates/touring-cognitive/src/)
  → cada engineer edita arquivos de UM crate → zero conflito

ESTRATEGIA B — por MÓDULO (para mesmo crate):
  Engineer-1: pre_tool_use/  (módulo de entrada)
  Engineer-2: task_list/     (módulo de dados)
  Engineer-3: plan_mode/     (módulo de output)
  → disjoint por diretório → risco de conflito ~0

ANTI-PATTERN — evitar:
  Múltiplos engineers editando mesmo arquivo (ex: hook_runtime.rs)
  → merge conflicts, composite_score degradado, respawn necessário
```

### Telemetria de validação pós-fase

Após FASE 5 de wave preditiva, verificar os 9 counters D5 via gate-metrics:

```bash
touring gate-metrics -j | jq '{
  blast_timeout: .blast_timeout_count,
  mcts_deadlock: .mcts_shadow_deadlock_detected_count,
  linucb_hints: .linucb_route_hint_count
}'
# blast_timeout == 0 e mcts_deadlock == 0 = P99 guards OK
# linucb_hints > 0 = workflow hints sendo consumidos corretamente
```

Ref: `~/.claude/skills/Touring/references/touring-cli-rl-quality.md` secao "Predictive Wave Counters (2026-04-20)" para interpretacao completa.
