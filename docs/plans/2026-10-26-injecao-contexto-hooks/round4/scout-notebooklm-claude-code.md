---
plan_id: 2026-10-26-injecao-contexto-hooks
type: Research
title: "NotebookLM 'Claude Code ' (91 fontes, 25/09/2026) - boas praticas e evidencia sobre injecao de contexto em agentes de codigo"
description: "Extrato verificado do notebook NotebookLM indicado pelo Gabriel, com citacao de fonte resolvida por indice global e o mapa de falhas metrologicas do proprio notebook."
timestamp: 2026-10-26
author: touring-scouter (round4)
notebook_id: 65c3ecb1-a4ce-4901-9864-a2b28c04b469
notebook_title: "Claude Code "
source_count: 91
declared_date: 2026-09-25
queries_run: 6
wrong_notebook_discarded: 91efd704-8540-4a1c-950b-5e233b3e1ab3
---

# NotebookLM "Claude Code " (91 fontes) - injecao de contexto em agentes de codigo

> **Escopo**: o que a fonte externa afirma sobre hooks/injecao de contexto, janela de
> contexto, quando calar, qualidade do conteudo injetado, efeito de sermoes, e metrica de
> harness. Toda afirmacao leva `[n]` **como o notebook a emitiu**, e o indice global
> resolvido `g[gi]` com o titulo da fonte.

## 0. Metodo - como resolver as citacoes (leia antes de usar qualquer [n])

**Descoberta metodologica que muda o uso deste artefato.** O `[n]` que o NotebookLM
imprime **nao e o indice global da fonte no notebook**. E **posicional dentro do
subconjunto de fontes que aquela resposta consultou**. Prova executada, nao inferida:

1. **Teste de fonte unica.** `nlm notebook query <id> --source-ids 9585f4d3-1e5e-4425-ac40-2bffa7063eb3`
   (fonte global **g[10]**, "Anthropic Just Revealed 10 NEW Rules for Claude Skills").
   A resposta cita a regra dos 500 linhas como `[1]`, **nao** `[10]`.
2. **Teste de duas fontes.** `--source-ids 689ba9fd-...` (global **g[1]**, Matt Pocock)
   `,9585f4d3-...` (global **g[10]**). A resposta diz, por nome,
   *"According to Anthropic's guidelines detailed in 'Anthropic Just Revealed 10 NEW Rules
   for Claude Skills'"* e marca `[1]`. A fonte global 10 e citada como `[1]`.

**Consequencia**: um `[n]` lido cru nao identifica fonte. A resolucao autoritativa esta
no campo `references[]` da resposta JSON, onde cada entrada traz
`{source_id, citation_number, cited_text}`. Foi assim que **todas** as citacoes abaixo
foram resolvidas para o indice global real e o excerto literal. **Sem `references`, nao ha
citacao defensavel.**

| `[n]` da resposta | resolve via | indice global | titulo |
|---|---|---|---|
| Q2 `[1]` | `references[0].source_id` = `9585f4d3-...` | **g[10]** | Anthropic Just Revealed 10 NEW Rules for Claude Skills |

**Conferencia de identidade**: o `cited_text` recuperado para Q2 `[1]` e a transcricao do
video que **ensina** a regra dos 500 linhas ("...it is best practice to keep your main
skill.md file under 500 lines and everything else gets split into its own files...").
O piso de 500 linhas pertence a **g[10]**, nao a g[1]. A resolucao esta correta.

**Teto de fontes**: as 6 consultas citem ate `g[91]`
(`tokio-prompt-orchestrator - crates.io`). O teto do notebook alvo e 91. Um `[n]` que
parasse em 42 indicaria o notebook errado.

## 1. Procedencia do notebook

| campo | valor | como foi obtido |
|---|---|---|
| titulo | `Claude Code ` (com **espaco final** em ASCII) | `nlm notebook list --full` |
| notebook_id | `65c3ecb1-a4ce-4901-9864-a2b28c04b469` | `nlm notebook get` |
| fontes | **91** | `nlm notebook get` -> `source_count` |
| `created_at` | **2026-09-25T22:44:15Z** | `nlm notebook list --full` |
| `updated_at` | 2026-10-26T16:47:03Z | `nlm notebook list --json` |
| data declarada pelo Gabriel | "25 de setembro de 2026" | **CONFERE** com `created_at` |

### 1.1 A armadilha do titulo duplicado (descarte registrado)

| notebook_id | titulo literal | fontes | created_at | destino |
|---|---|---:|---|---|
| `91efd704-8540-4a1c-950b-5e233b3e1ab3` | `Claude Code` (sem espaco) | 42 | 2025-06-27T14:58:49Z | **DESCARTADO** |
| `65c3ecb1-a4ce-4901-9864-a2b28c04b469` | `Claude Code ` (com espaco) | **91** | **2026-09-25T22:44:15Z** | **ALVO** |

O erro ja tinha sido cometido uma vez nesta rodada: a primeira consulta (e o primeiro
rascunho de extrato) foi feita contra o id de 42 fontes. **Todo o material extraido do
`91efd704` foi descartado** e nao aparece neste artefato. Qualquer `batch`/match por
titulo `"Claude Code"` resolve para o id errado; o id literal e obrigatorio.

### 1.2 Que material e este (limitacao de proveniencia)

**Resposta direta do notebook** (Q6, `[1]` -> **g[38]** ECC, `[3,7-9]`): **nenhuma das 91
fontes e documentacao oficial da Anthropic.** O corpus e integralmente de terceiro:

- **Videos do YouTube** que *transcrevem* e reinterpretam anuncios e guias da Anthropic
  (`Anthropic Just Revealed 10 NEW Rules for Claude Skills` g[10], `... 12 New Rules for
  Prompting Opus 5.5` g[11], `Anthropic Engineers Just 10x'd Everyone's Claude Code`
  g[7], `Anthropic Just Donated Claude Skills. Here's What to Delete.` g[8]).
- **Repos GitHub**: `affaan-m/ECC` g[38], `google-research/harness2` g[41],
  `google-research/reasoning-bank` g[42], `langchain-ai/open-swe` g[43],
  `github/spec-kit` g[40], `anomalyco/opencode` g[39], `yvgude/lean-ctx` g[46],
  `sopaco/terrain` g[44].
- **crates.io**: `pond-db` g[87], `memvid-core` g[86], `reflex-search` g[89],
  `tokio-prompt-orchestrator` g[91], `iocraft` g[85], `formualizer` g[84].
- **PDFs / Docs proprios**: `Arquitetura Agentica e Code Mode` g[14] e g[15],
  `fast-jev-compaction-guia-completo.pdf` g[83].

**Como usar este artefato**: e uma **sintese de terceiro sobre material da Anthropic**,
nao a fonte primaria. Onde o excerto recuperado (`cited_text`) e a fala do autor do
video, o numero e a citacao do video; o conteudo por tras pode ser da Anthropic, mas
**a media foi三个人 e a medicao nao e reproduzivel aqui**. Quando um numero aparece
so no texto da resposta do notebook e **nao** no excerto recuperado, isso e **sintese do
notebook**, e marco como tal.

### 1.3 Datas declaradas pelas fontes (Q6)

| data | evento | fonte |
|---|---|---|
| 17/09/2026 | `fast-jev-compaction` criado (Tamara Tran); 7.335 stars em 02/10 | g[83] |
| 21/09/2026 | variante npm publicada | g[83], g[28] |
| **25/09/2026** | caso do app de cursos do Matt Pocock ("On September twenty-fifth, in the app Matt uses to make his courses") | g[68] |
| 26/09/2026 | fixture do adaptador Devin CLI no `pond-db` | g[87] |
| 01/10/2026 | lancamento dos Claude Code Mods + ECC v2.2.3 | g[3], g[38] |
| 04/10/2026 | skills de AI do Matt Pocock v1.3 | g[68] |
| 16/10/2026 | Open Source Friday: Spec Kit Part 2 | g[40] |
| **23/10/2026** | data mais recente citada em qualquer fonte (expiracao de cota) | Q6 |

> **Coerencia interna**: o notebook foi criado em 25/09/2026, mas **continua sendo
> atualizado** - as fontes citam eventos de 16/10 e de 23/10/2026. O corpus e de
> **setembro-outubro/2026**, coerente com a data que o Gabriel indicou. A unica fonte
> com "25 de setembro de 2026" como **evento de conteudo** (e nao de criacao) e g[68].

## 2. O que o notebook afirma sobre injecao de contexto

### 2.1 O harness e a camada que injeta; o hook e o middleware dele

> "a harness is just everything that wraps around the LLM that is not the actual LLM
> itself. So in our case it's claude code. It's the system prompt. It's the hooks. It's the
> tools that it has..." - **g[16]** "CLAUDE CODE ADVANCED FULL COURSE (3 HOURS)", via Q1 `[11]`

O notebook mapeia o ciclo de vida de injecao do Claude Code (Q1 `[9]`..`[17]`):

| ponto de injecao | para que serve, segundo o notebook | fonte global |
|---|---|---|
| **SessionStart** (`startup\|resume\|clear\|compact`) | re-injetar contexto duravel, `NOTAS-DA-SESSAO.md`, `PASSAGEM.md` pos-compactacao, instintos aprendidos | g[15], g[38] |
| **UserPromptSubmit** | roteamento dinamico de modelo, selecao de skill por classificador, deteccao de segredos, troca de schema MCP pesado por indice enxuto | g[7], g[15], g[38], g[19] |
| **PreToolUse** | barreiras de seguranca, bloqueio de leitura de `.env`, bloqueio de `rm -rf`/`git push --force`, limite do blast radius | g[15], g[38], g[19], g[14] |
| **PostToolUse** | **Output Diet**: reduz saida massiva de terminal a poucas milhares de linhas e grava o log bruto em disco | g[38], g[47] |
| **Stop** | checar tarefas pos-execucao (testes de regressao, changelog, verificacao de PR) | g[7], g[38] |

**Teto de caracteres no SessionStart**: o ECC define `ECC_SESSION_START_MAX_CHARS` com
**padrao de 8.000 caracteres**, justamente para evitar poluicao (Q1 `[16]` -> **g[38]**).

### 2.2 Tool sprawl e lazy loading: o achado quantitativo central

> "Enthropic also tested loading MCP tool definitions only when needed. They reported an
> **85% reduction in token usage** in their example and their MCP evaluation scores
> improved from **49% to 74% for Opus 4** and from **79.5 to 88.1% for Opus 4.5**" -
> **g[7]** "Anthropic Engineers Just 10x'd Everyone's Claude Code", via Q1 `[30]`, Q3 `[4]`

> "Don't enable all MCPs at once. Each MCP tool description consumes tokens from your
> 200k window, potentially reducing it to **~70k**. Keep under **10 MCPs** enabled per
> project" - **g[38]** ECC, via Q1 `[29]`

O notebook afirma que carregar dezenas de schemas MCP no system prompt e o que produz
*tool sprawl*, e que a migracao para **Skills + CLI local** (com `--json`/`jq`) entrega
resposta deterministica e mais barata (Q1 `[26]`..`[32]`).

### 2.3 Injecao efemera na cauda (Tail-Only Ephemeral Suffix Injection)

Este e o achado mais diretamente aplicavel ao Touring, e o mais raro do corpus.

> "O segundo mecanismo [...] e a **Injecao Efemera Exclusiva na Cauda** (Tail-Only
> Ephemeral Suffix Injection). O gateway mantem o System Prompt, o catalogo base de
> ferramentas e todas as mensagens historicas estritamente imutaveis byte a byte,
> posicionando o checkpoint de cache na fronteira final. A recomendacao de ferramenta ou
> skill gerada pelo Jev e anexada **exclusivamente como um bloco auxiliar no final da
> mensagem do turno**; assim que o turno conclui, o hook remove esse sufixo antes que a
> mensagem integre o historico, preservando **100% de reaproveitamento do cache de
> prompts**" - **g[15]** "Arquitetura Agentica e Code Mode - c jev", via Q1 `[36]`

**Por que importa**: injecao de hint dinamico **nao precisa custar cache**. A dica entra
como sufixo efemero do turno corrente e e removida antes da entrada no historico. O
Touring hoje injeta via `hookSpecificOutput.additionalContext` em hooks `PreToolUse`
(gelo) e `PostToolUse`; o mecanismo do notebook sugere que **o bloco injetado deva ser
retirado do historico no fim do turno**.

### 2.4 Sub-agente como isolamento de contexto

Sub-agentes criam janelas separadas e limpas, evitando poluir a janela principal
(Q2 `[55]`,`[56]` -> **g[55]**, **g[14]**); padrao fan-out/fan-in com modelo barato no
paralelo e modelo forte so no sintetizador (Q2 `[57]`..`[59]`).

## 3. Qualidade e acionabilidade do conteudo injetado

O notebook monta uma tabela de 4 categorias (Q1, tabela "O que torna o conteudo injetado
ALTA QUALIDADE e ACIONAVEL versus PREJUDICIAL"). Traduzida e com a fonte resolvida:

| Categoria | Alta qualidade / acionavel | Prejudicial | fonte global |
|---|---|---|---|
| **Escopo e limites** | "o melhor contexto e a menor quantidade de contexto que entrega o resultado de maior qualidade pelo menor custo" | carregar dezenas de MCP ou manuais gigantescos na inicializacao degrada o desempenho | **g[7]**, **g[33]** |
| **Verificabilidade** | ancorado em dados do sistema, saida de teste deterministico, esquema estrito (`additionalProperties: false`), feedback loop em que a propria skill checa se passou | **Context Poisoning**: o agente grava conclusao errada na memoria e o modelo a trata como verdade, propagando em cascata | **g[10]**, **g[33]** |
| **Justificativa ("so what?")** | regra **com o motivo** (ex.: *"nao use topicos em lista porque este texto sera lido em voz alta num podcast"*) | regra rigida **sem o motivo** => o agente obedece cego, com resultado artificial | **g[19]** |
| **Orientacao** | foco no **verbo** (o que deve acontecer), nao no artefato estatico | **Context Clash / Distraction**: instrucoes contraditorias ou historico longo que faz o modelo entrar em laco | **g[13]**, **g[33]** |

**As tres perguntas do "so what?"** que o notebook sustenta como filtro de injecao
(Q1, `[26]`/`[44]`, via **g[7]** e **g[19]**): a injecao e acionavel (o modelo pode
Fazer algo diferente), e nao-derivavel (nao vai descobrir sozinho) e e especifica deste
ponto. Isso **coincide com o filtro ja declarado no codigo do Touring**.

**Regra sem motivo**: o notebook e explicito de que "rules without a reason" forcam
compliance cega e produzem comportamento absurdo em caso de borda; **uma clausula de
motivo** basta para o modelo aplicar bom senso (Q3 `[14]`, Q1 `[44]` -> **g[19]**).

**Graus de liberdade** (Q2 `[4]`, **g[10]**): *High Freedom* para tarefa criativa;
*Medium Freedom* para template com parametros; *Low Freedom* (script e regra estrita)
para tarefa de alto risco em que erro nao e tolerado.

**Verificacao por loop de feedback** (Q2 `[10]`, **g[10]**; Q4 `[20]`,`[21]`, **g[38]**):
exigir que o agente prove que terminou (teste unitario, linter, screenshot, ou um segundo
agente revisor em contexto limpo) torna o resultado final **2 a 3 vezes melhor** do que
confiar em geracao em fluxo direto.

## 4. Silencio, ruido, redundancia

Esta e a secao mais diretamente aplicavel a politica de injecao do Touring.

### 4.1 A regra de ouro

> "Enthropic's applied AI team recommends using the smallest amount of context that gets
> the result you want. And I turned that into sort of my own saying which is **the best
> context is the smallest amount of context that gets you the highest quality result at
> the cheapest cost**" - **g[7]**, via Q1 `[26]`, Q2 `[43]`, Q3 `[1]`

### 4.2 Comportamento de "silencio por padrao" com numeros

| afirmacao | numero | fonte global | via |
|---|---|---|---|
| **60% a 70% dos turnos** de agente em producao sao checagens deterministicas (`git status`, teste, linter, boolean de seguranca) que um harness avancado resolve **na borda**, sem chamar o LLM - evitando invalidar cache, eliminando latencia e economizando tokens | 60-70% | **g[7]**, **g[14]** | Q3 `[2]` |
| Output Diet / truncamento: Chisel reduz a mesma tarefa de **1.500** para **~600 tokens** (65%) | 65% | **g[55]** | Q3 `[7]` |
| **80%+ do system prompt do Claude Code deletado** pelos engenheiros da Anthropic para os modelos geracao 5, **sem queda nos benchmarks** e com execucao mais rapida | >80% | **g[7]** | Q1/Q2/Q3 `[23]`, Q4 `[27]` |
| Harness enxuto (SMOL coder) reducedo a injecao inicial de ~30.000 para **1.400 tokens** | 21x | **g[47]** | Q2 `[41]`, Q5 `[7]` |
| **18 modelos de fronteira**: todos perderam confiabilidade conforme a janela cresceu (*context rot*) | 18/18 | **g[33]**, **g[47]** | Q3 `[20]`,`[24]` |
| Estudo controlado em 5 modelos: performance caiu **13,9% a 85%** conforme o input crescia, **mesmo com a informacao necessaria presente no prompt** | 13,9%-85% | **g[33]** | Q3 `[1]` |
| Tool sprawl: falhou a tarefa com **46 ferramentas**, passou a **mesma** tarefa com **19** | 46 vs 19 | **g[33]** | Q1 `[28]`, Q2 `[46]`, Q3 `[31]` |

> **Ressalva de honestidade**: o numero "**~30.000 tokens**" aparece no **texto sintetico**
> da resposta do notebook (Q2 `[40,41]`, Q5 `[6,7]`), mas o excerto recuperado
> (`cited_text`) traz a formulacao qualitativa *"these harnesses inject way too much
> context for these small models"* e o numero confirmado *"only takes **1,400** tokens"*
> (**g[47]**). Trato "30.000" como **sintese do notebook**, nao como citacao literal.

### 4.3 Os 4 modos de falha nomeados (Q3, **g[33]**)

1. **Context Poisoning** - erro ou conclusao falsa entra na janela e e tratada como
   verdade; cascata de erros. *Correcao*: validar contra o estado real/testes antes de
   anexar. (Q3 `[26]`,`[27]`)
2. **Context Distraction** - historico longo puxa o modelo por continuacao de padrao;
   uma hora de acoes passadas vira padrao mais forte que o raciocinio treinado, e o agente
   trava em laco. *Correcao*: compactar em resumo e mover o detalhe para arquivo.
   (Q3 `[27]`,`[29]`)
3. **Context Confusion / Tool Sprawl** - definicoes demais dificultam a escolha de
   ferramenta. *Correcao*: curar o conjunto de ferramentas para o passo ativo. (Q3 `[29]`)
4. **Context Clash** - informacoes contraditorias convivem (suposicao velha ao lado da
   correcao nova). *Correcao*: podar notas superadas ou comecar contexto novo. (Q3 `[31]`)

### 4.4 Sermao, boilerplate e redundancia - o que o notebook diz que NAO funciona

| padrao | veredito do notebook | fonte global | via |
|---|---|---|---|
| "think carefully before answering", "don't be lazy", "take it step by step", **CAPS** | **ruido nocivo** em modelos de fronteira; remover *"think carefully"* faz o modelo comecar mais rapido **sem queda mensuravel de qualidade** | **g[19]** | Q3 `[10]`,`[11]`; Q4 `[3]`; Q2 `[18]`-`[20]` |
| "evite um visual generico de IA" | **nao funciona** - o modelo troca um estilo default por outro; a saida e **trocar fundos creme por outras paletas** | **g[19]** | Q3 `[12]`; Q4 `[5]` |
| regra rigida **sem motivo** | compliance cego, comportamento absurdo em borda | **g[19]** | Q3 `[14]`; Q4 `[4]` |
| **duplicar** a mesma regra em `CLAUDE.md`, skills e arquivos de memoria | confunde o roteador interno e polui o contexto | **g[14]**, **g[16]** | Q3 `[15]`-`[17]` |
| **100 skills ativas** | 100 chances do modelo escolher a ferramenta errada e "envenenar a sessao" | **g[8]**, **g[55]** | Q3 `[17]`,`[18]`; Q2 `[30]`-`[32]` |
| skills que MCP/CLI substituiu | deletar | **g[8]** | Q2 `[28]`,`[29]` |
| relatorios redundantes (ex.: *Friday brief* + revisao trimestral) | deletar; a Anthropic manteve so o *Monday brief* sob o principio **"menos ruido, mais sinal"** | **g[8]** | Q2 `[33]` |
| paragrafos/explicacoes de conceito que o modelo ja domina | remover - a janela de contexto e **"um bem publico"** | **g[7]** | Q2 `[7]`; Q3 `[9]` |
| "muitas regras" de modelos antigos (*crutches*) | restringem o modelo novo e o impedem de usar a capacidade nativa | **g[19]**, **g[24]**-**g[27]** | Q2 `[24]`-`[27]` |
| `CLAUDE.md` | **abaixo de 200 linhas**, e deve ser **sumario/roteador**, nao carga | **g[50]**, **g[7]** | Q2 `[34]`,`[35]`,`[36]`,`[37]` |
| `SKILL.md` | **abaixo de 500 linhas**; sub-arquivos com **1 nivel** de profundidade; arquivo >100 linhas precisa de **indice no topo**; frontmatter custa ~**100-110 tokens** | **g[10]** | Q2 `[1]`-`[8]`, `[39]` |

### 4.5 "Falar nao garante fazer" - a frase mais importante para o Touring

> "Adicionar instrucoes em texto no prompt ou no arquivo de regras (CLAUDE.md / AGENTS.md),
> mesmo usando letras maiusculas como 'NUNCA FAÇA X', **nao garante conformidade de
> 100%**. O modelo segue a regra na maioria das vezes, mas falha em situacoes de
> estresse, conversas longas ou com casos de borda" - Q4 `[1]`,`[2]` -> **g[10]**,
> **g[50]**

E a recomendacao que o notebook da:

> "Se a regra nao puder ser quebrada em hipotese alguma (ex.: nunca enviar fatura acima de
> \$10.000 sem aprovacao humana), **remova o texto da skill e converta-o em um Hook
> deterministico** do sistema" - Q2 `[11]`,`[12]` -> **g[10]**, **g[7]**; Q4 `[1]`,`[2]`,
> `[7]`,`[8]` -> **g[47]**, **g[38]**

> "Os Hooks executam **fora do contexto do LLM** e interceptam chamadas de forma
> deterministica antes (PreToolUse) ou depois (PostToolUse), **garantindo 100% de
> conformidade** independentemente de o modelo tentar seguir a instrucao ou nao" -
> Q4 `[7]`,`[8]` -> **g[38]**, **g[47]**

### 4.6 O achado mais dura do corpus para o Touring

> "Em um teste pratico rigoroso com **9 das skills mais populares e com mais estrelas no
> GitHub**, testadas ao longo de **450 execucoes no Opus 5.5**, **apenas 2 das 9 skills**
> produziram resultados superiores a um **arquivo de texto vazio (dummy file)** do mesmo
> tamanho. As outras 7 apenas consumiram tokens sem gerar ganho mensuravel" -
> Q4 `[6]`, Q5 `[22]` -> **g[50]** "How the Top 1% Use Claude Code Differently"

Excerto recuperado (mesmo `cited_text`):
> "...one developer actually went away and tested nine of them **at the end of September**.
> He did **450 runs**..." - **g[50]**

**Este e o paralelo exato do achado do Touring**: 7 de 9 skills de terceiros, com o
mesmo tamanho de arquivo, sem ganho mensuravel. O notebook mediu o mesmo fenomeno que o
Touring mediu em 400 sessoes / 28.295 blocos (SERMÃO 18,7% com ~0,3% de adesao;
ALERTA sem alvo 20,1% com 0%; RUÍDO 5,3%) - **por um caminho independente**.

### 4.7 Como medir (o que o notebook oferece como metodo)

| metodo | o que faz | fonte global | via |
|---|---|---|---|
| **Caliper** | testes pareados **com vs. sem** a skill, para ver se o ganho paga o custo em tokens | **g[50]** | Q4 `[15]`-`[17]` |
| **`claude plugin eval`** | roda tarefas em paralelo com e sem o plugin/skill | **g[50]** | Q4 `[6]` |
| **suite de evals em CI** | Anthropic recomenda **20 a 50 tarefas reais** ja concluidas no projeto, rodando em CI, para pegar regressao ao mexer em prompt/skill/modelo | **g[50]**, **g[7]** | Q4 `[18]`,`[19]` |
| **loops de verificacao** | 2 a 3x melhor resultado final | **g[38]**, **g[47]** | Q4 `[20]`,`[21]` |
| **rubrica compilada** (Abide) | compila arquivo de instrucao em rubrica formal e faz pergunta booleana por mudanca de codigo, forçando correcao antes de finalizar | **g[76]** | Q4 `[9]`-`[11]` |

## 5. Metricas e benchmarks de harness mencionados

### 5.1 Harness engineering (Q5)

| resultado | numero | fonte global | via |
|---|---|---|---|
| **Google Research Harness2** - framework plug-and-play que edita **8 componentes** (system prompt, guardrails, memory, tool guidance, subagents, skills, scripts, plugins), com pesos de modelo fixos | WorkBuddy-Bench **+11,5 pts** (GLM-5, Gemini 3.5 Flash); JobBench **+4,5 pts**; LAB **+11,8 pp** em partial pass | **g[41]** | Q5 `[1]`,`[2]`,`[3]` |
| **LangChain Deep Agents** - mesmo modelo (GPT-5.2), **mesmo prompt**, so o harness mudou | **53% -> ~66%** | **g[47]** | Q5 `[4]` |
| **Can Boluk / Grok Code Fast 1** - "the coding harness was the bottleneck" | **6,7% -> quase 70%** | **g[47]** | Q5 `[5]` |
| **SMOL coder** - harness enxuto para modelo local | injecao inicial **1.400 tokens** | **g[47]** | Q5 `[7]` |
| **Runlayer, skills auto-melhoradoras** - Sonnet 4.6 executando um fork do Chromium em AWS Lambda; o distilador extrai skill a cada run | **36% -> 100%** | **g[36]** | Q5 `[9]` |
| **LeanCTX** - gateway de contexto com 16 modos de leitura de arquivo (`map`, `signatures`, `lines:N-M`, `density:0.4`); `density:0.4` retem as linhas de maior entropia | orcamento SDE reduzido a **~40%** dos tokens; 85+ padroes de compressao de saida de shell (git/npm/cargo/docker/terraform) | **g[46]** | Q5 `[14]` |
| **Fast Jev Compaction** | **~1.000.000 -> 86.000 tokens** em ~1 s | **g[83]** | Q5 `[30]` |
| **Shopify (Tobi Lutke)**, loop de auto-research | **53% mais rapido**, **61% menos alocacoes** | **g[68]** | Q5 `[31]`,`[32]` |
| **Tokio Prompt Orchestrator** - 24 agentes Claude Code em 1 RTX 4070, com deduplicacao semantica | uso de API reduzido a **26%** (**74% de reducao**) | **g[91]** | Q5 `[34]` |
| **Memvid-core** em LoCoMo | **+35% SOTA**, **+76% em raciocinio multi-hop**, latencia P50 **0,025 ms** | **g[86]** | Q5 `[33]`, Q4 `[26]` |

### 5.2 Benchmarks de agente citados

| benchmark | resultado | fonte global | via |
|---|---|---|---|
| **SlopCodeBench** | **nenhum** agente resolveu **20 tarefas iterativas** consecutivas (acumulo de erro e degradacao de contexto, o *slop debt*) | **g[23]**, **g[24]** | Q4 `[22]`,`[23]` |
| **TerminalBench 4.0** | Claude Haiku 5.5 = **39,2%** em codificacao agentica vs **0%** do Haiku 4.5 | **g[25]** | Q4 `[24]` |
| **GAIA** | raciocinio silencioso hibrido = **52,4** vs **37,9** (GPT-6 Luna) vs **6,8** (Coconut) | **g[86]** | Q4 `[25]` |
| **LoCoMo** | **+35% SOTA**, **+76% multi-hop** | **g[86]** | Q4 `[26]` |
| **reverts na Anthropic** apos revisao continua em 6 camadas | **-40%** de codigo revertido | **g[7]** | Q4 `[28]` |

### 5.3 Parametros de compactacao concretos (Q2)

| parametro | valor | fonte global | via |
|---|---|---|---|
| `CLAUDE_AUTOCOMPACT_PCT_OVERRIDE` - padrao do Claude Code | **95** (dispara no 95% da janela) | **g[50]**, **g[83]** | Q2 `[50]`,`[51]` |
| valor recomendado pelo corpus | **50** (compacta mais cedo, preserva qualidade de raciocinio) | **g[50]**, **g[83]** | Q2 `[50]`,`[52]` |
| janela de **1 milhao** de tokens (Sonnet 5/5.5, Opus 5.5, Fable) | gatilho em **25%** (~250 mil) | **g[83]** | Q2 `[51]`,`[53]` |
| janela de **200 mil** | gatilho entre **60% e 70%** (25% cortaria aos 50 mil, cedo demais) | **g[83]** | Q2 `[51]` |
| `keep threshold` do Jev - padrao **0,5** apaga entre **88,8% e 96,4%** do texto e elimina as chamadas de ferramenta que provam a execucao | recomendado **0,2** (faixa util 0,10-0,25) | **g[83]** | Q2 `[53]`,`[54]` |
| truncamento de saida de ferramenta do Jev | mantem os primeiros **300 caracteres** (600 em configuracao ajustada) | **g[83]** | Q2 `[54]`,`[64]` |

### 5.4 Regra de dimensionamento de tarefa (Matt Pocock)

> "Estime os tokens que o job exige e **divida por 150.000**; um refactor de 1 milhao de
> tokens deve ser quebrado em **7 tickets/sessoes**" - Q5 `[24]` -> **g[68]**

## 6. Implicacoes para o Touring

### 6.1 O notebook CONFIRMA a politica que o codigo ja declara - e da o porquê faltante

O codigo do Touring ja declara, em `crates/touring-hook-handlers/src/hooks/pre_read.rs:3`,
*"SILENCE IS THE DEFAULT. Only inject context when it passes the 'So what?' test"*, com as
tres perguntas (acionavel / nao-derivavel / especifico). O notebook da **base externa**
para essa politica:

- "a melhor janela e a menor janela que entrega o resultado de maior qualidade pelo menor
  custo" - **g[7]**;
- regra **com motivo** - **g[19]**;
- e o **custo de nao calar**: 18/18 modelos degradam, 13,9%-85% de queda, 46 ferramentas
  derrubam a tarefa - **g[33]**.

**O que falta no codigo hoje** e a **medida**: o notebook da um par contra ("com vs. sem")
que o Touring ainda nao tem para injecao. Ver 6.3.

### 6.2 O SERMAO de 18,7% e o ALERTA de 20,1% tem agora um par experimental publicado

O achado de **g[50]** (2 de 9 skills venceram um arquivo vazio, 450 execucoes) e o
**mesmo fenomeno** que o Touring mediu: 7/9 skills de terceiros sem ganho mensuravel. Duas
medidas independentes, mesmo padrao. **A政策 do Touring esta correta e o custo esta
quantificado por fora** - isso strengthens o caso de deletar/reduzir as familias sem
adesao (SERMÃO, ALERTA sem alvo) em vez de so documentar.

**A analogia estrutural mais forte**: o notebook diz que **hook deterministico tem 100% de
conformidade**, e que **texto no prompt nao garante** (falha em estresse, conversa longa,
caso de borda). O Touring mediu exatamente isso: `pre-edit` (payload de hook, portao
deterministico) = **75,6% CONFORME** / **98,8%** na familia `pre-edit-prevention`, contra
`code-mode-contract` (texto de contrato) = **0,0%** e `touring-suggest` = **0,0%**.
**A politica do Touring ja segue a regra do notebook. O proximo passo e reduzir o
volume, nao a severidade.**

### 6.3 O gap de metodo: o Touring mede adesao, o notebook mede *ganho causal*

O instrumento do Touring classifica a adesao (CONFORME / CONTRARIO / SEM EFEITO /
INDET). O notebook mede outra coisa: **o texto injetado faz o resultado MELHOR do que o
mesmo espaco em branco?** O Caliper e o `claude plugin eval` respondem isso com pares
pareados. **O `generic-noise` (1.499 blocos, 100% SEM EFEITO) e o `touring-suggest` (4.647
blocos, 100% SEM EFEITO) sao candidatos naturais a um par "com vs. sem"** - a hipotese a
testar nao e "o agente obedeceu?", e "o bloco injetado mudou o resultado?".

### 6.4 O padrao de injecao efemera na cauda (tail-only) e aplicavel hoje

O mecanismo de **g[15]**: o bloco injetado entra como **sufixo do turno corrente** e e
**removido antes de o turno integrar o historico**, preservando 100% do cache de prompt.
Hoje o Touring injeta via `hookSpecificOutput.additionalContext` em `PreToolUse` (frio) e
`PostToolUse`. **O `PostToolUse` e o ponto natural**: um payload de hook em `PostToolUse`
existe para orientar o proximo passo do **mesmo** turno - nao ha razao para ele
permanecer no historico. **Ganho esperado**: payload do `PostToolUse` = **358.957 bytes
em 1.329 blocos, 100% SEM EFEITO** e **75,1% do "tocou"**; se o ruido historico for o
que impede o uso, tirar do historico e remover o custo sem remover o sinal.

### 6.5 Teto de caracteres ja existe no corpus (8.000) e o Touring nao tem

O ECC define `ECC_SESSION_START_MAX_CHARS` = **8.000 caracteres** para o `SessionStart`
(g[38], via Q1 `[16]`). O `SessionStart` do Touring injetou **673.533 bytes** (625
blocos) em `startup` + **577.567 bytes** (185 blocos) em `compact`, com **0,0% de
adesao** e **17,1% de "tocou"**. AFontes do corpus sugerem **teto**; o Touring nao tem
nenhum nesse caminho.

### 6.6 "Falar nao garante fazer" - o principio que autoriza remover texto

Se o notebook diz que hook deterministico = 100% e texto = nao-garantido, entao **cada
bloco de texto que o Touring injeta e um bloco que deveria ser hook**. As familias com
0% de adesao e 100% de "sem efeito" (`generic-noise`, `touring-suggest`,
`code-mode-contract`, `evidence-guard`, `session-contract`, `post-tool-echo`) sao
candidatas a **remocao ou conversao em portao**, nao a reforco de texto.

### 6.7 O principio de "1 nivel" e "indice no topo" (g[10]) como regra de injecao

Se o modelo so le as **primeiras 100 linhas** de um arquivo aninhado e so ve o topo
quando pre-visualiza, entao **injetar mais de ~100 linhas de material aninhado e
desperdicio garantido** - o conteudo injetado tem de ser autossuficiente no topo ou
nao e lido. Isso da um criterio **quantitativo** para dimensionar payload de injecao, e
nao so "so what?".

### 6.8 Nao ha, no corpus, um numero de "quanto contexto um agente deve receber"

**Resposta honesta**: nao. O corpus da **faixas e relacoes**, nao um orcamento:
- reduza ate o limite onde a qualidade cai (85% de reducao de tokens com lazy loading,
  sem queda; 80% de system prompt deletado, sem queda nos evals);
- **nao** reduza abaixo do ponto em que a informacao necessaria deixa de estar la
  (13,9%-85% de queda com o input crescendo - o que e o **custo de ter o contexto
  completo**, nao de injecao);
- **nao** mantenha mais de ~19-46 ferramentasDEFINIDAS simultaneamente;
- `CLAUDE.md` < 200 linhas, `SKILL.md` < 500 linhas, bloco de injecao < ~100 linhas.

O que falta e um **teto absoluto de bytes por evento de injecao** - que e exatamente a
lacuna que o corpus nao fecha.

## 7. Lacunas honestas

1. **Nenhuma das 91 fontes e documentacao oficial da Anthropic** (declarado pelo proprio
   notebook, Q6). Todo o material e terceiro: video do YouTube que transcreve a Anthropic,
   README de GitHub, pagina do crates.io. **Nenhum numero foi reproduzido por mim** - sao
   numeros de terceiro, medidos por terceiros, em condicoes que nao conheco.
2. **Os numeros sao reportados, nao verificados.** "450 runs", "36% -> 100%", "6,7% ->
   70%", "+11,5 pts" sao afirmacoes de第三个. O unico que **confirmei por texto** foi o
   par "1.500 -> 600 tokens" (**g[55]**), "1.400 tokens" (**g[47]**), "49% -> 74% /
   79,5% -> 88,1%" e "85% reduction" (**g[7]**), "46 vs 19 ferramentas" (**g[33]**),
   ">80% do system prompt" (**g[7]**).
3. **O "30.000 tokens" nao tem excerto literal** que o sustente - o excerto recuperado
   traz a formulacao qualitativa. Marquei como sintese do notebook.
4. **A numeracao `[n]` das respostas nao e o indice global** (provado em secao 0). Qualquer
   artefato derivado que use `[n]` cru como identificador de fonte esta errado. O
   `references[].source_id` e a unica chave confiavel.
5. **O notebook nao mede injecao de hook diretamente.** As metricas de harness (Harness2,
   LangChain, Grok, Runlayer) medem **harness inteiro** (prompt + ferramentas + memoria +
   subagentes), nao "um bloco injetado". A extrapolar "harness inteiro" -> "bloco de
   injecao" e **inferencia minha**, nao dado da fonte.
6. **Sem data de publicacao por fonte.** As datas do corpus sao **datas de evento
   citadas** (17/09, 25/09, 01/10...), nao metadados de publicacao das fontes. A
   unica data metadata e `created_at` do notebook (25/09/2026).
7. **Sobreposicao com outras fontes da rodada.** O arquivo
   `round4/fonte-notebooklm-claude-code.md` (scout de outra frente) ja extrai deste
   mesmo notebook a secao "o que deletar" e "as 12 regras". Este artefato **nao**
   reexplica a lista de delecao - cobre injecao de contexto, janela, silencio e
   benchmarks de harness, que e o escopo pedido.
8. **Nenhuma acao foi executada no codebase.** Este e um artefato de pesquisa. As
   implicacoes da secao 6 sao hipoteses para a fase de arquitetura/decisao, nao mudancas
   feitas.

## Apêndice A - log de consultas (6 queries, id literal)

Todas contra `notebook_id=65c3ecb1-a4ce-4901-9864-a2b28c04b469`, com
`nlm notebook query <id> "<pergunta>" --json -t 240`. O `id` literal foi usado em todas -
**nenhum** `batch` por titulo (que resolve para o id errado).

| # | foco | `sources_used` | `references` (citacoes resolviveis) | teto global |
|---|---|---:|---:|---:|
| Q1 | hooks, `additionalContext`, qualidade e acionabilidade | 19 | 49 | g[87] |
| Q2 | janela de contexto, compactacao, orcamento de tokens, sub-agentes | 18 | 64 | g[87] |
| Q3 | quando calar, ruido, redundancia, modos de falha | 12 | 32 | g[55] |
| Q4 | adesao a instrucao, texto vs codigo, como medir | 16 | 29 | g[86] |
| Q5 | benchmarks de harness e metricas de contexto | 17 | 34 | **g[91]** |
| Q6 | proveniencia: titulos, datas, 1as 10 fontes | 28 | 33 | **g[91]** |

**Testes de controle executados** (para provar a semantica da citacao):

| teste | comando | resultado |
|---|---|---|
| fonte unica | `--source-ids 9585f4d3-...` (g[10]) | cita a regra dos 500 linhas como `[1]`, nao `[10]` |
| duas fontes | `--source-ids 689ba9fd-...,9585f4d3-...` (g[1], g[10]) | nomeia "Anthropic Just Revealed 10 NEW Rules..." e marca `[1]` |
| id errado | `--source-ids adc8e5e5-...` (fonte do notebook de 42) | resolve e responde ("The selected source ... does not mention SKILL.md") |

## Apêndice B - simbolos do Touring citados (verificados por CLI)

| simbolo | evidencia | veredito |
|---|---|---|
| `compose_high_signal_context` | `touring index find compose_high_signal_context -j` -> definicao `crates/touring-hook-handlers/src/hooks/pre_read.rs:1069`; 20 referencias (incl. `pre_read_tests.rs:1236` e `crates/touring-hooks/tests/integration_e2e.rs:80,95,1088,1220`) | `verified_existing` |
| `pre_read` (modulo) | `touring index find pre_read -j` -> `crates/touring-hook-handlers/src/lib.rs:85` | `verified_existing` |
| `hook_rewrite` (modulo) | `touring index find hook_rewrite -j` -> `crates/touring-cli/src/lib.rs:52` | `verified_existing` |
| `hookSpecificOutput.additionalContext` | `touring index find additionalContext -j` -> **count 0** (nao e simbolo Rust; e chave JSON). Verificado por grep: `crates/touring-cli/src/hook_rewrite.rs:156` (`obj.insert("additionalContext".into(), json!(context));`), `crates/touring-cli/src/cli_suggester.rs:4180`, `:5378`, `:5412`, `:6229` | `verified_existing` (chave JSON, via grep) |
| invariante "SILENCE IS THE DEFAULT" | `crates/touring-hook-handlers/src/hooks/pre_read.rs:3` (leitura direta) | `verified_existing` |
| teste da invariante | `crates/touring-hook-handlers/src/hooks/pre_read_tests.rs:1220` `audit_purpose_silence_is_default_invariant` - corpo lido: monta `FileKnowledge` so com metadados e exige `ctx.is_none()` | `verified_existing` |

Tamanhos conferidos (`wc -l`): `pre_read.rs` 1979 · `pre_read_tests.rs` 1867 ·
`hook_rewrite.rs` 273 - todas as linhas citadas existem.
