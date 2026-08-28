---
type: PhaseReport
title: F2: KPIs honestos — router_accuracy e plan_refine_iters reais ou advisory declarado — phase report
description: F2 fechada: os 2 STUBs eram medidores reais com fonte vazia — remédio foi EXERCITAR a infra: (1) factory start real (ticket chore com verify
plan_id: 2026-08-28-adw-potencializacao
tags: [loop, phase, F2: KPIs honestos — router_accuracy e plan_refine_iters reais ou advisory declarado]
timestamp: 2026-08-28T09:03:16.919023-03:00
okf_version: "0.1"
---

# F2: KPIs honestos — router_accuracy e plan_refine_iters reais ou advisory declarado — phase report

**Status**: done

Part of the [log](/log.md).

## Summary

F2 fechada: os 2 STUBs eram medidores reais com fonte vazia — remédio foi EXERCITAR a infra: (1) factory start real (ticket chore com verify_cmd real; o gate recusou verify fake — L2 viva) → router_accuracy STUB→1.0 PASS vivo, runs 45→46; (2) plan_refine.py real sobre o strategy doc + ledger CCE → .refine.json no disco, que revelou BUG produtor≠consumidor: plan_refine grava {version,iterations}, o KPI só aceitava array cru — corrigido no kpi.rs aceitando ambas as formas + teste que espelha o formato do produtor (34/34). Vivo do plan_refine_iters vira PASS no próximo deploy (o dispatch roda no daemon instalado). ZTE fica para F6 como planejado.

## Afirmações sem endereço

_Nenhum fato com run_id foi citado neste fechamento (`--facts`). As afirmações do Summary acima valem o que vale a narrativa — um probe (FACT=) daria endereço a cada uma. (claim-ledger, S-5.5)_

## Knowledge

Typed abstract: [/knowledge/F2: KPIs honestos — router_accuracy e plan_refine_iters reais ou advisory declarado.json](/knowledge/F2: KPIs honestos — router_accuracy e plan_refine_iters reais ou advisory declarado.json).
