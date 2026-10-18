---
type: Report
title: RELATORIO-E2EHERM — wave25_task_enrichment_e2e hermético
plan_id: 2026-10-08-vgp-motor-base
timestamp: 2026-10-10T23:30:00Z
autor: e2eherm (engenheiro; coordenadora touring-09)
brief: briefs/2026-10-10-e2eherm.md
tags: ["#kind:report", "#status:done-with-1-foreign-failure", "#domain:hooks", "#domain:testing"]
---

# RELATORIO-E2EHERM

Evidência em `B/diagnostics/e2eherm/` (B = `/home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base`);
índice de hashes em `B/diagnostics/e2eherm/SHA256SUMS`. Os números abaixo vêm desses arquivos.

## 1. Identidade

- Engenheiro `e2eherm`, modelo Sonnet 5.5, sob a coordenadora touring-09. Brief: `B/briefs/2026-10-10-e2eherm.md`.
- Worktree `/home/gabrielgadea/projects/touring-wt-e2eherm`, branch `fix/2026-10-10-e2eherm`, base `c118aa52`.
- `CARGO_TARGET_DIR=/home/gabrielgadea/projects/touring/target/wt-e2eherm` (exclusivo; perfil `dev`).
- Todo `cargo` passou pelo `hw-slot --class rust`; a fila por temperatura (88 °C) custou horas, nunca contornei.

## 2. O que foi feito, com evidência

**Causa-raiz (medida).** O teste passava na árvore principal e falhava num worktree por DUAS razões que o `start_in(…, Some(&workspace_root()))` não cobre:
1. O daemon escolhe o projeto pelo cwd/ambiente do **cliente** que conecta; `TOURING_PROJECT_ROOT` no spawn do daemon é só o default.
   `scout()` herdava o cwd do cargo (`crates/touring-hooks` → raiz do worktree, sem estado) — daí o contexto vazio.
2. A recall é **federada**: `discover_canonical_dbs(primary, $HOME/.claude, "memory.db")` lê todo `memory.db` sob o HOME. Sem trocar o HOME,
   um sinal semeado disputa o top-2 com a memória real da máquina (no experimento exploratório o seed ficou fora do top-2 da consulta de EnterPlanMode).

**Conserto** (`git diff c118aa52`, 2 arquivos, +305/−46; patch `B/diagnostics/e2eherm/e2eherm.patch`, 466 linhas):
- `tests/common/private_daemon.rs` — só ADICIONA: `ProjectPin {root, home}`, `PrivateDaemon::start_isolated`, `PrivateDaemon::client_command`,
  `drive` (extraído do corpo de `run_with_stdin`, comportamento idêntico) e `daemon_answers(socket, pin)` (o probe de prontidão também sai do mundo fechado).
  No daemon isolado: `HOME`, `CLAUDE_PROJECT_DIR`, `TOURING_PROJECT_ROOT`, cwd = tmp; `TOURING_EMBED_DEVICE=cpu`; `TOURING_FASTEMBED_CACHE` = cache de modelos
  da máquina real (somente leitura). Sem este último o daemon com HOME falso **baixou 417 MB** da rede (medido no experimento) e estourou o orçamento de 15 s.
- `tests/wave25_task_enrichment_e2e.rs` — fixture `Hermetic` (tempdir com `project/` e `home/`, 1 daemon isolado por teste, `Drop` mata pelo PID próprio);
  `control_then_seed`; testes 1 e 2 reescritos: CONTROLE (raiz vazia ⇒ contexto exatamente `""`), SEMENTE (`memory store` pelo binário contra o socket privado),
  asserção `contains("memory: <chave semeada>")`. O teste imprime `E2EHERM-RAN …` ao fim e `E2EHERM-SKIP …` se o binário não existir.
- Achado extra, mesma família do SKIP silencioso: o arquivo tinha cópia própria de `touring_binary()` que só olhava `<workspace>/target`, ignorando `CARGO_TARGET_DIR`
  e a raiz que compilou o teste; num worktree com target próprio TODOS os testes saíam pelo SKIP e "passavam". Agora delega a `private_daemon::locate_binary("touring")`.

## 3. Estado por item do brief

| Item | Estado | Evidência |
|---|---|---|
| 1. raiz tmp + daemon privado + sinal semeado | FEITO | `Hermetic::start`, `seed_memory`; `10-gatesA…out`, `11-suite-clean-env.run.log` |
| 2. asserção afirma o sinal semeado | FEITO | `memory: e2eherm-seed-task-create` / `…-plan-mode` nas linhas `E2EHERM-RAN` |
| 3. controle negativo | FEITO | mesma raiz/daemon/payload, antes da semente: `control=""` nos dois testes; a asserção `assert_eq!(before, "")` também denuncia estado ambiente vazando |
| 4. mutante no extrator de `prompt` | FEITO | `20-mutant-chain.out`: vermelho e depois verde restaurado (abaixo) |
| Armadilha SKIP | FECHADA | binários em `target/wt-e2eherm/debug/{touring,touring-daemon}`; 0 linhas `SKIP`/`failed` nos logs; 2 linhas `E2EHERM-RAN` por execução |
| Armadilha semente na memória real | FECHADA | HOME+root+cwd tmp nos dois lados; prova de contagem e de bind read-only abaixo |
| Teste que passa sem provar | FECHADA | controle + mutantes A e B |

**Baseline reproduzido** (HEAD + só a correção do localizador, para o binário ser achado), worktree, env limpo e env da sessão:
`4 passed; 2 FAILED` — `task_create_payload_returns_enriched_context` (L106) e `enter_plan_mode_prompt_is_treated_as_subject` (L123) (`00-…`, `01-…`).

**Final**: `6 passed; 0 failed` em env limpo (2,25 s), em bind read-only (1,96 s), em env da sessão (2,10 s) e da sessão + read-only (1,40 s).

**Mutantes** (cadeia numa só vaga; o handler vive no daemon, então cada fase recompila `touring-daemon` e reexecuta o mesmo exe de teste; reversão POR CONTEÚDO via `mutate.py`, que exige exatamente 1 ocorrência):
- A — `for key in ["subject","description"]` (sem `"prompt"`): `5 passed; 1 FAILED` — só `enter_plan_mode_prompt_is_treated_as_subject`
  (`EnterPlanMode with prompt must surface the seeded memory …; got: `), `task_create` segue verde.
- B — `parts.push(format!("memory: …"))` removido: `4 passed; 2 FAILED` — os dois testes de enriquecimento (o controle passa, a asserção da semente cai).
- Restaurado: `6 passed`, sha256 de `pretask.rs` idêntico ao original (`3af3bbe2…fa18`) e `git diff --stat -- crates/touring-cli` vazio.

**Gates**:
- `cargo fmt --all -- --check` rc=0.
- `cargo clippy -p touring-hooks --all-targets -- -D warnings` rc=0.
- `cargo test -p touring-hooks --no-fail-fast`: 69 resultados de suíte, **1171 passed, 1 failed** (rc=101). A falha, com a linha:
  `cli_suggester_e2e::classifier_edit_rust_emits_pre_edit_triage_with_tdg_gate` — `crates/touring-hooks/tests/cli_suggester_e2e.rs:269`, `assertion failed: ctx.contains("pre-edit-triage-rust")`.
  **Não é do meu patch**: o arquivo não inclui o helper que alterei, e o diff NÃO commitado da árvore principal para esse mesmo teste descreve a causa
  (o orçamento do turno, 2 000 B, esgota entre o Read e o Edit e o cabeçalho do cluster some; o conserto adiciona `reset_turn` + `cli_suggester.rs`, +454 linhas de produção
  em andamento por outra sessão — provavelmente `hooks-e2e-edit-rust`). Não toquei nele: é produção alheia sob edição ativa (REGRA #19/#21: registrar e coordenar).
  **Não reproduzi a falha sem o meu patch** — a conclusão vem do isolamento do arquivo e do diff da árvore principal. `wave25_task_enrichment_e2e` dentro dessa corrida: 6 passed.
- `cargo check` dos alvos de teste das outras 3 crates que incluem o helper por `#[path]` (touring-generator 1, touring-integration-tests 5, touring-server 5): rc=0 (`40-gatesC…`).

**Prova dos bancos reais** (`dbcount.py`: 20 DBs em `~/.claude/touring`, `~/.touring`, `projects/touring/.claude|.touring`, `projects/touring-wt-e2eherm/.claude|.touring`; abertos em `file:…?mode=ro`, contagem de linhas de toda tabela):
- Janela da suíte (env limpo, A→B, ~2 s): 20 DBs, **263 tabelas idênticas, 0 alteradas, 0 arquivos com mtime/size novo**.
- Janela em **bind read-only** (`bwrap --ro-bind` de `~/.claude`, `~/.touring`, `projects/touring/.claude`, `projects/touring/.touring`, `projects/touring-wt-e2eherm/.claude`; C→D): suíte `6 passed` e 263/263 idênticas.
  Escrita nessas árvores é EROFS para qualquer processo da suíte; como passou, a suíte não depende de escrever lá.
- Controle ambiente (E→F, sem suíte): 263/263 idênticas.
- Env da sessão (S1→S2): 258 idênticas, **5 alteradas** em `projects/touring/.claude/touring` (`knowledge.db bash_outcomes` +1; `memory.db` `memories_fts_data/idx`, `tags_fts_data/idx`).
  Repetido DENTRO do bind read-only (R1→R2): as MESMAS 5 tabelas mudaram (`bash_outcomes` +1 de novo; `tags_fts_data` 184→171). Como nenhum processo da suíte pode escrever ali, a deriva é
  ambiente: hooks da própria sessão do Claude Code e manutenção FTS de daemons vivos. `memory_entries` e `memory_tags` não mudaram em nenhuma janela.
- Janela larga (~40 min por causa da fila do hw-slot, `snap/wide40min.*`): 35 tabelas alteradas, 2 arquivos com mtime novo — é por isso que apertei as janelas; número NÃO atribuível à suíte.

## 4. Os processos

- Daemons privados: 2 por execução (um por teste hermético), sockets `/tmp/touring-e2e-{task-create,plan-mode}-<pid>.sock`, encerrados pelo `Drop` (kill pelo PID que o teste criou) com watchdog de 90 s como rede.
  Depois das corridas: 0 sockets e 0 tempdirs `touring-e2eherm-*` sobrando, 0 daemon meu vivo.
- Os testes 3–6 continuam no daemon compartilhado `shared()` (watchdog 90 s), como antes.
- Experimento exploratório (11:02 BRT): daemon scratch com `TOURING_IDLE_TIMEOUT_SECS=40` — encerrou sozinho. Nenhum `kill`/`pkill` meu em processo touring.
- Builds: baseline 9 min 41 s (`touring` + `touring-daemon`), depois incrementais de ~11 s por fase de mutante.

**INCIDENTE (registro pedido pela coordenadora).** ~14:02Z (11:02 BRT), experimento exploratório `exp1.sh` com binários release da árvore principal: daemon scratch com `TOURING_PROJECT_ROOT=tmp`
e `HOME=tmp`, mas o CLIENTE com cwd `/home/gabrielgadea/projects/touring`. Um `touring memory store e2eherm-seed-wiring-orphans …` gravou 1 linha na base REAL
`/home/gabrielgadea/projects/touring/.claude/touring/memory.db` (`memory_entries`, tier semantic). **Causa**: o daemon resolve o projeto pelo cwd/ambiente do cliente — exatamente a lição da memória
`daemon-resolve-projeto-pelo-cwd-do-cliente`, que eu tinha disponível e não apliquei ao desenhar o experimento (pinei só o daemon). Avisei a coordenadora na hora, com a evidência.
**Remediação feita pela coordenadora (~14:12Z)**: removeu numa transação a entrada e as dependentes (memory_entries 1, memory_tags 3, embeddings 1, memory_coserved 18; FTS limpo pelos triggers; `integrity-check` de
`memories_fts`/`tags_fts` ok). Eu não toquei no banco. Minhas baselines de contagem são posteriores (snapshots de 14:12:37–14:12:44 BRT = 17:12Z). Daí em diante todo experimento meu rodou com cwd, HOME e raiz em tmp, cliente e daemon.

## 5. Os dados, com sha256

| Arquivo | sha256 |
|---|---|
| `B/diagnostics/e2eherm/e2eherm.patch` (466 linhas; `git diff c118aa52` do worktree) | `8b2cfcc3d22fe60c1a2089499c1a236399a17d796b65042859a8300235b854a9` |
| `crates/touring-hooks/tests/wave25_task_enrichment_e2e.rs` (final) | `da28a0b03baff469f3a1916f73fd9b63f66fb16e369e93a5d14aced24ab8218a` |
| `crates/touring-hooks/tests/common/private_daemon.rs` (final) | `f8197b104f9bc8ea0a6e79988252095ec674ff5990a5d11e1acc10ca0510b46d` |
| `crates/touring-cli/src/cli/pretask.rs` (original = restaurado) | `3af3bbe2cc4aa69c09d468cdc33bd9ca77283d7481c4c5361c896fb021d5fa18` |

O resto (logs de baseline, gates, cadeia de mutantes, snapshots A–F/S/R, scripts `dbcount.py` `proof_inner.sh` `mutant_cycle.sh` `mutate.py`…) está em `B/diagnostics/e2eherm/` com `SHA256SUMS` (38 entradas).

## 6. Decisões e riscos

- **Um daemon por teste hermético** (2 por execução, ~2 s no total) em vez de um compartilhado: a sequência controle→semente→asserção muda o estado da raiz, então raízes e daemons por teste evitam corrida entre os dois testes em paralelo.
- **Só ADICIONEI ao helper** (15 includers em 4 crates, todos com `#[allow(dead_code)]` no `mod`): `start_in`/`run_with_stdin`/`shared()` mantêm o comportamento. `cargo check` das 11 alvos de teste das outras 3 crates passou; nenhuma delas foi executada.
- **Duplicação consciente**: `real_fastembed_cache()` repete a regra de `touring_storage::…::fastembed_cache_dir()` (o helper é incluído por crates sem a dependência). Se aquela regra mudar, esta precisa acompanhar — está comentado.
- **`TOURING_EMBED_DEVICE=cpu`** no daemon isolado: cada daemon tomaria 2 GiB de VRAM numa GPU de 8 GiB dividida por 6+ sessões.
- **Asserção `before == ""`**: forte de propósito (qualquer estado ambiente que vaze derruba o teste), mas acopla o teste a «raiz vazia ⇒ nenhum dos três sinais». Se alguém passar a emitir um hint fixo em raiz vazia, o controle quebra — e isso é o aviso certo.
- **`--timeout`**: um `--timeout N` ANTES de um subcomando clap (`memory store`) dá `unrecognized subcommand 'N'`, e DEPOIS dá `unexpected argument`; só os comandos de parse manual o entendem. O default do cliente já é 120 s, então o fixture não passa a flag.
- **Resíduo de disco**: (a) `<worktree>/target` (1,9 GB), criado por um hook PostToolUse que roda `cargo check` no alvo padrão do projeto — não é meu build; (b) `…/scratchpad/e2eherm/root.4ZWIVA` (417 MB, o modelo baixado no experimento). Não apaguei (`rm -r` é seu).
- Observação, não verificada como defeito: todo daemon sobe `capnp-embed` em `/run/user/1000/holon/{registry,generator}.sock`, caminhos GLOBAIS (o `ss` mostrava vários daemons ouvindo o mesmo caminho). Preexistente, vale para todo daemon privado dos e2e.

## 7. Git

```
$ git -C /home/gabrielgadea/projects/touring-wt-e2eherm status --short
 M crates/touring-hooks/tests/common/private_daemon.rs
 M crates/touring-hooks/tests/wave25_task_enrichment_e2e.rs
$ git -C … log --oneline -1
c118aa52 docs(vgp): estado do plano original após o release de 09/10 — o que falta
```
Nada commitado nem empurrado. `crates/touring-cli` sem diff (mutantes revertidos por conteúdo, sha conferido). A INT97 aplica `e2eherm.patch` contra `c118aa52`.

## 8. O que NÃO foi verificado

- A falha `cli_suggester_e2e::classifier_edit_rust_emits_pre_edit_triage_with_tdg_gate` não foi reproduzida sem o meu patch nem corrigida (produção alheia em edição); o gate «0 falhas» do `touring-hooks` portanto está em **1 falha estrangeira**.
- Os testes 3–6 seguem no estado ambiente (daemon compartilhado, HOME real): `read_only_task_tools_suppress_wiring_hint` (4) é **vácuo** onde não há órfãos (num worktree sem índice nunca acharia «wiring orphans:» de qualquer jeito). Não migrei porque o brief pede o mínimo; fica como dívida.
- O caminho `E2EHERM-SKIP` (binário ausente) existe mas não foi exercitado; só provei que, com os binários presentes, ele não é tomado.
- Não verifiquei se o embedder semântico (ONNX) de fato carregou do cache real dentro do daemon isolado; as asserções passam também com o fallback de hash, e o bind read-only mostra apenas que nada foi escrito.
- Perfil `release`, CI sem modelos/binários, e rodada com a suíte inteira do workspace não foram feitos; `clippy` e execução dos e2e das 3 crates vizinhas que incluem o helper também não (só `cargo check`).
- Contagem de linhas prova ausência de INSERT/DELETE; não prova ausência de UPDATE in-place (para isso valeria o bind read-only, que cobre as árvores listadas, não `/tmp` nem o HOME inteiro).
- A deriva ambiente de 5 tabelas na janela da sessão foi atribuída por exclusão (a mesma deriva ocorre com a suíte impedida de escrever); não identifiquei o processo escritor.
