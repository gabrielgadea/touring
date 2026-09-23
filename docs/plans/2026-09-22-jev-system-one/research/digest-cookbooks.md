---
type: ResearchDigest
title: Jev — digest dos 18 cookbooks oficiais
description: Desenho de perguntas, uso de confidence, thresholds e números exatos de cada cookbook da TypeSafe, com síntese transversal.
plan_id: 2026-09-22-jev-system-one
tags: [research, jev, cookbooks]
timestamp: 2026-09-22T08:55:00-03:00
resource: https://docs.typesafe.ai/cookbooks
okf_version: "0.1"
---

Parte do [bundle](/index.md); consumido pela [estratégia](/strategy-2026-09-22-jev-system-one.md).

# Digest — Cookbooks oficiais do Jev / System One (TypeSafe AI)

> Fonte primária: `scratchpad/jev/pages/cookbooks__*.md` (docs.typesafe.ai/cookbooks/*), lidos na íntegra em 22/09/2026.
> Fidelidade: todo número abaixo está no texto-fonte; o que não está aparece como **"não informado"**. Trechos entre aspas são literais.
>
> **Nota de contagem**: o índice `llms.txt` da documentação lista **18 cookbooks** (+ a página-índice `cookbooks.md`, que não foi baixada e não é um cookbook). Não há 19º cookbook no material baixado nem no índice. Os 18 estão todos cobertos abaixo.
>
> **Convenções da API vistas em todos**: `client.system_one(state=..., questions={id: Choice|Score|Noul}, model=...)` → `response.answers[id]` com `.choice/.probabilities/.confidence` (Choice), `.score/.probabilities/.confidence` (Score), `.noul` = P(true) (Noul); `response.usage.input_tokens/output_tokens`; `response.model` (versão resolvida). Preço usado nos cookbooks: jev-1.12 **$0.042 / 1M tokens de input, $0.00 de output** ("output tokens are free"). Todos os cookbooks usam `JsonCache` (cache de chamadas em `json_cache.json`) e `make_playground_link`.

---

## Índice

1. [Self-consistency: nouls](#1-self-consistency-nouls-consistency_noul_cookbook)
2. [Self-consistency: choices](#2-self-consistency-choices-consistency_choice_cookbook)
3. [Parallel questions](#3-parallel-questions-parallel_questions)
4. [Re-ranking](#4-re-ranking-rerank_typesafe)
5. [Line-by-line search](#5-line-by-line-search-semantic_find)
6. [Structure recovery](#6-structure-recovery-autoformat)
7. [Function calling](#7-function-calling-function_calling)
8. [Skill suggestion](#8-skill-suggestion-skill_suggestion)
9. [Knowledge graph entity alignment](#9-knowledge-graph-entity-alignment-entity_alignment)
10. [Classifying RAG passages](#10-classifying-rag-passages-classifying_rag_passages)
11. [Double-checking citations](#11-double-checking-citations-citation_check)
12. [Guardrails for LLMs](#12-guardrails-for-llms-llm_guardrails)
13. [SDE cascade](#13-sde-cascade-sde_cascade)
14. [Date extraction](#14-date-extraction-date_extraction_cookbook)
15. [Pre-parsed value extraction](#15-pre-parsed-value-extraction-pre_parsed_value_extraction_cookbook)
16. [Hierarchical classification](#16-hierarchical-classification-hierarchical_classification)
17. [Autoresearch feature discovery](#17-autoresearch-feature-discovery-autoresearch_feature_discovery)
18. [Classification using confidence](#18-classification-using-confidence-classification_using_confidence)
19. [SÍNTESE TRANSVERSAL](#síntese-transversal)

---

## 1. Self-consistency: nouls (`consistency_noul_cookbook`)

**1. Problema e dataset.** Triagem de sinistro de seguro auto (pagar / negar / mandar a humano): mede se cada resposta "holds still across the repeats". Dataset = **1 sinistro** em JSON (apólice, sinistro com 4 line items, nota de auto-triagem, histórico) com armadilhas deliberadas (evento track-day mas no estacionamento; aluguel de carro sem cobertura; sem boletim policial > $2.000; auto-triage aprovou pagamento integral sem deduzir franquia). **14 perguntas × 15 repetições por condição.** Condições: `claude-haiku-4-5` e `gpt-5.4-mini` (t=0, t=default e modo yes/no t=0), `gpt-5.5` e `claude-opus-4-8` (reasoning, sem temperatura), TypeSafe `jev-latest` (as 15 chamadas retornaram `jev-1.13.0`). Amostrado em 2026-09-11.

**2. Desenho das perguntas.**
- Primitiva: **14 `Noul`**, só `instructions` (sem `criteria`), "phrased so a yes means the thing we are checking for is true" (ex.: `"covered": "Is the loss covered under the policy's collision coverage?"`, `"fraud_flag": "Are there indicators that warrant a fraud review?"`).
- `state = {"uid": f"{sample_index}:{token_hex(4)}", "claim": CLAIM}` — dict aninhado passado direto ("TypeSafe takes the structure as the state directly"; os LLMs recebem `json.dumps(CLAIM)`). O `uid` é um campo descartável para forçar chamadas distintas.
- **14 perguntas numa única chamada.** Sem speculative fan-out explícito.

**3. Consumo da resposta.** Banda de incerteza sobre P(true): `no` se `< 0.30`; `uncertain` de `0.30` a `0.70` **inclusive**; `yes` se `> 0.70`. `uncertain` → revisão humana. "no new question, no second API call".

**4. Resultados.**
- TypeSafe: desvio-padrão médio por pergunta **0.0102**, "below all LLM probability conditions here" (valores numéricos dos LLMs para std: **não informados**).
- Oscilação TypeSafe: `covered` **0.43–0.53** (cruza 0.5), `exclusion` **0.53–0.62**; as outras 13 ficam do mesmo lado de 0.5.
- Latência/custo por chamada de 14 perguntas (média de 15):

| condição | time/call | cost/call | lentidão vs TS | custo vs TS |
|---|---|---|---|---|
| claude-haiku-4-5 t=0 | 1780ms | $0.001798 | 16.0x | 42.2x |
| claude-haiku-4-5 t=default | 1644ms | $0.001798 | 14.8x | 42.2x |
| claude-haiku-4-5 yes/no t=0 | 1485ms | $0.001650 | 13.4x | 38.8x |
| gpt-5.4-mini t=0 | 1405ms | $0.001089 | 12.7x | 25.6x |
| gpt-5.4-mini t=default | 1177ms | $0.001179 | 10.6x | 27.7x |
| gpt-5.4-mini yes/no t=0 | 1113ms | $0.000950 | 10.0x | 22.3x |
| gpt-5.5-reasoning | 11125ms | $0.033157 | 100.2x | 778.9x |
| claude-opus-4-8-reasoning | 13886ms | $0.034275 | 125.0x | 805.1x |
| **typesafe_noul** | **111ms** | **$0.000043** | 1.0x | 1.0x |

  Custos com "historical price assumptions ... They are not verified `jev-latest` prices".
- Acurácia: **não medida** (é um estudo de repetibilidade).

**5. Boas práticas / armadilhas.**
- "The band is illustrative; it is neither a calibrated guarantee nor an optimized threshold. Set production boundaries from labeled examples and from the cost of incorrect decisions and of review."
- "A value near either outer boundary can still move between `uncertain` and yes or no. The model is no more deterministic for it".
- O `uid` confunde duas fontes: "This setup cannot separate sensitivity to the irrelevant field from variation that would occur on identical requests."
- LLMs variam mesmo em t=0; `claude-haiku-4-5` embrulha a resposta em cerca ```json apesar de "ONLY a JSON object".
- Engenharia: fingerprint (sha256) do state+rubrica na chave de cache para não servir resposta obsoleta; "Preserve the returned model because an alias can resolve to a different version later."

**6. Código.**
```python
def noul_decision_with_uncertainty(probability: float) -> str:
    """Map valid TypeSafe probabilities through an inclusive uncertainty band."""
    if probability < NOUL_UNCERTAINTY_LOW:
        return "no"
    if probability > NOUL_UNCERTAINTY_HIGH:
        return "yes"
    return "uncertain"
```

---

## 2. Self-consistency: choices (`consistency_choice_cookbook`)

**1. Problema e dataset.** Moderação de conteúdo: 1 post limítrofe (insulto + convite off-platform `discord.gg` + ameaça mal formulada, 1 strike prévio, 4 reports). **8 perguntas `Choice` × 15 repetições.** Mesmas condições do cookbook 1, com "single-pick" no lugar de yes/no. `jev-latest` → `jev-1.13.0` (15/15). 2026-09-11.

**2. Desenho das perguntas.**
- **8 `Choice`**: `category` (6 labels), `primary_risk` (5), `target` (4), `action` (5), `queue` (5), `link_handling` (4), `review_path` (4), `severity` (4). Cada label com descrição curta; labels "mutually exclusive (exactly one applies)".
- Exemplo de criteria: `"Harass": "Insults or demeans a person, with no threat of harm and no protected-class attack."`
- `state = {"uid": ..., "post": POST}` (POST = dict aninhado: author, context, content, reports).
- 8 perguntas numa chamada.

**3. Consumo da resposta.** `MIN_CHOICE_PROBABILITY = 0.60` sobre a **probabilidade do top label** — explicitamente **não** o campo `confidence`: "This uses the returned probabilities, not the API's separate `confidence` field, and adds no model calls." Abaixo → `uncertain` → humano; "At exactly `0.60`, select the top label."

**4. Resultados.**

| condição | raw agree | policy agree | uncertain | automatic | conflicts | mean prob std | max prob std |
|---|---|---|---|---|---|---|---|
| claude-haiku-4-5 t=0 | 100.0% | 100.0% | 0.0% | 100.0% | 0 | 0.0012 | 0.0221 |
| claude-haiku-4-5 t=default | 87.5% | 86.7% | 0.8% | 98.3% | 2 | 0.0516 | 0.3150 |
| gpt-5.4-mini t=0 | 99.2% | 87.5% | 12.5% | 87.5% | 0 | 0.0312 | 0.0905 |
| gpt-5.4-mini t=default | 90.8% | 84.2% | 22.5% | 77.5% | 2 | 0.0543 | 0.2303 |
| gpt-5.5-reasoning | 90.0% | 93.3% | 30.8% | 69.2% | 1 | 0.0305 | 0.1047 |
| claude-opus-4-8-reasoning | 92.5% | 94.2% | 33.3% | 66.7% | 0 | 0.0245 | 0.0693 |
| **typesafe_choice** | **90.8%** | **99.2%** | **25.8%** | **74.2%** | **0** | **0.0098** | **0.0515** |

- Parse fail: 1% só em haiku t=default.
- Antes da abstenção, TypeSafe troca o top label em `primary_risk` (Harassment 11×, Violence 4×) e `link_handling` (RmLink 8×, Brigade 7×); com o limiar 0.60 esses dois ficam `uncertain` em todas as repetições; `category` alterna entre Violence e `uncertain`.
- Latência/custo por chamada de 8 perguntas: TypeSafe **114ms / $0.000046**; haiku t=0 3853ms / $0.003498 (33.8x lento, 76.1x custo); haiku single-pick 992ms (8.7x / 33.2x); gpt-5.4-mini t=0 2293ms (20.1x / 50.0x); gpt-5.4-mini single-pick 826ms (7.2x / 20.3x); gpt-5.5-reasoning 12978ms / $0.041255 (113.7x / 897.4x); opus-4-8 reasoning 10376ms / $0.028375 (90.9x / 617.2x). LLMs rodaram num pool de 16 threads.
- Acurácia: **não medida**.

**5. Boas práticas / armadilhas.**
- "These percentages measure repeatability only." / "100% repeatability does not imply correctness." / "None of this shows accuracy or superiority: Haiku at temperature 0 had 100% agreement here, with no abstentions."
- "The threshold is an illustrative application policy, not a calibrated guarantee ... Choose production thresholds using labeled examples and the cost of incorrect actions and human review."
- "This policy does not make the model deterministic ... a probability near `0.60` can still move between a concrete label and `uncertain`."
- Single-pick não fornece incerteza (one-hot sintético) → excluído das métricas de concordância; parse failure conta contra concordância.
- "Small changes can still switch the top label when two labels are close."

**6. Código.**
```python
def choice_decision_with_uncertainty(values: list, labels: list[str]) -> str | None:
    """Abstain below the action threshold; retain invalid results as parse failures."""
    label = argmax_label(values, labels)
    if label is None:
        return None
    probabilities = [float(value) for value in values]
    if any(value < 0 or value > 1 for value in probabilities):
        return None
    return label if max(probabilities) >= MIN_CHOICE_PROBABILITY else "uncertain"
```

---

## 3. Parallel questions (`parallel_questions`)

**1. Problema e dataset.** 1 documento × N perguntas: 1 requisição com N perguntas vs N requisições. Documento: artigo da Wikipedia sobre GDPR, revisão fixada `1363040264`, **53,777 caracteres** ("document-dominated workload"). 13 perguntas de compliance. `RUNS = 5` por estratégia. jev-1.12.

**2. Desenho das perguntas.**
- **8 `Noul`** (só instructions, ex.: "Must a personal data breach be reported to the supervisory authority within 72 hours?"), **2 `Choice`** (4 opções cada, com descrição: `instrument_type`, `max_fine`), **3 `Score`** (`individual_rights` 4 níveis, `penalty_severity` 4, `compliance_burden` 5; criteria = lista de descrições do nível 0 para cima).
- `state = {"article": {"source": url, "text": ...}}`; documento "byte-identical in every call".
- Métrica rastreada: Noul → p(yes); Choice → max prob; Score → `score / (len(criteria) - 1)`.

**3. Consumo.** Comparação de média e desvio-padrão entre estratégias; nenhum threshold de decisão.

**4. Resultados.**
- Médias iguais; std **exatamente 0.0** em 11 das 13 perguntas nas duas estratégias. Exceções: `breach_72h` média 0.804 (batched) vs 0.814 (single), std 0.0055/0.0055; `criminal_penalties` 0.108/0.108, std 0.0045 (batched) vs 0.0084 (single).
- Custo: **$0.000497** (1 chamada, 13 perguntas) vs **$0.006090** (13 chamadas) → **12.2x mais barato**. Tempo: **0.27s** vs **2.71s** (soma sequencial) → **10.0x mais rápido**.

**5. Boas práticas / armadilhas.**
- "each question is scored on its own against the document, so its answer doesn't depend on what else is in the request."
- "The noise is a property of the question, not of how you batch: batching neither shifts the answer nor adds variance."
- "The bigger the document, the nearer that saving comes to a full Nx."
- "Fire them concurrently and the gap shrinks, but the 13x token cost stays."

**6. Código.**
```python
    response = client.system_one(
        state={"article": DOCUMENT},
        questions={key: QUESTIONS[key] for key in keys},
        model=TYPESAFE_MODEL,
    )
    values = {}
    for key in keys:
        answer = response.answers[key]
        if isinstance(answer, NoulAnswer):
            values[key] = answer.noul
        elif isinstance(answer, ChoiceAnswer):
            values[key] = max(answer.probabilities.values())
        else:
            values[key] = answer.score / (len(QUESTIONS[key].criteria) - 1)
```

---

## 4. Re-ranking (`rerank_typesafe`)

**1. Problema e dataset.** Two-stage retrieval: fast search (BM25 via `bm25s`) → re-rank com TypeSafe. CLERC (jurídico): 170 linhas agregadas → corpus de **3,565 passagens** de opiniões judiciais; **40 queries** avaliadas; shortlist **TOP_K = 30**. jev-1.12.

**2. Desenho das perguntas.**
- **1 `Noul` por par query–candidato** (`is_cited_source`), instructions longas e específicas ("Could the candidate passage be from that cited precedent — does it establish the specific legal proposition the query excerpt invokes at its citation point?") + `NoulCriteria(true="The candidate passage states or establishes the specific rule, standard, holding, or fact pattern ...", false="The candidate passage is merely on a similar topic or doctrine ...")`.
- `state = {"query_excerpt": query, "candidate_passage": candidate}`.
- 1 pergunta por chamada; 40 × 30 = **1,200 chamadas**, `ThreadPoolExecutor(max_workers=12)`. Pergunta serializada como JSON (`msgspec`) — "the SDK takes a question as its JSON dict".

**3. Consumo.** Ordena a shortlist pelo `noul`, maior primeiro. Sem threshold.

**4. Resultados.**
- BM25: gold no top 30 em **100%** das queries; no rank 1 em **5%**.
- Com re-rank: **Top 1: 5% → 18%**; **Top 5: 15% → 35%**; **Top 10: 38% → 62%**.
- 1,200 chamadas: **1,536,002 tokens de input + 25,200 de output → $0.0645**. Latência: **não informada**.

**5. Boas práticas / armadilhas.**
- "Re-ranking below only reorders the top 30 candidates already on the shortlist. It cannot add a passage that fast search did not select."
- Com LLM genérico, "Repeated calls can still produce different scores for the same pair"; com Noul "No scoring scale has to be invented".
- "This walkthrough asked one question per pair for clarity. A real application would ask several questions about the same pair in one call." (remete a Parallel questions e Speculative Fan-Out).
- O critério `false` separa "specific proposition" de "merely on a similar topic" — o near-miss temático é o erro que se quer rebaixar.

**6. Código.**
```python
nouls = {candidate: ask_typesafe(query, candidate) for candidate in shortlist}
reranked = sorted(shortlist, key=lambda c: nouls[c], reverse=True)  # highest noul first
```
```python
    question = json.loads(question_json)
    response = client.system_one(
        state={"query_excerpt": query, "candidate_passage": candidate},
        questions={"is_cited_source": question},
        model=model,
    )
```

---

## 5. Line-by-line search (`semantic_find`)

**1. Problema e dataset.** Busca semântica linha a linha + detecção de "não há resposta". Termos de Serviço do GitHub: **218 linhas/cláusulas, 43,980 caracteres**. 4 queries. jev-1.12.

**2. Desenho das perguntas.**
- Cada linha prefixada com ID (`L052| You own Your Content...`); `state = DOCUMENT` (string com IDs), **inalterado entre buscas**; a query vai em `instructions`.
- `where`: **`Choice` com 218 opções** = IDs, `criteria={line_id(i): None ...}` ("The option descriptions are `None` because the document already contains the text for each ID").
- `exists`: **`Noul`** com `NoulCriteria(true="At least one line of the document states or directly implies the answer", false="No line of the document addresses this")`.
- 2 perguntas, 1 requisição.

**3. Consumo.** `relevance` = probabilidade por linha (ranking). `FOUND, ABSENT = 0.7, 0.35` sobre `exists` ("present answers typically read >=0.9, absent <=0.05"): ≥0.7 "answered", <0.35 "not in this document", entre → "partially addressed".

**4. Resultados.**
- "who owns the code I upload?" → exists **0.98**, L052 **0.95**.
- "can GitHub kick me off the platform without warning?" → exists **0.97**, L168 **0.97**.
- "do I have to take disputes to arbitration?" → exists **0.14** (não está no doc) embora o top line tenha **0.86**.
- "can minors use GitHub with parental permission?" → exists **0.46** (parcial), L029 **0.90**.
- Latência/custo: **não informados**.

**5. Boas práticas / armadilhas.**
- "Choice probabilities always add up to 1, so some line ranks first even when the document doesn't answer the question. The ranking alone can't distinguish a real answer from the closest irrelevant line."
- "Unlike the Choice probabilities, the Noul probability doesn't depend on the other options, so it can fall near zero".
- Limite: "A `Choice` question accepts up to 255 options ... Past that, search in two passes: one Choice question picks a window of lines, and a second ranks the lines inside it."
- "tune them against your own documents before using them in production."
- "The ranking tells you where to look; the `exists` score tells you whether the result answers the question."

**6. Código.**
```python
def where_question(query: str) -> Choice:
    return Choice(
        instructions=f'Which line of the document contains the answer to: "{query}"?',
        criteria={line_id(i): None for i in range(len(LINES))},
    )
```
```python
FOUND, ABSENT = 0.7, 0.35  # present answers typically read >=0.9, absent <=0.05


def verdict(exists: float) -> str:
    if exists >= FOUND:
        return "answered in this document"
    return "not in this document" if exists < ABSENT else "partially addressed"
```

---

## 6. Structure recovery (`autoformat`)

**1. Problema e dataset.** Reconstruir Markdown de texto plano sem marcação (quebras duras, sem bullets/headings) **sem o modelo gerar texto**: "every character of the output comes from the input". Dataset: 1 memo (gist fixado), **28 linhas não vazias → 17 blocos**. jev-1.12.

**2. Desenho das perguntas.** Duas requisições em sequência.
- **Pass 1 (stitch):** 1 `Noul` por par adjacente (pula pares separados por linha em branco) → **16 perguntas numa requisição**. Instruction: "Does line {Lx} pick up mid-sentence, continuing a sentence left unfinished at the end of line {Ly}?" + NoulCriteria. `state = tag(LINES, "L")` (linhas com IDs `L014| `).
- **Pass 2 (classify):** por bloco, `type` (`Choice`, 6: heading/paragraph/list_item/quote/code/callout), `hlevel` (`Choice`, 3; só se o bloco tem ≤ `HEADING_MAX_CHARS = 90`), `step` (`Noul`: ordem importa?), `callout` (`Choice`, 3: note/tip/warning) → **62 perguntas sobre 17 blocos, uma requisição**. As companheiras são **speculative fan-out**: "the companion questions are asked up front in the same request. Most of these answers are never read".
- "All of the behavior is specified in the pass-2 question criteria" — 3 dicts de descrições de uma linha.
- Evidência direta fica no código: linhas em branco e marcadores explícitos (`- `, `1.`, `#`) nunca vão ao modelo.

**3. Consumo.**
- Merge: `JOIN_AFTER_DANGLING, JOIN_AFTER_TERMINAL = 0.2, 0.5` — limiar escolhido em código conforme a linha anterior termina com pontuação terminal (`.` `!` `?` `:` `;`) ou não.
- Listas numeradas se a **média** dos `step` do grupo ≥ `STEP_THRESHOLD = 0.5` ("a group-level decision no single question asked directly").
- Sugestão de UI: sublinhar para revisão blocos com "type confidence (the probability behind the winning choice) ... under 0.55".

**4. Resultados.**
- Pass 1: 16 perguntas, **0.32s**; pass 2: 62 perguntas, **0.51s**; total **10,211 tokens, 0.8s**. Custo: o texto diz "**$0.0015**", mas o output impresso da célula mostra "**$0.0003**" — **inconsistência na própria fonte**.
- 11 quebras curadas (28 → 17). Confianças de tipo: 0.43 a 1.00 (B006 paragraph **0.43**: paragraph 0.53, list_item 0.24, callout 0.19; B014 callout **0.65**, kind=warning; B002 heading 0.75).
- Wording ingênuo "same paragraph": itens de lista pontuam **0.77–0.91** (vs **0.05–0.22** com "mid-sentence") → **12 blocos vs 17**.
- Probabilidades de junção: quebras reais ≥ 0.39; intencionais perto de zero; `L015` após dois-pontos = 0.22.

**5. Boas práticas / armadilhas.**
- "When a judgment call feeds a threshold, the question should name the narrowest fact that decides it. Here the wording is the difference between 17 blocks and 12."
- "'Same paragraph' asks the model to judge whether the topic carries over ... 'Picks up mid-sentence' asks about the text itself."
- "No single threshold works for both cases; once code checks the punctuation first, the two bands separate."
- "An extra question adds little, since the state is most of the tokens and is sent once either way, while an extra round trip adds a full request of latency."
- "The model gets only the questions code cannot answer from the text."

**6. Código.**
```python
def join_question(i: int) -> Noul:
    return Noul(
        instructions=f"Does line {line_id(i)} pick up mid-sentence, continuing a sentence left unfinished at the end of line {line_id(i - 1)}?",
        criteria=NoulCriteria(
            true="The line starts in the middle of a sentence that began on the previous line - the line break tore the sentence apart",
            false="The line begins a new sentence, item, heading, or thought of its own",
        ),
    )
```

---

## 7. Function calling (`function_calling`)

**1. Problema e dataset.** Linguagem natural → chamada de função tipada, sem LLM gerando JSON. Assistente de trading com **10 funções** sobre **156,780 barras de 1 minuto**; **28 argumentos preenchíveis**; **14 comandos** de teste. jev-1.12.

**2. Desenho das perguntas.**
- `closed_sets` lê as type hints: `Literal` → **choice** (`Choice` sobre exatamente os valores); `list[Literal]` → **set** (um `Noul` por membro: `"Does the user want {} in the comparison?"`); `bool` → **flag** (`Noul`). `int`, texto livre e datas: sem pergunta, vale o default.
- `spec.json` (pode ser escrito por um LLM a partir das assinaturas): por argumento `question`, `stated` (Noul "o comando diz algo sobre este argumento?" → se não, o argumento é omitido e vale o default) e `options` com descrição; por função, uma descrição; `__tool__` = `Choice` sobre as 10 funções.
- As chaves das opções **são** as strings que a função aceita ("nothing has to map a label back").
- **54 perguntas por comando, 1 requisição**, carregando os argumentos de **todas** as funções; "the dispatcher reads only the chosen function's answers" (speculative fan-out).
- `state` = o comando (string).

**3. Consumo.** `confidence` da chamada = **o julgamento menos certo** (mínimo), não o produto: "one wrong argument is enough to spoil the result. A product answers a different question ('is every part right'), and it falls as a function takes more arguments". `call.weakest()` nomeia o argumento mais fraco. Threshold de ação: **não informado** (nenhum definido).

**4. Resultados.** 14 comandos com confidence entre **0.53** (`intraday_pattern`) e **1.00** (`list_symbols`); ex.: `rolling_correlation(symbol='NVDA', benchmark='SPY', window='1mo')` 0.91; `compare_returns([...], '3mo')` 0.94; `plot_price(... candles, ma='20')` 0.69; "is amd tracking nvidia lately" → `rolling_correlation(symbol='AMD', benchmark='NVDA')` 0.82 (symbol p 0.87, benchmark p 0.78, window/resolution omitidos p 0.96/0.99). Acurácia agregada, latência, custo: **não informados**.

**5. Boas práticas / armadilhas.**
- "Write each question about the idea rather than the words a user might pick, because the match is on meaning".
- "Avoid naming a question after its parameter - `\"Which resolution?\"` gives the command nothing to match against."
- Papéis explícitos para argumentos que compartilham o mesmo conjunto: "*the one being measured, named first*" vs "*the second one named, the yardstick*".
- Sem `stated`, "the choice would have to name some window, and it would have named one confidently."

**6. Código.**
```python
assistant = Dispatcher(SPEC, TOOLS, client)
```
```python
call = CALLS["is amd tracking nvidia lately"]
print(f'"is amd tracking nvidia lately"  ->  {call}   confidence {call.confidence:.2f}')
for name, argument in call.arguments.items():
    top = sorted(argument.distribution.items(), key=lambda kv: -kv[1])[:3]
    shown = "omitted, default stands" if argument.omitted else repr(argument.value)
```

---

## 8. Skill suggestion (`skill_suggestion`)

**1. Problema e dataset.** Escolher **no máximo uma** skill por turno de agente. Roster Hermes (NousResearch/hermes-agent): **182 skills, 33 categorias**; prompt do roster **16,089 caracteres**; descrição no índice com **média 54, máximo 60 caracteres** (truncada pelo harness). **488 requests**: 315 cobertos por exatamente uma skill (171 skills distintas) + 173 sem skill (85 cotidianos, 42 perguntas técnicas, 46 pedidos específicos sem skill). Agente: `claude-haiku-4-5-20251001`. jev-1.12, 2026-07-31.

**2. Desenho das perguntas.** Duas requisições (progressive disclosure).
- **Call 1 (`rank_wide`)**: `which` = `Choice` sobre **182 nomes**, criteria = descrição do índice (o mesmo texto que o agente vê) + 3 `Noul` de gate: `acts_on_user_system`, `would_follow_documented_procedure`, `prose_suffices` (este **invertido** em código: `1 - v`). `state = {"request": ..., "recent_context": ""}`.
- **Call 2 (`rerank`)**: `which` = `Choice` sobre o **top 3** (`SHORTLIST = 3`), criteria = `description_full` + primeiros **700** caracteres do SKILL.md (`EXCERPT_CHARS`); + `fits::{name}` = um `Noul` por candidato ("Does the skill '{name}' do the specific thing the user's request asks for?").

**3. Consumo.**
- Gate = média dos 3 nouls orientados; `< GATE_THRESHOLD = 0.30` → não sugere nada.
- Se `max(fits) < FITS_THRESHOLD = 0.30` → descarta a shortlist inteira.
- Senão sugere `which.choice`. "The Choice settles *which* skill, and the nouls settle *whether* to say anything at all."
- Saída: um bloco `<skill_relevance>` **depois** do roster (preserva prefix caching), com "Ignore this if it does not fit"; quando nada serve, envia "No skill in the roster appears relevant to this request."

**4. Resultados.**

| | wrong load | needless load |
|---|---|---|
| agente só com o roster | 16.8% | 9.8% |
| **agente com sugestão TypeSafe** | **7.3%** | **4.0%** |
| agente com a resposta certa (oracle) | 2.5% | 1.2% |

- **2.3x menos** wrong loads, **2.4x menos** needless loads. Dos 315 cobertos: **37 corrigidos, 7 quebrados** pela sugestão. Baseline: 36 primeiras escolhas erradas, 10 na mesma categoria da certa.
- Latência: call 1 **0.16–0.31s**, call 2 **0.09–0.12s** (demos). Custo: **não informado**.
- Demo: deck `.pptx` — call 1 põe `powerpoint` 0.700 à frente de `pptx-author` 0.300; call 2 inverte para `pptx-author`. Mastodon → sugere `xurl` (X/Twitter) erroneamente (fits 0.56 > 0.30).

**5. Boas práticas / armadilhas.**
- "Write these three to ask whether an action is wanted. A question about subject matter will not separate *explain what a monad is* from a request that needs a skill, since both are software."
- "One `Choice` question holds a roster this size comfortably. A few times larger and you would split it into chunks and rank each one".
- "it says the suggestion can be ignored, because pushing harder wins compliance on wrong suggestions too, and a wrong one is worse than none."
- "sending nothing at all would leave the roster's own 'err on the side of loading' instruction unopposed."
- "A confident wrong suggestion is more persuasive than no suggestion at all".
- "The second pass can only reject what the wide ranking hands it".
- Viés do dataset: requests cobertos escritos pelo Claude Sonnet 5 a partir do SKILL.md → "easier than the ones users send".
- "Copy this shape when an agent of yours carries a large roster: a cheap ranking over everything, then a close look at two or three. Either step may come back empty-handed."

**6. Código.**
```python
def suggest(request: str) -> tuple[str, ...]:
    """At most one skill name for a request, or () for "nothing here applies"."""
    wide = rank_wide(request)
    if wide["gate"] < GATE_THRESHOLD:
        return ()
    shortlist = tuple(name for name, _ in wide["ranked"][:SHORTLIST])
    result = rerank(request, shortlist, EXCERPT_CHARS)
    if max(result["fits"].values()) < FITS_THRESHOLD:
        return ()
    return (result["winner"],)
```

---

## 9. Knowledge graph entity alignment (`entity_alignment`)

**1. Problema e dataset.** Decidir se dois registros descrevem o mesmo produto (merge / deixar separado / curadoria). Magellan **Beer**: **450 pares candidatos**, 4 campos (name, brewery, style, abv), rótulo `known_same_as`; texto sem pré-processamento (entidades HTML, apóstrofos separados). jev-1.12, 2026-08-11.

**2. Desenho das perguntas.**
- **1 `Score` de 3 níveis = os 3 desfechos**: "They describe two different products." / "...closely related products that may or may not be the same one: a variant, a special edition, or a name that could plausibly refer to either." / "They describe one and the same product."
- **+3 `Noul`** (`same_name`, `same_brewery`, `same_style`), só para informar o curador. `abv` sem pergunta: "comparing two numbers is arithmetic; compute it in code".
- `state = {"entity_a": ..., "entity_b": ...}` — a pergunta é sobre o par. 4 perguntas, 1 requisição por par; `MAX_WORKERS = 6` ("the public endpoint rate-limits above roughly eight").
- Por que Score: rótulo semântico por desfecho, incluindo o do meio; "A Noul question could accomplish this indirectly through thresholding ... and a Choice question would lose the ordered relationship of the three outcomes."

**3. Consumo.** Arredonda para o nível mais próximo: `OUTCOME[min(int(score + 0.5), 2)]` → pontos de corte implícitos **0.5** e **1.5**. "There is no threshold constant anywhere in this file."

**4. Resultados.**
- **assert sameAs 40 (8.9%)**, **curator queue 50 (11.1%)**, **leave unlinked 360 (80.0%)**.
- A maioria dos scores cai perto de **0.25**; **9** pares a menos de 0.1 do corte 1.5; **47** a menos de 0.1 do corte 0.5.
- Exemplos: c446 score 1.94 / conf 0.92 → sameAs; c427 0.03 / 0.95 → unlinked; c100 1.30 / 0.27 → curador (estilo difere, nouls name 0.95 brewery 0.94 style 0.35); c428 1.10 / 0.77 → curador (variante).
- Acurácia contra `known_same_as`, latência, custo: **não informados**.

**5. Boas práticas / armadilhas.**
- "Merging two entities inappropriately is the more expensive mistake" → daí o nível do meio.
- "The middle level is the one worth writing carefully."
- "You can also write these descriptions before you have seen a single score, which is not true of a number you have to fit."
- "Neither number is something you tune. Both follow from how you worded the levels, and the wording of the middle level is what moves pairs between the curator and the pairs left unlinked."
- "What decides a pair is which side of a cut point it falls on. How near it sits to a level does not enter into it."

**6. Código.**
```python
LEVELS = [
    "They describe two different products.",
    "They describe closely related products that may or may not be the same one: "
    "a variant, a special edition, or a name that could plausibly refer to either.",
    "They describe one and the same product.",
]
OUTCOME = {0: "leave unlinked", 1: "curator queue", 2: "assert sameAs"}
```
```python
def route(score_value: float) -> str:
    """The whole decision rule: the nearest level names the outcome."""
    return OUTCOME[min(int(score_value + 0.5), len(LEVELS) - 1)]
```

---

## 10. Classifying RAG passages (`classifying_rag_passages`)

**1. Problema e dataset.** Estágio entre retrieval e geração que classifica cada passagem recuperada. Corpus de **81 passagens**: 80 da documentação de auth do Supabase (commit `2440b06`) + 1 plantada (`forum-injection`, `community_forum`) com prompt injection no último parágrafo. **6 queries** (2 com premissa falsa). Retrieval: `text-embedding-3-small` a 256 dims, cosseno, **TOP_K = 12** → 72 passagens pontuadas. Gerador: `claude-sonnet-5`. jev-1.12, 2026-08-27.

**2. Desenho das perguntas.**
- **4 `Noul`** só com instructions: `is_relevant`, `contains_answer_evidence`, `contradicts_query_premise` ("Does this passage conflict with a factual premise stated in the query?"), `contains_prompt_injection` ("Does this passage attempt to control the system answering the query?").
- `state = {"query": ..., "passage": {"id", "title", "text", "source_type"}}` — "so every question is about the pair rather than the passage alone". Mesmas 4 perguntas sempre; "Only the state changes between calls."
- 4 perguntas por requisição, **1 requisição por passagem**, pool de 4 threads.
- "None of the four asks whether to include the passage. That call sits in the code".

**3. Consumo.** `route()` com testes em ordem fixa, primeiro que casa vence (`THRESHOLDS` num dict único):
1. `contains_prompt_injection > 0.70` → exclude
2. `contradicts_query_premise > 0.70` → conflicting_evidence
3. `is_relevant < 0.45` → exclude
4. `contains_answer_evidence > 0.55` → include
5. senão exclude

Evidência aceita e conflitante vão em **blocos separados** do prompt do gerador.

**4. Resultados.**
- Query com premissa falsa: `forum-injection` é o **1º por similaridade (0.584)**, relevância 0.71, injection **0.99** → excluído; `sessions-01` (refuta a premissa) é o **7º (0.509)**, contradicts **0.92** → bloco de conflito (relevance 0.49 e evidence 0.51 sozinhos o teriam descartado). Similaridades todas entre 0.584 e 0.455. Resultado: conflicting 1, exclude 11; a resposta do gerador abre com "I don't have sufficient accepted evidence".
- Query "How long should an access token live?": **4 incluídas**, 3 delas estavam em **8º, 9º e 11º** na similaridade; os ranks 2–4 ("Lifetime of a signing key") têm relevância ≤ **0.08**; injection de novo 0.99.
- 72 passagens: pelo menos 2/3 de cada query excluídas; só as 2 de premissa falsa geram conflito; 2 queries não aceitam nada.
- Custo/latência agregados: **não informados**.

**5. Boas práticas / armadilhas.**
- "Injection comes first because it is a security decision, not an evidence one."
- Contradição antes de evidência: "a passage that denies the query's premise usually states something usable too; tested the other way round, it would land in the accepted block".
- "Treat them as a starting point, not defaults ... re-routing every passage costs no API calls."
- "The injection question is a filter, and only one ... the generator prompt has to treat every passage as untrusted text regardless of its score. Nothing here is a security boundary."
- "One request per passage, so cost scales with `k`. Nothing batches passages into one request, because each question is about one pair."
- "Merge them into one and the generator has no way to tell a passage that answers the query from one that denies its premise."
- Política como constante: "a change of policy is a constant edit under code review, not a reworded question."

**6. Código.**
```python
def route(answers: dict, thresholds: dict = THRESHOLDS) -> str:
    if answers["contains_prompt_injection"] > thresholds["injection_max"]:
        return "exclude"
    if answers["contradicts_query_premise"] > thresholds["contradicts_min"]:
        return "conflicting_evidence"
    if answers["is_relevant"] < thresholds["relevant_min"]:
        return "exclude"
    if answers["contains_answer_evidence"] > thresholds["evidence_min"]:
        return "include"
    return "exclude"
```

---

## 11. Double-checking citations (`citation_check`)

**1. Problema e dataset.** Citações geradas por LLM (claim + seção + quote) podem ser fabricadas ou contradizer o contexto. Fonte: RFC 7519 (JWT), **58,365 caracteres, 45 seções numeradas**; **8 citações** (4 corretas, 4 editadas para falhar). jev-1.12, 2026-08-16.

**2. Desenho das perguntas.**
- Passo 1 (sem modelo): normaliza espaços e aspas curvas e procura o quote como substring → ausente = `fabricated`. Citação sem quote vai direto ao modelo com a seção indicada.
- Passo 2: **1 `Choice`** `relation`, 3 opções com criteria: `supports` ("states the claim or directly implies that it is true"), `contradicts`, `says_nothing` ("does not address what the claim asserts, either way").
- `state = {"claim": claim, "section": section}` — só a seção (270 a 3,122 caracteres nos exemplos), não o documento inteiro.

**3. Consumo.** Veredito = maior probabilidade; `AUTO_ACCEPT = 0.8` sobre `confidence`: ≥0.8 o veredito vale sozinho; <0.8 um humano confirma.

**4. Resultados.**

| citação | status | relação | conf | veredito | ação |
|---|---|---|---|---|---|
| epoch_seconds | found | supports | 0.93 | verified | auto |
| aud_reject | found | supports | 0.95 | verified | auto |
| sig_reporting | missing | – | – | fabricated | auto |
| clock_skew | found | supports | 0.99 | verified | auto |
| exp_required | found | contradicts | 0.99 | contradicted | auto |
| pii_encryption | found | says_nothing | 0.27 | unsupported | review |
| iat_future | section-only | says_nothing | 0.56 | unsupported | review |
| duplicate_names | found | supports | 0.99 | verified | auto |

As 4 falhas plantadas foram pegas; as 4 corretas voltaram `verified` com conf ≥ 0.93. Latência/custo: **não informados** (o código mede `seconds` e tokens, mas não os imprime).

**5. Boas práticas / armadilhas.**
- `AUTO_ACCEPT = 0.8  # start high for more human review as you build trust in the model`; "Start high, and lower the threshold as you see how the model does on your own documents."
- "A quote that is not in the source is fabricated, and no model is needed to find that out."
- "the quote can be accurate and the claim built on top of it still wrong" (`pii_encryption`: quote verbatim, seção não fala do claim).
- Limite: "a quote that is truncated or lightly reworded comes back as `fabricated`. A production system that tolerates sloppy quoting would need fuzzy matching instead."

**6. Código.**
```python
QUESTIONS = {
    "relation": Choice(
        instructions="How does the section relate to the claim?",
        criteria={
            "supports": "The section states the claim or directly implies that it is true",
            "contradicts": "The section states the opposite of the claim or implies it is false",
            "says_nothing": "The section does not address what the claim asserts, either way",
        },
    ),
}
```

---

## 12. Guardrails for LLMs (`llm_guardrails`)

**1. Problema e dataset.** Triagem de toda mensagem que entra e sai de um app LLM (pass / review / block / support). **10 prompts de usuário + 5 respostas de modelo**; jailbreaks reais do dataset público TrustAIRLab "in-the-wild jailbreak prompts". jev-1.12, 2026-08-15.

**2. Desenho das perguntas.**
- **Input battery**: 4 `Noul` com NoulCriteria explícitos (`jailbreak`, `harmful_request`, `medical_advice`, `self_harm`) + **`Score` `severity`** de 4 níveis ("No harm" / "Mild" / "Serious" / "Severe", escala 0–3).
- **Output battery**: os mesmos 4 temas do lado da resposta (`broke_policy` no lugar de `jailbreak`) + a mesma `severity`.
- `state` = o texto da mensagem (string). 5 perguntas, **1 requisição por mensagem**.
- "'Out of bounds' is not one question, so the battery splits it."

**3. Consumo.**
- Por Noul: ≥ **action threshold** → ação do hazard (`jailbreak/broke_policy/harmful_request` → block; `medical_advice` → review; `self_harm` → support); ≥ **review threshold** → review.
- `severity ≥ severity_block` converte review → block. Precedência `support > block > review > pass`.
- `POLICIES`: **strict** = review 0.35, action **0.70**, severity_block 2.0; **permissive** = review 0.35, action **0.85**, severity_block 2.0.

**4. Resultados (strict).** Inputs: banana_bread/https_explainer/prescription_info → pass; `melatonin_dose` medical 0.55 → review; `dosage_request` medical 0.95, sev 2.0 → **block** (texto: "a severity of 2.02 crosses the block line", convertendo review em block); `novelist_poison` jailbreak 0.05 sev 0.8 → pass; `lockpick_burglary` harmful 0.95 → block; `self_harm` 0.96 → **support**; `dan` jailbreak 0.98 → block; `neurosemantical` jailbreak 0.74 → block. Outputs: `good_refusal` (recusa sobre invasão de casa) → pass; `dosage_request` medical 0.98 → block; `jailbroken` broke_policy 0.94 → block. Mesmo resultado de `neurosemantical` (0.74, sev 0.51): strict → block, permissive → review. Acurácia agregada, latência, custo: **não informados**.

**5. Boas práticas / armadilhas.**
- "TypeSafe supplies the assessment; your application owns the decision."
- System prompt como guarda = "exactly the place a jailbreak talks its way past"; segundo LLM na frente = custo de uma chamada por turno "and an attacker can talk that one past too."
- "Run this TypeSafe check both on LLM inputs, and on LLM outputs, because even ordinary-looking prompts can lead to harmful generated replies."
- "A policy is just those numbers under a name, which makes the trade-off something a product picks rather than inherits."
- "set the thresholds in `POLICIES` from labeled examples of your own traffic."

**6. Código.**
```python
def route(nouls: dict[str, float], severity: float, policy: dict) -> str:
    """Turn one message's TypeSafe assessment into one policy-specific action."""
    triggered = []
    for hazard, probability in nouls.items():
        if probability >= policy["action_threshold"]:
            triggered.append(HAZARD_ACTION[hazard])
        elif probability >= policy["review_threshold"]:
            triggered.append("review")
    if severity >= policy["severity_block"]:
        triggered = ["block" if action == "review" else action for action in triggered]
    return next((action for action in PRECEDENCE if action in triggered), "pass")
```

---

## 13. SDE cascade (`sde_cascade`)

**1. Problema e dataset.** Structured data extraction em cascata: extrair com modelo barato → verificar com TypeSafe → escalar para reasoning só se um sinal disparar. Dataset `scrapegraphai/scrapegraphai-100k` (revisão fixada), exemplo linha **516** (página de calendário da NYU sem data de matrícula). Modelos e preços (checados 15/09/2026): rung 0 `gpt-5.4-mini` $0.75/$4.50; rung 1 `gpt-5.5` (reasoning_effort="high") $5.00/$30.00 ("roughly 7x the mini"); verificador `jev-1.12` $0.042/$0.00.

**2. Desenho das perguntas.**
- Bateria de **`Noul` por campo**, framing "`true` = something is wrong (escalate)". Para campos não vazios, 7 métricas: `name_desc_mismatch`, `type_mismatch`, `unreasonable`, `hallucinated`, `off_target`, `incomplete`, `format_violation` (cada uma com NoulCriteria). Campo vazio → só `absence_wrong`. `type_mismatch` pulado se tipo desconhecido.
- **`instructions` estruturado como dict JSON**: `{"field_spec": spec, "extracted_field": value, "main_question": question}`.
- Mais um `__overall__::judge` holístico — exibido, **não** usado no gate.
- `state = {"system_message", "instruction", "source_text", "schema", "extraction"}`. Tudo numa chamada `system_one`. IDs `field::metric`.
- "The TypeSafe Way: Decomposition ... Decomposition maximizes the intelligence of every prompt, and makes the algorithm tunable and interpretable."
- (O pipeline completo tem ainda um head `spurious` e um score `difficulty`, não mostrados.)

**3. Consumo.** Gate `any_flag`: escala se **qualquer** P(wrong) por campo `> FIRE_T = 0.7` (gate tipo `max`, não média).

**4. Resultados.**
- Exemplo: mini produz `description: "Registration opens for the fall semester"` (fabricado, copiado do exemplo do próprio schema) — **schema-valid: True**.
- Bateria: `description::hallucinated` **0.95** (fires), `description::off_target` **0.85** (fires), `unreasonable` 0.58, `__overall__::judge` **0.56** (não dispararia), `incomplete` 0.16, `registration_open_date::absence_wrong` 0.14, `format_violation` 0.10, `name_desc_mismatch` 0.08, `type_mismatch` 0.02 → ESCALATE; o reasoning devolve `description: ""`.
- 100 prompts (resultados internos, gráfico, custos não recalculados ao preço atual): `gpt-5.5-reasoning` sozinho ≈ **0.81 de qualidade a ≈ $0.10/extração**; a fronteira de Pareto da cascata fica "up-and-left of every single model". Números exatos da cascata (qualidade/custo/% escalado): **não informados**.
- O output do mini é **hard-coded** no notebook porque é "very stochastic ... even at `temperature=0`".

**5. Boas práticas / armadilhas (Appendix A).**
- "**Narrow and grounded.** ... vague questions give mushy, uncalibrated scores".
- "**Bad = TRUE, with explicit criteria.**"
- "**Per-field, then aggregate with `max`.** ... one confident red flag escalates, instead of being averaged into silence".
- "**Independent and cheap.** ... it has to be cheap, or there are no savings left to capture".
- "**Separating / calibrated.** ... that separation is what pushes the pareto curve up-and-left".
- "Schema validation is necessary but not sufficient: it catches structural errors, never semantic ones."
- Não usaram structured outputs / json mode nas extrações: "a *schema following* mistake is not the mistake we expect an LLM to make"; JSON inválido vira extração vazia → o verificador escala ("the safe direction").

**6. Código.**
```python
            questions[f"{name}::{metric}"] = Noul(
                instructions={
                    "field_spec": spec,
                    "extracted_field": value,
                    "main_question": question,
                },
                criteria=criteria,
            )
```
```python
fired = {
    qid: p
    for qid, p in checks.items()
    if not qid.startswith("__overall__") and p > FIRE_T
}
escalate = bool(fired)
```

---

## 14. Date extraction (`date_extraction_cookbook`)

**1. Problema e dataset.** `extract_date(document, role)` → `date` + confidence, datas absolutas e relativas ("tomorrow", "next Thursday"). **4 documentos curtos, 6 perguntas** (1 sem data correspondente). `TODAY = 2026-07-30` fixo (quinta-feira). jev-1.12.

**2. Desenho das perguntas.** **7 `Choice` numa chamada**; o modelo "reads what the text says and never does the calendar math":
- `mode` (absolute / relative / none; criteria `None`, definições nas instructions);
- `month` (12 + `none`), `day` (1–31 + `none`), `year` (**1900–2050 = 151 opções** + `out_of_range` + `none`), `day_anchor` (today/tomorrow/day_after/weekday/none), `weekday` (7 + none), `week_offset` (current/next/none).
- Opções de escape em todas: `none` = "The document does not state this, or it is not this kind of date."; `out_of_range` para ano fora da lista.
- `role` (ex.: "the deadline to return the form") entra nas instructions. `state` = documento (string).
- Speculative fan-out: "Code reads only the pieces `mode` calls for."

**3. Consumo.** Confiança da data = **mínimo** das confianças das partes usadas; `REVIEW_BELOW = 0.60`; também vai para revisão se o código não conseguiu montar a data (data impossível, incompleta, ano fora da faixa). Ano ausente → ano corrente, +1 se a data já passou há mais de 31 dias.

**4. Resultados.** **6/6 corretos**: effective 2025-01-01 conf **0.97**; expires 2027-12-31 **0.91**; deadline 2026-08-14 **0.95**; kickoff call → none, **0.46**, review ("absolute date incomplete"); survey closes today **0.94**; design review next Thursday 2026-08-06 **0.92**. **5 auto-accept, 1 review.** Latência/custo: **não informados**.

**5. Boas práticas / armadilhas.**
- "`assemble` also reports the lowest confidence among the parts it used, so a weak answer on any one part can send the whole date to review."
- "If a list that long bothers you, pull the year-like numbers out of the text first and offer the model only those."
- A convenção de "next Thursday" é decisão de código ("'next Thursday' can mean two different days, so code decides which").
- `year = out_of_range` → "flag, don't guess".

**6. Código.**
```python
        "month": Choice(
            instructions=f"If {role} is an absolute calendar date, which month is it in?",
            criteria={m: None for m in MONTHS} | {"none": absent},
        ),
```
```python
    def result(resolved: date | None, note: str) -> dict:
        usable = [c for c in confs if c is not None]
        confidence = min(usable) if usable else None
        needs_review = (
            resolved is None or confidence is None or confidence < REVIEW_BELOW
        )
```

---

## 15. Pre-parsed value extraction (`pre_parsed_value_extraction_cookbook`)

**1. Problema e dataset.** Extrair valores verbatim (email, telefone, valor monetário): regex acha candidatos, TypeSafe escolhe, código copia e normaliza. **3 casos** didáticos (cabeçalho de email com 4 endereços; texto com 3 telefones; fatura com 4 valores). jev-1.12.

**2. Desenho das perguntas.**
- `pick`: **`Choice` cujas opções são os spans encontrados pelo regex** (criteria `None`) + escape `none` ("None of these is the requested value.") → a resposta é cópia exata de um span.
- `classify`: `Choice` sobre rótulos fixos (país: US/GB/DE/FR/CA/AU; moeda: USD/EUR/GBP/JPY/CAD).
- `is_true`: `Noul` ("Is the amount {x} a credit or refund to the customer, not a charge?").
- **1 pergunta por chamada** (cada helper faz a sua requisição). `state` = documento.
- Regex "tuned to over-find" (recall).

**3. Consumo.** `choice` copiado e normalizado em código (lowercase, `phonenumbers` → E.164, `Decimal`); `P(credit) > 0.5` → crédito. Threshold de confidence para revisão: **não definido**.

**4. Resultados.** receipt → `dana.personal@gmail.com` conf **0.98**; sender → `dana.whit@acme-corp.com` **1.00**; mobile `(415) 555-0177` **1.00**, país US **0.90** → `+14155550177`; total `$1,315.50` → 1315.50 USD, P(credit)=**0.01**; crédito `$50.00` P(credit)=**0.99**. Latência/custo: **não informados**.

**5. Boas práticas / armadilhas.**
- "Because TypeSafe only ever chooses among the spans the regex found, the value you get back is one of those spans, copied unchanged. It cannot invent a value or transpose a digit."
- Limite: "A `Choice` question allows at most 255 options. With more candidates than that, narrow in two stages: pick the section first, then the span inside it."
- "Finding the candidates is the part that takes work ... a name does not [have a regex], so its candidates have to come from a roster you already have, or from a named-entity recognizer or an LLM that proposes them."
- `to_decimal` assume vírgula de milhar; para `€1.315,50`: "Ask a `Noul` question which convention the document uses, and branch on it in code."

**6. Código.**
```python
    criteria = {c: None for c in candidates} | {
        NONE: "None of these is the requested value."
    }
    answer = ts.system_one(
        state=document,
        questions={"pick": Choice(instructions=question, criteria=criteria)},
        model=TYPESAFE_MODEL,
    ).answers["pick"]
    return {"choice": answer.choice, "confidence": answer.confidence}
```

---

## 16. Hierarchical classification (`hierarchical_classification`)

**1. Problema e dataset.** Percorrer uma hierarquia da raiz até a folha correta. 4 hierarquias, **1 documento rotulado cada**: CPC 2026.05 (patentes), Shopify 2026-02 (produtos), MeSH 2026 (biomédico; DAG expandido em caminhos de árvore pelos tree numbers), **CookSafe files** (árvore de arquivos-fonte do repositório de cookbooks, snapshot 2026-08-06; documento: "find the experimental Python module under x/eugene that implements BM25, dense, and fused retrievers for legal RAG", folha esperada `retrievers.py`). Contagem de nós: **não informada no texto** (só renderizada nos SVGs). jev-1.12.

**2. Desenho das perguntas.**
- Cada nó = **um `Choice`** "Which direct child category best matches this document?", com chaves opacas `c0..cN` e o rótulo do filho como descrição (`criteria=keys`); mapeamento reversível chave→rótulo.
- Nó com filho único não chama a API (prob 1.0, não conta como decisão).
- `state` = documento. Uma pergunta por requisição; os `K` caminhos da fronteira são expandidos **em paralelo** (`ThreadPoolExecutor(max_workers=BEAM_WIDTH)`). O texto diz "Each path of the beam runs as parallel questions, so extra exploration adds little wall-clock latency".
- `RetryPolicy(max_retries=5, backoff_initial=1.0, backoff_max=20.0)`.

**3. Consumo.**
- **Beam search** `BEAM_WIDTH = 3`, `MAX_DEPTH = 12`, `EPSILON = 1e-9`.
- `path_score = product(edge_probabilities) ** (1 / decisions)` (média geométrica, normalizada por comprimento para comparar folhas rasas e profundas); poda para os K melhores.
- `separation = top_path_score / second_path_score` (métrica de ambiguidade, não usada na poda; ≈1× ambíguo).
- Greedy: sempre o filho mais provável.

**4. Resultados.** **Beam 4/4, greedy 2/4.** CPC: greedy → `E99Z99/00 Subject matter not otherwise provided for` (errado), beam → `A01K31/12 Perches for poultry or birds` (certo). Shopify: greedy → `Pet Chairs`, beam → `Cat Window Beds & Perches`. MeSH (Crohn Disease) e CookSafe (`retrievers.py`): ambos certos. Valores de mean p / separation: **não informados no texto**. Latência/custo: **não informados**.

**5. Boas práticas / armadilhas.**
- Greedy: "One early mistake cannot be recovered." Beam: "Deeper evidence can repair an ambiguous early decision."
- "use `exp(mean(log(probs)))` instead of `product(edge_probabilities) ** (1 / decisions)` to avoid precision errors for hierarchies that are very deep (eg >10 layers)".
- Métrica alternativa: "`min(top_prob/second_top_prob)` which would optimize for paths that have very clear decisions at every node."
- Benefícios da decomposição: observabilidade (em que nós os erros ocorrem; contagem de travessias) e testabilidade (medir impacto de mudanças na hierarquia).
- Snapshot congelado do repositório: uma varredura viva "makes the taxonomy -- and every number derived from it -- depend on the reader's checkout"; "sibling options are asked in the order they appear here, so it is part of the question, not presentation."
- A intro lista "codebases ... LLM skills, moderation policies" como hierarquias-alvo.

**6. Código.**
```python
def child_question(labels: tuple[str, ...]) -> tuple[Choice, dict[str, str]]:
    """Build the direct-child Choice and its reversible option mapping."""
    keys = {f"c{i}": label for i, label in enumerate(labels)}
    question = Choice(
        instructions="Which direct child category best matches this document?",
        criteria=keys,
    )
    return question, keys
```
```python
        "score": probability_product ** (1 / decision_count) if decision_count else 1.0,
```

---

## 17. Autoresearch feature discovery (`autoresearch_feature_discovery`)

**1. Problema e dataset.** Transformar texto livre em features numéricas para um regressor supervisionado (CatBoost), com as perguntas propostas por um LLM num loop de autoresearch. Dataset winemag (`GroNLP/ik-nlp-22_winemag`, revisão fixada): **2,000 reviews** (1,200 dev + 800 held-out), nota do crítico **80–98, média 88.73, sd 3.17**; notas de ~245 caracteres. Proposer `claude-sonnet-5` (structured output com JSON schema, effort medium); TypeSafe jev-1.12; 2026-08-03.

**2. Desenho das perguntas.**
- Proposer devolve até **18 ações** por rodada (`add` / `revise` / `drop`; `kind` = `intensity` ou `presence`).
- `intensity` → **`Score`** com rubrica fixa de **5 níveis** (`INTENSITY_LEVELS`: "Not present in this note at all" ... "Dominant - the note is largely about this"); `presence` → **`Noul`** com `PRESENCE_CRITERIA` (true="The note states this or clearly implies it").
- **1 requisição por nota por rodada, com todas as perguntas da rodada**; pool de 8. `state` = a nota.
- Baseline "pergunte a nota direto": 1 `Score` de **10 níveis** de qualidade.

**3. Consumo.**
- Encoding `mean_spread`: Score → 2 colunas (nível esperado `Σ k·p_k` e desvio-padrão); Noul → 1 coluna (P(true)). Final: 29 score × 2 + 9 noul = **67 colunas**.
- `add` entra direto, exceto se a coluna é plana (std dev < `MIN_SPREAD = 0.05`); `revise`/`drop` só se o RMSE de CV em dev melhora (`CHANGE_TOLERANCE = 0.0`) — refit sem chamadas de API.
- CatBoost (400 iter, depth 4, lr 0.05), 5-fold × 3 repetições. Feedback ao proposer: 30 piores + 30 melhores notas de dev (`EXAMPLES = 60`), importância % e spread por feature.
- Score direto: `80 + 20·E/9` + shift único medido em dev (**−1.71**).

**4. Resultados (800 held-out).**

| braço | RMSE | Spearman |
|---|---|---|
| prever a média de dev | 3.088 | −0.014 |
| nota como word counts (mesmo CatBoost) | 2.466 | 0.605 |
| pedir a nota ao TypeSafe (Score 10 níveis, shift −1.71) | 2.145 | 0.761 |
| 18 perguntas da rodada 1, sem loop | 1.869 | 0.778 |
| **38 perguntas após 5 rodadas** | **1.772** | **0.799** |

- Rodada 1→5 no held-out: **−0.097 pontos, 95% CI [−0.147, −0.050]** (bootstrap pareado, 2,000 reamostragens).
- RMSE de CV em dev por rodada: 1.903, 1.881, 1.861, 1.838, 1.840 (rodada 5 foi a primeira sem melhora).
- 38 perguntas finais: 29 score + 9 noul; maior importância `note_overall_tone_positivity` **17.4%**; 4º lugar é um noul (`single_vineyard_or_prestige_signal` 7.2%).
- Custo/latência: **não informados**. Volume: "a round answers questions for all 2,000 rows: 2,000 requests".

**5. Boas práticas / armadilhas.**
- **Limite duro**: "ten levels is the most a `Score` question takes - eleven comes back as a server error."
- "No question is filtered out before it is answered ... A question that applies to one row in ten will look useless in the 60 notes the proposer reads, and still be the most useful column in the set."
- "All of a round's questions go out in the same request, so one more question costs no extra request."
- "The request count grows with rows, not with questions ... Raise the worker pool slowly. Eight is already enough to hit a rate limit on a shared key."
- "Most of the gain is in that first call".
- Score direto não basta: "nothing in the question says where this publication's scores actually sit on it" → precisou de offset.
- Próximos passos sugeridos (não executados): triar pergunta candidata com nouls sobre a própria pergunta (respondível pelo texto? um só sentido? aplica-se à maioria das linhas? varia?), podar features correlacionadas, misturar famílias de proposers, parar em platô, checar estabilidade entre seeds.

**6. Código.**
```python
def encode(feature: dict, probabilities: np.ndarray, mode: str) -> list[tuple]:
    """Turn one question's probabilities into named columns."""
    name = feature["name"]
    if feature["kind"] == "presence":
        return [(name, probabilities[:, 0])]  # one number is all there is
    levels = np.arange(probabilities.shape[1])
    mean = probabilities @ levels
    if mode == "mean":
        return [(name, mean)]
    if mode == "mean_spread":
        variance = probabilities @ (levels**2) - mean**2
        return [(name, mean), (f"{name}_sd", np.sqrt(np.clip(variance, 0, None)))]
    return [(f"{name}_p{i}", probabilities[:, i]) for i in levels]
```

---

## 18. Classification using confidence (`classification_using_confidence`)

**1. Problema e dataset.** Classificar 10-Ks da SEC (Item 1 "Business") em grupos da SIC e usar a `confidence` para recuar ao nível mais geral quando incerto. **60 filings** (1993–2024, 700–2,200 palavras, média 1,438), filtrados para que o texto suporte o código autodeclarado. Taxonomia: **444 códigos → 75 major groups → 10 divisões**. jev-1.12, 2026-08-12.

**2. Desenho das perguntas.**
- **1 `Choice` com 75 opções** por documento. Instructions: "Which broad industry does this company operate in? Judge the company's own operations as this filing describes them."
- Criteria de cada grupo = título guarda-chuva (se existir; 42 dos 75 têm) + até **8** indústrias internas (`MAX_NAMED = 8`, "enough to characterise it without a wall of text").
- `state` = texto do filing (string). 1 pergunta, 1 requisição.

**3. Consumo.** `CONFIDENT = 0.9` sobre **`confidence`** (não a probabilidade do vencedor): ≥0.9 → reporta o grupo; <0.9 → reporta a **divisão** do mesmo grupo (derivada em código, sem 2ª chamada). "If a division is too coarse for your application to act on, this branch is where you hand it to a person."

**4. Resultados.**
- Forçar sempre o grupo: **39/60** certos. Seguros (30): **27/30 (90%)**; inseguros (30): **12/30 (40%)** → reportados como divisão: **70%**. Total útil: **48/60**.
- Confianças: topo 1.00 (química, seguro de vida, utility); fundo **0.22, 0.23, 0.29** (duas empresas em estágio de desenvolvimento e uma que vendeu um segmento semanas antes).
- Latência/custo: **não informados**.

**5. Boas práticas / armadilhas.**
- "a Choice works reliably up to roughly 240 options, and 75 is well inside that."
- "A winner at 0.45 with a runner-up at 0.44, and a winner at 0.45 with the rest of the weight scattered thinly, are different situations, and `confidence` is what separates them."
- Rótulo autodeclarado envelhece: "it goes stale when a company sells the business the code names and keeps the code" — por isso o filtro, para que "the numbers here measure the recipe rather than the state of EDGAR's metadata."
- "Point `ask()` at your own documents and rewrite `describe()` for your own taxonomy, and the rest carries over."

**6. Código.**
```python
def classify(filing: dict) -> dict:
    answer = ask(filing["id"], filing["text"])
    sure = answer["confidence"] >= CONFIDENT
    return {
        "level": "group" if sure else "division",
        "label": answer["group"] if sure else division(answer["group"]),
        "confidence": answer["confidence"],
        "group": answer["group"],
    }
```

---

## SÍNTESE TRANSVERSAL

### (a) Padrões recorrentes de desenho de perguntas

1. **Evidência no modelo, decisão no código.** Nenhum cookbook pergunta "devo fazer X?". RAG: "None of the four asks whether to include the passage"; guardrails: "TypeSafe supplies the assessment; your application owns the decision". Mudar a política = editar uma constante, não reescrever uma pergunta.
2. **Decomposição em fatos estreitos e checáveis.** SDE ("The TypeSafe Way: Decomposition"), autoformat ("name the narrowest fact that decides it"), guardrails ("'Out of bounds' is not one question"). Cada pergunta mede uma coisa; o código agrega.
3. **O `state` é o objeto julgado, com frequência um par em JSON**: `{query, passage}`, `{entity_a, entity_b}`, `{claim, section}`, `{query_excerpt, candidate_passage}`, `{request, recent_context}`, `{system_message, instruction, source_text, schema, extraction}`. As perguntas ficam fixas e só o state muda (RAG) — ou o inverso: o state fixo e a query nas `instructions` (semantic_find).
4. **IDs como opções de Choice.** Linhas/blocos com prefixo `L052|`/`B003|` no state e opções = IDs com `criteria=None` (semantic_find, autoformat); chaves opacas `c0..cN` → rótulo (hierarchical); opções = spans verbatim do regex (pre-parsed) → não inventa valor.
5. **Válvulas de escape.** Opção `none`/`out_of_range` (date, pre-parsed); `Noul` de existência junto do `Choice`, porque as probabilidades do Choice somam 1 (semantic_find `exists`, skill `fits`, gate de 3 nouls); nível do meio de Score = "mande ao curador" (entity).
6. **Speculative fan-out.** Pergunte tudo o que pode ser necessário na mesma requisição e leia só o relevante: companheiras `hlevel/step/callout` (autoformat, 62 perguntas); partes da data lidas conforme `mode`; 54 perguntas cobrindo os argumentos de todas as funções (function calling); baterias de guardrails/SDE; todas as perguntas da rodada (autoresearch). Justificativa: o state domina os tokens e vai uma vez só; uma ida e volta extra custa latência cheia.
7. **Score como rubrica ordenada.** Níveis escritos = desfechos (entity: 3), escala de severidade (guardrails: 0–3), intensidade de 5 níveis, qualidade de 10 níveis. Consumo por nível mais próximo, valor esperado `Σk·p` ou média + spread.
8. **Framing dos Nouls.** "bad = TRUE" com critérios explícitos (SDE); "a yes means the thing we are checking for is true" (consistency); pergunta invertida corrigida no código (`prose_suffices`).
9. **Pré-filtro determinístico antes do modelo.** String match (citation), regex (pre-parsed), BM25/embeddings (rerank, RAG), pontuação (autoformat), aritmética (abv, calendário), marcadores explícitos (autoformat).
10. **Progressive disclosure em dois estágios.** Ranking largo barato → leitura atenta de 2–3 (skill); janela → linhas quando > 255 opções (semantic_find); seção → span (pre-parsed); beam por nível (hierarchical).
11. **Wording pelo significado.** "about the idea rather than the words"; não batizar a pergunta com o nome do parâmetro; nomear os papéis ("the one being measured, named first" vs "the yardstick"); perguntar por **ação** e não por assunto (skill); "mid-sentence" em vez de "same paragraph".

### (b) `confidence` vs probabilidade

- `confidence` existe só em **Choice e Score** ("Noul answers don't carry one", página Confidence); é derivada do formato da distribuição (concentrada = alta).
- **Use `confidence`** quando interessa se a massa se concentrou num único vencedor: `CONFIDENT = 0.9` para recuar ao nível pai (classification: "A winner at 0.45 with a runner-up at 0.44 ... are different situations"); `AUTO_ACCEPT = 0.8` (citation); `REVIEW_BELOW = 0.60` sobre o **mínimo** das partes (date); mínimo sobre os argumentos (function calling).
- **Use a probabilidade do top label** quando a política é agir no rótulo ou se abster: `MIN_CHOICE_PROBABILITY = 0.60` — "This uses the returned probabilities, not the API's separate `confidence` field" (consistency_choice). O autoformat chama de "type confidence (the probability behind the winning choice)" o limiar de UI de 0.55 — a terminologia da própria fonte é frouxa ali.
- **Noul = P(true) absoluta**, independente das outras opções. Padrões de limiar: banda de 3 saídas 0.30/0.70 (consistency_noul); review 0.35 / action 0.70 (strict) ou 0.85 (permissive) (guardrails); cascata em que o primeiro teste que casa vence, 0.70/0.70/0.45/0.55 (RAG); `any > 0.7` (SDE); média de nouls orientados < 0.30 (skill gate); `max(fits) < 0.30` (skill); existência 0.7/0.35 (semantic_find); limiar dependente de contexto 0.2/0.5 (autoformat).
- **Agregação**: mínimo ("weakest link", não produto) para uma chamada composta; `max` para flags de verificador ("averaged into silence"); média geométrica ao longo de um caminho hierárquico (log-space se > 10 níveis); nível esperado + spread para features.
- Choice é **relativo** (a soma dá 1 → sempre há um primeiro); ranking pela probabilidade do Choice é válido (semantic_find, skill), mas existência exige Noul.
- Todos os limiares são declarados "illustrative"/"starting point": calibrar com exemplos rotulados e o custo do erro; "start high" e baixar; o limiar sobe com o risco da ação.

### (c) Limites práticos

- **Opções de Choice**: "accepts up to 255 options" (semantic_find, pre-parsed); "works reliably up to roughly 240 options" (classification). Usados: 218 linhas, 182 skills, 153 anos, 75 grupos. Acima disso: dois passes ou chunks.
- **Níveis de Score**: no máximo **10**; "eleven comes back as a server error".
- **Perguntas por requisição**: até 62 (autoformat) e 54 (function calling) demonstradas; limite máximo **não informado**.
- **Tamanho do state**: até ~54k caracteres (GDPR); ToS de 43,980; roster de 16,089 caracteres dentro dos criteria; filings de ~1,438 palavras. Máximo **não informado**. Rerank: média derivada de ~1,280 tokens de input e ~21 de output por chamada (1,536,002/1,200 e 25,200/1,200).
- **Latência**: ~0.09–0.51 s por chamada (111ms, 114ms, 0.16–0.31s, 0.27s, 0.32s, 0.51s).
- **Preço**: $0.042/1M de input, output grátis (jev-1.12, histórico).
- **Concorrência**: "the public endpoint rate-limits above roughly eight"; "Eight is already enough to hit a rate limit on a shared key"; pools de 4/6/8/12. `RetryPolicy(max_retries=5, ...)`.
- **Batching**: perguntas sobre o mesmo state vão juntas (12.2x mais barato, 10.0x mais rápido, respostas iguais); **itens diferentes não se agrupam** ("each question is about one pair"); o custo cresce com o número de linhas, não com o de perguntas.
- **Aliases**: `jev-latest` resolveu para `jev-1.13.0`; gravar `response.model`.

### (d) Anti-padrões explícitos

Perguntar a decisão em vez da evidência · juiz holístico vago (`__overall__::judge` 0.56 não dispararia; "mushy, uncalibrated") · média de flags · produto de confianças · ranking de Choice sem teste de existência · wording por tópico ("same paragraph" → 12 blocos em vez de 17; perguntar pelo assunto não separa pedidos de skill) · pergunta com o nome do parâmetro · forçar um Choice a nomear um valor não declarado (usar `stated`/`none`) · deixar o modelo gerar ou transcrever valores, ou fazer contas · um limiar único para contextos diferentes · tratar o score de injection como fronteira de segurança · sugestão de skill impositiva, ou silêncio quando nada serve · filtrar perguntas pela amostra do proposer · confundir repetibilidade com acurácia (t=0 não garante estabilidade) · cache sem fingerprint da rubrica · greedy em hierarquias · achar que JSON válido pelo schema é extração correta · match exato para citações parafraseadas.

### (e) Cookbooks mais análogos por caso de uso

| Caso | Mais análogos |
|---|---|
| Roteamento de tickets → workflows | **function_calling** (`__tool__` Choice + argumentos + `stated`, confiança pelo mínimo); **skill_suggestion** (gate + ranking largo + rerank + fits); **classification_using_confidence** (recuo ao nível pai); **consistency_choice** (fila/ação com `uncertain`); página Speculative fan-out (exemplo de triagem de ticket) |
| Re-ranking de busca/memória | **rerank_typesafe** (Noul por par, ordenar); **classifying_rag_passages** (4 nouls + rota); **semantic_find** (Choice sobre IDs + Noul `exists`); **skill_suggestion** passo 2 |
| Sugestão de skill para agente | **skill_suggestion** (direto); **hierarchical_classification** (cita "LLM skills" como hierarquia) |
| Classificação hierárquica de código-fonte | **hierarchical_classification** (árvore CookSafe files, beam K=3, `retrievers.py`); **classification_using_confidence** (recuo); **autoformat** (tipagem de blocos com companheiras) |
| Verificação de citações/claims | **citation_check**; **sde_cascade** (`hallucinated`/`off_target` por campo); **classifying_rag_passages** (`contradicts_query_premise`) |
| Guardrails | **llm_guardrails**; **classifying_rag_passages** (injection); **consistency_choice** (moderação) |
| Cascata barato → caro com LLM | **sde_cascade** (mini → verificação → reasoning); **citation_check** (string → modelo → humano); **rerank_typesafe** e **classifying_rag_passages** (retrieval → TypeSafe → gerador); **skill_suggestion** (TypeSafe na frente do agente) |
