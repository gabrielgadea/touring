---
type: PhaseReport
title: "F4-sdk-tipada-protocol — phase report"
description: "F4 SDK tipada Protocol entregue: injetado record_hook_call + wrap de query() no template Python (crates/touring-server/src/cli/run.rs:298). "
plan_id: 2026-08-31-code-mode-sinal
okf_version: 0.1
tags: [loop, phase, F4-sdk-tipada-protocol]
timestamp: 2026-09-01T06:56:52.822267-03:00
---

# F4-sdk-tipada-protocol — phase report

**Status**: done

Part of the [log](/log.md).

## Summary

F4 SDK tipada Protocol entregue: injetado record_hook_call + wrap de query() no template Python (crates/touring-server/src/cli/run.rs:298). Mirror sink fail-soft (exception nunca aborta programa). Touring-server build verde (4m45s); 1572/1572 lib tests verdes. Cada touring.run().query() agora cronometra + escreve mirror line — quando touringrunning realmente tocar o SDK, signal_use counts vao subir. Schema em concordancia com sdk.rs SignalReport.hooks keys (8 hooks canonicos).

## Afirmações sem endereço

_Nenhum fato com run_id foi citado neste fechamento (`--facts`). As afirmações do Summary acima valem o que vale a narrativa — um probe (FACT=) daria endereço a cada uma. (claim-ledger, S-5.5)_

## Knowledge

Typed abstract: [/knowledge/F4-sdk-tipada-protocol.json](/knowledge/F4-sdk-tipada-protocol.json).
