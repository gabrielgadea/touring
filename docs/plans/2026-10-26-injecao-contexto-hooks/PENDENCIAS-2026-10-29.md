---
type: Audit
title: "Pendências do plano de injeção de contexto dos hooks — estado real em 29/10/2026"
description: "Classificação item a item (FEITO / PENDENTE / BLOQUEADO / SUPERADO) com evidência do estado atual do código. Documento é intenção; aqui cada status aponta para commit, arquivo:linha ou teste."
plan_id: 2026-10-26-injecao-contexto-hooks
okf_version: 1
timestamp: 2026-10-29
---

# Pendências — injeção de contexto dos hooks (29/10/2026)

**Método**: leitura do plano (`RETOMADA-AQUI.md`, `ANALISE-CONSOLIDADA.md` §7/§8/§11/§12,
`IMPACTO-POR-HOOK.md`, `round4/`) e verificação do **estado atual** por `git grep`/`git log`/leitura
de arquivo, mais execução dos dois scripts leves do próprio plano
(`scripts/test_injection_policy.py`, `scripts/guard_injection_policy.py`).
Nenhum arquivo foi alterado, nenhum commit, nenhum deploy, nenhum toque em `settings.json` ou em
hook vivo.

**Convenção de status**: `FEITO` = evidência no código atual · `FEITO-PARCIAL` = parte entregue,
parte não · `PENDENTE` = falta o quê e o próximo passo · `BLOQUEADO` = impedido por janela/regra ·
`SUPERADO` = o plano sucessor ou outro commit resolveu · `NÃO VERIFICADO` = não deu para checar.

**Plano sucessor usado como fonte de "FEITO aqui"**: `docs/plans/2026-10-26-touring-janela-eficacia/PLANO.md`
(commit `fc4c8252` = S-13/B2, `09871099` = S-14/B1, `62082b20` = S-16/B3m, `6a89128d` = nível ao daemon,
`96113cd4` = journal Python).

---

## 1. Tabela

| # | item | status | evidência | próximo passo |
|---|---|---|---|---|
| 1 | Política nativa em Rust (módulo + taxonomia + journal) | FEITO | `crates/touring-foundation/src/injection_policy.rs` (55.702 B), `injection_family.rs` (12.217 B), `injection_journal.rs`; re-export em `touring-foundation/src/lib.rs:78` e `touring-hooks-shared/src/lib.rs:72`; teste `the_rust_policy_mirrors_the_python_policy` (`injection_policy.rs:919`) | — |
| 2 | Aplicação da política nos sítios (§8.3, passos 3-5) | FEITO | `crates/touring-hooks/src/main.rs:723,763` (`SITE_DAEMON_FORWARD`), `:786` (`SITE_STANDALONE`); `crates/touring-hooks-core/src/hook_response.rs:178` (`SITE_EMIT_CORE`); `crates/touring-hook-runtime/src/hook_runtime.rs:203` (`SITE_EMIT_RUNTIME`); `crates/touring-hook-runtime/src/verdict.rs:83,166` (`SITE_VERDICT` via `retain_allowed`) | — |
| 3 | §8.7 passo 0 — unificar o tipo `HookResponse` | PENDENTE | o enum duplicado continua: `crates/touring-hook-runtime/src/hook_runtime.rs:61` `pub enum HookResponse`; `lib.rs:82` reexporta `runtime::{HookResponse,…}` e `lib.rs:98` reexporta `touring_hooks_core::hook_response`. Sem furo funcional (a política roda nos dois emits), mas o tipo duplicado persiste | remover o enum duplicado e reexportar o canônico em `runtime/mod.rs:21-23`; gate `cargo test -p touring-hook-runtime` |
| 4 | §8.7 passo 2 — taxonomia de família movida para `touring-foundation` | FEITO | `crates/touring-foundation/src/injection_family.rs` (`FAMILY_MARKERS:88`, `EMITTERS`); `crates/touring-cli/src/cli/context_budget.rs:204` reexporta `injection_family::{attributed_family, injection_family}` | — |
| 5 | §8.7 passo 6 — telemetria `injection_suppressed_*` + nível no KPI | FEITO-PARCIAL | journal Rust `crates/touring-foundation/src/injection_journal.rs` grava o mesmo arquivo do Python (`client/hooks/injection_policy.py:285-289` `JOURNAL_FILE=injection_suppressed.jsonl`); o KPI expõe `injection_suppressed_by_family` e `injection_suppressed` (`context_budget.rs:1008-1021`, teste `:2087`). **Falta**: os contadores no formato `gate_metrics.rs` do §8.5 (0 hits de `injection_suppressed` em `gate_metrics.rs`/`gate_metrics_snapshot.rs`) e o KPI **não expõe o nível corrente** (0 hits de `injection_level`) | expor `injection_level` + `injection_suppressed_bytes_count`/`_blocks_count` no `kpi`/`gate-metrics` |
| 6 | §8.7 passo 7 — A/B de corpus com os 3 braços da política | PENDENTE | `crates/touring-hooks-shared/tests/s8_corpus_ab.rs:102` `enum Modo {Total, DeployFiel, Combinado}`: mede **bytes**, não desfecho, e **não tem braços da política** (0 hits de `NoAdvisory`/`InjectionLevel`) | harness novo com modelo no circuito; o quase-experimento A3 (sucessor S-12) é o substituto parcial, em curso |
| 7 | §8.7 passo 8 — propagação (`update-touring` / release) | BLOQUEADO | 30.4.100 no binário global (`704fbf72`), mas toolchain default e projeto `analise` em 30.4.99 e `latina` em 30.4.97 (`PLANO.md` §11, verificação 29/10 ~13:20); o 3º rito morreu na suspensão da máquina | congelamento do A3 até 06/11 04:00; retomar `scripts/propagate-release.sh` |
| 8 | Ativar o interruptor no live (`TOURING_HOOK_INJECTION` no `env`) | PENDENTE (decisão do Gabriel) | `~/.claude/settings.json` → `env` **sem** a chave; único escritor é `a3_arm.py:859` (`apply`), usado pelo A3; journal vivo `~/.claude/touring/injection_suppressed.jsonl` com 8 linhas, última em 28/10 17:15 (`level=no-advisory`, `site=verdict`, `family=touring-suggest`) | depois de 06/11 04:00, o Gabriel decide se `no-advisory` vira o default |
| 9 | Defeito do `similar-fail` (alerta sem alvo) | FEITO | `crates/touring-intelligence/src/reasoning/pensieve.rs:185` `check_known_failure_detail`, `:297` `command_notice`, testes `:717,:758`; consumidores `pre_bash.rs:232,239`, `pre_edit.rs:431`, `pre_write.rs:582`, `crates/touring-cli/src/cli/pretask.rs:321` (`check_known_failure_seq_detail`) | — |
| 10 | Taxonomia por conteúdo (S-3) e paridade Python x Rust | FEITO | `context_budget.rs:1565` `the_python_policy_mirrors_this_taxonomy`; `injection_policy.rs:919` `the_rust_policy_mirrors_the_python_policy`; ~10,4% do contexto ainda cai em `other` (`round4/RELATORIO-RODADA-4.md:127`) | fechar a lacuna residual de taxonomia |
| 11 | Alcance do dedup (S-4p) | FEITO | dedup-stdin nos hooks Python: `client/hooks/code_mode_sdk_section.py:212`, `delegation_nudge.py:369`, `session_startup_intelligence.py:80` (ponte Rust `injection_dedup.dedup_output`); rota standalone + `systemMessage` no Rust (`crates/touring-hooks-shared/src/injection_dedup.rs`) | — |
| 12 | Ordem MUST executável (S-7) | FEITO | `crates/touring-hooks-shared/src/turn_budget.rs:120` `directive_program_closes` (`:127` `has_heredoc_operator`, `:146` `quotes_balance`, `:173` `directives_only` omite a diretiva cujo programa não fecha), teste `:279` | — |
| 13 | Interruptor Python (espelho) + guarda | FEITO | `client/hooks/injection_policy.py` (18.874 B) idêntico a `~/.claude/hooks/injection_policy.py`; `scripts/test_injection_policy.py` → **89 PASS** (rodado 29/10); `scripts/guard_injection_policy.py` → **154/154, 0 lacunas, 0 nomes obsoletos** (rodado 29/10) | — |
| 14 | Gate que nega gravar hook que injeta sem consultar a política | FEITO (sucessor) | `client/hooks/injection_policy_gate.py` (8.133 B) idêntico a `~/.claude/hooks/injection_policy_gate.py`; registrado em `settings.json` PreToolUse `Write\|Edit\|MultiEdit` (`RETOMADA-AQUI.md:316-323`); `client/hooks/tests/test_injection_policy_gate.py` (11) | — |
| 15 | Guard shell fail-closed (`check_context.sh`, `doc-coverage-gate.sh`) | FEITO (sucessor S-8/C3) | `client/hooks/check_context.sh:3` e `client/hooks/doc-coverage-gate.sh:53` ("só rc 1 silencia o hook") | — |
| 16 | Comando cru no log (`post_bash`) | FEITO (sucessor S-6/C5) | `crates/touring-hook-handlers/src/hooks/post_bash.rs:706` `fn log_safe_command` | — |
| 17 | Regra operacional do "So what?" (§12.5 rec. 1) | PENDENTE | o invariante está **declarado** (`crates/touring-hook-handlers/src/hooks/pre_read.rs:3-4`) e tem **um** teste (`pre_read_tests.rs:1221` `audit_purpose_silence_is_default_invariant`), mas ~34% do contexto injetado viola os três testes que ele declara (`ANALISE-CONSOLIDADA.md` §11.4) e não há enforcement para os demais injetores | transformar o invariante em predicado único sobre o payload de todo injetor, ou reduzir a emissão até caber |
| 18 | "Para influenciar, o hook precisa reescrever ou negar" (§12.5 rec. 2) | SUPERADO | resolvido no plano sucessor: B2 (`fc4c8252`; `crates/touring-cli/src/cli_suggester/gates97_tests.rs:967` `test_every_gate_reason_names_the_next_command`) e B1 (`09871099`, compactador de resultado, desligado por default) | — |
| 19 | Contador nativo de adesão do canal do **fato** (§4.3a) | PENDENTE | a régua conta *proposições* (`context_budget.rs:124-127,758,844-845` `must_emitted`/`must_followed`), não *fatos*; 0 hits de `fact_kb` em `crates/` | implementar `fact_kb` no `kpi` |
| 20 | Medida de **desfecho** (§4.3b) | PENDENTE | nada mede desfecho: `s8_corpus_ab.rs` mede bytes; o A3 (sucessor S-12) mede desfecho por quase-experimento, em curso | — |
| 21 | Defeito (a) da régua — veredito FAIL sobre amostra pequena (n=3) | PENDENTE | `context_budget.rs:758` protege só `must_emitted == 0`; não há piso de amostra para `injection_follow_ratio` (o piso `ARM_MIN_SAMPLE` existe em `cli/kpi/code_mode.rs:31`, para outro KPI) | piso mínimo de emissão; abaixo dele, "não medido" em vez de FAIL |
| 22 | Defeito (b) da régua — ratio mal-formado (`followed`×`emitted`, `denied` fora do denominador) | PENDENTE | `context_budget.rs:842-845` mantém `followed/emitted`; `denied` segue separado (`GateEvent`, `crates/touring-foundation/src/gate_metrics.rs:1500-1509`) | separar `denied` no denominador (o desfecho de um gate que nega é `Denied`, não `Followed`) |
| 23 | Defeito (c) + "Wave 2 da régua" — `fact_kb` e `nudge_follow_ratio` | PENDENTE | 0 hits de `fact_kb`/`nudge_follow_ratio` em `crates/` | implementar os dois |
| 24 | 4 engineers mortos por HTTP 429 (S-2, S-3, S-4p, S-7) | SUPERADO | os quatro foram recuperados e estão fechados — ver itens 9, 10, 11, 12 | — |
| 25 | 2 P0 de workspace (F2_1, F2_5) | SUPERADO | F2_1 = falso positivo corrigido (`crates/touring-analysis/src/quality/security.rs:188-214`); F2_5 → 0 CVEs após C1/C1b/C1c + `deny.toml` (bundle do sucessor) | re-verificar por `touring e2e -j` (não rodado aqui) |
| 26 | 4 órfãos (C4) | SUPERADO | resolvido no bundle do sucessor (1 removido, 3 falsos do índice) | — |
| 27 | Clippy FAIL pré-existente em `touring-quality/f3_1_py_imports.rs:130,131,198,261,387` | SUPERADO | verificado em 29/10 pela sessão coordenadora: `cargo clippy -p touring-quality --all-targets` com `-D warnings` **limpo** | — |
| 28 | "O ramo `systemMessage` da política nativa é código morto" (§12.4, rodada 4) | SUPERADO | afirmação do estado pré-S-4: hoje a política trata `systemMessage` (`injection_policy.rs:184` `CONTEXT_FIELDS`, testes `:806,:1206`) e `hook_response.rs:506` o declara | — |
| 29 | Propagação do nível de injeção ao daemon, por requisição | FEITO | `6a89128d`: `crates/touring-dispatch/src/daemon.rs`, `crates/touring-hook-runtime/src/daemon_protocol.rs`, `crates/touring-hooks-core/src/ipc.rs`, `verdict.rs`, `main.rs` | — |
| 30 | Journal Python no mesmo arquivo do Rust | FEITO | `96113cd4`; `client/hooks/injection_policy.py:285-309` (`JOURNAL_FILE`, `journal_cut`) | — |
| 31 | Mod de evicção (B3m) | FEITO no código, **não instalado** | `62082b20`: `tools/mods/touring-compact-evict/` (8 arquivos, 911 linhas); `~/.claude/plugins` e `~/.claude/mods` **sem** o mod | instalar depois de 06/11 |
| 32 | D2 — régua semanal (sucessor S-15) | PENDENTE | depende do A3 e do deploy (`PLANO.md` S-15) | calibrar após o primeiro ciclo completo |
| 33 | Regressão dos 27 testes vermelhos em `client/hooks/tests/` | FEITO | guard fail-closed + fixture autouse em `client/hooks/tests/conftest.py` (`RETOMADA-AQUI.md:302-306`) | (suíte não reexecutada nesta auditoria) |
| 34 | Espelho versionado da política em `client/hooks/` | FEITO | `client/hooks/{injection_policy.py,injection_policy_gate.py,injection_dedup.py}` versionados e byte-idênticos ao live; `scripts/sync-client-skills.py --check` → **CLEAN (395)**, verificado em 29/10 pela sessão coordenadora | — |

---

## 2. Resumo (3 pendentes, contradições, não verificado)

**Os 3 pendentes que mais importam**
1. **Item 6/20 — A/B de desfecho com modelo no circuito.** É o único caminho para decidir se
   `no-advisory` pode ir além; hoje só medimos adesão (proxy) e bytes.
2. **Itens 21-23 — a régua (`context_budget`).** O `injection_follow_ratio` mede o oposto do que diz
   (`denied` fora do denominador), reprova sobre n=3 e não tem contador de fato: é o instrumento
   que decidirá o default.
3. **Item 17 — a regra do "So what?".** O invariante que o próprio harness declara é violado por
   ~34% do contexto injetado; é a causa mecânica do veredito, e nada o torna executável.

**Contradições documento x código**
- §8.7 passo 0 exige **um** tipo `HookResponse`; hoje há **dois** (`hook_runtime.rs:61` e
  `touring-hooks-core`) — a lição "declaração lida como referência" do repo se repete aqui.
- §8.5 pede os contadores em `gate_metrics.rs`; o código os põe no `kpi` (`context_budget`), e o
  **nível corrente não é exposto em lugar nenhum**.
- `RETOMADA-AQUI.md:87` ("A4 não entregou") e `:110` ("Wave 2 não implementada") descrevem o estado
  de 26/10, superado pelo código atual; a guarda está em 154/154 e o documento fala em 152/152.
- §12.4/`RELATORIO-RODADA-4` chamam o ramo `systemMessage` de "código morto": hoje é vivo.

**Não verificado**
- A suíte `client/hooks/tests/` e os testes Rust da wave de injeção (`cargo test`) — não rodados
  (peso; exigiriam `hw-slot`).
- `touring e2e -j`, `touring doctor -j` e o estado de qualidade/órfãos do workspace.
- O braço do A3 de **hoje**: a chave está ausente do `settings.json`, e a máquina dormiu de
  28/10 18:42 a 29/10 13:08, então não há troca registrada hoje — o dia 0 do calendário
  (`a3-calendario.json`: `day0=2026-10-29`, `switch_hour_local=4`, braços `A=full`/`B=no-advisory`)
  não tem amostra.

**Corrigido depois deste relatório** (verificado por mim, 29/10): o bypass do F2.4 por
desestruturação — achado meu, relatado à sessão coordenadora — foi corrigido em `4a93eac0`
(`binds_secret` + `is_destructuring`); minhas 6 fixtures do bypass agora dão **Fail 0,000** e o
falso-positivo que `20d2b05a` removeu continua **Pass 1,000**. Detalhe:
`docs/audits/f2-4-secrets-destructuring-bypass-2026-10-29.md`. Os "não verificado" do clippy
(item 27) e do espelho (item 34) também foram fechados pela sessão coordenadora.
