---
type: Analysis
title: "Análise qualitativa do conteúdo injetado pelos hooks — prosa, acionabilidade e especificidade, com payloads verbatim"
description: "Leitura do conteúdo literal que o modelo lê: 16 famílias de injeção do Touring, 92.481 blocos e 43,6 MB (ponderados por ocorrência) em 799 sessões, com exemplos verbatim reais, juízo de prosa (frase ou caco), de ação (diz ou informa), de especificidade e de fração derivável. Inclui a ordem de leitura real no transcript e a regra de qualidade extraída do próprio corpus."
plan_id: 2026-10-26-injecao-contexto-hooks
okf_version: 1
tags: [hooks, injecao, contexto, corpus, qualitativo, prose, scouter]
timestamp: 2026-10-26
---

# Análise qualitativa do contexto injetado — o que o modelo realmente lê

> **Escopo**: leitura qualitativa do conteúdo literal dos blocos de contexto que os hooks do Touring
> injetam no modelo. Não é contagem — é leitura, com o texto na mão.
> **Fonte**: `scripts/measure_injection_effect.py` (taxonomia `FAMILIES`) + `scripts/measure_injection_impact.py`
> (`M._read_records`, `M.extract_injected`, `M.is_touring`).
> **Contrato declarado pelo harness**: `crates/touring-hook-handlers/src/hooks/pre_read.rs:1-7` —
> *"SILENCE IS THE DEFAULT. Only inject context when it passes the 'So what?' test: Is it actionable?
> Is it non-derivable? Is it specific to THIS file?"*

---

## 1. Método

### 1.0 Onde esta a evidencia

| arquivo | conteudo |
|---|---|
| `round4/evidence/scout-corpus-digest.json` | indice dos **59.379** payloads distintos (familia, ocorrencias, bytes) + os 6 exemplares mais frequentes de cada familia, **com texto integral** — permite re-verificar qualquer numero deste relatorio sem reextrair o corpus |
| `round4/evidence/scout-verbatim-picks.json` | os 6 payloads mais distintos por familia (assinatura `\d+` normalizada), com texto integral |
| `round4/evidence/scout-deep.json` | composicao de atomos por familia, blocos por turno, pares de familia que citam o mesmo caminho no mesmo turno |
| `round4/evidence/scout-order.json` | contagem de posicao de cada bloco na sequencia de registros do transcript |

O texto integral de **qualquer** payload citado aqui esta no transcript original
(`~/.claude/projects/*/*.jsonl`), no stdout bruto do hook.

### 1.1 De onde veio cada exemplo

Cada bloco citado neste relatório é **verbatim**: o texto exato que saiu do hook, extraído dos transcripts
reais em `~/.claude/projects/`. Nada foi reescrito, reformatado ou parafraseado — quando o payload é longo,
o corte é literal (`…` no fim da linha) e o arquivo de origem está nomeado.

| elemento | origem | como se chegou nele |
|---|---|---|
| taxonomia das 16 famílias | `scripts/measure_injection_effect.py:60-80` (`FAMILIES`) | casada por **conteúdo**, não por cabeçalho |
| corpus | `~/.claude/projects/*/*.jsonl` (3 projetos, **799 sessões**) | lido com `M._read_records` |
| extração do payload | `M.extract_injected` + `E.extract_channels` | separa `additionalContext` (o que o modelo lê) do `permissionDecisionReason` (a razão do deny) |
| filtros de pertinência | `M.is_touring` | descarta hooks de terceiros (`CLAUDE_PLUGIN_ROOT`, `gitnexus`, `$CLAUDE_PROJECT_DIR`) |
| ordem de leitura | ordem dos registros no `.jsonl` | reconstruída registro a registro, sem filtro |
| escolha dos exemplos | 6 payloads mais distintos por família | assinatura `re.sub(r"\d+","N", texto)[:110]`, para não repetir o mesmo texto com números diferentes |

**Exemplo de arquivo de origem** (do bloco `code-mode-contract`):

```
MUST    touring run --lang bash --code '.venv/bin/python3 - <<'\''EOF'\''
```

*origem: `0215c306-1abf-47bd-ab36-68...` · hook `PreToolUse:Bash` · evento `PreToolUse` · 168 ocorrências · 172 bytes*

### 1.2 O que é medido e o que é julgado

**Medido** (contagem determinística sobre o corpus):

- contagem de blocos, cardinalidade distinta e bytes por família;
- fração de bytes repetidos byte-a-byte (payload idêntico reemitido);
- posição de cada bloco na sequência de registros (antes/depois da chamada de ferramenta);
- composição de átomos por família (`FILE_ECHO` / `CROSS_FILE` / `GLOBAL` / `NOISE` / `PROSE` / `DIRECTIVE`);
- coocorrência de famílias no mesmo turno e sobre o mesmo caminho.

**Julgado** (leitura do texto, com critério declarado):

- **prosa** — o bloco é uma frase que um humano entende numa leitura, ou um caco de fragmentos?
- **ação** — o bloco diz o que fazer, ou só informa?
- **especificidade** — o texto muda com o arquivo/comando, ou é genérico?
- **derivabilidade** — quantos bytes o modelo obtém sozinho lendo o arquivo (que o `pre_read.rs:6` diz para NÃO injetar)?

O critério de derivabilidade é o **próprio contrato do repo**, `pre_read.rs:1-7`:

```
//! Philosophy: SILENCE IS THE DEFAULT.
//! Only inject context when it passes the "So what?" test:
//! - Is it actionable? (Claude can DO something differently)
//! - Is it non-derivable? (Claude WON'T find this by reading the file)
//! - Is it specific to THIS file? (Not generic information)
```

O mesmo contrato está em `pre_bash.rs:3`, `pre_edit.rs:582`, `instructions_loaded.rs:17` e
`pre_edit_prevention.rs:309`, e tem um teste de invariante que afirma a regra
(`pre_read_tests.rs:1220-1241`, `audit_purpose_silence_is_default_invariant`):

```rust
let ctx = compose_high_signal_context(&db, "src/plain.rs");
assert!(ctx.is_none(), "File with only metadata must be SILENT — Claude reads this itself: {ctx:?}");
```

### 1.3 Duas armadilhas de método que a leitura qualitativa expõe

**(a) O transcript tem DOIS registros por hook, e as contagens medem o errado.**
Cada invocação de hook produz `attachment.type=hook_success` (o stdout bruto, com o JSON de
`additionalContext` dentro) e, quando há injeção, um segundo registro `attachment.type=hook_additional_context`
(o texto que o modelo efetivamente lê). Em 80 sessões: **17.062** `hook_success/PreToolUse` e **3.388**
`hook_additional_context/PreToolUse`. Cruzando os dois, **3.437** blocos de contexto repetem literal o stdout
do `hook_success` que os precede, e **2.243** divergem — o `additionalContext` traz algo que o stdout não traz.
As contagens da rodada 3 medem o **stdout emitido**; o que o modelo **lê** é o subconjunto
`hook_additional_context`. A diferença é grande e não é visível numa contagem.

**(b) A ordem de leitura não é a que o nome do evento sugere.**
A sequência real dos registros no transcript é:

```
assistant/tool_use                <- o modelo JA emitiu a chamada
  ATT:hook_success:PreToolUse
  ATT:hook_additional_context:PreToolUse   <- o bloco "pre" entra AQUI
user/tool_result                 <- so agora o resultado volta
```
*(diagrama do fluxo de registros — reconstrucao minha, nao e payload)*

Medido: **3.391 de 3.391** blocos `hook_additional_context/PreToolUse` têm um `assistant/tool_use`
emitido imediatamente antes. O prefixo `Pre` nomeia o **evento do hook**, não o momento da leitura.
Isto tem consequência direta e medida:

- **700** blocos `Pre-edit prevention` em `PreToolUse`;
- **680** deles são seguidos de `tool_result` de `Edit`/`Write` — ou seja, o aviso de prevenção chega
  **depois** que a edição já foi aplicada (`"The file ... has been updated successfully."`);
- **20** são seguidos de outra ferramenta;
- **0** chegam antes da execução.

O bloco que se chama "prevention" é, na prática, um **post-mortem do edit**. O aviso não previne: informa.

---
## 2. Catalogo por familia

Cada familia traz: **2-3 exemplos verbatim**, o julgamento de prosa, a acionabilidade, a especificidade,
o custo em bytes e a fracao derivavel. A fracao derivavel segue o criterio do contrato (`pre_read.rs:6`):
bytes que o modelo obtem sozinho **lendo o arquivo ou executando o comando**.

| # | familia | blocos | distintos | bytes distintos | bytes (com repeticao) | mediana | veredito |
|---|---|---|---|---|---|---|---|
| 1 | `pre-edit` | 19062 | 17795 | 21.445.443 | 22.032.450 | 1233 B | CACOS em linha de pipes — o pior caso de prosa do corpus |
| 2 | `touring-suggest` | 16422 | 13412 | 7.122.286 | 8.637.095 | 373 B | FRASE por diretiva; cada linha e uma ordem completa |
| 3 | `post-edit` | 12373 | 8398 | 2.946.259 | 3.584.647 | 248 B | FRASE (a melhor prosa da familia "post") |
| 4 | `post-tool-echo` | 7639 | 4208 | 1.242.808 | 2.134.945 | 279 B | FRASE; degenera quando o caminho falta |
| 5 | `code-mode-deny` | 2782 | 1198 | 761.870 | 1.995.429 | 440 B | DOIS BLOCOS: a deny e FRASE; o contrato de sessao e um MANUAL |
| 6 | `code-mode-contract` | 5347 | 3923 | 1.336.065 | 1.580.480 | 237 B | ORDEM TRUNCADA — 81% com o programa partido |
| 7 | `similar-fail` | 12570 | 2638 | 549.386 | 1.123.771 | 177 B | FATO sem alvo — 74,6% nao nomeia comando nenhum |
| 8 | `plugin-hook` | 1247 | 863 | 580.502 | 1.047.869 | 497 B | CACOS de dashboard — o pior de todos |
| 9 | `evidence-guard` | 4874 | 221 | 47.095 | 757.846 | 178 B | FRASE regra-geral, 1.509x repetida byte-a-byte |
| 10 | `pipefail-evidence` | 1962 | 318 | 67.205 | 343.583 | 204 B | FRASE quase-perfeita: nomeia o comando e o conserto |
| 11 | `generic-noise` | 7374 | 341 | 118.641 | 184.461 | 307 B | LIXO — 95% e o literal `[generic]` |
| 12 | `compact-warning` | 426 | 11 | 2.644 | 112.117 | 282 B | A melhor prosa do corpus inteiro |
| 13 | `session-contract` | 193 | 4 | 528 | 36.691 | 132 B | FRASE-lista; 4 payloads distintos para 193 blocos |
| 14 | `pre-edit-prevention` | 175 | 166 | 29.956 | 31.754 | 182 B | FRASE; a que o modelo precisa E que chega tarde |
| 15 | `pre-read` | 27 | 27 | 31.559 | 31.559 | 1170 B | CACOS de pipes — igual ao pre-edit, mas o conteudo e derivavel |
| 16 | `debug-stdout` | 8 | 7 | 2.966 | 3.036 | 295 B | LIXO de debug — 8 blocos no corpus |

**Total das 16 familias: 92.481 blocos.** As duas colunas de bytes sao distintas de proposito:
`bytes distintos` soma cada payload **uma vez**; `bytes (com repeticao)` e o volume **realmente injetado**,
ponderado pelo numero de ocorrencias — **43.637.733 bytes (43,6 MB)** nas 16 familias e **46.300.168 bytes
(46,3 MB)** com `outros`. A diferenca (5,3 MB) e **repeticao**: e a repeticao, nao o numero de payloads novos,
que produz o volume (ver 2.1 e 2.8).

### 2.1 `pre-edit` — o maior volume, o pior texto

| n | distintos | bytes | mediana | media | max |
|---|---|---|---|---|---|
| 19062 | 17795 | 21.445.443 | 1233 B | 1205 B | 3003 B |

O `pre-edit` e a unica familia que entrega **dois formatos diferentes no mesmo payload**: o de blast radius
(comeca com `HOTSPOT:`) e o de prevencao (comeca com `Pre-edit prevention for <arquivo>`). Sao 4.076 e 3.028
payloads distintos, respectivamente, e **nenhum** payload contem os dois — a fusao e do hook, nao do corpus.

**Exemplo 1 — formato blast (`HOTSPOT:`), payload completo:**

```
HOTSPOT: `get` has 14 callers | callers: [_contents, _bash_uses, full_command, _edits_of, _apply_bash_heredocs (+9 more)] | 5x edited recently | 📋 overview: (22fn, 0types, 300L) [const HOME:L33-33, const VGP:L34-34, const OUT:L35-35, const PROJECTS:L36-36, const SOCKETS:L37-37, fn transcripts_of:L40-41, fn _records:L44-48, fn _contents:L51-52, fn _bash_uses:L55-65, fn full_command:L68-77, fn _apply_edit:L80-83, fn _edits_of:L86-90, const SESSION_DIRS:L93-93, fn session_work:L96-99, const HEREDOC:L102-102, const ASSIGN:L103-103, fn _apply_bash_heredocs:L106-114, fn _fold:L117-121, fn _apply_tool:L124-139, fn rebuild_tmp:L142-156] +11 more | ⚙ code-workflow [python]: symbols=31 exports=19 complexity=0.00 (simple) max_cc=10 antipatterns=2 | co-edits: 5 file(s) frequently edited together [__subagent_stop__, __notification__, scripts, __session_end__, __config_change__] | 📚 related docs: [/home/gabrielgadea/projects/analise/scripts/aco/generators/gen_touring_plan.py@pgcc_warm.json, Plan Doc@project_cronflow_2026_04_24.md] | 📋 kind context: [10 definition] | project: healthy | Analysis: health=0.88 (healthy) | trend=Stable | orphans=8931 | [python] pub API 10
```

*3 ocorrencias | 1182 bytes | hook `PreToolUse:Edit` | evento `PreToolUse` | sessao `10fb71c6`*


**Exemplo 2 — formato prevencao, com sinal e sem `related docs` duplicado:**

```
Pre-edit prevention for /home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/scripts/replay.py (2 signals):
ANTIPATTERN: [python] `print()` — debug output left in production code (1x)
📚 related docs: [Plan Doc@project_cronflow_2026_04_24.md, /home/gabrielgadea/projects/analise/scripts/aco/generators/gen_touring_plan.py@pgcc_warm.json]
```

*2 ocorrencias | 361 bytes | hook `PreToolUse:Edit` | evento `PreToolUse` | sessao `10fb71c6`*


**Exemplo 3 — o formato prevencao mais frequente (o `related docs` sai repetido):**

```
Pre-edit prevention for scripts/eleitoral/documento_mpv_1376.py (1 signal):
📚 related docs: [Diagnostic — `/home/gabrielgadea/projects/analise/scripts/eleitoral`@eleitoral-20260918T152941.md, /home/gabrielgadea/projects/analise/scripts/eleitoral/__init__.py@pgcc_warm.json]
```

*60 ocorrencias | 278 bytes | hook `PreToolUse:Edit` | evento `PreToolUse` | sessao `65d27b45`*

**Julgamento.**

- **Prosa: CACO.** O payload e uma linha de pipes. `HOTSPOT: `classify` has 40 callers | callers: [...] |
  note: Exit 0 (OK), 37 lines output (1088 bytes); bash_metrics[generic]: byte_count=1088 exit_code=0 line_c |
  5x edited recently | 📋 overview: (87fn, 0types, 1308L) [fn test_classify_debug:L6-10, ...] +71 more | ...`
  Cada segmento e autossuficiente, mas **nao ha verbo comum**: o modelo precisa remontar a proposicao
  "o que eu devo fazer com isto?". Nenhum humano leria 2.121 bytes assim para decidir uma acao.
- **Acao: nao.** O bloco **informa**. O unico trecho que manda fazer algo e o `[C08]` no fim
  ("the same change may apply there"), e ele chega depois de 1.900 bytes de outra coisa.
- **Especificidade: ALTA.** `callers:`, `overview:`, `co-edits:`, `related docs:` mudam com o arquivo.
- **Derivavel: ALTA — e aqui esta o defeito.** O segmento `📋 overview: (87fn, 0types, 1308L)
  [fn test_classify_debug:L6-10, fn test_classify_code:L13-16, ...] +71 more` e, literalmente, a **assinatura do
  arquivo** que o modelo esta prestes a ler — 71 simbolos com numero de linha. O `pre_read.rs:6` proibe
  exatamente isso. Medido na amostra de 600 payloads: **19,8%** dos bytes de `pre-edit` sao `FILE_ECHO`
  (o que o modelo le ao abrir o arquivo), contra **3,9%** de `NAO-derivavel (grafo)` (os `callers`) e
  **3,5%** de `ACAO`. O `overview` sozinho sao 13,6% dos bytes — e sao os mais derivaveis de todos.
- **Redundancia: ALTA.** `pre-edit` e `touring-suggest` coexistem no mesmo turno citando o mesmo caminho
  **1.327 vezes**; `pre-edit` com `pre-edit`, **508 vezes**; `post-tool-echo` com `pre-edit`, **833 vezes**.
  Sao dois hooks falando do mesmo arquivo no mesmo turno.
- **Custo: 21,4 MB** — **59% de todo** o contexto injetado no corpus, para uma familia cujo unico item
  acionavel sao ~7% dos bytes.

---
### 2.2 `touring-suggest` — a melhor prosa: uma ordem por linha

| n | distintos | bytes | mediana | media | max |
|---|---|---|---|---|---|
| 16422 | 13412 | 7.122.286 | 373 B | 531 B | 2983 B |

Payload tipico — cada linha e um comando completo, com o arquivo certo:

```
MUST    touring ast meta /home/gabrielgadea/projects/analise/scripts/eleitoral/documento_tendencia_folhas.py --depth summary -j
  MUST    touring ast blast /home/gabrielgadea/projects/analise/scripts/eleitoral/documento_tendencia_folhas.py
  SHOULD  touring pre-edit
  SHOULD  touring gotcha match /home/gabrielgadea/projects/analise/scripts/eleitoral/documento_tendencia_folhas.py
  SHOULD  Edit too ...
```

*41 ocorrencias | 595 bytes | hook `PreToolUse:Edit` | evento `PreToolUse` | sessao `9105f79c`*

E o caso de orcamento de turno, em que o bloco e reduzido a so as diretivas:

```
MUST    touring ast meta /home/gabrielgadea/projects/analise/scripts/eleitoral/tests/test_documento_mpv_1376.py --depth summary -j
  MUST    touring ast blast /home/gabrielgadea/projects/analise/scripts/eleitoral/tests/test_documento_mpv_1376.py
  SHOULD  touring pre-edit
  SHOULD  touring gotcha match /home/gabrielgadea/projects/analise/scripts/eleitoral/tests/test_documento_mpv_1376.py
  SHOULD  ...
```

*41 ocorrencias | 607 bytes | hook `PreToolUse:Edit` | evento `PreToolUse` | sessao `9105f79c`*

**Julgamento.**

- **Prosa: FRASE por linha.** `MUST    touring ast meta /home/.../documento_tendencia_folhas.py --depth summary -j`
  tem verbo (`touring ast meta`), objeto (o caminho absoluto) e efeito. Um humano le e sabe o que fazer.
  Medido: **79,2%** dos bytes da familia sao `DIRECTIVE` — a maior taxa de acao do corpus.
- **Acao: SIM.** E a unica familia que cumpre o item 1 do contrato (`pre_read.rs:5`) de forma systematica.
- **Especificidade: ALTA.** O caminho absoluto do arquivo esta em cada linha.
- **Derivavel: BAIXA.** O modelo nao sabe o que `touring ast meta` retornara para este arquivo antes de rodar.
- **Redundancia: MEDIA-ALTA.** Coexiste com `pre-edit` no mesmo turno citando o mesmo caminho **1.327 vezes** —
  e `pre-edit` (21,4 MB) carrega `overview:` do mesmo arquivo que `touring-suggest` manda consultar por comando.
  Sao duas rotas para o mesmo fato, uma por injecao e outra por comando.
- **Defeito medido: truncamento.** **3.667** blocos (2.197 distintos) contem a linha `SHOULD  Edit tool --path <arquivo> ...`
  — a diretiva e **cortada no meio**, terminada em reticencias. E **7.689** blocos (47% da familia) carregam o
  sufixo `(orcamento do turno esgotado - so as diretivas; TOURING_TURN_BUDGET=<bytes> ajusta o teto)`:
  o proprio bloco admite que foi truncado por orcamento, mas nao diz o que foi cortado.
- **Custo: 7,1 MB**, mediana 373 B — a melhor relacao acao/byte do corpus.

---

### 2.3 `similar-fail` — o bloco que nao diz o que fazer

| n | distintos | bytes | mediana | media | max |
|---|---|---|---|---|---|
| 12570 | 2638 | 549.386 | 177 B | 208 B | 717 B |

O caso mais frequente do corpus — 9.161 blocos (73% da familia) sao **so** isto:

```
Similar command failed recently (similarity=62%)
```

*893 ocorrencias | 48 bytes | hook `PreToolUse:Bash` | evento `PreToolUse` | sessao `05166acc`*

E, quando ha detalhe, o detalhe vem de **outro** comando:

```
Similar command failed recently (similarity=44%)
⚡ last fail: touring — Exit code 1
```

*14 ocorrencias | 87 bytes | hook `PreToolUse:Bash` | evento `PreToolUse` | sessao `aa782be4`*

**Julgamento.**

- **Prosa: FRASE truncada.** `Similar command failed recently (similarity=62%)` e uma frase que termina antes do
  sujeito: falta *qual* comando.
- **Acao: NAO.** Nao ha verbo, nem comando, nem arquivo. O modelo nao tem sobre o que agir.
- **Especificidade: BAIXA.** O texto e o mesmo para qualquer comando; so o numero da similaridade muda.
- **Derivavel: NAO APLICAVEL** — nao e derivavel, e isso e o problema: e um fato sem consequence connected.
- **Custo: 549 KB** — 2.638 distintos para 12.570 blocos. A repeticao e alta, mas o bloco repetido nao diz nada.
- **O `pre_bash.rs:3` declara** *"Only inject when a RELEVANT past failure exists"* e o codigo emite
  (`pre_bash.rs:236`) `Similar command failed recently (similarity={:.0}%)` — o nome do comando que falhou
  **nao entra no payload**. A relevancia que o modulo decide nao e transmitida.

---

### 2.4 `post-edit` — o aviso de qualidade do edit

| n | distintos | bytes | mediana | media | max |
|---|---|---|---|---|---|
| 12373 | 8398 | 2.946.259 | 248 B | 351 B | 3076 B |

Payload com problema de qualidade (o caso acionavel):

```
post-write: 1 issue(s) | ⚙ health-delta: first-observation new=1.00
```

*89 ocorrencias | 69 bytes | hook `PostToolUse:Write` | evento `PostToolUse` | sessao `83f700cf`*

E o caso degenerado — so telemetria, nenhum achado:

```
post-write: ℹ ⚙ health-delta: first-observation new=1.00
```

*1553 ocorrencias | 60 bytes | hook `PostToolUse:Write` | evento `PostToolUse` | sessao `133dd82b`*

**Julgamento.**

- **Prosa: FRASE.** `post-edit verification: 5 issue(s)` + `🚨 COMPLEXITY: parallel_groups has CC=19
  (threshold=15, excess=4)` tem o formato `sintoma (valor, limiar, excesso)` — e o **excesso** que diz a urgencia.
- **Acao: PARCIAL.** Aponta o simbolo e o numero, mas nao diz `o que fazer` com ele.
- **Especificidade: ALTA.** Simbolo, limiar e arquivo do crate.
- **Derivavel: BAIXA.** O CC de um simbolo so aparece depois de compilar/analisar.
- **Redundancia: ALTA com `pre-edit`.** O bloco de `post-edit` (`⚙ check(crate): OK | post-edit verification:`)
  repete o `check` que o `post-tool-echo` tambem invoca, e ambos descrevem o mesmo arquivo editado —
  `post-tool-echo` + `pre-edit` coexistem no mesmo turno **833 vezes**.
- **Defeito medido:** **6.543** blocos (3.464 distintos) trazem **apenas** `⚙ health-delta: first-observation new=1.00` —
  telemetria de uma medicao que o modelo nao pediu e nao pode usar. Nenhuma acao, nenhum achado.
- **Custo: 2,9 MB**, dos quais ~1,2 MB sao esse ruido de telemetria.

---

### 2.5 `post-tool-echo` — a acao certa, o caminho as vezes ausente

| n | distintos | bytes | mediana | media | max |
|---|---|---|---|---|---|
| 7639 | 4208 | 1.242.808 | 279 B | 295 B | 1085 B |

O formato bom — arquivo nomeado + comando a rodar:

```
post-tool-batch: 1 arquivo(s) que podem carregar símbolo público editados: /home/gabrielgadea/projects/analise/scripts/eleitoral/documento_mpv_1376.py
MUST touring wiring orphans -j   // REGRA #0 — ligue todo pub novo a um consumidor
```

*108 ocorrencias | 237 bytes | hook `PostToolBatch` | evento `PostToolBatch` | sessao `65d27b45`*

E o formato degenerado, com o placeholder literal:

```
post-tool-batch: 1 arquivo(s) que podem carregar símbolo público editados: <sem caminho no payload>
```

*15 ocorrencias | 101 bytes | hook `PostToolBatch` | evento `PostToolBatch` | sessao `2be42f79`*

**Julgamento.**

- **Prosa: FRASE.** `post-tool-batch: 1 arquivo(s) que podem carregar simbolo publico editados: <caminho>` +
  `MUST touring wiring orphans -j   // REGRA #0 - ligue todo pub novo a um consumidor` — comando, justificativa e regra.
- **Acao: SIM.** E a unica familia que carrega explicitamente a **REGRA #0** com o comando que a executa.
- **Especificidade: MEDIA-ALTA** quando o caminho esta; **nula** quando nao esta (17 blocos com `<sem caminho no payload>`).
- **Derivavel: BAIXA.** O modelo nao sabe quais pub symbols o edit introduziu sem rodar o grafo.
- **Custo: 1,2 MB.** Historico de 4.208 distintos para 7.639 blocos; 71,8% dos bytes sao repeticao byte-a-byte.
- **Redundancia:** reemitido a cada `PostToolBatch`, mesmo quando o arquivo nao mudou desde a emissao anterior.

---

### 2.6 `generic-noise` — o pior bloco do corpus

| n | distintos | bytes | mediana | media | max |
|---|---|---|---|---|---|
| 7374 | 341 | 118.641 | 307 B | 348 B | 982 B |

**7.024** dos 7.374 blocos (95%) sao exatamente estes 9 caracteres:

```
[generic]
```

*7024 ocorrencias | 9 bytes | hook `PostToolUse:Bash` | evento `PostToolUse` | sessao `133dd82b`*

**Julgamento.**

- **Prosa: NAO E PROSA.** `[generic]` e um marcador interno de classificador vazado para o contexto do modelo.
- **Acao: NAO. Fato: NAO.** Nao afirma nada — o unico conteudo e o nome de uma categoria.
- **Especificidade: ZERO.** Serve para qualquer comando, qualquer arquivo, qualquer resultado.
- **Derivavel: irrelevante** — nao ha nada a derivar porque nao ha nada.
- **Custo: 118 KB** — pequeno em bytes, mas **7.024 blocos** poluem a janela de contexto do modelo. O dano nao e o byte;
  e o fato de o modelo aprender a ler que existe uma categoria `generic` que nao significa nada.
- **Achado qualitative:** o bloco vem **junto** com o `⚡ last fail:` e o erro real no mesmo payload. O conteudo
  acionavel esta **enterrado**: a mensagem util (`process exited with code 1 with empty stderr - for grep/test/diff
  exit 1 usually means 'no match', a valid result to handle`) chega **depois** de `[generic]` e do `⚡ last fail`.
  Em 3.835 blocos o modelo le primeiro o que nao significa nada.

---

### 2.7 `code-mode-contract` — a ordem truncada

| n | distintos | bytes | mediana | media | max |
|---|---|---|---|---|---|
| 5347 | 3923 | 1.336.065 | 237 B | 341 B | 2223 B |

O payload mais frequente:

```
MUST    touring run --lang bash --code '.venv/bin/python3 - <<'\''EOF'\''
  (orçamento do turno esgotado — só as diretivas; `TOURING_TURN_BUDGET=<bytes>` ajusta o teto)
```

*168 ocorrencias | 172 bytes | hook `PreToolUse:Bash` | evento `PreToolUse` | sessao `0215c306`*

**Julgamento.**

- **Prosa: FRASE**, e uma frase que **manda** — `MUST touring run --lang bash --code '...'`.
- **Acao: SIM, no maximo grau.** `MUST` + o comando exato.
- **Especificidade: ALTA** — o programa e derivado do comando que o modelo estava executando.
- **Derivavel: BAIXA** — a fusao do programa nao e derivavel pelo modelo.
- **Defeito medido: ORDEM INEXECUTAVEL.** **1.380** blocos (784 distintos) terminam o programa em aberto
  (`--code '...` sem aspa de fecho) e **3.275** blocos (1.919 distintos) tem citacao impar — o programa **nao fecha**.
  O modelo recebe `MUST touring run --lang bash --code '.venv/bin/python3 - <<'EOF''` e nao pode copia-colar.
  O sufixo `(orcamento do turno esgotado - so as diretivas; ...)` aparece em **3.048** blocos (1.877 distintos).
  O corte e **por design** (o orcamento do turno), mas o resultado e uma ordem que quebra o shell.
- **Custo: 1,3 MB.** Mediana 237 B — barata por bloco, e a familia mais numerosos de ordem dada.

---

### 2.8 `evidence-guard` — a regra repetida 4.874 vezes

| n | distintos | bytes | mediana | media | max |
|---|---|---|---|---|---|
| 4874 | 221 | 47.095 | 178 B | 213 B | 398 B |

O bloco mais frequente do corpus (3.235 ocorrencias, o mesmo payload byte-a-byte):

```
⚠ EVIDENCE GUARD · DEGRADED
  · [verde-de-cache-nao-prova-medicao] 'forbidden_calls'
    → leia o stderr e o exit code, não só o stdout
```

*3235 ocorrencias | 143 bytes | hook `PostToolUse:Bash` | evento `PostToolUse` | sessao `24006143`*

**Julgamento.**

- **Prosa: FRASE.** `⚠ EVIDENCE GUARD · DEGRADED` + `· [verde-de-cache-nao-prova-medicao] 'forbidden_calls'` +
  `→ leia o stderr e o exit code, nao so o stdout` — tres linhas, cada uma com verbo.
- **Acao: SIM.** A seta `→` e a acao concreta a tomar.
- **Especificidade: MEDIA.** O padrao (`verde-de-cache-nao-prova-medicao`) e nomeado e o token (`'forbidden_calls'`)
  ancorra a leitura — mas e o mesmo para todo comando que tenha essa string.
- **Derivavel: BAIXA.** O modelo nao sabe ler exit code por habito; e exatamente o que se quer instalar.
- **Defeito medido: REPETICAO BRUTA.** 221 payloads distintos, 4.874 blocos. Os bytes em repeticao
  (710.751) sao **15x** os bytes distintos (47.095) — a familia repete 4.874 vezes 21 rules que cabem em 47 KB.
- **O paradoxo `DEGRADED`:** o proprio bloco anuncia que o guard esta degradado, e ainda assim e o quarto
  maior consumidor de contexto em numero de repeticoes. Um guard degradado que repete 4.874 vezes e um guard
  que nao esta economizando o token que promete economizar.
- **Custo: 47 KB distintos, 4.874 blocos.**

---

### 2.9 `code-mode-deny` — dois blocos com naturezas opostas

| n | distintos | bytes | mediana | media | max |
|---|---|---|---|---|---|
| 2782 | 1198 | 761.870 | 440 B | 636 B | 14342 B |

O bloco de telemetria (o que o modelo le no PreToolUse):

```
[G5 telemetria] rajada de 3 edits terminou sem build/teste — P9 medido em 17%. Valide agora (cargo check/test no crate tocado); a regressão silenciosa nasce aqui.
```

*292 ocorrencias | 165 bytes | hook `PreToolUse:Bash` | evento `PreToolUse` | sessao `24006143`*

E o contrato de sessao (1.697 bytes, emitted uma vez por sessao):

```
[CODE MODE — contrato desta sessão · apresentação `code` (/home/gabrielgadea/projects/analise/.touring/touring.toml (projeto))]
Efeito: inspeção ISOLADA PASSA — de qualquer classe. O que colapsa é a RAJADA: a 2ª chamada de `grep`/`cat`/`find`/`ls`/`wc`/`sed-n` dentro de 300s volta NEGADA com as duas fundidas em 1 programa (a 1ª já executou intacta). Calibrado por medição em 115 transcripts: 77,5%  ...
```

*334 ocorrencias | 1697 bytes | hook `SessionStart:startup` | evento `SessionStart` | sessao `0215c306`*

**Julgamento.**

- **Prosa: a telemetria e FRASE** — `[G5 telemetria] rajada de 3 edits terminou sem build/teste - P9 medido em 17%.
  Valide agora (cargo check/test no crate tocado); a regressao silenciosa nasce aqui.` Tem gatilho, medicao, acao e
  justificativa. E o melhor bloco de `post` do corpus.
- **Prosa: o contrato de sessao e MANUAL** — 1.697 bytes de prosa tecnica (calibracao, tabela de gates,
  precedence) lidos **uma vez por sessao**, no `SessionStart`. O bloco conta como 334 ocorrencias no corpus,
  mas por sessao ele e 1. O custo real e 1,7 KB por sessao, nao 1,7 KB por bloco.
- **Acao: SIM** nos dois casos. O contrato descreve o que o gate faz — o modelo precisa saber para nao Surpreender-se.
- **Especificidade: ALTA** no `G5` (conta quantos edits, sem build); **GENERICA** no contrato (vale para toda sessao).
- **Derivavel: NULA** nos dois — nenhum dos dois e descobrivel pelo modelo.
- **Custo: 761 KB**, mas 1.233.559 bytes (161%) sao repeticao byte-a-byte do contrato de sessao.

---

### 2.10 `pipefail-evidence` — quase perfeito

| n | distintos | bytes | mediana | media | max |
|---|---|---|---|---|---|
| 1962 | 318 | 67.205 | 204 B | 211 B | 1016 B |

Payload tipico:

```
`.venv/bin/python3` exited 0, but its output shows errors after a pipe without `set -o pipefail` — the real exit code may be masked (suspected; not recorded as a failure).
```

*328 ocorrencias | 173 bytes | hook `PostToolUse:Bash` | evento `PostToolUse` | sessao `042745e8`*

**Julgamento.**

- **Prosa: FRASE.** `touring exited 0, but its output shows errors after a pipe without set -o pipefail - the real
  exit code may be masked (suspected; not recorded as a failure).` Sintoma, causa e ressalva de confianca.
- **Acao: IMPLICITA mas clara** — o bloco nomeia `set -o pipefail` e o modelo sabe o que fazer.
- **Especificidade: ALTA** — nomeia o comando que saiu (`touring`, `python3`, `.venv/bin/python3`).
- **Derivavel: BAIXA** — o exit code mascarado e invisivel ao modelo sem o hook.
- **Honestidade:** o paragrafo diz *(suspected; not recorded as a failure)*. Diz ao modelo o que **nao** sabe.
  Isso e raro no corpus e e o que torna o bloco confiavel.
- **Custo: 67 KB.** Mediana 204 B, 318 distintos para 1.962 blocos.

---

### 2.11 `plugin-hook` — o dashboard

| n | distintos | bytes | mediana | media | max |
|---|---|---|---|---|---|
| 1247 | 863 | 580.502 | 497 B | 673 B | 1840 B |

O painel ATZILUTH:

```
🌅 ATZILUTH — Painel de Emanação (o estado do mundo, classificado)
Nada crítico vivo — campo limpo para criar.
🪞 Espelho (F4): n=5 medições · antecipação média 0.429 · últimas 5: 0.429 · mais ausentes: emanacao, imagem, fronteira
🔁 Isomorfismo? harness (faltou: cadeia, custo-invisivel, emanacao, fronteira, pronto) × codigo (faltou: cadeia, custo-invisivel, emanacao, fronteira, imagem) — mesma forma ...
```

*58 ocorrencias | 606 bytes | hook `SessionStart:startup` | evento `SessionStart` | sessao `0321ec39`*

**Julgamento.**

- **Prosa: CACO.** `🌅 ATZILUTH - Painel de Emanacao (o estado do mundo, classificado)` + `Nada critico vivo - campo
  limpo para criar.` + `🪞 Espelho (F4): n=5 medicoes · antecipacao media 0.429 · ultimas 5: 0.429 · mais ausentes:
  emanacao, imagem, fronteira` — sao tres paineis diferentes empilhados, cada um com metrica propria, sem verbo comum.
  O modelo nao sabe o que fazer com `Isomorfismo? harness (faltou: cadeia, custo-invisivel, emanacao, fronteira,
  pronto) x codigo (faltou: ...) - mesma forma`.
- **Acao: NAO.** O painel **informa** o estado de um sistema que o modelo nao opera.
- **Especificidade: GENERICA** — o painel e o mesmo para toda sessao do projeto.
- **Derivavel: NULA** (o dado e novo) mas tambem **INUTIL** — o modelo nao tem poder sobre `emanacao`/`fronteira`.
- **Defeito medido: leitura degenerada.** 137 blocos trazem `Doctor **0/0 OK**` e 234 trazem `composite **0.00**` —
  uma leitura de saude que **nao mediu nada** (0 de 0) e ainda assim alarma: `composite 0.00 < 0.7 -> touring e2e -j`.
  O bloco diz ao modelo que algo esta errado quando o denominador e zero.
- **Custo: 580 KB**, mediana 497 B — o maior corpo de prosa por bloco depois do `pre-edit`.

---

### 2.12 `compact-warning` — a melhor prosa do corpus

| n | distintos | bytes | mediana | media | max |
|---|---|---|---|---|---|
| 426 | 11 | 2.644 | 282 B | 240 B | 282 B |

Payload mais frequente:

```
Contexto em 55% — a autocompactação dispara aos 65%. Grave agora o estado em disco (RETOMADA do bundle, `touring decompose`, nó no grafo) para que a retomada não dependa do resumo. Se a próxima tarefa for distinta desta, prefira /clear e reconstrução pelo disco a /compact.
```

*116 ocorrencias | 282 bytes | hook `PostToolUse:Edit` | evento `PostToolUse` | sessao `2e4db816`*

E o caso limite:

```
Contexto em 95% (threshold: 95%). Considere /compact.
```

*21 ocorrencias | 53 bytes | hook `PostToolUse:Bash` | evento `PostToolUse` | sessao `24006143`*

**Julgamento.**

- **Prosa: FRASE PERFEITA.** `Contexto em 55% - a autocompactacao dispara aos 65%. Grave agora o estado em disco
  (RETOMADA do bundle, touring decompose, no no grafo) para que a retomada nao dependa do resumo. Se a proxima tarefa
  for distinta desta, prefira /clear e reconstrucao pelo disco a /compact.` — numero atual, limite, acao concreta,
  **motivo** (`para que a retomada nao dependa do resumo`) e **alternativa** (`/clear` em vez de `/compact`).
  Um humano le uma vez e sabe o que fazer. E o padrao a replicar.
- **Acao: SIM, com o COMO.** Nao apenas `grave o estado` — diz *onde* (`RETOMADA do bundle`, `touring decompose`,
  `no no grafo`) e *por que*.
- **Especificidade: ALTA** — o percentual e o limiar mudam a cada bloco; so o miolo e fixo.
- **Derivavel: NULA** — o modelo nao sabe quanto de contexto resta.
- **Custo: 2.644 bytes no total** — 11 payloads distintos para 426 blocos. A familia mais barata do corpus,
  e a de melhor prosa. E o **melhor par custo/qualidade medido**.

---

### 2.13 `session-contract` — a ordem de inicializacao

| n | distintos | bytes | mediana | media | max |
|---|---|---|---|---|---|
| 193 | 4 | 528 | 132 B | 132 B | 211 B |

Payload mais frecuente:

```
MUST    touring doctor -j
  MUST    touring status -j
  SHOULD  touring e2e -j
  SHOULD  touring gate-metrics -j
  (orçamento do turno esgotado — só as diretivas; `TOURING_TURN_BUDGET=<bytes>` ajusta o teto)
```

*148 ocorrencias | 211 bytes | hook `PreToolUse:Bash` | evento `PreToolUse` | sessao `24006143`*

**Julgamento.**

- **Prosa: FRASE-lista.** Cada linha e um comando completo (`MUST touring doctor -j`), e a lista inteira
  (`doctor`, `status`, `e2e`, `gate-metrics`) e o **pre-flight** do protocolo — o mesmo do `CLAUDE.md`.
- **Acao: SIM.** Os quatro comandos sao exatamente os que o reflexo 1 manda rodar.
- **Especificidade: GENERICA** — nao muda com o arquivo nem com o comando; e o pre-flight da sessao.
- **Derivavel: NULA** (o modelo nao tem o estado do daemon) mas **pre-derivavel**: e knowledge, nao contexto.
- **Defeito medido:** 4 payloads distintos para 193 blocos. O sufixo de orcamento
  `(orcamento do turno esgotado - so as diretivas; ...)` aparece em **163** deles, reduzindo o pre-flight a
  `MUST touring doctor -j` + `MUST touring status -j` — o pre-flight truncado e pre-flight partido.
- **Custo: 528 bytes.** O menor do corpus. Justamente o que deveria ser.

---

### 2.14 `pre-edit-prevention` — a unica com acao de verdade

| n | distintos | bytes | mediana | media | max |
|---|---|---|---|---|---|
| 175 | 166 | 29.956 | 182 B | 180 B | 337 B |

Payload de ANTIPATTERN:

```
Pre-edit prevention for /home/gabrielgadea/projects/touring/docs/plans/2026-10-08-vgp-motor-base/scripts/vivos.py (1 signal):
ANTIPATTERN: [python] `print()` — debug output left in production code (2x)
```

*2 ocorrencias | 203 bytes | hook `PreToolUse:Edit` | evento `PreToolUse` | sessao `10fb71c6`*

E o de NEAR_DUPLICATE:

```
Pre-edit prevention for /home/gabrielgadea/projects/touring/scripts/test_drift_coverage.py (1 signal):
NEAR_DUPLICATE: edit is 100% similar to original (AST-token Jaccard) — did you mean to change more?
```

*3 ocorrencias | 204 bytes | hook `PreToolUse:Edit` | evento `PreToolUse` | sessao `8614d1f9`*

**Julgamento.**

- **Prosa: FRASE.** `ANTIPATTERN: [python] print() - debug output left in production code (2x)` — rotulo, linguagem,
  simbolo, explicacao e contagem. `NEAR_DUPLICATE: edit is 100% similar to original (AST-token Jaccard) - did you
  mean to change more?` — metodo de medicao e **pergunta ao modelo**.
- **Acao: SIM.** E a unica familia que **interroga** o modelo: *did you mean to change more?*
- **Especificidade: ALTA** — arquivo, linguagem, simbolo, ocorrencias.
- **Derivavel: BAIXA** — o modelo nao sabe que ha 2 `print()` no arquivo nem que o edit e 100% identico ao original.
- **O problema e o TIMING.** Esta e a familia `pre` mais acionavel do corpus, e ela chega tarde:
  de **700** blocos `Pre-edit prevention` observados, **680** vem **depois** que o `Edit` ja foi aplicado
  (`The file ... has been updated successfully`), **20** vem antes de outra ferramenta, **0** impedem a edicao.
  O aviso e post-facto.
- **Redundancia:** `pre-edit` e `pre-edit-prevention` coexistem no mesmo turno **8 vezes** — o `pre-edit` maior
  absorve o prevention (3.028 dos 17.795 payloads de `pre-edit` comecam com `Pre-edit prevention`, porque a
  taxonomia casa `related docs:` antes de `Pre-edit prevention`).
- **Custo: 30 KB** — familia pequena, e por isso a que mais sofre com oAbsentismo: 4.057 blocos (3.028 distintos)
  estao **dentro** de `pre-edit`, e `pre-edit-prevention` propria so tem 175 blocos.

---

### 2.15 `pre-read` — o mesmo caco, com o arquivo inteiro

| n | distintos | bytes | mediana | media | max |
|---|---|---|---|---|---|
| 27 | 27 | 31.559 | 1170 B | 1169 B | 1961 B |

Payload tipico:

```
📌 defs[123]: SEMENTE(41)·SAIDA_PADRAO(42)·LINGUAGENS(43)·EXT(44)·SUFIXO(45)·ANALISE(47)·FRONTEND(48)·PORTAL(49)·+115 | ⚠️ High complexity: gera_codigo(CC=14), gera_html(CC=11); quality: CC_max=14, CC_avg=3.3, symbols=123, complex=2 | 💡 ~935 linhas: `touring ast overview corpus_semeado.py` mapeia sem ler completo (~85% tokens) | 💡 `touring ast overview corpus_semeado.py` mapeia sem ler completo (~8 ...
```

*1 ocorrencias | 1162 bytes | hook `PreToolUse:Read` | evento `PreToolUse` | sessao `2e4db816`*

**Julgamento.**

- **Prosa: CACO.** `📌 defs[123]: SEMENTE(41)·SAIDA_PADRAO(42)·LINGUAGENS(43)·...·+115 | ⚠️ High complexity:
  gera_codigo(CC=14), gera_html(CC=11); quality: CC_max=14, CC_avg=3.3, symbols=123, complex=2 | 💡 ~935 linhas:
  touring ast overview corpus_semeado.py mapeia sem ler completo (~85% tokens) | ...` — e a mesma linha de pipes do
  `pre-edit`, com os mesmos emoji, mas agora listando **os simbolos do arquivo que o modelo vai ler**.
- **Acao: SIM, mas indirecta.** `💡 touring ast overview X mapeia sem ler completo (~85% tokens)` manda o modelo
  trocar a leitura pelo comando — e o `pre_read.rs:5` (`Is it actionable?`) cumprido. O `~85% tokens` quantifica o ganho.
- **Especificidade: ALTA.**
- **Derivavel: **ALTA** — e o pior caso do corpus.** `defs[123]` lista os simbolos do arquivo com numero de linha
  (medido: **29,8%** dos bytes da familia sao `FILE_ECHO`). O `pre_read.rs:6` diz explicitamente:
  *"Claude WON'T find this by reading the file"* — mas aqui o hook entrega o que a leitura daria, antes da leitura.
  E o `overview:` do `pre-edit` faz o mesmo (13,6% dos bytes).
- **Contradição interna:** o proprio bloco diz `~935 linhas: touring ast overview X mapeia sem ler completo (~85% tokens)`
  — ou seja, *nao leia o arquivo, use o comando* — **depois** de ter delivered 123 simbolos dele. O bloco se cancela.
- **Custo: 31 KB em 27 blocos** — familia rarissima (o `Read` e raro no corpus), e cada bloco e caro (mediana 1.170 B).

---

### 2.16 `debug-stdout` — lixo de debug

| n | distintos | bytes | mediana | media | max |
|---|---|---|---|---|---|
| 8 | 7 | 2.966 | 295 B | 424 B | 922 B |

Payload tipico:

```
`F=$PWD/backend/app/services/learning_service.py;` failed (exit 1): [POST_QUALITY_GATE_V4-ERROR] Failed - 3 errors
[generic]
temporal: reliability 88% (7d), velocity 470.9 edits/day — drift detected
⚡ last fail: F=$PWD/backend/app/services/learning_service.py; — [POST_QUALITY_GATE_V4-ERROR] Failed - 3 errors, systemd-run — E   AssertionError: assert 'RlmMemory indisponível, ranking padrão' in '',  ...
```

*1 ocorrencias | 615 bytes | hook `PostToolUse:Bash` | evento `PostToolUse` | sessao `83f700cf`*

**Julgamento.**

- **Prosa: CACAO.** `⚡ last fail: echo - [POST_QUALITY_GATE_V4-ERROR] Failed - 2 errors` e
  `temporal: reliability 88% (7d), velocity 470.9 edits/day - drift detected` — duas mensagens de sistema
  **sobre outros comandos** (`echo`, `systemd-run`) que o modelo nunca executou nesta sessao.
- **Acao: NAO.** `temporal: velocity 470.9 edits/day` nao habilita nenhuma operacao.
- **Especificidade: ZERO** para a sessao.
- **Custo: 2.966 bytes em 8 blocos.** Volume irrelevante — mas revela que existe um caminho de debug que
  chega ao contexto do modelo.

---

### 2.17 `outros` — a familia que a taxonomia nao fechou

**10.400** blocos, **2,0 MB**, `distinct=5.849`, `mediana=92 B`. Nao e uma familia: e o resto. O que cai nela:

```
[G2 exit-code-through-pipe → REESCRITO] `$?` após pipe leria o status do último estágio; `set -o pipefail; ` prefixado (spike S-8.6: updatedInput honrado).
[G8 laço-de-inspeção → REESCRITO] o laço já É o programa; uma varredura no sandbox devolve só o agregado (Anthropic CodeAct). Kill switch humano: TOURING_G8_REWRITE_DISABLED=1.
[pytest] passed=0 failed=0 errors=0
[ruff] errors=0 fixable=0
[M3 delegação] 10º arquivo DISTINTO lido nesta janela — exploração desta largura paga o isolamento de contexto: delegue a 1-3 subagentes read-only (Explore/general-purpose), cada um com objetivo + formato de saída + fronteiras, e receba só as conclusões (50 arquivos lidos voltam como 3 achados). Leitura larga paraleliz ...
```

**Julgamento.** O `[G2]` e o `[G8]` sao **os dois melhores blocos de todo o corpus** — frase, verbo, causa, mecanismo,
**kill switch humano** e a referencia de spike. Sao as razoes de `permissionDecisionReason` do code mode: o gate
reescreveu a chamada e **explicou por que**. Nenhum outro bloco do corpus tem essa propriedade.

Porem, como estao classificados em `outros`, a rodada 3 nao os mede: eles ficam fora de qualquer familia,
fora do relatorio de conformidade e fora de qualquer veredito. **O melhor bloco do sistema esta na familia que
a taxonomia nao nomeia.** Isso e um achado do instrumento, nao do conteudo.

Os outros (`[pytest] passed=0 failed=0 errors=0`, `[ruff] errors=0 fixable=0`) sao resultado de check sem
achado — 210 e 135 blocos, respectivamente — e sao ruigo puro.

---

## 3. Os 5 piores blocos

Ordem por dano: bytes injetados x (nao-acionabilidade) x redundancia. Todos verbatim do corpus.

### 3.1 `generic-noise` — `[generic]`

```
[generic]
```

*7.024 ocorrencias | 9 bytes | hook `PostToolUse:Bash` | evento `PostToolUse`*

**Defeito.** Nao e prosa: e um marcador interno de classificador vazado para o contexto do modelo. Nao afirma nada
sobre o comando, o arquivo ou o resultado. Repete **7.024 vezes** (95% da familia) com byte identico. E o unico
payload do corpus que **nao contem uma unica palavra em portugues ou ingles** — e ainda assim ocupa uma posicao na
janela de contexto do modelo. Pior: ele chega **antes** do conteudo util no mesmo payload (ver 3.2).

**O que deveria estar la.** Nada. Este e o caso limite do contrato (`pre_read.rs:3`): *SILENCE IS THE DEFAULT*. Um
hook que nao tem nada a dizer deveria devolver `Allow` sem `additionalContext`. Se a ausencia de informacao precisa
ser sinalizada, o sinal tem de ser **uma palavra** (`ok`), nao um rotulo de taxonomia interna.

### 3.2 `post-edit` — telemetria de uma medicao que ninguem pediu

```
post-write: ℹ ⚙ health-delta: first-observation new=1.00
```

*1.553 ocorrencias | 60 bytes | hook `PostToolUse:Write` | evento `PostToolUse`*

**Defeito.** Informa uma metrica interna do sistema de saude (`health-delta`) que o modelo nao pediu, nao pode
verificar e nao pode usar para nada. Nao ha verbo, nao ha alvo, nao ha consecuencia. **6.543** blocos sao
exatamente isto (so `health-delta`, sem nenhum achado). Num payload que costuma vir acompanhado de achados reais
de qualidade, este **enterra o achado atras da telemetria**: o modelo le `health-delta` (60 B) antes de ler
`🚨 COMPLEXITY: parallel_groups has CC=19 (threshold=15, excess=4)` — que e a unica parte acionavel.

**O que deveria estar la.** So o achado, e so quando ha achado. `post-write: ✓` quando nao ha nada (uma palavra),
e o `🚨 COMPLEXITY: ... (threshold=15, excess=4)` quando ha. A telemetria de `health-delta` pertence a um log, nao
a um canal que o modelo le.

### 3.3 `similar-fail` — o fato sem sujeito

```
Similar command failed recently (similarity=62%)
```

*893 ocorrencias | 48 bytes | hook `PreToolUse:Bash` | evento `PreToolUse`*

**Defeito.** A frase termina antes do sujeito. **9.161** blocos (73% da familia) sao **exatamente** isto — so a
similaridade, nenhum comando. Dos 2.638 payloads distintos, so **670 (25%)** nomeiam algum comando, e mesmo esses
nomeiam o **binario** (`touring`, `python3`), nao a invocacao. O modelo recebe um numero de similaridade sem
saber com o que compara. O `pre_bash.rs:3` promete *"Only inject when a RELEVANT past failure exists"* — e o
payload **nao transporta a relevancia**: o nome do comando que falhou nao entra.

**O que deveria estar la.** `touring doctor -j` falhou com `JSONDecodeError` — o `?` na saida pode ser o
`JSONDecodeError`. Ou: `Similar a 'touring e2e -j' que falhou ha 3 min (exit 1, JSONDecodeError)`. O **nome do
comando que falhou** e o `?` — sem ele o bloco nao passa no `pre_bash.rs:6`.

### 3.4 `pre-edit` — a linha de pipes

```
HOTSPOT: `classify` has 40 callers | callers: [test_classify_debug, test_classify_code, test_classify_test, test_classify_refactor, test_classify_analysis (+35 more)] | note: Exit 0 (OK), 37 lines output (1088 bytes); bash_metrics[generic]: byte_count=1088 exit_code=0 line_c | 5x edited recently | 📋 overview: (87fn, 0types, 1308L) [fn test_classify_debug:L6-10, fn test_classify_code:L13-16, fn tes
```

*(primeiros 400 caracteres do payload, corte literal; o payload tem 2.121 bytes)*

E o **unico** item acionavel, na posicao 1.709 de 2.121 (o ultimo):

```
[C08] `classify_with_details` changes here and has 6 other call sites in 2 files: crates/touring-hook-runtime/src/prompt_enhance.rs:226, crates/touring-hook-runtime/src/prompt_enhance.rs:953, crates/touring-hooks/tests/e2e_r1_to_r20.rs:485, crates/touring-hooks/tests/e2e_r1_to_r20.rs:490 (+2) — the same change may apply there (cross-caller compare) | [rust] semantic 0.46 · pub API 0 · unsafe 23
```

*(fragmento final do mesmo payload, verbatim)*

*4.550 blocos com `HOTSPOT` | 2.121 bytes neste payload | hook `PreToolUse:Edit` | evento `PreToolUse`*

**Defeito.** E o maior consumidor do corpus (21,4 MB) e o menos legivel. Quatro defeitos somados no mesmo payload:

1. **cacOS**: segmentos separados por `|`, cada um autossuficiente, sem verbo comum. O modelo tem de remontar a
   proposicao a partir de sete fragmentos. Um humano nao le 2.121 bytes assim para decidir uma acao;
2. **derivavel**: `📋 overview:` (13,6% dos bytes da familia) e a **assinatura do arquivo** — 71 simbolos com
   numero de linha, que e o que o modelo obtem abrindo o arquivo. `pre_read.rs:6` proibe isso explicitamente;
   medido: **19,8%** dos bytes de `pre-edit` sao `FILE_ECHO`;
3. **eco do comando**: `note: Exit 0 (OK), 37 lines output (1088 bytes); bash_metrics[generic]: byte_count=1088
   exit_code=0` — o modelo **acabou de ver essa saida**; repetir e Derivavel por construcao;
4. **irrelevante**: `project: healthy | Analysis: health=0.81 (healthy) | trend=Degrading | orphans=1294` — a
   saude do projeto nao muda a decao sobre este edit.

O bloco tem **um** item acionavel (`[C08] ... the same change may apply there`) e ele chega **na posicao 20 de 21**.

**O que deveria estar la.** `classify` tem 40 chamadores, 35 deles em testes. `classify_with_details` tem 6 call
sites em 2 arquivos (`prompt_enhance.rs:226`, `:953`, `e2e_r1_to_r20.rs:485`, `:490`) — a mesma mudanca pode
aplicar-se la. **Tres linhas, 700 bytes, tudo nao-derivavel.** O resto do payload e o que o modelo le sozinho.

### 3.5 `code-mode-contract` — a ordem que nao fecha

```
MUST    touring run --lang bash --code '.venv/bin/python3 - <<'\''EOF'\''
  (orçamento do turno esgotado — só as diretivas; `TOURING_TURN_BUDGET=<bytes>` ajusta o teto)
```

*168 ocorrencias | 172 bytes | hook `PreToolUse:Bash` | evento `PreToolUse`*

**Defeito.** E uma ordem `MUST` **quebrada**: o programa fica aberto no meio do heredoc, sem o `EOF` final, sem a
aspa de fecho. **1.380** blocos (784 distintos) terminam assim e **3.275** (1.919 distintos) tem citacao impar —
o programa **nao executa** se copiado. E o pior tipo de defeito: o bloco *parece* uma instrucao (tem `MUST`, tem
o comando, tem o shebang do sandbox) e por isso o modelo pode tentar obedecer e falhar. O sufixo
`(orcamento do turno esgotado — so as diretivas)` admite o corte, mas nao diz **o que** foi cortado.

**O que deveria estar la.** `MUST touring run --lang bash --code 'cat ARQUIVOS | wc -l'` — completo e executavel.
Se o orcamento nao couber o programa inteiro, o certo e **nao dar a ordem** (o `SILENCE IS THE DEFAULT` do
contrato), nao dar a ordem partida.

---

## 4. Os 5 melhores blocos

### 4.1 `compact-warning` — a prosa de referencia

```
Contexto em 55% — a autocompactação dispara aos 65%. Grave agora o estado em disco (RETOMADA do bundle, `touring decompose`, nó no grafo) para que a retomada não dependa do resumo. Se a próxima tarefa for distinta desta, prefira /clear e reconstrução pelo disco a /compact.
```

*116 ocorrencias | 282 bytes | hook `PostToolUse:Edit` | evento `PostToolUse`*

**O padrao.** Quatro elementos, nessa ordem: **(1) o numero atual e o limiar** (`55%`, `65%`); **(2) o verbo e o
onde** (`Grave agora o estado em disco`, `RETOMADA do bundle`, `touring decompose`, `no no grafo`); **(3) o
motivo** (`para que a retomada nao dependa do resumo`); **(4) a alternativa** (`Se a proxima tarefa for distinta
desta, prefira /clear`). Um humano le uma vez e sabe o que fazer — e sabe o que **nao** fazer. Nao-derivavel
(o modelo nao sabe quanto de contexto resta), especifico (o percentual muda), acionavel (verbo + onde + como).
Custo: **2.644 bytes** para 426 blocos — a familia mais barata e a de melhor prosa. **E o padrao a replicar.**

### 4.2 `pipefail-evidence` — a frase que nomeia o sintoma, a causa e a ressalva

```
`touring` exited 0, but its output shows errors after a pipe without `set -o pipefail` — the real exit code may be masked (suspected; not recorded as a failure).
```

*1.962 blocos | 163 bytes | hook `PostToolUse:Bash` | evento `PostToolUse`*

**O padrao.** Nomeia o **comando exato** que saiu (`touring`), o **sintoma** (`exited 0` + `output shows errors`),
a **causa** (`after a pipe without set -o pipefail`), o **efeito** (`the real exit code may be masked`) e a
**ressalva** (`suspected; not recorded as a failure`). Esse ultimo paragrafe e o que torna o bloco confiavel: o
modelo sabe **o que o hook nao sabe**. Nao ha emoji, nao ha pipe, nao ha fragmento. Uma frase, 163 bytes.

### 4.3 `pre-edit-prevention` — a unica que faz uma pergunta

```
Pre-edit prevention for /home/gabrielgadea/projects/touring/scripts/test_drift_coverage.py (1 signal):
NEAR_DUPLICATE: edit is 100% similar to original (AST-token Jaccard) — did you mean to change more?
```

*175 blocos | 204 bytes | hook `PreToolUse:Edit` | evento `PreToolUse`*

**O padrao.** E o unico bloco do corpus que **pergunta** ao modelo: *did you mean to change more?* — em vez de
afirmar, convida a reconsiderar. Nomeia o **arquivo**, o **sinal** (`NEAR_DUPLICATE`), o **metodo** da medicao
(`AST-token Jaccard`) e o **grau** (`100% similar to original`). E o unico com **acao de verdade** por familia:
98,8% de conformidade na rodada 3, contra 0,3% do `touring-suggest`. O defeito e de **timing**, nao de conteudo
(ver 6.b).

### 4.4 `code-mode-deny` — o bloco que fecha o laco

```
[G5 telemetria] rajada de 3 edits terminou sem build/teste — P9 medido em 17%. Valide agora (cargo check/test no crate tocado); a regressão silenciosa nasce aqui.
```

*292 ocorrencias | 165 bytes | hook `PreToolUse:Bash` | evento `PreToolUse`*

**O padrao.** Cinco elementos em 165 bytes: **gatilho** (`rajada de 3 edits terminou sem build/teste`), **medida**
(`P9 medido em 17%`), **acao com comando** (`cargo check/test no crate tocado`), **verbo no imperativo**
(`Valide agora`), e **motivo** (`a regressao silenciosa nasce aqui`). O modelo nao pode saber que esta em 3 edits
sem build — e nao pode saber que isso e medido em 17% dos casos. Nao-derivavel, acionavel, especifico, em uma
frase.

### 4.5 `outros` — o melhor bloco do sistema, na familia que a regua nao ve

```
[G2 exit-code-through-pipe → REESCRITO] `$?` após pipe leria o status do último estágio; `set -o pipefail; ` prefixado (spike S-8.6: updatedInput honrado).
```

*1.380 ocorrencias | 160 bytes | hook `PreToolUse:Bash` | evento `PreToolUse`*

**O padrao.** E a razao de um **deny**, nao de um contexto. Tem: o **rotulo do gate** (`G2`), o **mecanismo**
(`$?` apos pipe leria o status do ultimo estagio), a **correcao concreta** (`set -o pipefail;` prefixado), a
**referencia de spike** (`spike S-8.6: updatedInput honrado`) e — o mais raro do corpus — a **confissao de que
o hook reescreveu** (`-> REESCRITO`). O modelo sabe exatamente o que aconteceu com a chamada e por que. E o
`[G8]` acrescenta o **kill switch humano** (`TOURING_G8_REWRITE_DISABLED=1`) e a **referencia a pratica**
(`Anthropic CodeAct`).

Este bloco e a **prova de que o sistema sabe escrever bem** — e de que o defeito nao e do motor de prosa, e da
**politica de injecao**: o texto de maior qualidade vive no canal de `permissionDecisionReason`, que a rodada 3
classifica como `outros` e nao mede.

---

## 5. A regra de qualidade, extraida do corpus

Os cinco melhores blocos tem algo em comum que os piores nao tem. O padrao **repetido** pelos melhores:

> **um numero** (o estado) + **um verbo no imperativo** (a acao) + **um porque** (a razao) — em **uma frase**,
> **sem pipe**, nomeando o **alvo**.

O `pre_read.rs:3-7` ja diz isso em tres perguntas (*actionable? non-derivable? specific to THIS file?*), mas tres
perguntas nao sao uma regra executavel: nao dizem **quanto** e **onde**. O corpus diz.

### A regra operacional

> **Injete um bloco so quando ele trouxer, em uma unica frase: (1) o estado medido, com o numero; (2) a acao,
> com o verbo no imperativo e o alvo nomeado; (3) o porque — o que quebra se o modelo nao agir.**
>
> **E cale quando:** o fato ja esta na tela (o modelo acabou de ve-lo); o fato e o conteudo do arquivo (o modelo
> vai abrir o arquivo); o fato nao tem consequencia nenhuma (health-delta, orphans=1294, project: healthy); ou
> o bloco nao cabe em uma frase (a linha de pipes).
>
> **E nunca injete:** um marcador interno de taxonomia (`[generic]`); um numero sem sujeito (`similarity=62%`);
> uma ordem que nao executa (programa truncado no meio); ou o mesmo texto reemitido sem novelty.

### Como um hook decide com ela

Tres testes, nesta ordem, sobre o payload **pronto**:

| # | teste | operacao | passa se |
|---|---|---|---|
| 1 | **tem verbo no imperativo?** | procurar `MUST`/`SHOULD`/`MAY`/imperativo (`Grave`, `Valide`, `Delgue`, `rode`) | ha pelo menos **um**, com alvo nomeado |
| 2 | **o fato e novo?** | o bloco repete algo que o modelo acabou de ver (a saida do comando, o conteudo do arquivo, o estado global do projeto)? | **nao** repete |
| 3 | **cabe em uma frase?** | o bloco tem `|` como separador de proposicoes independentes? | **nao** tem (ou: cada segmento tem verbo proprio) |

Teste 1 e o que separa `pre-edit` (1,0% de bytes `DIRECTIVE`, 75,6% de conformidade por **distal_tocou** — o modelo
tocou o arquivo, nao por causa do bloco) de `pre-edit-prevention` (53% `PROSE`, 0% `DIRECTIVE`, 98,8% de conformidade). Teste 2
e o que separa `post-edit` (6.543 blocos de eco) de `post-tool-echo` (acao com comando). Teste 3 e o que separa
`compact-warning` (uma frase) de `pre-edit` (21 pipes).

**Se os tres passarem, injete; se algum falhar, cale.** O teste 3 e o que o `pre_read.rs` nao tem: e o que impede
que a contagem de conformidade engane — `pre-edit` passa nos testes 1 e 2 e mesmo assim e 21 MB de caco.

### Reconciliacao com as taxas medidas

| familia | bytes `DIRECTIVE` | prosa | conformidade (rodada 3) | o que a regra diz |
|---|---|---|---|---|
| `pre-edit` | **1,0%** | caco (21 pipes) | 75,6% (por `distal_tocou`) | **calar**: sem verbo, 48,3% `NOISE`, nao cabe em uma frase |
| `touring-suggest` | **79,2%** | frase por linha | **0,3%** | **manter o formato, mudar a entrega**: a ordem esta certa e nao e obedecida — porque chega no `additionalContext`, que a doc define como *"contexto da aplicacao"*, nao ordem do operador |
| `pre-edit-prevention` | 0,0% (53% `PROSE`) | frase | **98,8%** | **manter**; corrigir so o **timing** (ver 6.b) |
| `compact-warning` | 0,0% | frase | 46,3% | **manter** — e o padrao |
| `evidence-guard` | 50,4% | frase | 0,0% | **manter o texto, calar a repeticao**: 4.874 blocos para 221 distintos |
| `code-mode-contract` | 54,5% | ordem truncada | 0,0% | **nao dar ordem quebrada**: ou o programa inteiro, ou silencio |
| `generic-noise` | 0,0% | nao e prosa | 0,0% | **calar sempre** |

O caso do `touring-suggest` e o mais revelador da regra: ele tem a **melhor prosa** (79,2% de bytes sao ordem
completa, um comando por linha) e a **pior adesao** (0,3%). A regra explica por que: o teste 1 passa, o teste 2
passa, o teste 3 passa — e mesmo assim nao funciona. Porque **prosa boa nao e o gargalo; o canal e**. A doc
oficial (via Context7) diz que `additionalContext` e *contexto da aplicacao*: um **fato** nesse canal vira dado
disponivel; um **sermao** vira conselho de uma aplicacao que o modelo nao tem obrigação de obedecer. Um `MUST`
injetado por `additionalContext` e um sermao, nao uma ordem. Logo: **o `touring-suggest` so converte se passar do
contexto para o canal de enforcement** (`permissionDecision`/`systemMessage`), que e onde o `[G2]`/`[G8]` do code
mode vivem e onde a reescrita da chamada realmente acontece.

---

## 6. Achados que so a leitura qualitativa revela

### 6.a Redundancia entre familias: o mesmo arquivo, tres blocos, o mesmo turno

A contagem por familia separa `pre-edit` (19.062) de `touring-suggest` (16.422) de `post-edit` (12.373) e **nao
vê que eles falam do mesmo arquivo no mesmo turno**. Cruzando por turno:

| par de familias, mesmo caminho no mesmo turno | ocorrencias |
|---|---:|
| `pre-edit` + `touring-suggest` | **1.327** |
| `post-tool-echo` + `pre-edit` | **833** |
| `pre-edit` + `pre-edit` | **508** |
| `post-tool-echo` + `touring-suggest` | **498** |

E o numero de blocos por turno:

| blocos no mesmo turno | turnos |
|---|---:|
| 1 | 4.401 |
| 2 | 2.373 |
| 3 | 765 |
| 4 | 727 |
| 5 | 405 |
| 6+ | 65 |

**Metade dos turnos carrega 2+ blocos** — e em 2.500 desses turnos, dois ou mais blocos citam **o mesmo caminho**.
O pior caso medido (sessao `24006143`): **8 blocos e 12.446 bytes** injetados numa unica chamada de `Edit`, com o
mesmo `HOTSPOT: 'new' has 12 callers` repetido **tres vezes** (2.540 + 2.570 + 2.908 bytes) — a segunda com o
marcador `↑ repetido nesta sessao (ha 31s, 1538 B elididos)`, que **elide 1.538 bytes mas reenvia 2.570**, e a
terceira **sem qualquer marcacao**. O modelo le o mesmo fato tres vezes no mesmo turno, com corte parcial.

**Isto nao aparece em nenhuma contagem por familia.** O `pre-edit` e o `touring-suggest` nao sao duas informacoes:
sao **duas rotas para o mesmo fato** — uma por injecao (que entrega `📋 overview:` do arquivo) e outra por comando
(`MUST touring ast meta <arquivo>`). O modelo recebe a assinatura do arquivo **e** a ordem de pedir a assinatura do
arquivo.

### 6.b A ordem de leitura: o `additionalContext` do PreToolUse entra DEPOIS do prompt da ferramenta

Este e o achado que a contagem nao alcança, porque o instrumento conta blocos, nao posicoes.

Sequencia real dos registros no transcript (medida em 799 sessoes):

```
assistant/tool_use                <- o modelo JA emitiu a chamada (com o comando, o caminho, o old_string)
  ATT:hook_success:PreToolUse     <- o hook roda e escreve o stdout
  ATT:hook_additional_context:PreToolUse   <- o bloco "pre" entra AQUI
user/tool_result                 <- so agora o resultado volta
```
*(diagrama do fluxo de registros — reconstrucao minha, nao e payload)*

**3.391 de 3.391** blocos `hook_additional_context/PreToolUse` tem um `assistant/tool_use` emitido
imediatamente antes. O prefixo `Pre` nomeia o **evento do hook**, nao o momento da leitura. O bloco chega depois
que o modelo **ja escolheu** a chamada.

Consequencia medida na familia que mais precisa de timely-ness (`pre-edit-prevention`): de **700** blocos
`Pre-edit prevention` observados, **680** vem **depois** que o `Edit` ja foi aplicado
(`The file ... has been updated successfully`), **20** vem antes de outra ferramenta, e **ZERO** chegam antes da
execucao. O aviso de que a edicao e redundante chega **depois** de a edicao redundante estar no disco.

Isto reconcilia as taxas de conformidade da rodada 3, que de outro modo parecem contraditorias: `pre-edit-prevention`
tem **98,8%** de conformidade e `touring-suggest` tem **0,3%** — sendo que o segundo tem a prosa muito melhor. A
diferenca nao e de qualidade de texto: e de **momento**. `pre-edit-prevention` chega tarde, mas chega **depois** da
ferramenta e a **medida** de conformidade (`distal_tocou`) conta o uso do fato **depois** — e ele e usado. Ja o
`touring-suggest` chega no `additionalContext` (contexto da aplicacao, nao ordem) e nao obriga ninguem.

### 6.c A fracao derivavel, por familia

Medido pela classificacao de atomos (o bloco e partido em segmentos; cada segmento e julgado quanto a ser
derivavel pelo modelo lendo o arquivo ou vendo a saida do comando):

Uma **unica** medicao, aplicada igual em todas as familias: o payload e partido em segmentos (por `|` e por
linha) e cada segmento e classificado por **uma** regra. Amostra: 400 payloads distintos por familia
(600 para `pre-edit`). Percentual de **bytes**, nao de segmentos.

| familia | `FILE_ECHO` (le o arquivo) | `CROSS_FILE` (grafo) | `GLOBAL` (estado do projeto) | `DIRECTIVE` (acao) | `NOISE` | `PROSE` |
|---|---:|---:|---:|---:|---:|---:|
| `pre-edit` | 5,0% | **37,6%** | 0,9% | 1,0% | **48,3%** | 7,2% |
| `pre-read` | **29,8%** | **37,5%** | 7,5% | 8,3% | 17,0% | 0,0% |
| `touring-suggest` | 0,0% | 0,0% | 0,0% | **79,2%** | 20,8% | 0,0% |
| `post-edit` | 0,7% | 0,0% | 0,0% | 0,0% | **63,6%** | 35,7% |
| `post-tool-echo` | 0,0% | 0,0% | 0,0% | 0,0% | 32,6% | **67,4%** |
| `evidence-guard` | 0,0% | 0,0% | 0,0% | **50,4%** | 35,6% | 14,0% |
| `code-mode-contract` | 0,0% | 0,0% | 0,0% | **54,5%** | 45,5% | 0,0% |
| `pipefail-evidence` | 0,0% | 0,0% | 0,0% | 0,0% | **99,3%** | 0,7% |
| `generic-noise` | 0,0% | 0,0% | 13,7% | 0,0% | **83,7%** | 2,6% |
| `code-mode-deny` | 0,1% | 0,7% | 0,0% | 0,0% | **91,8%** | 7,5% |
| `plugin-hook` | 0,0% | 0,0% | 0,0% | 3,1% | **96,9%** | 0,0% |
| `compact-warning` | 0,0% | 0,0% | 0,0% | 0,0% | 0,0% | **100,0%** |
| `session-contract` | 0,0% | 0,0% | 0,0% | **62,2%** | 37,8% | 0,0% |
| `pre-edit-prevention` | 0,0% | 0,0% | 0,0% | 0,0% | 47,0% | **53,0%** |
| `similar-fail` | 0,0% | 0,0% | 0,0% | 0,0% | **69,9%** | 30,1% |
| `debug-stdout` | 0,0% | 0,0% | 15,3% | 0,0% | **83,8%** | 0,9% |

Uma **segunda** segmentacao, mais fina, foi usada **apenas** no `pre-edit` (600 payloads, o payload partido por
`|`, com o segmento classificado por origem semantica): nela o `pre-edit` aparece com **19,8%** de bytes
`DERIVAVEL (le o arquivo)`, **13,6%** de `📋 overview:` (a assinatura do arquivo), **3,9%** de `NAO-derivavel
(grafo)` (os `callers`), **3,5%** de `ACAO` e **1,1%** de eco da saida do comando. **As duas tabelas nao sao
somaveis** — sao instrumentos diferentes, com taxonomia diferente; por isso a tabela acima usa um so. A leitura
que as duas sustentam e a mesma: o `pre-edit` carrega muito `FILE_ECHO` e pouco `DIRECTIVE`.

Duas leituras:

1. **O `pre-edit` e o `pre-read` injetam derivavel.** 19,8% e 29,8% dos bytes sao o **conteudo do arquivo** que o
   modelo vai ler — o `📋 overview:` com 71 assinaturas e numero de linha. E o `pre_read.rs:6` proibe isso pelo
   nome (*"Claude WON'T find this by reading the file"* — aqui o hook entrega antes da leitura, e o proprio bloco
   se cancela: `~935 linhas: touring ast overview X mapeia sem ler completo (~85% tokens)`).
2. **O `pre-edit` e o `pre-read` injetam pouquissimo nao-derivavel.** O `CROSS_FILE` verdadeiro (os `callers`, o
   `C08`) e **6,2%** do `pre-edit` e 37,5% do `pre-read`. Todo o resto ou e o arquivo (19,8%), ou e o estado global
   do projeto (`project: healthy`, `orphans=1294`, `Analysis: health=0.81` — 1,5%), ou e eco da saida do comando
   (`note: Exit 0 (OK), 37 lines output (1088 bytes)`).

### 6.d O teto de 10.000 caracteres: o bloco gigante nao chega ao modelo — por mecanismo

Ha uma segunda camada de inerte que a contagem por familia nao revela: **o canal mesmo**.

A **medida** (`evidence-v1-v2.md:V1`): **10,2%** dos bytes injetados estariam em blocos acima de 10.000 chars e
seriam, portanto, **inertes por mecanismo** — a doc oficial diz que acima de 10.000 chars o Claude Code escreve
o texto num arquivo e *"Claude Code doesn't ask it to"*.

**Reconciliacao (reproduzi o instrumento do irmao e separei as populacoes).** O numero de **10,2%** so se
reproduz quando a medicao inclui **todos** os hooks, nao so os do Touring. No mesmo recorte de 400 sessoes:

| populacao | blocos | bytes | >10.000 chars | bytes acima do teto | % |
|---|---:|---:|---:|---:|---:|
| **todos** os hooks (ctx+reason) | 34.344 | 19.284.976 | 114 | 1.954.376 | **10,13%** |
| **so o Touring** (ctx+reason) | 29.030 | 15.114.509 | 18 | 275.212 | **1,82%** |

O reproduz do irmao (34.271 blocos / 18.898.797 bytes / 114 blocos / 10,2%) casa com a primeira linha: a populacao
dele e **todos** os hooks. E **96 dos 114** blocos gigantes (1.679.164 bytes) sao de **hook de terceiro** —
`bash "${CLAUDE_PLUGIN_ROOT}/scripts/session-start-hook.sh"`. A **parcela do Touring** e **1,82%** (400 sessoes) /
**1,23%** (799 sessoes: 38 blocos, 569.714 bytes). O numero de **10,2%** e real, mas **nao e do Touring** — e
importante nao atribuir ao Touring um byte que e de plugin.

**O mecanismo, verificado no transcript (esta parte e do Touring e e o achado):**

- as **139** invocacoes de hook com stdout acima de 10.000 chars sao **todas** em `SessionStart` — nenhuma em
  `PreToolUse`, `PostToolUse` ou `PostToolBatch`;
- o `hook_additional_context` — o texto que o modelo **de fato le** — tem **maximo de 9.043 chars** e **zero**
  blocos acima de 10.000, em 107.032 blocos medidos;
- nas sessoes com stdout gigante, o `additional_context` correspondente e uma **previa** de ~2.000 chars:
  `<persisted-output> | Output too large (18.2KB). Full output saved to: /home/.../xxx` — **com o caminho do
  arquivo, mas sem a ordem de le-lo**.

Isto e mais forte do que a contagem sugere: nao e que o modelo **ignora** o bloco gigante, e que ele **nao
recebe**. A leitura qualitativa ve a prova no proprio transcript — a previa de 2.000 chars com o caminho, sem a
instrucao. E o `pre-edit`, que e o **maior** consumidor de contexto (22,0 MB ponderados), **nunca** passa desse
teto: e truncado **antes**, pelo proprio hook.

Consequencia para o design: **truncar o payload no lado do Touring e pior que a ausencia** quando o sinal e
truncado (o `pre_bash.rs` corta em 9.500 chars). Um bloco cortado no meio e um bloco que *parece* completo — o
modelo nao tem como saber que faltou algo. O `code-mode-contract` ja mostra essa patologia na forma mais pura:
3.275 blocos com o programa aberto no meio.

### 6.e O melhor bloco do sistema esta na familia que a regua nao ve

O `[G2 exit-code-through-pipe → REESCRITO]` e o `[G8 laco-de-inspecao → REESCRITO]` sao, por prosa, os dois
melhores blocos do corpus — frase, verbo, mecanismo, causa, referencia de spike, kill switch humano e a confissao
de que o hook reescreveu a chamada. Vem pelo canal de `permissionDecisionReason`.

A taxonomia do harness (`measure_injection_effect.FAMILIES`) classifica por **conteudo**, e nenhum padrao casa
`[G2 ...`. Resultado: os dois vao para `outros` (10.400 blocos, 2,0 MB), **fora de qualquer familia**, fora do
relatorio de conformidade da rodada 3 e fora de qualquer veredito. O `[M3 delegacao]` (109 blocos) cai no mesmo
buraco — e e acao com justificativa economica (*"50 arquivos lidos voltam como 3 achados"*).

**E a inversao exata do achado de 6.d:** os blocos **inerte por mecanismo** (os gigantes, >10k) estao classificados
e medidos; os blocos **melhores do sistema** estao fora da regua. A taxonomia mede bem o que produz **muito** e
nao mede o que produz **bem**.

### 6.f O `pre-edit` sao duas familias, e a regua as junta

`pre-edit` (19.062) e `pre-edit-prevention` (175) sao payloads de **formatos diferentes**, de hooks diferentes
(`pre_edit.rs` e `pre_edit_prevention.rs`), com contratos diferentes. O corpus medido: **3.028** dos 17.795
payloads distintos de `pre-edit` **comecam** com `Pre-edit prevention for <arquivo>` — e **nenhum** payload contem
os dois formatos. Ou seja: **17%** da familia `pre-edit` sao, na verdade, `pre-edit-prevention` mal classificados
(a regua casa `related docs:` antes de `Pre-edit prevention`).

O efeito e duplo: a familia `pre-edit` aparece 19.062 no relatorio quando so 4.550 sao de fato `HOTSPOT`; e a
familia `pre-edit-prevention` — a de **98,8% de conformidade** — aparece com 175, escondida dentro da outra. **A
familia que funciona e a que a regua subestima; a que nao funciona e a que a regua infla.**

### 6.g O eco do comando dentro do proprio pre-edit

`note: Exit 0 (OK), 37 lines output (1088 bytes); bash_metrics[generic]: byte_count=1088 exit_code=0 line_c` — o
modelo **acabou de ver essa saida** na tela, imediatamente antes. O bloco reentrega a metrica do comando que o
proprio hook acabou de medir. E a forma mais barata de derivabilidade: nao precisa abrir o arquivo, precisa so
olhar para cima. Medido: **19,8%** dos bytes de `pre-edit` sao `FILE_ECHO` e mais um bloco relevante e eco.

### 6.h O que o modelo recebe nao e o que o hook emitiu

O transcript tem **dois** registros por hook: `attachment.type=hook_success` (o stdout bruto) e
`attachment.type=hook_additional_context` (o texto injetado). Cruzando os dois em 40 sessoes: **3.437** blocos de
contexto repetem literal o stdout do `hook_success` que os precede, e **2.243** divergem. As contagens da rodada 3
medem o **stdout emitido**; o modelo le o `additional_context`. São populacoes diferentes, e a diferenca (17.062
`hook_success/PreToolUse` para 3.388 `hook_additional_context/PreToolUse` em 80 sessoes) e da ordem de grandeza
do que se mede vs. o que se le.

---

## 7. Limites desta leitura

1. O corpus e de **799 sessoes** em 3 projetos (`-touring`, `-analise`, `-konverter`); a rodada 3 usou 400. As
   familias e a taxonomia sao as mesmas (`measure_injection_effect.FAMILIES`), mas os **totais absolutos nao sao
   comparaveis** com `conformidade-por-familia-r3.txt` — as **taxas** sao.
2. A classificacao de atomos (6.c) e uma **amostra de 400 payloads distintos por familia** (600 para `pre-edit`),
   nao o corpus inteiro. As porcentagens sao de amostragem. As **duas** segmentacoes citadas em 6.c (a tabela
   uniforme e a segmentacao fina do `pre-edit`) tem taxonomias diferentes e **nao sao somaveis entre si**.
3. O veredito de prosa (frase/caco) e **leitura humana**, com o criterio declarado em 1.2. Onde a leitura e
   contestavel, o trecho verbatim esta ao lado para o leitor conferir.
4. O paragrafo 6.d reconcilia **duas populacoes** que a `evidence-v1-v2.md:V1` misturou: o **10,2%** inclui
   hooks de **terceiros** (96 dos 114 blocos gigantes sao do plugin); a parcela **do Touring** e **1,82%**
   (400 sessoes) / **1,23%** (799 sessoes). O **mecanismo** (previa de ~2.000 chars em vez do texto) foi
   verificado diretamente no transcript e vale para ambos.
5. Nao foi feita medicao de **adesao** nesta rodada: e medido o conteudo, nao o efeito. A conformidade da rodada 3
   e usada aqui como **contexto**, nunca como medicao deste relatorio.
