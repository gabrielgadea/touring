---
type: Report
title: N5 — eixo de inspeção e funil pós-deny (code mode)
description: classes honestas (S2) + funil pós-deny, minerado de 17 transcripts
tags: [n5, kpi, code-mode, affordance]
---

# N5 — eixo de inspeção e funil pós-deny

- sessões com sinal: **14** / 17 transcripts
- eixo: bash_native=686 · native_tool=417 · code_route=175
- **rota-code no eixo: 13.7%**
- **pós-deny → rota: 100.0%** (denies=1: rota 1 · nativa 0 · re-emissão 0 · bypass 0 · outra 0)

| dia | bash_native | native_tool | code_route | share |
|---|---|---|---|---|
| 2026-08-23 | 51 | 14 | 8 | 11.0% |
| 2026-08-24 | 196 | 34 | 14 | 5.7% |
| 2026-08-25 | 308 | 175 | 42 | 8.0% |
| 2026-08-26 | 131 | 194 | 111 | 25.5% |

Gate-fatigue (S5b): `TOURING_GATE_OK=1` ×**39** no período (bypass por-comando).


Top sessões por volume no eixo:

| sessão | bash_native | native_tool | code_route | share |
|---|---|---|---|---|
| 0b0b06ef | 59 | 128 | 37 | 16.5% |
| 9028f796 | 94 | 21 | 22 | 16.1% |
| 67cfcd88 | 30 | 43 | 60 | 45.1% |
| 52b1d7cf | 62 | 54 | 0 | 0.0% |
| 917d967b | 82 | 31 | 0 | 0.0% |
| febd99ba | 113 | 0 | 0 | 0.0% |
| bbef3912 | 60 | 39 | 0 | 0.0% |
| dc87e4e0 | 54 | 25 | 13 | 14.1% |
| 49d91b53 | 57 | 33 | 0 | 0.0% |
| d315e69d | 44 | 31 | 0 | 0.0% |

Classes (S2): `bash_native` = inspeção via Bash; `native_tool` = Grep/Glob/Read; `code_route` = touring run/exec. Funil pós-deny: primeiro tool call após um deny `[CODE MODE]` (rota / nativa / re-emissão / bypass / outra). Verbos espelham `resolved_tokens`/`effective_tokens` S1 do Rust.
