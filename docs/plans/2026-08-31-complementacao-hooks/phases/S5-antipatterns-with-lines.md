---
type: PhaseReport
title: "S5-antipatterns-with-lines — phase report"
description: "pre_write::antipattern_signals passa a usar detect_antipatterns_with_lines e prefixa cada aviso com L{linha} (linha 0 fica sem prefixo) — si"
plan_id: 2026-08-31-complementacao-hooks
okf_version: 0.1
tags: [loop, phase, S5-antipatterns-with-lines]
timestamp: 2026-09-01T22:17:19.615566-03:00
---

# S5-antipatterns-with-lines — phase report

**Status**: done

Part of the [log](/log.md).

## Summary

pre_write::antipattern_signals passa a usar detect_antipatterns_with_lines e prefixa cada aviso com L{linha} (linha 0 fica sem prefixo) — sinal acionavel (A5) sobre o conteudo PROPOSTO. maybe_add_eval_check preservado. Teste test_pre_write_antipattern_signal_carries_line_numbers (RED comportamental -> GREEN); suite pre_write 69/69 com features; clippy handlers (features de producao) limpo.

## Afirmações sem endereço

_Nenhum fato com run_id foi citado neste fechamento (`--facts`). As afirmações do Summary acima valem o que vale a narrativa — um probe (FACT=) daria endereço a cada uma. (claim-ledger, S-5.5)_

## Knowledge

Typed abstract: [/knowledge/S5-antipatterns-with-lines.json](/knowledge/S5-antipatterns-with-lines.json).
