---
type: PhaseReport
title: "S0-signal-context-v2 — phase report"
description: "SignalContext v2 (touring-hooks-shared/signal_layer.rs): ProposedChange {Write{content} | Edit{old_string,new_string}} + campos tool_name/pr"
plan_id: 2026-08-31-complementacao-hooks
okf_version: 0.1
tags: [loop, phase, S0-signal-context-v2]
timestamp: 2026-09-01T22:05:48.341725-03:00
---

# S0-signal-context-v2 — phase report

**Status**: done

Part of the [log](/log.md).

## Summary

SignalContext v2 (touring-hooks-shared/signal_layer.rs): ProposedChange {Write{content} | Edit{old_string,new_string}} + campos tool_name/proposed + with_tool_name/with_proposed/analysable_text (proposta vence source; vazio cai em source). Call sites reescritos via helpers unicos context_for_write/edit/read em shared/signal_pipeline.rs (pre_write.rs, pre_edit.rs, pre_read.rs); literal do e2e atualizado. Descoberta: em pre_write/pre_edit o pipeline e 1 StaticSignalLayer com contexto pre-montado — os analisadores de conteudo rodam fora dele; S0 destrava plugar layers TIER-1 reais no pipeline.

## Afirmações sem endereço

_Nenhum fato com run_id foi citado neste fechamento (`--facts`). As afirmações do Summary acima valem o que vale a narrativa — um probe (FACT=) daria endereço a cada uma. (claim-ledger, S-5.5)_

## Knowledge

Typed abstract: [/knowledge/S0-signal-context-v2.json](/knowledge/S0-signal-context-v2.json).
