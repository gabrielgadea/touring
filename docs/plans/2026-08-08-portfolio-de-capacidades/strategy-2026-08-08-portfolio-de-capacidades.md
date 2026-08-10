---
okf_version: "1.0"
type: Strategy
title: "Portfólio de Capacidades — descoberta de prior-art por propósito"
description: >
  Indexar o corpus de artefatos (scripts, funções, templates, ADWs, skills) por
  PROPÓSITO e não por nome, de modo que toda criação seja precedida por uma
  consulta ao prior-art — sem que o portfólio vire uma âncora que limita a LLM.
tags: [portfolio, prior-art, descoberta-por-intento, aco, context7, afordancia]
timestamp: 2026-08-08T12:10:00-03:00
plan_id: 2026-08-08-portfolio-de-capacidades
---

# Portfólio de Capacidades

## 1. O pedido (Gabriel, 08/08/2026)

> Antes de criar um script `.py` que gera um mapa, explorar completamente os
> scripts que já geram mapas e ter isso como um portfólio de scripts/funções/
> classes/símbolos disponíveis — **mas sem limitar a LLM a esse portfólio**;
> buscar também as melhores práticas no Context7. Idem para "gerar um PDF
> profissional" (estratégias em `.py` ou HTML). E que isso valha para os mais
> diferentes casos de uso do Touring e da LLM.

## 2. Diagnóstico — o que foi MEDIDO (FACT [1.0])

| # | Fato | Evidência |
|---|---|---|
| D1 | Descoberta por intento **não existe**. O índice é keyed por nome de símbolo. | `touring tantivy search "prior art"` → `art_root` (shell de fuzz, `scripts/fuzz-gc.sh:37`) e `with_prior` (preditor bayesiano, `touring-ceg/src/gateway/predict.rs:117`). Casamento léxico em nome ≠ propósito. |
| D2 | `touring search-tools` existe mas indexa **só os comandos do próprio Touring**. | Devolve o mesmo `touring generate verify --symbol <name>` para "generate a map" E "generate professional PDF"; devolve `No matching tool` para as duas formulações em português. Zero cobertura multilíngue. |
| D3 | `touring find-code search` **não faz busca semântica** — devolve corpus fabricado. | `touring-storage/src/hybrid_search/hybrid/pipeline.rs:565-582`: calcula o embedding real via `embed_query`, **descarta**, e retorna `[("doc_sem_1",0.98)…("doc_sem_5",0.50)]` constante. Idêntico para as 3 consultas testadas. `vector_store: None` em 3 dos 4 construtores (l.185/205/228). |
| D4 | O sinal de propósito **existe e é rico**, mas num campo que ninguém indexa. | 3.881 scripts `.py` varridos; **96% têm prosa de propósito** (docstring de módulo ou `argparse description`), média 411 chars. |
| D5 | O corpus é **cross-project**, não per-project. | analise 3.464 · skills 128 · touring 104 · transferegov 89 · konverter 82 · kazuba 12 · trading 2. |
| D6 | Mineração só por docstring de script **perde justamente o exemplo do PDF**. | Os 8 scripts de `~/.claude/skills/pdf-anthropic/scripts/` estão nos 4% sem docstring; mas o `SKILL.md` traz descrição excelente. ⇒ o minerador precisa de **dois níveis** (bundle + script) com herança. |
| D7 | Já existe uma biblioteca de prior-art semântico **projetada e morta**. | `touring-intelligence/src/rl/aco/template_library.rs` (376 LOC): `EmbeddingStore` com **0 implementadores**; `find_similar_semantic` / `find_similar_by_embedding` / `record_template` / `with_embedding_store` com **0 chamadores externos**. `aco/mod.rs:33` declara o módulo e não o reexporta. Homônima da `rl/templates/evolving.rs::TemplateLibrary`, que é a viva. |
| D8 | Provider de embedding **real** existe e está wirado. | `touring-storage/src/embeddings/providers/fastembed.rs` — `embed_one_sync` (l.221), `embed_batch_sync` (l.244); feature `storage-emb-fastembed` no `default`. |
| D9 | O `explore` tem catálogo de lentes extensível e um contrato que torna lente nova **inescapável**. | `explore_until_dry.py:57` `AUTOMATED_LENSES = (lexical, structural, institutional, antistaleness, quality)`; uma rodada só conta como seca se **todo** o catálogo rodou. |
| D10 | A ADW de criação já tem o nó onde o portfólio entra de graça. | `feature.toml` nó `recall` já roda `memory recall` + `gotcha match`, e `{{nodes.recall.summary}}` já alimenta o prompt do `scout`. |

**Lente externa (Context7)** — `/websites/rs_tantivy_tantivy`: `QueryParser::set_field_boost`
pondera campos; `TextFieldIndexing::set_tokenizer("en_stem")` para prosa, `raw_ids` para id.
⇒ schema com `purpose` (en_stem, boost alto) acima de `name`/`path` (boost baixo).

## 3. A tese

O portfólio **não é um índice a mais** — é a inversão da chave: indexar o campo
*propósito* que já existe em 96% do corpus e que hoje nenhum índice lê. Buscar
"gerar um mapa" falha não por falta de embeddings, mas porque o BM25 está apontado
para o nome do símbolo em vez da prosa que descreve o que ele faz.

O segundo risco é comportamental, e o repositório já o mediu: *"adoption does not
emerge from availability; it must be actively induced"* (`touring-4-pillars.md`).
Um comando novo que ninguém chama é dívida. Por isso a entrega inclui os pontos
de **afordância** — o portfólio tem de chegar ao contexto sem que a LLM precise
lembrar de pedi-lo.

## 4. O contrato de saída (o que impede a âncora)

Toda consulta devolve **três seções**, nunca uma lista:

```
prior_art[]  — candidatos com PROVENIÊNCIA e EVIDÊNCIA
               (última execução, tem teste?, veredito anterior, reward)
gaps[]       — o que o prior-art NÃO cobre para este intento
external[]   — a consulta a fazer (Context7 library-id + pergunta específica)
```

e exige um **veredito** ∈ `{reusar, estender, superar, criar-novo}` com
justificativa. Nomear o buraco (`gaps`) é o que convida a superar; sem essa seção
a injeção vira âncora. Sem candidato acima do piso, a saída é honesta:
`prior_art: []` + `gaps` + `external` — "nada encontrado" é resultado válido.

O veredito persistido (memory + `learning reward`) é o feromônio: na próxima vez
que "gerar PDF profissional" aparecer, o portfólio já sabe o que foi escolhido
antes e se deu certo. É exatamente `record_template` + `find_similar_semantic`
da biblioteca ACO órfã (D7) — ligar o que existe, não criar paralelo (REGRA #0).

## 5. Fases

| Fase | Entrega | Risco | Depende |
|---|---|---|---|
| **P1** núcleo | Minerador de propósito (2 níveis, D6) + índice global em `~/.touring/portfolio/` + `touring portfolio "<intento>"` com as 3 seções | baixo — puramente aditivo | — |
| **P2** afordância | Lente `portfolio` no `explore` (D9 ⇒ inescapável no loop) + portfólio no nó `recall` de `feature.toml`/`chore.toml` (D10 ⇒ inescapável na ADW) | baixo | P1 |
| **P3** compounding | Veredito persistido em `memory` + `learning reward`; portfólio passa a rankear por outcome | baixo | P1 |
| **P4** semântico | Implementar `EmbeddingStore` ligando a `TemplateLibrary` ACO ao provider fastembed (D7+D8); re-rank semântico sobre o BM25 | médio — fastembed baixa modelo do HF no 1º uso (rede) | P1 |
| **P5** injeção no Write | Hook `cli_suggester` injeta o portfólio ao criar arquivo novo | **alto** — afeta toda sessão de todo projeto | P1-P3 |
| **P6** defeito D3 | Consertar `find-code search` (hoje devolve corpus fabricado) ou marcá-lo explicitamente como stub | médio | — |

## 6. Decisões (Gabriel, 08/08/2026)

| Questão | Decisão |
|---|---|
| Escopo | **P1–P6 completo** |
| Corpus | **tudo, sem teto** — inclusive os 3.464 scripts do `analise` |
| `find-code` | **consertar agora** |

Resultado da decisão do corpus: o `analise` domina o ranking de intentos genéricos
(era o risco previsto), mas os resultados medidos são legítimos — `gerar_pdf_premium.py`
e `gerar_mapas_bamin.py` são exatamente o prior-art que o pedido queria expor.

## 7. Referências

- Diagnóstico OKF: `diagnostics/touring-20260808T120605.md`
- Ledger CCE: `cce-ledger.json` (lente `external` visitada com o achado do Context7)
- Pilares: `~/.claude/rules/touring-4-pillars.md` (afordância vs persuasão)
