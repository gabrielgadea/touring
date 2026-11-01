---
type: LoopBundle
title: Ligar, não construir — R1-R6
description: Bundle OKF da wave que fecha os furos medidos do ciclo de aprendizado ligando estruturas já compiladas.
plan_id: 2026-08-29-ligar-nao-construir
tags: [loop, rl, learning, kpi]
timestamp: 2026-08-29T14:20:00-03:00
okf_version: "0.1"
---

# Bundle — Ligar, não construir (R1-R6)

DAG: `task_1788020982583817162` · Origem: auditoria da inteligência 29/08
(`assessment:ciclo-aprendizado-touring:2026-08-29`), recomendações aprovadas
verbatim por Gabriel via `/loop-engineering`.

| Documento | Conteúdo |
|---|---|
| [strategy-2026-08-29-ligar-nao-construir.md](strategy-2026-08-29-ligar-nao-construir.md) | As 6 fases, critérios de aceite, fontes externas |
| [log.md](log.md) | Histórico cronológico |
| phases/p1..p6 | Relatórios OKF por fase (`loop_phase_close`) |
| knowledge/p1..p6 | Abstracts tipados (Hyper-Extract) |
| diagnostics/ | Diagnóstico OUTER (`loop_diagnose`) |

Ledger CCE: `.touring-explore/ligar-nao-construir-r1-r6-aprendizado.ledger.json`
(converged; lente external herdada do bundle autoresearch).

Consome/alimenta: [/docs/plans/2026-08-25-autoresearch-rl-intelligence/](../2026-08-25-autoresearch-rl-intelligence/RETOMAR-AQUI.md)
— o P4 de lá parte do gatilho `criterio:p4-dspy-gatilho-familia:2026-08-29`.
