# Touring Diagnostic Precision Plan — Análise Completa e Plano de Correção

> **Data**: 20/04/2026 | **Sessão**: 891a50fd | **Prioridade**: CRÍTICA

---

## SUMÁRIO EXECUTIVO

A análise profunda revelou **8 root causes** de falsos positivos, imprecisões e gaps operacionais
no sistema Touring. Os problemas vão de FP feedback loops quebrados a RL com overflow catastrófico,
passando por hooks inativos e comandos Git proibidos em agentes oficiais.

**Impacto estimado**: 40-60% dos falsos positivos são evitáveis com as correções descritas.

---

## ROOT CAUSE 1 — D7 FP Feedback Loop QUEBRADO (P0 CRÍTICO)

**Evidência**:
```bash
touring memory recall "fp:task:" -j | jq '.entries[:5]'
# Retorna CAMINHOS DE ARQUIVO, não registros de FP:
# ".claude/rust/crates/touring-antt/src/rlm_integration.rs → edited:Edit:..."
# ".claude/projects/-home-gabrielgadea/memory/..."
```

**Root Cause**: As entradas `fp:task:` na memória semântica são file paths editados,
NÃO registros de falsos positivos. Nenhum FP real foi catalogado. Cada sessão pode repetir
os mesmos FPs indefinidamente.

**Formato correto esperado (mas não existente)**:
```
fp:task:S-1:orphan_symbol_false → "wiring stale: grep found consumer"
fp:task:S-2:compilation_error_false → "cargo check returned 0"
fp:pattern:unwrap_in_tests → "all .unwrap() were in #[test] modules"
```

**Fix**: Ver Frente A abaixo.

---

## ROOT CAUSE 2 — Health Delta Loop INATIVO (P1 CRÍTICO)

**Evidência**:
```bash
touring gate-metrics -j | jq '{
  hd_record: .health_delta_record_count,
  hd_compute: .health_delta_compute_count,
  blast_inject: .blast_inject_count,
  linucb_manual: .linucb_route_manual_count,
  mcts_runs: .mcts_shadow_run_count,
  cache_hits: .query_cache_hit_count
}'
# TODOS retornam 0 — Waves 9-19 hooks não estão disparando em sessões reais
```

**Root Cause**: Os hooks `pre_edit`/`post_edit`/`pre_write`/`post_write` documentados nas
Waves 9-19 (health_delta, LinUCB, MCTS shadow, query cache) podem não estar configurados
em `~/.claude/settings.json` ou o daemon está em modo degraded.

**Fix**: Ver Frente C abaixo.

---

## ROOT CAUSE 3 — Gotcha Stats Corrompidos (P1 ALTO)

**Evidência**:
```bash
touring gotcha stats -j
# total_count: 129
# unresolved_count: 37,966  ← IMPOSSÍVEL (37K > 129 total)
# resolved_count: 0
# total_prevented_errors: 0  ← 52 gotchas com hits mas ZERO prevenções
```

**Root Cause**: Schema mismatch no SQLite — `unresolved_count` conta algo diferente do esperado
(possivelmente `occurrence_count` ao invés de registros únicos). O circuito entre `touring gotcha match`
e `post-tool-failure` hook está quebrado — gotchas nunca fecham o loop de prevenção.

**Fix**: Investigar schema `gotcha_stats` no SQLite; corrigir query de unresolved_count.

---

## ROOT CAUSE 4 — RL TD Error Catastrófico (P1 ALTO)

**Evidência**:
```bash
touring learning status -j | jq '{ema_reward, mean_td_error, agentic_active}'
# mean_td_error: -6809231916146816.0  ← overflow/NaN propagation
# agentic_rl_state.active: false
# update_count: 9
```

**Root Cause**: Overflow numérico na TD error computation. Com apenas 9 updates e um erro de
magnitude 10^15, é provável que algum reward extremo (-1e9 ou similar) foi injetado e propagou
para NaN/overflow. O estado agentic RL está desativado, tornando `touring suggest next` não-confiável.

**Fix**: Reset RL state; investigar qual reward causou overflow; ativar agentic_rl_state.

---

## ROOT CAUSE 5 — Sequential Thinking Nunca Carregado (P1 MÉDIO)

**Evidência**:
```
# Em deferred tools list (requer ToolSearch antes de usar):
mcp__sequential-thinking__sequentialthinking
```

**Root Cause**: O TACO protocol manda usar sequential-thinking entre fases, mas nenhum passo
no protocolo inclui o `ToolSearch(query="select:mcp__sequential-thinking__sequentialthinking")`
necessário para carregar a ferramenta. Ela nunca é realmente utilizada.

**Fix**: Adicionar passo de ToolSearch ao Phase 0 do TACO em TACO-subagent.md e CLAUDE.md.

---

## ROOT CAUSE 6 — Index Coverage Check Não Enforced (P2 MÉDIO)

**Evidência**: Step 0.6 no touring-scouter.md documenta o check mas não há GATE que bloqueie
o scouting se o crate não estiver indexado. Em sessões reais, `touring index find` retorna
empty para crates não-indexados, gerando "símbolo não existe" como falso positivo.

**Fix**: Adicionar GATE explícito: se `touring index find <known_symbol_from_crate>` retorna
vazio → rebuild index ANTES de continuar.

---

## ROOT CAUSE 7 — VP-Scout Chain 7 Não Universal (P1 MÉDIO)

**Evidência**:
```bash
touring wiring status -j | jq '{orphan_count, total_pub_symbols}'
# orphan_count: 203,709
# total_pub_symbols: 63,961
# RATIO: 3.18x — matematicamente IMPOSSÍVEL se orphan = pub symbol sem consumer
```

**Root Cause**: A definição de "orphan" no wiring DB é diferente do esperado — pode incluir
referências cruzadas, versões antigas, ou symbols de arquivos deletados. A Chain 7 (Wiring
Cache Staleness) do VP-Scout existe mas NÃO é aplicada universalmente para todas as claims
de orphan. Agentes aceitam o valor de 203K sem questionar.

**Fix**: Adicionar Chain 7 como MANDATORY na tabela de chains do scouter.md; documentar
o modelo de contagem do wiring DB.

---

## ROOT CAUSE 8 — Touring-Scriber Usa Comandos Git (P0 BLOQUEADOR PROTOCOLAR)

**Evidência**: `touring-scriber.md` Phase 1 contém:
```bash
git diff --stat HEAD~1 2>/dev/null
git log --oneline -10 2>/dev/null
```
E Phase 4 contém:
```bash
git diff --name-only 2>/dev/null >> session_report.md
```

**Root Cause**: Violação direta do CLAUDE.md Hard Rule #11 (PROIBIÇÃO TOTAL DE GIT).
Git é absolutamente proibido — Gabriel gerencia git manualmente.
Evidência histórica: git stash destruiu 162 módulos em 06/04/2026 causando 6h de retrabalho.

**Fix**: Substituir todos os `git diff/log` por equivalentes Touring CLI.

---

## PLANO DE CORREÇÃO — 7 FRENTES

### FRENTE A — Reparar D7 FP Loop (P0, hoje)

**Ação**: Catalogar FPs históricos com chave correta; atualizar documentação do formato de chave.

**Chave formato correto**:
- `fp:task:<task_id>:<short_name>` — FP específico de task
- `fp:pattern:<pattern_name>` — Padrão de FP recorrente
- `fp:file:<file_basename>:<reason>` — FP associado a arquivo

**Catálogo inicial** (FPs históricos conhecidos):
```bash
touring memory store "fp:pattern:orphan_wiring_stale" "Wiring DB pode ter staleness. Sempre confirmar via grep antes de classificar como orphan real" --tier semantic --type lesson
touring memory store "fp:pattern:plan_doc_as_state" "Plan docs descrevem INTENÇÃO, não estado atual. Sempre executar cargo check" --tier semantic --type lesson
touring memory store "fp:pattern:homonymia_aco" "ACO em touring-simd ≠ ACO em touring-hooks. São sistemas independentes" --tier semantic --type lesson
touring memory store "fp:pattern:compilation_inference" "NUNCA inferir erros de compilação de plan docs. Sempre cargo check --workspace" --tier semantic --type lesson
touring memory store "fp:pattern:feature_consumer_check" "Feature opcional pode já estar ativada por consumer. Verificar Cargo.toml do consumer" --tier semantic --type lesson
```

### FRENTE B — Gates Obrigatórios de Evidência CLI (P0, hoje)

**B1**: Adicionar COMPILATION EVIDENCE GATE em scouter + auditor
**B2**: Adicionar WIRING STALENESS GATE (Chain 7 universal) em scouter
**B3**: Adicionar PLAN DOC STALENESS CHECK (Chain 6) enforcement
**B4**: Enforçar INDEX COVERAGE CHECK como GATE (não apenas documentado)

### FRENTE C — Reparar Sistema RL (P1, esta semana)

**C1**: Investigar e resetar RL state (overflow TD error)
**C2**: Verificar configuração de hooks em `~/.claude/settings.json`
**C3**: Ativar `agentic_rl_state` se disponível
**C4**: Identificar reward que causou overflow numérico

### FRENTE D — Aprofundar Exploração Diagnóstica (P1, esta semana)

**D1**: Adicionar `touring tantivy fuzzy` para descoberta de símbolos próximos
**D2**: Adicionar `touring health-delta status <file>` em pre-edit de todos os agentes
**D3**: Adicionar `touring gate-metrics -j` no pre-flight de todos os agentes
**D4**: Usar `touring ast rust-semantic` para Rust antes de qualquer edit

### FRENTE E — Atualizar Arquivos de Agentes (P1, esta semana)

**E1**: touring-scouter.md → Chain 7 mandatory; INDEX FRESHNESS GATE; FP format fix
**E2**: touring-scriber.md → Remover git commands (git diff/log → touring equivalents)
**E3**: touring-engineer.md → Adicionar gotcha gate obrigatório pré-edit
**E4**: touring-auditor.md → Adicionar Cadeia 7 no pre-implementation audit
**E5**: _shared-touring-base.md → Fix D7 FP key format documentation

### FRENTE F — Mecanismos de Auto-Correção (P2, próxima semana)

**F1**: Adicionar ToolSearch para sequential-thinking no Phase 0 do TACO
**F2**: Implementar Diagnostic Confidence Score field em todos os outputs de agentes
**F3**: Adicionar self-check checklist de FPs antes de retornar findings

### FRENTE G — Monitoramento Contínuo de Precisão (P2, permanente)

**G1**: Dashboard semanal de gate-metrics (contadores de blast/linucb/mcts)
**G2**: Análise de evolução drift semanal (`touring evolution drift -j`)
**G3**: Relatório de FPs evitados por sessão (campo `false_positives_avoided` obrigatório)

---

## STATUS DE EXECUÇÃO

| Frente | Status | Prioridade |
|--------|--------|-----------|
| A — FP Loop Repair | ⬜ Pendente | P0 |
| B — Evidence Gates | ⬜ Pendente | P0 |
| C — RL System Repair | ⬜ Pendente | P1 |
| D — Deeper Diagnostics | ⬜ Pendente | P1 |
| E — Agent File Updates | ⬜ Pendente | P1 |
| F — Auto-Correction | ⬜ Pendente | P2 |
| G — Continuous Monitoring | ⬜ Pendente | P2 |

---

*Gerado em: 20/04/2026 | Sessão: 891a50fd | Autor: TACO v6.2*
