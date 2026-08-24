---
type: strategy
title: "Fix de classe da flakiness de suíte: S-4 + peer-cred → isolamento e2e → rebuild cede o ator"
description: "Estratégia aprovada por Gabriel (21/08 23:15: 'implemente tudo na sequência') para eliminar a classe de starvation do ator single-thread que mantém cargo_green vermelho: três fases ordenadas por custo-benefício, cada uma com gate por exit code."
plan_id: 2026-08-21-suite-isolation-actor-yield
tags: [strategy, daemon, starvation, e2e, nextest, generator, actor]
timestamp: 2026-08-21T23:25:00-03:00
---

# Estratégia — suite isolation + actor yield

Diagnóstico de origem: `/docs/plans/2026-08-20-work-outer/diagnostics/2026-08-21-cargo-green-starvation.md`
(memórias `cargo-green-starvation-diagnosis-2026-08-21`, `suite-shared-state-flakiness-class-2026-08-21`).

## Intenção

Tornar `cargo test --workspace` determinístico pelo CÓDIGO e o daemon imune a
negação-de-serviço por heavy op — sem quebrar o invariante single-writer
(tantivy `IndexWriter` + rusqlite `!Sync`) do ator do projeto.

## Fases (ordem = custo-benefício; cada uma fecha com `loop_converged.py` exit 0)

### P1 — (a) origem + observabilidade  [deploy: sim]
- **a1** `generator_tools.rs` S-4: trocar `touring index rebuild --dir <pai>` por
  diretório (fire-and-forget, N paralelos) por **uma** task sequencial de
  `touring index ingest <arquivo>` por arquivo commitado, dedup + teto (64) +
  timeout por chamada (10s). Helper puro `ingest_targets()` testável.
- **a2** `daemon.rs`: `stream.peer_cred()` (tokio, sem dep nova) → `peer_pid`
  no `DaemonRequest` (`#[serde(skip)]`) → linha `heavy op:` ganha
  `peer_pid=<pid> comm=<proc/comm>`. Helper `peer_label()` testável.
- Gate: cargo test nos crates tocados + `update-touring` + prova viva
  (`heavy op` com pid no log do daemon global).

### P2 — (c) isolamento e2e + fim do mascaramento  [deploy: não]
- **c1** 16 arquivos e2e que spawnam `touring` com env herdado passam a usar
  `PrivateDaemon` (padrão `rfc100-b310`), helper compartilhado por `#[path]`
  (test-only; sem ciclo de deps). Injeção de `TOURING_DAEMON_SOCKET`/`_SOCK`
  no helper de spawn de cada arquivo.
- **c2** `.config/nextest.toml` `[profile.ci]`: remover `retries` — CI para de
  converter a classe em flaky-passed.
- Gate: `cargo test --workspace` verde sob o juiz (socket privado) + gate W1
  (`task_1787234572792401084`) converge e arquiva o marker.

### P3 — (b) rebuild cede o ator  [deploy: sim]
- Desenho: `cli_index_rebuild` vira agendador de um `RebuildJob` guardado no
  runtime; `run_project_actor` executa `job.step(budget≈200ms)` entre comandos
  (drena a fila com `try_recv` entre passos); finalize (resolução G3 + sweep +
  commit tantivy) só no último passo; `REBUILD_IN_PROGRESS` cobre o job inteiro.
- Contrato: `touring index rebuild` devolve `{"status":"scheduled",files_total}`;
  `--wait` faz o CLI pollar `index status` (que expõe
  `rebuild:{in_progress,files_done,files_total}`); `init --fast` usa `--wait`.
- Descartado: rebuild em thread própria — segundo `IndexWriter` tantivy colide
  no lock do índice (LockBusy) e quebra single-writer.
- Gate: cargo test + mutants dirigido ao módulo do job + prova viva: `index
  status` responde <100ms DURANTE um rebuild do workspace.

## Fora de escopo (explícito)
Propagação para projetos pinados (`propagate-release.sh`) — gate humano separado.
