# PLAN: Diagnostic Precision v2 — Full Ecosystem Analysis + Improvement Plan

> **Version**: v2.0 | **Date**: 2026-04-12
> **Predecessor**: `PLAN-diagnostic-precision-v1.md` (7 root causes, 5 fixes)
> **Method**: 4 parallel analysis agents + direct investigation on clean context
> **Coverage**: Hooks, Agent Definitions, Skills/MCP/Rules, TACO Protocol, CILA Routing, Daemon Signals

---

## Executive Summary

O ecossistema Touring opera em 6 camadas: **Daemon** (Rust binary), **CLI** (~88 commands), **MCP Tools** (80 registered), **Hooks** (22 hooks across 15 events), **Agent Definitions** (5 agents, 3.401 linhas), **Skills** (11 touring-related), **Rules** (4 files). A análise v1 identificou 7 root causes; esta expansão adiciona **5 novos root causes** (RC8-RC12) e expande os 5 fixes originais para **12 fixes priorizados**.

**Token waste estimado por sessão**: ~40% (confirmado pelo v1). **Root cause dominante**: plan docs como fonte de verdade estática (RC1) + CILA routing decorativa (RC8).

---

## PARTE 1: Estado Atual do Ecossistema

### 1.1 Daemon Health (snapshot 2026-04-12)

| Componente | Status | Nota |
|-----------|--------|------|
| knowledge_db | healthy | always initialized |
| linucb_bandit | healthy | loaded |
| symbol_store | healthy | 297.545 symbols, 6.059 files |
| crdt_graph | healthy | loaded |
| predictor | healthy | loaded |
| cognitive_runtime | healthy | initialized |
| enrichment_pipeline | active | auto-triggered at session-start |
| gotcha_db | healthy | integrated in knowledge_db |

**ema_reward**: 0.1796 | **arm_count**: 8 | **update_count**: 1 | **sessions**: 20

### 1.2 Hook Infrastructure (22 hooks, 15 events)

| Event | Hooks | Commands |
|-------|-------|----------|
| **PreToolUse:Read** | 1 | `touring-hook pre-read` (filtered: code files only) |
| **PreToolUse:Edit** | 2 | `touring-hook pre-edit` + gotcha check |
| **PreToolUse:Write** | 1 | `touring-hook pre-write` (speculative validation) |
| **PreToolUse:Bash** | 2 | `touring-hook pre-bash` + `block_git.sh` |
| **PreToolUse:Grep\|Glob\|Bash** | 1 | `gitnexus-hook.cjs` |
| **PostToolUse** | 5 | `touring-hook post-{read,edit,write,bash}` + gitnexus |
| **PostToolUseFailure** | 1 | `touring-hook post-tool-failure` |
| **SessionStart/Stop** | 2 | `touring-hook session-start/stop` |
| **FileChanged** | 1 | Touring file change tracker |
| **CwdChanged** | 1 | Project context switch |
| **SubagentStart/Stop** | 2 | Subagent lifecycle tracking |
| **Stop** | 1 | Session persistence |
| **PreCompact/PostCompact** | 2 | Cache management |
| **InstructionsLoaded** | 1 | Project knowledge injection |
| **Setup** | 1 | Initial configuration |
| **UserPromptSubmit** | 1 | `prompt_enhancer.py` (intent classification + technique injection) |

**Noise sources identificadas**:
1. `touring-hook pre-bash` → `ls crates/touring-memory/src/` (crate inexistente) → **Exit code 2 em cada Bash call**
2. Hooks tentam `cargo check` em `$HOME/.claude` e `$HOME` (sem Cargo.toml) → **Exit code 101** constante
3. Gotcha DB entry #46: pattern "touring" com hit_count **4.230** — gotcha genérico demais captura tudo

### 1.3 Agent Definitions (5 agents)

| Agent | Lines | KB | Model | Info Density |
|-------|-------|-----|-------|-------------|
| touring-engineer | 720 | 37.1 | sonnet-4-6 | ~36% |
| touring-auditor | 736 | 28.6 | sonnet-4-6 | ~38% |
| touring-architect | 692 | 26.3 | sonnet-4-6 | ~32% |
| touring-scouter | 638 | 22.8 | sonnet-4-6 | ~28% |
| touring-scriber | 615 | 20.2 | sonnet-4-6 | ~41% |
| **TOTAL** | **3.401** | **135** | — | **35% avg** |

**Duplicação mapeada**:

| Bloco Duplicado | Fonte Original | Presente Em | Linhas Desperdiçadas |
|----------------|----------------|-------------|---------------------|
| CLI Quick Reference (~100 lines) | `touring-cli-commands.md` (global rule) | Todos 5 | ~400 |
| VP-Scout 4 chains (~70 lines avg) | `VP-Scout.md` (global rule) | scouter, architect, engineer, auditor | ~210 |
| Pre-flight phase (~15 lines) | Padrão comum | Todos 5 | ~60 |
| FASE 4.5 anti-FP gate (~70 lines) | `TACO-subagent.md` (global rule) | auditor, engineer | ~70 |
| Quality gates table | `TACO-subagent.md` | Todos 5 | ~40 |
| RL reward injection pattern | Padrão comum | architect, engineer, auditor, scriber | ~30 |
| **Total desperdiçado** | | | **~810 linhas** |

**Token tax**: ~33.750 tokens consumidos em prompts de agentes (antes de qualquer trabalho).
**Projeção após limpeza**: ~25.000 tokens (-26%).

### 1.4 Skills Touring (11 ativas)

| Skill | Linhas | Propósito |
|-------|--------|-----------|
| Touring | 612 | Master skill: code intelligence, memory, sessions, RL |
| TACO-subagent | 444 | Orchestration protocol: sequential phases |
| touring-evolve | 423 | Self-evolution: 14+ telemetry signals |
| touring-excellence | 283 | Potencialização do workspace |
| taco-planning | 142 | Geração de planos com 9 dimensões de qualidade |
| touring-token-efficient-workflow | 72 | Padrões de eficiência de tokens |
| touring-scip | 66 | Export SCIP do symbol index |
| touring-query | 35 | DSL para queries de file metadata |
| touring-file-metadata | 29 | File-level LOC, quality, blast radius |
| touring-wiring-suggest | 28 | Ataque a orphan pub symbols |
| touring-search | 19 | BM25 FTS5 symbol/doc search |

### 1.5 MCP Tools vs CLI Commands

| Métrica | Valor |
|---------|-------|
| MCP tools registrados | 80 |
| CLI commands | ~88 |
| CLI-only (sem MCP) | ~12 |
| MCP-only (sem CLI) | ~15 |

**Gaps CLI→MCP significativos**:

| CLI Command | Severidade |
|------------|-----------|
| `touring diary *` (4 commands) | MEDIUM — 0 MCP tools |
| `touring cognitive metrics/engines` | MEDIUM — 0 MCP tools |
| `touring memory stats/list` | MEDIUM — 0 MCP tools |
| `touring e2e` | MEDIUM — 0 MCP tools |
| `touring learning reward` | MEDIUM — 0 MCP tools |
| `touring index search/find/rebuild` | MEDIUM — apenas `index_status` exposto |
| `touring gotcha add/match` | LOW |

### 1.6 Rules (4 files)

| File | Size | Versão | Propósito |
|------|------|--------|-----------|
| TACO-subagent.md | 17K | v6.1 | Phase protocol, subagent contracts, gates |
| touring-cli-commands.md | 20K | v3.4 | Referência completa: ~88 commands |
| VP-Scout.md | 13K | v1.0 | 5 cadeias de verificação anti-FP |
| file-metadata-first.md | 508B | — | `touring ast meta` antes de editar |

---

## PARTE 2: Root Cause Analysis Expandida

### Root Causes do v1 (RC1-RC7) — Confirmados

| RC | Severidade | Resumo |
|----|-----------|--------|
| **RC1** | CRITICAL | Plan docs como fonte de verdade estática — 13 FPs |
| **RC2** | HIGH | Background agents falham silenciosamente — 2 agentes zero output |
| **RC3** | HIGH | Decisões arquiteturais não validadas antes de implementação |
| **RC4** | MEDIUM | Touring memory FP feedback loop quebrado (key format inconsistente) |
| **RC5** | LOW | Hook system gera noise constante (~50+ mensagens/sessão) |
| **RC6** | MEDIUM | Agent definitions bloat (680 lines avg, 35% info density) |
| **RC7** | MEDIUM | TACO protocol overhead para tasks simples |

### Novos Root Causes (RC8-RC12) — Descobertos nesta Análise

#### RC8: CILA Routing Decorativa (CRITICAL)

**Evidência**: O TACO rule (TACO-subagent.md, linha 41) diz **"NUNCA pular fases"**, enquanto o TACO SKILL (linha 365-373) define routing por CILA level:

| CILA | Fases no SKILL | Fases Executadas na Prática |
|------|----------------|---------------------------|
| L0-L1 | SOLO — sem subagents | **Todas 7+ fases** (rule override) |
| L2 | Fases 1+5 | **Todas 7+ fases** |
| L3 | Fases 1+2+5+6 | **Todas 7+ fases** |
| L4+ | Todas | Todas (correto) |

**Impacto**: Tasks L1-L2 consomem 10-50x mais tokens que o necessário. A classificação CILA no Rust (79 regex patterns, LRU-cached) funciona corretamente para budget de hooks, mas NÃO influencia seleção de fases TACO.

**Root Cause**: Contradição direta entre rule (NUNCA pular) e skill (skip por CILA). Rule tem prioridade — CILA routing é letra morta para TACO.

#### RC9: Hook Cascade em Diretórios sem Cargo.toml (HIGH)

**Evidência**: A cada Bash call, os PreToolUse hooks disparam:
```
⚡ Bash failure: Exit code 101
error: could not find `Cargo.toml` in `/home/gabrielgadea/.claude` or any parent directory
```

**Impacto**: Noise em TODA operação Bash. Hooks running `cargo check` no CWD (`$HOME` ou `$HOME/.claude`) que não são workspaces Rust. Cada message adiciona ~200 bytes ao contexto.

**Root Cause**: Hooks não verificam se CWD contém Cargo.toml antes de invocar cargo. O `touring-hook` binary assume que está sempre no touring workspace.

#### RC10: Gotcha DB Entry #46 com Pattern Genérico (MEDIUM)

**Evidência**: `touring gotcha match "crates/touring-memory"` retorna entry com:
- pattern: `"touring"` (captura QUALQUER coisa com "touring" no path)
- hit_count: **4.230** 
- severity: warning
- resolved: false

**Impacto**: Gotcha genérico gera falso alarme em basicamente todo arquivo do workspace. O gotcha correto seria `"touring-memory"` (crate específico), não `"touring"` (match-all).

**Root Cause**: `post-tool-failure` auto-cria gotchas com patterns extraídos do erro. O pattern extraction foi overly broad.

#### RC11: MCP/CLI Coverage Gaps Causam Fallback Ineficiente (MEDIUM)

**Evidência**: 12 CLI commands sem MCP equivalente (diary, cognitive, memory stats/list, e2e, learning reward). Quando o orchestrator precisa destes, faz fallback para Bash → `touring CLI command` → parse stdout → extrai resultado. Cada fallback custa 2-3x mais tokens que um MCP tool call direto.

**Impacto**: ~20% das operações touring usam fallback CLI via Bash ao invés de MCP tools nativos.

**Root Cause**: MCP tools foram implementados incrementalmente; gaps nunca foram sistematicamente auditados.

#### RC12: Prompt Enhancer Dispara em TODA Interação (LOW)

**Evidência**: `UserPromptSubmit` hook roda `prompt_enhancer.py` a cada mensagem do usuário. O classificador seleciona técnicas e injeta `additionalContext`. Para mensagens simples ("ok", "continue"), o enhancer adiciona ~500-1000 tokens de "Chain of Thought" + "Structured Output" desnecessários.

**Impacto**: ~500-1000 tokens de overhead por interação simples. Em sessões com 50+ interações = ~25-50k tokens desperdiçados.

**Root Cause**: Sem short-circuit para mensagens curtas/triviais. O enhancer trata "ok" e "implement a distributed consensus algorithm" com a mesma pipeline.

---

## PARTE 3: Improvement Plan — 12 Fixes Priorizados

### Tier 1: IMMEDIATE (Bloqueadores de Eficiência)

#### Fix 1: Ativar CILA Routing no TACO (CRITICAL — 1 linha)

**Problema**: RC8 — "NUNCA pular fases" anula CILA routing.

**Solução**: Editar `~/.claude/rules/TACO-subagent.md` linha 41:

```diff
- NUNCA pular fases ou fundir fases adjacentes
+ Fases executadas conforme CILA routing: L0-L1=SOLO, L2=fases 1+5, L3=1+2+5+6, L4+=todas. Dentro de cada fase, sequência obrigatória.
```

**Impacto esperado**: -80% tokens em L1-L2, -40% em L3.
**Esforço**: S (30min) | **Risco**: LOW

---

#### Fix 2: Code-First Verification Gate (CRITICAL — rule update)

**Problema**: RC1 — Scouts tratam plan docs como ground truth.

**Solução**: Adicionar ao VP-Scout.md e ao touring-scouter agent definition:

```
RULE: VERIFY_BEFORE_REPORT
Before classifying ANY finding as "NOT_IMPLEMENTED" or "PENDING":
1. `touring index find <symbol>` — count > 0 = EXISTS
2. `grep -rn <pattern> crates/ | head -5` — matches = EXISTS
3. Se claims compilation error: `cargo check --workspace 2>&1 | tail -3` — exit 0 = COMPILES
Plan docs are INTENT. Code is STATE. Code wins.
```

**Esforço**: S (1h) | **Risco**: LOW

---

#### Fix 3: Fix Hook Noise — Cargo.toml Guard (HIGH — hook fix)

**Problema**: RC9 — Hooks disparam `cargo check` em dirs sem Cargo.toml.

**Solução**: Modificar `touring-hook` binary ou adicionar guard no settings.json:

```json
{
  "matcher": "Bash",
  "hooks": [{
    "type": "command",
    "command": "$HOME/.claude/hooks/touring-hook pre-bash",
    "timeout": 10,
    "if": "Bash(cargo *|rustc *|touring *)"
  }]
}
```

Alternativa: no `touring-hook` binary, verificar `Cargo.toml` existence no CWD antes de qualquer cargo operation.

**Esforço**: S (1h) | **Risco**: LOW

---

#### Fix 4: Fix Gotcha #46 Pattern Genérico (HIGH — 1 comando)

**Problema**: RC10 — Pattern "touring" captura tudo.

**Solução**:
```bash
# Opção A: Resolver o gotcha (marcar como resolvido)
touring gotcha resolve 46

# Opção B: Recriar com pattern correto
touring gotcha add "touring-memory" "Crate touring-memory foi planejado mas nunca criado. Referências a este crate são stale." --severity low
```

**Esforço**: S (15min) | **Risco**: LOW

---

### Tier 2: THIS WEEK (Eficiência Estrutural)

#### Fix 5: Compact Agent Definitions (HIGH — refactor)

**Problema**: RC6 — 3.401 linhas, 35% info density, 810 linhas duplicadas.

**Solução**: Para cada agent:
1. **REMOVER** CLI Quick Reference (já é global rule) → -500 linhas
2. **REMOVER** VP-Scout chains de architect/engineer/auditor (já em VP-Scout.md) → -210 linhas
3. **EXTRAIR** pre-flight + quality gates + JSON format para `_shared-touring-agent-base.md` → -100 linhas

**Target por agente**:

| Agent | Atual | Target | Método |
|-------|-------|--------|--------|
| touring-scouter | 638 | <200 | Extrair chains para VP-Scout.md ref |
| touring-architect | 692 | <200 | Extrair CLI ref + VP-Scout |
| touring-engineer | 720 | <200 | Extrair VGP + CLI |
| touring-auditor | 736 | <250 | Extrair checklist + VP-Scout |
| touring-scriber | 615 | <200 | Extrair templates |

**Esforço**: L (6h) | **Risco**: MEDIUM (agents podem perder context necessário se corte for excessivo)
**Mitigação**: Testar cada agent individualmente após corte com task real.

---

#### Fix 6: Agent Output Verification Protocol (HIGH)

**Problema**: RC2 — Background agents "completam" sem produzir arquivos.

**Solução**: Adicionar ao TACO orchestrator behavior:

```
POST-AGENT VERIFICATION (obrigatório para mode=acceptEdits):
1. Agent MUST declare `expected_files: [...]` no JSON output
2. Orchestrator verifica existência de CADA expected_file após completion
3. Se expected_file missing → status = "partial", respawn com escopo reduzido
4. Se ALL expected_files exist → status = "completed", prosseguir
```

**Esforço**: M (3h) | **Risco**: LOW

---

#### Fix 7: Memory Key Format Standardization (MEDIUM)

**Problema**: RC4 — Keys inconsistentes impedem recall.

**Solução**: Padronizar formato:

```
<category>:<scope>:<identifier>
  fp:session:2026-04-12:schema_v7        # false positive
  lesson:engineer:include_macro          # lesson learned
  pattern:split:separate_impl_blocks     # reusable pattern
  gotcha:hook_registry:dual_assert       # known pitfall
```

Adicionar validação ao touring memory store (reject keys sem `:` separator).

**Esforço**: M (2h) | **Risco**: LOW

---

#### Fix 8: Prompt Enhancer Short-Circuit (MEDIUM)

**Problema**: RC12 — Enhancer dispara pipeline completa para "ok".

**Solução**: Adicionar early return em `prompt_enhancer.py`:

```python
# Short-circuit for trivial prompts
if len(prompt.strip()) < 20 and not any(kw in prompt.lower() for kw in ['plan', 'fix', 'debug', 'test', 'implement']):
    return {}  # No enhancement needed
```

**Esforço**: S (30min) | **Risco**: LOW

---

### Tier 3: NEXT WEEK (Cobertura e Evolução)

#### Fix 9: MCP Tool Coverage — Fill Top Gaps (MEDIUM)

**Problema**: RC11 — 12 CLI commands sem MCP equivalent.

**Solução** prioritizada:

| Gap | MCP Tool a criar | Prioridade |
|-----|-----------------|-----------|
| `touring e2e` | `touring_e2e_analysis` | HIGH — usado para validação |
| `touring memory stats/list` | `touring_memory_list` | HIGH — usado para pattern discovery |
| `touring learning reward` | `touring_learning_reward` | MEDIUM — RL feedback loop |
| `touring index find/search` | `touring_index_find` | MEDIUM — symbol lookup direto |
| `touring cognitive metrics` | `touring_cognitive_metrics` | LOW |
| `touring diary *` | `touring_diary_*` | LOW |

**Esforço**: L (8h para top 4) | **Risco**: LOW

---

#### Fix 10: TACO Phase 0 Conditional Execution (MEDIUM)

**Problema**: RC7 parcial — Phase 0 roda `cargo check --workspace` (~30-120s) para TODA task.

**Solução**: Phase 0 condicional:

```
SE task NÃO envolve arquivos .rs → SKIP cargo check
SE CWD não contém Cargo.toml → SKIP cargo check
SE último cargo check < 5 min atrás (cache) → SKIP
SENÃO → rodar cargo check
```

**Esforço**: M (2h) | **Risco**: LOW

---

#### Fix 11: Phase 3 (Context7) Removal para Codebase Interno (LOW)

**Problema**: RC7 parcial — Context7 retorna docs genéricos para codebase touring.

**Solução**: Skip Phase 3 para tasks que não envolvem bibliotecas externas:

```
SE task referencia crate externo (serde, tokio, etc.) → Context7 útil
SE task é 100% codebase touring → SKIP Phase 3
```

**Esforço**: S (30min) | **Risco**: LOW

---

#### Fix 12: Evolution Drift Auto-Detection (LOW)

**Problema**: `touring evolution drift` nunca é usado na prática.

**Solução**: Adicionar ao SessionEnd hook:

```bash
touring evolution drift -j | jq '.alert_level'
# Se "degraded" ou "structural" → injetar warning no session-stop output
```

**Esforço**: S (1h) | **Risco**: LOW

---

## PARTE 4: Execution Plan

```
IMMEDIATE (hoje):
  Fix 1 (CILA routing)  ─── S (30min) ─── 1 linha em TACO-subagent.md
  Fix 2 (Code-First)    ─── S (1h)    ─── rule update VP-Scout + scouter
  Fix 3 (Hook noise)    ─── S (1h)    ─── guard Cargo.toml
  Fix 4 (Gotcha #46)    ─── S (15min) ─── touring gotcha resolve 46
                                          Total: ~3h
  ↓
THIS WEEK:
  Fix 5 (Agent compact) ─── L (6h)    ─── refactor 5 agent definitions
  Fix 6 (Agent verify)  ─── M (3h)    ─── output verification protocol
  Fix 7 (Memory keys)   ─── M (2h)    ─── key format standardization
  Fix 8 (Enhancer SC)   ─── S (30min) ─── short-circuit trivial prompts
                                          Total: ~12h
  ↓
NEXT WEEK:
  Fix 9 (MCP gaps)      ─── L (8h)    ─── top 4 MCP tools
  Fix 10 (Phase 0 cond) ─── M (2h)    ─── conditional cargo check
  Fix 11 (Phase 3 skip) ─── S (30min) ─── Context7 conditional
  Fix 12 (Drift detect) ─── S (1h)    ─── SessionEnd hook
                                          Total: ~12h

GRAND TOTAL: ~27h
```

---

## PARTE 5: Success Metrics

| Métrica | Atual | Target v2 | Medição |
|---------|-------|----------|---------|
| False positives/sessão | 13 | ≤ 1 | Count FPs em audit phase |
| Agent success rate (files created) | 40% (2/5) | ≥ 90% | Check expected_files |
| Agent definition size | 680 lines avg | <200 lines avg | `wc -l agents/*.md` |
| Hook noise/sessão | ~50+ mensagens | 0 | Count `⚡ Bash failure` em context |
| Memory FP recall accuracy | 0/13 | ≥ 12/13 | `touring memory recall "fp:"` |
| TACO overhead L2 tasks | 7+ phases | 2 phases | Phase count por CILA level |
| Token waste/sessão | ~40% | <10% | Estimate útil vs desperdiçado |
| Prompt enhancer overhead (trivial) | ~1000 tokens | 0 tokens | Short-circuit counter |
| MCP tool fallback rate | ~20% | <5% | Count `touring CLI` em Bash |
| Gotcha false alarm rate | 4.230 hits | <100 | `touring gotcha stats` |
| CILA routing compliance | 0% (decorativa) | 100% (enforced) | CILA level vs phases executed |
| cargo check noise/sessão | Every Bash call | 0 (guarded) | Count exit code 101 |

---

## PARTE 6: Dependency Graph

```
Fix 1 (CILA) ──────────────────────→ Fix 10 (Phase 0 conditional)
                                   ↗
Fix 2 (Code-First) ──→ Fix 5 (Agents compact) ──→ Fix 6 (Agent verify)
                                   ↘
Fix 3 (Hook noise) ──→ Fix 4 (Gotcha #46)         Fix 11 (Phase 3 skip)
                                                         ↓
Fix 7 (Memory keys) ───────────────────────→ Fix 9 (MCP gaps)
                                                         ↓
Fix 8 (Enhancer SC) ──────────────────────→ Fix 12 (Drift detect)
```

**Nota**: Fixes 1-4 são independentes entre si — podem ser implementados em paralelo.

---

## PARTE 7: Risk Register

| Risk | Probabilidade | Impacto | Mitigação |
|------|--------------|---------|-----------|
| Fix 5 (compact agents) remove context necessário | MEDIUM | HIGH | Testar cada agent com task real antes de deploy |
| Fix 1 (CILA) permite agents L2 pularem scouts necessários | LOW | MEDIUM | VP-Scout chains ainda aplicam na fase 1 |
| Fix 3 (hook guard) filtra hooks legítimos junto com noise | LOW | HIGH | Guard específico para Cargo.toml, não broad filter |
| Fix 9 (MCP tools) introduz novos bugs no daemon | LOW | HIGH | Feature-gate cada novo tool, testar isoladamente |
| Fix 8 (enhancer SC) perde prompts curtos mas complexos | LOW | LOW | Whitelist de keywords preserva prompts relevantes |
| CILA classifier categoriza errado (L4 como L2) | LOW | HIGH | Fallback: se task falha em L2, retry como L4 |

---

## PARTE 8: Apêndice — Audit Trail Completo

### A. Hooks por Event Type

| Event | Count | Touring Hooks | Other Hooks |
|-------|-------|---------------|-------------|
| PreToolUse | 5 | 4 (pre-read, pre-edit, pre-write, pre-bash) | 1 (gitnexus, block_git) |
| PostToolUse | 5 | 4 (post-read, post-edit, post-write, post-bash) | 1 (gitnexus) |
| PostToolUseFailure | 1 | 1 (post-tool-failure) | 0 |
| SessionStart | 1 | 1 (session-start) | 0 |
| SessionEnd | 1 | 1 (session-stop) | 0 |
| FileChanged | 1 | 1 | 0 |
| CwdChanged | 1 | 1 | 0 |
| SubagentStart/Stop | 2 | 2 | 0 |
| Stop | 1 | 1 | 0 |
| PreCompact/PostCompact | 2 | 2 | 0 |
| InstructionsLoaded | 1 | 1 | 0 |
| Setup | 1 | 1 | 0 |
| UserPromptSubmit | 1 | 0 | 1 (prompt_enhancer.py) |

### B. MCP Tools por Módulo

| Module | Tools | Key Tools |
|--------|-------|-----------|
| tools_core.rs | 17 | ast_overview, ast_find, memory_store, memory_recall |
| tools_analysis.rs | 9 | graph, decompose, session, evolve, suggest |
| tools_infra.rs | 22 | wiring, wiring_audit, blast_radius, detect_changes, spawn_worker |
| tools_generator.rs | 23 | submit_plan, validate_plan, render_plan, template_list |
| tools_metadata.rs | 9 | ast_callgraph, ast_meta, search_symbols, query_dsl |
| **TOTAL** | **80** | |

### C. Agent Definition Content Distribution

| Content Type | % do Total | Linhas | Tratamento |
|-------------|-----------|--------|-----------|
| Role-specific (único) | 35% | ~1.190 | MANTER |
| CLI reference (duplicado) | 25% | ~850 | EXTRAIR (já em global rules) |
| VP-Scout chains (duplicado) | 12% | ~410 | EXTRAIR (já em VP-Scout.md) |
| Shared boilerplate | 7% | ~240 | EXTRAIR para base compartilhada |
| Output format + examples | 11% | ~375 | COMPACTAR |
| Hard rules + gates | 10% | ~340 | MANTER (role-specific subset) |

---

*Diagnostic Precision v2.0 — Produced from 4 parallel analysis agents + direct investigation. Evidence-based, no agents spawned for meta-analysis speculation.*
