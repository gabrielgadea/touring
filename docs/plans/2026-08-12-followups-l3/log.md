---
type: Log
title: Log — chronological history of this loop run
description: Append-only history; PreCompact resume notes and phase closes land here.
plan_id: 2026-08-12-followups-l3
tags: [loop, log]
timestamp: 2026-08-12T17:05:46.617813-03:00
okf_version: "0.1"
---

# Log

Part of the [bundle](/index.md).

## 2026-08-12T17:18:23.284104-03:00 — H0 done

Censo completo: (1) FORKS — 3 pares DIFFER, resilience=canônico (let-chains modernos), foundation=cópia antiga com mais comentários; único consumer dos forks foundation é interno (failover/mod.rs); gate_metrics.rs da foundation já consome resilience::sentinel; (2) WRITE PATHS memory_entries — só 2 production full-row (rlm.rs + ceg_impls RPC); demais são UPDATEs manutenção (outcome_reward/access_count), fixtures cfg(test) e migration tooling (consolidation.rs, migrate.rs); (3) ENV — 40 arquivos, 33 sem ENV_LOCK: ~31 testes (inline cfg(test) + tests/ integration) + 2 candidatos produção (recursion_guard.rs, rollback_plan.rs) p/ auditar; (4) rust-analyzer 1.97.1 disponível com subcomando scip.

## 2026-08-12T18:24:32.257279-03:00 — H3 done

Env-lock sistêmico COMPLETO: censo 40 arquivos com set_var/remove_var → 38 com fns mutantes analisadas; 24 desprotegidos → todos serializados (hooks-core via #[serial] no domínio existente; touring-server reusando crate::cli::ENV_LOCK; 5 src files + 6 integration binaries com ENV_LOCK file-local documentado; hooks-shared user_filters com domínio disjoint documentado); 2 sítios de PRODUÇÃO (recursion_guard HookGuard, rollback_plan RestoreEnv) marcados AUDITED com justificativa (process-global por design, single-threaded main path); 126 TODOs 'Audit that the environment access' → 0 (todos AUDITED c/ guarda verificada); capnp_embed deixado tolerante por decisão documentada do autor. Suíte 12 crates: 3.660+ testes verdes. Achados processuais: (1) cargo check standalone deu FALSO PASS em módulo all(test,feature=post-hooks) — verificar com o feature set do dependente; (2) inserção de static após bloco de doc-comments orfa a doc (deny(missing_docs) pegou); (3) ls -la|tail esconde arquivo novo (ordenação alfabética) — gate determinístico é o verificador.

## 2026-08-12T18:33:06.365642-03:00 — H1 done

Unificação RPC→RlmMemory COMPLETA e DEPLOYADA: (1) MEMORY_ENTRIES_DDL canônico single-source (key-only PK — REGRA #17, 4 DBs de produção 29.190 rows JÁ eram key-only: migração de dados em produção = zero, provado por probe); (2) migrate_composite_pk: rebuild transacional de tabelas RLM-legadas com dedupe determinístico (latest accessed_at vence) + conversão epoch→TEXT datetime; (3) RlmMemory::store_rich(RichMemoryEntry) com semântica de conflito pinada: created_at first-write-wins, access_count incrementa, importance sticky COALESCE, outcome per-write NULL-honest, supersedes retira sem deletar, auto-tags + explicit_tags; (4) handler RPC cli_memory_store delega (parser extraído p/ CC), SQL inline removido, CREATE_MEMORY_ENTRIES/apply_memory_tags mortos removidos; (5) 3 readers tipados i64→cell_to_epoch (InvalidColumnType silenciado virava lista vazia em query_tags — E4); (6) maintain_tiers: 3 sítios de aritmética created_at com coerção typeof→epoch (r7_rlm_maintain_tiers prova). Testes: parity G6 vs SQL legado, migração composite-PK, conflito pinado, supersede — 178/178 rl::memory + 24/24 e2e_r1_to_r20 + e2e cli_handlers verde (os 2 testes que falhavam por NOT NULL created_at agora passam — a falha era o dual schema ao vivo). E2E live pós-deploy: store→row canônico TEXT→re-store access_count=2→recall 17 hits.

## 2026-08-12T22:27:54.006003-03:00 — H4 done

Fork knowledge/ hooks-core↔storage CONSOLIDADO: a consolidação já estava 90% feita (W72: hooks-core lib.rs tem 'pub use touring_storage::knowledge' desde então) — os 11 arquivos em hooks-core/src/knowledge{,.rs/} eram MORTOS em disco (fora da module tree), ferramenta de análise lia o fork stale (schema sem wiring_unresolved, sem fix OR IGNORE 2026-08-07, sem fixture V8→V9) como se fosse vivo. Censo de divergência: 6 arquivos byte-idênticos (analytics/bash/core/edits/metadata/query), 4 divergentes com storage SEMPRE mais novo (gotchas let-chains, schema +wiring_unresolved, tests +fixture drift V8→V9, models doc-link), pub surface storage ⊃ hooks-core (único delta: top_error_patterns pub vs pub(crate) — widened, harmless). Ação: 11 arquivos mortos deletados (knowledge.rs + knowledge/ inteiro); único teste afetado (deep_module_path_keeps_every_segment) agora pinna comportamento MELHOR: resolver segue o re-export W72 e aterrissa em touring-storage/src/knowledge/models.rs canônico (antes: aterrissava no fork morto — o bug invisível). 474/474 hooks-core verde. Lição: 'fork' era na verdade débito de arquivo-morto pós-consolidação — o risco real era edição/análise do arquivo errado; sweep de arquivos-órfãos-de-mod-tree fica como follow-up.

## 2026-08-12T23:02:31.573019-03:00 — H2 done

SCIP type-aware wiring COMPLETO + LIVE: (1) scip_ingest.rs (hook-runtime, prost workspace) — decode wire-compatible real scip.proto (tags corrigidos via raw protobuf walk: Document.occurrences=2 não 3 — o erro 'symbol_roles invalid wire type' era mis-tag lendo SymbolInformation como occurrences; roles=varint singular); def-map first-wins determinístico (110k dup defs = re-emissão por target cargo — 674 conflitos de 53.404 símbolos auditados: crate-root + locals-skip, benignos); dedupe (producer,consumer,symbol) p/ idx_wiring_unique; idempotente delete+insert transacional. (2) 'touring wiring scip-ingest [--file]' + '--trusted' em cycles/orphans (contract_source != ast_inferred). (3) LIVE: 104MB index, 1.663 docs, 163.509 defs, 624.666 refs → 40.032 arestas scip_resolved em 0,62s; 828 consumers que ast_resolved nunca viu. (4) Medição final: default 1 mega-SCC (SCIP fundiu os 7 antigos — 917-giant absorveu os pequenos); --trusted 29 SCCs REAIS: 28 intra-crate (file-level, Rust legal — informativo) + 1 cross-crate (686 arquivos, via dev-deps/test paths — nota arquitetural honesta). Phantom cycles (fork/homônimos) eliminados do view trusted por construção. Tripwires 233→234/235→236 + mirror files + ALL_DAEMON_HOOK_NAMES 229→230. Prova anti-circular: decode_real_rust_analyzer_index (#[ignore], bytes reais).
