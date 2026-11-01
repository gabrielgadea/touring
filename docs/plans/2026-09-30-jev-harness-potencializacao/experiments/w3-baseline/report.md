---
type: ExperimentReport
title: W3 — baseline de prova de vida das métricas de decisão
description: Estado vivo das 6 réguas da família decision (S-3.1) contra os journals reais em 01/10. Três réguas jev_* medem dado real; as três de intent/recall respondem "unmeasured" honesto até a propagação do emissor (S-3.2) e do contador (S-3.4).
plan_id: 2026-09-30-jev-harness-potencializacao
tags: [w3, baseline, kpi, decision, jev]
timestamp: 2026-10-01T02:40:00-03:00
okf_version: "0.1"
---

# W3 — prova de vida das métricas de decisão (S-3.3)

Parte do [bundle](/index.md). Gate anti-`instrumento-que-mente`: cada régua deve
estar viva com dado real OU declarar `unmeasured` com razão — nunca um zero
disfarçado. **[F] = medido em 01/10.**

## 1. Fontes

| Journal | Estado em 01/10 | Linhas |
|---|---|---|
| `~/.claude/touring/logs/jev_journal.jsonl` (adapter, S-0.2) | **vivo** — chamadas reais da W1 (100), do G2 (238) e dos probes | 243+ [F] |
| `~/.claude/touring/logs/intent_journal.jsonl` (emissor, S-3.2) | **nasceu limpo** após o expurgo da poluição de testes (~80 linhas removidas; isolamento `cfg(test)` aplicado). Dados de produção só após `update-touring` | 0 [F] |
| `~/.claude/touring/logs/recall_journal.jsonl` (contador, S-3.4) | **não existe** — emissor em implementação | 0 [F] |

## 2. As 6 réguas (família `decision`, `crates/touring-cli/src/cli/kpi/decision.rs`)

| Régua | Estado | Evidência |
|---|---|---|
| `jev_calls_total` | **MEDIDO** | 183 chamadas api + 6 cache na janela (prova da touring-02 contra o journal vivo) [F] |
| `jev_latency_p99` | **MEDIDO** | p50 367,7 ms · max 2.148,4 ms [F] |
| `jev_confidence_hist` | **MEDIDO** | 83 confianças do `answers_digest`, histograma em 9 faixas [F] |
| `intent_distribution` | `unmeasured` | journal nasceu limpo; emissor aguarda propagação (ação de estado — Gabriel) [F] |
| `keyword_fallback_rate` | `unmeasured` | idem — e a flag Jev ainda não existe (W4.4) [F] |
| `recall_curated_share` | `unmeasured` | `{"status":"unmeasured","reason":"no_recalls"}` até S-3.4 entregar [F] |

## 3. Veredito do gate

**PASSA**: as 3 réguas que TÊM fonte de dados medem dado real com evidência
publicada; as 3 que não têm declaram `unmeasured` com a razão exata e o
pré-requisito nomeado. Nenhuma finge zero. O binário vivo ainda não expõe a
família (`touring kpi -j` mostra 12 famílias sem `decision`) — a virada é a
propagação, decisão de estado do Gabriel. **[F]**
