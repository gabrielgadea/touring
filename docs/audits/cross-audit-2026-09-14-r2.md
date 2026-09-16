# Cross-audit 14/09/2026, rodada 2: o que a rodada 1 corrigiu e o que o ambiente revelou

> **Estado: pausa humana cumprida (14/09, ~19:45). Gabriel aprovou os lotes L1-L6, a variante `.gitignore` no L2 e a release 30.4.48 com aviso às sessões. Fase 5 em andamento.**
> **Registro original: fases 1-4 concluídas; PAUSA HUMANA antes da fase 5.** Nenhuma correção de código aplicada nesta rodada. A única ação feita foi truncar o log de 25,5 GB, autorizada por Gabriel e com amostra preservada. Rodada 1: `docs/audits/cross-audit-2026-09-14.md`.

## Escopo

Tudo o que foi implementado desde a abertura da fase 5 da rodada 1, levantado por mtime (≥ 15h de 14/09) e não pela memória do autor. São 51 arquivos no repo e 5 scripts vivos em `~/.claude/skills`:
- as correções dos lotes 1-3;
- o scanner textual reescrito;
- o extrator de tipos (B12 e declaração);
- o `update-touring` (verify e resolvedor);
- o pragma no `run_bench.py`;
- a release 30.4.47 propagada.

## Método

O defeito estrutural da rodada 1 era o autor julgar o próprio código com mutantes que ele mesmo escolheu. Por isso esta rodada teve:
- **quatro críticos** em sessões novas, só leitura e sem cargo, com lentes distintas: (A) daemon, ator e processo; (B) wiring, índice e tantivy; (C) touring-quality, segurança e integridade da prova; (D) docs e contrato texto-executor;
- **o orquestrador** investigando em paralelo e depois **reproduzindo cada achado dos críticos**. Nenhum achado entra aqui só pela palavra do crítico.

**Aviso de validade.** Das ~17:43 até pouco antes das 19:34, o Bash desta sessão, o dos quatro críticos inclusive, saiu com exit 1 em qualquer comando, e o `touring` chamado dali saiu com 134. Os críticos trabalharam quase só com leitura de arquivo, marcaram muito como `UNVERIFIED` e produziram falsos positivos de caminho. O orquestrador seguiu pelo sandbox do daemon (`touring_ctx_execute`), que roda noutro cgroup. A causa da queda **não foi provada**. A hipótese do cgroup carregado pelo tmpfs foi refutada: o shell voltou antes da truncagem do log.

## Juiz de convergência (17:35-17:45)

`loop_converged.py --task task_1789406779265539522 ... --rust-full` saiu com **rc=1** (`target/audit-2026-09-14/judge.log`):

| Cláusula | Resultado | Leitura |
|---|---|---|
| judge_intact | ok (advisory: file_changed) | — |
| dag_done | 4/11 | contabilidade do DAG pendente |
| quality_gold | Unranked 0,875, blocker F2_1 | R2-2 (cópia velha em `site/`) |
| no_p0_fail | F2_1 | idem |
| orphans_base | 1.607 contra 1.560 | R2-5 e R2-6 |
| cargo_green | `inferlets serialized::tests::test_file_save_load` | R2-1 (/tmp cheio) e R2-8 (caminho fixo) |

## Achados verificados

Coluna "Prova": **R** = reproduzido por comando nesta rodada; **L** = conferido por leitura de código com file:linha.

### P1

| ID | Achado | Causa-raiz | Prova |
|---|---|---|---|
| R2-1 | **O watcher do MCP server encheu o `/tmp`.** 25,5 GB de `ERROR ... File event channel full, dropping event` entre 09:23 e 20:42 UTC, a 5-11 MB/s durante cada build. O tmpfs chegou a 100% às 17:42, e daí vieram arquivos de 0 bytes: o estado do circuit breaker (`/tmp/touring-circuit-1000.tmp`) e o arquivo que reprovou o teste do juiz. | Três defeitos somados: (1) `watcher.rs:236` usa `Gitignore::matched`, que não considera diretórios pais (doc do ignore 0.4.26), então `target/` não filtra `target/debug/...`; (2) `watcher.rs:243-246` emite um `error!` por evento descartado, sem contador nem limite de taxa; (3) `telemetry_init.rs:190-191` grava em `/tmp` (tmpfs) com rotação diária, sem teto nem retenção. | R: tamanho, taxa por janela de tempo, primeira e última linha, 64 arquivos de 0 bytes entre 17:42 e 17:47, `df`. Amostra: `target/audit-2026-09-14/r2/touring-log-sample.txt`. |
| R2-2 | **O build do MkDocs (`site/`, 231 MB, ignorado por `.gitignore:10`) é indexado e pontuado.** O índice tem 47.877 símbolos de `site/`, cerca de 14% do total. O F2.1 do workspace reprova por `site/docs/agentic-bench/run_bench.py`, uma cópia de 26/08 sem o pragma. A evidência imprime só o basename (`aggregate.rs:228`, `short(worst_path)`), o que levou a mim e ao crítico C a atribuir a falha ao arquivo errado. | O walker do índice e o corpus do touring-quality não aplicam `.gitignore`: não há `WalkBuilder`/`git_ignore` em `index.rs` nem em `reindex.rs`. | R: `touring index why site/...` → `indexed, 28 symbols`; `symbols.db` com 47.877 linhas `site/%`; `score docs/agentic-bench --dims F2.1` → Pass 1.0 (o pragma funciona). |
| R2-3 | **O F2.5 falha aberto sem `Cargo.lock`.** Um alvo Rust sem lock recebe `1.0 — pass`, enquanto o mesmo gate devolve 0.5 `UNVERIFIED` quando a advisory DB está offline. | `f2_5_dep_cves.rs:202-206`. | L. |
| R2-4 | **O pragma do F2.1 vale em qualquer lugar do conteúdo.** Um arquivo de produção com a string numa literal ou docstring silencia o gate P0 inteiro. | `f2_1_owasp.rs:146`, `raw.contains(ALLOW_ATTACK_FIXTURE_PRAGMA)`, sem restrição de linha nem de comentário. | L (crítico C, conferido). |
| R2-5 | **O B12 cria órfãos falsos quando o tipo chega por reexport.** `ALL_HANDLERS` (`touring-assists/src/handlers/mod.rs`) é usado 15× em `touring-server/src/cli/assist.rs` via `use touring_assists::{ALL_HANDLERS, ...}` e ainda assim aparece como órfão novo. | Antes, a inferência por nome ligava a aresta. O B12 passou tipos de outra crate ao resolvedor de imports, que não segue o `pub use` da raiz da crate (lição `reexport-intra-crate-nao-seguido`). Dos 5 órfãos novos amostrados, 1 é falso e 4 são reais (`WiringModulesReport`, `EmbeddingSearchExt`, `ExtensionCount` sem uso fora da definição; `PaletteCtx` só reexportado). | R: grep de uso fora do arquivo que define. O censo completo dos +47 fica para a fase 5. |

### P2

| ID | Achado | Causa-raiz | Prova |
|---|---|---|---|
| R2-6 | **342 arestas legadas `ast_read` sobrevivem ao rebuild completo.** São palpites por nome de método de mai/jun (ex.: `domain_circuit.rs::map` consumido por `touring-resource-monitor`). 17 dos 23 consumidores não existem em disco e nenhum está no `file_knowledge`. Voltam do banco como `AstResolved` (0.9), e 72 produtores públicos só estão ligados por elas. | A varredura de fantasmas parte do `file_knowledge` e nunca visita consumidores ausentes dele (`stale_files_purged=0`). O schema tem `contract_source TEXT DEFAULT 'ast_read'` (`touring-storage/src/knowledge/schema.rs:176`, `touring-foundation/src/schema/knowledge.rs:161`, `scip_ingest.rs:278`), então um INSERT que omite a coluna ganha proveniência resolvida. | R: SQL read-only em `knowledge.db`. |
| R2-7 | **Estado heavy global ao processo.** `HEAVY_CALL` e `DAEMON_READ_TIMEOUT_SECS` (`daemon_client.rs:27,59`) são statics nunca resetados. No `touring serve` (MCP, processo longo), uma chamada heavy deixaria todas as seguintes com piso de ~1.860 s e a mensagem "consulte, não repita". | Estado por processo em vez de por chamada. | L (crítico A). Falta provar que o MCP chega a um hook heavy: fica para o teste da fase 5. |
| R2-8 | `inferlets::serialized::tests::test_file_save_load` usa o caminho fixo `temp_dir()/test_inferlet_cache.inf1`. Com ENOSPC deixa arquivo de 0 bytes, e em execução concorrente colide. | `crates/inferlets/src/serialized.rs:175`. | R: arquivo de 0 bytes às 17:42:47. |
| R2-9 | **Lacunas de mutação da rodada 1.** Nenhum mutante reverte `FailClosedLoc` para `WeightedLoc` (D2) nem põe `VENDORED_DIRS` no predicado de segurança (D3). | — | L (crítico C, conferido nos `mut_lote*.py`). |
| R2-10 | O MCP server não persiste a QTable: `WARN Failed to persist QTable state: sqlite error: no such table: learning_qtable`. | Tabela ausente no banco que o servidor abre. | R: log. |

### P3

| ID | Achado |
|---|---|
| R2-11 | `global_daemon_pid`: um registro velho cujo PID foi reciclado por outro `touring-daemon` passaria no teste de `comm`. Falta conferir que o PID do registro segura o socket (`update-touring:145-158`). |
| R2-12 | O wrapper `touring-quality score --help` trata `--help` como alvo e quebra no `readlink`. |
| R2-13 | O doc de `upsert_symbol` não diz que é para atualização incremental e nunca para rebuild em lote (`tantivy_index.rs:1695-1704`). |
| R2-14 | Precisão de documento: o item 15(a) do `CLAUDE.md` não diz onde vive `may_run_during_heavy` (`touring-dispatch/src/daemon.rs`); as citações A1 e A2 do relatório R1 omitem o crate (`daemon.rs:596`); e a tabela de achados da R1 não traz status de resolução, então quem lê toma o estado pré-correção pelo atual. |
| R2-15 | Os testes de ciclos (`wiring_tests.rs:36-56`) inserem direto em `wiring_map`, sem nenhum teste passando pelo gate. |
| R2-16 | Erros de export OTLP (`BatchSpanProcessor.ExportError`) sem coletor rodando. |
| R2-17 | Resíduo: bancos por diretório em `~/.claude/skills/*/.claude/touring/` (criados até 23/08, sem escrita desde então) seguem sendo abertos para leitura. |
| R2-18 | A queda do Bash (17:43 até ~19:3x) segue sem causa provada; os itens que os críticos deixaram `UNVERIFIED` precisam rodar de novo na fase 6. |

## Achados dos críticos rejeitados, com evidência

| Crítico | Alegação | Por que foi rejeitada |
|---|---|---|
| D (RD1), B (RB1) | O `touring` aborta (134) por defeito do D1. | Ambiente: fora da janela da queda, `touring index find` → rc=0 e `touring --version` → 30.4.47. |
| D (RD4), A (RA8) | O juiz não para o daemon privado (C9). | `loop_converged.py:512` `_stop_private_daemon` e `:540-541` `finally`. |
| D (RD5) | Exit 79 nunca é emitido (A4). | `daemon_client.rs:72` `prepare_for_hook`; A4 corrigido na fase 5 da R1 (verificado pelo crítico A). |
| A (RA4) | O `Running::Drop` talvez não reinstale a função. | `actor_yield.rs:88-96` reinstala. |
| A (RA6) | `verify_or_restart` ainda lê o `$?` errado. | `update-touring:554` `verify_health \|\| rc=$?`. |
| A (RA7) | O A6 talvez não esteja corrigido. | `index.rs:132` e teste `a_building_answer_is_never_cached` (`:3717`). |
| B (RB3) | `supers <= inside` descarta arestas em `mod.rs`. | Dentro de `mod a { }`, `super` aponta para o módulo do próprio arquivo, seja `foo.rs` ou `foo/mod.rs`; o desconto está certo nos dois casos. |
| C (RC0) | O pragma não é honrado no modo escopo. | É honrado (`score docs/agentic-bench` → Pass 1.0); a falha vinha de `site/` (virou R2-2). |
| C (RC5) | Os `scripts/test_*.py` geram falso positivo de F2.1. | Não reproduzido; especulativo. |
| C (RC6) | `mut_lote_update_touring.py` não existe. | O arquivo é `mut_update_touring.py`, citado assim no relatório. |

## Verificações que passaram (amostra com evidência)

- **A1:** backpressure restaurado (`daemon.rs:619-621`, fila adiada limitada a 128).
- **A2:** o único `Sender` forte fica em `ProjectRuntime` (`hook_runtime.rs:287-311`, `WeakSender`).
- **O1:** leitura, `file_changed` e `task_output` passam por `refresh_file_wiring*` (`post_read.rs:253`, `file_changed/mod.rs:74`, `task_output.rs:155`).
- **A10:** `stage_symbol` faz delete e add no mesmo guard do writer (`tantivy_index.rs:1715-1730`).
- **Tantivy:** `scoped_root` nunca cai no `$HOME` (`tantivy_index.rs:1868-1885`).
- **B6:** `.git` arquivo conta como marcador (`paths.rs:419-428`).
- **D3:** dois predicados de skip com propósitos declarados (`verifications/mod.rs:127,143`).
- **update-touring:** o registro é filtrado por socket (`update-touring:152`).
- **Rebuild e doctor depois da propagação:** geração 22 completa, doctor 8/8.

## Plano proposto para a fase 5 (aguarda aprovação)

| Lote | Itens | Correção (REGRA #0) | Prova |
|---|---|---|---|
| **L1: ambiente e ruído** | R2-1, R2-8, R2-16 | Watcher: `matched_path_or_any_parents` com guarda de raiz, contador de descarte e um `warn!` agregado por intervalo. Log: default em `~/.claude/touring/logs` com retenção (`max_log_files`) e teto. Teste do inferlets com tempdir único. OTLP silencioso sem coletor. | Teste do filtro com `target/debug/x.o` e do contador; build com o log medido antes e depois. |
| **L2: corpus e índice** | R2-2 | Walker do índice e corpus do touring-quality aplicam o `.gitignore` da raiz. Evidência do worst-of com caminho relativo. | `index why site/...` → ignorado; rebuild sem `site/`; F2.1 do workspace medido. |
| **L3: gates P0** | R2-3, R2-4, R2-9 | F2.5 sem lock → Warn 0.5 `UNVERIFIED`. Pragma só em linha de comentário nas primeiras 5 linhas. Mutantes para D2 e D3. | Fixtures e mutantes. |
| **L4: wiring** | R2-5, R2-6 | Resolvedor segue `pub use` da raiz da crate. Rebuild purga consumidores fora do `file_knowledge`. Migração relabela `ast_read` como `ast_inferred`, e o default do schema também. Censo dos +47. | Testes de reexport e purga; órfãos medidos contra a baseline. |
| **L5: cliente e processo** | R2-7, R2-11, R2-12, R2-10 | Estado heavy por chamada. Registro conferido contra quem segura o socket. `--help` no wrapper. Tabela da QTable. | Testes unitários e o pytest do `update-touring`. |
| **L6: docs** | R2-13, R2-14, R2-15, R2-17 | Docs e tabela de status na R1; teste de ciclo pelo gate. | Leitura. |

Depois: rodar de novo tudo o que ficou `UNVERIFIED`, os gates do workspace, a release (se Gabriel aprovar) e o juiz.

## Fase 5: correções aplicadas (aprovação de Gabriel: L1-L6, `.gitignore`, release 30.4.48)

| Lote | Achado | Correção | Testes novos (verdes) |
|---|---|---|---|
| L1 | R2-1 | `watcher.rs`: `is_ignored` com `matched_path_or_any_parents` (guarda de raiz); `DropReporter` conta descartes e emite um `warn!` a cada 30 s. `telemetry_init.rs`: `log_dir` (`TOURING_LOG_DIR` → `~/.claude/touring/logs` → temp) com `max_log_files(7)`. | `a_directory_rule_ignores_every_file_below_it`, `a_project_rule_without_a_glob_covers_nested_files`, `drops_are_reported_once_per_interval_with_their_count`, `logs_go_to_the_home_state_dir_never_to_the_tmpfs` |
| L1 | R2-8 | `inferlets` `test_file_save_load` com caminho único por processo e instante. | o próprio teste (4/4) |
| L1 | R2-16 | OTLP só com `OTEL_EXPORTER_OTLP_ENDPOINT` definido; erro do exportador desliga a camada em vez de `expect`. | compila e é coberto pelo gate do workspace |
| L2 | R2-2 | `touring_foundation::gitignore::GitIgnoreRules`, fonte única: `info/exclude` + `.gitignore` aninhados, nada volta de diretório ignorado. `IndexPolicy::gitignored` + `IndexVerdict::Gitignored`; os walkers do rebuild e do backfill e a varredura consultam a política. Os cinco walkers do touring-quality usam as mesmas regras. `aggregate::scope_relative` nomeia o pior arquivo pelo caminho no escopo. | `gitignore::tests` (6), `a_path_git_ignores_is_refused_with_its_rule_named`, `what_git_ignores_is_never_walked_and_an_old_row_is_swept`, `the_backfill_walks_what_git_keeps_and_nothing_it_ignores`, `what_git_ignores_is_in_no_corpus`, `the_worst_file_is_named_by_its_path_in_the_scope_not_its_basename` |
| L3 | R2-3 | F2.5 sem lock → 0.5 `UNVERIFIED`. O teste antigo afirmava só a faixa [0, 1]. | `a_manifest_without_a_lockfile_is_unverified_not_a_pass` |
| L3 | R2-4 | `carries_attack_fixture_pragma`: linha de comentário entre as 5 primeiras. | `only_a_header_comment_declares_an_attack_fixture` |
| L4 | R2-5 | Resolvedor: o nome completo `touring_*` como módulo resolve para `lib.rs`, e o `definer_module` segue o glob. Alias curto excluído. | `a_crate_root_import_resolves_through_the_root_to_the_definer` |
| L4 | R2-6 | `FileKnowledgeDB::consumer_files` + segunda passada da varredura sobre consumidores sem símbolos (`consumers_purged`). | `a_consumer_edge_of_a_vanished_file_is_swept_by_the_rebuild` |
| L5 | R2-7 | `daemon_client::wait_plan` por chamada; `HEAVY_CALL` e `raise_timeout_floor` removidos. | `a_heavy_call_leaves_the_next_light_call_as_it_was`, `an_explicit_timeout_always_wins_over_the_heavy_floor` e o de hooks heavy reescrito |
| L5 | R2-10 | `learning_db_path` (graph.db) para o loader e o loop de evolução. | `the_learning_loop_persists_where_the_loader_reads` |
| L5 | R2-11 | A entrada do registro só vale se o PID segurar o socket (quando há `lsof`). | `test_a_registered_pid_that_no_longer_holds_the_socket_is_not_the_owner`; o teste do registro agora distingue do "mais novo" |
| L5 | R2-12 | `score --help` vai ao motor. | `test_score_help_reaches_the_engine_instead_of_being_read_as_a_target` |
| L6 | R2-13/14/15 | Doc do `upsert_symbol`; `CLAUDE.md` item 15(a) e item 18; `CLAUDE.md` do touring-cli, touring-server e touring-quality; citações com crate e nota de status no R1; ciclo pelos escritores de produção; teste do walker do backfill (não havia nenhum). | `a_cycle_written_through_the_production_writers_is_found` |

Decisões tomadas na execução, com a razão:
- **R2-6, relabel de `ast_read`: não aplicado.** `from_contract_source("ast_read") → AstResolved` é uma decisão registrada em teste (`legacy_ast_read_rows_read_back_as_resolved_not_as_weakest`): as linhas legadas legítimas vêm da resolução de `use`. As 342 linhas fantasmas saem pelo critério que as prova fantasmas (consumidor fora do disco ou do walk), não pela etiqueta.
- **R2-6, `DEFAULT 'ast_read'` do schema: não alterado.** O censo dos INSERTs sem a coluna encontrou só código de teste (touring-analysis, schema_guard, testes de integração); nenhum escritor de produção depende do default.
- **R2-17 (bancos antigos em `~/.claude/skills/*/.claude/touring/`): não apagados.** Sem escrita desde agosto e fora deste repositório; fica para decisão de Gabriel.
- **Rastro da execução:** um filtro `cargo test` que casava zero testes (`cli::handlers::index`) saiu rc=0 com `0 passed`; foi pego e rerodado com o nome real do módulo (`cli_handlers_index`). O mesmo sinal revelou que o walker do backfill não tinha teste.

### Prova por mutação (fase 5)

`target/audit-2026-09-14/r2/mut_r2.py` desliga uma defesa por vez e exige um teste falhando. Mutante morto por erro de compilação conta como não provado. Todos os arquivos foram restaurados e conferidos.

- **Primeira passada: 21/22 mortos.** Nenhum morreu por compilação.
- **Sobrevivente:** "o walker do rebuild aceita arquivo ignorado, mantendo a checagem de diretório". A fixture só tinha uma regra de diretório (`/site/`), então a checagem de arquivo não tinha teste próprio. Os três testes (rebuild, backfill e corpus de qualidade) ganharam um padrão de arquivo (`*.gen.rs` / `*.gen.py`) dentro de um diretório walkado.
- **Segunda passada** (`mut_r2_files.py`): os três mutantes só de arquivo, **3/3 mortos**.

| Mutante | Teste que o matou |
|---|---|
| filtro do watcher sem pais | `a_directory_rule_ignores_every_file_below_it`, `a_project_rule_without_a_glob_covers_nested_files` |
| descarte logado sempre | `drops_are_reported_once_per_interval_with_their_count` |
| log de volta ao temp | `logs_go_to_the_home_state_dir_never_to_the_tmpfs` |
| conteúdo de diretório ignorado volta | três testes de `gitignore::tests` |
| veredito sem git | `a_path_git_ignores_is_refused_with_its_rule_named` |
| walker do rebuild (dir + arquivo / só arquivo) | `what_git_ignores_is_never_walked_and_an_old_row_is_swept` |
| walker do backfill (dir + arquivo / só arquivo) | `the_backfill_walks_what_git_keeps_and_nothing_it_ignores` |
| corpus de qualidade (dir + arquivo / só arquivo) | `what_git_ignores_is_in_no_corpus` |
| worst-of pelo basename | `the_worst_file_is_named_by_its_path_in_the_scope_not_its_basename` |
| F2.5 sem lock passa | `a_manifest_without_a_lockfile_is_unverified_not_a_pass` |
| pragma em qualquer linha / fora de comentário | `only_a_header_comment_declares_an_attack_fixture` |
| D2: F2.1 volta a média (R2-9) | `block_dims_are_fail_closed_kinds`, `table_is_50_and_distribution_correct` |
| D3: segurança pula vendorizado (R2-9) | `a_secret_in_vendored_code_fails_the_security_dims_but_not_the_style_corpus` |
| import de raiz de crate não resolve / alias curto aceito | `a_crate_root_import_resolves_through_the_root_to_the_definer` |
| varredura de consumidores desligada | `a_consumer_edge_of_a_vanished_file_is_swept_by_the_rebuild` |
| `--timeout` explícito perde para o piso | `an_explicit_timeout_always_wins_over_the_heavy_floor` |
| loop de learning no caminho da RLM | `the_learning_loop_persists_where_the_loader_reads` |
| PID do registro sem conferir o socket | `test_a_registered_pid_that_no_longer_holds_the_socket_is_not_the_owner` |
| `score --help` lido como alvo | `test_score_help_reaches_the_engine_instead_of_being_read_as_a_target` |

Os dois mutantes de R2-9 morreram por testes da rodada 1 que já existiam: a lacuna era de prova registrada, não de teste.

## Achado na fase 5: o gate de testes travava num deadlock do SQLite (R2-19, P1)

O primeiro `cargo test --workspace` da fase 5 parou 12 minutos no binário de testes do `touring-dispatch`: 32 testes `lifecycle` acima de 60 s, 1.986 threads, CPU perto de zero.

Investigação, na ordem, com o que cada passo descartou:

1. **Não é lock de banco.** O `~/.claude/touring/memory.db` global está em WAL e aceitou `BEGIN IMMEDIATE` na hora, com WAL de 0 bytes.
2. **Não é memória.** Uma execução limpa pica ~1 GB de RSS.
3. **É intermitente e de concorrência.** Um teste isolado passa em 0,04 s e o grupo `lifecycle` sozinho passa; o binário completo travou em 2 de 4 execuções.
4. **Backtraces** (`gdb` lançando o binário como filho, porque `ptrace_scope=1`): toda thread SQLite em `pthread_mutex_lock`, dentro de `unixShmBarrier` (22), `walIndexWriteHdr` (4), `unixLock ← sqlite3WalClose` (3), `unixShmUnmap`, `unixShmMap` e `unixClose`. Nenhuma presa em syscall, o que indica inversão de ordem de locks dentro do VFS unix.
5. **Primeira correção, parcial.** O recall federado da memória abria cada `memory.db` de outros projetos (inclusive o global) com `Connection::open`, em leitura e escrita: o mesmo defeito capturado sob `gdb` em 24/08/2026 e corrigido então só no `cli_suggester`. Foi criado o abridor único `cli::shared::open_db_readonly`, usado em `memory_recall_sql`, `memory_backfill_stored_at`, `compute_tag_filter`, `fetch_tagged_entries`, `memory_metrics` e no `cli_suggester`. O guard `no_lessons_db_is_opened_for_writing` foi estendido a essas funções, e entrou o teste `federated_memory_reads_never_create_the_database_they_probe`. **Efeito medido:** 1 travamento em 8 (antes 2 em 4). A pilha nova mostrou até a conexão só-leitura parada no `sqlite3WalClose`.
6. **Causa-raiz.** O workspace embutia o **SQLite 3.51.1** (`rusqlite 0.38` / `libsqlite3-sys 0.36`). Notas oficiais: a 3.51.0 (item 17) introduziu a detecção de locks POSIX quebrados por `close()`; a **3.51.2** corrigiu *"an obscure deadlock in the new broken-posix-lock detection logic"*; a **3.51.3** corrigiu *"the WAL-reset database corruption bug"*, presente desde a 3.7.0 com várias conexões em WAL, o padrão do daemon. O mesmo deadlock aparece relatado em [mattn/go-sqlite3#1369](https://github.com/mattn/go-sqlite3/issues/1369).

**Decisão de Gabriel: rusqlite 0.40.2** (SQLite 3.53.2), com `deadpool-sqlite` 0.13 → 0.14, porque o 0.13 prendia o rusqlite em `^0.38` e o `links = "sqlite3"` só admite uma cópia. A troca não exigiu mudança de código: `cargo check --workspace --all-targets` rc=0. Fontes: [SQLite release notes](https://sqlite.org/changes.html), [rusqlite releases](https://github.com/rusqlite/rusqlite/releases).

## Fase 6: prova E2E (executada)

| Prova | Comando | Resultado |
|---|---|---|
| Gates do workspace, com SQLite 3.53.2 | `target/audit-2026-09-14/r2/gates.sh` (unit `touring-r2-gates2`) | `cargo check --workspace --all-targets` rc=0 · `cargo clippy --workspace --all-targets -D warnings` rc=0 · `cargo test --workspace --no-fail-fast` rc=0 (275 binários de teste, 0 falhas) |
| Travamento do binário de testes | `hang_proof.py`: binário completo do `touring-dispatch` (1.338 testes) | SQLite 3.51.1: 2 travamentos em 4 · 3.51.1 + abridor read-only: 1 em 8 · **3.53.2: 0 em 12** (16-17 s cada) |
| Mutação | `mut_r2.py` + `mut_r2_files.py` | 24/24 mortos por teste, arquivos restaurados |
| Testes Python dos scripts | `pytest scripts/test_update_touring.py scripts/test_touring_quality_score.py` | 46 passed |
| `/tmp` depois das correções | `df -h /tmp` | 6% (o log de spam foi truncado duas vezes; os MCP servers antigos só param de escrever depois de `/mcp` na release) |
| Recall federado não cria bancos | `federated_memory_reads_never_create_the_database_they_probe` | verde; o mutante equivalente (voltar a `Connection::open`) cria o arquivo |
| Filtro de ignore no índice, com o rebuild real | `what_git_ignores_is_never_walked_and_an_old_row_is_swept` | verde (rebuild em fixture: `site/` purgado com motivo `gitignored`, `why` nomeia a regra) |

## Fase 7: release 30.4.48, juiz e segunda remediação

### Release 30.4.48 e pós-release (14/09, 23:07–23:14)

| Etapa | Resultado |
|---|---|
| `propagate-release.sh 30.4.48` | rc=0 · build 7m08 · verify limpo (PID 4138845) · analise e konverter 30.4.47 → 30.4.48 · prova comportamental 40/40 · sessões analise avisadas |
| teste de versão do binário | rc=0 |
| `index rebuild` (geração 23) | rc=0 · 5.112 arquivos · 91.561 símbolos (antes 160.937) · purga: 214 `gitignored` (todos em `site/`), 18 `missing`, 6 `skipped_dir` · `consumers_purged` 24 · `phantom_modules_purged` 2 · 0 erros · 217 s |
| `doctor -j` | 8/8 ok · wiring 83.629 linhas (0 `kind_unknown`, 0 caminhos absolutos) |

### Juiz: rc=1 (14/09, 23:27)

`judge_intact` ✅ · `dag_done` ✅ · `quality_gold` ❌ (F2_1 bloqueia; composite 0,902) · `no_p0_fail` ❌ (F2_1) · `measured_whole_scope` ✅ · `orphans_base` ❌ (1.613 contra 1.553) · `cargo_green` ✅ · `cross_audit` ➖.

A cláusula `cargo_green` passou sem valer: ver R2-22.

### Achados da segunda remediação

| # | Achado | Causa-raiz provada | Evidência |
|---|---|---|---|
| **R2-20** (P0 gate) | 7 arquivos reprovavam o F2.1 sem vulnerabilidade: `<script>` em docstring e em placeholder de flag, `../../` em caminho de `wit_bindgen::generate!` e em listagem em árvore, `=*)` em `case` de shell e em prosa, scripts shell lidos como Rust. | O `SecurityAnalyzer` descarta match em comentário e em `#[cfg(test)]`, nunca em docstring; a linguagem vinha só da extensão (`.sh` e sem extensão → `rust`); os regexes não tinham contexto. O `site/` velho (R2-2) era o pior arquivo e escondia os 7. | bisseção por diretório com `touring-quality check --gate F2.1` |
| **R2-20b** (falso negativo) | Cada padrão devolvia só o primeiro match; a supressão por região vinha depois. Um payload citado num comentário no topo apagava o sink real mais abaixo. | `detect` → `re.find` / `find_iter().find`; `SecurityAnalyzer::analyze` filtrava um único match por padrão. | ao corrigir a docstring do `cc_build.py`, o segundo `<script>` (linha 872) apareceu |
| **R2-21** (wiring) | 65 órfãos novos. O extrator de imports **nunca rodou tree-sitter para Rust**: `rust_imports.scm` usava o nó `default_import` da gramática TypeScript, `Query::new` falhava, e todo arquivo Rust caía no regex, que só lê linhas iniciadas por `use ` (ignora `pub use`, `pub(crate) use` e listas com chaves em várias linhas). A correção 17(d) da R1 tirou esses tipos da inferência por nome, e a aresta sumiu. | `QueryError { row: 52, column: 14, message: "default_import", kind: NodeType }` no teste novo `every_import_query_compiles` | `MAX_INDEXABLE_FILE_BYTES` sem nenhuma linha de consumidor nem em `wiring_unresolved` |
| **R2-22** (gate) | O E2E da cláusula `cargo_green` rodou `target/llvm-cov-target/debug/touring-daemon` de 13:53, nove horas mais velho que as correções. O binário instrumentado deixou 192 `default_*.profraw` na raiz (ignorados pelo git). | `locate_binary` (4 cópias) procurava `llvm-cov-target` antes de `target/`. | PIDs dos daemons de teste no cgroup do juiz; mtime dos binários |
| R2-23 | ~~Daemons de teste vazados~~ **rejeitado**: o `shared()` já nasce com `TOURING_IDLE_TIMEOUT_SECS=90`; os 12 vistos tinham 0–96 s. | — | `private_daemon.rs::shared` |
| **R2-24** (aberto) | O daemon global morreu duas vezes durante o juiz (23:16:45 e 23:22:06, scopes `run-p4138845` e `run-p332`) e duas vezes logo depois do boot de 15/09 (13:07:19 e 13:11:17), junto com um daemon per-project da analise. Sem linha de encerramento, sem core, sem OOM do kernel nem systemd-oomd: padrão de SIGKILL. | **Não provado.** Descartados: `daemon-ctl status` da analise (o daemon já estava morto), `daemon-ctl stop` do juiz (o registro exclui o global), vazamento de daemons de teste. Os daemons vivos têm PPID no `systemd --user`, sessão e scope próprios; a janela vulnerável é enquanto o hook que os criou ainda vive. | vigia persistente `touring-daemon-watch` (unit de usuário) grava dono do lock, pai, hooks e suspeitos por segundo em `target/audit-2026-09-14/r2/watch-persistent.log` |
| **R2-25** (P1, daemon) | O `cargo test --workspace` de 15/09 (depois do reboot das 13:06) morreu com SIGSEGV em `proptest_parser_fuzz`; isolado, o mesmo binário travou 1 em 8 execuções. | Com `PROPTEST_RNG_SEED=2` a trava é determinística. Delta debugging reduziu a entrada a **86 bytes ASCII**; sob `gdb` a thread fica em `ts_parser_parse → ts_language_next_state` (gramática TypeScript 0.23.2, runtime tree-sitter 0.26.9) para sempre. Nenhum dos 10 parses do touring-code tinha limite: o daemon parseia todo `.ts` indexado, e um arquivo assim prendia o ator do projeto. | backtrace em `ts_hang_bt.txt`; entrada em `ts_hang_minimal.txt`; o SIGSEGV não voltou em 20 execuções depois da correção (não provado que seja a mesma entrada) |
| **R2-26** (ambiente → produto) | `test_js_execution` reprovou: `node` no sandbox saía 1 com `mise ERROR Permission denied` lendo `~/.config/mise/config.toml`. | O arquivo foi criado em 15/09 07:01; o shim do mise lê a própria config e o registro de confiança, e o Landlock só concedia `~/.local/share/mise`. O próprio `sandbox_read_roots` manda enumerar o caminho quando uma ferramenta legítima quebra. | `mise-ctx.log`: 21/21 depois do grant |

### Decisões de Gabriel (15/09)

1. F2.1: **precisão no detector** (não pragma, não voltar à média).
2. Órfãos: **resolvedor + potencializar**.
3. **Release 30.4.49 com aviso**.
4. Os 55 órfãos sem uso depois do resolvedor: **remover os mortos, integrar os de contrato não cumprido, registrar a API na baseline** com autorização registrada aqui.

### Correções

| Lote | Mudança | Prova |
|---|---|---|
| W1 (R2-22) | `private_daemon.rs::locate_binary` é a fonte única (as cópias de `e2e_diagnostic_rfc100` ×2 e `wave24_hook_integration_e2e` delegam); ordem das raízes: `CARGO_TARGET_DIR` → raiz de build do próprio binário de teste → `target/` → `llvm-cov-target/`. Funções puras `target_roots` e `pick_binary`. | `tests/locate_binary_roots.rs` (6 testes, incluindo a fixture exata de 14/09) |
| W2 (R2-20) | `VulnerabilityPattern::detect_every` e `PatternRegistry::detect_every`; `analyze` filtra todos os matches por região e mantém o primeiro sobrevivente por padrão. `lang_for_source` (extensão + shebang) no F2.1. Sintaxe `SHELL` com `#` só em início de palavra. `python_docstring_regions`, separada de `non_executable_regions` porque ~40 motores de qualidade leem esta. Contexto nos regexes: placeholder de flag, script do próprio documento HTML, campo `path:` de macro, alvo de symlink em listagem, LDAP `(atributo=*)`, e `&&`/`;`/`|` do CMDi não contam em código shell. | `r2_context_false_positives_and_their_true_positives`, `detect_every_returns_every_match_and_detect_the_first`, `shell_comments_open_only_at_a_word_start`, `python_docstrings_are_told_apart_from_triple_quoted_code`, `a_commented_payload_does_not_hide_the_real_sink_below_it`, `shell_operators_are_not_command_injection_in_a_shell_script`, `python_docstrings_do_not_reach_a_sink_but_triple_quoted_arguments_do`, `r2_workspace_false_positives_pass_and_real_sinks_still_fail`, `the_security_scan_reads_shell_and_shebangs` |
| W3 (R2-21) | `rust_imports.scm` com `use_wildcard` e `use_as_clause`; para Rust, `extract_imports_treesitter` percorre a árvore de `use` e compõe o caminho completo de cada folha (`rust_use_imports`). | `every_import_query_compiles`, `the_rust_query_sees_uses_behind_a_visibility_modifier`, `rust_use_trees_yield_every_leaf_under_its_full_module_path` |
| W3b (órfãos) | 7 `pub(crate)` com uso de produção no próprio arquivo (`DAEMON_BUSY_EXIT_CODE`, `DAEMON_STILL_RUNNING_EXIT_CODE`, `MutationType`, 4 `Output` de inferlets) e `is_code_language`. 26 itens removidos sem nenhuma referência (params MCP e payloads de hook nunca ligados, `MinimalContext`, `ContextPriority`, `EdgeStyle`/`NodeStyle`, `FusionDebug`, `refresh_file_producers_from_disk`…), por `remove_dead.py`. Grupo D: `HOOKS_MODE`/`HooksMode` e `SdkValue` não tinham como ser integrados (os hooks moram no `touring-dispatch`, que não pode depender da fachada; nenhuma função Rust do `sdk.rs` devolve valor de hook), então saíram, e os textos que prometiam o contrato foram corrigidos. | clippy do workspace `-D warnings` e `bind-web`/`bind-wasm` do `touring-bindings` |

| R2-25 | `ast::parser::parse_bounded` / `parse_bounded_with`: `ParseOptions::progress_callback` corta o parse em 2 s + 2 s/MiB e dá `reset()` no parser; os 10 parses do touring-code passam por ele (`parse_thread_local`, `parse_full`, `parse_rope`, `parse_incremental`, `incremental_parse` — que deixou de prometer "never returns None" e devolve `AstResult` —, `method_calls`, `imports`, `surgery`, `polyglot_semantic`, `go_wiring`). | `tests/ts_parse_budget.rs` (a entrada de 86 bytes como literal Rust, nunca como `.ts` que um daemon antigo indexaria): corta em 2,02 s e o parser da mesma thread lê o documento seguinte; `PROPTEST_RNG_SEED=2` termina; 10/10 execuções aleatórias do fuzz |
| R2-26 | `sandbox_read_roots` ganha `.config/mise` e `.local/state/mise`, nunca `.config` inteiro (`gh/hosts.yml`). | `the_mise_shim_reads_its_config_but_the_config_dir_stays_out`; `ctx_execute_e2e` 21/21 |

**Mutação (`mut_r3.py`): 17/17 mortos por teste**, arquivos restaurados. **`mut_r4.py` (R2-25 e R2-26): 4/4** — parse sem orçamento, orçamento que nunca estoura, parser sem `reset()` e grant do mise removido.

**Gates do workspace depois do reboot de 15/09** (unit `touring-r2-gates4`): `cargo test -p touring-code --lib` e `--doc` rc=0 · `cargo clippy --workspace --all-targets -D warnings` rc=0 · `cargo test --workspace --no-fail-fast` **rc=0, 284 binários de teste, 0 falhas**. A rodada anterior (`gates3`) tinha reprovado em 2 binários, os achados R2-25 e R2-26. Cada defesa do W1, W2 e W3 tem um mutante; o de dedupe só morreu depois que o teste ganhou dois sinks reais do mesmo padrão.

**F2.1 real, com o motor novo:** os 9 arquivos que reprovavam passam (1,0); o workspace inteiro mede **1,0 Pass em 2.373 arquivos** (`fail-closed: no file fails`).

**Deploy local 30.4.49 + rebuild (geração 24):** `update-touring` rc=0 · 5.115 arquivos · 91.625 símbolos · linhas de consumidor 71.211 → 71.751 · `MAX_INDEXABLE_FILE_BYTES` com consumidor `ast_resolved` · órfãos no escopo 1.613 → 1.556.

### Release 30.4.49 (15/09, depois dos gates)

| Etapa | Resultado |
|---|---|
| `propagate-release.sh 30.4.49` | rc=0 · analise e konverter com `lock=30.4.49` e `touring 30.4.49` · prova comportamental 40/40 · analise-08 avisada |
| teste de versão | rc=0 |
| `index rebuild` | rc=0 · 5.118 arquivos · 91.603 símbolos · 0 erros · `ts_fuzz_capture.rs` (harness temporário, apagado) purgado como `missing` |
| `doctor -j` | rc=0 |
| censo de órfãos | 1.522 no escopo; 28 novos = os 26 de API + 2 mortos em cascata (`DimensionScore`, `ContextStats`, só usados pelos itens removidos) → removidos, clippy rc=0, reindexados → **1.520 no escopo, 26 novos, exatamente a lista aprovada** |
| baseline | os 26 nomes acrescentados a `docs/plans/2026-09-14-work-outer/.baseline/orphans-scoped.txt` (1.553 → 1.579) sob a autorização acima; justificativa e lista em `orphans-scoped.r2-api-2026-09-15.md` ao lado |

Os dois mortos em cascata foram removidos depois da propagação da 30.4.49. A 30.4.51 (embeddings em GPU, publicada pela touring-61 no global, na analise e no konverter a partir da mesma árvore) já as leva, então fonte e binário voltaram a coincidir.

### Veredito do juiz (15/09, 14:22, unit `touring-r2-judge2`)

**rc=1**, por uma cláusula só:

| Cláusula | 14/09 23:27 | 15/09 14:22 |
|---|---|---|
| `judge_intact` | ✅ | ✅ |
| `dag_done` | ✅ | ❌ 15/16 — W4 (R2-24) aberto de propósito |
| `quality_gold` | ❌ Unranked, blocker F2_1 | ✅ **Platinum 0,938** |
| `no_p0_fail` | ❌ F2_1 | ✅ nenhum |
| `measured_whole_scope` | ✅ | ✅ |
| `orphans_base` | ❌ 1.613 contra 1.553 | ✅ 1.520 contra 1.579, nenhum nome novo |
| `cargo_green` | ✅ (sobre binário velho, R2-22) | ✅ check + test + clippy, E2E sobre o build do próprio teste |
| `cross_audit` | ➖ | ➖ |

A sessão touring-61 tirou da árvore, antes do juiz, o trabalho em andamento de embeddings em GPU (`ort`, `storage-emb-cuda`, `ORT_CUDA_VERSION`) e não rodou cargo durante ele; o `Cargo.lock` sem `ort` confirma que nenhuma invocação o resolveu.

**Vigia do R2-24 durante esta rodada:** nenhuma troca de dono do socket global durante o juiz inteiro (incluindo `cargo test --workspace`). As trocas registradas caem todas na janela do `update-touring` da propagação (14:03:29–14:10:57): um daemon subido por hook durou 1 s (PID 320050) enquanto o script reiniciava o global — o restart do próprio script, não a morte silenciosa de 14/09 e do boot de 15/09, que segue sem autor.

### Terceiro juiz (15/09, 15:32, unit `touring-r2-judge3`, `MemoryHigh=40G`, `CARGO_BUILD_JOBS=8`)

Com o W4 movido: `dag_done` ✅ 16/16 · `quality_gold` ✅ Platinum 0,938 · `no_p0_fail` ✅ · `cargo_green` ✅ (árvore já com a GPU 30.4.51 da touring-61) · **`orphans_base` ❌** com 5 nomes de `gate_metrics.rs` da GPU. Não eram órfãos: o índice estava velho para os arquivos editados depois do rebuild das 14h (3 dos 5 têm chamador em `fastembed.rs`). Depois de `touring index rebuild` completo (rc=0, 5.124 arquivos), o censo dá **1.520 no escopo e 1 nome novo**, `touring-storage/.../fastembed.rs::cuda_fallback_reason` (pub sem consumidor, trabalho da touring-61), devolvido a ela pela REGRA #0.

Vigia do R2-24 até 15:32: toda troca de dono do socket global desde as 13:14 coincide com um `update-touring` (propagações 30.4.49 às 14:03–14:10 e da GPU às 15:04–15:19); nenhuma morte silenciosa neste intervalo.

### Veredito final: CONVERGED (15/09, 15:52, unit `touring-r2-judge4`) — **exit 0**

A touring-61 removeu `cuda_fallback_reason` (o motivo do fallback já vive só em `gate_metrics`, que o doctor lê), reindexou e ficou sem cargo nem edição durante a rodada.

`judge_intact` ✅ · `dag_done` ✅ 16/16 · `quality_gold` ✅ Platinum 0,938 · `no_p0_fail` ✅ nenhum · `measured_whole_scope` ✅ · `orphans_base` ✅ 1.519 contra 1.579 · `cargo_green` ✅ check + test + clippy · `cross_audit` ➖.

DAG `task_1789429679945581333` finalizado (16/16, 0 falhas). Lição de processo: com outra sessão editando o mesmo workspace, o índice fica velho para os arquivos dela e o juiz acusa órfãos falsos; `index rebuild` antes do juiz e janela combinada sem cargo resolvem.

### Pendências para Gabriel

1. **R2-24 movido para acompanhamento** por decisão de Gabriel (15/09, AskUserQuestion "Mover para acompanhamento"): DAG `task_1789496833724619867` (subtask `ATTRIBUTE`), instrumento `touring-daemon-watch`; o W4 desta tarefa foi fechado apontando para ele. **Pista nova, não provada:** a touring-61 mediu 57,7 GB de pico no `--rust-full` (máquina de 62 GB) e o harness matou processos dela sob essa pressão; a unit do juiz de 14/09 picou 50,4 GB + 22,9 GB de swap no intervalo das mortes, e as de 15/09 caíram no boot, com sessões e daemons subindo juntos. O juiz deste ciclo passou a rodar com `MemoryHigh=40G` e `CARGO_BUILD_JOBS=8`.
9. **E4 corrigido no detector (15/09, decisão de Gabriel "Precisão no detector")**: os 3 antipadrões do workspace eram `print(` em `cidata_gen.py`, `cidata_iso.py` e `hooks_runnable.py`, onde `print` é a saída do programa. `detect_antipatterns` passa a tratar `print` como saída num módulo com `if __name__ ==` e como diagnóstico quando vai para `sys.stderr`; `print` solto em módulo de biblioteca continua sendo reportado (teste `print_is_output_in_a_cli_script_and_debug_residue_in_a_library`). O F2.1 não muda (a dimensão pontua só por vulnerabilidade); o efeito aparece no `e2e` depois do deploy. **`touring e2e -j` em 0,8238** contra o baseline 0,8749 do `CLAUDE.md` (medido pela touring-61 depois da 30.4.51; top issues: órfãos 27%, antipadrões em `client/omarchy`, hot files). Sem medição de antes das mudanças de 15/09 para atribuir a queda.
2. **Conserto do wiring Python** pedido pela analise-08: **feito em 15/09** (`b4-wiring-python-2026-09-15.md`), na 30.4.52. Visibilidade de binding Python vem do nome, o arquivo passa a registrar o uso dos próprios símbolos, e a classe interno-only sai à parte. Medição viva no analise fica com a analise-08 depois do `touring update`. **B5 (16/09, decisão de Gabriel "Inclui relativo" + "Propagar 30.4.53 ao final")**: a analise-08 mediu os 231 órfãos que sobraram e nenhum estava sem consumidor — 136 eram from-import relativo (a consulta perdia os pontos), ~90 uso qualificado `alias.Nome` (`import x` não escrevia consumidor algum) e 7 dunder. As três famílias foram corrigidas num ciclo só, com controle negativo ao lado de cada uma no teste de rebuild real e mutação 9/9. A medição no repositório vivo segue com a analise-08 depois do `touring update` da 30.4.53.
3. **H6**: a classe interno-only agora é **reportada** (`internal_only` em `touring wiring orphans -j`), o que dá nome aos ~2.093 pubs só autoconsumidos. A política sobre eles — tornar privado ou dar o primeiro consumidor — continua com você.
4. **R2-17**: bancos antigos em `~/.claude/skills/*/.claude/touring/`.
5. **45 dimensões do touring-quality leem `.sh` e scripts sem extensão como Rust** (`lang_from_ext`); só o F2.1 passou a ler shell. Corrigir muda notas de todas as dimensões para arquivos shell. **Investigado em 15/09 (`canvas-d-shell-2026-09-15.md`):** shell nunca entra na nota de diretório (`SOURCE_EXTS` sem `sh`), então os gates P0 do juiz nunca leram os 186 scripts dos três projetos; o D3 foi revisto em cinco etapas. **Correção ao R2-20:** os scripts shell listados ali reprovavam só como alvo de arquivo, e a W2 não mexeu na nota do juiz.
6. **192 `default_*.profraw` na raiz** do workspace (ignorados pelo git), deixados pelos daemons de cobertura velhos (R2-22): remover quando quiser.
7. **Reconexão `/mcp`** nas sessões abertas: as bridges antigas seguem com o binário anterior e o log em `/tmp`.
8. **`judge_attest.py --attest`** é ato humano; e nada foi commitado.
10. **O motor do `touring-quality` morreu duas vezes em 16/09** (SIGILL 01:00, SIGSEGV 01:26, ambos dentro de units do juiz; core com um frame só, `0x4`, em thread worker) e o juiz traduziu isso como `tier=None` + "raise touring-quality to >= Gold" — **ausência de medição lida como nota baixa**. Censo: 0 crashes em 24 execuções fora do juiz (inclusive 8 num diretório só de shell, o que **isenta o Canvas D**), 2 em 5 passadas dentro dele; a chamada reproduzida à mão logo depois devolveu Platinum 0,9384. Uma das passadas ficou verde por **cache** (`secs=0`), o que escondeu o defeito por uma rodada. Evidência completa e as 5 propostas (ASan no corpus, `RAYON_NUM_THREADS=1`, persistir o stderr do motor no wrapper, mensagem do juiz distinguindo não-medido de abaixo-do-piso — muda arquivo do juiz, exige atestação — e um memtest para descartar hardware): `docs/audits/motor-de-qualidade-morre-2026-09-16.md`. Sem causa provada.
