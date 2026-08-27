---
type: KPIReport
title: N5 — injeção nativa (followed × resisted)
description: taxa de injeção nativa do CC seguida × resistida por sessão — baseline minerado de 31 transcripts
tags: [n5, kpi, code-mode, affordance]
plan_id: task_1787767900017576294
---

# N5 — KPI da injeção nativa

- sessões com sinal: **24** / 31 transcripts
- overall: followed=496 · resisted=283 · code_route=863
- **injeção seguida: 63.7%**

| dia | followed | resisted | code_route | taxa |
|---|---|---|---|---|
| 2026-08-23 | 1 | 44 | 79 | 2.2% |
| 2026-08-24 | 251 | 147 | 71 | 63.1% |
| 2026-08-25 | 185 | 50 | 309 | 78.7% |
| 2026-08-26 | 59 | 42 | 404 | 58.4% |

Gate-fatigue (S5b): `TOURING_GATE_OK=1` ×**24** no período (bypass por-comando — o hábito de bypass é candidato a FP, S5c).


Top sessões por volume no eixo:

| sessão | followed | resisted | code_route | taxa |
|---|---|---|---|---|
| 2f2d716c | 330 | 53 | 153 | 86.2% |
| 01ce8edf | 50 | 40 | 312 | 55.6% |
| 46e84c07 | 70 | 0 | 126 | 100.0% |
| e07a20b5 | 1 | 44 | 79 | 2.2% |
| 205e116d | 3 | 30 | 0 | 9.1% |
| 79e4693d | 6 | 19 | 1 | 24.0% |
| 6acb0416 | 7 | 14 | 5 | 33.3% |
| 54a0ba2f | 2 | 17 | 0 | 10.5% |
| b1ed63cf | 2 | 17 | 0 | 10.5% |
| e94aef3f | 3 | 13 | 0 | 18.8% |

Classificação: `is_search_bash`/`is_read_bash` espelham o `resolved_tokens` S1 do cli_suggester.rs (wrappers transparentes). `resisted` = tool dedicada (Grep/Glob/Read); `code_route` = touring run/exec.
