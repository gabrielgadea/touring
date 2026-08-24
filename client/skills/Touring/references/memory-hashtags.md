# Memory Hashtag Library (pointer)

> **Canonical guide**: `~/projects/touring/docs/memory-hashtag-library.md` (convenção completa de facetas, codetags, consulta, MOCs) | **Bundle do plano**: `~/projects/touring/docs/plans/2026-08-11-memory-hashtag-library/`

Cada memória do Touring carrega hashtags facetadas `#facet:value` (7 facetas: kind/purpose/lang/domain/process/artifact/status; vocabulário controlado-extensível — unseeded = WARN). Auto-tagging determinístico em todo `memory store`; explícitas via `--tag` (maior confiança).

Reflexos novos:
- **Antes de criar** snippet/script/artefato → `touring portfolio "<intento> [#kind:… #lang:…]"` + `touring memory query "#kind:snippet #domain:<d>"` (prior art por faceta).
- **Ao gerar código reutilizável** → ancorar `// #tags: kind:snippet lang:… purpose:… domain:…` na 1ª linha do bloco (o hook post-write indexa sozinho; remover o codetag apaga a memória).
- **Ao buscar contexto** → `touring memory recall "<texto> #facet:value"` (filtro conjuntivo pré-RRF; `tag_filter.relaxed` no payload mostra afrouxamento) e `touring memory moc <tópico>` para o mapa emergente do domínio.
- **Ao corrigir/evoluir** → `touring memory link <novo> <antigo> --rel supersedes` (grafo tipado, id determinístico).
- **KPI**: `touring.memory.tag_coverage` ≥ 0.9 (gate não-advisory em `touring kpi -j`).

Origem: loop 2026-08-11/12 (F0-F6). Prior art: A-MEM (arXiv 2502.12110), Ranganathan PMEST/ISO 25964, Graphiti, GraphRAG, PEP 350, Qdrant payload filters.
