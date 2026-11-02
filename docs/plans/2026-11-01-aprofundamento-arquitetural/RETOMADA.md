---
type: Handoff
title: "RETOMADA — aprofundamento arquitetural (10 candidatos do architecture review de 01/11)"
description: "Estado das 10 frentes de aprofundamento no worktree feat/2026-11-01-aprofundamento; fusão só depois do AB34 (08/11 00:10) ou por decisão do Gabriel."
timestamp: 2026-11-02T00:30:00-03:00
---

# RETOMADA (01–02/11/2026)

Pedido do Gabriel: "Aprofunde todos os 10, começando pelo #1" (após `/mattpocock-skills:improve-codebase-architecture`
e "aproveite e de uma geral no touring"). DAG: `task_1793579505824457457`.

## Onde

- Worktree: `/home/gabrielgadea/projects/touring-wt-aprof`, branch `feat/2026-11-01-aprofundamento` (base `b2a4bca5`).
- Build: `CARGO_TARGET_DIR=$HOME/.cache/touring-wt-g6-target` sob `scripts/hw-slot --class rust` (disco a 95%: o
  target reaproveitado evita um novo de dezenas de GB).
- Por que worktree: o que for commitado na branch principal sai no release de 04/11, no meio do AB34; os
  candidatos mexem em hooks, daemon e KPIs. Fusão depois de 08/11 00:10.

## Estado

| # | Candidato | Estado | Commits | Verificação independente |
|---|---|---|---|---|
| 1 | Transcript reader dono da política | feito | 2f5f178b 94a71cba e2a9d87d | touring-cli 813/0 (base 802), clippy limpo, 0 consumidor lendo isMeta/sidechain |
| 2 | Veredito de comando destrutivo | feito | c3ce28ac 0791ff32 dee9718a | shared 544+1+1, core 484, handlers 809 (0 falhas), clippy --all-features limpo, dispatch/ceg check ok; replay 643 comandos com JUSTIFIED_DIFFERENCES vazio; divergência block_git 3/3 |
| 3 | Desfecho de run (tipo + journal) | feito | 2d9474e2 251c8ec3 0395912d 74de54e9 0c41991e 9257f3c5 | 7 crates rc=0 (foundation, code, quality, ceg, hooks-core, cli, server), clippy limpo (server sem --all-features: alocadores exclusivos, igual à base), dispatch check ok; replay de 49.757 linhas reais idêntico byte a byte |
| 4 | Nomes de evento de hook: fonte única | feito (fases 1 e 2) | df57547b 8a4d9e3e 3c0acc32 e422e156 8e27fd33 | dispatch 1353/0, touring-hooks 1216/0, hooks-core 485/0, handlers 809/0, clippy --all-features limpo, touring-server --tests check ok; 29 nomes × 52 payloads idênticos byte a byte ao binário anterior |
| 5 | Árvores-gêmeas não compiladas | feito | c03484d6 | 5.424 linhas removidas; nada as incluía |
| 6 | Gates do code mode fora do cli_suggester | feito | 1aee80af a63ad737 db9b22d7 21c6533d | touring-cli lib 720/0 (base 815/1: testes por cenário viraram 114 linhas de tabela), cli_suggester_e2e 20/20, g10 3/3, clippy --all-features limpo, server --tests e dispatch check ok; replay 114 sequências/444 passos + digest de 30.406 chamadas, JUSTIFIED_DIFFERENCES vazio; 29/29 mutantes mortos; latência sem mudança; 0 aresta Cargo nova |
| 7 | touring-dispatch: testes pelos handle_* | fila | — | — |
| 8 | VGP: leitura contida em caminho.rs | fila | — | — |
| 9 | adw.py: interface de nó tipada | fila | — | — |
| 10 | Crates de passagem (capnp-server, contracts) | fila | — | — |

## Achados que nasceram da execução

- #1: `context_budget` contava 1.539 turnos humanos em 65 sessões; 860 eram `isMeta` (certo: 679). Somava também
  4,2 MB de tool_result de registros `api-request-blob`.
- #4: 8 nomes `cli-*` fora do atalho do daemon; e 18 hooks do daemon sem braço standalone (inclui `pre-tool-use`,
  `post-tool-use`, `stop`, `pre-compact`, `user_prompt_submit`) respondem unknown-subcommand com o daemon fora →
  fase 2 do #4: os braços do `main.rs` derivam de `dispatch_table()`.

- #2: `command_policy` (touring-hooks-shared) é dono único do vocabulário (39 linhas). Replay de 652 comandos idêntico
  antes/depois. Lacunas fixadas por teste, não corrigidas (mudam comportamento): `pkill -9 nginx` passa; `sudo rm
  /etc/passwd` passa na superfície de linha de comando; heredoc python com `rm -rf` segue bloqueado pela leitura
  estrutural. block_git × Rust: 71 formas git destrutivas simples e 83 aninhadas só o shell nega; `--force-with-lease`
  com veredito oposto. O CEG (`pre_exec.rs:180`) só pergunta a leitura estrutural (não vê dd/mkfs). Superfície morta:
  `HookRuntime.ctx.pre_tool_validator` sem chamador de produção.

- #3: `touring_foundation::run_outcome` — `FailureKind` único, `ExitStatus {Code, TimedOut, NoExitCode}` (o -1/-2 só
  no fio), três predicados nomeados (`is_success`, `exited_zero`, `has_failure`) porque as definições antigas
  divergem (1 de 49.258 runs: exit 0 com `output-limit`). O denominador do KPI conta 58 eventos de recusa e 3 linhas
  não-objeto como runs falhos (0,92729 medido; 0,92844 sem eles). O touring-ceg ainda tem um terceiro formato
  (`SandboxOutcome` com `timed_out` bool; 32 refs `exit_code: i32`).

- #4 fase 2: a rota standalone do `touring-hook` deriva de `dispatch_table()` (13 overrides com motivo, 18 nomes só
  standalone fora da tabela, 200 `cli-*` recusados como endpoint RPC). Os 18 eventos sem braço agora rodam sem daemon.
  `cli-suggest` standalone descartava a sugestão (`let _ =`): com o daemon fora, o PreToolUse (9 registros) não
  responde nada. 11 handlers da tabela devolvem prosa em vez de JSON (viram `{}` em toda rota, inclusive o daemon).
  `pre-compact` standalone regrava o estado do bandit a partir de um runtime recém-carregado.

## Decisões do Gabriel pendentes

- #4: remover o override do `cli-suggest` (passa a sugerir com o daemon fora); os 11 handlers em prosa (embrulhar
  em Context ou remover as dicas); convergir os 13 overrides; `pre-compact` standalone regravando o bandit.

- #3: denominador do `code_mode_adherence` (excluir eventos de recusa?) antes de recalibrar o piso; tipar
  `SandboxResult.exit_code` no touring-ceg (próximo passo do mesmo aprofundamento).

- #2: fechar as lacunas `pkill -9`/`sudo rm`; CEG passar a chamar `judge()`; remover a superfície morta do
  `PreToolValidator` em touring-hook-runtime; reconciliar block_git (começar por `--force-with-lease`) depois de 08/11.

- Unificar o minerador do `touring-server` com o reader (#1): muda o que um escritor de produção grava na memória.
- Liberar `~/.cache/touring-wt-g6-snapshot` (496 MB, cópia congelada dos transcritos usada nas sondas do #1).

## Restrições

AB34 armado até 08/11 00:10 (sem release fora de 04/11, sem hooks/settings, sem `--release` no target principal).
`block_git.sh` e `loop_turn_record.py` são hooks vivos: só depois de 08/11.
