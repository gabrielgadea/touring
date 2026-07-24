# Tantivy FTS + SCIP Emit — Design Spec

> **Date**: 2026-04-12
> **Predecessor**: PLAN-file-metadata-expansion-v2-squared.md (P10 + C-scip)
> **Approach**: Abordagem 1 — Tantivy em touring-hooks + SCIP em touring-server, independentes, feature-gated

---

## Objective

Implementar busca full-text avançada via Tantivy 0.22 (ADDITIVE ao FTS5 existente) e exportação SCIP para integração IDE. O Tantivy é o mais completo possível: schema rico com 8+ campos, BM25 ranked search, fuzzy matching, phrase queries, multi-field boosting, batch commit policy, e integração com todos os hooks relevantes.

---

## Part 1: Tantivy Integration (P10)

### T-1: Workspace dependency + features

**Arquivos**:
- `Cargo.toml` (workspace): `tantivy = { version = "0.22", optional = true }`
- `touring-hooks/Cargo.toml`: `tantivy = { workspace = true, optional = true }`, feature `tantivy-fts = ["dep:tantivy"]`
- `touring-server/Cargo.toml`: `tantivy-fts = ["touring-hooks/tantivy-fts"]` (passthrough)

**T-shirt**: S

### T-2: TantivyIndex module (touring-hooks)

**Arquivo**: `touring-hooks/src/tantivy_index.rs` (~400 LOC)

**Schema** (8 campos, o mais completo possível):

```rust
pub struct TantivyIndex {
    index: tantivy::Index,
    writer: Arc<RwLock<IndexWriter>>,
    reader: IndexReader,
    pending_count: AtomicU32,
    last_commit: AtomicU64,  // epoch millis
}
```

**Campos do schema**:

| Campo | Tipo Tantivy | Propósito |
|-------|-------------|-----------|
| `symbol_name` | TEXT (tokenized, stored) | Nome do símbolo (camelCase-aware tokenizer) |
| `file_path` | STRING (stored, fast) | Caminho do arquivo |
| `symbol_kind` | STRING (stored, fast) | fn/struct/enum/trait/const/mod/type |
| `module_path` | TEXT (tokenized) | Caminho do módulo (crate::module::item) |
| `docstring` | TEXT (tokenized, stored) | Documentação /// do símbolo |
| `functional_signature` | TEXT (tokenized) | Assinatura de tipo (fn(A,B) -> C) |
| `line_number` | u64 (stored, fast) | Linha no arquivo |
| `language` | STRING (stored, fast) | rust/python/typescript/etc |

**Custom Tokenizer**: Reusa `touring_antt::search_index::CodeAwareTokenizer` (camelCase split, snake_case split, path-aware) registrado como tokenizer "code_aware" no Tantivy.

**API pública**:

```rust
impl TantivyIndex {
    pub fn open_or_create(index_dir: &Path) -> Result<Self, String>;
    pub fn upsert_symbol(&self, doc: SymbolDoc) -> Result<(), String>;
    pub fn upsert_batch(&self, docs: Vec<SymbolDoc>) -> Result<usize, String>;
    pub fn search(&self, query: &str, top_k: usize) -> Result<Vec<SearchHit>, String>;
    pub fn search_with_filters(&self, query: &str, kind: Option<&str>, lang: Option<&str>, top_k: usize) -> Result<Vec<SearchHit>, String>;
    pub fn fuzzy_search(&self, query: &str, distance: u8, top_k: usize) -> Result<Vec<SearchHit>, String>;
    pub fn phrase_search(&self, phrase: &str, field: &str, top_k: usize) -> Result<Vec<SearchHit>, String>;
    pub fn commit_if_needed(&self) -> Result<bool, String>;  // batch policy
    pub fn force_commit(&self) -> Result<(), String>;
    pub fn delete_by_file(&self, file_path: &str) -> Result<u64, String>;
    pub fn stats(&self) -> IndexStats;
}
```

**Batch commit policy**:
- Auto-commit every 100 upserts OU 30s timer (o que vier primeiro)
- `try_write()` com 10ms timeout para evitar contention nos hooks
- Se write lock falha, incrementa counter `tantivy_write_contention` e retorna Ok (non-blocking)

**SymbolDoc** (input struct):
```rust
pub struct SymbolDoc {
    pub symbol_name: String,
    pub file_path: String,
    pub symbol_kind: String,
    pub module_path: Option<String>,
    pub docstring: Option<String>,
    pub functional_signature: Option<String>,
    pub line_number: u64,
    pub language: String,
}
```

**SearchHit** (output struct):
```rust
pub struct SearchHit {
    pub symbol_name: String,
    pub file_path: String,
    pub symbol_kind: String,
    pub line_number: u64,
    pub score: f32,
    pub snippet: Option<String>,  // highlighted match context
}
```

**Index location**: `~/.claude/touring/tantivy_index/` (alongside knowledge.db)

**T-shirt**: L

### T-3: Wire into hooks

**Arquivos modificados**:
- `touring-hooks/src/post_edit.rs` — após metadata collection, upsert símbolos editados
- `touring-hooks/src/post_write.rs` — após write, upsert todos os símbolos do arquivo
- `touring-hooks/src/shared/mod.rs` — registrar módulo `tantivy_index`

**Wiring pattern** (non-blocking, feature-gated):
```rust
#[cfg(feature = "tantivy-fts")]
{
    if let Some(ref idx) = runtime.tantivy_index {
        let symbols = extract_symbols_from_content(content, file_path);
        for sym in symbols {
            let _ = idx.upsert_symbol(sym);  // fire-and-forget, non-blocking
        }
        let _ = idx.commit_if_needed();
    }
}
```

**HookRuntime extension**: Adicionar `pub tantivy_index: Option<Arc<TantivyIndex>>` ao `HookRuntimeContext`.

**T-shirt**: M

### T-4 (NOVO — potencializar): CLI handler + MCP tool

**CLI handler** em `cli_handlers.rs`:
```rust
pub fn cli_tantivy_search(rt: &mut HookRuntime, payload: &serde_json::Value) -> String
pub fn cli_tantivy_stats(rt: &mut HookRuntime, payload: &serde_json::Value) -> String
pub fn cli_tantivy_reindex(rt: &mut HookRuntime, payload: &serde_json::Value) -> String
```

**Registrar em hook_registry.rs**: +3 hooks (113→116)

**CLI router** em `touring-server/src/cli/search.rs` (já existe):
- Adicionar backend Tantivy quando feature ativa, fallback para FTS5

**MCP tools**: `touring_tantivy_search`, `touring_tantivy_stats` em `tools_metadata.rs`

**T-shirt**: M

### T-5 (NOVO — potencializar): Backfill command

**CLI**: `touring tantivy reindex [--parallel 4]`
- Rayon parallel walk de todos arquivos indexados
- Extrai símbolos via tree-sitter (IncrementalPipeline)
- Upsert batch no Tantivy
- Reporta: `{"files_processed": N, "symbols_indexed": M, "elapsed_ms": T}`

**T-shirt**: M

---

## Part 2: SCIP Emit (C-scip + C-18)

### S-1: Workspace dependencies

**Arquivos**:
- `Cargo.toml` (workspace): `prost = "0.13"`, `prost-types = "0.13"`
- `touring-server/Cargo.toml`: `prost = { workspace = true, optional = true }`, feature `scip-emit = ["dep:prost"]`

**T-shirt**: S

### S-2: SCIP emit module

**Arquivo**: `touring-server/src/scip_emit.rs` (~300 LOC)

**SCIP format** (Source Code Intelligence Protocol):
- Definido por Sourcegraph: https://sourcegraph.com/docs/code-intelligence/scip
- Binary protobuf format com `Index`, `Document`, `Occurrence`, `SymbolInformation`

**Struct**:
```rust
pub struct ScipEmitter {
    project_root: PathBuf,
}

impl ScipEmitter {
    pub fn new(project_root: PathBuf) -> Self;
    pub fn emit(&self, symbols: &[SymbolEntry], output: &Path) -> Result<usize, String>;
}
```

**Popula a partir de**: `touring-index::IncrementalIndex::search_symbols("")` (all symbols) + `touring-hooks::knowledge.rs` file metadata

**Output**: Binary protobuf file (`.scip`)

**T-shirt**: L

### S-3: CLI handler + MCP tool

**CLI**: `touring emit scip --out /tmp/index.scip [-j]`
**MCP**: `touring_emit_scip` em `tools_metadata.rs`
**Hook registry**: +1 hook (116→117)

**T-shirt**: S

---

## Risks

| Risk | Severity | Probability | Mitigation |
|------|----------|-------------|------------|
| Tantivy 15MB compile overhead | LOW | HIGH | Feature-gated, não ativa por default |
| Writer lock contention em hooks | MEDIUM | MEDIUM | `try_write()` 10ms timeout, non-blocking |
| Index corruption em crash | LOW | LOW | Tantivy auto-recovery + `force_commit()` |
| prost encode/decode compatibility | LOW | LOW | Pin versão específica, schema versionado |
| SCIP format evolving | LOW | MEDIUM | Pin SCIP schema version 0.3.x |

---

## Success Criteria

1. `touring search symbols "parse" --top 10 -j` retorna BM25 ranked results via Tantivy (<50ms P95)
2. `touring tantivy stats -j` mostra symbol count, doc count, index size
3. `touring tantivy reindex -j` processa todos os arquivos do projeto
4. `touring emit scip --out /tmp/test.scip` gera arquivo binário válido
5. `cargo test -p touring-hooks --features tantivy-fts` — 0 failures
6. `cargo test -p touring-server --features scip-emit` — 0 failures
7. Hooks post_edit/post_write alimentam Tantivy automaticamente quando feature ativa

---

## Timeline

```
S-1 (deps) ──────┐
T-1 (deps) ──┐   │
             ├──► T-2 (TantivyIndex) ──► T-3 (hooks wire) ──► T-4 (CLI+MCP) ──► T-5 (backfill)
             │                                                                        │
             └──► S-2 (scip_emit.rs) ──► S-3 (CLI+MCP) ──────────────────────────────┘
```

**Effort total**: ~3L + 3M + 2S = ~20-28h
**Parallelization**: T-2 e S-2 podem rodar em paralelo após deps
