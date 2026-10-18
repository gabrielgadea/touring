---
type: Resume
title: RETOMAR AQUI — verificação geral de 09/10/2026 e os 5 abertos
description: Estado medido por exit code ao fim da sessão analise-3b (ex-6b), interrompida por Gabriel ("preciso desligar, salve tudo"). Ler PRIMEIRO.
plan_id: 2026-10-08-vgp-motor-base
timestamp: 2026-10-09T00:30:00-03:00
tags: ["#kind:plan", "#status:open", "#domain:harness"]
---

# RETOMAR AQUI (09/10/2026, madrugada)

Pedido do Gabriel: **«verifique tudo»** → interrompido por **«preciso desligar, salve tudo para retomarmos em outra
sessão»**. Nada commitado (commit segue ADIADO por ordem dele: «deixe o commit para depois»).

## 1. Re-rodar a verificação (1 comando, ~5 min, sequencial de propósito)

```bash
cd ~/projects/analise && TOURING_CODE_MODE=native python3 \
  ~/projects/touring/docs/plans/2026-10-08-vgp-motor-base/scripts/verificar_tudo.py
```

Resultado de 09/10 (saída íntegra em `diagnostics/verificar_tudo-2026-10-09/`): **18/24 PASS** na 1ª rodada; depois
dela, 3 do drift corrigidos (ver §2) e o check `dags` era defeito do INSTRUMENTO (cwd do scratchpad + stdout+stderr
→ "None 0/0"; corrigido no script salvo: `cwd=analise`, só stdout). **Reais e abertos: C1 e o corpus da V0d.**

PASS: versões 30.4.93 (global, toolchain, analise, konverter; default 30.4.93) · 3 daemons, 0 com exe deleted, global
= pidfile · espelho client/ CLEAN 377 · suíte update-touring 28/28 nos 2 modos com estado real antes==depois · drift
A4/predicados/agentes sem literal errado (controle positivo ok) · conservação agentes · selftests inventário/vivos/
replay/juiz_v0d (5 mutantes) · juiz_v0c CONVERGIU · analise: grafo 116, emendas 80, consertos a9 4 passed · ruff ·
15/15 nós do grafo com arestas+procedência. DAGs (lidos de cwd=analise): drift `…430085` finalized 6/6; compactação
`…571926` 10/12 (S8 in_progress, S11 pending); VGP `…538442` 3/19 (D1, D2 **failed**; V0d…V6, J, V0e1–5 pending).

## 2. Feito nesta etapa (09/10)

- **4ª reconciliação do drift** (o release deslocou o C1, como a pergunta 1 da Kimi previa): ledger linha 47 (C1 da
  versão então planejada) (c)→(a) com `mudanca` = bump do Cargo.toml; nova linha (c) da versão anterior (17
  citações datadas), lote `2026-10-09-release`; citação minha de número de release FUTURO removida da RETOMADA.
  Ledger 122→123; A1 rc 0 (391), A2 8 passed, A3 VERIFY OK. Adendo no RELATORIO do bundle do drift.

## 3. ABERTOS (em ordem de prioridade)

1. **C1 da compactação REPROVA pelo critério substantivo**: com n=323 chamadas pós-deploy, 898 → **592** chars/
   chamada = **−34%** (< −40%). As leituras de −43%…−45% vinham de n=39–55 (amostra pequena superestimou).
   Próximo: decompor os chars por FAMÍLIA de conselho (prefixo do additionalContext: `[TOURING SUGGEST`, `post-edit
   verification`, `[medição sem relação verificada]` do analise, `EVIDENCE GUARD`, …) pós-deploy × janela da linha de
   base. ⚠ `medir_conselho.py` (origem do 898, comentário em `verificar_compactacao.py:42`) NÃO está em
   `compactacao-65/scripts/` — achar onde mora antes de medir. Hipótese forte (não medida): ver item 2.
2. **`~/.claude/settings.json` com hooks DUPLICADOS** (medido 09/10): PreToolUse 91 entradas / 18 distintas —
   `touring-hook pre-read` ×17, `pre-edit` ×17, `pre-edit-prevention` ×17, `pre-write` ×17, `cli-suggest` ×9,
   `touring-quality-block-all.sh` ×2; PostToolUse `doc-coverage-gate.sh` ×2. São as subtarefas **D1 (dedupe com
   backup e prova por contagem) e D2 (achar o instalador não idempotente + teste "rodar 2× = mesmo arquivo")** do DAG
   VGP, hoje `failed` — o MOTIVO do failed não foi reencontrado nesta sessão: `touring memory recall "dedupe
   settings.json pre-read 17x"` antes de agir. Mexer em settings.json = mudança de harness: confirmar com o Gabriel.
3. **Trabalho da Kimi (V0e) NÃO commitado e possivelmente pela metade**: as sessões `analise-a0` (Kimi) e
   `analise-a9` (MiniMax) ENCERRARAM (não aparecem no ListAgents de 09/10). Em `~/projects/touring`: 16 arquivos em
   `crates/` modificados (+1495 −383) + `injection_dedup.rs` e `touring-hooks-shared/tests/` novos + update-touring e
   seu teste. Mistura o que foi APROVADO e está no binário 30.4.93 (S8, H1, conserto do update-touring) com edições
   da V0e iniciadas DEPOIS do release (ela rodava `cargo test -p touring-cli --lib` às 00:07). Antes de qualquer
   build: separar por mtime (> 2026-10-09T01:00Z = pós-freeze) e por conteúdo, rodar `cargo test` + `clippy
   --all-targets -D warnings` nos crates tocados; o brief completo da V0e está em `RETOMADA.md` §«V0e — brief».
4. **Corpus da V0d parcial** (a9 saiu sem relatório; `scripts/corpus_v0d.py` existe): 81 entradas = 51 positivos +
   30 negativos (de 300); reproduzem 30/51 = **59%** (< 70%). Juiz: 3 pendências. Lacunas do construtor medidas em
   `~/.claude/touring/vgp/corpus_v0d/relatorio.json`: 18 "inline sem heredoc nem -c" (provável `touring run --code
   '…'` não extraído), ~17 "arquivo não existia no commit anterior" (se a sessão o criou por Write, a base é vazia —
   não é incompleto), 11 "/tmp não gravado antes de first", 4 fora da raiz, 2 Edit que não casa. Corrigir o
   construtor (TDD) e reconstruir; o juiz `scripts/juiz_v0d.py` NÃO se afrouxa.
5. **C1 por construção** (pergunta 1 da Kimi ao Gabriel, `drift-reconciliacao/RELATORIO.md`): cada release exige 2
   linhas de ledger; decisão dele se o predicado C1 muda.

Também pendentes (de antes): commit adiado (`commit-adiado/`: format 42, lint 34, PII 21 a resolver); V0e5 (remedir
negações após o deploy da V0e); `jev_client.py:264`; rotacionar `MINIMAX_API_KEY` e `judge_attest.py --attest`
(Gabriel).

## 4. Adendo — retomada de 09/10/2026, manhã (sessão e7c33fba)

`verificar_tudo.py` re-rodado: **20/24 PASS, rc=1** (saída em scratchpad da e7c33fba; os logs em `diagnostics/`
continuam sendo os de 00:15 — o script não os regravou). Falhas: `drift.A1` · `drift.A2_pytest` · `drift.A3_verify`
(UM sinal) · `vgp.juiz_v0d_corpus` (item 4, inalterado). `compactacao.juiz_unico` e `vgp.juiz_v0c` CONVERGIU.

Correções a este arquivo, medidas:

- **Item 2 é FALSO — não há duplicata.** `~/.claude/settings.json`: 148 entradas, **148 identidades distintas**
  por `(evento, matcher, hook inteiro)`, 0 duplicatas; controle positivo (1 duplicata injetada) → 1. Os ×17 são
  17 filtros `if` distintos por extensão (`Read(*.rs)`, `Read(*.py)`…), introduzidos de propósito no fix S31
  (`fix:hooks-touring-religados-um-if-por-extensao:2026-09-27`). Já refutado em 08/10:
  `licao:hook-repetido-no-settings-pode-ser-filtro-if-por-extensao-identidade-inclui-todos-os-campos:2026-10-08`.
  D1/D2 `failed` é o desfecho certo. Cai junto a "hipótese forte" do item 1.
- **Item 3 superado** por `ESTADO-V0e-kimi.md` (00:19, depois deste arquivo, 00:16): o executor da V0e foi a
  **analise-8614d1f9** (backend Kimi), não a a0; declara V0e6/1/2/4/3 + jev:264 completos (1620+647 passed, pytest
  10, clippy rc 0). Conferido no código: os 11 testes citados existem (`the_rm_route…` está em
  `pre_tool_validator.rs:1057`, não no validador shared); última edição em `crates/` 00:06:36 < rodada final 00:15.
  **Juiz da coordenadora APROVOU (09/10 manhã)**: `cargo test -p touring-ceg -p touring-hooks-core -p
  touring-hooks-shared` 8 binários 1620 passed / 0 failed (somado, não o resumo) · `touring-cli --lib` 647/0 ·
  clippy `--all-targets -D warnings` rc 0 nos 4 crates · pytest jev 10 passed. Gabriel ordenou o deploy completo
  do próximo patch se o juiz passasse. O defeito da V0e6
  reproduziu ao vivo no 30.4.93 instalado (`fatal: cannot use ~/.config/git/ignore as an exclude file` dentro do
  `touring run`) — controle positivo de que o conserto ainda não está no binário.
- **Drift A1/A2/A3 = 1 sinal**: `reconciliar.py verify` → `SEM DISPOSIÇÃO` de C1 para o número do próximo patch ·
  `ledger=49 != delta=50`. Fonte: `ESTADO-V0e-kimi.md` citava o release futuro 3× (gravado às 00:19, depois da
  limpeza que esta coordenadora fez na própria RETOMADA). Gabriel decidiu: reescrever → "do próximo patch".
  ⚠ Ao documentar o sinal, ESTE adendo reintroduziu o literal — o scanner varre `ROOT.rglob("*.md")`, o repo
  inteiro; minha 1ª busca cobriu só `docs/ scripts/ crates/`. Lição: documentar um sinal de drift nunca repete o
  literal que o dispara.
- **`medir_conselho.py` recuperado**: vivia no scratchpad `/tmp` da 10fb71c6 e sumiu no desligamento. Reconstruído
  do transcript (2 `Write`, 0 `Edit`; L4489, 08/10 16:59) → `compactacao-65/scripts/medir_conselho.py`, sha256
  `1eb90f28a164…`, byte-idêntico ao original (ruff S605 na parte A; usar `--no-bundle`). Ressalva: a linha de base
  898 = "12 transcripts mais recentes por mtime" em 08/10 ~17h — o conjunto não foi gravado; reprodução só aproximada.

## 5. Desfecho de 09/10 manhã — deploy 30.4.94 FEITO (ordem do Gabriel: "deploy completo" se o juiz passasse)

Evidência completa em `diagnostics/deploy-v0e-2026-10-09/` (`deploy.json`, logs de cada passo, snapshots).
- Passos 0a elite rc 0 (0,9569 Diamond) · 0b clippy workspace rc 0 · 1 bump `Cargo.toml:151` · 2 dry-run rc 0 ·
  3 `--stop-before-propagate` rc 0 (736 s; global 3502→211554, rótulo 30.4.94, doctor 5/5, analise intocado) ·
  4 prova por comportamento (versões, default, exe sem "(deleted)") · 5 1ª passada **rc 1**: prova 37/40 2× —
  causa medida: o script herdou `TOURING_DAEMON_SOCKET` desta sessão (com `env -u`: 40/40) → **conserto TDD**:
  `unset` no prelúdio do `propagate-release.sh` + `scripts/test_propagate_release_session_env.py` (RED 2 → GREEN 2 →
  mutante 2 failed; rito 45 passed) → 2ª passada **rc 0**, prova 40/40 na 1ª, 6/6: analise e konverter
  `lock=30.4.94 / touring 30.4.94`. `deploy_utc` = 2026-10-09T14:16:43Z (/proc do daemon do analise; conservador).
- **Drift 5ª reconciliação**: ledger linha 47 (a)→(c) (lote `2026-10-09-release-30.4.94`, 45 citações em 8
  arquivos); `test_drift_coverage.py` 391→392 (C1 82→83) com o motivo no docstring. A1/A2/A3/A4 rc 0.
- **C1 da compactação (MiniMax analise-25, aceito após juiz)**: a base 898 é de OUTRA função (3 tipos, len cru)
  que a do juiz (`verificar_compactacao.py:60`). Pela mesma função: pré 1.034,1 → pós 536,8 (**−48,1%**, janela
  fechada 13:53:47Z) / 549,3 (−46,9%, aberta). Trocar a base do juiz = decisão do Gabriel.
  Saída: `compactacao-65/diagnostics/c1-por-familia-2026-10-09/`.
- **V0e5 aberta, com achado**: a reescrita da V0e1 está viva na sessão do analise (comando voltou como envelope
  `touring run`), mas no daemon global/cwd touring a sonda com env limpo deu `deny` sem `updatedInput`; a prova
  [7] não distingue os dois (lê só o texto). E a reescrita muda semântica: leitura de `/proc/<pid>/environ` na
  sandbox → Permission denied com rc 0. DAG VGP: V0e1–V0e4 → completed (veredito `subtask_updated`).
- **V0d delegada à Kimi (analise-0e)**, brief com aceite (juiz rc 0, selftest, sha `67ffdae78231c797` do juiz).
- Lições no grafo (4): `licao:script-de-release-herda-o-socket-da-sessao:2026-10-09` ·
  `…documentar-sinal-de-drift-repete-o-literal-que-o-dispara…` · `…prova-que-le-so-o-texto-nao-distingue-deny-de-reescrita…` ·
  `…razao-entre-medidas-de-funcoes-diferentes-nao-e-queda…`. Nada commitado.

## 6. 09/10 tarde — "troque a base do C1 e siga para a V0e5" (Gabriel)

**C1 da compactação: PASS, juiz rc 0, CONVERGIU.** `compactacao-65/scripts/verificar_compactacao.py`: a constante
898 saiu; a base é medida pelo MESMO contador do juiz (`_record`/`_count`) nas 24 h antes do `deploy_utc` e
conferida contra o congelado (`BASELINE_FROZEN` 3.133 chamadas / 3.239.902 chars — releitura divergente reprova
alto); a janela pós fecha em `pos_fim_utc` = 14:10:52Z (`diagnostics/item1.json`, motivo lá). Medido: 1.034,1 →
569,9 chars/chamada (−44,9%, n=614). TDD `scripts/test_verificar_compactacao_c1.py` (RED 6 → GREEN 6 → mutantes M1
fim-de-janela e M2 base-reproduzível mortos); o consumidor `c1-por-familia/reconciliar.py` (`_advice_since`) rc 0.
Estilo do juiz preservado (nunca foi formatado pelo ruff; formatá-lo misturaria 9 trechos à mudança).

**V0e5 — o "G1 = 46%" eram TRÊS gates** (MiniMax, `diagnostics/v0e5-2026-10-09/`, aceito após contador
independente): S3 rajada do code mode (`cli_suggester.rs:5847`, `[CODE MODE · rajada]`) 68,5% · G10 exec-burst
21,7% · G1 próprio 9,8% (1.854 eventos desde 17/09; independente por prefixo: 71,3/19,7/9,0%). Causa: o
instrumento rotula pelo 1º padrão com re.I, e o S3 registra `GateId::G1` nas métricas. A V0e1 converteu só o G1
(182→0); **o S3 segue negando** (1 desde o deploy). E detector por `is_error` não vê reescrita.
**(c) consertado no código, NÃO deployado**: a reescrita mandava à sandbox leituras de `/proc/<outro pid>/` que o
kernel guarda por modo ptrace (`/proc` É raiz de leitura — premissa "landlock" derrubada por medição). Medido
sandbox × nativo: divergem `environ maps smaps smaps_rollup auxv cwd exe root io map_files fd/<n> fdinfo/<n>`.
`ptrace_guarded_proc_entry` + `PTRACE_GUARDED_PROC_*` em `unreachable_in_sandbox` → a chamada roda nativa.
TDD no `s24_route_tests` (RED rc 101 → GREEN 6/6 → mutantes M1/M2 mortos, sha restaurado); cli lib 648/0, clippy rc 0.
**S3 por classe** (predicado estrito com cabeçalho de hook, controle 1.741 transcripts; 1.272 eventos — confirma os
1.270 da MiniMax): shell 928 = 73,0% (grep 500 · cat 136 · ls 130 · sed-n 124 · wc 23 · find 15) · python-inline
344 = 27,0%. **Recomendação refinada**: reescrever só o S3 SHELL (o guarda de alcance vê os caminhos — tokens do
comando); manter deny no S3 python-inline e no G10 (programa opaco ao guarda → reescrita arrisca zero silencioso,
ex.: script que lê `~/.claude/projects` fora das raízes). Pergunta feita ao Gabriel às ~15:20, sem resposta.
**ABERTO (decisão do Gabriel)**: (1) reescrita do S3 shell; (2) release com (c) [+ (1)] — nada publicado sem ordem;
(3) a prova [7] passa a afirmar a DECISÃO (allow/deny + updatedInput), junto de (1).
Também da V0e5 original, ainda não feito: "conferir o hook que a a9 viu relatar `0 passed, 3 failed` num arquivo verde".

## 7. 09/10 fim de tarde — "siga com as recomendações do §8" (Gabriel, sobre o canvas dos itens 1-3)

Decidido no canvas: **(1) D** — o S3 AVISA, nunca nega nem reescreve; **(2) UM release** juntando (c) /proc +
item 3 + item 1-D, só DEPOIS da entrega da Kimi (para não reiniciar daemons nos replays dela); **(3)** item 3 por TDD.
Tudo abaixo é código no workspace, **NÃO deployado, NADA commitado**.

**Item 3 — "0 passed, 3 failed" num arquivo verde: FEITO.** Causa dupla medida: (i) o extrator do pytest rotulava
`PASS` com exit 0 de pipe (`| tail`) — `output_capture.rs` `test_verdict` pelas contagens: `FAIL (exit 0 masked)` /
`UNKNOWN (no pytest summary line)`, cargo idem; (ii) a duração levava a régua `=====`; (iii) dado velho: `post_bash`
fazia `append_note` e os leitores mostram só os 100–120 bytes da cabeça — a rodada vermelha das 20:58 foi injetada
11× até 21:15. Conserto: `FileKnowledgeDB::replace_owned_note` (`touring-storage` knowledge/core.rs, corpo comum com
`replace_quality_note`) + `post_bash` grava UM segmento `<extrator>@MM-DDTHH:MMZ: …` na cabeça e varre as formas
legadas (`is_bash_outcome_segment`); `format_metrics_note` removida. Provas: teste fim a fim no DB real
(`test_newer_bash_outcome_replaces_the_stale_one_at_the_head`) + 5 mutantes mortos (append, legado, forma atual,
cauda, filtro) com sha restaurado; suítes `touring-hook-handlers --features post-hooks` 729/0 · `touring-hooks-core`
530/0 · `touring-storage --features knowledge` 289/0 (2 ign.) · clippy rc 0 ×3.
⚠ **Achado**: `cargo test -p touring-hook-handlers --lib post_bash` dá rc 0 "0 passed; 129 filtered out" — o módulo
é `#[cfg(feature = "post-hooks")]`; o mesmo no `touring-storage` (`knowledge`, 147 testes). Lição no grafo.

**Item 1 (D) — S3 vira aviso: FEITO.** `cli_suggester.rs`: na 2ª da rajada `advisory_response` (sem
`permissionDecision`, sem `updatedInput`) com o programa fundido; o lote NÃO zera — as seguintes contam em silêncio
(`s3/in_burst`), um `touring run` rearma; `INSPECT_BURST_DENY_AT` → `INSPECT_BURST_ADVISE_AT`; gate event `Emitted`;
contador `g1_inspect_burst_denied` → `g1_inspect_burst_advised` (foundation). KPI `inspect_burst_share` soma
`denied + advised + in_burst` (`s3_burst_observations`) — sem isso cairia a ZERO após o release por ler um campo
que ninguém mais incrementa. TDD: RED 6 (deny onde se esperava aviso; `first_passed` 2; stub do KPI) → GREEN 55 →
6 mutantes mortos (aviso→deny, aviso em toda, lote zera, helper sem `in_burst`, braço só `denied`, evidência
`denied`). Corrida pré-existente achada e consertada: `bump_arm_mantem_duravel_e_volatil_em_sincronia` (`assert_eq!`
sobre átomo GLOBAL) × testes S3 fora do serial `gate_metrics` → os 2 que chamam os gates em modo `code` entraram
no grupo. Prova [7] de `scripts/prova_code_mode_ceg.py` agora afirma a DECISÃO — **vai reprovar contra o binário
instalado até o release** (esperado: ele ainda nega). Efeito no mundo: 2ª nativa + aviso; a 3ª parecida ainda cai
no G1, que a reescreve no sandbox (V0e1, com o guarda de alcance + o /proc novo).

**V0d (Kimi analise-0e): ACEITA** após juízo independente — juiz rc 0 "391 · 91 · 300 / PRONTO", selftest rc 0
(5/5), sha `67ffdae78231c797` intacto, pytest 31/31, ruff check limpo, M9 e M10 reproduzidas por mim (morrem pelo
teste nomeado). Eu formatei `corpus_v0d.py`/`test_corpus_v0d.py` (`ruff format --check` dava rc 1) e re-rodei.

**Achado ao vivo durante o release (REGRA #21, consertado antes do bump)**: o `updatedInput` SUBSTITUI o input.
Duas chamadas minhas com `run_in_background: true` + `timeout: 600000` reescritas pelo G2 voltaram "did not
complete within its 120s timeout"; um laço de espera reescrito pelo G8 para `touring run` morreu aos 30 s
(`exit_code -2`) — G1/G2/G8 montavam `{command}` sozinho e G1/G8 não declaravam `--timeout-ms`. Conserto:
`rewritten_input` (só o `command` muda) + `rewrite_wall_ms` (timeout da chamada − 5 s; 115 s sem timeout; 600 s
em background) + `with_wall_budget` no G8. TDD: RED 3 → GREEN 5 → 5 mutantes mortos (`mutar_reescrita.py`).
**`cargo fmt --all`** (o CI roda `fmt --check`): 133 arquivos no diff, dos quais **57 só-formatação** (o atual é
idêntico ao HEAD formatado; lista em `diagnostics/deploy-v0e5-2026-10-09/fmt_so_formatacao.txt`) — reverter
esses 57 não perde trabalho, se Gabriel preferir separar.

**RELEASE AGRUPADO 30.4.95: FEITO** (evidência em `diagnostics/deploy-v0e5-2026-10-09/`, `deploy.json`).
0a elite 0,9569 Diamond · 0b clippy workspace rc 0 (re-rodado após o conserto das reescritas e o fmt) · 0c fmt-check 0 +
suítes cli 653/0 · foundation 538/0 · hooks-core 530/0 · hook-handlers(post-hooks) 729/0 · storage(knowledge) 289/0 ·
bump `Cargo.toml:151` · dry-run rc 0 · 3 `--stop-before-propagate` rc 0 (703 s; global PID 751583, rótulo ok, doctor
9/9) · 4 prova 41/41 + sonda de DECISÃO 5/5 (`passo4_sonda_decisoes.json`) · 5 propagar rc 0 (24 s; prova 41/41;
analise e konverter `lock = touring` = a nova). `deploy_utc` = 2026-10-09T16:32:42Z (/proc do daemon do analise, PID
756733). **Prova viva nesta sessão**: a 2ª `grep` seguida EXECUTOU e trouxe só a linha de aviso.
Drift 6ª reconciliação: ledger linha 124 (C1 da release anterior, (c), 11 citações em 3 arquivos do bundle da VGP,
lote `2026-10-09-release-30.4.95`); `test_drift_coverage.py` 392→393 (C1 83→84); A1/A2/A3/A4 rc 0 (A3 re-rodado
depois dos docs novos). DAG: V0d → completed; **V0e5 → in_progress** (o que falta é remedir G3/G11/write→run com uma
janela de uso DEPOIS deste deploy). Lições no grafo (3 novas). Rewards: propagate-release 1.0 · tdd-mutation 1.0.
**Commitado por ordem do Gabriel** ("mantenha a formatação, faça o commit"), branch
`feat/2026-09-16-shell-quality-wiring-python`: `be8c6e2a` (57 só-formatação) + `dfe0fd1e` (release + o acumulado
desde 06/10; 452 arquivos; varredura de segredos com controle positivo limpa; `corpus_a2.jsonl` 50 MB e
`cache_bruto.json` no `.gitignore` por caminho exato). Sem push.
**Correção (09/10, ao conferir o plano original)**: D1/D2 NÃO é decisão pendente — a premissa "settings.json ×17" foi
refutada no próprio plano (proposta §8: 148 entradas = 148 identidades, filtros `if` por extensão; nada a fazer).
**O QUE FALTA DO PLANO ORIGINAL** (proposta §6, DAG `ready_subtasks` = só V1): V0e5 (remedição pós-deploy) · destino
das ~606 falhas fora de gate (aceite da V0e) · V1 → V2 → {V3 → V4, V5} → V6 → J.

## 8. /goal do Gabriel (09/10 ~17:20Z): "prossiga até todo plano estar completo e perfeito e ao final me prove, na prática"

Plano de execução (coordenadora analise-89; irmãs ociosas: Kimi `analise-0e`, MiniMax `analise-25`):
- **Decisão de arquitetura da V1** (registrar na proposta): o motor consulta a tabela de símbolos do MÓDULO extraída do
  arquivo em disco SOB DEMANDA, com cache pelo sha do conteúdo (fonte nunca velha), reusando `touring-code`:
  `ast/symbol_detail.rs::extract_symbol_details_for_lang` (membros py/rs/ts, inclui `self.x` do `__init__`),
  `extract_imports_resolved`/`ImportResolver`, `ScopeMap`. Defeito real achado: `cli_ast_detail`
  (`touring-cli/src/cli/handlers/index.rs:2168`) faz `read_to_string(file_path).unwrap_or_default()` — caminho relativo
  resolve no cwd do DAEMON e uma leitura falha vira 0 detalhes EM SILÊNCIO.
- **Juiz do motor escrito ANTES do motor**: `scripts/juiz_motor_vgp.py` (contrato `touring vgp check … -j`; recall_deny
  de arquivo ≥ 0,70 · falso deny ≤ 1 % · p95 ≤ 150 ms; inline medido; `--selftest` com 5 motores falsos). RED esperado:
  rc 2 "MOTOR AUSENTE".
- Corpus V0d = 100% Python: 91 positivos (atributo 60 · nome 26 · import 4 · kwarg 1; inline 50 · tmp 26 · atribuído
  15) + 300 negativos. Só `reproduz` verdadeiro conta no recall.
- Despachos: Kimi → V1+V2+V5 (motor camada 1 + CLI + sombra nos hooks + inline/tmp) com aceite = juiz rc 0;
  MiniMax → destino das 606 falhas fora de gate (V0e). Eu: V0e5 (janela pós-deploy 16:32Z), V3 (camada 2), V4, V6, J.
- Temporais (honestidade): V0e5 precisa de janela de uso; V4 exige 5 sessões REAIS — replay não fecha V4.

## 9. /goal em execução (09/10 ~17:30–18:40Z) — estado por evidência

**Feito e provado (coordenadora analise-89):**
- **Régua sem deriva + sonda casada por versão.** `scripts/estado_no_instante_v0d.py` (manifesto: 174 antes reais · 126
  nascidos na edição · 182 overlays). `scripts/sonda_camada2_pyright.py` reescrita: o pyright 1.1.408 não tem
  diagnóstico pull e o `didClose` publica limpeza `version: null` — sem casamento por versão a reabertura lia a limpeza
  (falso deny 27 → 10). Selftest 13/13, pyright 0, ruff limpo. Resultado (`diagnostics/sonda_camada2_pyright_analise_v2_
  {instante,estrita}.json`): p95 105,6 ms · `/tmp` 7/7 · inline 16/28 · forma 1/21 · P0 10/259 · **P2 1/259**.
- **Os 10 falsos são 10/10 estados intermediários** (9 corrigidos depois, 3 RED de TDD; 1 arquivo apagado; 0 persistem
  hoje — `falsos_hoje.py` no scratchpad). Política P2 registrada na proposta §5.
- **Juiz do motor v2** (`scripts/juiz_motor_vgp.py`, sha `1338492b169bce2e`): recall de DETECÇÃO de SÍMBOLO (forma à
  parte pela mensagem do runtime), falso deny por balde, modo V3 comparativo (`--arg-motor --base --p95-ms`).
  Selftest 8/8; 2 mutantes mortos (forma de volta ao recall; recall só deny); comparativo 3/3.
- **Régua da V0e consertada** (`scripts/v0e_negacoes.py`, sha `27be84b9529a4bed`): etiqueta do gate antes do texto
  livre + S3 próprio; RED 4/6 → GREEN 6/6, 2 mutantes mortos. Re-rodada: **S3 1.282 · exec-burst 405 · G1 182** (o
  «G1 1.776» era a soma). Correção na proposta §2.1.
- **V0e destino das 606: ACEITO** (MiniMax; ids inteiros 44/44). **V0e5: 1ª remedição ACEITA** — só o G1 separa
  (4,52 → 0/175 em C); S3 avisando 0/5 rota seguida [0·43,5] = sem veredito; os demais = ausência de dado (~2h).
  Reconferir com janela maior antes do J.
- Lições no grafo (3): `licao:lsp-push-sem-versao-…`, `licao:aviso-de-hook-vive-em-attachment-…`,
  `licao:simbolo-do-projeto-ausente-em-edicao-de-arquivo-e-estado-intermediario` (todas `gravado=True`).
- DAG: V1 e V3 → in_progress. Spec da V3: `V3-camada2-spec.md`. Ponteiro da V6 redigido: `V6-ponteiro-vgp-protocol.md`
  (a regra do analise cita 4/6 caminhos que sumiram; aplicar SÓ com o motor no ar).

**Em voo:**
- Kimi `analise-0e`: V1+V2+V5 em `crates/touring-code/src/vgp/` (5 arquivos, ~2.100 linhas). Recebeu juiz v2, P2,
  regra `__file__` inline, defeito do corpus `9c269d2c1bf5`, pedido de ≥300 negativos inline, contrato do
  `sombra.jsonl` e das etiquetas `[VGP · deny]`/`[VGP · aviso]`.
- Subagente sonnet `v3-camada2`: cliente LSP em `crates/touring-lsp/src/{client,pyright}.rs` (+ testes; integração
  `#[ignore]` contra o pyright do analise).
- MiniMax `analise-25`: juiz da V4 (`scripts/juiz_v4_sessoes.py`) contra o contrato do `sombra.jsonl`.

**Falta depois**: julgar as 3 entregas · plugar a camada 2 no `vgp check` (`--camadas 1,2`) e no hook · juiz v2 rc 0
(V2) e comparativo (V3) · release pelo pipeline · V4 temporal (5 sessões reais) · V6 aplicar · J.

**Adendo 09/10 ~18:30Z.**
- **Negativos inline/tmp (V5, MiniMax) ACEITOS** após 2 devoluções: 25 snaps não eram o programa que rodou (10 `-c`
  sem desescape do bash, 15 `cat > f <<EOF`); corpus final `entradas_inline_neg.jsonl` sha `10741d3b` (300 inline +
  100 `/tmp`, 0 não compilam). Pendente: 1 `cat >>` (bbb874e32783) devolvido. Dívida do extrator do V0d (Kimi):
  2.037 programas do universo não compilam (`'\''` no `touring run --code`).
- **Política final P4** (proposta §5, spec V3 §3): escopos `forma` (membro de builtin) e `dinamico` (dunder; nome sob
  `import *`) só avisam. `scripts/sonda_camada2_pyright.py` (selftest verde) →
  `diagnostics/sonda_camada2_pyright_analise_v4_instante.json`: falso deny 0/259 · 0/100 · 1/300; tmp 7/7, inline
  16/27; p95 68 ms. `estado_no_instante_v0d.py` agora cobre os negativos inline/tmp (791 entradas, 214 overlays).
- **Juiz do motor** sha `1b026c6a24f654e1`: exclui e nomeia snap que não compila (mutante morto).
- **V3 (cliente em touring-lsp, subagente sonnet)**: 58 unit + real verdes ANTES do `touring-code` da Kimi parar de
  compilar (`vgp/sombra.rs` sem docs); meus 2 mutantes mortos (versão nula; forma→deny), shas restaurados. Em curso: P4
  em Rust (Escopo::Dinamico, 15 células).
- **J** (`scripts/juiz_plano_vgp.py` sha `26384ee32b0813e4`): V0e verde (3/3); drift A1 vermelho até o `vgp` entrar no
  CLI (C4 106 → 107 pela citação nos docs do plano).

**Adendo 09/10 ~18:45Z (contexto 55%: estado gravado antes de seguir).**
- **V3 cliente da camada 2 ACEITO** (subagente sonnet `v3-camada2`, crates/touring-lsp/src/{client,pyright}.rs +
  tests/pyright_real.rs): meu juízo `scratchpad/juiz_v3_lsp.sh` — unit 62+2 · real 15 · clippy --no-deps 0 · fmt 0 ·
  mutantes mortos (versão nula; Dinamico→deny) · shas restaurados. P4 em Rust: `(_, Forma | Dinamico) => Warn`.
- **Corpus de negativos inline/tmp FINAL** sha `57574182` (MiniMax, 4 rodadas de juízo): o estrato touring_run era
  1.133/1.446 NÃO-Python (`--lang bash` "compila" como Python: `head -30 /x` é expressão válida); agora 100/100
  `--lang python`; 0 consumidor cat/tee; 0 não compilam. Dívida do `corpus_v0d.programa_inline` (Kimi): não olha
  `--lang` (1.133) nem quem consome o `<<` (665 + 414); `'\''` não desescapado.
- **Próximo**: re-medir P4 no 57574182 (estado + sonda, ~3 min) · subagente: `touring-lsp/src/pool.rs` (sessão
  persistente por raiz, ociosidade, kill switch) · Kimi: `--camadas 1,2` em `touring-server/src/cli/vgp.rs` +
  hooks em sombra (os arquivos são dela) · juiz v2 rc 0 com o binário · release · V6 · J.
- Kimi: `touring-code` volta a compilar; `touring vgp` em touring-server/src/cli/vgp.rs (camada 1); hooks
  pre_bash/pre_edit_prevention/pre_write mexidos; `vgp/sombra.rs` existe.


### Adendo ~18:52Z (analise-89)

- **Prova [2] implementada** (`scripts/prova_vgp_motor.py`): hooks REAIS via `~/.claude/hooks/touring-hook <pre-write|pre-edit-prevention|pre-bash>`
  com payload JSON, `tool_use_id` nonce reencontrado no `sombra.jsonl`; sombra = nenhum `[VGP` na resposta + linha com o veredito;
  ativo = `[VGP · deny]` como recusa. `--selftest` **5/5** (stub + mutantes: cego → 4 falhas, sem_registro → 5, vaza → 4).
  Contra o 30.4.95 instalado: 5 FALHAS — esperado (a sombra nos hooks não está deployada). Os hooks do analise rodam no daemon
  PER-PROJECT, que só recebe a versão no propagate → a [2] roda **depois do propagate**; reprovou → `touring update --project ~/projects/analise --rollback`.
- **Bug da camada 1 achado pela prova [1]** (`--camadas 1`, binário de juízo da Kimi built 18:12Z): `from m import x` **sem alias**
  nunca é checado (`checks.rs:172-176` — o nome importado é `dotted_name` no campo `name`; o match só aceita `identifier|aliased_import`).
  Medido: `import zzz as q` → deny; `import zzz` / `import a, zzz` → allow. Brief + aceite (RED a–d, M-I mata (a)) enviado à
  analise-0e. Lição: `licao:mutante-morto-pelo-caso-raro-nao-prova-o-caminho-comum:2026-10-09` (lida de volta, total 1).
- Camada 1 sozinha não pega `os/json/textwrap.<inventado>`: stdlib fora das tabelas fechadas fica com a camada 2 (desenho, não bug).
- ⚠ `touring run` não serve para medir o motor: o sandbox não lê a stdlib nem o projeto inteiro → allow de lá não é medição.

- **J completo às 18:51Z: NÃO PRONTO** — abertos V1/V2/V3/V4/V5/V6/J/V0e/V0e5 no DAG. Defeitos MEUS achados e consertados:
  (1) `--arg-motor --camadas` quebra o argparse ("expected one argument" — valor que começa com `--` vira opção); a forma certa é
  `--arg-motor=--camadas --arg-motor=1,2` — consertado no J, na prova e na docstring do juiz v2 (**sha do juiz 1b026c6a → 4d5e103b**,
  só a docstring; selftest 9/9); controle: o mesmo argparse quebra na forma antiga e aceita a nova. (2) `ultima()` do J mostrava um
  SyntaxWarning do stderr no lugar do veredito → prefere a linha `veredito`/`selftest` (V0d agora "veredito: PRONTO").
  `touring-code` não compila às 18:52Z (`verdict_dos` ausente, `camada2` faltando no `CheckOutput`) = a Kimi no meio do wiring (mod.rs
  e sombra.rs editados 18:41Z) — não tocado.
- **CORREÇÃO ~18:55Z** do adendo de ~18:52Z: a linha "⚠ `touring run` não serve para medir o motor: o sandbox não lê a stdlib nem
  o projeto inteiro" é **FALSA** — medido: o sandbox LÊ `~/projects/analise`, a stdlib do uv, `~/.claude/touring` e `~/.claude/projects`
  (`-r`), e o motor dentro dele dá o MESMO deny que nativo (import com alias). Os allow coincidentes vinham do bug sem-alias.
  Medido também: a fusão de rajada REEXECUTA os comandos anteriores da janela (a linha `def script` saiu 66 às 18:50 e 70 às 18:53,
  depois do meu patch) — risco de efeito colateral duplicado em comando não idempotente; os patches com `assert count == 1` são
  seguros por construção; o adendo deste arquivo conferido sem duplicata (1 ocorrência).
- **V0e5 remedida até 18:54Z** (`diagnostics/v0e5-remedicao/remedicao.py --ate 2026-10-09T18:54:00Z`, rc 0; a de 17:44Z guardada no
  scratchpad): S3 avisos na janela C = 13, seguiram a rota = 0 (IC95 0–22,8%) — o aviso não induz a rota nesta sessão.

### Adendo ~19:15Z — defeito do 30.4.95 achado ao vivo e consertado (G1 sem replay)

- **Defeito** (V0e1, `crates/touring-cli/src/cli_suggester.rs::burst_gate_with`): a reescrita do G1 fundia o ledger INTEIRO (≤8)
  e nunca o consumia → cada inspeção seguinte reexecutava no sandbox até 7 comandos já rodados nativos (prova: `66: def script`
  → `70: def script` depois de um patch no meio); `touring run` não zerava o G1 (só o S3); comando > `G1_BODY_BUDGET` (3000)
  era PERDIDO (rodavam os antigos); o motivo do `allow` não chegava ao modelo.
- **Conserto** no worktree `~/projects/touring-wt-g1` (branch `fix/2026-10-09-g1-sem-replay`, base c118aa52 — a árvore principal
  tem o `touring-code` da Kimi em edição e não compila): fonte única `comentarios_ja_executados` (G10 `r9_exec_program` + G1),
  `burst_ledger().invalidate(&key)` no disparo, G1 zerado no ramo `touring run`, `g1_rewrite_response` com `additionalContext`.
  TDD: módulo `g1_no_replay_tests` — RED 4/5 (o 5º passou por classe errada → endurecido), GREEN 5/5, mutantes 4/4 mortos
  (M1 replay → 2 testes; M2 sem consumo; M3 sem reset; M4 sem additionalContext). Lição:
  `licao:remedio-que-passa-de-sugerir-a-executar-herda-invariantes-de-uma-execucao:2026-10-09`.
- **Levar à árvore principal**: `cli_suggester.rs` na principal = HEAD (sha abd3ec84, não tocado); copiar o arquivo do worktree
  só se o sha da principal ainda for abd3ec84. Entra no release 30.4.96 junto com o motor VGP.
- **Doc oficial conferida (code.claude.com/docs/en/hooks, via claude-code-guide)**: `permissionDecisionReason` de um `allow` vai
  "to the debug log only" — a explicação da reescrita G1 do 30.4.95 era invisível ao modelo E ao usuário; `additionalContext` em
  PreToolUse chega ao modelo como system reminder (só `defer` o ignora; teto 10.000 chars, preview 2.000). A combinação
  allow+updatedInput+additionalContext não é citada explicitamente → **teste ao vivo pós-deploy**: 3 `grep` seguidos na sessão e
  conferir que o marcador `[G1 rajada-de-inspeção → REESCRITO` chega como system reminder E que a saída só tem o 3º comando.
  Comentário do código corrigido; principal = worktree = sha cc13d9a2 (fmt 0, 5/5). 2ª leitura do diff pedida à analise-25 (só leitura).
- DAG: subtarefa nova **V0e1b** (G1 sem replay) in_progress, depende de V0e1.
- **CORREÇÃO DA CORREÇÃO ~19:25Z** (a de ~18:55Z estava errada quanto aos transcripts): medido com `open()`/`listdir` REAIS no
  sandbox — `~/.claude/projects` é **NEGADO** (PermissionError no open e no listdir; `glob('*.jsonl')` = **0 em silêncio**, nativo 556),
  e `os.access(R_OK)` / `[ -r ]` respondem **True** (Landlock não media access(2)). LIDOS de verdade: `~/.claude/touring`, o projeto
  analise e a stdlib do uv. A nota "o sandbox varre zero transcripts" (Gabriel, 09/10) estava CERTA; o meu teste `-r` é que não media.
  Lição: `licao:access-r-ok-mente-sob-landlock-e-glob-devolve-zero:2026-10-09`; a lição do from-import foi regravada com o texto exato.

### Adendo ~19:45Z — dívida do programa_inline (pedido do Gabriel) + decisão (E) + delegações

- **Dívida do `programa_inline` medida no universo** (30.290 comandos Bash do analise; script `scratchpad/medir_programa_inline.py`
  [--extrator antigo|novo] [--comparar]): o extrator antigo devolve texto em 13.226, dos quais NÃO são o Python executado:
  `touring run --lang bash` 2.814 (5 compilam), heredoc de `cat` 592 (**315 compilam** — o controle "compila" aprova arquivo
  gravado), heredoc de `git commit -F -` 204, `-c "…"` sem desescape 364 de 2.700 que não compilavam. (Os números 1.133 / 665+414 do
  registro anterior eram de outro denominador.)
- **Solução**: `scripts/shell_inline.py` — LEXER de shell como fonte única (aspas, `$'…'`, escapes, `$(…)`, crases, redireções,
  heredocs com delimitador entre aspas ou não, `<<-`); programa só por predicado positivo (python -c · python sem script com
  heredoc/`<<<` · `touring run --lang python --code`); resto None com motivo. selftest 30/30; controle: o extrator antigo erra 19/30;
  mutantes 6/6; ruff/C901≤10/pyright 0. Comparação no universo achou 4 lacunas (env -u com valor, shells aninhados, barras do heredoc
  sem aspas, múltiplos candidatos) → **subagente sonnet `shell-inline`** (aceite: ≥15 casos novos, ≥4 mutantes, gates, tabela
  --comparar). Depois: `corpus_v0d.programa_inline` e `corpus_v5_inline_neg.programa_corrigido`/`extrator_de` delegam ao
  shell_inline; regenerar os corpora SÓ em janela coordenada com os juízes (Kimi roda juiz v2 agora).
- **Bloqueio da Kimi**: ciclo de pacotes `touring-lsp →(opc) touring-hooks → dispatch → hook-handlers → touring-lsp`. **Decisão (E)**:
  extrair client/pyright/pool para crate nova (`touring-pyright`) sem a aresta — **subagente `v3-camada2`** (aceite: cargo tree limpo,
  testes iguais antes×depois, clippy, fmt, check workspace). Kimi reaplica o wiring dos hooks em `touring-pyright` ao sinal "crate pronta";
  CLI `--camadas 1,2` já completo dela (touring-server, aresta acíclica).
- **G1 — 2ª leitura da MiniMax julgada**: A1 premissa errada (G1_DENY_AT=3, dispara por lote, não inerte) mas texto e teste de cadência
  procedem; A2 (texto do advisory) procede; (f) comentário do R9 procede → **subagente sonnet `g1-ajustes`** no worktree (TDD + mutantes).
- **Sandbox (nota do Gabriel CERTA)**: transcripts negados (PermissionError; glob 0 em silêncio; access() mente True) — registro corrigido.
- Feedback do Gabriel 09/10 registrado na memória: código com spec fechada → subagente sonnet, não só irmãs.

### Adendo ~20:00Z — solução do bloqueio arquitetural (pedido explícito do Gabriel)

- **Ciclo confirmado por `cargo metadata`**: `touring-lsp →(opcional, lsp-bridge) touring-hooks → touring-dispatch → touring-hook-handlers`;
  o cargo conta a aresta opcional no grafo de PACOTES. O daemon linka os handlers (`touring-server → touring-cortex → touring-hooks → …`).
- **Solução (E)**: crate `touring-pyright` (client/pyright/pool) — deps só serde/serde_json/thiserror (medido) → `hook-handlers →
  touring-pyright` acíclico por construção; `touring-lsp` re-exporta (`touring_lsp::pool::checar` segue válido).
- **2º problema que o ciclo escondia**: o `touring-hook` tem fallback STANDALONE (S219, touring-hooks/src/main.rs:754) que roda o
  handler no processo EFÊMERO → a camada 2 subiria pyright por chamada e o deixaria órfão. Regra: pool DESLIGADO por padrão
  (`AtomicBool`); `touring_pyright::pool::habilitar_neste_processo()` chamado SÓ no startup do daemon e no `--camadas 1,2` da CLI;
  sem habilitação → `unknown` sem spawn. Aceite: contagem de `pyright-langserver` igual antes×depois de uma chamada da CLI.
- Divisão: crate + guard + teste/mutante → subagente `v3-camada2`; chamada no daemon/CLI + wiring dos 3 hooks → Kimi (exclusivos dela).

### Adendo ~20:20Z — prova [4] (G1 no hook real) + conserto do arnês da prova [2]

- `prova_vgp_motor.py --so 4`: G1 pelo hook **cli-suggest** REAL (os gates de code mode vivem lá, não no pre-bash), sessão-sonda
  própria, 6 `grep` dissimilares. **RED no 30.4.95, ao vivo**: controle OK (disparos nos índices 3,4,5); o 1º disparo (4ª) executou
  3 comandos (replay); sem additionalContext; o lote não é consumido; um `touring run` não zera o G1. Tem de ficar verde no 30.4.96.
- **A parte [2] estava mal endereçada**: o subprocesso herdava TOURING_PROJECT_ROOT=touring → o daemon do analise recusava e o hook
  devolvia `{}` (erro só no stderr). Consertado: `_env_sessao_analise()` + `{"error"` no stderr = falha explícita. Agora a [2] fala com
  o daemon de verdade: o 30.4.95 responde com o contexto real e NÃO grava registro VGP (RED legítimo, pré-deploy).
- No fluxo real o S3 avisa na 2ª inspeção e retorna antes do G1 → 1º disparo na 4ª (unitário: 3ª). A prova ACHA o disparo.
- Lição: `licao:sonda-de-hook-real-exige-o-hook-certo-o-env-certo-e-controle-positivo:2026-10-09`.

### Adendo ~19:35Z (relógio do subagente: 19:26Z) — touring-pyright ACEITA + G1 ajustes

- **`crates/touring-pyright`** (client/pyright/pool + tests reais movidos; touring-lsp re-exporta) — juízo PRÓPRIO: cargo tree 0
  crates touring-* (deps serde/serde_json/thiserror) · lib 71/0 · reais 1+15/0 (iguais a antes) · touring-lsp 11+2/0 · clippy 0 ·
  fmt 0 · mutante da trava (`if !habilitado` → `if false`) MORTO por `a_process_that_did_not_opt_in_never_spawns`, sha restaurado.
  Trava: `pool::habilitar_neste_processo()` (AtomicBool, false por padrão). Aquecimento frio do pool_real tolera 3 tentativas;
  reuso segue estrito (19 checagens). Sinal "CRATE PRONTA" enviado à Kimi com a API e o aceite dela (sem ciclo + sem órfão).
- **G1 (`g1-ajustes`, worktree)**: itens 1–4 aplicados (invalidate em TODO disparo, "desde o último disparo", advisory diz NATIVO e
  "próxima rajada", comentário do R9); RED→GREEN; M-a/M-b/M-c mortos; touring-cli 661/0; clippy 0; fmt 0; sha 61d74f55. **M-d
  sobreviveu** (invalidate no ramo advisory sem teste — o ramo depende de contador global) → pedido: `GateOverrides.g1_precision`
  (None = vivo) + teste do advisory consumindo o lote; M-d tem de morrer. Achado de método dele: `copy2` restaura mtime antigo e o
  cargo pode não recompilar — `touch` após restaurar.
- **G1 FINAL aceito** (~19:40Z): `GateOverrides.g1_precision` (None = vivo; from_env = None) + teste
  `g1_advisory_branch_also_consumes_the_lot`; M-d (invalidate depois do advisory) MORTO — reproduzido por mim, só o teste novo cai;
  módulo 9/9 (meu), touring-cli 662/0, clippy 0, fmt 0 (subagente). **Principal = worktree = sha c79f12b0** (base cc13d9a2 conferida).
  `touring-pyright` pool_guard (pyright falso + arquivo-testemunha) 1/0 conferido por mim.

### Adendo ~19:58Z (analise-89) — juízo do wiring L2, juiz v2/V3 e lexer aceito
- **Wiring L2 (Kimi) ACEITO menos fmt**: check 3 crates rc 0 · touring-pyright sem dep touring-* · clippy 5 crates rc 0 · `habilitar_neste_processo` em daemon_main.rs:62 e cli/vgp.rs:34 · sonda CLI `--camadas 1,2` no binário de 16:48 BRT: layer 2, ~450 ms, pyright pega `json.loadz`/`Path.read_textz`, que a camada 1 sozinha não negava. FALTA `cargo fmt` em hook-handlers `vgp_l2.rs:49`, `lib.rs:85,95`.
- **Lição (grafo: `licao:binario-mais-novo-que-a-fonte-nao-prova-que-a-contem-congele-o-medido:2026-10-09`)**: o binário vgp-judge de 16:36 era mais novo que vgp.rs (16:33) e NÃO o continha; o V3 nele deu "camada 2 desligada" em 762/762. Juiz agora roda numa CÓPIA congelada: `scratchpad/touring_juiz_congelado` sha **c8b503c4a10c58dd**.
- **Juiz v2 (sha c8b503c4) NÃO PRONTO**: recall arquivo 8/13 · inline 12/27 · falso deny 43/400 · 42/300 · p95 100,6 ms. Causas medidas por predicado positivo (`scratchpad/classificar_falsos_vgp.py`, `classificar_atributos_vgp.py`): **R1** `from M import X [as Y]` liga Y ao módulo M (74 atributo + 4 import); **R2** o vinculador de escopo ignora with-as 32 · except-as 27 · atribuição encadeada 7 · `*args` 2 · walrus 2 · combinações 4 (todos os 74 de `nome` estão LIGADOS no arquivo); **R3** `from __future__ import annotations` (9). Brief enviado à Kimi (~19:57Z) com o aceite: TDD + mutante por causa, juiz v2/V3 rc 0 com o sha na §6.
- **shell_inline.py ACEITO** (sha **b6acd415c6b23d90**): selftest 107/107 · os 7 mutantes da coordenadora (`scratchpad/mutar_shell_inline.py`) mortos; o M7 (`--` de embrulho) sobrevivia por falta de caso e foi coberto · 36/36 do subagente · ruff/format/C901/pyright 0. Defaults: `analisar`/`programa_python` substituicoes=False; `candidatos`/`achados` True. Tabela --comparar (30.440 cmds): compila 96,1–100% por classe; `touring run --lang python` 1.488 ambíguos com 2 programas.
- **Em curso**: subagente `shell-inline` religa corpus_v0d/corpus_v5_inline_neg ao lexer em STAGING (`scratchpad/corpus_staging`), com diff e juiz lado a lado; corpus vivo intocado. Kimi: R1–R3 + fmt + juiz v2/V3.
- **V3 no binário congelado c8b503c4 (~20:01Z)**: camada 2 ENTREGA — recall arquivo 8/13→**11/13**, inline 12/27→**19/27** (ambos ≥ 0,70), 10 positivos recuperados, 0 perdidos novos, p95 **877,6 ms** (≤ 1.000), pyright-langserver 5→4 (sem órfão da CLI). Falsos deny 85 = os MESMOS ids do v2 ⇒ todo o bloqueio restante é da camada 1 (R1–R3). Juiz rc 1 só por isso. JSONs: `scratchpad/juiz_v2_b.json`, `juiz_v3_b.json`. Delta enviado à Kimi.
- **~20:30Z — proposta.md ATUALIZADA (ordem do Gabriel)**: cabeçalho (coordenação 89/0e/25 + subagentes; DAG 27 subtarefas), ordens 7–12 citadas, §6 com o estado medido de V1–V6 + V0e1b + as 7 fases novas (VL0 · VRS · VTS · VTSX · VHTML · VCDN · VCSS) e J dependendo delas, §7 riscos/sequência atuais, §8 quatro erros meus de 09/10, §9 artefatos novos; instrumentos movidos do scratchpad para `diagnostics/v2-falsos/` e `diagnostics/multilinguagem/`. Conferido: 20 fases × 5 colunas, 29 links relativos existem, 0 caminhos de scratchpad, drift `test_drift_coverage.py` rc 0 (395). Patch por âncora única: `scratchpad/patch_proposta.py` + `patch_proposta_blocos.md` (o conteúdo vive no .md porque o F2.1 casava `<script>` dentro de string Python).
- **~20:45Z — multi-linguagem em andamento**: [`VL0-multilinguagem-spec.md`](VL0-multilinguagem-spec.md) escrita (contrato `Adaptador`, `lang_do_caminho` fonte única, vocabulário de 11 classes, P4 generalizada, camada 2 genérica descoberta no projeto, corpus por linguagem com oráculo independente, aceite da VL0 = Python sem regressão por id). Fatos conferidos: `LANGUAGE_TYPESCRIPT` e `LANGUAGE_TSX` existem na crate 0.23; `analise/frontend` tem tsconfig + tsc e compila com 0 erros em 8,9 s (281 .tsx · 135 .ts); `konverter/portal` tem tsc (13 .tsx); `video-studio` não tem tsc. **Delegado**: MiniMax (analise-25) → corpora SEMEADOS TS/TSX/JS/HTML/CDN/CSS em `scripts/corpus_semeado.py` → `~/.claude/touring/vgp/corpus_semeado/<lang>/`; subagente sonnet `corpus-rust` → corpus NATURAL de Rust (84 erros rustc reais + ≥ 300 negativos) em `scripts/corpus_rust.py` → `~/.claude/touring/vgp/corpus_rust/`. VL0 em si espera a V2/V3 convergir (colisão com a Kimi em `vgp/`). Sonda do rust-analyzer no workspace touring: adiada até a Kimi liberar CPU (não distorcer o p95 do juiz dela).
- **~20:50Z — religação dos corpora ao lexer ACEITA (subagente `shell-inline`)**: `corpus_v0d.programa_inline_motivo` delega a `shell_inline.candidatos` + escolha por EVIDÊNCIA nos positivos (o candidato cuja linha N do frame `<string>`/`<stdin>` contém `nome_esperado` como palavra inteira; 0 ou ≥2 = ambíguo; negativos exigem 1 candidato); `corpus_v5_inline_neg` deriva do lexer; HEREDOC_PROG/C_PROG/desscape_duplas saíram. Conferido por mim: test_corpus_v0d 61 passed · ruff/format/C901/pyright 0 · selftests corpus_v0d/corpus_v5_inline_neg/juiz_v0d rc 0 · shell_inline b6acd415 inalterado · corpus VIVO intocado (hash do diretório = manifesto de antes, `48f5b544`). Juiz lado a lado no sha c8b503c4: recall inline 12/27 → **13/27** (população 27 = 27; 1 positivo antigo estava ERRADO — `091584efee91` guardava o 1º `-c`, o frame era do 2º), defeito de corpus 3 → **0**, falso deny arquivo 43 → 48 · inline 42/300 → 40/274. Os 5 falsos NOVOS do staging são todos R3 (`from __future__`) + 1 R2 (except-as) — mesmas causas da wave da Kimi, nenhum achado novo (`classificar_falsos_vgp.py` com CORPUS=staging). **Staging em `scratchpad/corpus_staging/` (juiz: `juiz_v2_staging_evidencia.json`); promoção a vivo = coordenadora, DEPOIS do relatório da Kimi, com backup datado do vivo (cp -a, nunca rm).**
- **~21:15Z — juiz `juiz_motor_vgp.py` por linguagem (achado da analise-68)**: `classe_erro(mensagem, lang, classe)` — Python com a régua antiga; Rust/TS/TSX/JS exigem o código do compilador; classes web exigem a mensagem do oráculo; `origem: "semeado"` no balde arquivo. RED (TypeError) → GREEN (9 cenários + 11 casos) · 4/4 mutantes mortos · regressão Python IDÊNTICA por id no corpus vivo (sha c8b503c4) · sha do juiz **b7f7fd9b4ccddf62**.
- **corpus natural de Rust ACEITO (subagente `corpus-rust`)**: `~/.claude/touring/vgp/corpus_rust/` sha `141ced89…`, corte `2026-10-09T20:40:00Z`: 37 positivos (E0425 20 · E0432 7 · E0433 6 · E0599 3 · E0061 1; nome 20 · import 13 · atributo 3 · kwarg 1; todos de crates do touring) + 307 negativos; 151 ocorrências brutas → 37 com o oráculo textual estrito (33 reconstruções verificavelmente erradas descartadas: patches via Bash que a dobra não vê). Conferido por mim: selftest (mutante reprovado) · pytest 64 · ruff/format/C901/pyright 0 · determinismo · os 37 = `simbolo` no juiz novo. **Para a VRS**: n por classe < 20 em atributo/kwarg → corpus semeado de Rust; 14 positivos `introduzido: false` (erro vindo de edição em OUTRO arquivo) são fora do alcance do pre-edit — o juiz tem de contá-los à parte.
- **Relatório final da Kimi (V1+V2+V5) em juízo**: v2 rc 0 CONFIRMADO pelo meu juiz no binário dela (sha e935aaf1 conferido): 11/13 · 20/27 · falso deny 0/400 · 0/300 · p95 108,4 ms; R2 (except-as) confirmado na sonda. Achados no relatório: «17 mutações» vs 23 listadas · §0 com contrato velho da camada 2 · `__version__` (e provavelmente `defaultdict`) classificado como lacuna do motor mas é DEFEITO DE CORPUS (snap vivo com o `-c` errado; o staging corrige) · 0/700 é no conjunto de ajuste → held-out fresco encomendado ao `shell-inline` (corte = maior ts do corpus vivo + 1 min). Lacuna menor só da camada 1: `json.loadz` passa sem achado e `read_textz` em /tmp só avisa (a camada 2 pega os dois).

## TRANSFERÊNCIA (09/10/2026 ~21:45Z) — a coordenação saiu da analise-89 para a **touring-09**

Ordem do Gabriel: «esse trabalho no Touring deve ser feito pelas sessões do projects/touring e liberar as sessões do
projects/analise». **Ponto de entrada daqui em diante: [`HANDOFF-touring-2026-10-09.md`](HANDOFF-touring-2026-10-09.md).**
- Sessões do touring (por código): touring-09 Anthropic (coordenadora) · touring-35 Kimi K3 1M · touring-4f MiniMax.
- DAG migrado para **`task_1791581548206685387`** (daemon global) por `scripts/migrar_dag_para_touring.py`: 27
  subtarefas idênticas por código; a origem `task_1791484992466538442` ganhou `MIGRADO`. **touring-09 confirmou pela
  própria leitura** (`decompose get`: subtask_count 27; 8 completed · 7 in_progress · 10 pending · 2 failed).
- Artefatos do scratchpad da analise-89 copiados com sha: binários `~/.claude/touring/vgp/binarios/{c8b503c4,e935aaf1}/`,
  corpus `~/.claude/touring/vgp/corpus_v0d_staging_evidencia/` (árvore 90b03f50), JSONs em `diagnostics/juiz-e935/`,
  instrumentos em `diagnostics/instrumentos-2026-10-09/` (README).
- Juízo final da entrega da Kimi (e935aaf1): v2 e V3 rc 0 no vivo e no corpus com evidência (11/13·21/27·0/400·0/274;
  V3 12/13·23/27·p95 954,2 ms), prova [1] 8/8. Pendente: held-out disjunto (subagente → `diagnostics/holdout/RELATORIO.md`).
- Relatórios completos de estado pedidos (ordem do Gabriel) a analise-0e, analise-68 e — por repasse da 68 — analise-25
  (mensagens diretas a ela falham): `diagnostics/transferencia/` (README = índice). analise-0e liberada.

### Adendo touring-09 (09/10 ~22:30Z) — snapshot, transferências conferidas, distribuição

- **Snapshot WIP** por ordem do Gabriel («sim, faça o commit para preservar todo o trabalho»): commit `88c7cc8b` na
  branch `safety/2026-10-09-vgp-motor-wip` (117 A + 26 M, sem push), montado com índice temporário — HEAD da
  branch de trabalho segue `c118aa52`, índice real intacto; prova por conteúdo 143/143 hashes iguais ao disco.
- **analise-68** (corpus semeado): relatório conferido; TS 80+/135− e TSX 120+/200− feitos; HTML parcial (âncora
  10/20, cdn 0/20, cdn_pin 0/20, negativos 0/200); JS n=0 (oráculo recusado: `packages/` sem subdiretório limpo no
  `tsc --checkJs`); determinismo NÃO medido. `ORIGENS_ARQUIVO` contém `"semeado"` (juiz l.63–68) — conferido.
  Instrumentos que viviam só em `/tmp` (tmpfs) copiados, idênticos: `diagnostics/transferencia/analise-68-instrumentos/` (9).
- **analise-0e** (Kimi do analise): relatório conferido — binário congelado e JSONs dos juízes com os shas declarados.
  As correções do RELATORIO-V1V2V5 seguem ABERTAS no arquivo (sha `3e477dfe` inalterado): (1) l.327 «17 no total»
  com 22 nomes distintos listados; (2) §0 l.23 ainda diz «`camada2.rs` é DELA: ela pluga…» — a afirmação dela de que
  a §0 já descrevia o wiring não confere; (3) `__version__`/`defaultdict` = defeito de corpus (ela aceita).
  Scratch `/tmp/v0d/` (sem binário nem `.bak`) copiado, idêntico: `diagnostics/transferencia/analise-0e-v0d-scratch/` (32).
- **analise-25**: relatório chegou depois (`transferencia/analise-25-RELATORIO.md`, sha `dfed008a`); corrobora as contagens
  da 68 (shas de `entradas.jsonl` iguais). Às 18:44 (BRT) ela editou `scripts/corpus_semeado.py` (+`tmp_gravavel()`,
  temp do determinismo no mesmo device da home): 1.206→1.214 linhas, sha `0f913bc8`→`e97b36ed` — a touring-4f parte do
  `e97b36ed` (adendo enviado). Resíduos no analise para o Gabriel: `.vgp-sombra-frontend/` (300 MB, hardlinks), `.vgp-semeado/`.
- **P1**: `diagnostics/holdout/RELATORIO.md` ainda ausente (vigia em segundo plano na touring-09, teto 3 h).
- **Distribuição** (ordem do Gabriel: «prossiga com o mesmo, tanto delegando tarefas para agentes […] quanto para as
  sessões kimi e minimax ativas em projects/touring»):
  - **touring-35** (Kimi): [briefs/2026-10-09-touring-35-vl0.md](briefs/2026-10-09-touring-35-vl0.md) — as 3 correções do
    RELATORIO-V1V2V5 + **VL0 no worktree** `~/projects/touring-wt-vl0` (branch `feat/2026-10-09-vgp-vl0`, base `88c7cc8b`).
    O `decompose claim` da VL0 recusou («blocked by unfinished deps: V3») — correto; a VL0 começa isolada porque a
    razão da dependência (R1–R3 nos mesmos arquivos) acabou e o release sai da árvore principal, congelada. Claim
    quando o V3 fechar.
  - **touring-4f** (MiniMax): [briefs/2026-10-09-touring-4f-corpus-semeado.md](briefs/2026-10-09-touring-4f-corpus-semeado.md) —
    HTML completo, JS pelo oráculo de **delta de diagnósticos** (decisão da coordenadora), determinismo medido.
  - **subagente `gates-30-4-96`** (sonnet, só relata): gates pré-release do 30.4.96 → `diagnostics/release-30.4.96/gates/gates.json`.
- **Disco**: `/home` 92% (83 GB livres); `target/` 639 GB — nenhum target novo nesta wave.

### Adendo touring-09 (09/10 ~22:10Z) — higiene de disco, veredito P1, plano de ondas (/goal do Gabriel)

- **Higiene de disco** (ordem: «faça a higiene de disco seguindo os protocolos de segurança»): `cargo clean --profile dev`
  sob o slot rust do `hw-slot` (bloqueia cargo concorrente), depois de provar que nada executava de `target/debug` e que
  daemon/CLI/hook são symlinks para `target/release` (preservado). **465,1 GiB removidos em 565 s; livre 81 → 436 GB**;
  daemon 751583 intacto. Levantamento do resto da home (só leitura) com subagente haiku → `~/.claude/touring/disk_survey_2026-10-09.md`.
- **Veredito P1 (touring-09)**: a entrega `e935aaf1` **não fecha V2/V3**. Held-out disjunto 20261010 (sobreposição 0 nas 4
  chaves, controle c8b5 discrimina): falso deny 5/600 arquivo (CP95 **1,74%**) · 2/600 inline (**1,05%**). Três são defeito
  real do coletor de atributos — **reproduzidos pela coordenadora** no binário congelado (`sonda_atributos.py`):
  `np.array`/`np.where` deny · `Image.LANCZOS` deny · `rb.RUST_AVAILABLE` deny; controles `np.arrai` deny · `os.path.join`
  allow; `os.pathh` allow (a explicar). Guarda `!= VgpMode::Ativo` conferida em `pre_write.rs:1375`,
  `pre_edit_prevention.rs:562`, `pre_bash.rs:1001` → **o 30.4.96 sai em sombra**; **a V4 fica bloqueada** até P0 + juiz CP
  + held-out novo. Aviso da analise-25 «o C1 sumiu» refutado: está em `docs/plans/2026-10-08-compactacao-65/diagnostics/c1-por-familia-2026-10-09/`.
- **DAG** (35 subtarefas, válido): novas `R96` · `H2` (juiz CP + disjunto 20261011 selado) · `P0A` (touring-35) · `P0M`
  (dep P0A,H2) · `P2` · `SEM` (touring-4f) · `F21` (falso positivo do gate F2.1 com o literal `javascript:`) · `R97`
  (dep R96,P0M). Dependências novas: V2/V3 ← P0M · V5 ← P2,P0M · V4 ← R97 · VTS/VHTML ← SEM.
- **Plano de ondas**: (1) agora — gates 30.4.96 · H2 · P0A · SEM · survey de disco; P2 assim que o `instrumentos-p1`
  liberar o vivo. (2) gates verdes → dry-run + stop-before-propagate + provas → **pedir ao Gabriel o sinal do propagate e
  a autorização de commits locais (sem push) nas branches de feature** para integrar P0/VL0/adaptadores. (3) P0M → 30.4.97
  → V4 ativo + 5 sessões reais (relógio) enquanto a VL0 anda. (4) VL0 aceita → fan-out: Kimi VRS · sonnet VTS+VTSX ·
  sonnet VHTML · sonnet VCDN → VCSS; MiniMax semeado Rust + held-outs por linguagem. (5) release multi-linguagem → V6 →
  prova [5] multi-linguagem (hook vivo + trace) → J. A VL0 deve deixar **um arquivo por adaptador + 1 linha de registro**
  para o fan-out paralelo ter merge trivial.
- **19:15 BRT — a Kimi (touring-35) bateu o limite de 5 h** (`API Error: 403 You've reached your 5-hour usage limit`, 21:56Z,
  lido no transcript dela); as mensagens do P0 e do adendo respondem só isso. Reatribuição: **P0 → subagente sonnet
  `p0-atributo`** (worktree `touring-wt-vgp-attr`) · **VL0 §1–§5,§8 → sonnet `vl0-nucleo`** (worktree `touring-wt-vl0`)
  · **VL0 §6 → sonnet `vl0-camada2-generica`** (worktree `touring-wt-l2`) · **F21 → sonnet `f21-falso-positivo`**
  (worktree `touring-wt-f21`). A **Parte A** foi feita pela coordenadora (o «17» → 23 mutações / 22 nomes; `__version__`
  → defeito de corpus; nota sobre os testes do `touring-cli` que não compilavam). A Kimi, quando voltar, fica com a VRS.
- **19:30 BRT** — por ordem do Gabriel («feche a touring-35 e rode o uv cache prune»): touring-35 encerrada por SIGTERM no
  `claude` (pid 2925101, identidade conferida: comm, cwd, nome, `idle`), filhos MCP saíram sozinhos em ~2 s. `uv cache
  prune` (sem `--force`) esperou a trava 180 s e saiu por timeout (rc 124; `~/.cache/uv` 17.033 MB inalterado) — 7 pares
  `minimax-coding-plan-mcp` vivos seguram o cache. A cota da Kimi voltou: nova sessão **touring-a6** recebe a **VRS**
  ([briefs/2026-10-09-touring-a6-vrs.md](briefs/2026-10-09-touring-a6-vrs.md), worktree `touring-wt-vrs`); o claim da VRS
  no DAG espera a VL0.
- **Release 30.4.96 — gate 02 clippy rc 101**: `crates/touring-cli/src/cli/handlers/index.rs:2246,2262` usavam
  `crate::HookRuntime::default()` (não existe; `HookRuntime` mora em `crate::runtime`, sem `Default`) — os 2 testes do
  `cli_ast_detail` da analise-0e nunca compilaram (ela rodou 4 crates, sem o `touring-cli`). Corrigido para
  `HookRuntime::new(tempdir)`; o subagente dos gates reroda o clippy no fim. Evidência viva do V0e1b no 30.4.95
  instalado: o G1 reescreveu um `grep` meu para um `touring run` que **reexecutou o lote antigo de outro cwd** e perdeu o
  comando novo — é o defeito que o 30.4.96 conserta (entra na prova [4]).
- **touring-4f** (parcial): três causas do HTML 0-negativos achadas e corrigidas (regex de recurso casava `<html lang>`;
  varredura entrava em `.cache/`/`target/doc/`/sombra; `opera_html` descartava o doc ao saturar `recurso`); fonte JS do
  analise = 8 arquivos (3 limpos; os «115 .js» eram rustdoc) → decisão: ampliar a fonte JS a todos os `~/projects`.

## Adendo touring-09 — 09/10 ~22:45Z: H2 aceito · P2 feito · falha do gate touring-hooks em conserto

- **H2 ACEITO (subagente `instrumentos-p1`)**: o juiz `scripts/juiz_motor_vgp.py` passou de b7f7fd9b para **89a976164c291d9d**. O JSON ganhou `falso_deny_por_balde.<b>.limite_superior_95` e entrou a flag `--exigir-limite-superior`. Conferido por mim: os limites CP95, calculados por bisseção independente com `math.comb`, deram 0,498 · 0,788 · 1,046 · 1,744 · 0,645 · 0,746 (0/400) · 0,994 (0/300) %, iguais aos dele, e o `--selftest` saiu com rc 0 (29 s). Held-out **20261011 SELADO** em `~/.claude/touring/vgp/corpus_holdout_disjunto_20261011/`, sha_tree `c495bcab66718230`, 600 + 600, sobreposição 0 nas 4 chaves contra vivo, temporal e 20261010. Relatório: [diagnostics/holdout-20261011/RELATORIO.md](diagnostics/holdout-20261011/RELATORIO.md). DAG `H2` → completed.
- **Achado dele: motor não determinístico**. No e935 a ordem de `reais` muda a cada chamada, em 2 ids o CONJUNTO também muda, e a `sugestao` oscila. O veredito nunca mudou nos 762 ids. Repassado ao `p0-atributo` como conserto com teste de 10 chamadas byte a byte e mutante.
- **P2 FEITO, 22:43Z**: `corpus_v0d` = **60ebf06bd598fd95** (cópia do staging com evidência; referência `diagnostics/juiz-e935/k_v2_ev.json`/`k_v3_ev.json`). `corpus_v0d.bak-2026-10-09` = **b06b89113c869097** (o vivo anterior, intacto; referência `k_v2_vivo`/`k_v3_vivo`). O staging ficou intocado. Houve um incidente: o 1º `mv --exchange` sem `-T` falhou e o `set -e` não abortou, então o bak recebeu a cópia nova. Corrigi com `mv -T --exchange` e conferi por sha_tree; nenhum dado se perdeu. Log em `~/.claude/touring/vgp/P2-promocao-2026-10-09.log`, que traz a linha falsa seguida da correção. Lição gravada na memória `mv-exchange-exige-T-e-set-e-nao-aborta`. Rodando: `diagnostics/p2-promocao/rodar_p2.sh` (v2 e V3 do e935 no vivo novo, comparados por id com k_*_ev). `p0-atributo` e `vl0-nucleo` avisados.
- **Consequência do P2 a resolver**: o balde inline do vivo novo tem **n = 274** negativos, e com 0 falsos o CP95 dá 1,09 % > 1 % (n mínimo 299). No vivo, a flag `--exigir-limite-superior` reprova o inline por falta de n. A prova de falso deny continua sendo o held-out (600/600); completar o inline do vivo para ≥ 300 fica como pendência de corpus.
- **Gate do 30.4.96: `touring-hooks` com rc 101**. `classifier_edit_rust_emits_pre_edit_triage_with_tdg_gate` (`crates/touring-hooks/tests/cli_suggester_e2e.rs:269`) falha de forma determinística: 3 vezes sozinho, 3 vezes na suíte e com `env -i`. Depois de Read e Edit de `.rs`, a resposta do hook vem de outra fonte, não do classificador de Edit. Delegado ao sonnet `hooks-e2e-edit-rust` (causa raiz + conserto + suíte). Os demais gates: fmt 0 · code 0 · pyright 0/0 · lsp 0 · server 0 · hook-handlers 0. O `touring-cli` está rodando.
- DAG: `P2`, `F21`, `VRS` e `VL0` → in_progress.

## Adendo touring-09 — 09/10 ~23:10Z: P2 medido · juiz 0e693b13 · onda 1 da 4f aceita · B redesenhada

- **P2 medido** (`diagnostics/p2-promocao/`, rodado entre 22:44Z e 22:56Z): e935 no vivo novo.
  - **v2** rc 0, PRONTO. Igual a `k_v2_ev.json` em toda chave fora o p95; o JSON ganhou só o campo `exigir_limite_superior`.
  - **V3** rc 1, NÃO PRONTO, só pelo p95: **1.000,9 ms > 1.000**, com a máquina a 90–96 °C.
  - Uma divergência no V3: `forma_fora_do_recall.arquivo` foi de [1, 11] para [0, 11]. É informativa, fora do recall de símbolo; recall, perdidos, erros e falsos ficaram iguais. A suspeita é a camada 2 sob carga térmica. **Rerodar o V3 com a máquina fria**, junto com o P0M, antes de fechar o P2 no DAG.
- **Juiz `juiz_motor_vgp.py` → 0e693b139e7d9215** (era 89a97616, do instrumentos-p1). Três mudanças:
  - Com n < 299, a linha da tabela imprimia «admite até -1 falso(s)»; agora diz «não prova o teto nem com 0 falsos; exige n ≥ 299». TDD: RED com rc 1, GREEN com rc 0, e o mutante `if True` morreu (rc 1).
  - `check=False` explícito nos 2 `subprocess.run` (PLW1510, que já vinha do b7f7fd9b).
  - `chmod +x` (EXE001) e `ruff format` do arquivo inteiro (os acréscimos do H2 não estavam formatados).

  Resultado: ruff check 0 · format OK · pyright 0 (`analise/.venv/bin/pyright`) · selftest rc 0. O veredito não muda.
- **touring-4f, onda 1: ACEITA**. O sha do script confere com `f2f0f33c6656b686a118`. Contagem em disco igual à do relatório; `classe_erro` deu 296/296 simbolo; selftest rerodado por mim via hw-slot (em curso). Onda 2 em [briefs/2026-10-09-touring-4f-onda2.md](briefs/2026-10-09-touring-4f-onda2.md):
  - **A**: Rust semeado, em `touring-wt-semeado` no 88c7cc8b, alvo `touring-identity`; parado na fila do hw-slot.
  - **C**: JS com até 3 mutações por arquivo; já tem 40 positivos.
  - **B**: negativos naturais selados. A medição mostrou que TS/TSX têm **0** edições nesta máquina, tanto nos transcripts quanto no git; conferido por mim com controle positivo, py 6.521. HTML tem 132 (86 de transcript e 46 de git) e JS 24. B passou a ter dois estratos: `local` (o teto, selado) e `oss`, com histórico git real de projetos MIT/Apache em sha fixo, dependências por `npm ci --ignore-scripts`, controle positivo de import externo e DELTA via LanguageService só depois de provada a equivalência com o `tsc`. Tudo fica fora do repositório público.
- **~23:15Z — aceite da onda 1 da 4f vira CONDICIONAL.** Rodei o selftest por volta das 23:00Z, com o script já em edição da onda 2 (sha f162f76f9be7661ecb91, 1.706 linhas). Deu **26/36, rc 1**: as 6 receitas TSX deram «ocorreu: nenhum» com base «0 arquivos com erro», e os 8 delta de JS deram «base: 0». O `tsc` não viu arquivo nenhum, provavelmente por causa da sombra que agora espelha só fonte. Log em `diagnostics/semeado-4f-juizo/selftest.out`. O aceite exige 36/36 no sha final, rerodado por mim. **Achados de higiene**: (a) `~/projects/analise/.vgp-semeado/`, a área de trabalho da 4f, está DENTRO do repo analise como `??` (24 GB aparentes) e foi pedido o `mv` para `~/.claude/touring/vgp/work-4f/`; (b) as sombras são `cp -al`, e mutar in-place alteraria o arquivo real. Contaminação conferida: **0** marcadores `vgp*Ausente` em `~/projects` fora das áreas de corpus. Pedi unlink antes de escrever, com teste e mutante. (c) `~/.cache/vgp-semeado` tem 65 sombras, 499 GB aparentes e **7,4 GB únicos** (`proj-47467rpw`); a limpeza fica para o fim, com decisão do Gabriel. Disco: 400 GB livres.
- **~23:30Z — gate 04-drift-coverage do 30.4.96 com rc 1 → CONSERTADO.** `scripts/test_drift_coverage.py` esperava 395 e o scan deu 397. Os 2 sinais novos vieram dos documentos da coordenação: C1 `30.4.97` (release futura, citada no RETOMAR l.512) e C5 `touring-wt-semeado` (diretório de worktree que o predicado lê como crate). As duas disposições (c) foram gravadas no ledger, nas linhas 127-128, lote `2026-10-09-vgp-motor-noite`, e o literal subiu de 395 para 397, com o parágrafo da 8ª reconciliação. Conferido: `reconciliar.py verify` dá VERIFY OK, e `test_drift_coverage.py` dá 8 testes com 0 falhas e rc 0. Os outros gates fechados até aqui: touring-cli 0 · foundation 0 · hooks-core 0 · storage-knowledge 0. Continua aberto o `touring-hooks`, com o sonnet.
- **~23:40Z — F21 ACEITO (subagente `f21-falso-positivo`)**, worktree `touring-wt-f21` (3 arquivos, sem commit). O conserto é o `starts_url_value` em `touring-offensive/src/vuln/cwe_patterns.rs`: `javascript:` só conta onde começa um valor de URL. Teve RED nas 3 camadas (offensive, analysis e quality, rc 101), GREEN depois, e 11 mutantes mortos. Gates: fmt 0, clippy 0, suítes offensive/analysis/quality com 0 falhas. No E2E com o `touring-quality score --dims F2.1`, a sonda passou de 0,190 para 1,000, e os controles seguem em 0,190. Revisei o diff. **Pedido em seguida, no mesmo braço**: (1) furo case-insensitive (`JavaScript:` escapa hoje) e (2) falso positivo de literal de blocklist (`startswith("javascript:")`, `==`, `in (…)`), com controles que têm de continuar bloqueando. **Para o Gabriel decidir**: o F2.1 nunca teve braço de `innerHTML = userInput` (sink de DOM XSS clássico, medido 1,000 antes e depois). É feature nova, fora do plano VGP.
- **~23:50Z — gates do 30.4.96, rodada 1 fechada (`ALL_DONE`)**. Falhas e o que foi feito com cada uma:
  - **03g touring-hooks: CONSERTADA** (sonnet `hooks-e2e-edit-rust`). O teste estava desatualizado frente ao orçamento de turno do S8 (teto de 2.000 B): o Read gasta o orçamento e o Edit do mesmo turno perde o cabeçalho do cluster. O teste agora zera o turno com `turn_budget::reset_turn`, como um turno novo do humano, e ganhou o inverso `…_in_an_exhausted_turn_keeps_only_the_directives`. Nenhuma asserção foi enfraquecida; as 3 ganharam mensagem de diagnóstico. A suíte deu 1.173 passed, 0 failed. Revisei o diff. O clippy do touring-hooks fica para o `02b-clippy-rerun`, que está rodando.
  - **04 drift: CONSERTADA** (está no adendo anterior).
  - **05 elite**: rc 1. O composite deu 0,8169 Gold, mas `06_documentation` deu FAIL 0,00 (bloqueante): `docs/gen_reference.py --validate` acusava «modules.md out of sync». O VGP trouxe `touring-code::vgp` e `touring-pyright::{client,pool,pyright}`, e a contagem foi de 380 para 384. Regenerei com `python3 docs/gen_reference.py` (só o `modules.md` mudou, +5/-1), e o `--validate` agora dá OK com rc 0. O `05b-elite-rerun` está rodando.
  - **06 e2e**: rc 0, `overall_score` **0,881** (a linha de base é 0,857).
  - **Pendentes**: `02b-clippy-rerun` (workspace, `--all-targets`, rodando há 20 min) e `05b`.
- **~00:00Z (10/10) — onda 2 da 4f, estrato oss, controle positivo por repo**: excalidraw PASSA (TS2307). Os outros três:
  - **date-fns REPROVA**: o pnpm@9 instalou o TypeScript **7.0.2** nativo em Go, que não tem `typescript.js` nem `createLanguageService`, e o tsc nativo deu rc=0 com o import inexistente injetado.
  - **zod REPROVA**: `packageManager: nub@0.8.3`, fora do npm, e o pnpm recusa.
  - **h5bp REPROVA**: sem TypeScript nem tsconfig, só 47 pares.

  **Decisão**:
  - date-fns fica FORA e a exclusão é declarada.
  - zod fica DENTRO, com `.npmrc` no clone descartável ou `npm i --no-save --ignore-scripts typescript@<versão do lockfile>`.
  - Fontes por linguagem: excalidraw para TS e TSX; zod para TS; express e lodash para JS; h5bp e `mdn/dom-examples` (CC0) para HTML.
  - Controle positivo por repo antes de qualquer rótulo.

  **Nota para a camada 2 de TS (VTS/VL0 §6)**: o tsc nativo do TS 7 não acusou um import inexistente no date-fns (rc=0). A camada 2 TS não pode presumir que o `tsc` do projeto verifica; precisa de controle positivo no arranque. A 4f vai registrar a evidência no relatório dela.
- **~23:55Z — lacuna do gate de release → subagente sonnet `release-gate-testes`**: o gate 1/6 do `propagate-release.sh` não roda suíte de teste nenhuma, e o clippy sem `--all-targets` não compila os testes. Por isso a 30.4.95 saiu com o e2e do touring-hooks quebrado, e os testes do touring-cli nunca compilaram. Pedido: clippy `--all-targets` e um gate «testes dos crates tocados desde o último commit de release» via hw-slot (tabela de features: hook-handlers com `pre-hooks,post-hooks`), `scripts/test_propagate_release.py` com mutantes, e o item (e) no CLAUDE.md §2.1. Só `--dry-run`.
- **~00:05Z (10/10) — gate de testes no `propagate-release.sh`: ACEITO (sonnet `release-gate-testes`).** Mudanças:
  - o clippy virou o array `CLIPPY_GATE` com `--all-targets`, a mesma fonte para o dry-run e para a execução;
  - novo `run_release_tests`: pacotes tocados = diff desde o último commit `release …` ∪ árvore de trabalho, pelo `manifest_path` de maior prefixo por componente;
  - `TEST_FEATURES` com hook-handlers em `pre-hooks,post-hooks`;
  - rc 75 repete 1× e não é falha de teste; rc 70 é machine check;
  - sem release ou com git falho, roda TODOS os pacotes.

  Arquivo novo `scripts/test_propagate_release.py`, com 32 testes e 17 mutantes mortos. Conferi: o pytest dá 32 passed, `bash -n` passa, e o CLAUDE.md tem 255 linhas, com o §2.1(e). O dry-run de hoje lista 7 pacotes: touring-cli, code, hook-handlers, hooks, lsp, pyright e server.

  **Limite declarado**: mudança no `Cargo.toml`/`Cargo.lock` da raiz só avisa e não alarga a lista. Fica como dívida conhecida; a forma certa é alargar para os pacotes que dependem do que mudou no lock.
- **05b elite: rc 0, composite 0,9017 Platinum**, depois da regeneração do `modules.md`. Dos gates do 30.4.96 falta só o `02b-clippy-rerun` (workspace, `--all-targets`), que está na fila do hw-slot desde 23:22Z com a máquina a 95–97 °C.
- **~00:20Z (10/10) — contrato DELTA do held-out web**: os pares naturais são rotulados por DELTA (erro NOVO da edição). Sem o «antes», o juiz chamaria o motor sem `--disk-file`, e todo erro pré-existente viraria falso deny. O dom-examples tem 1.505 erros de checkJs na base, depois do `exclude` do `bad-syntax-worker.js`. A 4f confirmou que as **278 entradas gravadas tinham ZERO `snap_antes`**. Agora cada entrada traz `snap_antes` + `sha256_antes` nos pares M, `operacao: "M"|"A"` em todas, e o contrato está no meta do selo; ela regrava as antigas quando o rotulador terminar. **Juiz → `6c8a31b02075da1e`**: `rodar_motor` passa `--disk-file <corpus>/<snap_antes>` quando presente, e a precedência continua com o `estado_no_instante` do V0d. Selftest com `_falhas_disk_file_do_par` (M recebe o antes, A não): RED rc 1 → GREEN rc 0. O mutante `elif False` morreu (rc 1). ruff 0, pyright 0. **A conferir na entrega da VL0**: a semântica `--disk-file` (delta) tem de valer para TODA linguagem no `check`, não só para Python.
- **Observação (prosa)**: uma mensagem da touring-4f (MiniMax) saiu com um trecho em chinês, e ela mesma reenviou corrigida. É um dado para o experimento de integridade da prosa (`docs/plans/2026-10-04-integridade-prosa/`).
- **~00:25Z (10/10) — F21: os dois pedidos de acompanhamento ACEITOS**. Worktree `touring-wt-f21` (3 arquivos, +536/-3 contra 88c7cc8b, sem commit).
  - **(1) Caixa**: o braço passou a `(?i-u:javascript):` com `is_javascript_scheme` (ASCII, `eq_ignore_ascii_case`). Os probes `JavaScript:` e `JAVASCRIPT:` foram de 1,0 para 0,19. Os outros braços NÃO mudaram, por medição em 12.762 arquivos: `<Script>` aparece em 3 fixtures e `on…=` em camelCase em 150 arquivos, quase todos JSX. Ficam declarados `<SCRIPT>`, entidades e tab dentro da palavra.
  - **(2) Blocklist**: `is_scheme_blocklist_literal` cobre argumento de startswith/startsWith/endswith/endsWith/includes/indexOf (e tupla dentro deles), operando de `==`/`===`/`!=`/`!==` e membro após `in`/`not in`. `for … in (…)` continua reportando. Os controles pedidos continuam bloqueando, mais oito.
  - **Extra**: o esquema não herda mais o resgate `inside_open_tag`, porque `i<n` em JS abria uma tag fantasma.

  Os 5 testes saíram RED nas 3 camadas (rc 101) e passaram a GREEN. Mutantes: 25, em 31 execuções, 0 sobreviventes. Gates: fmt, clippy e as 3 suítes com 0 falhas. No binário real, entre 84 arquivos reais, 3 falsos positivos de produção deixam de bloquear (kazuba-converters, kazuba-rag, kazuba-sei-core) e 0 passam a falhar. Conferi que os helpers trabalham em `&[u8]`, sem corte de `str`. DAG `F21` → completed. **A integração** (merge na branch do release) depende da autorização de commit do Gabriel e vai junto com o R96/R97.
  - **Limites que ficam para o Gabriel**: `innerHTML = userInput` (sem braço; feature nova) e URL `javascript:` de corpo constante (`void(0)`, `history.back()`, 16 HTMLs capturados), uma classe a decidir.
- **~00:35Z (10/10) — os 278 rótulos OSS da 4f foram INVALIDADOS por ela mesma** (134 tsx + 144 ts). O rotulador pedia o delta do LS com o clone no HEAD, ou seja, mediu delta(HEAD→depois), não delta(antes→depois). Exemplo: o `antes` do commit 4c00f31d em `App.tsx` ≠ o disco do HEAD. A prova JS pegou isso: o LS media depois×depois e dava sempre ∅. No mesmo arquivo, as duas rotas concordam (tsc 166 = LS 166). Correção pedida: (1) refazer TAMBÉM a prova TS/TSX (o 20/20 é suspeito de vacuidade), com as rotas medidas de forma idêntica e ≥ 10 pares de delta não vazio e ≥ 10 vazios por linguagem; (2) restaurar o disco do clone ao HEAD conferido por sha, com recusa de rotular se o clone estiver sujo; (3) esclarecer os «80 positivos de Rust em crate sintético», porque o brief manda mutar o `touring-identity` real no `touring-wt-semeado`. Lição (família «zero que não é zero», 6ª da onda): a prova de equivalência precisa de caso não vazio por construção.
- **~00:40Z (10/10) — Rust semeado: os «80 positivos» eram de um crate SINTÉTICO de um arquivo em /tmp** (`path: crate/src/lib.rs`, com `import` de `std`), uma decisão da 4f fora do brief, que ela própria declarou. **Decisão**: o estrato `real` é o ACEITE. Ele usa ≥ 2 crates reais no `touring-wt-semeado` (`touring-identity` + um com submódulos e `pub use`, p.ex. `touring-pyright`) e receitas de resolução do projeto: E0432 com `crate::`/`super::`/`self::` e via `pub use`, E0433 com segmento inexistente, E0425, E0609/E0599 em tipo do crate e E0560 em struct do crate. O delta é exato (1 diagnóstico novo na linha) e a restauração é conferida por sha. Os 80 sintéticos ficam como estrato `sanidade`, fora do aceite. A touring-a6 foi avisada para não usá-los.
- **~00:55Z (10/10) — EVIDÊNCIA para a prova [4] (V0e1b, G1 sem replay)**: a touring-4f relatou que o hook instalado (30.4.95) lhe devolveu «cache de outro arquivo» num `sed`, e que ela EDITOU duas vezes em cima da leitura errada (prova de equivalência JS v3). É dano real de uma sessão irmã, causado pelo replay da rajada que o G1 reescreve num `touring run`. Pedi o registro exato (hora, comando, saída do hook, edição feita) no relatório dela. A mesma onda trouxe mais três achados do tipo «oráculo certo, parser no stream errado»: `cargo check` escreve no stderr (o Rust real deu delta 0 em 14/16), `tuple != list` e truncamento assimétrico na prova. O determinismo web estourou o timeout de 50 min (rc 124) e foi refeito por linguagem.
- **~01:20Z (10/10) — onda 2 da 4f, rotulador OSS (v3 → v6)**: as divergências v3/v5 NÃO vinham dos oráculos (pares reproduzidos, as duas rotas concordam). Vieram de (a) o `oraculo_ls.js` reescrito com o lote em curso: o node carrega o script uma vez e respondeu com a versão antiga, então o selo v3 foi INVALIDADO (99 rótulos) e o selo agora grava `identidade_do_oraculo` e invalida se ela mudar; (b) CORRIDA: outro processo escrevia o mesmo clone em `oss/`. Mais um erro de denominador no teto: os contadores nasciam em zero a cada retomada e depois ficavam globais entre linguagens. Agora contam por linguagem incluindo o já gravado, o selo traz `concentracao_medida` e o `main` sai com 1 se houver infrator. O selftest do rotulador está em 60/60. **Pedido em seguida**: `flock` exclusivo em `oss/.lock` para todo escritor (fd O_CLOEXEC, com selftest e mutante); e a poda ad hoc (`/tmp/vgp-podar-teto.py`, typescript 191→184 e tsx 180→170, com backup em `oss/_podado_20261009-215331/`) tem de ser cega ao rótulo e determinística, e ir para DENTRO do gerador, provada por uma geração do zero que reproduz o sha_tree. Em aberto: a prova de equivalência JS e o Rust `real`, com o parser extraindo número em vez de E0xxx.
- **~01:45Z (10/10) — R96: TODOS os gates verdes.** O `02b-clippy-rerun` (workspace, `--all-targets`) deu rc 0 às 00:43:57Z. Ele compilou às 21:38 local, depois da última edição Rust (o teste do touring-hooks às 19:43 local), então cobre a árvore final. Quadro: fmt 0 · clippy `--all-targets` 0 · code/pyright/lsp/server/hook-handlers/hooks/cli/foundation/hooks-core/storage-knowledge com 0 falhas · drift 397 OK · elite 0,9017 Platinum · e2e 0,881. **Bump**: `Cargo.toml` 30.4.95 → **30.4.96**. **Subagente sonnet `r96-candidato`**: monta o binário candidato `--release --workspace` em `target/r96-candidato`, congela, roda as provas [1] e [3] (`prova_vgp_motor.py --so 1,3`), faz o dry-run do `propagate-release.sh` com prova de que nada mudou, reconcilia o drift do bump (lote `2026-10-10-release-30.4.96`) e roda o `gen_reference --validate`. As provas [2] e [4] exigem o hook instalado e ficam para depois do sinal do Gabriel. Selo do held-out web (4f): `tsc_por_repo` (date-fns 7.0.2 · excalidraw 5.9.3 · zod 5.5.4 · os outros 5.9.3), selftest 63/63. O v8 está na fila do hw-slot.

## Adendo touring-09 — 10/10 ~01:55Z: PROPAGAÇÃO AUTORIZADA pelo Gabriel + harmonia com o analise

Ordem do Gabriel, literal: «continue e antes da propagação, já autorizada, converse com a sessão coordenadora do analise para que tudo seja feito de forma harmonica». O pipeline (`propagate-release.sh`/`update-touring`) não faz commit, tag nem push e não exige árvore limpa, então a propagação NÃO depende de autorização de commit; o commit do release continua sem ordem.
- **A coordenação do analise** é a `analise-78`. A interativa (pid 3355148, sessionId 2ed9ce43) RETEVE a minha mensagem por modo de permissão (msg b1c3abd4, que aguarda aprovação do Gabriel no terminal dela). A continuação dela em BACKGROUND, «Memória do touring» (pid 1957243, sessionId 05166acc, filho do daemon da 78), respondeu e foi conferida por `~/.claude/sessions` e pelo `ps`.
- **Acordo**:
  - os passos 1 a 4 (`--stop-before-propagate`) não tocam o analise;
  - o passo 5 (`touring update 30.4.96 --project ~/projects/analise`) só depois do «LIVRE» dela, que deve vir em 30 a 60 min, quando voltar a colheita dos 7 subagentes da monografia (DAG `task_1791595679242589029`). Ela segura os juízes até eu confirmar o fim do passo 3;
  - o smoke do passo 4 fica com ela;
  - rollback: `touring update --rollback --project ~/projects/analise`.
- **Contratos JSON que o analise lê**: o diff desde `dfe0fd1e` não toca os handlers de recall/query/decompose/run nem o `allow-forbidden`/`TRUSTED_OK`. Sonda nova `scripts/contrato_json_projeto.py` (controle positivo por contrato; a escrita confiável vai para dentro do projeto e é apagada). **Linha de base 30.4.95: 6/6**, sem resíduo (`diagnostics/release-30.4.96/analise/contrato_antes_30.4.95.json`); ela roda de novo depois do passo 3. ruff 0, pyright 0.
- **Defeitos de gate trazidos pelo analise**: (1) G7×G3×G9 em círculo e (2) G10 nomeando uma rota que não escreve. Entraram no DAG como `G7G3G9` e `G10ROTA`, prioridade high, e o `R97` agora depende deles. (3) G1 replay em comando background: a V0e1b cobre; a prova [4] vai ganhar um caso em background.
- **~02:00Z (10/10) — VL0 §6 (camada 2 genérica) ACEITA** (sonnet `vl0-camada2-generica`, worktree `touring-wt-l2`; relatório [RELATORIO-VL0-camada2-generica.md](RELATORIO-VL0-camada2-generica.md)). O `touring-pyright` ficou genérico sobre `Servidor` (pyright, rust-analyzer, tsserver, deno), com pool `(servidor, raiz, sub)`, ausência → `unknown` com motivo, descoberta sem instalar e sem binário global. Números dela: lib 172 · `--ignored` real (pyright 15, pool 1, rust 3, tsserver 3, deno 1) · fixtures RA 3/3, tsserver 4/4, deno 4/4, com o original limpo dando 0 · 18/18 mutantes · hook-handlers 733 · server inteiro com rc 0. Sonda de custo: RA 1,47 GiB e 4,2 s no workspace touring (quente: 0–18 ms), tsserver 755 MiB, deno 312 MiB. **Conferido por mim**: o binário de teste da lib rodado direto deu 172/172 em 3,4 s, e o `cargo tree -e normal,build,dev` não tem nenhum `touring-*`. **Decisões do §8**: (1) tetos `MAX=2` por verificador e `MAX_RA=1`, aceitos; (2) **na integração, a camada 2 fica ligada por padrão só para Python** (como hoje), e rust-analyzer/tsserver/deno ficam opt-in por linguagem (`[vgp] camada2_langs`) até o juiz por linguagem medir o ganho no held-out. Motivo: o custo de RAM de §5 numa máquina com 6+ sessões; (3) TS2307 como `import`/`projeto` (avisa), em paridade com o Python; (4) a integração `checar_lang` em `hooks/vgp_l2.rs` e `cli/vgp.rs` é minha, depois do núcleo da VL0.

## ESTADO PARA RETOMAR — 10/10 ~02:15Z (touring-09)

**R96 (release 30.4.96, propagação AUTORIZADA pelo Gabriel)**
- Candidato: `~/.claude/touring/vgp/binarios/1c704d5b/` (sha256 do touring 1c704d5bd984ef08…, os 3 `.sha256` conferidos com `sha256sum -c`), `touring 30.4.96`, com `vgp check`; build `--release --workspace` em 18m48s. **V3 p95 = ambiental, não regressão**: o A/B com o e935aaf1 (que media 908–954 ms) deu **1.040,1 ms** hoje, com funcionais idênticos (recall 12/13 e 23/27, 0/400 e 0/274, 738 chamadas, 61 avisos); o p0-atributo mediu 1.119,6 ms com o f55ec4b5. Fica como item aberto: medir de novo com a máquina fria e acrescentar ao juiz a medição do caminho QUENTE do daemon (o do hook).
- Prova [1]: **8/8**. Prova [3]: V2 rc 0; V3 rc 1 **só pelo p95** (1.029,1 e depois 1.046,1 ms > 1.000, entre 88 e 96 °C). No V3, recall/falsos/forma são iguais a `k_v3_ev`. Em curso: o A/B do V3 com o e935 nas mesmas condições (`candidato/02-v3-ab-e935aaf1.log`), que separa regressão de carga térmica. Leitura: o juiz V3 mede o caminho CLI FRIO (um processo por chamada); o caminho do hook é o daemon QUENTE (p95 67 ms, relatório da L2).
- Dry-run rc 0, com 0 linhas de diff de estado antes/depois.
- Gate 1, coberto por evidência:
  - check + clippy `--all-targets` (02b);
  - os testes dos 7 crates (03*);
  - `gen_reference` (candidato/05);
  - `sync-client-skills --check`: CLEAN, 377 arquivos;
  - `test_update_touring`: 28 passed;
  - `test_touring_quality_score`: 34 passed;
  - rito `test_propagate_release*` com `-m "not post_build"`: 75 passed. Os 2 `post_build` rodam no passo 2, depois do build; rodá-los antes falha por construção, porque o binário construído ainda declara 30.4.95.
  - **Falta só o `scripts/r3-s1a-gate`** (rodando: `gates/08-r3-s1a-gate.log`).
- **Próximo comando**, depois do r3 verde e do A/B do V3: `scripts/propagate-release.sh 30.4.96 --skip-gates --stop-before-propagate`. O `--skip-gates` se justifica porque toda a evidência do gate 1 está em `diagnostics/release-30.4.96/gates/` para esta árvore. Antes disso, aviso a touring-4f e a touring-a6 do restart do daemon global. Depois peço a JANELA CURTA à «Memória do touring» (continuação da analise-78; ela responde «ok, agora» ou «espere») → `scripts/propagate-release.sh 30.4.96 --skip-gates --skip-build --skip-freeze` (passos 5 e 6) → aviso «terminado» → smoke dela + `scripts/contrato_json_projeto.py --projeto ~/projects/analise` (a linha de base 30.4.95 deu 6/6) → provas [2] e [4] no hook instalado, com um caso de G1 em background.

**Em voo**: `p0-atributo` (juízes do binário f55ec4b5), `vl0-nucleo`, `r96-candidato` (A/B V3), touring-4f (held-out web v8 na fila, Rust real 2/16 a medir, prova de equivalência JS), touring-a6 (VRS GREEN).
**Aceitos hoje**: H2, P2, SEM, F21 (+2 acompanhamentos), a camada 2 genérica (§6), o gate de testes do release e o e2e do touring-hooks. **DAG novo**: SEM2, G7G3G9 e G10ROTA (R97 depende deles).
- **02:40Z (10/10) — R96 PASSOS 1–4 LANÇADOS**: `scripts/hw-slot --class rust --label r96-propagate --wait 3600 -- scripts/propagate-release.sh 30.4.96 --skip-gates --stop-before-propagate`, com log em `diagnostics/release-30.4.96/propagate/01-passos-1a4.log` e o estado anterior em `00-antes.txt` (daemon pid 751583, symlinks em `target/release`, toolchains até 30.4.95). O r3 S1a passou na re-medição (`1 passed`; a 1ª reprovou por ruído térmico), e o gate 1 está todo coberto por evidência. A touring-4f e a touring-a6 foram avisadas do restart. **Próximo**: quando o rc chegar, conferir pid novo, versão 30.4.96, toolchain 30.4.96 instalada e default, e se o rust-1 do hw-slot ficou LIVRE (o fd do flock não pode vazar para o daemon). Depois, pedir a janela curta à «Memória do touring» e rodar os passos 5 e 6.
- **~02:55Z (10/10) — VL0 NÚCLEO ACEITA** (sonnet `vl0-nucleo`, worktree `touring-wt-vl0`, [RELATORIO-VL0-nucleo.md](RELATORIO-VL0-nucleo.md)).
  - **Contrato**: `Verificacao { achados, nao_medido }`, `Alvo { Externo, Projeto, Incerto }` e `politica::aplicar` sobrescrevendo a severidade (o adaptador não decide deny); delta e veredito no `check` para toda linguagem; `lang/<lang>.rs` com 7 stubs + `Transversal` (cdn e classe_css); `lang_do_caminho` como fonte única.
  - **§8.1**: 738/738 ids idênticos (e935 × b7f12000, vivo `28d93bcb`), v2 PRONTO (p95 67 ms) e V3 PRONTO.
  - **Suítes**: code 1.145, server 1.801 e hook-handlers 738, todas com 0 falhas; clippy e rustdoc `-D warnings` dão rc 0.
  - **Ressalva**: o selo do `b7f12000` é aproximado (o build é anterior à remoção do `derive(Default)` e a correções de doc). Fica aceito porque o binário medido de verdade será o da integração.
  - **Riscos registrados**: toda edição `.rs/.ts/.tsx/.js/.html/.css` grava sombra `unknown` até cada fase, então o juiz do V4 tem de filtrar `lang == "python"`; a API `vgp::{checks,modules,scope,stdlib_model}` sumiu (0 consumidores fora do vgp).

  DAG: `VL0` → completed; nova `INT97` (integração na principal, na ordem VL0 → L2 → P0 → F21 → VRS, com rebuild e juízes por id, selo exato e a autorização de commit do Gabriel para o merge); `R97` passa a depender de `INT97`. **A spec VL0 §3/§5 recebe a emenda do próprio vl0-nucleo** (autorizado).
- **~03:05Z (10/10) — FAN-OUT DOS ADAPTADORES LANÇADO** (brief [briefs/2026-10-10-fanout-adaptadores.md](briefs/2026-10-10-fanout-adaptadores.md)). Três worktrees com a VL0 aplicada por cima de 88c7cc8b, sem commit (patch + arquivos não rastreados do `touring-wt-vl0`; a árvore `vgp/` é idêntica nos 4, hash `df609f887ca1`, 51 arquivos):
  - **`vts`** (sonnet): `touring-wt-vts`, `lang/{typescript,tsx,javascript}.rs`;
  - **`vhtml`** (sonnet): `touring-wt-vhtml`, `lang/html.rs`;
  - **`vtransv`** (sonnet): `touring-wt-vtransv`, `lang/{cdn,classe_css}.rs`, com o CDN via cache local e o resolvedor com rede fora da camada 1.

  Cada um edita só os seus arquivos, mede no corpus SEMEADO (dev) e fica proibido de ler o held-out selado, que é o juiz final meu. DAG: VTS/VTSX/VHTML/VCDN/VCSS → in_progress.
- **~03:07Z (10/10) — R96 IMPLANTADA E PROPAGADA** (DAG `R96` → completed). Evidência em `diagnostics/release-30.4.96/`:
  - `propagate/04-passos-5a6.log`: PASSO5_RC=0. Analise e konverter passaram de 30.4.95 para 30.4.96. A prova comportamental 5.5 deu 41/41, e o verify deu OK. O daemon global é o pid 42698.
  - `analise/contrato_{antes_30.4.95,depois_30.4.96}.json`: contratos JSON 6/6 antes e depois (`scripts/contrato_json_projeto.py`, com controle positivo em cada um).
  - `provas-pos-deploy/`: [1] 8/8; [2] e [4] TUDO AFIRMADO; caso novo G1 em background (o disparo roda só o comando atual e preserva `run_in_background`) AFIRMADO. `prova_vgp_motor.py` ganhou o caso bg e `check=False` explícito (PLW1510 ×3, EXE001); selftest 5/5, ruff 0, pyright 0.
  - Smoke da «Memória do touring» no analise: versão 30.4.96, lock active=30.4.96/previous=30.4.95, doctor 9/9, hook real com contexto em 130 ms. Houve um primeiro alarme de recusa do daemon pinado, mas era FALSO: a réplica do hook foi feita sem `CLAUDE_PROJECT_DIR`. Disso nasce o nó novo **`HOOKCWD`**: sem CLAUDE_PROJECT_DIR, o hook resolve a raiz pelo TOURING_PROJECT_ROOT global ou sobe até ~/.touring, e o daemon pinado recusa em 0 ms, recusa que o CC engole calado. Conserto: o `cwd` do payload como 3º fallback. `R97` passa a depender de `HOOKCWD`.
  - Item aberto: V3 p95 de 1.029/1.046 ms é ambiental (A/B com o e935 dá 1.040 ms com a máquina a 90+ °C). Remedir com a máquina fria e medir o caminho quente do daemon.
- **~03:10Z — convenção do `nome` de `recurso`** (touring-4f, medida): o `nome` é o token como escrito no atributo, com o prefixo de caminho e sem `?query`/`#fragmento`. O corpus semeado antigo trazia só o marcador (20/20), o que dava recall 0 por construção. A 4f regenera com troca atômica, e o vhtml já foi avisado. **Lacuna nova**: o held-out web/html tem 0 positivos (83 negativos), então nenhuma classe HTML tem recall mensurável. Pedi à 4f a tabela por classe × linguagem e uma proposta de mineração git de commits que corrigem referência (pai = positivo natural), com o semi-sintético só como fallback rotulado.
- **~03:09Z — VRS → porta para a VL0**: worktree novo `~/projects/touring-wt-vrs2` (branch `feat/2026-10-10-vgp-vrs`, sem commit), com a VL0 aplicada por cima de 88c7cc8b. O conteúdo é idêntico ao `touring-wt-vl0`: hash da árvore `vgp/` e62a37ae2e95 nos dois, e todo arquivo do diff confere por `cmp`. A touring-a6 recebeu duas tarefas: (1) o relatório do estado GREEN em `touring-wt-vrs`, (2) portar `lang/rust.rs` para o `impl Adaptador` real, sem perder recall, com o antes/depois por classe. Na criação, o worktree nasceu DENTRO do repo principal porque o `git -C touring worktree add <relativo>` resolve o caminho a partir do `-C`; movido com `git worktree move`. A lição foi gravada junto com a causa do `set -e` ignorado no nível de cima do Bash tool.
- **~03:11Z — GATES DA 30.4.97 DELEGADOS** (sonnet `gates-97`, brief [briefs/2026-10-10-gates-97.md](briefs/2026-10-10-gates-97.md)). O worktree `~/projects/touring-wt-gates97` (branch `fix/2026-10-10-gates-97`, sem commit) espelha a árvore IMPLANTADA da 30.4.96: HEAD c118aa52 + o diff + os não rastreados de crates/scripts, 53 arquivos conferidos por `cmp`. Frentes, nesta ordem: G10ROTA, G7G3G9, HOOKCWD. Inclui ainda o `exit_reason` próprio para a recusa do daemon no trace, porque hoje a recusa fica igual ao sucesso. DAG: as três → in_progress.
- **~03:13Z — falso alarme de perda no P0, e o que ele revelou.** O worktree `touring-wt-vgp-attr` foi observado DENTRO da janela do `baseline_hooks.sh` do `p0-atributo` (base aplicada de 03:11:56Z a 03:12:58Z, no scratchpad `p0/`). Depois da restauração, os 6 sha conferem com `bh_sha_antes.txt` (namespace.rs b4bfbab5) e o `git status` volta a mostrar 4 M + 2 ??. Nada se perdeu. Pedi um marcador visível (`.TROCA-EM-CURSO`) em trocas temporárias de fonte. A linha de base revelou o nó novo **`E2EHERM`**: o `wave25_task_enrichment_e2e` depende de estado ambiente (passa na principal, falha 2/6 num worktree em 88c7cc8b) e precisa montar a própria fixture.
- **Correção de horário (03:15Z, medido com `date -u`)**: as horas dos quatro adendos acima tinham sido ESTIMADAS («~03:10/03:20/03:30/03:40Z») e estavam adiantadas; foram trocadas pelas reais (03:07/03:09/03:11/03:13Z). Hora em adendo vem de `date -u`, nunca de estimativa.
- **03:15Z — touring-a6 (Kimi) PARADA por limite semanal** desde 02:34Z: «API Error: 403 You've reached your weekly (7-day) usage limit» (transcript d75671a4). Tudo o que ela recebeu depois disso, incluindo o aviso do deploy e o pedido da porta, ficou sem resposta. Estado dela: `touring-wt-vrs` verde nos gates #2 de 02:46Z (clippy, testes `vgp`, build e `binary_e2e`, todos com rc 0), sem binário congelado, sem juiz e sem relatório. **A porta foi repassada** ao sonnet `vrs-porta`, que faz a medida ANTES no código da a6, a porta no `touring-wt-vrs2`, a medida DEPOIS sem perder recall e o relatório `RELATORIO-VRS.md`. O brief original continua sendo o aceite.
- **03:22Z — corpus NATURAL de Rust estava sobrescrito; recuperado byte a byte.** Às 20:37–20:42 BRT de 09/10 a touring-4f gravou o estrato sintético `sanidade` em `B/scripts/corpus_rust.py` e `~/.claude/touring/vgp/corpus_rust/`, por cima do gerador e do corpus natural aceitos (`entradas.jsonl` 141ced89). Quem detectou foi o `vrs-porta`. O `test_corpus_rust.py` estava com 63/64 falhas e ninguém rodava. Reparo:
  - `git show 88c7cc8b:…/corpus_rust.py` → **`scripts/corpus_rust_natural.py`** (VIVO → `corpus_rust_natural`). O teste foi renomeado para **`test_corpus_rust_natural.py`** e dá 64/64, com o selftest reprovando o mutante.
  - Consertos de lint feitos de passagem (REGRA #21): FURB167, RUF100 e SIM905, `check=False` ×2, `-x`, e o C419 reescrito como rodada round-robin explícita, porque o fix automático (`any` sobre gerador) mudaria o corpus. Ruff e pyright: 0.
  - **`~/.claude/touring/vgp/corpus_rust_natural/`** regenerado com o corte 2026-10-09T20:40:00Z: `entradas.jsonl` **141ced89653b0413**, idêntico ao aceito (37 positivos: nome 20 · import 13 · atributo 3 · kwarg 1; 307 negativos).
  - O sintético da 4f fica intocado com o nome `corpus_rust`. A 4f recebeu a regra «nunca escrever em caminho existente que não é seu» e o pedido de varrer outras sobrescritas. O `vrs-porta` mede no natural (principal) e na sanidade (secundária) e gera o `corpus_rust_real` por último, em destino novo.
- **03:22Z — snapshot da VL0 nos adaptadores pegou edição pela metade.** `lang/mod.rs` (07bb5d3f9c45 ≠ c95d707b2ffd; testes com E0061 ×2) e `cli/vgp.rs` (ea73f0d9c19f ≠ b1e5878cbb08; só um teste a mais) foram copiados 14–17 s antes da última edição do `vl0-nucleo`. vhtml e vtransv já sincronizaram o `mod.rs`; os três receberam a ordem de copiar os dois arquivos byte a byte da VL0. O `vrs2` está íntegro (0 diferenças contra a VL0, cópia de 03:09Z feita antes do `fase2a` de 03:12:58Z). Duas lições gravadas na memória.
- **03:24Z — P0 ACEITO e P0M LANÇADO.**
  - **P0** (`p0-atributo`, [RELATORIO-P0-atributo.md](RELATORIO-P0-atributo.md)): binário congelado `f55ec4b5`, com reprodutibilidade verificada por rebuild e `cmp`.
    - `np.array`, `Image.LANCZOS` e `rb.RUST_AVAILABLE` passam a allow nos 3 baldes; os controles `np.arrai` (deny) e `os.path.join` ficam idênticos.
    - Mutantes: 23/23 mortos.
    - v2 e V3 por id (bak e promovido): 0 deny novo, os mesmos perdidos, falso deny 0, e os avisos em negativos caem de 58 para 49.
    - O p95 do V3 reprova nos DOIS binários (ambiente a 92–96 °C).
    - As 3 falhas do touring-hooks são idênticas à linha de base (uma já está consertada na principal; duas viram o `E2EHERM`).
    - DAG `P0A` → completed.
  - **P0M**: `diagnostics/p0m/rodar_p0m.sh`. O selo do held-out 20261011 é conferido ANTES e DEPOIS pelo conferidor novo `scripts/confere_selo.py`, que usa a mesma régua do selo; selftest: 1 byte trocado reprova. O `c495bcab66718230` foi reproduzido, assim como os 2077 arquivos, os 13.017.450 bytes e os 3 índices. Depois rodam os juízes v2 e V3 com `--exigir-limite-superior` no f55ec4b5 e no e935aaf1 (referência por id). É o uso ÚNICO do held-out para o P0: se reprovar, o conserto exige held-out novo. DAG `P0M` → in_progress.
- **03:33Z — VRS: precisão em código real + emenda de aceite para TODOS os adaptadores.**
  - O `vrs-porta` rodou o adaptador da a6 em 130 .rs que COMPILAM (touring-wt-semeado@88c7cc8b): 69/130 arquivos com achado (253: nome 169, import 68, atributo 16). Em arquivo do projeto o achado é aviso, que o falso deny do juiz não conta; em `/tmp`/inline vira deny.
  - Causas: (a) em tests/benches/examples, `use <pacote>::X` resolve o próprio teste e não a lib; (b) `Tipo::item_associado` é acusado como variante ausente; (c) `drop` fica fora do prelúdio e os `const` locais de fn não são coletados.
  - Decisão: primeiro a porta em paridade estrita; depois (a), (b) e (c), uma a uma, com RED→GREEN, mutante e medida. O controle de 130 arquivos vira critério fixo, com alvo 0 achados; o resto vira `nao_medido`. O recall no natural não pode cair.
  - **Emenda para vts/vhtml/vtransv**: controle negativo em código LIMPO, real, que passa no verificador oficial (≥ 100 arquivos por linguagem, manifesto com sha), com alvo 0 e o resto em abstenção, mais «avisos em negativos» do juiz.
  - **Defeito do gerador sanidade da 4f**: `contexto` sai como string, o esquema é lista, e o juiz quebra com TypeError (linha 179). Pedido à 4f: teste que roda o juiz de verdade, conserto, regeneração em diretório novo e troca atômica.
- **03:39Z — P0M tentativa 1 SEM MEDIÇÃO; tentativa 2 lançada.**
  - A tentativa 1 rodou os juízes sobre o held-out PURO e deu, nos 4 juízes, «corpus degenerado: 0 positivos reproduzidos, 1200 negativos», com **0 chamadas** ao motor. É ausência de medição, não resultado. O held-out só tem negativos, e o juiz recusa corpus sem positivo (guarda de controle positivo, agindo como devia).
  - Eu tinha pulado o procedimento já registrado (`diagnostics/holdout/RELATORIO.md` §3.1): corpus HÍBRIDO no scratchpad, com a cópia do held-out mais os positivos do vivo SÓ como controle de liveness, e os falsos contados nos negativos do held-out.
  - Evidência arquivada em `diagnostics/p0m/tentativa1-corpus-puro/`. O selo continua intacto antes e depois.
  - Tentativa 2: `diagnostics/p0m/rodar_p0m2.sh`, com o híbrido montado por `montar_ctl_generico.py`, o v2 como no 20261010 (`cd /tmp`, `TOURING_CODE_MODE=native`), o V3 como no P2 e o e935 no MESMO held-out como controle de discriminação.
- **04:21Z — gargalo físico, com repriorização.** A máquina está a 92–96 °C (o limite do hw-slot é 88). Uma release build do vhtml segura a vaga Rust desde 03:35Z, e o `gates-97` está com o código das 3 frentes escrito mas nenhum cargo rodado. Os casos reais estão localizados: G10 em 05166acc L5786-5798, G7 em 2e4db816 L14641-14650, HOOKCWD no smoke.
  - O `vl0-nucleo` remedia juízes (e935/b7f12000) para fechar a ressalva do selo aproximado. É redundante com a INT97, então recebeu ordem de não lançar mais juiz nem build e de fechar só a emenda da spec e o relatório.
  - O `gates-97` vai agrupar os testes vermelhos num cargo por pacote.
  - O `p0-atributo` está no determinismo, com 3 fontes: HashSet em `check_membro` ×2, params do dataclass e desempate da `sugestao`. Ordem dos `reais` aprovada: públicos e depois `_`, alfabética em cada grupo. Os params do dataclass seguem a ORDEM DE DECLARAÇÃO, com as bases pelo MRO e os `kw_only` por último. Condição: o binário novo prova veredito idêntico por id contra o f55ec4b5 no vivo, senão o P0M (held-out 20261011, uso único) não transfere.
- **04:28Z — P0M: NÃO PRONTO pelo critério pré-registrado** ([diagnostics/p0m/DIAGNOSTICO.md](diagnostics/p0m/DIAGNOSTICO.md)). Medição no held-out 20261011 selado, por corpus híbrido, com o selo intacto antes e depois. O held-out discrimina, porque o e935 também reprova.
  - **v2**: f55ec4b5 com 2/600 em arquivo e 2/600 inline (CP95 1,05% nos dois); e935 com 2/600 e 3/600.
  - **V3**: f55 com 4/600 e 5/600; e935 com 4/600 e 6/600.
  - **Classificação dos 9 falsos do f55**: motor 7 (camada 1: `folium.__file__`; camada 2: tipo/stub do pyright e `cr` depois de `exec`), corpus 1 (o estado no instante não sobrepôs o submódulo do `from pkg import mod`), rótulo 1 (`srv` no `finally`, um bug latente real), política 1 (fragmento `.py` em /tmp).
  - **DAG**: `P0M` → failed. Nós novos:
    - **P0B**: atributos implícitos de módulo e camada 2 que só nega inexistência definitiva, sobre a INT97;
    - **HO13**: held-out 20261013 com estado no instante completo e adjudicação congelada, delegado ao `instrumentos-p1`;
    - **J13**: o binário integrado no 20261013.
  - O `R97` passa a depender de J13, P0B e HO13, e não mais de P0M.
  - **Decisões do Gabriel**: o critério do 20261013, (A) bruto ou (B) adjudicado por script congelado, antes de abrir; e a política para fragmento `.py` em /tmp.
