---
type: PhaseReport
title: "F5-best-practices-gate-warn — phase report"
description: "F5 BestPracticesGate signal_use entregue: 4a regra em best_practices.rs (signal_use_counts + threshold 6/8 = 75%). Severity mantida em Warn "
plan_id: 2026-08-31-code-mode-sinal
okf_version: 0.1
tags: [loop, phase, F5-best-practices-gate-warn]
timestamp: 2026-09-01T06:59:33.584670-03:00
---

# F5-best-practices-gate-warn — phase report

**Status**: done

Part of the [log](/log.md).

## Summary

F5 BestPracticesGate signal_use entregue: 4a regra em best_practices.rs (signal_use_counts + threshold 6/8 = 75%). Severity mantida em Warn (nao fail-closed; promocao requer decisao Gabriel). 5/5 best_practices tests verdes (2 novos: missing_file + distinct_hooks). CC=19 (threshold=15; refatorar quando gate tiver 5+ regras). Payload merged carrega signal_use {used,total,ratio,mirror}.

## Afirmações sem endereço

_Nenhum fato com run_id foi citado neste fechamento (`--facts`). As afirmações do Summary acima valem o que vale a narrativa — um probe (FACT=) daria endereço a cada uma. (claim-ledger, S-5.5)_

## Knowledge

Typed abstract: [/knowledge/F5-best-practices-gate-warn.json](/knowledge/F5-best-practices-gate-warn.json).
