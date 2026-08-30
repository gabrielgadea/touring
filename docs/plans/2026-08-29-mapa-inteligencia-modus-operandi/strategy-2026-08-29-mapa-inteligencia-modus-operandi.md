---
type: Strategy
title: Mapa — a inteligência no modus operandi (ADW · loop · Touring · Claude Code)
description: Síntese de como a estrutura de inteligência (memória + política RL) se relaciona com os fluxos ADW, o loop engineering e o modus operandi Touring/Claude Code — com o estado medido de cada elo em 29/08/2026.
plan_id: 2026-08-29-mapa-inteligencia-modus-operandi
tags: [strategy, rl, adw, loop-engineering, modus-operandi]
timestamp: 2026-08-29T22:10:00-03:00
okf_version: "0.1"
---

# O mesmo organismo em quatro tempos

Três planos: **execução** (Claude Code + runner ADW) gera eventos; **julgamento**
(gates determinísticos — quality 50-dim, verdict contracts, loop_converged, CEG)
rotula os eventos por código, nunca por narrativa (L2/L3); **aprendizado** (a
estrutura de inteligência) transforma eventos rotulados em viés para a PRÓXIMA
decisão dos dois primeiros planos. O ciclo ACO: consultar → executar → julgar →
registrar → reforçar → reconsultar.

| Estrutura | Papel no ciclo | Evidência (29/08) |
|---|---|---|
| **Claude Code** | Substrato de eventos + ponto de influência: PostToolUse alimenta o trickle (post-tool-rl → +1 update); PreToolUse é onde a inteligência muda a ação (nudges, gates G1-G10) | update_count 7→19→20 provado; enforcement no executor (D8) |
| **ADW** | Produtor estrutural de sinal rotulado (deposit_run_outcome, gate-reject:flow:nó, router premiado) e consumidor por construção (nós recall/prior_art) | outcome_reward 1,70%→7,87%; 7 gate-rejects acumulando p/ DSPy |
| **Loop engineering** | Fábrica do rótulo mais confiável: phase_close = store+reward+variant→experiment_log; loop_converged exit 0 é o sucesso que narrativa não fabrica; OUTER = "consultar" institucionalizado | 4 phase_closes hoje; converged exit 0 Platinum 0.9183 |
| **Inteligência** | O que atravessa sessões: declarativo (memória re-ranqueada por procedência) + procedimental (QTable/LinUCB/OnlineRL — agora com retenção e replay) | backfill 3→822; restart monotônico 822→822; replay_share 1.0 |

## Maturidade dos elos (medida, não inferida)

- **Fortes**: registro (10k memórias facetadas) · julgamento (críticos cegos +
  judge_attest) · crédito semântico (7,87%) · **sinal→motor (fechado hoje)**.
- **Médios**: recall→decisão (re-rank por procedência + access_count, wave R1-R6).
- **Fracos — a próxima fronteira, com gatilho**: política→decisão (predict-action
  já devolveu 0.990 constante; ninguém mediu se suggest/mcts MUDAM com o
  aprendizado) · auto_learn lê rlm.db morto (2º engine sem leitor) · DSPy/GEPA
  aguarda ~30 rejeições/verificador · pillar induction desarmado (humano).

## As três leis são o que impede cada tempo de degenerar

L1 (OUTER-first) = ler o feromônio antes de agir · L2 (código encerra) = o rótulo
do sinal é de gate, não de autoavaliação · L3 (artefato) = o que não está em disco
não aconteceu. REGRA #0 é o vetor: cada peça órfã encontrada vira elo do ciclo.

Âncoras: `2026-08-29-onlinerl-sinal-em-volume/` (sinal→motor) ·
`2026-08-29-ligar-nao-construir/` (R1-R6) · `2026-08-25-autoresearch-rl-intelligence/`.
