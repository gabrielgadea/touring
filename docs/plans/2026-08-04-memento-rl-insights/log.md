---
type: Log
title: Log — chronological history of this loop run
description: Append-only history; PreCompact resume notes and phase closes land here.
plan_id: 2026-08-04-memento-rl-insights
tags: [loop, log]
timestamp: 2026-08-04T19:52:21.033604-03:00
okf_version: "0.1"
---

# Log

Part of the [bundle](/index.md).

## 2026-08-04T21:30-03:00 — Fases 1-4 implementadas

Aprovado o escopo completo (fases 1-4). Todos os gates verdes:
`cargo check --workspace --all-targets` exit 0 · `cargo clippy --workspace
--all-targets -D warnings` 0 erros · testes de touring-intelligence /
touring-cli / touring-server / touring-hook-runtime 0 falhas · 0 órfãos.

| Fase | Entrega | Prova |
|---|---|---|
| F1 dinâmica | `terminal` honrado; traces limpos; γ único; `mean_td_error` real | `repeated_terminal_self_loop_updates_stay_on_the_reward_scale` (Q ≤ escala do reward, antes → 655) |
| F2 crédito | `DecisionLedger`; `get_bandit()`; 25 features; 5 rewards com variância | `a_recorded_decision_is_credited_instead_of_the_hash_bucket` |
| F3 casos | `outcome_reward` + `outcome_context`; `--reward` | `store_parses_key_and_value` (NULL ≠ 0.0) |
| F4 recall | `case_value` + rerank estável em 3 classes | `rerank_changes_the_order_it_is_given` (contraprova) |

REGRA #21 (fora do escopo original, corrigidos mesmo assim): `project_root`
não usado sob `cfg(not(tantivy-fts))`; doctest de `leiden.rs` quebrado —
corrigido com bloco `cfg` e **contraprova executada**.

Pendente: deploy (`update-touring`) — gate humano.
