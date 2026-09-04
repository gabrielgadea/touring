---
type: LoopBundle
title: "Bundle — /tmp como sintoma: sandbox, harness e portfólio"
description: "OUTER (strategy-loop + explore CCE) e estratégia com decision-canvas sobre contenção do trabalho efêmero e afordância do portfólio."
plan_id: 2026-09-02-tmp-sandbox-portfolio-afordancia
okf_version: 0.1
tags: [loop, bundle, code-mode, sandbox, portfolio]
timestamp: 2026-09-02T04:58:00-03:00
---

# Bundle — /tmp como sintoma

- [Estratégia + decision-canvas](/strategy-2026-09-02-tmp-sandbox-portfolio-afordancia.md) — o entregável deste OUTER.
- [Diagnóstico OKF](/diagnostics/touring-20260902T041748.md) — saúde, 50-dim, wiring, memória.
- `diagnostics/census-2026-09-02.json` — censo do `/tmp`, DBs, journal, CEG (gerado por `diag_tmp_afordancia.py` via `touring run`).
- Ledger CCE: `.touring-explore/afordancia-sandbox-portfolio-tmp-code-mode-harne.ledger.json` (convergido; lente externa visitada).
- [Histórico](/log.md).
- [Retomar aqui](/RETOMAR-AQUI.md) — estado da execução e o próximo comando.
- [Lições consolidadas](/LICOES-CONSOLIDADAS-2026-09-02.md) — o que foi feito, as 12 lições com a evidência de cada uma, os números e as decisões pendentes.
- [Proposta de condensacao do CLAUDE.md](/PROPOSTA-CONDENSACAO-CLAUDE-MD.md) — o texto condensado de cada item e o destino do que sai; nao aplicada.

## Fases (DAG `task_1788340910617776382`)

- [W — wiring: por que o rebuild perdeu arestas](/phases/W-wiring-rebuild-perde-arestas-inferidas-fix-detector-rejulgar-TIER2.md)
- [B1 — CEG: tmp privado por run, Landlock, `tmp_bytes`](/phases/B1.md)
- [B2 — run_journal v2](/phases/B2.md)
- [B3 — F2.1 XSS: fim do falso positivo `onerror=`](/phases/B3.md)
- [C1 — escada automática de snippets](/phases/C1.md)
- [C2 — scratch persistente em `.touring/scratch/`](/phases/C2.md)
- [C3 — prior-art pré-write pela docstring](/phases/C3.md)
- [C4 — KPI `code_mode_reuse`](/phases/C4.md)

Estado: decisão de Gabriel = **B + C** (Canvas 1) e **W antes de rejulgar** (Canvas 2). W done; B1..C4 completed (código verde). Falta: deploy 30.4.32, prova comportamental ao vivo e rejulgamento (ver Retomar).
