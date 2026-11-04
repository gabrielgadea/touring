# RETOMADA — C2 + C3b (04/11/2026)

## Pedido e fronteiras

- Brief: `BRIEF-touring-d0-kimi-C2-C3b.md`, no diretório correspondente da árvore principal.
- Worktree: `touring-wt-c2` · branch `feat/2026-11-04-c2-destrutivo` · base `4cc2d806`.
- Escopo: `command_policy/**`, `bash_ast_validator.rs`, `touring-ceg/**`, `hook_runtime.rs`,
  `hooks/pre_tool_validator.rs` e consumidores diretos do tipo alterado.
- Não tocar: `touring-dispatch`, `touring-hooks` (main/handlers), `touring-cli`, `touring-code`,
  `hook_trace.rs`. Não editar `block_git.sh`, hooks/settings vivos, daemon nem `update-touring`.
- Cargo sempre por `scripts/hw-slot --class rust -- cargo …`; rc 75 = sem vaga, repetir.
- DAG Touring: `task_1793805535147108333`.

## Estado por item

1. C2.1 — commit `f095f2d9` (`fix(command_policy): close process-kill and elevated-delete gaps`).
   `pkill -9`, `killall -9`, `kill -9` e `sudo rm` são `Block` nas leituras de linha e estrutural.
2. C2.2 — commit `2a52a30a` (`fix(ceg): judge the full command policy before guarded dry-run`).
   `guarded_dry_run` chama `command_policy::judge`; marcador e exit 126 preservados.
3. C2.3 — commits `b43a54d0` (`feat(command_policy): mirror block_git destructive forms in Rust`)
   e `dd047d9a` (`test(command_policy): align hook fixtures with destructive git`).
   Analisador git consciente de dados em `command_policy/git/{mod,direct,nested,interpreter,text}.rs`;
  `--force-with-lease` é destrutivo; token só vale como assignment executável; rota ensina REGRA #11.
4. C2.4 — commit `ed6df99c` (`refactor(hook-runtime): drop the unused validator field`).
   O campo morto foi removido. A fachada pública permanece porque `touring-hook-runtime/src/lib.rs`
   e `touring-dispatch/src/lib.rs` a reexportam; todos os métodos apenas delegam ao `command_policy`.
   A remoção do módulo/reexport é follow-up da coordenadora, sem editar `touring-dispatch`.
5. C3b — commit `031d69c1` (`refactor(ceg): type sandbox exit statuses at the domain boundary`).
   `SandboxResult.exit_code` e `SandboxOutcome.exit_code` usam `ExitStatus`; `timed_out` permanece
   só como discriminador do fio legado (`-2/false` no fallback e `-1/true` no timeout explícito).
   O replay opt-in vive em `sandbox_stage.rs` e só lê o journal quando as duas variáveis de ambiente
   são fornecidas.

Sanitização prévia: `835b1ee2` (`test(command_policy): scrub the real username from the replay corpus`).
Foram 11 substituições; comparação estrutural provou 643/643 vereditos inalterados. `tool_trio_tests.rs`
pertence à branch de portabilidade e não foi editado.

## Evidências verdes

- `cargo check --workspace --all-targets` (`b33nlr5ij`): exit 0.
- Política (`b1syqx77z`): 37 passed / 0 failed / 514 filtered out.
- `touring-hooks-shared` (`bxb2t3gfv`): exit 0; 549 passed / 2 ignored / 0 failed, mais 1 teste de
  integração e 1 doctest (8 doctests ignorados). Tentativas anteriores `bv7wyztzb` e `bg2k7ib5v`
  foram exit 75, sem testes.
- CEG (`bzkmi2gvx`): exit 0; 616 passed / 0 failed / 1 ignored, 1 integration test e 4 doctests.
  A primeira tentativa (`bmdjxcczq`) terminou com exit 75 por temperatura, sem testes.
- Servidor, repetição `bomhx1owf`: exit 0; `cli_wave6_e2e` 5/5 e `w12_5_per_project_daemon_e2e`
  3/3 depois do build debug do workspace.
- `touring-hooks --all-features` (`b2spyl69y`): exit 0; **1.226 passed / 0 failed em 73 alvos**
  (19 filtered out no total agregado). A falha intermitente anterior de `task-created` também passou
  isolada em `b2ow3byi8` (1/1); os dois testes de `doctor` passaram na execução agregada.
- Replay de comandos: 643 linhas; 451 diferenças justificadas (2 do item 1 na leitura de linha +
  449 do item 3), mais 1 diferença de superfície do item 1 (`kill -9 2345`).
- Paridade git: fixture `git_parity.jsonl` com 279 linhas — 146 block / 133 allow — derivada do
  oráculo shell em repositórios com e sem hook ativo. `scripts/test_block_git_divergence.py`: 3 passed.
- Replay C3b (`bf7mk6b1u`): exit 0; 49.757 linhas processadas, 49.259 desfechos comparados byte a
  byte, 61 sem exit utilizável e 437 malformados; 1 passed / 0 failed / 616 filtered out.
- Runtime/core/server (`bfljjgnhm`): runtime 578 passed / 2 ignored; core 629 passed / 1 ignored;
  server lib 1699 passed / 1 ignored; demais alvos verdes, exceto os dois E2E abaixo.
- `touring wiring orphans -j`: uma medição anterior retornou 0; a repetição após os edits retornou
  66 órfãos já presentes no índice do worktree. C3b não introduz símbolo `pub` novo (o replay é um
  teste privado); não se afirma zero órfão global com base nessa saída divergente.
- Qualidade de `sandbox_stage.rs` após o replay: composite 0,96672153, Diamond, sem blockers.

## Falhas já diagnosticadas

- Na execução `bfljjgnhm` (exit 101), `cli_wave6_e2e` teve 5 falhas e
  `w12_5_per_project_daemon_e2e` teve 3. Todas pararam antes da ação porque `touring-daemon`
  não existia em `target/{debug,release}` do worktree. Não houve falha funcional observada nesses
  testes. `cargo build --workspace` terminou com exit 0; `bomhx1owf` repetiu os dois alvos com
  5/5 e 3/3, exit 0.
- O replay C3b primeiro mostrou quatro imports não usados na biblioteca (`decision.rs`,
  `dry_run_cache.rs`, `speculative.rs`, `supervised.rs`). Eles agora são `#[cfg(test)]`;
  a suíte CEG final (`bzkmi2gvx`) confirmou 616/0, sem warnings de import na saída.
- Primeira suíte `touring-hooks --all-features` (`bifq5gcky`, exit 101): dois testes de `doctor`
  não achavam os bancos locais do crate; `touring index rebuild --dir crates/touring-hooks`
  indexou 100 arquivos com 0 erros. Como o `doctor` resolve o projeto pelo cwd do cliente, foi
  criado o link local ignorado `crates/touring-hooks/.claude/touring -> ../../../.claude/touring`;
  `../../target/debug/touring doctor -j` no cwd do crate então confirmou `project_db`,
  `wiring_diagnostic` e `index_generation` ok. O link não aparece no `git status` nem será entregue.
  Três expectativas git em `wave_v429_ast_grep_hooks_e2e.rs` foram atualizadas para `Block`, e
  `standalone_table_parity.rs` agora exige explicitamente a única diferença justificada
  `pre-bash#git-force`. O caminho real presente no teste foi sanitizado.
- Primeiro clippy (`bxuic7zs3`, exit 101): `collapsible_if` em `nested.rs` e `type_complexity`
  na fixture. Corrigidos com let-chain e o alias `JustifiedDifference`. A execução `bq938ptoj`
  passou por esses pontos e parou só no `compile_error!` intencional de `touring-server` contra
  `jemalloc` + `prod-allocator` + `dhat-heap` com `--all-features` (a mesma restrição medida no #3).
  Os demais crates passaram com all-features (`b607ud0w4`, exit 0) e o servidor passou com as
  features padrão (`btz0tedad`, exit 0), como no #3.
- Handlers (`b2nqy8g94`, exit 101): 808 passed / 1 failed. A única falha é
  `pre_bash::vgp_ordem_tests::pre_bash_responde_o_que_a_command_policy_julga`, em `pre_bash.rs:1199`,
  que espera `git reset --hard HEAD` permitido. Está proibido editar esse arquivo; informar à
  coordenadora no relatório final. A tentativa anterior (`bq1bpyp90`) foi exit 75, sem testes.
- Cinco esperas iniciais de cargo terminaram com exit 75 por temperatura >88 °C, sem testes.
  O replay também aguardou temperatura antes de executar; rc 75 nunca foi contado como falha de código.

## Fechamento e pendências da coordenadora

1. Commitar `RETOMADA-C2-C3b.md` e `RELATORIO-C2-C3b.md` após a verificação de sanitização.
2. Enviar UM SendMessage para `touring-02` com os sete commits de código/teste, o commit de docs,
   comandos/exit codes reais, `JUSTIFIED_DIFFERENCES`, decisão do item 4 e as duas pendências abaixo.
3. Coordenadora/dono: atualizar `pre_bash.rs:1199` para esperar `Block` e decidir a retirada do
   reexport de `PreToolValidator` em `touring-dispatch/src/lib.rs`. Nenhum dos arquivos foi tocado.
4. A entrega continua hipótese até o juiz da coordenadora. Não se declara 0 falhas totais enquanto
   a expectativa proibida de handlers estiver aberta; o resultado real é 808/1.
5. `loop_converged.py` foi executado duas vezes e terminou exit 1. Depois de registrar os cinco itens
   na DAG, `dag_done` ficou 5/5 e a única cláusula aberta restante é F2.4 em
   `touring-code/src/vgp/lang/html/tests/recurso.rs:416` (`high-entropy-literal` num nome fictício de
   arquivo). O arquivo está inalterado nesta branch e seu crate é proibido; convergência global NÃO
   alcançada. Evidência completa na seção "Juiz de convergência" do relatório.

## Arquivos tocados

- Política: `command_policy/{mod,lexical,vocabulary,tests,bash_ast_validator}.rs` e `git/**`;
  fixtures `baseline.jsonl`, `git_parity.jsonl`, `justified_differences.jsonl`.
- CEG: `pre_exec.rs`, `sandbox_stage.rs`, `sandbox_executor.rs`, `decision.rs`, `dry_run_cache.rs`,
  `fast_path.rs`, `speculative.rs`, `supervised.rs`, `static_stage.rs`.
- Runtime/core: `hook_runtime.rs`, `hooks/pre_tool_validator.rs`, `sandbox_output_store.rs`.
- Consumidores: `touring-hooks/benches/dry_run_cache.rs`, `touring-hooks/tests/context_mode_e2e.rs`,
  `touring-server/src/cli/exec.rs`, `touring-server/src/tools/ctx_execute_tools.rs`.
