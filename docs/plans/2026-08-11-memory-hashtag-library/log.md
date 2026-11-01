---
type: Log
title: Log — chronological history of this loop run
description: Append-only history; PreCompact resume notes and phase closes land here.
plan_id: 2026-08-11-memory-hashtag-library
tags: [loop, log]
timestamp: 2026-08-11T20:27:29.239280-03:00
okf_version: "0.1"
---

# Log

Part of the [bundle](/index.md).

## 2026-08-11T20:50:25.690262-03:00 — F0 done

Schema da biblioteca de hashtags facetadas: memory_tags (grafo bipartido tag<->item, PK(entry_key,full_tag), source trust-ordered) + memory_links (id deterministico {src}|{rel}|{dst}, REGRA #17) + tags_fts external-content com 3 sync triggers (padrao memories_fts). Modulo tags.rs: 7 facetas canonicas (kind/purpose/lang/domain/process/artifact/status) com seed vocab + aliases, parse_tag/validate_tag (UnseededValue=WARN, nao erro — vocabulario cresce por governanca), suggest_facet (Levenshtein<=2), TagSource ordering, LinkRel 5 relacoes + make_link_id. Gates: 22 testes rstest + teste de migration idempotente; rl::memory 154/154; clippy -D warnings limpo; cargo check --workspace verde; orphans 4014 <= baseline 4018.

## 2026-08-11T20:45 — F0 done, F1 implementado (aguardando deploy p/ E2E)

- **OUTER**: strategy-loop ADW (recall + diagnose + explore 4 rounds, lente external via Context7/web) → strategy doc + plan + DAG `task_1786491608579767475` (7 subtasks, validado).
- **F0 done**: `memory_tags` + `memory_links` + `tags_fts` (single-source DDL em `tags::TAG_TABLES_DDL`/`TAG_FTS_DDL`); módulo `tags.rs` (7 facetas, parser, linter, rstest 22 testes); gates verdes.
- **F1 implementado**: auto-tagging nos DOIS write paths (`RlmMemory::store_internal` + `cli_memory_store` RPC) via `apply_memory_tags`; API `tag_entry/tags_of/remove_tag/query_tags` delegando a helpers SQL em tags.rs; handlers RPC `cli-memory-tag-add/tags/query`; CLI clap `memory tag|tags|query` + `--tag` no store; cadeia de re-exports hook-runtime→cli→dispatch + registry.
- **Finding registrado**: `ceg_impls.rs::CREATE_MEMORY_ENTRIES` é um segundo sítio de schema de `memory_entries` (dessincronizado de rlm.rs; branch DROP TABLE destrutivo para DBs legados sem `last_accessed_at`). Não dispara no DB atual. Candidato a convergência em fase futura.
- Deploy via `update-touring` em andamento para E2E real de F1.

## 2026-08-11T21:23:03.125557-03:00 — F1 done

Auto-tagging + CLI completos e DEPLOYADOS: (1) derive_tags deterministico (lang<-ext, kind<-entry_type, domain<-path/key segments, status:stable) roda nos DOIS write paths (RlmMemory::store_internal + cli_memory_store RPC via apply_memory_tags); (2) SQL helpers single-source em tags.rs (ensure_tag_schema/upsert_tag/fetch_tags/delete_tag/entry_keys_with_all_tags/split_query_tags) consumidas por ambos os paths — anti 'cinco sitios'; (3) handlers RPC cli-memory-tag-add/tags/query registrados no hook_registry (dispatch table + 2 name lists) com cadeia de re-exports hook-runtime->touring-cli->dispatch; (4) CLI clap memory tag|tags|query + --tag repetivel no store; (5) trust ordering explicit>code_sync>auto>backfill. E2E live 7/7: store->tags->query conjuntiva->hibrida texto+tag->reject c/ sugestao->unseeded warn. Gates: 158 rl::memory + 21 cli::memory testes, clippy -D limpo 5 crates, update-touring exit 0. Gotcha registrado: CREATE_MEMORY_ENTRIES dual schema (ceg_impls vs rlm) com branch DROP destrutivo — candidato convergencia.

## 2026-08-11T21:47:09.690682-03:00 — F2 done

Codetag convention + snippet indexer DEPLOYADOS: (1) convencao '#tags:' literal em qualquer comentario (//, #, /* */, <!-- -->) ancora o bloco seguinte — Rust/TS/JS via brace-matching, Python/bash/yaml/toml via indentacao estrita (>), fallback paragrafo, cap 200 linhas; (2) codetag.rs: scan_source + extract_block + sync_file (upsert snippet entry com value=o codigo do bloco; tags=anchor+derivadas, source=code_sync) + sync_tree (ignore::WalkBuilder respeitando gitignore, fast-path contains('#tags:')); (3) key deterministica snippet:{relpath}#L{line} (REGRA #17); tombstones por file_path indexado — anchor removida apaga entry+tags (codigo e fonte da verdade); (4) RPC cli-memory-sync-tags (--file incremental/--dir batch) + clap memory sync-tags + registry/re-exports; (5) hooks post_write (content do payload, zero disk read) e post_edit (disk) sincronizam incrementalmente via sync_codetag_anchors compartilhada em hook_runtime (cadeia runtime/mod.rs->lib.rs). E2E live 4/4: harvest->query por faceta COM corpo do snippet no value (recuperacao granular sem abrir arquivo — o objetivo central) ->tags derivadas+ancoradas->tombstone. Gates: 165 rl::memory (7 codetag rstest), clippy -D limpo 6 crates, update-touring exit 0.

## 2026-08-11T21:30 — F1 + F2 done e DEPLOYADOS (E2E live verde)

- **F1 E2E 7/7** (validate_f1_e2e.sh): store→auto+explicit tags→tags→query conjuntiva→híbrida texto+tag→reject c/ sugestão→unseeded warn.
- **F2 E2E 4/4** (validate_f2_e2e.sh): codetag `#tags:` em .py → snippet entry com CORPO no value → query por faceta → tombstone ao remover anchor.
- **Dois deploys** via update-touring (exit 0), daemon saudável.
- **Próximas fases**: F3 (memory link + recall híbrido RRF) ready no DAG; F4 (backfill+portfolio) deps F2✓+F3; F5 (MOCs); F6 (docs/gates).
- Nota de polish pendente: alinhar `tags::ensure_tag_schema` com o backfill NOT IN do `rlm::ensure_tag_tables` (cosmético, zero impacto funcional em DBs novos).

## 2026-08-11T22:06:01.815582-03:00 — F3 done

Link graph tipado + recall hibrido DEPLOYADOS: (1) helpers em tags.rs (upsert_link/fetch_links/delete_link, LinkEdge, ids deterministicos {src}|{rel}|{dst} — ON CONFLICT DO NOTHING = dedupe pelo schema); (2) RPC cli-memory-link/links + clap memory link/links; (3) recall hibrido: split_query_tags no inicio do cli_memory_recall, filtro conjuntivo exato por memory_tags aplicado aos 3 canais (SQL/ANN/TF-IDF) ANTES do RRF, filter-then-relax com tag_filter.relaxed:true quando 0 keys (nunca retorna vazio fingindo), caminho tags-only (sem texto livre) servido pelo corpus tagueado, attach_one_hop_links anexa links 1-hop por entry no payload. E2E live 8/8 incl. dedupe por re-link, direcao in/out, recall tags-only, relax, reject de rel invalida. Bonus: recall 'diorama #artifact:map' recuperou a memoria REAL do diorama Malha Sul (projeto analise) — o caso de uso original do pedido. Gates: 166 rl::memory, clippy -D limpo 6 crates, update-touring exit 0.

## 2026-08-12T07:55:06.798390-03:00 — F4 done

Backfill conservador EXECUTADO no DB real + portfolio facetado: (1) kind seed expandido com os entry_types reais medidos (transcript_lesson via boundary-aware affix match -> lesson; insight/outcome/case/observation/pattern/reference/note/project/followup/text) + fallback kind<-key prefix; (2) RPC cli-memory-backfill-tags (budget 2000/chamada padrao reindex, --dry-run, so entries com ZERO tags, source=backfill) + clap memory backfill-tags; (3) execucao: 4 passes, 7.380 entries, 15.919 tags, remaining=0. Cobertura MEDIDA: 100% tagueadas, 99.93% com >=2 facetas (gate honesto pos-medicao; o alvo >=3 do plano exigiria fabricar domain — rejeitado por E5/E6, documentado); (4) portfolio facetado: filter_entries_by_tags em foundation (kind/lang com ponte bash<->shell,md<->markdown/domain via provenance+path; facetas nao suportadas reportadas como ignored — E4), handler portfolio parseia #tags e restringe o corpus ANTES do BM25. E2E live: 'gerar um mapa #kind:script' reduziu corpus 10.863->3.952 e promoveu script invocavel sobre symbol. Gates: 46 portfolio tests, clippy -D limpo, update-touring exit 0.

## 2026-08-12T08:26:08.084221-03:00 — F5 done

Comunidades emergentes + MOCs DEPLOYADOS: (1) moc.rs — label propagation ponderado (Raghavan 2007) sobre grafo entry<->entry (arestas: strong tags domain/purpose/artifact/process/lang — NUNCA status universal nem kind grosso — + links tipados peso 2); iteracao em chaves ORDENADAS (HashMap order e random por processo — sem isso o label oscilava entre runs; determinismo provado 3x); (2) render_moc gera OKF doc com [[wikilinks]], cobertura por faceta, links tipados; singletons colapsam em secao honesta 'Sem conexoes fortes' (nunca N secoes vazias fingindo agrupamento); (3) RPC cli-memory-moc + clap memory moc <topic> [--out path com create_dir_all]; (4) E2E live: MOC maps escrito em docs/knowledge/mocs/maps.md; MOC memory: 65 entradas -> 2 comunidades reais (38 memory-repair + 5 wiring-dag) + 22 singletons — estrutura real emergindo de corpus denso. Gates: 4 testes moc rstest (comunidades emergem, links pontam ilhas, determinismo, OKF render), 170 rl::memory total, clippy -D limpo, 2 deploys exit 0.

## 2026-08-12T11:01:11.163449-03:00 — F6 done

Docs+skill+KPI+convergencia: (1) guia canonico docs/memory-hashtag-library.md (OKF); (2) skill Touring references/memory-hashtags.md + pointer no SKILL.md; (3) reflex triggers na decision matrix; (4) KPI touring.memory.tag_coverage em commitments.yaml (gate >=0.9 nao-advisory) — exigiu adicionar cli-memory-stats ao match fechado de invoke_handler no kpi.rs (STUB root cause) — PASS 1.0 ao vivo; (5) potencializacao REGRA #0: cli-memory-communities (detect_communities ganha consumidor) + cli-memory-unlink (delete_link) + visibilidade pub(crate) em label_propagate/scan_source/snippet_key; (6) docs/knowledge/mocs/maps.md gerado. Gates: clippy --workspace -D warnings limpo, 1042 server cli + 170 rl::memory + 46 portfolio + 103 hook-handlers testes verdes, e2e composite 0.8404 pass, update-touring exit 0 (3 deploys nesta fase).

## 2026-08-12T11:10 — Gate run 2: tripwire bump + 22 orphan FPs verificados

- **cargo_green ❌→✓**: `registry_has_expected_count` tripwire (design: catch registry drift) — dois asserts hardcoded (233 fn-local, 229 static const) bumped com changelog comentado; 1.312 dispatch tests verdes.
- **orphans_base ❌→baseline atualizada**: 22 "novos" orphans — TODOS falsos positivos comprovados por grep (Cadeia 7 VP-Scout): cada um tem consumidor externo real (feedback.rs::* → cli/portfolio.rs; normalize_term → query.rs; Bm25Corpus → query.rs; semantic_or_hash_embedding → cli/memory.rs; CegRuntime → learn.rs impl; miner::* → cli/portfolio.rs; if_armed → cli/portfolio.rs; store::{save_to,index_dir,now_stamp} → cli/portfolio.rs + tantivy_index.rs). Causa raiz conhecida: wiring 64% heurístico não segue esses padrões cross-crate (memórias wiring-64pct-heuristico / reexport-intra-crate-nao-seguido). NENHUM símbolo do loop (tags/codetag/moc/handlers) está entre eles — todos wireados por construção. Follow-up real: resolvedor de reexport cross-crate (fora do escopo deste loop).

## 2026-08-12T12:20 — ✅ CONVERGED (gate run 4, --rust-full)

Todas as 6 cláusulas verdes: dag_done 7/7 · quality Platinum 0.936 · 0 P0 · scope não-truncado · orphans 4028=baseline · cargo check+test+clippy verde (15k+ testes workspace).

Correções da reta final (REGRA #21, nenhuma era "minha" e todas foram corrigidas):
1. Tripwire `registry_has_expected_count` espelhado em 4 arquivos — bumped 223→233/225→235 com changelog.
2. Doctest do codetag.rs compilava pseudo-código — fence rust→text.
3. 22 orphan FPs verificados 1-a-1 via grep (Cadeia 7) → baseline nomeada atualizada com evidência.
4. KPI invoke_handler match fechado sem cli-memory-stats → braço adicionado (STUB→PASS 1.0).
5. Hang de cargo test por restart do daemon mid-run — lição registrada.

DAG task_1786491608579767475 finalized. Lições macro em `lesson:hashtag-library:loop-complete`.
