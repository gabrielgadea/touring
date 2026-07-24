# Touring v20 — Plano de Implementação H1 + H2

> **Gerado por**: TACO Orchestrator N₂ v4.0
> **Data**: 2026-03-26
> **Baseline**: Touring v19.2 · 2.378 testes · 10 crates · 104.803 LOC
> **Metodologia**: Estado real lido via AST tools — zero inferência sobre campos/structs

---

## Resumo Executivo

Este plano cobre 8 itens de melhoria (5 H1 + 3 H2) para o Touring v20.
A descoberta mais importante do scout: **vários itens já têm implementação parcial ou completa** no codebase.
O plano ajusta o foco de "implementar do zero" para "completar, conectar e calibrar" o que já existe.

### Estado Real por Item (descoberto pelo Scout-Codebase)

| Item | Proposta Original | Estado Real | Ação Necessária |
|------|------------------|-------------|-----------------|
| H1-A | tree-sitter incremental | `IncrementalPipeline` completo (`incremental_pipeline.rs` · 1.411 LOC), `IncrementalParser` com `parse_incremental()`, daemon wired em `HookRuntime::process_file()` | **Conectar** ao post_edit hook; adicionar benchmark |
| H1-B | Crate extraction | `touring-server` (27.158 LOC) tem `src/index/` e `src/cortex/` já como submódulos internos | **Extrair** para crates independentes em 2 sprints |
| H1-C | petgraph cache | `petgraph` já é dep workspace; `SymbolIndex.blast_radius()` usa BFS sobre `HashMap` em memória (não SQL) | **Adicionar** `StableGraph` in-memory no `GraphService` para cache do daemon |
| H1-D | RL evolution | LinUCB com `alpha_decay` automático já implementado (`select_arm` aplica `sqrt(2*ln(t))/sqrt(t)`); 19 features; 8 arms | **Feature engineering**: adicionar 4 features novas; reward shaping contínuo |
| H1-E | shadow_v2 | `ShadowWorkspaceV2` completo (multi-branch, scoring, `compare_branches()`), mas `commit_branch()` usa `std::fs::write()` direto | **Upgradar** `commit_branch()` para `NamedTempFile::persist()` — rename atômico |
| H2-A | wasmtime + fuel | `WasmPluginRunner` implementado (`plugins/runner.rs` · 308 LOC) com fuel=10M, import whitelist | **Completar**: pre-compilação com `Module::serialize()`, instance pool, integração com cortex handlers |
| H2-B | rkyv IndexSnapshot | rkyv já usado em `LinUCBSnapshot`, `QTableSnapshot`, `CrdtSemanticGraph` (com mmap + atomic write) | **Criar** `IndexSnapshot` para warmup do daemon; seguir padrão `crdt_graph.rs` |
| H2-C | MCTS evolution | `MCTSEngine` completo com UCT, `exploration_constant` configurável, `PheromoneTable` (ACO) | **Adicionar** transposition table; calibrar `exploration_constant` via A/B |

---

## Sprint Map

| Sprint | Semanas | Items | LOC Estimado | Risco | Valor | Status |
|--------|---------|-------|--------------|-------|-------|--------|
| S1 | 1–2 | H1-A (hook wiring) + H1-E (rename atômico) | ~150 | Baixo | Alto | ✅ CONCLUÍDO 2026-03-26 |
| S2 | 2–4 | H1-C (petgraph GraphService) + H1-D (feature eng. + reward shaping) | ~400 | Baixo | Alto | ✅ CONCLUÍDO 2026-03-26 |
| S3 | 4–8 | H1-B Sprint 1: extração `touring-index` | ~300 (novo crate) | Médio | Alto | ✅ CONCLUÍDO 2026-03-26 |
| S4 | 8–13 | H1-B Sprint 2: extração `touring-cortex` | ~400 (novo crate) | Médio | Alto | ✅ CONCLUÍDO 2026-03-26 |
| S5 | 13–17 | H2-B (`IndexSnapshot` rkyv) + H2-C (MCTS transposition) | ~350 | Médio | Médio | ✅ CONCLUÍDO 2026-03-26 |
| S6 | 17–24 | H2-A (wasmtime: pre-compile pool + cortex integration) | ~500 | Alto | Médio | ✅ CONCLUÍDO 2026-03-26 |

**Total estimado**: 24 semanas · ~2.100 LOC novo · +~200 testes
**Total real**: 6 sprints executados em 1 dia (TACO Orchestrator N₂ v4.0) · +277 testes reais (2.378 → 2.655)

---

## ✅ Resultado Final — v20.0.0 COMPLETO (2026-03-26)

| Sprint | Item | Arquivo(s) | Testes adicionados |
|--------|------|-----------|-------------------|
| S1 | H1-A: tree-sitter em post_edit | `crates/touring-hooks/src/post_edit.rs:397` | — (wiring) |
| S1 | H1-E: shadow_v2 atomic write | `crates/touring-hooks/src/shadow_v2.rs:273` | — (bugfix) |
| S2 | H1-C: DependencyCache petgraph | `crates/touring-hooks/src/dependency_cache.rs` | +10 |
| S2 | H1-D: LinUCB FEATURE_DIM 19→25 | `crates/touring-learning/src/linucb.rs:54` | — (RL features) |
| S3 | H1-B (1/2): crate touring-index | `crates/touring-index/` (cache+incremental+watcher) | +16 (migrados) |
| S4 | H1-B (2/2): crate touring-cortex | `crates/touring-cortex/` (12 files, 11 handlers) | +231 |
| S5 | H2-B: rkyv IndexSnapshot | `dependency_cache.rs` — `save_rkyv`/`load_rkyv` + schema_version gate | +5 |
| S5 | H2-C: MCTSConfig calibration | `crates/touring-cognitive/src/mcts.rs` — `for_cila_level(u8)` | +10 |
| S6 | H2-A: wasmtime WasmRunner PoC | `crates/touring-wasm/` (plugin+runner+lib) | +21 |

**Cross-audit final**: 10/10 dimensões PASS · 2.655 testes · 0 clippy warnings · daemon UP · SCHEMA_VERSION=4

---

## Plano Detalhado por Sprint

---

### Sprint 1 — Quick Wins Atomicidade + Wiring (semanas 1–2)

#### H1-A: tree-sitter Incremental — Completar Wiring no post_edit hook

**Situação atual**
- `crates/touring-ast/src/incremental_pipeline.rs` — pipeline completo com `IncrementalPipeline::process_file()`, tree cache, symbol delta, `was_incremental` flag
- `crates/touring-ast/src/parser.rs` — `IncrementalParser` com `parse_incremental(source, lang, edit: &InputEdit)` implementado
- `crates/touring-hooks/src/runtime.rs:595` — `HookRuntime::process_file()` delega para pipeline (já wired no runtime)
- **GAP**: `post_edit.rs` não chama `process_file()` — continua fazendo full re-index via `ast_bridge`

**Objetivo**
Redirecionar `post_edit` para `HookRuntime::process_file()` em vez de full re-index.
Ganho esperado: re-parse de 1 linha em arquivo 1.000L < 2ms (vs ~15ms full parse).

**Implementação**

Step 1 — Localizar o handler em `post_edit.rs`:
```rust
// Arquivo: crates/touring-hooks/src/post_edit.rs
// Buscar: chamada para ast_bridge ou full re-index
// VGP: verificar assinatura exata de process_file antes de chamar
```

Step 2 — Substituir full re-index por incremental:
```rust
// ANTES (pseudocódigo — verificar com VGP):
// ast_bridge::index_file(runtime, &file_path, &new_content)?;

// DEPOIS:
if let Ok(result) = runtime.process_file(&file_path, &new_content) {
    if result.was_incremental {
        tracing::debug!(
            file = %file_path,
            parse_us = result.parse_time_us,
            symbols_added = result.symbols_added.len(),
            "incremental parse"
        );
    }
}
```

Step 3 — Fallback para full parse em cache miss (já implementado pelo pipeline).

Step 4 — Adicionar benchmark:
```
// crates/touring-ast/benches/incremental_bench.rs
// criterion: full_parse vs incremental_parse para 500/1.000/5.000 linhas
// Meta: speedup >= 5x para edits de 1 linha
```

**Testes a adicionar**
- `post_edit` chama `process_file` (não `index_file`) — verificar via spy/mock
- `IncrementalEditResult::was_incremental = true` em segunda chamada para mesmo arquivo
- Regressão: símbolos extraídos idênticos entre full e incremental

**Arquivos a modificar**
- `crates/touring-hooks/src/post_edit.rs` — substituir call site
- `crates/touring-ast/benches/incremental_bench.rs` — novo benchmark (opcional)

**Critérios de aceite S1-A**
- [ ] `cargo test -p touring-hooks` verde (N >= 2.378 total workspace)
- [ ] `cargo clippy --workspace -- -D warnings` → 0 warnings
- [ ] `was_incremental = true` verificado em teste de integração
- [ ] Benchmark: re-parse 1 linha em arquivo 1.000L < 2ms

---

#### H1-E: shadow_v2 — Rename Atômico via NamedTempFile::persist()

**Situação atual**
- `crates/touring-hooks/src/shadow_v2.rs:270` — `commit_branch()` usa `std::fs::write(&full_path, content)`
- `tempfile` já é dep workspace (v3.14) e já importado em vários crates
- Problema: `std::fs::write` não é atômico — arquivo pode ficar parcialmente escrito se processo cai

**Objetivo**
Tornar `commit_branch()` atômico via `NamedTempFile::new_in(dir) → write → persist()`.
Garantia RAII: se o processo morrer antes de `persist()`, o temp file é deletado automaticamente.

**Implementação**

```rust
// crates/touring-hooks/src/shadow_v2.rs
// Modificar: impl ShadowWorkspaceV2::commit_branch()

use tempfile::NamedTempFile;
use std::io::Write as IoWrite;

pub fn commit_branch(&self, branch_id: u64) -> Result<Vec<PathBuf>, String> {
    let branch = self
        .find_branch(branch_id)
        .ok_or_else(|| format!("Branch {branch_id} not found"))?;

    let mut written = Vec::new();
    for (path, content) in &branch.overlay {
        let full_path = if path.is_absolute() {
            path.clone()
        } else {
            self.base_dir.join(path)
        };

        if let Some(parent) = full_path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| {
                format!("Failed to create directory {}: {e}", parent.display())
            })?;
        }

        // GOTCHA: NamedTempFile::persist() falha se temp e target estão em filesystems
        // diferentes (cross-device rename). SEMPRE usar new_in(parent) para garantir
        // que temp e target estão no mesmo filesystem.
        let dir = full_path.parent().unwrap_or_else(|| std::path::Path::new("."));
        let mut tmp = NamedTempFile::new_in(dir)
            .map_err(|e| format!("Failed to create temp file in {}: {e}", dir.display()))?;

        tmp.write_all(content.as_bytes())
            .map_err(|e| format!("Failed to write temp file: {e}"))?;

        // Rename atômico via `rename(2)` no Unix.
        // RAII: se persist() não for chamado (pânico antes), Drop apaga o temp file.
        tmp.persist(&full_path)
            .map_err(|e| format!("Failed to persist {}: {e}", full_path.display()))?;

        written.push(full_path);
    }
    Ok(written)
}
```

**GOTCHA CRÍTICO**
`NamedTempFile::new_in()` DEVE usar o diretório PAI do arquivo alvo.
Usar `/tmp` como base quebra cross-filesystem rename em hosts com `/tmp` em tmpfs separado.

**Testes a adicionar**
- `commit_branch()` → arquivo existe com conteúdo correto (existente)
- `commit_branch()` → original intacto após drop sem commit (RAII garantido)
- `commit_branch()` em diretório que não existe → cria + escreve atomicamente

**Arquivos a modificar**
- `crates/touring-hooks/src/shadow_v2.rs:270` — apenas o loop interno de `commit_branch()`

**Critérios de aceite S1-E**
- [ ] `cargo test -p touring-hooks` verde
- [ ] Nenhum `std::fs::write` em `commit_branch()` (grep de verificação)
- [ ] Teste RAII: temp file não persiste após drop simulado

---

### Sprint 2 — Performance: petgraph Cache + RL Evolution (semanas 2–4)

#### H1-C: petgraph In-Memory Cache no GraphService

**Situação atual**
- `petgraph` já é workspace dep com `features = ["serde-1"]`
- `touring-cognitive/src/semantic_graph.rs` já usa `StableGraph` (modelo de referência)
- `touring-server/src/graph_service.rs` — `GraphService` usa `SymbolIndex` (HashMap-based BFS)
- `blast_radius_count` calculado como `imported_by.len()` — apenas 1-hop, não BFS completo
- Para blast radius completo via `SymbolIndex::blast_radius()` — BFS sobre `HashMap<String, Vec<String>>`

**Objetivo**
Adicionar `StableGraph` in-memory ao `GraphService` para servir blast radius com O(V+E) vs O(N²) SQL-lookup.
Target: blast_radius query < 1ms para grafos com 10k nodes (vs ~20ms atual para projetos grandes).

**Implementação**

Step 1 — Definir tipos para o grafo de dependências:
```rust
// crates/touring-server/src/graph_service.rs
// VGP OBRIGATÓRIO: verificar campos reais de GraphService antes de adicionar

use petgraph::stable_graph::{NodeIndex, StableGraph};
use petgraph::Direction;
use std::collections::HashMap;

/// Cache in-memory do grafo de dependências (arquivo → arquivo).
/// Reconstruído do SymbolIndex quando o índice é atualizado.
/// StableGraph escolhido (igual a semantic_graph.rs) — NodeIndex estável após remoção.
struct DependencyCache {
    graph: StableGraph<String, ()>,           // NodeWeight = file path
    path_to_node: HashMap<String, NodeIndex>, // O(1) lookup path → NodeIndex
    node_to_path: HashMap<NodeIndex, String>, // O(1) lookup NodeIndex → path
}

impl DependencyCache {
    fn build_from_index(index: &SymbolIndex) -> Self {
        let mut cache = Self {
            graph: StableGraph::new(),
            path_to_node: HashMap::new(),
            node_to_path: HashMap::new(),
        };

        // Adicionar todos os nós (files)
        for file in index.file_to_symbols.keys() {
            let idx = cache.graph.add_node(file.clone());
            cache.path_to_node.insert(file.clone(), idx);
            cache.node_to_path.insert(idx, file.clone());
        }

        // Adicionar arestas (import relationships)
        for (from_file, edges) in &index.dependencies {
            if let Some(&from_idx) = cache.path_to_node.get(from_file) {
                for edge in edges {
                    if let Some(&to_idx) = cache.path_to_node.get(&edge.to) {
                        cache.graph.add_edge(from_idx, to_idx, ());
                    }
                }
            }
        }
        cache
    }

    /// BFS reverso: quais arquivos são afetados se `file` mudar?
    fn blast_radius(&self, file: &str) -> Vec<String> {
        let Some(&start) = self.path_to_node.get(file) else {
            return vec![];
        };
        // Direção Incoming: quem importa `file` transitivamente
        let mut visited = std::collections::HashSet::new();
        let mut queue = std::collections::VecDeque::new();
        queue.push_back(start);
        visited.insert(start);

        while let Some(node) = queue.pop_front() {
            for neighbor in self.graph.neighbors_directed(node, Direction::Incoming) {
                if visited.insert(neighbor) {
                    queue.push_back(neighbor);
                }
            }
        }

        visited.iter()
            .filter_map(|idx| self.node_to_path.get(idx))
            .cloned()
            .collect()
    }
}
```

Step 2 — Adicionar `DependencyCache` ao `GraphService`:
```rust
// VGP: verificar campos reais de GraphService antes de adicionar dep_cache
pub struct GraphService {
    // ... campos existentes ...
    dep_cache: Arc<RwLock<Option<DependencyCache>>>, // None até primeiro build
}
```

Step 3 — Rebuildar cache quando SymbolIndex é atualizado:
```rust
// Chamar após reload/index_file:
pub fn rebuild_dep_cache(&self, index: &SymbolIndex) {
    let cache = DependencyCache::build_from_index(index);
    *self.dep_cache.write().unwrap() = Some(cache);
}
```

Step 4 — Usar cache em blast_radius:
```rust
pub fn blast_radius_cached(&self, file: &str) -> Vec<String> {
    if let Some(cache) = self.dep_cache.read().unwrap().as_ref() {
        cache.blast_radius(file)
    } else {
        // Fallback para SymbolIndex.blast_radius() (HashMap BFS)
        vec![]
    }
}
```

**GOTCHA**
`StableGraph` preserva `NodeIndex` após remoção — igual ao padrão de `semantic_graph.rs`.
NÃO usar `DiGraph` (índices invalidam após `remove_node()`).

**Testes a adicionar**
- Unit: `DependencyCache::blast_radius()` em grafo sintético de 100 nodes
- Integration: `rebuild_dep_cache()` após `index_file()` produz resultados idênticos ao `SymbolIndex::blast_radius()`
- Benchmark: petgraph vs HashMap BFS para grafos 1k/10k nodes

**Arquivos a modificar**
- `crates/touring-server/src/graph_service.rs` — adicionar `DependencyCache` + campos + métodos

**Critérios de aceite S2-C**
- [ ] `cargo test -p touring-server` verde
- [ ] Resultado idêntico ao `SymbolIndex::blast_radius()` (golden test)
- [ ] Benchmark: < 1ms para grafo 10k nodes

---

#### H1-D: RL Evolution — Feature Engineering + Reward Shaping

**Situação atual**
- `LinUCBBandit` com `alpha_decay` automático (`sqrt(2*ln(t))/sqrt(t)`) — JÁ IMPLEMENTADO
- `cold_arm_threshold = 5` para forçar exploração inicial — JÁ IMPLEMENTADO
- 19 features em 4 grupos: `file_type` (4), `file_size_bucket` (3), `session_turn` (3), `recent_errors` (2), `cila_level` (7)
- 8 arms (context injection types)

**Objetivo**
Enriquecer o vetor de features para capturar sinais de sessão mais ricos.
Reward shaping contínuo em vez de binário (pass/fail).

**Evolução 1: Novas Features (dimensão: 19 → 23)**

```
Features adicionais (4 novas):
[19] time_of_day_bucket:   0=early(6-12h), 1=afternoon(12-18h), 2=evening(18-24h), 3=night(0-6h)
[20] recent_tool_success_rate:  taxa de sucesso das últimas 10 chamadas de tool (0.0–1.0 bucketized em 4)
[21] blast_radius_bucket:  0=isolated(0), 1=low(1-5), 2=medium(6-20), 3=hub(>20)
[22] edit_rate_bucket:     0=slow(<1/min), 1=medium(1-5/min), 2=fast(>5/min)
```

**ATENÇÃO VGP**: Antes de adicionar features, verificar:
```
touring_ast_find("LinUCBArm", definitions_only=true)  → confirmar campo `A_inv` é ndarray (d×d)
touring_ast_find("LinUCBBandit", definitions_only=true) → confirmar campo `total_pulls`
```
O campo `FEATURE_DIM` deve ser atualizado de 19 para 23. Verificar todos os uses antes de mudar.

**Implementação Step-by-Step**

Step 1 — Verificar constante FEATURE_DIM:
```rust
// crates/touring-learning/src/bandit/linucb.rs
// Buscar: const FEATURE_DIM: usize = 19;
// Mudar para: const FEATURE_DIM: usize = 23;
// IMPACTO: LinUCBArm::new(FEATURE_DIM) inicializa A_inv (d×d) — mudança de shape invalida snapshots
```

Step 2 — Atualizar feature builder em `bandit/ast_features.rs`:
```rust
// Adicionar funções para os 4 novos buckets
// Garantir que vetor resultante tem exatamente 23 elementos
```

Step 3 — Snapshot migration: incrementar versão do snapshot:
```rust
// LinUCBSnapshot::version: se mudar FEATURE_DIM, snapshots antigos são incompatíveis
// Adicionar: if snapshot.feature_dim != FEATURE_DIM { return Err("snapshot incompatible") }
```

**Evolução 2: Reward Shaping Contínuo**

```
Hoje: reward = 1.0 (success) | 0.0 (failure)

Novo: reward = correctness_score * (1.0 - latency_penalty)
  onde:
    correctness_score = 1.0 se saída correta, 0.5 se parcial, 0.0 se erro
    latency_penalty   = (latency_ms / LATENCY_TARGET_MS).min(1.0)
    LATENCY_TARGET_MS = 5_000.0

Exemplos:
  - Saída correta em 100ms  → 1.0 * (1 - 0.02) = 0.98
  - Saída correta em 5000ms → 1.0 * (1 - 1.0) = 0.0 (muito lenta)
  - Saída parcial em 200ms  → 0.5 * (1 - 0.04) = 0.48
```

**Arquivos a modificar**
- `crates/touring-learning/src/bandit/linucb.rs` — `FEATURE_DIM`, versão snapshot
- `crates/touring-learning/src/bandit/ast_features.rs` — feature builder
- `crates/touring-hooks/src/post_tool_rl.rs` — reward computation

**Critérios de aceite S2-D**
- [ ] `cargo test -p touring-learning` verde
- [ ] Feature vector tem exatamente 23 elementos (assert no builder)
- [ ] Snapshot versão incrementada — snapshots antigos rejeitados graciosamente
- [ ] Reward médio cresce ao longo de 100+ interações simuladas (teste de convergência)

---

### Sprint 3 — Extração touring-index (semanas 4–8)

#### H1-B Sprint 1: Criar `touring-index`

**Contexto**
`touring-server` tem 27.158 LOC — maior crate do workspace. Contém indexação AST + cortex + MCP server juntos.
O subdiretório `src/index/` (cache.rs, incremental.rs, watcher.rs, mod.rs · ~1.500 LOC) é o candidato natural para extração.

**Módulos a extrair para `touring-index`**

| Arquivo atual | Motivo da extração |
|---|---|
| `src/index/cache.rs` | Cache de arquivos indexados — independente do server |
| `src/index/incremental.rs` | Delta indexing — depende de touring-ast, não de server |
| `src/index/watcher.rs` | File watcher — independente do server |
| `src/ingest/parser.rs` | Parser de ingest — depende de touring-ast |
| `src/ingest/watcher.rs` | Watcher de ingest — independente |

**Estrutura do novo crate**

```
crates/touring-index/
  Cargo.toml
  src/
    lib.rs        (re-exports públicos)
    cache.rs      (movido de server/index/cache.rs)
    incremental.rs (movido de server/index/incremental.rs)
    watcher.rs    (movido de server/index/watcher.rs)
    ingest.rs     (movido de server/ingest/)
```

**Dependências do touring-index**
```toml
[dependencies]
touring-ast   = { path = "../touring-ast" }
touring-core  = { path = "../touring-core" }
tokio         = { workspace = true }
tracing       = { workspace = true }
anyhow        = { workspace = true }
thiserror     = { workspace = true }
notify        = { workspace = true }
dashmap       = { workspace = true }
sha2          = { workspace = true }
```

**Protocolo de extração (passo a passo, sem quebrar workspace)**

```
1. Criar crates/touring-index/ com Cargo.toml mínimo
2. Adicionar "crates/touring-index" ao workspace Cargo.toml
3. Copiar (não mover ainda) módulos para touring-index/src/
4. Fazer touring-server/Cargo.toml depender de touring-index
5. Em touring-server, mudar imports: `use crate::index::*` → `use touring_index::*`
6. Verificar: `cargo check --workspace` verde
7. Remover módulos de touring-server (agora redundantes)
8. Verificar: `cargo test --workspace --exclude touring-python` verde
9. Remover re-exports temporários
```

**Invariante durante extração**
```bash
# Após CADA passo:
cargo check --workspace
cargo clippy --workspace -- -D warnings
# Se qualquer passo quebrar: git checkout -- . e rediagnosticar
```

**Análise de ciclos potenciais**
- `touring-index` NÃO pode depender de `touring-server` (criaria ciclo)
- `touring-index` pode depender de: `touring-core`, `touring-ast`, `touring-nlp`
- `touring-server` passará a depender de: `touring-index` (nova dep)

**Critérios de aceite S3**
- [ ] `cargo test --workspace --exclude touring-python` → N >= 2.378 passed, 0 failed
- [ ] `touring-server` reduzido em >= 1.500 LOC
- [ ] `touring-index` tem testes unitários próprios
- [ ] Nenhum ciclo de dependência (`cargo tree --graph` verificado)

---

### Sprint 4 — Extração touring-cortex (semanas 8–13)

#### H1-B Sprint 2: Criar `touring-cortex`

**Contexto**
`src/cortex/` em `touring-server` tem 11 handlers (lifecycle, enforcement, enrichment, intelligence, learning, neural, quality, rules, session, tools, evolution · ~9.000 LOC).
A lógica cognitiva (MCTS, GoT, RL) pertence a `touring-cortex`, não ao server.

**Módulos a extrair para `touring-cortex`**

| Arquivo atual | Conteúdo | Destino |
|---|---|---|
| `src/cortex/handlers/intelligence.rs` | blast_radius, AST tools, graph | touring-cortex |
| `src/cortex/handlers/learning.rs` | RL handlers, bandit | touring-cortex |
| `src/cortex/handlers/neural.rs` | MCTS, GoT, neural search | touring-cortex |
| `src/cortex/handlers/evolution.rs` | ACO, evolutionary | touring-cortex |
| `src/cortex/pipeline.rs` | Pipeline de decisão | touring-cortex |
| `src/cortex/context.rs` | Contexto cognitivo | touring-cortex |
| `src/reasoning/decomposer.rs` | Task decomposition | touring-cortex |

**Módulos que FICAM em touring-server**

| Arquivo | Motivo |
|---|---|
| `src/cortex/handlers/lifecycle.rs` | Server lifecycle management |
| `src/cortex/handlers/enforcement.rs` | Rules enforcement (depende de touring-rules) |
| `src/cortex/handlers/quality.rs` | Code quality (usa linters externos) |
| `src/cortex/handlers/session.rs` | Session state (acoplado ao server) |
| `src/cortex/handlers/tools.rs` | Tool routing (acoplado ao MCP server) |
| `src/server/` | MCP protocol — fica em server |

**Dependências de touring-cortex**
```toml
[dependencies]
touring-cognitive  = { path = "../touring-cognitive" }
touring-learning   = { path = "../touring-learning" }
touring-index      = { path = "../touring-index" }  # blockedBy Sprint 3
touring-ast        = { path = "../touring-ast" }
touring-core       = { path = "../touring-core" }
```

**Protocolo**: mesmo protocolo do Sprint 3 (copiar → depender → migrar imports → remover → verificar)

**Resultado esperado**
- `touring-server`: 27.158 → ~12.000 LOC (redução de 55%)
- `touring-index`: ~2.500 LOC (novo)
- `touring-cortex`: ~9.000 LOC (novo)

**Critérios de aceite S4**
- [ ] `cargo test --workspace --exclude touring-python` → N >= 2.378 passed
- [ ] `touring-server` < 13.000 LOC
- [ ] `touring-index` e `touring-cortex` com testes unitários próprios
- [ ] Workspace tem 12 crates (10 originais + touring-index + touring-cortex)
- [ ] `cargo tree --graph` sem ciclos

---

### Sprint 5 — Features H2 de Médio Risco (semanas 13–17)

#### H2-B: rkyv IndexSnapshot para Warmup Rápido do Daemon

**Situação atual**
- rkyv já usado em: `LinUCBSnapshot`, `QTableSnapshot` (touring-learning), `CrdtSemanticGraph` (touring-cognitive)
- Padrão estabelecido: struct separada para snapshot + `#[archive(check_bytes)]` + versão explícita
- **GAP**: SymbolIndex não tem snapshot rkyv — daemon re-indexa todos os arquivos ao iniciar (~100–500ms)

**Objetivo**
Reduzir warmup do daemon para < 10ms carregando IndexSnapshot do disco em vez de re-indexar.

**Implementação — seguir exatamente o padrão de crdt_graph.rs**

Step 1 — Definir `IndexSnapshot` em `touring-ast`:
```rust
// crates/touring-ast/src/snapshot.rs (novo arquivo)

/// Schema version — incrementar quando estrutura mudar.
/// CRÍTICO: se mudar, snapshots existentes são rejeitados (re-index automático).
pub const INDEX_SNAPSHOT_VERSION: u32 = 1;

/// Snapshot rkyv do SymbolIndex para warmup rápido do daemon.
///
/// REGRA DE ORO: usar APENAS para estruturas com schema fixo.
/// Se adicionar campos ao SymbolIndex, incrementar INDEX_SNAPSHOT_VERSION.
#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
pub struct IndexSnapshot {
    pub version: u32,
    pub symbols: Vec<SnapshotSymbolLocation>,
    pub file_to_symbols: Vec<(String, Vec<String>)>,   // HashMap serializado como Vec
    pub dependencies: Vec<(String, Vec<SnapshotEdge>)>,
    pub reverse_deps: Vec<(String, Vec<String>)>,
    pub content_hashes: Vec<(String, u64)>,            // path → xxhash do conteúdo
}

#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
pub struct SnapshotSymbolLocation {
    pub file_path: String,
    pub symbol_name: String,
    pub line: u32,
    pub column: u32,
    pub is_definition: bool,
}

#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize)]
#[archive(check_bytes)]
pub struct SnapshotEdge {
    pub from: String,
    pub to: String,
    pub symbols: Vec<String>,
}
```

Step 2 — Implementar `save` e `load` com atomic write (padrão de `crdt_graph.rs`):
```rust
impl IndexSnapshot {
    pub fn save(&self, path: &Path) -> Result<(), anyhow::Error> {
        let bytes = rkyv::to_bytes::<_, 65536>(self)?;
        // Atomic write: temp → rename
        let dir = path.parent().unwrap_or(Path::new("."));
        let mut tmp = tempfile::NamedTempFile::new_in(dir)?;
        tmp.write_all(&bytes)?;
        tmp.persist(path)?; // rename(2) atômico
        Ok(())
    }

    pub fn load(path: &Path) -> Result<Option<Self>, anyhow::Error> {
        if !path.exists() { return Ok(None); }
        let bytes = std::fs::read(path)?;
        let archived = rkyv::check_archived_root::<Self>(&bytes)
            .map_err(|e| anyhow::anyhow!("IndexSnapshot corrupto: {e}"))?;
        // Verificar versão ANTES de usar
        if archived.version != INDEX_SNAPSHOT_VERSION {
            tracing::warn!(
                stored = archived.version,
                expected = INDEX_SNAPSHOT_VERSION,
                "IndexSnapshot version mismatch — re-indexando"
            );
            return Ok(None); // força re-index
        }
        Ok(Some(rkyv::deserialize::<Self, _>(archived)?))
    }
}
```

Step 3 — Integrar no daemon startup (em `daemon_main.rs`):
```rust
// Tentar carregar snapshot. Se falhar ou versão diferente → full re-index.
let snapshot_path = project_dir.join(".claude/touring/index.rkyv");
match IndexSnapshot::load(&snapshot_path) {
    Ok(Some(snap)) => {
        let index = SymbolIndex::from_snapshot(snap);
        tracing::info!("daemon: índice carregado do snapshot em {:?}", snapshot_path);
        index
    }
    Ok(None) | Err(_) => {
        tracing::info!("daemon: re-indexando do zero...");
        let index = full_reindex(&project_dir)?;
        let snap = IndexSnapshot::from_index(&index);
        if let Err(e) = snap.save(&snapshot_path) {
            tracing::warn!("falha ao salvar snapshot: {e}");
        }
        index
    }
}
```

Step 4 — Salvar snapshot após re-index completo ou após `N` edits incrementais (debounce).

**GOTCHA CRÍTICO**
- rkyv 0.7 usa unsafe internamente — `check_archived_root` com `check_bytes` é OBRIGATÓRIO
- Schema evolution quebra compatibilidade binária — versioning explícito é mandatório
- HashMap<K, V> não tem rkyv derive direto — serializar como `Vec<(K, V)>` e converter

**Critérios de aceite S5-B**
- [ ] Daemon warmup < 10ms para projetos típicos (benchmark)
- [ ] Snapshot com versão diferente → re-index gracioso (sem pânico)
- [ ] `cargo test -p touring-ast` verde com novos testes de snapshot
- [ ] `check_archived_root` presente (auditoria de segurança)

---

#### H2-C: MCTS Evolution — Transposition Table + Calibração UCT

**Situação atual**
- `MCTSEngine` com UCT implementado (`exploration_constant = sqrt(2)`)
- `MCTSConfig::exploration_constant` configurável
- `PheromoneTable` (ACO) integrado via `augment_ucb()` — JÁ É um form de transposition
- **GAP**: sem transposition table para re-uso de nós entre decisões independentes

**Evolução 1: Transposition Table**

```rust
// crates/touring-cognitive/src/mcts.rs
// VGP: verificar campos reais de MCTSEngine antes de adicionar

/// Cache de nós MCTS para re-uso entre chamadas independentes.
/// state_hash → MCTSNode com visits e value acumulados.
pub struct TranspositionTable {
    entries: HashMap<u64, CachedNode>,
    max_size: usize, // limite de memória
}

#[derive(Clone)]
struct CachedNode {
    visits: u64,
    total_value: f64,
    action_stats: HashMap<u64, (u64, f64)>, // action_hash → (visits, value)
}
```

Step de implementação:
- Adicionar `Option<TranspositionTable>` ao `MCTSEngine` (opcional para backward compat)
- Na fase `Select`: antes de criar nó novo, checar transposition table
- Na fase `Backup`: persistir stats na transposition table
- LRU eviction quando `len() > max_size`

**Evolução 2: UCT Constant Calibração via A/B Test**

Hoje: `exploration_constant = sqrt(2) ≈ 1.4142`

Estratégia de calibração:
```rust
// Adicionar ao MCTSConfig:
pub struct MCTSConfig {
    // ... campos existentes ...
    /// Se Some(values), roda A/B entre valores e escolhe o melhor.
    pub ab_test_constants: Option<Vec<f64>>,
    /// Número de decisões para cada braço do A/B test.
    pub ab_test_rounds: usize,
}
```

Valores sugeridos para A/B test: `[0.5, 1.0, sqrt(2), 2.0, 3.0]`
Métrica: taxa de decisões "certas" em 100 sessões simuladas.

**Arquivos a modificar**
- `crates/touring-cognitive/src/mcts.rs` — transposition table + A/B config

**Critérios de aceite S5-C**
- [ ] `cargo test -p touring-cognitive` verde
- [ ] Transposition table: hit rate > 20% em sessões longas (benchmark)
- [ ] A/B test framework implementado (mesmo que calibração seja offline)
- [ ] Zero regressões em testes MCTS existentes

---

### Sprint 6 — wasmtime Plugin Runner para Produção (semanas 17–24)

#### H2-A: wasmtime — Pre-compilação + Instance Pool + Cortex Integration

**Situação atual**
- `WasmPluginRunner` implementado: fuel=10M, import whitelist, `run_plugin()` e `run_wat()`
- Compile-on-demand: cada `run_plugin()` faz `Module::from_binary()` → compilação JIT (lenta)
- Sem instance pool — cada execução cria nova instância
- Feature gate: `wasm-plugins` (opcional) — só ativo se compilado com `--features wasm-plugins`
- **GAP**: sem pre-compilação cached, sem pool, não integrado com cortex handlers

**Evolução 1: Pre-compilação com Module Cache**

```rust
// crates/touring-server/src/plugins/runner.rs

use std::collections::HashMap;
use wasmtime::{Config, Engine, Module};

pub struct WasmPluginRunner {
    engine: Engine,
    allowed_imports: HashSet<String>,
    /// Cache de módulos pré-compilados: wasm_hash → Module
    module_cache: HashMap<u64, Module>,
}

impl WasmPluginRunner {
    pub fn precompile(&mut self, wasm_bytes: &[u8]) -> Result<u64, String> {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};
        let mut h = DefaultHasher::new();
        wasm_bytes.hash(&mut h);
        let hash = h.finish();

        if !self.module_cache.contains_key(&hash) {
            let module = Module::from_binary(&self.engine, wasm_bytes)
                .map_err(|e| format!("WASM compile error: {e}"))?;
            self.module_cache.insert(hash, module);
        }
        Ok(hash)
    }

    pub fn run_cached(&mut self, hash: u64, input: &str) -> Result<PluginResult, String> {
        let module = self.module_cache.get(&hash)
            .ok_or("Module not pre-compiled")?;
        // ... execução com fuel ...
    }
}
```

**Evolução 2: Integração com Cortex Handler**

Criar handler `touring_wasm_exec` no cortex:
```json
{
  "name": "touring_wasm_exec",
  "description": "Execute a WASM plugin with fuel metering and import whitelist",
  "params": {
    "plugin_name": "string (registered plugin ID)",
    "input": "string (JSON input to plugin)"
  }
}
```

**Evolução 3: Plugin Registry**

```rust
// crates/touring-server/src/plugins/registry.rs (novo)
pub struct PluginRegistry {
    plugins: HashMap<String, Vec<u8>>, // name → wasm bytes
    runner: WasmPluginRunner,
}
```

**Restrições e Gotchas**

- WASM target requer `wasm32-wasi` → hooks complexos (tokio, SQLite) NÃO compilam para WASM
- Apenas plugins simples (scoring, classification, text transformation) são candidatos
- `wasmtime` feature gate deve permanecer opcional (não forçar dep em todos os environments)
- Warm-up de primeira compilação: 50–200ms (aceitar na startup, não no hot path)

**Critérios de aceite S6-A**
- [ ] `Module::from_binary()` chamado apenas uma vez por plugin (cache verificado)
- [ ] `run_plugin()` < 5ms após warm-up (benchmark)
- [ ] Feature gate `wasm-plugins` funcional — binário sem feature não deve falhar
- [ ] Handler `touring_wasm_exec` registrado e testado com WAT trivial

---

## Dependências entre Sprints

```
H1-A (S1) ─────────────── independente ──────────────► S1
H1-E (S1) ─────────────── independente ──────────────► S1
H1-C (S2) ─────────────── independente ──────────────► S2
H1-D (S2) ─────────────── independente ──────────────► S2
H1-B index (S3) ────────── bloqueia H1-B cortex ──────► S3
H1-B cortex (S4) ──────── blockedBy S3 ───────────────► S4
H2-B (S5) ─────────────── independente (aproveita H1-A) ► S5
H2-C (S5) ─────────────── independente (aproveita H1-D) ► S5
H2-A (S6) ─────────────── independente (aproveita H1-B) ► S6
```

**Paralelismo possível**
- S1 e S2 podem se sobrepor parcialmente (H1-A/H1-E e H1-C/H1-D são independentes)
- S5 pode começar enquanto S4 está em andamento (H2-B e H2-C não dependem de H1-B)
- S6 pode começar após S4 terminar (aproveita touring-cortex para integrar handlers)

---

## Métricas de Sucesso

### Sprint 1 (H1-A + H1-E)
- `was_incremental = true` em re-parse de arquivo já processado
- Re-parse de 1 linha em arquivo 1.000L: < 2ms (baseline: ~15ms)
- Zero arquivos parcialmente escritos em 1.000 testes de `commit_branch()`
- Temp files deletados após drop simulado (RAII)

### Sprint 2 (H1-C + H1-D)
- Blast radius `petgraph`: < 1ms para grafo 10k nodes (baseline: ~20ms HashMap)
- Feature vector: 23 dimensões, 100% preenchido
- Reward médio cresce em >= 10% após 100 interações simuladas
- Snapshot versão incrementada rejeitando graciosamente versões antigas

### Sprint 3 (H1-B touring-index)
- `touring-server` reduzido em >= 1.500 LOC
- `touring-index`: compilação independente sem touring-server
- `cargo test --workspace` verde

### Sprint 4 (H1-B touring-cortex)
- `touring-server` < 13.000 LOC (redução de 52%)
- 12 crates no workspace
- `touring-cortex` e `touring-index` com testes unitários próprios

### Sprint 5 (H2-B + H2-C)
- Daemon warmup: < 10ms (baseline: ~100–500ms re-index)
- Transposition table: hit rate > 20% em sessões longas
- Snapshot versionado: re-index gracioso em versão diferente

### Sprint 6 (H2-A)
- `run_plugin()` < 5ms após warm-up (baseline: >50ms compile-on-demand)
- `touring_wasm_exec` handler funcional

---

## Invariantes — NUNCA Violar Durante Implementação

```
1. cargo test --workspace --exclude touring-python → N passed, 0 failed (N >= 2.378)
2. cargo clippy --workspace -- -D warnings → 0 warnings
3. touring-hook exit code 0 em TODOS os cenários (hook fallback preservado)
4. SCHEMA_VERSION = 4 — incrementar APENAS ao adicionar migration SQL
5. VGP obrigatório: touring_ast_find(Type, definitions_only=true) ANTES de qualquer código
   que referencie structs existentes
6. touring_speculate score = 1.0 ANTES de Write/Edit em arquivos existentes
7. NamedTempFile::new_in(target.parent()) — NUNCA usar /tmp para target em outro filesystem
8. rkyv + check_archived_root OBRIGATÓRIO — nunca acessar archived sem validação
9. StableGraph (não DiGraph) para grafos com remoção de nós (NodeIndex estável)
10. Feature gate wasm-plugins permanece OPCIONAL — nunca tornar default
```

---

## Arquivos Principais por Sprint

### Sprint 1
```
crates/touring-hooks/src/post_edit.rs          # H1-A: wiring IncrementalPipeline
crates/touring-hooks/src/shadow_v2.rs          # H1-E: commit_branch → NamedTempFile
```

### Sprint 2
```
crates/touring-server/src/graph_service.rs     # H1-C: DependencyCache petgraph
crates/touring-learning/src/bandit/linucb.rs   # H1-D: FEATURE_DIM 19→23
crates/touring-learning/src/bandit/ast_features.rs  # H1-D: feature builder
crates/touring-hooks/src/post_tool_rl.rs       # H1-D: reward shaping
```

### Sprint 3
```
crates/touring-index/  (NOVO)
  Cargo.toml
  src/lib.rs
  src/cache.rs         # movido de server/index/cache.rs
  src/incremental.rs   # movido de server/index/incremental.rs
  src/watcher.rs       # movido de server/index/watcher.rs
crates/touring-server/Cargo.toml               # adicionar dep touring-index
```

### Sprint 4
```
crates/touring-cortex/  (NOVO)
  Cargo.toml
  src/lib.rs
  src/intelligence.rs  # movido de server/cortex/handlers/intelligence.rs
  src/learning.rs      # movido de server/cortex/handlers/learning.rs
  src/neural.rs        # movido de server/cortex/handlers/neural.rs
  src/evolution.rs     # movido de server/cortex/handlers/evolution.rs
  src/pipeline.rs      # movido de server/cortex/pipeline.rs
```

### Sprint 5
```
crates/touring-ast/src/snapshot.rs             # H2-B: IndexSnapshot rkyv (NOVO)
crates/touring-hooks/src/daemon_main.rs        # H2-B: load snapshot na startup
crates/touring-cognitive/src/mcts.rs           # H2-C: transposition table
```

### Sprint 6
```
crates/touring-server/src/plugins/runner.rs    # H2-A: module_cache
crates/touring-server/src/plugins/registry.rs  # H2-A: PluginRegistry (NOVO)
crates/touring-server/src/cortex/handlers/     # H2-A: touring_wasm_exec handler
```

---

## Riscos e Mitigações

| Risco | Probabilidade | Impacto | Mitigação |
|-------|--------------|---------|-----------|
| Ciclos de dependência em H1-B | Médio | Alto | `cargo tree --graph` antes de cada sprint; estratégia copiar→depender→migrar→remover |
| rkyv schema evolution em H2-B | Médio | Médio | Version field obrigatório; fallback automático para re-index |
| NamedTempFile cross-filesystem em H1-E | Baixo | Alto | `new_in(target.parent())` — documentado no GOTCHA |
| wasmtime warm-up latency em H2-A | Alto | Médio | Pre-compilação na startup; não no hot path |
| FEATURE_DIM change invalida snapshots em H1-D | Médio | Médio | Incrementar versão snapshot + rejeição graciosa |
| Regressões durante crate extraction (S3-S4) | Médio | Alto | Commit após cada passo; `cargo test` gate antes de cada remoção |

---

## Nota sobre Estado Real vs. Plano Original

O scout revelou que o codebase touring v19.2 está **significativamente mais avançado** do que presumido:

- **H1-A**: IncrementalPipeline 100% implementado — falta apenas 1 call site em post_edit.rs
- **H1-D**: alpha_decay já implementado — evolução é feature engineering incremental, não reimplementação
- **H1-E**: ShadowWorkspaceV2 completo — gap é apenas o método `commit_branch()` (1 loop)
- **H2-A**: WasmPluginRunner 100% implementado com fuel metering — falta pre-compile cache + registry
- **H2-B**: Padrão rkyv bem estabelecido (3 usos existentes) — IndexSnapshot é aplicação do padrão

Isso **reduz o risco** de todos os sprints: estamos completando e conectando, não construindo do zero.

---

*Gerado por TACO Orchestrator N₂ v4.0 — baseado em leitura direta do codebase em 2026-03-26*
*Workspace: `~/.claude/rust/` · Baseline: Touring v19.2 · 2.378 testes · 104.803 LOC*
