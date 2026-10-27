---
type: Analysis
title: "Matriz de diagnóstico da rodada 4 — a regra externa x a medição interna, família por família"
description: "S-1 do DAG task_1793033502719494636: 16 famílias de contexto injetado, o veredito complementar sobre a análise consolidada, o contra-diagnóstico, e o desenho de S-2/S-3 com a decisão sobre S-4/S-5."
plan_id: 2026-10-26-injecao-contexto-hooks
okf_version: 1
timestamp: 2026-10-26
---

# Matriz de diagnóstico — rodada 4 (S-1)

> **O que este documento faz.** Para cada família de contexto injetado, ele põe
> lado a lado **a regra externa aplicável** (com arquivo:linha da fonte) e **a
> medição interna** (o que o corpus do Touring mostra que aconteceu). Da
> interseção sai uma **regra operacional** executável, o **veredito complementar**
> sobre `ANALISE-CONSOLIDADA.md`, o **contra-diagnóstico** (o que a evidência
> externa **não** sustenta), e o **desenho** das correções S-2 e S-3 mais a
> decisão sobre S-4/S-5.
>
> **Regra de leitura.** Toda linha da matriz é uma interseção de **duas**
> evidências independentes: uma de fora (a fonte) e uma de dentro (o corpus). Onde
> as duas **não** concordam, o veredito é `DISPUTADO` e a pendência é nomeada.

---

## 0. Procedência — os três corpora e por que eles não são um só

Antes da matriz, um fato que condiciona **toda** a leitura: a rodada 4 tem
**três** medições de corpus, de tamanhos e instrumentos diferentes. Confundi-las
é o erro que a rodada 3 já cometeu uma vez (D1).

| corpus | fonte | tamanho | o que mede |
|---|---|---|---|
| **C-400** | `scripts/measure_injection_effect.py` (`IMPACTO-POR-HOOK.md` §2) | 400 sessões · **28.295 blocos** · 14.644.672 B | conformidade por família (efeito observado) |
| **C-799** | `round4/scout-corpus-qualitativo.md` §1.1 | 799 sessões · 3 projetos | leitura **verbatim** + bytes por família + julgamento qualitativo |
| **C-V12** | `round4/evidence-v1-v2.md` | 400 sessões · **34.271 blocos** · 18.898.797 B | canal de protocolo + teto de 10.000 chars |
| **C-WIN** | medição do orquestrador, 26/10/2026 | 400 sessões · transcript **completo** (sem o filtro `attachment\|assistant`) | **a janela real**: o que entra na janela x o que vai só para o transcript |
| **C-RED** | medição do orquestrador, 26/10/2026 | 400 sessões | **repetição**: bytes excedentes ao conteúdo distinto, por família |

=> **A coluna `n` e `conformidade` vêm do C-400. A coluna `bytes` vem do C-799.**
Não são o mesmo corpus; a proporção n<->bytes **não** deve ser lida como
densidade por família. Onde uma coluna não existe, a célula diz `[não medido]`
— não um número inventado.

**Verificação de código usada nesta matriz** (tudo conferido neste ciclo, com
`touring index find`, `touring ast blast`, `touring wiring impact` e `grep`):
`pensieve.rs` (23-33, 80, 100, 117, 139, 163, 181-189) · `ann_index.rs:171` ·
`pre_bash.rs` (3-11, 222, 229-239) · `post_bash.rs` (199-206, 332, 430, 464, 469) ·
`pre_edit.rs:431-436` · `pre_write.rs:582-589` · `pretask.rs:321-327` ·
`got.rs:962` · `pre_read.rs:3-7` · `pre_read_tests.rs:1221` ·
`context_budget.rs` (200, 298, 369, 375) · `models.rs:124-142` ·
`injection_policy.py` (82-93, 107-130, 236, 247, 258) ·
`measure_injection_effect.py` (60-80, 116, 134) · `s8_corpus_ab.rs` ·
`Cargo.toml` de 7 crates (direção de dependência) ·
`touring index status` (382.305 símbolos) · `touring wiring orphans` (765).

---

## 1. A matriz — 16 famílias, regra externa x medição interna

**Colunas.** `n` (C-400) · `bytes` (C-799) · `conformidade medida` (C-400) ·
`forma do efeito` (C-400 §3.1) · `regra externa aplicável` (arquivo:linha da
fonte) · `veredito`.

**Vereditos possíveis.** `FATO — manter` · `ALERTA — corrigir o formato` ·
`SERMÃO — cortar` · `RUÍDO — cortar` · `ENFORCEMENT — manter` ·
`NÃO É FAMÍLIA — desdobrar` (S-3) · `DISPUTADO`.

| # | família | n | bytes | conformidade | forma do efeito | regra externa aplicável | veredito |
|---|---|---:|---:|---|---|---|---|
| 1 | `pre-edit` | 6.636 | 21.445.443 | **75,6%** | `usou-o-fato-depois` (distal) | harness2 `prompts.py:860-862` — "would this text have changed it?"; Context7 `doc-websites_code_claude-context.txt:1224` — "context from the application" | **FATO — manter** |
| 2 | `touring-suggest` | 4.647 | 7.122.286 | **0,0%** (2,1% cita sem agir) | `internalizou-sem-agir` (98) | harness2 `prompts.py:129-133`/`:237-241` — "ALWAYS-ON PROSE IS THE EXPENSIVE OPTION"; notebook "Claude Code " **[7]** — a janela é bem público; Regra 11 **[12,13]** | **SERMÃO — cortar** |
| 3 | `post-edit` | 3.060 | 2.946.259 | 0,8% | `nenhum` | harness2 `prompts.py:864` — "a repeated concrete failure"; `prompts.py:868` — "duplicated instruction dilutes the rest" | **ALERTA — cortar até ter gate** |
| 4 | `outros` | 2.953 | ~2,0 MB | 15,7% | `usou-o-fato-depois` (463) | — (não há fonte aplicável: **é um catch-all**) | **NÃO É FAMÍLIA — desdobrar (S-3)** |
| 5 | `evidence-guard` | 2.148 | 47.095 | **0,0%** | `nenhum` | harness2 `prompts.py:868` — duplicado dilui o resto; Context7 `doc-llmstxt_code_claude_llms_txt-hooks.txt:928` — "Don't copy untrusted tool output into it" | **REGRA REPETIDA — cortar (volume baixo)** |
| 6 | `code-mode-contract` | 1.840 | 1.336.065 | **0,0%** | `nenhum` | harness2 `prompts.py:456-458` — "if you add always-on text, cut something else"; Context7 `doc-websites_code_claude-context.txt:348` — teto de 10.000 chars | **DIRETIVA sem execução — cortar** |
| 7 | `similar-fail` | 1.676 | 549.386 | **INDETERMINADO** | `nenhum` | harness2 `prompts.py:860-862` — sem momento nomeado, o texto "costs context for nothing" | **INALERCIAL POR CONSTRUÇÃO — S-2** |
| 8 | `generic-noise` | 1.499 | 118.641 | — (não é contexto) | `nenhum` | harness2 `prompts.py:868` — não repetir o que já se sabe | **RUÍDO — cortar** |
| 9 | `post-tool-echo` | 1.332 | 1.242.808 | 0,0% | `nenhum` | harness2 `context.py:25-27` (evicção, não resumo) + notebook "Claude Code " **[3,4]** (`fast-jev-compaction`) | **ECHO — cortar** |
| 10 | `code-mode-deny` | 970 | 761.870 | **100% por construção** (13 negações, telemetria) | **reescreveu a chamada** | harness2 `prompts.py:212-215`/`:494-497` — *"Prose can be stopped reading; a hook cannot"*; Table 5, C8 — "enforces a behavior rather than instructs it" | **ENFORCEMENT — manter** |
| 11 | `plugin-hook` | 578 | 580.502 | 2,9% | `nenhum` (561) | notebook "Claude Code " **[1-6]** — 6 categorias de "What to Delete" (always-on em [6]) | **DASHBOARD — cortar** |
| 12 | `pipefail-evidence` | 530 | 67.205 | **0,0%** | `nenhum` | harness2 `prompts.py:864` — "a repeated concrete failure" (que aqui **não houve**) | **DISPUTADO — FATO sem momento** |
| 13 | `compact-warning` | 255 | 2.644 | **46,3%** | `gravou-estado` (118) | harness2 `prompts.py:860` — o momento **existe e é observável** (o estado foi gravado) | **FATO — manter** |
| 14 | `pre-edit-prevention` | 86 | 29.956 | **98,8%** | `usou-o-fato-depois` (85) | harness2 `prompts.py:864` — falha concreta; **mas** `scout-corpus-qualitativo.md` §1.3(b) — chega **depois** do edit | **FATO — manter (momento tardio)** |
| 15 | `session-contract` | 71 | 528 | 0,0% | `nenhum` | harness2 `prompts.py:456-458` — always-on que não paga | **DIRETIVA — cortar** |
| 16 | `pre-read` | 14 | 31.559 | 7,1% | `nenhum` | `pre_read.rs:1-7` (o próprio contrato) + harness2 `context.py:399` (`assert_no_leak_paths`) | **QUASE SILENCIOSO — manter** |

### 1.1 As três leituras que a matriz permite (e que a rodada 3 não tinha)

**(a) O valor está no *canal*, não no *volume*.** O topo da matriz por valor é
`pre-edit` (75,6%) e o **segundo** é `code-mode-deny` (100% por construção, o
único que reescreve). O pior em volume é `touring-suggest` (16,4% do contexto,
0,0%). **O instrumento de 400 sessões mede adesão por proximalidade e por isso
classifica o gate como "0%"** (`IMPACTO-POR-HOOK.md` §6) — o gate funciona; o
instrumento não o alcança.

**(b) `outros` não é uma família — é o resto, e o resto contém os dois extremos.**
`outros` = 10,4% do volume (2.953 blocos) e 15,7% de conformidade agregada. Mas
`round4/scout-corpus-qualitativo.md` §2.17 mostra que dentro dela estão:
- o **pior** bloco do sistema — os `[G2 …]`/`[G8 …]` saem pelo canal
  `permissionDecisionReason`, e `permissionDecisionReason` **não é classificado
  pela taxonomia de contexto** (o instrumento só classifica `additionalContext`);
- o **melhor** bloco do sistema — `[G2 exit-code-through-pipe -> REESCRITO]` e
  `[G8 laco-de-inspecao -> REESCRITO]`: frase, verbo, causa, mecanismo, kill
  switch e referência de spike. *"O melhor bloco do sistema está na família que
  a taxonomia não nomeia."*

=> **Veredito da linha 4: `outros` não pode ser uma linha da matriz.** É uma
pendência de instrumento, e o desdobramento é **S-3**. A matriz honesta tem 15
famílias nomeadas + 1 catch-all marcado como **pendência de taxonomia**, não
como família.

**(c) Um "FATO" com 0% observado existe — e ele é o teste de momento do
harness2, não uma falha do conteúdo.** `pipefail-evidence` é um fato verdadeiro
(o exit code foi mascarado por um pipe), tem verbo, tem alvo, é específico — e
mede **0%**. Não porque o modelo "não entendeu", mas porque **não houve momento
em que aquilo mudaria o desfecho**: a sessão não dependia do status do pipe. O
harness2 chama isso pelo nome (`prompts.py:860-862`): *"Every edit must trace to
a moment in a trajectory… would this text have changed it? If you cannot name
the moment, the edit is speculation and costs context for nothing."*

### 1.2 O canal `permissionDecisionReason` — o furo de classificacao da regua

| canal | quem usa | o que é | medido |
|---|---|---|---|
| `hookSpecificOutput.additionalContext` | **todas as 16 famílias** | o que o modelo lê | 28.188 (C-V12) |
| `hookSpecificOutput.systemMessage` | **ninguém** | — | **0** |
| `additionalContext` (raiz) / `systemMessage` (raiz) | **ninguém** | — | **0** |
| stdout cru (não-JSON) | hooks que imprimem texto | **não classificado** | 68.821 |
| `permissionDecisionReason` | **só o gate** (+ `outros`) | **a razão do deny** | 91 (gate) + 685 (`outros`) |

Duas consequências que **mudam o desenho**:

1. **O `systemMessage` nunca foi usado pelo Touring.** A §8.3.2 da analise
   consolidada afirma que "os dashboards do `SessionStart` saem pelo canal do
   `systemMessage` (sem depender da classificacao de familia)" — o corpus
   **contradiz** isso: os dashboards (familia `plugin-hook`) viajam por
   `hso.additionalContext`. => **O `off` não é independente da classificação
   para os dashboards**: a classificação é o que decide. Isso é uma correção
   material (ver §4, C3).
2. **Os 68.821 stdout crus não entram em nenhuma contagem de canal.** É onde
   vive `debug-stdout` (`scout-corpus-qualitative.md` §2.16: 8 blocos, 2.966 B)
   — volume irrelevante, mas **prova que existe um caminho de debug que chega ao
   contexto do modelo**.

### 1.3 A janela real — reconciliação do headline "27,8%"

A rodada 1 (`analysis-2026-10-26.md` §2) afirmou **injeção = 27,8% da janela do
modelo** (replicando 26,6% de 04/09). A medição nova (C-WIN, 400 sessões, 3
projetos, transcript **completo**, separando o que entra na janela do que vai
só para o transcript) dá:

| componente | bytes | % |
|---|---:|---:|
| resultados de ferramenta | 53.602.262 | 43,2% |
| **chamadas de ferramenta do modelo** | **30.644.663** | **24,7%** |
| mensagens do usuário | 16.214.848 | 13,1% |
| **injeção do Touring que entra na janela** | **14.858.084** | **12,0%** |
| texto do modelo | 4.438.050 | 3,6% |
| injeção de terceiros na janela | 1.108.353 | 0,9% |
| stdout cru de terceiros (só transcript) | 2.969.938 | 2,4% |
| stdout cru do Touring (só transcript) | **695** | **0,0%** |

**A reconciliação, feita em vez de substituída em silêncio:**

| | rodada 1 (200 sessões) | medição de hoje (400 sessões) |
|---|---:|---:|
| janela total | 30.973.693 B (154.868 B/sessão) | 123.836.893 B (309.592 B/sessão) |
| injeção | 8.606.515 B (**43.033 B/sessão**) | 14.858.084 B (**37.145 B/sessão**) |
| injeção / janela | **27,8%** | **12,0%** |

=> **O numerador concorda** (43,0 -> 37,1 B/sessão, -13,7%). **O que diverge é o
denominador: 2,00x.** As *chamadas de ferramenta do modelo* (24,7%) são uma
componente que a rodada 1 não separava — sem elas, a injeção seria 15,9%.
O resto da diferença (~9 pp) é **corpus e filtro**: a rodada 1 mediu 200
sessões **com** o filtro `attachment|assistant`; a medição de hoje usa 400
sessões e o transcript **inteiro**.

> **REGRA DE REGISTRO.** O **12,0%** é o **número medido hoje**, com a definição
> declarada (janela = tudo que o modelo recebe, incluindo as chamadas de
> ferramenta). O **27,8%** fica como **número herdado, não reconciliado** — não
> é apagado, e não é substituído. Nenhum dos dois está errado por si; eles
> medem **denominadores diferentes**.

**Uma hipótese refutada.** A suspeita natural era que o 27,8% estivesse
inflado por **stdout cru** do Touring. **Não está**: o stdout cru do Touring é
**695 bytes em 400 sessões** (0,0%). O stdout cru é de **terceiros** (2.969.938 B
— e nem esse vai para a janela). **100% da injeção do Touring vai por
`hookSpecificOutput.additionalContext`** — o que reforça, por um terceiro
caminho, o veredito do §1.2.

**O que a reconciliação NÃO muda:** a decisão. 12% da janela é grande. E a
composição é o que decide (abaixo).

### 1.4 A densidade — 69,8% dos bytes injetados são repetição

Por família (400 sessões), **excedente ao conteúdo distinto** sobre o total de
bytes da família:

| família | blocos | payloads **distintos** | taxa de repetição |
|---|---:|---:|---:|
| `debug-stdout` | 2.869 | **46** | **99,6%** |
| `compact-warning` | 284 | **11** | **99,5%** |
| `evidence-guard` | 2.356 | 134 | **96,2%** |
| `session-contract` | 72 | **4** | **95,8%** |
| `plugin-hook` | 780 | 473 | 93,4% |
| `similar-fail` | 1.678 | 208 | 92,3% |
| `pipefail-evidence` | 540 | 116 | 90,5% |
| `post-tool-echo` | 1.336 | 732 | 77,2% |
| `code-mode-deny` | 999 | 457 | 72,6% |
| `outros` | 5.230 | 1.988 | 65,7% |
| `touring-suggest` | 4.755 | 4.102 | 61,2% |
| `pre-edit` | 6.818 | 6.578 | 64,8% |

**Total: 13.193.705 de 18.910.157 bytes = 69,8% são repetição.**

Os payloads mais repetidos: o aviso DEGRADED do `evidence-guard` **1.577x**
(135 B) · `[generic]` **1.405x** (9 B) · `[gabrielgadea]` **1.206x** (14 B) · o log
`[EVENT_READER]` **1.854x** · `[mediacao sem relacao verificada]` **490x**
(1.131 B) · **`[G2 exit-code-through-pipe -> REESCRITO]` 399x** (155 B — um dos
"melhores blocos" do corpus!).

**A aritmética que decide a onda:**

| alavanca | ganho na janela | o que exige |
|---|---:|---|
| **dedup** (69,8% da injeção) | **12,0% x 69,8% = 8,4%** | infra **já existe** (`injection_dedup.rs`) |
| **corte do sermão** (18,7% da injeção) | 12,0% x 18,7% = **2,2%** | política nova + A/B de desfecho |

=> **O dedup vale ~3,7x mais que o corte do sermão, em bytes de janela.** E o
`[G2 ...]` repetido 399x prova que **o melhor bloco do sistema também é o mais
repetido** — o que reforça a §1.1(b): repetição e qualidade são eixos
**independentes**, e um bloco bom repetido 399 vezes ainda são 399 vezes de
bytes que não mudam nada.

---

## 2. A regra operacional

### 2.1 O cruzamento — os quatro critérios, e quem é o mais estrito

| critério | origem | o que exige | o que o Touring **nao** tem |
|---|---|---|---|
| **"So what?"** | Touring, `pre_read.rs:4-7` | acionavel? nao-derivavel? especifico? | declarado em **1** arquivo, testado em **1** (`pre_read_tests.rs:1221`) |
| **"o momento"** | harness2, `prompts.py:858-868` | **qual momento** da trajetoria? "would this text have changed it?" + precedencia DIVERGENCE > falha repetida > risco visivel + "nao repetir o que o harness ja diz" | **nada** — nenhum codigo exige que um bloco aponte para um momento |
| **"o que deletar"** | notebook "Claude Code " [1-6] | muleta · acumulado sem uso · duplicado · substituivel · always-on · sobreposto a memoria | a duplicacao ([3]) e o always-on ([6]) sao **medidos** e nao tratados |
| **"bem publico"** | notebook "Claude Code " [7] | nao gastar janela explicando o que o modelo ja sabe | o sermao (16,4%) e exatamente isso |

**O scout corrigido (contra o notebook certificado) acrescenta quatro dados
externos que fecham o cruzamento** (`round4/scout-notebooklm-claude-code.md`,
fonte global `g[n]`):

| dado | número | o que decide |
|---|---:|---|
| a **regra de ouro** do time de AI aplicada da Anthropic: *"the best context is the smallest amount of context that gets you the highest quality result at the cheapest cost"* | — | o **"menor contexto que entrega"** é a métrica, não a conformidade |
| **"80%+ do system prompt do Claude Code deletado"** pelos engenheiros da Anthropic para os modelos geração 5, **sem queda nos benchmarks** e com execução mais rápida | **>80%** | o corte do always-on **não custa** desempenho — o contra-dado direto ao medo de cortar |
| **context rot**: 18/18 modelos de fronteira perderam confiabilidade conforme a janela cresceu; em estudo controlado de 5 modelos, a performance caiu **13,9% a 85%** conforme o input crescia, **mesmo com a informação necessária presente no prompt** | 18/18 · 13,9%-85% | o **custo é do volume**, mesmo com o conteúdo presente — o sermão não é "de graça" |
| **60-70% dos turnos** de agente em produção são checagens determinísticas (`git status`, teste, linter) que um harness avançado resolve **na borda, sem chamar o LLM** | 60-70% | **justifica o gate e condenna a diretiva**: o que é determinístico pertence ao hook, não ao contexto |

**E há um quinto eixo, que a rodada 4 mediu e que os outros três só
pressupõem: a DENSIDADE.** A cláusula de redundância do harness2
("duplicated instruction dilutes the rest") tem, no corpus do Touring, uma
**magnitude medida**: **69,8% dos bytes injetados são repetição** (§1.4), e
o bloco repetido mais vezes é o **melhor** bloco do sistema
(`[G2 … -> REESCRITO]`, 399x). Repetição e qualidade são **eixos
independentes**: um bloco excelente repetido 399 vezes continua sendo 399
vezes de bytes que não mudam nada.

**Veredito sobre qual é mais estrito: o do harness2.** Ele é o unico que exige
**um contrafactual observavel** ("would this text have changed it?") e o unico
que tem **clausula de redundancia** ("do not restate what the harness already
says clearly; duplicated instruction dilutes the rest"). O "So what?" do Touring
e um predicado de 3 perguntas **sem momento** e **sem redundancia** — o que
explica por que ele passa: `pipefail-evidence` satisfaz as tres perguntas e
mesmo assim e inutil; e `evidence-guard` repete a mesma regra 4.874 vezes
(221 distintos, 47.095 B) sem violar nenhuma delas.

**A "regra oficial" que o notebook atribui a Anthropic:** `fonte-notebooklm-claude-code.md`
§3.1 afirma que *"o criterio do Touring … e a versao local da regra oficial"*.
**Isso e contradito** pelo scout do Context7 (§4.1): a varredura de 65 documentos
first-party **nao encontrou criterio editorial** de qualidade de conteudo
injetado — nem "so what?", nem "acionavel", nem "nao-derivavel". O que existe
sao 12 criterios **mecanicos/operacionais**. => **O "So what?" do Touring e
local, nao oficial.** Ver §4, C1.

### 2.2 A regra operacional, na forma executavel

> **R0 — O PORTÃO DO FATO-OU-DECISÃO.**
> Um bloco so entra na janela se carregar **(a) um FATO acionavel** — verbo +
> alvo nomeado, que o modelo nao obtem sozinho — **ou (b) uma DECISÃO**: o
> hook reescreveu a chamada ou negou a acao, e **explicou por que**.
> *O sermao isolado nao tem nenhuma das duas.*
> *(a) e (b) sao as duas propriedades que os blocos de efeito alto do corpus
> tem: `pre-edit` 75,6%, `pre-edit-prevention` 98,8%, `compact-warning` 46,3%,
> e o gate 100% por construcao.)*

> **R1 — O PORTÃO DO MOMENTO.**
> O fato, para ser pago, precisa de um **momento observavel** em que ele mudaria
> o desfecho. Sem o momento, o bloco e *verdadeiro e inutil* — e sai.
> *(E o criterio que separa `pre-edit` (75,6%) de `pipefail-evidence` (0%),
> ambos fatos, ambos acionaveis.)*

> **R2 — O PORTÃO DO ORÇAMENTO.**
> O bloco tem de caber no teto do canal (**10.000 chars** no contexto; **2.000**
> no classificador) e **nao pode repetir** o que a sessao ja disse nem o que o
> `CLAUDE.md` ja diz. *"Se voce adiciona texto always-on, corte outra coisa."*

> **R3 — O CANAL.**
> Payload que o **modelo** deve ler -> `hookSpecificOutput.additionalContext`,
> aninhado (fora disso e **ignorado em silencio**). Payload de **decisao** ->
> `permissionDecisionReason`. Os dois campos de texto > 10.000 chars -> **o texto
> nao chega**: vira arquivo + previa, e *"Claude Code doesn't ask it to"*.

**A justificativa externa do eixo (R0), que o notebook certificado dá com
palavras:** *"Adicionar instruções em texto no prompt ou no arquivo de regras
(CLAUDE.md / AGENTS.md), mesmo usando letras maiúsculas como 'NUNCA FAÇA X',
**não garante conformidade de 100%**. O modelo segue a regra na maioria das
vezes, mas falha em situações de estresse, conversas longas ou com casos de
borda"* (Q4 `[1]`,`[2]` -> `g[10]`, `g[50]`) — e a recomendação que daí decorre:
*"Se a regra não puder ser quebrada em hipótese alguma … **remova o texto da
skill e converta-o em um Hook determinístico** do sistema"*; *"os Hooks
executam **fora do contexto do LLM** … garantindo **100% de conformidade**
independentemente de o modelo tentar seguir a instrução ou não"* (Q4 `[7]`,`[8]`
-> `g[38]`, `g[47]`). => **É a mesma distinção que a matriz mediu**: a prosa é
*best-effort*, o hook é *determinístico*. O gate (`code-mode-deny`) é a única
coisa do sistema que está no lado determinístico.

> **R4 — O PORTÃO DA DENSIDADE.**
> O mesmo payload **nao** entra duas vezes na mesma janela. Repeticao nao e
> um peso menor: e o **mesmo** peso, duas vezes. No corpus do Touring,
> **69,8% dos bytes injetados sao repeticao** (§1.4) — e o excedente e
> **concentrado** nas familias que **escapam** do dedup (§3.2, C9).
> *Esta clausula e a "densidade" do cruzamento: o que funciona (fato) e o que
> decide (gate) sao dois eixos; a repeticao e o terceiro.*

**O que o Touring ja cumpre.** O **portao do fato-ou-decisao** esta em boa
medida cumprido no **canal do gate**: o gate reescreve **e** explica (e a unica
coisa do sistema com essa propriedade). O `pre-edit` cumpre R0(a) com 75,6% de
adesao distal. O `compact-warning` cumpre R1 (o momento e observavel: o estado
foi gravado, 118 casos). O `pre-read` e quase silencioso por construcao
(14 blocos em 400 sessoes) — o invariant testado (`pre_read_tests.rs:1221`)
funciona.

**O que o Touring nao cumpre.**
- **R0**: 36,5% do contexto (sermao 16,4% + alertas 20,1%) nao e fato nem
  decisao. O `similar-fail` e o caso extremo: **percentual sem alvo** — inacionavel
  por construcao.
- **R1**: **nada no codigo exige um momento.** O `pipefail-evidence` prova que
  isso custa: fato verdadeiro, 0%.
- **R2**: **10,2% dos bytes** (C-V12) estao acima do teto de 10.000 chars —
  **inerte por mecanismo**, nao por conteudo. E a redundancia nao e tratada:
  `evidence-guard` repete a mesma regra 4.874 vezes; o `touring-suggest` repete
  o que `~/.claude/rules/touring-cli-index.md` e o `CLAUDE.md` ja dizem.
- **R3**: 102 dos 114 blocos acima do teto caem em `outros` — **dois furos
  independentes que se somam no mesmo lugar**: o que a regua nao nomeia e o que
  o canal nao entrega.

### 2.3 A regra em uma linha executavel

```
injete(p)  <=>  (fato_acionavel(p) OU decisao(p))           # R0
            E  existe momento_observavel(p)                   # R1
            E  len(p) <= 10_000                              # R2
            E  nao repetido(p, sessao, epoch)                # R4
            E  traz_motivo(p)                                # R5
canal(p)   =   additionalContext  |  permissionDecisionReason   # R3
```

> **R5 — A REGRA TRAZ O MOTIVO.**
> Uma regra **sem** o motivo obriga o modelo a obedecer a ceegas e produz
> comportamento absurdo em caso de borda; **uma cláusula de motivo** basta para
> o modelo aplicar bom senso (Q3 `[14]` -> `g[19]`).
> *Isto e a razao externa de o `similar-fail` ser um defeito e nao apenas um
> bloco inutil: nao e so que ele nao nomeia o alvo — e que um "similarity=62%"
> sem motivo e sem alvo e o **caso degenerado da regra sem motivo**.*

**Os tres eixos estruturais, em uma frase:** *fato* (o modelo nao tinha) ·
*decisao* (reescreve e explica) · *densidade* (nao repete). O sermao isolado nao
tem nenhum dos tres; o `[generic]` tem so o terceiro contra si.

**O quinto (R5) e o que separa um bloco inerte de um bloco acionavel com o
mesmo volume de bytes.**

**A regra de substituicao, herdada do harness2 e da regra do bem publico:** a
janela e um bem publico — *injecao nova **substitui**, nunca **soma***.

---

## 3. Veredito complementar sobre `ANALISE-CONSOLIDADA.md`

### 3.1 O que a analise consolidada **acerta** (e a rodada 4 confirma)

| # | afirmacao | o que a rodada 4 acrescenta |
|---|---|---|
| A1 | A separacao **fato x sermao** e real e e o que decide o efeito | O **mecanismo** (Context7 C10: "context from the application, not as messages from the user") e a **forma vencedora do payload** (notebook "GitHub Repos" [80],[81],[85],[67]: `file:line` + trecho minimo + id estavel) |
| A2 | O `similar-fail` e um **defeito de interface, nao de dados** | **Confirmado e completado**: o `reason` **esta** gravado (`FailedPath.reason`, `pensieve.rs:27`, preenchido em `post_bash.rs:202` com `outcome.error_pattern`); o que falta e o **comando** (§5) |
| A3 | O gate (`code-mode-deny`) funciona **por coercao** | O corpus qualitativo mostra **por que** ele e o melhor bloco: ele **reescreveu e explicou** — harness2: *"a hook cannot"* |
| A4 | A **familia `outros`** precisa ser fechada | **Confirmado e agravado**: `outros` contem o melhor e o pior bloco do sistema ao mesmo tempo; e o melhor viaja por `permissionDecisionReason`, canal que a taxonomia de contexto **nao classifica** |
| A5 | O interruptor precisa ir para Rust nativo, e o destino e `touring-foundation` | **Confirmado por mim**: `touring-foundation` tem **zero** dependencias internas (e a raiz do DAG) e e dependencia direta de `touring-cli`, `touring-hook-handlers`, `touring-hooks-shared`, `touring-hooks`, `touring-hooks-core`, `touring-intelligence` => **nenhuma aresta nova** |
| A6 | O `no-advisory` corta 19,7% (depois da correcao da taxonomia) | Mantem-se, mas com a **ressalva do canal**: o que ele corta depende da taxonomia, e a taxonomia e a que nao fecha (§4, C4) |

### 3.2 O que a rodada 4 **corrige**

| # | onde | antes | **agora** | por que |
|---|---|---|---|---|
| **C1** | `fonte-notebooklm-claude-code.md` §3.1 e a premissa da matriz | "o criterio do Touring e a versao local da **regra oficial** da Anthropic" | **CORRIGIDO COM PRECISAO.** O "So what?" **nao e publicado como criterio para o canal de hook** — a varredura de 65 docs first-party nao achou criterio editorial de conteudo injetado; existem 12 criterios **mecanicos** (canal, teto, forma, proveniencia, dedup, reentrancia). **MAS** o scout corrigido (contra o notebook certificado) mostra que as **tres perguntas coincidem** com o conselho do time de AI aplicada da Anthropic sobre construcao de contexto em geral (Q1 `[26]`,`[44]` -> `g[7]`, `g[19]`) — *"coincide com o filtro ja declarado no codigo do Touring"*. => **O veredito honesto**: o criterio do Touring **nao e original**, mas **nao ha criterio oficial publicado para o canal em que ele opera** — ele e a **versao local, e nao oficial, de um conselho que existe fora** | `scout-context7.md` §4.1 e §6.2 · `scout-notebooklm-claude-code.md` §3 (Q1, `g[7]`, `g[19]`) |
| **C2** | `ANALISE-CONSOLIDADA.md` §8.8 (linha "A/B de corpus: `s8_corpus_ab.rs` … comparando **resultado**, nao adesao") | o harness existente "ja faz A/B de injecao sobre corpus" e pode ser estendido as 3 arms | **IMPOSSIVEL COM ESSE HARNESS.** `s8_corpus_ab.rs` nao invoca modelo nenhum (verificado: sem `Command::new`, sem cliente de API) — ele **repassa um JSONL de stdout bruto** por `dedup_hook_output` + `turn_budget` e compara **bytes antes/depois**. Ele mede **payload**, nao **desfecho**. Estende-lo as 3 arms mede *quanto contexto cada braco remove* — numero ja conhecido (19,7%). Para medir **resultado** e preciso reexecucao com modelo no circuito | `crates/touring-hooks-shared/tests/s8_corpus_ab.rs` (lido) |
| **C3** | `ANALISE-CONSOLIDADA.md` §8.3.2 — "os dashboards do `SessionStart` saem pelo canal do `systemMessage` (sem depender da classificacao de familia); o `off` nao depende dela" | o `off` e independente da classificacao | **CONTRA.** O corpus mede **zero** `systemMessage` (28.188 `hso.additionalContext`, 0 `systemMessage` — C-V12). Os dashboards viajam por `additionalContext` => **a classificacao decide tambem o `off` para os dashboards**. O ramo `systemMessage` da politica nativa e **hoje codigo morto** | `round4/evidence-v1-v2.md` §V2 + `measure_injection_effect.py:134-152` (`extract_channels` conta `systemMessage` na raiz e em `hookSpecificOutput`) |
| **C4** | `ANALISE-CONSOLIDADA.md` §4.6 / §2.1 (a classificacao por familia) | `outros` = 10,4% "sem taxonomia", numero residual | **`outros` e o resto, e contem os dois extremos** — o pior (`[generic]`) e o melhor (`[G2 …]`/`[G8 …]`, com a propriedade "explicou por que"), este ultimo por um canal (`permissionDecisionReason`) que a taxonomia **nao classifica**. **E um achado de INSTRUMENTO, nao de conteudo** | `round4/scout-corpus-qualitativo.md` §2.16-2.17 + `measure_injection_effect.py:145-152` |
| **C5** | `ANALISE-CONSOLIDADA.md` §4 e `IMPACTO-POR-HOOK.md` §4 (o contrato "So what?" aparece em 4 lugares) | "o mesmo contrato aparece em `pre_bash.rs:3`, `pre_edit.rs:582`, `instructions_loaded.rs:17` e `pre_edit_prevention.rs:309`" | **A letra do contrato existe em 1 lugar.** `pre_read.rs:3-7` tem as 3 clausulas; `pre_bash.rs:3-4` tem a variante "RELEVANT past failure"; `pre_edit.rs:580-586`, `instructions_loaded.rs:13-17` e `pre_edit_prevention.rs:306-310` tem **parafrase** ("HIGH-SIGNAL-ONLY", "if there is nothing actionable", "or stay silent") — nao o contrato. E o teste (`pre_read_tests.rs:1221`) cobre **um** caminho | leitura direta dos 5 arquivos |
| **C6** | `IMPACTO-POR-HOOK.md` §5.1 — a proposta `FailureMatch { similarity, reason, command }` | a correcao e "de superficie" | **Metade nao e de superficie.** `FailedPath` (`pensieve.rs:23-33`) **nao tem campo de comando** — so `state_hash`, `reason`, `depth`, `timestamp`, `hit_count`. Nomear o comando exige **estender o registro**, nao so a consulta (§5) | `pensieve.rs:23-33` + `post_bash.rs:199-206` |
| **C8** | `analysis-2026-10-26.md` §2 e `ANALISE-CONSOLIDADA.md` (o headline) | "injecao = **27,8%** da janela do modelo" | **12,0% medido hoje** (C-WIN: 400 sessões, transcript completo, separando stdout-cru). O **27,8%** fica como **numero herdado, nao reconciliado**: o numerador concorda (43,0 -> 37,1 B/sessão); o que diverge e o **denominador** (2,00x) — a rodada 1 mediu 200 sessões **com** o filtro `attachment\|assistant` e nao separou as **chamadas de ferramenta do modelo** (24,7% da janela). **Nenhum dos dois numeros foi apagado** | §1.3 · C-WIN |
| **C9** | `ANALISE-CONSOLIDADA.md` §7 e §8 (o que falta) | a politica de injecao e a lacuna; o dedup e um detalhe de implementacao | **O dedup ja existe e e maior que a politica.** `injection_dedup::dedup_hook_output` tem **um unico call site de producao** (`crates/touring-dispatch/src/daemon.rs:623`), e **69,8% dos bytes injetados sao repeticao** (§1.4). O que falta nao e a **politica** — e o **alcance** do dedup: os hooks **Python** (`client/hooks/evidence_guard.py`, `delegation_nudge.py`, `check_context.sh`…) sao processos separados e **nunca passam** por `run_hook_command`; o caminho **standalone** tambem nao tem dedupe (declarado em `daemon.rs:518-521`); e `MIN_LINE_BYTES = 24` (`injection_dedup.rs:41`) faz `[generic]` (9 B) e `[gabrielgadea]` (14 B) **nunca** serem suprimidos. **E por isso que as 1.577 repeticoes do `evidence-guard` sobreviveram** | §1.4 · `injection_dedup.rs:41,271` · `daemon.rs:518-537,622-623` |
| **C7** | `ANALISE-CONSOLIDADA.md` §7 (a regua) e a premissa da matriz | "a regua mede o que emite" | A regua Rust conhece **8 familias**; a politica Python conhece **26**; o instrumento conhece **16**. **Tres vocabularios.** E o mesmo conteudo tem **nomes diferentes**: "Similar command failed recently" e `past-lessons` na regua Rust (`:229-232`) e `similar-fail` na politica Python. O principio "o nome e o contrato" (§8.4.2) **esta violado entre Rust e Python** | `context_budget.rs:201-237` x `injection_policy.py:82-130` x `measure_injection_effect.py:60-80` |

### 3.3 O que a analise consolidada **nao podia saber** (limites honestos)

1. **Conformidade nao e atribuicao.** O corpus mostra que o efeito **aconteceu**;
   nao que **aconteceu por causa** da injecao. O `pipefail-evidence` (fato
   verdadeiro, 0%) e a prova viva: um fato verdadeiro pode nao ter mudado nada
   porque nao houve momento.
2. **O A/B de resultado continua nao existindo** — e agora sabemos que **o
   harness que se acreditava existir nao serve** (C2).
3. **O harness2 abla o prompt do *solver*, nao hook de agente em coding.**
   Table 6 §E.1 remove system prompt/guardrails/memory/tool guidance do harness
   do executor; nao e proxy perfeito para "hook de 2 KB em `additionalContext`".
4. **Nenhum corpus externo mede adesao a nudge.** O par **98% (hook) vs 60%
   (arquivo de instrucao)** do context-mode (notebook "GitHub Repos" **[61]**) e
   **"% saved" de contexto**, nao adesao — usar esse par como adesao e ler errado
   a fonte. O 60% e o **nivel de enforcement** do arquivo de instrucao
   ("The GEMINI.md file is the only enforcement method (~60% compliance)").
5. **O Figure 11 do harness2 e contra-intuitivo e e o controle honesto:**
   remover a entrada da biblioteca **subiu** o score (+1,9 / +1,7), e o paper
   atribui parte disso a reexecucao. => **O controle "o modelo faria isso de
   qualquer jeito?" provavelmente nao e 0.**
6. **Os tres corpora (400 / 799 / 400-sessoes) nao sao um so.** Densidade por
   familia lida da matriz e enganosa.
7. **O teto de 10.000 chars e um teto de mecanismo, nao de conteudo.** "10,2%
   dos bytes inertes" e **teto superior**: o arquivo + a previa (2.000 chars)
   ainda entrega alguma coisa.
8. **O `post-edit` e o `pre-edit-prevention` tem um problema de momento que a
   conformidade nao revela:** `scout-corpus-qualitativo.md` §1.3(b) mediu **680
   de 700** blocos de "prevention" chegando **depois** do `tool_result` do
   `Edit` — o bloco que se chama "prevention" e, na pratica, um **post-mortem**.
   98,8% de conformidade **e** arrival tardio.

---

## 4. O contra-diagnostico — o que a evidencia externa **nao** sustenta

### 4.1 Minhas afirmacoes anteriores que a evidencia **contradiz**

| # | afirmacao minha (rodadas 1-4) | a fonte que a contradiz | correcao |
|---|---|---|---|
| **A** | *"O 'So what?' do Touring e a regra da Anthropic"* / *"o criterio do Touring e a versao local da regra oficial"* (`fonte-notebooklm-claude-code.md` §3.1) | `scout-context7.md` §4.1: 65 docs first-party varridos, **zero** criterio editorial de conteudo injetado. O que existe sao 12 criterios **mecanicos** (canal, teto, forma, proveniencia, dedup, reentrancia, precedencia) | **O "So what?" e LOCAL.** A versao oficial mais proxima e a da **doc de hooks** (C5-C7: condicional + precondicao verdadeira + falha -> calar) — que e **operacional** (com guarda de `exit 0` e codigo de referencia) enquanto a do Touring e **declarativa** (docstring + 1 teste). O cruzamento certo e: **o harness2 e mais estrito na evidencia; a doc da Anthropic e mais estrito na implementacao** |
| **B** | *"O `systemMessage` e canal de modelo e o `off` nao depende da classificacao"* (`ANALISE-CONSOLIDADA.md` §8.3.2) | C-V12: 28.188 `additionalContext`, **0** `systemMessage` | **O ramo `systemMessage` nunca e exercido.** A classificacao decide o `off` tambem para os dashboards (§3.2, C3) |
| **C** | *"o A/B de corpus (s8_corpus_ab.rs) … comparando resultado, nao adesao"* (§8.8) | o arquivo, lido: harness **sem modelo** | **Mede payload, nao desfecho** (§3.2, C2) |
| **D** | *"a correcao do `similar-fail` e de superficie"* (`IMPACTO-POR-HOOK.md` §5.1) | `pensieve.rs:23-33`: `FailedPath` nao tem comando | **Duas camadas**: a consulta (superficie) e o **registro** (o comando precisa ser gravado) (§3.2, C6) |
| **E** | *"o 'So what?' aparece em 4 lugares no codigo"* (`IMPACTO-POR-HOOK.md` §4) | leitura dos 5 arquivos | **1 lugar literal + 1 variante + 3 parafrases** (§3.2, C5) |
| **F** | *"o harness2 nao tem criterio de contexto; o 'So what?' do Touring e o criterio mais proximo"* | `scout-harness2.md` §3.4 | **O harness2 TEM um criterio, e e mais estrito**: o criterio do momento (`prompts.py:858-868`). O que ele nao tem e a *palavra* "so what?" (0 hits). A formulacao e outra, a exigencia e maior |

### 4.2 O que a evidencia externa **nao** sustenta, e eu nao vou afirmar

- **Que o corte do sermao melhora o resultado.** A medicao mostra **adesao
  zero**; isso e uma **premissa**, nao um resultado. O harness2 mede o valor
  *retido* (§E.1) e o resultado do leave-one-out foi **contra-intuitivo**.
  Enquanto o A/B nao existir, o corte e uma **aposta com base em 0% de
  adesao observada** — e a adesao proximal ja se provou o instrumento errado
  (`IMPACTO-POR-HOOK.md` §3.1: o conteudo que funciona e justamente o que o
  instrumento marcaria como "nao seguido").
- **Que o `pre-edit` e bom porque "funciona".** 75,6% de conformidade **sem
  atribuicao** (limite 3.3.1) — e o `pre-edit` e a familia **maior** (23,4% do
  contexto). Ela pode estar conformando por coincidencia de caminho.
- **Que a doc oficial "corrobora" o veredito fato x sermao.** O proprio scout marca
  como **parafrase sua** (§5.1, §6.3): a doc sustenta *condicoes de contorno
  mecanicas*; a explicacao do 0,3% e leitura do scout. O Context7 **nao**
  publica adesao, e o corpus **nao** e da Anthropic.
- **Que "hook ~98% vs arquivo-de-instrucao ~60%" prova que o Touring deveria
  cortar a prosa.** A metrica e **"% saved"**, e o eixo e *hook programatico vs
  arquivo de instrucao* — nao *sermao vs fato*. O Touring **ja** usa hook; o
  problema dele nao e o canal, e o **conteudo**.
- **Que a "forma vencedora do payload" (ponteiro + trecho + coordenadas) ja
  funciona no Touring.** O `pre-edit` tem `file:line` e 75,6% — o que **nao**
  foi demonstrado e que a forma seja a **causa**. O Trail-Code (notebook
  "GitHub Repos" **[81]**) e o A/B que isola a forma: **push** (injetar) -32%
  custo / -60% latencia / **correcao igual (93%)**; **pull** (sob demanda)
  **98% de correcao, 5 pontos acima do frio**. => **push e pull sao trade-offs,
  nao hierarquia** — e o Touring hoje e 100% push.
- **Que existe um criterio editorial oficial que "o Touring ja cumpre"** (§4.1, A).

---

## 5. Desenho S-2 — o `similar-fail` que nao nomeia o alvo

### 5.1 O defeito, em duas camadas (nao uma)

**Camada 1 — a consulta descarta o dado que existe.**
`Pensieve::check_known_failure` (`pensieve.rs:117`) devolve `Option<f64>` (so o
score). O dado existe: `FailedPath.reason` (`pensieve.rs:27`) e o
`outcome.error_pattern` gravado pelo produtor (`post_bash.rs:202-206`), e o
`AnnIndex::query` (`ann_index.rs:171`) devolve `Vec<(String, f32)>` — o `String`
**e o indice** do `FailedPath` correspondente (`pensieve.rs:181-189`: o id e
`i.to_string()`). O `let (_, similarity) = results.first()?;` (`:125`) **joga o
id fora**.

**Camada 2 — o comando que falhou nunca foi gravado.**
`FailedPath` (`pensieve.rs:23-33`) tem `state_hash`, `reason`, `depth`,
`timestamp`, `hit_count` — **nenhum campo de comando**. O produtor tem o comando:
`BashOutcome.command` (`models.rs:126`, preenchido por `truncate(command, 500)` em
`post_bash.rs:464`), e `post_bash.rs:199-206` grava
`command_to_states(&outcome.command)` + `reason`. **O comando e
conhecido-so-como-estados, nao como texto.**

=> **Um alerta que nomeia so o erro ainda nao e acionavel** ("Exit code 1" nao
diz o que evitar). A correcao completa precisa das duas camadas.

### 5.2 API nova — assinatura exata

```rust
// crates/touring-intelligence/src/reasoning/pensieve.rs

/// O que uma falha conhecida tem a dizer: QUANTO parecido e O QUE (e qual alvo).
#[derive(Debug, Clone)]
pub struct FailureMatch {
    /// Similaridade do cosseno com a falha conhecida (0.0-1.0).
    pub similarity: f64,
    /// Motivo legivel gravado no momento da falha (`FailedPath.reason`).
    pub reason: String,
    /// O QUE foi consultado e falhou: comando (post_bash) ou caminho (pre_edit/pre_write).
    pub subject: String,
}

impl Pensieve {
    /// Detalhe da falha conhecida mais proxima, acima do limiar.
    ///
    /// Mesma semantica de [`check_known_failure`](Self::check_known_failure) —
    /// a diferenca e que devolve o QUE, nao so o QUANTO.
    pub fn check_known_failure_detail(&self, state: u64) -> Option<FailureMatch>;

    /// Mesma coisa para uma sequencia de estados.
    pub fn check_known_failure_detail_seq(&self, states: &[u64]) -> Option<FailureMatch>;
}
```

**Compatibilidade: `check_known_failure` passa a ser um wrapper** — assinatura
inalterada, comportamento **identico**:

```rust
pub fn check_known_failure(&self, state: u64) -> Option<f64> {
    self.check_known_failure_detail(state).map(|m| m.similarity)
}
```

E `check_known_failure_seq` idem (`pensieve.rs:139`).

### 5.3 Diff conceitual

**5.3.1 — `FailedPath` ganha o alvo (camada 2).**

```diff
 pub struct FailedPath {
     pub state_hash: u64,
     pub reason: String,
+    /// O QUE falhou (comando ou caminho), para que o aviso possa nomear o alvo.
+    pub subject: String,
     pub depth: usize,
     pub timestamp: u64,
     pub hit_count: u32,
 }
```

**Como manter a assinatura de `record_failure` sem quebrar nada:** `record_failure`
continua recebendo `(states, reason, depth)`, e o **alvo entra por um setter
separado** no mesmo registro. **Recomendado: opcao (i) abaixo.**

**Tres opcoes avaliadas** (com o criterio do harness2: *"o que a injecao nova
troca por que"*):

| opcao | o que muda | breaking? | veredito |
|---|---|---|---|
| **(i) campo novo + setter** | `FailedPath.subject` + `pub fn set_subject(&mut self, i: usize, subject: &str)`; o produtor chama o setter depois do `record_failure` | **nao** — `record_failure` intacto; `FailedPath` so e construido em `pensieve.rs:100` (verificado) | **RECOMENDADA** — a menor superficie |
| (ii) `record_failure(&mut self, states, reason, depth, subject)` | assinatura muda | **sim** para quem chama: 3 sitios no repo (`post_bash.rs:205`, `pretask.rs:547`, `pretask.rs:564`); `FailedPath` e `pub use`d em `reasoning/mod.rs:228` | aceitavel, mas **por que** quebrar se (i) basta? |
| (iii) so a camada 1 (devolver o `reason`) | nenhuma assinatura muda; mensagem nomeia o **erro**, nao o alvo | nao | **insuficiente** — "Exit code 1" nao e acionavel |

**5.3.2 — a consulta devolve o detalhe (camada 1).**

```diff
 pub fn check_known_failure_detail(&self, state: u64) -> Option<FailureMatch> {
     if self.failures.is_empty() { return None; }
     let embedding = compute_embedding(&[state], self.dim);
     let results = self.index.query(&embedding, 1);
-    let (_, similarity) = results.first()?;
+    let (id, similarity) = results.first()?;          // <- o id E o indice do FailedPath
     let sim = f64::from(*similarity);
     if sim >= self.threshold {
-        Some(sim.clamp(0.0, 1.0))
+        let idx: usize = id.parse().ok()?;             // id = i.to_string() (pensieve.rs:181-189)
+        let failure = self.failures.get(idx)?;
+        Some(FailureMatch {
+            similarity: sim.clamp(0.0, 1.0),
+            reason: failure.reason.clone(),
+            subject: failure.subject.clone(),
+        })
     } else { None }
 }
```

> **Por que isso e seguro:** o `id` do ANN **e** o indice posicional no
> `self.failures` — `rebuild_index` (`pensieve.rs:181-189`) grava
> `(i.to_string(), emb)`, e `self.failures` e `self.embeddings` crescem **em
> par** (`record_failure` faz `push` nos dois, `pensieve.rs:100-108`). O
> `get(idx)` com `?` e fail-safe: se o indice nao bater, devolve `None` — nunca
> um `FailedPath` errado.

**5.3.3 — o produtor grava o alvo (camada 2, opcao i).**

```diff
 // crates/touring-hook-handlers/src/hooks/post_bash.rs:199-206
 if !outcome.success {
     let states = crate::shared::command_hash::command_to_states(&outcome.command);
     if !states.is_empty() {
         let reason = outcome.error_pattern.as_deref().unwrap_or("unknown error");
         let depth = states.len();
         match runtime.learning.pensieve.try_borrow_mut() {
             Ok(mut pensieve) => {
-                pensieve.record_failure(&states, reason, depth);
+                let before = pensieve.failure_count();
+                pensieve.record_failure(&states, reason, depth);
+                // o comando que falhou — o dado que o aviso precisa para ser acionavel
+                pensieve.set_subject_for_last(&outcome.command);
             }
```

> **Cuidado de deduplicacao (verificado em `pensieve.rs:92-98`):** quando o
> `state_hash` ja existe, `record_failure` **incrementa `hit_count` e retorna
> cedo — sem novo `FailedPath`**. => o `subject` gravado e o do **primeiro**
> registro daquele estado. Isso e **honesto** (o aviso diz "similar command
> failed", nao "this exact command failed") e **deve ser dito na mensagem**:
> "similar" ja esta no texto. O setter deve ser **idempotente por
> `state_hash`**: nao sobrescrever o alvo de um registro mais antigo com o de
> um novo.

**5.3.4 — as mensagens mudam.**

```diff
 // crates/touring-hook-handlers/src/hooks/pre_bash.rs:229-239
- let penalty = match states.first() {
-     Some(&single) if states.len() == 1 => pensieve.check_known_failure(single),
-     _ => pensieve.check_known_failure_seq(&states),
- };
- penalty.map(|sim| {
-     format!(
-         "Similar command failed recently (similarity={:.0}%)",
-         sim * 100.0
-     )
- })
+ let m = match states.first() {
+     Some(&single) if states.len() == 1 => pensieve.check_known_failure_detail(single),
+     _ => pensieve.check_known_failure_detail_seq(&states),
+ };
+ m.map(|m| {
+     format!(
+         "Similar command failed recently (similarity={:.0}%): {m.subject} — {m.reason}",
+         m.similarity * 100.0
+     )
+ })
```

O mesmo formato, com a forma **"ponteiro + trecho + coordenadas"** que o corpus
externo nomeia como vencedora (notebook "GitHub Repos" **[80]**, **[81]**,
**[85]**, **[67]**) e **igual a que o Touring ja emite** no canal do echo
(`⚡ last fail: <comando> — <erro>`, familia `post-tool-echo` /
`failure-echo`). => **Alinhar `similar-fail` ao `failure-echo`**: uma forma, um
criterio.

> **Nota de forma (nao de conteudo):** a forma `⚡ last fail:` e a que **ja
> nomeia o comando e o erro** (ver `scout-corpus-qualitativo.md` §2.3, exemplo
> real: `⚡ last fail: touring — Exit code 1`). O `similar-fail` deixa de ser
> uma categoria propria e vira **o mesmo bloco, com a similaridade a mais**.

**5.3.5 — `pre_edit.rs` e `pre_write.rs`.**

```diff
 // crates/touring-hook-handlers/src/hooks/pre_edit.rs:431-436  (e pre_write.rs:582-589)
- let penalty = match states.first() { ... pensieve.check_known_failure(single) ... };
- penalty.map(|sim| signals.push((1.2, format!("⚠ pensieve: similar path had recorded failures ({:.0}% match)", sim * 100.0))))
+ let m = match states.first() { ... pensieve.check_known_failure_detail(single) ... };
+ m.map(|m| signals.push((1.2, format!("⚠ pensieve: similar path failed before ({:.0}% match): {} — {}", m.similarity * 100.0, m.subject, m.reason))))
```

> **Este bloco consulta por CAMINHO, nao por comando.** Em `pre_edit`/`pre_write`
> os `states` vem do hash do **caminho do arquivo** (mesma estrutura
> `command_to_states`, mas com o path). => o `subject` que o `FailedPath`
> carrega precisa ser **o que foi consultado** — e como o mesmo `Pensieve`
> recebe registros de **dois dominios** (comando, do `post_bash`; caminho, do
> `pre_edit`), o `subject` gravado pelo produtor **precisa ser rotulado**.
> **Recomendacao:** `FailedPath.subject: String` (rotulo do produtor, ex.
> `"cmd: cargo test"` / `"path: crates/x/src/y.rs"`) — assim `pre_bash` mostra o
> comando e `pre_edit` mostra o caminho, e a mensagem e coerente nos dois.
> **Isto e uma correcao de escopo ao DAG**: o S-2 tal como escrito ("uma API,
> tres consumidores") nao fecha — sao **quatro** consumidores em **dois
> dominios** (comando x caminho), e o dominio do `got.rs` e um **terceiro**
> (§5.4).

### 5.4 Os consumidores — e o `got.rs`, que e outro dominio

| # | consumidor | o que consulta | o que a mensagem precisa | acao |
|---|---|---|---|---|
| 1 | `pre_bash.rs:229-239` | `command_to_states(&short_key)` — **forma do comando** | o **comando** que falhou | **corrige** |
| 2 | `pre_edit.rs:431-436` | estados do **caminho** | o **caminho** | **corrige** (com `subject` rotulado) |
| 3 | `pre_write.rs:582-589` | estados do **caminho** | o **caminho** | **corrige** (com `subject` rotulado) |
| 4 | `pretask.rs:321-327` | `check_known_failure_seq(&state_hashes)` — **tarefa** | o motivo | **corrige** (o motivo basta) |
| 5 | `got.rs:962` | `check_known_failure(result.node_id)` — **`node_id` numerico** | **nada** — e um **penalty numerico** (`result.score -= penalty`) | **nao muda** |

> **O `got.rs` e uma homonimia que precisa ser dita.** A mesma API
> `check_known_failure` e usada no **raciocinio** (`got.rs:962`, com
> `node_id` — um id de no de arvore, nao um hash de comando) e nos **hooks**
> (comando/caminho). Sao **dois dominios** que compartilham uma funcao cujo
> nome sugere um unico. A assinatura nova `FailureMatch` continua servindo aos
> dois (`similarity` e o que o `got.rs` quer), mas **o `reason` de um
> `node_id` nao significa nada** — => `FailureMatch.reason` deve ser
> `Option<String>` ou acompanhado de um discriminador, ou o `got.rs` deve ficar
> com a API numerica.
> **Recomendacao:** manter `check_known_failure` (numerica) **intocada** para o
> `got.rs`, e introduzir `check_known_failure_detail` (com detalhe) **so onde
> ha mensagem**. Isso mantem o `got.rs` intocado e elimina o risco de
> **expor um `reason` sem sentido** para o consumidor de penalty.

### 5.5 Risco de breaking change

| API | quem usa | breaking? |
|---|---|---|
| `check_known_failure` (assinatura) | `pre_bash.rs:231`, `pre_edit.rs:431`, `pre_write.rs:582`, `got.rs:962` + 6 internas (`pensieve.rs:268-325`, testes) | **NAO** — a assinatura nao muda; so o corpo passa a delegar |
| `check_known_failure_seq` (assinatura) | `pretask.rs:321`, `pre_bash.rs:232`, `pre_edit.rs:432`, `pre_write.rs:583` | **NAO** |
| `record_failure` (assinatura) | `post_bash.rs:205`, `pretask.rs:547`, `pretask.rs:564` | **NAO** (opcao i) |
| `FailedPath` (struct) | construido **so** em `pensieve.rs:100`; `pub use` em `reasoning/mod.rs:228` | **QUASE-NAO** — campo novo e aditivo; nenhum sitio externo constroi o struct no repo (verificado por grep) |
| `FailureMatch` (novo) | — | **NAO** — aditivo |

`touring wiring impact check_known_failure --depth 2` -> **5 consumidores
diretos**, profundidade maxima 1. `touring ast blast pensieve.rs` ->
**blast_radius 30** (o arquivo e consumido por 30 arquivos, mas a **API** so
por 5). **Sem bindings Python** (`touring-bindings/src/python/simd_bindings.rs`
nao menciona pensieve — verificado) e **sem tools do servidor**
(`crates/touring-server/src/` nao menciona pensieve — verificado). => **O risco
de breaking change e baixo e medido.**

### 5.6 Plano de teste (S-2)

| # | teste | onde | o que prova |
|---|---|---|---|
| 1 | `check_known_failure_detail` devolve `reason` e `subject` do registro que casou | `crates/touring-intelligence/src/reasoning/pensieve.rs` (modulo de testes, ao lado de `check_known_failure_returns_some_for_recorded_state`, `:276`) | a camada 1 funciona: o `reason` gravado chega |
| 2 | `check_known_failure` continua devolvendo **o mesmo `f64`** que antes | idem (`:262`, `:276`, `:292`) | **nao-regressao de comportamento** — o wrapper nao muda nada |
| 3 | `check_known_failure_detail` devolve `None` quando o indice do ANN nao casa | idem | fail-safe do `get(idx)?` |
| 4 | o produtor grava o alvo: `record_failure` + setter => `subject` nao-vazia | `crates/touring-intelligence/src/reasoning/pensieve.rs` (novo teste) | a camada 2 funciona |
| 5 | **a mensagem nomeia o alvo** | `crates/touring-hook-handlers/src/hooks/` (teste do `pre_bash`) | o objetivo do S-2: o payload contem o comando que falhou e o motivo |
| 6 | `pre_edit`/`pre_write` mostram o **caminho**, nao o comando | idem | coerencia por dominio (o `subject` rotulado) |
| 7 | a deduplicacao por `state_hash` **preserva** o alvo do primeiro registro | idem | o aviso nao mente sobre "qual" alvo |
| 8 | `got.rs` continua usando a API numerica e **nao** ve `reason` | `crates/touring-intelligence/src/reasoning/got.rs` | a homonimia nao contamina o dominio do penalty |

**Teste de aceitacao (o que a rodada 4 exige):** *"qual teste prova que a
mensagem agora nomeia o comando"* -> e o **#5**: um teste que monta o
`HookResponse` do `pre_bash` com um Pensieve que tem uma falha gravada, e
**afirma que o `additionalContext` contem o texto do comando e o motivo**. Sem
isso, o defeito volta em silencio — que e a licao *instrumento que mente*.

---

## 6. Desenho S-3 — a taxonomia por conteudo

### 6.1 O defeito, medido

**Tres vocabularios para o mesmo corpus:**

| sistema | onde | quantas familias | como classifica |
|---|---|---|---|
| **regua Rust** | `crates/touring-cli/src/cli/context_budget.rs:200` (`injection_family`), `:298` (`attributed_family`) | **8 + `other`** | **marcador literal** (substring, em qualquer posicao) (`:201-237`) + marca do emissor (`EMITTERS`, `:264-283`) |
| **politica Python** | `client/hooks/injection_policy.py:82-93` (Advisory, 8) + `:107-130` (Fact, 18) | **26** | marcadores + regex |
| **instrumento** | `scripts/measure_injection_effect.py:60-80` (`FAMILIES`) | **16** | regex de **conteudo** + regra do sermao sem cabecalho (`:116-123`) |

**Consequencias medidas:**
1. **A regua nao ve 19% do contexto** — e a causa é **precisa**: não é que a
   regra seja "por cabeçalho" (ela casa por **substring, em qualquer posição** —
   `context_budget.rs:239-241`); é que os **5.630 blocos do sermão sem cabeçalho
   não têm marcador nenhum** ⇒ caem em `other`. ⇒ **O conserto do S-3 é "casar
   também o que hoje não casa", não "mudar para posicional".**
2. **A regua VÊ o `similar-fail`** — sob o nome **`past-lessons`** (marcador
   `"Similar command failed"`, `context_budget.rs:229-232`). O que ela **não**
   vê é o sermão sem cabeçalho e as **8 famílias novas**.
3. **As DUAS taxonomias do produto divergem em 9 famílias** — e o comentário da
   política Python diz que ela "segue a taxonomia da régua":

| conceito | régua (Rust) | política (Python) |
|---|---|---|
| aviso de falha recorrente | **`past-lessons`** | `similar-fail` |
| dashboard de sessão | **`session-start`** | `plugin-hook` |
| contrato do code mode | **(não tem)** | `session-start-contract` |
| "MUST touring doctor -j" | **(não tem)** | `session-contract` |
| programa fundido | **(não tem)** | `code-mode-contract` |
| exit-code mascarado | **(não tem)** | `pipefail-evidence` |
| compaction | **(não tem)** | `compaction-gaps` / `compact-warning` |
| `[generic]` | **(não tem)** | `generic-noise` |

   ⇒ **O defeito não é só "a régua classifica por marcador": é que existem DUAS
   taxonomias, em DUAS linguagens, sem paridade** — e a que o **interruptor**
   usa é a que divergiu. É a lição do repo paga de novo: *"predicado escrito
   duas vezes nunca se encontra"*. **O nome é o contrato — e há dois contratos.**
4. **O `outros` da regua contem o melhor e o pior bloco** (§1.1-b) — e o
   melhor viaja por `permissionDecisionReason`, **canal que a taxonomia de
   contexto nao classifica** (`measure_injection_effect.py:134-152`).

### 6.2 A mudanca — assinatura exata

**Uma tabela canonica, em `touring-foundation`, classificada por CONTEUDO.**

```rust
// crates/touring-foundation/src/injection_family.rs   (NOVO)

/// De qual classe o payload e — o mesmo contrato de nome que a politica usa.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FamilyKind { Advisory, Fact, Unknown }

/// Uma familia da taxonomia: nome (o CONTRATO), classe, e como casa-la.
pub struct Family {
    /// O nome — o contrato entre a regua, a politica e o instrumento.
    pub name: &'static str,
    /// A classe da politica.
    pub kind: FamilyKind,
    /// Padroes de conteudo que identificam a familia.
    pub markers: &'static [&'static str],
}

/// A tabela canonica — 26 nomes, os mesmos da politica Python.
pub const FAMILIES: &[Family] = &[ /* … */ ];

/// As familias presentes num payload (um payload pode misturar classes).
pub fn families(text: &str) -> Vec<&'static str>;

/// A familia primaria (a primeira da ordem canonica).
pub fn family(text: &str) -> &'static str;
```

**O porque de `touring-foundation`** — verificado por mim neste ciclo:
`touring-foundation` tem **zero** dependencias internas (e a **raiz** do DAG) e
e dependencia **direta** de `touring-cli`, `touring-hook-handlers`,
`touring-hooks-shared`, `touring-hooks`, `touring-hooks-core` e
`touring-intelligence`. => **colocar a tabela la cria ZERO arestas novas** — o
mesmo argumento do §8.4, agora conferido em vez de inferido.

**A regua passa a ler de la:**

```diff
 // crates/touring-cli/src/cli/context_budget.rs
-use <tabela local de needles>;
+pub use touring_foundation::injection_family::{family, families};
+
+/// A familia atribuida, com a marca do emissor como fallback.
+/// (a assinatura de `attributed_family` NAO muda — os 2 consumidores ficam intactos)
 pub fn attributed_family(text: &str, emitter: Option<&str>) -> &'static str {
     let claimed = emitter.and_then(emitter_claim);
     if let Some((family, Claim::Owns)) = claimed { return family; }
     match injection_family(text) {
         "other" => claimed.map_or("other", |(family, _)| family),
         family => family,
     }
 }
```

`injection_family` (`context_budget.rs:200`) **mantem o nome e a assinatura**
(`pub fn injection_family(text: &str) -> &'static str`) e passa a delegar a
tabela canonica — assim **os 2 consumidores** (`touring ast blast
context_budget.rs` -> blast_radius **2**: `kpi.rs` e o proprio arquivo) **nao
mudam**, e `touring-cli` ja declara `touring-foundation` (verificado no
`Cargo.toml`).

### 6.3 O que muda no comportamento (e o que NAO muda)

| # | mudanca | efeito |
|---|---|---|
| 1 | a tabela passa de **8 needles de cabecalho** para **26 familias por conteudo** | a regua passa a ver o que a politica ja corta — o `no-advisory` e a regua passam a falar do **mesmo** conjunto |
| 2 | `past-lessons` -> **`similar-fail`** (mesmo nome da politica) | **elimina a colisao de nome** entre Rust e Python (§6.1-2) |
| 3 | a regra do **sermao sem cabecalho** (do instrumento, `measure_injection_effect.py:120-122`) entra na tabela Rust | fecha o furo de 19% que a rodada 3 nomeou (D1) |
| 4 | **`permissionDecisionReason` passa a ser classificado** | o melhor bloco do sistema (as razoes do gate) sai do `outros` e ganha veredito |
| 5 | a marca do emissor (`EMITTERS`) **continua** como fallback | o `Claim::Owns`/`Fallback` e **informacao do emissor, nao do texto** — nao e defeito de cabecalho, e metadado |

> **O que NAO muda (por principio):** a **assinatura** de `injection_family` e
> de `attributed_family`, o **nome** das 2 familias que a regua ja conta, e a
> **estrutura** de `EMITTERS`. A mudanca e de **conteudo da tabela**, nao de
> interface — e o mesmo movimento do `cila.rs` (`foundation/src/cila.rs:1-13`:
> *"Both crates depend on this one, so the answer can be given once"*).

### 6.4 Plano de teste (S-3)

| # | teste | onde | o que prova |
|---|---|---|---|
| 1 | a tabela tem **exatamente** os 26 nomes da politica Python | `crates/touring-foundation/src/injection_family.rs` | **as duas listas nao divergem** — o teste que falta hoje |
| 2 | cada nome da tabela e **conhecido** pela politica | idem (cross-check contra a lista exportada) | o guard `guard_injection_policy.py` ganha um par em Rust |
| 3 | `injection_family` classifica **por conteudo**: o payload `MUST touring run --lang …` (sem cabecalho) -> `code-mode-contract` | `crates/touring-cli/src/cli/context_budget.rs` (teste) | **o furo de 19% (D1) esta fechado no Rust** |
| 4 | `"[generic]"` -> `generic-noise` | idem | o bloco-lixo tem nome |
| 5 | `"Similar command failed recently"` -> `similar-fail` (nao `past-lessons`) | idem | **a colisao de nome esta resolvida** |
| 6 | o payload do gate (`[G2 … -> REESCRITO]`) -> familia propria, via `permissionDecisionReason` | idem | **o melhor bloco sai do `outros`** |
| 7 | a regua e a politica concordam sobre **cada** payload do corpus | teste de corpus (C-400) | **o `no-advisory` corta o que a regua mediu** — hoje nao ha como saber |
| 8 | `attributed_family` **nao mudou de assinatura** | compilacao + teste existente | os 2 consumidores (`kpi.rs`) intactos |

> **O teste #7 e o que fecha o ciclo:** hoje a regua (8 familias) e a politica
> (26) contam coisas **diferentes** sobre o **mesmo** corpus, e nada acusa. Um
> teste que roda os dois classificadores sobre os payloads reais e exige
> **conjunto igual** e a **regra de ouro** da §8.4 ("o nome e o contrato")
> virada em assercao.

### 6.5 REGRA #0 — potencializar

A tabela nova **nao e codigo morto**: ela tem **dois** consumidores
imediatos — a regua (`context_budget.rs`) e, no passo seguinte, a politica
nativa (S-4). E ela **substitui** uma tabela local (`context_budget.rs:201-237`)
— ou seja, **corta** codigo em vez de somar. Se S-4 for adiado, a tabela
continua consumida pela regua — nao ha modulo orfao.

---

## 7. S-4 e S-5 — entram nesta onda?

### 7.1 O criterio aplicado

O harness2 (`prompts.py:456-458`): *"Prefer an on-demand skill over new
always-on prose; **if you add always-on text, cut something else**."*
E o NotebookLM [3,4]: `fast-jev-compaction` — **eviccao, nao resumo**.

| item | e "soma" ou "corte"? | o que troca |
|---|---|---|
| **S-2** (o `similar-fail` nomeia o alvo) | **soma de bytes** — a mensagem cresce ~40 B | **troca um bloco inerte por um bloco acionavel** — o mesmo orcamento, outra qualidade. E o `no-advisory` pode entao corta-lo por classe |
| **S-3** (a taxonomia por conteudo) | **corte** — a tabela local (8 needles) e **substituida** por uma canonica (26) | nao injeta nada; **faz a regua e a politica falarem do mesmo conjunto** |
| **S-4** (politica nativa em Rust) | **corte** — o switch corta 19,7% do contexto | troca **um processo Python por invocacao** (~15-25 ms por hook) por uma **decisao em Rust** |
| **S-5** (A/B de corpus) | **nao injeta** — e experimento | o que troca e **a incerteza**, nao a janela |

### 7.2 Veredito

> ## **S-4: entra nesta onda, mas ATRÁS de um S-4' novo (o alcance do dedup).
> ## S-5: NAO entra — e o experimento de maior valor NAO é o que o DAG propôs.**

**S-4 — entra.** Tres razoes, todas verificadas:
1. **E um corte, nao uma soma** (criterio do harness2): o switch nativo
   **remove** 19,7% do contexto que hoje chega ao modelo.
2. **O custo por invocacao e real e medido**: §8.1 — 1 processo `python3` por
   hook (~15-25 ms) contra uma mediana de hook de 60-430 ms. Num workspace com
   6+ sessoes, isso e latencia em **todo** hook.
3. **A dependencia esta resolvida**: `touring-foundation` e a raiz do DAG, tem
   **zero** dependencias internas, e e dependencia direta de **todos** os
   candidatos => **nenhuma aresta nova** (§6.2). O modulo novo nao e um
   modulo orfao: nasce consumido pela regua (S-3) e pelo switch.

**S-4' (NOVO, e o PRIMEIRO da onda): o alcance do dedup.** A aritmetica da §1.4
decide: o dedup vale **8,4% da janela** contra **2,2%** do corte do sermao
(**3,7x**), e a **infra ja existe** (`injection_dedup.rs`). O que falta e
**cobertura**, e a causa de cada repeticao sobrevivente esta identificada
(§3.2, C9):

| o que escapa | onde | correcao |
|---|---|---|
| hooks **Python** (processo separado) | `client/hooks/evidence_guard.py`, `delegation_nudge.py`, `check_context.sh`, `skill_picker_hook.py` | o dedup precisa alcancar o output **antes** do hook responder, ou o hook precisa reusar o mesmo estado |
| caminho **standalone** (daemon fora) | `crates/touring-hooks/src/main.rs` (`forward_daemon_output`) | aplicar `dedup_hook_output` tambem ali — o precedente ja existe no mesmo crate (`injection_dedup`) |
| campos na **raiz** | `additionalContext` / `systemMessage` na raiz | `dedup_hook_output` so le `/hookSpecificOutput/additionalContext` (`injection_dedup.rs:283-286`) |
| linhas **curtas** | `[generic]` (9 B), `[gabrielgadea]` (14 B) | `MIN_LINE_BYTES = 24` (`injection_dedup.rs:41`) — o piso que protege a diretiva protege tambem o lixo |
| **epoch** | a cada compact, o epoch avanca e o bloco volta | `advance_epoch` reemite o que ja foi dito (decisao declarada) |

**S-4' e um CORTE, e o corte maior.** Ele e, por construcao, a unica das
alavancas que **nao precisa de A/B de desfecho para ser justificada**: o dedup
e uma funcao **pura** do stream de payload (o mesmo texto nao entra duas
vezes), o byte economizado e **exato**, e o A/B **ja existe**
(`s8_corpus_ab.rs` mede exatamente isto, com aceite de N >= 200 payloads,
queda >= 40% e controle positivo). => **O S-5 do DAG (o A/B) pode ser
reaproveitado como o instrumento do S-4'**, em vez de precisar de um harness
novo.

**Mas S-4 so entra depois de S-3** — porque a politica nativa precisa da
**mesma tabela** que a regua. Se a politica nativa nascesse com a sua propria
tabela (o caminho que a §8.4 sugere), teriamos **duas** taxonomias em Rust
— exatamente o defeito que a §8.4.2 quer eliminar. **A ordem S-3 -> S-4 e
consequencia, nao preferencia.**

**O harness do S-5 nao precisa ser inventado do zero** — o notebook certificado
publica **tres metodos** de A/B que sao exatamente o que falta (Q4 `[15]`-`[17]`,
`[6]`, `[18]`,`[19]` -> `g[50]`; `g[7]`):

| metodo | o que faz |
|---|---|
| **Caliper** | testes **pareados com vs. sem** a skill, para ver se o ganho paga o custo em tokens |
| **`claude plugin eval`** | roda tarefas **em paralelo** com e sem o plugin/skill |
| **suite de evals em CI** | Anthropic recomenda **20 a 50 tarefas reais** ja concluidas no projeto, rodando em CI, para pegar regressao ao mexer em prompt/skill/modelo |

**S-5 — NAO entra nesta onda (e o desenho que a §8.8 propoe esta errado).
E o DAG tem a prioridade errada:** o experimento de maior valor **nao** e
"tres bracos da politica" — e **"dedup ligado vs desligado"**, porque 69,8% dos
bytes sao repeticao e a infra do dedup ja existe.
`crates/touring-hooks-shared/tests/s8_corpus_ab.rs` (16.477 B, **existe**,
verificado) **nao tem modelo no circuito**: lido, ele repassa um JSONL de stdout
bruto por `dedup_hook_output` + `turn_budget` e compara **bytes**. Ele mede
**payload**, nao **desfecho**. Estende-lo as tres arms produziria *"quanto
contexto cada braco remove"* — numero **ja conhecido** (19,7%).
=> **O S-5 verdadeiro precisa de um harness diferente**: reexecucao com o
**mesmo** benchmark/executor/subset/seed, julgada pelo **mesmo** juiz
(disciplina de comparabilidade do harness2, `[48]`), com a arm fixada por
**variavel de ambiente** (`TOURING_HOOK_INJECTION=full|no-advisory|off`) — e o
**controle**: reexecutar a arm `full` duas vezes, para medir o **ruido de
reexecucao** (o "Repeated execution may also contribute", `scout-harness2.md`
§Figure 11).

**Custo de adiar S-5 (o que fica sem medir):**
- **A premissa do corte continua sendo uma aposta.** O corte do sermao (18,7% do
  contexto) e do alerta (20,1%) se apoia em **adesao zero observada** — e a
  adesao proximal **ja se provou o instrumento errado** (`IMPACTO-POR-HOOK.md`
  §3.1: o conteudo que funciona e justamente o que o instrumento marcaria como
  "nao seguido"). **Sem o A/B, nao se sabe se o corte perde resultado.**
- **O `off` fica sem justificativa de escolha**: `no-advisory` (o recomendado) e
  defensavel pela medicao de adesao; `off` (que mata tambem o canal do fato,
  35,5% do volume com 52% de conformidade) **nao** tem defesa medido. =>
  **Decision risk: alguem liga `off` e perde o fato.**
- **O ruido de reexecucao continua nao medido** — e o Figure 11 do harness2
  mostra que ele **nao e zero** (+1,9 / +1,7 quando remover uma entrada da
  biblioteca).
- **A atribuicao continua aberta**: conformidade != atribuicao. Nenhum corpus
  externo mede adesao a nudge (limite 3.3.4).

**Custo de adiar S-4 (o que fica sem medir):** o custo por invocacao do Python
(15-25 ms x todo hook) e a **duplicacao de politica em duas linguagens** — que
e o que produziu a colisao `past-lessons`<->`similar-fail` (§6.1-2) e o
`session-start` (§8.4.2). **Adiar S-4 e barato; adiar S-5 e caro.**

### 7.3 A ordem que eu proponho

```
S-2   (correcao de conteudo: o similar-fail nomeia o alvo)
      └─ nao depende de nada; entrega o bloco que hoje e inerte (5,9% da injecao)

S-4'  (ALCANCE DO DEDUP)  <-- PRIMEIRO DA ONDA, pelo maior ganho em bytes
      └─ o corte maior (8,4% da janela); infra ja existe; o A/B ja existe
      └─ precisa de S-3? NAO — a cobertura do dedup nao depende da taxonomia

S-3   (taxonomia por conteudo, 1 tabela nova + 1 reexport)
      └─ nao depende de S-2; e pre-requisito de S-4

S-4   (politica nativa em Rust, foundation + 4 sitios)
      └─ depende de S-3 (mesma tabela)

S-5'  (A/B DE DESFECHO, COM MODELO NO CIRCUITO, harness NOVO)
      └─ depende de S-4 (precisa da arm nativa) — e o passo 7 da §8.7
      └─ responde a pergunta que o dedup NAO responde: "o corte perde resultado?"
```

**O que muda em relacao ao DAG:** S-5 nao e "estender o `s8_corpus_ab.rs`" — e
**um harness novo**, com modelo no circuito. O `s8_corpus_ab.rs` **continua
testando a dedupe** (e isso que ele faz, e funciona). **Reescrever o S-5 como
"estender o s8" seria escrever um teste que passa sem medir o desfecho** — a
licao *teste que passa sem provar*.

---

## 8. Riscos de implementar

| # | risco | gravidade | o que exige de verificacao |
|---|---|---|---|
| R1 | **Quebrar o `check_known_failure` ao delegar** — o `got.rs:962` usa o `f64` para penalizar score; um erro de conversao muda o raciocinio | **ALTA** | teste de nao-regressao que compare o `f64` **antes** e **depois** da delegacao, no mesmo `Pensieve` com a mesma seed de estados (a ANN e deterministica: `compute_embedding` e modular hashing) |
| R2 | **O `subject` errado** — `pre_edit`/`pre_write` consultam por caminho e o `FailedPath` guarda o comando => mensagem incoerente ("similar path failed before: `cargo test`") | **ALTA** | teste que monte um `FailedPath` de comando e force um consumidor de caminho; e o inverso. **Este risco nao aparece se S-2 for feito pela opcao (b) (deixar `pre_edit`/`pre_write` fora)** — o que e uma razao a mais para ele ser considerado |
| R3 | **Deduplicacao por `state_hash`** — `record_failure` retorna cedo e **nao** cria `FailedPath` novo => o `subject` gravado e o do primeiro registro | MEDIA | teste que grava 2 vezes o mesmo `state_hash` com alvos diferentes e exige que o alvo **persista o primeiro** |
| R4 | **O id do ANN pode nao bater com o indice** se `failures` e `embeddings` dessincronizarem | MEDIA | invariante: `record_failure` faz `push` nos **dois** (`pensieve.rs:100-108`); o `get(idx)?` e fail-safe. **Teste**: forcar `id` invalido => `None`, nunca `panic` |
| R5 | **Indexacao orfa**: `pub fn` novo em `touring-intelligence` que ninguem consome (se S-2 for feito e o consumidor nao mudar) | MEDIA | `touring wiring orphans -j` **antes e depois** — o delta tem de ser 0. **REGRA #0**: se `check_known_failure_detail` ficar sem consumidor, ele **nao entra** |
| R6 | **A taxonomia nova divergir da politica Python** — duas listas de 26 nomes, uma em Rust e outra em Python, **duas fontes** | **ALTA** | teste #1 e #2 de §6.4 (contagem exata + cada nome conhecido). **Ate a politica Python existir em Rust, a lista Python e a referencia** — e o guard `scripts/guard_injection_policy.py` (152 asercoes) deve ganhar o par |
| R7 | **`injection_family` mudar de comportamento** quebra a regua (2 consumidores: `kpi.rs` + o proprio arquivo) | BAIXA | `touring ast blast context_budget.rs` -> blast_radius **2** (verificado). O historico da regua **nao e recalculado** — os numeros de §8.1 da `IMPACTO-POR-HOOK.md` passam a ser de uma taxonomia antiga => **anotar a data da taxonomia em cada saida da regua** |
| R8 | **A unificacao do `HookResponse` (passo 0 da §8.7)** — dois `enum` com o mesmo nome, em crates diferentes, com reexports que divergem | **ALTA** | `grep -rn "enum HookResponse" crates --include=*.rs` -> o numero de definicoes; e a cadeia de reexport (`touring-hook-runtime/src/lib.rs:82` e `:98`) tem de resolver ao **mesmo** tipo. **Este passo e do S-4, nao do S-3** |
| R9 | **Workspace grande + 765 orfaos**: uma mudanca que cria 1 `pub fn` e 1 modulo novo pode aumentar a divida de orfaos | BAIXA-MEDIA | `touring wiring orphans -j` antes/depois (baseline: **765**, `orphan_count`); `touring index status` (baseline: **382.305** simbolos) |
| R10 | **O S-5 mede payload em vez de desfecho** (o erro que a §8.8 ja cometeu) | **ALTA** | o harness **tem que** chamar um modelo. **Prova**: `grep -n "Command::new\|reqwest\|anthropic" <harness>` **tem que** dar resultado. Sem isso, o teste passa sem medir |
| R11 | **Ruido de reexecucao confundindo o A/B** (a licao do Figure 11) | **ALTA** | braco de controle: `full` reexecutado. **Reportar a dispersao** (n, CI), como o paper faz |
| **R13** | **Estender o alcance do dedup toca o caminho do daemon** — `dedup_hook_output` e o filtro central da saida de **todos** os handlers da dispatch table; mudar o que ele suprime muda o output de todo hook | **ALTA** | `touring ast blast crates/touring-hooks-shared/src/injection_dedup.rs`; o **controle positivo** do A/B (`s8_corpus_ab.rs`: "o primeiro payload de cada sessao passa intacto", e `dedupe_molde_suprime_telemetria_mas_nunca_diretiva_com_path_novo`) tem de continuar **verde** — um dedup mais agressivo que **suprima uma diretiva com path novo** e um defeito de correcao, nao de economia |
| **R14** | **Baixar `MIN_LINE_BYTES` (24) faz o dedup passar a suprimir linhas curtas** — e a razao do piso e proteger a diretiva curta (que e a que manda o modelo olhar o arquivo certo) | **ALTA** | o teste `dedupe_linhas_curtas_nunca_sao_suprimidas` (`injection_dedup.rs:427`) e o que impede isso; se o piso baixar, esse teste **tem de mudar de regra** (de "nunca" para "nunca, exceto `[]`/ruido"). **Prova**: um teste que injete `MUST ast meta a.rs` (curta, com path) e exija que ela **sobreviva** |
| **R15** | **O numero da janela e uma media que depende do corpus** — 12,0% (400 sessoes, 3 projetos) vs 27,8% (200 sessoes, filtro aplicado). Uma decisao tomada sobre o numero errado corta o alvo errado | MEDIA | **nunca** decidir por porcentagem: registrar **o denominador** junto com o numero (a §1.3 e o modelo). A regua (`touring kpi -j`) ja reporta `injected_bytes_per_turn` com teto — o **por turno** e o numero de decisao, nao o por janela |
| R12 | **A regra do "momento" (R1 da §2.2) nao e implementavel hoje** — nao ha telemetria que ligue um bloco a um desfecho | MEDIA | e a **lacuna estrutural**: a medicao de conformidade e distal e observacional. **A honestidade e declarar isso no A/B**, nao prometer um "momento" que o Touring nao sabe detectar |

---

## 9. Indice de evidencias verificadas neste ciclo

**Codigo (verificado por `touring index find`, `touring ast blast`, `touring wiring impact`, `grep`):**

| fato | file:line |
|---|---|
| `FailedPath { state_hash, reason, depth, timestamp, hit_count }` — **sem campo do alvo** | `crates/touring-intelligence/src/reasoning/pensieve.rs:23-33` |
| `record_failure(&mut self, states: &[u64], reason: &str, depth: usize)` — **3 call sites** | `pensieve.rs:80` · `post_bash.rs:205` · `pretask.rs:547,564` |
| `FailedPath` construido **so** em `pensieve.rs:100` | `pensieve.rs:100` |
| deduplicacao por `state_hash` retorna cedo (o alvo do 1º registro prevalece) | `pensieve.rs:92-98` |
| `check_known_failure -> Option<f64>` / `check_known_failure_seq -> Option<f64>` | `pensieve.rs:117` / `:139` |
| `AnnIndex::query -> Vec<(String, f32)>` — o `String` **e** o indice | `ann_index.rs:171` · `pensieve.rs:181-189` |
| o produtor grava o comando e o motivo | `post_bash.rs:199-206` |
| a mensagem que nao nomeia o alvo | `pre_bash.rs:236` · `pre_edit.rs:436` · `pre_write.rs:589` · `pretask.rs:326` |
| `BashOutcome.command` (truncado a 500) e `.error_pattern` | `models.rs:126,137` · `post_bash.rs:464,469` |
| `check_known_failure` — **5 consumidores diretos**, profundidade 1 | `touring wiring impact check_known_failure --depth 2` |
| `pensieve.rs` blast_radius **30** | `touring ast blast crates/touring-intelligence/src/reasoning/pensieve.rs` |
| `context_budget.rs` blast_radius **2** (`kpi.rs` + o proprio) | `touring ast blast crates/touring-cli/src/cli/context_budget.rs` |
| a regua Rust: **8 familias + `other`**, por needle | `context_budget.rs:201-237` |
| `attributed_family` — assinatura | `context_budget.rs:298` |
| a regua conta `systemMessage` como contexto | `context_budget.rs:369,375` |
| a politica Python: **26 familias** (8 Advisory + 18 Fact) | `client/hooks/injection_policy.py:82-93,107-130` |
| o instrumento: **16 familias**, casadas por **conteudo** | `scripts/measure_injection_effect.py:60-80,116-123` |
| o `extract_channels` conta `systemMessage` na raiz e em `hookSpecificOutput` | `measure_injection_effect.py:134-152` |
| `s8_corpus_ab.rs`: replay de payload, **sem modelo** | `crates/touring-hooks-shared/tests/s8_corpus_ab.rs` (16.477 B) |
| o contrato "So what?" — **1 lugar literal + 1 variante + 3 parafrases** | `pre_read.rs:3-7` · `pre_bash.rs:3-4` · `pre_edit.rs:580-586` · `instructions_loaded.rs:13-17` · `pre_edit_prevention.rs:306-310` |
| o teste do invariant — **1** caminho | `pre_read_tests.rs:1221` |
| `touring-foundation` tem **zero** dependencias internas | `crates/touring-foundation/Cargo.toml` |
| `touring-foundation` e dependencia direta de 6 crates | `Cargo.toml` de `touring-cli`, `touring-hook-handlers`, `touring-hooks-shared`, `touring-hooks`, `touring-hooks-core`, `touring-intelligence` |
| baseline: **382.305** simbolos · **765** orfaos | `touring index status -j` · `touring wiring orphans -j` |
| `crates/touring-foundation/src/injection_policy.rs` **nao existe** | `ls` — confirmado |
| o dedup tem **um unico** call site de producao | `crates/touring-dispatch/src/daemon.rs:623` -> `maybe_dedup_output` (`:522-537`) -> `dedup_hook_output` (`:529`) |
| o caminho **standalone** nao tem dedupe (declarado) | `daemon.rs:518-521` (docstring de `maybe_dedup_output`) |
| o dedup so le `/hookSpecificOutput/additionalContext` | `crates/touring-hooks-shared/src/injection_dedup.rs:283-286` |
| o piso de linha suprimivel e **24 B** — `[generic]` (9 B) e `[gabrielgadea]` (14 B) escapam | `injection_dedup.rs:41` (`MIN_LINE_BYTES`) + teste `:427` |
| `evidence_guard.py` e hook **Python** — processo separado, nao passa por `run_hook_command` | `client/hooks/evidence_guard.py:311` (emite "EVIDENCE GUARD"); nenhum hook Python chama dedup (grep) |
| o stdout cru do Touring e **695 B** em 400 sessoes | C-WIN |
| a reconciliacao da janela: numerador concorda, denominador 2,00x | §1.3 |
| **69,8%** dos bytes injetados sao repeticao | C-RED |

**Fontes externas (arquivo:linha, como citadas nos scouts):**

| regra | fonte |
|---|---|
| o criterio do momento + precedencia + "duplicated instruction dilutes the rest" | harness2 `prompts.py:858-868` |
| "if you add always-on text, cut something else" | harness2 `prompts.py:254`, `:456-458` |
| "ALWAYS-ON PROSE IS THE EXPENSIVE OPTION" | harness2 `prompts.py:129-133`, `:237-241` |
| "Prose can be stopped reading; a hook cannot" | harness2 `prompts.py:212-215`, `:494-497` |
| truncamento que fabrica defeito (`obs_trunc` >= 10.000) | harness2 `spec.py:163-167` |
| contexto como arquivo, prompt como ponteiro | harness2 `context.py:25-27` |
| a Tabela 6 §E.1 (prompt -1,5 / skills -5,3 / scripts -3,7) | paper arXiv 2610.14224 §E.1 |
| o Figure 11 (leave-one-out **+1,9 / +1,7**) | paper §4 / Figure 11 |
| teto de 10.000 chars -> arquivo, *"Claude Code doesn't ask it to"* | Context7 `doc-websites_code_claude-context.txt:348` |
| "context from the application, not as messages from the user" | Context7 `doc-websites_code_claude-context.txt:1224` |
| "Don't copy untrusted tool output into it" | Context7 `doc-llmstxt_code_claude_llms_txt-hooks.txt:928` |
| `additionalContext` aninhado, senao ignorado | Context7 `doc-websites_code_claude-hooks.txt:1181` |
| `systemMessage` — contradicao entre 2 fontes first-party | Context7 `doc-websites_code_claude-hooks.txt:676` x `doc-anthropics_claude-code-hooks.txt:805` |
| **NAO existe criterio editorial de conteudo injetado** (65 docs varridos) | `scout-context7.md` §4.1, §6.2 |
| a janela como **bem publico**; as 6 categorias de "What to Delete" | notebook "Claude Code " **[7]**, **[1-6]** |
| `fast-jev-compaction` = **eviccao**, nao resumo | notebook "Claude Code " **[3,4]** |
| `pond-db` — disciplina de staleness | notebook "Claude Code " **[2]** |
| hook ~98% vs arquivo-de-instrucao ~60% (**% saved**, nao adesao) | notebook "GitHub Repos" **[61]** |
| Trail-Code push vs pull (push -32% custo / 93% correcao; pull 98%) | notebook "GitHub Repos" **[81]** |
| XERJ — 2,7x/26x menos tokens, **mesmo** solve rate | notebook "GitHub Repos" **[85]** |
| a forma vencedora do payload (`file:line` + trecho + id estavel) | notebook "GitHub Repos" **[80]**, **[85]**, **[67]** |
| o better block: o gate **reescreve e explica** | `round4/scout-corpus-qualitativo.md` §2.17 + harness2 `prompts.py:494-497` |
| "falar nao garante fazer"; hook = 100% de conformidade | `round4/scout-notebooklm-claude-code.md` §4.5 (Q4 `[1]`,`[2]`,`[7]`,`[8]` -> `g[10]`, `g[38]`, `g[47]`, `g[50]`) |
| a regra de ouro: o menor contexto que entrega o resultado | idem §4.1 (`g[7]`) |
| 80%+ do system prompt deletado sem queda nos benchmarks | idem §4.2 (`g[7]`) |
| context rot: 18/18 modelos; queda de 13,9% a 85% com a informacao presente | idem §4.2 (`g[33]`, `g[47]`) |
| 60-70% dos turnos sao checagens deterministicas que pertencem a borda | idem §4.2 (`g[7]`, `g[14]`) |
| regra **sem motivo** = compliance cego; **com motivo** = bom senso | idem §3 (Q3 `[14]` -> `g[19]`) |
| os 4 modos de falha: poisoning / distraction / confusion / clash | idem §4.3 (`g[33]`) |
| 2 de 9 skills venceram um arquivo vazio em 450 execucoes | idem §4.6 (`g[50]`) |
| metodos de A/B publicados: Caliper, `claude plugin eval`, suite de evals em CI | idem §4.7 (`g[50]`, `g[7]`) |
| a janela real (12,0%) e a repeticao (69,8%) | `round4/evidence-janela-e-redundancia.md` |
| as DUAS taxonomias divergem em 9 familias | `context_budget.rs:201-237` x `injection_policy.py:82-130` |

---

_Analise por TACO sob ordem de Gabriel Gadea, 26/10/2026 — rodada 4, S-1 do DAG
`task_1793033502719494636`. Todo simbolo citado foi verificado com
`touring index find` / `touring ast blast` / `touring wiring impact` / `grep`;
onde a verificacao nao foi possivel, a celula diz `[nao verificado]`._
