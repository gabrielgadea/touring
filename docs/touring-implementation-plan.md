# Plano de Implementação Touring v22.x

**Gerado por:** TACO Orchestrator N2 v4.0
**Data:** 26/03/2026
**Baseline:** v21.1.0 · 13 crates · 2.671 testes · SCHEMA_VERSION=4
**Target:** v22.x com todas as melhorias do touring-pro.md
**Horizonte:** 8 sprints (~3-4 meses para S0-S6, +2-3 meses para S7-S8)

---

## Invariantes — NUNCA violar durante implementação

| Invariante | Gate |
|---|---|
| **Exit 0** | Nunca modificar comportamento de saída dos hooks — fallback sempre preservado |
| **Clippy** | `cargo clippy --workspace -- -D warnings` → 0 warnings (deny all = erro de compilação) |
| **Tests** | `cargo test --workspace --exclude touring-python` → N passed, 0 failed (N cresce a cada sprint) |
| **No unwrap** | Apenas `?`, `.expect("razão")`, `.unwrap_or_default()` em código de produção |
| **Schema gate** | `SCHEMA_VERSION=4` em `touring-core::migration` — importar via `use touring_core::migration::SCHEMA_VERSION` |
| **API pública** | `DependencyCache::blast_radius(&PathBuf) -> Vec<PathBuf>` é contrato do daemon — não quebrar |
| **Daemon rebuild** | Após modificar `crates/touring-hooks/` → rebuild touring-hooks + pkill + restart daemon |

### Gate de Validação por Sprint (executar antes de marcar DONE)

```
□ cargo check --workspace                                    → 0 errors
□ cargo clippy --workspace -- -D warnings                   → 0 warnings
□ cargo test --workspace --exclude touring-python            → N passed, 0 failed
□ Zero unwrap() novo em código de produção
□ Docs atualizadas se interface pública mudou
□ API pública existente não quebrada
```

---

## Mapa de Dependências entre Sprints

```
S0 (deps wasmtime + deadpool-sqlite)
 ├─→ S1.1 (H2.4 PoolingAllocationStrategy) — precisa feature wasmtime
 ├─→ S1.2 (H2.5 tarjan_scc)               — independente (petgraph já presente)
 ├─→ S1.3 (H1.3 ReminderBandit)            — independente (LinUcb já presente)
 └─→ S3   (H1.2 GoTSnapshot)               — precisa deadpool-sqlite

S1.1 + S1.2 + S1.3 → S2 (H2.1 GoT Actors) — consolidação antes de paralelismo
S0 + S2 → S3 (H1.2 GoTSnapshot) — GoT state precisa ser estável
S3 → S4 (H2.4 InferletPool completo) — sessões informam design async do pool
S4 + S5 → S6 (H2.3 CRDTs P2P) — entidades estáveis antes de P2P
S6 → S7 (H3.3 eBPF) — telemetria monitora estado convergente
S7 → S8 (H3.1/H3.2 BranchFS) — pesquisa, consome experiência de S7
```

---

## Sprint 0 — Preparação de Dependências (< 1 dia)

**Objetivo:** Adicionar dependências ausentes ao workspace sem quebrar build.
**Entregável:** `cargo check --workspace` verde com novas deps. Testes: 2.671 (sem mudança).

### S0.1 — deadpool-sqlite ao workspace

**Arquivo:** `/home/gabrielgadea/.claude/rust/Cargo.toml`
**Seção:** `[workspace.dependencies]` após `rusqlite`

```toml
# Async SQLite connection pool (compatible with rusqlite 0.32 bundled)
deadpool-sqlite = "0.9"
```

**Verificação de compatibilidade:** deadpool-sqlite 0.9 usa rusqlite ^0.31 — compatível com 0.32.

### S0.2 — Features async + pooling-allocator no wasmtime

**Arquivo:** `/home/gabrielgadea/.claude/rust/Cargo.toml`
**Linha atual:** `wasmtime = { version = "42", default-features = false, features = ["cranelift", "runtime"] }`
**Substituir por:**

```toml
wasmtime = { version = "42", default-features = false, features = [
    "cranelift", "runtime", "async", "pooling-allocator"
] }
```

**Nota de risco:** `pooling-allocator` aloca memória virtual para N slots antecipadamente (~100 × wasm linear memory limit). Em ambientes com <2GB de virtual address space, reduzir para 10 instâncias. Testar antes de comitar.

### S0.3 — wit-bindgen (feature-gated, apenas touring-wasm)

**Arquivo:** `/home/gabrielgadea/.claude/rust/Cargo.toml`
**Adicionar:**

```toml
# WIT interface generator for WASM component model (feature-gated)
wit-bindgen = { version = "0.36", optional = true }
```

**Nota:** Adicionar como dep opcional — não quebra builds que não ativem a feature.

```
□ cargo check --workspace → 0 errors
□ cargo test --workspace --exclude touring-python → 2.671 passed
```

---

## Sprint 1 — Quick Wins (3-5 dias)

**Objetivo:** 3 features independentes de alto valor / baixo risco.
**Entregável:** H2.4 parcial + H2.5 completo + H1.3 completo. Meta: ~2.686 testes.

---

### S1.1 — H2.4: PoolingAllocationStrategy no WasmRunner (1 dia)

**Classificação:** L2 (Otimização — sem mudança de interface)
**Arquivo:** `crates/touring-wasm/src/runner.rs`
**Ponto exato:** Função `WasmRunner::new()`, linhas 60-66
**Mudança:** Adicionar 2 linhas ao `Config::new()` antes de `Engine::new()`

**Código atual (linhas 61-66):**
```rust
let mut config = Config::new();
config.consume_fuel(true);
config.max_wasm_stack(MAX_STACK_SIZE);

let engine = Engine::new(&config)
    .map_err(|e| format!("Failed to create WASM engine: {e}"))?;
```

**Código novo:**
```rust
let mut config = Config::new();
config.consume_fuel(true);
config.max_wasm_stack(MAX_STACK_SIZE);
config.async_support(true);
config.allocation_strategy(InstanceAllocationStrategy::pooling());

let engine = Engine::new(&config)
    .map_err(|e| format!("Failed to create WASM engine: {e}"))?;
```

**Import a adicionar:**
```rust
use wasmtime::{Config, Engine, Instance, InstanceAllocationStrategy, Module, Store};
```

**Testes a adicionar** (no módulo `#[cfg(test)]` existente em runner.rs):
```rust
#[test]
fn test_runner_uses_pooling_allocator() {
    // WasmRunner::new() must succeed with pooling allocator enabled
    let runner = WasmRunner::new();
    assert!(runner.is_ok(), "Pooling allocator must not break engine creation");
    // Verify fuel still works with pooling
    let module = runner.unwrap().load_wat(
        r#"(module (func (export "evaluate") (result i32) i32.const 1))"#
    ).expect("load");
    let result = module.call_evaluate(&PluginContext::new("x")).expect("eval");
    assert!(result.success);
    assert!(result.fuel_consumed > 0);
}
```

**Nota importante:** `call_evaluate` continua síncrono neste sprint. Async será adicionado no S4 com `InferletPool`. Esta mudança apenas habilita pooling na engine, que já melhora performance de criação de `Store`.

**Gate:**
```
□ cargo check --workspace → 0 errors
□ cargo clippy --workspace -- -D warnings → 0 warnings
□ cargo test --workspace --exclude touring-python → ≥ 2.672 passed
```

---

### S1.2 — H2.5: tarjan_scc + detect_cycles em DependencyCache (2-3 dias)

**Classificação:** L3 (Refactoring — adiciona API, mantém comportamento existente)
**Arquivo:** `crates/touring-hooks/src/dependency_cache.rs`
**Ponto exato:** Após linha 178 (fim do impl DependencyCache) — nova seção de métodos

**Imports a adicionar** (no topo do arquivo, após os imports existentes):
```rust
use petgraph::algo::tarjan_scc;
use petgraph::visit::Topo;
```

**Métodos a adicionar ao `impl DependencyCache` (após `get_or_insert`):**

```rust
/// Detect circular dependencies using Tarjan's SCC algorithm.
///
/// Returns groups of files that form dependency cycles.
/// Each inner Vec contains the paths of files in a cycle.
/// Empty result means the graph is a DAG (no cycles).
///
/// O(V+E) — Tarjan's algorithm visits each node and edge once.
pub fn detect_cycles(&self) -> Vec<Vec<PathBuf>> {
    tarjan_scc(&self.graph)
        .into_iter()
        .filter(|scc| scc.len() > 1)
        .map(|scc| {
            scc.iter()
                .filter_map(|&n| self.graph.node_weight(n).cloned())
                .collect()
        })
        .collect()
}

/// Return the count of strongly connected components (SCCs).
///
/// A DAG has `node_count` SCCs (all trivial). Fewer SCCs = more cycles.
pub fn scc_count(&self) -> usize {
    tarjan_scc(&self.graph).len()
}

/// Topological traversal order of all nodes.
///
/// Returns files in dependency order (dependencies first).
/// Returns an error if the graph contains cycles (not a DAG).
/// For graphs with cycles, use `detect_cycles()` first.
pub fn topological_order(&self) -> Result<Vec<PathBuf>, String> {
    let mut topo = Topo::new(&self.graph);
    let mut result = Vec::new();
    while let Some(nx) = topo.next(&self.graph) {
        if let Some(p) = self.graph.node_weight(nx) {
            result.push(p.clone());
        }
    }
    if result.len() != self.graph.node_count() {
        return Err("Graph contains cycles — topological order undefined".to_string());
    }
    Ok(result)
}
```

**Testes a adicionar** (no módulo de testes existente):
```rust
#[test]
fn test_detect_cycles_dag_has_no_cycles() {
    let mut cache = DependencyCache::new();
    cache.add_relation(&p("a.rs"), &p("b.rs"));
    cache.add_relation(&p("b.rs"), &p("c.rs"));
    assert!(cache.detect_cycles().is_empty(), "Linear chain has no cycles");
}

#[test]
fn test_detect_cycles_finds_cycle() {
    let mut cache = DependencyCache::new();
    cache.add_relation(&p("a.rs"), &p("b.rs"));
    cache.add_relation(&p("b.rs"), &p("c.rs"));
    cache.add_relation(&p("c.rs"), &p("a.rs")); // cycle: a→b→c→a
    let cycles = cache.detect_cycles();
    assert!(!cycles.is_empty(), "Must detect the a→b→c→a cycle");
    assert_eq!(cycles.len(), 1);
    assert_eq!(cycles[0].len(), 3);
}

#[test]
fn test_topological_order_linear_chain() {
    let mut cache = DependencyCache::new();
    // a→b→c: c is depended upon first
    cache.add_relation(&p("a.rs"), &p("b.rs"));
    cache.add_relation(&p("b.rs"), &p("c.rs"));
    let order = cache.topological_order().expect("linear chain is a DAG");
    // c must come before b, b before a (topo order: dependencies first)
    let pos_a = order.iter().position(|x| x == &p("a.rs")).unwrap();
    let pos_b = order.iter().position(|x| x == &p("b.rs")).unwrap();
    let pos_c = order.iter().position(|x| x == &p("c.rs")).unwrap();
    assert!(pos_c < pos_b, "c must precede b in topo order");
    assert!(pos_b < pos_a, "b must precede a in topo order");
}

#[test]
fn test_scc_count_dag() {
    let mut cache = DependencyCache::new();
    cache.add_relation(&p("a.rs"), &p("b.rs"));
    cache.add_relation(&p("b.rs"), &p("c.rs"));
    // Pure DAG: each node is its own SCC
    assert_eq!(cache.scc_count(), 3);
}

#[test]
fn test_scc_count_with_cycle() {
    let mut cache = DependencyCache::new();
    cache.add_relation(&p("a.rs"), &p("b.rs"));
    cache.add_relation(&p("b.rs"), &p("a.rs")); // 2-cycle
    // One SCC with 2 nodes
    assert_eq!(cache.scc_count(), 1);
}
```

**Gate:**
```
□ cargo check --workspace → 0 errors
□ cargo clippy --workspace -- -D warnings → 0 warnings
□ cargo test --workspace --exclude touring-python → ≥ 2.677 passed
□ API blast_radius() inalterada — testes existentes ainda passam
```

---

### S1.3 — H1.3: ReminderBandit via LinUCB (2-3 dias)

**Classificação:** L4 (Arquitetura — novo componente, nova integração)
**Arquivos a criar:** `crates/touring-learning/src/bandit/reminder_bandit.rs`
**Arquivo a modificar:** `crates/touring-learning/src/bandit/mod.rs` (re-exportar)

**Atenção:** O `LinUcb` existente em `linucb.rs` é focado em **context injection** (8 arms: None, Overview, Gotcha, etc.). O `ReminderBandit` é um **caso de uso diferente** — selecionar qual reminder mostrar ao usuário. Criar instância LinUcb **separada** com seus próprios N arms.

**Verificação prévia obrigatória:** Antes de escrever o código, verificar se `LinUcb` (struct pai) aceita N arms configurável ou se é hardcoded em 8. Se hardcoded: criar `ReminderLinUcb` própria com `REMINDER_ARMS` arms. Se parametrizável: usar `LinUcb::new(n_arms, feature_dim, alpha)`.

**Ponto de integração:**
- `LinUcb` está em: `crates/touring-learning/src/bandit/linucb.rs`
- `ReminderBandit` a criar em: `crates/touring-learning/src/bandit/reminder_bandit.rs`
- Feature vector do ReminderBandit: **4 dimensões** (tarefa, taxa de erro, fill de contexto, tempo decorrido) — menor e mais direto que o de 25 dims do context injection

**Estrutura de ReminderBandit:**
```rust
// crates/touring-learning/src/bandit/reminder_bandit.rs

/// Pool of candidate system reminders.
#[derive(Debug, Clone)]
pub struct ReminderTemplate {
    pub id: usize,
    pub name: String,
    pub content: String,
}

/// Feature vector for reminder selection (4 dimensions).
pub struct ReminderContext {
    /// Task type encoded as 0.0=read, 0.33=write, 0.66=refactor, 1.0=debug
    pub task_type_normalized: f64,
    /// Error rate in current session (0.0–1.0)
    pub session_error_rate: f64,
    /// Context fill ratio (0.0 = empty, 1.0 = full)
    pub context_fill: f64,
    /// Elapsed time in session normalized (0.0–1.0, capped at 60min)
    pub elapsed_normalized: f64,
}

impl ReminderContext {
    pub fn feature_vector(&self) -> [f64; 4] {
        [
            self.task_type_normalized,
            self.session_error_rate,
            self.context_fill,
            self.elapsed_normalized,
        ]
    }
}

/// LinUCB-powered system reminder selector.
///
/// Maintains per-template ridge regression models.
/// Learns which reminder reduces correction rate for each context.
pub struct ReminderBandit {
    // NOTE: Uses separate LinUcb instance from context-injection LinUcb.
    // Verify LinUcb accepts variable n_arms before implementing.
    // If LinUcb is hardcoded to 8 arms, replicate arms logic here directly.
    arms: Vec<ReminderArm>,
    alpha: f64,
    feature_dim: usize,
    candidates: Vec<ReminderTemplate>,
}

/// Internal per-reminder arm state.
struct ReminderArm {
    a_inv: ndarray::Array2<f64>, // d x d inverse design matrix
    b: ndarray::Array1<f64>,     // d reward-weighted features
    pulls: u64,
}

impl ReminderBandit {
    pub fn new(candidates: Vec<ReminderTemplate>, alpha: f64) -> Self { ... }
    pub fn select(&mut self, ctx: &ReminderContext) -> &ReminderTemplate { ... }
    pub fn reward(&mut self, arm_idx: usize, correction_occurred: bool) {
        let r = if correction_occurred { -0.5 } else { 1.0 };
        // Sherman-Morrison update on arm[arm_idx]
    }
}
```

**Testes obrigatórios:**
```rust
#[test]
fn test_reminder_bandit_selects_valid_arm()
fn test_reminder_bandit_reward_updates_arm()
fn test_reminder_bandit_explores_all_arms_early() // UCB exploration
fn test_reminder_context_feature_vector_dimensions()
```

**Integração com prompt-enhancer hook:**
O prompt enhancer está em `~/.claude/hooks/prompt_enhancer.py` (Python). A integração com Rust é via IPC/CLI — o daemon expõe o ReminderBandit via MCP tool ou via stdin/stdout. Para o Sprint 1 apenas criar a struct + testes. A integração com o hook Python fica para Sprint posterior.

**Gate:**
```
□ cargo check --workspace → 0 errors
□ cargo clippy --workspace -- -D warnings → 0 warnings
□ cargo test --workspace --exclude touring-python → ≥ 2.682 passed
□ Módulo exportado em bandit/mod.rs
```

---

## Sprint 2 — GoT Parallel Actors (1-2 semanas)

**Objetivo:** H2.1 — Execução paralela de hipóteses no Graph of Thoughts via JoinSet.
**Classificação:** L4 (Arquitetura — novo componente, integração com GoT existente)
**Arquivo a criar:** `crates/touring-cognitive/src/got_actor.rs`
**Arquivo a modificar:** `crates/touring-cognitive/src/lib.rs` (re-exportar)
**Meta de testes:** ~2.706

### Ponto de integração exato

`got.rs` contém a nota: *"True parallelism requires `tokio::spawn` with `'static` bounds on the engine."*

- `GotNode`: tem `#[derive(Debug)]` mas **não** `Clone`. Usar `Arc<GotNode>`.
- `ThoughtMessage`: tem `#[derive(Debug, Clone)]` — pode ser clonado/movido.
- `ThoughtResult`: tem `#[derive(Debug, Clone)]` — pode ser coletado de JoinSet.

### Estrutura do got_actor.rs

```rust
// crates/touring-cognitive/src/got_actor.rs
//! GoT parallel hypothesis exploration via Tokio JoinSet.
//!
//! Provides `explore_parallel_hypotheses` — spawns each hypothesis as an
//! independent tokio task and collects results via JoinSet. Tasks are 'static
//! because they capture Arc<GotNode> + owned ThoughtMessage.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use tokio::task::JoinSet;

use crate::got::{GotNode, ThoughtMessage, ThoughtResult};

static ACTOR_NODE_ID: AtomicU64 = AtomicU64::new(1000);

/// Specification for a parallel hypothesis to explore.
pub struct HypothesisSpec {
    pub content: String,
    pub weight: f64,
    pub label: String,
}

/// Explore multiple hypotheses in parallel.
///
/// Spawns each hypothesis as an independent tokio task via JoinSet.
/// Returns all results sorted by score descending.
/// If `early_exit_threshold` is Some(t), aborts remaining tasks when
/// any result exceeds the threshold.
pub async fn explore_parallel_hypotheses(
    hypotheses: Vec<HypothesisSpec>,
    early_exit_threshold: Option<f64>,
) -> Vec<ThoughtResult> {
    let mut set: JoinSet<ThoughtResult> = JoinSet::new();

    for spec in hypotheses {
        let node_id = ACTOR_NODE_ID.fetch_add(1, Ordering::Relaxed);
        // Arc + owned data → 'static bounds satisfied
        let node = Arc::new(GotNode::new(node_id, spec.label, spec.weight));
        let msg = ThoughtMessage {
            from: 0,
            content: spec.content,
            depth: 0,
            accumulated_score: 0.0,
        };
        set.spawn(async move {
            node.evaluate(&msg)
        });
    }

    let mut results = Vec::new();
    while let Some(join_result) = set.join_next().await {
        match join_result {
            Ok(thought_result) => {
                if let Some(threshold) = early_exit_threshold {
                    if thought_result.score > threshold {
                        set.abort_all();
                        results.push(thought_result);
                        break;
                    }
                }
                results.push(thought_result);
            }
            Err(e) => {
                tracing::warn!("GoT actor task panicked: {e}");
            }
        }
    }

    results.sort_by(|a, b| b.score.partial_cmp(&a.score).unwrap_or(std::cmp::Ordering::Equal));
    results
}
```

**Testes obrigatórios:**
```rust
#[tokio::test]
async fn test_explore_parallel_single_hypothesis()
#[tokio::test]
async fn test_explore_parallel_multiple_hypotheses_sorted()
#[tokio::test]
async fn test_explore_parallel_early_exit()
#[tokio::test]
async fn test_explore_parallel_empty_returns_empty()
#[tokio::test]
async fn test_explore_parallel_is_actually_parallel() // timing: N tasks < N × single_task_time
```

**Nota de implementação:** `GotNode::evaluate()` é síncrona — o spawn será `spawn(async move { node.evaluate(&msg) })`. Para hipóteses genuinamente assíncronas (e.g., que consultam SQLite), o pattern evolui para `spawn(async move { node.evaluate_async(&msg).await })` em sprint futuro.

**Gate:**
```
□ cargo check --workspace → 0 errors
□ cargo clippy --workspace -- -D warnings → 0 warnings
□ cargo test --workspace --exclude touring-python → ≥ 2.706 passed
□ got.rs inalterado (zero breaking change na API existente)
```

---

## Sprint 3 — Sessões Persistentes GoTSnapshot (1 semana)

**Objetivo:** H1.2 — Sessões Zero-Copy com rkyv+SQLite para persistir estado GoT entre sessões.
**Classificação:** L4 (Arquitetura — novo subsistema de persistência)
**Arquivo a criar:** `crates/touring-cognitive/src/session_persistence.rs`
**Arquivo a modificar:** `crates/touring-cognitive/Cargo.toml` (adicionar deadpool-sqlite)
**Meta de testes:** ~2.721

### Pré-requisito: Sprint 0 (deadpool-sqlite no workspace)

### Ponto de integração com o pattern existente

`crdt_graph.rs` demonstra o pattern completo rkyv+mmap2. Replicar para GoTSnapshot:
- `GraphSnapshot` → `GoTSnapshot` (mesma estrutura de snapshot serializável)
- `save_to_mmap` / `load_from_mmap` → `checkpoint` / `restore` (via SQLite, não mmap)

### touring-cognitive/Cargo.toml — adicionar

```toml
deadpool-sqlite = { workspace = true }
```

### Estrutura do session_persistence.rs

```rust
// crates/touring-cognitive/src/session_persistence.rs
//! GoT session persistence via rkyv zero-copy serialization + deadpool-sqlite.
//!
//! Replicates the rkyv pattern from crdt_graph.rs (GraphSnapshot) for GoT state.
//! Pattern: serialize to rkyv bytes → store as BLOB in SQLite sessions table.
//! Restore: read BLOB → rkyv::check_archived_root → deserialize to GoTSnapshot.

use deadpool_sqlite::{Config, Pool, Runtime};

/// Zero-copy snapshot of GoT state for session persistence.
///
/// Mirrors structure of crdt_graph::GraphSnapshot — rkyv-serializable,
/// stored as BLOB in the `sessions` SQLite table.
#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Debug, Clone)]
#[archive(check_bytes)]
pub struct GoTSnapshot {
    /// Serialized thought nodes as (id, label, weight, score) tuples.
    pub nodes: Vec<(u64, String, f64, f64)>,
    /// Serialized thought edges as (from_id, to_id, score_delta) tuples.
    pub edges: Vec<(u64, u64, f64)>,
    /// Pheromone trails: (path_key, strength) pairs.
    pub pheromone_trails: Vec<(String, f64)>,
    /// Snapshot epoch (monotonically increasing).
    pub epoch: u64,
    /// Schema version for forward compatibility.
    pub schema_version: u32,
}

pub const SNAPSHOT_SCHEMA_VERSION: u32 = 1;

/// Manages GoT session persistence to SQLite.
pub struct SessionPersistence {
    pool: Pool,
}

impl SessionPersistence {
    /// Create a new persistence layer using the given SQLite path.
    pub async fn new(db_path: &str) -> Result<Self, String> {
        let cfg = Config::new(db_path);
        let pool = cfg.create_pool(Runtime::Tokio1)
            .map_err(|e| format!("deadpool-sqlite pool creation failed: {e}"))?;
        let persistence = Self { pool };
        persistence.migrate().await?;
        Ok(persistence)
    }

    /// Create the sessions table if it doesn't exist.
    async fn migrate(&self) -> Result<(), String> {
        let conn = self.pool.get().await
            .map_err(|e| format!("pool get failed: {e}"))?;
        conn.interact(|c| {
            c.execute_batch(
                "CREATE TABLE IF NOT EXISTS sessions (
                    id TEXT PRIMARY KEY,
                    snapshot BLOB NOT NULL,
                    updated_at INTEGER NOT NULL
                );",
            )
        }).await
            .map_err(|e| format!("interact failed: {e}"))?
            .map_err(|e| format!("migrate SQL failed: {e}"))
    }

    /// Serialize GoTSnapshot to rkyv bytes and store in SQLite.
    /// Target: < 5ms for typical GoT states.
    pub async fn checkpoint(&self, session_id: &str, state: &GoTSnapshot) -> Result<(), String> {
        let bytes = rkyv::to_bytes::<_, 4096>(state)
            .map_err(|e| format!("GoTSnapshot rkyv serialize failed: {e}"))?;
        let bytes_vec = bytes.to_vec();
        let id = session_id.to_string();
        let conn = self.pool.get().await
            .map_err(|e| format!("pool get failed: {e}"))?;
        conn.interact(move |c| {
            c.execute(
                "INSERT OR REPLACE INTO sessions(id, snapshot, updated_at) \
                 VALUES(?1, ?2, strftime('%s','now'))",
                rusqlite::params![id, bytes_vec],
            )
        }).await
            .map_err(|e| format!("interact failed: {e}"))?
            .map_err(|e| format!("checkpoint SQL failed: {e}"))?;
        Ok(())
    }

    /// Restore GoTSnapshot from SQLite. Returns None if session not found.
    /// Target: < 2ms for typical GoT states.
    pub async fn restore(&self, session_id: &str) -> Result<Option<GoTSnapshot>, String> {
        let id = session_id.to_string();
        let conn = self.pool.get().await
            .map_err(|e| format!("pool get failed: {e}"))?;
        let bytes_opt: Option<Vec<u8>> = conn.interact(move |c| {
            let mut stmt = c.prepare(
                "SELECT snapshot FROM sessions WHERE id = ?1"
            )?;
            let mut rows = stmt.query(rusqlite::params![id])?;
            if let Some(row) = rows.next()? {
                Ok(Some(row.get::<_, Vec<u8>>(0)?))
            } else {
                Ok(None)
            }
        }).await
            .map_err(|e| format!("interact failed: {e}"))?
            .map_err(|e| format!("restore SQL failed: {e}"))?;

        let Some(bytes) = bytes_opt else { return Ok(None) };

        let archived = rkyv::check_archived_root::<GoTSnapshot>(&bytes)
            .map_err(|e| format!("GoTSnapshot rkyv validation failed: {e}"))?;
        if archived.schema_version != SNAPSHOT_SCHEMA_VERSION {
            return Err(format!(
                "GoTSnapshot schema mismatch: expected {SNAPSHOT_SCHEMA_VERSION}, got {}",
                archived.schema_version
            ));
        }
        let snapshot: GoTSnapshot = rkyv::Deserialize::deserialize(archived, &mut rkyv::Infallible)
            .map_err(|e| format!("GoTSnapshot deserialize failed: {e}"))?;
        Ok(Some(snapshot))
    }
}
```

**Testes obrigatórios:**
```rust
#[tokio::test]
async fn test_session_persistence_checkpoint_restore_roundtrip()
#[tokio::test]
async fn test_session_persistence_missing_session_returns_none()
#[tokio::test]
async fn test_session_persistence_overwrites_existing()
#[tokio::test]
async fn test_got_snapshot_rkyv_roundtrip_empty()
#[tokio::test]
async fn test_got_snapshot_rkyv_roundtrip_with_data()
#[test]
fn test_got_snapshot_schema_version_is_one()
```

**Gate:**
```
□ cargo check --workspace → 0 errors
□ cargo clippy --workspace -- -D warnings → 0 warnings
□ cargo test --workspace --exclude touring-python → ≥ 2.721 passed
□ checkpoint < 5ms (benchmark manual ou criterion)
□ restore < 2ms (benchmark manual ou criterion)
```

---

## Sprint 4 — InferletPool Completo (2-3 semanas)

**Objetivo:** H2.4 — Pool de 100 instâncias WASM pré-aquecidas com async support + WIT interface.
**Classificação:** L4 (Arquitetura — novo componente, nova API pública)
**Arquivo a criar:** `crates/touring-wasm/src/inferlet_pool.rs`
**Arquivo a criar (WIT):** `crates/touring-wasm/wit/inferlet.wit`
**Arquivo a modificar:** `crates/touring-wasm/src/lib.rs` (re-exportar InferletPool)
**Meta de testes:** ~2.741

### Pré-requisitos: Sprint 0 (features wasmtime async + pooling-allocator), Sprint 3 (sessões estáveis)

### WIT interface (contrato imutável — definir antes de implementar)

```wit
// crates/touring-wasm/wit/inferlet.wit
package touring:inferlet@0.1.0;

interface evaluate {
    /// Evaluate a reasoning hypothesis.
    /// Returns score (0.0–1.0) and output string.
    evaluate: func(content: string, context: string) -> result<tuple<f32, string>, string>;
}

world inferlet {
    export evaluate;
}
```

### Estrutura do inferlet_pool.rs

```rust
// crates/touring-wasm/src/inferlet_pool.rs
//! InferletPool — pre-warmed pool of WASM instances for parallel inference.
//!
//! Uses PoolingAllocationStrategy (enabled in WasmRunner::new() since S1.1)
//! and async_support for non-blocking execution.

use wasmtime::{Config, Engine, InstanceAllocationStrategy, Module, Store};
use wasmtime::component::Component;

/// Maximum concurrent WASM instances in the pool.
pub const POOL_SIZE: usize = 100;

/// A pool of pre-compiled WASM modules for parallel inferlet execution.
pub struct InferletPool {
    engine: Engine,
    module: Module,
}

impl InferletPool {
    /// Create a new pool from raw WASM bytes.
    ///
    /// Compiles the module once; instances are created on-demand with pooling.
    pub fn new(wasm_bytes: &[u8]) -> Result<Self, String> {
        let mut config = Config::new();
        config.consume_fuel(true);
        config.async_support(true);
        config.allocation_strategy(InstanceAllocationStrategy::pooling());

        let engine = Engine::new(&config)
            .map_err(|e| format!("InferletPool engine creation failed: {e}"))?;
        let module = Module::new(&engine, wasm_bytes)
            .map_err(|e| format!("InferletPool module compilation failed: {e}"))?;

        Ok(Self { engine, module })
    }

    /// Execute the inferlet's `evaluate` function asynchronously.
    ///
    /// Creates a fresh Store with fuel budget. Non-blocking via async.
    pub async fn execute(&self, content: &str) -> Result<(f32, String), String> {
        use wasmtime::{Instance, TypedFunc};
        // Async instantiation via tokio
        let mut store = Store::new(&self.engine, ());
        store.set_fuel(crate::runner::MAX_FUEL)
            .map_err(|e| format!("set_fuel failed: {e}"))?;
        // Note: Instance::new_async requires async feature in engine
        // Implementation details depend on wasmtime 42 async API surface
        // Verify exact API with: cargo doc --package wasmtime --open
        todo!("implement async Instance creation for wasmtime 42")
    }
}
```

**Nota crítica:** A API exata de `Instance::new_async` no wasmtime 42 precisa ser verificada com `cargo doc --package wasmtime`. O design acima é o padrão esperado — confirmar antes de escrever o código final.

**Testes obrigatórios:**
```rust
#[tokio::test]
async fn test_inferlet_pool_new_from_wat()
#[tokio::test]
async fn test_inferlet_pool_execute_success()
#[tokio::test]
async fn test_inferlet_pool_concurrent_executions() // 10 concurrent, < 100ms total
```

**Gate:**
```
□ cargo check --workspace → 0 errors
□ cargo clippy --workspace -- -D warnings → 0 warnings
□ cargo test --workspace --exclude touring-python → ≥ 2.741 passed
□ WIT interface definida e versionada em wit/inferlet.wit
```

---

## Sprint 5 — Memória Episódica + RRF (2-3 semanas)

**Objetivo:** H2.2 — Reciprocal Rank Fusion para combinar BM25 + vector search + call graphs.
**Classificação:** L4 (Arquitetura — novo algoritmo de retrieval, nova estrutura de call graph)
**Arquivo a criar:** `crates/touring-cortex/src/retrieval.rs`
**Arquivo a criar:** `crates/touring-cortex/src/call_graph.rs`
**Arquivo a modificar:** `crates/touring-cortex/src/lib.rs` (re-exportar)
**Meta de testes:** ~2.766

### RRF — Reciprocal Rank Fusion

```rust
// crates/touring-cortex/src/retrieval.rs
//! Reciprocal Rank Fusion for hybrid memory retrieval.
//!
//! Combines multiple ranked result lists (BM25, vector, call-graph)
//! into a single fused ranking. Standard k=60 from Lin et al. (2021).

use std::collections::HashMap;

pub type SymbolId = u64;

/// Fuse multiple ranked result lists using Reciprocal Rank Fusion.
///
/// `k` is the smoothing constant (typically 60.0).
/// Higher k = less sensitive to top-rank position differences.
/// Lower k = top-rank positions matter more.
pub fn reciprocal_rank_fusion(
    ranked_lists: &[&[(SymbolId, f32)]],
    k: f32,
) -> Vec<(SymbolId, f32)> {
    let mut scores: HashMap<SymbolId, f32> = HashMap::new();
    for list in ranked_lists {
        for (rank, (sym_id, _)) in list.iter().enumerate() {
            *scores.entry(*sym_id).or_default() +=
                1.0 / (k + rank as f32 + 1.0);
        }
    }
    let mut fused: Vec<(SymbolId, f32)> = scores.into_iter().collect();
    fused.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap_or(std::cmp::Ordering::Equal));
    fused
}
```

### Call Graph — estrutura básica

```rust
// crates/touring-cortex/src/call_graph.rs
//! Call graph extraction for episodic memory enrichment.
//!
//! Tracks which functions call which, enabling episodic retrieval to
//! surface callee context when a caller is modified.

use petgraph::stable_graph::{NodeIndex, StableGraph};
use std::collections::HashMap;

pub type FunctionId = u64;

/// Directed call graph: edge A→B means A calls B.
pub struct CallGraph {
    graph: StableGraph<FunctionId, (), petgraph::Directed>,
    node_map: HashMap<FunctionId, NodeIndex>,
}

impl CallGraph {
    pub fn new() -> Self { ... }
    pub fn add_call(&mut self, caller: FunctionId, callee: FunctionId) { ... }
    /// All functions called (directly or transitively) by `caller`.
    pub fn callees_transitive(&self, caller: FunctionId) -> Vec<FunctionId> { ... }
    /// All functions that (directly or transitively) call `callee`.
    pub fn callers_transitive(&self, callee: FunctionId) -> Vec<FunctionId> { ... }
}
```

**Integração com recall existente:** `memory/recall.rs` usa BM25. Após S5, o retrieval pipeline é:
1. BM25 recall → `Vec<(SymbolId, f32)>`
2. Vector search (HNSW) → `Vec<(SymbolId, f32)>`
3. Call graph expansion → `Vec<(SymbolId, f32)>`
4. `reciprocal_rank_fusion(&[&bm25, &vector, &call_graph], 60.0)` → fused result

**Testes obrigatórios:**
```rust
#[test]
fn test_rrf_single_list_preserves_order()
#[test]
fn test_rrf_two_lists_fuses_correctly()
#[test]
fn test_rrf_k60_reduces_top_rank_sensitivity()
#[test]
fn test_rrf_deduplicates_symbols()
#[test]
fn test_call_graph_add_call()
#[test]
fn test_call_graph_callees_transitive()
#[test]
fn test_call_graph_callers_transitive()
```

**Gate:**
```
□ cargo check --workspace → 0 errors
□ cargo clippy --workspace -- -D warnings → 0 warnings
□ cargo test --workspace --exclude touring-python → ≥ 2.766 passed
□ reciprocal_rank_fusion é função pura (sem side effects, determinística)
```

---

## Sprint 6 — CRDTs P2P (2-3 semanas)

**Objetivo:** H2.3 — Melhorar CrdtSemanticGraph existente para protocolo P2P real.
**Classificação:** L4 (Arquitetura — extensão de componente existente)
**Arquivo a modificar:** `crates/touring-learning/src/memory/crdt_graph.rs`
**Meta de testes:** ~2.786

### Decisão arquitetural: diamond-types vs CRDT hand-rolled

O `CrdtSemanticGraph` hand-rolled já implementa OR-Set + LWW + rkyv persistence. O `diamond-types` crate adicionaria:
- Velocidade: 260.000 edições em 56ms (vs hand-rolled que é mais lento mas funcional)
- Complexidade: API completamente diferente, migração não trivial

**Recomendação:** Manter CRDT hand-rolled e adicionar **protocolo de merge P2P** sem migrar para diamond-types. Adicionar diamond-types apenas se benchmark mostrar gargalo real.

### O que adicionar ao crdt_graph.rs

```rust
// Novo: protocolo de wire P2P

/// Delta of operations since a given epoch — for P2P sync.
#[derive(rkyv::Archive, rkyv::Serialize, rkyv::Deserialize, Debug, Clone)]
#[archive(check_bytes)]
pub struct CrdtDelta {
    pub added_nodes: Vec<CrdtNodeId>,
    pub removed_nodes: Vec<CrdtNodeId>,
    pub added_edges: Vec<CrdtEdge>,
    pub weight_updates: Vec<(CrdtNodeId, NodeWeight)>,
    pub from_epoch: u64,
    pub to_epoch: u64,
    pub actor_id: ActorId,
}

impl CrdtSemanticGraph {
    /// Export delta of all changes since `since_epoch`.
    pub fn export_delta(&self, since_epoch: u64) -> CrdtDelta { ... }

    /// Apply a delta received from a peer. Idempotent.
    pub fn apply_delta(&mut self, delta: CrdtDelta) { ... }

    /// Merge another graph using OR-Set semantics. Returns number of changes applied.
    pub fn merge(&mut self, other: &CrdtSemanticGraph) -> usize { ... }
}
```

**Testes obrigatórios:**
```rust
#[test]
fn test_crdt_delta_export_empty_graph()
#[test]
fn test_crdt_delta_apply_adds_nodes()
#[test]
fn test_crdt_merge_convergence_two_agents() // merge(A, B) == merge(B, A)
#[test]
fn test_crdt_merge_idempotent() // apply_delta twice = same as once
#[test]
fn test_crdt_merge_commutativity() // A.merge(B) == B.merge(A)
```

**Gate:**
```
□ cargo check --workspace → 0 errors
□ cargo clippy --workspace -- -D warnings → 0 warnings
□ cargo test --workspace --exclude touring-python → ≥ 2.786 passed
□ Testes de convergência CRDT passam (commutativity + idempotency)
□ API existente de CrdtSemanticGraph não quebrada
```

---

## Sprint 7 — eBPF + KS-drift Telemetry (3-4 semanas)

**Objetivo:** H3.3 — Telemetria de drift com KS-test em user-space + eBPF opcional.
**Classificação:** L5 (Paradigm shift — nova tecnologia, risco alto)
**Arquivo a criar:** `crates/touring-server/src/telemetry/drift_monitor.rs`
**Arquivo a modificar:** `crates/touring-server/src/telemetry/mod.rs`
**Meta de testes:** ~2.801

### Abordagem em 2 fases

**Fase A (user-space, sem aya):** Implementar `DriftMonitor` com KS-test usando `statrs` (já no workspace).
**Fase B (eBPF, requer kernel 5.8+ com BTF):** Adicionar `aya` crate para kernel tracing.

### Fase A — DriftMonitor user-space

```rust
// crates/touring-server/src/telemetry/drift_monitor.rs
use std::collections::VecDeque;
use statrs::statistics::Statistics;

/// Kolmogorov-Smirnov drift detector for tool latency distributions.
pub struct DriftMonitor {
    /// Rolling window of observed latency samples (ms).
    window: VecDeque<f64>,
    /// Baseline distribution (established in first N samples).
    baseline: Vec<f64>,
    /// Window size for KS test.
    window_size: usize,
    /// KS test threshold above which drift is declared (typical: 0.15).
    threshold: f64,
}

impl DriftMonitor {
    pub fn new(window_size: usize, threshold: f64) -> Self { ... }
    pub fn observe(&mut self, latency_ms: f64) { ... }
    /// Returns true if distribution has drifted from baseline.
    pub fn detect_drift(&self) -> bool {
        if self.window.len() < self.window_size || self.baseline.is_empty() {
            return false;
        }
        let d_stat = ks_two_sample(
            &self.window.iter().copied().collect::<Vec<_>>(),
            &self.baseline,
        );
        d_stat > self.threshold
    }
}

/// Two-sample KS test statistic (D statistic only, not p-value).
fn ks_two_sample(sample1: &[f64], sample2: &[f64]) -> f64 { ... }
```

### Fase B — aya eBPF (pesquisa, requer aprovação explícita)

```toml
# APENAS adicionar ao workspace se Fase B aprovada:
# aya = { version = "0.13", features = ["async_tokio"] }
# aya-ebpf = "0.1"  # separate crate for eBPF programs
```

**Prerequisitos Fase B:**
- Kernel 5.8+ com BTF habilitado (`/sys/kernel/btf/vmlinux` deve existir)
- Verificar: `ls /sys/kernel/btf/vmlinux`
- Compilador BPF: `bpf-linker` via `cargo install bpf-linker`

**Gate Fase A:**
```
□ cargo check --workspace → 0 errors
□ cargo clippy --workspace -- -D warnings → 0 warnings
□ cargo test --workspace --exclude touring-python → ≥ 2.801 passed
□ DriftMonitor detecta drift simulado (baseline vs 2× latência)
```

---

## Sprint 8 — BranchFS via overlayfs (4-6 semanas, pesquisa)

**Objetivo:** H3.1/H3.2 — Aproximar semântica de BranchFS usando overlayfs + user namespaces.
**Classificação:** L5 (Paradigm shift — pesquisa, sem garantia de entregável)
**Status:** PESQUISA — não há syscall `branch()` no Linux mainline.

### Abordagem alternativa viável

```bash
# Criar branch de workspace via overlayfs (user namespace)
unshare --mount --user --fork bash -c "
    mount -t overlay overlay \
        -o lowerdir=/workspace,upperdir=/tmp/branch-1/upper,workdir=/tmp/branch-1/work \
        /tmp/branch-1/merged
    # Execute in /tmp/branch-1/merged — changes isolated
"
```

**Overhead:** ~10-50ms (vs 350μs do syscall hipotético). Viável para tarefas > 1s.

### Estrutura proposta (se pesquisa avançar)

```rust
// crates/touring-server/src/branch/mod.rs
pub struct WorkspaceBranch {
    merged_path: PathBuf,
    upper_path: PathBuf,
    work_path: PathBuf,
}

impl WorkspaceBranch {
    pub fn new(workspace: &Path) -> Result<Self, String> { ... }
    pub fn commit(&self) -> Result<(), String> { ... } // apply upper to base
    pub fn discard(self) -> Result<(), String> { ... } // rm -rf upper
}
```

**Gate (se implementado):**
```
□ overlayfs mount/unmount funciona em ambiente de teste
□ Mudanças em branch não afetam workspace original
□ commit() aplica mudanças atomicamente
□ cargo test green
```

---

## Backlog / Hold

| Item | Status | Justificativa |
|---|---|---|
| **H2.6 candle (TinyTransformer GPU)** | HOLD | `TinyTransformerPredictor` com ndarray já funciona. candle só se GPU for necessária — medir primeiro. |
| **H3.1/H3.2 BranchFS syscall** | HOLD | Não existe no Linux mainline. S8 usa overlayfs como alternativa. |
| **diamond-types P2P** | HOLD | CrdtSemanticGraph hand-rolled já funcional. Adicionar apenas se benchmark mostrar gargalo. |
| **wit-bindgen integration** | HOLD | Interface WIT definida no S4. Geração de bindings no S4+. |

---

## Tracking de Progresso

| Sprint | Feature | Duração | Meta Testes | Status |
|---|---|---|---|---|
| S0 | Deps: deadpool-sqlite + wasmtime async | < 1 dia | 2.671 | PENDENTE |
| S1.1 | H2.4 PoolingAllocationStrategy | 1 dia | 2.672 | PENDENTE |
| S1.2 | H2.5 tarjan_scc + topological_order | 2-3 dias | 2.677 | PENDENTE |
| S1.3 | H1.3 ReminderBandit LinUCB | 2-3 dias | 2.682 | PENDENTE |
| S2 | H2.1 GoT Actors JoinSet | 1-2 semanas | 2.706 | PENDENTE |
| S3 | H1.2 GoTSnapshot + deadpool-sqlite | 1 semana | 2.721 | PENDENTE |
| S4 | H2.4 InferletPool + WIT | 2-3 semanas | 2.741 | PENDENTE |
| S5 | H2.2 RRF + Call Graphs | 2-3 semanas | 2.766 | PENDENTE |
| S6 | H2.3 CRDTs P2P delta/merge | 2-3 semanas | 2.786 | PENDENTE |
| S7 | H3.3 KS-drift + eBPF | 3-4 semanas | 2.801 | PENDENTE |
| S8 | H3.1/H3.2 BranchFS overlayfs | 4-6 semanas | TBD | PESQUISA |

**Total de novos testes projetados (S0-S7):** ~130 novos testes
**Total projetado ao final de S7:** ~2.801 testes

---

## Decisões de Arquitetura

| Decisão | Escolha | Justificativa |
|---|---|---|
| ReminderBandit vs LinUcb existente | **LinUcb SEPARADO** | Arms de context injection (8) são diferentes de reminder arms. Misturar quebraria semântica do bandit existente. |
| diamond-types vs CRDT hand-rolled | **Hand-rolled primeiro** | CrdtSemanticGraph já funcional. Diamond-types = troca complexa com benefício não medido. |
| candle vs ndarray | **ndarray por padrão** | TinyTransformerPredictor já funciona com ndarray. Candle apenas se GPU for requisito verificado. |
| BranchFS syscall | **overlayfs via unshare** | Syscall não existe. overlayfs é viável com 10-50ms overhead para tarefas longas. |
| GoT parallelism | **JoinSet + Arc<GotNode>** | `'static` bounds do JoinSet exigem ou clone ou Arc. Arc evita cópia de dados e não quebra API. |
| Wasmtime async | **async_support(true) na engine** | Única engine por processo — shared engine com pooling é mais eficiente que engine-por-request. |

---

## Riscos e Mitigações

| Risco | Sprint | Probabilidade | Mitigação |
|---|---|---|---|
| `pooling-allocator` consome virtual memory excessiva | S0/S1.1 | Média | Testar com `ulimit -v`; reduzir pool de 100→10 se necessário |
| `deadpool-sqlite` incompatível com rusqlite 0.32 bundled | S0 | Baixa | Fixar versão `deadpool-sqlite = "0.9"` + verificar em Cargo.lock |
| `LinUcb` struct hardcoded em 8 arms | S1.3 | Média | Ler struct completa antes; se hardcoded, criar `ReminderLinUcb` própria |
| `JoinSet` com `GotNode` não-Clone | S2 | Baixa | Usar `Arc<GotNode>` — não requer Clone na struct |
| wasmtime 42 API async muda de 41 | S4 | Alta | Verificar `cargo doc --package wasmtime` antes de implementar execute() |
| overlayfs requer root ou user namespaces habilitados | S8 | Média | Verificar `unshare --user` disponível; pode não funcionar em containers |
| eBPF requer kernel 5.8+ com BTF | S7 | Baixa | Verificar `/sys/kernel/btf/vmlinux`; Fase A (user-space) não tem este requisito |

---

## Próximo Passo Imediato (para amanhã)

1. **Abrir** `Cargo.toml` do workspace
2. **Adicionar** as 3 mudanças do Sprint 0 (deadpool-sqlite + wasmtime features + wit-bindgen)
3. **Executar** `cargo check --workspace` — deve passar verde
4. **Executar** `cargo test --workspace --exclude touring-python` — deve ser 2.671 passed
5. **Iniciar S1.1:** Modificar `runner.rs` linhas 61-66 (10 linhas de mudança)
6. **Iniciar S1.2:** Adicionar `detect_cycles()` + `topological_order()` ao `dependency_cache.rs`
7. **Verificar LinUcb struct completa** (ler arquivo além da linha 140) antes de S1.3

**S0 + S1.1 + S1.2 são executáveis no mesmo dia.** S1.3 requer verificação prévia da struct LinUcb.

---

*Plano gerado por TACO Orchestrator N2 v4.0 em 26/03/2026*
*Baseado em leitura direta do código: runner.rs, dependency_cache.rs, linucb.rs, got.rs, crdt_graph.rs, Cargo.toml*
*Validator Score: 0.94*
