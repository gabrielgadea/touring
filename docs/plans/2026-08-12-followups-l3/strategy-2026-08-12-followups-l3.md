---
type: Strategy
title: "Follow-ups L3 — unificação store_rich, SCIP type-aware, env-lock sistêmico, consolidação de forks"
description: "Estratégia para os 4 follow-ups L3 nomeados por Gabriel: (1) unificação RPC→RlmMemory via store_rich com análise de risco; (2) wiring type-aware via SCIP/rust-analyzer eliminando o giant-SCC por construção; (3) test-hygiene env-lock em todos os arquivos; (4) consolidação foundation↔resilience (ciclos depth-2)."
plan_id: 2026-08-12-followups-l3
tags: [loop, strategy, memory, wiring, scip, test-hygiene, consolidation]
timestamp: 2026-08-12T17:15:00-03:00
okf_version: "0.1"
---

# Estratégia — Follow-ups L3 (todos, com prova)

Part of the [bundle](/index.md). Origem: pedido explícito de Gabriel (2026-08-12) sobre os follow-ups registrados no [loop anterior](/../2026-08-12-hashtag-followups/log.md).

## Estado medido (evidência, não inferência)

| Sinal | Valor | Fonte |
|---|---|---|
| Ciclos wiring | 7 (#1 hook-handlers depth-3; **#2/#3/#4 forks foundation↔resilience depth-2**; #5 bidirectional; #6 mcp_overhead server↔hooks-shared; **#7 giant-SCC 928 módulos**) | `touring wiring cycles --min-depth 2` |
| Arquivos com `set_var`/`remove_var` | **40**, ~20+ sem ENV_LOCK | grep workspace |
| SCIP hoje | `scip_emit.rs` é **exporter** (touring→IDE), 416 LOC, feature `scip-emit` com prost+prost-types | leitura do módulo |
| Dual schema | CREATE (ceg_impls) vs rlm — divergência residual de write path (DROP destrutivo já removido no loop anterior) | gotcha `ceg-impls-dual-memory-schema` |
| Forks | resilience é o lar canônico ("peeled A4 P3 2026-06-15" nos Cargo.tomls); foundation ainda tem `sentinel/memory/{meminfo,psi}.rs` + `failover/impl_vector_store.rs` físicos | Cargo.tomls + cycles |

## As 5 fases

| # | Fase | Conteúdo | Depende de |
|---|---|---|---|
| **H0** | Censo (read-only, paralelo) | diff exato dos 3 pares de forks; inventário de TODOS os write paths de `memory_entries`; classificação teste-vs-produção dos 40 arquivos env; `rust-analyzer` availability | — |
| **H3** | Env-lock sistêmico | todo `set_var`/`remove_var` em `#[cfg(test)]` serializado via ENV_LOCK (padrão crate-local já estabelecido em touring-server); produção single-threaded → nota AUDITED | H0 |
| **H1** | Unificação store_rich | `RlmMemory::store_rich(RichMemoryEntry)` com semântica G6 exata (COALESCE access_count, NULL discipline); handler RPC **delega**; **teste de paridade byte-a-byte ANTES do deploy**; produção (7.4k+21.7k rows) intocada — migração é de código, não de dados | H0 |
| **H2** | SCIP type-aware | `rust-analyzer scip` → parse prost (deps existem) → arestas `origin='ScipResolved'` (trust > AstInferred; heurísticas preservadas — REGRA #0) → `wiring cycles/orphans --origin resolved`; comando opt-in `touring wiring scip-ingest` (nunca hot path); medir colapso do giant-SCC | H0 |
| **H4** | Consolidação forks | foundation vira re-export canônico de resilience (API pública preservada — REGRA #0); ciclos #2/#3/#4 eliminados por construção; consumers provados por grep + (se H2 pronto) arestas resolved | H0 (+H2 ideal) |

Ordem: **H0 → H3 → H1 → H2 → H4**. H3 primeiro porque destrava suites confiáveis que as outras fases executam; H4 por último porque suas provas ficam mais fortes com arestas resolved.

## Riscos e mitigações

- **H1 paridade de defaults** (importance/outcome/access_count em conflito) → teste byte-parity (2 DBs, mesma entry pelos 2 caminhos, rows idênticas) é gate de deploy, não opcional.
- **H2 wall-clock** do rust-analyzer num workspace de 30 crates → ingest é comando opt-in, nunca em hook; mapeador symbol SCIP → producer_file ancorado em `document.relative_path` das definitions (role=1).
- **H4 quebrar API pública** de foundation → deduplicação via `pub use` re-export (paths públicos idênticos); feature gates (`resource-monitor-sysinfo`) respeitados.
- **H3 tocar 20+ arquivos** → pattern mecânico único; suites verdes são o gate.

## Convergência (medida)

`loop_converged.py --rust-full` exit 0 + cláusulas específicas: (a) teste paridade G6 verde; (b) `wiring cycles --origin resolved` sem giant-SCC; (c) zero arquivos sentinel/failover duplicados (re-export puro ou diff vazio); (d) censo env 100% classificado, 0 test-env sem lock; (e) orphans ≤ baseline (REGRA #0).
