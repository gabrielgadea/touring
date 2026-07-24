# Estado Atual do Touring vs Propostas de Implementação

**Data:** 19/04/2026 | **Versão:** v30.3.1 | **Crates:** 15 | **Testes:** 5.154+

---

## Status por Proposta

| ID | Proposta | Status | Evidência | O que falta |
|----|----------|--------|-----------|-------------|
| H1.1 | Extração touring-cortex + touring-index | ✅ COMPLETO | Crates existem no workspace | — |
| H1.2 | Sessões Persistentes Zero-Copy (rkyv + SQLite) | ⚠️ PARCIAL | `rkyv` disponível no workspace; `CrdtSemanticGraph` já persiste via rkyv+mmap2; `SessionManager` existe em `touring-server/src/session/manager.rs` mas gerencia métricas, não GoT state | Criar `GoTSnapshot` struct, tabela `sessions` no SQLite, `SessionPersistence` em `touring-cognitive`, conectar ao daemon |
| H1.3 | Learned System Reminders via LinUCB | ⚠️ PARCIAL | `LinUcb` completo em `touring-learning/src/bandit/linucb.rs`; `TemplateLibrary` + UCB1 existem em `touring-learning/src/templates/`; comentário no código: "Unlike the bandit module (LinUCB for *what type* of context to inject)..." — arquitetura já prevê isso | Criar `ReminderBandit` wrapper; conectar LinUCB ao prompt-enhancer hook; definir pool de candidatos de reminders; wiring com reward signal |
| H2.1 | GoT Parallel Execution via Tokio Actors | ⚠️ PARCIAL | `tokio::spawn` existente em `touring-cognitive` (predictor_task, mcts_streaming); `got.rs` existe mas nota que requer `'static` bounds; Tokio `full` features já declaradas no workspace | Criar `GotSemanticNodeActor`, implementar `JoinSet` para hipóteses paralelas — `JoinSet` ainda NAO usado em touring-cognitive |
| H2.2 | Memória Episódica com Call Graphs + RRF | ❌ PENDENTE | `semantic_search.rs` em touring-ast usa scores mas sem RRF; `memory/recall.rs` existe mas usa BM25 simples; nenhum `call_graph` ou RRF encontrado | Implementar `reciprocal_rank_fusion()` em touring-cortex; criar estrutura de call graph; integrar com blast_radius existente |
| H2.3 | Multi-Agent P2P com diamond-types CRDTs | ⚠️ PARCIAL | `crdts = "7.3"` já no workspace Cargo.toml; `CrdtSemanticGraph` hand-rolled em `touring-learning/src/memory/crdt_graph.rs` com OR-Set + LWW semantics + rkyv persistence; P2P wire protocol faltando | `diamond-types` crate NAO presente; mas CRDT hand-rolled já funcional — avaliar se diamond-types adiciona valor sobre implementação existente |
| H2.4 | Wasmtime Inferlets (touring-wasm) | ⚠️ PARCIAL | `touring-wasm` crate completo com `consume_fuel(true)`, `set_fuel(MAX_FUEL=10M)`, sandbox, import allowlist — tudo implementado em `runner.rs`; sem `async_support`, sem `PoolingAllocationStrategy` | Adicionar `async_support(true)` e `PoolingAllocationStrategy` ao Config; criar `InferletPool` para pre-warm 100 instâncias; definir interface WIT |
| H2.5 | petgraph Blast Radius Enriched | ⚠️ PARCIAL | `petgraph` no workspace; usado em: `dependency_cache.rs` (StableGraph + BFS para blast radius), `semantic_graph.rs` (StableGraph), `cross_validator.rs` (kosaraju_scc), `aco/graph.rs` (toposort); `dependency_cache.rs` JA implementa blast radius com petgraph BFS | Substituir BFS por `Topo` incremental; adicionar `tarjan_scc()` para detecção de ciclos; expor `DependencyGraph` struct como API pública |
| H2.6 | MCTS TinyTransformerPredictor (candle) | ⚠️ PARCIAL | `TinyTransformerPredictor` completo em `touring-learning/src/rl/tiny_transformer.rs` usando ndarray (CPU-only, 2 layers, 8 heads, d_model=64); `cognitive_mcts.rs` + `mcts.rs` + `mcts_streaming.rs` existentes | `candle` crate NAO presente — implementação atual usa ndarray (sem GPU). Integrar TinyTransformer com MCTS como prior para seleção de nós. Migrar para candle apenas se GPU necessária |
| H3.1 | BranchFS Copy-on-Write Integration | ❌ PESQUISA | syscall `branch()` não existe no Linux mainline | Implementar via overlayfs + user namespaces como alternativa (~10-50ms overhead) |
| H3.2 | BR_MEMORY + Effect Gating | ❌ PESQUISA | Dependente de H3.1 | Bloqueado por H3.1 |
| H3.3 | eBPF + KS-drift Telemetry | ⚠️ PARCIAL | `touring-server/src/telemetry/mod.rs` existe; `statrs = "0.17"` no workspace (inclui KS test); nenhum `aya` ou eBPF encontrado | Adicionar `aya` crate; implementar eBPF program para kernel tracing; conectar KS-drift ao DriftMonitor; requer kernel 5.8+ com BTF |

---

## Dependências Ausentes no Cargo.toml

| Dependência | Necessária para | Prioridade |
|-------------|-----------------|------------|
| `deadpool-sqlite` | H1.2 (async SQLite pool) | P0 — quick win |
| `diamond-types` | H2.3 (P2P CRDTs) | P2 — CRDT hand-rolled já funciona |
| `candle-core`, `candle-nn` | H2.6 (TinyTransformer GPU) | P3 — ndarray já funcional |
| `aya`, `aya-ebpf` | H3.3 (eBPF telemetry) | P3 — pesquisa |
| `wit-bindgen` | H2.4 (Wasmtime WIT interface) | P1 |

### Dependências JA presentes que desbloqueiam implementações

| Dependência | Versão | Desbloqueia |
|-------------|--------|-------------|
| `rkyv` | 0.7 (workspace) | H1.2 sessões, H2.3 CRDT persistence |
| `petgraph` | 0.6 (workspace + 6 crates) | H2.5 blast radius enriched |
| `wasmtime` | 42 (workspace) | H2.4 inferlets |
| `crdts` | 7.3 (workspace) | H2.3 P2P CRDTs |
| `tokio` | 1.40 full (workspace) | H2.1 GoT actors |
| `statrs` | 0.17 (workspace) | H3.3 KS-drift (user-space) |
| `ndarray` | 0.16 (workspace) | H2.6 TinyTransformer (já funcionando) |
| `LinUcb` | — (touring-learning) | H1.3 ReminderBandit |

---

## Quick Wins com Infraestrutura Ja Presente

Estas propostas podem começar HOJE porque a infraestrutura core ja existe:

### 1. H2.4 — Wasmtime Inferlets: PoolingAllocationStrategy (1 dia)
**O que existe:** `touring-wasm/src/runner.rs` ja tem `consume_fuel(true)`, `set_fuel(10M)`, sandbox completo.
**O que falta:** Adicionar `PoolingAllocationStrategy` + `async_support(true)` ao `Config::new()` — mudanca cirurgica de 10 linhas.

### 2. H1.3 — ReminderBandit via LinUCB (2-3 dias)
**O que existe:** `LinUcb` completo e testado em `touring-learning/src/bandit/linucb.rs`; `TemplateLibrary` com UCB1 ja em `touring-learning/src/templates/`.
**O que falta:** Criar `ReminderBandit` struct que wrapa `LinUcb`; definir pool de candidatos; conectar no `prompt_enhance.rs`.

### 3. H2.5 — petgraph Blast Radius com tarjan_scc (3-4 dias)
**O que existe:** `dependency_cache.rs` ja usa petgraph `StableGraph + BFS` para blast radius. `cross_validator.rs` ja usa `kosaraju_scc`. `aco/graph.rs` ja usa `toposort`.
**O que falta:** Substituir BFS por `Topo` incremental; adicionar `tarjan_scc()` para ciclos; expor API publica.

### 4. H2.1 — JoinSet para GoT hipoteses paralelas (1-2 semanas)
**O que existe:** `tokio::spawn` ja usado em `touring-cognitive`; `got.rs` existente.
**O que falta:** Criar `GotSemanticNodeActor`; usar `JoinSet` para coleta paralela; configurar early-exit quando confidence > 0.8.

### 5. H1.2 — GoTSnapshot para sessoes persistentes (1-2 semanas)
**O que existe:** `rkyv` disponivel; `CrdtSemanticGraph::save_to_mmap` ja demonstra pattern rkyv+mmap2; `SessionManager` em touring-server para lifecycle.
**O que falta:** Criar `GoTSnapshot` struct derivando rkyv; tabela `sessions` no SQLite schema; `SessionPersistence` em touring-cognitive; `deadpool-sqlite` para async pool.

---

## Estado Real dos Crates Chave

| Crate | Estado Relevante para Propostas |
|-------|--------------------------------|
| `touring-wasm` | `consume_fuel(true)` ja ativo; sem async, sem pooling — H2.4 85% completo |
| `touring-learning` | LinUCB, TinyTransformer, CrdtGraph, TemplateLibrary, ACO — infraestrutura rica |
| `touring-cognitive` | MCTS (3 arquivos), GoT, predictor_task com tokio::spawn — H2.1 bem posicionado |
| `touring-hooks` | dependency_cache.rs com petgraph blast radius — H2.5 ja parcialmente presente |
| `touring-antt` | kosaraju_scc, DiGraph — experiencia com SCC ja no codebase |
| `touring-server` | SessionManager (metrics), telemetry/mod.rs — H3.3 user-space ja estruturado |

---

## Sumario Executivo de Estado

**Muito mais implementado do que o touring-pro.md sugeria:**

- **touring-wasm (H2.4):** Nao e "feature-gated pendente" — esta ~85% implementado com fuel, sandbox e allowlist. Falta so PoolingAllocationStrategy + async.
- **petgraph blast radius (H2.5):** Nao e "pendente" — dependency_cache.rs JA usa petgraph BFS para blast radius. Falta apenas upgradar para Topo + tarjan_scc.
- **TinyTransformer (H2.6):** JA existe em touring-learning usando ndarray. Candle seria upgrade de GPU, nao criacao do zero.
- **CRDT (H2.3):** CrdtSemanticGraph hand-rolled JA funcional com OR-Set + LWW + rkyv persistence. diamond-types seria alternativa, nao prerequisito.
- **LinUCB (H1.3):** JA testado e funcionando. Falta apenas o wrapper ReminderBandit + wiring.
- **rkyv (H1.2):** JA no workspace e em uso. Falta apenas GoTSnapshot struct + sessoes table.

**Dependencias criticas ausentes (apenas 2 para quick wins):**
1. `deadpool-sqlite` — async connection pool para H1.2
2. `wit-bindgen` — contrato WIT para H2.4 completo

**Ordem de implementacao recomendada (revisada):**
1. H2.4 PoolingAllocationStrategy (1 dia — mudanca cirurgica)
2. H2.5 Topo + tarjan_scc no dependency_cache (2-3 dias)
3. H1.3 ReminderBandit (2-3 dias — LinUCB ja pronto)
4. H1.2 GoTSnapshot + deadpool-sqlite (1 semana)
5. H2.1 JoinSet actors no GoT (1-2 semanas)
6. H2.2 RRF + call graphs (2-3 semanas)
7. H2.3 diamond-types ou melhorar CRDT existente (2-3 semanas)
8. H3.3 eBPF + aya (pesquisa, 3-4 semanas)
9. H2.6 candle integration para TinyTransformer (apenas se GPU necessaria)

---

*Gerado por TACO Orchestrator N2 v4.0 em 26/03/2026 | Baseline: v21.1.0 | Scout mode: SOLO*

---

## Plano de Implementação

**Arquivo:** [`touring-implementation-plan.md`](touring-implementation-plan.md)
**Gerado em:** 26/03/2026
**Estrutura:** 8 sprints + backlog

| Sprint | Foco | Duração | Meta Testes |
|---|---|---|---|
| S0 | Deps: deadpool-sqlite + wasmtime async/pooling | < 1 dia | 2.671 |
| S1 | Quick Wins: H2.4 pool + H2.5 tarjan + H1.3 ReminderBandit | 3-5 dias | ~2.682 |
| S2 | H2.1 GoT Actors JoinSet | 1-2 semanas | ~2.706 |
| S3 | H1.2 GoTSnapshot + deadpool-sqlite | 1 semana | ~2.721 |
| S4 | H2.4 InferletPool + WIT interface | 2-3 semanas | ~2.741 |
| S5 | H2.2 RRF + Call Graphs | 2-3 semanas | ~2.766 |
| S6 | H2.3 CRDTs P2P delta/merge | 2-3 semanas | ~2.786 |
| S7 | H3.3 KS-drift + eBPF | 3-4 semanas | ~2.801 |
| S8 | H3.1/H3.2 BranchFS overlayfs | 4-6 semanas | PESQUISA |

**Próximo passo imediato:** Sprint 0 — modificar `Cargo.toml` com 3 mudanças de deps.
