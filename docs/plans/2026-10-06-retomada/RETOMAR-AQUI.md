---
type: Resume
title: Retomar aqui — 06/10/2026 (rodada de retomada pós-30.4.92)
description: Estado exato quando Gabriel pediu para parar. O que está pronto e vivo, o que está editado sem teste, o que falta e em que ordem, com os comandos.
plan_id: 2026-10-06-retomada
timestamp: 2026-10-06T10:55:00-03:00
tags: ["#kind:checkpoint", "#status:open", "#domain:release"]
---

# Retomar aqui — 06/10/2026

Sessão `touring-f9` (transcript `96191e32-8512-4e57-9400-ce83c3299d78`, Opus 5.5), parada por ordem de Gabriel
("preciso desligar, pare o trabalho e salve todo o contexto"). DAG: **`task_1791289467984112110`**.

## As ordens de Gabriel desta rodada (06/10)

1. "recomendações aprovadas": C1 (a) cláusula `cross_audit` com procedência do `--bundle`; C2 (c) contadores do
   `context_budget` por sessão, com DOIS acumuladores (projeto com `seen` global + sessão com `seen` próprio);
   C3 (a) índice de memória, com a memória do Touring como estrutura principal "sempre, em todos os casos".
2. "Já vi o vídeo do chrome, está ok" — frente fechada.
3. "Gate de integridade de prosa deve bloquear write/edit".
4. "Faça o experimento, gasto da API autorizada".
5. Usar a sessão MiniMax (`touring-b4`, transcript `133dd82b-0029-4061-be0e-ed64cb669ac3`) como colaboradora.
6. Via `analise-92` (por ordem dele): corrigir D1–D5 da 30.4.92 a partir daqui e responder com o commit/release.

## Pronto e VIVO (testado; falta commit)

| Item | Onde | Prova |
|---|---|---|
| S-01 Chrome fechado | memória `video-preto-navegador-libva-nvidia-hibrido.md` | Gabriel confirmou; `rtkit` ativo, `data-loop.0` em RR 20 |
| S-02 gate de prosa bloqueia Write/Edit/MultiEdit/NotebookEdit | `~/.claude/skills/Touring/scripts/hooks/prose_integrity_gate.py` (+ teste ao lado) | 47/47; controles C1–C6 ok; revisão adversarial da touring-b4 incorporada (cerca não fechada era bypass; pragma só após marcador de comentário; recusa diz "mesma linha"; diretório não é "conteúdo desconhecido"); `replace_all` refutado como bypass por reprodução |
| S-03 C1 juiz | `~/.claude/skills/loop-engineering/scripts/loop_converged.py` + `test_cross_audit_provenance.py` | suíte inteira do loop-engineering verde; `bundle_requested` keyword-only SEM default (revisão touring-b4: default False falhava aberto) |
| D-1 painel de abertura | `client/hooks/session_startup_intelligence.py` (traz o bloco Jev que só existia no instalado; ausência = "indisponível (motivo)"; `STATUS_TIMEOUT` 3 s; `JEV_TIMEOUT_S` 2 s; caminho do `jev_client` derivado do symlink) | `client/hooks/tests/test_d1_startup_absence.py`; painel real: "composite 0.68 · Doctor 9/9" |
| D-1 drift estrutural | `~/.claude/hooks/{session_startup_intelligence,evidence_guard,skill_picker_hook,investigate_on_start}.py` agora são SYMLINKS para `client/hooks/`; `evidence_guard.py`/`skill_picker_hook.py` versionados; testes movidos para `client/hooks/tests/` (ali o CI roda) | `test_s48_install.py` estendido a `.py` (só olhava `.sh` — por isso o drift de 01/10 passou calado); 107/107 em `client/hooks/tests`; backup dos instalados em `/tmp/claude-1000/-home-gabrielgadea-projects-touring/96191e32-8512-4e57-9400-ce83c3299d78/scratchpad/hooks-backup-20261006/` (some no reboot — os conteúdos já estão versionados no `client/`) |
| D-3 socket per-project | `client/hooks/session_env_setup.sh` (3 camadas = `TouringConfig::resolve_daemon_socket_path`) | `test_d3_session_env_socket.py` 6/6; no analise real sem env herdada resolve `…/analise/.touring/daemon.sock` |
| S-05/S-06 C3 promoção | `c3/inventario.jsonl` (v1, `touring_entry` com defeito), `c3/inventario-v2.jsonl` (critério slug-na-chave), `c3/promocao.jsonl` | 175/175 entradas na memória do Touring (`fato` 89, `licao` 64, `hub` 7, `estado` 15), 0 erro; strip de `--type` medido: 0 de 136 com perda |

## Editado e NÃO testado (rodar antes de qualquer outra coisa)

- **S-04 C2** `crates/touring-cli/src/cli/context_budget.rs`: `ContextBudgetAggregate::absorb_session`,
  `aggregate_session_lines` (o `aggregate_lines` antigo virou wrapper), `record_injection`, `session_summary`,
  payload com `unit`, `by_session`, `cross_session_duplicate_*`; campos novos em `FamilyStats`; 3 testes
  (`the_project_number_through_sessions_is_the_single_seen_scan` é o decisivo). O hook pós-edição deu
  `check(touring-cli): OK`; **`cargo test` não rodou**.
- **D-5** `crates/touring-hooks/src/daemon_main.rs`: `early_exit` lê os argumentos antes de qualquer efeito
  (`--version`/`-V`, `--help`/`-h`, desconhecido = exit 2); 4 testes unitários. **Não compilado.**

```bash
scripts/hw-slot --status
scripts/hw-slot --class rust -- cargo test -p touring-cli --lib context_budget
scripts/hw-slot --class rust -- cargo test -p touring-hooks --bin touring-daemon
```

## Não começado

- **D-2** (`status -j` 4,6 s no analise; 0,49 s aqui): `crates/touring-analysis/src/quality/signal/workspace_io.rs`
  `collect_rs_files` (`:101`, só pula `target`/`.git`/`node_modules`) → filtrar com
  `touring_foundation::gitignore::GitIgnoreRules::for_root` (`ignored_abs(&path, is_dir)`) e
  `TouringConfig::index_excluded_dirs_for(root)`. **Não usar `IndexPolicy` ali: ciclo** (touring-hooks-shared
  depende de touring-analysis). `crates/touring-cli/src/cli_e2e.rs` `collect_files_recursive` (`:1542`, denominador
  `disk_files` do e2e) → `touring_hooks_shared::index_policy::IndexPolicy`, como `crates/touring-cli/src/cli/knowledge.rs:41-79`.
  O `status` comum varre uma vez (`status.rs:237`); a outra chamada (`:190`) é só da federação. Meta: < 1 s no analise.
- **D-4**: origem da falha `Critical` no circuit breaker (06/10 09:12:25 BRT, 3 s após o spawn do daemon do analise).
  Pistas da touring-b4: `crates/touring-hooks-core/src/circuit_breaker.rs:227`; `record_failure` em
  `crates/touring-hooks/src/main.rs:800,825,838,857`; `touring.log.2026-10-06:788` "Failed to start MCP service:
  expect initialized request" (12:11:58Z) e `:789-791` FileWatcher start/stop no mesmo ms. Nada de `Critical` no log.

## Experimento CJK (S-07 feito, S-08 em curso)

- Pré-registro commitado em `071d9afb` (`experimento-cjk/pre-registro.md`); executor `cjk_experiment.py` + 23 testes.
- Piloto ok (4/4; Opus curto 1.182 tokens de entrada = sem CLAUDE.md/hooks).
- **Coleta: 136/180 válidas, 0 falha, US$ 40,69.** MiniMax 90/90; Opus `curto` 12, `longo` 16, `longo_cjk` 18.
  O processo em segundo plano morre no desligamento. **Retomar** (retomável: pula (célula, rep) já válidas;
  recusa se o pré-registro mudou):

```bash
cd ~/projects/touring/docs/plans/2026-10-06-retomada/experimento-cjk
TOURING_CODE_MODE=native python3 cjk_experiment.py run > run-summary.json 2>> run-progress.log
TOURING_CODE_MODE=native python3 cjk_experiment.py blind      # blind.jsonl + blind-key.jsonl
# avaliadores cegos: labels-f9.jsonl (touring-f9) e labels-b4.jsonl (touring-b4), {"id","label"} CORRUPT|LEGIT|UNCLEAR
TOURING_CODE_MODE=native python3 cjk_experiment.py analyze    # analysis.json
```

- **Desvio a declarar no relatório:** a coleta foi partida em dois dias (a ordem intercalada da semente segue).
- Dado extra, fora da amostra: a touring-b4 corrompeu duas mensagens desta rodada (U+628A U+5B83 U+4EEC no meio de
  "o clap os tira"); o gate do SendMessage, em sombra, avisou.

## Pendências de documentação e espelho (antes do commit)

1. `python3 scripts/sync-client-skills.py --check` e `--apply` (gate de prosa e `loop_converged.py` mudaram no live).
2. Adendo ao pré-registro de 04/10 (`docs/plans/2026-10-04-integridade-prosa/`): a decisão de Gabriel de 06/10
   (arquivos bloqueiam; `SendMessage` segue pelo critério) e as rotas de escape; atualizar o README daquele bundle.
3. CLAUDE.md do projeto, nota 32 (ainda diz "bloqueia só o SendMessage" e "em SOMBRA"): reescrever; registrar o
   contrato novo dos hooks (`.py` registrados são symlinks; S48 cobre `.py`).
4. MEMORY.md: tirar os 13 ponteiros que a touring-b4 julgou `covered` pelo CLAUDE.md (só o ponteiro; o arquivo e a
   entrada do Touring ficam): `daemon-cwd-cega-o-resolvedor` (nota 23), `daemon-herda-relaxacao-por-comando` (30b),
   `declaracao-vence-inferencia` (28), `decompose-ready-sem-claim-atomico` (9), `hook-if-regra-unica-fail-open` (13a),
   `hw-slot-semaforo-de-hardware` (31), `import-raiz-de-crate-nao-resolvia` (18c), `oraculo-da-remocao-e-o-compilador`
   (23c), `ort-provider-pelo-argv0` (19c), `parse-treesitter-sem-orcamento` (18e),
   `predicado-escrito-duas-vezes-nunca-se-encontra` (29a), `selo-de-geracao-por-walk-parcial` (15d),
   `shim-serve-toolchain-nao-o-build` (30c). Ficam (partial/not): `propagacao-rotulo-nao-prova-build`,
   `varredura-mascara-o-que-o-walk-grava`, `daemon-resolve-projeto-pelo-cwd-do-cliente`.

## Ordem para terminar

1. Testes da S-04 e da D-5 (acima). 2. D-2 com testes; D-4 investigado. 3. Revisão adversarial dos diffs Rust pela
touring-b4. 4. Release **30.4.93**: `python3 docs/elite_aggregate.py --check` (pré-voo) → bump `[workspace.package]
version` → `cargo build --release --workspace` (hw-slot) → clippy `-D warnings` + testes → `scripts/propagate-release.sh
30.4.93`, avisando a analise-92 UMA linha antes do `touring update --project` (ela pediu) e a analise-97.
5. Espelho + docs (seção anterior). 6. Terminar o experimento, rotular às cegas, analisar, relatório.
7. Commits. 8. `audit-plan-completion.sh` neste bundle — com o C1, `--bundle` sem o script agora REPROVA — e o juiz:
`loop_converged.py --task task_1791289467984112110 --bundle docs/plans/2026-10-06-retomada --rust-full --json`.
9. Responder a analise-92 com commit e versão (D1 e D3 já estão vivos via symlink; D2/D5 dependem da release).
10. Pedir a Gabriel o `judge_attest.py --attest` (ato humano; até lá os vereditos saem com advisory `file_changed`).

## Pendências que são de Gabriel

- **Rotacionar a `MINIMAX_API_KEY`**: exposta neste transcript por uma máscara de `sed` que falhou com aspas
  (lição gravada em `signature-com-default-environ-vaza-segredos.md`).
- `judge_attest.py --attest` depois de revisar a mudança do juiz.

## Coordenação

- **touring-b4** (MiniMax): fez S-05, D1/D2/D4/D5 (leitura), revisões do gate e do C1, S-06. Próximos papéis: 2º
  avaliador cego do experimento; revisão adversarial de C2/D2/D5. Regras que funcionaram: tarefa autossuficiente,
  critério numérico, `arquivo:linha`, conferir uma amostra antes de usar (o 1º `touring_entry` dela estava errado).
- **analise-92**: aguarda a 30.4.93 para revalidar painel, `status -j` e `$TOURING_DAEMON_SOCKET`; DAG dela
  `task_1791289821995785817`.
- Ticket do scout perpétuo aberto e não tratado: `task_1791289267243494248` (adw=audit).
