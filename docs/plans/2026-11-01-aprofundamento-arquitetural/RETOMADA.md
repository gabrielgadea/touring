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
| 3 | Desfecho de run (tipo + journal) | fila | — | — |
| 4 | Nomes de evento de hook: fonte única | fase 1 feita | df57547b 8a4d9e3e | dispatch 1352/0, touring-server check ok, clippy limpo |
| 5 | Árvores-gêmeas não compiladas | feito | c03484d6 | 5.424 linhas removidas; nada as incluía |
| 6 | Gates do code mode fora do cli_suggester | fila | — | — |
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

## Decisões do Gabriel pendentes

- #2: fechar as lacunas `pkill -9`/`sudo rm`; CEG passar a chamar `judge()`; remover a superfície morta do
  `PreToolValidator` em touring-hook-runtime; reconciliar block_git (começar por `--force-with-lease`) depois de 08/11.

- Unificar o minerador do `touring-server` com o reader (#1): muda o que um escritor de produção grava na memória.
- Liberar `~/.cache/touring-wt-g6-snapshot` (496 MB, cópia congelada dos transcritos usada nas sondas do #1).

## Restrições

AB34 armado até 08/11 00:10 (sem release fora de 04/11, sem hooks/settings, sem `--release` no target principal).
`block_git.sh` e `loop_turn_record.py` são hooks vivos: só depois de 08/11.
