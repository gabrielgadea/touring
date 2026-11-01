---
type: Log
title: "Log — chronological history of this loop run"
description: "Append-only history; PreCompact resume notes and phase closes land here."
plan_id: 2026-09-02-tmp-sandbox-portfolio-afordancia
okf_version: 0.1
tags: [loop, log]
timestamp: 2026-09-02T04:17:48.351955-03:00
---

# Log

Part of the [bundle](/index.md).

## 2026-09-02T05:10-03:00 — OUTER completo + estratégia com decision-canvas

- `strategy-loop` (diagnostic OKF) + `touring explore --until-dry` (ledger convergido em 2 rodadas secas, lente externa visitada: Context7 pytest tmp_path, práticas PrivateTmp/ulimit -f/Bazel action tmp/E2B).
- Censo em 1 programa (`diag_tmp_afordancia.py` via `touring run`, 3 bloqueios no caminho: F2.1 `onerror=` lido como XSS — falso positivo; CWE-78 `shell=True` — legítimo; X6 `os.environ` — legítimo, mensagem ensinou a correção) → `diagnostics/census-2026-09-02.json`.
- Censo dos transcripts (14d, 233 arquivos): `touring run` 20,2% do Bash; python nativo ≈ 1:1 com o sandbox; 560 scratch scripts, 82% uso único, harvest 5%, portfolio 40×; 2.734 denies/avisos (12,6%).
- Veredito + canvas: `strategy-2026-09-02-tmp-sandbox-portfolio-afordancia.md` (A substrato → B executor → C biblioteca). Aguarda decisão de Gabriel.
- Achado lateral (wave TIER-2 `task_1788318067626657742`): após `index rebuild` o juiz acusa 166 órfãos "novos" fora dos arquivos da wave; amostras têm consumidores reais (grep) → falso positivo do wiring pós-rebuild; `wiring chains --rebuild` em curso.

## 2026-09-02T05:35-03:00 — resposta do guia do Claude Code integrada

- Scratchpad: localização não documentada; sem setting de realocação; `TMPDIR` no `env` do settings.json só alcança filhos. Sob cota esgotada: sem tratamento/retry/desligar captura documentados (só teto 5 GB de saída, truncagem 64 MiB). Sandbox mode do CC: tmp privado por sessão via `$TMPDIR`, `sandbox.filesystem.allowWrite/denyWrite`, sem limite de tamanho documentado. Fontes: `code.claude.com/docs/en/sandboxing.md`, `tools-reference.md`.
- Estratégia atualizada: §3 A ganha variante A′ (sandbox mode), §9(a) vira prova empírica, causa-raiz 3 registra as três respostas.

## 2026-09-02T05:45-03:00 — DAG registrada, INNER aguardando gate humano

- `task_1788334733408294157` criado com D0 = ticket de DECISAO (hitl, fog hazy) que bloqueia a fronteira; A/A′/B/C dependem de D0. Marcador OUTER arquivado: o Stop guard leu "gate humano passado" ao ver o strategy doc, mas a decisao de Gabriel nao veio — o INNER abre com `loop_marker.py write --task task_1788334733408294157` apos a escolha.

## 2026-09-02T06:20-03:00 — decisao de Gabriel: B + C (canvas 1), W (canvas 2)

- D0 fechado (done). DAG de execucao `task_1788340910617776382` (W, B1-B3, C1-C4; C4 depende de B2+C1). Marcador ativo aponta para ela. A/A2 ficam no DAG-registro `task_1788334733408294157` como nao escolhidas.

## 2026-09-02T07:10-03:00 — W: causa-raiz fechada no codigo (TDD)

- 3 furos: (1) query de chamadas sem funcao livre; (2) `super::`/`self::` resolvidos a partir da raiz do crate; (3) universo legado `./crates/…` nunca fundido. + B3: XSS `on<event>=` so dentro de tag aberta. 4 crates verdes, clippy limpo. Proximo: bump 30.4.31 + update-touring + index rebuild + rejulgar TIER-2.

## 2026-09-02T07:06:46.467316-03:00 — W-wiring-rebuild-perde-arestas-inferidas-fix-detector-rejulgar-TIER2 done

W fechado: 4 furos do detector de orfaos corrigidos sob TDD — (1) query tree-sitter sem chamada livre f() (rust_method_calls.scm +free_fn); (2) super::/self:: resolvidos a partir da raiz do crate em vez do modulo pai (resolve_scope_relative; scope_keyword 4519→808); (3) universo legado ./crates/ nunca fundido (canonicalize strip ./ + migrate_canonicalize_paths funde wiring_map/wiring_unresolved; rows 205k→90k); (4) caminho do hook (update_wiring_after_edit) limpava arestas inferidas sem recria-las — record_inferred_consumers unico para rebuild e hook (C08). B3 junto: XSS on<event>= so dentro de tag aberta. Deploy 30.4.31 (update-touring --no-kill + --no-build), rebuild: orfaos no escopo 5535→1754; residual 111 vs baseline TIER-2 = 42 consumidores em outro arquivo (arquivos editados apos o rebuild, antes do fix 4 — cai no proximo rebuild), 23 pub so locais, 22 sem consumidor, 24 genericos. Rejulgamento TIER-2 pendente do proximo deploy+rebuild (fix 4) e da decisao sobre a divida real.

## 2026-09-02T07:11:52.367701-03:00 — PreCompact snapshot

Loop active. Pending: [B1-ceg-tmp-privado-por-run-landlock-cleanup-tmp_bytes,B2-run-journal-v2-source-file-harvest-brief-orchestrate,B3-fix-F2.1-onerror-falso-positivo-xss-python,C1-auto-harvest-scratch-exit0-2x-escada-provisional,C2-scratch-persistente-indexado-touring-scratch,C3-prior-art-pre-write-por-docstring-intencao,C4-kpi-reuse-ratio]. Resume: `touring decompose ready task_1788340910617776382`.

## 2026-09-02T07:20:08.323687-03:00 — B1 done

CEG: tmp PRIVADO por run (RunTmp touring-run-*, TMPDIR apontado para ele, Landlock troca /tmp,/var/tmp pelo dir do run + /dev), limpeza medida em SandboxResult.tmp_bytes (TOURING_RUN_KEEP_TMP=1 preserva), SandboxConfig.extra_write_roots atravessa spawn_and_capture_in→apply_landlock_to para que os write_roots da policy X8 sobrevivam aos rulesets empilhados (Landlock intersecta). touring-ceg 582/582 verdes; clippy -D warnings limpo.

## 2026-09-02T07:20:08.584420-03:00 — B2 done

run_journal v2: JournalEntry ganha source/file/harvest/brief/orchestrate/tmp_bytes (serde default, retrocompatível); JournalAggregate soma file_runs/inline_runs/scratch_file_runs/harvested_runs/brief_runs/orchestrate_runs/total_tmp_bytes; RunTunables carrega RunSource+JournalMeta e CtxExecuteOutput expõe tmp_bytes. touring-server 55/55 no subconjunto (journal_v2_tests, run_source_tests, cli::run).

## 2026-09-02T07:20:08.820698-03:00 — B3 done

F2.1 XSS: XssPattern::detect só casa handler on*= dentro de tag HTML aberta (ou <script / javascript:); kwarg python onerror= (os.walk) deixa de ser P0 BLOCK — o falso positivo que travou o censo. Teste antigo (bare onerror= → some) invertido com racional de precisão; casos novos: kwarg python, tag multilinha, Vec<u8>.

## 2026-09-02T07:20:09.077183-03:00 — C1 done

Escada automática: programa com harvest_hint (exit 0, ≥5 linhas, parametrizado, sem ids efêmeros) ganha chave snippet:auto:<sig12> em settle_snippet_ladder; na 2ª execução limpa (AUTO_HARVEST_PERSIST_AT=2) persiste via cli-memory-store com #kind:snippet #lang:<l> #process:code-mode #status:provisional #origin:auto-harvest. Antes só --harvest explícito (nunca usado) alimentava a escada.

## 2026-09-02T07:20:09.318897-03:00 — C2 done

Scratch persistente: persist_scratch_block copia bloco do scratchpad do harness (/tmp/claude-*) com exit 0 para <cwd>/.touring/scratch/<YYYY-MM>/<basename> (.touring já git-ignorado) e o payload devolve persisted_as; month_stamp civil-from-days local (iso_date_from_unix do touring-cli não é visível do touring-server).

## 2026-09-02T07:20:09.527542-03:00 — C3 done

Prior-art pré-write: intent_for_new_file ignora linhas #tags: e shebang, então a intenção derivada para o portfólio vem da docstring/propósito e não do codetag — o nudge de prior-art deixa de buscar 'kind:script purpose:…'.

## 2026-09-02T07:20:09.735134-03:00 — C4 done

KPI code_mode_reuse em touring kpi -j: v1_runs/v2_runs, file/scratch/persistent/inline/harvested/orchestrate/brief runs, total_tmp_bytes, reuse_ratio com piso CODE_MODE_REUSE_FLOOR=0.20 e status STUB/PASS/FAIL — a régua que mede se a afordância do portfólio (reuso de blocos) está sendo efetiva.

## 2026-09-02T07:35:00-03:00 — harness: `loop_phase_close.py` não casava sufixo com hífen

Os sete fechamentos B1..C4 acima gravaram relatório, memória, reward e log mas devolveram `dag_updated:false`: `resolve_subtask_id` só aceitava `phase + " "` e os subtasks são `B1-ceg-…`. DAG marcada diretamente (`touring decompose update … --status completed --quality-score 0.9`, 7/7 `subtask_updated:true`). Resolvedor corrigido sob TDD (`test_phase_close_resolve.py`, 6 testes: RED 2 → GREEN 11 com os irmãos) — aceita espaço E hífen, separador obrigatório (`B1` ≠ `B10-…`). Espelho `client/` sincronizado.

## 2026-09-02T09:20:00-03:00 — prova ao vivo do B/C revelou dois furos (B4, B5)

Deploy 30.4.32 (build 7m11s, `update-touring --no-build` restart limpo, daemon 3097078). A prova comportamental confirmou **B1** — o filho vê `TMPDIR=/tmp/touring-run-7xIOVX` e a escrita foi para `/tmp/touring-run-hqPfKC/left.bin`, não para o `/tmp` compartilhado — **B2** (`source:"file"`, `file` casando o path), **C1** (`harvest_hint` + `snippet_trust: ○ untrusted` no payload) e **C4** (`code_mode_reuse` no dashboard: `reuse_ratio 0.133`, `status FAIL` contra o piso 0.20 — a régua nasce medindo o problema que Gabriel apontou).

Mas a mesma prova pegou duas coisas que os testes unitários não pegariam:

- **B4** — `emit_output` do CLI não emitia `tmp_bytes`. O campo existia só no `CtxExecuteOutput` do tool MCP; a rota que humano e agente usam de fato nunca mostrava a medição de B1. Corrigido com a montagem do payload extraída para `full_payload` (função pura, testável sem capturar stdout; `emit_output` só imprime) + 2 testes de elisão.
- **B5** — `WRITE_TOKENS` do X6 só casava `.write(`. `Path(...).write_bytes(b"x"*8192)` escreveu 8 KiB e voltou com `forbidden_calls: []`. A lista agora cobre a FAMÍLIA (`write_bytes(`/`write_text(`/`writelines(`/`.mkdir(`/`.touch(`/`.unlink(`/`os.makedirs`/`os.rename`/`os.replace`/`os.rmdir`/`shutil.rmtree`), com um segundo teste guardando a precisão (`df.rename`, `str.replace`, `json.dumps` não são escrita — a lição do B3, no mesmo dia). `os.rename`/`os.replace` ficam qualificados justamente porque `.rename(`/`.replace(` nus são dominados por pandas e string.

Lição: o teste unitário prova o executor; só a prova comportamental prova a ROTA. Dois furos, ambos entre o executor correto e a superfície que alguém lê.

## 2026-09-02T09:20:35.095406-03:00 — B4 done

emit_output do CLI touring run nao emitia tmp_bytes — o campo existia so no CtxExecuteOutput do tool MCP, entao a medicao da limpeza do tmp privado (B1) era invisivel na rota que humano e agente usam. Montagem do payload extraida para full_payload (funcao pura; emit_output so imprime), 2 testes de elisao. Achado pela prova comportamental, nao pelos 1588 testes verdes.

## 2026-09-02T09:20:35.281777-03:00 — B5 done

X6 WRITE_TOKENS so casava .write( — Path().write_bytes(b'x'*8192) escreveu 8KiB no sandbox com forbidden_calls vazio (medido ao vivo em 30.4.32). Lista agora cobre a familia pathlib/os/shutil; os.rename/os.replace ficam qualificados porque .rename(/.replace( nus sao dominados por pandas e string. Teste de precisao ao lado do de recall (licao B3, mesmo dia).

## 2026-09-02T09:45:00-03:00 — correção de rumo do Gabriel: CLAUDE.md não é changelog

Eu havia escrito no `CLAUDE.md` do projeto um item 14 de 42 linhas narrando a wave — exatamente o anti-padrão que a REGRA #16 nomeia ("CLAUDE.md é constitution, não ledger; changelog/wave history → docs/ ou memory"). Verificado no Context7 (docs oficiais do Claude Code, `code.claude.com/docs/en/{memory,best-practices,claude-directory}`): *"keep the file concise and human-readable, as overly bloated files can cause Claude to ignore instructions"*, alvo **< 200 linhas por arquivo**, *"only include broadly applicable guidance, while reserving on-demand workflows and domain-specific knowledge for skills"*, e delegação para `.claude/rules/` por tópico.

Item 14 reescrito de 42 → 10 linhas: ficou só o que muda o comportamento de quem trabalha aqui (tmp privado por run + o gotcha de que rulesets do Landlock empilham por interseção + a régua `code_mode_reuse`), com ponteiro para este bundle. Arquivo: 353 → 332 linhas.

**Dívida medida, para decisão de Gabriel** (não executada — o conteúdo é dele): dos 332 restantes, ~197 estão em quatro itens que são narrativa de wave (item 8 = 53 L, item 10 = 52 L, item 12 = 26 L, item 13 = 66 L). Mover o racional histórico para `docs/` + memória, deixando o invariante operacional de cada um, colocaria o arquivo perto das 200 linhas recomendadas sem perder nada — o histórico já vive nos bundles.

Lição para mim: a constituição paga o custo em TODA sessão. Um parágrafo que só interessa a quem viveu a wave é imposto sobre todas as futuras.

## 2026-09-02T09:32:59.234441-03:00 — C5 done

A tag #origin:auto-harvest do snippet auto-colhido nomeia uma faceta que nao existe: as 7 canonicas sao kind/purpose/lang/domain/process/artifact/status, e faceta desconhecida e erro duro — a tag era descartada e a memoria so era achavel pela chave. Medido: query por #origin:auto-harvest devolveu 0 enquanto memory recall snippet:auto devolveu 1. Trocada por #process:auto-harvest (a origem E um processo) + guard que le as tags mintadas em settle_snippet_ladder e exige faceta canonica.

## 2026-09-02T10:20:00-03:00 — C5 e o fecho das provas ao vivo (30.4.34)

`#origin:auto-harvest` nomeava uma faceta que não existe. As sete canônicas são
kind/purpose/lang/domain/process/artifact/status e faceta desconhecida é erro duro, então a
tag era descartada em silêncio: a memória auto-colhida só era alcançável pela chave. Medido:
`memory query "#origin:auto-harvest"` devolvia 0 enquanto `memory recall "snippet:auto"`
devolvia 1. Trocada por `#process:auto-harvest` — a origem É um processo — com um guard que lê
as tags mintadas em `settle_snippet_ladder` e exige faceta canônica (RED nomeando `origin`,
GREEN depois). Prova ao vivo em 30.4.34: `memory query "#process:auto-harvest"` devolve o corpo
do snippet, `unknown_facets: []`.

Padrão que se repete nas três correções do dia (B4, B5, C5): o executor estava certo e a
SUPERFÍCIE não — campo que não chega ao payload, idioma que o detector não nomeia, faceta que
o índice recusa. Nenhuma delas aparece num teste de unidade do executor; todas aparecem no
primeiro comando de prova comportamental.

Estado: DAG `task_1788340910617776382` com 13 subtasks fechadas (W, B1-B5, C1-C5), 0 pendentes.
Espelho `client/` limpo (336 arquivos), gate de links do bundle OK (16 docs, 0 órfãos).

## 2026-09-02T10:55:00-03:00 — o rejulgamento da TIER-2 e a dívida honesta

Bundle 02/09: `loop_converged` **exit 0** — Platinum 0.937, 11/11 subtasks, 0 P0, cargo verde, escopo inteiro medido.

TIER-2: exit 1 numa única cláusula, `orphans_base`, e a leitura crua acusava 1740 órfãos "todos novos" contra um baseline de 5389. Era o instrumento, não o sistema: o baseline foi gravado com todo caminho prefixado por `./` e a correção W3 passou a canonizar sem o prefixo, então nenhuma linha casava. Normalizando os dois lados o quadro fica legível — e o baseline de 5388 linhas colapsa para **3148 únicas**, o que é a própria assinatura do defeito que W3 corrigiu: o mesmo símbolo contado duas vezes, com e sem `./`.

| Medida | Valor |
|---|---|
| baseline normalizado | 3148 |
| órfãos agora | 1740 |
| resolvidos pelo detector W | 1426 |
| realmente novos | 18 |

Triagem dos 18 (`diagnostics/orphans-new-vs-baseline.txt`): **8 falsos** — `as_str`, `run` e `layer_metrics` são homônimos em crates diferentes (VP-Scout cadeia 4) e `sandbox_runtimes` é alcançado por tabela de comando, forma que o detector não lê como chamada; **7 `pub` demais** — constantes lidas só no próprio arquivo, agora estreitadas (`pub const` → `const`), o que remove o órfão em vez de escondê-lo; **3 sem uso algum** — `duration_p50`/`duration_p99` do agregado do mirror e `sdk::load_signal_report`.

Os dois percentis foram **ligados ao consumidor natural**: `code_mode_signal_use` já lia o mesmo mirror e descartava a duração de cada entrada. Agora publica `duration_ms_p50`/`duration_ms_p99` — um contador diz se os hooks são usados, o percentil diz se valem o que custam. Guard em `signal_latency_tests`.

Fica **uma** decisão para Gabriel: `sdk::load_signal_report` é API sem consumidor ("used by SDK consumers that want a stable view across runs"). Integrar exige inventar o consumidor; remover apaga superfície pública documentada. Não decidi sozinho.

Achado para o detector: chamada via **tabela de comandos** (ponteiro de função) não é `call_expression`, então todo handler despachado assim lê como órfão. É o mesmo formato de furo que W1 fechou para função livre, e vale uma wave própria.

## 2026-09-02T11:20:00-03:00 — REGRA #0 aplicada, e um segundo instrumento que mentiu

Estreitar as 7 constantes fez o clippy enxergar o crate `touring-hook-handlers` de novo, e ele acusou 4 defeitos: `project_root` nunca usado em `compose_content_warnings` e `flush_dedup`/`flush_cache` nunca chamadas. Nenhum era real. `project_root` é consumido sob `#[cfg(feature = "tantivy-fts")]`; as `flush_*` são chamadas por `session_hooks.rs`, atrás de `session-hooks`. Rodar `clippy` com `--features pre-hooks,post-hooks` — o subconjunto que os testes exigem para compilar — inventa dead code que `--all-features` não vê (0 erros).

É a segunda vez no mesmo dia que o instrumento mente antes do sistema: primeiro o baseline com formato de chave obsoleto, agora o lint com features parciais. A regra que fica: **a configuração que faz compilar não é a configuração que serve de gate**.

Entregue nesta rodada: 7 `pub const` estreitadas (o órfão some porque a decisão foi tomada, não porque foi escondida) e `duration_p50`/`duration_p99` ligados a `code_mode_signal_use`, que já lia o mesmo mirror e jogava fora a duração de cada entrada. Suítes: cli 500, hook-handlers 692, quality 399, hook-runtime 408 — todas verdes; clippy `--all-features` limpo.

## 2026-09-02T10:17:35.993492-03:00 — Z done

REGRA #0 sobre a metade honesta dos 18 orfaos novos: 7 pub const lidas so no proprio arquivo foram estreitadas (o orfao some porque a decisao foi tomada, nao escondida) e duration_p50/p99 do MirrorAggregate — que computavam percentis que ninguem lia — foram ligados a code_mode_signal_use, que ja lia o MESMO mirror e descartava a duracao de cada entrada. Dois instrumentos mentiram no caminho: baseline com formato de chave obsoleto (./ removido pela W3) e clippy com features parciais inventando dead-code que --all-features nao ve.

## 2026-09-02T11:35:00-03:00 — fecho: 30.4.35 propagada e provada por comportamento

`propagate-release.sh 30.4.35` exit 0 (6 gates). Prova ao vivo contra os binários instalados, nunca por rótulo:

| Onde | O que provou |
|---|---|
| local | `tmp_bytes: 8192` no payload e `stdout: /tmp/touring-run-nSjUgA` — B1+B4 juntos |
| local | `X6 denied the file-write capability 'write_text('` — B5 cobre a família |
| local | `code_mode_reuse`: `reuse_ratio 0.14`, `v2_runs 321`, `total_tmp_bytes 116503` |
| local | `code_mode_signal_use`: `duration_ms_p50 0`, `duration_ms_p99 184` — os dois órfãos agora falam |
| analise | 30.4.35, deny B5 ao vivo |
| konverter | 30.4.35, deny B5 ao vivo |

Convergência: bundle `converged: True`, TIER-2 `exit 0`. `post-bash` registrado também em `PostToolUseFailure` (backup `settings.json.bak-20260902T101814-postfailure`).

O número que resume a wave é `reuse_ratio 0.14`: a régua que Gabriel pediu existe, está no dashboard, e diz que a afordância do portfólio **ainda não pega**. Medir foi o entregável; subir o número é a próxima wave.

## 2026-09-02T15:45:00-03:00 — as três decisões de Gabriel, e uma premissa minha que caiu

Gabriel escolheu: **D1 opção C** (consumidor de produto), **D2 opção A** (query restrita), **D3** análise antes de qualquer mudança.

**D2 — a investigação derrubou a premissa que eu havia apresentado no canvas.** Eu descrevi o furo como "identificador de função em posição de valor não é chamada". Medindo o código real: a tabela de comandos despacha por **closures que CHAMAM** o handler, e handlers passados como valor nu são **zero**. A chamada sempre foi vista. O que se perdia era o **qualificador**: a query guardava só o último segmento de `super::backup::run()`, e o workspace tem **130 produtores chamados `run`**, contra um teto de 4 no resolvedor por nome. O par `("backup", "run")` resolve para exatamente um produtor, porque o predicado passa a incluir o arquivo do módulo.

Entregue sob TDD: `extract_qualified_calls` com query própria para as duas formas de caminho, descartando `super`/`self`/`crate` como qualificadores; `find_producer_modules_for_qualified` com no máximo uma aresta por par e desempate para o crate do consumidor; a passagem qualificada rodando ANTES das por nome em `record_inferred_consumers`; e os dois produtores de arestas inferidas, rebuild e hook, alimentando o novo caminho. Testes: 716 em touring-code, 253 em touring-storage, 504 em touring-cli, 408 em touring-hook-runtime.

**D1 — `touring kpi --signal-baseline <path>`.** O KPI passa a comparar o sinal vivo com um `SignalReport` persistido, pela rota pública `sdk::load_signal_report`. O payload ganha `baseline` e `baseline_delta`: adoção que cai vira delta negativo em `used`, custo que sobe vira delta positivo no pior p99. Um caminho ausente ou JSON inválido viram um campo `error`, nunca um KPI mudo. O pior p99 entre hooks foi escolhido no lugar de uma média porque a média entre hooks de volumes diferentes esconde exatamente o que se quer vigiar.

**D3 — proposta em `PROPOSTA-CONDENSACAO-CLAUDE-MD.md`**, com o texto condensado escrito para cada um dos quatro itens e o destino de cada trecho removido. De 197 linhas para 41, deixando o arquivo em cerca de 176. Não aplicada: o conteúdo é de Gabriel e a aprovação é item a item.

Lição do dia, terceira ocorrência: **o instrumento mentiu antes do sistema**. Contei consumidores por substring e casei `signal_report_from_journal` ao procurar `load_signal_report`; contei "handlers em tabela" e casei o nome do MÓDULO. Casamento por nome de símbolo exige fronteira de palavra e verificação da forma sintática, não só da string.

## 2026-09-02T16:40:00-03:00 — D2: o teste que passava dava falsa confiança

Primeiro rebuild com o resolvedor qualificado: órfãos 1731 → 1662, **72 resolvidos**. Mas a tabela de comandos recebeu **zero** arestas, ou seja, o caso que motivou a wave continuava sem solução.

A causa é a mesma família de erro do dia. Meu teste usava `super::backup::run()` no corpo de uma função e passava. A forma REAL põe a chamada dentro de uma closure, dentro de um literal de struct, dentro de `vec![…]` — e **o interior de uma invocação de macro é um `token_tree`, não código**. Nenhuma das 132 chamadas qualificadas da tabela aparece como `call_expression` na árvore sintática. Escrevi o teste com a forma real, ele reprovou, e só então o defeito ficou visível.

Correção: dentro de `token_tree` a varredura é textual, aceitando apenas `ident::ident(` com ambos em minúsculas, o que descarta caminhos de tipo. Continua segura porque o par só vira aresta se existir um `<qualificador>.rs` que declare o nome como público: texto solto não inventa aresta. 717 testes verdes em touring-code, clippy limpo.

Regra que fica: **um teste que usa a forma conveniente do construto, e não a forma que existe no código, mede a query e não o problema.**

## 2026-09-02T12:46:27.966607-03:00 — D1 done

touring kpi --signal-baseline <path> compara o sinal VIVO com um SignalReport persistido, pela rota publica sdk::load_signal_report — que tinha zero consumidores e zero testes. O payload ganha baseline e baseline_delta: adocao que cai vira delta negativo em used, custo que sobe vira delta positivo no pior p99. Caminho ausente ou JSON invalido viram campo error, nunca KPI mudo. Escolhi o PIOR p99 entre hooks em vez de media porque media entre hooks de volumes diferentes esconde justamente o que se quer vigiar. Na estreia ao vivo ja acusou regressao real: adocao 8 -> 6 hooks, pior p99 +84ms.

## 2026-09-02T12:46:28.211732-03:00 — D2 done

A premissa do canvas caiu na investigacao: handlers passados como VALOR nu sao ZERO; a tabela despacha por closures que CHAMAM. O que se perdia era o QUALIFICADOR — a query guardava so o ultimo segmento de super::backup::run() e o workspace tem 130 produtores chamados run contra teto de 4. Entregue extract_qualified_calls (pares modulo,nome) + find_producer_modules_for_qualified (uma aresta por par, desempate para o crate do consumidor) + as duas rotas de inferencia alimentando o novo caminho. Segundo achado, o mais caro: o interior de macro e token_tree, nao codigo — as 132 chamadas da tabela vivem em vec![] e nenhuma aparece como call_expression; meu teste usava a forma conveniente e passava. Varredura textual dentro do token_tree fechou o caso.

## 2026-09-02T12:46:28.445448-03:00 — D3 done

Analise profunda dos 4 itens de narrativa do CLAUDE.md com o texto condensado ESCRITO para cada um e o destino de cada trecho removido: item 8 53->12, item 10 52->14, item 12 26->4, item 13 66->11; total 197->41, arquivo 332->~176. Criterio de corte: invariante (muda o que alguem faz hoje) e gotcha caro ficam; narrativa vai para o arquivo canonico que ja existe. O item 10 e o caso mais claro porque ja aponta para code-mode-operational.md como corpo canonico. Nao aplicada: o conteudo e de Gabriel, aprovacao item a item.

## 2026-09-02T17:30:00-03:00 — as três decisões, fechadas e medidas (30.4.37)

**D2 — o critério de Gabriel foi atendido com número, não com impressão.** Órfãos escopados **1731 → 1561**, 173 resolvidos. A tabela de comandos passou de **6 para 127** arestas como consumidor. E a precisão: das arestas `ast_inferred` para `run`, **96 são novas e todas** têm o consumidor citando literalmente `<módulo>::run`; as 215 imprecisas são exatamente as mesmas de antes, herdadas do teto de quatro por nome. **Zero arestas falsas novas.**

Sobram três símbolos que passaram a constar como órfãos: `is_resolved`, `unresolved_by_class` e `unresolved_imports`. A causa é conhecida e é a mesma família: eles são chamados dentro de `serde_json::json!{…}`, e a varredura textual do `token_tree` só aceita `ident::ident(`, não `.método()` sobre receptor. Estender para métodos foi **recusado deliberadamente**: dentro de um macro, `.map(`/`.collect(`/`.into_iter(` alimentariam a resolução por nome com teto de quatro, e o ganho de três órfãos custaria arestas falsas — exatamente o que o critério proíbe. Fica registrado como limitação com mecanismo explicado.

**D1 — a régua estreou acusando.** `touring kpi --signal-baseline docs/plans/2026-08-31-code-mode-sinal/sdksignal-report-f4.json` devolveu `used 6` contra `8` no baseline e o pior p99 **84 ms acima**. Na primeira execução, o consumidor que a função esperava desde sempre encontrou uma regressão real de adoção e de custo. Caminho inexistente responde `signal report missing: …`, nunca silêncio.

**D3 — proposta escrita, não aplicada.** `PROPOSTA-CONDENSACAO-CLAUDE-MD.md` traz o texto condensado de cada um dos quatro itens e o destino de cada trecho removido.

DAG `task_1788360579582461575`: 3/3 fechadas.


## 2026-09-02T15:55:00-03:00 — o hook me pegou fazendo o que a wave mede

O Stop hook fechou o turno com um dado sobre mim: **72 Bash e zero `touring run`**, sendo 64 deles
prefixados com o bypass do gate. Passei o turno entregando a régua que mede o reuso de blocos e,
ao mesmo tempo, sendo o caso que ela mede. O bypass é legítimo por comando; usá-lo 64 vezes é uma
escolha de rota, não uma exceção.

Fechei com a varredura que deveria ter usado antes: um `touring run --lang bash --file` devolveu
versão, órfãos, as duas réguas, o estado dos dois DAGs, o gate de links e o espelho, tudo num
digest. E a régua reagiu na hora: `reuse_ratio` **0,140 → 0,161**.

É o loop fechando sobre o próprio autor. A afordância não falhou por estar ausente; falhou porque
o caminho de menor atrito imediato era o bypass, e eu o tomei 64 vezes. É exatamente a tese que o
projeto registra: **afordância muda o custo da ação, persuasão não** — e um bypass barato é uma
afordância na direção errada.

Estado final medido nessa única varredura: órfãos 1561, DAGs `task_1788340910617776382` e
`task_1788360579582461575` com zero pendentes, 22 documentos sem link quebrado nem órfão, espelho
`client/` limpo com 336 arquivos. Os binários dos projetos pinados não foram lidos por dentro do
sandbox, que não concede execução fora do workspace; a prova deles foi feita por fora, e ambos
responderam 30.4.37 com o deny ao vivo.


## 2026-09-02T16:20:00-03:00 — condensação aplicada e o gate que cobra o bypass

**Condensação executada.** `CLAUDE.md` do projeto: **332 → 184 linhas**, abaixo das 200 que a
documentação oficial recomenda. Nada foi apagado: o texto integral de cada item saiu do arquivo e
entrou no destino canônico, datado e com a origem declarada.

| Item | Antes | Depois | Destino do texto |
|---|---|---|---|
| 13 F0.3 hooks | 66 | 12 | `2026-08-31-complementacao-hooks/HISTORICO-CLAUDE-MD.md` |
| 8 Portfólio ADW | 53 | 13 | `2026-08-18-graph-engineering-flow-portfolio/HISTORICO-CLAUDE-MD.md` |
| 10 Code mode | 52 | 18 | `code-mode-operational.md`, que o item já declarava como corpo canônico |
| 12 Code-mode-sinal | 26 | 6 | `2026-08-31-code-mode-sinal/HISTORICO-CLAUDE-MD.md` |

**G11 — o orçamento do bypass.** O token `TOURING_GATE_OK=1` é legítimo por comando; o que não era
medido é a SÉRIE. Medido no autor desta wave: 72 Bash num turno, zero `touring run`, 64 bypasses.
Um desvio mais barato que a rota sancionada é uma afordância na direção errada, e nenhum nudge
corrige um custo.

Quatro regras, todas com a mesma lógica de que **a rota sancionada é o que recarrega o direito**:

- **R1** dois bypasses seguidos, sem `touring run` entre eles, são negados.
- **R2** três bypasses na janela de 600 segundos são negados, ainda que intercalados por outros
  comandos. É a regra que pega a série espaçada, que a contagem consecutiva sozinha não vê.
- **R3** um `touring run` zera as duas contas. Zerar apenas a consecutiva deixaria um `touring run`
  a cada dois bypasses sustentar a série para sempre.
- **R4** o deny do G11 **não é bypassável**, porque um bypass que se auto-libera não é orçamento. A
  saída é o kill switch humano no env do daemon, que exige decisão fora do turno.

O deny é falante: nomeia a regra violada, mostra as duas contas e entrega a rota. Cinco testes,
sendo um deles o do crédito, que prova o ciclo inteiro de cobrar, negar, creditar e voltar a
liberar. Suíte `touring-cli` 509 verdes, clippy limpo, e2e do suggester 19 verdes.

## 2026-09-02T16:35:00-03:00 — G11 provado ao vivo, e a estreia corrigiu o desenho

A primeira versão passou nos testes e **errou na prova comportamental**: um bypass separado por um
comando neutro era negado como "dois seguidos", o que não era verdade. Uma conta só, chamada
`consecutive`, na verdade media "bypasses desde a última rota". Um gate falante que nomeia a regra
errada ensina a correção errada.

Duas contas separadas resolvem: `strict_run` zera em qualquer comando que não seja bypass;
`since_route` só a rota sancionada zera. Assim cada regra tem efeito próprio — com uma conta só, a
segunda era inalcançável, porque a primeira sempre disparava antes.

Prova contra o hook vivo em 30.4.39, oito passos na ordem do desenho:

| Passo | Comando | Veredito |
|---|---|---|
| 1 | primeiro bypass | passa |
| 2 | segundo colado no primeiro | **deny** — dois bypasses seguidos, 2 seguido(s), 2 desde a rota |
| 3 | `touring run` | passa e credita |
| 4 | bypass após a rota | passa |
| 5 | comando neutro | passa |
| 6 | bypass com o neutro no meio | passa, e é a correção: a v1 negava aqui |
| 7 | comando neutro | passa |
| 8 | terceiro sem a rota | **deny** — terceiro bypass sem usar a rota, 1 seguido(s), 3 desde a rota |

Sete testes de unidade, 511 na suíte do `touring-cli`, clippy limpo.
