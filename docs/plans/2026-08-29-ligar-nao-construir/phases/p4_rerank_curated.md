---
type: PhaseReport
title: p4_rerank_curated — phase report
description: R5 em 2 ligações: (1) recall RRF agora incrementa access_count das entries servidas no DB canônico — curated_recall_share e never_recalled_r
plan_id: 2026-08-29-ligar-nao-construir
tags: [loop, phase, p4_rerank_curated]
timestamp: 2026-08-29T13:52:02.855241-03:00
okf_version: "0.1"
---

# p4_rerank_curated — phase report

**Status**: done

Part of the [log](/log.md).

## Summary

R5 em 2 ligações: (1) recall RRF agora incrementa access_count das entries servidas no DB canônico — curated_recall_share e never_recalled_ratio passam a medir o retrieval que declaram; (2) rerank_by_case_value ganha eixo de procedência: dentro da classe de valor, curadoria antes de traço de processo (outcome:/decomp:/subtask:/loop:/adw:), sort estável. Teste da ordem completo; clippy 0.

## Afirmações sem endereço

_Nenhum fato com run_id foi citado neste fechamento (`--facts`). As afirmações do Summary acima valem o que vale a narrativa — um probe (FACT=) daria endereço a cada uma. (claim-ledger, S-5.5)_

## Knowledge

Typed abstract: [/knowledge/p4_rerank_curated.json](/knowledge/p4_rerank_curated.json).
