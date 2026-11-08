# BRIEF — touring-d0 (Kimi) · C2 (#2 veredito destrutivo) + C3b (#3 tipagem do SandboxResult)

Coordenadora: touring-02 (Anthropic). Aprovação do Gabriel em 04/11/2026: "opção A, aprovo P0 a P3".
Contexto: `docs/plans/2026-11-01-aprofundamento-arquitetural/RETOMADA.md` (seções #2 e #3).

## Onde trabalhar (exclusivo seu)

- Worktree: `~/projects/touring-wt-c2` · branch `feat/2026-11-04-c2-destrutivo` (base `4cc2d806`).
- Arquivos/crates seus: `crates/touring-hooks-shared/src/command_policy/**`, `crates/touring-hooks-shared/src/bash_ast_validator.rs`,
  `crates/touring-ceg/**`, `crates/touring-hook-runtime/src/hook_runtime.rs`, `crates/touring-hooks-core/src/hooks/pre_tool_validator.rs`
  (e os consumidores diretos que o compilador apontar ao mudar tipos do touring-ceg).
- NÃO tocar: `touring-dispatch`, `touring-hooks` (main/handlers), `touring-cli`, `touring-code`, `hook_trace.rs` — são de outras sessões.
- Proibido até 08/11 00:10 (AB34): hooks/settings vivos, daemon, `update-touring`, `cargo build --release` no `target/` principal.
  Seu worktree tem `target/` próprio: debug e testes ok.
- Todo cargo pesado via `hw-slot --class rust -- cargo …` (1 job rust por máquina; rc 75 = sem vaga, repita).

## C2 — o que fazer (commits separados, um por item)

1. **Lacunas**: `pkill -9 <x>`, `killall -9 <x>`, `kill -9 <pid>` e `sudo rm …` (qualquer `sudo` prefixando um comando
   destrutivo do vocabulário) passam a `Block` na leitura de linha de comando E na estrutural. Os testes que hoje FIXAM a
   lacuna (passam porque a lacuna existe) viram testes do comportamento novo.
2. **CEG chama `judge()`**: `crates/touring-ceg/src/gateway/pre_exec.rs::guarded_dry_run` hoje só pergunta
   `validate_command` (leitura estrutural; não vê `dd`/`mkfs`). Passe a perguntar `touring_hooks_shared::command_policy::judge`
   (dono único do vocabulário) — mantendo o `REFUSED_PROFILE_MARKER`/exit 126 do desfecho recusado.
3. **Reconciliar com o `block_git`**: o hook `~/.claude/hooks/block_git.sh` é o DONO ÚNICO da política git (REGRA #11 v2).
   As 71 formas git destrutivas simples e 83 aninhadas que só o shell nega passam a ser negadas também pelo Rust.
   `git push --force-with-lease` passa a DESTRUTIVO (o shell nega na linha 190; boas práticas Claude Code 2026 também).
   A `route` em `vocabulary.rs:214` deixa de sugerir `--force-with-lease`; passa a ensinar: push sem force, ou, se a reescrita
   for intencional, o ritual da REGRA #11 com `GIT_DESTRUCTIVE_OK=1` no próprio comando. Atualize os doc-comments
   (`command_policy/mod.rs:28,46`, `lexical.rs:25-36`, `pre_tool_validator.rs:4`) — doc não pode afirmar o contrário do código.
   Leia o `block_git.sh` como oráculo; não o edite.
4. **Superfície sem chamador (REGRA #0: integrar antes de remover)**: `HookRuntime.ctx.pre_tool_validator` não tem
   chamador de produção. Primeiro decida com evidência: o `PreToolValidator` carrega alguma capacidade que o `judge()` não
   tem? Se sim, integre-a ao `command_policy` (dono único) e ligue o chamador. Se é duplicata do `command_policy`, remova o
   campo e, sem outro consumidor de produção, o módulo. Registre a evidência da decisão no relatório. O re-export em
   `touring-dispatch/src/lib.rs:60` NÃO é seu: se ele for o único uso, me avise em vez de editar.

## C3b — depois do C2

Tipar `SandboxResult.exit_code` (`crates/touring-ceg/src/gateway/sandbox_executor.rs:101`) e o `SandboxOutcome`
(`timed_out: bool` + `exit_code: i32`, 32 refs) com `touring_foundation::run_outcome::ExitStatus {Code, TimedOut, NoExitCode}`,
o tipo que o #3 já criou. O -1/-2 só existe no fio (serialização igual à atual, byte a byte).

## Aceite (o meu juiz vai rodar tudo isto de novo; entrega sem evidência = hipótese)

- Replay dos 643/652 comandos do #2 (corpus e teste dos commits `c3ce28ac 0791ff32 dee9718a`, hoje rebaseados — procure
  pelo título no `git log`): as ÚNICAS diferenças são as fechadas de propósito (lacunas + git + force-with-lease), listadas
  uma a uma em `JUSTIFIED_DIFFERENCES` com o motivo.
- Novo teste de paridade: para cada forma git destrutiva do `block_git.sh`, o Rust também nega (e o contrário: nada que o
  shell permite passa a ser negado, salvo o justificado).
- `cargo test` de `touring-hooks-shared`, `touring-ceg`, `touring-hook-runtime`, `touring-hooks-core` e de cada crate que
  consome tipos alterados: 0 falhas. `cargo clippy --all-targets --all-features -D warnings` nesses crates: limpo
  (`touring-hook-handlers` exige `--features pre-hooks,post-hooks`).
- C3b: replay dos desfechos de run idêntico byte a byte (o teste do #3, 49.757 linhas).
- Commits pequenos, mensagem em inglês, sem nome de usuário nem `/home/<user>` em nenhum arquivo.

## Relatório (um só, ao terminar)

SendMessage para `touring-02` com: commits (hash + título), comandos de verificação com o exit code real e a contagem
(passed/failed), a lista `JUSTIFIED_DIFFERENCES`, a evidência da decisão do item 4, e o que ficou de fora com o motivo.
Escreva também `docs/plans/2026-11-04-correcoes-aprofundamento/RELATORIO-C2-C3b.md` no SEU worktree e commite.
