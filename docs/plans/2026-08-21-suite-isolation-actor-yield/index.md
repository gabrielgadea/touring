---
type: LoopBundle
title: "Fix de classe da flakiness de suíte — S-4 + peer-cred · isolamento e2e · rebuild cede o ator"
description: "Bundle OKF do loop que elimina a classe de starvation do ator single-thread que mantinha cargo_green vermelho: P1 (origem + observabilidade), P2 (isolamento e2e + fim do mascaramento), P3 (rebuild reentrante)."
plan_id: 2026-08-21-suite-isolation-actor-yield
okf_version: 1
tags: [loop, daemon, starvation, e2e, nextest, generator, actor]
timestamp: 2026-08-22T01:00:00-03:00
---

# Bundle — suite isolation + actor yield

DAG: `task_1787365388353596293` · escopo: `/home/gabrielgadea/projects/touring`
Origem do diagnóstico: `/docs/plans/2026-08-20-work-outer/diagnostics/2026-08-21-cargo-green-starvation.md`

## Documentos

| Doc | O quê |
|---|---|
| [Estratégia](/strategy-2026-08-21-suite-isolation-actor-yield.md) | as 3 fases, ordem por custo-benefício, escopo e não-escopo |
| [Design P3](/design-p3-rebuild-job.md) | `RebuildJob`, alternativa descartada, prior art, como se prova |
| [Diagnóstico OUTER](/diagnostics/touring-20260821T231940.md) | varredura determinística do `strategy-loop` |
| [Log](/log.md) | histórico cronológico da execução |

## Fases

| Fase | Estado | Entrega |
|---|---|---|
| [P1a1](/phases/P1a1.md) | done | S-4 do generator: `rebuild --dir` por diretório → `index ingest` por arquivo (dedup + teto 64 + timeout 10 s) |
| [P1a2](/phases/P1a2.md) | done | `SO_PEERCRED` → `peer_pid` → `heavy op: … peer=pid=<n> comm=<x>` |
| [P1gate](/phases/P1gate.md) | done | 6 testes, clippy, 2× `update-touring`, prova viva do `peer=` |
| [P2c1](/phases/P2c1.md) | done | 13 e2e com `PrivateDaemon` por processo (34 spawns) — 116 testes verdes |
| [P2c2](/phases/P2c2.md) | done | `nextest`: `retries` 3→0 (ci) e 2→0 (default) |
| P2gate | em curso | `cargo test --workspace` + clippy sob daemon privado |
| P3b1/P3b2/P3gate | pendente | `RebuildJob` + `--wait` + prova viva (`index status` <100 ms durante rebuild) |
