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
**Nada commitado** (só por ordem do Gabriel). **ABERTOS**: remedição pós-deploy da V0e5 (amanhã, com dados); os 57
arquivos só-formatação no diff (manter = CI verde; separar = reverter esses caminhos, sem perda); `settings.json`
hooks ×17 (D1/D2 `failed`, decisão do Gabriel); as pendências antigas do §3.
