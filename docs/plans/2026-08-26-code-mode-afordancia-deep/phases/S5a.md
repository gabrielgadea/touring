---
type: PhaseReport
title: S5a — phase report
description: Advisory CEG fora do stderr: gate_run retorna Option<CegRunAdvisory> (warn->debug forense); emit_output insere campo ceg_advisory no JSON (f
plan_id: 2026-08-26-code-mode-afordancia-deep
tags: [loop, phase, S5a]
timestamp: 2026-08-26T16:29:06.716768-03:00
okf_version: "0.1"
---

# S5a — phase report

**Status**: done

Part of the [log](/log.md).

## Summary

Advisory CEG fora do stderr: gate_run retorna Option<CegRunAdvisory> (warn->debug forense); emit_output insere campo ceg_advisory no JSON (full) e no summary (brief, via to_value — shape aditivo). PROVA E2E: ./target/debug/touring run --lang bash --code 'echo hi' → stdout JSON com ceg_advisory {composite 0.675, reason X6, note}, stderr com ZERO ocorrencias do advisory (resta 1 linha INFO enforcement — aviso de capacidade, fora do escopo nomeado). Teste do produtor (gate_run bash echo hi → Ok(Some)). server 1527 passed; clippy limpo.

## Afirmações sem endereço

_Nenhum fato com run_id foi citado neste fechamento (`--facts`). As afirmações do Summary acima valem o que vale a narrativa — um probe (FACT=) daria endereço a cada uma. (claim-ledger, S-5.5)_

## Knowledge

Typed abstract: [/knowledge/S5a.json](/knowledge/S5a.json).
