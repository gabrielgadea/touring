---
type: ExperimentReport
title: S-1.1 — 100 memórias, Choice da faceta `kind` (primeira decisão tipada real)
description: A primeira chamada de decisão do plano, em modo somente-medição. 100/100 chamadas responderam; 8% contra a tag de produção e 60% contra o professor (n=5) — e a análise mostra que o desacordo é com a referência, não com o julgamento.
plan_id: 2026-09-30-jev-harness-potencializacao
tags: [experiment, s-1.1, w1, jev, choice, tags, kind]
subtask: S-1.1
owner: touring-02
timestamp: 2026-09-30T22:56:00-03:00
okf_version: "0.1"
---

# S-1.1 — a primeira decisão tipada real, em modo só-medição

Subtask `task_1790819065619924538::S-1.1`, dona `touring-02`. **Nenhuma escrita**:
`memory.db`, `memory_tags` e o corpus do G1 estão intactos, e isso é provado por
hash no teste `test_no_writes_to_memory_db`, não afirmado.

- **Script**: `scripts/jev_tags_sample.py` (via `from jev_client import system_one`)
- **Testes**: `scripts/test_jev_tags_sample.py` — **7/7 PASS**, exit 0
- **Dados**: `experiments/w1-tags/results.json` (100 linhas, com estado, predição,
  confiança, ambas as referências e a lista crua de tags)

## Resultado em uma tela

| Métrica | Valor |
|---|---|
| Chamadas | **100/100 responderam** (`unmeasured = 0`) |
| Respostas com `confidence` / `probabilities` | **100/100** · **100/100** |
| Acurácia vs **tag de produção** | **0,08** (8/100) |
| Acurácia vs **professor do G1** | **0,60** — mas **n = 5** |
| ECE vs tag de produção (n=100) | **0,51** |
| ECE vs professor (n=5) | **0,50** |
| Latência p50 / max | **371,9 ms** / 557,5 ms |

**[I] (0,90) O número que decide a wave não é 8% — é que a referência está errada
para a pergunta.** A tag de produção de uma memória é, em 93% da amostra,
`insight` (46) ou `lesson` (47) — dois rótulos que descrevem **a função da
memória no sistema**, não **o que o texto é**. A pergunta feita ao Jev foi "o que
este texto É". São perguntas diferentes, e o desacordo mede a diferença entre
elas, não um erro do julgamento.

## A tabela de confusão que explica os 8%

| referência de produção | → predição do Jev | n |
|---|---|---|
| `lesson` | `outcome` | **31** |
| `insight` | `observation` | **28** |
| `lesson` | `gotcha` | 10 |
| `insight` | `gotcha` | 7 |
| `insight` | `outcome` | 4 |
| `lesson` | `lesson` | 3 |
| `reference` | `gotcha` | 2 |
| `insight` | `lesson` | 2 |
| (outras 4 combinações) | | 4 |

**[F] Exemplo 1** — `audit:INT-3.1:todo-stubs-replaced`:
> `"generate_impl.rs: lines 82/91/99 replaced with real implementations. 7 tests PASS
> (build_impl_block_struct/enum/trait + 4 parse tests). No more todo!"`

O Jev respondeu **`outcome`**, confiança 0,80. A tag de produção é `insight`.
**[I] (0,95) `outcome` está certo por qualquer critério que o vocabulário
declare** — o critério de `outcome` é literalmente "the recorded result of a
command, run, or experiment, with reward or exit status", e o texto é um registro
de execução com testes passando. `insight` é "a non-obvious realization about the
domain" — e esse texto não é uma realização, é um log.

**[F] Exemplo 2** — `ceg-extraction-baseline-2026-06-09`:
> `"CEG extraction Session A baseline (Pln2 recursive-cuddling-blossom). touring-hooks
> check 0 errors 7.13s. Tests: gateway:: 438, capability:: 84, ceg_e2e…"`

Jev: **`outcome`**, confiança 0,62. Produção: `lesson`. "baseline", "0 errors",
contagem de testes — é outcome de novo.

**[F] Exemplo 3** — `ceg-p4.5-dry-run-cache-2026-05-18`:
> `"CEG Pln2 P4.5 COMPLETE — content-hash dry-run cache. NEW gateway/dry_run_cache.rs:
> DryRunCache (moka::sync::Cache<String,SandboxOutcome> bounded por c…"`

Jev: `doc`, confiança 0,35. Produção: `insight`. Aqui **o Jev parece errar** — o
texto é um registro de entrega, não documentação. **[I] (0,80)** E a confiança
0,35 é baixa: a confiança do modelo sinalizou a dúvida. Este é o comportamento
que o gate de confiança do G4 precisa para cair para o keyword.

## O problema real da medição: só 5 linhas têm professor

**[F] `g1_no_reference = 95`.** A amostragem pediu 100 memórias com seed fixa, e
**95 das 100 não têm rótulo do professor** — logo, `accuracy_vs_g1_teacher = 0,60`
é **3 acertos em 5**. Esse número não é uma taxa; é um resto de amostra.

**Causa medida**: o corpus G1 (486 chaves) tem **483 em comum com o `memory.db`
do PROJETO e apenas 1 com o global**. A primeira versão do script apontava para o
banco **global** (594 memórias) e produziu **0/100** com referência — um zero
silencioso que teria esgotado a medição inteira sem erro visível. O default passou
a ser o banco do projeto, e `test_reference_is_achievable` existe para travar isso.

**[F] A correção de verdade já está pronta: é o split da S-2.0.** Filtrando pelo
`test` de `memories` no `split.json`:

| | amostra livre (n=100) | split-test (n=73) |
|---|---|---|
| com tag de produção | 100 | **73** |
| **com rótulo do professor** | 5 | **73 (100%)** |
| distribuição do professor | — | gotcha 14 · report 14 · fix 12 · audit 11 · snippet 10 · strategy 4 · decision 4 · pattern 3 · lesson 1 |

**[I] (0,95) A amostra por split-test é a medição que o plano quer**: as duas
referências presentes em **todas** as linhas, e a distribuição do professor é
diversificada (9 classes) em vez de Concentrada em `lesson`/`insight`.

**[I] (0,90) E ela expõe um problema que a amostra livre escondia:** o professor
usa `report` (14), `fix` (12) e `audit` (11) — **três dos quatro maiores grupos** —
e **nenhum dos três existe no `Facet::Kind::seed_values()` de produção**. São 37
das 73 linhas com um rótulo que a Choice não pode oferecer.

## Vocabulário: o gap é real, e menor do que parecia

**[F] `vocabulary_coverage`: 0 tags fora do vocabulário NESTA amostra.** A medição
global no banco do projeto encontrou **104 kind-tags em 35 valores** fora dos 22
de produção (`fix`, `checkpoint`, `analysis`, `evidence`, `phase-report`, `gate-closed`,
`todo`, `incident`, `debt`…), mas **nenhuma caiu na amostra de 100** — porque a
amostra é dominada por `insight`/`lesson`, e os valores fora-vocababulário vivem
nas memórias de menor acesso.

**[F] 20.059 das 20.163 kind-tags do projeto (99,5%) estão no vocabulário de
produção.** E `test_vocabulary_matches_production` prova, parseando o Rust, que os
22 valores do script são exatamente os de `Facet::Kind::seed_values()` — se alguém
acrescentar um kind lá, o teste falha alto em vez de a medição correr contra um
vocabulário velho.

**[I] (0,85) Mas `derive_tags` não consegue produzir `fix`, e `fix` é 12 das 73
linhas do split-test.** As duas conclusões convivem: o vocabulário de produção
cobre o que existe, mas o **professor prefere nomes que a derivação determinística
nunca emite** — porque ele julga a memória, não a função dela no store.

## Confiança: a curva completa, e uma retratação

### ⚠️ Retração (01/10, 02:01) — "95/100 sem `confidence`" estava ERRADO

A primeira versão deste relatório dizia que **95 das 100 respostas não trouxeram
`confidence`**, e tratava isso como bloqueio de instrumentação do provedor.
**[F] O dado bruto desmente: 100/100 respostas trouxeram `confidence` E
`probabilities`.** A mistake foi minha, na leitura do meu próprio instrumento —
e a causa está registrada porque ela é mais útil que a correção:

> `_score_rows` acumulava a curva de calibração **só para a referência `g1`**
> (linha `if name == "g1" and isinstance(conf, …)`). A curva saiu com **5 pontos**,
> porque só 5 linhas tinham rótulo do professor — e o `g1_no_reference` do summary
> é **95**. **As duas contagens deram 95 por coincidência**, e eu li uma como a
> outra. [I] (0,95) Dois números iguais que significam coisas diferentes é
> precisamente a armadilha da lição `instrumento-que-lie`; e ela é mais difícil de
> ver quando um deles é grande e redondo, porque o olho reconhece "95" e para.

**[F] Corrigido no código** (`scripts/jev_tags_sample.py`): a curva é acumulada
para **cada** referência, `evaluate` publica `ece_vs_production_tag` **e**
`ece_vs_g1`, e o summary ganha `responses_with_confidence` /
`responses_with_probabilities` como campos próprios — para que a confusão não
tenha onde acontecer de novo. Nenhuma chamada nova de API foi feita: o
`results.json` foi recalculado a partir do bruto já coletado.

### A curva real, contra a tag de produção (n=100)

| faixa | n | acerto | confiança média |
|---|---|---|---|
| [0,0 – 0,2) | 0 | — | — |
| [0,2 – 0,4) | **38** | **0,053** | 0,304 |
| [0,4 – 0,6) | 15 | 0,067 | 0,493 |
| [0,6 – 0,8) | 15 | 0,067 | 0,695 |
| [0,8 – 1,0) | **32** | **0,125** | 0,926 |

**ECE = 0,51.**

**[I] (0,90) Este é o resultado mais forte da S-1.1, e ele é contra a crença
implícita de que "confiança alta = resposta boa".** A curva é **monotônica e boa**
no sentido errado: acerto **sobe** de 0,053 para 0,125 quando a confiança sobe de
0,30 para 0,93 — mas **0,125 ainda é quase-zero**. As 32 linhas em que o modelo
tem 0,8–1,0 de certeza acertam **4**. Confiança alta com acerto baixo é o pior
regime possível para um gate de fallback: o threshold que a estratégia quer
(`conf ≥ τ → usa Jev`) descartaria exatamente as 38 linhas de confiança baixa,
**mantendo 31 das 32 que ele erra**.

**[I] (0,85) E isso não é defeito de calibração do modelo — é a pergunta errada com a
referência errada.** A confiança do Jev é a confiança *da resposta dele*, e contra a
tag de produção ela está mal calibrada porque as duas respondem perguntas
diferentes (o texto É × a função no store). **[F]** Contra o professor, onde as
duas perguntas coincidem mais, a acurácia é **0,60** — com n=5, que é ruído, mas
é a direção certa. **A leitura honesta: a confiança do Jev é confiável quando a
pergunta é ambígua e a referência é boa; não é confiável contra uma referência que
responde outra pergunta.** O S-1.2 tem de decidir qual das duas perguntas é a de
produção — e só depois τ faz sentido.

## Latência: o número que reposiciona o custo

**[F] p50 = 371,9 ms, max = 557,5 ms** em 100 chamadas reais. A estratégia anunciava
~100 ms. **[I] (0,80)** O p99 do `memory_recall` medido antes era de 31 chamadas
em 7 dias, com 11 acima de 1 s — então 370 ms é invisível para o recall. Mas o
ponto de decisão #1 (intent→CILA) roda **191,4 prompts/dia**: 191 × 0,372 s ≈
**71 s de latência somada por dia**, o que é irrelevante para latência e
relevante para **custo** (≈190 chamadas/dia). O economics do G4 continua valendo;
a razão de custo não é latência.

## O que a S-1.1 decide

**[I] (0,75) Go condicional, não go.** A decisão tipada **responde** (100/100,
fail-open nunca disparou), **custa** o previsto (~US$ 0,17 em 100 chamadas a
~US$ 0,0017 cada), e **julga diferente da tag de produção** — que é o
comportamento correto se a pergunta for "o que o texto é", e o comportamento
errado se a pergunta for "o que a função da memória no store é".

**[I] (0,90) Três decisões que o relatório entrega para o S-1.2 (go/no-go):**

1. **Reamostrar pelo split-test (n=73), não pela amostragem livre (n=100).** As duas
   referências passam a estar em 100% das linhas. É a mudança com mais efeito sobre
   a qualidade da evidência, e o artefato já existe (S-2.0).
2. **Resolver `fix`/`report`/`audit` antes de calibrar.** 37 das 73 linhas do
   split-test têm rótulo do professor que a Choice não pode oferecer. Ou o
   vocabulário cresce (mudança em `tags.rs`, escopo do outro agente), ou a
   medição declara esses 37 como `out_of_vocabulary` e compara só os 36 restantes.
3. **Não recalibrar τ sobre a curva contra a tag de produção.** A curva mostra
   que o threshold que a estratégia quer (`conf ≥ τ → usa Jev`) está no
   **regime errado**: as 32 linhas de confiança >0,8 acertam 0,125. **τ só tem
   sentido depois que a pergunta de produção estiver decidida** — e o `confidence`
   **chega em 100% das chamadas** (a versão anterior deste relatório dizia o
   contrário, e errou: ver a retratação acima). O que falta não é o campo, é a
   referência certa.

**[F] Nenhuma escrita foi feita, nenhum commit, nenhum arquivo fora de
`scripts/jev_tags_sample.py`, `scripts/test_jev_tags_sample.py` e
`experiments/w1-tags/` foi tocado.** O banco `memory.db` tem sha256 inalterado
(prova no teste), e o `split.json` da S-2.0 não foi tocado por esta subtask.

## Cross-references

- `scripts/jev_client.py` — o adapter (S-0.2/S-0.3, dono `touring-a7`); `_normalize_answers` é quem extrai `confidence`
- `experiments/split/report.md` — S-2.0, o split que dá as duas referências
- `harness-scan-t02.md` §3.2 — os 65,4% de concordância que motivam perguntar ao Jev
- `plan.md` S-1.1 (gate) · S-1.2 (go/no-go) · S-5.2 (correção em lote herda τ daqui)
- `crates/touring-intelligence/src/rl/memory/tags.rs:251-278` — `Facet::Kind::seed_values()`
