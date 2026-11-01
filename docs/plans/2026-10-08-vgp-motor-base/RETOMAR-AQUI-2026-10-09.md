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
- **04:29Z — INT97 LANÇADA SEM COMMIT** (sonnet `int97`, effort alto, brief [briefs/2026-10-10-int97.md](briefs/2026-10-10-int97.md)).
  - Worktree `~/projects/touring-wt-int97` (branch `int/2026-10-10-int97`), espelho exato da 30.4.96 implantada, com 0 diferenças por `cmp`.
  - Ordem: VL0 → L2 (com a ligação `checar_lang`; default só Python) → F21; o P0 entra só quando eu liberar, depois do determinismo, PORTADO para `lang/python*`.
  - Método: patch contra 88c7cc8b com índice temporário (`GIT_INDEX_FILE`, sem tocar nos worktrees de origem) e `git apply --3way`, preservando o delta da 30.4.96.
  - Integrar não exige commit; a autorização do Gabriel continua necessária só para levar o resultado às branches.
- **04:35Z — VL0: ressalva do selo FECHADA** (`vl0-nucleo`, terminou às 04:35Z, antes da repriorização). Conferido por mim:
  - o rebuild do release da fonte final deu o MESMO sha `b7f12000576170c2…`, então o selo vale para a fonte final;
  - a spec emendada (§3 «Emenda 10/10/2026» e §5 item 4) tem sha FINAL `5ce667479c16793a` e o relatório, `383471539ecd7d8c` (o 0e19df44/b4e2476f que conferi antes foi substituído pelo fechamento de texto dele às ~04:40Z; o §3 agora registra `Transversal -> Verificacao` como pendente do vtransv/INT97);
  - o delta mora no `check` (`politica::aplicar_delta`, mod.rs:183), com os testes `delta_vale_para_toda_lang_registrada…` (7 linguagens) e `disk_file_chega_ao_check_para_qualquer_lang` presentes;
  - os números, conforme o relatório dele: 762/762 por id no bak e 738/738 no vivo, mutantes 45/45, code 1146, server 1802 e hook-handlers 738, todos com 0 falhas.

  O p95 do V3, 1014 ms (e935 na mesma hora: 1012 ms), é ambiental. É a base que a INT97 integra.
- **04:58Z — HO13 construído, conferido e mandado RECONSTRUIR antes de abrir.** O `instrumentos-p1` entregou:
  - o 20261013 (sha_tree 8ce465ddeb03f500; 600 de arquivo e 553 inline, porque o `touring_run` parou em 153/200 pelo teto por sessão);
  - o estado no instante com os módulos importados (TDD RED 20 → GREEN 25, mutantes 11/11, a fixture do caso `_publicar` dá deny no manifesto antigo e allow no completo);
  - o adjudicador `scripts/adjudica_rotulo.py` congelado em `fabc36c9…` (selftest 84/84, mutantes 49/49).

  Conferido por mim: sha igual ao `congelados.json`, selftest e selo intacto. Dois problemas, e por isso a RECONSTRUÇÃO (NENHUM motor rodou nele):
  - (1) profundidade 2: 91/161 entradas importam módulos que importam outros fora do manifesto (717). É a mesma classe do falso de corpus do P0M, via reexporte;
  - (2) o `fe5620ecee0a` também está em `corpus_v0d.bak-20261009-pre` (dado de dev).

  A ordem foi: fecho transitivo, `--excluir-corpus` também do `.bak-20261009-pre` e do staging, destino novo `corpus_holdout_disjunto_20261013f`, o 20261013 atual marcado como SUPERADO e nunca aberto (a remoção dele é do Gabriel), o viés medido, e parar se algum balde ficar abaixo de 300.
- **05:18Z — INT97 parcial** (int97, sem commit):
  - **VL0**: aplicada com `--3way` e 0 conflitos, porque os 19 arquivos que ela toca estavam idênticos aos blobs de 88c7cc8b na árvore implantada. A sobreposição de 24 arquivos que medi antes contava os não rastreados da principal, sem comparar conteúdo. Está VERDE: code 1146, hook-handlers 738 e server --lib 1655, todos com 0 falhas.
  - **L2**: o touring-pyright foi aplicado e é idêntico ao wt-l2. A ligação `checar_lang` foi escrita: `[vgp] camada2_langs` com default só python, e o CLI anota «desligada». O check e os testes esperam o hw-slot (94–96 °C).
- **05:41Z — VRS: PORTA ACEITA, VRS ainda NÃO** ([RELATORIO-VRS.md](RELATORIO-VRS.md)). O adaptador da a6 virou `impl Adaptador for Rust` no vrs2, sem `TEMPORARIO-VRS`, com diff só em `lang/rust*`.
  - Binários conferidos por `sha256 -c`: ANTES `d5b434d7` e DEPOIS `42830beb`. Recall idêntico antes e depois; sanidade com 280/280 ids idênticos; natural com 182 idênticos e 162 diferentes, todos explicados (57 de allow para unknown com motivo, nenhum achado novo).
  - Testes 26/26 contra 0/26 no stub; mutantes 24/24; touring-code 1171 com 0 falhas.
  - **Aceite da VRS aberto**: o recall de `nome` é 5/20 = 0,25 (piso 0,70), igual antes e depois, logo herdado da a6. O total é 15/37 (import 7/13, atributo 2/3, kwarg 1/1). Ruído de warn em 214/307 negativos e em 63/130 .rs que compilam; a causa principal é `use super::*` não resolvido (204/344 ids).
  - Pedido ao vrs-porta, nesta ordem: 0) categorizar as 15 perdas de `nome`; 1) glob de módulo DO PROJETO; 2) a lib do pacote em tests/benches/examples; 3) o prelúdio completo e os itens locais. Aceite: nome ≥ 0,70, 0/307 mantido e controle de 130 com alvo 0. Se não chegar, ele para e traz a categorização.
  - **Lacuna**: nenhum Rust inline ou em /tmp nos corpora, então o balde de deny do Rust não está medido. O held-out de Rust é tarefa minha.
- **06:29Z — INT97: VL0+L2+F21 VERDES** (int97, sem commit, logs em `scratchpad/int97/logs/{vl0,l2,f21}.log`).
  - **Patches**: contra 88c7cc8b, com índice temporário e `--3way`, 0 conflitos. VL0 tem 21 arquivos e 5 renames (`90126f2d…`), L2 19 (`ba8f046b…`), F21 3 (`19628d6b…`). Cada arquivo confere por `cmp` com a origem.
  - **Ligação**: `sombra.rs` traz `camada2_langs`/`camada2_ligada` (a `[vgp] camada2_langs` declarada SUBSTITUI o default `["python"]`, e `[]` desliga). `vgp_l2.rs` e `cli/vgp.rs` passam por `pool::checar_lang`. No CLI, a camada 2 desligada sai como «desligada: …», nunca em silêncio.
  - **Testes, todos com 0 falhas**: pyright 175, code 1149, hook-handlers 741, server --lib 1656, lsp 13, analysis 1129, offensive 314, quality 493.
  - **Base por id**: o binário IMPLANTADO `aca22e42`, congelado.
  - **Decisão**: gates, binário e juízes UMA vez, depois do P0, para não pagar duas vezes a fila térmica. Até lá, o int97 só prepara o mapa da porta do P0 para `lang/python*`. O P0B vem depois, sobre o worktree do int97.
- **12:43Z — RETOMADA DEPOIS DA QUEDA DE REDE.** Às 08:09Z, um `EAI_AGAIN` (API inalcançável) derrubou ao mesmo tempo 7 agentes: vtransv, gates-97, vhtml, p0-atributo, vts, int97 e vrs-porta. O `instrumentos-p1` já estava ocioso desde 05:14Z, esperando um monitor. Foram 4,5 h paradas. Às 12:4xZ a máquina está fria (60 °C), com o hw-slot livre.
  - Os 8 receberam «RETOME», com a ordem de conferir o estado em disco antes de agir e com o ponto em que pararam.
  - **HO13f**: construído com rc 0 às 05:16Z (600 de arquivo, 553 inline, 0 incompletas, fecho transitivo, viés logado, `SUPERADO.md` do 20261013 escrito). Ainda FALTAM a sobreposição, o selo e o relatório.
  - A touring-4f recebeu o pedido de estado das 3 frentes (regeneração do html de `recurso`, tabela do held-out web com a proposta, gerador sanidade), com `notify_when_idle`.
  - **V3 a frio**: `diagnostics/v3-frio/rodar.sh` roda o e935 no vivo agora, com a máquina fria. É o controle da hipótese de que o p95 > 1000 ms era ambiental.
- **12:44Z — P0 determinismo.** O binário intermediário `70e0051a` (antes da regra do dataclass) mostra:
  - determinismo: 10/10 mutantes mortos; varredura com 0/738 e 0/765 ids divergentes, contra 7/738 do f55;
  - veredito idêntico ao f55 em 738 ids no vivo (v2 e V3) e no bak v2;
  - no bak V3, 1 id diverge (`158cbb7178bb`), por timeout de 1000 ms da camada 2: não determinismo por TEMPO.

  **Achado (pré-existente, e935 e 70e)**: falso deny de kwarg em classe sem `__init__` próprio importada do projeto (`Filha(Base)` com `Filha(a=1)`): `sig_de` não segue o MRO. **Decisão (A)**: entra no mesmo binário, pelo primeiro `__init__` no MRO C3, com abstenção em base irresolvida, `__new__`, metaclasse, `__init_subclass__` ou `__signature__`. Inclui o RED da sonda, controles, mutantes «só o `__init__` da classe» e «primeira base», e a condição por id: cada divergência tem de ser desta classe ou do dataclass.
- **12:45Z — 4f: três decisões.**
  - **(1) Troca do html de `recurso` AUTORIZADA**, com `mv -T --exchange` a partir do staging. O suporte do exchange está medido no mesmo diretório (promoção P2, 09/10). O sha_tree ANTES é `73a063a8…` (296 entradas), e o vhtml é avisado depois da troca.
  - **(2) REGRA DO JUIZ J (pré-registrada)**: uma classe só é «medida» com n ≥ 20 E denominador independente do gerador. Abaixo disso, o rótulo é «não provada, n=X», a classe fica FORA da média agregada e não há afirmação de recall. Origem: o held-out web tem 0 positivos nas 5 classes HTML; os 89 positivos são todos de símbolo tsx/ts. Os commits públicos que corrigem referência deram 0 nas 4 classes em 360 commits de 3 repositórios (os 10 de `classe_css` eram falso positivo do detector). Próximo passo: minerar transcripts no modelo do `corpus_rust_natural`, contando ANTES de construir.
  - **(3)** Sanidade Rust depois. A auditoria de sobrescritas da 4f achou 0 além do corpus_rust já recuperado.
- **12:45Z — HO13 ACEITO: `corpus_holdout_disjunto_20261013f` SELADO** (`2f9282aa59a0566f`, 2.525 arquivos, 17.755.226 bytes; conferido por mim com `confere_selo.py`; adjudicador `fabc36c9`). [Relatório](diagnostics/holdout-20261013f/RELATORIO.md).
  - Baldes: 600 de arquivo; 553 inline (c 200, heredoc 200, touring_run 153, tmp 0).
  - Fecho transitivo: TDD com RED 14/40 e GREEN 42/42; mutantes 23/23, incluindo «para na profundidade 1»; 0 entradas incompletas.
  - Sobreposição 0 nas 4 chaves contra o vivo, o temporal, o 20261010, o 20261011, o bak-20261009-pre, o staging e o bak-2026-10-09, todos com controle positivo. O `fe5620ecee0a` ficou fora.
  - **LIMITE DECLARADO do J13**: os aceitos têm fecho médio de 1,19, contra 7,03 nos candidatos, e só 15,7% têm fecho não vazio, contra 28,7% no vivo. A taxa medida vale para arquivos com grafo de import pequeno.
  - O 20261013 fica SUPERADO, nunca aberto; a remoção é do Gabriel. DAG `HO13` → completed.
- **12:47Z — INT97 e o «V3 a frio».**
  - A int97 já tinha rodado os gates completos antes da queda: fmt 0; clippy --workspace 0; testes com 0 falhas em code 1149, server 1803, pyright 175, hook-handlers 741, cli 668 e lsp 13. O touring-hooks tem 1171 ok e 2 falhas, que são exatamente as do `E2EHERM`. Um erro de rustdoc dela foi corrigido.
  - Binário PRÉ-P0 congelado `6715d1b8` (VL0+L2+F21). O `aca22e42` foi conferido como o implantado (symlink, release e cópia com o mesmo sha); os juízes dele por id batem com o k_*_ev / k_*_vivo do e935.
  - Mapa da porta do P0 e `portar_p0.py`: 2 conflitos no ensaio, os dois por renomeação da VL0, resolvidos mantendo as duas mudanças.
  - Aprovei os juízes por id do 6715d1b8 contra o aca22e42, para ISOLAR VL0+L2+F21 do P0.
  - **O «V3 a frio» foi CONTAMINADO**: começou a 63 °C e chegou a 97 °C em 3 min, quando os 8 agentes retomados pegaram as vagas. O p95 dele não vale como medição a frio. A medição a frio de verdade fica para uma janela SEM builds, com o binário integrado final.
- **12:47Z — HTML com sinal natural MEDIDO; nó `HOHTML`.** A troca do corpus semeado html está em curso com `--exchange` (ANTES 73a063a8…).
  - **Contagem nos transcripts** (1.814 transcripts, 87 edições `.html`; é um TETO, antes do oráculo): recurso 30, ancora 27, cdn_pin 45, classe_css 822, cdn 0. No git público o mesmo sinal deu 0 em 360 commits: na sessão, a referência quebrada nasce porque a ferramenta reclamou.
  - **Aprovado**: oráculo por instância (por DELTA, no estado do disco), `--corte 2026-10-10T00:00Z`, exclusão das sessões do plano VGP e de `~/.claude/touring/vgp/`, Tailwind/CSS-in-JS excluídos e não positivos, destino novo `corpus_holdout_html_natural_20261010` selado. A regra C vale.
  - Gerador sanidade Rust: o teste RED que roda o juiz de verdade está escrito; o conserto vem depois.
- **12:49Z — VRS: das 15 perdas de `nome`, 14 são do CORPUS; nó `HORUST`.** Categorização do vrs-porta:
  - **A (6)**: o snapshot DEFINE o nome no módulo do uso. O rótulo não é reproduzível pelo conteúdo; provavelmente a edição foi via Bash, que a dobra não vê.
  - **B (5)**: `crate::m::f` aponta para um módulo que HOJE define o nome; o overlay está vazio.
  - **C (3)**: `use super::*` com o pai fora dos overlays.
  - **D (1)**: lacuna REAL, o escopo de módulo inline.

  O teto honesto do recall de `nome` nesse corpus é ~6/20. É a mesma classe de defeito do P0M: o estado no instante está incompleto, e para Rust ele está vazio.
  - Tabela ANTES→V4 do vrs-porta: recall igual em todas as variantes; falso deny 0/307 em todas; avisos em negativos 222→125; controle de 130 com 69/253 → 16/41.
  - **Decisões**: a D foi autorizada. O NOVO ACEITE da VRS é controle de 130 com 0 achados ou cada um em abstenção, 0/307 mantido, nenhuma correção derrubando recall, e o recall de `nome` reportado como teto honesto. O piso de 0,70 fica para eu julgar no corpus refeito e no held-out de Rust (regra C).
  - **HORUST** (instrumentos-p1): estado do instante do CRATE inteiro, a classe A excluída como não reproduzível, MEDIR antes de construir, e a divisão dev/held-out disjunta.
  - As duas decisões da ligação da camada 2 propostas pela int97 foram validadas: `[]` desliga; uma lang desligada vira «desligada», nunca silêncio.
- **12:49Z — gerador sanidade Rust: dois defeitos em cadeia.** (1) `contexto` como string fazia o juiz quebrar com TypeError. (2) Com (1) consertado, o juiz deixou de quebrar e mediu ZERO, porque a `mensagem` vinha como `delta=E0425 em…` e não no formato `error[E0425]: …` que o `SIMBOLO_POR_LANG["rust"]` exige. O conserto foi repassar a linha REAL do rustc. Com os dois, o teste `test_corpus_rust_contrato_juiz.py` passa, com 104 chamadas e positivos de símbolo existindo. A regeneração vai para `corpus_rust_sanidade_v2`, SEM trocar o vivo enquanto o vrs-porta mede por id. **HTML natural**: regra do oráculo para caminhos de /tmp: a existência no instante vem da dobra do transcript, de `git show` no instante, ou do disco de hoje só com prova de que nada mudou desde o instante; o que não dá para saber vira INCOMPLETA. A sobrevivência é medida antes de construir.
- **12:51Z — HTML natural nos transcripts: UNIVERSO INSUFICIENTE, medido antes de construir** (DAG `HOHTML` → failed, com motivo). Corte 2026-10-10T00:00Z: 1.818 transcripts, 92 excluídos por serem do plano VGP. O oráculo julga ARQUIVO, e não referência. As 30 referências de `recurso` estão em 5 arquivos, dos quais só 3 têm base reconstruível no instante. Teto com base de instância: recurso 3, ancora 1, cdn 0, cdn_pin 4, classe_css 3. Nenhuma classe chega a 20.
  - Próximo: MEDIR, só contar, o universo dos repositórios com histórico do `oss/`. Se nada chegar a 20, o **J declara as 5 classes HTML como «não provada, n=X» no RECALL** e mede só a PRECISÃO (falso deny) no held-out web. Isso é limite a levar ao Gabriel.
  - Sanidade v2: o natural grava `error[EXXXX]: …` e o v2 gravava só o texto depois do cabeçalho; daí o kwarg não classificado, que é do GERADOR. Além disso, ele saiu com 4 positivos e não 80. Pedido: regenerar com os parâmetros originais e o cabeçalho, com prova no juiz de `nao_classificado` 0 e 20 por classe.
- **12:52Z — HTML: universo do `oss/` medido.** 7 repositórios com `.git`: 27.788 commits, 1.792 tocando `.html`, 222 `.html` no HEAD e 163 com referência local (149 no dom-examples). Ordem de grandeza acima dos transcripts.
  - Regra do oráculo adaptada ao git: instante = commit C, árvore = `git ls-tree C`. Positivo = C INTRODUZ a referência quebrada E um D posterior a CORRIGE. O par é o que separa erro real de artefato de build; diretórios de build e gitignorados ficam excluídos. `cdn_pin` é classe de forma e é reportada à parte.
  - A 4f só CONTA os pares C→D por classe antes de qualquer construção. O `HOHTML` fica reaberto em função dessa contagem.
- **12:53Z — sanidade Rust v2 ACEITO** (`~/.claude/touring/vgp/corpus_rust_sanidade_v2`, `entradas.jsonl` `1f583417…`, conferido por mim). 80 positivos (20 por classe), 100 negativos (o original tinha 200: fica registrado), `contexto` em lista nas 180 entradas, 0 positivos sem `error[E…]`. Prova da 4f no juiz de verdade com o f55ec4b5: 180 chamadas, `nao_classificado` 0, positivos de símbolo 80; o recall é 0/80, esperado porque o 30.4.95 responde stub em Rust. O v2 com n=1 ficou em `.prior-n1-20261010`. O vivo `corpus_rust/` fica intocado até a entrega da VRS; o vrs-porta mede o final no v2. O oráculo por instância no git foi escrito (`scripts/oraculo_instancia_git.py`), e a contagem dos pares roda em background.
- **12:55Z — V3 «a frio» CONTAMINADO** ([diagnostics/v3-frio/LEIA.md](diagnostics/v3-frio/LEIA.md)). Foi de 63 para 95 °C, com o load de 2,6 para 13,3; p95 de 1.120,7 ms no e935. O p95 sobe com a carga (era ~1.012 ms com load ~6), o que é compatível com a hipótese ambiental, mas não a prova. Medição a frio de verdade: janela EXCLUSIVA no J, com o binário integrado.
- **13:11Z — corpus semeado html REGENERADO e trocado** (4f, `mv -T --exchange`; conferido por mim). `entradas.jsonl` `bf643ba6da16bee2` (sha_tree `4178277f…`, antes `73a063a8…`): 300 entradas, 20 positivos por classe nas 5 classes, 200 negativos. `recurso` traz o caminho como escrito em 20/20, e 0 só com o marcador. O `cdn_pin` foi de 16 para 20. O determinismo deu IDÊNTICO nas duas gerações (`bf643ba6`) e o selftest, 38/38. O antigo está intacto em `staging-html-20261010/html` (`5c068940…`); a remoção é do Gabriel. vhtml e vtransv foram avisados.
  - **Oráculo git (HTML natural)**: a 1ª contagem deu recurso 85, e os 3 exemplos conferidos à mão eram FALSOS (URL absoluta tratada como caminho local; alvo criado no mesmo commit). O par foi corrigido: D ≠ C, mesmo arquivo, alvo ausente na árvore de C, URLs absolutas e `//`/`#`/`data:`/`mailto:` excluídos. Ajuste meu: D também pode ser o commit que ADICIONA o alvo no caminho exato. Recontagem em curso. Até aqui ancora, cdn e classe_css dão 0; `cdn_pin` 589 é classe de forma e fica fora da média; há 833 negativos para medir a PRECISÃO por «avisos em negativos».
- **13:18Z — INT97: VL0+L2+F21 NEUTROS por id** contra o implantado `aca22e42` (int97: 738/762 ids, v2 e V3, vivo e bak, 0 divergências em veredito, classe, nome, linha e severidade).
  - **Recomputado por mim** sobre os `por_id` com os `findings` inteiros: em cada combinação, 7 ids diferem SÓ em `reais` (e em 1 `sugestao`), com o veredito igual. É o não-determinismo conhecido do motor antigo (o `p0-atributo` mediu 7/738 no f55), não regressão. O conserto de determinismo do P0 deve zerá-lo, e a INT97 pós-P0 tem de mostrar 0 nos `findings` inteiros.
  - O p95 das medições está sob carga (94–98 °C).
- **13:18Z — HTML natural via git: FECHADO.** A regra «abrir 3» derrubou o `recurso` de 85 para 5, depois 1, depois 0. Os defeitos eram do par (D = C; URL absoluta; `/favicon.ico` do site lido como `src/favicon.ico`; referência comparada com ela mesma). Resultado: `recurso`, `ancora`, `cdn` e `classe_css` com n=0 («não provada, n=0», regra C). **`cdn_pin` com 329 positivos naturais é MEDÍVEL** (denominador do git, oráculo de forma determinístico); corrijo a minha decisão anterior de deixá-lo fora. 166 negativos confirmados servem à PRECISÃO. Pedido: montar e selar `corpus_holdout_html_git_20261010` (166 negativos + 329 de `cdn_pin`), com DISJUNÇÃO contra o `corpus_semeado/html`.
- **13:19Z — régua R2 da int97** (`scratchpad/int97/comparar_porid2.py`: findings INTEIROS mais os campos do registro, com a lista dos campos que mudaram). Pré-P0 contra o implantado: R1 = 0 e R2 = 7 ids (só `reais`/`sugestao`), igual à minha recomputação. **Classe nova no V3**: o `camada2` difere em 23 ids no vivo e 60 no bak, por «unknown: prazo de 1000 ms estourado esperando publishDiagnostics» nos DOIS sentidos, sempre com R1 = 0. É não-determinismo por TEMPO da camada 2 sob carga (97 °C). No pós-P0, o «0 entre duas execuções» vale para o v2 inteiro; o `camada2` do V3 é reportado à parte, e testo no J a hipótese de que ele cai em janela exclusiva.
- **13:24Z — `confere_selo.py` endurecido (defeito MEU).** O selo do HTML git trazia só o sha do `entradas.jsonl` e morava DENTRO do corpus. O conferidor dizia «INTACTO» porque só conferia os campos presentes, e o `snap/` não estava selado. Agora `sha_tree`/`arquivos`/`bytes` são OBRIGATÓRIOS e selo dentro do corpus reprova. Selftest com 4 casos (intacto, 1 byte, sem `sha_tree`, selo dentro); ruff e pyright 0. Reconferidos: 20261011 e 20261013f INTACTOS; HTML git VIOLADO (rc 1).
  - **HTML git (4f)**: 244 negativos (7 repos), com disjunção contra o `corpus_semeado/html` medida em 0 (os 296 do semeado vêm todos de `~/projects/analise`; 0 colisão de sha contra os 5.032 blobs). O `cdn_pin` de 329 era TODO URL absoluta não-CDN (istanbul.js.org, Wikipedia, DOI, NASA): quarto defeito pego pela regra «abrir 3». O critério novo (`@` com versão não exata) ficou estreito demais; pedi recontagem por HOST de CDN, incluindo sem versão e versão no caminho, mais o selo refeito FORA do corpus.
- **13:29Z — HOHTML ACEITO, como held-out de PRECISÃO.** `~/.claude/touring/vgp/corpus_holdout_html_git_20261010`: selo em `diagnostics/holdout-html-git-20261010/selo.json` (FORA do corpus; sha_tree `b6966be51d13e5ee`, 249 arquivos, `confere_selo` rc 0); 236 negativos de 7 repositórios mais 12 `cdn_pin`. O `cdn_pin` agora é contado por HOST de CDN (`scripts/cdn_pin.py`, contrato de 21 casos em `test_cdn_pin.py`, 21/21 como script; pedido o wrapper de pytest, porque hoje dá «no tests ran»). Os 3 primeiros conferidos à mão são Google Fonts sem versão, `cdn_pin` legítimo.
  - **Recall HTML no J**: `recurso`, `ancora`, `cdn` e `classe_css` com n=0; `cdn_pin` com n=12. Todas «não provada» pela regra C. **Precisão HTML no J**: avisos e denies nos 236 negativos, medidos com o binário integrado (`scripts/mede_avisos_por_classe.py`, que conta o erro à parte do silêncio). Esse é o LIMITE a levar ao Gabriel.
  - A regra «abrir 3» derrubou quatro tetos falsos na onda: classe_css 10, recurso 85, recurso 5→1→0 e cdn_pin 329.
- **13:30Z — testes do bundle que o pytest NÃO coleta (padrão, não arquivo).** Conferido por mim com `ast`: `test_chama_oraculo.py`, `test_corpus_rust_contrato_juiz.py` e `test_vgp_lock.py` têm 0 funções `test_`, só `main()`. Um CI com pytest testaria quase nada, e o exit 5 («no tests ran») vira verde. O `test_cdn_pin.py` já foi consertado e provado com mutante (pytest reprova com o critério quebrado). A 4f foi AUTORIZADA a consertar os três com prova de mutante e a escrever a GUARDA ESTRUTURAL `test_todo_teste_e_coletavel.py`, que reprova todo `test_*.py` sem função coletável.
- **13:37Z — suíte de scripts do bundle RODADA por mim: 227 passed, 0 falhas** (rc 0, 42 s, pelo hw-slot; log e junit em `scratchpad/suite_bundle/`). A coleta (227) bate com o executado. A guarda estrutural `test_todo_teste_e_coletavel.py` está no bundle, os 3 testes antes não coletados agora coletam, e cada novo assert foi provado com mutante pela 4f. **Onda da 4f FECHADA.**
- **13:55Z — VTRANSV (cdn, cdn_pin, classe_css) ACEITA na fase de desenvolvimento** ([RELATORIO-VTRANSV.md](RELATORIO-VTRANSV.md); binário `49830a73`).
  - Recall 20/20/20 no semeado novo (`bf643ba6`); falso deny 0/200 (CP95 de 1,49%: n=200 não prova o teto); p95 29,7 ms; mutantes 54/54; touring-code 1302 com 0 falhas.
  - Emendas: E1 `Transversal::verificar -> Verificacao`, com `medir` juntando o `nao_medido` do adaptador e dos transversais e 3 mutantes do núcleo mortos. Mexe em `vgp/mod.rs` e `lang/mod.rs`, núcleo autorizado pela spec. E2 `sha2`/`base64` do workspace. E3 `cli/vgp.rs` sincronizado.
  - **Controle limpo**: 0 achados em 491 HTML e 15 TSX; 415 Tailwind em abstenção. No CDN, 3 deny são verdadeiros positivos (`integrity` não bate com o arquivo servido) e 52 `cdn_pin` foram conferidos um a um.
  - **Ruído aberto**: 49/200 negativos do semeado com aviso de `classe_css` (classes do SEI sem folha carregada). Pedi a categorização por causa e uma regra de abstenção MEDIDA numa cópia; aplicar é decisão minha.
  - O núcleo tocado pelo E1 entra no passo de integração dos adaptadores, depois da INT97 com o P0.
- **13:59Z — DAG: o «dedupe do settings.json (17x)» é premissa FALSA**, e o `failed` é o desfecho certo. As 17 entradas de `pre-read`, `pre-edit`, `pre-edit-prevention` e `pre-write` são UMA regra `if` por extensão (`Read(*.rs)`, `Read(*.py)`… 17 extensões), que é o desenho exigido pelo gotcha 13(a): o `if` aceita uma regra só e falha aberto. Medido agora: 0 duplicatas por (matcher, comando, if). Nada a fazer.
- **13:59Z — E2EHERM delegado** (brief `briefs/2026-10-10-e2eherm.md`; worktree `touring-wt-e2eherm` sobre c118aa52; sonnet): o teste semeia o próprio estado no daemon privado; controle negativo; mutante no extrator de `prompt`; prova de que não houve SKIP; os bancos reais com contagem idêntica antes e depois.
- **14:00Z — VRS, decisão A** (o vrs-porta pediu, porque «controle 0» colidia com «recall não cai»).
  - O `import` caiu de 7/13 para 3/13 nos 4 positivos `use super::super::gates_limpo;` de `cli_suggester_tests.rs`. Conferi: L2128 está dentro de `mod code_mode_gates_w1` (L2124), aninhado no `mod tests` por `#[path]` (cli_suggester.rs:6590), então `super::super` = `cli_suggester`. O `gates_limpo` nasceu no dfe0fd1e: o E0432 era o vermelho de TDD do instante, e o acerto do V4 vinha do posicionamento pelo layout, motivo errado.
  - Aceite: duas contagens (bruta 3/13 e os 4 como «rótulo do instante irreproduzível sem o estado do crate»); teste do motivo certo com mutante; a política (p), nome dinâmico → `nao_medido`, com a contagem de abstenções.
  - Os 4 ids viram controle positivo do HORUST.
  - Medidas do V12: controle 130 de 69/253 para 3/3; avisos em negativos de 222 para 105/307; falso deny 0/307.
- **14:05Z — HORUST aceito** ([diagnostics/horust-2026-10-10/RELATORIO.md](diagnostics/horust-2026-10-10/RELATORIO.md)).
  - **Held-out natural de Rust = 0 positivos**, em qualquer janela, fora das sessões VGP. A prova final declara o recall Rust natural «não provada, n=0».
  - DEV = 26 positivos (19 fora do VGP; 6 sessões, 10 arquivos).
  - O overlay de crate recupera 0/16 perdas: as causas são do motor (`use super::*` 5, crate irmão 5, dobra divergente 3, avulsos 3), repassadas ao vrs-porta para DEPOIS da rodada atual.
  - **Aprovado o HORUST-REAL**: held-out por erro INJETADO em .rs reais (worktree destacado em c118aa52), com o rustc como oráculo; n ≥ 20 por classe e ≥ 320 negativos; disjunto do dev, do controle 130 e das fixtures; pré-registro antes de gerar, selo fora do corpus; julgado uma vez, por mim, com o binário integrado final. Limite declarado: erro injetado, não a distribuição natural.
- **14:06Z — Incidente E2EHERM, remediado.**
  - O que houve: um experimento exploratório do e2eherm (cliente com cwd = árvore principal) gravou `e2eherm-seed-wiring-orphans` no `memory.db` REAL do projeto, às 14:03:23Z. É a lição `daemon-resolve-projeto-pelo-cwd-do-cliente` de novo.
  - Remediação: removi numa transação, com cópia em `scratchpad/memoria_e2eherm_removida.json`: memory_entries 1, memory_tags 3, embeddings 1, memory_coserved 18. O FTS foi limpo pelos triggers, e o `integrity-check` de `memories_fts` e `tags_fts` deu ok.
  - Causa do E2EHERM, confirmada: o `scout()` herda o cwd do cargo test e o daemon resolve o projeto por ele. O `start_in(…, root)` não pina o cliente.
- **14:08Z — Ruído de `classe_css` medido pelo vtransv** (`binarios/49830a73/evidencias/ruido/`, coleta 2f967f241ea381b0).
  - Causas dos 128 avisos: classe do editor SEI 78 (61%); marcadores Legis do SEI 42 (33%); relatório/slide gerado 8; typo real 0.
  - **R2** (documento com `contenteditable` → `nao_medido`): avisos de 128 para 6, recall 20/20, controle A 0.
  - A R1 (cobertura) não separa os grupos: perde de 4 a 12 positivos.
  - Viés apontado por ele: os positivos do semeado são quase-acertos (sufixo `q`), então o recall de `classe_css` NÃO prova digitação real (limite declarado).
  - Fora da amostra (5.165 HTML): R2 silencia 1.332 documentos. Os 46.514 avisos restantes são `htmlcov` (R3/R5 abertos e não decididos).
  - **Decisão: aplicar R2**, medindo ao lado a R2s (por subárvore) e implementando a R2s se ela empatar; TDD + mutantes + sha8 novo + o juiz e os controles de novo.
- **14:18Z — CORREÇÃO da entrada «VRS, decisão A»** (a premissa estava errada; quem mediu foi o HORUST, RELATORIO §4b).
  - O `gates_limpo` NÃO nasceu no dfe0fd1e quanto ao instante: o working tree já o tinha sob `#[cfg(test)]` desde 08/10 22:58:31Z (Edit UK4mc0vc). O `mod tests;` estava sem `#[cfg(test)]`, e o E0432 vem do alvo não-teste do `--all-targets`. Às 23:20:47Z o `#[cfg(test)]` entrou no `mod tests` e o erro sumiu.
  - **O rótulo é REAL e reproduzível**, mas só avaliando `cfg` por alvo.
  - A decisão A segue (posição do `#[path]` correta), e os 4 contam como PERDA do motor: `import` 3/13, SEM recall ajustado.
  - Candidata medida para a próxima rodada da VRS: `cfg` por alvo (6/37 sob `#[cfg]`), `use super::*`, crates irmãos.
  - HORUST: o estado do crate recupera 0 (V12/V13 0/4 nos gates_limpo; restaurados 0/26); recall `nome` do V12 no DEV = 6/26.
  - Causa do meu erro: datei pelo `git log -S` (data do COMMIT) e não pelo transcript (o instante).
- **14:23Z — gates-97 ACEITO pelo juiz independente da coordenadora** (`scratchpad/juiz_gates97.sh`; a 1ª tentativa saiu com rc 75 sem vaga e «exit 0» do script, e foi lida pelo `rcs.txt`).
  - Testes, todos rc 0: cli gates97/g10/hookcwd 24/24 · ceg sandbox_can_write/run_admission 8/8 · server waiver 2/2 · g10_trusted_route_e2e 2/2 · hooks e2e (hook_root_payload_cwd + cli_suggester) 22/22.
  - Binários do e2e (debug touring/touring-daemon/touring-hook) construídos pela própria rodada às 14:22Z, nenhuma fonte mais nova, 0 marcas de skip.
  - G10ROTA, G7G3G9 e HOOKCWD = completed. Mandado a seguir o conserto da ordem G5/G9 (deny antes de advisory).
- **14:23Z — Corpus semeado html: sha_tree canônico `1b83ba7392dd8778`** (método do `confere_selo.py`, 602 arquivos; `entradas.jsonl` bf643ba6). O `4178277f…` da 4f é OUTRO método (64 hex). Nenhum arquivo foi escrito depois da troca de 13:09Z (find -newermt = 0): o vtransv e o vhtml mediram a mesma árvore.
- **14:25Z — vhtml ACEITO na fase de desenvolvimento** (binário `ec542c4b`; sha conferido à mão, porque o `.sha256` não está no formato do `sha256sum -c`).
  - Medidas: recurso 20/20, ancora 19/20 (fragmento = abstenção), falso deny 0/200, mutantes 79/79, controle de 4.969 com 0 achados de recurso e de id ausente, duplicados iguais ao oráculo `html.parser`.
  - Decisões:
    - `deny` de recurso/ancora em `/tmp`/`.cache`/inline → `warn`, pela spec §5.1: a próxima escrita conserta (ordem de escrita);
    - id duplicado fica `warn` e é reportado por classe;
    - CLI não-UTF-8 → `lang: python` é DEFEITO: patch separado contra o `cli/vgp.rs` da VL0, aplicado na integração.
- **14:25Z — touring-a6 (Kimi K3) recebeu o A6-HTML**: 2º leitor adversarial do adaptador HTML contra a WHATWG e a URL spec, com fixtures e o binário congelado, só leitura (brief `briefs/2026-10-10-a6-revisao-html-whatwg.md`). A touring-4f (MiniMax) está em shell, e confiro o que ela faz antes de dar mais.
- **14:43Z — gates-97: ordem G5/G9 ACEITA.**
  - Conserto em `cli_suggester.rs`: `code_mode_gates_chain` com avisos depositados e `merge_gate_verdict` pelo mais severo.
  - Prova: red 2 falhas (G5×G9 e S3×G6) → green; 7/7 mutantes; touring-cli 686, hook-handlers 733, server e hooks rc 0; clippy workspace rc 0.
  - Precedências mantidas de propósito: S3×G1 (ordem do Gabriel 09/10 D; ORD-07) e G6×G1.
  - **Mesmo curto-circuito LATENTE no caminho VGP** (`pre_bash.rs:69` antes do bash_ast_validator, do PreToolValidator e do W-540; `pre_edit_prevention.rs:151` antes do S210; `pre_write.rs:133`): vira defeito vivo na V4. Conserto AUTORIZADO, com uma regra só, TDD com costura + 1 e2e em modo ativo, e mutantes por sítio.
- **14:52Z — vtransv: R2 por documento ACEITA** (binário `de359e81`).
  - A R2s por subárvore ficou em 128 → 36: os spans `dark-mode` ficam fora da ilha editável.
  - Medidas da R2: avisos em negativos 128 → 6; recall 20/20/20; falso deny 0/200; mutantes 60/60; controles A–D com o mesmo sha; população 4.186 → 2.855 documentos com aviso, e 1.331 viram `nao_medido`, contados.
  - Pendente: a prova de reprodutibilidade do de359e81.
- **14:52Z — Defeito de PROTOCOLO meu, corrigido.** Os briefs diziam «restauração por conteúdo + `os.utime`» sem o tempo; o vtransv usou o mtime original e o cargo reusou o artefato do mutante (falhas fantasmas).
  - Corrigido em `briefs/2026-10-10-fanout-adaptadores.md` e `briefs/2026-10-10-gates-97.md`: «mtime = AGORA».
  - Comunicado a vtransv, vhtml, vrs-porta e vts. Item de aceite novo para todos: binário congelado reproduzível da fonte final (`touch` → `build --release` → `cmp`).
- **14:57Z — HORUST-REAL em andamento.**
  - Pré-registro gravado às 14:23:50Z (sha 45fd89a1…, em `diagnostics/horust-real/pre-registro.hora`), ANTES de qualquer entrada.
  - Worktree `touring-wt-horust-real` destacado em c118aa52.
  - Instrumento testado: 157 testes, 83/83 mutantes.
  - Esperando a vaga rust para o `cargo check` de base. A fila da vaga rust está longa (p0, vtransv, e2eherm, gates-97, vhtml, vts, vrs-porta, horust-real), e a ordem de chegada já põe o P0, que é o caminho crítico, na frente.
- **15:11Z — gates-97 adendo 2 ACEITO: ordem do VGP nos 3 handlers.**
  - Regra única em `touring-hook-runtime/src/verdict.rs`, com `match` exaustivo; um teste compara o fio dos dois adaptadores.
  - Prova: red real no e2e com `modo = "ativo"` (um comando destrutivo encadeado passava sob o aviso do VGP); 15/15 mutantes; hook-runtime 499, cli 686, hook-handlers 744, hooks e server rc 0.
  - Achado extra: no `pre_write` o aviso também pulava o `triad_hook::arm` (write sem snapshot TRIAD).
  - Integração: entra na INT97 como **componente 5**, com base = árvore implantada da 30.4.96 (brief atualizado). A verificação independente fica para os gates completos da INT97.
- **15:28Z — P0 FECHADO e LIBERADO para a INT97** (binário final `4501e1e5`; fonte checks.rs 7fb2c562 · modules.rs e121c6e0 · checks/tests.rs c61ff1e3).
  - Veredito por id idêntico ao f55: corpus_v0d v2 e V3 738/738; bak v2 762/762; bak V3 760/762, e os 2 são timeout do pyright no run do f55, re-medidos 5/5.
  - Contra a e935: 0 deny novo. Determinismo 0/738 e 0/765. Dataclass pela `dataclasses.rst` (C3 real). 26/26 mutantes.
  - Pendentes do p0: `dg2.rc` dos gates finais e o p95 pareado. Se a fonte mudar, a INT97 refaz só o P0.
  - **Falha de entrega minha**: a decisão (A) do MRO para classes comuns (L758 deste arquivo) não chegou ao p0-atributo, que fez B (só dataclass). O falso DENY de kwarg em `Filha(Base)` sem `__init__` importada (pré-existente, e935/70e) vai para o escopo do **P0B**, antes de qualquer release. Lição: decisão registrada não é decisão entregue, e a próxima mensagem do agente tem de ecoá-la.
  - INT97 instruída: P0 (com conferência dos shas da fonte) + **componente 5, gates-97** (base = árvore implantada). O patch do CLI não-UTF-8 e o E2EHERM entram depois.
- **15:31Z — A INT97 parou certo na conferência de sha.**
  - O `checks.rs` do vgp-attr mudou às 15:28:37Z, depois do fechamento do p0: 7fb2c562 → ed8a5319. Conferi o diff: só o comentário de doc das L951–954 (10 linhas `///`), sem código.
  - A INT97 porta o ed8a5319. A fonte do vgp-attr está CONGELADA.
  - O p0 confirma que o `dg2` e o rebuild + `cmp` com 4501e1e5 cobriram o ed8a5319.
- **15:32Z — vtransv FECHADO.**
  - `de359e81` reproduzível: `touch` nos .rs tocados, `build --release` e `cmp` IGUAL; conferi o sha e o cmp eu mesma.
  - VCDN e VCSS = completed (desenvolvimento). O recall no held-out segue «não provada» pela regra C, e a precisão será medida no juiz J.
  - O núcleo tocado pelo E1 (`Transversal -> Verificacao`) entra na integração dos adaptadores.
- **16:12Z — VRS, status e resumo consolidado** (o vrs-porta dizia «A/B do `#[path]` pendente»: as minhas mensagens provavelmente estão na fila de um agente preso na espera da vaga).
  - Mandei um resumo de 10 itens que substitui os anteriores, com eco obrigatório.
  - Itens novos dele: (w) conserta uma regressão própria (`use rstest::rstest;` virava reexport de si mesmo: controle 130 com 1 achado no 46460c35); (x) só afirma ausência em módulo ENUMERÁVEL (macro de item ou reexport não fechado → abstém); risco herdado da a6, provado em 3 binários. **A (x) fica mesmo que o recall caia.**
  - p95 sob carga (A/B intercalado): a6 152 / final 164 ms, max 446 (`hook_registry.rs`, pela (q)). É RISCO, a decidir na janela exclusiva do juiz final.
- **16:42Z — P0 FECHADO pelo p0-atributo.**
  - Binário: rebuild do `ed8a5319` = 4501e1e5 (`cmp` rc 0).
  - Gates: fmt/clippy 0; touring-code 1.166, server 1.799, hook-handlers 733, todos com 0 falhas. touring-hooks rc 101 com as MESMAS 3 falhas da base 88c7cc8b: o classifier (consertado na 30.4.96) e as 2 wave25 (E2EHERM).
  - p95 pareado: camada 1 113,0 → 111,7 ms; V3 957 → 920 ms (máquina a 95 °C; o V3 isolado passa de 1 s em todos os binários).
  - §6 validado: públicos primeiro; só ANOTADOS são campos de dataclass (é a semântica de runtime); abstenção ampliada; `__init__` explícito vale.
  - DAG: **P0BMRO** criado (high, depende de INT97; dono p0-atributo), e o J13 passa a depender de INT97, P0B e P0BMRO.
- **16:43Z — Correção do registro do P0.**
  - A mensagem da decisão (A) NÃO se perdeu: chegou atrasada, junto com a seguinte. O p0 seguiu a última (binário 4501e1e5 como está; MRO para classe comum no P0BMRO). Foi atraso de caixa de agente ocupado, não falha de envio.
  - Proveniência confirmada: o `dg2` começou às 15:50:27Z sobre o `ed8a5319` (sha gravado no topo do `dg2.rc`); o rebuild + `cmp` foi o último passo dele, com rc 0.
  - O p0 espera o aval para o P0BMRO depois da INT97.
- **16:48Z — INT97: componente 5 (gates-97) APLICADO e VERDE** (sem commit).
  - Patch `g97.patch` (f517f5071812f3fa, 17 arquivos: 10 modificados e 7 novos) contra a árvore implantada, provado em cópia pristina byte a byte.
  - Merge de 3 vias: 8 arquivos idênticos ao gates97; `pre_write` com merge limpo com a VL0; `pre_edit_prevention` com 1 conflito (dois `mod` de teste no fim), resolvido mantendo os dois.
  - Testes: hook-runtime 499, ceg 612, cli 690, hook-handlers 752, server 1807, hooks 1182 + as 2 wave25 conhecidas.
  - Ordem invertida (gates-97 antes do P0) aceita, porque os conjuntos são disjuntos. OK para portar o ed8a5319 REPETIDO: a mensagem anterior não tinha sido lida.
- **17:06Z — INT97: componente 4 (P0) PORTADO e VERDE.**
  - Fonte relida antes de aplicar: checks.rs ed8a5319d87d, modules e121c6e0, tests c61ff1e3, namespace b4bfbab5.
  - `lang/python.rs` ficou com 4 conflitos, todos resolvidos com motivo:
    - C1 e C2: `reais_dos_membros` do P0 mais `Alvo::Incerto` da VL0;
    - C3: `super::modules` → `self::modules` (layout novo, ×5);
    - C4: o código do P0 fica e o `verdict_dos` NÃO volta (mora em `politica.rs`).
  - Testes: touring-code 1201 (+52 do P0), hook-handlers 752, server --lib 1658, 0 falhas; doc -D warnings rc 0.
  - Em fila (1 vaga): gates completos, 11 mutantes da ligação, release, congelar, reprodutibilidade, depois os juízes R1/R2 por id.
  - O patch CLI não-UTF-8 e o E2EHERM entram num 2º passe dos pacotes afetados.
- **17:19Z — vhtml, 2ª rodada (`38181087`): resultado ACEITO, meio RECUSADO.**
  - Resultado: HTML nunca nega; 83/83; juiz recurso 20/20, ancora 19/20, falso deny 0/200, p95 26 ms; controle 4.969 com 0 deny; patch do CLI não-UTF-8 pronto (`B/diagnostics/vhtml/cli-nao-utf8.patch`, 2e245078127d), não aplicado.
  - Recusado: o adaptador rotulava ausência local como `Alvo::Incerto` para escapar do P2.
  - Pedido: patch de NÚCLEO com `recurso`/`ancora` em `CLASSES_CONSULTIVAS` (§5.1: o deny de «roda agora» é só para classes de símbolo), aplicado no worktree dele e integrado depois; o adaptador volta a `Projeto` onde o disco decide.
- **22:04Z — RETOMADA depois do limite de sessão** (todos os agentes falharam com «session limit» entre 17:26Z e 17:36Z; nada vivo até as 22:01Z).
  - **INT97, passo final CONCLUÍDO antes do limite** (`scratchpad/int97/logs/final.log`, 17:05–18:32Z, rc 0):
    - gates fmt, clippy workspace, testes de code/hook-runtime/ceg/server/pyright/hook-handlers/cli/lsp e doc rc 0;
    - hooks 101 = SÓ as 2 da wave25 (L111/L128, E2EHERM);
    - 11/11 mutantes da ligação;
    - release reproduzível (build1 == build2), congelado em **`b30d107c`** (sha conferido).
  - Retomados: int97 (patch do CLI não-UTF-8 → rebuild → juízes R1/R2 por id, ref. primária 4501e1e5), vrs-porta (medidas finais + eco dos 10 itens), vts (da leitura do juiz até a entrega), vhtml (patch CLASSES_CONSULTIVAS), e2eherm (mutante + gates + prova dos bancos), instrumentos-p1 ((a)/(b) antes da rodada 2).
  - **P0B + P0BMRO iniciados** (brief `briefs/2026-10-10-p0b.md`): worktree `touring-wt-p0b` espelhando o int97.
  - touring-a6 (Kimi): o 1º pedido nunca foi lido. Provavelmente está retido por modo de permissão diferente, à espera da aprovação do Gabriel naquela sessão. Reenviado com `notify_when_idle`.
  - Para poupar créditos e contexto, cada agente manda UMA mensagem no fim.
- **22:06Z — HORUST-REAL: método resolvido.** Os itens da rodada 2 eram todos do tipo (b) e já tinham rodado (17:19Z) antes da minha regra chegar. Nenhum motor rodou.
  - Itens: `nome_tipo` aceita E0425; reparo de `import_use` pelo penúltimo segmento; revalidação de 22 descartes (17 válidos).
  - Registro: Adendos 1 e 2 RETROATIVOS gravados às 22:05Z; o prefixo original segue 45fd89a1, o arquivo inteiro passa a 7b8063b4. O adendo `cfg` foi feito ANTES das entradas (17:19:46Z, 590b00f3, com corpus e selo próprios).
  - Só com as regras originais, todas as classes já passam de 20: kwarg 53, atributo 57, import 36, nome 38.
  - **Decisão: o veredito PRIMÁRIO do J = `ids_regra_original`**; o conjunto com os adendos (53/57/86/82) é secundário, reportado ao lado.
- **22:13Z — HORUST-REAL principal SELADO e conferido por mim.**
  - `confere_selo` INTACTO, rc 0, sha_tree `8fecdb20ff6bafac` (`~/.claude/touring/vgp/corpus_horust_real_20261010`, selo em `diagnostics/horust-real/selo.json`).
  - Tamanho: 278 positivos (kwarg 53, atributo 57, import 86, nome 82) e 340 negativos. `ids_regra_original` = 184 (53/57/36/38), `ids_regra_adendo` = 94.
  - Reconfirmação 30/30; abrir 3 com 12/12 coerentes.
  - **Juiz J de Rust: veredito PRIMÁRIO sobre os 184 + os 340 negativos**; o conjunto completo é secundário. Quem julga é a coordenadora, uma vez, com o binário integrado final (worktree `touring-wt-horust-real` em c118aa52).
  - O estrato `cfg` (80 tentativas, adendo próprio) está na fila.
- **22:18Z — HORUST-REAL re-selado** (o veredito primário foi para o meta antes do meu «não mexa» chegar). Comparei os dois selos: entradas, arquivos, resultados e reconfirmação IGUAIS; mudaram só o meta (+499 bytes), o gerador (gate novo de negativos disjuntos de todo arquivo tentado) e o sha_tree. **Canônico: `4ed4f50b6e38016e`** (conferido INTACTO, rc 0); o `8fecdb20` está superado.
- **22:18Z — vts (TS/TSX/JS) ACEITO com 1 ajuste** (binário `d774ae2e`, reproduzível).
  - Medidas: recall TS/TSX 1,0 (n=20 por classe); JS 19/20 (o perdido em unknown por orçamento); falso deny 0/135, 0/200, 0/9.
  - Controle limpo: TS 186/0 e TSX 317/0; JS 227 com 11 warn e 0 deny (2 verdadeiros; o resto são globais de hospedeiro).
  - Mutantes 111/113 (2 equivalentes provados); gates 1299 passaram.
  - Decisões: (b) nome livre em JS clássico = warn; (c) orçamento → warn/unknown.
  - **Ajuste (a)**: import de pacote externo vira UMA linha `nao_medido` por arquivo (REGRA ZERO); membros via .d.ts ficam fora, declarado.
  - Limite para a prova: a precisão de JS no semeado tem n=9, «não provada».
- **22:21Z — CORREÇÃO: o selo canônico do HORUST-REAL VOLTA a `8fecdb20ff6bafac`.** O instrumentos-p1 desfez o re-selo para cumprir o «não mexa»: meta ca6e5235 e selo.json 80b20710, iguais aos que eu conferi primeiro; gerador revertido, provado por hash. Conferi agora: INTACTO, rc 0. O `4ed4f50b` ficou guardado só como evidência (`selo.segundo-desfeito.json`) e NÃO vale. Negativos disjuntos de todo arquivo tentado: 340 negativos × 388 tentados, 0 interseções (`negativos_tentados.json`, fora do corpus). O veredito primário (`ids_regra_original`, 184) fica declarado no RELATORIO e aqui.
- **22:43Z — DECISÕES DO GABRIEL** («Recomendações aprovadas para todas as decisões»):
  1. **Commits locais, sem push: AUTORIZADOS.** Feito agora por snapshot NÃO invasivo (índice temporário → `write-tree` → `commit-tree` → `update-ref`), sem mexer em HEAD, índice ou arquivos dos agentes: 15 refs em `refs/heads/safety/2026-10-10/*` (principal a3b6561c, int97 e3994c53, p0b 2dff1e7f, gates97 1ba071b5, vts 3b03133c, vhtml 5e43a5f0, vtransv 6d2bef27, vrs2 3e793a9b, vl0 f1a57bae, l2 a549e0c5, vgp-attr 2a5532c2, f21 fcd85cd8, e2eherm 59119838, g1 8984bfa1, vrs 84527c28).
     - Script: `scratchpad/snapshot_safety.py`.
     - Varredura de segredos: 9+1 ocorrências, TODAS fixtures falsas de `test_t09_redacts_*` em `sandbox_executor.rs`, revisadas mascaradas.
     - **Limite**: isto protege contra erro de ferramenta ou agente, NÃO contra falha do disco (mesmo NVMe, sem backup do /home).
  2. **J13 = critério (A), contagem BRUTA** (os falsos de rótulo do adjudicador saem impressos à parte, só como diagnóstico).
  3. **Fragmento `.py` em /tmp** (minha recomendação, que eu não tinha escrito antes e escrevo agora): a escrita em /tmp e .cache vira warn, e o deny passa para a EXECUÇÃO (pre-bash `python3`/`node` X). P1 externo inalterado. Frente **FRAG**, do gates-97.
  4. **Release 30.4.97 autorizado** quando os juízes dos held-outs passarem: R97 = Python (INT97 + P0B + FRAG); os adaptadores vão num release seguinte, depois dos J de Rust, HTML e TS. Assim a V4 (5 sessões reais) começa cedo.
  5. **touring-a6**: a aprovação precisa ser feita NA sessão dela (permissão por sessão); não posso aprovar daqui.
  6. **Limpeza aprovada**, mas o guard bloqueia a remoção recursiva para mim; o comando vai ao Gabriel. Achado: `~/.cache/vgp-semeado/proj-47467rpw` é uma cópia de `~/projects` inteiro (09/10 19:51, gerador de corpus semeado) com **411,6 GiB EXCLUSIVOS** (btrfs fi du), mais `proj-0mez08w5` com 21,6 GiB. Sem referência em corpus vivo e sem fd aberto. Disco em 85% (150 GB livres). **F2.1**: `javascript:` de corpo constante deixa de ser achado (frente F21K, do f21-falso-positivo); `innerHTML` vai para o BACKLOG (F21INNER, fora deste plano).
- **23:18Z — E2EHERM ACEITO** (patch `diagnostics/e2eherm/e2eherm.patch` sha256 8b2cfcc3…, 2 arquivos de teste; o helper só adiciona `ProjectPin`/`start_isolated`/`client_command`).
  - Prova: baseline 4/2 → 6/6 nos 4 modos (limpo, sessão, e os dois com `~/.claude` em bind read-only pelo bwrap); controle negativo ("" → "memory: <chave>"); mutantes A e B mortos; 0 SKIP.
  - Bancos reais: 20 DBs, 263 tabelas, contagem idêntica; a deriva no env da sessão apareceu igual dentro do read-only, logo é ambiente.
  - Achado extra: o `touring_binary()` próprio ignorava `CARGO_TARGET_DIR` (SKIP silencioso em todo worktree).
  - Limites: os testes 3–6 seguem dependentes do ambiente, e o 4 é vácuo sem órfãos (backlog).
  - Mandado à int97 para o passe de teste depois dos juízes.
  - Resíduos para a limpeza do Gabriel: `touring-wt-e2eherm/target` (1,9 GB, criado pelo cargo check do hook) e `scratchpad/e2eherm/root.4ZWIVA` (417 MB).
- **23:45Z — vts FECHADO: binário `bc3c979f`** (reproduzível 2×; d774ae2e superado).
  - O ajuste (a): import de pacote externo vira UMA linha de `nao_medido` (nome do pacote normalizado, até 6, depois «e mais K»).
  - Prova: 120/122 mutantes (2 equivalentes); juiz idêntico (recall TS/TSX 1,0; JS 19/20; falso deny 0); 1304 testes.
  - Consequência: TS/TSX reais vão de allow para unknown. O hook só mostra deny/aviso ao modelo (`sombra.rs:32`), então não há ruído, e o juiz não muda.
  - VTS e VTSX = completed (desenvolvimento).
- **23:46Z — Brief INT-ADAPT escrito** (`briefs/2026-10-11-int-adapt.md`; DAG INTADAPT). Começa quando o int97 tiver CLI + E2EHERM + P0B + FRAG; os patches dos adaptadores são calculados contra o worktree da VL0; o FRAG e o vhtml mexem na mesma `severidade`, e a combinação é «classe consultiva → warn sempre», antes do contexto.

## VERIFICAÇÃO EM DISCO — 16/10/2026 ~14:15Z (sessão efe418c2, só leitura)

Máquina reiniciada em 16/10 às 10:53 BRT; o `/tmp` é tmpfs e foi zerado. Nada andou no plano depois de 10/10 ~21:16 BRT: os agentes pararam no limite (de sessão e depois o semanal) entre 21:06 e 21:16 BRT, e o último arquivo do bundle é de 21:33 BRT (`diagnostics/p0b/resultados/mutantes/rc.txt`). As notas do `.remember` datadas de «12/10» e «13/10» descrevem eventos de 10/10 (a troca do html de `recurso` foi às 13:11Z de 10/10) e dizem que P0B e FRAG não começaram, o que é falso.
- **Snapshots `safety/2026-10-10/*`** (comparação blob a blob contra o working tree): cobrem 100% de e2eherm, g1, gates97, int97, l2, vgp-attr, vl0, vrs, vtransv e vts. **NÃO cobrem**: f21 (3 arquivos, F21K), p0b (3), vhtml (`politica.rs`), vrs2 (`lang/rust.rs` + `rust/tests.rs`), frag (sem snapshot nenhum: 29 M + 51 ??) e a principal (docs posteriores a 19:43 BRT, este diário incluso).
- **Binários**: 29 em `binarios/`; 27 conferem com o `.sha256`/`.sha`, e c8b503c4 e e935aaf1 não têm declaração. Três não têm registro no diário: `294d0c0e` (21:06), `18f985eb` (21:41) e `11600c7a` (22:02), coerentes com builds em background do vrs-porta, do vhtml e do int97 que terminaram depois de o agente parar. Não provado.
- **Selos** (`confere_selo.py`, rc 0): 20261011 `c495bcab`, 20261013f `2f9282aa`, html_git_20261010 `b6966be5`, horust_real `8fecdb20`, todos INTACTOS; selftest ok.
- **Suíte do bundle**: 250 passed, rc 0 (43 s, hw-slot). Eram 227 às 13:37Z de 10/10.
- **Evidência PERDIDA com o /tmp**: `int97/logs/final.log` (gates e reprodutibilidade do b30d107c) e `{vl0,l2,f21}.log`, `juiz_gates97.sh`, `snapshot_safety.py`, `suite_bundle/`, `memoria_e2eherm_removida.json`, `juiz_v2_b.json`, `juiz_v3_lsp.sh`, `patch_proposta.py`, `corpus_staging/` e `touring_juiz_congelado`. Só o `comparar_porid2.py` tem cópia (`diagnostics/p0b/scripts/`). O binário b30d107c segue com o sha certo; o que se perdeu foi a prova dos gates.
- **Frentes paradas, estado medido**:
  - **INT97**: o patch do CLI não-UTF-8 ESTÁ na árvore (86/86 linhas, por conteúdo); o `e2eherm.patch` NÃO está (aplica limpo); dos juízes R1/R2 do b30d107c não há evidência.
  - **P0B**: VERMELHO. O verde de 21:28 deu rc 101 em touring-code, com 2 falhas: `p0b_c_classe_comum_sem_init_herda_a_assinatura_do_primeiro_init_no_mro` (tests.rs:1857) e `p0b_c_a_identidade_do_no_e_arquivo_e_nome_e_nao_so_o_nome` (tests.rs:1882). Nenhum mutante rodou. A fonte em disco é igual à `fonte_bom` registrada (5/5 sha).
  - **FRAG**: 31 mutantes (os 20 conferidos, todos mortos), verde depois da restauração. Faltam os itens 5 e 6 do brief: juízes por id, gates, binário reproduzível e o `frag.patch` (ausente).
  - **F21K**: 3 arquivos editados no touring-wt-f21, sem `f21-constante.patch` e sem relatório.
  - **vhtml (3ª rodada)**: `politica-recurso-ancora.patch` existe e está aplicado; faltam mutantes, juiz e reprodutibilidade.
  - **vrs-porta**: faltam as medidas finais e o `corpus_rust_real`.
  - **instrumentos-p1**: falta o estrato `cfg` (reconfirmação, montagem, «abrir 3» e selo).
  - **touring-a6** (Kimi): nunca leu o A6-HTML.
- **DAG `task_1791581548206685387`**: 51 nós, sendo 24 completed, 15 in_progress, 9 pending e 3 failed. Vários in_progress (V1, V2, V3, V0e*) estão parados desde o P0M e não refletem trabalho em curso.
- **Disco**: /home em 62% (367 GB livres). `~/.cache/vgp-semeado` ainda ocupa **411,96 GiB exclusivos** (`btrfs fi du`); a remoção é do Gabriel.

## RETOMADA — 17/10/2026 ~12:35Z (sessão efe418c2, «prossiga»)

- **Novo boot às 07:57 BRT de 17/10**: o `/tmp` foi zerado de novo (os scripts s2–s13 da verificação de 16/10 se perderam; o adendo acima segue valendo). Única outra sessão viva está no `analise`.
- **Snapshot aditivo das 6 árvores descobertas** (sem push, sem tocar branch/índice/HEAD; recaptura idêntica nas 6): `refs/heads/safety/2026-10-17/` f21 `3d61626b` · p0b `5511d594` · vhtml `57ac9434` · vrs2 `2e0ab44a` · frag `f883b55e` · principal `d79bad86`. Ferramenta recriada NO BUNDLE (não mais no scratchpad): `scripts/snapshot_safety.py` + `scripts/test_snapshot_safety.py` (6 testes verdes, ruff limpo; recusa ref existente e arquivo > 50 MiB).
- **`touring/target/wt-*` sumiram** (wt-int97, wt-p0b…; só resta `wt-vhtml-cli`). Causa não identificada — o safe-clean semanal (11/10 04:00, 18,5 GB) só toca `incremental/`. Os symlinks `touring-wt-int97/target/debug/{touring,touring-daemon}` ficaram pendurados até o rebuild. Builds do int97 recomeçam a frio.
- **Fato novo**: o `b30d107c` (10/10 15:32) é ANTERIOR ao patch do CLI não-UTF-8 (fontes do int97 tocadas às 21:56) — não é o binário final do INT97; o final sai deste 2º passe.
- **INT97, 2º passe — EM CURSO**: `diagnostics/e2eherm/e2eherm.patch` (8b2cfcc3) aplicado com `git apply --3way` no touring-wt-int97 (2 arquivos de teste; o `--3way` os deixa staged; originais em `diagnostics/int97/backup-pre-e2eherm-2026-10-17/`). Pipeline `diagnostics/int97/scripts/pipeline_int97.sh` → `diagnostics/int97/resultados-2026-10-17/`: gates (fmt; clippy --workspace --all-targets; testes de code, pyright, hook-handlers c/ features, cli, lsp, server, hooks, generator e integration-tests — os 4 últimos incluem o helper do E2EHERM; rustdoc -D warnings) → release congelado + reprodutibilidade (touch + rebuild + cmp) → juízes v2/V3 do binário novo no vivo (28d93bcb) e no bak (7b819af2) → réguas R1/R2 (`p0b/scripts/comparar_porid2.py`) contra as saídas do 4501e1e5 já gravadas em `p0-atributo-2026-10-09/resultados/juizes_dc_{ev,bak}`. Para no primeiro elo vermelho.

## CONSOLIDAÇÃO DOS WORKTREES — 17/10/2026 ~13:30Z (pedido do Gabriel: «consolide todos os worktrees do touring»)

- Pergunta ao Gabriel (destino: duas árvores × uma árvore × só limpar; e remover os 7 contidos) ficou SEM RESPOSTA em 5 min → segui o recomendado e aditivo: **duas árvores** (R97 = int97 + P0B + FRAG; adaptadores no `touring-wt-adapt`, release seguinte — decisão 4 de 10/10). **Nenhum worktree removido** (destrutivo: espera o sim dele).
- **Mapa por conteúdo** (merge 3 vias arquivo a arquivo contra o int97, fora os `.db` de runtime em `crates/touring-server/.claude/`): CONTIDOS no int97/HEAD = e2eherm, l2, g1 (100% iguais), q50 (11 commits) e wiring (1) (`git cherry`: todos com equivalente no HEAD); contidos pelo registro da integração = vl0, vgp-attr, gates97 (componentes 1/4/5; o merge conflita só porque o int97 reeditou as mesmas linhas depois). Ainda insumo: vrs (fonte do vrs-porta), semeado e horust-real (cópias limpas usadas como corpus), frag-base (base do FRAG).
- **Adaptadores consolidados**: `diagnostics/int-adapt/scripts/patches_adapt.sh` → patches contra a VL0 em `diagnostics/int-adapt/patches/` (vtransv 16 arq. sha 186d8b8a · vts 17 dd22107f · vhtml 11 7c44b7e9 · vrs2 2 7ddd6eb1 · f21k 3 40434c7b, este contra o F21 do int97) e simulação em sequência sobre o int97 num índice temporário: **os 5 aplicam, 0 conflito** → árvore `f670d1ec`. Materializada no worktree NOVO **`/home/gabrielgadea/projects/touring-wt-adapt`** (destacado em c118aa52 + `git archive` da árvore; espelho exato provado por write-tree = f670d1ec). Sem build ainda. Ressalvas: F21K sem relatório/aceite; vhtml 3ª rodada e vrs-porta sem as medidas finais — o juiz da INT-ADAPT decide.
- **P0B e FRAG** (`diagnostics/int-adapt/scripts/simula_r97.sh`): P0B (7 arquivos) aplica limpo no int97 e no adapt; o FRAG (14 arquivos) depois dele tem **1 conflito, só a linha `use super::{…}` do `sombra.rs`** (P0B importa `Alvo`; FRAG importa `Contexto` e troca `check` por `check_em`) — resolver juntando e deixar o compilador confirmar. O `politica.rs` (FRAG × vhtml na `severidade`) mescla sem conflito textual, mas a regra «classe consultiva → warn sempre, antes do contexto» tem de ser conferida à mão.
- **P0B — conserto em teste**: as 2 falhas eram `self` na assinatura do construtor (`func_sig` guarda o 1º parâmetro do `def __init__`; o teste espera só os kwargs). Conserto `sem_instancia` em `construtor_de` e `init_herdado` (lang/python.rs) + teste novo `p0b_c_o_primeiro_parametro_do_init_e_a_instancia_e_nao_um_kwarg` (`C(self=1)` é TypeError no CPython e passava calado). Nas referências 4501e1e5 só há 2 achados kwarg por corpus e nenhum com `self` → R1/R2 não devem mudar por isso. Teste de touring-code em fila no hw-slot → `diagnostics/p0b/resultados-2026-10-17/test_code.log`.
- **~13:45Z — 7 worktrees REMOVIDOS com o sim do Gabriel** («sim, remova os 7»): e2eherm, l2, g1, q50, wiring, vgp-attr, gates97. Ritual: MEDIR (árvore atual == `safety/2026-10-10/<nome>` em e2eherm/l2/g1/vgp-attr/gates97; q50 e wiring limpos == HEAD; nenhum processo dentro; o que foi junto era só estado de runtime — `.claude/` com bancos locais, caches, 8 KB de `.touring/scratch` do gates97) → `GIT_DESTRUCTIVE_OK=1 git worktree remove [--force]`, um por worktree, rc 0. VERIFICAR: 14 worktrees restam (principal, adapt, f21, frag, horust-real, int97, p0b, semeado, vhtml, vl0, vrs, vrs2, vtransv, vts); as 7 branches e as 5 refs safety seguem; `worktree prune --dry-run` vazio. Livre em /home: 356 → 362 GB na hora (btrfs devolve espaço de forma assíncrona e os builds do int97/p0b escrevem ao mesmo tempo; a soma `du` dos 7 era ~16 GB).
- **Pastas comuns (não-git) `touring-wt-frag-base*`**: `touring-wt-frag-base-c118aa52` (63 MB, só crates+Cargo) tem árvore IDÊNTICA à do c118aa52 (c7f5fa36) → redundante. `touring-wt-frag-base` (183 MB, cópia completa) = c118aa52 fora de crates + os crates do `safety/2026-10-10/int97` (árvore b35ff023, diff vazio) → fixada na ref aditiva `safety/2026-10-17/frag-base` (af515c6e); ainda é a BASE do `diagnostics/frag/scripts/faz_patch.py` (`BASE = PROJ / "touring-wt-frag-base"`), então fica até o `frag.patch` sair. Apagar pasta comum exige `rm -r`, que é do Gabriel.
- **~14:00Z — Gabriel apagou `touring-wt-frag-base-c118aa52`** (`rm -r`, rc 0; era idêntica ao c118aa52).
- **~13:15Z — VERIFICAÇÃO INDEPENDENTE («verifique tudo»)**, por artefato, não por memória:
  - **INT97 gates (GATES_OK, FALHAS=0)**: code 1.201 · server 1.810 · hooks 1.184 · cli 690 · hook-handlers 752 · generator 433 · integration-tests 208 · pyright 175 · lsp 13 passed, **0 failed** em todos; fmt, clippy --workspace --all-targets e rustdoc -D warnings rc 0. O `wave25_task_enrichment_e2e` (as 2 falhas antigas) deu 6/6. O int97 difere do `safety/2026-10-10/int97` SÓ nos 2 arquivos do E2EHERM; `e2eherm.patch` contido (reverso aplica).
  - **Patch do CLI não-UTF-8 contido**: as 179 linhas `+` estão no `cli/vgp.rs` do int97 NA ORDEM; o reverso não aplica só porque edições posteriores mudaram o contexto (linha 48), e o único `lang: "python".into()` que sobrou é o helper de teste `out_base` (L292), não o sítio do patch (que agora tem `lang: recusa.lang`). Meu primeiro critério (reverso aplica) era estrito demais — falso alarme do verificador, não do int97.
  - **Snapshots 17/10**: f21, vhtml, vrs2, frag == worktree; p0b difere só no conserto (python.rs + tests.rs); `safety/2026-10-17/frag-base` == pasta. **adapt** == f670d1ec; os 5 patches com os sha registrados. 7 removidos ausentes, 12 branches/refs deles intactas, `worktree prune` vazio, 14 worktrees.
  - **Suíte do bundle**: 256 passed (250 + os 6 do `test_snapshot_safety.py`), rc 0, 41,7 s. **Doctor**: 9/9 ok (30.4.96, daemon healthy, 4 projetos).
  - **P0B**: 1.211 passed, 1 failed — as 2 falhas de MRO SUMIRAM; a falha restante era o MEU teste novo (dois `self=` na mesma fonte, e o `out_find` guarda um achado por (classe, nome) de propósito). Teste dividido em duas fontes; touring-code de novo na fila (`test_code_2.log`).
  - Máquina a 95–96 °C (pacote), o hw-slot segura jobs acima de 88 °C. Disco: 340 GB livres (os builds frios do int97/p0b ocuparam ~20 GB).
- **~13:55Z — INT97 FECHADO** (pipeline rc 0, FIM 10:55 BRT): gates 14/14 com 0 falhas; binário final **`8fd0efb0`** reproduzível; juízes por id contra o 4501e1e5 — v2 IDÊNTICO no vivo (738) e no bak (762); V3 com 0 deny novo/perdido, diferenças só de timeout do pyright na camada 2 (vivo 2 R1 + 43 camada2; bak 0 R1 + 58 camada2) e p95 > 1 s igual ao da referência (1.055,2 × 1.055,9 ms no bak). Relatório: [RELATORIO-INT97.md](RELATORIO-INT97.md). P0B: touring-code 1.212/0 (as 2 de MRO resolvidas); gates completos do P0B disparam sozinhos agora.

## ROTEAMENTO DE MODELOS — 17/10/2026 ~14:30Z (Gabriel: «aplique todas as correções para que o roteamento dos agentes com os modelos fique perfeito»)

- Diagnóstico medido (memória `roteamento-de-modelo-so-exige-presenca`): 144 chamadas Agent desde 08/10 (sonnet 116, haiku 16), haiku 0,1% dos tokens; o gate só exige `model` presente; nudge de subagente morto (`cli_suggester` só casa `"Task"`); nada induz delegar; Fable sem checagem de consentimento; KPI de aderência nunca existiu.
- Feito por mim: `~/.claude/agents/touring-scouter.md` `model: sonnet → haiku` (busca = haiku pela matriz; espelho `client/agents` a sincronizar no deploy).
- Delegado (worktree NOVO `touring-wt-roteamento`, espelho exato do int97 = 163948c5; nada vivo é tocado até a coordenadora conferir):
  - **sonnet `roteamento-eng`**: braço `Agent` no `cli_suggester` (TDD + mutante + varredura do mesmo defeito) e `scripts/model_routing_adherence.py` + testes (tokens por família, delegações, desajustes, logs do gate e do nudge, taxa de conversão), plugar no `touring kpi` se for declarativo.
  - **sonnet `roteamento-gate`**: `agent_model_gate.py` passa a conferir modelo × tipo (BUSCA→haiku; IMPLEMENTA sem opus) e consentimento do Fable (`fable_consent.json` + CLI `--consentir-fable`); hook novo `delegation_nudge.py` (Read/Grep/Glob em volume no Opus principal → aconselha Explore+haiku; UserPromptSubmit avisa troca de modelo no meio); doc §gate e §7.
- Deploy (eu, depois de conferir): copiar `client/hooks/*` para a principal (= vivo via symlink), registrar o `delegation_nudge` no `~/.claude/settings.json` e em `client/claude/hooks.settings.json`, sincronizar `client/agents`; o Rust (`cli_suggester`) entra no release seguinte.
- **~15:20Z — P0B: aceite 1–3 rodados** (`diagnostics/p0b/scripts/pipeline_p0b.sh` → `diagnostics/p0b/resultados-2026-10-17/`): gates completos verdes (fmt, clippy --workspace, code, pyright, hook-handlers, server 1.810, 0 falhas); mutantes **36/36 MORTOS** (+ C16/C17 novos do `sem_instancia`), VERMELHO com 17 falhas, fontes restauradas por cmp; binário **`4047373f` reproduzível**. Juízes por id contra o INT97 `8fd0efb0`: **0 deny novo** nos 2 corpora; divergências — `2a78ac89aad8` perde o `__file__` (item a); `d94f8090d150` deny→warn em `st_m_size` (item b — é um POSITIVO real `os.stat_result`, a perda de recall é o custo declarado do item b; candidato a refinamento: tipo de stdlib tipado sem `__getattr__` É existência); `eae0c2b76058`/`76d4d4a88939` são timeout do pyright na camada 2. **Pendência**: v2 no bak com p95 175,6 ms > teto 150 (INT97 137,7) — A/B alternado 2×2 numa vaga rust exclusiva em `resultados-2026-10-17/ab_p95/` decide se é regressão ou ruído. Faltam `p0b.patch` e `RELATORIO-P0B.md`.
- **~15:40Z — P0B FECHADO** ([RELATORIO-P0B.md](RELATORIO-P0B.md)): o p95 do v2 no bak NÃO é regressão — A/B alternado 3×, mesmo binário oscila 66–168 ms (INT97 164/164/70 × P0B 66/168/130; mediana por id 5,4 × 5,0 ms); o teto de 150 ms está dentro do ruído da máquina (risco registrado para o J13). `diagnostics/p0b/p0b.patch` (1cb86d59, 7 arq.) aplica limpo no int97 e no adapt (check). Próximo no R97: FRAG (itens 5–6) → integrar P0B+FRAG no int97 (1 conflito: `use` do `sombra.rs`) → J13.
- **~15:50Z — ROTEAMENTO: DEPLOY FEITO** (conferido por mim; 2 sonnet implementaram, 1 haiku buscou):
  - Vivo agora (14 arquivos da `touring-wt-roteamento` → principal; backup em `diagnostics/roteamento/backup-pre-deploy-20261017T124424/`, com o `settings.json.antes`): `agent_model_gate.py` confere modelo × tipo (busca → haiku; implementação sem opus; vale para o model da chamada E o do frontmatter) e o consentimento do Fable (arquivo de consentimento ≤ 24 h, concedido só pelo `!` do Gabriel); `fable_consent_guard.sh` (shell, Bash + Write/Edit) nega ao modelo conceder o próprio consentimento; `delegation_nudge.py` (Read/Grep/Glob em volume no Opus principal → aconselha Explore+haiku; avisa troca de modelo no meio); `cli-suggest` registrado em `Task|Agent`; `scripts/model_routing_adherence.py` + KPI `touring.routing.adherence` (advisory, piso 0,8) em `docs/kpi/commitments.yaml`; `touring-scouter` → haiku (vivo + espelho `client/agents`, sync CLEAN 377).
  - Provas: 517 testes verdes na principal; `install_claude_hooks.py --check` sem drift; fumaça com HOME temporário (8 casos ok); **E2E real**: um `Agent(Explore, model sonnet)` desta sessão foi RECUSADO pelo gate vivo com a correção; o guard recusou o meu próprio Bash que citava a flag de consentimento. KPI no `touring kpi -j`: 0,5 (8 de 16 buscas decidíveis com modelo caro — 1 é a minha prova E2E).
  - **Limite medido do guard**: no Bash ele nega QUALQUER menção ao nome do arquivo de consentimento ou à flag (o comando poderia escrevê-lo) — inclusive um heredoc de documentação; doc que cita esses nomes vai por Edit/Write (que só olham o caminho). Falso positivo do lado seguro, aceito.
  - Fica para o release: o braço `Agent` do `cli_suggester` e o `action_signature` (Rust) — `diagnostics/roteamento/roteamento-crates.patch` (43d2fd4f, 4 arquivos) aplica limpo no int97 e no adapt; entra junto com P0B+FRAG.
  - Débito observado, fora desta frente: `ruff check client/hooks/` acusa ~40 achados pré-existentes em outros hooks (`session_startup_intelligence.py`, `evidence_guard.py`, `rate_limits_snapshot.py`, testes antigos).
- **~16:05Z — VARREDURA RUFF DE `client/hooks`: 59 → 0, IMPLANTADA** (Gabriel: «investigue os 40 problemas antigos em outros hooks»):
  - Medido: eram **59** achados em 18 arquivos, e **não há config de ruff no repo nem gate que rode ruff** (CI só roda pytest). O veredito era o default do ruff instalado (0.16.6: 413 regras; E402, PLC0415 e F403 fora). Por isso os 6 RUF100 eram `noqa` documentados de regras que saíram do default — o `--fix` cego APAGARIA as justificativas. Medi com E402 ligado: 3 dos 4 `noqa: E402` são obsoletos de verdade (o ruff isenta import após `sys.path.insert` solto); só o do `conftest` (insert dentro de `if`) servia → insert virou incondicional.
  - **Defeitos reais (TDD, RED 6 → verde)**: (1) payload SessionStart que não é objeto (`[]`, `"x"`, `42`) quebrava `data.get` e o handler de última linha trocava o PAINEL INTEIRO pela nota degradada — agora é payload ausente; (2) `except: pass` do recall journal — a classe exata que escondeu o defeito de 01/10 («o fail-open engolia calado») — agora diz no stderr (convenção da casa); guarda externa redundante removida; (3) `code_mode_sdk_section` e `skill_picker_hook` caíam mudos na última linha → stderr; (4) `datetime.now().timestamp()` → `time.time()`; (5) variáveis mortas `status` (sobra do 30.4.95) e `elapsed_ms`/`t0` (morta desde o 1º commit; levá-la ao contexto quebraria o prefixo byte-estável W6 D9); imports mortos `typing.Any`, `os`.
  - Estreitado onde a exceção é enumerável (`parse_json_safe` → ValueError/TypeError/RecursionError; `_fmt_age` → TypeError/ValueError/OverflowError; stdin → ValueError/OSError; `_is_deny` do teste); `noqa: BLE001 — <razão>` nos fail-open de fronteira (coletores, Jev, seções, última linha); `noqa: FLY002` no `api_key` montado de propósito (anti-scanner); `check=False` explícito em 11 `subprocess.run` (o ruff 0.16.6 não oferece fix — inserido por AST); +x nos 3 hooks e no teste com `__main__`; shebang removido do teste sem `__main__`.
  - Provas: `client/hooks/tests/test_hook_fail_open_contract.py` (13 casos) + **7/7 mutantes mortos** (o M3 sobreviveu na 1ª rodada: nenhum caso atingia OverflowError — `-(10**400)` atinge); worktree 465/465; **vivo 480/480** (hooks + sync) com `-W error::UserWarning`; `ruff check client/hooks` → All checks passed; espelho CLEAN 377; fumaça viva: SessionStart rc 0, stderr vazio, payload `[]` não degrada; `rate_limits --show` ok.
  - Deploy: 19 arquivos `touring-wt-roteamento` → principal (todos principal==base c118aa52, 0 conflitos; o script exige que o conjunto divergente seja exatamente o da varredura); backup em `diagnostics/hooks-lint/backup-pre-deploy-20261017T130420/`.
  - **Gate no CI** (Gabriel: «inclua ruff==0.16.6 mais ruff check client/hooks no job de hooks do CI»): passo novo no job `gates`, logo após o pytest de `client/hooks` — venv em `$RUNNER_TEMP`, `pip install ruff==0.16.6`, `ruff check --no-cache client/hooks`. Reproduzido localmente extraindo o `run` do próprio YAML: rc 0, All checks passed, nenhum `.ruff_cache` no checkout. Prova de que o pin importa: a 0.17.1 (a que um runner sem pin instalaria) acusa **6 × UP017** (`timezone.utc` → `datetime.UTC`) — limpar quando a versão subir. Não commitado. `ruff format` reformataria 30 arquivos (fora do escopo: o código nunca foi formatado pelo ruff).
- **~16:25Z — INTEGRAÇÃO NO INT97 EM CURSO** (Gabriel: «prossiga com a integração no int97»):
  - Snapshot antes de tocar: `safety/2026-10-17/int97-pre-integracao` = `0512f955` (98 caminhos sobre o HEAD, recaptura igual, índice/HEAD intocados).
  - **Aplicados** no `touring-wt-int97`: `p0b.patch` (1cb86d59) e `roteamento-crates.patch` (43d2fd4f) — `--check` limpo antes, `--reverse --check` depois (cada patch inteiro presente). P0B × roteamento: 0 arquivos em comum.
  - **FRAG preparado, aplicação ESPERA o aceite dele** (pipeline em release → juízes): `diagnostics/int97-integrado/scripts/aplica_frag.py` refaz o 3 vias (base frag-base af515c6e, ours int97, theirs frag) — 15 caminhos, 1 conflito: o `use super::{…}` do `vgp/sombra.rs`. Resolução: topo = `{Alvo, CheckInput, CheckOutput, Contexto, Finding, check_em}`; os 3 testes do P0B chamam `check(…)` (= `check_em(…, Contexto::Escrita)`, `vgp/mod.rs:140`) via `use super::*`, então `use crate::vgp::check;` vai no `mod tests` (no topo seria import morto fora de cfg(test) → clippy -D). O script só resolve se o bloco for byte a byte o esperado e aborta com outro conflito ou outra contagem de arquivos. Dry-run ok; rustfmt (edição 2024) limpo no sombra.rs; as 4 "diferenças" do rustfmt em arquivo avulso eram artefato do scratch (submódulos ausentes) — no worktree do FRAG os mesmos arquivos dão rc 0.
  - Aceite do integrado: `pipeline_int97i.sh` → `gates_int97i.sh` (pacotes DERIVADOS dos arquivos tocados ∪ a lista do gate do INT97 — a derivação sozinha perdia generator/integration-tests, consumidores do E2EHERM; ganha hooks-core, hooks-shared, quality, ceg, analysis, offensive, hook-runtime, que o gate do INT97 não testava) → `release_int97.sh` reproduzível → juízes v2/V3 por id (vivo e bak) contra `8fd0efb0` + réguas contra INT97 e P0B. Divergência por id contra o INT97 tem de ser explicada por P0B ou FRAG.
  - Por que não enfileirar já: o hw-slot solta a vaga entre etapas do FRAG; os gates do integrado tomariam a vaga antes dos juízes do FRAG e atrasariam o aceite dele.
  - DAG: `P0B` e `P0BMRO` → completed (0,95); `FRAG` → in_progress.
- **~16:40Z — DECISÕES 1 e 2 APROVADAS** (canvas da sessão; Gabriel: «recomendações aprovadas»):
  - **(1) P0B-d — tipo FECHADO corroborado**: fechado = não subclassável + instância sem `__dict__` + sem `__getattr__` (medido em runtime: `stat_result`, `struct_time`, `re.Match` sim; `str`/`int`/`object`/`ast.expr`/`LogRecord` não). A regra larga «stdlib sem `__getattr__`» reprovada pelos dados: os achados `str`/`int` dos corpora são NEGATIVOS (`0893b1f5187f`, `158cbb7178bb`, `d136171ad214`). Causa: `Alvo::Incerto` com dois sentidos (nome ligado por exec × receptor inferido) — a fusão do `sombra.rs` trata os dois como «nunca escalar». Previsão registrada: só `d94f8090d150` muda (warn→deny) nos dois corpora.
    - Worktree `touring-wt-p0bd` = int97 (+P0B +roteamento) + FRAG aplicado pelo `aplica_frag.py` (`APLICA_FRAG_ALVO`); refs `safety/2026-10-17/int97-p0b-rot` = `931544ba`, `safety/2026-10-17/p0bd-base` = `d1e8c250`. Implementação delegada ao sonnet `p0bd-eng` (TDD, 6 mutantes, gates; entrega `diagnostics/p0bd/p0bd.patch` + relatório). Entra no int97 ANTES do pipeline integrado e do J13.
  - **(2) p95 — protocolo C′ PRÉ-REGISTRADO** em [PREREGISTRO-P95-PAREADO-2026-10-17.md](PREREGISTRO-P95-PAREADO-2026-10-17.md) (sha `3e40aa8e9efddf5e`, 13:35 BRT, antes de qualquer resultado do integrado): intercalado por id ABBABAAB, A/A antes do A/B na mesma reserva rust, referência FIXA `8fd0efb0`, portão = IC95 superior de R_típico e R_p95 ≤ 1,10; os 150 ms absolutos só relatados até a fase D (`cpu_ms` no motor). Scripts em `diagnostics/int97-integrado/scripts/` (`tempos_pareados.py`, `analisa_pareado.py` + 8 testes, `pareado_corpus.sh`) e etapa nova no `pipeline_int97i.sh`. A 1ª versão do executor passava string onde o juiz espera `list[str]` — pega por leitura antes do registro.
  - FRAG: release `e59f61e2` REPRODUZIVEL (13:27); juízes em curso.
- **~17:17Z — FRAG ACEITO e APLICADO no int97**: juízes 14:16, `compara_frag` rc 0 (0 DENY_NOVO, 0 INESPERADO); toda divergência é `esperado_rebaixa` deny→warn de fragmento `.py` em `/tmp` (4 ids no v2; no v3 também `d94f8090d150`); `0893b1f5187f` (negativo, `str`) oscila allow↔warn em sentidos opostos no vivo e no bak = ruído da camada 2. p95 de rodada única (informativo pelo pré-registro): bak v2 150,02 ms → `pronto` False por 0,02 ms — exatamente o defeito que o C′ resolve. `frag.patch` b38f42bd reconstrói o FRAG; aplicado no int97 pelo `aplica_frag.py` (backup `diagnostics/int97-integrado/backup-pre-frag-20261017T141721/`), 0 marcador de conflito, int97 == `p0bd-base` nos 15 arquivos.
  - **EMENDA da previsão do P0B-d (antes de qualquer resultado do integrado)**: `d94f8090d150` mora em `/tmp/eleitoral_probe/causa_raiz.py` — o FRAG rebaixa a ESCRITA em `/tmp` para warn, então no juiz de escrita do integrado o P0B-d NÃO muda esse id (previsão corrigida: 0 mudança nos corpora atribuível ao P0B-d). O efeito do P0B-d se prova no contexto de EXECUÇÃO (e em arquivo fora de `/tmp`): checagem dirigida com o binário integrado, `--contexto execucao --camadas 1,2` sobre o conteúdo do `d94f8090d150` → deny; com o P0B `4047373f` → warn. Erro de previsão meu: não cruzei o caminho do caso com a política do FRAG.
- **~17:35Z — ITEM «aprovação do touring-a6» ENCERRADO** (canvas; Gabriel: «recomendação aprovada»): não há mais o que aprovar — a sessão touring-a6 (Kimi) reteve o A6-HTML desde 10/10 por rodar noutro modo de permissão (mensagem entre sessões espera aprovação NAQUELA sessão) e morreu no reboot de 17/10 07:57; não está no ListAgents. O TRABALHO continua valendo (§8 do RELATORIO-VHTML: `#top`, fragmento vazio, `:~:`, `<a name>`, decodificação percentual; nenhuma sessão real com `.html`). DAG: `A6HTML` (depende de `INTADAPT`) e `JHTML` (juiz HTML no held-out web selado, depende de `INTADAPT` + `A6HTML`) — o pré-requisito é estrutural, `validate` sem ciclo (55 nós). Brief marcado `#status:blocked` com o que atualizar antes de reabrir (binário e worktree do adapt). Reabertura: sessão Kimi no MESMO modo de permissão do coordenador.
- **~17:50Z — P0B-d ENTREGUE (v1) e APLICADO no int97; pipeline integrado SEGURADO**:
  - `p0bd-eng` (sonnet): `diagnostics/p0bd/p0bd.patch` v1 sha `598dbbfd5fa8be1c` (9 arquivos), reconstrói a árvore `463e7d98`; RED registrado; gates verdes (fmt; clippy workspace 0 warnings; code 1.228; pyright 181; hook-handlers 780; server 1.813; 15× estresse `vgp::` 181/0); mutantes M1–M6 MORTOS + M7–M13 (M9 sobreviveu na 1ª rodada, morto por teste novo). Decisão: `Finding.receptor` com `#[serde(skip)]` → JSON do juiz byte-idêntico; um parser só da mensagem do pyright. Pendências do autor: a L1 não tipa `os.stat(...)` direto (o caso real usa forma que a L1 tipa — `Path(…).stat()`); `fundir_findings` em CC 13.
  - Aplicado no int97 depois do snapshot `safety/2026-10-17/int97-pos-frag` = `f9ba1491`; `apply --reverse --check` ok; 0 marcador.
  - **Divergência pega na conferência**: o `vgp_l2.rs` do worktree mudou às 14:44, DEPOIS do patch (14:41) — o autor ainda respondia à emenda do FRAG (`camadas_1_e_2_em` com caminho + contexto). Pedido a ele: gates afetados + patch REGENERADO + previsão emendada no RELATORIO (o dele ainda traz a antiga). **Próximo**: chegar o patch final → sincronizar os 9 arquivos do worktree para o int97 (base idêntica: blob do `vgp_l2.rs` em `d1e8c250` = em `f9ba1491`, `8a624599`) → `cmp` → disparar `pipeline_int97i.sh` em `diagnostics/int97-integrado/resultados-2026-10-17/` → checagem dirigida do `d94f8090d150` em execução.
  - Frente nova aberta (fora do VGP): «contexto por chamada», DAG `task_1792258981432513400`, bundle `docs/plans/2026-10-17-contexto-por-chamada/`.
- **~17:57Z — P0B-d FINAL e EMENDA RETIRADA**:
  - Patch final `0dcb772a0797c7b8` (árvore `d20dd11c`); gates rerodados na árvore final (hook-handlers 781 com o teste novo). int97 sincronizado SEM tocar o worktree do autor: os 9 arquivos refeitos de `d1e8c250` + patch (bases idênticas aos blobs de `f9ba1491`), árvore = `d20dd11c` declarada, `apply --reverse --check` ok.
  - **Pega na conferência**: o worktree tinha `false && …` em `corrobora_tipo_fechado` — um MUTANTE aplicado pela rodada `mut_p0bd.py` em curso (memória `copiar-worktree-de-agente-vivo`); patch e int97 estão com o código certo. Copiar do worktree naquele instante teria levado o mutante ao integrado.
  - **A EMENDA DAS ~17:17Z ESTAVA ERRADA — retirada.** O autor mediu (teste `p0bd_a_corroboracao_nega_em_todo_balde_e_momento_a_politica_do_frag_nao_a_mascara`) e a tabela de `politica.rs` confirma: P1 = alvo EXTERNO (stdlib, builtin fechado) → `deny` em todo balde; o FRAG manteve «P1 externo inalterado» e só rebaixa alvo do PROJETO na escrita em `/tmp`. Tipo fechado da stdlib com a ausência confirmada pelas duas camadas é externo. **Previsão vigente: no juiz de escrita do integrado, `d94f8090d150` passa de warn a DENY; nenhum outro id muda por causa do P0B-d.** Meu erro: confundi o rebaixamento do FRAG (P2, projeto) com P1.
  - Próximo: fim da rodada de mutantes (vigia em background) → conferir resultado + worktree restaurado == patch → disparar `pipeline_int97i.sh`.
- **~17:53Z — P0B-d ACEITO como componente; PIPELINE INTEGRADO DISPARADO**: rodada final de mutantes sobre a árvore final (`mutantes_final2`) = RED MORTO (8 falhas) + M1, M2, M3a, M3b, M4–M13 todos MORTOS (15/15); worktree restaurado == int97 (base+final) nos 9 arquivos, 0 divergência. DAG `P0BD` → completed (0,95). `bash diagnostics/int97-integrado/scripts/pipeline_int97i.sh diagnostics/int97-integrado/resultados-2026-10-17` em background (gates derivados → release reproduzível → juízes v2/V3 vivo+bak contra `8fd0efb0` + réguas vs INT97 e P0B → A/A e A/B pareados do C′). Ler `resultados-2026-10-17/pipeline.rc`. Depois: reconciliação por id (toda divergência vs INT97 explicada por P0B, FRAG ou P0B-d; previsão do P0B-d: só `d94f8090d150` warn→deny) → checagem dirigida em execução → J13.

## ~18:30Z 17/10 — pipeline integrado, rodada 1 VERMELHA (2 falhas), consertadas; rodada 2 em curso

Rodada 1 (`int97-integrado/resultados-2026-10-17/`, preservada como evidência): fmt, clippy `--all-targets -D warnings`,
builds e 15 dos 16 pacotes verdes; `FALHAS=2`, o pipeline parou nos gates (REGRA #21).

1. **doc** — `RUSTDOCFLAGS="-D warnings" cargo doc -p touring-code`: «public documentation for `fundir_findings` links to
   private item `corrobora_tipo_fechado`» (`sombra.rs:369`). Origem: P0B-d (os gates do componente não incluíam o doc).
   Conserto de uma linha no int97 (o link intra-doc virou código simples). Adendo: `p0bd/ADENDO-DOC-2026-10-17.md`.
2. **touring-cli** — `cli::kpi::tests::routing_adherence_commitment_parses_and_resolves_from_its_external_file` panicou
   «the routing adherence commitment is declared». Causa: a entrega do roteamento chegou PARTIDA. O `kpi.rs` com o teste
   foi para o int97, mas o `docs/kpi/commitments.yaml` com `touring.routing.adherence` e os scripts produtores foram só
   para o vivo (main). Conserto: as 33 peças não-Rust já aceitas e implantadas no vivo (client/hooks e testes,
   `hooks.settings.json`, `delegation_nudge.py`, `fable_consent_guard.sh`, `docs/economia…`, `commitments.yaml`,
   `scripts/model_routing_adherence.py` e o teste dele), mais `client/agents/touring-scouter.md` e `client/MANIFEST.sha256`,
   foram copiadas do main (fonte aceita, nunca do worktree com agente) para o int97, com cmp 35/35 idênticos.
   Backup das versões do int97: `int97-integrado/backup-pre-rot-20261017/`.
   Verificado: doc rc=0; o teste do KPI passa (1 passed); `sync-client-skills.py --check` CLEAN, 377 arquivos;
   `ruff check client/hooks` limpo; pytest (sync + routing + client/hooks) 538 passed sob hw-slot.
   **Aberto:** a mesma suíte rodada SEM slot, ao mesmo tempo que o `cargo doc`, deu 3 falhas em 152 s (uma delas é
   `test_s46_block_all.py::test_block_all_honors_pragma_allowlist`); com slot, 0 falhas em 29 s. Reprodução sob a carga
   da rodada 2 em curso (REGRA #21: falhar sob carga é defeito, não ambiente).

Rodada 2: `resultados-2026-10-17b/`, mesmo pipeline (o cabeçalho agora nomeia P0B-d e o roteamento).

## ~18:50Z 17/10 — as falhas sob carga eram o hook da qualidade parado no lock global (consertado e implantado)

- Reprodução: suíte de hooks rodada durante a compilação da rodada 2 → run1 538 passed (27,7 s); run2 **5 failed** em
  `test_s46_block_all.py`, TODOS em 35,04 s (o timeout do subprocesso), contra 0,3 s normalmente.
- Causa (medida em `~/.claude/touring/logs/quality-runs.jsonl`): o `touring-quality` do PATH é o wrapper
  `scripts/touring-quality-score`, com lock único de máquina (`/run/user/1000/touring-quality.lock`) e espera padrão de
  **900 s**. Scores de diretório do `~/projects/analise` (165–184 s; à tarde em 15:03, 15:10, 15:16, 15:22 e 15:29) seguram
  o lock. O hook `touring-quality-block-all.sh` calculava `LOCK_WAIT` (20 s, teto 25 < timeout 30) e **nunca o repassava**;
  então todo Write/Edit de toda sessão ficava parado no hook até o timeout de 30 s do Claude Code, enquanto o lock estava
  tomado. O teste estático (`"LOCK_WAIT" in content`) passava com o código morto.
- Conserto (hook vivo = symlink para `client/hooks/`, implantado no main e copiado ao int97): a variável própria do hook,
  `TOURING_BLOCK_ALL_LOCK_WAIT_SECS` (senão `TOURING_QUALITY_LOCK_WAIT_SECS`, senão 20, teto 25, validação numérica),
  exportada ao wrapper; o rc 75 (ocupado) passa a ser registrado pelo nome no log. Teste novo
  `test_block_all_gives_up_on_a_held_lock_within_its_budget` (lock PRIVADO segurado via fcntl, orçamento de 2 s: falha
  aberto em ~2 s) e `_run_block_all` hermético (lock privado por `TOURING_QUALITY_LOCK` e as duas variáveis de espera
  removidas do env).
- Prova: verde candidato 7/7 em 3,3 s; **vermelho** contra o hook original = timeout de 25 s; mutante sem o `export` morre
  (25 s). O 1º desenho do teste passava contra o hook quebrado (usava a variável que o wrapper lê por herança) e foi
  refeito. Suítes: main 539 passed, int97 539 passed (eram 538), `ruff` limpo, espelho CLEAN. Backup em
  `diagnostics/hooks-lint/backup-pre-blockall-20261017/`.
- **Para o Gabriel decidir** (fica proposto, não feito): mesmo consertado, com o lock tomado por um score de diretório,
  todo Write/Edit espera até 20 s e depois falha ABERTO, ou seja, o gate P0 não roda. À tarde isso cobriu cerca de 40% do
  tempo. O conserto de fundo é separar faixas: score de arquivo único (0,03 s) não toma o lock de CPU, que existe para
  scans de 250% de CPU. Alternativa barata: orçamento padrão do hook de 3 s em vez de 20.

## 18/10/2026 ~15:10Z — R97 FEITO: release 30.4.97 propagado (sessão efe418c2; Gabriel: «Lançar o R97 agora (Recomendado)», sem push)

- `propagate-release.sh 30.4.97` rc 0 na 6ª tentativa. Gate 1/6: 260 suítes ok, 0 FAILED. Prova 41/41. `analise` e
  `konverter` em 30.4.97. Detalhe, as 5 tentativas reprovadas e os consertos: `RELATORIO-INT97I-FASE-D.md` §5.
- Binário publicado = binário julgado: corpus ev, 738 ids, 0 divergências de veredito e de findings contra o `5c7b3d80`
  (`diagnostics/r97/vereditos-ev/`).
- Commits só locais (sem push): main `feat/2026-09-16-shell-quality-wiring-python` até o commit «release 30.4.97»; o
  int97 está em `59eb46d3`.
- Próximo: liberar o hw-slot para o r98-rust-eng (lote F: itens 7 TESTES-DB-RUNTIME e 8 JSONL-ATOMIC-RESTO), integrar
  o R98 no main (no conflito em `pre_write.rs:1492` fica o `c70fd6c3`), e depois o release R98. No DAG VGP seguem
  P0B2, JUIZ-TIMEOUT e INTADAPT; no DAG ECC `task_1792263269279844388`, CHECK-COMPILE-SLOT (alta), TMP-PYTEST-COTA e o
  resto da lista.
