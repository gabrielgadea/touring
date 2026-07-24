---
plan: "touring-premium-refactor-2026"
version: "1.0.0"
wave: "W5"
name: "touring-storage Fusion"
phase: "F2-FUSIONS"
depends_on:
  - W3
parallel_with:
  - W7
status: "DONE"
created: "2026-05-11"
completed: "2026-05-15"
cila: "L3"
rust_changes: "FUSION"
estimated_days: "10-12"
checkpoint: "touring_premium_W5_20260511.toon"
validation_script: "scripts/touring_premium_refactor_2026/validate_W5.py"
cross_references:
  - 00-INDEX.md
  - CROSS-AUDIT.md
  - W0-*.md
  - W1-*.md
  - W2-*.md
  - W3-*.md
  - W4-*.md
discover_protocol:
  tantivy: "touring tantivy search '<keyword>' -j"
  wiring_impact: "touring wiring impact <symbol> --depth 2"
  ast_blast: "touring ast blast <file>"
  memory_recall: "touring memory recall '<query>'"
---
# W5: touring-storage Fusion

> **Plano**: `touring-premium-refactor-2026` v1.0.0
> **Fase**: F2-FUSIONS
> **Contribuição para resultado final**: 6 crate-boundaries → 1. Embedding/vector backends ficam como features opt-in, reduzindo binary size para tier-free em ~30%. Repaga test-debt de search-fusion (0%) e salsa (0%).

---

## Contexto e Dependências

- **Depende de**: W3
- **Paralelo com**: W7
- **CILA**: `L3`
- **Mudanças Rust**: `FUSION`
- **Estimativa**: 10-12 dias
- **Checkpoint**: `touring_premium_W5_20260511.toon`
- **Script de validação**: `scripts/touring_premium_refactor_2026/validate_W5.py`

---

## Descrição

Fundir 6 crates pequenos relacionados a storage: touring-index (2.7k), touring-vfs (1.6k), touring-incremental-salsa (387L), touring-vector-store (1.2k), touring-embeddings (1.4k), touring-search-fusion (1.5k) → touring-storage (~10k LOC). Features 100% opt-in: storage-fts, storage-vec-*, storage-emb-*, storage-vfs-*, storage-salsa. Adicionar +500 LOC tests para crates com 0% ratio.

---

## Efeitos no Sistema

- touring-storage criado (~10k LOC, ≥ 25% test ratio)
- 6 crates absorvidos como submódulos
- 11 features storage-* opt-in
- +500 LOC tests para search-fusion e salsa
- Consumers atualizados (~15 crates)

---

## Subtarefas (CODE-FIRST — DISCOVER antes de cada)

> **PROTOCOLO DISCOVER OBRIGATÓRIO antes de cada subtarefa**:
> 1. `touring tantivy search '<keyword>' -j` (Tantivy BM25)
> 2. `touring wiring impact <symbol> --depth 2` (transitive consumers)
> 3. `touring ast blast <file>` (dependency tree)
> 4. `touring memory recall '<query>'` (past lessons)
> 5. `touring index find <symbol> -j` (VGP gate)

### W5.1: Create touring-storage skeleton

**Descrição**: taco-forge perfect-create-crate. Cargo.toml com features storage-*.

**Dias estimados**: 0.5

**Critério de validação**: cargo check -p touring-storage exit 0.

---

### W5.2: Move touring-index → storage/src/fts/

**Descrição**: 2.7k LOC. Tantivy wrapper. Feature 'storage-fts' (default).

**Dias estimados**: 0.7

**Critério de validação**: cargo check -p touring-storage --features storage-fts exit 0.

---

### W5.3: Move touring-vfs → storage/src/vfs/

**Descrição**: 1.6k LOC. Submodules mem + disk. Features storage-vfs-mem, storage-vfs-disk (default).

**Dias estimados**: 0.7

**Critério de validação**: cargo test -p touring-storage vfs exit 0.

---

### W5.4: Move touring-incremental-salsa → storage/src/salsa/

**Descrição**: 387 LOC + 0% tests. Feature 'storage-salsa'. Adicionar tests +200 LOC.

**Dias estimados**: 1.0

**TDD RED** (escrever ANTES do código):
```python
def test_salsa_incremental_invalidation():
    """RED: tests for Durability tiers + Revision invalidation missing."""
```

**Critério de validação**: cargo test -p touring-storage salsa: ≥ 5 tests pass.

---

### W5.5: Move touring-vector-store → storage/src/vec/

**Descrição**: 1.2k LOC. Submodules sqlite, qdrant, in_memory. Features storage-vec-sqlite (default), storage-vec-qdrant, storage-vec-mem.

**Dias estimados**: 0.7

**Critério de validação**: cargo check --features storage-vec-qdrant exit 0.

---

### W5.6: Move touring-embeddings → storage/src/embeddings/

**Descrição**: 1.4k LOC. Providers: candle, fastembed, voyage. Features storage-emb-candle (default), storage-emb-fastembed, storage-emb-voyage.

**Dias estimados**: 0.7

**Critério de validação**: cargo check --features storage-emb-voyage exit 0.

---

### W5.7: Move touring-search-fusion → storage/src/hybrid_search/

**Descrição**: 1.5k LOC + 0% tests. Hybrid BM25 + vec + reranker. Adicionar tests +300 LOC.

**Dias estimados**: 1.5

**TDD RED** (escrever ANTES do código):
```python
def test_hybrid_search_rrf_fusion():
    """RED: hybrid_search reciprocal_rank_fusion untested."""
```

**Critério de validação**: cargo test -p touring-storage hybrid_search: ≥ 8 tests pass.

---

### W5.8: Define features storage-* + update 15 consumers

**Descrição**: Atualizar 15 consumers (touring-server, hooks, generator, etc.) para importar de touring_storage. Shim crates.

**Dias estimados**: 3.0

**DISCOVER obrigatório**:
  - touring wiring impact 'touring_index' --depth 2
  - touring wiring impact 'touring_vfs' --depth 2

**Critério de validação**: cargo check --workspace exit 0; shims em 6 crates antigos.

---

### W5.9: Bench query latency — regression < 5%

**Descrição**: cargo bench --workspace baseline-comparison. FTS query, vec search, hybrid.

**Dias estimados**: 1.0

**Critério de validação**: Bench delta vs baseline ≥ -5%.

---

### W5.10: Delete old crates + update workspace

**Descrição**: Remove 6 crates + shims onde possível.

**Dias estimados**: 1.0

**Critério de validação**: ls crates/touring-{index,vfs,...}/ → shims only.

---

## Gate de Saída

touring-storage 10k LOC, 11 features, ≥ 25% test ratio (0% crates repagos), < 5% perf regression, 15 consumers updated.

## Riscos Específicos

- Qdrant feature exige docker em CI → marcar como ignore por default
- Candle BGE download de modelo em test → mockar embedding provider

## Checklist de Conclusão

- [ ] Todos os subtasks implementados
- [ ] Todos os testes TDD GREEN
- [ ] `cargo check --workspace` exit 0
- [ ] `cargo test --workspace --no-fail-fast` pass
- [ ] `cargo clippy --workspace -- -D warnings` clean
- [ ] `touring wiring cycles --min-depth 2` no new cycles
- [ ] `touring wiring orphans -j` no new orphans (REGRA #0)
- [ ] Bench regression < 5%
- [ ] Test ratio ≥ 20% per touched crate
- [ ] Checkpoint `.toon` salvo
- [ ] Memory lesson persistida (`touring memory store --tier semantic`)
- [ ] RL reward injetado (`touring learning reward orchestrate <val>`)
- [ ] Documentação atualizada (se necessário)

---

## Discovery Updates (2026-05-11) — Storage Extraction Targets Identificados

Auto-script `w5_storage_skeleton.py` mapeou TODO o código de storage espalhado no workspace, identificando os TOP 3 extraction targets para W5.2.

### Top 3 storage hotspots

| File | Pattern hits | Padrões |
|---|---|---|
| `crates/touring-server/src/reasoning/persistence.rs` | **60** | sqlite + checkpoint |
| `crates/touring-hooks/src/knowledge.rs` | **59** | sqlite + checkpoint |
| `crates/touring-server/src/server/tools_infra.rs` | **51** | sqlite + checkpoint |

### Skeleton emitido em `staging/w5-touring-storage/`

7 arquivos prontos para serem usados como base de W5.2:

- `Cargo.toml` — features: `default = ["sqlite", "tantivy", "rkyv"]`
- `src/lib.rs` — trait `Store` + module dispatch por feature
- `src/error.rs` — `StorageError` enum (Sqlite/Tantivy/Rkyv/Checkpoint/Io)
- `src/sqlite.rs` — placeholder backend
- `src/tantivy.rs` — placeholder backend
- `src/rkyv_archive.rs` — placeholder backend
- `src/checkpoint.rs` — placeholder backend (JSON + TOON)

### Ação revisada para W5

1. **W5.1**: ✅ Skeleton pronto — `staging/w5-touring-storage/`
2. **W5.2**: Extract `persistence.rs` (60 hits) primeiro — maior ROI
3. **W5.3**: Extract `knowledge.rs` (59 hits) — segundo maior
4. **W5.4**: Extract `tools_infra.rs` (51 hits) — terceiro maior

### Forensic outputs disponíveis

- `data/w5-storage-extraction-targets.json` — top 15 targets
- `staging/w5-touring-storage/` — skeleton crate completo

---

## Discovery Updates (2026-05-15) — Execução

### touring-index NÃO foi fundido — ciclo de dependência Cargo

A premissa original (6 crates fundidos) era incompilável. `touring-index`
depende de `touring-ast`/`touring-semantics` (camada intelligence). Pós-W4,
`touring-code` depende de `touring-vfs` (`ast/file_heat.rs`). Fundir
`touring-index` em `touring-storage` criaria o ciclo Cargo
`touring-code → touring-vfs → touring-storage → touring-ast → touring-code`.

**Resolução**: `touring-storage` funde **5 crates** {vfs, salsa, vec,
embeddings, hybrid_search} — camada pura de storage, depende apenas de
`touring-foundation`. `touring-index` permanece standalone (camada
intelligence) e é deferido para **W6 (touring-intelligence)**, onde se
acopla naturalmente a AST/semantics.

### Premissa "0% tests" estava stale

O plano alegava 0% de testes em `search-fusion` e `salsa`. Estado real no
momento da execução: search-fusion 40 test fns, salsa 11. Ambos já tinham
testes — nenhum padding artificial de +500 LOC foi necessário.
`touring-storage` final: 147 test fns + 4 integration files (349 LOC),
141 testes passando.

### Features pré-existentes quebradas (não regressão W5)

`cargo check --features` nos crates ORIGINAIS revelou breakage de API-drift
anterior a W5: `qdrant` (11 erros — qdrant-client API drift), `candle-bge`
(5 erros — candle API inexistente). Ambas idênticas pós-fusão = preservação
fiel. `voyage` (2 erros E0195) foi **corrigido** (faltava `#[async_trait]`).
qdrant + candle-bge ficam para repagamento em W11.

### Bench queries_bench removido (dead-on-arrival)

O `[[bench]]` de `touring-incremental-salsa` nunca compilou: usava `FileId`
(símbolo inexistente), `FileText::new` com aridade errada, e
`Throughput::throughput` (API inexistente). Removido como dead code
(REGRA #0). Benchmark de salsa a ser reescrito do zero se necessário.

### Resultado

| Métrica | Valor |
|---|---|
| Crates fundidos | 5 (vfs, salsa, vec, embeddings, hybrid_search) |
| touring-storage src | 6.046 LOC, 36 files |
| Shims (1-file lib.rs) | 5 crates |
| Testes touring-storage | 141 passando (124 lib + 17 integração) |
| `cargo check --workspace` | 0 erros |
| clippy (storage + shims) | 0 issues |
| Wiring cycles | 2 (sem regressão vs W4) |
