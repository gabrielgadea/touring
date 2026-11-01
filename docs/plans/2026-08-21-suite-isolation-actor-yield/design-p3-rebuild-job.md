---
type: design
title: "P3 — rebuild que cede o ator: RebuildJob reentrante com passos orçados"
description: "Desenho aprovado para a fase (b): transformar cli_index_rebuild de chamada monolítica em job dirigido pelo ator, com passos de 250ms, progresso em index status e --wait no CLI. Inclui a alternativa descartada e o prior art consultado."
plan_id: 2026-08-21-suite-isolation-actor-yield
tags: [design, daemon, actor, rebuild, cooperative-yield]
timestamp: 2026-08-22T00:55:00-03:00
---

# P3 — RebuildJob: o rebuild cede o ator

## O problema (medido)

`cli_index_rebuild` é uma chamada monolítica no ator single-thread do projeto:
walk → N arquivos → G3 → sweeps → backfill, 10-40 min neste workspace (4.665
arquivos). Enquanto roda, **toda** op barata enfileirada atrás estoura budget:
`viz workspace` (15s), `doctor`, `index status`. Foi o mecanismo que reprovou 13
gates de convergência em 21/08 (diagnóstico:
`/docs/plans/2026-08-20-work-outer/diagnostics/2026-08-21-cargo-green-starvation.md`).

## O desenho

| Peça | O quê |
|---|---|
| `RebuildJob` | todo estado que o rebuild carrega entre passos (paths, cursor, acumuladores, flags de RSS/oversized) |
| `REBUILD_JOB` | `Mutex<Option<RebuildJob>>` — tocado só pela thread do ator; mutex é por ser `static`, não por contenção |
| `rebuild_step(rt)` | um passo orçado em `REBUILD_STEP_BUDGET` (250 ms); `Some(report)` quando finaliza, `None` enquanto restam passos; `catch_unwind` aborta o job em vez de deixar `REBUILD_IN_PROGRESS` preso |
| `run_project_actor` | entre comandos: `try_recv` — se há comando, serve; se a fila está vazia e o job tem passos, roda um passo |
| `index status` | ganha `rebuild:{in_progress, files_done, files_total, elapsed_ms}` **fora** do corpo cacheado (senão o cache de 60s reportaria um rebuild que já acabou) |
| `index rebuild` | devolve o relatório final quando a árvore é pequena (primeiro passo inline), ou `{"status":"in_progress"}`; `--wait` pola `index status` |
| `init --fast` | passa a usar `--wait` (mantém a semântica sequencial que o comando promete) |

**Invariante preservado por construção**: todo passo roda na thread do ator com o
`HookRuntime` do ator — single-writer (rusqlite `!Sync`, tantivy `IndexWriter`)
continua valendo. Nenhum estado novo é compartilhado entre threads.

## Alternativa descartada

**Rebuild em thread própria.** É a saída canônica da literatura ("opt-out do
cooperative scheduling: rode o ator que bloqueia em sua própria thread"), e não
serve aqui: o `IndexWriter` do tantivy é single-writer — um segundo writer colide
no lock do índice (`LockBusy`) — e o `HookRuntime` carrega `rusqlite::Connection`,
que é `!Sync` e está pinado à thread do ator.

**Fila com prioridade** (reads furam heavy ops) resolve o mesmo problema e é mais
invasiva: mexe em `ProjectCommand`/dispatch, não só no handler. Fica registrada
como evolução possível se o yield se mostrar insuficiente.

## Prior art (lente external do CCE)

- Swift Concurrency: `Task.yield()` existe exatamente para trabalho CPU-bound longo
  — "a task that never suspends monopolizes the thread and starves others". É o
  `REBUILD_STEP_BUDGET`.
- Akka / libcppa (arXiv 1301.0748): mailbox FIFO single-threaded com prioridade
  opcional; o opt-out para trabalho bloqueante é a thread dedicada — descartada acima.

## Como se prova (gate P3)

1. `cargo test` dos 3 testes novos em `cli_handlers_e2e.rs` (grupo serial `rebuild_job`):
   árvore pequena termina inline · job cede entre passos, `index status` mostra
   progresso ao vivo, segundo rebuild é recusado, guard liberado no fim · `rebuild_step`
   sem job é no-op.
2. `cargo test --workspace` + clippy verdes.
3. `update-touring` + **prova viva**: `index status` responde em <100 ms *durante* um
   rebuild do workspace (hoje: 15 s de timeout).
