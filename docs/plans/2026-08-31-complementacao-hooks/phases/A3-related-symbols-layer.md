---
type: PhaseReport
title: "A3-related-symbols-layer — phase report"
description: "A3 RelatedSymbolsLayer (pre_write) entregue com motor deterministico: nomes que o arquivo PROPOSTO declara (struct/enum/trait/type/union/fn;"
plan_id: 2026-08-31-complementacao-hooks
okf_version: 0.1
tags: [loop, phase, A3-related-symbols-layer]
timestamp: 2026-09-01T23:09:54.866839-03:00
---

# A3-related-symbols-layer — phase report

**Status**: done

Part of the [log](/log.md).

## Summary

A3 RelatedSymbolsLayer (pre_write) entregue com motor deterministico: nomes que o arquivo PROPOSTO declara (struct/enum/trait/type/union/fn; class/def; class/function/interface/type/enum) e que o SymbolStore (runtime.symbol_store().find_symbol) ja define em OUTRO arquivo viram '[related] X already defined in f:l (+N more) — homonym (VP-Scout chain 4)'; nomes genericos/curtos nunca chegam ao indice (MAX 8 nomes/arquivo); sinal so em Write. Rota ANN (embedding de sitios) fica como v2 — nada aqui a fecha. Modulo touring-hook-handlers/src/shared/related_symbols.rs.

## Afirmações sem endereço

_Nenhum fato com run_id foi citado neste fechamento (`--facts`). As afirmações do Summary acima valem o que vale a narrativa — um probe (FACT=) daria endereço a cada uma. (claim-ledger, S-5.5)_

## Knowledge

Typed abstract: [/knowledge/A3-related-symbols-layer.json](/knowledge/A3-related-symbols-layer.json).
