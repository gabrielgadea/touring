# RELATÓRIO — C2 + C3b (04/11/2026)

Worktree `touring-wt-c2` · branch `feat/2026-11-04-c2-destrutivo` · base `4cc2d806`.
Brief `BRIEF-touring-d0-kimi-C2-C3b.md`. DAG `task_1793805535147108333`.

## Commits

| Hash | Título | Item |
|---|---|---|
| `835b1ee2` | `test(command_policy): scrub the real username from the replay corpus` | sanitização |
| `b43a54d0` | `feat(command_policy): mirror block_git destructive forms in Rust` | C2.3 |
| `f095f2d9` | `fix(command_policy): close process-kill and elevated-delete gaps` | C2.1 |
| `2a52a30a` | `fix(ceg): judge the full command policy before guarded dry-run` | C2.2 |
| `ed6df99c` | `refactor(hook-runtime): drop the unused validator field` | C2.4 |
| `031d69c1` | `refactor(ceg): type sandbox exit statuses at the domain boundary` | C3b |
| `dd047d9a` | `test(command_policy): align hook fixtures with destructive git` | follow-up C2.3 |

O follow-up cobre as expectativas dos testes de hooks e os dois diagnósticos de clippy, sem alterar o oráculo `block_git.sh`.

## O que mudou

- C2.1: `pkill -9`, `killall -9`, `kill -9` e `sudo rm` são `Block` na leitura lexical e na estrutural.
- C2.2: `guarded_dry_run` chama `command_policy::judge`; `REFUSED_PROFILE_MARKER` e exit 126 preservados.
- C2.3: o analisador git cobre formas diretas, shells/wrappers, `xargs`, `find -exec`, substituições, stdin/heredoc e interpretadores. Texto comprovadamente dados não é executado; `--force-with-lease` é destrutivo; a rota ensina push sem force ou o ritual REGRA #11 com `GIT_DESTRUCTIVE_OK=1` no próprio comando.
- C2.4: o campo morto `HookRuntime.ctx.pre_tool_validator` foi removido.
- C3b: `SandboxResult.exit_code` e `SandboxOutcome.exit_code` usam `ExitStatus`. Inteiros ficam no fio: fallback `-2/false`, timeout legado `-1/true`, spawn sem exit `-1/false`.

## Verificação executada

Todos os comandos cargo abaixo foram antecedidos por `scripts/hw-slot --class rust --`. Exit 75 em tentativas anteriores significou espera térmica/sem vaga, sem execução de testes; essas tentativas não são contadas como resultado.

| Comando | Exit real | Resultado |
|---|---:|---|
| `cargo check --workspace --all-targets` (`b33nlr5ij`) | 0 | workspace inteiro, todos os alvos |
| `cargo test -p touring-hooks-shared --no-fail-fast` (`bxb2t3gfv`) | 0 | 549/0 lib + 1 integração + 1 doctest; 2 + 8 ignored |
| `cargo test -p touring-ceg --no-fail-fast` (`bzkmi2gvx`) | 0 | 616/0 lib + 1 integração + 4 doctests; replay opt-in ignored |
| `cargo test -p touring-hook-runtime -p touring-hooks-core -p touring-server --no-fail-fast` (`bfljjgnhm`) | 101 | runtime 578/0, core 629/0, server lib 1699/0; 8 E2E pararam por daemon debug ausente |
| `cargo build --workspace` (`bez0iu2cl`) | 0 | daemon e demais binários debug construídos no target do worktree |
| `cargo test -p touring-server --test cli_wave6_e2e --test w12_5_per_project_daemon_e2e --no-fail-fast` (`bomhx1owf`) | 0 | 5/5 + 3/3; as 8 falhas ambientais foram fechadas |
| `cargo test -p touring-hooks --all-features --no-fail-fast` (`b2spyl69y`) | 0 | **1.226 passed / 0 failed em 73 alvos** |
| `cargo test -p touring-hooks --test wave24_hook_integration_e2e task_created_event_returns_scaffolded_receipt -- --exact --nocapture --test-threads=1` (`b2ow3byi8`) | 0 | 1/1; confirma a intermitência de daemon privado vista antes da execução agregada verde |
| `cargo test -p touring-hook-handlers --features pre-hooks,post-hooks --no-fail-fast` (`b2nqy8g94`) | 101 | 808/1; única falha fora do escopo, detalhada abaixo |
| `cargo clippy -p touring-hooks-shared -p touring-ceg -p touring-hook-runtime -p touring-hooks-core -p touring-hooks -p touring-hook-handlers --all-targets --all-features -- -D warnings` (`b607ud0w4`) | 0 | limpo |
| `cargo clippy -p touring-server --all-targets -- -D warnings` (`btz0tedad`) | 0 | limpo com as features padrão |
| `python3 -m pytest -q -p no:cacheprovider scripts/test_block_git_divergence.py` | 0 | 3 passed em 0,30 s |
| `TOURING_RUN_OUTCOME_REPLAY=$HOME/.claude/touring/run_journal.jsonl TOURING_RUN_OUTCOME_REPLAY_LINES=49757 cargo test -p touring-ceg real_run_outcome_wire_replay_is_byte_identical -- --ignored --nocapture` (`bf7mk6b1u`) | 0 | 49.757 linhas; 49.259 desfechos byte-idênticos; 61 sem exit utilizável; 437 malformados |
| `../../target/debug/touring doctor -j`, no cwd `crates/touring-hooks` | 0 | `project_db`, `wiring_diagnostic` e `index_generation` ok |
| `touring-quality score crates/touring-ceg/src/gateway/sandbox_stage.rs --format json` | 0 | composite 0,96672153, Diamond, sem blockers |
| `git diff --check` | 0 | limpo |

### Exceção estrutural do clippy do servidor

`cargo clippy ... -p touring-server --all-targets --all-features -- -D warnings` (`bq938ptoj`, exit 101) chega ao `compile_error!` intencional de `src/main.rs`: `jemalloc`, `prod-allocator` e `dhat-heap` instalam alocadores concorrentes. Não é lint do código alterado nem uma configuração válida para medir. Os demais crates passaram com all-features e o servidor passou com suas features padrão, como no #3.

## Replay de comandos e paridade git

- Corpus: 643 linhas, vereditos preservados após sanitização estrutural em 643/643.
- `JUSTIFIED_DIFFERENCES` completo: `crates/touring-hooks-shared/tests/fixtures/command_policy/justified_differences.jsonl`, uma linha por diferença.
- SHA-256 da fixture: `d28eda3ca429536ca5f642e788474cec86f87e22789341bb4659958622b98433`.
- Total: **451 linhas** — 156 structural, 149 lexical, 146 pipeline.
  - 2 do item 1 (processo/sudo).
  - 440 do item 3 (formas git diretas/aninhadas).
  - 3 do item 3 (assignment executável do ritual).
  - 6 do item 3 (heredoc como dados, não invocação git).
- Há ainda 1 diferença na superfície `(tool, params)`: `kill -9 2345`, documentada em `SURFACE_JUSTIFIED_DIFFERENCES`.
- Paridade nos dois sentidos: `git_parity.jsonl` tem **279 linhas — 146 block / 133 allow**, derivadas do oráculo shell em repositórios com e sem hook ativo. A suíte da política inclui esse teste e terminou 549/0.

## Decisão do item 4

`PreToolValidator` não acrescenta capacidade ao `command_policy`:

- `validate` → `judge_tool`;
- `validate_command` → `judge_lexical`;
- `validate_params` → `judge_params`;
- `is_dangerous_tool` → `command_policy::is_dangerous_tool`;
- `severity_of` → `command_policy::severity_of`;
- `command_of` e `shell_pipelines` são reexports do mesmo módulo.

Por isso o campo não utilizado foi removido do runtime. A fachada pública permanece porque `touring-hook-runtime/src/lib.rs` e `touring-dispatch/src/lib.rs` a reexportam. A retirada do módulo/reexport em `touring-dispatch` fica para a coordenadora, conforme o brief; esse arquivo não foi editado.

## Pendências fora do escopo

1. `touring-hook-handlers/src/hooks/pre_bash.rs:1199`: `pre_bash::vgp_ordem_tests::pre_bash_responde_o_que_a_command_policy_julga` ainda espera `git reset --hard HEAD` permitido. Resultado real: 808 passed / 1 failed. O arquivo estava explicitamente proibido; sua expectativa precisa ser atualizada pelo dono para `Block`.
2. Reexport de `PreToolValidator` em `touring-dispatch/src/lib.rs`: não removido porque o arquivo é de outra sessão.
3. `--all-features` do `touring-server`: incompatibilidade intencional de alocadores descrita acima; não contornada com edição fora do escopo.

Nenhum hook/settings vivo, daemon, `update-touring`, `touring-dispatch`, `touring-cli`, `touring-code` ou handler foi editado. O link local ignorado `crates/touring-hooks/.claude/touring -> ../../../.claude/touring` foi usado apenas para reproduzir o índice no cwd dos testes de `doctor` e não aparece no `git status`.

A entrega permanece hipótese até o juiz da coordenadora executar novamente os gates.
