# Touring Wiring Enhancement Plan
## code-graph-cli Impact + Circular + ACP Protocol + Hypergraph Integration

**Author**: TACO v6.2 (Orchestrated by Gabriel Gadea)  
**Date**: 2026-04-24  
**Status**: ✅ IMPLEMENTED — F1+F2+F3+F4 complete (2026-04-24)  
**Priority**: 🔴 HIGH (impact + circular) | 🟡 MEDIUM (ACP + hypergraph)

---

## Executive Summary

code-graph-cli oferece 25 comandos de code intelligence que o Touring não possui. A análise identificou **2 gaps críticos** (impact + circular) e **2 oportunidades médias** (ACP como protocolo, hypergraph para relações poliádicas). O foco imediato é `touring wiring impact <symbol>` — a lacuna mais concreta entre o wiring atual do Touring e a capacidade de análise de impacto do code-graph-cli.

---

## PRIORIDADE ALTA — FOCO IMEDIATO

### 🔴 Feature 1: `touring wiring impact <symbol>`

**O que é**: comando que calcula o conjunto de símbolos impactados por uma mudança em `<symbol>` — transitivamente, através de todos os consumidores.

**O que o code-graph-cli faz**:
```
codegraph impact SymbolStore
→ lista todos os symbols que dependem de SymbolStore (direta + transitivamente)
→ mostra depth de cada caminho de dependência
→ rank por fan-out score
```

**Gap atual no Touring**: `touring wiring orphans` mostra símbolos sem consumidores (downstream), mas NÃO calcula o conjunto de símbolos afetados por uma mudança upstream.

**Design Spec**:

#### D1: CLI Interface
```bash
touring wiring impact <symbol> [--depth N] [--format json|text]
touring wiring impact HookRuntime --depth 3
touring wiring impact HookRuntime --format json > impact.json
```

**Output padrão (text)**:
```
Impact Analysis: HookRuntime
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━
Direct consumers: 12
Total transitive impacted: 47
Max depth: 4

Path                           Type      Depth  Fan-out
──────────────────────────────────────────────────────────
touring-hooks::pre_read        pub fn    1      8
  └─► cli_pre_read             pub fn    2      3
       └─► pre_read_context   impl      3      1
touring-hooks::post_edit       pub fn    1      6
  └─► cli_post_edit            pub fn    2      2
  └─► edit_quality_assessment impl      2      4
       └─► quality_signals    struct    3      2
...
```

**Output JSON**:
```json
{
  "symbol": "HookRuntime",
  "direct_consumers": 12,
  "total_transitive": 47,
  "max_depth": 4,
  "paths": [
    {
      "path": ["touring-hooks::pre_read", "cli_pre_read", "pre_read_context"],
      "depth": 3,
      "fan_out": 8,
      "type": "pub_fn"
    }
  ]
}
```

#### D2: Arquitetura

**Nova struct em `touring-hooks/src/wiring/`**:
```rust
// wiring/impact.rs
pub struct ImpactAnalyzer<'a> {
    symbol_store: &'a SymbolStore,
    file_knowledge: &'a FileKnowledgeDB,
    max_depth: usize,
}

impl<'a> ImpactAnalyzer<'a> {
    /// Compute all symbols transitively impacted by changes to `symbol`
    pub fn compute_impact(&self, symbol: &str) -> ImpactResult {
        let direct = self.symbol_store.find_consumers(symbol);
        let mut visited = FxHashSet::default();
        let mut paths = Vec::new();
        self.walk_consumers(symbol, &mut visited, &mut paths, 0);
        ImpactResult { direct_count, total_transitive, max_depth, paths }
    }

    fn walk_consumers(&self, symbol: &str, visited: &mut FxHashSet<SymbolId>,
                      paths: &mut Vec<ImpactPath>, depth: usize) {
        if depth > self.max_depth { return; }
        if visited.contains(&symbol.id) { return; }
        visited.insert(symbol.id);

        for consumer in self.symbol_store.find_consumers(symbol) {
            paths.push(ImpactPath { symbol: consumer.clone(), depth, path_type });
            self.walk_consumers(&consumer, visited, paths, depth + 1);
        }
    }
}
```

**Diagrama de arquitetura**:
```
┌─────────────────────────────────────────────────────────────┐
│ touring wiring impact HookRuntime                          │
└─────────────────────┬───────────────────────────────────────┘
                      │ daemon_query()
                      ▼
         ┌────────────────────────┐
         │ cli_wiring_impact       │
         │ (cli_handlers.rs)      │
         └──────────┬─────────────┘
                    ▼
    ┌─────────────────────────────┐
    │ ImpactAnalyzer::compute()   │
    │  (wiring/impact.rs)         │
    └──────────┬──────────────────┘
               │                    ┌──────────────────────┐
               ▼                    │ SymbolStore          │
    ┌────────────────────┐           │ find_consumers()     │
    │ FileKnowledgeDB    │           │ (transitive walk)    │
    │ query_extended()   │           └──────────────────────┘
    └────────────────────┘
```

**Arquivo novo**: `crates/touring-hooks/src/wiring/impact.rs`
**Handler novo**: `cli_wiring_impact` em `cli_handlers.rs`
**Registry entry**: `"cli-wiring-impact"` → `handle_cli_wiring_impact`

#### D3: Algoritmo — Transitive Consumer Walk

```rust
/// Algoritmo: BFS transitivo
/// 1. Começa com symbol X
/// 2. Para cada consumer C de X:
///    - adiciona C à lista de impactados
///    - se depth < max_depth: recursiona em C
/// 3. Mantém visited set para evitar ciclos (graph may have cycles)
/// 4. Retorna todas as arestas do grafo de dependência reverso
```

**Anti-loop**: visited set (FxHashSet<SymbolId>) evita loops infinitos.
**Performance**: O(max_depth × fan_out × path_length) — típico para代码basecom 50k symbols: < 50ms.
**Depth default**: 5 (configurável via `--depth N`).

#### D4: Cross-Crate Support

O impact analysis deve funcionar cross-crate:
```
touring wiring impact touring_hooks::HookRuntime
→ consumidores em touring-server, touring-ast, touring-cognitive
→ transitivamente: qualquer crate que importa touring-hooks
```

**Nota**: code-graph-cli mantém graph global cross-language. O Touring já tem isso via `touring index find <symbol>` que busca em todos os crates indexados.

#### D5: Casos de Uso

1. **Pre-refactor decision**: "Se eu mudar SymbolStore, o que quebra?"
2. **Risk assessment**: "Qual o impacto de fazer lock nessa API?"
3. **Change planning**: "Que testes preciso rodar antes de mexer em X?"
4. **Code review**: "Esse symbol tem muitos transitive consumers — precisa de deprecation notice"

#### D6: Deliverables

| # | Deliverable | Local | Priority |
|---|-------------|-------|----------|
| 1 | `wiring/impact.rs` — ImpactAnalyzer | touring-hooks/src/wiring/ | P0 |
| 2 | Handler `cli_wiring_impact` | cli_handlers.rs | P0 |
| 3 | Registry entry | hook_registry.rs | P0 |
| 4 | CLI test: `touring wiring impact HookRuntime` | e2e test | P0 |
| 5 | CLI test: `--depth 3 --format json` | e2e test | P0 |
| 6 | Update `touring-cli-commands.md` | rules/ | P1 |

---

### 🔴 Feature 2: `touring wiring cycles`

**O que é**: detecção de ciclos de dependência no grafo de módulos — o que code-graph-cli chama de `circular`.

**O que o code-graph-cli faz**:
```
codegraph circular
→ detecta dependency cycles entre módulos
→ mostra o ciclo completo (path)
→ classifica por severidade (módulo único vs multi-crate)
```

**Gap atual no Touring**: `touring wiring status` mostra scores e integrities, mas não detecta ciclos específicos. petgraph tem `toposort` mas não é exposto como CLI.

**Design Spec**:

#### D1: CLI Interface
```bash
touring wiring cycles [--format text|json] [--min-depth 2]
touring wiring cycles --min-depth 3
```

**Output (text)**:
```
Dependency Cycles Detected: 3
━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━

Cycle #1 (depth: 4, modules: 3)
  touring-hooks::wiring/impact ↺
  touring-hooks::wiring/orphans
  touring-hooks::wiring/modules
  touring-hooks::wiring/impact   [CYCLE CLOSES]

Cycle #2 (depth: 2, modules: 2)
  touring-hooks::shared/gate_metrics ↺
  touring-hooks::shared/metrics

Note: Cycle #2 may be a false positive due to feature-gated code.
```

**Output (json)**:
```json
{
  "cycle_count": 3,
  "cycles": [
    {
      "id": 1,
      "depth": 4,
      "modules": ["touring-hooks::wiring/impact", "touring-hooks::wiring/orphans", "touring-hooks::wiring/modules"],
      "severity": "high"
    }
  ]
}
```

#### D2: Arquitetura

```rust
// wiring/cycles.rs
pub struct CycleDetector {
    graph: DiGraph<ModuleId, ()>,
}

impl CycleDetector {
    /// Returns all cycles in the dependency graph using Tarjan's algorithm
    pub fn find_all_cycles(&self) -> Vec<Cycle> {
        tarjan_scc(&self.graph)
    }
}

#[derive(Debug)]
pub struct Cycle {
    pub modules: Vec<ModuleId>,
    pub depth: usize,
    pub severity: CycleSeverity,
}
```

**Algoritmo**: Tarjan's Strongly Connected Components (SCC) — O(V+E), mesmo usado pelo Touring cognitive MCTS para skeleton analysis.

**Nota sobre false positives**: ciclos podem aparecer por feature gates (#[cfg(feature = "X")] que só ativa em builds específicas). O output deve notificar quando cycles contém módulos com feature-gated edges.

#### D3: Deliverables

| # | Deliverable | Local | Priority |
|---|-------------|-------|----------|
| 1 | `wiring/cycles.rs` — CycleDetector (Tarjan SCC) | touring-hooks/src/wiring/ | P0 |
| 2 | Handler `cli_wiring_cycles` | cli_handlers.rs | P0 |
| 3 | Registry entry | hook_registry.rs | P0 |
| 4 | CLI test: detect cycle | e2e test | P0 |
| 5 | CLI test: `--min-depth 3` filter | e2e test | P0 |
| 6 | Update `touring-cli-commands.md` | rules/ | P1 |

---

## PRIORIDADE MÉDIA

### 🟡 Feature 3: ACP (Agent Client Protocol) como protocolo formal de comunicação

**O que é**: ACP é o wire protocol que Zed Industries definiu para comunicação editor↔agent (similar ao LSP mas otimizado para agentes de IA). Ver https://agentic-coding.com.

**Análise do gap**: O Touring daemon já tem um socket Unix com JSON payload (similar em conceito ao ACP). A oportunidade não é "reescrever" mas definir uma **camada de protocol** sobre o socket existente que seguem padrões ACP-like (structured messages, capability negotiation, streaming responses).

**O que ACP traz de valor**:
1. **Capability negotiation** — cliente e servidor negociam features suportadas
2. **Streaming responses** — SSE-like para outputs longos (impact analysis pode ser grande)
3. **Message correlation** — IDs para correlacionar request/response
4. **Error taxonomy** — error codes estruturados (não só string messages)

**Design Spec**:

#### D1: Camada de Protocolo sobre Socket Existente

O Touring daemon NÃO precisa substituir seu socket — precisa de uma **camada de serialização** que:

1. Adota Message envelope com `id`, `method`, `params`, `correlation_id`
2. Adiciona capability negotiation no handshake inicial
3. Suporta streaming via chunked responses (Content-Length header em cada chunk)

**Schema do Message envelope**:
```json
{
  "jsonrpc": "2.0",
  "id": "req-001",
  "method": "wiring.impact",
  "params": { "symbol": "HookRuntime", "depth": 3 },
  "correlation_id": null
}
```

**Response**:
```json
{
  "jsonrpc": "2.0",
  "id": "req-001",
  "result": { "direct_consumers": 12, "total_transitive": 47 }
}
```

#### D2: Implementação — ACP Shim Layer

```rust
// protocol/acp.rs
pub mod acp {
    use serde::{Deserialize, Serialize};

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Message {
        pub jsonrpc: &'static str, // "2.0"
        pub id: String,
        pub method: String,
        pub params: serde_json::Value,
        #[serde(skip_serializing_if = "Option::is_none")]
        pub correlation_id: Option<String>,
    }

    #[derive(Debug, Clone, Serialize, Deserialize)]
    pub struct Capabilities {
        pub version: String,        // "1.0"
        pub streaming: bool,        // supports chunked responses
        pub impact_analysis: bool, // supports wiring.impact
        pub cycle_detection: bool,   // supports wiring.cycles
    }

    pub const PROTOCOL_VERSION: &str = "acp-1.0";
}
```

**Integração com daemon existente**: Feature flag `acp-protocol` (default off) — quando ativo, o socket handler parseia envelopes ACP e serializa responses como ACP.

**Nota**: ACP é primariamente um **research/integration** item. O touring-daemon socket JSON já funciona bem. A adoção de ACP completo requereria mudança no client (CLI) também. Recomendação: implementar como **opt-in layer** para projetos que já usam ACP nativamente (ex: Zed extension, Claude Code com ACP adapter).

#### D3: Deliverables

| # | Deliverable | Local | Priority |
|---|-------------|-------|----------|
| 1 | `protocol/acp.rs` — ACP message types + envelope | touring-hooks/src/protocol/ | P1 |
| 2 | Feature flag `acp-protocol` | Cargo.toml | P1 |
| 3 | Socket handler with ACP parsing (opt-in) | daemon.rs | P2 |
| 4 | ACP capability negotiation on connect | daemon.rs | P2 |
| 5 | Update `touring-cli-commands.md` | rules/ | P2 |

**Timeline**: Feature 3 é de escopo menor, pode ser implementada em 1 sprint paralelo ao Feature 4.

---

### 🟡 Feature 4: Hypergraph para relações poliádicas

**O que é**: hypergraph é um grafo onde **hyperedges** podem conectar múltiplos nodes (não só pares). code-graph-cli não tem hypergraph, mas a análise identificou que Touring tem casos de uso legítimos para isso.

**Casos de uso no Touring**:

1. **Cross-file refactor**: um símbolo em A.rs é usado em B.rs E C.rs simultaneamente — relation triple (A, B, C) não é díade, éトライade
2. **Multi-module imports**: `use foo::{A, B, C}` — uma linha de import conecta 1 a N
3. **Feature gates**: `#[cfg(all(feature = "X", feature = "Y"))]` — decisão de feature que ativa baseado em 2+ features (hyperedge)
4. **CRDT merge operations**: merge de 3+ replicas — relação N-ária
5. **ACO pheromone trails**: pheromone intensity influenced by multiple factors

**Design Spec**:

#### D1: Por que NÃO petgraph para hypergraph

petgraph é díade (Edge conecta exatamente 2 nodes). Hypergraph needs:
- Hyperedge conecta 2..N nodes
- Não direcionado por default (mas pode ter direção)
- Membership queries: "quais hyperedges incluem este node?"

#### D2: Alternativas evaluated

| Library | Pros | Cons |
|---------|------|------|
| **hypergraph** (crates.io) | Rust-native, well-tested | zero deps philosophy vs petgraph dependency |
| **petgraph + wrapper** | reuse petgraph, no new dep | hack: N-ary edge = artificial node |
| **custom** | zero deps, match exact needs | maintenance burden |

**Recommendation**: **petgraph + artificial node pattern** (minimal deps, reuse existing infrastructure). Um hyperedge é um node especial com `HyperEdge` marker que conecta aos membros via díades.

```rust
// hypergraph.rs
use petgraph::graph::{DiGraph, NodeIndex};

/// Hypergraph via petgraph: hyperedges are artificial nodes with marker type
pub struct HyperGraph<N, E> {
    graph: DiGraph<HyperNode<N>, HyperEdge<E>>,
    members: FxHashMap<NodeId, Vec<NodeIndex>>, // node -> hyperedge memberships
}

#[derive(Debug, Clone)]
enum HyperNode<N> {
    Real(NodeData<N>),
    HyperEdge(HyperEdgeMarker),
}

#[derive(Debug, Clone)]
struct HyperEdge<E> {
    data: E,
    member_count: usize,
}

/// Create hyperedge connecting nodes A, B, C
pub fn create_hyperedge(g: &mut HyperGraph, data: E, members: &[NodeId]) {
    let edge_node = g.graph.add_node(HyperNode::HyperEdge(HyperEdgeMarker));
    for member in members {
        g.graph.add_edge(member, edge_node, ());
        g.graph.add_edge(edge_node, member, ());
        g.members.entry(*member).or_default().push(edge_node);
    }
}
```

**Nota sobre deps**: não adicionar hypergraph crate como dependency. A pattern acima reutiliza petgraph que já está em todos os 10 Touring crates.

#### D3: Casos de uso concretos para implementação

| Caso | Implementação | Priority |
|------|--------------|----------|
| Cross-file symbol impact (Feature 1) | Hyperedge(symbol, [consumers]) | P0 — ja coberto por Feature 1 |
| Feature gate analysis | Hyperedge(feature_combo, [cfg_features]) | P1 |
| Multi-import lines | Hyperedge(import_line, [imported_symbols]) | P1 |
| CRDT merge graph | Hyperedge(merge_op, [replicas]) | P2 — deferred |

#### D4: Deliverables

| # | Deliverable | Local | Priority |
|---|-------------|-------|----------|
| 1 | `hypergraph.rs` — HyperGraph wrapper over petgraph | touring-hooks/src/wiring/ | P1 |
| 2 | `FeatureGateHyperedge` — hyperedge for cfg combinations | touring-hooks/src/wiring/ | P2 |
| 3 | `MultiImportHyperedge` — hyperedge for import lines | touring-hooks/src/wiring/ | P2 |
| 4 | Tests for hyperedge operations | hypergraph tests | P1 |
| 5 | Update `touring-cli-commands.md` | rules/ | P2 |

---

## Phase 0 — Health Gate

```bash
# Compilation check
cargo check --workspace 2>&1 | grep "^error\[" | wc -l  # expect: 0

# Touring daemon health
touring doctor -j 2>/dev/null | jq '.daemon_socket.status'  # expect: ok

# Pre-condition: touring wiring status works
touring wiring status -j | jq '.orphan_count'  # expect: > 0
```

---

## Implementation DAG

```
[F0: Health Gate]
       │
       ▼
┌─────────────────────────────────────────────────────────┐
│ F1: touring wiring impact <symbol>                      │
│   ├─ D1: ImpactAnalyzer struct                         │
│   ├─ D2: CLI handler cli_wiring_impact                  │
│   ├─ D3: Registry entry                                │
│   └─ D4: E2E tests                                     │
└────────────────────────┬────────────────────────────────┘
                         ▼
┌─────────────────────────────────────────────────────────┐
│ F2: touring wiring cycles                              │
│   ├─ D1: CycleDetector (Tarjan SCC)                   │
│   ├─ D2: CLI handler cli_wiring_cycles                 │
│   ├─ D3: Registry entry                                │
│   └─ D4: E2E tests                                     │
└────────────────────────┬────────────────────────────────┘
                         ▼
┌─────────────────────────────────────────────────────────┐
│ F3: ACP protocol layer (opt-in)                       │
│   ├─ D1: ACP message types                            │
│   ├─ D2: Feature flag acp-protocol                    │
│   └─ D3: Socket handler integration                   │
└────────────────────────┬────────────────────────────────┘
                         ▼
┌─────────────────────────────────────────────────────────┐
│ F4: Hypergraph for polyadic relations                 │
│   ├─ D1: HyperGraph wrapper (petgraph-based)          │
│   ├─ D2: FeatureGateHyperedge                        │
│   └─ D3: MultiImportHyperedge                         │
└─────────────────────────────────────────────────────────┘
```

**Parallelism**: F1 e F2 são independentes e podem ser executados em paralelo (dois engineers). F3 e F4 dependem de F1+F2 estar completo (usam infraestrutura de wiring).

---

## TACO Phase Execution Plan

### L3+ Full Phase Protocol

| Phase | Action | Mode |
|-------|--------|------|
| FASE 0 | Health gate: cargo check + touring doctor | solo |
| FASE 1 | Scout: verify impact/cycles gap exists via touring index | solo |
| FASE 2 | Architect: design ImpactAnalyzer + CycleDetector specs | touring-architect |
| FASE 3 | Context7: cross-reference petgraph docs for SCC algorithms | touring-architect |
| FASE 4 | Decompose: create DAG with F1 + F2 subtasks | solo |
| FASE 4.5 | Pre-implementation audit: verify no duplicate work | touring-auditor |
| FASE 5 | Engineers: F1 (engineer-1) + F2 (engineer-2) parallel | touring-engineer × 2 |
| FASE 6 | Post-implementation audit: E2E + wiring orphan check | touring-auditor |
| FASE 7 | Documentation: update CLI commands + skills | touring-scriber |

**Estimated time**: 2 sprints (F1+F2: 1 sprint, F3+F4: 1 sprint)

---

## Risks and Mitigations

| Risk | Probability | Impact | Mitigation |
|------|-------------|--------|------------|
| petgraph não escala para hypergraph | LOW | HIGH | Usar artificial node pattern (já testado em CRDT graph) |
| Cycles em feature-gated code geram false positives | MEDIUM | MEDIUM | Adicionar `--ignore-feature-gated` flag + warning no output |
| ACP opt-in adiciona complexidade ao socket handler | MEDIUM | LOW | Manter como feature flag, default OFF |
| Impact analysis pode ser lento em repos com 50k+ symbols | MEDIUM | MEDIUM | Cache de resultados + budget 100ms com timeout |

---

## Success Metrics

| Metric | Target | Measurement |
|--------|--------|-------------|
| `touring wiring impact HookRuntime` | < 100ms | `time touring wiring impact HookRuntime` |
| `touring wiring cycles` | < 200ms | `time touring wiring cycles` |
| E2E tests F1 | 100% PASS | `touring e2e -j` |
| E2E tests F2 | 100% PASS | `touring e2e -j` |
| New orphans introduced | 0 | `touring wiring orphans -j` delta |
| Hypergraph integration | petgraph reused | Zero new dependencies |
| ACP layer | Opt-in, no regression | Default off = existing behavior preserved |

---

## Files to Create

```
crates/touring-hooks/src/wiring/
├── impact.rs      # ImpactAnalyzer (F1)
└── cycles.rs      # CycleDetector, Tarjan SCC (F2)

crates/touring-hooks/src/wiring/
└── hypergraph.rs  # HyperGraph wrapper (F4)

crates/touring-hooks/src/protocol/
└── acp.rs        # ACP message types (F3)

Total: 4 new files
Total LOC: ~600 (impact: 200, cycles: 150, hypergraph: 150, acp: 100)
```

---

## References

- code-graph-cli analysis: `~/.claude/rust/docs/2026-04-24-cronflow-touring-enhancement-plan.md`
- Touring wiring current state: `touring wiring status -j`
- petgraph docs: `https://docs.rs/petgraph/`
- ACP protocol: `https://agentic-coding.com`
- Tarjan's SCC: O(V+E) — mesma implementação usada em cognitive MCTS skeleton analysis

---

**Gabriel approval required before FASE 1 execution.**