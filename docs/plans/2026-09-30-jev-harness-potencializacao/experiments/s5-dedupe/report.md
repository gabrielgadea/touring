---
type: ExperimentReport
title: S-5.5 — dedupe: o eco do scout é determinístico, e o Noul confirma 10/10
description: O plano pedia Noul por par. A medição mostrou que o defeito que a subtask ataca (37,5% do banco com o mesmo texto) é aritmética, não julgamento. Dois estágios: determinístico onde há eco, Noul onde há quase-duplicata — e ele confirmou 10/10. Zero deleções.
plan_id: 2026-09-30-jev-harness-potencializacao
tags: [experiment, s-5.5, w5, dedupe, scout-eco, noul]
subtask: S-5.5
owner: touring-02
timestamp: 2026-10-01T13:12:00-03:00
okf_version: "0.1"
---

# S-5.5 — P5 dedupe (sinal, nunca verdade)

Subtask `task_1790819065619924538::S-5.5`, dona `touring-02`.
**`deletions_performed: 0` · `writes_performed: 0`.** O banco abriu em `mode=ro` e
o teste `test_the_database_is_opened_read_only` prova que o caminho de URI
realmente recusa um `DELETE`.

- **Script**: `scripts/jev_dedupe.py` (**0,977 Diamond**) · **Testes**: `scripts/test_jev_dedupe.py` (**0,973 Diamond**, 9/9)
- **Dados**: `experiments/s5-dedupe/dedupe.json`

## O resultado

| | |
|---|---|
| entradas varridas | **20.044** |
| grupos de valor normalizado idêntico | **73** |
| entradas em grupos | **7.714** |
| **excesso total** | **7.641** |
| grupos com chave redundante (só timestamp) | **4** — **7.512** de excesso |
| pares quase-duplicados (≥0,85) | **10** (cap 400) |
| **Noul: pares confirmados como mesmo fato** | **10 de 10** |
| Latência (10 Noul) | ~370 ms cada · ~US$ 0,0002 o lote |

## O que a medição mudou: o desenho

O plano diz "**Noul por par candidato**". Medi antes de codar, e o número
obrigou a mudar a implementação:

**[F] Chaves duplicadas: 0. Valores idênticos: 43 grupos, 7.508 entradas.**
O maior deles tem **7.506 entradas com o texto `"Insufficient compliance data for
CILA progression analysis"`**, cujas chaves diferem **apenas pelo timestamp**
(`insight:cila_progression:1787517420`, `:1787517720`, …, criadas de 5 em 5
minutos).

**[I] (0,95) Isto é o eco do scout perpétuo** — literalmente o defeito que a
subtask nomeia ("scout perpétuo sem eco próprio") — e **um Noul é a ferramenta
errada**: 7.506 entradas idênticas são 7.505 decisões óbvias, e gastá-las a
~US$ 0,0000167 cada para o modelo responder "são iguais" sobre um `GROUP BY value`
que resolve em SQL determinístico e grátis. **[I] (0,90)** Pior: a 7.506ª chamada
levaria ~46 minutos, porque o custo é linear e o ganho é zero.

**Por isso existem dois estágios, e o `signal_kind` de cada par diz de qual veio:**

| estágio | o quê | custo | ferramenta |
|---|---|---|---|
| **1 — determinístico** | valores normalizados idênticos; marca a **chave redundante** | grátis | `GROUP BY` |
| **2 — Noul** | pares **não** idênticos acima de 0,85 | ~US$ 0,0002 | `system_one(noul)` |

**[I] (0,90)** O estágio 2 é insubstituível justamente onde o 1 cala: os 10 pares
`artifact:*` e `cah-conformance:*` são os que a similarity pega e o agrupamento
por valor não pega. Um relatório que contasse só "pares" mentiria sobre a origem —
e a origem é o que decide se aquilo é fato ou palpite.

## O Noul: 10/10, e o que ele disse

| similaridade | veredito | par |
|---|---|---|
| 0,963 | `none` | `artifact:67` ~ `artifact:68` |
| 0,940 | `none` | `artifact:66` ~ `artifact:67` |
| 0,937 | `none` | `cah-conformance:21:11:46` ~ `cah-conformance:22:00:19` |
| 0,937 | `none` | `cah-conformance:22:00:19` ~ `cah-conformance:22:01:08` |
| 0,917 | `none` | `artifact:66` ~ `artifact:69` |
| 0,913 | `none` | `cah-conformance:21:11:46` ~ `cah-conformance:22:01:08` |
| 0,912 | `none` | `artifact:68` ~ `artifact:69` |
| 0,896 | `none` | `artifact:67` ~ `artifact:69` |
| 0,871 | `none` | `artifact:48` ~ `artifact:61` |

**[I] (0,85) A convergência é o resultado.** Duas fontes independentes — o
`SequenceMatcher` determinístico (similaridade textual) e o Noul (semântica) —
chegam ao mesmo veredito em 10 pares. **[I] (0,80)** Isso **concorda** com o
G2, que mediu recall com ECE 0,709 e proibiu τ ali: o Noul é confiável como
**ranking**, e é exatamente isso que o dedupe é — sinal para auditoria humana, não
autonomia para apagar.

## ⚠️ Retração: "zero quase-duplicatas" estava errado

**[F]** A primeira mediçãoafirmou **0 pares ≥0,85** e eu escrevi isso no
docstring do `near_pairs`. **Era falso:** eu tinha medido só as **120 primeiras**
entradas do banco (7.140 comparações). O número **cresce com o cap**:

| cap | pares |
|---|---|
| 120 | 0 |
| 200 | **1** |
| 400 | **10** |

**[I] (0,95)** Um zero medido sobre um subconjunto que não é o universo é o pior
tipo de zero: parece ausência e é limite de amostragem. É a mesma classe do
`recalls=0` dentro do sandbox e do `g1_no_reference=95` que eu confundi com
`confidence`. **[I] (0,90)** O `cap` por isso é um campo do relatório
(`cap_entries_compared`), e o teste afirma **os dois** números — porque "10 pares
em 400 entradas" e "10 pares em 20.044" não são a mesma afirmação.

## Os testes (9/9)

| Teste | O que fixa |
|---|---|
| `test_dedupe_is_advisory_signal` | **[F] o nomeado do plano** — `writes_performed == 0` **e** `deletions_performed == 0` |
| `the_redundant_key_marks_the_scout_echo` | chave só-timestamp = eco; chave de arquivos diferentes = agrupamento legítimo |
| `test_normalization_is_whitespace_and_case_only` | normalizar pontuação criaria um "quase igual" determinístico — que é o trabalho do estágio 2 |
| `test_a_single_entry_is_never_a_group` | grupo de tamanho 1 não é duplicata |
| `test_identical_pairs_belong_to_stage_one_not_two` | o estágio 2 não repete o que o 1 resolveu |
| `test_short_texts_never_reach_similarity` | duas frases de 20 chars têm similaridade 0 ou 1, e nenhum dos dois significa algo |
| `the_real_database_shows_the_echo_and_the_near_pairs` | **os dois** números do banco real — incluindo a correção do zero |
| `test_the_database_is_opened_read_only` | `mode=ro` recusa `DELETE` de verdade |

**[F] Um defeito do SDK que o teste pegou:** passei `criteria: ["same fact, …"]`
para o `Noul` e o pydantic recusou — *"Input should be a valid dictionary"*. O
tipo é `NoulCriteria`, "descrições para **qualquer um dos** os dois desfechos".
A forma aceita é a de `scripts/pre_review_triage.py:51`: **Noul sem `criteria`**,
tudo no `instructions`. **[I] (0,85)** O docstring do cookbook de primitives não
dizia isso; só o `ValidationError` disse. É a quarta vez hoje que a **fonte**
responde o que a documentação nãoRespondia.

## O que a S-5.5 **não** entrega

1. **Nada é apagado, por desenho.** O plano diz "sinal, não verdade", e o que
   falta para transformar sinal em ação é uma política de remoção — que é
   decisão do Gabriel, não desta subtask. **[I] (0,90)** A primeira leva de ação
   recomendada é o grupo de **7.506 entradas idênticas**, que é o único onde a
   remoção é mecanicamente segura (mesmo texto, mesma chave-base) e o único que
   libera 37,5% do banco.
2. **O `cap` limita o universo.** Com `difflib` em CPU pura, o custo é O(n²): 400
   entradas são ~80.000 comparações (~25 s). 20.044 entradas serían ~200 milhões
   de comparações — horas. **[I] (0,85)** O relatório declara `cap_entries_compared`
   justamente por isso; um relatório de dedupe sem dizer o que foi varrido é um
   número sem dono.
3. **Só memórias.** O plano menciona "scout/memórias"; o sinal para o **scout**
   (tickets repetidos no decompose) é a S-5.4/loop, não esta.

## Cross-references

- `scripts/jev_dedupe.py` · `scripts/test_jev_dedupe.py` · `experiments/s5-dedupe/dedupe.json`
- `plan.md` S-5.5 (gate) · o "quando NÃO usar" de S-5.1 (gate de silêncio mensurável)
- `experiments/g2-baselines/report.md` — o ECE 0,709 do recall, que é a razão de o Noul ser ranking aqui
- `crates/touring-cli/src/cli/kpi/decision.rs` — `memory_entries` e o KPI que lê o resultado
