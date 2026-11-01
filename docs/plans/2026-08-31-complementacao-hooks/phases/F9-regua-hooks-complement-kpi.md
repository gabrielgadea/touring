---
type: PhaseReport
title: "F9-regua-hooks-complement-kpi — phase report"
description: "hooks_complement em touring kpi -j (kpi.rs): hook_dispatch_by_name (daemon, F0.3d) desde hook_dispatch_since_epoch (novo em gate_metrics) x "
plan_id: 2026-08-31-complementacao-hooks
okf_version: 0.1
tags: [loop, phase, F9-regua-hooks-complement-kpi]
timestamp: 2026-09-01T22:08:31.675511-03:00
---

# F9-regua-hooks-complement-kpi — phase report

**Status**: done

Part of the [log](/log.md).

## Summary

hooks_complement em touring kpi -j (kpi.rs): hook_dispatch_by_name (daemon, F0.3d) desde hook_dispatch_since_epoch (novo em gate_metrics) x entregas canonicas ao mirror na mesma janela (ts>=epoch; HookName::ALL; alias contados a parte) -> post_bash_dispatched, mirror_deliveries_since, post_bash_delivery_ratio (null sem despacho; available:false antes do 1o despacho). E a regua que a sonda F0.3 nao tinha. 3 testes puros (hooks_complement_from).

## Afirmações sem endereço

_Nenhum fato com run_id foi citado neste fechamento (`--facts`). As afirmações do Summary acima valem o que vale a narrativa — um probe (FACT=) daria endereço a cada uma. (claim-ledger, S-5.5)_

## Knowledge

Typed abstract: [/knowledge/F9-regua-hooks-complement-kpi.json](/knowledge/F9-regua-hooks-complement-kpi.json).
