---
type: LoopBundle
title: Reparo do subsistema de memória
description: Bundle OKF do programa que conserta list/reindex/gotcha_stats/erro-engolido e adiciona filtro de ruído + KPI de memória.
plan_id: 2026-08-02-memory-subsystem-repair
tags: [memory, daemon, kpi, recall]
timestamp: 2026-08-02T15:00:00-03:00
okf_version: "0.1"
---

# Bundle — reparo do subsistema de memória

| Documento | Tipo | Conteúdo |
| --- | --- | --- |
| [strategy-2026-08-02-memory-subsystem-repair.md](/strategy-2026-08-02-memory-subsystem-repair.md) | Strategy | 4 causas-raiz em código, 2 features, plano de 6 fases, gate humano |
| [diagnostics/touring-20260802T145651.md](/diagnostics/touring-20260802T145651.md) | Diagnostic | baseline medido no início do programa |
| [phases/P1.md](/phases/P1.md) | PhaseReport | D4 — o CLI deixa de engolir a mensagem do daemon |
| [phases/P2.md](/phases/P2.md) | PhaseReport | D1 — `list` passa a ler `memory_entries` |
| [phases/P3.md](/phases/P3.md) | PhaseReport | D3 — campos de `GotchaStats` renomeados para o que medem |
| [phases/P4.md](/phases/P4.md) | PhaseReport | D2 — reindex incremental e orçado |
| [phases/P5.md](/phases/P5.md) | PhaseReport | F1 — `outcome:*` fora do corpus de recall |
| [phases/P6.md](/phases/P6.md) | PhaseReport | F2 — família KPI `touring.memory.*` (+ D5 `--sort`) |
| [log.md](/log.md) | Log | histórico cronológico |

Estado: **implementado e verificado em teste; deploy pendente de gate humano.**

O gate humano do step 9 foi aprovado (Gabriel: "corrija absolutamente tudo",
com as duas escolhas de design registradas — D3 "renomear para o que medem",
F1 "excluir por padrão, flag para incluir"). As 6 fases estão fechadas e o DAG
`task_1785693784638907875` está drenado.

Evidência de convergência em teste:

| Gate | Resultado |
| --- | --- |
| Suíte completa do workspace | 15101 passed, **0 failed** (exit 0) |
| `clippy --workspace --all-targets -D warnings` | limpo (MSRV honesto em 1.95) |
| `touring doctor` | 6/6 ok |

**Ainda não deployado**: `target/release/` é anterior às correções, então os
quatro comandos reparados seguem verificados apenas por teste, não em runtime.
O deploy (`update-touring` — rebuild + restart do daemon global) reinicia o
daemon compartilhado por todas as sessões CC abertas e por isso é um segundo
gate humano, separado do step 9.
