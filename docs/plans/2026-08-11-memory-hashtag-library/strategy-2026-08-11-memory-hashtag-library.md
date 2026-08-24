---
type: Strategy
title: "Memória Catalogada por Hashtags Facetadas — biblioteca de snippets e conhecimento"
description: "Estratégia para dotar cada memória do Touring de hashtags multi-facetadas (#facet:value), auto-tagging na geração (codetags PEP 350 em código, inline em docs), storage híbrido (grafo bipartido tag↔item + links tipados + hierarquia intra-faceta + comunidades emergentes) sobre o memory.db existente, para recuperação como contexto de LLM e reuso de artefatos (mapas, designs, scripts) em vez de recriação."
plan_id: 2026-08-11-memory-hashtag-library
tags: [loop, strategy, memory, hashtags, faceted-classification, knowledge-graph, snippet-library]
timestamp: 2026-08-11T20:35:00-03:00
okf_version: "0.1"
---

# Estratégia — Biblioteca de Memórias por Hashtags Facetadas

Part of the [bundle](/index.md). Diagnostic: [touring-20260811T202729](/diagnostics/touring-20260811T202729.md).

## 1. Objetivo (intenção do Gabriel)

1. Toda memória do Touring carrega **hashtags que dizem o que ela é em diversos aspectos** — conteúdo de código, estratégia, objeto de processo, tipo de funcionalidade, assunto de um script/pasta/diretório.
2. Uma **grande biblioteca de snippets e conteúdo** catalogada, organizada e inventariada: o que faz, como faz, pra que serve.
3. **Auto-tagging na criação**: ao gerar snippet/arquivo/conteúdo, as hashtags nascem junto — comentário no código; inline no corpo quando não-código — e são salvas na memória de forma estruturada.
4. **Recuperação granular**: consultar trechos específicos (snippet, não arquivo inteiro) — enrichment que personaliza consultas sem engessar.
5. **Reuso sobre recriação**: ao complementar/evoluir/refinar trabalho anterior (ex: documento com artefato de mapa de design específico), o banco já oferece o que existe.

## 2. Evidência externa (lente `external` — Context7 + web, 2026-08-11)

| Fonte | O que prova para nós |
|---|---|
| **A-MEM** ([arXiv:2502.12110](https://arxiv.org/abs/2502.12110), NeurIPS'25) | Validação acadêmica exata: memória = nota Zettelkasten {contexto, keywords, **tags**, embedding} + **link generation** dinâmico + **memory evolution** |
| **Graphiti/Zep** ([getzep/graphiti](https://github.com/getzep/graphiti)) | Produção: `Node.labels: list[str]` + `episode_metadata` p/ filtering; namespaces entity/episode/community |
| **Ranganathan PMEST** ([faceted classification](https://en.wikipedia.org/wiki/Faceted_classification), ISO 25964) | Facetas ortogonais; folksonomia livre colapsa por synonymy/polysemy/homonymy → **vocabulário controlado por faceta** |
| **Microsoft GraphRAG** ([docs](https://microsoft.github.io/graphrag/), [paper](https://arxiv.org/html/2404.16130v2)) | Hierarquia deve **emergir** do grafo (Leiden communities + summaries bottom-up); híbrido local+global vence |
| **LlamaIndex extractors** ([metadata extraction](https://github.com/run-llama/llama_index)) | Padrão de engenharia de auto-tagging na ingestão (Title/Questions/Summary/Keyword/Entity) |
| **PEP 350 codetags** ([pep-0350](https://peps.python.org/pep-0350/)) | Convenção canônica de tags inline em comentários de código — grep-able, tooling maduro |
| **Qdrant payload index** ([qdrant.tech](https://qdrant.tech)) | Hybrid retrieval: filtro exato por tag + busca vetorial |

## 3. Gap verificado no Touring (forense — sqlite3, 2026-08-11)

`memory.db::memory_entries` = `{key, value, tier, entry_type, file_path, palace_path, embedding, importance, pinned, superseded_by}` + `memories_fts` (FTS5) + `embeddings` + `chunks`. **Não existe**: campo/tabela de tags, auto-tagging no store, granularidade sub-arquivo (snippet), links tipados além de `superseded_by`. Portfolio (4.068 artefatos) indexa por `intent` único — não multi-facetado. O caso de reuso é real: `render_map.py` já tem `artifact_id + intent + verdict` no portfolio.

## 4. Decisões arquiteturais

### D1 — Modelo de tag: **facetado namespaced** `#facet:value` (não flat folksonomy)

Facetas canônicas (vocabulário controlado, extensível por governança):

| Faceta | Responde | Exemplos |
|---|---|---|
| `#kind:` | o que é | snippet, doc, script, map, plan, strategy, lesson, gotcha, decision, process |
| `#purpose:` | pra que serve | blast-radius, pre-edit-gate, map-rendering (alinha com portfolio `intent`) |
| `#lang:` | linguagem/formato | rust, python, bash, md, toml, svg |
| `#domain:` | subsistema/domínio | wiring, memory, index, hooks, adw, ceg, portfolio |
| `#process:` | objeto de processo | diagnose, explore, converge, cross-audit, decompose |
| `#artifact:` | tipo de artefato gerado | map, dashboard, report, diorama |
| `#status:` | maturidade | stable, experimental, deprecated |

Hierarquia permitida **dentro** da faceta: `#domain:wiring.orphans` (nested tags — Obsidian/ISO 25964-1 §10).

### D2 — Estrutura: **híbrida em 4 camadas** (nem grafo puro, nem árvore pura)

```
L1  tag ↔ item      grafo BIPARTIDO (tabela memory_tags)        → filtro exato por faceta
L2  dentro da faceta  ÁRVORE local (purpose:maps.diorama < …)   → navegação/descoberta
L3  memória ↔ memória LINKS TIPADOS (memory_links, ids {src}|{rel}|{dst}
    relates-to · supersedes · extends · exemplifies · generated-by — REGRA #17)
L4  comunidades EMERGENTES (Leiden sobre L1+L3) + summaries     → MOCs automáticos
    bottom-up (GraphRAG) — responde o caso "artefato de mapa"
```

Justificativa: árvore pura falha para item multi-atributo (Ranganathan); grafo puro sem vocabulário = caos de folksonomia; GraphRAG provou que hierarquia emerge do grafo. **Storage: SQLite adjacency + FTS5 + embeddings já existentes** — zero DB novo; retrieval híbrido = tag-exato + BM25 + vetorial (padrão Graphiti/Qdrant).

### D3 — Auto-tagging na geração (LlamaIndex + PEP 350)

- **Código**: comentário âncora na 1ª linha do snippet — `// #tags: kind:snippet lang:rust purpose:blast-radius domain:wiring` (grep-able, sincronizável código→memória por indexer = anti-drift).
- **Docs/md**: `#facet:value` inline no corpo ou `tags:` no frontmatter OKF.
- **`memory store`**: derivação automática — determinística (`lang`←extensão, `kind`←entry_type/path) + classificação pelo próprio TACO na geração (regra em skill/constituição).
- **Tag linter**: valida contra vocabulário (fail-warn); aliases novos acumulam no gotcha DB (governança).

### D4 — Snippet-level (granularidade sub-arquivo)

`entry_type='snippet'` + `file_path` + `span{start_line,end_line}` + codetag como âncora. Indexer re-colhe via hooks post-write/post-edit existentes.

### D5 — Potencialização (REGRA #0), não duplicação

Estende `memory.db`; portfolio ganha facetas além de `intent`; `memory recall` parseia `#tags` → filtro exato + BM25 + embedding; links seguem REGRA #17 (id determinístico).

## 5. Fases (INNER loop — convergência medida por fase)

| Fase | Entrega | Gate |
|---|---|---|
| **F0** | Schema: `memory_tags` + `memory_links` + migration; vocabulário de facetas v1 + tag linter | migration idempotente + linter testado |
| **F1** | Auto-tagging no `memory store`; CLI `touring memory tag add/list` + `memory query "#kind:snippet …"` | E2E: store→tag→query |
| **F2** | Convenção codetag + snippet indexer (code→memory sync, span-aware) | E2E: snippet ancorado recuperável por tag |
| **F3** | Link graph memória↔memória + recall híbrido (tag-exato + FTS + embedding) | E2E: recall com filtro de faceta |
| **F4** | Backfill das memórias existentes + facetas no portfolio | ≥90% memórias classificadas |
| **F5** | Comunidades emergentes (Leiden) + MOCs automáticos | MOC de um domínio real (ex: mapas) |
| **F6** | Docs, skill, hooks (post-write auto-tag), gates, OKF close | `loop_converged.py` exit 0 |

## 6. Riscos e mitigações

| Risco | Mitigação |
|---|---|
| Vocabulário degrada para folksonomia (synonymy) | Tag linter + aliases no gotcha DB + `evolution drift` periódico |
| Custo de tagging em toda geração | Derivação determinística primeiro; LLM-classificação só onde não derivável |
| Drift código↔memória (codetag editado) | Indexer re-colhe em post-edit; conflito → memória perde para código (fonte da verdade) |
| Backfill de 20k+ memórias com tags ruins | Backfill conservador (só facetas deriváveis); revisão incremental por acesso |

## 7. Convergência

`loop_converged.py --task <id> --scope <path>` exit 0 é o único "pronto" — por fase e no close global (Lei L2).
