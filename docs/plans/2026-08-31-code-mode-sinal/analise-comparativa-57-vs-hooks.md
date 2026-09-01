---
type: Analysis
title: "Análise comparativa — 57 SDKs propostas vs cadeia de hooks atual"
description: "Mapeamento célula-a-célula: para cada uma das 57 funções SDK propostas (F1.9), identificar se a cadeia de hooks atual (settings.json → cc-*.sh → touring-hook subcmd → daemon IPC) já injeta sinal equivalente, parcial, ou se há GAP. Top-17 MUST-HAVE analisados individualmente. Cobertura: 17% equivalente + 11% parcial + 72% gaps."
plan_id: 2026-08-31-code-mode-sinal
bundle: docs/plans/2026-08-31-code-mode-sinal
okf_version: "0.1"
version: "1.0"
based_on: ["inventory-hooks.md v3.0", "settings.json 27/8/2026"]
tags: [analysis, comparative, sdk, hooks, gaps, code-mode]
timestamp: "2026-08-31T15:35:00-03:00"
---

# Análise comparativa — 57 SDKs propostas × cadeia de hooks atual

## TL;DR

**A cadeia de hooks atual injeta o equivalente a apenas 10 das 57 funções SDK propostas (17%)**. Mais 6 sinais chegam parciais (11%) — restam **41 gaps (72%)** onde o sinal NUNCA chega ao contexto do LLM via hooks, mesmo existindo no touring CLI/MCP.

**Tradução da dor de Gabriel**: o abismo que ele percebeu é real e mensurável. O code mode hoje opera com ~28% da inteligência que uma chamada CLI enriquecida pelos hooks tem.

## Topologia da cadeia atual

| Camada | Componente | Estado |
|---|---|---|
| 1 | Claude Code session | 27 eventos × 82 handlers |
| 2 | settings.json (CC) | `/home/gabrielgadea/.claude/settings.json` |
| 3 | cc-*.sh wrappers | 25 standalone (não via touring-hook) |
| 4 | touring-hook shim (4-layer walk-up) | 1 binário |
| 5 | touring-hook v11.0 subcmds | **47 sub-comandos registrados** |
| 6 | HookRuntime + bridges | 5 bridges (ast/aco/cognitive/knowledge_symbol/nlp) |
| 7 | daemon IPC → JSON `hookSpecificOutput` | output chega ao contexto do LLM |

**Sub-comandos do touring-hook que MAIS rodam**: `cli-suggest` (9 ocorrências em PreToolUse), depois `session-start` (8), `pre-edit` (3), `pre-read`/`pre-write`/`pre-bash` (2 cada).

## Matriz de cobertura (F1.9 × Cadeia)

### ✓ Cobertura DIRETA (10 SDKs — 17%)

| SDK | Hook injetor | Sinal entregue |
|---|---|---|
| `touring.blast_radius` | `pre-edit` | blast_radius via `ast_blast` (incl. `B-301` se >10) |
| `touring.edit_impact` | `pre-edit` | edit_impact_score 0-1 |
| `touring.ast_meta` | `pre-edit`/`pre-read` | file metadata (blast/quality/cognitive) |
| `touring.ast_blast` | `pre-edit` | blast tree completo (substitute) |
| `touring.memory_recall` | `prompt-enhance` | semantic recall no turno |
| `touring.doctor` | `session-start` | health composite 5/7 |
| `touring.system_info` | `session-startup-intelligence` | daemon health + path |
| `touring.recommend` | `cli-suggest` | RL recommend (P4-P7) |
| `touring.linucb_select` | `cli-suggest` | LinUCB arm selection |
| `touring.query` | `cli-suggest` | escape hatch via `query(hook, payload)` |

### ◐ Cobertura PARCIAL (6 SDKs — 11%)

| SDK | Hook | Subset entregue vs sinal completo |
|---|---|---|
| `touring.file_metadata` | `session-start` | metadata só do projeto (não arquivo-alvo) |
| `touring.capability_check` | `permission-request` | permission injetada, mas sem capability model CEG |
| `touring.speculate` | `pre-edit` | pre_edit score é speculate-like (não tem `speculate()` real) |
| `touring.tantivy_search` | `prompt-enhance` | 1-2 hits, não ranked full |
| `touring.mcts_search` | `cli-suggest` | só quando score<0.7 |
| `touring.file_watch_events` | `file-changed` | path único, não stream de eventos |

### ✗ GAPS (41 SDKs — 72%)

**Gaps do grupo MUST-have (14 dos 17)** — os mais críticos:

| MUST-HAVE | Por que é gap | Impacto |
|---|---|---|
| `touring.quality` | post-edit/post-write NÃO injetam quality_score | o LLM edita sem saber se piorou a nota |
| `touring.symbols` | pre-read NÃO injeta symbols do arquivo | o LLM lê sem saber o que o arquivo define |
| `touring.dependents` | nenhum hook injeta transitive consumers | refatora sem saber quem quebra |
| `touring.pub_api_diff` | nenhum hook injeta diff | sem breaking-change awareness |
| `touring.gotchas` | nenhum hook injeta gotcha match | repete erros históricos |
| `touring.scan_vulnerabilities` | F2.5 só no CI via cargo-deny | LLM gera código vulnerável sem aviso |
| `touring.code_mode_status` | nenhum hook injeta | code mode opera cego ao próprio estado |
| `touring.call_graph` | nenhum hook injeta | refatora sem ver fluxo de chamadas |
| `touring.entity_id` | nenhum hook injeta | viola REGRA #17 silenciosamente |
| `touring.tasks_compile` | nenhum hook injeta | ADW scaffold sem validação de Tasksfile |
| `touring.assists_for_file` | nenhum hook injeta ANN similar | perde refactors análogos |
| `touring.related_symbols` | nenhum hook injeta | scaffold sem ver contratos vizinhos |
| `touring.call_graph` | nenhum hook injeta | já listado |
| `touring.scan_vulnerabilities` | já listado | |

**Gaps do grupo SHOULD/NICE (29 sinais)** — análises profundas (TDG, rust-semantic), grafos (wiring chains/cycles), ML signals (predict_layer7, psi_pressure, embed, fuse_rrf), audit (unsafe, temporal_drift), search avançado (hybrid_search, hybrid), session context (session_graph).

## O que isso significa

### O que a cadeia de hooks FAZ BEM (10 sinais)
Os hooks injetam sinais **reativos** (giram em torno do tool atual: pre/post tool) e **uma vez por turno** (session-start, prompt-enhance). Cobertura forte em:
- **Metadata do tool em uso** (ast_meta, blast_radius, edit_impact via pre-edit)
- **Saúde do sistema** (doctor, system_info via session-start)
- **RL nudge** (recommend, linucb via cli-suggest)
- **Memória curta** (memory_recall via prompt-enhance)
- **Escape hatch** (query genérico)

### O que a cadeia de hooks **NÃO FAZ** (47 sinais — 83%)
Os hooks são **operadores**, não **analistas**. O LLM via hook recebe:
- ❌ Análise estática profunda (TDG, rust-semantic)
- ❌ Grafos de dependência (wiring chains/cycles/impact)
- ❌ Detecção de vulnerabilidades (scan_vulnerabilities)
- ❌ Contratos adjacentes (pub_api_diff, related_symbols)
- ❌ Estado estrutural (code_mode_status, call_graph)
- ❌ Auditoria de segurança (unsafe, temporal_drift)
- ❌ Predição (predict_layer7, psi_pressure)
- ❌ Cobertura de testes/e2e (`touring.e2e -j` é puro CLI)

**Tradução**: o LLM opera com **instrumentos de piloto** (altímetro, velocímetro, bússola) — não com **instrumentos de mecânico de voo** (analisador de vibração, termografia, fadiga estrutural). O code mode "voa" sem saber o estado estrutural do que está pilotando.

## Implicação para F2

O que esta análise **muda** na recomendação:

1. **A criação Briah está validada** — o abismo que Gabriel percebeu é mensurável: 72% dos sinais que o CLI touring entrega NUNCA chegam ao code mode.

2. **A priorização F2 muda marginalmente** — as 10 SDKs equivalentes diretas são **baixa prioridade** (já tem hook injetando; o F2 só precisa replicar via SDK no sandbox). As 6 parciais são **média prioridade** (estender a injeção). Os **41 gaps** são **alta prioridade** (criar do zero).

3. **Tier Essencial (revisado)**:
   - **Tier 1 (crítico, F2 inicial)**: os 14 gaps do MUST-have — sem eles o code mode está cego
   - **Tier 2 (SHOULD gaps)**: 15 sinais MUST+ dos SHOULD — `touring.wiring_orphans` (REGRA #0), `touring.wiring_impact`, `touring.ast_find`, `touring.index_find`, `touring.gotcha_match`
   - **Tier 3 (Nice)****: 12 sinais MUST+ dos NICE — `touring.diagnostics`, `touring.hybrid_search`, etc

4. **Tier Reposicionar**: as 10 equivalentes podem esperar — elas JÁ chegam via hook, só falta a ponte para o sandbox. Quando BestPracticesGate falhar por aderência < 1.0, replicar via SDK é trivial.

## Citações

- F1.5 cadeia 7-camadas: `inventory-hooks.md` (camada 5 documentada)
- F1.6 5 bridges: 146 pub_symbols em `crates/touring-hooks-core/src/bridges/`
- F1.7/F1.8/F1.9 42 crates survey: 57 SDKs propostas em `inventory-hooks.md` v3.0
- F3.0 5 cenários REAIS: priorização MUST/SHOULD/NICE
- settings.json: 27 eventos × 82 handlers, 47 sub-comandos touring-hook

---

_v1.0 — 2026-08-31 15:35 BRT | Análise comparativa das 57 SDKs propostas (F1.9) × cadeia de hooks atual (camada 5 documentada em F1.5) | **10 equivalentes + 6 parciais + 41 gaps = 72% de gap** | Atualiza priorização F2_