---
type: PhaseReport
title: S5bc — phase report
description: KPI gate-fatigue nas 2 pontas. ESTRUTURAL: GateFatigue computado no capture() do snapshot (ratio=bypassed/(denied+bypassed) por gate e globa
plan_id: 2026-08-26-code-mode-afordancia-deep
tags: [loop, phase, S5bc]
timestamp: 2026-08-26T17:07:47.297530-03:00
okf_version: "0.1"
---

# S5bc — phase report

**Status**: done

Part of the [log](/log.md).

## Summary

KPI gate-fatigue nas 2 pontas. ESTRUTURAL: GateFatigue computado no capture() do snapshot (ratio=bypassed/(denied+bypassed) por gate e global; None sem volume — Lei L2; fp_candidate=ratio>0.20 com volume>=10 — o KPI que a revisao S5c consome). Teste baseline-delta+autoconsistencia. MINERADOR: N5 conta TOURING_GATE_OK=1 por sessao/dia (24 usos no corpus). Baseline do eixo injecao RECALCULADO com o classificador S1-completo (cd/segmentos): 24/08 119->251 followed — o numero antigo subcontava; documentado no relatorio. INCIDENTE de edicao no meio: minha insercao roubou derive+doc do GateMetricsSnapshot (anchor colado) — pego pelo deny(missing_docs); corrigido e guardado pela suite. foundation 486 passed; clippy limpo; pytest 7/7.

## Afirmações sem endereço

_Nenhum fato com run_id foi citado neste fechamento (`--facts`). As afirmações do Summary acima valem o que vale a narrativa — um probe (FACT=) daria endereço a cada uma. (claim-ledger, S-5.5)_

## Knowledge

Typed abstract: [/knowledge/S5bc.json](/knowledge/S5bc.json).
