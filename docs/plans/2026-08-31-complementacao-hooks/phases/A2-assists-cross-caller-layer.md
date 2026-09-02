---
type: PhaseReport
title: "A2-assists-cross-caller-layer — phase report"
description: "A2 CrossCallerLayer (pre_edit) entregue: quando o Edit MUDA uma chamada (conjunto de expressoes name(...) sem whitespace difere entre old/ne"
plan_id: 2026-08-31-complementacao-hooks
okf_version: 0.1
tags: [loop, phase, A2-assists-cross-caller-layer]
timestamp: 2026-09-01T23:09:55.106126-03:00
---

# A2-assists-cross-caller-layer — phase report

**Status**: done

Part of the [log](/log.md).

## Summary

A2 CrossCallerLayer (pre_edit) entregue: quando o Edit MUDA uma chamada (conjunto de expressoes name(...) sem whitespace difere entre old/new; definicoes fn/def/function e macros name!( nao sao chamadas; keywords e nomes <4 chars excluidos), o SymbolStore (find_references) responde os OUTROS call sites -> '[C08] total changes here and has N other call sites in M files: f:l …' (cap 5 callees, 4 sites nomeados). O pipeline do pre_edit passa a rodar tambem quando so este sinal existe (antes exigia contexto assembled nao-vazio). Motor deterministico; a rota ANN de sitios analogos fica como v2 com o mesmo gatilho.

## Afirmações sem endereço

_Nenhum fato com run_id foi citado neste fechamento (`--facts`). As afirmações do Summary acima valem o que vale a narrativa — um probe (FACT=) daria endereço a cada uma. (claim-ledger, S-5.5)_

## Knowledge

Typed abstract: [/knowledge/A2-assists-cross-caller-layer.json](/knowledge/A2-assists-cross-caller-layer.json).
