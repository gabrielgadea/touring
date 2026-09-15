---
type: PhaseReport
title: "R8-fechamento — phase report"
description: "Rodada 8: SIGSEGV do daemon provado sob ASan (dois defeitos no scanner md), cessão cooperativa provada ao vivo, produtores de wiring, selo d"
plan_id: 2026-09-12-graft-analysis
okf_version: 0.1
tags: [loop, phase, R8-fechamento]
timestamp: 2026-09-14T09:37:56.999069-03:00
---

# R8-fechamento — phase report

**Status**: done

Part of the [bundle](/index.md) · [plan](/plan.md) · [log](/log.md).

## Summary

Rodada 8: SIGSEGV do daemon provado sob ASan (dois defeitos no scanner md), cessão cooperativa provada ao vivo, produtores de wiring, selo de geração, 75 de hook pesado, nota Silver por vendorizado; releases 30.4.44 e 30.4.45 propagadas com prova 40/40; juiz Platinum

## Schema

| Gate clause | Result | Evidence |
|-------------|--------|----------|
| judge_intact | PASS | judge of record intact, 8 clauses |
| dag_done | FAIL | 18/19 subtasks done |
| quality_gold | PASS | tier=Platinum composite=0.937443 |
| no_p0_fail | PASS | P0 fails: none |
| measured_whole_scope | PASS | no dim reported a truncated corpus |
| orphans_base | PASS | scoped orphans=1553 baseline=1560 |
| cargo_green | PASS | cargo check green (test+clippy deferred; pass --rust-full for the final gate) |
| cross_audit | N/A | no audit-plan-completion.sh — skipped |

## Fatos com endereço (claim-ledger)

| Fato | Valor | run_id |
|---|---|---|
| asan.analise_md.parsed | 32979 arquivos, 0 relatos ASan (scanner com patch) | `ts-asan-full2` |
| asan.controle_negativo | upstream 0.5.3: SEGV em 260/400/1000 aspas e lista 300; U+FF495 SEGV parse_ordered_list_marker | `ts-asan-nocap` |
| a1.recall_durante_rebuild_ms | 535/591/783 ms, rc=0, index status 16-28 ms, geração building | `a1-live-2026-09-14` |
| produtores.editados_sem_produtor | 10 de 15 editados vs 3 de 1415 não editados | `producer-census-2026-09-14` |
| suite.workspace | 16367 passed, 0 failed | `touring-gate3` |
| release.30.4.45 | prova 40/40; analise e konverter lock=30.4.45 | `touring-release45` |
| quality.workspace | Platinum 0.937443, sem bloqueios | `judge3` |
| analise.geracao8 | complete 50682 arquivos, kind_unknown 1553->0, produtores 31736->55796 | `analise-b2-rebuild` |

## Knowledge

Typed abstract: [/knowledge/R8-fechamento.json](/knowledge/R8-fechamento.json).
