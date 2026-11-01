---
type: AuditReport
title: "Cross-audit 23/08/2026 — delta não-commitado do workspace touring"
description: "Auditoria do delta não-commitado (frontmatter OKF adicionado retroativamente em 30/08 — REGRA #21 do censo de audits sem frontmatter)."
tags: [cross-audit, delta, workspace]
timestamp: 2026-08-23T12:00:00-03:00
---

# Cross-Audit 2026-08-23 — delta não-commitado do workspace touring

> **Skill**: TACO-cross-audit (7 fases) · **Escopo**: os 457 paths não commitados
> desde `a8fa5a7` (sessões 18–23/08: biblioteca de hashtags, ADW portfolio,
> scip_ingest, tooling de deploy versionado, espelho client/, move do módulo
> knowledge hooks-core→storage) · **Executor**: TACO / Claude Fable 5 ·
> **Sessão**: e07a20b5 · **Daemon**: touring 30.4.13 (PID 91333)

## VERDICT

**PASS com ressalvas registradas.** 3 defeitos P0/High, 5 P1 e ~10 P2/P3
encontrados nas fases 1–4; **24 correções aplicadas** (17 na fase 5 + 7
descobertas pelos ciclos de gate da fase 6; todas potencializadoras — nenhuma
remoção de capacidade). Gate final 100% verde:
`xaudit-gates-1787519539` exit 0 — check + clippy `-D warnings` (workspace)
+ 151 testes Python + espelho CLEAN + `cargo test` em 8 crates, tudo PASS.
Dívidas remanescentes são legadas, fora do delta, listadas com dono e
recomendação.

## FASE 1 — MAP (executado)

- `git status --porcelain`: 457 paths; delta substantivo ≈ 120 arquivos Rust em
  ~25 crates + 10 scripts Python/bash + espelho `client/` + docs/planos.
- `touring e2e -j` baseline: **0.8343 pass** (warns: orphan rate 36,3%;
  CC alto em `client/omarchy/bin/*`).
- `touring wiring cycles --min-depth 2`: **0 ciclos** (`{"cycle_count":0}`).
- `touring wiring orphans -j`: 2.375 órfãos workspace-wide (63.589 de 77k
  arestas são heurísticas `ast_inferred` — contexto: memória
  "wiring-64pct-heuristico").
- Módulo `knowledge` movido de touring-hooks-core → touring-storage: sem
  referência pendente (cargo check workspace **exit 0**, `/tmp/xaudit_check.log`).

## FASE 2 — PURPOSE (3 exploradores read-only, corpos de teste lidos)

### 2a. Biblioteca de hashtags de memória (tags/codetag/moc)
- Cadeia CLI→daemon→módulo fechada; auto-derive roda nos 2 write paths reais.
- **D1 (P0)**: guard `content.contains("#tags:")` em post_write/post_edit/
  sync_tree impedia o tombstone quando o ÚLTIMO âncora era removido —
  contradiz "código é fonte da verdade" (README + docs/memory-hashtag-library.md:49).
  O teste existente passava porque chamava `sync_file` direto, pulando o guard.
- **D2 (P1)**: `detect_communities(conn, Some(domain))` degenerava em 1
  comunidade completa (o filtro descartava toda tag ≠ `domain:<d>`).
- **D3 (P1)**: recall é federado (todos os `memory.db`), mas o filtro de tag
  era construído só do DB canônico — todo hit cross-projeto era descartado
  silenciosamente por `apply_tag_filter`.
- D4 (P2, registrado): `tag_coverage` mede "≥1 tag", não cobertura por faceta
  como o doc descreve. D5 (P3): `old_source` em post_edit.rs nomeia conteúdo
  pós-edição.
- Órfãos pub P3: `CodetagHit`, `suggest_facet`, `make_link_id`,
  `TagViolation::is_hard`, impls `Display` — registrados (uso interno/teste).

### 2b. scip_ingest.rs (novo, H2)
- Módulo **wired** (lib.rs:40, handler `cli_wiring_scip_ingest`, registry,
  CLI verb, tripwires, docs) — não é órfão. Zero TODO/dead_code.
- **S2 (High)**: `symbol_name` gravava o descriptor SCIP (`a/compute().`);
  rows AST usam o identificador puro (`compute`); o join de
  `orphan_symbols_with_trust` por igualdade de `symbol_name` nunca casava —
  arestas SCIP contavam para ciclos mas **nunca limpavam um órfão**.
- **S1 (P1)**: doc do módulo afirma "scip_resolved ranks above every AST
  origin", mas `WiringOrigin` não tinha variante `ScipResolved` —
  `from_contract_source` degradava para `AstResolved` (0.9 < AstDeclared 1.0).
- S3 (P2, registrado): 40.032 arestas na comissão (log.md H2) → 89 hoje;
  ingest é comando explícito, nada o re-executa — capacidade dormente, sem
  sinal de staleness. S4/S5 (P3): prova real-format `#[ignore]`d; handler CLI
  sem teste de integração.
- S6 (aceito com rationale): `SCIP_CONTRACT_SOURCE` pub consumido só no módulo;
  o literal em `hook-runtime/wiring.rs:277` vive dentro de SQL documentado.

### 2c. Tooling de deploy/espelho
- 7 de 8 invariantes documentados (CLAUDE.md §2.1/2.2/6) implementados e
  citáveis por file:line.
- **T1 (P1)**: `update-touring:67-68` — socket/lock com **uid 1000 fixo**
  enquanto a linha 140 usa `$(id -u)`; em uid ≠ 1000 o resolvedor
  `global_daemon_pid` nunca acha o socket e kill/cleanup/verify agem sobre nada.
- T2 (P2): `propagate-release.sh:246` — `$bin` não-quotado em posição de comando.
- T3 (P2): `test_cli_check_returns_one_when_drifted` só provava o caso exit 0;
  o ramo de falha do gate (o que `propagate-release.sh` consome) tinha
  cobertura zero.
- T4 (P2): gate 1/6 do release pulava em silêncio (pytest.skip) quando o
  symlink `~/.local/bin/update-touring` não existe — exatamente o cenário em
  que o passo 2/6 executaria uma cópia não revisada.
- T5 (P3): teste anti-name-match proibia só `pgrep`, não `pkill`/`killall`/
  `ps|grep`. T6 (P1): `--prune` do sync varria junk em TODO o `client/` —
  incluindo `client/omarchy`, que é FONTE (única cópia), não espelho.
- T7 (P3, registrado): `propagate-release.sh` sem arquivo de teste próprio.

## FASE 3 — DEBT SCAN (executado sobre os 246 arquivos do delta)

- Falsos positivos dominam (o produto MODELA TODOs: `TodoKind`, extração em
  reindex.rs, testes do detector de antipadrões).
- Dívida real encontrada e tratada: `duplication.rs:87 #[allow(dead_code)]`
  em campo nunca lido (`Line.index`); `allow(unused_imports)` justificados em
  post_write/pre_edit (trait em escopo — aceitos com rationale);
  `params.rs #![allow(dead_code)]` module-wide com justificativa documentada
  (schema-gen MCP — aceito).
- `client/skills/Touring/scripts/adw.py:2219/2245` — TODOs são o scaffold
  intencional fail-closed de `adw new` (template para o autor preencher).
- Sentinela `__exit_fail__` nos 13 fragmentos da adw-library: **falso positivo
  descartado por prova** — `adw.py:258-264` documenta que `__exit__`/
  `__exit_fail__` são costuras reescritas no inlining; em spec standalone o
  linter exige `__fail__` (comportamento correto, verificado com
  `touring adw lint`).
- Junk real: 3 dirs `__pycache__` cpython-314 + `.ruff_cache` dentro de
  `client/` (regenerados pela própria execução de testes) — varridos pelo nó
  `clean_mirror` e prevenidos com `PYTHONDONTWRITEBYTECODE=1 -p no:cacheprovider`.
- **Pendência para Gabriel (git)**: `docs/__pycache__/sync_metrics.cpython-312.pyc`
  é um `.pyc` RASTREADO no índice git — requer `git rm --cached` (REGRA #11:
  git é do Gabriel).

## FASE 4 — HARMONY (executado)

- Ciclos: **0**. Órfãos: baseline 2.375 (workspace, majoritariamente heurística).
- 6 P0 BLOCK dims (`touring-quality check --gate F2.1/F2.4/F2.5/F2.6/F4.3/F4.5`)
  sobre os arquivos-chave do delta (scip_ingest.rs, codetag.rs, moc.rs, tags.rs,
  sync-client-skills.py, update-touring): **Pass/N-A, 0 blockers, 0 FAIL**
  (F2.5/F4.5 = NotApplicable em alvo-arquivo; aplicam a projeto Cargo).
- `touring-quality score scripts`: 0.9159 → **0.9311** após fixes; blocker
  F3_11 (README ausente) **fechado** com `scripts/README.md` (seções canônicas).
  Blocker F1_3 remanescente: crates-fixture LEGADAS
  `scripts/touring_premium_refactor_2026/staging/w{5,7,10,12}-*` com duplicação
  intencional de scaffold (wave 2026-05-11) — fora do delta; recomendação:
  arquivar fora de `scripts/` ou excluir fixtures do escopo de score.
- Enrichment honesty (E5b provado por execução): `touring find-code search`
  reporta `backends{keyword:false, embedding:true, vector:false}` + nota
  explícita "vazio NÃO significa ausência no código" — sem fabricação.

## FASE 5 — FIX & POTENTIALIZE (17 correções aplicadas)

| # | Achado | Correção (file:sítio) |
|---|---|---|
| 1 | D1 P0 marker guard | `codetag.rs`: novo `has_snippet_entries()` + guard do `sync_tree` corrigido; `hook_runtime.rs::sync_codetag_anchors` decide sozinho (fast path sem DB novo); hooks post_write/post_edit chamam incondicionalmente |
| 2 | D1 teste pelo caminho real | `codetag.rs::sync_tree_tombstones_file_whose_last_anchor_was_removed` |
| 3 | S1 variante ausente | `knowledge_wiring.rs`: `WiringOrigin::ScipResolved` (as_str/from_contract_source/strength=1.0/is_resolved) |
| 4 | S2 mismatch descriptor | `scip_ingest.rs`: `identifier_of()` reduz descriptor→identificador; asserts atualizados + teste `identifier_extraction_matches_ast_symbol_names` |
| 5 | T1 uid fixo | `update-touring`: socket/lock via `$(id -u)` (com split p/ SC2155) |
| 6 | T1 guard de regressão | `test_update_touring.py::test_no_uid_is_hardcoded_in_runtime_paths` |
| 7 | T2 quote | `propagate-release.sh:246`: `"$bin"` |
| 8 | T3 exit-1 nunca exercido | `test_sync_client_skills.py::test_cli_check_returns_one_when_drifted` agora dirige drift→1 e apply→0 in-process |
| 9 | T4 skip silencioso | `test_update_touring.py`: modo estrito `UPDATE_TOURING_REQUIRE_SYMLINK=1` (skip→fail); exportado no gate 1/6 do `propagate-release.sh` |
| 10 | T5 guard estreito | teste amplia para `pkill|killall|ps\|grep` |
| 11 | T6 prune na fonte | `sync-client-skills.py`: junk sweep + rmdir restritos às ÁREAS espelhadas; teste planta noise em omarchy e assere sobrevivência |
| 12 | D2 slice degenerado | `moc.rs`: membership por `domain:<d>`, arestas de todas as strong facets EXCETO a tag de corte; teste `domain_slice_preserves_internal_structure` |
| 13 | D3 filtro local-only | `cli/memory.rs`: `compute_tag_filter`/`fetch_tagged_entries` federados (mesma lista de DBs do recall; DBs estrangeiros sem `memory_tags` são pulados, nunca mutados) |
| 14 | clippy `chunks_exact` | 7 sítios → `as_chunks::<4>()`: gpu/mod.rs:521, sqlite_vec.rs:61, sqlite_graph.rs:271, embedding_u4.rs:62, pattern_cluster.rs:531, recall.rs:618, ann_memory/persistence.rs:75 (os 4 de touring-intelligence eram mascarados pelo fail-fast do run baseline) |
| 15 | dead field | `duplication.rs`: `Line.index` + `allow(dead_code)` removidos (nunca lido) |
| 16 | F3.11 | `scripts/README.md` criado (seções canônicas; documenta REGRA #19, gotchas de `--version`, espelho gerado) |
| 17 | superfície do módulo | `mod.rs` re-exporta `CODETAG_EXTENSIONS` + `has_snippet_entries` |

Correções nº 1, 12 e 13 fazem o comportamento convergir para o doc (o doc
estava certo); nenhuma correção reduziu escopo (REGRA #0).

## FASE 6 — E2E PROOF (gates determinísticos, ADW `xaudit-gates`)

Runner: `touring adw run xaudit-gates` (spec em `.touring/adw/xaudit-gates.toml`;
nós: clean_mirror → cargo check workspace → clippy workspace `-D warnings` →
pytest (8 suítes, `PYTHONDONTWRITEBYTECODE=1`) → sync --check → cargo test em
8 crates tocados). Logs de evidência: `/tmp/xaudit_{check,clippy,pytest,mirror,tests}.log`.

**Baseline (pré-fix)**: check PASS · clippy FAIL (2 sítios) · pytest FAIL
(1 teste, junk no espelho) · mirror PASS · tests FAIL (nextest ausente —
falha de instrumento, trocado por `cargo test`).

**Run final (pós-fix)** — `xaudit-gates-1787519539-e07a20b5.446864`, **exit 0**:

| Nó | Comando | Veredito |
|---|---|---|
| clean_mirror | sweep `__pycache__/.pytest_cache/.ruff_cache` em client/ | **PASS** |
| check | `cargo check --workspace --all-targets` | **PASS** (exit 0) |
| clippy | `cargo clippy --workspace --all-targets -- -D warnings` | **PASS** (exit 0; único note remanescente: future-incompat de `proc-macro-error2` — dependência externa) |
| pytest_scripts | 8 suítes Python, **151 passed / 0 failed** (37,7 s) | **PASS** |
| mirror_check | `sync-client-skills.py --check` → "CLEAN, 298 arquivos espelhados" | **PASS** |
| rust_tests | `cargo test` em 8 crates (foundation, intelligence, storage, hooks-core, simd, quality, identity, hook-runtime) — 0 falhas, inclui os testes novos D1/D2/S2/T1/T3/T6 | **PASS** |

Foram necessários 6 ciclos: cada `-D warnings` fail-fast revelava um crate por
vez (lição persistida: `lint_failfast_mascara_sitios`); o inventário
warnings-only (`xaudit-lintscan`) fechou a cauda de uma vez — **0 warnings no
workspace inteiro**.

Correções ADICIONAIS descobertas pelos ciclos 3-6 (além da tabela da fase 5):

| # | Achado | Correção |
|---|---|---|
| 18 | `clippy::result_large_err` (2 handlers axum) | `touring-bindings/web/server/mod.rs`: retorno `Result<_, AppError>` no lugar de `axum::response::Result` (ErrorResponse embute Response ≥128 B) |
| 19 | `clippy::result_large_err` (typestate) | `touring-generator/executor/typestate.rs`: `verify`/`speculate` → `Box<ReplanRequest>` (936 B fora do happy path); callers compatíveis via auto-deref |
| 20 | `useless_format` + **caminho morto** | `touring-cli/handlers/mcp.rs:807`: payload apontava hook na árvore CONGELADA `~/.claude/rust` (não existe); agora deriva de `TOURING_WORKSPACE_ROOT` com fallback canônico |
| 21 | flaky `ceg_adapter::observe_only_fails_open_on_malformed_input` | igualdade exata sobre contador global compartilhado → N chamadas + delta < N |
| 22 | flaky `query_cache::invalidate_by_path_increments_counter` | put→invalidate com retry limitado (cache moka é process-global) |
| 23 | flaky `health_delta::non_rust_paths_...` | `pending_len()` before/after → asserção de membership por path no DashMap |
| 24 | teste do espelho conflava disco com espelho | `test_no_generated_artifact_sits_in_the_mirror` escopado às ÁREAS espelhadas — `client/omarchy` é fonte viva de outro workstream que regenera caches mid-suite |

`touring e2e -j` pós-fix: **0.8341 pass** (órfãos 4398 → 4355; warns
remanescentes são os já triados: taxa de órfãos heurística do workspace e os
scripts do workstream omarchy).

## PROVENANCE (instrumentos e suas falhas — provadas antes de acusar o sistema)

- Sandbox CEG matou `cc1` por rlimit ao compilar libsqlite3-sys → cargo roda
  pelos nós do runner ADW, não pelo sandbox (falha de instrumento documentada).
- `cargo nextest` não instalado → `cargo test` (idem).
- `touring exec` só classifica (verdict Allow), não executa — provado com
  probe de side effect ausente.
- Fail-fast do clippy mascarou 4 sítios em touring-intelligence no baseline —
  a varredura por grep do padrão no workspace inteiro achou e fechou todos os 7.
- Meu próprio teste D2 falhou por fixture (`kind` fora de STRONG_FACETS) —
  o fix estava certo; o fixture foi corrigido para `purpose`.

## AÇÕES REMANESCENTES (donos e recomendações)

1. **Gabriel/git**: `git rm --cached docs/__pycache__/sync_metrics.cpython-312.pyc`.
2. **S3**: re-executar `touring wiring scip-ingest` (regenera as ~40k arestas
   type-resolved com o novo `symbol_name` identificador) + considerar gatilho
   periódico/staleness signal. Após o re-ingest, `wiring orphans --trusted`
   passa a se beneficiar do S2 corrigido.
3. **F1_3 legado**: arquivar `scripts/touring_premium_refactor_2026/staging/`
   fora de `scripts/` (fixtures com duplicação intencional rebaixam o score do
   tree inteiro).
4. **client/omarchy**: CC=18 em `cc_build.py` + 4 antipadrões (e2e warns) —
   workstream P0/P1 ativo hoje; não tocado por risco de edição concorrente
   (exceção única da REGRA #21); tratar no fechamento daquele plano.
5. **D4**: alinhar métrica `tag_coverage` (cobertura por faceta) ou corrigir o
   doc. **T7**: testes próprios para `propagate-release.sh`.
6. **S4/S5**: promover a prova real-format do SCIP a job opcional de CI;
   teste de integração do handler `cli_wiring_scip_ingest`.
