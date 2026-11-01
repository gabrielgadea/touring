---
type: Analysis
title: "Impacto qualitativo — rodada 4 (com as fontes externas): o que a injeção FAZ com o modelo"
description: "A rodada 4 da análise, com as melhores práticas do Context7, dos notebooks NotebookLM e do harness2, sobre o corpus real de 400 sessões."
plan_id: 2026-10-26-injecao-contexto-hooks
okf_version: 1
timestamp: 2026-10-26
---

# Impacto qualitativo — o que a injeção **faz** com o modelo (rodada 4)

> **Rodada 4** — qualitativa, com fontes externas. Complementa `ANALISE-CONSOLIDADA.md`
> (volume), `IMPACTO-QUALITATIVO.md` (os cinco modos de influência) e
> `IMPACTO-POR-HOOK.md` (conformidade por família).
>
> Corpus: **400 sessões** (`-touring`, `-analise`, `-konverter`), **28.295 blocos** de
> contexto do Touring. Fontes externas: Context7 (65 docs first-party da Anthropic),
> notebook NotebookLM **"Claude Code "** (91 fontes, criado em 2026-09-25 — o do
> Gabriel, certificado em `CERTIFICACAO-FONTES.md`), notebook **"GitHub Repos"**
> (107 fontes) e `google-research/harness2` + o paper **arXiv 2610.14224**.

---

## 0. Procedência — os três corpora, e por que não são um só

| corpus | o que é | o que responde |
|---|---|---|
| **interno** (400 sessões do corpus local) | o que o modelo **realmente** recebeu e o que fez | **o que a injeção faz** (medido) |
| **doc oficial** (Context7, 65 docs first-party) | o que a Anthropic diz que o hook **deve** fazer | **o critério declarado** (e a ausência dele) |
| **harness2 + notebooks** | o que um harness de pesquisa e a comunidade medem | **o efeito causal** (ablation) e a **melhor prática** |

O cruzamento dos três é o que permite dizer: não só **que** o modelo faz, mas
**por que** — e **quanto custa**.

---

## 1. A pergunta, e por que "adesão" não responde

A pergunta de Gabriel era: **cada conteúdo injetado por cada hook gera o impacto
esperado no modelo, e de que forma?**

A rodada 3/4 mediu isso por **conformidade** (o efeito declarado aconteceu). Mas
conformidade **não é causa** — o modelo pode fazer isso de qualquer jeito. O
harness2 mede por **ablação** (remover a injeção e medir a diferença). Os dois são
necessários: conformidade sem ablação é correlação; ablação sem conformidade não diz
**como** o efeito se deu.

---

## 2. O veredito: o que a injeção faz, em quatro categorias

| classe | n | % do contexto | conformidade | o que faz |
|---|---:|---:|---:|---|
| **FATO** (o modelo não tinha) | 10.051 | 35,5% | **≈ 52%** | o modelo age em cima do fato, sem citá-lo |
| **SERMÃO** | 5.296 | 18,7% | **≈ 0,3%** | o modelo **cita** e **não age** — o sermão é lido e ignorado |
| **DIRETIVA** (programa pronto) | 1.840 | 6,5% | **0,0%** | **a ordem chega inválida** — 58,0% das ordens MUST chegam quebradas |
| **ALERTA** | 5.686 | 20,1% | **0,0%** (exceto `compact-warning`: 46,3%) | o modelo **não muda nada** — o alerta não nomeia o alvo |
| **COERÇÃO** (reescreve) | 970 | 3,4% | **1,000 por construção** | o hook **reescreve a chamada** — o modelo não tem escolha |
| **RUÍDO** (`[generic]`) | 1.499 | 5,3% | — | **ruído puro** — o payload é a palavra `[generic]` |

**A variável que decide não é volume nem proeminência: é o conteúdo carregar um fato
que o modelo não tinha — ou ser uma decisão.** O sermão e o alerta sem alvo não têm
nenhum dos dois.

---

## 3. Os três mecanismos que explicam o veredito

### 3.1 ⚡ A ordem chega tarde demais

**Medido**: em **77.794 de 77.794** casos (100,0%), o `additionalContext` do
`PreToolUse` é gravado **depois** do `tool_use` que o modelo já emitiu — zero casos
antes.

⇒ **A injeção de contexto não influencia a decisão que ela pretendia orientar —
influencia a próxima.** O `touring-suggest` tem **a melhor prosa do corpus** (79,2%
dos bytes são ordem completa, um comando por linha) e **a pior adesão (0,3%)** — e
a explicação não é "o modelo ignora", é "o modelo já decidiu".

**A consequência de projeto**: um hook que **informa** em `PreToolUse` chega depois
do ato; para **influenciar** o ato, ele precisa **reescrever** (o caminho do gate,
que é o único que age no momento da decisão) ou ir para um evento que precede a
decisão.

### 3.2 O canal é "contexto da aplicação", não ordem do operador

A doc oficial (via Context7, `agent-sdk/modifying-system-prompts:1224`):

> "Treat them as context **from the application**, not as messages from the user."

O conteúdo injetado por hook é, para o modelo, **contexto da aplicação** — não
ordem do operador. Um **fato** nesse canal é lido como dado disponível; um
**sermão** é lido como conselho que o modelo não tem obrigação de obedecer. E a
forma do payload importa: a doc oficial diz que **prose always-on é a opção mais
cara** (`harness2:prompts.py:129-133`).

### 3.3 69,8% dos bytes injetados são repetição — e a repetição dilui o resto

| família | blocos | **payloads distintos** | taxa de repetição |
|---|---:|---:|---:|
| `debug-stdout` | 2.869 | **46** | **99,6%** |
| `compact-warning` | 284 | **11** | **99,5%** |
| `evidence-guard` | 2.356 | 134 | **96,2%** |
| `session-contract` | 72 | **4** | **95,8%** |
| `plugin-hook` | 780 | 473 | 93,4% |
| `similar-fail` | 1.678 | 208 | 92,3% |

**Total: 13.193.705 de 18.910.157 bytes = 69,8% são repetição.**

O harness2 nomeia o custo: *"duplicated instruction dilutes the rest"* — e o corpus
do Touring mede a magnitude. O bloco repetido mais vezes é o **melhor** bloco do
sistema (`[G2 … → REESCRITO]`, 399×) — repetição e qualidade são **eixos
independentes**: um bloco excelente repetido 399 vezes continua sendo 399 vezes de
bytes que não mudam nada.

---

## 4. A regra operacional — o que o corpus sugere

> **Injete um bloco só quando ele trouxer, em uma única frase: (1) o estado
> medido, com o número; (2) a ação, com o verbo no imperativo e o alvo nomeado;
> (3) o porquê — o que quebra se o modelo não agir.**
>
> **E cale quando:** o fato já está na tela (o modelo acabou de vê-lo); o fato é o
> conteúdo do arquivo; o fato não tem consequência; ou o bloco não cabe em uma
> frase.

**Os três testes executáveis** (na ordem):

| # | teste | pergunta |
|---|---|---|
| **T1** | verbo no imperativo com alvo nomeado? | o bloco diz o que fazer? |
| **T2** | o fato é novo? | o modelo acabou de ver? |
| **T3** | o bloco cabe em uma frase? | ou é uma linha de pipes? |

O `pre_read.rs:1-7` já diz isso em **três perguntas** — mas três perguntas não são
uma regra executável: não dizem **quanto** e **onde**. O corpus diz.

---

## 5. A matriz — o que cada família deve fazer

| família | veredito | a regra externa aplicável |
|---|---|---|
| `pre-edit` | **FATO — manter** | harness2 `prompts.py:860-862` |
| `touring-suggest` | **SERMÃO — cortar** | notebook "Claude Code " [7] (a janela é bem público) |
| `post-edit` | **ALERTA — cortar até ter gate** | harness2 `prompts.py:868` |
| `outros` | **NÃO É FAMÍLIA — desdobrar** | — (catch-all) |
| `evidence-guard` | **REGRA REPETIDA — cortar** | harness2 `prompts.py:868` |
| `code-mode-contract` | **DIRETIVA sem execução — cortar** | harness2 `prompts.py:456-458` |
| `similar-fail` | **INACIONÁVEL POR CONSTRUÇÃO — S-2** | harness2 `prompts.py:860-862` |
| `generic-noise` | **RUÍDO — cortar** | harness2 `prompts.py:868` |
| `post-tool-echo` | **ECHO — cortar** | harness2 `context.py:25-27` |
| `code-mode-deny` | **ENFORCEMENT — manter** | harness2 `prompts.py:212-215` |
| `plugin-hook` | **DASHBOARD — cortar** | notebook "Claude Code " [1-6] |
| `pipefail-evidence` | **DISPUTADO — FATO sem momento** | harness2 `prompts.py:864` |
| `compact-warning` | **FATO — manter** | harness2 `prompts.py:860` |
| `pre-edit-prevention` | **FATO — manter (momento tardio)** | harness2 `prompts.py:864` |
| `session-contract` | **DIRETIVA — cortar** | harness2 `prompts.py:456-458` |
| `pre-read` | **QUASE SILENCIOSO — manter** | `pre_read.rs:1-7` |

---

## 6. O que a análise consolidada **acerta**, e o que a rodada 4 corrige

### Acerta (e a rodada 4 confirma)

| # | afirmação | o que a rodada 4 acrescenta |
|---|---|---|
| A1 | A separação **fato × sermão** é real | o **mecanismo** (o canal é "contexto da aplicação") e a **forma vencedora** do payload |
| A2 | O `similar-fail` é defeito de interface | **confirmado e completado** — o `reason` está gravado; falta o comando |
| A3 | O gate funciona **por coerção** | o corpus mostra **por quê**: ele **reescreveu e explicou** |
| A4 | A família `outros` precisa ser fechada | **confirmado e agravado** — contém o melhor e o pior bloco |
| A5 | O interruptor precisa ir para Rust nativo | **confirmado** — `touring-foundation` é a raiz do DAG |
| A6 | O `no-advisory` corta 19,7% | mantém-se, com a ressalva do canal |

### Corrige (as nove, com a fonte)

| # | onde | antes | agora |
|---|---|---|---|
| **C1** | a premissa da matriz | "o 'So what?' é a regra oficial da Anthropic" | **não é publicado como critério para o canal de hook** (65 docs, negativo); **mas** coincide com o conselho do time de AI aplicada |
| **C2** | §8.8 do consolidado | "`s8_corpus_ab.rs` mede resultado" | **impossível** — mede **bytes**, não desfecho |
| **C3** | §8.3.2 | "os dashboards vão por `systemMessage`" | **contra** — o corpus mede **zero** `systemMessage`; os dashboards vão por `additionalContext` |
| **C4** | §4.6 | `outros` = residual | **`outros` é o resto, e contém os dois extremos** |
| **C5** | §4 | o contrato aparece em 4 lugares | **a letra existe em 1 lugar** (`pre_read.rs:3-7`); os outros são paráfrase |
| **C6** | §5.1 | `FailureMatch` é correção de superfície | **metade não é de superfície** — `FailedPath` não tem campo de comando |
| **C7** | §7 | "a régua mede o que emite" | **duas taxonomias em duas linguagens divergem em 9 famílias** |
| **C8** | §2 | "injeção = 27,8% da janela" | **12,0% medido hoje** — divergência metodológica **a reconciliar** |
| **C9** | a premissa do A/B | "a política é a lacuna" | **o dedup já existe e é maior** — o que falta é **alcance** |

---

## 7. O contra-diagnóstico — o que a evidência externa **não** sustenta

1. **A doc oficial não publica critério editorial** — 65 docs first-party varridos,
   resultado **negativo**. O "So what?" do Touring é **invenção local**, não doutrina.
2. **O harness2 abla o prompt do *solver*, não hook de agente** — a analogia é
   estrutural, não idêntica.
3. **Nenhum corpus externo mede adesão a nudge** — o problema do Touring é
   **medido só aqui**.
4. **Conformidade ≠ atribuição** — o modelo pode fazer isso de qualquer jeito; o A/B
   de desfecho **não existe** ainda.
5. **`s8_corpus_ab.rs` mede bytes, não desfecho** — não invoca modelo nenhum.

---

## 8. O que a rodada 4 entregou

| # | o que | onde |
|---|---|---|
| 1 | a **certificação** das fontes (há DOIS "Claude Code"; o do Gabriel é o de 91 fontes) | `CERTIFICACAO-FONTES.md` |
| 2 | a **matriz** (16 famílias, regra externa × medição interna) | `MATRIZ-DIAGNOSTICO.md` |
| 3 | o **extrato** do notebook certificado (as 6 categorias de "What to Delete", as 12 regras) | `fonte-notebooklm-claude-code.md` |
| 4 | a **ablação** do harness2 (Table 6 do paper) | `scout-harness2.md` |
| 5 | o **scout do Context7** (a doc oficial não tem critério editorial) | `scout-context7.md` |
| 6 | o **scout do notebook "GitHub Repos"** (hook ~98% vs instrução ~60%) | `scout-notebooklm-github-repos.md` |
| 7 | a **leitura qualitativa** dos payloads verbatim (a regra T1/T2/T3) | `scout-corpus-qualitativo.md` |
| 8 | as **evidências** (a ordem de leitura, as ordens inválidas, a janela real, a redundância) | `evidence-*.md` |

---

## 9. Os limites honestos desta rodada

1. **Corpus = 3 projetos, 400 sessões** — não é o universo de uso do Touring.
2. **Conformidade ≠ atribuição** — o A/B de desfecho é o passo que falta.
3. **O `outros` (10,4%) continua sem taxonomia** — restam famílias de cabeçalho raro.
4. **A taxa de adesão do sermão (0,3%) é um piso** — medida no corpus local.
5. **O harness2 mede o *solver*, não o *hook*** — a analogia é estrutural.

---

## 10. Reprodução

```bash
# a medição
python3 scripts/measure_injection_effect.py --max-sessions 400 --window 3
python3 /tmp/measure_redundancy.py
python3 /tmp/measure_window_share2.py
python3 /tmp/verify_order_and_10k.py
python3 /tmp/check_broken_directives.py

# a política
python3 scripts/test_injection_policy.py   # 89 PASS / 0 FAIL
python3 scripts/guard_injection_policy.py  # 152/152, 0 lacunas
```

---

_Análise por TACO sob ordem de Gabriel Gadea, 26/10/2026 — rodada 4.
Documento complementar a `ANALISE-CONSOLIDADA.md` e `IMPACTO-POR-HOOK.md`._
