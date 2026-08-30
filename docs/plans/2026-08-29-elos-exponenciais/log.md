---
type: Log
title: Log — chronological history of this loop run
description: Append-only history; PreCompact resume notes and phase closes land here.
plan_id: 2026-08-29-elos-exponenciais
tags: [loop, log]
timestamp: 2026-08-29T21:40:58.113057-03:00
okf_version: "0.1"
---

# Log

Part of the [bundle](/index.md).

## 2026-08-29T22:45 — INNER P1+P2+P3 (implementação)

- Grounding que definiu o escopo: `cli_suggest_action` é canal Pln3 (não política);
  `predict-action` consertado em 20/08 (envelope); LinUCB tem subciclo fechado
  (select→reward); o consumidor documentado da QTable (`suggest_context_level`
  "using QTable") na verdade delega ao LinUCB — **a QTable é órgão write-only**.
- P1: core `runtime/replay.rs` no hook-runtime (cursor_read/corpus_pending/
  replay_outcomes_into); casca CLI delega; session-start drena delta (cap 500,
  fail-open) — consumo automático POR CONSTRUÇÃO.
- P2: KPI `touring.learning.policy_discrimination` (dispersão de Q por estado
  multi-ação, na MESMA learning_qtable que o replay treina — 127 pares, eram 71
  pré-backfill) + commitment; doc do trait corrigida (classe
  comentário-afirma-simetria). Ligar QTable a decisor real fica GATED no KPI.
- P3: débito auto_learn/rlm.db seco → decisão formal em memória
  (`debito:auto-learn-rlm-db-seco:2026-08-29`, hitl, pending).

## 2026-08-29T21:49:23.490228-03:00 — P1 done

Replay automático por construção: core movido para touring_hook_runtime::runtime::replay (cursor_read/corpus_pending/replay_outcomes_into), casca CLI delega, session-start drena o delta (cap 500, fail-open, silencioso em 0). O corpus nunca mais acumula sem consumo. Suites: e2e round-trip + runtime 382 + handlers 103 + cli 484 verdes.

## 2026-08-29T21:49:23.627600-03:00 — P3 done

Honestidade: débito auto_learn/rlm.db seco (2º engine sem leitor) registrado como decisão formal hitl pending (debito:auto-learn-rlm-db-seco:2026-08-29); a wave deliberadamente não construiu sobre engine sem leitor. CHANGELOG + bundle atualizados.

## 2026-08-29T21:55:59.009153-03:00 — P2 done

Régua política→decisão viva: KPI touring.learning.policy_discrimination actual=1.0 PASS (todos os estados multi-ação da learning_qtable com dispersão >0.01; 127 pares — a mesma tabela que o replay treina); predict-action DISCRIMINA ao vivo (false→0.500, cargo check→0.991, echo ok→0.972 — a constante 0.990 morreu); doc do trait suggest_context_level corrigida (dizia QTable, sempre foi LinUCB); rationale do ema_reward atualizado (retenção P1 tornou obsoleto o 'zera a cada restart' — 0.84 medido pós-restart). Ligar QTable a decisor real fica GATED no KPI.

## 2026-08-29T21:55:59.126404-03:00 — P4 done

Gates e provas: e2e round-trip + runtime 382 + handlers 103 + cli 484 verdes, clippy 4 crates -D warnings exit 0, deploy update-touring OK (doctor 6/7). Provas vivas: drain automático do session-start zerou pendência 4→0 (update_count 993→1002) sem verbo manual; replay_share 0.995 PASS; policy_discrimination 1.0 PASS; ema_reward 0.84 sobrevivendo restart (prova extra da retenção P1 da wave anterior).
