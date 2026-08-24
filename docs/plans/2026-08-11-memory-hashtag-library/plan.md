---
type: Plan
title: "Plano — Biblioteca de Memórias por Hashtags Facetadas"
description: "Plano operacional F0-F6: schema de tags/links sobre memory.db, auto-tagging na geração (codetags PEP 350 + frontmatter), snippet indexer span-aware, link graph tipado, recall híbrido (tag-exato + FTS5 + embedding), backfill e MOCs emergentes."
plan_id: 2026-08-11-memory-hashtag-library
tags: [loop, plan, memory, hashtags, snippet-library]
timestamp: 2026-08-11T20:40:00-03:00
okf_version: "0.1"
---

# Plano — Hashtag Library F0-F6

Part of the [bundle](/index.md). Estratégia aprovada: [strategy-2026-08-11-memory-hashtag-library](/strategy-2026-08-11-memory-hashtag-library.md).

## Convenções de implementação

- **Crate alvo**: `touring-foundation` (memory store) + `touring-cli` (comandos `memory tag/query`) + hooks (post-write/post-edit indexer).
- **Sem DB novo**: tudo sobre `.claude/touring/memory.db` (migration aditiva, idempotente).
- **Formato de tag**: `#facet:value` — regex `#[a-z]+:[a-z0-9._-]+`; hierarquia intra-faceta via `.` (`wiring.orphans`).
- **Codetag em código**: `// #tags: kind:snippet lang:rust purpose:<p> domain:<d>` (1ª linha do snippet ou acima da fn).
- **Docs OKF**: `tags:` no frontmatter (já existe!) + inline `#facet:value` no corpo.
- **REGRA #17**: `memory_links.id = {src}|{rel}|{dst}` (determinístico).
- **Gates por fase**: cross-audit + 50-dim + `loop_phase_close.py` + `loop_converged.py`.

## F0 — Schema + vocabulário (deps: —)

- Migration `memory.db`: `CREATE TABLE IF NOT EXISTS memory_tags(entry_key TEXT NOT NULL, facet TEXT NOT NULL, value TEXT NOT NULL, full_tag TEXT NOT NULL, source TEXT NOT NULL DEFAULT 'auto', created_at TEXT NOT NULL DEFAULT (datetime('now')), PRIMARY KEY(entry_key, full_tag))` + índices `(facet,value)` e `(full_tag)`.
- `memory_links(src TEXT, dst TEXT, rel TEXT, id TEXT PRIMARY KEY /* {src}|{rel}|{dst} */, created_at …)`.
- `tags_fts` (FTS5 sobre full_tag + value) ou coluna `tags` adicionada ao `memories_fts` — decidir na impl pelo custo de rebuild (FTS externa é mais segura).
- Vocabulário v1 em `crates/touring-foundation/src/memory/facets.rs`: 7 facetas canônicas + valores seed + aliases.
- Tag linter: `validate_tag(s) -> Result<ParsedTag, Vec<TagViolation>>` (faceta desconhecida → warn+sugestão via gotcha DB).
- **Exit**: migration idempotente (2ª run no-op), linter com testes paramétricos (rstest), `cargo test -p touring-foundation` verde.

## F1 — Auto-tagging no store + CLI (deps: F0)

- `memory store`: derivação determinística — `lang`←extensão de `file_path`; `kind`←`entry_type`; `domain`←path heurístico (crate/ subsistema); `status:stable` default.
- Flags `--tag "#facet:value"` (repetível) para tags explícitas (LLM-classificadas na geração).
- CLI: `touring memory tag add <key> <tag>`, `touring memory tags <key>`, `touring memory query "<texto> #facet:value …"` — parser separa `#tags` (filtro exato) do texto (FTS).
- **Exit**: E2E store→auto-tag→query por faceta; tags explícitas prevalecem sobre derivadas em conflito.

## F2 — Codetag convention + snippet indexer (deps: F1)

- Convenção documentada + template no generator: novos snippets nascem com `// #tags:` ancorado.
- Indexer: scan de `#tags:` em arquivos (ripgrep → parse → upsert em `memory_tags` com `entry_type='snippet'`, `file_path`, `span`).
- Hook post-write/post-edit: re-colhe tags do arquivo tocado (incremental, <10ms por arquivo; batch sob demanda).
- Drift rule: código é fonte da verdade; tag removida do código → removida da memória (tombstone, não delete duro).
- **Exit**: E2E — snippet ancorado em `.rs` recuperável via `memory query "#lang:rust"`; edit que remove codetag sincroniza.

## F3 — Link graph + recall híbrido (deps: F1)

- `touring memory link <src> <dst> --rel relates-to|supersedes|extends|exemplifies|generated-by`.
- `memory recall` enriquecido: resultado + 1-hop links (extends/supersedes) no payload.
- Scoring híbrido: tag-match exato (boost) + FTS5 BM25 + embedding ANN — fusão RRF (reciprocal rank fusion), pesos na config.
- **Exit**: E2E — recall de "mapa diorama" retorna `render_map.py` + links; tag-filter reduz corpus antes do BM25.

## F4 — Backfill + portfolio facetado (deps: F2, F3)

- Backfill conservador das memórias existentes: só facetas deriváveis deterministicamente (kind/lang/domain); marca `source='backfill'`.
- Portfolio: `touring portfolio "<intent>"` passa a aceitar `#facet:value` e expõe facetas dos artefatos.
- **Exit**: ≥90% das `memory_entries` com ≥3 facetas; nenhuma tag inventada por LLM no backfill em massa.

## F5 — Comunidades emergentes + MOCs (deps: F3)

- Detecção de comunidades (Leiden ou label-propagation simples sobre L1+L3) em `touring memory moc <domain>`.
- MOC = doc OKF gerado: resumo bottom-up + links `[[…]]` para entradas (Hyper-Extract pattern já usado no loop).
- Caso prova: MOC do domínio "mapas" (render_map.py + diorama + data_layers do projeto analise).
- **Exit**: MOC de um domínio real regenerável; links resolvem; scriber documenta.

## F6 — Docs, skill, hooks, gates (deps: F4, F5)

- `docs/` (OKF): convenção de facetas, codetag spec, guia de query.
- Skill `Touring` atualizada (seção memory-tags) + decision matrix (linha nova: "catalogar/recuperar snippet").
- Hooks: post-write auto-tag armado por padrão; KPI `touring.memory.tag_coverage` em `touring kpi -j`.
- **Exit**: `loop_converged.py` exit 0; cross-audit limpo; REGRA #0 (0 orphans novos).

## Paralelização

F2 ∥ F3 (após F1); F4 ∥ F5 (após F2+F3); F6 fecha. F0→F1 sequenciais.
