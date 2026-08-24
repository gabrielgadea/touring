---
type: Strategy
title: "Follow-ups da cross-audit 2026-08-12 — correção total"
description: "Estratégia para fechar os 6 follow-ups registrados pela auditoria da hashtag library: fallback sem definitions, rebuild full, dual schema memory_entries (convergência RPC→RlmMemory), mega-ciclo 806, TODOs env-unsafe, cycle registry↔daemon."
plan_id: 2026-08-12-hashtag-followups
tags: [loop, strategy, wiring, memory, cycles, followups]
timestamp: 2026-08-12T15:00:00-03:00
okf_version: "0.1"
---

# Estratégia — Follow-ups da auditoria (todos, sem exceção)

Part of the [bundle](/index.md). Origem: [cross-audit-2026-08-12](/../../audits/cross-audit-2026-08-12.md).

## Os 6 follow-ups e a decisão de cada um

| # | Follow-up | Decisão |
|---|---|---|
| **F-1** | `extract_symbols_fallback` não grava nem definitions no `symbols.db` (só knowledge) | Fallback grava defs (+ call-sites via `with_call_sites`) no symbol_store — simetria total dos 3 caminhos |
| **F-2** | Rebuild full para call-sites do histórico | G0 — executar PRIMEIRO (o fix `with_call_sites` já deployado torna o rebuild o backfill); re-mede tudo depois |
| **F-3** | `CREATE_MEMORY_ENTRIES` dual schema + DROP TABLE destrutivo | Convergência real: `RlmMemory::store_rich` (todos os campos S4) + handler RPC delega; DROP removido (migração aditiva sempre); schema single-source |
| **F-4** | Mega-ciclo wiring depth-806 (storage/hooks-core) | Medir pós-rebuild: se heurístico (provável — 64% name-matching), corrigir o detector; se real, isolar a aresta que fecha o ciclo |
| **F-5** | 6 TODOs `unsafe env` (Rust 2024) em inferlet/toolchain | Auditar cada set_var/remove_var: startup single-threaded ou teste → documentar a auditoria com evidência (o TODO pede exatamente isso: "Audit that…") |
| **F-6** | Cycle hook_registry↔daemon (depth 2) | Mover `HookHandler` para módulo de tipos neutro; daemon consome, registry consome — aresta invertida some |

## Riscos

- **F-3** é o de maior superficie: o INSERT RPC tem semântica G6 byte-compat (COALESCE access_count, NULL discipline de outcome/importance). `store_rich` preserva a semântica exata + testes de paridade (RPC vs RlmMemory produzem a mesma row).
- **F-4** pode ser irredutível em uma sessão se o ciclo for real e arquitetural — o compromisso é diagnóstico completo + a aresta exata + quebra se for heurística; se for real, a quebra mínima (a aresta mais fraca) ou plano L4 documentado.
- **F-2** é wall-clock longo (rebuild full ~minutos a dezenas de minutos) — roda em background.

## Convergência

`loop_converged.py --rust-full` exit 0 sobre o workspace + evidência por fase (cada follow-up fecha com a prova que o elimina: query SQL, cycle count, scan_debt limpo).
