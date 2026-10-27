---
type: Research
title: "Fonte externa — NotebookLM 'Claude Code ' (91 fontes, 25/09/2026): o que deletar, as 12 regras, e a janela como bem público"
description: "Extrato direto do notebook certificado, com citações e números de fonte."
plan_id: 2026-10-26-injecao-contexto-hooks
okf_version: 1
timestamp: 2026-10-26
---

# NotebookLM "Claude Code " (91 fontes) — o que a Anthropic manda deletar, e as 12 regras

> **Procedência**: notebook `65c3ecb1-a4ce-4901-9864-a2b28c04b469`, título com
> **espaço final**, 91 fontes, `created_at` 2026-09-25T22:44:15Z. Certificado em
> `CERTIFICACAO-FONTES.md`. Extrato de consulta direta (não do scout — o scout foi
> corrigido para este id e reexecuta em paralelo).
> **Ressalva de proveniência**: as fontes são **material de terceiro (YouTube,
> Matt Pocock, Flávio Augusto, etc.) que *transcreve* documentos oficiais da
> Anthropic**. Onde há citação literal da fonte, marco `[n]`; onde é paráfrase do
> autor do vídeo, digo isso.

## 1. "Anthropic Just Donated Claude Skills. Here's **What to Delete**" [1-6]

As seis categorias que a fonte manda **deletar**:

| # | o que deletar | por quê (fonte) |
|---|---|---|
| 1 | **Skills "muleta"** — instruções ultra-prescritivas escritas para modelos antigos (Opus 4.1), que não sabiam derivar subtarefas | "a lot of these skills acted as **crutches** for models that were decent at input and output but not as decent without actually taking that input and extrapolating the next subtasks" [1, 2] |
| 2 | **Skills acumulados e não usados** — coleções de 50+, 100+, 121+ que nunca são usados | "you might be **feeding your model tons of instructions it actually doesn't need**" [2, 3] |
| 3 | **Skills com regras duplicadas/sobrepostas** (ex.: 6 skills de design dizendo quase o mesmo) | "with 100 skills that's **100 chances to pick the wrong one**" [4] |
| 4 | **Skills substituíveis** por conector/MCP nativo ou por **uma linha no `AGENTS.md`** | "even that skill could become one singular line in an agent MD" [5] |
| 5 | **Skills always-on** — mover para fora do carregamento inicial e ler *just-in-time* | "use **just in time** skills versus skills that load every session" [6] |
| 6 | **Conteúdo que sobrepõe a memória / `AGENTS.md`** | "make sure that there's no overlap between the skills themselves and something like your memory" [5] |

### 1.1 O achado mais forte da fonte

> "the first time we saw this core difference was with Claude Opus 5 now compared to
> Opus 5.5 it was an absolutely horrific model but that's not the point — the core fix
> that people discovered is that **if you remove the majority of your skills and
> instructions it worked a lot better**" [4]

E a formulação do problema:

> "I had more than 121 skills constantly **bloating my context window** with their
> name and description — which isn't a lot individually but **at scale it starts to
> really poison your context**" [3]

**Leitura para o Touring**: o dano do acúmulo é **superadditivo** — "não é muito
individualmente, mas em escala envenena". O Touring injeta **28.295 blocos em 400
sessões** com **16 famílias**, várias delas telling the same thing in different
words (blast radius / pre-edit / pre-edit-prevention / post-edit falam do mesmo
arquivo).

### 1.2 As três categorias que **valem** manter

A fonte é explícita: das skills, só três merecem ficar — (a) as **personalizadas**
(o que o modelo não tem no treino nem acha online), (b) as que **não têm MCP
nativo** (navegar uma API com centenas de portas), (c) as **de uso real**. [2, 5]

## 2. "Anthropic Just Revealed 12 New Rules for Prompting Opus 5.5" [7-13]

| # | regra | citação |
|---|---|---|
| 1 | effort **medium** como default | "the recommended default is now medium" [7] |
| 2 | **testar** os níveis de effort na sua própria trabalho | [8] |
| 3 | usar `AGENTS.md` | [8] |
| 4 | trocar effort **sem limpar o cache** | [9] |
| 5 | reservar os resets de cota | [9] |
| 6 | **não pedir exibição do raciocínio** | "this might actually get flagged now with Opus 5.5 … reasoning extraction" [10] |
| 7 | **tratar respostas anteriores como consolidadas** | "tell it that earlier answers are settled in long-running chats — Opus 5.5 sometimes goes back over its earlier answers" [10] |
| 8 | **checklist** para evitar parada precoce | "5.5 has sometimes been observed to stop working even when the whole task technically isn't 100% complete" [11] |
| 9 | **orçamento de tempo** explícito | "finish the task in 3 minutes and it would pace itself" [12] |
| 10 | a expressão **"time matters"** | "in their testing that alone made agent teams finish sooner" [12] |
| 11 | **direção de design explícita**, não "não genérico" | "prompting Opus to make it look less generic just **swaps one default for another**" [12, 13] |
| 12 | **ferramentas de corte e zoom** (PIL, OpenCV) | "keeps token usage lower by feeding it smaller images" [13] |

### 2.1 As três regras que tocam diretamente a injeção de contexto

**Regra 7 (contexto redundante)** — a fonte atribui a Anthropic: Opus 5.5 *"às vezes
volta sobre suas respostas anteriores enquanto pensa"*, e o remédio é **duas
frases no `CLAUDE.md`**. [10]

**Regra 11 (instrução genérica é inócua)** — *"design não genérico" não funciona;
troca um default por outro*. [12, 13] **É a mesma forma do "sermão" do Touring**:
um bloco que nomeia um objetivo sem nomear a ação. E é exatamente o que a
medição do Touring encontrou: **18,7% do contexto, ≈0,3% de adesão**.

**Regra 12 (o objeto certo, não o resumo do objeto)** — dar ao modelo a
**ferramenta** de olhar o pedaço, em vez do pedaço. [13] Paralelo direto ao
`fast-jev-compaction` e ao `context.py:25-27` do harness2.

## 3. "10 NEW Rules for Claude Skills" — a regra do "bem público"

> "**Não gastar espaço explicando conceitos básicos que o modelo já conhece** — a
> janela de contexto é um **'bem público'**." [7]
> "Testar skills em modelos específicos: no **Haiku** (precisa de mais guia?), no
> **Sonnet** (está claro e eficiente?) e no **Opus/Fable** (**evitar
> superespecificações** para não limitar a criatividade)." [5, 6]
> **Divulgação progressiva**: `SKILL.md` < 500 linhas como índice; detalhe em
> arquivos separados, com **no máximo 1 nível** de referência — "modelos novos
> tendem a pré-visualizar apenas as primeiras 100 linhas". [1, 2]
> Qualquer arquivo com > 100 linhas deve ter **sumário no topo**. [3]
> **Calibrar graus de liberdade**: alta (criativo), média (templates), baixa
> (scripts rígidos para tarefa de alto risco). [4, 5]

### 3.1 O cruzamento com o `pre_read.rs:1-7`

O `pre_read.rs` do Touring já declara, na íntegra, duas destas regras:

- *"Is it non-derivable? (Claude WON'T find this by reading the file)"* ⇒
  **"não explicar o que o modelo já sabe"** [7].
- *"Is it actionable?"* ⇒ a **direção explícita** da Regra 11 [12].

⇒ **O critério do Touring não é ORIGINAL nem frouxo: ele é a versão local da
regra oficial.** O problema nunca foi o critério — foi a **aplicação**: 34% do
volume o viola.

## 4. `fast-jev-compaction` — a alternativa arquitetural [3, 4]

Fonte: plugin de compactação (criado em 17/09/2026 por Tamara Tran, 7.335 stars
em 02/10/2026), citado no próprio notebook.

> "**Nunca resume.** Todo texto teu e do Claude fica, palavra por palavra e na
> ordem, e **só chamada e saída de ferramenta podem sair**. A primeira mensagem e
> as 6 mais recentes nunca saem tocadas." [4]

Medições citadas na fonte:
- sessão de 173.575 tokens: mediana de **743 ms** no plugin contra **30.375 ms**
  no resumo nativo (issue 89, 6 rodadas);
- num resumo nativo de ~950 mil tokens, **75 a 170 segundos** (PR 122).

> "O que fica, fica palavra por palavra, e o que sai é **leitura de arquivo,
> busca e log que já cumpriram o papel**." [3]

⇒ **É uma estratégia de EVICÇÃO, não de summarizing** — e o alvo da evicção
("já cumpriram o papel") é exatamente a categoria do Touring que a medição
apontou como ruído: o bloco de `pre-edit` que já foi usado, o `post-tool-batch`
de uma edição que acabou de acontecer.

## 5. `pond-db` — disciplina de staleness [2]

A crate `pond-db` mantém uma tabela de adaptadores por agente com
**"Last verified"**: `claude-code` 2026-08-14 · `codex-cli` 2026-09-01 ·
`opencode` 2026-10-12 · `devin` 2026-09-26 · `agy` 2026-09-10.

⇒ O acervo do notebook é **datado por fonte e verificado por data** — o mesmo
princípio que o `touring index why` e o `pre-edit` (que injeta "generic@10-26T15:41Z")
praticam, e que o `similar-fail` do Touring **não** pratica (avisa sem dizer o quê).

---

_Extrato direto por consulta ao notebook certificado. As citações `[n]` são
numeração do NotebookLM dentro da resposta da consulta; ver
`CERTIFICACAO-FONTES.md` para a identidade do notebook._
