---
type: PhaseReport
title: "P5-medicao-real — phase report"
description: "P5 done: prova comportamental FAIL=0 nas 6 provas do p5_verify.sh — touring 30.4.29 (A), orchestrate executa com exit 0 onde era SyntaxError"
plan_id: 2026-08-31-code-mode-sinal
okf_version: 0.1
tags: [loop, phase, P5-medicao-real]
timestamp: 2026-09-01T17:29:47.028192-03:00
---

# P5-medicao-real — phase report

**Status**: done

Part of the [bundle](/index.md) · [log](/log.md).

## Summary

P5 done: prova comportamental FAIL=0 nas 6 provas do p5_verify.sh — touring 30.4.29 (A), orchestrate executa com exit 0 onde era SyntaxError (B/P1), mirror 9->19 linhas com escrita REAL do sandbox atravessando o Landlock FILE-grant (C/P2), KPI code_mode_signal_use lido vivo (E), satélites 30.4.29 (F). A MEDIÇÃO REAL FEZ SEU TRABALHO e pegou o que os seeds escondiam: (1) o wrap do SDK grava o nome ALIASED do daemon (cli-index-find) e não o canônico (index_find) — drift template vs HookName::ALL; (2) code_mode_signal_use conta strings distintas SEM validar contra os 8 canônicos — ratio 1.0 atual está inflado (cobertura canônica real ~4/8); (3) feeder post-bash provado correto por invocação direta (payload -> mirror delta 1, linha canônica index_find), mas o evento da SESSÃO não o alcança — diagnóstico de entrega pendente. Os 3 achados entram como Fase 0 da wave signal-layer-tier-ab aprovada por Gabriel (mesmo subsistema), não ficam abertos sem dono.

## Afirmações sem endereço

_Nenhum fato com run_id foi citado neste fechamento (`--facts`). As afirmações do Summary acima valem o que vale a narrativa — um probe (FACT=) daria endereço a cada uma. (claim-ledger, S-5.5)_

## Knowledge

Typed abstract: [/knowledge/P5-medicao-real.json](/knowledge/P5-medicao-real.json).
