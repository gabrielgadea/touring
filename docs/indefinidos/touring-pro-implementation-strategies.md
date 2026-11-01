# Análise touring-pro.md + Estratégias de Implementação

> **STATUS:** DOCUMENTO HISTRICO — Criado em 26/03/2026 para analisar estado v21.1.0
> **NÃO ATUALIZAR:** As referncias a v21.1.0/v18.0.0 so fatos histricos do documento original
> **Versao atual do Touring:** v30.3.1 (este documento preserva o estado da analise original)

**Data:** 26/03/2026 | **Estado baseline:** Touring v21.1.0 | **Testes baseline:** 2.671 | **Crates:** 13

---

## Resumo Executivo

O `touring-pro.md` é um documento de arquitetura avançada estruturado em **5 instâncias do motor ASR-GoT BIGMAS-L6**, cobrindo um roadmap evolutivo de 3 horizontes temporais para levar o Touring à autonomia CILA L6. O documento foi escrito contra o estado v18.0.0 — portanto **H1.1 (extração de touring-cortex e touring-index) já está completo no v20.0/v21.x**.

O roadmap restante compreende **11 propostas ativas**, classificadas em P0-P3 por viabilidade e valor, com 4 quick wins implementáveis em menos de 1 sprint.

---

## Mapeamento Completo das Propostas

### HORIZONTE 1 — Curto Prazo (Desacoplamento + Continuidade Cognitiva)

| ID | Proposta | Status | Complexidade | Valor |
|----|----------|--------|-------------|-------|
| H1.1 | Extração touring-cortex + touring-index | ✅ COMPLETO (v20.0) | — | — |
| H1.2 | Sessões Persistentes Zero-Copy (rkyv + SQLite) | Pendente | Média | 0.89 |
| H1.3 | Learned System Reminders via LinUCB | Pendente | Baixa | 0.86 |

### HORIZONTE 2 — Médio Prazo (Concorrência + P2P)

| ID | Proposta | Status | Complexidade | Valor |
|----|----------|--------|-------------|-------|
| H2.1 | GoT Parallel Execution via Tokio Actors | Pendente | Média-Alta | 0.83 |
| H2.2 | Memória Episódica com Call Graphs + RRF | Pendente | Alta | 0.81 |
| H2.3 | Multi-Agent P2P com diamond-types CRDTs | Pendente | Alta | 0.74 |
| H2.4 | Wasmtime Inferlets (touring-wasm) | Pendente (feature-gated) | Média | 0.78 |
| H2.5 | petgraph Blast Radius Enriched | Pendente | Baixa-Média | 0.71 |
| H2.6 | MCTS TinyTransformerPredictor (candle) | Pendente | Alta | 0.68 |

### HORIZONTE 3 — Longo Prazo / Pesquisa (CILA L6)

| ID | Proposta | Status | Complexidade | Valor |
|----|----------|--------|-------------|-------|
| H3.1 | BranchFS Copy-on-Write Integration | Pesquisa | Muito Alta | 0.45* |
| H3.2 | BR_MEMORY + Effect Gating | Pesquisa | Muito Alta | 0.42* |
| H3.3 | eBPF + KS-drift Telemetry | Pendente | Alta | 0.62 |

> *H3.1/H3.2: syscall `branch()` não existe no Linux mainline. Alternativa viável: overlayfs + user namespaces.

---

## Estratégias de Implementação Detalhadas

### P0 — Implementar Agora

#### H1.3 — Learned System Reminders via LinUCB
**Motivação:** Eliminar 24 templates manuais. LinUCB já existe em `touring-learning`.

**Abordagem:**
```rust
// touring-learning/src/reminder_bandit.rs
pub struct ReminderBandit {
    linucb: LinUcb,  // já existe
    candidates: Vec<ReminderTemplate>,
}

impl ReminderBandit {
    pub fn select(&mut self, ctx: &ReminderContext) -> &ReminderTemplate {
        let features = ctx.feature_vector(); // [task_type, error_rate, context_fill, elapsed_ms]
        let arm = self.linucb.select_arm(&features);
        &self.candidates[arm]
    }

    pub fn reward(&mut self, arm: usize, correction_occurred: bool) {
        let r = if correction_occurred { -0.5 } else { 1.0 };
        self.linucb.update(arm, r);
    }
}
```

**Sequência:** (1) criar `ReminderBandit` wrapper, (2) migrar 24 templates para pool de candidatos, (3) integrar no prompt-enhancer hook, (4) conectar reward signal ao pós-processamento de edições do usuário.

**Métricas de sucesso:** Taxa de correções pós-reminder < 15% (baseline estimado ~35%).

**Esforço:** 2-3 dias. **Risco:** Baixo — LinUCB já testado.

---

#### H2.5 — petgraph Blast Radius Enriched
**Motivação:** Substituir HashMap-based blast radius por grafo dirigido com algoritmos nativos.

**Best Practice Context7 (petgraph):** `toposort()` iterativo com DFS, `tarjan_scc()` para ciclos, `Topo` struct para traversal incremental.

```rust
// touring-index/src/blast_radius.rs
use petgraph::Graph;
use petgraph::algo::{toposort, tarjan_scc};
use petgraph::visit::Topo;

pub struct DependencyGraph {
    graph: Graph<FileId, ImportType, Directed>,
    node_map: HashMap<FileId, NodeIndex>,
}

impl DependencyGraph {
    pub fn blast_radius(&self, changed_file: FileId) -> Vec<FileId> {
        let start = self.node_map[&changed_file];
        let mut topo = Topo::new(&self.graph);
        let mut affected = vec![];
        while let Some(nx) = topo.next(&self.graph) {
            affected.push(self.graph[nx]);
        }
        affected
    }

    pub fn detect_cycles(&self) -> Vec<Vec<FileId>> {
        tarjan_scc(&self.graph)
            .into_iter()
            .filter(|scc| scc.len() > 1)
            .map(|scc| scc.iter().map(|&n| self.graph[n]).collect())
            .collect()
    }
}
```

**Esforço:** 3-4 dias. **Risco:** Baixo — petgraph bem documentado, substituição cirúrgica.

---

#### deadpool-sqlite para Connection Pool Async
**Motivação:** `rusqlite` atual não é async-safe; deadpool-sqlite é drop-in.

```rust
// touring-core/src/db.rs
use deadpool_sqlite::{Config, Pool, Runtime};

pub async fn create_pool(path: &str) -> Pool {
    let cfg = Config::new(path);
    cfg.create_pool(Runtime::Tokio1).expect("pool creation")
}
```

**Esforço:** 1-2 dias. **Risco:** Baixo.

---

### P1 — Próximo Sprint

#### H2.1 — GoT Parallel Execution via Tokio Actors
**Motivação:** Pipeline ASR-GoT executa Fases 3+4 sequencialmente, subutilizando CPUs.

**Best Practice Context7 (Tokio):** `tokio::task::Builder::new().name("got-actor")`, `JoinSet` para coleta de resultados.

```rust
// touring-cognitive/src/got_actor.rs
use tokio::sync::mpsc;
use tokio::task::JoinSet;
use std::sync::atomic::{AtomicU64, Ordering};

static NODE_ID: AtomicU64 = AtomicU64::new(0);

pub async fn explore_parallel_hypotheses(
    hypotheses: Vec<HypothesisSpec>,
) -> ThoughtNode {
    let mut set = JoinSet::new();

    for spec in hypotheses {
        let id = NODE_ID.fetch_add(1, Ordering::Relaxed);
        set.spawn(async move {
            let (tx, rx) = mpsc::channel(32);
            let actor = GotSemanticNodeActor { rx, node_id: id };
            actor.evaluate(spec).await
        });
    }

    while let Some(result) = set.join_next().await {
        if let Ok(node) = result {
            if node.confidence > 0.8 {
                set.abort_all();
                return node;
            }
        }
    }
    ThoughtNode::fallback()
}
```

**Esforço:** 1-2 semanas. **Risco:** Médio.

---

#### H1.2 — Sessões Persistentes Zero-Copy (rkyv + SQLite)
**Motivação:** Cada sessão recomeça do zero; continuidade conversacional profunda requer re-hidratação do GoT state.

```rust
// touring-cognitive/src/session_persistence.rs
use rkyv::{Archive, Serialize, Deserialize};

#[derive(Archive, Serialize, Deserialize)]
pub struct GoTSnapshot {
    pub nodes: Vec<ThoughtNode>,
    pub edges: Vec<(u64, u64, f32)>,
    pub crdt_oplog: Vec<u8>,
    pub epoch: u64,
}

impl SessionPersistence {
    pub async fn checkpoint(&self, session_id: &str, state: &GoTSnapshot) -> Result<()> {
        let bytes = rkyv::to_bytes::<_, 256>(state)?;
        let conn = self.pool.get().await?;
        conn.interact(move |c| {
            c.execute(
                "INSERT OR REPLACE INTO sessions(id, snapshot, updated_at) VALUES(?1, ?2, strftime('%s','now'))",
                params![session_id, bytes.as_slice()],
            )
        }).await??;
        Ok(())
    }
}
```

**Métricas:** checkpoint < 5ms, restore < 2ms. **Esforço:** 1-2 semanas. **Risco:** Médio.

---

#### H2.4 — Wasmtime Inferlets (touring-wasm)
**Best Practice Context7:** `PoolingAllocationStrategy` para 100 instâncias pré-aquecidas, `set_fuel(10_000)`.

```rust
// crates/touring-wasm/src/inferlet_pool.rs
use wasmtime::*;

pub struct InferletPool {
    engine: Engine,
    module: Module,
}

impl InferletPool {
    pub fn new(wasm_bytes: &[u8]) -> Result<Self> {
        let mut config = Config::new();
        config.consume_fuel(true);
        config.async_support(true);
        config.allocation_strategy(InstanceAllocationStrategy::pooling());

        let engine = Engine::new(&config)?;
        let module = Module::new(&engine, wasm_bytes)?;
        Ok(Self { engine, module })
    }
}
```

**Esforço:** 2-3 semanas. **Risco:** Médio.

---

#### H2.2 — Memória Episódica com Call Graphs + RRF

```rust
// RRF fusion em touring-cortex/src/retrieval.rs
pub fn reciprocal_rank_fusion(
    bm25_results: &[(SymbolId, f32)],
    vector_results: &[(SymbolId, f32)],
    k: f32,  // tipicamente 60.0
) -> Vec<(SymbolId, f32)> {
    let mut scores: HashMap<SymbolId, f32> = HashMap::new();
    for (rank, (sym, _)) in bm25_results.iter().enumerate() {
        *scores.entry(*sym).or_default() += 1.0 / (k + rank as f32 + 1.0);
    }
    for (rank, (sym, _)) in vector_results.iter().enumerate() {
        *scores.entry(*sym).or_default() += 1.0 / (k + rank as f32 + 1.0);
    }
    let mut fused: Vec<_> = scores.into_iter().collect();
    fused.sort_by(|a, b| b.1.partial_cmp(&a.1).unwrap());
    fused
}
```

**Esforço:** 2-3 semanas. **Risco:** Médio.

---

### P2 — Médio Prazo

#### H2.3 — diamond-types CRDTs para P2P
**Motivação:** Eliminar `Arc<RwLock<GoTState>>` — gargalo em paralelismo.
**Benchmark diamond-types:** 260.000 edições em 56ms = ~4.600 edições/ms.
**Risco:** Médio-Alto — serialização intermediária via rkyv necessária.

---

#### H3.3 — eBPF + KS-drift Telemetry

```rust
// User-space (Rust com aya crate)
pub struct DriftMonitor {
    ring_buf: RingBuf<DriftEvent>,
    ks_window: VecDeque<f64>,
}

impl DriftMonitor {
    pub fn detect_drift(&self) -> bool {
        let d_stat = ks_two_sample(&self.ks_window, &self.baseline);
        d_stat > 0.15
    }
}
```

**Esforço:** 3-4 semanas. **Risco:** Alto — requer kernel 5.8+ com BTF.

---

### P3 — Longo Prazo / Pesquisa

#### H3.1+H3.2 — BranchFS + BR_MEMORY
**ATENCAO:** `syscall branch()` e `FS_IOC_BRANCH_*` são **propostas acadêmicas** — não existem no Linux mainline.

**Alternativa viável com overlayfs:**
```bash
unshare --mount --user --fork
mount -t overlay overlay \
  -o lowerdir=/workspace,upperdir=/tmp/branch-1/upper,workdir=/tmp/branch-1/work \
  /tmp/branch-1/merged
```

**Custo real:** ~10-50ms (vs 350μs hipotético). Viável para tarefas de longa duração.

---

## Critical Path

```
[AGORA]
H1.3 (LinUCB Reminders) --+
H2.5 (petgraph blast)     +-> [SPRINT 2]  H2.1 (Tokio Actors) <- DESBLOQUEADOR
deadpool-sqlite -----------+              H1.2 (Sessoes rkyv)
                                              |
                                    [SPRINT 3-4]
                                    H2.4 (Wasmtime WIT interface)
                                    H2.2 (RRF + Call Graphs)
                                              |
                                    [SPRINT 5-6]
                                    H2.3 (diamond-types CRDTs)
                                    H3.3 (eBPF telemetria)
                                    H2.6 (MCTS candle)
                                              |
                                    [PESQUISA]
                                    H3.1/H3.2 (BranchFS via overlayfs)
```

---

## Quick Wins (< 1 semana, zero risco)

| Quick Win | Esforço | Impacto |
|-----------|---------|---------|
| `deadpool-sqlite` connection pool | 1-2 dias | Async SQLite sem contention |
| `LinUCB ReminderBandit` | 2-3 dias | RL-driven reminder selection |
| petgraph blast radius | 3-4 dias | `tarjan_scc` + `toposort` real |
| Wasmtime `consume_fuel(true)` no touring-wasm existente | 1 dia | Budget de sandbox imediato |

---

## Best Practices Context7 por Tecnologia

| Tecnologia | Key Pattern |
|-----------|-------------|
| **Tokio** | `JoinSet` para GoT actor collection, `Task::Builder::name()` para debugging |
| **Wasmtime** | `PoolingAllocationStrategy` (100 instâncias pré-aquecidas), `Store` por Inferlet |
| **petgraph** | `Topo` struct para traversal incremental O(V+E), `tarjan_scc()` para ciclos |
| **rusqlite** | `PRAGMA user_version` alinhado com `SCHEMA_VERSION=4`, `deadpool-sqlite` para async |
| **tree-sitter** | Incremental parsing — reprocessa apenas diffs |

---

## Gaps e Oportunidades Não Mencionadas no Documento

1. **sqlite-vec** como alternativa ao VectorLite — extensão SQLite nativa para vetores
2. **tracing + OTLP** — spans assíncronos através de `await` chains nos actors GoT
3. **rkyv schema versioning** — evolução de schema dos archives antes de H1.2 em produção
4. **MCTS existente antes de candle** — usar `touring_mcts_search` existente antes de TinyTransformerPredictor
5. **wit-bindgen interface** — definir interface WIT antes de Wasmtime Inferlets (contrato imutável)

---

## Próximos Passos Sugeridos

1. **Amanhã:** `deadpool-sqlite` + wasmtime `consume_fuel` (1-2 dias, zero risco)
2. **Esta semana:** `ReminderBandit` via LinUCB + petgraph blast radius
3. **Próximo sprint:** `GotSemanticNodeActor` com Tokio actors (H2.1) — desbloqueia tudo
4. **Semana 3-4:** rkyv sessions + Wasmtime Inferlets WIT interface design
5. **Trimestre:** RRF + call graphs + diamond-types CRDTs
6. **Backlog de pesquisa:** eBPF telemetria, MCTS candle, BranchFS via overlayfs

---

*Gerado por TACO Orchestrator N2 v4.0 em 26/03/2026 | Validator Score: 0.91*
