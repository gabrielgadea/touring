---
type: ExperimentReport
title: S-5.4 — replay vendado do rerank Jev sobre o recall vivo
description: Replay do recall real com rerank Jev (1 chamada Noul por query) sobre o split de teste congelado — direção e magnitude, não significância (33 queries / 6 yes). O experimento alimentou a decisão da flag TOURING_JEV_RECALL_RERANK (off por default).
plan_id: 2026-09-30-jev-harness-potencializacao
tags: [s54, replay, recall, rerank, jev]
timestamp: 2026-10-01T19:05:00-03:00
okf_version: "0.1"
---

# S-5.4 — replay do rerank Jev sobre o recall vivo

Parte do [bundle](/index.md). Dados: [results.json](/experiments/s54-replay/results.json).
Gerador: `scripts/s54_replay.py` (vendável: testes em `scripts/test_s54_replay.py`).

## O que mediu

O recall vivo (`touring memory recall`, ordem fundida por score desc — proxy RRF
documentado) é reordenado por UMA chamada Jev com N Nouls (P(sim) por candidato).
Métricas por sistema: top1 (o primeiro é relevante?) e MRR (posição do primeiro
relevante).

## Resultado (split de teste congelado, seed 20260922)

| Sistema | top1 | MRR |
|---|---|---|
| current (ordem viva) | 4 | 0,1212 |
| jev (rerank) | **5** | **0,1515** |

- 33 queries · 6 pares yes · **21 candidatos ausentes do recall vivo** (o recall nem
  os retornou — o rerank não pode recuperar o que a busca não trouxe).
- **Caveat (do próprio results.json)**: ler direção+magnitude, não significância.

## Decisão derivada

O rerank vence em direção (+1 top1, +0,03 MRR) mas a margem é pequena e 21/39
candidatos nem chegam ao recall — o gargalo é a **recuperação**, não a ordenação.
Por isso a S-5.4 virou **flag opt-in** (`TOURING_JEV_RECALL_RERANK`, default OFF)
em `~/.claude/hooks/session_startup_intelligence.py`, e a W5 recomendou medir a
recuperação antes de investir em rerank.

## Nota de auditoria (2026-10-01)

Este report.md estava ausente (só results.json existia) — drift OKF apontado no
cross-audit de 01/10 e corrigido aqui. O script ganhou cobertura de testes
(F3.1: 0,6 → 1,0 — 5 testes vendados incluindo `main()` E2E com corpus fixture).
