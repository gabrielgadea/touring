---
type: PhaseReport
title: "B4-call-graph-compacto-layer — phase report"
description: "B4 re-escopado como S10 entregue: callgraph dos hooks destravado para TS/JS — pre_edit.callgraph_signal_for_file e pre_write.callgraph_signa"
plan_id: 2026-08-31-complementacao-hooks
okf_version: 0.1
tags: [loop, phase, B4-call-graph-compacto-layer]
timestamp: 2026-09-01T23:00:34.632237-03:00
---

# B4-call-graph-compacto-layer — phase report

**Status**: done

Part of the [log](/log.md).

## Summary

B4 re-escopado como S10 entregue: callgraph dos hooks destravado para TS/JS — pre_edit.callgraph_signal_for_file e pre_write.callgraph_signal_for_write gateiam por touring_code::ast::call_graph::supports_call_graph (predicado unico ao lado do dispatch, debug_assert de concordancia) em vez da lista local .rs/.py; compactacao: enrich_with_callgraph agora devolve callers/callees DISTINTOS (main chamando helper 6x lia como HOTSPOT de 6 callers e 'callers: [main, main, main, main, main (+1 more)]').

## Afirmações sem endereço

_Nenhum fato com run_id foi citado neste fechamento (`--facts`). As afirmações do Summary acima valem o que vale a narrativa — um probe (FACT=) daria endereço a cada uma. (claim-ledger, S-5.5)_

## Knowledge

Typed abstract: [/knowledge/B4-call-graph-compacto-layer.json](/knowledge/B4-call-graph-compacto-layer.json).
