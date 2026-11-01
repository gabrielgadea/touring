---
type: Strategy
title: OnlineRL — consumir o sinal em volume (offline replay + retenção)
description: Resolve o débito "motor neural girando quase a seco" — replay do corpus de outcomes no engine vivo, retenção do estado entre restarts, e régua medível.
plan_id: 2026-08-29-onlinerl-sinal-em-volume
tags: [strategy, rl, online-rl, replay, learning]
timestamp: 2026-08-29T20:45:00-03:00
okf_version: "0.1"
---

# Estratégia — OnlineRL consumir sinal em volume

## Diagnóstico (tudo provado por execução nesta sessão)

| # | Fato | Prova |
|---|---|---|
| 1 | O caminho de VOLUME online **vive**: post-tool-rl registrado no PostToolUse; cada tool call soma +1 no engine do HookRuntime do daemon | `update_count` 7→19 com as tool calls da sessão; probe post-tool-rl → 20 |
| 2 | O corpus acumulado **nunca** alcança o engine: 820 outcomes com reward (153 negativos) em `memory_entries`, zero canal offline→engine | SELECT no memory.db; grep: nenhum caller liga outcomes a `process_reward` |
| 3 | Retenção **zero**: `update_count`/EMA/janela TD zeram a cada restart do daemon; só QTable (71 pares no graph.db; rkyv 336B) e LinUCB persistem | 13 (ontem) → 7 (hoje pós-restart); grep: nenhum save/load do estado do engine |
| 4 | O auto_learn do server (300s) lê `touring_rlm.db` **morto desde março** e atualiza um SEGUNDO engine que ninguém lê | mtime Mar 31, WAL 0 bytes; `server/mod.rs:1145` usa `self.online_rl` próprio |
| 5 | O "13 updates" do assessment era o instrumento medindo *lifetime do daemon*, não aprendizado da vida | fato 3; a classe `sinais-de-progresso-que-mentem` |

**Reenquadre honesto do débito**: o gargalo não é o trickle online (funciona); é
(a) o corpus offline sem consumidor e (b) a evaporação do estado a cada restart —
somados, o motor recomeça quase do zero várias vezes por dia.

## Lente externa (Context7 `/takuseno/d3rlpy`, offline RL de referência)

Padrão canônico da área: **pretreino offline no dataset logado → fine-tuning
online no MESMO engine** (`fit(dataset)` → `fit_online(env)`); e
`copy_policy_from` ensina que **retenção/transferência de estado é pré-condição
do valor do pretreino**. Melhor prática de replay (linhagem DQN): embaralhar o
batch para quebrar correlação temporal.

## Resolução (ligar, não construir — REGRA #0)

- **R-B RETENÇÃO (P1, primeiro — sem ela o replay evapora)**:
  `OnlineRLEngine::export_stats()/restore_stats()` (update_count, ema_reward);
  save no MESMO batch do `persist_qtable_batched` (cadência 10, zero I/O novo);
  restore quando o runtime constrói o engine. Arquivo:
  `.claude/data/online_rl_state.json`.
- **R-A REPLAY (P2)**: handler in-daemon `cli-learning-replay` + verbo
  `touring learning replay [--limit N] [--dry-run]`. Lê `memory_entries WHERE
  outcome_reward IS NOT NULL AND rowid > cursor`, embaralha, mapeia
  outcome→`ImmediateReward` (tool_name = 2º segmento da key; accepted =
  reward ≥ 0.5; quality_score = reward) e dirige o MESMO
  `process_immediate_reward` do caminho vivo. Cursor durável
  `.claude/touring/learning_replay_cursor.json` (ilegível ⇒ recomeça declarando
  `resumed_from_zero`; TD re-aplicado converge — seguro). Key malformada ⇒
  `replay-unknown` contado, nunca descarte silencioso (lição F-1).
- **R-D HONESTIDADE (P2)**: `learning status` ganha `corpus_pending`
  (rewarded_total − cursor) — o gap fica visível (E4). O engine server-side
  duplicado + rlm.db seco viram ticket, não escopo desta wave.
- **R-C RÉGUA (P3)**: braço KPI `touring.learning.replay_share`
  (consumido/total) lendo o cursor de disco.

## Gatilhos medíveis (o contrato da pendência)

1. `update_count` vitalício > 800 após o backfill **e monotônico através de
   `daemon-ctl restart`** (prova-rainha da retenção).
2. KPI `replay_share ≥ 0.95` pós-backfill.
3. `corpus_pending ≈ 0` no learning status (e cresce só com outcomes novos).
4. Trickle online segue vivo: probe post-tool-rl soma +1 antes e depois.

## Fases

P1 retenção → P2 replay+honestidade → P3 régua+provas vivas → P4 gates+audit+docs.

## Links

- Diagnóstico OUTER: `diagnostics/touring-20260829T203038.md` · Ledger CCE do tema (explore)
- Âncora anterior: `/home/gabrielgadea/projects/touring/docs/plans/2026-08-25-autoresearch-rl-intelligence/RETOMAR-AQUI.md`
- Wave que limpou o sinal: `docs/plans/2026-08-29-ligar-nao-construir/`
