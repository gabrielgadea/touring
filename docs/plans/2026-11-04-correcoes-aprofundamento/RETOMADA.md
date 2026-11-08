---
type: Handoff
title: "RETOMADA — correções do aprofundamento (opção A, P0–P3)"
description: "Estado em 04/11/2026 ~16h: ritual 343912cd feito; C7/C4/C6 commitados em wt-corr; C8/C3a, C1 e C9 commitados; C2 (Kimi) e hermetic-doctor em curso; fusão só após o AB34 (08/11 00:10)."
timestamp: 2026-11-04T16:00:00-03:00
---

# RETOMADA (04/11/2026, atualizada ~16h)

Pedido do Gabriel: «sim, opção A, aprovo P0 a P3 e o ritual. prossiga até todas as pendências, correções,
melhorias e aperfeiçoamentos estarem absolutamente completas e perfeitas, sempre de acordo com as melhores
práticas (consulte o context7 e o notebooklm "Claude Code" de setembro de 2026». «lembre-se das sessões kimi e minimax».

Opção A = fundir as 10 frentes do aprofundamento como estão (comportamento preservado, provado por replay) e
aplicar as correções que mudam comportamento depois, em commits separados.

## Restrição viva

AB34 armado até **08/11 00:10**: nada de hooks/settings/plugin/daemon, `update-touring`, `cargo build --release`
no `target/` principal. Todo cargo via `hw-slot --class rust --wait 7200`.

## Branches (todas a partir de `4cc2d806`, ponta do aprofundamento após o ritual)

| Worktree | Branch | Estado | Commits |
|---|---|---|---|
| touring-wt-aprof | feat/2026-11-01-aprofundamento | pronto p/ fusão | ponta `4cc2d806` (ritual REGRA #11 feito, tree idêntica, snapshot `safety/2026-11-04-pre-reescrita-343912cd`) |
| touring-wt-corr | feat/2026-11-04-correcoes | **commitado, gates verdes** | `24e74749` C7 · `ac20a1b5` C6 gates · `55fe0878` session-end stderr · `4efee418` C4 + C6 trace |
| touring-wt-c8 | feat/2026-11-04-c8-c3 | commitado (MiniMax desenhou, eu terminei) | `752e81e2` C8 sombra via `ler_contido` · `1ef7c553` Familia · `5b08daa2` C3a denominador (0,926892→0,928183) |
| touring-wt-c1 | feat/2026-11-04-c1-miner-reader | commitado, em sombra | `0c377bc5`…`6d2be39e`; `TOURING_MINER_READER=shadow` default; corpus 1141 arq/5757 pares, 0 diferença |
| touring-wt-port | feat/2026-11-04-portabilidade | commitado (C9 subagente + eu) | `fde8327c` `c70ea5ae` `405508d3` `5966b833` `c9f61488` (segurança gotcha) `642aaeff` `9d2f9378` (inferlets) + anonimização txn.rs/kpi.rs (testes rodando) |
| touring-wt-c2 | feat/2026-11-04-c2-destrutivo | **aceito** (Kimi + eu) | `835b1ee2`…`0e522880` (C2, C3b, baseline anonimizado, relatório) + `cafdf49e` (eu: teste pre_bash) |
| touring-wt-corr (cont.) | feat/2026-11-04-correcoes | commitado | `24e8c545` (pins stale: cli-suggest Route::Table; hook_trace em FRAMEWORK) |
| touring-wt-hdoc (cont.) | feat/2026-11-04-hermetic-doctor | commitado | `e7ed37fe` (doctor do touring-generator hermético; dev-dep touring-hooks, sem ciclo) |
| touring-wt-docsan | feat/2026-11-04-docs-sem-usuario | commitado | `fb25f1a4`: 1.032 arquivos de prosa/log em docs/ com placeholder; 105 revertidos (hash citado, selos/holdouts/juízes/ledgers/dados lidos por código, diretórios com manifesto) |
| touring-wt-drift | fix/2026-11-04-drift-coverage | aceito (subagente; reverifiquei 8/8, 3/3, sync_metrics rc=0) | `997f6ef1`: scanner com ROOT do próprio checkout (media o repo principal); ARCHITECTURE.md regenerado (2 crates removidos em 02/11); ledger reconciliado (136 linhas). Sugestões p/ Gabriel pós-fusão: plugar o juiz no CI/propagate; escopo por `git ls-files` |
| touring-wt-codesan | feat/2026-11-04-code-sem-usuario | **em curso — subagente sonnet** | scripts/ (24) + código em docs/ (270) derivando caminho |
| touring-wt-hdoc | feat/2026-11-04-hermetic-doctor | commitado, aceito | `412c2347`: `hermetic_doctor.rs` portado de `66189d5f`; os 2 e2e do touring-hooks verdes em worktree limpo (baseline RC=101 → 5/5 + 5/5), clippy 0, 2 mutantes mortos. `fbb60fdd`: resolvedor único `circuit_state_path()` (o doctor tinha cópia do caminho) + override absoluto `TOURING_CIRCUIT_STATE_FILE` (produção inalterada); mundo hermético dono do seu estado; novo e2e ok→open, mutante morto |

## Achados fora da lista aprovada (todos corrigidos)

- `dag-gap` disparava em todo TaskList e gravava reward falso −0,05 (C7).
- `decompose ready` sem task_id respondia sempre vazio (C7).
- 13 handlers respondiam prosa (não 11); subagent-stop nunca bloqueava na rota daemon; teammate-idle-gate fora do contrato (C4).
- session-stop violava o contrato de stdout do Stop.
- Replay do lifecycle não era hermético: 5 saídas gravadas carregavam fragmentos da memória real da máquina (`lesson:`, `recall: 2`) — as chaves reais `task:1:started` etc. casavam por similaridade. `with_claude_dir` + corpus corrigido. Não houve vazamento de memória de teste para a memória real (verificado).
- C9: fallback de gotchas para `<project_root>/docs/gotchas` = injeção de prompt por repo clonado → `default_gotcha_dir() -> Option`, só env ou `$HOME/projects/touring` existente.
- inferlets `crates_size_via_cli_wrapper.js` dava 0 crates (caminho `crates/crates/x`), comentário virava membro, `find -exec wc {} +` só somava o último lote → contagem em processo, 3 testes node + 3 mutantes mortos.

## Próximo

1. ~~anonimização txn.rs/kpi.rs~~ feito (`a3c75f2e`); em `crates/` só restam o README (URL pública) e o baseline.jsonl (Kimi; `tool_trio_tests.rs` já limpo em `642aaeff`, Kimi instruído a não tocar).
2. Julgar Kimi C2 quando reportar (replay 643/652 + JUSTIFIED, paridade block_git, testes, clippy, `git grep gabrielgadea -- crates/touring-hooks-shared` vazio).
3. Julgar hermetic-doctor (os 2 e2e verdes em worktree limpo + mutante).
4. Após 08/11 00:10, ordem de integração: aprofundamento → correções + c8 + c1 + port + c2 + hdoc → patches #9 (adw-node) → medir latência #8 em release → regenerar tabela de crates → `propagate-release.sh --dry-run` → release 30.4.103.
5. Commits intermediários da wt-corr não foram compilados um a um (só a ponta); verificar com `cargo check` por commit se for preciso bisect.

## Decisões do Gabriel (04/11/2026, respondidas)

- **D-8**: «Amostra maior» — coletar mais casos de html antes de fixar o teto (NÃO o teto próprio que recomendei).
- **teammate-idle-gate**: registrar no settings.json **após o AB34**, com kill switch.
- **TOURING_MINER_READER=on**: só **após varreduras em sombra** com 0 diferença nos dados reais pós-deploy.
- **Lote pós-fusão aprovado (as 4)**: (a) rustfmt do workspace em commit isolado; (b) usuário → placeholder em docs/ + scripts/ (client/ pela fonte viva + `sync-client-skills.py`); (c) liberar `~/.cache/touring-wt-g6-snapshot` (496 MB) após a fusão do #1; (d) remover os 6 worktrees obsoletos após medir que não guardam trabalho não commitado (REGRA #11).

## Achado D8 (04/11 ~20h) — texto da sessão prometia deny que o executor não aplica

`spec.rs`: «S3 — a 2ª avisa (D, 09/10/2026: antes negava)»; `chain.rs`: «a rajada AVISA, nunca nega» (1.272 negações/24h). Mas `code_mode_sdk_section.py` (SessionStart) dizia «volta NEGADA», e o CLAUDE.md item 10 «nega em 2ª». O guard D8 (`test_code_mode_sdk_section.py`) lia as constantes do `cli_suggester.rs`, de onde saíram em 02/11 — falhava por constante ausente, não pela deriva. `954194d4` (wt-corr): guard lê `code_mode_gates/{shell,spec}.rs` + checa o VERBO (mutantes «promete deny» e «ordinal errado» mortos); texto corrigido no espelho `client/hooks/`; CLAUDE.md corrigido; teste da wheel do CLK faz skip sem artefato (como os irmãos).
Mais dois leitores do mesmo arquivo antigo, corrigidos em `c1d…` (wt-corr, commit «two more readers»): `test_s3_burst_distribution` (falhava por constante ausente) e `audit_code_mode.py` item g1-teeth (reprovava um gate que TEM dentes; e na wt-corr ainda auditava o checkout principal por `RAIZ` fixo — corrigido pela branch code-sem-usuario). Na fusão de ensaio: auditoria sem nenhum `[FALHA]`, s3_burst 12/12. Restantes da suíte scripts/ no worktree, ambientais por construção (memória `suite-scripts-worktree-falhas-ambientais`): `complementacao` h7 (exige `.touring/touring.toml`, config local ausente no worktree), 2 testes de symlink instalado (apontam p/ a árvore principal), e os 2 de espelho×vivo (divergência real, remédio = `sync --apply` na integração). Inventário REGRA #21 definitivo = rodar a suíte na árvore principal depois da fusão real.
**PASSO PÓS-AB34 OBRIGATÓRIO**: aplicar o mesmo texto no hook VIVO `~/.claude/hooks/code_mode_sdk_section.py` (o espelho é gerado do vivo) e então `python3 scripts/sync-client-skills.py --check` (+ `--apply` para as 2 falhas de espelho×vivo que já existiam na base).

## Pós-AB34 — ordem consolidada

1. fusão real: aprofundamento → correcoes, c8-c3, c1, portabilidade, c2, hermetic-doctor, docs-sem-usuario, drift-coverage, code-sem-usuario (conflitos já resolvidos no ensaio: `standalone_table_parity.rs`, `drift_semantic_scan.py` — `git rerere` gravou);
2. hook vivo `code_mode_sdk_section.py` + `sync-client-skills.py --apply/--check`; registrar `teammate-idle-gate` com kill switch;
3. patches #9 (adw-node); `cargo build --release` + medir latência #8;
4. rustfmt isolado; `client/` sem usuário pela fonte viva; `cargo clean --profile dev` no target principal sem build ativo; liberar `touring-wt-g6-snapshot`; remover worktrees obsoletos (medir antes);
5. `propagate-release.sh --dry-run` → release 30.4.103;
6. pós-deploy: varreduras em sombra do miner reader → decidir `on`; coletar amostra maior de html (D-8).

## Fusão de ensaio (04/11, ~18h)

Worktree descartável `touring-wt-integ`, branch `trial/2026-11-04-integracao` = aprofundamento + correcoes + c8-c3 + c1 + portabilidade + c2 (`cafdf49e`) + hermetic-doctor (`fbb60fdd`). Único conflito: `standalone_table_parity.rs` (meu `is_exit_code_block` × rustfmt do Kimi) — resolvido mantendo ambos. Na árvore fundida, `git grep gabrielgadea -- crates` = só o README (URL pública). Gate completo do workspace rodando (log: scratchpad `integ-gate.log`). Disco: 97% → 89% após `cargo clean` dos targets de hdoc/port/c8/c1/corr (cache, REGRA #12); o de c2 (64 GB) é do Kimi.

**Resultado do gate da fusão de ensaio**: check/build/clippy/pytest rc=0; testes 17.917 passed / 3 failed:
(a) `touring-hooks --bin touring-hook` `rpc_endpoints_are_refused_and_the_cli_hook_is_not` esperava `Route::Override` p/ cli-suggest — OVERRIDES esvaziado no C4 (a suíte `--bin` não tinha sido rodada na wt-corr) → agora `Route::Table`;
(b) `updated_input_writers` — `hook_trace.rs` casa o detector por um fixture de teste interno (só LÊ `updatedInput`) → entra em FRAMEWORK;
(c) `touring-generator` `generator_binary_wired_and_healthy` — o mesmo doctor não-hermético (cwd do pacote) → `DoctorWorld` + dev-dep `touring-hooks` (sem ciclo).

**Gate 2 da fusão de ensaio (após `24e8c545` na wt-corr e `e7ed37fe` na hdoc) — VERDE**: build rc=0 · `cargo test --workspace --no-fail-fast` rc=0, 315 linhas `test result:`, 17.917 passed pela soma (a linha intercalada não entra direito na conta), 0 linhas FAILED (o "failed=1" da soma por awk era saída intercalada de 2 harnesses filhos na linha 7385 do log) · clippy `--workspace --all-targets -D warnings` rc=0 · pytest block_git rc=0 · os 4 testes de doctor reexecutados com `--nocapture` imprimem `E2EDOCTOR-RAN` (rodaram, não pularam). Ponta da integração de ensaio: `3846374f`.

## Higiene de disco (04/11 ~17h50, pedido do Gabriel, REGRA #12)

91% → 79% (207 GB livres, ~116 GB). `safe-clean.sh incremental` (7,9 GB) · `~/.cache/touring-wt-g6-target/debug` órfão (worktree g6 inexistente, nenhum cargo vivo; `before/touring-hook` 218 MB preservado) 70,4 GB · `cargo clean` wt-c2 69,2 GB (Kimi avisado) e wt-aprof 13,6 GB. NÃO tocados: `touring/target/debug` 118 GB (build vivo de outra sessão — protocolo proíbe; repetir `cargo clean --profile dev` quando não houver cargo ativo), `touring/target/release` 171 GB (daemon global roda dele + AB34 proíbe rebuild), `~/.cache/uv` 28 GB (`uv cache prune` travado por uv de sessões analise; não forçado), `~/.cache/touring-wt-g6-snapshot` (decisão: após a fusão do #1).

## C2/C3b (Kimi, entregue 04/11 ~17h) — ACEITO

Relatório: `RELATORIO-C2-C3b.md` (`0e522880` na wt-c2). Eu: corrigi `pre_bash.rs` (808/1 → 809/0; `git reset --hard` agora Block, aviso estrutural = `chmod -R 777`), removi o symlink-muleta `crates/touring-hooks/.claude/touring`. Pendente comigo na integração: reexport de `PreToolValidator` em `touring-dispatch/src/lib.rs` (fachada sem capacidade própria). `--all-features` do servidor = `compile_error!` intencional (alocadores exclusivos), não falha.

## Revisão em dois eixos da fusão de ensaio (05/11, madrugada) — achados e correções

Revisão `/code-review` (padrões × spec, dois subagentes sonnet independentes) sobre `4cc2d806...trial`.
Cada achado foi reconferido no código antes de corrigir. Commits novos (todos testados sob `hw-slot`):

| Worktree | Commit | O quê |
|---|---|---|
| wt-corr | `3145ed76` | **C7 não chegava aos hooks**: o `cli_decompose_status` que o dispatch importa (`cli/decompose.rs`) era uma 2ª cópia sem `open_tasks` → dag-gap seguia disparando em todo TaskList; o wrapper de `ready` descartava `scope` → todo hook lia todos os planos abertos. Ambos delegam/repassam agora; `latest_plan_subtasks` (plano ativo) é o denominador da dag-gap; ausente = silêncio; `[live]` denso só com plano aberto; pretask usa `latest`. dispatch 750 ✓ (replay inalterado) |
| wt-corr | `a36dd54f` | doc do `gate_tag` do hook_trace dizia «mesmo vocabulário do tool_trio» — não é (de propósito) |
| wt-c2 | `26d35e28` | D8: comentários `vocabulary.rs`/`pre_bash.rs` contradiziam o executor; `GIT_ROUTE` mandava «push sem force» até para reset/clean/stash; fixture com a rota 440× → placeholder `{GIT_ROUTE}`. hooks-shared 549 ✓ |
| wt-c2 | `bb1611bd` | C2 item 4: fachada `PreToolValidator` removida (módulo + 3 reexports); stringzilla_e2e lê `command_policy::judge_tool` |
| wt-c2 | `3e9d712b` | **comportamento**: X5 pesava o timeout salvo pelo executor (`-2/false`) como falha comum (0,4); decide pelo `ExitStatus` agora (0,2). Wire inalterado. ceg 616 ✓ |
| wt-c8 | `5da7b993` | C8: doc-comment do `cdn.rs` com o motivo da exceção |
| wt-c8 / wt-hdoc | `4e9611f2` / `1c8c41f2` | comentários em inglês nos arquivos novos (subagente; só linhas `//`) |
| wt-port | `ae081eaf` | `touring_claude_dir` pelo resolvedor único (`Option`; sem home = só o banco do projeto; attest falha nomeado); `NO_HOME_REASON` no lugar de 5 cópias; testes `*_dir_path` sem passe vácuo |
| wt-codesan | `f4aee537` | `holon-wasm-components/tests/e2e_run.sh` deriva os caminhos (apontava para `~/.claude/rust`) |
| wt-docsan | `65302c8b` | 62 `.pyc` rastreados (já ignorados) saem do índice |

Fusão de ensaio atualizada: `c63fe3a1` (conflito `shared.rs` resolvido: override thread-local + resolvedor; rerere gravou).

**Achados NÃO corrigidos, com o motivo** (decisão do Gabriel ou pós-AB34):
- ~316 arquivos de `docs/` com `/home/<usuário>`: são os excluídos de propósito no `fb25f1a4` (sha citado, selos, diretórios com manifesto, dados lidos por código). Corrigir quebra a integridade que eles provam; tirar de vez exige reescrita de histórico. **Decisão do Gabriel.**
- `client/` (28 arquivos, inclui hooks e `.coverage` no MANIFEST): conserto na fonte viva `~/.claude` + sync — **pós-AB34** (hooks congelados).
- 3 `.wasm` com caminhos de `.cargo`/`.rustup` embutidos: rebuild com `--remap-path-prefix` — **pós-AB34** (build).
- rustfmt misturado em commits de C2/C4 e scripts religados dentro do commit de portabilidade: separar = reescrever histórico das branches (rebase, REGRA #11) — **pedir ao Gabriel**; alternativa sem reescrita: registrar e seguir.
- Smells de julgamento não tratados (predicado «é shell?» ×4, `xargs_risky` × `is_destructive`, `prose_as_context` ×13, `transcript_miner.rs` 2.678 linhas, nomes PT em `reader.rs`): refatoração sem defeito observado — backlog.

**Gate 2 da fusão de ensaio `c63fe3a1` (05/11 ~04h55) — VERDE**: build rc=0 · `cargo test --workspace --no-fail-fast` rc=0, 316 linhas `test result:`, 17.919 passed, 0 FAILED · `touring-hook-handlers --features pre-hooks,post-hooks` 809 passed · clippy `--workspace --all-targets -D warnings` rc=0 · pytest (block_git S25 + guards D8 do code mode) 720 passed.

## Decisões do Gabriel (05/11 ~05h, AskUserQuestion)

- **docs/ com `/home/<usuário>` nos ~316 arquivos de integridade: «Aceitar e registrar»**. Ficam como estão; a
  exceção e o motivo (sha citado, selos, diretórios com manifesto, dados lidos por código; editar quebraria a
  prova) já estão no corpo do commit `fb25f1a4` da wt-docsan e aqui. Sem reescrita de histórico.
- **Commits misturados: «Registrar e seguir»**. Sem rebase. O que cada um mistura:
  - `4efee418` (C4, wt-corr): reflow rustfmt de `assert_eq!` em `tests/standalone_table_parity.rs` e linha em
    branco dupla em `src/standalone.rs`; `dd047d9a` também toca o mesmo teste;
  - `fbb60fdd` (wt-hdoc): linha em branco dupla em `touring-hooks-core/src/circuit_breaker.rs`;
  - `e122724a` (wt-codesan): `scripts/orphan-classify.py` e `scripts/doc-coverage.py` passam a medir o
    checkout em vez da árvore congelada `~/.claude/rust` (muda o que medem).
  O rustfmt isolado do pós-AB34 (item 5 da ordem consolidada) absorve as linhas em branco.

## Backlog de smells (05/11 ~06h) — tratado

| Worktree | Commit | O quê |
|---|---|---|
| wt-c2 | `15081dad` | «é shell?» em 3 cópias → `lexer::SHELLS` + `is_shell` (veredito inalterado, corpus 549 ✓) |
| wt-c8 | `7e67e9d4` | `Familia{Run,Recusa,NaoObjeto}` → `LineFamily{Run,Refusal,NotObject}`; rustfmt nas linhas da branch |
| wt-corr | `9975475f` | contagens do `cli_decompose_status` viram `null` em erro (não 0); rustfmt nos arquivos da branch |

Não tratados, com motivo: `xargs_risky` não duplica `is_destructive` (marca subcomando cujos argumentos chegam por
stdin, outra pergunta); os 13 `prose_as_context` são a declaração explícita por entrada da tabela de dispatch, não
lógica repetida; `transcript_miner.rs` (2.678 linhas) fica intocado até as varreduras em sombra decididas pelo Gabriel
— dividir antes misturaria refactor estrutural com a medição.

**Gate 2 da fusão de ensaio `db4b9f8c` — VERDE**: build/test/handlers/clippy/pytest rc=0 · 17.919 passed, 0 FAILED ·
handlers 809 · pytest 720.

## .wasm sem o usuário (05/11 ~06h30, decisão do Gabriel: «Instalar e reconstruir já»)

Instalados: target `wasm32-unknown-unknown`, `wasm-bindgen-cli` 0.2.126 (= Cargo.lock), `trunk` 0.22.1 (em
`~/.cargo/bin`, fora do PATH padrão — exportar `PATH=$HOME/.cargo/bin:$PATH`). wt-port `7ba455c5`: os dois
`libinferlets.wasm` e o dist do touring-web reconstruídos com `--remap-path-prefix`; 0 ocorrências do usuário.
O `libinferlets.wasm` rastreado era de 24/07 e anterior a código de inferlet posterior (60 KB → 246 KB, mesmos
imports/entradas) — a troca entra no daemon só no deploy pós-AB34. Testes: 19 inferlets e2e + 117 do crate ✓.
Fusão de ensaio atualizada com a wt-port.
Dist novo do touring-web verificado no Chrome (servidor estático local 127.0.0.1:3071): `.js` e `_bg.wasm` 200,
o app Leptos renderiza a navegação («Touring — Overview»), 0 erros no console após recarregar.

## AB34 ABORTADO e execução pós-AB34 (05/11 ~19h25 →, decisão do Gabriel: «Encerrar o AB34 agora»)

- AB34: R4 do pré-registro — timers off, units removidas, flags resetadas, mod/marketplace desinstalados, cópia
  `~/.claude/ab34-marketplace` removida (era cópia de `tools/mods/touring-compact-evict`), análise no aborto
  (válidos 0/8, inconclusivo) em `AB34-resultado-abortado-dia4.{json,txt}` (untracked no checkout vivo).
- Fusão real: worktree `../touring-wt-fusao`, branch `fusao/2026-11-05-pos-ab34` a partir da viva `be328f6b` +
  `trial/2026-11-04-integracao` sem conflito (`f44ac278`). Gate: build/test 17.939 passed 0 FAILED/handlers 809/
  clippy/pytest 720 rc=0. Depois: `7108f95b` (client sem usuário: 22 fontes vivas limpas com backup em
  `backup-live-pre-sanitize-20261105/`, 6 fontes do repo; patches #9 aplicados no vivo com backup em
  `backup-adw-pre9-20261105/`; `.coverage` fora do espelho; testes de portfolio herméticos — coordenado com
  analise-67, dona do D09) e `fix(ab34)` (marcador `{home_slug}`).
- Hook vivo `code_mode_sdk_section.py` = symlink para `client/hooks` do checkout vivo → entra no fast-forward.
- teammate-idle-gate: registrar SÓ depois do deploy do binário novo, com kill switch NO COMANDO do settings
  (o handler roda no daemon; env da sessão não chega): `if [ "${TOURING_TEAMMATE_IDLE_GATE_DISABLED:-}" = 1 ];
  then exec $HOME/.claude/hooks/touring-hook teammate-idle; else exec $HOME/.claude/hooks/touring-hook
  teammate-idle-gate; fi` substituindo a entrada `teammate-idle` atual.
- Próximo: avançar a viva para a fusão (o ff abortou só pelo `ab34-calendario.json` modificado localmente —
  commitar o local e fundir), depois `propagate-release.sh --dry-run` → confirmar com o Gabriel → release
  30.4.103 → registrar o gate → latência #8 → limpezas → inventário REGRA #21.

## Release 30.4.103 (05/11 ~21h45) e pós-release

- `propagate-release.sh 30.4.103` rc=0 na 4ª rodada (log `propagate-30.4.103.log`): 1ª parou no gate de referência
  (`modules.md` sem `user_home`, `086ac4ea`); 2ª no disco cheio (80 KB livres — `cargo clean` em 8 worktrees já
  fundidos liberou 238 GB); 3ª em 2 guardas de relógio do touring-hooks que mediam a máquina (`test(hooks)`, aquecimento
  fora do histograma + mediana de 21). Prova comportamental 43/43; analise e konverter em 30.4.103; doctor 9/9.
- teammate-idle-gate registrado no `~/.claude/settings.json` (backup `settings.json.antes-teammate-idle-gate-20261105`),
  kill switch no comando: gate on → rc 2 + instrução no stderr; `TOURING_TEAMMATE_IDLE_GATE_DISABLED=1` → gravador
  `teammate-idle` (rc 0). Handler testado no binário novo (tasks_completed 2 → libera; 0 → bloqueia).
- Latência #8 em release (`latencia-8-release.jsonl`, sonda `latencia_do_vgp_na_arvore`, 1000 × 86 casos):
  geral p50 0,060 / p95 0,123 / p99 0,151 ms; pior grupo TS p95 0,141 / p99 0,164 ms — orçamento 150 ms.

## Fechamento da REGRA #21 (05/11 ~23h)

- `5726bfd0`: rustfmt no workspace, isolado em 53 arquivos sem outra mudança.
- `64167d5f`: o drift scanner passou a contar só arquivos rastreados (`git ls-files`). Antes, o checkout vivo dava 400 contra 398 no checkout limpo, porque contava o bundle não rastreado e o relatório r98 de outra sessão. Com isso entrou a 12ª reconciliação: C1 91, total 398.
- Suíte `scripts/` inteira: **1434 passed, 26 skipped, 0 failed**.
- `test_complementacao_hooks.py`: 15/15. As 2 falhas iniciais vieram de timeout no binário frio, logo depois do rebuild que se seguiu ao `cargo clean`.
- Worktrees desta frente: 12 removidos. Os 8 antigos (f21, frag, intadapt, p0b, p0bd, roteamento, semeado, tiers) têm trabalho não commitado e ficaram como estavam. O d07-risco é de outra sessão.
- Os backups foram para `~/.local/share/touring-backups/2026-11-05/`: o vivo antes da sanitização, o adw antes do #9 e o settings.
- O cron b22ec88b, de retomada pós-AB34, foi cancelado: estava obsoleto.
- 6 worktrees antigos com trabalho não commitado agora têm snapshot em branches `safety/2026-11-05-wip-{f21,frag,p0b,p0bd,roteamento,tiers}`, todos com 0 arquivos sujos. Os commits WIP são aditivos e não foram revisados.
- **Minerador reader → `on`** (`9a0ae0fc`). A condição de 04/11 era «0 diferença em dados reais pós-deploy». O resultado de `corpus_shadow_measurement` sobre o corpus inteiro: 1.320 transcripts, 1.554.047 linhas, 6.092 pares de cada lado, only_legacy 0 e only_reader 0. Um payload antigo de `MinerSweepStats` passa a ler como `off`, via `ReaderMode::before_c1`.
  Achado: o daemon nunca gravou a linha `TranscriptMiner shadow`. O minerador roda nos `touring serve` das sessões CC, e os dois processos vivos têm binário `(deleted)`, anterior ao deploy. O `on` entra no próximo release e pega cada sessão quando ela reinicia.
- **J15/30.4.103**: o falso deny do J15 (30.4.102) era da reconstrução do estado, com `overlays: []` (`RELATORIO-10607309d143.md`). Julgar de novo no mesmo held-out reproduziria o artefato. Antes de um J16 é preciso que o construtor do corpus congele as dependências locais ou declare a entrada inconclusiva.
- **D-8**: falta população html cerca de 4× maior (n ≥ 3.700 contra 1.704 no acervo), clonando repositórios públicos. O download depende da aprovação da lista pelo Gabriel.
- Ficam para o Gabriel: `! rm -r ~/.cache/touring-wt-g6-snapshot` (496 MB) e o destino dos 8 worktrees antigos.
