---
type: Analysis
title: "Impacto qualitativo da injeção de contexto no comportamento do modelo — os cinco modos de influência"
description: "O que cada conteúdo injetado por hook FAZ com o modelo — não quantas vezes, mas de que forma: Informar, Instruir, Restringir, Desviar, Imprimir. Conformidade medida por família e a descoberta de que o efeito do conteúdo é distal."
plan_id: 2026-10-26-injecao-contexto-hooks
okf_version: 1
tags: [hooks, injecao, qualitativo, comportamento, modelo]
timestamp: 2026-10-26
---

# Impacto qualitativo — o que a injeção **faz** com o modelo

> Complementa `ANALISE-CONSOLIDADA.md` (§2–§4 medem **quanto**; aqui: **de que forma**).
> Payloads citados são **literais do corpus** (21 dias, 1.028 sessões). A conformidade por
> família foi medida em 300 sessões por `scripts/measure_injection_effect.py`.

---

## 1. A pergunta, e por que "adesão" não responde

A pergunta do Gabriel: *"se cada conteúdo injetado por cada hook gera o impacto esperado no
modelo e de que forma"*.

"Adesão" responde a uma pergunta **derivada** — *"o modelo fez o que o bloco pedia?"* — e não diz
**o que o bloco faz com o modelo**. A diferença importa porque o conteúdo injetado não é uma
coisa só: são **modos de influência qualitativamente distintos**.

| modo | o que injeta | o que muda no modelo | direção | quem |
|---|---|---|---|---|
| **INFORMAR** | fato sobre o código | o modelo passa a **saber** algo que não sabia | **ALARGA** o espaço de ações | blast radius · file context · syntax pre-check · lacunas da compactação |
| **INSTRUIR** | MUST/SHOULD com um comando | o modelo passa a **preferir** um caminho que o *harness* prefere | **ESTREITA** | cli-suggest · delegation-nudge · skill-picker · prompt-enhance |
| **RESTRINGIR** | deny com o remédio | o modelo **não tem escolha** | **ELIMINA** | gates G1/G7/quality/guard/process-guard |
| **DESVIAR** | aviso de falha anterior | o modelo passa a **evitar** (ou não) uma ação | estreita (se funcionar) | *last fail* · evidence-guard |
| **IMPRIMIR** | dado bruto / dashboard | o modelo **ocupa atenção** sem ganhar informação | **DILUI** | `post_quality_gate` · ATZILUTH · dashboard slim |

**INFERENCE [0.9]** — **três dos cinco modos estreitam o espaço de ações do modelo** e **só um o
alarga**. O Touring passa a maior parte do seu tempo de janela **empurrando o modelo para um
subconjunto de caminhos** — e o único modo que o alarga é o que já tem o maior efeito medido.

---

## 2. A descoberta: o efeito do conteúdo é **DISTAL**, não proximal

**FACT [1.0]** — medindo a conformidade (o efeito declarado aconteceu?) e a **forma** do efeito,
em 300 sessões:

| família | n | CONFORME | CONTRARIO | SEM EFEITO | INDETERMINADO | forma dominante |
|---|---:|---:|---:|---:|---:|---|
| **pre-edit** (blast radius) | 5.613 | **60,3%** | 0,0% | 39,7% | 0,0% | **usou-o-fato-depois** (3.385) |
| **pre-edit-prevention** (syntax) | 25 | **96,0%** | 0,0% | 4,0% | 0,0% | **usou-o-fato-depois** (24) |
| outros | 10.003 | 23,3% | 0,0% | 76,7% | 0,0% | usou-o-fato-depois (2.333) |
| plugin-hook (dashboards) | 604 | 2,8% | 0,0% | 97,2% | 0,0% | usou-o-fato-depois (17) |
| post-edit | 777 | 0,4% | 0,0% | 99,6% | 0,0% | — |
| **touring-suggest** (sermão) | 701 | **0,0%** | 0,0% | **100,0%** | 0,0% | **nenhum** (701) |
| **code-mode-deny** | 627 | 0,0% | 0,0% | 100,0% | 0,0% | nenhum — **artefato de medição** (ver §5) |
| **evidence-guard** | 1.312 | 0,0% | 0,0% | 0,0% | **100,0%** | não medível (zero proposições) |
| **post-tool-echo** | 1.047 | 0,0% | 0,0% | 0,0% | **100,0%** | extração do comando falhou (ver §5) |
| debug-stdout | 854 | 0,0% | 0,0% | 0,0% | 0,0% | (não é contexto) |

**FACT [1.0]** — a forma do efeito do `pre-edit` é **`usou-o-fato-depois`** em 3.385 de 5.613
casos (60,3%), e **`executou-o-comando` e `desviou-alvo` são 0**. O modelo **não muda a chamada
seguinte** ao receber o blast radius — ele **carrega o fato e o usa mais tarde na sessão**.

**INFERENCE [0.9] — isto inverte a leitura da §4.6.** A §4.6 media efeito **proximal** (janela de
3 chamadas) e por isso via o canal do fato como "fraco". O efeito real do fato é **distal**. E o
sermão é o oposto: **100% sem efeito**, proximal e distal.

**INFERENCE [0.9] — e daí sai uma consequência de desenho:** um instrumento que mede adesão
proximal ("o MUST foi seguido na chamada seguinte") **penaliza exatamente o canal que funciona**
e premia o que não funciona. O KPI `injection_follow_ratio` da régua mede adesão proximal — e
por isso ele reprova o `no-advisory` que a §4.6 sustenta. **A régua e a medição de conformidade
perguntam coisas diferentes, e a régua está perguntando a coisa errada.**

---

## 3. O mapa, hook a hook: o que cada um melhora e o que cada um restringe

| hook | modo | o que injeta (literal do corpus) | o que **melhora** | o que **restringe** |
|---|---|---|---|---|
| **blast radius**<br>(PreToolUse:Edit) | INFORMAR | `HOTSPOT: 'harness_concordancia' has 6 callers \| callers: [test_harness_e_deterministico…, test_harness_classes_do_gate_sao_exaustivas…]` | o modelo sabe **quem depende** antes de editar — evita quebrar um chamador sem saber | nada |
| **file context**<br>(PreToolUse:Read) | INFORMAR | `📌 defs[99]: PendingConsumer(33)·PendingQualifiedUse(38)·cli_index_status(46)·index_status_json(73)…` | o modelo sabe **quais símbolos o arquivo tem** — usa a API certa em vez de adivinhar | nada |
| **syntax pre-check**<br>(PreToolUse:Edit) | INFORMAR | `Pre-edit prevention for … (7 signals): SYNTAX: 'fase_procedencia' has CC=16 (threshold=15, excess=1) \| COMPLEXITY: … \| SHADOW: variable 'feito' shadows outer definition (L132)` | o modelo **não grava** o defeito que o hook já viu — **96% de conformidade** | nada |
| **speculative validation**<br>(PreToolUse:Write) | INFORMAR + INSTRUIR | `⚙ rust-workflow: pub_surface=4 complexity=0.00 … \| ⚠ pensieve: similar path had recorded failures (64% match) \| wiring_predict: 4 new pub symbol(s) — ensure consumers exist or reduce visibility` | o modelo sabe a **superfície pública** que está criando | o aviso "ensure consumers exist" é **instrução disfarçada de fato** |
| **edit quality**<br>(PostToolUse:Edit) | INFORMAR | `⚙ check(touring-storage): OK \| post-edit verification: 3 issue(s) \| API surface changed: +pub: AcoGraph, Complexity, DimResult…` | o modelo sabe **o que a edição expôs** | nada |
| **lacunas da compactação**<br>(SessionStart:compact) | INFORMAR | `[COMPACTAÇÃO · lacunas do resumo, medidas no transcript] O resumo não cita o que segue; confira no disco antes de agir sobre isto. - arquivos alterados: …` | **impede o modelo de agir sobre um resumo incompleto** — previne uma classe inteira de erro | nada — e é o **de maior valor por byte** |
| **cli-suggest**<br>(PreToolUse:*) | INSTRUIR | `[TOURING SUGGEST · pre-edit-triage-rust · conf=0.89] Enrichment: file_indexed=no, dependents=0, pub_symbols=16 Stale-index: touring index rebuild --dir … // file absent from blake3 registry — enrichment above may under-report MUST touring ast meta …` | **nada medido: 100% sem efeito** (701 blocos) | **estreita**: o modelo passa a preferir `touring X` a `grep` — e o MUST vem de um índice **velho**, o próprio payload avisa |
| **prompt-enhance**<br>(UserPromptSubmit) | INSTRUIR | `## Constitutional Constraints — CODE constraints: add type hints to every function signature. Include docstrings on all public functions. Handle errors with specific exception types, never bare except.` | a formato de resposta (CoT, tags de precisão) | **estreita e desinforma**: as regras são **Python-centric num workspace Rust** — `type hints` e `bare except` não existem em Rust |
| **delegation-nudge**<br>(PreToolUse:Read\|Grep\|Glob) | INSTRUIR | `[delegation-nudge] 3 leituras … Delegue: Agent(subagent_type="Explore", model="haiku")…` | (delegação quando o volume justifica) | tira trabalho do modelo contexto principal — decisão do harness, não do modelo |
| **skill-picker**<br>(UserPromptSubmit) | INSTRUIR | `[SUGESTÃO DE SKILL: 'Touring' (confiança 0.75) — avaliada por decisão tipada sobre o prompt anterior. Invoque com /Touring se fizer sentido, ou ignore: o gate de silêncio só deixa passar sugestões acima do τ medido (0,55).]` | (acerta a skill certa quando acerta) | estreita — e o texto **instrui mesmo quando diz "ignore"** |
| **last fail**<br>(PreToolUse:Bash) | DESVIAR | `Similar command failed recently (similarity=51%) ⚡ last fail: ( — E subprocess.TimeoutExpired: Command '['unshare', '-rn', '/home/gabrielgadea/projects/analise/.venv/bin/python', '-m', 'pytest', 'scripts/eleitoral/tests/test_publicar_mapa.py::TestACredencialPel…, .venv/bin/ruff — E ImportError: ca…` | evita repetir um comando que **falhou** | e o comando falho é **de outro projeto** — o modelo desvia de algo que nem ia fazer |
| **evidence-guard**<br>(PostToolUse:Bash) | DESVIAR | `⚠ EVIDENCE GUARD · DEGRADED · [verde-de-cache-nao-prova-medicao] 'forbidden_calls' → leia o stderr e o exit code, não só o stdout` | evita o erro de leitura | 1.312 blocos e **zero** proposições extraíveis: nomeia **lições**, não ações |
| **G1 rajada**<br>(PreToolUse:Bash) | RESTRINGIR | `[G1 rajada-de-inspeção] {motivo}. Rode-o como programa — os já executados da rajada vêm como comentário e NUNCA são repetidos: {programa} Bypass por-comando: prefixe {GATE_BYPASS_TOKEN}` | **determinístico**: o modelo roda o programa | elimina a alternativa — e é o **único modo com adesão garantida** |
| **G7 re-inspeção**<br>(PreToolUse:Read) | RESTRINGIR | `[G7 re-inspeção] {n}ª leitura de {fp} na janela — reler é o sinal mais direto de programa-faltando (campeões medidos: adw.py 39×, cli_suggester.rs 32×). Varra de uma vez (R1): {rota}.{edicao}` | determinístico, com o **remédio pronto** | elimina a releitura |
| **code-mode-deny**<br>(PreToolUse) | RESTRINGIR | `[CODE MODE · rajada] aviso — 2ª inspeção 'python-inline' em 300s, executada nativa. Se a próxima também for inspeção, rode o lote de uma vez: touring run --lang python --code '…'` | determinístico + **o programa substitui o comando** | elimina a execução atômica |
| **PostToolBatch**<br>(PostToolUse) | IMPRIMIR | `post-tool-batch: 12 arquivo(s) que podem carregar símbolo público editados: …/cli_suggester.rs, …/cli_suggester.rs, …` | (quase nada) | dilui — e o **mesmo path 12×**: o modelo não pode fazer nada com "12 arquivos" |
| **post_quality_gate**<br>(PostToolUse:Edit\|Write) | IMPRIMIR | `[EVENT_READER] Reading hook event [EVENT_READER] Received dict event [POST_QUALITY_GATE_V4] Hook execution started 🚀 Quality Gate - Checking 1 file(s) —…` | **nada** — é log de depuração | **dilui**: 5,3 MB (9% da injeção) com **1% de unicidade** |
| **ATZILUTH**<br>(SessionStart) | IMPRIMIR | `🌅 ATZILUTH — Painel de Emanação (o estado do mundo, classificado) Itens vivos, por GUT: 1. [ 60·G4U5T3] RETOMAR: RETOMAR AQUI — 2026-08-25 …` | (nenhuma, para a tarefa corrente) | **troca de contexto**: o modelo passa a pensar no estado do mundo, não na tarefa |
| **dashboard slim**<br>(SessionStart) | IMPRIMIR | `🧠 TOURING (slim) ## Saúde: composite indisponível (timeout 3s) · Doctor indisponível (timeout 1.5s)` | (nenhuma, para a tarefa corrente) | troca de contexto |

---

## 4. Quatro efeitos qualitativos que a contagem não diz

### 4.1 O único modo que **alarga** o modelo é o que já funciona

**FACT [1.0]** — INFORMAR: **60,3%** de conformidade (`pre-edit`), **96,0%** (`pre-edit-prevention`),
e **56%** do volume do Touring. INSTRUIR: **100% sem efeito** (701 blocos). RESTRINGIR: **1,000
por construção** (reescreve o comando; 13 negações; 581,9 KB elidos).

**INFERENCE [0.9]** — o perfil do Touring é, qualitativamente, **majoritariamente um empurrador**:
gasta 27,8% da janela para substituir o modelo por um subconjunto de caminhos, e a fração que
efetivamente o **capacita** é a que o harness menos "controla" — porque não é ordem, é fato.

**⇒ É a razão qualitativa pela qual o interruptor não pode ser binário:** `no-advisory` corta os
modos **INSTRUIR** e **IMPRIMIR** e preserva **INFORMAR** e **RESTRINGIR** — são efeitos de
natureza diferente, e só dois deles são substituíveis por um interruptor sem perda.

### 4.2 O pior caso qualitativo não é ruído: é **desinformação**

**FACT [1.0]** — o `prompt-enhance` injeta em **todo** `UserPromptSubmit`:

```
## Constitutional Constraints
CODE constraints: add type hints to every function signature. Include docstrings on all
public functions. Handle errors with specific exception types, never bare except.
```

O workspace é **Rust**. `type hints`, `bare except` e docstrings no formato Python **não existem**
em Rust — e `except` nem compila lá.

**INFERENCE [0.85]** — isto não é diluição: é **instrução contrária ao código**, com forma
imperativa ("add…", "never…"). O efeito é pior que o de um log de debug: o log ocupa atenção sem
informar; isto **informa errado**.

**FACT [1.0]** — o defeito foi documentado em 04/09/2026
(`docs/2026-06-26-touring-llm-coupling-strategy.md` §3.2: *"`ConstitutionalConstraints` é
Python-centric … num workspace Rust"*) e **continua quatro meses depois**. É regressão de
**qualidade do payload**, não de volume.

### 4.3 O aviso de falha desvia o modelo do que ele não ia fazer

**FACT [1.0]** — o *last fail* nomeia comandos de **outro projeto**
(`/home/gabrielgadea/projects/analise/.venv/bin/python -m pytest scripts/eleitoral/…`), numa
sessão do `touring`. A consulta é `key LIKE "outcome:{tool_class}:%:failure" LIMIT 15` — casa
qualquer intent/arquivo **sem filtro de relevância**. **46.796 invocações, 2,05 MB, 6% de
unicidade.**

**INFERENCE [0.85]** — o efeito é um **desvio sem objeto**: um "não faça isso" sobre uma ação que
não estava no radar. Não há ganho; há uma restrição adicionada ao espaço de ações, sem
contrapartida.

### 4.4 O conteúdo de maior valor por byte é o que tem menos mecanismo

**FACT [1.0]** — o `loop_resume` ocupa 892 KB e diz uma coisa estruturalmente importante:
*"O resumo não cita o que segue; confira no disco antes de agir sobre isto"* — com a **lista dos
arquivos alterados**.

**INFERENCE [0.9]** — é o único payload que previne uma **classe inteira de erro** (agir sobre um
resumo incompleto), em vez de melhorar uma ação específica. Ele não "faz" o modelo agir de outro
jeito; ele **corrige o estado epistêmico** do modelo antes da ação. Nenhum outro injetor faz isso.

**INFERENCE [0.85]** — e é por isso que "adesão" mede mal o valor de um payload: o `loop_resume` não
tem proposição extraível (nenhum MUST, nenhum path-como-comando), então a máquina de conformidade
o classifica como **indeterminado/sem efeito** — **e está errado**. O efeito dele é **preventivo**,
não conformativo.

---

## 5. As três limitações honestas desta medição

**FACT [1.0] — (a) `code-mode-deny` aparece como 100% "sem efeito", e isso é ARTEFATO DE
MEDIÇÃO, não achado.** O payload do gate viaja em `permissionDecisionReason` — **fora** do
`additionalContext` que esta análise examina. O gate funciona (§4.2: 1,000 por construção, 581,9
KB elidos); a medição é que não o alcança.

**FACT [1.0] — (b) `post-tool-echo` e `evidence-guard` são 100% INDETERMINADOS.** O primeiro
porque a extração do comando falhou (o formato do payload não casou com o padrão esperado); o
segundo porque **não tem proposição extraível** — nomeia lições, não ações. **A ausência de
sinal aqui é do instrumento, não do conteúdo** (a armadilha "ausência de sinal tem duas causas",
pela segunda vez nesta sessão).

**FACT [1.0] — (c) isto mede CONFORMIDADE, não ATRIBUIÇÃO.** "O efeito declarado aconteceu" não
é "o modelo teria feito isso de qualquer jeito". A atribuição exige o **A/B de corpus**
(`crates/touring-hooks-shared/tests/s8_corpus_ab.rs`) — o passo que ainda falta.

**INFERENCE [0.85]** — e há um quarto, estrutural: **a Janela de 3 chamadas é curta demais para
medir o modo INFORMAR**, porque o efeito dele é distal por natureza (§2). Uma janela de 3
chamadas é a medida certa para o modo RESTRINGIR (o efeito é imediato) e a medida **errada** para
o modo INFORMAR.

---

## 6. O que a análise qualitativa acrescenta à de volume

| | o que a contagem diz | o que a qualitativa acrescenta |
|---|---|---|
| o `cli-suggest` | 1,3% de adesão | **é o único modo que estreita sem ganhar** — 100% sem efeito, proximal e distal — e o MUST vem de um índice que o próprio payload declara velho |
| o `prompt-enhance` | 92 B por prompt, volume pequeno | **é contrapropósito**, não ruído: regras de Python num workspace Rust, quatro meses depois de documentado |
| o *last fail* | 6% de unicidade | **desvia de ação alheia** — contaminação de sessão |
| o `loop_resume` | 138 blobs, sem proposição | **o de maior valor por byte**: previne uma classe de erro, e a máquina de conformidade o classifica como "sem efeito" |
| o `post_quality_gate` | 9% do volume | **a maior fonte de ruído isolada**, e é um bug (stdout cru) |
| o dashboard | 5,8% de impacto | **troca o contexto**: tira o modelo da tarefa |
| o `pre-edit` | 60,3% de conformidade | **o efeito é distal** — a janela de 3 chamadas da §4.6 media só o efeito proximal e por isso via o canal que funciona como fraco |

---

_Análise qualitativa por TACO sob ordem de Gabriel Gadea, 26/10/2026. Payloads literais do
corpus (21 dias); conformidade medida em 300 sessões por `scripts/measure_injection_effect.py`.
Complementa `ANALISE-CONSOLIDADA.md` (volume e taxas) e `strategy-2026-10-26.md` (o interruptor)._
