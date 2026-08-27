---
type: PhaseReport
title: N3a — phase report
description: G9 gate escrita-cega-inline: predicado is_inline_blind_edit ancorado em posicao de comando (echo "rode sed -i" e prosa; find -exec/xargs doc
plan_id: 2026-08-26-code-mode-afordancia-deep
tags: [loop, phase, N3a]
timestamp: 2026-08-26T15:58:39.952357-03:00
okf_version: "0.1"
---

# N3a — phase report

**Status**: done

Part of the [log](/log.md).

## Summary

G9 gate escrita-cega-inline: predicado is_inline_blind_edit ancorado em posicao de comando (echo "rode sed -i" e prosa; find -exec/xargs documentados como lacuna). Deny carrega rotas derivadas (Edit no alvo real + mesmo comando no sandbox com aspas escapadas + ast grep --rewrite). GateId::G9=6 em foundation (array 6->7, label g9_sed_inline, all() aditivo). Followed/Bypassed/Denied nos 4 eventos. 3 testes novos (deny+rotas, variantes/poupas, bypass). BONUS: 2 testes de config/paths falhavam por /tmp/.touring (debris do strategy-loop de probes 13:28) — quarentenado, 11/11 verde. cli 443 + foundation 484 passed, 0 failed; clippy limpo.

## Afirmações sem endereço

_Nenhum fato com run_id foi citado neste fechamento (`--facts`). As afirmações do Summary acima valem o que vale a narrativa — um probe (FACT=) daria endereço a cada uma. (claim-ledger, S-5.5)_

## Knowledge

Typed abstract: [/knowledge/N3a.json](/knowledge/N3a.json).
