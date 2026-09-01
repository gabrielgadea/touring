---
type: Strategy
title: "Strategy v0.1 — complementacao-hooks-touring (reconhecimento de nova frente)"
description: "Yetzirah nível 0 — reconhece a criação como projeto próprio (separado de code-mode-sinal), confirma os 15 sinais reativos endereçáveis, declara arquitetura (handlers novos em settings.json + subprocess fire-and-forget). Pronto para F1 (especificação handler-by-handler) quando Gabriel aprovar."
plan_id: 2026-08-31-complementacao-hooks
bundle: docs/plans/2026-08-31-complementacao-hooks
okf_version: "0.1"
version: "0.1"
based_on: ["analise-comparativa-57-vs-hooks.md", "criacao.md"]
tags: [strategy, strategy-loop, complementacao-hooks, settings-json, subprocess, registry]
timestamp: "2026-08-31T18:50:00-03:00"
---

# Strategy v0.1 — complementacao-hooks-touring

## TL;DR

**Nova frente de trabalho** (reconhecida por Gabriel 31/08 18:50, em paralelo à code-mode-sinal). Catalogação empírica da análise comparativa 57-vs-hooks revelou **15 sinais reativos** que merecem virar **handlers novos em `settings.json`** para os eventos que JÁ existem (PreToolUse/PostToolUse/SessionStart).

Esta strategy v0.1 é o **reconhecimento** — define o que é o projeto, escopo, arquitetura. F1 (especificação handler-by-handler) e F2-F4 (execução) virão a seguir com ordem explícita de Gabriel.

## Reconhecimento da criação

- **Origem empírica**: análise comparativa 57-vs-hooks (F3.0 final, 15:35 BRT)
- **Gap mensurável**: 41 sinais não chegam ao code mode; **15 são reativos**
- **Complementaridade**: os 27 sob-demanda ficam no projeto irmão `code-mode-sinal` (F2.1 SDK tipada)
- **Bundle próprio**: `docs/plans/2026-08-31-complementacao-hooks/`
- **DAG nova**: a ser criada na sequência (task_178820XXXXXXXX)
- **Status atual**: **RECONHECIMENTO** (Yetzirah v0.1)

## Os 15 sinais reativos (matriz handler × evento)

| # | Sinal | Evento | Matcher | Bridge/Crate de origem | Latência budget |
|---|---|---|---|---|---|
| 1 | `touring.quality` (delta) | PostToolUse | `Edit\|Write` | `touring-quality` (50-dim scorer) | <100ms |
| 2 | `touring.symbols` | PreToolUse | `Read` | `touring-cli::ast_overview` | <50ms |
| 3 | `touring.dependents` | PreToolUse | `Edit` | `touring-cli::wiring_impact` | <50ms |
| 4 | `touring.pub_api_diff` | PostToolUse | `Edit\|Write` | `touring-generator::pub_api_diff` | <80ms |
| 5 | `touring.gotchas` | PreToolUse | `Edit` | `touring-cli::gotcha_match` | <30ms |
| 6 | `touring.scan_vulnerabilities` | PostToolUse | `Write` | `touring-offensive` (CWE taxonomy) | <100ms |
| 7 | `touring.code_mode_status` | SessionStart | `*` | `touring-foundation::code_mode` | <30ms |
| 8 | `touring.wiring_orphans` | PostToolUse | `Edit\|Write` | `touring-cli::wiring_orphans` (REGRA #0) | <50ms |
| 9 | `touring.wiring_impact` | PreToolUse | `Edit` | `touring-cli::wiring_impact` | <50ms |
| 10 | `touring.gotcha_match` | PreToolUse | `Edit` | `touring-cli::gotcha_match` (alias) | <30ms |
| 11 | `touring.find_references` | PostToolUse | `Edit\|Write` | `touring-cli::find_references` | <80ms |
| 12 | `touring.entity_id` | PostToolUse | `Write` | `touring-identity::EntityId` (REGRA #17) | <30ms |
| 13 | `touring.audit_unsafe` | PostToolUse | `Edit\|Write` | `touring-analysis::quality::RustQualitySignals` | <50ms |
| 14 | `touring.temporal_drift` | SessionStart | `*` | `touring-intelligence::drift` | <50ms |
| 15 | `touring.evolution_status` | SessionStart | `*` | `touring-cli::evolution` | <30ms |

## Arquitetura (P4 — preservar shim)

**Decisão arquitetural chave**: NÃO criar touring-hook subcmds novos. O shim v11.0 (`~/.claude/hooks/touring-hook`) está congelado em camadas per-project (Pln2 F1). Os 15 handlers novos vão DIRETO em `~/.claude/settings.json` como entradas adicionais nos arrays `.hooks.PreToolUse` / `.hooks.PostToolUse` / `.hooks.SessionStart` — nunca substituindo entradas existentes.

```
PreToolUse:   [19 atual] + [3 novos] = 22 total
PostToolUse:  [25 atual] + [7 novos] = 32 total  (alguns Edit|Write matcher novo)
SessionStart: [9 atual]  + [3 novos] = 12 total
```

## Padrão de invocação (p4)

Cada handler novo segue o padrão `arch:generator-hooks-integration:pattern`:
- **Subprocess fire-and-forget** (nunca importar módulo Rust direto)
- **JSON canônico tipado** no `additionalContext`
- **Latência budget <100ms p95** medido via `touring gate-metrics`
- **Fail-open em erro de parse** (consistência com hooks existentes)

## F1-F4 (a executar com ordem Gabriel)

| Fase | Descrição | Artefato |
|---|---|---|
| **F1** | Especificação handler-by-handler | `docs/complementacao-hooks/specs/H1-H15.md` (15 specs, 1 por handler) |
| **F2** | Validação empírica das latências budget | `docs/complementacao-hooks/benchmarks/handler-latency.json` |
| **F3** | Implementação + registro em settings.json | patch no `~/.claude/settings.json` + 15 handlers |
| **F4** | Testes + BestPracticesGate Warn | `scripts/test_complementacao_hooks.py` + KPI counter |

## Critérios de pronto (4 AND — replicado de Briah)

1. `python3 scripts/test_complementacao_hooks.py --verbose` exit 0 com **N=15 testes verdes**
2. `jq '.hooks.PostToolUse | length'` = **32**, `jq '.hooks.PreToolUse | length'` = **22**, `jq '.hooks.SessionStart | length'` = **12**
3. `touring kpi -j hooks_complement.composite ≥ 0.80` ≥ 7 dias
4. `touring gate-metrics -j hook_latency_p95` < **100ms** por handler

## Anti-goals firmes (replicado de Briah)

- ❌ NÃO criar touring-hook subcmd novo (shim v11.0 congelado)
- ❌ NÃO duplicar handlers (1 handler por sinal por evento)
- ❌ NÃO acoplar via Rust import direto (sempre subprocess)
- ❌ NÃO mexer em handlers existentes (só adicionar)
- ❌ NÃO tocar SDK do code mode (projeto irmão F2-F6)
- ❌ NÃO tornar opt-in (esteira, não puxadinho)

## Status & next-action

- ✅ Bundle criado
- ✅ Briah `criacao.md` (ratio 1.0)
- ✅ Strategy v0.1 (este doc — reconhecimento)
- ⏳ DAG `task_<novo>` — pendente Gabriel
- ⏳ F1-F4 — aguardam ordem explícita

**Próximo passo (gate humano)**: Gabriel aprovar F1 (especificação handler-by-handler) ou propor alternativa.

---

_v0.1 — 2026-08-31 18:50 BRT | Strategy de reconhecimento da nova frente complementacao-hooks | 15 sinais reativos endereçáveis | 4 fases pendentes | aguarda ordem Gabriel_