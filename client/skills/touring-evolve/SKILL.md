---
name: touring-evolve
description: >
  TACO Orchestrator — Autonomous self-evolution engine v3.0 for the Touring workspace (~/projects/touring/).
  Orchestrates 14+ telemetry signals collection, ranks improvement opportunities via
  Rust OpportunityScorer (10 categories), spawns parallel TACO subagents in isolated worktrees,
  validates through 6 gates, and closes the feedback loop via ROI tracking.
  Gates: cargo clippy -D warnings + cargo test (zero regression) + wiring + flywheel.
  Flags: --dry-run, --auto, --min-score=N, --focus=category, --max=N, --parallel=N, --scope=crate.
license: MIT
metadata:
  author: TACO/Touring
  version: 3.0.0
  category: self-evolution
  tags:
    - touring
    - self-improvement
    - rl
    - autoresearch
    - parallel-evolution
version: 3.0.0
triggers:
  - /touring-evolve
  - touring evolve
  - self-evolution touring
  - autoaperfeiçoamento touring
  - evolução touring
  - touring autoevolve
context: fork
allowed-tools:
  - Read
  - Write
  - Edit
  - Bash
  - Glob
  - Grep
  - Agent
  - touring-cli
---

# /touring-evolve v3.0 — TACO Orchestrator: Touring Self-Evolution Engine

Ciclo autônomo de autoaperfeiçoamento do Touring. Como **TACO Orchestrator**, coordena
subagentes puros que coletam telemetria em paralelo, identificam oportunidades via
`OpportunityScorer`, implementam em worktrees isolados, validam em 6 gates, e fecham
o loop via ROI tracking.

**Arquitetura**: TACO Orchestrator (v5.1) — pure subagents, sem Agent Teams.
NÃO usa TaskList, TaskUpdate, SendMessage. Subagentes retornam JSON diretamente.

## Paradigma TACO Orchestrator

```
/touring-evolve [flags]
    │
    ├── [P0] Orchestrator: touring CLI (telemetry) + scoring
    ├── [P1] Orchestrator: DAG decompose (seeding task)
    ├── [P2] Orchestrator: spawn TACO subagents (pure, foreground+background)
    └── [P3] Orchestrator: consolidate JSON results + quality gate
```

**TACO Orchestrator Responsibilities**:
- Phase 0: Coleta telemetria via touring CLI (read-only)
- Phase 1: Score de oportunidades (Rust OpportunityScorer)
- Phase 2: Spawn subagentes puros via `Agent()` — coleta resultados diretamente
- Phase 3: Consolida resultados, roda 6 gates, armazena memória

**TACO Subagent Responsibilities**:
- Recebe JSON com oportunidade específica
- Executa RECALL → IMPLEMENT → VALIDATE → STORE
- Retorna JSON estruturado (único output válido)

---

## Phase 0 — Telemetria (Orchestrator: touring CLI)

```bash
# Baseline de wiring ANTES da evolução
WIRING_BASELINE=$(touring wiring status -j 2>/dev/null | jq '.orphan_count // 0')

# Batch paralelo — 14 sinais simultâneos
touring learning status -j     2>/dev/null > /tmp/te_learning.json    &
touring evolution drift -j      2>/dev/null > /tmp/te_drift.json      &
touring wiring orphans -j       2>/dev/null > /tmp/te_orphans.json    &
touring wiring modules -j       2>/dev/null > /tmp/te_modules.json    &
touring wiring status -j        2>/dev/null > /tmp/te_wstatus.json    &
touring cognitive metrics -j    2>/dev/null > /tmp/te_cognitive.json  &
touring cognitive engines -j    2>/dev/null > /tmp/te_engines.json    &
touring flywheel status -j      2>/dev/null > /tmp/te_flywheel.json    &
touring incremental status -j   2>/dev/null > /tmp/te_incremental.json &
touring evolution tools -j      2>/dev/null > /tmp/te_tools.json      &
touring memory recall "touring evolution improvement" -j 2>/dev/null > /tmp/te_memory.json &
touring gotcha list -j          2>/dev/null > /tmp/te_gotcha.json      &
touring evolution insights -j    2>/dev/null > /tmp/te_insights.json   &
touring memory stats -j         2>/dev/null > /tmp/te_memstats.json    &

wait

# Sintetizar TelemetrySnapshot
cat /tmp/te_*.json | jq -s 'add // {}' > /tmp/te_snapshot.json
```

**TelemetrySnapshot (14 campos)**:

| Campo | Fonte | Threshold |
|-------|-------|-----------|
| `mean_td_error` | `learning status → last_td_error` | > 0.5 |
| `wiring_orphan_count` | `wiring orphans → length` | > 0 |
| `drift_detected` | `evolution drift → trend == "degrading"` | any |
| `low_integration_modules` | `wiring modules → integration_score < 0.5` | any |
| `avg_hook_latency_ms` | `cognitive metrics → avg_latency_ms` | > 10ms |
| `optimizer_reset_count` | `memory recall → SelfOptimizer reset events` | > 0 |
| `parser_cache_hit_rate` | `incremental status → cache_hit_rate` | < 0.70 |
| `flywheel_unhealthy` | `flywheel status → components[].status != healthy` | any |
| `gotcha_recurrence_count` | `gotcha list → high decay_score entries` | > 5 |
| `tool_effectiveness_degraded` | `evolution tools → score < 0.5` | any |
| `undertested_modules` | `wiring modules → test_count < 3` | any |
| `cognitive_engine_degraded` | `cognitive engines → status != healthy` | any |
| `linucb_arm_distribution` | `learning status → arm_stats[]` | arm with < 5 pulls |
| `ema_reward_trend` | `learning status → ema_reward` | < 0.3 sustained |

---

## Phase 1 — Scoring de Oportunidades (Orchestrator)

**Deduplicação via memory recall**:
```bash
HASH=$(echo "{category}:{title}:{signals}" | sha256sum | cut -c1-16)
PREV=$(touring memory recall "evolution:attempt:${HASH}" -j 2>/dev/null | jq 'length')
```

**Categorias e scoring**:

| Categoria | Impact | Feasibility | Detecção |
|-----------|--------|------------|----------|
| `PerformanceHotspot` | 0.90 | 0.60 | `avg_hook_latency_ms > 10.0` |
| `DriftResponse` | 0.85 | 0.85 | `drift_detected == true` |
| `ConvergenceIssue` | 0.80 | 0.70 | `mean_td_error > 0.5` |
| `HyperparamDegradation` | 0.75 | 0.90 | `optimizer_reset_count > 0` |
| `CognitiveBottleneck` | 0.75 | 0.70 | `cognitive_engine_degraded == true` |
| `MissingIntegration` | 0.70 | 0.80 | `integration_score < 0.5` |
| `CacheInefficiency` | 0.65 | 0.85 | `parser_cache_hit_rate < 0.70` |
| `IntegrationGap` | 0.60 | 0.90 | `wiring_orphan_count > 0` |
| `GotchaRecurrence` | 0.55 | 0.95 | `gotcha_recurrence_count > 5` |
| `TestCoverageGap` | 0.50 | 0.95 | `undertested_modules not empty` |

```
composite_score = impact_score × feasibility_score
```

**Output para usuário (ou --auto)**:

```
TELEMETRIA TOURING (YYYY-MM-DD HH:MM)
=======================================
RL:          update_count=N, ema_reward=X, mean_td_error=X
Wiring:      orphan_count=N, modules_below_50%=[...]
Drift:       DETECTADO / ESTAVEL
Cognitive:   avg_latency=Xms | engines=healthy/degraded
Cache:       hit_rate=X (OK / ABAIXO)
Flywheel:    healthy / N componentes degraded
Gotchas:     N recorrentes

OPORTUNIDADES (min_score=N)
================================
[1] [score=X.XX] Categoria — Titulo
    Sinais: ...
    Impl: ...

Implementar? [S/n] ou especifique: 1,3
```

---

## Phase 2 — Spawn TACO Subagents (Orchestrator)

Para cada oportunidade com `composite_score >= min_score`:

**Primeiro subagente: FOREGROUND** (valida bootstrap)
```bash
RESULT=$(Agent(
  description="evolve: {opportunity.title}",
  prompt="$(cat <<'SUBAGENT_PROMPT'
@/home/gabrielgadea/.claude/rules/TACO-subagent.md

# TACO SUBAGENT — EVOLUTION ENGINE

## ROLE: engineer
## TASK: Implementar evolucao Touring
## ORCHESTRATOR: touring-evolve

## OPPORTUNITY JSON:
{candidate_json}

## MANDATORY: Touring CLI Discovery (VGP)
1. touring index find <symbol>    # Verificar simbolos relevantes
2. touring ast blast <file>      # Blast radius
3. touring memory recall <query> # Lições de evoluções anteriores

## EXECUTE: RECALL → IMPLEMENT → VALIDATE → STORE
1. RECALL: touring memory recall + touring gotcha match
2. IMPLEMENT: aplicar mudanca no worktree
3. VALIDATE: cargo clippy + cargo test
4. STORE: touring memory store (sucesso ou falha)

## RETURN — ONLY RAW JSON:
{"role": "engineer", "status": "completed|failed", "result": {...}, "quality_gates": {...}, "composite_score": N.N}
SUBAGENT_PROMPT
)",
  subagent_type="general-purpose",
  run_in_background=False
))
```

**Demais subagentes: BACKGROUND** (paralelo)
```bash
Agent(
  description="evolve: {opp2.title}",
  prompt="$(cat <<'SUBAGENT_PROMPT'
...same template...
SUBAGENT_PROMPT
)",
  subagent_type="general-purpose",
  run_in_background=True
)
```

**Controle**: `--parallel=N` (default: 3, max: 5)

---

## Phase 3 — Consolidação e Gates (Orchestrator)

Após receber JSON de todos subagentes:

**Gate 1 — Cargo check**:
```bash
cargo check --workspace --exclude touring-python 2>&1 | tail -3
# Esperado: Finished ... 0 errors
```

**Gate 2 — Clippy deny**:
```bash
cargo clippy --workspace --exclude touring-python -- -D warnings 2>&1 | tail -3
# Esperado: Finished ... 0 warnings
```

**Gate 3 — Tests (zero regression)**:
```bash
cargo test --workspace --exclude touring-python 2>&1 | grep -E "FAILED|^test result:"
# Esperado: 0 failed
```

**Gate 4 — Wiring orphans**:
```bash
NEW_ORPHANS=$(touring wiring status -j | jq '.orphan_count // 0')
# Esperado: NEW_ORPHANS <= WIRING_BASELINE
```

**Gate 5 — Flywheel health**:
```bash
touring flywheel status -j | jq '.components[] | select(.status != "healthy")'
# Esperado: output vazio
```

**Gate 6 — Integration scores**:
```bash
touring wiring modules -j | jq '.[] | select(.integration_score < 0.4)'
# Esperado: output vazio
```

---

## Phase 4 — Feedback Loop (Orchestrator)

```bash
# 1. Memory store por candidato
HASH=$(echo "{category}:{title}:{signals}" | sha256sum | cut -c1-16)
touring memory store "evolution:attempt:${HASH}" \
  "Tentativa: {title}. Resultado: {gate_results}. Score: {composite_score_realized}"

# 2. RL reward injection
touring learning reward {reward_value} -j 2>/dev/null
# Sucesso: 1.0 | Falha: -0.3 | Parcial: 0.5

# 3. ROI tracking
touring memory store "evolution:roi:{category}:$(date +%Y%m%d)" \
  "ROI: sucesso={success}, score_previsto={cs}, score_realizado={csr}"

# 4. Session checkpoint
touring session checkpoint -j 2>/dev/null
```

---

## Bandeiras

| Flag | Comportamento | Default |
|------|--------------|---------|
| `--dry-run` | Análise + scoring, sem implementação | — |
| `--auto` | Implementa todos >= min_score | — |
| `--min-score=0.7` | Threshold mínimo composite_score | 0.5 |
| `--focus=performance` | Apenas PerformanceHotspot + CognitiveBottleneck | — |
| `--focus=integration` | Apenas IntegrationGap + MissingIntegration | — |
| `--focus=learning` | DriftResponse + ConvergenceIssue + HyperparamDegradation | — |
| `--focus=quality` | TestCoverageGap + GotchaRecurrence + CacheInefficiency | — |
| `--max=3` | Máximo candidatos por execução | 5 |
| `--parallel=2` | Máximo worktrees simultâneos | 3 |
| `--scope=crate` | Limitar a um crate específico | workspace |

---

## TACO Subagent Prompt (para spawn via Agent())

```markdown
@/home/gabrielgadea/.claude/rules/TACO-subagent.md

# TACO SUBAGENT — BOUND TO RULE

## YOUR ROLE: engineer
## TASK: Implementar melhoria autônoma no Touring
## ORCHESTRATOR: touring-evolve

## OPPORTUNITY:
- ID: {candidate.id}
- Categoria: {candidate.category}
- Título: {candidate.title}
- Descrição: {candidate.description}
- Implementação: {candidate.suggested_implementation}
- Evidências: {candidate.signals}
- composite_score: {candidate.composite_score}

## MANDATORY: Touring CLI Discovery (VGP)
Antes de qualquer código:
```bash
touring index find <relevant_symbol>
touring ast blast <target_file>
touring memory recall "evolution:{category}"
touring gotcha match <target_file>
```

## EXECUTE: RECALL → IMPLEMENT → VALIDATE → STORE (max 3 tentativas)

### PASSO 1 — RECALL
```bash
touring memory recall "evolution:{candidate.category}" -j 2>/dev/null | head -20
touring gotcha match {candidate.target_file} -j 2>/dev/null | head -10
```
→ Se recall retornar lições "NEVER"/"ALWAYS": seguir estritamente.
→ Se tentativa prévia com falha: usar abordagem diferente.

### PASSO 2 — IMPLEMENT
Implementar conforme suggested_implementation, ajustada pelos insights do recall.
Workspace: ~/projects/touring/ | Testes baseline: ~3735

### PASSO 3 — VALIDATE
```bash
cargo check -p {target_crate} 2>&1 | tail -3
cargo clippy -p {target_crate} -- -D warnings 2>&1 | tail -3
cargo test -p {target_crate} 2>&1 | grep "^test result:"
```

### PASSO 4 — STORE
```bash
# Sucesso:
touring memory store "evolution:{id}:ok" "Sucesso: {o_que_funcionou}"
# Falha:
touring memory store "evolution:{id}:fail:{tentativa}" "Falha: {diagnóstico}"
```

## REGRAS INVIOLÁVEIS:
1. clippy: zero warnings (deny all)
2. Zero regression: testes existentes devem passar
3. Novos módulos: mínimo 3 testes
4. Zero unwrap() em produção — usar `?` ou `.expect("reason")`
5. Max 3 tentativas → escalar se ainda falhar

## RETURN — ONLY RAW JSON (NO fences, NO prose):
{"role": "engineer", "status": "completed|failed", "result": {"implemented": [], "gate_check": "PASS|FAIL", "gate_clippy": "PASS|FAIL", "gate_tests": "PASS|FAIL", "attempts": N, "composite_score_realized": N.N, "loc_added": N, "tests_added": N, "lessons_stored": N}, "quality_gates": {"functional": "PASS|FAIL", "robust": "PASS|FAIL", "readable": "PASS|FAIL", "documented": "PASS|FAIL", "secure": "PASS|FAIL", "no_regression": "PASS|FAIL"}, "composite_score": N.N}
```

---

## Padrões TACO v5.1 — Pure Subagents

| Aspecto | Agent Teams (ANTIGO) | TACO Pure Subagents (NOVO) |
|---------|---------------------|---------------------------|
| Spawn | TeamCreate + TaskCreate + TaskUpdate | `Agent()` direto |
| Coordenação | TaskList + SendMessage | JSON return direto |
| team_name | Obrigatório | Proibido |
| Memória | TaskList | touring CLI + JSON |
| Resultado | SendMessage | Agent() return value |

---

## Invariantes de Segurança

1. **Sempre em worktree** — mudanças nunca chegam ao workspace sem 6 gates
2. **Regressão zero** — teste falhar → worktree descartado
3. **Clippy deny** — zero warnings antes de integrar
4. **Human gate** — API pública OU LOC > 200 → confirmar antes
5. **Memory store obrigatório** — toda tentativa (sucesso ou falha)
6. **RL reward** — sucesso=1.0, falha=-0.3, parcial=0.5
7. **Wiring baseline** — capturado ANTES da evolução

---

## Exemplos

### Auto-evolução com foco em integração
```
/touring-evolve --auto --focus=integration --max=2 --parallel=2
```

### Dry run — diagnóstico completo
```
/touring-evolve --dry-run
```

### Resposta a drift
```
/touring-evolve --focus=learning --min-score=0.7 --auto
```
