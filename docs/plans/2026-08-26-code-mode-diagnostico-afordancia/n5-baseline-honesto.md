---
type: Report
title: N5 — eixo de inspeção e funil pós-deny (code mode)
description: classes honestas (S2) + funil pós-deny, minerado de 31 transcripts
tags: [n5, kpi, code-mode, affordance]
---

# N5 — eixo de inspeção e funil pós-deny

- sessões com sinal: **24** / 31 transcripts
- eixo: bash_native=502 · native_tool=283 · code_route=900
- **rota-code no eixo: 53.4%**
- **pós-deny → rota: 69.5%** (denies=59: rota 41 · nativa 1 · re-emissão 1 · bypass 3 · outra 13)

| dia | bash_native | native_tool | code_route | share |
|---|---|---|---|---|
| 2026-08-23 | 1 | 44 | 79 | 63.7% |
| 2026-08-24 | 251 | 147 | 71 | 15.1% |
| 2026-08-25 | 185 | 50 | 309 | 56.8% |
| 2026-08-26 | 65 | 42 | 441 | 80.5% |

Gate-fatigue (S5b): `TOURING_GATE_OK=1` ×**28** no período (bypass por-comando).


Top sessões por volume no eixo:

| sessão | bash_native | native_tool | code_route | share |
|---|---|---|---|---|
| 2f2d716c | 330 | 53 | 153 | 28.5% |
| 01ce8edf | 50 | 40 | 312 | 77.6% |
| 46e84c07 | 70 | 0 | 126 | 64.3% |
| e07a20b5 | 1 | 44 | 79 | 63.7% |
| dfed64be | 2 | 0 | 107 | 98.2% |
| 8d7ca948 | 9 | 0 | 83 | 90.2% |
| 205e116d | 3 | 30 | 0 | 0.0% |
| 6acb0416 | 7 | 14 | 5 | 19.2% |
| 79e4693d | 6 | 19 | 1 | 3.8% |
| 54a0ba2f | 2 | 17 | 0 | 0.0% |

Classes (S2): `bash_native` = inspeção via Bash; `native_tool` = Grep/Glob/Read; `code_route` = touring run/exec. Funil pós-deny: primeiro tool call após um deny `[CODE MODE]` (rota / nativa / re-emissão / bypass / outra). Verbos espelham `resolved_tokens`/`effective_tokens` S1 do Rust.
