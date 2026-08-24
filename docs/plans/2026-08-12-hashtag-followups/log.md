---
type: Log
title: "Log — Follow-ups da cross-audit 2026-08-12"
description: "Cronologia do loop que fechou os 6 follow-ups da auditoria da hashtag library."
plan_id: 2026-08-12-hashtag-followups
tags: [loop, log, followups]
timestamp: 2026-08-12T16:30:00-03:00
okf_version: "0.1"
---

# Log — follow-ups cross-audit 2026-08-12

Part of the [bundle](/index.md). Estratégia: [strategy-2026-08-12-hashtag-followups](/strategy-2026-08-12-hashtag-followups.md).

## 2026-08-12T15:00 — G0 done

Rebuild full (3.186 arquivos, 68.214 símbolos, 138s) — call-sites históricos backfilled. Cycle registry↔daemon SUMIU (fantasma de grafo stale — F-6 fechado por evidência). Orphans subiram para 4.064 com os novos concentrados nos arquivos do loop → diagnóstico do déficit do resolvedor (ordem do walk + tipos nunca wireados).

## 2026-08-12T15:40 — G1 done

`extract_symbols_fallback` persiste defs+calls no symbol_store (antes: só knowledge). Teste `fallback_writes_defs_and_calls_to_store`.

## 2026-08-12T16:00 — G3 done (o resolvedor de wiring)

3 eixos: ordem-independência (pass global pós-walk), type/const refs (novo extrator + lookup por kinds), crate-locality (same-crate primeiro no cap-4). **Orphans 4.028→2.224 (-45%)**. Ciclos: 7 restantes, todos provadamente heurísticos (grep=0 imports cruzados); mega-ciclo 803 eliminado; giant-SCC inerente ao name-matching documentado → rota type-aware = SCIP (feature existe).

## 2026-08-12T16:15 — G2 done (dual schema)

DROP destrutivo removido (migração aditiva; o teste pegou um segundo bug latente — SQLite proíbe NOT NULL+DEFAULT não-constante em ALTER — e prova o fix: row legado sobrevive). CREATE_MEMORY_ENTRIES = picture completa (16 colunas). Unificação total RPC→RlmMemory registrada como proposta L3 (decisão de PK + migração 7.4k rows) — não é mais risco de dados.

## 2026-08-12T16:30 — G4 done

6 TODOs unsafe-env auditados: ENV_LOCK crate-wide no server, toolchain.rs envolvido, inferlet no lock compartilhado, teste do reindex com lock. Finding novo: 12+ arquivos mutam env em testes sem lock (sistêmico, fora do escopo dos TODOs) — candidato a loop de test-hygiene.

## 2026-08-12T16:45 — G5: gates

clippy --workspace -D warnings limpo · cargo test --workspace RC=0 (0 FAILED) · quality Platinum 0.936 · orphans baseline 2.228.

## 2026-08-12T16:30:38.113342-03:00 — G5 done

Gates finais do loop de follow-ups: clippy --workspace -D warnings LIMPO; cargo test --workspace RC=0 com 0 FAILED (suite completa, sem cap de tempo); quality Platinum 0.936; orphans baseline 2.228 (resolvedor G3: -45% vs pre-loop); 0 P0. Todos os 6 follow-ups da cross-audit-2026-08-12 fechados: F-1 fallback persiste no store; F-2 rebuild full executado; F-3 DROP destrutivo eliminado + schema alinhado (unificacao write-path registrada como proposta L3 com analise de risco); F-4 mega-ciclo diagnosticado heuristico + reduzido pelo resolvedor (rota type-aware SCIP registrada); F-5 6 TODOs env auditados com lock crate-wide; F-6 cycle registry-daemon sumiu no rebuild (fantasma stale).

## 2026-08-12T16:51:46.278331-03:00 — G5 done

prova de integracao ADW<->hashtag library

## 2026-08-12T17:00 — Release v30.4.0 propagada + skills/CLAUDE.mds + integração ADW

- **Bump**: workspace `30.3.1 → 30.4.0` (minor: hashtag library é feature nova backward-compat + fixes).
- **Propagação**: `propagate-release.sh 30.4.0` — toolchain install (snapshot imutável) → default → `touring update` por projeto: analise, transferegov_pipeline, konverter (fonte fica em dev). **Verificação por comando novo** (memória propagacao-rotulo-nao-prova-build): `memory communities` (só existe na 30.4) responde JSON válido nos 3 projetos — o binário novo roda de fato.
- **Skills**: `touring-cli-index.md` (seção MEMORY HASHTAGS no cheatsheet) · loop-engineering (seção "Hashtag library — output catalogued by construction") · TACO-cross-audit (facet discovery no MAP + validators vivos no bundle).
- **CLAUDE.md**: touring workspace (regra 6) + konverter/analise/transferegov_pipeline (bloco na seção Touring Integration).
- **Integração ADW verificada**: (leitura) nós recall dos specs são facet-aware por construção (backward-compat sem `#`); (escrita) `loop_phase_close.py` ganhou `--tag` repetível — prova: fase gravou `domain:adw process:converge` explícitas + `kind:lesson status:stable` auto-derivadas.

## 2026-08-12T17:15 — Backfill nos projetos pinados (21.747 memórias → 100%)

- analise 16.005 → 100% · transferegov_pipeline 121 → 100% · konverter 5.621 → 100% (8/1/3 passes, remaining=0).
- Prova funcional: `#kind:lesson` 10 hits/projeto; híbrida `diorama #kind:lesson` no analise → lesson do diorama K2 real.
- **Finding**: FTS5 `memories_fts` do analise em drift (missing row) — legado de writes pré-trigger; rebuild FTS5 aplicado nos 3; gotcha `fts5-external-content-drift` registrado.

## 2026-08-12T16:59:34.252903-03:00 — PreCompact snapshot

Loop active. Pending: []. Resume: `touring decompose ready task_1786557551597753376`.
