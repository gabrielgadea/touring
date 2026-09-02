---
type: PhaseReport
title: "S1-ast-grep-source-layer — phase report"
description: "AstGrepRiskSignalLayer::enrich prefere ctx.analysable_text() (conteudo proposto, SignalContext v2) via scan_source_cached e cai para scan_pa"
plan_id: 2026-08-31-complementacao-hooks
okf_version: 0.1
tags: [loop, phase, S1-ast-grep-source-layer]
timestamp: 2026-09-01T22:15:26.679012-03:00
---

# S1-ast-grep-source-layer — phase report

**Status**: done

Part of the [log](/log.md).

## Summary

AstGrepRiskSignalLayer::enrich prefere ctx.analysable_text() (conteudo proposto, SignalContext v2) via scan_source_cached e cai para scan_path_cached sem proposta (pre_read intacto). Layer registrado nos pipelines de pre_write e pre_edit (ja estava em pre_read). Testes: 2 novos no layer (arquivo inexistente + proposta vence disco limpo), 8/8 layer_tests; handler pre_write flags [risk] unwrap=1 em arquivo novo (test_pre_write_flags_unwrap_in_proposed_content_of_a_new_file); pre_write 68/68, pre_edit+pre_read 204/204 com --features pre-hooks,post-hooks.

## Afirmações sem endereço

_Nenhum fato com run_id foi citado neste fechamento (`--facts`). As afirmações do Summary acima valem o que vale a narrativa — um probe (FACT=) daria endereço a cada uma. (claim-ledger, S-5.5)_

## Knowledge

Typed abstract: [/knowledge/S1-ast-grep-source-layer.json](/knowledge/S1-ast-grep-source-layer.json).
