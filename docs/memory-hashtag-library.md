---
type: Guide
title: "Hashtag Library — convenção de facetas, codetags e consulta"
description: "Como o Touring classifica cada memória com hashtags #facet:value (7 facetas, vocabulário controlado-extensível), como escrever codetags em código, e como consultar a biblioteca (query, recall, MOCs)."
plan_id: 2026-08-11-memory-hashtag-library
tags: [guide, memory, hashtags, codetags, moc]
timestamp: 2026-08-12T08:00:00-03:00
okf_version: "0.1"
---

# Hashtag Library — guia canônico

Toda memória do Touring carrega **hashtags facetadas** `#facet:value` que dizem o que ela é em eixos ortogonais. Fundamento: classificação facetada de Ranganathan (PMEST/ISO 25964) — folksonomia livre colapsa por sinonímia/polissemia, então cada faceta tem vocabulário controlado que cresce por governança (valores fora do seed são aceitos com WARN, nunca erro).

## As 7 facetas

| Faceta | Responde | Exemplos de valores |
|---|---|---|
| `#kind:` | o que é | snippet, doc, script, map, plan, strategy, lesson, gotcha, decision, module, insight, outcome, pattern, reference |
| `#purpose:` | pra que serve | blast-radius, pre-edit-gate, map-rendering, retrieval, auto-tagging |
| `#lang:` | linguagem/formato | rust, python, bash, md, toml, yaml, json, sql, svg, ts, js |
| `#domain:` | subsistema/domínio | memory, wiring, index, hooks, adw, ceg, portfolio, quality, daemon, cli, generator, intelligence |
| `#process:` | objeto de processo | diagnose, explore, converge, cross-audit, decompose, recall, store, reward |
| `#artifact:` | artefato gerado | map, dashboard, report, diorama, infographic, deck, table |
| `#status:` | maturidade | stable, experimental, deprecated |

Hierarquia intra-faceta com `.`: `#domain:wiring.orphans` ⊂ `#domain:wiring`.

## Auto-tagging (nada a fazer)

Todo `touring memory store` deriva deterministicamente: `lang`←extensão de `file_path`, `kind`←`entry_type` (boundary-aware: `transcript_lesson`→`lesson`) ou prefixo da key, `domain`←segmentos do path/key, `status:stable`. Tags explícitas (`--tag`) são a fonte de maior confiança e nunca são sobrescritas por derivação (trust ordering: `explicit > code_sync > auto > backfill`).

## Codetags — snippets indexáveis em código

Uma linha de comentário com o literal `#tags:` ancora o bloco seguinte (PEP 350 lineage):

```rust
// #tags: kind:snippet lang:rust purpose:blast-radius domain:wiring
pub fn blast_radius(...) { ... }
```

```python
# #tags: kind:script purpose:map-rendering artifact:map
def render_diorama(layers): ...
```

- O bloco vira uma memória `snippet:<path>#L<linha>` cujo **value é o próprio código** — recuperável por tag sem abrir o arquivo.
- Extração: brace-matching (rust/ts/js), indentação estrita (python/bash/yaml/toml), parágrafo (demais); cap 200 linhas.
- `post_write`/`post_edit` sincronizam a cada escrita (fail-open); remover o codetag do código **remove a memória** (tombstone — o código é a fonte da verdade).
- Batch: `touring memory sync-tags --dir <pasta>` (respeita .gitignore).

## Consulta

```bash
touring memory query "#kind:snippet #lang:python"   # conjuntivo exato
touring memory query "diorama #artifact:map"        # híbrido: tag + BM25
touring memory recall "mapa #domain:analise"        # recall filtra corpus por tag
touring memory tags <key>                           # facetas de uma memória
touring memory tag <key> "#domain:wiring"           # anexar (explicit)
touring memory link <src> <dst> --rel extends       # aresta tipada (id determinístico)
touring memory links <key>                          # vizinhança 1-hop
touring memory moc <tópico> [--out <path>]          # Mapa de Conteúdo emergente
touring memory backfill-tags [--dry-run] [--limit N]  # retroativo conservador
touring portfolio "gerar um mapa #kind:script"      # portfólio facetado
```

O recall/report sempre mostra `tag_filter.relaxed: true` quando um filtro impossível foi afrouxado — filtro vazio nunca finge ser "nada existe".

## MOCs — Maps of Content

`touring memory moc <tópico>` detecta comunidades emergentes (label propagation ponderado sobre tags fortes + links; determinístico) e renderiza um doc OKF com `[[wikilinks]]`, cobertura por faceta e links tipados. Singletons vão para "Sem conexões fortes" — o balde que sinaliza onde o tagging pode enriquecer. Exemplo vivo: `docs/knowledge/mocs/maps.md`.

## KPI

`touring kpi -j` → `touring.memory.tag_coverage` (gate ≥ 0.9, não-advisory): cobertura de facetas no store. Queda sinaliza write path sem derivação.
