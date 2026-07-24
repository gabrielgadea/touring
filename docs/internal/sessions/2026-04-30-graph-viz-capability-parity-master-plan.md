# Touring — Graph Visualization, Capability-Parity, Hybrid Semantic Search, Knowledge Layer & CC Integration Master Plan

> **Created**: 2026-04-30 (v1) | **Updated v2**: 2026-04-30 (CodeWeaver) | **Updated v3**: 2026-05-01 (Thread T1-T13 → D29-D41) | **Updated v4**: 2026-05-01 (CC Integration foco — CC1-CC15 → D42-D49)
> **Author**: TACO orchestration | **Status**: PROPOSED
> **Inputs v1**: Graphviz, cargo-depgraph (jplatte), code-graph-ai (MonsieurBarti), best practices 2024–2025 em large graph viz (FDEB/SBEB/HEB)
> **Inputs v2**: knitli/codeweaver (Python, 9★, alpha→0.x — arquitetura sofisticada: hybrid search + intent + family-aware checkpoint + failover + 17 embedding providers)
> **Inputs v3**: knitli/thread (Rust, 3★, AGPLv3 alpha — high-perf code analysis platform: service-library dual arch + ThreadFlowBuilder + YAML rule engine + semantic classification 99.7%/27 langs + Multi-Resolution Knowledge Layer + Cloudflare Workers Edge + Postgres/D1/Qdrant)
> **Inputs v4**: re-análise dos 3 repos sob lens "Claude Code integration" — code-graph-ai `setup.rs` (`include_str!` + permission auto-add `Bash(code-graph *)`), Thread 10 speckit slash commands (`.claude/commands/speckit.{analyze,plan,specify,implement,...}`) com `handoffs:` frontmatter, Thread `.claude/plugins/ctx/` plugin system per-project, code-graph-cli PreToolUse Grep/Glob enrichment com symbol patterns (massive token saving)
> **Touring baseline**: v30.3.0 | 81 CLI cmds + 24 hooks | 96 MCP tools | 176 hook registry | 45 synergy WIRED_PAIRS
> **Target**:
> - **v30.5.0** (Wave 1+2 obrigatórias — visual foundation + rich encoding + intent + RRF + chunker resilience + governor + profiles)
> - **v30.7.0** (Wave 3 ship — capability parity + move detection + **D37 Overlay Graph**)
> - **v30.9.0** (Wave 4 ship — resilience patterns: family-aware checkpoint + failover + rignore + node types KB + **D31 semantic classification + D33 conflict tier SLAs**)
> - **v31.0.0** (Wave 5 ship — hybrid semantic search: embeddings + vector store + reranking + manifest integration)
> - **v31.2.0** (Wave 6 ship — agent UX: find_code super-tool MCP)
> - **v31.5.0** (Wave 7 ship — plugin DI + cost reporting + **D29 FlowBuilder + D30 YAML rules + D32 tier UX + D38 perf benchmarks**)
> - **v32.x** (Wave 8 — opcional/research: Web UI + FDEB + DSL + **D34 Postgres + D35 Edge + D36 bidir sync + D37 Overlay extended + D39 MVKL + D40 Unison-store + D41 CGM**)

---

<objective>

## 1. Objective

Fechar **7 gaps competitivos** identificados em três análises comparativas (v1: cargo-depgraph + code-graph-ai + Graphviz; v2: CodeWeaver; v3: Thread), **sem comprometer** a primazia analítica do Touring (RL LinUCB+QTable, speculative validation, generator typestate 5-stage, memory tiers, 96 MCP tools, 176 hooks, 5,100+ tests):

### Gaps v1 (visual & capability)

1. **Visual Export** — Touring possui rica análise estrutural (`graph file|god-nodes|shortest-path|communities`, `wiring impact|cycles|orphans`, `ast blast|tdg|rust-semantic`), mas emite apenas JSON. Falta camada de tradução para DOT/Mermaid/SVG, perdendo 10× em comunicabilidade vs cargo-depgraph e code-graph-ai.

2. **Capability Parity com code-graph-ai** — concorrente direto (39k LOC Rust, 25 commands, MCP integration) tem 6 features triviais sobre o dado existente: `flow A→B`, `rename --plan`, `snapshot create/diff`, `clones`, `confidence tiers`, `Reciprocal Rank Fusion`.

3. **Reduções para grafos densos** — workspace tem 38815 symbols + 18957 orphans (W1: até 199818 reportados em algumas leituras — possible WIRING_DB_ANOMALY). Render direto é ilegível. Falta `transitive reduction` (tred), `--max-nodes/--max-edges`, e edge bundling (FDEB).

### Gaps v2 (CodeWeaver — search & resilience)

4. **Hybrid Semantic Search** — CodeWeaver tem dense embeddings (17 providers) + sparse (BM25/SPLADE) + RRF + reranking (5 providers cascading) + intent-aware boost (até 20%). **Touring tem só BM25 (tantivy) + index find separados — gap competitivo crítico** vs CodeWeaver, code-graph-ai, Cursor, Continue.dev. Conceito como "where do we handle retries?" não funciona sem dense vectors. Voyage Code-3 paper mostra +14.52% precision com hybrid vs dense-only.

5. **Resilience Patterns Maduros** — CodeWeaver implementa **CheckpointSettingsFingerprint family-aware** (asymmetric embeddings: query model muda sem reindex se família igual), **VectorReconciliationService** (lazy repair de backup vectors missing), **FailoverService cross-subsystem** (primary↔backup com zero functionality loss durante transição), **FileManifestManager** (move detection por hash). Touring tem circuit breaker mas falta orquestração unificada.

6. **Agent UX & Cognitive Load Reduction** — CodeWeaver expõe 1 tool (`find_code(description)`) com prompt overhead de ~500 tokens vs ~16,000 em Serena (21 active tools). Filosofia "Agent-first tools" reduz cognitive load (context poisoning + wrong tool selection). Touring tem 96 MCP tools — adicionar 1 super-tool agent-friendly (sem remover os 96) maximiza ROI sem regressão.

### Gap v3 (Thread — knowledge layer & dataflow extensibility)

7. **Knowledge Layer & Dataflow Extensibility** — Thread implementa: (a) **ThreadFlowBuilder** declarative API (fluent dataflow pipeline com 4 steps + 2 targets); (b) **YAML Rule Engine + fix transformations** (config-driven autofix com `pattern → fix`, sem tocar Rust); (c) **Semantic Classification System** (22 categories language-agnostic, 99.7% accuracy across 27 langs, ~1,800 LOC data-driven, ~41 LOC TOML override por lang); (d) **Tier-based language support** UX honest (Tier 1-4); (e) **Multi-tier conflict detection com SLAs explícitos** (<100ms/<1s/<5s); (f) **Overlay Graph Architecture** (Base immutable + Delta ephemeral + Unified View merged at query); (g) **AI-Native Knowledge Layer Multi-Resolution MVKL** (Levels 0-2: file index + parsed definitions + semantic graph) — _"the graph is the source of working truth; files are projections"_. Touring tem `decompose` mas não fluent builder; tem `assists` mas não YAML rules; tem cognitive mas não classification formal data-driven; tem 16+ langs mas sem tier UX; tem snapshots planejados (D8) mas não overlay; tem cognitive::semantic_graph mas não Multi-Resolution Knowledge Layer.

### 1.1 Success criteria (mensuráveis)

| Métrica | Baseline | W1 | W2 | W3 | W4 | W5 | W6 | W7 |
|---|---|---|---|---|---|---|---|---|
| `touring graph` subcmds com export visual | 0 (JSON) | 4 | 4 | 4 | 4 | 4 | 4 | 4 |
| `touring viz` novos cmds | 0 | 0 | 5 | 5 | 5 | 5 | 5 | 5 |
| One-liner `cmd \| dot -Tsvg` | ❌ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Mermaid export GitHub-renderable | ❌ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `--max-nodes/--max-edges/--reduce` cap | ❌ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| RRF unified search (tantivy + index + fuzzy) | ❌ | ❌ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| **Intent classification + semantic weighting** | ❌ | ❌ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| **GracefulChunker fallback chain** | ❌ | ❌ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| **ResourceGovernor unificado** | ad-hoc | ad-hoc | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| **`touring init --profile` UX** | ❌ | ❌ | ✅ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `flow A→B` paths | ❌ | ❌ | ❌ | ✅ | ✅ | ✅ | ✅ | ✅ |
| `rename --plan` | ❌ | ❌ | ❌ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Snapshot create/list/diff/diff-impact | ❌ | ❌ | ❌ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Clone detection (signature hashing) | ❌ | ❌ | ❌ | ✅ | ✅ | ✅ | ✅ | ✅ |
| Confidence tiers configuráveis | numeric | numeric | tier+config | tier+config | tier+config | tier+config | tier+config | tier+config |
| **Move detection (incremental dedup)** | ❌ | ❌ | ❌ | ✅ | ✅ | ✅ | ✅ | ✅ |
| **CheckpointSettingsFingerprint family-aware** | ❌ | ❌ | ❌ | ❌ | ✅ | ✅ | ✅ | ✅ |
| **FailoverService cross-subsystem** | circuit-only | circuit-only | circuit-only | circuit-only | ✅ | ✅ | ✅ | ✅ |
| **rignore-style filtering audited** | partial | partial | partial | partial | ✅ | ✅ | ✅ | ✅ |
| **Node-types JSON KB exposto** | implicit | implicit | implicit | implicit | ✅ | ✅ | ✅ | ✅ |
| **Embedding provider abstraction** | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ | ✅ | ✅ |
| **Vector store (sqlite-vec / Qdrant)** | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ | ✅ | ✅ |
| **Hybrid scoring (dense+sparse+RRF+rerank)** | ❌ | ❌ | partial(RRF) | partial | partial | ✅ | ✅ | ✅ |
| **Asymmetric embeddings + manifest** | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ | ✅ | ✅ |
| **`touring find_code` super-tool MCP** | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ | ✅ |
| **Plugin DI runtime swap** | typestate-only | typestate-only | typestate-only | typestate-only | typestate-only | typestate-only | typestate-only | ✅ |
| **MCP overhead self-report** | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ |
| Hook Registry | 176 | 178 | 180 | 183 | 187 | 192 | 194 | 197 |
| Synergy WIRED_PAIRS | 45 | 47 | 49 | 51 | 54 | 60 | 63 | 66 |
| Workspace test count | 5,100+ | 5,123 | 5,153 | 5,197 | 5,261 | 5,461 | 5,521 | 5,571 |
| Total LOC delta cumulativo (core) | 0 | +600 | +1,710 | +3,470 | +4,720 | +7,720 | +8,320 | +9,120 |
| `cargo check --workspace` | exit 0 | exit 0 | exit 0 | exit 0 | exit 0 | exit 0 | exit 0 | exit 0 |
| `touring doctor -j` | 5/5 ok | 5/5 ok | 5/5 ok | 5/5 ok | 5/5 ok | 5/5 ok | 5/5 ok | 5/5 ok |

### 1.2 Scope explicito

**IN-SCOPE (Waves 1-7 — core)**:
- Modificar `touring-server/src/cli/graph.rs` para suportar `--format dot|mermaid|json|svg` + `--max-nodes/--max-edges/--reduce`
- Criar `touring-server/src/cli/viz.rs` (novo top-level: workspace/blast/wiring/cycles/orphans/feature)
- Adicionar serializers em `touring-server/src/visual/` (dot, mermaid, encoding, layout, theme, cap, tred, flow, bundling)
- Estender daemon handlers em `touring-hooks::cli_handlers` para retornar metadados ricos (cores, formas, edges classificados, tiers)
- Adicionar daemon handlers (`cli-graph-flow`, `cli-graph-rename-plan/apply`, `cli-graph-snapshot-*`, `cli-graph-clones`, `cli-search-unified`, `cli-find-code`)
- Implementar RRF + intent classification em novo `touring-search-fusion` ou estender `touring-tantivy`
- Implementar `GracefulChunker` + `ResourceGovernor` em `touring-core::chunker` (refactor patterns existentes)
- Profiles (`recommended/quickstart/airgapped/ci`) em `~/.claude/touring/profiles/*.toml` consumidos por `touring init`
- Move detection em `touring-vfs::manifest` (hash matching cross-path)
- `CheckpointSettingsFingerprint` family-aware em `touring-core::checkpoint`
- `FailoverService<P,B>` trait genérico em `touring-core::failover` (substitui circuit breaker ad-hoc)
- rignore audit + node-types KB JSON exposure
- Novo crate `touring-embeddings` (provider abstraction trait + Candle/FastEmbed providers)
- Novo crate `touring-vector-store` (trait + sqlite-vec local + Qdrant remoto)
- Hybrid scoring + reranking pipeline + asymmetric embeddings
- 1 novo MCP tool `touring_find_code` que orquestra hybrid search + intent + RRF + reranking
- Plugin DI runtime swap (Provider trait + ProviderRegistry)
- MCP overhead cost reporting
- Documentação: SKILL.md update + ~10 novos references + changelog entries por wave

**OUT-OF-SCOPE (Wave 8 — opcional, gated por demand validation)**:
- Web UI Axum+Svelte+WebGL (D10) — avaliar demanda Gabriel antes
- FDEB Force-Directed Edge Bundling (D11) — opt-in flag, defaults dos Holten/van Wijk 2009
- gvpr-inspired filter DSL (D12) — pode ficar shell+jq composition
- RAG conversational agent — alto risco/baixo ROI imediato
- LSP server-side embedding — paralelo, fora do escopo
- Migrar storage SQLite → petgraph in-memory (storage atual é adequado)
- Substituir sistema RL atual por outra arquitetura (LinUCB+QTable está ótimo)
- Substituir generator typestate (5-stage Draft→Verified→Rendered→Speculated→Committed está ótimo)

</objective>

---

## 2. Terreno mapeado (FACTS verificados)

### 2.1 Componentes para Waves 1-3 (visual + capability)

| Componente | Status | Localização | Observações |
|---|---|---|---|
| `touring graph` CLI dispatcher | ✅ existe | `touring-server/src/cli/graph.rs` (48 LOC) | Wrapper trivial que delega via `daemon_query("cli-ast-blast"\|"cli-wiring-modules", ...)` e printa JSON. **Easy to extend.** |
| Daemon graph handlers | ✅ existem | `touring-hooks/src/cli_handlers.rs` (8249 LOC) | God-file. Handlers `cli_search_symbols` em 6371, `cli_search_docs` em 6411. Graph handlers idem (precisa localizar exato). |
| Graph service backend | ✅ existe | `touring-server/src/graph_service.rs` + tests | Já tem cache moka dual e graph indices (W2 wave moka). |
| Call graph | ✅ existe (3 lugares) | `touring-cortex::call_graph`, `touring-ast::call_graph`, `touring-cognitive::semantic_graph` | Múltiplas representações; verificar qual é canônico para `flow A→B`. |
| `petgraph` | ✅ no workspace | `Cargo.toml: petgraph = { version = "0.6", features = ["serde-1"] }` | `petgraph::algo::all_simple_paths` disponível. |
| `syntect`, `tantivy`, `syn`, `prettyplease`, `cargo_metadata`, `ast-grep` | ✅ todos | workspace deps | Wave 4 (2026-04-18) já wired. |
| `dot`/`tred`/`gvpr` binários | ⚠️ assumido instalado | sistema | Apenas usados como pipe externo opcional. Touring não embute. |
| `update-touring` v2 | ✅ disponível | `~/.local/bin/update-touring` | Pipeline canônica para rebuild + dual-target install + restart. |

### 2.2 Componentes para Wave 2 (intent + chunker + governor + profiles)

| Componente | Status | Localização | Observações |
|---|---|---|---|
| Chunker existente | ✅ existe | `touring-ast::chunker` (verify), `touring-vfs::chunker` (verify) | Chain-of-fallback formal não documentado; refactor em GracefulChunker pattern. |
| ResourceGovernor candidate sites | ⚠️ ad-hoc | timeouts dispersos em `touring-hooks::cli_handlers`, `touring-tantivy`, `touring-ast` | Refactor em `touring-core::governor` unified. |
| Tantivy + index find | ✅ existem | `touring-tantivy`, `touring-index` | RRF fusion entre eles é S-effort. |
| `touring init` CLI | ⚠️ verify | `touring-server/src/cli/init.rs` (provável) | Profiles necessitam: criar `~/.claude/touring/profiles/*.toml` + sub-cmd `touring init --profile <name>`. |
| Configuração TOML | ✅ provável | `touring-core::config` | Adicionar `[blast]`, `[search]`, `[chunker]` sections. |

### 2.3 Componentes para Wave 4 (resilience patterns)

| Componente | Status | Localização | Observações |
|---|---|---|---|
| Checkpoint manager existente | ✅ existe | `touring-core::checkpoint` (verify) | Adicionar `CheckpointSettingsFingerprint` family-aware sobre estrutura existente. |
| Circuit breaker | ✅ existe | per-class/per-project/per-session em daemon | Generalizar em trait `Failover<P,B>` sob `touring-core::failover`. |
| File manifest | ✅ existe | `touring-vfs::manifest` (verify) | Adicionar `move_detection` (hash matching cross-path). |
| File ignore | ✅ existe | `touring-vfs` ou similar | Auditar paridade com rignore: .gitignore + .ignore + global gitignore + override patterns + ~360 extensions. |
| Tree-sitter grammars | ✅ wired | `touring-ast::semantic`, language families | Node-types JSON KB já implícito; expor via `touring ast node-types <lang>`. |

### 2.4 Componentes para Wave 5 (hybrid semantic search) — GREENFIELD

| Componente | Status | Localização | Observações |
|---|---|---|---|
| Embedding provider abstraction | ❌ não existe | criar `touring-embeddings/` crate | Trait `EmbeddingProvider`. Backend MVP: Candle (puro Rust) ou FastEmbed via tokio-process. |
| Vector store abstraction | ❌ não existe | criar `touring-vector-store/` crate | Trait `VectorStore`. Backends: sqlite-vec (local zero-deps) + Qdrant client (remoto). |
| Reranking pipeline | ❌ não existe | parte de `touring-search-fusion` (criar) | Trait `Reranker` com cascading fallback. |
| Asymmetric embeddings | ❌ não existe | depende de `CheckpointSettingsFingerprint` (Wave 4) | Query model ≠ document model, mas mesma família = compatível. |
| Candle deps disponibilidade | ⚠️ verify | candle-core, candle-transformers, candle-nn | Usar features locais; pin versions. |
| sqlite-vec deps disponibilidade | ⚠️ verify | sqlite-vec crate | Compile-time C, requires sqlite >= 3.35. |

### 2.5 Componentes para Wave 6-7 (agent UX + plugin DI + cost report)

| Componente | Status | Localização | Observações |
|---|---|---|---|
| MCP server existente | ✅ existe | `touring-server::mcp` (96 tools) | Adicionar 1 tool `touring_find_code` que orquestra Wave 5 stack. |
| Plugin/provider registry | ❌ não existe | criar `touring-core::plugin` | Trait `Provider` + `ProviderRegistry` para runtime swap (não substitui typestate compile-time). |
| Token counter | ⚠️ parcial | `touring-server::telemetry` | Estender para self-report MCP overhead per-tool e per-session. |

---

## 3. Catálogo completo de deliverables (12 itens, T-shirt sizing)

<deliverables>

### TIER S — Visual Foundation (Wave 1)

#### **D1 — `touring graph <subcmd> --format dot|mermaid|json` em todos os 4 subcomandos** [Size: M, ~400 LOC]

**Why**: Maior gap visível. Dado já existe; só falta tradutor. cargo-depgraph mostrou que pipeline `cmd | dot -Tsvg` é a UX canônica.

**Scope (in)**:
- Modificar `touring-server/src/cli/graph.rs`: adicionar parsing de `--format` (default `json`), `--output <file>` (default stdout), `--include-orphans`, `--include-tests` (default false).
- Criar `touring-server/src/visual/dot.rs` com `to_dot(graph_data: &GraphData, opts: &DotOpts) -> String`.
- Criar `touring-server/src/visual/mermaid.rs` com `to_mermaid(graph_data: &GraphData, opts: &MermaidOpts) -> String` (suporta `flowchart TD` + `subgraph` para clusters).
- Estender 4 daemon handlers para retornar `GraphData` enriquecido (nodes com `quality_score`/`fan_in`/`fan_out`/`is_orphan`/`has_unsafe`; edges com `kind: normal|test|build|cross_feature|cycle`).
- Helper `dot_pipe()` que detecta `dot` no PATH e oferece `--svg` direto (`--format svg` → exec dot via std::process).

**Scope (out)**: encoding visual rico (cor por quality, forma por kind) — deferido para D5 (S2).

**Files affected** (com blast estimate):
- `touring-server/src/cli/graph.rs` — REWRITE (48→120 LOC) — blast=1
- `touring-server/src/cli/mod.rs` — minor edit (mod visual) — blast=1
- `touring-server/src/visual/mod.rs` — NEW — blast=0
- `touring-server/src/visual/dot.rs` — NEW (~150 LOC) — blast=0
- `touring-server/src/visual/mermaid.rs` — NEW (~120 LOC) — blast=0
- `touring-server/src/visual/types.rs` — NEW (~80 LOC, GraphData/NodeData/EdgeData/Opts) — blast=0
- `touring-hooks/src/cli_handlers.rs` — extend 4 handlers (cli_graph_file, cli_graph_god_nodes, cli_graph_shortest_path, cli_graph_communities) ~50 LOC each — blast=4 (high) but escopo localizado
- `touring-rkyv/src/graph_ipc.rs` — possivelmente extend `GraphResponse` schema — blast=2

**New deps**: nenhuma (já temos `serde_json`, `petgraph`).

**Acceptance criteria**:
1. `touring graph file <path> --format dot` emite DOT válido (testado via `dot -Tsvg < output.dot` exit 0)
2. `touring graph communities --format mermaid` emite Mermaid renderizável em GitHub (manual visual check)
3. `touring graph --format svg --output workspace.svg` produz arquivo SVG quando `dot` está no PATH
4. `touring graph --format json` mantém output existente 100% backward-compat
5. 4 unit tests por handler (16 total): roundtrip JSON → DOT → re-parse → match
6. 1 integration test: pipeline `touring graph communities --format dot | grep "digraph"` exit 0

**Tests added**: 16 unit + 1 integration = 17

**Risk**: LOW — escopo isolado, sem mudança de schema crítico

---

#### **D2 — `--max-nodes/--max-edges` + `--reduce` (auto-tred)** [Size: S, ~200 LOC]

**Why**: Grafo de 38815 nodes é ilegível. Cap explícito + transitive reduction é mandatório para usabilidade.

**Scope (in)**:
- Adicionar `--max-nodes <N>` (default 200) e `--max-edges <N>` (default 500) aos comandos D1.
- Algoritmo de cap: BFS a partir de root nodes (workspace members ou `--root <symbol>`), expand by `relevance score` (quality × log(blast)), descarta resto, marca nós cortados como ellipsis "(+N)".
- `--reduce` aplica transitive reduction:
  - Tenta executar `tred` externo via `which tred` + `Command::new`
  - Fallback: implementação Rust em `touring-server/src/visual/tred.rs` (Aho/Garey/Ullman algorithm em DAG, ou skip em ciclo com warning)
- Adicionar `--no-cap` para desabilitar.

**Files affected**:
- `touring-server/src/cli/graph.rs` — extend (~30 LOC)
- `touring-server/src/visual/cap.rs` — NEW (~80 LOC)
- `touring-server/src/visual/tred.rs` — NEW (~90 LOC)

**New deps**: nenhuma. petgraph já tem `algo::dijkstra` para BFS scoring.

**Acceptance criteria**:
1. `touring graph communities --max-nodes 50 --format dot` retorna ≤ 50 nodes + indicador "(+N more)"
2. `touring graph file <path> --reduce --format dot` reduz edges ≥ 30% em DAG denso (medido em workspace touring)
3. Em grafo com ciclo, `--reduce` emite warning para stderr e preserva grafo original
4. 6 unit tests (BFS cap, tred DAG, tred com ciclo warning, max_edges trimming, ellipsis marker, --no-cap bypass)

**Tests added**: 6

**Risk**: LOW — algoritmos clássicos, fallback bem definido

---

### TIER S — Rich Encoding & Search (Wave 2)

#### **D3 — `touring viz` top-level com encoding visual rico** [Size: L, ~800 LOC]

**Why**: D1+D2 expõem export. D3 explora o **diferencial absoluto do Touring**: Touring tem 23 metadata fields por arquivo (file-knowledge extended), 6 dimensões de TDG, semantic_complexity, has_unsafe, fan_in/out, blast_radius, modularity_score. cargo-depgraph encoda 7 cores em 3 estilos. Touring pode encodar **muito mais** sem perder legibilidade.

**Scope (in)**:
- Novo top-level command `touring viz <subcommand>`:
  - `viz workspace` — todo o workspace (clusters por crate, edges por cargo metadata)
  - `viz blast <symbol>` — blast radius com encoding por proximidade
  - `viz wiring [--scope <crate>]` — wiring graph + integration_score color
  - `viz cycles` — apenas SCCs com `min_depth=2`
  - `viz orphans` — apenas órfãos + tentativa de classificar por proximidade
  - `viz feature <feature_name>` — symbols gated by feature

- **Encoding visual canônico** (config em `~/.claude/touring/viz-theme.toml` com defaults):
  ```toml
  [node.shape]
  workspace_member = "box"
  external_crate = "ellipse"
  orphan = "triangle"
  god_node = "diamond"
  test_module = "note"
  
  [node.fill]
  # Gradient by quality_score: green (1.0) → yellow (0.5) → red (0.0)
  quality_high = "#a5d6a7"   # quality_score >= 0.8
  quality_med = "#fff59d"    # 0.5 <= quality_score < 0.8
  quality_low = "#ef9a9a"    # quality_score < 0.5
  
  [node.size]
  # font_size = 8 + log2(loc) * 1.5, clamped [8, 18]
  
  [node.border]
  has_unsafe = "double"
  feature_gated = "dashed"
  default = "solid"
  
  [edge.color]
  normal = "#000000"
  dev_dependency = "#1976d2"
  build_dependency = "#388e3c"
  cross_feature = "#7b1fa2"
  cycle = "#d32f2f"
  
  [edge.style]
  optional = "dotted"
  transitively_optional = "dashed"
  cycle = "bold"
  default = "solid"
  
  [cluster]
  workspace_root = "lightblue"
  test_dir = "lightgrey"
  ```

- **Layout selection automatic**: 
  - `viz workspace` → `dot` (hierarchical)
  - `viz wiring` → `sfdp` (large force-directed)
  - `viz cycles` → `circo` (circular)
  - `viz blast` → `twopi` (radial centered on symbol)
  - Override via `--layout dot|neato|fdp|sfdp|circo|twopi`

- **Tooltip embedding** (DOT supports tooltip attribute, renders as SVG title):
  - Each node: `tooltip="quality=0.72\nblast=42\nfan_in=12\nfan_out=5\ncomplexity=8.3\nLOC=234"`
  - Each edge: `tooltip="kind=normal\nfrequency=8"`

- **Output formats**: dot (default) | svg | png | mermaid | json

**Files affected**:
- `touring-server/src/cli/viz.rs` — NEW (~250 LOC, dispatcher)
- `touring-server/src/cli/mod.rs` — register viz subcommand — blast=1
- `touring-server/src/visual/encoding.rs` — NEW (~280 LOC, theme + node/edge styling)
- `touring-server/src/visual/theme.rs` — NEW (~120 LOC, TOML loader + defaults)
- `touring-server/src/visual/layout.rs` — NEW (~80 LOC, layout heuristics + auto-select)
- `touring-server/src/visual/dot.rs` — extend (use encoding) — blast=2
- `touring-hooks/src/cli_handlers.rs` — 6 new daemon handlers (`cli-viz-workspace`, `cli-viz-blast`, `cli-viz-wiring`, `cli-viz-cycles`, `cli-viz-orphans`, `cli-viz-feature`) ~60 LOC each — blast=6
- `~/.claude/touring/viz-theme.toml` — NEW user-level config seed

**New deps**: `toml = "0.8"` (provavelmente já no workspace; verify)

**Acceptance criteria**:
1. `touring viz workspace --format svg --output ws.svg` produz SVG renderizado com clusters por crate, encoding por quality_score
2. `touring viz blast <symbol> --layout twopi --format dot` produz layout radial centrado
3. Tooltip embedding verificável em SVG (open + hover preview)
4. Theme override: copiar `viz-theme.toml` → modificar `quality_low = "#purple"` → próximo run usa nova cor
5. `viz cycles` em workspace touring identifica ≥ 1 ciclo (já sabemos que touring-cognitive tem alguns) e renderiza com `circo`
6. 18 unit tests: 6 (1 por subcommand) + 6 encoding (shape/fill/border/edge_color/edge_style/cluster) + 6 theme (load/default/override/invalid/missing/partial)

**Tests added**: 18

**Risk**: MEDIUM (probabilidade 0.4 / impacto MEDIUM) — encoding visual é altamente subjetivo; pode requerer iteração com Gabriel para calibrar paleta. **Mitigação**: shipping com defaults conservadores + theme overridable via TOML.

---

#### **D4 — Reciprocal Rank Fusion (RRF) em search unificado** [Size: S, ~200 LOC]

**Why**: code-graph-ai usa RRF (60). Touring tem `tantivy search` (BM25) + `index find` (exato) + `tantivy fuzzy` (Levenshtein) **separados** — fundi-los via RRF aumenta precision/recall sem novo storage.

**Scope (in)**:
- Novo módulo `touring-server/src/cli/search.rs` (top-level `touring search`):
  - `search unified <query> [--limit N=20]` — chama 3 backends, funde via RRF
  - `search exact <query>` — só index find
  - `search fuzzy <query>` — só tantivy fuzzy
  - `search bm25 <query>` — só tantivy search
- RRF algorithm (60 = constante padrão paper Cormack et al.):
  ```rust
  // For each result, sum 1.0 / (60 + rank_in_each_backend)
  // Higher score = more rankings agree
  ```
- Output: ranked list (compact + table + json formats), com badge mostrando origem (`[E]xact / [F]uzzy / [B]M25 / [U]nified`)

**Files affected**:
- `touring-server/src/cli/search.rs` — NEW (~180 LOC)
- `touring-server/src/cli/mod.rs` — register — blast=1
- `touring-hooks/src/cli_handlers.rs` — `cli_search_unified` handler — blast=1
- Existing `touring-search/` ou `touring-tantivy/` (verify) — possivelmente extend trait — blast=2

**New deps**: nenhuma

**Acceptance criteria**:
1. `touring search unified "HookRuntime"` retorna top-20 com badges mostrando que >= 2 backends concordam nos top-3
2. RRF score is invariant a permutações dos backends
3. 8 unit tests: RRF arithmetic (3 cases), backend fusion (3 cases), tie-breaking, empty results

**Tests added**: 8

**Risk**: LOW — algoritmo trivial (10 linhas), backends já existem

---

#### **D5 — Confidence tiers configuráveis em blast/impact** [Size: XS, ~100 LOC]

**Why**: code-graph-ai tem `[impact] high_threshold=20, medium_threshold=5` em config. Touring atualmente emite só `score: f32`. Tier label torna output muito mais legível para humanos e PRs.

**Scope (in)**:
- Estender `touring config` (existente?) ou criar `~/.claude/touring/touring.toml`:
  ```toml
  [blast]
  high_threshold = 20    # files ou symbols
  medium_threshold = 5
  
  [impact.depth]
  high_threshold = 4
  medium_threshold = 2
  ```
- Adicionar campo `tier: "high"|"medium"|"low"` em respostas de:
  - `touring wiring impact <symbol>` — nodes ganham `tier`
  - `touring ast blast <file>` — root response ganha `tier`
- Output `compact` mostra emoji ou label `[HIGH]/[MED]/[LOW]`

**Files affected**:
- `touring-core/src/config.rs` ou `touring-server/src/config/blast.rs` — NEW (~40 LOC)
- `touring-hooks/src/cli_handlers.rs` — 2 handlers extended — blast=2
- `touring-server/src/cli/wiring.rs` — extend formatter — blast=1

**New deps**: nenhuma (toml already)

**Acceptance criteria**:
1. `touring wiring impact <hot-symbol>` retorna `tier: "high"` quando consumers >= 20
2. Override via `~/.claude/touring/touring.toml` muda thresholds
3. 4 unit tests (default thresholds, custom, edge=threshold-1, edge=threshold+1)

**Tests added**: 4

**Risk**: LOW

---

### TIER A — Capability Parity com code-graph-ai (Wave 3)

#### **D6 — `touring graph flow <a> <b>`** [Size: S, ~250 LOC]

**Why**: code-graph-ai oferece "flow A→B" mostrando todos os caminhos no call graph. Útil para entender "como X consegue chegar em Y". Touring tem `wiring impact` (1-to-many) mas não 1-to-1 path enumeration.

**Scope (in)**:
- Novo subcommand `touring graph flow <symbol_a> <symbol_b> [--max-paths 10] [--max-depth 8]`
- Implementação via `petgraph::algo::all_simple_paths(&graph, a, b, min_intermediate_nodes=0, max_intermediate_nodes=Some(max_depth))`
- Use call graph from `touring-cortex::call_graph` ou `touring-ast::call_graph` (verify canonical)
- Output: 
  - JSON: `{paths: [[a, m1, m2, b], [a, m3, b], ...], count: N, truncated: bool}`
  - DOT/Mermaid: highlight todos os paths em coloração distinta
  - Compact: `a → m1 → m2 → b\na → m3 → b\n...`

**Files affected**:
- `touring-server/src/cli/graph.rs` — extend dispatcher — blast=1
- `touring-server/src/visual/flow.rs` — NEW (~150 LOC)
- `touring-hooks/src/cli_handlers.rs` — `cli_graph_flow` handler — blast=1
- Possibly `touring-cortex/src/call_graph.rs` — expose flow API — blast=2

**New deps**: nenhuma (petgraph já tem)

**Acceptance criteria**:
1. `touring graph flow main spawn_daemon` retorna pelo menos 1 path
2. `--max-paths 1` retorna shortest path apenas
3. Graph com `a` e `b` desconectados retorna `{paths: [], count: 0}`
4. 8 unit tests (cycle, no path, single path, multi path, depth limit, max paths limit, identical a==b, missing symbol)

**Tests added**: 8

**Risk**: LOW

---

#### **D7 — `touring graph rename <symbol> --new <name> --plan`** [Size: M, ~600 LOC]

**Why**: Refactor planning. code-graph-ai oferece `rename` com impact analysis. Touring tem `wiring impact` + `ast grep --rewrite` mas falta o orquestrador que **gera o plan estruturado**.

**Scope (in)**:
- Novo command `touring graph rename <old_symbol> --new <new_name> --plan [--dry-run]`
- Pipeline:
  1. `touring index find <old_symbol>` para localizar definição
  2. `touring wiring impact <old_symbol> --depth ∞` para todos consumers
  3. Para cada consumer file: localizar call sites via `touring ast grep` ou `touring index find` 
  4. Gerar plan estruturado:
     ```json
     {
       "old": "old_symbol",
       "new": "new_name",
       "edits": [
         {"file": "...", "line": 42, "col": 10, "kind": "definition"},
         {"file": "...", "line": 8, "col": 15, "kind": "import"},
         {"file": "...", "line": 100, "col": 25, "kind": "call_site"},
         ...
       ],
       "blast_radius": 23,
       "tier": "medium",
       "files_affected": 12,
       "risk_factors": ["touches public API", "used in tests"]
     }
     ```
- `--dry-run` (default) só mostra plan
- `--apply` (off por padrão) aplica via `touring ast grep --rewrite` para cada edit, com pre-edit validation
- **NÃO toca em código por default** — apenas planeja

**Files affected**:
- `touring-server/src/cli/graph.rs` — extend — blast=1
- `touring-server/src/refactor/rename.rs` — NEW (~350 LOC)
- `touring-server/src/refactor/mod.rs` — NEW
- `touring-hooks/src/cli_handlers.rs` — `cli_graph_rename_plan`, `cli_graph_rename_apply` — blast=2

**New deps**: nenhuma

**Acceptance criteria**:
1. `touring graph rename HookRuntime --new HookEngine --plan` retorna plan JSON com edits ≥ 5 (todos os call sites do workspace touring)
2. Plan inclui blast_radius e tier
3. `--apply` apenas roda se `--plan-confirm <hash>` for fornecido (matching plan hash)
4. Atomicidade: se qualquer edit falhar, rollback all
5. 12 unit tests: plan generation (4), apply (4), rollback (2), idempotence (2)

**Tests added**: 12

**Risk**: MEDIUM (probabilidade 0.3 / impacto HIGH) — `--apply` pode quebrar código se sites de chamada têm contexto sutil (macros, generics). **Mitigação**: 
- Default mode é `--plan` (read-only)
- `--apply` requer hash do plan + speculative validation antes de cada edit (`touring shadow validate`)
- Apply only if all speculate scores >= 0.8

---

#### **D8 — `touring graph snapshot create/list/delete/diff`** [Size: M, ~500 LOC]

**Why**: code-graph-ai tem snapshots nomeados de grafo + diff. Útil para "antes/depois" de refactors L3+ e para CI (compare PR vs main). Touring tem `checkpoint` mas é genérico.

**Scope (in)**:
- Novo subcommand `touring graph snapshot <create|list|delete|diff>`:
  - `create <name> [--scope workspace|crate|file]` — serializa graph state em SQLite ou bincode em `~/.claude/touring/snapshots/<name>.bin`
  - `list` — JSON ou table: name, created_at, scope, node_count, edge_count
  - `delete <name>`
  - `diff <a> <b> [--format dot|mermaid|json]` — mostra:
    - Adicionados (verde)
    - Removidos (vermelho)
    - Modificados (quality_score change > 0.1, fan_in change > 5)
- `diff-impact <git-ref>` — extra: cruza com `git diff --name-only <ref>` e calcula blast radius dos files alterados

**Files affected**:
- `touring-server/src/cli/graph.rs` — extend — blast=1
- `touring-server/src/snapshot/mod.rs` — NEW
- `touring-server/src/snapshot/store.rs` — NEW (~200 LOC, bincode + SQLite metadata)
- `touring-server/src/snapshot/diff.rs` — NEW (~180 LOC)
- `touring-hooks/src/cli_handlers.rs` — 4 handlers — blast=4

**New deps**: `bincode = "1.3"` (verify; provavelmente já no workspace)

**Acceptance criteria**:
1. `touring graph snapshot create pre-refactor` cria arquivo bincode
2. `touring graph snapshot list` mostra com timestamp
3. `touring graph snapshot diff pre-refactor post-refactor --format dot` gera diff visual com cores
4. `touring graph snapshot diff-impact main` (em PR branch) mostra impact dos files git-changed
5. 14 unit tests: create (3), list (2), delete (2), diff (5), diff-impact (2)

**Tests added**: 14

**Risk**: LOW — escopo bem delimitado

---

#### **D9 — Clone detection (signature hashing)** [Size: M, ~400 LOC]

**Why**: code-graph-ai tem `clones --min-group N`. Hash signature: `(kind, body_size, edge_count, semantic_complexity, derives)`. Touring tem AST + cognitive — falta lógica de signature.

**Scope (in)**:
- Novo command `touring graph clones [--min-group 2] [--scope <path>] [--format compact|json|dot]`
- Compute signature por symbol:
  - SHA256 hash de tuple `(kind: enum, lines_count, ast_node_count, semantic_complexity_bucket, derives_sorted, generic_params_count, trait_bounds_count)`
  - Bucket semantic_complexity em 10 buckets [0.0, 0.1), [0.1, 0.2), ...
- Group symbols por hash
- Output:
  - Compact: `Group #1 (3 clones, signature kind=fn,size~50,complexity_bucket=4):\n  src/foo.rs:42 bar()\n  src/baz.rs:100 bar2()\n  ...`
  - JSON: structured
  - DOT: cluster por group

**Files affected**:
- `touring-server/src/cli/graph.rs` — extend — blast=1
- `touring-server/src/refactor/clones.rs` — NEW (~280 LOC)
- `touring-hooks/src/cli_handlers.rs` — `cli_graph_clones` handler — blast=1
- `touring-ast/src/signature.rs` — NEW (~100 LOC, signature computation) — blast=0

**New deps**: `sha2 = "0.10"` (verify; provavelmente já)

**Acceptance criteria**:
1. `touring graph clones --min-group 3` em workspace touring retorna ≥ 1 group (esperado em test code que tem padrões repetidos)
2. False positives bound: se 2 functions têm assinatura idêntica mas ASTs diferentes, devem aparecer no mesmo grupo (é estrutural, não semântico)
3. 10 unit tests: signature stable, group formation, min-group filter, scope filter, json roundtrip

**Tests added**: 10

**Risk**: LOW

---

### TIER B — Investments (Wave 4 — opcional, baseado em demanda)

#### **D10 — Web UI lite (Axum + dot.wasm)** [Size: XL, multi-wave]

**Why**: code-graph-ai tem Axum+Svelte+WebGL UI. Touring é CLI/MCP-only. Web UI é alto-leverage para usuários não-CLI; **mas não é Gabriel** primariamente. **Apenas se demand surge**.

**Scope (in)**:
- Novo crate `touring-webui/` (Axum 0.8 + askama templates)
- Endpoints:
  - `GET /` — landing
  - `GET /graph?type=workspace|blast|wiring&symbol=X&format=svg` — server-side dot render
  - `GET /api/v1/{find|impact|cycles}` — REST mirror dos MCP tools
  - `WebSocket /events` — health/quality updates streaming via SSE
- Client-side: dot.wasm (https://github.com/hpcc-systems/Visualization) para render rápido sem invocar dot CLI
- Atrás de feature flag `touring serve --features web` (paridade com code-graph-ai)
- Auth: localhost-only por default; opt-in `--public` requer token

**Files affected**:
- `touring-webui/` — NEW crate (~1500 LOC)
- `touring-webui/Cargo.toml` — feature `web`
- `touring-server/src/cli/serve.rs` — register web feature — blast=1
- Workspace root `Cargo.toml` — add member — blast=workspace

**New deps**: `axum = "0.8"`, `tokio-util`, `tower-http`, `askama = "0.13"`, possibly `serde_qs`

**Acceptance criteria**:
1. `touring serve --features web --port 8080` levanta UI
2. Browser navega para `localhost:8080/graph?type=workspace&format=svg` e vê SVG renderizado client-side
3. WebSocket atualiza quando arquivo do workspace muda
4. Auth: rejeita requests externas ao localhost por default
5. ~30 unit/integration tests

**Tests added**: ~30

**Risk**: HIGH (probabilidade 0.6 / impacto MEDIUM se delivered late) — XL effort, scope creep risk altíssimo. **Mitigação**: ship apenas se Gabriel pedir; entregar em 3 sub-waves (D10.1=infra, D10.2=graph viz, D10.3=health dashboard); cada sub-wave shippable independentemente.

---

#### **D11 — Edge Bundling (FDEB)** [Size: L, ~1200 LOC]

**Why**: Para grafos densos (orphans 18957 + symbols 38815), edges crossing causam visual clutter. FDEB bundles compatible edges, reduzindo clutter ≥ 70%.

**Scope (in)**:
- Implementação Holten/van Wijk Force-Directed Edge Bundling em `touring-server/src/visual/bundling.rs`
- Algoritmo iterativo: 
  - Cada edge é discretizada em N=20 control points
  - Iteração: cada control point é atraído por control points compatíveis (compatibility = angular + scale + position + visibility)
  - Convergência típica: 6 iterações
  - Output: edges como spline paths em DOT
- Acionado via `--bundle` flag em `touring viz`/`touring graph`
- Disabled por default (overhead computacional)

**Files affected**:
- `touring-server/src/visual/bundling.rs` — NEW (~900 LOC)
- `touring-server/src/visual/dot.rs` — extend para emitir spline edges — blast=2

**New deps**: nenhuma (cálculos vetoriais com nalgebra-glm? OR std puro)

**Acceptance criteria**:
1. `touring viz workspace --bundle --format svg` produz SVG com edges curvas agrupadas
2. Visual clutter reduction ≥ 30% medido por edge crossing count
3. Performance: ≤ 2 seg em workspace de 1000 nodes
4. 12 unit tests: compatibility metrics, control point movement, convergence detection, ASCII art smoke tests

**Tests added**: 12

**Risk**: HIGH (probabilidade 0.5 / impacto LOW se postponed) — algoritmo non-trivial, calibração de hyperparams custosa. **Mitigação**: opt-in flag, default off; ship em sub-wave separada com benchmarks.

---

#### **D12 — gvpr-inspired Filter DSL** [Size: L, ~1000 LOC]

**Why**: gvpr é AWK-para-grafos. Touring poderia ter DSL similar para queries declarativas: `WHERE node.quality_score > 0.7 AND node.fan_out > 5 SELECT node FORMAT dot`.

**Scope (in)**:
- DSL parser via `chumsky` ou `nom`
- Operators: comparações, AND/OR/NOT, group by, aggregations (count, avg, max)
- Predicates: contém todos os campos de file-knowledge extended (23 fields) + ast metadata
- Compilation: AST → petgraph filter chain
- CLI: `touring graph query "<dsl>"`

**Files affected**:
- `touring-server/src/dsl/` — NEW module (~750 LOC)
- `touring-server/src/dsl/parser.rs`
- `touring-server/src/dsl/eval.rs`
- `touring-server/src/cli/graph.rs` — extend — blast=1

**New deps**: `chumsky = "0.10"` ou `nom = "7.1"`

**Acceptance criteria**:
1. `touring graph query "WHERE quality_score < 0.5 SELECT * FORMAT dot"` retorna apenas nodes low-quality
2. Aggregation: `SELECT count(*) GROUP BY crate` retorna count por crate
3. Pipe-friendly: composable com `--reduce`, `--bundle`
4. ~20 unit tests para parser + ~10 eval

**Tests added**: 30

**Risk**: HIGH (probabilidade 0.5 / impacto LOW) — DSL é grande commit, requer iteração de UX. **Mitigação**: avaliar se demand existe — usuário pode preferir compor filtros via shell + jq por enquanto. Postergar até validation.

---

## 3.5 Catálogo de deliverables CodeWeaver-derived (D13-D28, 16 itens)

> Adicionados na revisão v2 (2026-04-30) após análise profunda de knitli/codeweaver. Estes 16 deliverables fecham os 3 gaps adicionais (Hybrid Semantic Search, Resilience Patterns Maduros, Agent UX) e complementam D1-D12 sem sobreposição.

### TIER S (CodeWeaver) — Search & Chunker Foundation (Wave 2 amplificado)

#### **D13 — Intent classification + semantic weighting** [Size: M, ~400 LOC]

**Why**: CodeWeaver classifica queries em `IntentType.{UNDERSTAND, DEBUG, IMPLEMENT}` com confidence + boost factor 0.2 (20% maximum). Touring search é intent-blind. Adicionar intent-awareness aumenta precision drasticamente para natural language queries sem requerer dense embeddings.

**Scope (in)**:
- Novo módulo `touring-search-fusion::intent` (ou `touring-tantivy::intent`):
  ```rust
  pub enum QueryIntent { Understand, Debug, Implement, Refactor, Document, Explore }
  pub struct IntentResult { intent: QueryIntent, confidence: f32, reasoning: String }
  pub fn detect_intent(query: &str) -> IntentResult  // v1: keyword heuristics
  pub fn apply_semantic_weighting(base: f32, intent: QueryIntent, chunk_meta: &ChunkMeta, boost: f32) -> f32
  ```
- Boost factor default 0.2 (configurable em `touring.toml [search.intent] boost_factor = 0.2`).
- Heurísticas keyword v1:
  - `UNDERSTAND`: "how", "why", "what", "explain", "describe"
  - `DEBUG`: "fix", "bug", "error", "fail", "broken", "panic"
  - `IMPLEMENT`: "add", "create", "build", "make", "implement"
  - `REFACTOR`: "rename", "extract", "inline", "simplify", "cleanup"
  - `DOCUMENT`: "document", "describe", "annotate"
  - `EXPLORE`: default fallback
- v2 (futuro): agent-driven via inferlets WASM ou LLM call (gated).

**Files affected**:
- `touring-search-fusion/src/intent.rs` — NEW (~200 LOC) ou estender `touring-tantivy`
- `touring-server/src/cli/search.rs` — extend com `--intent <type>` flag — blast=1
- `touring-hooks/src/cli_handlers.rs` — `cli_search_unified` integra intent — blast=1
- `touring.toml` schema — adicionar `[search.intent]` section

**New deps**: nenhuma

**Acceptance criteria**:
1. `touring search unified "how does authentication work"` detecta `UNDERSTAND` com confidence ≥ 0.7
2. `touring search unified "fix login bug"` detecta `DEBUG`
3. Boost de 20% em chunks com matching semantic_metadata (ex: chunks de error handling boostados quando intent=DEBUG)
4. Override via `--intent debug` força classificação
5. 12 unit tests (6 intent types × heuristic + 6 weighting cases)

**Tests added**: 12

**Risk**: LOW — algoritmo trivial, fallback graceful

---

#### **D14 — GracefulChunker fallback chain pattern** [Size: S, ~250 LOC]

**Why**: CodeWeaver formaliza chain `SemanticChunker → DelimiterChunker → text splitting` via `GracefulChunker<P, F>` wrapper. Touring tem chunkers mas chain implícita. Formalizar garante que **TODO arquivo é processável** (binary, oversized, parse error).

**Scope (in)**:
- Refactor em `touring-core::chunker` (ou `touring-ast::chunker`):
  ```rust
  pub trait Chunker {
      fn chunk(&self, content: &str, ctx: &ChunkContext) -> Result<Vec<CodeChunk>, ChunkError>;
  }
  
  pub struct GracefulChunker<P: Chunker, F: Chunker> {
      primary: P,
      fallback: F,
  }
  
  impl<P, F> Chunker for GracefulChunker<P, F> {
      fn chunk(&self, content, ctx) -> Result<Vec<CodeChunk>, ChunkError> {
          match self.primary.chunk(content, ctx) {
              Ok(chunks) => Ok(chunks),
              Err(e) => {
                  tracing::warn!(error = %e, "primary chunker failed, falling back");
                  self.fallback.chunk(content, ctx)
              }
          }
      }
  }
  ```
- Erros tipados: `BinaryFileError`, `ParseError`, `ChunkingTimeoutError`, `ChunkLimitExceededError`, `ASTDepthExceededError`
- Wrap padrão: `GracefulChunker::new(SemanticChunker::for(lang), DelimiterChunker::for(family))`
- ChunkerSelector cria fresh instances per file (no state contamination)

**Files affected**:
- `touring-core/src/chunker/mod.rs` — NEW trait
- `touring-core/src/chunker/graceful.rs` — NEW (~120 LOC)
- `touring-core/src/chunker/error.rs` — NEW (~50 LOC, typed errors)
- `touring-ast/src/chunker.rs` — refactor existing → impl Chunker — blast=2
- `touring-vfs/src/chunker.rs` — idem se existir — blast=2

**New deps**: nenhuma

**Acceptance criteria**:
1. Binary file: SemanticChunker emite `BinaryFileError`, fallback retorna single-chunk com `is_binary=true`
2. Oversized file: SemanticChunker emite `ChunkLimitExceeded`, fallback emite chunks size-capped
3. Parse error: SemanticChunker emite `ParseError`, fallback emite delimiter-based chunks
4. Recursive child chunking: AST node oversized → recurse; ainda oversized → delimiter; ainda oversized → as-is single chunk
5. 14 unit tests (5 error types × primary fallback + 4 happy paths + 5 edge cases: empty, single-line, whitespace, mixed-encoding, large)

**Tests added**: 14

**Risk**: MEDIUM (probabilidade 0.3 / impacto MEDIUM) — refactor toca chunker existing. **Mitigação**: shadow validate via `touring shadow validate` antes de cada change; testes abrangentes; pode ser rolled out incremental (1 lang per sub-task).

---

#### **D15 — ResourceGovernor unified context manager** [Size: S, ~150 LOC]

**Why**: CodeWeaver tem `ResourceGovernor` context manager que enforce timeout + chunk count + memory bounds. Touring tem timeouts dispersos em tantivy, chunker, indexer, search — cada um implementa do seu jeito. Refactor unifica.

**Scope (in)**:
- Novo `touring-core::governor`:
  ```rust
  pub struct ResourceGovernor {
      timeout: Duration,
      max_chunks: usize,
      max_memory_mb: Option<usize>,
      start_time: Option<Instant>,
      chunk_count: AtomicUsize,
  }
  
  impl ResourceGovernor {
      pub fn new(settings: PerformanceSettings) -> Self
      pub fn check_timeout(&self) -> Result<(), TimeoutError>
      pub fn register_chunk(&self) -> Result<(), LimitError>
      pub fn check_memory(&self) -> Result<(), MemoryError>  // via memory-stats
  }
  
  // RAII pattern (Rust-native vs Python context manager):
  pub struct GovernorGuard<'a> { gov: &'a ResourceGovernor }
  impl Drop for GovernorGuard { fn drop(&mut self) { /* cleanup */ } }
  ```
- Replace ad-hoc timeouts em ≥ 5 sites: `cli_handlers::cli_search_*`, chunker invocations, tantivy queries, indexer batches
- Integration com `memory-stats` crate (já wired em Wave 2026-04-20)

**Files affected**:
- `touring-core/src/governor/mod.rs` — NEW (~120 LOC)
- `touring-core/src/governor/error.rs` — NEW (~30 LOC)
- `touring-hooks/src/cli_handlers.rs` — refactor 5+ sites — blast=5
- `touring-tantivy/src/search.rs` — replace ad-hoc timeout — blast=2
- `touring-ast/src/chunker.rs` — replace ad-hoc — blast=2

**New deps**: nenhuma (memory-stats já no workspace)

**Acceptance criteria**:
1. Long-running query é abortada quando timeout excede
2. Chunker que produziria 100k+ chunks aborta no limit configurado
3. Memory pressure (RSS > threshold) emite degraded mode em search
4. 8 unit tests + 1 integration (timeout num search real)

**Tests added**: 9

**Risk**: LOW — refactor de pattern existente

---

#### **D16 — `touring init --profile` UX** [Size: XS, ~80 LOC + 4 TOML files]

**Why**: CodeWeaver tem `cw init --profile recommended|quickstart`. Touring tem 81 CLI cmds, mas falta UX para "novo projeto, defaults sensatos". Profiles = subset de features ativas + defaults.

**Scope (in)**:
- Criar `~/.claude/touring/profiles/` com 4 TOML:
  - `recommended.toml`: full features (daemon + tantivy + cognitive + RL + generator)
  - `quickstart.toml`: local-only minimal (no daemon, CLI fallback mode, basic search)
  - `airgapped.toml`: zero network calls, no telemetry
  - `ci.toml`: json-only output, no TUI, headless-friendly
- Estender `touring init`:
  ```bash
  touring init --profile recommended  # default
  touring init --profile quickstart
  touring init --list-profiles
  ```
- `touring init --profile <name>` copia `profiles/<name>.toml` → `<workspace>/.touring/touring.toml`
- Validação Pydantic-style (já temos serde + validation)

**Files affected**:
- `touring-server/src/cli/init.rs` — extend (~40 LOC) — blast=1
- `~/.claude/touring/profiles/{recommended,quickstart,airgapped,ci}.toml` — NEW assets

**New deps**: nenhuma

**Acceptance criteria**:
1. `touring init --profile quickstart` cria touring.toml válido
2. `touring init --list-profiles` mostra 4 com descrição one-liner
3. Re-run com `--profile <other>` faz backup do atual e substitui
4. 5 unit tests (load each profile + invalid name + backup)

**Tests added**: 5

**Risk**: LOW

---

### TIER A (CodeWeaver) — Resilience Patterns (Wave 4)

#### **D17 — Move detection (incremental dedup)** [Size: S, ~200 LOC]

**Why**: CodeWeaver `FileManifestManager` detecta arquivo movido (mesma hash, novo path) e evita re-embed/re-chunk. Touring tem `incremental-salsa` mas não detecta moves. Aplicação: gigantesco speedup em renames/refactors massivos.

**Scope (in)**:
- Estender `touring-vfs::manifest`:
  ```rust
  pub struct MoveEvent { from: PathBuf, to: PathBuf, content_hash: BlakeHash }
  
  impl FileManifest {
      pub fn detect_moves(&self, new_paths: &[PathBuf]) -> Vec<MoveEvent>
      pub fn apply_move(&mut self, ev: &MoveEvent) -> Result<()>
  }
  ```
- Algoritmo: para cada path em new_paths, compute hash; se hash existe em manifest com path diferente → MoveEvent
- Integration: incremental-salsa consume MoveEvent → skip recompute, apenas update path
- Edge case: 2 cópias com mesmo hash (não-move) — usar timestamp/recency tie-breaker

**Files affected**:
- `touring-vfs/src/manifest.rs` — extend (~150 LOC) — blast=2
- `touring-incremental-salsa/src/lib.rs` — consume MoveEvent — blast=2

**New deps**: nenhuma (blake3 já no workspace)

**Acceptance criteria**:
1. Mover `src/foo.rs → src/bar.rs` não dispara re-chunk
2. Mover + edit no mesmo passo é detectado como (move + change), não 2 events
3. 2 cópias mesmo hash → primeiro é original, demais são duplicates (não moves)
4. 10 unit tests

**Tests added**: 10

**Risk**: LOW

---

#### **D18 — CheckpointSettingsFingerprint family-aware** [Size: M, ~500 LOC]

**Why**: CodeWeaver introduz "asymmetric embedding configs" com family-aware compatibility — query model pode mudar sem reindex se família igual. Mesmo sem embeddings ainda (Wave 5), Touring beneficia: trocar tree-sitter version sem invalidar índice se família AST igual.

**Scope (in)**:
- Novo `touring-core::checkpoint::fingerprint`:
  ```rust
  pub struct CheckpointSettingsFingerprint {
      pub config_type: ConfigType,  // Symmetric | Asymmetric
      pub primary_chunker: String,
      pub primary_chunker_family: Option<String>,
      pub secondary_chunker: Option<String>,
      pub vector_store: Option<String>,  // for Wave 5
      pub config_hash: blake3::Hash,
  }
  
  pub enum ChangeImpact { None, Compatible, BreakingMinor, BreakingMajor }
  
  impl CheckpointSettingsFingerprint {
      pub fn is_compatible_with(&self, other: &Self) -> (bool, ChangeImpact)
  }
  ```
- Logic:
  - Symmetric: same chunker = compatible; different = breaking
  - Asymmetric: same family + same primary = compatible mesmo se secondary muda; different family = breaking
- Bridge: `IndexingCheckpoint::matches_settings()` (existing) → consult fingerprint
- Bridge: invocado em `touring session start` para decidir reuse vs reindex

**Files affected**:
- `touring-core/src/checkpoint/fingerprint.rs` — NEW (~250 LOC)
- `touring-core/src/checkpoint/manager.rs` — extend `is_index_valid_for_config()` — blast=2
- `touring-server/src/cli/session.rs` — show change impact ao iniciar — blast=1

**New deps**: nenhuma

**Acceptance criteria**:
1. Same config → `is_compatible=true, impact=None`
2. Tree-sitter minor version bump (same family) → `Compatible`
3. tree-sitter → ast-grep change → `BreakingMajor`
4. Session start mostra "config changed since last session: [Compatible/Breaking]"
5. 14 unit tests (matrix of symmetric/asymmetric × same/different)

**Tests added**: 14

**Risk**: MEDIUM (probabilidade 0.3 / impacto LOW) — engineering coordination com checkpoint manager existente. **Mitigação**: backward compat via `#[serde(default)]` em todos novos fields.

---

#### **D19 — FailoverService cross-subsystem (generalizar circuit breaker)** [Size: M, ~500 LOC]

**Why**: CodeWeaver tem `FailoverService` que coordena primary/backup vector store transitions com "zero functionality loss". Touring tem circuit breaker per-class/per-project/per-session no daemon, mas falta orquestrador unificado.

**Scope (in)**:
- Novo `touring-core::failover`:
  ```rust
  #[async_trait]
  pub trait Failover<P: Send + Sync, B: Send + Sync> {
      async fn primary_health(&self) -> Health;
      async fn activate_backup(&mut self) -> Result<(), FailoverError>;
      async fn sync_backup(&mut self) -> Result<(), FailoverError>;
      async fn restore_to_primary(&mut self) -> Result<(), FailoverError>;
  }
  
  pub struct FailoverCoordinator {
      services: Vec<Box<dyn Failover<_, _>>>,
      state: Arc<RwLock<FailoverState>>,
  }
  ```
- Default impls para 3 subsystems iniciais:
  - tantivy primary → fallback indices
  - daemon primary → CLI-only mode
  - vector store primary → local sqlite-vec backup (preparação Wave 5)
- Health monitor periódico (default 30s)
- Métricas em `touring gate-metrics -j`: `failover_active_count`, `failover_transitions_count`, `failover_recovery_count`

**Files affected**:
- `touring-core/src/failover/mod.rs` — NEW (~200 LOC trait + coordinator)
- `touring-core/src/failover/health.rs` — NEW (~80 LOC)
- `touring-server/src/health_monitor.rs` — extend para periodic check — blast=2
- `touring-hooks/src/cli_handlers.rs` — circuit breaker integration — blast=3

**New deps**: nenhuma (`async-trait` already)

**Acceptance criteria**:
1. tantivy primary failure → automatic backup activation em < 5s
2. Daemon socket failure → CLI-only mode preserva read-only ops
3. Restore quando primary recupera (3 health checks consecutivos OK)
4. `touring gate-metrics -j | jq '.failover_active_count'` reflete state real
5. 12 unit tests + 2 integration

**Tests added**: 14

**Risk**: MEDIUM — multi-subsystem coordination. **Mitigação**: rollout incremental por subsystem; cada um shippable standalone.

---

#### **D20 — rignore-style file filtering audit** [Size: XS, ~50 LOC research + ~150 LOC if gap]

**Why**: CodeWeaver usa `rignore` (Python wrapper para Rust `ignore` crate) com .gitignore + .ignore + global gitignore + override patterns + ~360 file types. Touring tem `touring-vfs` filtering — auditar paridade.

**Scope (in)**:
- FASE 1 audit (no code): grep `touring-vfs` para `.gitignore`, `ignore_hidden`, `read_git_ignore`, override patterns, extension filters
- Gap report:
  - ✅/❌ .gitignore parent dirs respect
  - ✅/❌ Global gitignore (~/.config/git/ignore)
  - ✅/❌ Override patterns para tooling dirs (.github, .vscode, .claude, .circleci)
  - ✅/❌ Hidden file handling (with whitelist)
  - ✅/❌ Extension filter (~360 types)
- Se gap: implementar via `ignore` crate (Rust native, mesma source que rignore Python wraps)

**Files affected**:
- `touring-vfs/src/filter.rs` (verify) — possivelmente extend — blast=2
- `touring-vfs/src/overrides.rs` — possivelmente NEW — blast=0

**New deps**: `ignore = "0.4"` (verify se já no workspace)

**Acceptance criteria**:
1. Audit report listando ✅/❌ para 5 capabilities
2. Se gaps: implementar + 8 unit tests
3. .gitignore com nested dirs honrado
4. Override pattern `**/.claude/**` whitelist mesmo com `ignore_hidden=true`

**Tests added**: 0 (se sem gaps) ou 8 (se gaps)

**Risk**: LOW — audit-first

---

#### **D21 — Knowledge base de node types JSON exposto** [Size: S, ~150 LOC]

**Why**: CodeWeaver mantém `data/node_types/<lang>-node-types.json` (49k–161k bytes per lang) com tipo + importance score. Touring tem tree-sitter grammars mas a metadata está implícita. Expor via CLI + MCP enriquece queries declarativas.

**Scope (in)**:
- Novo `touring ast node-types <lang>` CLI cmd que emite JSON:
  ```json
  {
    "language": "rust",
    "node_count": 234,
    "node_types": [
      {"name": "function_item", "importance": 0.9, "category": "definition"},
      {"name": "let_declaration", "importance": 0.4, "category": "statement"},
      ...
    ]
  }
  ```
- Importance scoring (porting CodeWeaver semantics):
  - definition: 0.9-1.0 (functions, structs, enums, traits)
  - declaration: 0.6-0.8 (impl blocks, modules)
  - statement: 0.3-0.5 (let, expr stmts)
  - expression: 0.1-0.2 (terminals)
- Novo `touring ast importance <file>` filtra nodes por importance threshold
- MCP tool mirror: `touring_ast_node_types(language)` e `touring_ast_importance(file, threshold)`

**Files affected**:
- `touring-ast/src/node_types/mod.rs` — NEW
- `touring-ast/src/node_types/data/{rust,python,go,typescript,...}.json` — NEW assets (~50KB each)
- `touring-server/src/cli/ast.rs` — extend dispatcher — blast=1
- `touring-hooks/src/cli_handlers.rs` — `cli_ast_node_types`, `cli_ast_importance` — blast=2

**New deps**: nenhuma (serde_json + tree-sitter já)

**Acceptance criteria**:
1. `touring ast node-types rust -j` emite JSON com ≥ 100 node types
2. `touring ast importance <file.rs> --threshold 0.5` filtra para apenas nodes high-importance
3. 6 unit tests (load each lang JSON + threshold filter + edge cases)

**Tests added**: 6

**Risk**: LOW — metadata exposure, sem breaking changes

---

### TIER B (CodeWeaver) — Hybrid Semantic Search (Wave 5 — STRATEGIC INVESTMENT)

> **CRITICAL**: Esta é a maior gap competitiva. Touring atualmente NÃO TEM dense embeddings. Concorrentes (CodeWeaver, code-graph-ai, Cursor, Continue.dev) todos têm. Sem isto, Touring perde para queries naturais como "where do we handle retries?". Voyage Code-3 paper: hybrid search = +14.52% precision vs dense-only.

#### **D22 — Embedding provider abstraction (Wave 5.1)** [Size: M, ~600 LOC]

**Why**: Sem provider abstraction, Touring fica preso a 1 backend de embeddings. CodeWeaver tem 17 providers via DI Pydantic — Touring precisa equivalente trait-based.

**Scope (in)**:
- Novo crate `touring-embeddings/`:
  ```rust
  #[async_trait]
  pub trait EmbeddingProvider: Send + Sync {
      fn id(&self) -> &str;
      fn family(&self) -> ModelFamily;
      fn dimensions(&self) -> usize;
      async fn embed(&self, texts: &[String]) -> Result<Vec<DenseVector>, EmbeddingError>;
      async fn embed_query(&self, query: &str) -> Result<DenseVector, EmbeddingError>;
  }
  
  pub struct ModelFamily {
      pub name: String,        // "voyage-code", "fastembed-bge", "candle-bge"
      pub generation: String,  // "v3", "v2", etc.
  }
  ```
- 3 implementações iniciais (priorizadas por airgapped-first):
  - **Candle** (puro Rust, no Python interop): BGE small/base/large via candle-transformers
  - **FastEmbed** (via tokio-process subprocess Python — opcional, behind feature flag)
  - **Voyage AI** (HTTP client — opcional, requer API key)
- Sparse provider trait paralelo (BM25 já temos via tantivy, mas wrap em trait):
  ```rust
  pub trait SparseProvider: Send + Sync { ... }
  ```

**Files affected**:
- `touring-embeddings/Cargo.toml` — NEW
- `touring-embeddings/src/{lib,trait,error,family}.rs` — NEW (~200 LOC)
- `touring-embeddings/src/providers/{candle,fastembed,voyage}.rs` — NEW (~400 LOC)
- Workspace Cargo.toml — add member — blast=workspace

**New deps**: `candle-core = "0.7"`, `candle-nn`, `candle-transformers`, optional `reqwest` (Voyage)

**Acceptance criteria**:
1. `EmbeddingProvider::embed(["fn foo()"])` retorna `Vec<DenseVector>` com dimensions consistente
2. Family compatibility: 2 providers same family → vectors são interoperáveis
3. 3 providers cada com 8 unit tests + 1 integration
4. Performance: Candle BGE-small < 50ms per embedding em CPU

**Tests added**: 27

**Risk**: HIGH (probabilidade 0.5 / impacto MEDIUM) — Candle deps podem trazer transitive issues; modelos requerem download. **Mitigação**: 
- Lazy model loading (não download em build)
- Modelo MVP pinned: BGE-small-en-v1.5 (33M params, ~130MB)
- Test em CI com mock provider para evitar download

---

#### **D23 — Vector store abstraction (Wave 5.2)** [Size: L, ~1000 LOC]

**Why**: Embeddings sem persistência são inúteis. CodeWeaver usa Qdrant; também suporta in-memory fallback. Touring precisa de vector store abstrato com 2 backends: local-first (sqlite-vec, zero ops) + remoto (Qdrant, scaling).

**Scope (in)**:
- Novo crate `touring-vector-store/`:
  ```rust
  #[async_trait]
  pub trait VectorStore: Send + Sync {
      async fn upsert(&self, points: Vec<Point>) -> Result<UpsertResult>;
      async fn search(&self, query: &SearchQuery) -> Result<Vec<SearchResult>>;
      async fn delete(&self, ids: &[PointId]) -> Result<()>;
      async fn collection_info(&self, name: &str) -> Result<CollectionInfo>;
      async fn create_collection(&self, name: &str, schema: &CollectionSchema) -> Result<()>;
  }
  ```
- Backend 1: **sqlite-vec** (puro local, zero deps externas, default)
- Backend 2: **Qdrant** (HTTP client, opt-in)
- Backend 3: **InMemory** (HashMap-based, para tests + airgapped fallback)
- Asymmetric embeddings support: Point armazena `dense_provider_id`, `sparse_provider_id`
- Hybrid scoring built-in: `SearchQuery { dense: Option<Vec<f32>>, sparse: Option<SparseVec>, weights: HybridWeights }`

**Files affected**:
- `touring-vector-store/Cargo.toml` — NEW
- `touring-vector-store/src/{lib,trait,error,query}.rs` — NEW (~250 LOC)
- `touring-vector-store/src/backends/{sqlite_vec,qdrant,inmemory}.rs` — NEW (~750 LOC)
- Workspace Cargo.toml — add member — blast=workspace

**New deps**: `sqlite-vec = "0.1"`, `qdrant-client = "1.x"` (opt-in feature), `rusqlite = "0.31"` (verify if already wired)

**Acceptance criteria**:
1. sqlite-vec backend persiste e recupera 10k vectors em < 100ms
2. Qdrant backend (com Docker test container) idem
3. InMemory backend para CI tests
4. Hybrid query (dense + sparse) com weights customizáveis funciona em todos backends
5. 24 unit tests + 6 integration (3 backends × 2 scenarios cada)

**Tests added**: 30

**Risk**: HIGH (probabilidade 0.4 / impacto MEDIUM) — sqlite-vec é young (compilação C), Qdrant requer deployment. **Mitigação**:
- sqlite-vec validado em CI matrix (Linux/macOS)
- Qdrant atrás de feature flag opt-in `--features qdrant`
- InMemory backend sempre disponível como fallback

---

#### **D24 — Hybrid scoring + RRF + reranking pipeline (Wave 5.3)** [Size: M, ~500 LOC]

**Why**: Tendo embeddings (D22) + vector store (D23), faltar hybrid scoring é desperdício. CodeWeaver usa: dense=0.65, sparse=0.35, depois RRF fusion, depois reranking (5 providers cascading). Touring deve replicar.

**Scope (in)**:
- Estender ou criar `touring-search-fusion`:
  ```rust
  pub struct HybridWeights { pub dense: f32, pub sparse: f32 }
  impl Default for HybridWeights { fn default() -> Self { Self { dense: 0.65, sparse: 0.35 } } }
  
  pub fn apply_hybrid_weights(candidates: &mut [SearchResult], weights: &HybridWeights);
  pub fn reciprocal_rank_fusion(rankings: &[Vec<SearchResult>], k: f32 /* default 60 */) -> Vec<SearchResult>;
  
  #[async_trait]
  pub trait Reranker: Send + Sync {
      async fn rerank(&self, query: &str, candidates: Vec<SearchResult>) -> Result<Vec<SearchResult>>;
  }
  
  pub struct RerankerCascade { providers: Vec<Box<dyn Reranker>> }
  impl Reranker for RerankerCascade { /* try each, fall back on error */ }
  ```
- Reranker MVP: trivial sort by score (no-op, prepara API)
- Reranker advanced (gated, opt-in): cross-encoder via Candle, Cohere API, Voyage API
- Integration: D13 (intent-aware boost) aplicado APÓS hybrid+RRF+rerank

**Files affected**:
- `touring-search-fusion/src/scoring.rs` — NEW (~250 LOC)
- `touring-search-fusion/src/reranker.rs` — NEW (~150 LOC)
- `touring-server/src/cli/search.rs` — wire pipeline — blast=2
- `touring-hooks/src/cli_handlers.rs` — `cli_search_unified` extend — blast=1

**New deps**: nenhuma adicional (Candle from D22)

**Acceptance criteria**:
1. RRF (k=60) fusion combina 3 backends → top-10 com badges mostrando agreement
2. Hybrid weights customizable em `touring.toml [search.hybrid]`
3. Reranker cascade: primary fail → fallback → no-op
4. Intent boost (D13) aplicado after rerank
5. 16 unit tests

**Tests added**: 16

**Risk**: MEDIUM — algoritmos clássicos, mas integration com 3 stages é complexa. **Mitigação**: cada stage testável em isolation.

---

#### **D25 — Asymmetric embeddings + manifest integration (Wave 5.4)** [Size: M, ~400 LOC]

**Why**: Sem isto, mudar embedding provider exige reindex completo. CodeWeaver permite query model muda sem reindex se família igual. Aproveita D18 (CheckpointSettingsFingerprint).

**Scope (in)**:
- Estender `touring-vfs::manifest` para incluir embedding metadata:
  ```rust
  pub struct FileManifestEntry {
      // existing
      pub path: PathBuf,
      pub content_hash: BlakeHash,
      pub chunk_ids: Vec<ChunkId>,
      // new (Wave 5.4)
      pub dense_embedding_provider: Option<String>,
      pub dense_embedding_model: Option<String>,
      pub sparse_embedding_provider: Option<String>,
      pub sparse_embedding_model: Option<String>,
  }
  
  impl FileManifest {
      pub fn get_files_needing_embeddings(&self,
          current_dense: Option<(&str, &str)>,
          current_sparse: Option<(&str, &str)>,
      ) -> EmbeddingsToAdd { /* dense_only, sparse_only, both */ }
  }
  ```
- Logic de "files_needing_embeddings":
  - Dense missing → embed dense
  - Sparse missing → embed sparse
  - Family changed → reembed both
  - Family same + model different (asymmetric) → keep document embeddings, only update query path
- Connect com D18 fingerprint para decide reindex strategy

**Files affected**:
- `touring-vfs/src/manifest.rs` — extend (~200 LOC) — blast=2
- `touring-core/src/checkpoint/fingerprint.rs` — wire embedding info — blast=2
- `touring-server/src/cli/session.rs` — show "files needing embeddings" — blast=1
- `touring-hooks/src/cli_handlers.rs` — `cli_manifest_status` — blast=1

**New deps**: nenhuma

**Acceptance criteria**:
1. New file → marked dense+sparse missing
2. Switch dense provider same-family → no reembed (asymmetric ok)
3. Switch dense provider different-family → trigger full reembed
4. `touring session start` mostra count `dense_only=12, sparse_only=3`
5. 14 unit tests

**Tests added**: 14

**Risk**: MEDIUM — coordenação multi-deliverable. **Mitigação**: deliver as last sub-wave de Wave 5; testes integration end-to-end.

---

### TIER A (CodeWeaver) — Agent UX (Wave 6)

#### **D26 — `touring find_code(description)` super-tool MCP** [Size: M, ~600 LOC]

**Why**: CodeWeaver philosophy: 1 super-tool reduz cognitive load do agente. Touring tem 96 MCP tools — adicionar 1 super-tool **sem remover outros** maximiza ROI agent-friendly. Não é substituição, é facade orchestrator.

**Scope (in)**:
- Novo MCP tool `touring_find_code`:
  ```rust
  pub struct FindCodeParams {
      pub query: String,
      pub intent: Option<QueryIntent>,
      pub focus_languages: Option<Vec<String>>,
      pub token_limit: Option<usize>,
      pub include_tests: bool,
      pub max_results: Option<usize>,
  }
  
  pub struct FindCodeResponse {
      pub matches: Vec<CodeMatch>,
      pub total_tokens: usize,
      pub intent_detected: QueryIntent,
      pub strategy_used: SearchStrategy,
  }
  ```
- Pipeline orquestrado:
  1. `detect_intent(query)` (D13)
  2. Hybrid search (Wave 5: D22+D23+D24) OU fallback BM25-only se Wave 5 not deployed
  3. Filter por focus_languages
  4. Exclude tests se !include_tests
  5. Format compact (token-efficient), respeita token_limit
- CLI mirror: `touring find-code <description>`
- Compact output: `file:line:col\n  symbol_name\n  one-line context\n` para máxima densidade

**Files affected**:
- `touring-server/src/cli/find_code.rs` — NEW (~250 LOC)
- `touring-server/src/mcp/tools/find_code.rs` — NEW (~150 LOC)
- `touring-hooks/src/cli_handlers.rs` — `cli_find_code` handler — blast=1
- MCP tools registry — register new tool — blast=1

**New deps**: nenhuma (depende de Waves 1-5 deliverables)

**Acceptance criteria**:
1. `touring find-code "where do we validate JWT tokens"` retorna ≤ 5 matches relevantes
2. Token count < `token_limit` se especificado
3. Mode degraded: se Wave 5 não disponível, cai para tantivy + index find via D4 (RRF)
4. Test em ≥ 3 projetos diferentes (touring workspace + 2 outros)
5. 18 unit tests + 4 integration

**Tests added**: 22

**Risk**: MEDIUM — depende cumulativamente de Waves 2-5. Se Wave 5 atrasar, Wave 6 atrasa. **Mitigação**: degraded mode shippable mesmo sem Wave 5 (D26 com BM25-only é viável).

---

### TIER B (CodeWeaver) — Polish & DI (Wave 7)

#### **D27 — Plugin architecture runtime swap (DI)** [Size: L, ~800 LOC]

**Why**: Touring tem typestate Rust (compile-time correctness). Adiciona DI runtime para providers (embedding, reranker, vector store) — não substitui typestate, complementa. CodeWeaver "zero-code provider swap" via Pydantic DI.

**Scope (in)**:
- Novo `touring-core::plugin`:
  ```rust
  pub trait Provider: Send + Sync + Any {
      fn id(&self) -> &str;
      fn version(&self) -> &str;
      fn capabilities(&self) -> ProviderCapabilities;
  }
  
  pub struct ProviderRegistry {
      embedding: HashMap<String, Arc<dyn EmbeddingProvider>>,
      vector_store: HashMap<String, Arc<dyn VectorStore>>,
      reranker: HashMap<String, Arc<dyn Reranker>>,
  }
  
  impl ProviderRegistry {
      pub fn get_default<T: Provider>(&self) -> Option<Arc<T>>;
      pub fn set_default<T: Provider>(&mut self, id: &str);
  }
  ```
- CLI: 
  - `touring config providers list` mostra registered + active
  - `touring config providers set embedding=fastembed`  → restart daemon → uses FastEmbed
  - `touring config providers test <id>` valida health
- Discoverable via `inventory` crate ou manual registration

**Files affected**:
- `touring-core/src/plugin/mod.rs` — NEW (~400 LOC)
- `touring-core/src/plugin/registry.rs` — NEW (~200 LOC)
- `touring-server/src/cli/config.rs` — extend — blast=2
- All providers (D22, D23, D24) — register via inventory — blast=3

**New deps**: `inventory = "0.3"` (verify) ou manual registration

**Acceptance criteria**:
1. `touring config providers list` mostra 3+ providers registered
2. Switch via CLI causa daemon restart graceful
3. Plugin desconhecido → error com sugestão (typo correction via tantivy fuzzy)
4. 16 unit tests + 4 integration

**Tests added**: 20

**Risk**: MEDIUM — runtime DI em Rust é não-idiomático mas viável; trait objects + Arc<dyn>. **Mitigação**: manter typestate como source-of-truth para correctness; runtime registry é só para provider selection.

---

#### **D28 — MCP overhead self-report** [Size: XS, ~80 LOC]

**Why**: CodeWeaver enfatiza que Serena tem 16k tokens em prompt overhead vs 500 em CodeWeaver. Touring tem 96 MCP tools — overhead pode ser alto. Auto-relatar permite Gabriel decidir quais ferramentas pruning.

**Scope (in)**:
- Estender `touring-server::telemetry`:
  ```rust
  pub fn estimate_mcp_overhead() -> McpOverheadReport {
      // Para cada tool registered: count tokens em description + parameters schema
      // Sum total
      // Per-tool breakdown
  }
  ```
- CLI: `touring mcp-overhead [--format json|table] [--top N]`
- Self-report em `touring instructions-loaded` quando session start: "MCP overhead estimado: N tokens"

**Files affected**:
- `touring-server/src/telemetry/mcp_overhead.rs` — NEW (~80 LOC)
- `touring-server/src/cli/mod.rs` — register cmd — blast=1
- `touring-hooks/src/instructions_loaded.rs` — wire report — blast=1

**New deps**: `tiktoken-rs = "0.5"` (token counting BPE) ou aproximação string len/4

**Acceptance criteria**:
1. `touring mcp-overhead --top 10` lista 10 tools mais "caros" em tokens
2. Total report < 16k é "good", > 16k é "warning" (visual indicator)
3. 4 unit tests

**Tests added**: 4

**Risk**: LOW

---

## 3.6 Catálogo de deliverables Thread-derived (D29-D41, 13 itens)

> Adicionados na revisão v3 (2026-05-01) após análise profunda de knitli/thread (Rust, AGPLv3, high-perf code analysis platform). Estes 13 deliverables fecham o 7º gap (Knowledge Layer & Dataflow Extensibility) e complementam D1-D28 sem sobreposição. T1-T13 da análise mapeiam para D29-D41.

### TIER S (Thread) — Quick wins e foundation (Wave 4 + Wave 7)

#### **D29 (T1) — `TouringFlowBuilder` declarative dataflow API** [Size: M, ~600 LOC]

**Why**: ThreadFlowBuilder mostra fluent API substitui boilerplate `decompose create/add/validate`. Touring decompose é poderoso mas exige 5+ comandos para pipelines simples. Builder pattern unifica e abre Touring para uso programático embedded.

**Scope (in)**:
- Novo crate `touring-flow/`:
  ```rust
  TouringFlowBuilder::new("audit_workspace")
      .source_local("crates/", &["**/*.rs"], &["target/**"])
      .parse_ast()                         // Step::ParseAst
      .extract_blast_radius()              // Step::ExtractBlast
      .extract_quality_score()             // Step::ExtractQuality (TDG 6-dim)
      .extract_wiring_audit()              // Step::ExtractWiring
      .extract_cycles(min_depth: 2)        // Step::ExtractCycles (Tarjan SCC)
      .extract_cognitive_metrics()         // Step::ExtractCognitive
      .filter(|node| node.tdg_grade <= TdgGrade::C)
      .target_sqlite("audit_results", &["file_path"])
      .target_dot("/tmp/audit.dot")        // pipe to D1 visual export
      .target_json("/tmp/audit.json")
      .build().await?
      .execute().await?
  ```

- Steps suportados (12 inicial):
  - `parse_ast` — tree-sitter AST
  - `extract_symbols` — definições + visibility
  - `extract_imports` — imports/uses
  - `extract_calls` — call graph edges
  - `extract_blast_radius` — touring-ast::blast
  - `extract_quality_score` — TDG 6-dim
  - `extract_wiring_audit` — orphans + integration_score
  - `extract_cycles` — Tarjan SCC
  - `extract_cognitive_metrics` — modularity, community, fan_in/out
  - `extract_rust_semantic` — generics, trait bounds, lifetimes
  - `extract_clones` — signature hashing (D9)
  - `apply_intent_classification` — D13 intent

- Targets suportados (5 inicial):
  - `target_sqlite(table, primary_key[])` — local persistent
  - `target_postgres(...)` — D34 quando shippado
  - `target_dot(path)` — D1 visual export
  - `target_mermaid(path)` — D1 mermaid export
  - `target_json(path)` — raw structured

- Filter chain:
  - `filter(predicate)` — closure-based
  - `filter_dsl(yaml-or-string)` — D12 DSL quando shippado

- CLI mirror: `touring flow run <yaml-config>` consome YAML pipeline declarativo
- YAML pipeline format:
  ```yaml
  name: audit_workspace
  source:
    local:
      path: crates/
      included: ["**/*.rs"]
      excluded: ["target/**"]
  steps:
    - parse_ast
    - extract_blast_radius
    - extract_quality_score
    - filter:
        predicate: "tdg_grade <= C"
  targets:
    - sqlite:
        table: audit_results
        primary_key: [file_path]
    - dot: /tmp/audit.dot
  ```

**Files**:
- `touring-flow/Cargo.toml` — NEW
- `touring-flow/src/lib.rs` — NEW (~50 LOC)
- `touring-flow/src/builder.rs` — NEW (~400 LOC, ThreadFlowBuilder analog)
- `touring-flow/src/{steps,targets,executor,error}.rs` — NEW (~200 LOC)
- `touring-flow/src/yaml_loader.rs` — NEW (~100 LOC)
- `touring-server/src/cli/flow.rs` — NEW (~80 LOC)
- Workspace Cargo.toml — add member — blast=workspace

**New deps**: nenhuma (toml + serde + tokio já)

**Acceptance criteria**:
1. `TouringFlowBuilder::new("test").source_local("src/").parse_ast().target_json("/tmp/out.json").build().await?.execute().await?` — happy path
2. YAML pipeline consumido: `touring flow run audit.yaml` — produces JSON + DOT outputs
3. 12 steps + 5 targets cada com 2 unit tests = 34 tests + 6 integration (full pipeline em 6 cenários)
4. Performance: pipeline com 1k files termina em < 30s

**Tests added**: 40

**Risk**: LOW — builder pattern bem-conhecido

---

#### **D30 (T2) — YAML Rule Engine + fix transformations** [Size: L, ~800 LOC]

**Why**: `touring-assists` (Wave C.1 2026-04-28) tem 10 handlers ad-hoc em Rust. Thread mostra config-driven YAML rules + fix permite usuários estender SEM tocar Rust. Massive extensibility leverage. Substitutes ad-hoc patterns por linter-style declarative rules.

**Scope (in)**:
- Novo crate `touring-rule-engine/` (mirror thread-rule-engine):
  ```yaml
  # ~/.claude/touring/rules/no-unwrap-prod.yaml
  id: no-unwrap-prod
  description: "Disallow .unwrap() in production code"
  language: Rust
  severity: error
  rule:
    pattern: "$EXPR.unwrap()"
    not:
      inside:
        kind: test_function    # exclude #[test] modules
  fix: "$EXPR?"
  ```

- Rule operators (port ast-grep YAML schema):
  - `pattern: <string>` — meta-variable pattern
  - `kind: <node-kind>` — tree-sitter node type
  - `regex: <pattern>` — fallback regex
  - `inside: <rule>` — context constraint
  - `not: <rule>` — negation
  - `all: [<rules>]` — conjunction
  - `any: [<rules>]` — disjunction
  - `has: <rule>` — child existence

- CLI:
  - `touring rule list` — registered rules
  - `touring rule run <file> [--rule <id>]` — run all or specific
  - `touring rule test <id>` — test fixtures
  - `touring rule explain <id>` — pattern + fix preview
  - `touring rule fix <file> [--rule <id>] [--apply]` — apply fix (com speculative validation D26 pattern)

- Built-in catalog (ship com ~30 rules curadas):
  - Rust: `no-unwrap-prod`, `unify-error-types`, `prefer-if-let`, `no-unused-mut`, `prefer-iter-collect`, `no-clone-in-loop`, etc.
  - TypeScript: `no-var-declarations`, `prefer-const`, `no-any-type`, etc.
  - Python: `no-mutable-default-args`, `prefer-f-string`, etc.

- Auto-discovery: `~/.claude/touring/rules/*.yaml` carregado em startup
- Per-project override: `<workspace>/.touring/rules/*.yaml`

**Files**:
- `touring-rule-engine/Cargo.toml` — NEW
- `touring-rule-engine/src/{lib,parser,runner,fixer,error}.rs` — NEW (~600 LOC)
- `touring-rule-engine/builtin/{rust,typescript,python,go}/*.yaml` — NEW (~30 files)
- `touring-server/src/cli/rule.rs` — NEW (~200 LOC)
- Integration com `touring-assists` (Wave C.1) — refactor 10 handlers para serem chamáveis via YAML — blast=10
- Workspace Cargo.toml — add member — blast=workspace

**New deps**: `serde_yaml = "0.9"` (verify)

**Acceptance criteria**:
1. `touring rule run src/foo.rs --rule no-unwrap-prod` retorna violations
2. `touring rule fix src/foo.rs --rule no-unwrap-prod --apply` aplica fix com speculative validation (score >= 0.8)
3. Rule operators all/any/not/inside/has todos testados
4. Built-in catalog ≥ 30 rules, todos com pelo menos 3 fixtures (positive, negative, edge)
5. 24 unit tests + 8 integration (rule loading, parsing, running, fixing) + 30 catalog tests

**Tests added**: 62

**Risk**: MEDIUM (probabilidade 0.4 / impacto HIGH if fix breaks code) — fix correctness é crítico. **Mitigação**:
- `--apply` requires speculative validation (touring shadow validate score >= 0.8)
- Default mode é dry-run (mostra diff mas não aplica)
- Each rule tem fixture-based tests (positive + negative + edge)
- Idempotência testada: `apply` 2x = mesmo resultado

---

#### **D31 (T3) — `touring-definitions` Semantic Classification crate** [Size: M, ~500 LOC + ~200KB data]

**Why**: Thread fez port direto de CodeWeaver Python (7,200 LOC) → Rust (~1,800 LOC) com **99.7% accuracy across 27 languages**. Touring pode beneficiar diretamente. Adicionar 22 categorias language-agnostic enriquece TODAS as outras análises (D13 intent, D24 hybrid scoring, file-knowledge extended, D21 node types). **Pareamento natural com D21** (Wave 4) — node types KB + classification = full semantic understanding.

**Scope (in)**:
- Novo crate `touring-definitions/`:
  ```rust
  pub enum SemanticClass {
      // Definitions (importance 0.9-1.0)
      FunctionDef, StructDef, EnumDef, TraitDef, ClassDef, InterfaceDef, TypeAlias,
      // Declarations (0.6-0.8)
      ImplBlock, Module, ModuleItem,
      // Statements (0.3-0.5)
      LetBinding, ExprStatement, Assignment,
      // Expressions (0.1-0.2)
      Literal, Identifier, Operator,
      // Imports/Exports
      Import, Export,
      // Comments
      Comment, Docstring,
      // Decorators/Attributes
      Decorator, MacroInvocation,
      // Other
      Unknown,
  }
  
  pub struct ImportanceRank(pub f32);  // 0.0 - 1.0
  pub struct TokenPurpose { /* ... */ }
  
  pub fn classify(node_type: &str, language: SupportLang) -> (SemanticClass, ImportanceRank);
  ```

- Pipeline determinístico (lookup, não decision engine):
  ```
  1. override          (TOML per-language, ~41 LOC avg)
  2. file_detection    (extension-based pre-filter)
  3. token_purpose     (universal "kind" hints)
  4. universal_exact   (2,444 cross-language exact rules)
  5. universal_majority (21 majority rules)
  6. category          (55 category mappings)
  7. name_heuristic    (suffix/prefix patterns)
  8. unclassified      (fallback)
  ```

- 22 categories language-agnostic (port from Thread spec)
- Data files (port from Thread):
  - `data/universal_rules.json` — 2,444 exact + 21 majority cross-language rules (~50KB)
  - `data/categories.json` — 55 category → SemanticClass mappings (~10KB)
  - `data/scoring.json` — ImportanceScores per class (~5KB)
  - `data/overrides/{rust,python,typescript,go,java,c,cpp,...}.toml` — ~25 files

- Adicionar nova língua = `node-types.json` + ~41 LOC TOML override → ZERO Rust changes
- Universal rules baseline: 82.6% accuracy sem language-specific data

- Integration:
  - `touring-ast` consome para enriquecer `ast meta` output
  - `touring-cognitive` usa classification em modularity/community detection
  - `D13` intent classification usa SemanticClass para ranking boost
  - `D24` hybrid scoring usa para chunk weighting
  - CLI: `touring definitions classify <file>` mostra classification per node

**Files**:
- `touring-definitions/Cargo.toml` — NEW (zero deps de tree-sitter — apenas serde + thiserror + toml)
- `touring-definitions/src/{lib,types,classifier,rules,scoring,error}.rs` — NEW (~400 LOC)
- `touring-definitions/data/{universal_rules,categories,scoring}.json` — NEW assets
- `touring-definitions/data/overrides/*.toml` — NEW (~25 files, ~41 LOC each)
- `touring-ast/src/meta.rs` — extend para incluir SemanticClass — blast=2
- `touring-server/src/cli/definitions.rs` — NEW (~80 LOC)
- Workspace Cargo.toml — add member — blast=workspace

**New deps**: nenhuma adicional (toml já)

**Acceptance criteria**:
1. `touring definitions classify src/main.rs -j` retorna classification per node com importance
2. Benchmark: 99.5%+ accuracy em corpus de 27 langs (relaxed de 99.7% para tolerância porting)
3. Adicionar nova língua (e.g. Zig): `data/overrides/zig.toml` ~50 LOC + zero Rust changes → ≥ 80% accuracy
4. Pipeline ordering preservado: override > file_detection > ... > unclassified
5. Performance: classify 10k nodes em < 50ms
6. 18 unit tests + 27 integration (1 per supported language) + 5 corner cases

**Tests added**: 50

**Risk**: LOW — data-driven, validated arquiteturalmente em Thread

---

#### **D32 (T4) — Tier-based language support honest UX** [Size: XS, ~80 LOC + docs]

**Why**: Thread declara `Tier 1/2/3/4` com transparência sobre maturity. Touring promete "16+ langs" mas qualidade varia muito. Honest UX builds trust e gerencia expectativas. Aplicação direta da filosofia "honest UX" do Thread.

**Scope (in)**:
- Novo `touring-language::Tier` enum:
  ```rust
  pub enum LanguageTier {
      Tier1Primary,    // Full feature parity (RL + generator + assists + chunking + classification)
      Tier2Full,       // Most features (no advanced refactor)
      Tier3Community,  // Basic AST + chunking + search
      Tier4Specialized,// Tree-sitter only, no quality scoring
  }
  ```

- Sugestão de tier para Touring:
  - **Tier 1 Primary**: Rust, TypeScript/TSX, Python (full feature parity em RL + generator + assists + classification)
  - **Tier 2 Full**: Go, Java, C/C++ (most features, advanced refactor parcial)
  - **Tier 3 Community**: JavaScript, Ruby, Swift, Kotlin, Scala, PHP, C# (basic AST + chunking + search)
  - **Tier 4 Specialized**: Bash, YAML, JSON, HCL/Terraform, Nix, Solidity, CSS, HTML, SQL (tree-sitter only)

- CLI:
  - `touring lang status -j` — JSON com matrix de capabilities por language
  - `touring lang status --tier 1` — apenas tier 1
  - `touring lang capabilities <lang> -j` — detail por capability

- Output JSON:
  ```json
  {
    "languages": [
      {"name": "rust", "tier": "Tier1Primary", "capabilities": ["ast", "rl", "generator", "assists", "classification"], "test_coverage": 0.92},
      {"name": "go", "tier": "Tier2Full", "capabilities": ["ast", "rl", "assists", "classification"], "test_coverage": 0.71},
      ...
    ]
  }
  ```

- Doc:
  - SKILL.md — adicionar "Language Support Tiers" section com tabela
  - README.md (se existe) — same
  - Per-tier expectation explicit

**Files**:
- `touring-language/src/tier.rs` — NEW (~40 LOC enum + impl)
- `touring-language/src/capabilities.rs` — NEW (~30 LOC capabilities matrix)
- `touring-server/src/cli/lang.rs` — NEW (~30 LOC dispatcher)
- `touring-hooks/src/cli_handlers.rs` — `cli_lang_status`, `cli_lang_capabilities` handlers — blast=2
- SKILL.md addition (~50 lines tier table)

**New deps**: nenhuma

**Acceptance criteria**:
1. `touring lang status --tier 1 -j` retorna apenas Tier 1 languages
2. `touring lang capabilities rust` retorna full capability matrix
3. SKILL.md mostra tabela 4-tier explícita
4. 5 unit tests (each tier + edge cases)

**Tests added**: 5

**Risk**: LOW

---

#### **D33 (T5) — Multi-tier conflict detection com SLAs explícitos** [Size: S, ~250 LOC]

**Why**: Thread documenta Tier 1 AST diff <100ms / Tier 2 Semantic <1s / Tier 3 Graph Impact <5s com SLAs formais. Touring tem detecção mas sem SLAs. Adicionar SLAs + benchmark regression-test = production-grade reliability. Pareamento com D19 (FailoverService) e D38 (perf benchmarks) — 3 components forming a quality signaling subsystem.

**Scope (in)**:
- Novo `touring-core::conflict`:
  ```rust
  pub enum ConflictTier {
      AstDiff,      // SLA: < 100ms p99
      Semantic,     // SLA: < 1s p99
      GraphImpact,  // SLA: < 5s p99
  }
  
  pub struct SlaSpec { pub tier: ConflictTier, pub p99_ms: u32 }
  
  pub trait ConflictDetector: Send + Sync {
      fn tier(&self) -> ConflictTier;
      fn sla(&self) -> SlaSpec;
      async fn detect(&self, before: &State, after: &State) -> Result<Vec<Conflict>, DetectorError>;
  }
  ```

- 3 default impls:
  - `AstDiffDetector` (Tier 1) — uses `touring-ast` for syntactic diff
  - `SemanticConflictDetector` (Tier 2) — uses `touring-semantics` (Wave B5 W3.0)
  - `GraphImpactDetector` (Tier 3) — uses `touring-cognitive::semantic_graph`

- CI gate: `iai-callgrind` benchmark fails se P99 > SLA
- `touring conflict detect <a> <b> [--tier 1|2|3]` CLI command
- Métricas em `touring gate-metrics -j`:
  - `conflict_tier1_p99_ms`, `conflict_tier2_p99_ms`, `conflict_tier3_p99_ms`
  - `conflict_tier1_violations_count` (cumulativo)

**Files**:
- `touring-core/src/conflict/{mod,detector,sla,error}.rs` — NEW (~120 LOC trait + types)
- `touring-core/src/conflict/impls/{ast_diff,semantic,graph_impact}.rs` — NEW (~80 LOC each + reuse existing)
- `touring-server/benches/conflict_sla.rs` — NEW (~50 LOC, iai-callgrind)
- `touring-server/src/cli/conflict.rs` — NEW (~30 LOC)
- `touring-hooks/src/cli_handlers.rs` — `cli_conflict_detect` — blast=1
- `touring-server/src/telemetry/gate_metrics.rs` — adicionar 6 counters — blast=1

**New deps**: nenhuma (iai-callgrind já no workspace)

**Acceptance criteria**:
1. `touring conflict detect <a> <b> --tier 1 -j` retorna conflicts em < 100ms (verified via bench)
2. CI fails se P99 regression > 10% sob workload típico
3. 3 detectors × 4 unit tests = 12 + 6 integration (cross-tier scenarios)
4. Métricas counters atualizam em cada detect() call

**Tests added**: 18

**Risk**: LOW

---

### TIER A (Thread) — Investimentos arquiteturais (Wave 3 + 7 + 8)

#### **D34 (T6) — Postgres backend opcional** [Size: M, ~600 LOC]

**Why**: Touring é SQLite-only. Thread tem Postgres backend para CLI deployment com <10ms p95 latency. Postgres = horizontal scaling + cross-machine cache para teams. **Apenas faz sentido se Touring escala além de Gabriel** (multi-team usage).

**Scope (in)**:
- Estender storage trait existente:
  ```rust
  pub trait StorageBackend: Send + Sync {
      async fn upsert(&self, ...) -> Result<()>;
      async fn query(&self, ...) -> Result<Vec<Row>>;
      async fn migrate(&self, version: u32) -> Result<()>;
  }
  ```

- Backends:
  - `SqliteBackend` (existing) — default
  - `PostgresBackend` (NEW) — feature flag `--features postgres-backend`
  - Eventually: D1 (Wave 8 D35), Qdrant (Wave 5 D23)

- CLI:
  - `touring config storage=postgres --url postgresql://user:pass@host/db`
  - `touring storage migrate` — schema migration
  - `touring storage health` — P95 latency check

- Schema parity: cada SQLite table → Postgres equivalent com migration path

**Files**:
- `touring-core/src/storage/postgres.rs` — NEW (~500 LOC)
- `touring-core/src/storage/migration.rs` — NEW (~80 LOC schema versioning)
- `touring-server/src/cli/storage.rs` — NEW (~50 LOC)
- Cargo.toml — `tokio-postgres = "0.7"`, `sqlx = { version = "0.8", features = ["postgres", "runtime-tokio"] }` (atrás de feature flag)

**New deps**: `tokio-postgres = "0.7"` ou `sqlx`

**Acceptance criteria**:
1. `touring config storage=postgres --url <DSN>` configura
2. `touring index --rebuild` populates Postgres com schema correto
3. Performance: query symbol exists em < 10ms p95
4. Migration: SQLite → Postgres preserve data
5. 18 unit tests + 6 integration (com Docker-postgres test container)

**Tests added**: 24

**Risk**: MEDIUM — schema migration cross-backend (R28-related) + Docker requirement em CI

---

#### **D35 (T7) — Cloudflare Workers Edge target via WASM** [Size: XL, multi-wave]

**Why**: Thread tem `thread-wasm` crate compilado para Cloudflare Workers + D1 distributed cache + tokio async. Touring tem `touring-wasm` (inferlets, Edge inference) mas não Cloudflare Workers nativos. Edge = global CDN distribution com <50ms p95 worldwide.

**Caveat**: HIGH effort (XL); não se sabe se Gabriel usa Touring Edge. **Postergado a Wave 8.A** (sub-wave isolada). Provavelmente NÃO ship sem demand validation.

**Scope (in)**:
- Estender `touring-wasm` para target Cloudflare Workers especificamente
- D1 backend (parallel a D34 Postgres)
- HTTP API endpoint para queries read-only
- Authentication: Cloudflare Access ou JWT
- Wrangler deploy script

**Files**:
- `touring-wasm/src/cloudflare.rs` — NEW (~600 LOC)
- `touring-wasm/wrangler.toml` — NEW
- Workspace Cargo.toml extensions

**New deps**: `worker = "0.4"` (Cloudflare Workers Rust SDK)

**Risk**: HIGH — Edge debugging painful, deployment infrastructure complexa

---

#### **D36 (T8) — Bidirectional file ↔ graph sync engine** [Size: L, ~1.000 LOC]

**Why**: Thread "Option C" exige isto: AI agents podem editar via graph, projetar de volta a files. Touring atualmente é unidirectional file → graph. Mudar significa redesign de pipelines.

**Caveat**: Paradigm shift grande. **Postergado a Wave 8.B**. Provavelmente NÃO ship — Touring é file-centric por design.

**Scope (in)** (se shipped):
- Trait `Projector`: graph → file source
- Edits-via-graph API: AI agent invoca `touring graph edit symbol=X new_body=Y`
- Conflict resolution: graph edits + concurrent file edits → merge
- Integração com D8 (snapshot) e D37 (overlay)

**Risk**: HIGH — design impact massivo

---

#### **D37 (T9) — Overlay Graph Architecture (Base + Delta + Unified)** [Size: L, ~1.200 LOC]

**Why**: Thread separa Base Layer (immutable at git commit) + Delta Layer (uncommitted edits) + Unified View (merged at query time). Pareamento natural com D8 (snapshot create/diff) — extender D8 para overlay completo. **Move from Wave 8 to Wave 3.C** (sub-wave de Capability Parity, evolução natural de D8).

**Scope (in)**:
- Estender `touring-server::snapshot` (D8):
  ```rust
  pub struct OverlayGraph {
      base: BaseLayer,        // immutable at last touring index rebuild
      delta: DeltaLayer,      // ephemeral, uncommitted file changes
  }
  
  impl OverlayGraph {
      pub fn unified_view(&self) -> UnifiedView { /* merge at query */ }
      pub fn promote_delta_to_base(&mut self) -> Result<()> { /* commit */ }
      pub fn discard_delta(&mut self) -> Result<()> { /* abort */ }
  }
  ```

- Multi-tier conflict detection (D33) consome Overlay para diff Base ↔ Delta
- CLI:
  - `touring overlay status -j` — base_commit, delta_files_count, conflicts_detected
  - `touring overlay promote` — equivalent a `touring index --rebuild` (full reindex)
  - `touring overlay discard` — clear delta layer
  - `touring overlay diff -j` — show what's in delta vs base

- Storage: delta in-memory (não persistido); base em SQLite/Postgres
- Performance target: query unified < 1s para 100k files

**Files**:
- `touring-server/src/overlay/{mod,base,delta,unified}.rs` — NEW (~700 LOC)
- `touring-server/src/snapshot/store.rs` — extend para integrar overlay (D8) — blast=2
- `touring-server/src/cli/overlay.rs` — NEW (~150 LOC)
- `touring-hooks/src/cli_handlers.rs` — 4 handlers — blast=4
- Integration com D33 (conflict detection) — blast=2

**New deps**: nenhuma

**Acceptance criteria**:
1. `touring overlay status` mostra base + delta state
2. Edit local file (não promoted) → `touring overlay diff` mostra delta
3. `touring overlay promote` move delta → base (equivalent a reindex)
4. Performance: unified view query < 1s em 100k files (target threshold)
5. 18 unit tests + 8 integration

**Tests added**: 26

**Risk**: MEDIUM — coordenação com D8 (snapshot) e D33 (conflict) — testes integration críticos

---

#### **D38 (T10) — Performance benchmarks formais cross-language** [Size: M, ~400 LOC]

**Why**: Thread tem benchmark suite formal (Phase 5 Real-World) com 1k+ files/s, 0.6s incremental por language. Touring tem `iai-callgrind` micro-benches mas não cross-language throughput formal. Adicionar fornece baseline para regressões e documenta capabilities reais.

**Scope (in)**:
- Novo `touring/benches/throughput.rs`:
  ```rust
  // Para cada language tier (D32):
  //   parse 10k synthetic files → measure p50/p99/throughput
  //   extract symbols/imports/calls → idem
  //   index full → idem
  //   incremental update 1% files → idem
  ```

- Benchmark targets:
  - Rust: 1,365+ files/s (match Thread baseline)
  - TypeScript: 944+ f/s
  - Python: 1,188+ f/s
  - Go: 1,870+ f/s
  - Cache hit rate: >= 90% em rerun
  - Incremental: 1% update < 10% full time

- CI gate em `~/.claude/rust/.github/workflows/`:
  ```yaml
  - name: Cross-language perf benchmark
    run: cargo bench --bench throughput
  - name: Regression check
    run: ./scripts/check_perf_regression.sh
  ```

- Output JSON em `~/.claude/rust/docs/perf-baseline.json` (commitado para tracking histórico)

**Files**:
- `touring/benches/throughput.rs` — NEW (~250 LOC)
- `touring/benches/incremental.rs` — NEW (~80 LOC)
- `scripts/check_perf_regression.sh` — NEW (~50 LOC)
- `docs/perf-baseline.json` — NEW (committed baseline)

**New deps**: nenhuma (iai-callgrind já)

**Acceptance criteria**:
1. `cargo bench --bench throughput` corre em CI (Linux Valgrind)
2. Regression > 10% versus baseline falha CI
3. Benchmark report mostra Rust/TS/Python/Go throughput
4. 9 benchmark scenarios (3 langs × 3 ops: parse/extract/index)

**Tests added**: 9 benchmark scenarios (count differently from unit tests)

**Risk**: LOW — measurement-only

---

### TIER B (Thread) — Research investments (Wave 8 — gated, multi-quarter)

#### **D39 (T11) — AI-Native Knowledge Layer (Multi-Resolution MVKL)** [Size: XL+, multi-quarter]

**Why**: Thread Option C é state-of-the-art proposal. MVKL = Levels 0-2 (file index + parsed definitions + semantic graph). Touring poderia adotar como **North Star de longo prazo** (3-6 meses).

**Scope (in)** (se autorizado):
- L0 file index — Touring tem (file-knowledge extended) ✅
- L1 parsed definitions — Touring tem parcial (touring-ast::definitions); D31 (semantic classification) prepara ✅
- L2 semantic graph — Touring tem (touring-cognitive::semantic_graph); precisa formalizar como L2 ⚠️
- L3 architectural patterns — research spike (deferred per Thread spec) ❌
- L4 intent/specifications — research spike (deferred per Thread spec) ❌

**Implementation phases**:
- Phase 1 (~3 weeks): Formalizar L0+L1+L2 como MVKL trait
- Phase 2 (~2 weeks): MCP tools 3-tier suite (query L0, query L1, query L2)
- Phase 3 (research spike): L3+L4 viability assessment

**Files**:
- `touring-knowledge-layer/` (NEW crate) — multi-quarter
- ~2.500-4.000 LOC estimated

**Risk**: VERY HIGH — multi-quarter investment, paradigm shift

---

#### **D40 (T12) — Content-Addressed Definition Store (Unison-inspired)** [Size: XL, multi-wave]

**Why**: Thread Option B explora isto. Definitions hash-identified (SHA3-512 of AST), names = metadata pointers. Renaming não muda hash.

**Recomendação explícita: NÃO IMPLEMENTAR**.

**Justification**:
- Paradigm shift muito grande para Touring file-centric design
- Thread escolheu Option C (Multi-Resolution) sobre Option B exatamente porque B é mais transformativo
- Use case overlap pequeno: Gabriel edita files via Claude Code, não via graph editing
- Investment XL com ROI incerto

**Status**: DOCUMENTADO PARA COMPLETUDE, NÃO RECOMENDADO. Listed como D40 para completude do espelhamento T11 → D40 mas marcado WONTFIX.

---

#### **D41 (T13) — Code Graph Model integration (NeurIPS 2025)** [Size: XL, research spike]

**Why**: 512x context compression via graph attention masking. Mas requer LLM integration profunda — fora do escopo atual de Touring (que é code intelligence platform, não LLM client).

**Recomendação**: SPIKE de 1 semana para avaliar viabilidade. Se ROI positivo → submeter como Wave 9 separada. Se negativo → DOCUMENTADO E POSTERGADO indefinidamente.

**Spike outputs**:
- Feasibility report: pode Touring expor graph via attention-mask-friendly format?
- Cost estimate: que LLM clients consumiriam?
- Alternative: SCIP export (já planejado) é "good enough" para mesmo benefício?

**Files** (apenas spike):
- `~/.claude/rust/docs/spike-2026-XX-cgm-feasibility.md` — research report

**Risk**: research spike, não risco de implementation

---

## 3.7 Catálogo de deliverables CC Integration (D42-D49, 8 itens)

> Adicionados na revisão v4 (2026-05-01) após re-análise dos 3 repos sob lens "Claude Code integration". Estes 8 deliverables fecham gaps de UX específicos: zero-friction install, slash commands estruturados, Grep/Glob enrichment, plugin system per-project, multi-project registry. **Touring já tem CC integration madura** (33 hooks settings.json, 96 MCP tools, 5 subagents, skill auto-load, bidirectional task flow), mas falta paridade UX em quick-wins identificados em code-graph-cli (setup.rs com `include_str!`) e Thread (speckit slash commands com `handoffs:` frontmatter).

### TIER S — CC Integration quick wins

#### **D42 (CC1) — `touring init --cc-setup [--global] [--uninstall]` zero-friction installer** [Size: M, ~500 LOC]

**Why**: code-graph-cli tem `code-graph setup [--global]` que **embed hook scripts em compile-time** via `include_str!` e **automatically populates** `.claude/settings.json` + permission entries `"Bash(code-graph *)"`. Touring tem `update-touring` (build tool dual-target) mas não installer fresh para outros workspaces ou usuários. Mirror direto deste pattern.

**Scope (in)**:
- Estender `touring-server::cli::init` (já tem D16 `--profile`):
  ```rust
  pub fn cc_setup(global: bool, uninstall: bool, profile: Option<&str>) -> Result<()> {
      let base_dir = if global {
          home_dir().join(".claude")
      } else {
          PathBuf::from(".claude")
      };
      
      if uninstall { return cc_uninstall(&base_dir); }
      
      // 1. Write embedded hooks (include_str!)
      write_embedded_hooks(&base_dir.join("hooks"))?;
      // 2. Merge settings.json (33 hook entries + 4 permissions)
      merge_settings_json(&base_dir.join("settings.json"))?;
      // 3. Apply profile (D16: recommended/quickstart/airgapped/ci)
      if let Some(p) = profile { apply_profile(&base_dir, p)?; }
      // 4. Install MCP server registration (.mcp.json)
      register_mcp_server(&base_dir)?;
      // 5. Print summary + next steps
      print_install_summary(global, profile);
      Ok(())
  }
  ```

- `EMBEDDED_HOOKS` constante com hook scripts compile-time (mirror code-graph-cli):
  ```rust
  const EMBEDDED_HOOKS: &[(&str, &str)] = &[
      ("touring-pretool-bash.sh", include_str!("../hooks/touring-pretool-bash.sh")),
      ("touring-pretool-grep.sh", include_str!("../hooks/touring-pretool-grep.sh")),  // D43
      ("touring-pretool-glob.sh", include_str!("../hooks/touring-pretool-glob.sh")),  // D43
      ("touring-startup.sh", include_str!("../hooks/touring-startup.sh")),
  ];
  
  const PERMISSION_ENTRIES: &[&str] = &[
      "Bash(touring *)",
      "Bash(update-touring *)",
      "Bash(touring-bootstrap *)",
      "Bash(touring-mcp *)",
  ];
  ```

- `merge_settings_json` JSON-aware merge (não substituir, preservar entries existentes)
- `cc_uninstall` simétrico — remove TODOS hook entries Touring + permissions
- Pattern: `cargo install touring-cli && touring init --cc-setup --global` (1 comando)

**Files**:
- `touring-server/src/cli/init.rs` — extend (~200 LOC adicional)
- `touring-server/hooks/touring-pretool-{bash,grep,glob,startup}.sh` — NEW assets (~200 LOC total, embedded via `include_str!`)
- `touring-server/src/cli/settings_merger.rs` — NEW (~150 LOC, JSON-aware merge)

**New deps**: nenhuma (`serde_json` já)

**Acceptance criteria**:
1. `touring init --cc-setup --global` cria `~/.claude/hooks/touring-*.sh` + merge settings.json + adiciona 4 permissions
2. `touring init --cc-setup --uninstall` remove TUDO sem afetar entries de outros tools
3. Re-run idempotente: 2x install não duplica entries
4. JSON-aware merge preserva fields desconhecidos (forward-compat)
5. 14 unit tests + 4 integration

**Tests added**: 18

**Risk**: LOW — pattern proven em code-graph-cli

---

#### **D43 (CC2) — PreToolUse Grep/Glob enrichment hook** [Size: S, ~250 LOC] **✅ DELIVERED 2026-05-01**

> **Shipped**: `crates/touring-hooks/src/{pre_grep,pre_glob}.rs` + `tests/d43_pre_grep_glob_e2e.rs` (305 LOC + 39 tests). P99 latency = **2 ms** (vs spec 50 ms — 25× margin). Counters `pre_grep_enrichment_count` and `pre_grep_zero_results_count` exposed in `touring gate-metrics -j`. settings.json wired (matchers `Grep` + `Glob` coexisting with gitnexus-hook). Disable switch: `TOURING_DISABLE_PREGREP=1`. Hook registry bumped 184→186 / 182→184. Session report: `~/.claude/rust/docs/2026-05-01-d43-d45-daemon-idle-fix.md`.

**Why**: code-graph-cli intercepta Grep/Glob com pattern PascalCase/snake_case e auto-injeta `code-graph find` results. **Massive token saving**: CC vê symbol locations sem precisar Read posterior. Touring tem PreToolUse Read/Edit/Bash mas NÃO Grep/Glob. Adicionar é trivial sobre infra existente e elimina reads redundantes.

**Scope (in)**:
- 2 novos hooks no `touring-hook` binary:
  - `touring-hook pre-grep` — interceptado quando CC chama Grep
  - `touring-hook pre-glob` — interceptado quando CC chama Glob

- Algoritmo enrichment:
  1. Detect pattern PascalCase: `^[A-Z][a-zA-Z0-9_]+$` OR snake_case: `^[a-z][a-z0-9_]+$` OR camelCase: `^[a-z][a-zA-Z0-9]+$`
  2. Se symbol-like: `touring index find <pattern> -j --limit 20` em <50ms
  3. Se results: emite enrichment block:
     ```
     === Touring symbol enrichment ===
     Pattern '$PATTERN' resolved to <N> symbols:
       - file:line:col → kind (e.g., src/foo.rs:42:10 → fn)
     
     Suggestion: use 'touring find-code' for direct symbol queries (saves Read).
     === End enrichment ===
     ```
  4. Se 0 results OR pattern não-symbol-like: pass-through (no-op)

- Whitelist guard: pattern length 3-50 chars (evita matches genéricos como "x" ou strings longas)
- Performance budget: 50ms timeout (fail-fast pass-through)

- `settings.json` entries (auto-adicionadas por D42):
  ```json
  {
    "matcher": "Grep",
    "hooks": [{
      "type": "command",
      "command": "$HOME/.claude/hooks/touring-hook pre-grep",
      "if": "Grep(*)",
      "timeout": 2,
      "statusMessage": "Touring: enriching symbol query..."
    }]
  },
  {
    "matcher": "Glob",
    "hooks": [{
      "type": "command",
      "command": "$HOME/.claude/hooks/touring-hook pre-glob",
      "if": "Glob(*)",
      "timeout": 2,
      "statusMessage": "Touring: enriching pattern..."
    }]
  }
  ```

**Files**:
- `touring-hooks/src/cli_handlers.rs` — adicionar `cli_pre_grep`, `cli_pre_glob` handlers (~200 LOC)
- `touring-server/src/cli/hook.rs` — register hooks (~30 LOC)
- `touring-server/hooks/touring-pretool-grep.sh` — NEW (~30 LOC, dispatcher to binary)
- `touring-server/hooks/touring-pretool-glob.sh` — NEW (~30 LOC)

**New deps**: nenhuma

**Acceptance criteria**:
1. CC Grep com pattern `HookRuntime` (PascalCase) emite enrichment com locations
2. CC Grep com pattern `the quick brown fox` (free text, > 50 chars) → pass-through silencioso
3. CC Grep com pattern existente em 0 symbols → pass-through silencioso (no false positive)
4. P99 latency < 50ms para enrichment
5. 10 unit tests (5 patterns matched + 5 não-matched) + 4 integration (com tantivy real)

**Tests added**: 14

**Risk**: MEDIUM (probabilidade 0.4 / impacto LOW) — enrichment muito barulhento se threshold mal-calibrado. **Mitigação**: 
- Whitelist conservadora (length 3-50, regex strict)
- Pass-through em qualquer erro/timeout (fail-open)
- Métrica `pre_grep_enrichment_count` em gate-metrics para Gabriel ajustar

---

#### **D44 (CC3) — Speckit-style slash command suite (10 commands)** [Size: M, ~500 LOC + 11 .md files]

**Why**: Thread tem 10 speckit commands estruturados (`speckit.{analyze, checklist, clarify, constitution, implement, plan, specify, tasks, taskstoissues}`) com `handoffs:` frontmatter sugerindo próximo phase. Touring tem 1 (`TACO-task.md`) + master `/Touring`. Disaggregação por TACO phase = UX-first agent flow + handoff system reduz friction de orchestration manual.

**Scope (in)**:
- 11 slash commands em `~/.claude/commands/`:
  - `touring.health.md` — FASE 0 (health gate)
  - `touring.scout.md` — FASE 1 (scout via touring-scouter)
  - `touring.architect.md` — FASE 2 (design via touring-architect)
  - `touring.context7.md` — FASE 3 (Context7 query)
  - `touring.decompose.md` — FASE 4 (sequential-thinking + DAG)
  - `touring.audit-pre.md` — FASE 4.5 (anti-FP gate)
  - `touring.implement.md` — FASE 5 (touring-engineer)
  - `touring.audit-post.md` — FASE 6 (cross-audit)
  - `touring.scribe.md` — FASE 7 (scriber + memory)
  - `touring.find.md` — quick search (D26 super-tool quando shippado)
  - `touring.flow.md` — D29 FlowBuilder shortcut

- Cada arquivo segue Thread speckit pattern (D49 frontmatter):
  ```yaml
  ---
  description: Run TACO Phase 1 — Scout the codebase area before implementation.
  handoffs:
    - label: Architect Design
      agent: touring.architect
      prompt: Design the implementation based on scout findings.
    - label: Decompose into subtasks
      agent: touring.decompose
      prompt: Decompose into validated DAG.
      send: true
  ---
  
  ## User Input
  
  ```text
  $ARGUMENTS
  ```
  
  ## Outline
  
  Run TACO FASE 1 (Scout) on $ARGUMENTS:
  1. Verify daemon health: `touring doctor -j` ALL OK
  2. Invoke touring-scouter agent with VP-Scout cadeias 1-7 obrigatórias
  3. ...
  ```

- D49 wire handoff system (subset desta deliverable, frontmatter já incluído)
- Backend: cada command pode invocar Agent tool com `subagent_type=touring-<role>` ou Bash com `touring <subcommand>`

**Files**:
- `~/.claude/commands/touring.{health,scout,architect,context7,decompose,audit-pre,implement,audit-post,scribe,find,flow}.md` — NEW (11 arquivos, ~30-60 LOC each)
- Documentação no SKILL.md update (D44 + D49)
- Optional: `touring init --cc-setup` (D42) também installa esses comandos
- Optional: helper script `~/.claude/scripts/touring-cmd-handoff.sh` (~50 LOC) para parsing handoff frontmatter

**New deps**: nenhuma

**Acceptance criteria**:
1. `/touring.scout <area>` invoca touring-scouter com VP-Scout
2. CC mostra handoff buttons "[Architect Design]" e "[Decompose into subtasks]" após output
3. Click em handoff invoca próximo command com prompt pré-formado
4. 11 commands cada com 1 fixture test = 11 tests + 1 integration (full chain scout → architect → decompose)

**Tests added**: 12

**Risk**: MEDIUM (probabilidade 0.3 / impacto LOW) — `handoffs:` frontmatter pode não ser feature CC nativa. **Mitigação**: SPECULATION 0.6 — verificar Anthropic docs antes de implementar; se CC não suporta, fallback é commands sem handoffs (perda parcial de UX, ainda útil).

---

#### **D45 (CC4) — `Bash(touring *)` permission auto-add (sub-task de D42)** [Size: XS, ~30 LOC] **✅ DELIVERED 2026-05-01**

> **Shipped**: 4 entries in `~/.claude/settings.json::permissions.allow` (`Bash(touring *)`, `Bash(update-touring *)`, `Bash(touring-bootstrap *)`, `Bash(touring-mcp *)`). Idempotent merge via Python script (verified by re-run — would-add count = 0). No more approval prompts for `touring` invocations. Session report: `~/.claude/rust/docs/2026-05-01-d43-d45-daemon-idle-fix.md`.

**Why**: code-graph-cli adiciona `"Bash(code-graph *)"` em `permissions.allow` automaticamente. Touring tem hooks Bash mas não permission entries — Gabriel recebe approval prompts em todo `touring` invocation. Auto-add reduz friction.

**Scope (in)**:
- D42 installer adds 4 entries:
  ```json
  "permissions": {
    "allow": [
      "Bash(touring *)",
      "Bash(update-touring *)",
      "Bash(touring-bootstrap *)",
      "Bash(touring-mcp *)"
    ]
  }
  ```

**Files**: subset de D42 (sem arquivos novos, ~30 LOC em `merge_settings_json`)

**Acceptance criteria**:
1. Após `touring init --cc-setup`, 4 permission entries presentes em `permissions.allow`
2. Re-install não duplica
3. 3 unit tests (add, idempotency, preserve other allow entries)

**Tests added**: 3

**Risk**: LOW

---

#### **D49 (CC11) — Slash command handoff frontmatter system (sub-task de D44)** [Size: XS, ~50 LOC]

**Why**: Thread `handoffs:` frontmatter pattern em commands cria UX flow contínuo (CC mostra botões para próximo phase). Crítico para D44 ser efetivo.

**Scope (in)**:
- Design de YAML frontmatter padrão para 11 commands D44
- Cada command lista 1-3 handoffs sugeridos:
  - `touring.scout` → `touring.architect` ou `touring.decompose`
  - `touring.architect` → `touring.context7` ou `touring.decompose`
  - `touring.context7` → `touring.decompose`
  - `touring.decompose` → `touring.audit-pre`
  - `touring.audit-pre` → `touring.implement` (se PASS) ou stop (se FAIL)
  - `touring.implement` → `touring.audit-post`
  - `touring.audit-post` → `touring.scribe`
  - `touring.scribe` → end (back to user)
  - `touring.health` → `touring.scout` (se OK) ou stop
  - `touring.find` → `touring.scout` (se quer expandir contexto)
  - `touring.flow` → end ou `touring.audit-post`

**Files**: subset de D44 — apenas YAML em cada `.md` file

**Acceptance criteria**:
1. CC renderiza handoff buttons após cada command output
2. Click invoca próximo command com prompt pré-formado
3. Documented em SKILL.md como flow diagram

**Tests added**: 4 (hand-validate em 4 chains representativas)

**Risk**: gated por feature CC (mesma de D44)

---

### TIER A — Investimentos estruturais

#### **D47 (CC6) — Multi-project registry** [Size: M, ~400 LOC]

**Why**: code-graph-cli tem `code-graph project add <alias> <path>` permitindo cross-project queries com `--project <alias>`. Touring é per-project actor (cada workspace tem seu daemon). Multi-project registry permite Gabriel trocar entre `~/.claude/rust/` e outros workspaces sem switching de sessão.

**Scope (in)**:
- Novo `touring-server::projects`:
  ```rust
  pub struct ProjectRegistry {
      pub projects: HashMap<String /* alias */, ProjectEntry>,
  }
  
  pub struct ProjectEntry {
      pub alias: String,
      pub path: PathBuf,
      pub daemon_socket: PathBuf,
      pub last_used: i64,
      pub default: bool,
  }
  ```
- Storage: `~/.claude/touring/projects.json`
- CLI:
  - `touring project add <alias> <path> [--default]`
  - `touring project list [-j]`
  - `touring project remove <alias>`
  - `touring project current` (resolve via cwd)
  - Suporte a `--project <alias>` em comandos read-only (find, blast, wiring, search)
- Per-project daemon socket isolation (já parcial — daemon é per-project actor; documentar)
- D26 (`find_code` super-tool) ganha `--project <alias>` opcional

**Files**:
- `touring-server/src/projects/mod.rs` — NEW (~200 LOC registry)
- `touring-server/src/projects/registry.rs` — NEW (~100 LOC persistence)
- `touring-server/src/cli/project.rs` — NEW (~80 LOC)
- `touring-hooks/src/cli_handlers.rs` — `cli_project_*` handlers — blast=4

**New deps**: nenhuma

**Acceptance criteria**:
1. `touring project add main-api ~/projects/main-api` cria entry
2. `touring find_code "auth" --project main-api` query no daemon do main-api
3. `touring project list` mostra alias + path + last_used
4. 12 unit tests + 4 integration (Multi-project workflows)

**Tests added**: 16

**Risk**: LOW

---

### TIER B — Optional/gated (Wave 8)

#### **D46 (CC5) — `.claude/plugins/touring/` plugin system per-project** [Size: L, ~800 LOC]

**Why**: Thread tem `.claude/plugins/ctx/` plugin system per-project. Permite Touring custom commands/agents/rules per-workspace. Apenas necessário se Touring escalar além de single-user.

**Scope (in)** (se autorizado):
- `<workspace>/.claude/plugins/touring/.claude-plugin/plugin.json` metadata
- `<workspace>/.claude/plugins/touring/agents/*.md` per-project subagents
- `<workspace>/.claude/plugins/touring/rules/*.yaml` per-project D30 rules
- `<workspace>/.claude/plugins/touring/commands/*.md` per-project slash commands

**Risk**: HIGH (escopo creep). **Postergado** Wave 8 — gated por demand validation

---

#### **D48 (CC7) — Multi-agent compatibility files (.specify/.serena/.jules/.gemini mirrors)** [Size: XS, ~80 LOC]

**Why**: Thread tem `.specify/.serena/.jules/.gemini` directories com agents mirrored. Para teams que usam Gemini/Specify além de CC. **LOW priority** — apenas se Gabriel adopt outros agents.

**Scope (in)** (se autorizado):
- D42 installer com flag `--multi-agent` espelha `~/.claude/agents/touring-*.md` para outros directories
- Subset focused: TOURING-task.md mirror em `.gemini/commands/touring.toml`

**Risk**: LOW. **Postergado** Wave 8 — gated by Gabriel adopting Gemini/Specify

</deliverables>

---

<timeline>

## 4. Timeline com sequenciamento e dependências

### 4.1 Dependency graph completo (49 deliverables, acyclic verification — v4)

**Adicionado v4 (D42-D49 CC integration)**:

```
WAVE 2 (Search Foundation) — adicionado D43:
  D43 (PreToolUse Grep/Glob enrichment) ──► uses tantivy + index find (existentes); zero deps externas

WAVE 7 (Polish & DI Runtime + Extensibility) — adicionados D42, D44, D45, D47, D49:
  D42 (touring init --cc-setup installer) ──┬─► uses D16 (profiles)
                                            ├─► includes D43 hook scripts (embedded)
                                            └─► auto-add D45 (permissions)
  D44 (Speckit slash commands suite) ──► includes D49 (handoffs frontmatter)
  D45 (Bash permission auto-add) ── sub-task de D42
  D47 (Multi-project registry) ──► consumido por D26 (find_code --project)
  D49 (Handoff frontmatter system) ── sub-task de D44

WAVE 8 (Optional, gated) — adicionados D46, D48:
  D46 (.claude/plugins/touring/) ──► uses D30 (YAML rules) + D44 (commands) per-project
  D48 (Multi-agent .specify/.serena/.jules mirrors) ── independent ── only if Gabriel uses
```

### 4.1.1 Dependency graph v3 (41 deliverables — preservado abaixo)

```
WAVE 1 (Visual Foundation):
  D1 (graph --format) ──┬─► D2 (--max-nodes/--reduce)
                        ├─► D3 (touring viz)
                        ├─► D6 (graph flow)
                        ├─► D8 (snapshot diff visualization)
                        ├─► D9 (clones output)
                        └─► D29 (TouringFlowBuilder targets DOT/Mermaid)

WAVE 2 (Rich Encoding & Search Foundation):
  D5 (confidence tiers) ── independent ── parallel to all
  D4 (RRF search) ──┬─► D13 (intent classification — boost on top of RRF)
                    └─► D24 (hybrid scoring uses RRF)
  D14 (GracefulChunker) ── independent ── prep para D22 (embedding processing)
  D15 (ResourceGovernor) ── independent ── usado por todos os subsequentes
  D16 (touring init --profile) ── independent ── pure UX

WAVE 3 (Capability Parity + Overlay):
  D7 (rename --plan) ──► uses D1 + D5 + D17 (move detection accelerates rename)
  D17 (Move detection) ── independent ── prep para Wave 5
  D37 (Overlay Graph) ──► uses D8 (snapshot é base layer); evolução natural

WAVE 4 (Resilience Patterns + Semantic Foundation):
  D18 (CheckpointSettingsFingerprint) ──► D25 (asymmetric embeddings uses fingerprint)
  D19 (FailoverService cross-subsystem) ──► D23 (vector store backend failover)
                                         └─► D33 (conflict tier metrics integration)
  D20 (rignore audit) ── independent
  D21 (node types KB) ──┬─► D13 (intent classification ranking boost)
                        └─► D31 (semantic classification — node-type lookup)
  D31 (Semantic classification 99.7%) ──┬─► D13 boost
                                        ├─► D24 hybrid scoring chunk weighting
                                        └─► D29 FlowBuilder extract step
  D33 (Conflict tier SLAs) ──► uses D19 (failover) + D37 (overlay) + D38 (perf)

WAVE 5 (Hybrid Semantic Search) — CRITICAL CHAIN:
  D22 (Embedding provider) ──► D23 (Vector store) ──► D24 (Hybrid scoring) ──► D25 (Manifest integration)
  D22 ──► também consumido por D26 super-tool

WAVE 6 (Agent UX):
  D26 (find_code super-tool) ──► uses D13 (intent), D24 (hybrid scoring), Wave 5 stack
  D26 fallback graceful: usa D4 (RRF only) se Wave 5 not deployed

WAVE 7 (Polish & DI Runtime + Extensibility):
  D27 (Plugin DI) ──► consumes D22, D23, D24 providers
  D28 (MCP overhead) ── independent
  D29 (TouringFlowBuilder) ──► uses D1+D8+D9+D31 (multiple extract steps)
  D30 (YAML rule engine + fix) ──► extends touring-assists; uses speculative validation
  D32 (Tier-based language UX) ── independent ── pure UX
  D38 (Cross-language perf benchmarks) ──► uses D32 (tier matrix), validates D33 SLAs

WAVE 8 (Optional/Research, gated):
  D10 (Web UI) ──► uses D1+D2+D3 (server-side rendering)
  D11 (FDEB)   ──► uses D1+D3 (replaces edge geometry)
  D12 (DSL)    ──► uses D1+D3+D21 (query → DOT, leverages node types KB)
  D34 (Postgres backend) ── independent ── feature flag opt-in
  D35 (Cloudflare Workers Edge) ──► uses D34 (D1 backend) — extends multi-deployment
  D36 (Bidirectional file↔graph sync) ──► uses D37 (overlay) — paradigm shift
  D39 (AI-Native Knowledge Layer MVKL) ──► uses D31+D37 — multi-quarter
  D40 (Content-Addressed Definition Store, Unison) — DOCUMENTED, NOT RECOMMENDED
  D41 (Code Graph Model NeurIPS 2025) — research spike only
```

**Verification**: graph é DAG (acyclic). Topological sort completo (v3, 41 deliverables):
```
Tier 0 (independent / leaf-deps):
[D1, D5, D4, D14, D15, D16, D17, D20, D21, D28, D32, D34]

Tier 1 (depend only on Wave 1-2):
[D2, D3, D6, D8, D9, D13, D18, D29, D31, D38]

Tier 2 (depend on Waves 1-4):
[D7, D19, D22, D33, D37]

Tier 3 (depend on Wave 4 stack):
[D23] (depends D22)
[D24] (depends D22+D23)
[D25] (depends D24+D18)
[D26] (depends D24)
[D27, D30] (depends D22+D23+D24 / D29 + assists)

Tier 4 (optional, gated):
[D10, D11, D12, D35, D36, D39, D40, D41]
```

**Critical path** (longest dependency chain — v3):
- Core: `D1 → D4 → D22 → D23 → D24 → D25 → D26 → D27` = 8 hops (~4-6 weeks core)
- Knowledge layer (longer): `D1 → D8 → D37 → D31 → D24 → D26 → D29 → D39` = 8 hops via knowledge layer (research spike)
- Extensibility: `D1 → D8 → D29 → D30` = 4 hops (Wave 7)

### 4.2 Wave plan (8 sequential waves)

#### **WAVE 1 — Visual Foundation** (Target: 2026-05-04 → 2026-05-07, 3-4 dias)

| Phase | Agent | Task |
|---|---|---|
| FASE 0 | orchestrator | `update-touring` rebuild (daemon healthy required); `cargo check --workspace`; `touring doctor -j` ALL OK |
| FASE 1 | touring-scouter | Scout sobre `touring-server/src/cli/graph.rs` + `touring-hooks::cli_handlers` para localizar 4 graph daemon handlers exatos. VP-Scout cadeias 1-7 obrigatórias. |
| FASE 2 | touring-architect | Design `GraphData` schema unificado + serializers DOT/Mermaid + flow de dados handler→serializer |
| FASE 3 | orchestrator | Context7 query: graphviz DOT spec, Mermaid flowchart spec |
| FASE 4 | sequential-thinking | Decompose D1+D2 em 8-10 subtasks (D1.1-D1.5, D2.1-D2.3) |
| FASE 4.5 | touring-auditor | Pre-impl audit: confirma símbolos via `touring index find`; reject FPs |
| FASE 5 | 2 engineers parallel | E1 = D1 (graph.rs + visual/{dot,mermaid,types}.rs); E2 = D2 (cap.rs + tred.rs); paralelismo por arquivo disjoint |
| FASE 6 | touring-auditor + code-reviewer parallel | Cross-audit; verify `cargo test -p touring-server` PASS |
| FASE 7 | touring-scriber | Update SKILL.md + new ref `touring-cli-viz.md` + changelog v4.25.0 entry |

**Deliverables Wave 1**: D1 (M) + D2 (S). Total ~600 LOC. 23 new tests.

**Validation gate**: `touring graph communities --format dot | dot -Tsvg > /tmp/test.svg && [ -s /tmp/test.svg ]` exit 0.

---

#### **WAVE 2 — Rich Encoding & Search** (Target: 2026-05-08 → 2026-05-13, 4-5 dias)

| Phase | Agent | Task |
|---|---|---|
| FASE 0 | orchestrator | Rebuild + doctor + cargo check |
| FASE 1 | touring-scouter | Scout `touring-tantivy` para identificar trait/struct para extender com RRF; identificar config loader pattern (touring.toml or similar) |
| FASE 2 | touring-architect | Design encoding theme TOML schema + RRF score function signature + tier struct |
| FASE 3 | Context7 | Mermaid clusters spec, DOT cluster syntax, BM25/RRF papers |
| FASE 4 | sequential-thinking | Decompose D3+D4+D5 em ~12 subtasks |
| FASE 4.5 | touring-auditor | Pre-impl: tem `toml` crate? Tem cache de tantivy results? |
| FASE 5 | 3 engineers parallel | E1 = D3 (viz.rs + encoding/theme/layout); E2 = D4 (search.rs + RRF); E3 = D5 (config + tiers) |
| FASE 6 | auditor + code-reviewer | Cross-audit |
| FASE 7 | scriber | Document theme TOML, RRF formula, tier semantics |

**Deliverables Wave 2 (AMPLIFICADA v4)**: D3 (L) + D4 (S) + D5 (XS) + D13 (M, intent) + D14 (S, GracefulChunker) + D15 (S, ResourceGovernor) + D16 (XS, profiles) + **D43 (S, Grep/Glob enrichment — NEW v4)**. Total ~2.430 LOC (v3: 2.180 / v2: 1.100 / v1: ~600 implícito). 98 new tests (v3: 84 / v2: 30).

**Adição v4**: E5 = D43 (PreToolUse Grep/Glob enrichment) — paralelo aos 4 engineers de v3. Massive token saving para sessões CC que fazem Grep/Glob frequente — pareamento natural com D4 (RRF) que já consome tantivy.

**Estratégia de execução (4 engineers parallel + 1 sequential)**:
- E1 = D3 (viz.rs + encoding/theme/layout) — 800 LOC
- E2 = D4 + D13 (search.rs + RRF + intent classification) — 600 LOC
- E3 = D5 + D16 (config + tiers + profiles) — 180 LOC
- E4 = D14 + D15 (GracefulChunker + ResourceGovernor refactor) — 400 LOC
- Sequential post-merge: D13 boost wired sobre D4 RRF (cannot parallelize)

**Validation gate Wave 2**:
```bash
# Test 1: encoding
touring viz workspace --format svg --output /tmp/ws.svg && grep -q "fill=\"#a5d6a7\"" /tmp/ws.svg

# Test 2: RRF + intent
touring search unified "how does authentication work" --format json | jq '.intent_detected'  # → "Understand"

# Test 3: GracefulChunker fallback
echo -e "\xff\xfe\x00binary" > /tmp/binary && touring ast meta /tmp/binary --depth summary -j  # → no panic, BinaryFileError handled

# Test 4: ResourceGovernor enforcement
TOURING_CHUNKER_TIMEOUT=100ms touring ast chunk <huge-file>  # → ChunkingTimeoutError

# Test 5: profiles
touring init --profile quickstart --output /tmp/ws-test && [ -f /tmp/ws-test/touring.toml ]
```

---

#### **WAVE 3 — Capability Parity** (Target: 2026-05-14 → 2026-05-22, 6-8 dias)

Wave 3 é maior; subdividida em 3.A e 3.B.

##### **WAVE 3.A** (D6 + D9): flow + clones — 2-3 dias

| Phase | Agent | Task |
|---|---|---|
| FASE 0 | orchestrator | Rebuild + doctor + cargo check |
| FASE 1 | touring-scouter | Identificar canonical call_graph (3 candidatos: cortex, ast, cognitive); confirmar petgraph::algo::all_simple_paths está disponível |
| FASE 2 | touring-architect | Schema for flow paths output, clone signature schema |
| FASE 4 | sequential-thinking | Decompose em 6-8 subtasks |
| FASE 4.5 | touring-auditor | Pre-impl audit |
| FASE 5 | 2 engineers parallel | E1 = D6 (flow.rs); E2 = D9 (clones.rs + signature.rs) |
| FASE 6 | auditor | Cross-audit |
| FASE 7 | scriber | Update SKILL.md |

##### **WAVE 3.B** (D7 + D8): rename plan + snapshot — 4-5 dias

| Phase | Agent | Task |
|---|---|---|
| FASE 0 | orchestrator | Health gate |
| FASE 1 | touring-scouter | Localizar `touring shadow validate` API; confirmar bincode no workspace |
| FASE 2 | touring-architect | Design Plan struct + apply pipeline + snapshot store schema |
| FASE 3 | Context7 | rust refactoring patterns, graph diff algorithms |
| FASE 4 | sequential-thinking | Decompose em ~10 subtasks (D7 é mais arriscado, separa apply de plan) |
| FASE 4.5 | touring-auditor | Pre-impl audit MUITO atento — D7 é maior risco |
| FASE 5 | 2 engineers parallel | E1 = D7 (rename.rs); E2 = D8 (snapshot/{store,diff}.rs) |
| FASE 6 | auditor + code-reviewer + extra security audit on D7's `--apply` path | Cross-audit reforçado |
| FASE 7 | scriber | Document apply guards, hash-confirm protocol |

**Deliverables Wave 3 (AMPLIFICADA v3)**: D6 (S) + D7 (M) + D8 (M) + D9 (M) + D17 (S, move detection) + **D37 (L, Overlay Graph — Wave 3.C NEW v3)**. Total ~3.150 LOC (v2: 1.950 / v1: 1.750). 80 new tests (v2: 54 / v1: 44).

**Estratégia v3 — 3 sub-waves**:
- **Wave 3.A** (D6+D9+D17): flow + clones + move detection — 2-3 dias, 3 engineers paralelos
- **Wave 3.B** (D7+D8): rename plan + snapshot — 4-5 dias, 2 engineers paralelos
- **Wave 3.C** (D37 NEW v3): Overlay Graph (Base+Delta+Unified) — 4-5 dias, 1 engineer + integração D8 e D33

##### **WAVE 3.C** (D37): Overlay Graph (Base + Delta + Unified) — 4-5 dias

| Phase | Agent | Task |
|---|---|---|
| FASE 0 | orchestrator | Health gate; verify D8 (snapshot) merged |
| FASE 1 | touring-scouter | Scout snapshot store; identify base layer entry points |
| FASE 2 | touring-architect | Design `OverlayGraph { base, delta }` + UnifiedView merge semantics + promote/discard transitions |
| FASE 3 | Context7 | Thread overlay spec (FR-017) docs |
| FASE 4 | sequential-thinking | Decompose D37 em 6 subtasks (base, delta, unified, promote, discard, diff) |
| FASE 4.5 | touring-auditor | Pre-impl audit: confirma D8 stable; conflict detection plan with D33 (Wave 4) |
| FASE 5 | 1 engineer | E1 = D37 (overlay/{mod,base,delta,unified}.rs + cli/overlay.rs) |
| FASE 6 | auditor | Cross-audit; verify D8 backward compat |
| FASE 7 | scriber | Document overlay state machine + promote/discard semantics |

**Validation gate Wave 3**:
```bash
# Test 1: flow paths
touring graph flow main run --format json | jq '.paths | length' | { read N; [ "$N" -ge 1 ] || exit 1; }

# Test 2: rename plan
touring graph rename HookRuntime --new HookEngine --plan --format json | jq '.edits | length'

# Test 3: snapshot diff
touring graph snapshot create base && touring graph snapshot diff base HEAD --format json

# Test 4: clones
touring graph clones --min-group 2 --format json | jq '.groups | length'

# Test 5: move detection (NEW)
mv src/foo.rs src/bar.rs
touring vfs sync --json | jq '.moves[] | select(.from == "src/foo.rs" and .to == "src/bar.rs")'
# Should return 1 move event without re-chunking
```

---

#### **WAVE 4 — Resilience Patterns** (NEW — Target: 2026-05-23 → 2026-06-01, 6-7 dias)

| Phase | Agent | Task |
|---|---|---|
| FASE 0 | orchestrator | Health gate + sequential-thinking ToolSearch |
| FASE 1 | touring-scouter | Scout `touring-core::checkpoint` existing structure; circuit breaker per-class/per-project; `touring-vfs` filtering capabilities (rignore parity audit); tree-sitter language list |
| FASE 2 | touring-architect | Design `CheckpointSettingsFingerprint` + `Failover<P,B>` trait + node-types JSON schema |
| FASE 3 | Context7 | rignore docs, tree-sitter node types reference, asymmetric embeddings papers |
| FASE 4 | sequential-thinking | Decompose D18+D19+D20+D21 em ~14 subtasks |
| FASE 4.5 | touring-auditor | Pre-impl audit: confirma `IndexingCheckpoint::matches_settings()` API existe; audit existing circuit breaker for failover trait migration |
| FASE 5 | 4 engineers parallel | E1 = D18 (fingerprint); E2 = D19 (failover trait + 3 default impls); E3 = D20 (rignore audit + gap fix); E4 = D21 (node types KB JSON gen + cmd) |
| FASE 6 | auditor + code-reviewer | Cross-audit; verify backward compat dos checkpoints existentes |
| FASE 7 | scriber | Document family-aware checkpoint semantics, failover state machine, node types format |

**Deliverables Wave 4 (AMPLIFICADA v3)**: D18 (M) + D19 (M) + D20 (XS) + D21 (S) + **D31 (M, semantic classification 99.7%) + D33 (S, conflict tier SLAs)**. Total ~2.000 LOC (v2: 1.250). 132 new tests (v2: 64).

**Estratégia v3** (6 engineers paralelos):
- E1 = D18 (CheckpointSettingsFingerprint family-aware)
- E2 = D19 (FailoverService trait + 3 default impls)
- E3 = D20 (rignore audit + gap fix)
- E4 = D21 (node types KB JSON gen + cmd)
- E5 = **D31 (touring-definitions crate — port CodeWeaver/Thread spec)** — NEW v3
- E6 = **D33 (multi-tier conflict detection com SLAs)** — NEW v3

D21 e D31 têm pareamento natural (node-types JSON é input do classifier). D33 integra com D19 (failover) e prepara D38 (perf benchmarks Wave 7).

**Validation gate Wave 4**:
```bash
# Test 1: family-aware compatibility
touring session start fam-test type "test"
touring config set chunker.tree_sitter_version 0.20.10  # minor bump same family
touring session start fam-test type "test"  # should NOT trigger reindex

# Test 2: failover service
touring failover status -j  # → {primary_active: true, backup_ready: true}
sudo lsof -ti:6669 | xargs kill -9  # simulate primary failure (tantivy port)
sleep 5
touring failover status -j  # → {primary_active: false, backup_active: true}

# Test 3: rignore parity
echo "secret.txt" >> .gitignore
touring file-knowledge extended secret.txt 2>&1 | grep -q "ignored"

# Test 4: node types KB
touring ast node-types rust -j | jq '.node_types | length'  # ≥ 100
touring ast importance src/main.rs --threshold 0.7 -j | jq '.high_importance_nodes'
```

---

#### **WAVE 5 — Hybrid Semantic Search** (NEW CRITICAL — Target: 2026-06-02 → 2026-06-22, 14-18 dias multi-sub-wave)

> **WAVE ESTRATÉGICA**: maior investimento do plano. Subdividida em 4 sub-waves (5.1, 5.2, 5.3, 5.4) sequenciais (não paralelizáveis pois D23 depende D22, etc.).

##### **WAVE 5.1** — D22 (Embedding Provider Abstraction) — 4-5 dias

| Phase | Agent | Task |
|---|---|---|
| FASE 0 | orchestrator | Health gate; verificar Candle deps disponibilidade no workspace |
| FASE 1 | touring-scouter | Scout existing embeddings (cognitive crate?); confirmar dimensões/family conventions |
| FASE 2 | touring-architect | Design `EmbeddingProvider` trait + `ModelFamily` + 3 backends (Candle, FastEmbed, Voyage) |
| FASE 3 | Context7 | Candle 0.7 docs, BGE model architecture, Voyage Code-3 spec |
| FASE 4 | sequential-thinking | Decompose D22 em 6 subtasks (trait, error, family, candle backend, fastembed backend, voyage backend) |
| FASE 4.5 | touring-auditor | Pre-impl audit: workspace size impact (Candle ~30MB), CI matrix viability |
| FASE 5 | 1 engineer + 2 sub-engineers | E1 main = trait + error + family; E1.1 = Candle backend; E1.2 = FastEmbed + Voyage backends |
| FASE 6 | auditor + perf-engineer | Benchmark: Candle BGE-small em CPU < 50ms |
| FASE 7 | scriber | Document provider matrix + airgapped guide |

##### **WAVE 5.2** — D23 (Vector Store Abstraction) — 4-5 dias

| Phase | Agent | Task |
|---|---|---|
| FASE 0-1 | orchestrator + scouter | Verificar sqlite-vec compilation viability; Qdrant client API |
| FASE 2 | architect | Design `VectorStore` trait + 3 backends (sqlite-vec, Qdrant, InMemory) |
| FASE 3 | Context7 | sqlite-vec docs, Qdrant Rust client docs |
| FASE 5 | 1 engineer + 3 sub-engineers parallel | E1 main = trait + query types; E1.1 = sqlite-vec; E1.2 = Qdrant; E1.3 = InMemory |
| FASE 6 | auditor + perf-engineer | Benchmark: 10k vectors upsert/search < 100ms em sqlite-vec |
| FASE 7 | scriber | Document backend selection guide |

##### **WAVE 5.3** — D24 (Hybrid Scoring + Reranking) — 3-4 dias

| Phase | Agent | Task |
|---|---|---|
| FASE 0-1 | orchestrator + scouter | Verificar D4 RRF integration points |
| FASE 2 | architect | Design hybrid scoring pipeline + Reranker trait + cascade |
| FASE 5 | 2 engineers parallel | E1 = hybrid weights + RRF; E2 = Reranker trait + cascade + MVP no-op impl |
| FASE 6 | auditor | Cross-audit; verify D13 intent boost wired post-rerank |
| FASE 7 | scriber | Document scoring formula + tuning guide |

##### **WAVE 5.4** — D25 (Manifest + Checkpoint Embedding Integration) — 3-4 dias

| Phase | Agent | Task |
|---|---|---|
| FASE 0-1 | orchestrator + scouter | Confirmar D17 (move detection) + D18 (fingerprint) merged |
| FASE 2 | architect | Design FileManifestEntry extension + asymmetric logic |
| FASE 5 | 1 engineer | E1 = manifest extend + checkpoint wire + session display |
| FASE 6 | auditor | Cross-audit fingerprint compatibility logic |
| FASE 7 | scriber | Document asymmetric embeddings flow + migration guide |

**Deliverables Wave 5**: D22 (M) + D23 (L) + D24 (M) + D25 (M). Total ~3.000 LOC. 200 new tests.

**Validation gate Wave 5**:
```bash
# Test 1: embedding provider
touring embeddings test-provider candle bge-small "fn foo() {}" -j | jq '.dimensions'  # → 384

# Test 2: vector store (sqlite-vec local)
touring vector-store status -j | jq '.backend'  # → "sqlite-vec"
touring index --rebuild-with-embeddings  # full reindex with vectors
touring search semantic "authentication flow" -j | jq '.results | length'  # ≥ 1

# Test 3: hybrid scoring
touring search unified "where do we validate JWT" --hybrid -j | jq '.strategy_used'  # → "hybrid"
touring search unified "where do we validate JWT" --hybrid -j | jq '.results[0].score_breakdown'  # → {dense, sparse, hybrid, reranked}

# Test 4: asymmetric embeddings
touring config set embeddings.dense.model bge-base-v1.5
touring session start asym-test type "test"  # should NOT reembed if family matches
touring config set embeddings.dense.model voyage-code-3  # different family
touring session start asym-test2 type "test"  # SHOULD trigger reembed warning
```

---

#### **WAVE 6 — Agent UX (find_code super-tool)** (NEW — Target: 2026-06-23 → 2026-06-28, 4-5 dias)

| Phase | Agent | Task |
|---|---|---|
| FASE 0 | orchestrator | Health gate + verify Wave 5 deployed (or graceful degradation strategy) |
| FASE 1 | touring-scouter | Scout MCP tools registry; identify token counting strategy |
| FASE 2 | touring-architect | Design `FindCodeParams`, `FindCodeResponse`, pipeline orchestration |
| FASE 3 | Context7 | MCP protocol, FastMCP best practices |
| FASE 4 | sequential-thinking | Decompose D26 em 6 subtasks (CLI, MCP tool, pipeline, format, fallback, tests) |
| FASE 4.5 | touring-auditor | Pre-impl: confirma Waves 1-5 deliverables disponíveis OR fallback path tested |
| FASE 5 | 2 engineers parallel | E1 = `cli/find_code.rs` + pipeline orchestration; E2 = MCP tool registration + token counting + format compact |
| FASE 6 | auditor + perf-engineer | Cross-audit; benchmark in 3 different projects |
| FASE 7 | scriber | Document agent UX philosophy, integration with Claude Code, examples |

**Deliverables Wave 6**: D26 (M). Total ~600 LOC. 22 new tests.

**Validation gate Wave 6**:
```bash
# Test 1: super-tool MCP
echo '{"method": "tools/call", "params": {"name": "touring_find_code", "arguments": {"query": "where do we validate JWT tokens", "token_limit": 1000}}}' | touring mcp-test

# Test 2: CLI mirror
touring find-code "where do we handle retries" --token-limit 500 --format compact

# Test 3: graceful degradation
TOURING_DISABLE_HYBRID=1 touring find-code "auth flow" -j | jq '.strategy_used'  # → "rrf-only"
```

---

#### **WAVE 7 — Polish & DI Runtime** (NEW — Target: 2026-06-29 → 2026-07-05, 5-6 dias)

| Phase | Agent | Task |
|---|---|---|
| FASE 0 | orchestrator | Health gate |
| FASE 1 | touring-scouter | Scout existing config infrastructure; identify DI candidate sites |
| FASE 2 | touring-architect | Design `Provider` trait + `ProviderRegistry` + lifecycle management |
| FASE 3 | Context7 | inventory crate docs, Rust DI patterns |
| FASE 4 | sequential-thinking | Decompose D27+D28 em 8 subtasks |
| FASE 4.5 | touring-auditor | Pre-impl: avaliar typestate vs runtime DI tension; confirm não breaking |
| FASE 5 | 2 engineers parallel | E1 = D27 (Plugin DI registry); E2 = D28 (MCP overhead self-report) |
| FASE 6 | auditor | Cross-audit, verify typestate preserved |
| FASE 7 | scriber | Document DI semantics, plugin authoring guide |

**Deliverables Wave 7 (AMPLIFICADA v4)**: D27 (L) + D28 (XS) + D29 (M, TouringFlowBuilder) + D30 (L, YAML rule engine + fix) + D32 (XS, tier UX) + D38 (M, cross-language perf benchmarks) + **D42 (M, cc-setup installer — NEW v4) + D44 (M, Speckit slash commands — NEW v4) + D45 (XS, permission auto-add — sub-task D42) + D47 (M, multi-project registry — NEW v4) + D49 (XS, handoff frontmatter — sub-task D44)**. Total ~4.190 LOC (v3: 2.760 / v2: 880). 191 new tests (v3: 138 / v2: 24).

**Adições v4 (5 deliverables)**:
- **E7 (NEW v4)**: D42 + D45 (cc-setup installer com permission auto-add embarcado)
- **E8 (NEW v4)**: D44 + D49 (10 slash commands speckit-style + handoff system)
- **E9 (NEW v4)**: D47 (multi-project registry)

**Total Wave 7 v4**: **9 engineers paralelos** (era 6 em v3). Risco R45 (paralelismo > histórico) reforçado — sub-dividir em W7.A (D27+D28+D29+D32+D38 — core polish) e W7.B (D30+D42+D44+D45+D47+D49 — extensibility + CC integration) se necessário.

**Estratégia v3** (6 engineers paralelos):
- E1 = D27 (Plugin DI registry)
- E2 = D28 (MCP overhead self-report)
- E3 = **D29 (TouringFlowBuilder declarative API + YAML pipeline)** — NEW v3
- E4 = **D30 (YAML rule engine + fix transformations + 30 builtin rules)** — NEW v3
- E5 = **D32 (Tier-based language UX matrix)** — NEW v3
- E6 = **D38 (Cross-language perf benchmarks com regression CI gate)** — NEW v3

Wave 7 amplificada é **wave de extensibility & polish completo**: depois dela, Touring tem flow API + rule engine + tier UX + DI runtime + cost reporting + perf benchmarks + classificação semântica (D31 da W4) — full extensibility e observability.

**Validation gate Wave 7**:
```bash
# Test 1: plugin runtime swap
touring config providers list -j | jq '.embedding | length'  # ≥ 3 registered
touring config providers set embedding=fastembed
touring config providers test fastembed -j | jq '.health'  # → "ok"

# Test 2: MCP overhead report
touring mcp-overhead --top 10 --format table  # human-readable
touring mcp-overhead -j | jq '.total_tokens'  # estimate
```

---

#### **WAVE 8 — Optional Investments** (Target: TBD, baseado em validação de demanda)

**Pré-requisito**: Gabriel valida que pelo menos 2 dos 3 (D10, D11, D12) trazem valor antes de começar.

##### **WAVE 8.A** — D11 (FDEB Edge Bundling) — Wave isolada, ~1 semana
##### **WAVE 8.B** — D12 (Filter DSL) — Wave isolada, ~1 semana
##### **WAVE 8.C** — D10 (Web UI) — Multi-wave (8.C.1=infra, 8.C.2=viz, 8.C.3=dashboard)

**Deliverables Wave 8 (AMPLIFICADA v4)**: D10 (XL) + D11 (L) + D12 (L) + D34 (M, Postgres backend) + D35 (XL, Cloudflare Workers Edge) + D36 (L, bidir file↔graph sync) + D39 (XL+, MVKL Knowledge Layer) + D40 (XL, Unison-store WONTFIX) + D41 (XL, CGM spike) + **D46 (L, .claude/plugins/touring/ system — NEW v4) + D48 (XS, multi-agent files — NEW v4)**. Total ~14.580 LOC se TODOS shippados (v3: 13.700 / v2: 3.700) — mas todos opcionais e gated.

**Adições v4 (2 deliverables)**:
- **Wave 8.J — D46 (.claude/plugins/touring/ plugin system)** — postergado, gated por demand de Touring multi-team
- **Wave 8.K — D48 (multi-agent .specify/.serena/.jules mirrors)** — postergado, apenas se Gabriel adopt outros agents

**Estratégia v3** (sub-waves isoladas, cada uma decisão Gabriel separada):
- **Wave 8.A** — D11 (FDEB Edge Bundling) — ~1 semana
- **Wave 8.B** — D12 (Filter DSL) — ~1 semana
- **Wave 8.C** — D10 (Web UI) — Multi-wave (8.C.1 infra, 8.C.2 viz, 8.C.3 dashboard)
- **Wave 8.D** — **D34 (Postgres backend) — NEW v3** — ~1 semana, feature flag opt-in
- **Wave 8.E** — **D35 (Cloudflare Workers Edge) — NEW v3** — multi-wave, requires D34 D1 backend
- **Wave 8.F** — **D36 (Bidirectional file↔graph sync) — NEW v3 (paradigm shift)** — provavelmente NÃO ship
- **Wave 8.G** — **D39 (MVKL Knowledge Layer) — NEW v3 (multi-quarter)** — North Star longo prazo
- **Wave 8.H** — **D40 (Unison-store) — NEW v3 — WONTFIX** — documentado para completude
- **Wave 8.I** — **D41 (CGM NeurIPS 2025) — NEW v3 — research spike only**

**Decisão crítica**: Wave 8 é **gated por demand validation**. Default: postergar TODAS as 9 sub-waves. Cada uma requer Gabriel approval explícito.

### 4.3 Cumulative metrics (revised v3 — 41 deliverables)

| Wave | Deliverables | Cum LOC | Cum Tests | Cum Days | Cum CLI cmds | Hook Registry | Synergy |
|---|---|---|---|---|---|---|---|
| Baseline | — | 0 | 5,100+ | 0 | 81 | 176 | 45 |
| W1 | D1+D2 | +600 | +23 | 4 | +0 (only --format flag) | 178 | 47 |
| W2 | D3+D4+D5+D13+D14+D15+D16 | +2,780 | +107 | 9-10 | +6 (viz×5 + search unified) | 180 | 49 |
| W3 (v3) | D6+D7+D8+D9+D17+**D37 NEW** | +5,930 | +187 | 20-23 | +12 (flow, rename, snapshot×4, clones, vfs-sync, **overlay×4**) | 184 | 53 |
| W4 (v3) | D18+D19+D20+D21+**D31+D33 NEW** | +7,930 | +319 | 28-32 | +9 (failover, ast node-types/importance, vfs-filter, **definitions classify, conflict detect**) | 190 | 58 |
| W5 | D22+D23+D24+D25 | +10,930 | +519 | 43-50 | +19 | 195 | 64 |
| W6 | D26 | +11,530 | +541 | 48-55 | +1 (find-code) | 197 | 67 |
| W7 (v3) | D27+D28+**D29+D30+D32+D38 NEW** | +14,290 | +679 | 60-72 | +18 (config providers, mcp-overhead, plugin, **flow run, rule list/run/test/explain/fix, lang status, perf bench**) | 203 | 75 |
| W8 (v3) — opt | D10+D11+D12+**D34+D35+D36+D39+D40+D41 NEW** | +27,990 | +1,029 | 120+ | +20 | 218 | 90 |

**Resumo de delivery core (Waves 1-7) v3**: ~**14.290 LOC**, ~**679 tests** novos, **8-10 semanas** (vs 6-8 v2), **+32 CLI cmds**, hook registry 176→**203** (+27), synergy 45→**75** (+30).

### 4.3.1 Cumulative metrics v4 (49 deliverables — CC integration merged)

| Wave | Deliverables | Cum LOC | Cum Tests | Cum Days | Cum CLI cmds | Hook Registry | Synergy |
|---|---|---|---|---|---|---|---|
| Baseline | — | 0 | 5,100+ | 0 | 81 | 176 | 45 |
| W1 | D1+D2 | +600 | +23 | 4 | +0 | 178 | 47 |
| W2 (v4) | D3+D4+D5+D13+D14+D15+D16+**D43** | +3,030 | +121 | 10-11 | +6 | 182 | 51 |
| W3 (v3) | D6+D7+D8+D9+D17+D37 | +6,180 | +201 | 21-24 | +12 | 186 | 55 |
| W4 (v3) | D18+D19+D20+D21+D31+D33 | +8,180 | +333 | 29-33 | +9 | 192 | 60 |
| W5 | D22+D23+D24+D25 | +11,180 | +533 | 44-51 | +19 | 197 | 66 |
| W6 | D26 | +11,780 | +555 | 49-56 | +1 | 199 | 69 |
| W7 (v4) | D27+D28+D29+D30+D32+D38+**D42+D44+D45+D47+D49** | +16,020 | +746 | 66-80 | +**40** (8 novos: cc-setup, project add/list/remove/current, command set ×11, init flag) | **211** (8 novos hooks: pre-grep, pre-glob, settings.json populador, etc.) | 80 |
| W8 (v4) — opt | D10+D11+D12+D34+D35+D36+D39+D40+D41+**D46+D48** | +30,680 (se TODOS) | +1,059 | 130+ | +20 | 226 | 95 |

**Resumo de delivery core (Waves 1-7) v4**: ~**16.020 LOC** (+12% vs v3), ~**746 tests** (+10%), **66-80 dias** (+10%), **+40 CLI cmds** (+25%), hook registry 176→**211** (+35), synergy 45→**80** (+35).

**Comparação v3 vs v4**:
- LOC: 14.290 → 16.020 (+12%)
- Tests: 679 → 746 (+10%)
- Days: 60-72 → 66-80 (+10%)
- CLI cmds: +32 → +40 (+25%)
- Hook Registry delta: +27 → +35 (+30%)
- Synergy WIRED_PAIRS delta: +30 → +35 (+17%)

**Comparação v1 vs v4 (versão original vs final)**:
- Deliverables: 12 → 49 (+308%)
- Waves: 4 → 8 (+100%)
- Risks: 16 → 50 (+213%)
- LOC core: ~5.800 → ~16.020 (+176%)
- Tests core: 60 (W1-3) → 746 (W1-7) (+1.143%)

**Comparação v2 vs v3**:
- LOC: 10.460 → 14.290 (+37%)
- Tests: 471 → 679 (+44%)
- Days: 54 → 72 (+33%)
- CLI cmds: 14 → 32 (+128%)
- Hook Registry delta: +21 → +27 (+29%)
- Synergy WIRED_PAIRS delta: +21 → +30 (+43%)

**Justificativa do crescimento**: 13 deliverables Thread-derived (D29-D41) adicionam **fundação estrutural permanente** — flow builder + rule engine + classification + tier UX + perf benchmarks + overlay graph + conflict SLAs. Investimento alto mas paybacks acumulativos: cada deliverable subsequente herda mais infraestrutura. Wave 7 sai como wave de **maior expansion** porque concentra 4 dos 13 novos.

</timeline>

---

<risks>

## 5. Risks com probabilidade × impacto e mitigações

| ID | Risco | P | I | Mitigation |
|---|---|---|---|---|
| R1 | Daemon degraded durante implementação (já está agora: `daemon_socket: error`) | HIGH | HIGH | **GATE FASE 0 obrigatório**. `update-touring` antes de cada wave. Se permanece degraded, fallback para `cargo check + grep` (REGRA #2 TACO-subagent.md). |
| R2 | `touring graph` daemon handlers não retornam dados ricos suficientes (e.g. falta `quality_score` em graph response) | MEDIUM | HIGH | FASE 1 scout valida exatamente o schema retornado por cada handler. Se faltar, **adicionar 1 sub-task no Wave** para extender daemon response (não inflar escopo do plano). |
| R3 | Encoding visual em D3 não agrada Gabriel | MEDIUM | LOW | Theme TOML overridable garante que ele pode customizar. Defaults conservadores (cores accessible WCAG-AA). |
| R4 | D7 `--apply` quebra código em macros/generics complexos | LOW (pelo design) | HIGH (se ocorrer) | Default `--plan` é read-only. `--apply` requer hash confirm + speculative validation. Speculative score < 0.8 aborta. |
| R5 | Performance regression em workspace grande (38k symbols) | MEDIUM | MEDIUM | Cap default `--max-nodes 200`. Benchmark P50/P99 antes/depois via `iai-callgrind` (já no workspace). |
| R6 | Conflito de schema com `touring-rkyv graph_ipc` quando estendemos GraphResponse | MEDIUM | MEDIUM | Versionar IPC: novos campos `#[serde(default)]`. Backward compat tested via 1 E2E test. |
| R7 | Acúmulo de god-file: `cli_handlers.rs` já tem 8249 LOC e vamos adicionar 6+ handlers | HIGH | LOW | Wave 1 inclui sub-task: split `cli_handlers.rs` em módulos por área (`cli_handlers/{graph,wiring,search,...}.rs`). REGRA #0 Potencializar — não deixar god-file crescer. |
| R8 | code-graph-ai pode adicionar features novas durante nossa implementação | MEDIUM | LOW | Não somos primeiros a chegar, somos melhores em outras dimensões (RL, generator typestate, memory tiers). Foco em paridade nas features high-ROI. |
| R9 | Wave 4 (Web UI) escopo creep | HIGH | MEDIUM | Postergada por design. Não inicia sem validation. |
| R10 | FDEB calibração de hyperparams custa muito tempo | HIGH | LOW | Opt-in flag. Defaults dos paper Holten/van Wijk 2009. Benchmark obrigatório. |
| R11 | Memory growth no daemon com snapshots | LOW | MEDIUM | Snapshots em disco (`~/.claude/touring/snapshots/`), não in-memory. Limite default: 50 snapshots; LRU eviction. |
| R12 | `tred` externo não disponível em alguns sistemas | LOW | LOW | Fallback Rust nativo já no plano D2. Detecta via `which tred` na inicialização. |
| R13 | Mermaid spec evolui (GitHub muda renderer) | LOW | LOW | Usar subset estável (flowchart TD + subgraph). Testar contra GitHub renderer manually a cada wave. |
| R14 | `cargo test --workspace` lentidão > 2 min com novos tests | MEDIUM | LOW | Mover testes pesados (Wave 4) para `#[ignore]` flag opt-in. Cargo nextest ainda OK. |
| R15 | Wave 3.B D7 entrega plan correto mas blast radius computado está stale | MEDIUM | MEDIUM | Cadeia 7 (Wiring Cache Staleness) obrigatória ANTES de gerar plan. Re-run `touring index rebuild` se snapshot > 5 min. |
| R16 | FASE 4.5 auditor reject muitas tasks como FALSE_POSITIVE | LOW | LOW | É feature, não bug — significa que o plan está caçando problems certos. Gabriel review final. |
| R17 (NEW v2) | Candle dependencies trazem transitive issues (CUDA/Metal feature flags em CI Linux) | MEDIUM | MEDIUM | Pin Candle 0.7+, default-features=false. CPU-only build em CI. Modelos lazy download (não em build). Modelo MVP pinned: BGE-small-en-v1.5 (33M params, ~130MB). |
| R18 (NEW v2) | sqlite-vec compilation falha em distros antigas (sqlite < 3.35) | MEDIUM | LOW | Atrás de feature flag `--features local-vector-store`. Detecta sqlite version no build.rs e emite warning. Fallback InMemory backend para tests. |
| R19 (NEW v2) | Qdrant client requer service externo (Docker) — quebra airgapped | LOW | MEDIUM | Qdrant é opt-in (`--features qdrant`). Default backend é sqlite-vec local. airgapped profile (D16) força sqlite-vec only. |
| R20 (NEW v2) | Asymmetric embeddings logic complex — bugs sutis em compatibility | MEDIUM | HIGH | D18 + D25 com 28 unit tests cobrindo matrix completa (symmetric/asymmetric × same/different family/model). E2E integration tests obrigatórios. |
| R21 (NEW v2) | Hybrid scoring weights (0.65/0.35) não otimais para Rust workspace específico | MEDIUM | LOW | Configurável via `[search.hybrid] dense_weight=0.X sparse_weight=0.Y`. Default = CodeWeaver values; Gabriel pode tunar. |
| R22 (NEW v2) | Wave 5 atrasa → Wave 6 (find_code) atrasa cascateado | HIGH | MEDIUM | D26 design inclui graceful degradation: se Wave 5 not deployed, find_code usa apenas D4 (RRF) — ainda útil mas sem dense vectors. Ship Wave 6 mesmo com Wave 5 incompleto. |
| R23 (NEW v2) | Embedding inference custa CPU/RAM em dev workstation | MEDIUM | MEDIUM | BGE-small (default) usa < 200MB RAM, < 50ms/embedding em CPU moderno. Profile via `iai-callgrind` antes/depois. Permitir desativar via `[embeddings] enabled=false`. |
| R24 (NEW v2) | D14 GracefulChunker refactor quebra chunkers existentes | MEDIUM | HIGH | Shadow validate via `touring shadow validate` em cada step. Refactor incremental: 1 lang per sub-task. Backward compat trait via blanket impl. |
| R25 (NEW v2) | D19 FailoverService cross-subsystem causa state inconsistency entre subsystems | MEDIUM | HIGH | Coordinator deve garantir invariants: nenhum subsystem em "transition" simultaneamente; rollout incremental subsystem-by-subsystem; observability via gate-metrics counters. |
| R26 (NEW v2) | Plugin DI runtime swap (D27) causa "stuck" daemon em provider mal configurado | MEDIUM | MEDIUM | Health check obrigatório antes de `set_default`. Rollback automático para previous provider em failure. Test command `touring config providers test <id>` antes de switch. |
| R27 (NEW v2) | MCP overhead self-report (D28) ele mesmo adiciona overhead | LOW | LOW | Caching: compute on session start, cache até config change. Approximate token count via str.len()/4 fallback se tiktoken-rs falha. |
| R28 (NEW v2) | Total LOC delta (~10k+) excede budget de manutenção | MEDIUM | MEDIUM | Wave gates obrigatórios: cada wave shippable independentemente; Gabriel pode parar em qualquer ponto sem deixar code half-done. Os deliverables são desacoplados (D5 standalone, D17 standalone, etc.). |
| R29 (NEW v2) | CodeWeaver evolui rápido (alpha→0.x) e supera gaps que estamos fechando | LOW | LOW | Filosoficamente diferentes: Touring optimizes structural analysis (RL, generator, blast/wiring/cycles), CodeWeaver optimizes search. Foco em **complementaridade**, não substituição. |
| R30 (NEW v2) | Token-counter (`tiktoken-rs`) tem licença restritiva | LOW | LOW | Verificar licença em D28. Fallback string len/4 approximate. Alternativa `cl100k_base` standalone implementation. |
| R31 (NEW v3) | D29 TouringFlowBuilder API surface incompatible com decompose existing | MEDIUM | MEDIUM | TouringFlowBuilder é API additive — decompose continua disponível e é canonical para tasks complexas. FlowBuilder serve uso embedded/programático. Documentar quando usar cada um. |
| R32 (NEW v3) | D30 YAML rule fix transformations quebram código (correctness crítico) | MEDIUM | HIGH | `--apply` requires speculative validation (touring shadow validate score >= 0.8). Default mode é dry-run. Each rule tem fixture-based tests (positive + negative + edge). Idempotência testada (apply 2x = mesmo resultado). |
| R33 (NEW v3) | D31 semantic classification accuracy < 99.7% após port (Touring tree-sitter version differ) | MEDIUM | MEDIUM | Relaxed acceptance: 99.5% (vs Thread 99.7%). TOML overrides per-language permitem ajuste fino. Fallback "unclassified" sempre válido. |
| R34 (NEW v3) | D31 data files (~200KB JSON + 25 TOML) inflam binário | LOW | LOW | `include_str!` em build → embedded data, mas binário cresce ~200KB. Aceitável (vs ~70MB binary atual). Alternative: lazy load from disk em initialization. |
| R35 (NEW v3) | D32 Tier categorization é subjetiva — Gabriel discorda | LOW | LOW | Tabela editável em SKILL.md. Tier matrix é configurável via `~/.claude/touring/lang-tiers.toml` override. Defaults baseados em test coverage real per language. |
| R36 (NEW v3) | D33 SLAs (<100ms/<1s/<5s) não são alcançáveis em workspace grande (38k symbols) | MEDIUM | MEDIUM | SLAs são targets, não hard bounds. CI gate emite warning, não failure, em violação. Tier 3 graph impact pode levar mais tempo em workspaces > 100k. Documentar limits. |
| R37 (NEW v3) | D37 Overlay Graph state inconsistency (delta + base divergem após crash) | MEDIUM | HIGH | Delta é in-memory only (não persistido). Crash → delta lost mas base preservado (não corrupted). `touring overlay status` detecta divergence. Auto-rebuild offered. |
| R38 (NEW v3) | D38 perf benchmark CI muito lento (>20min em CI) | MEDIUM | LOW | Use synthetic 1k file fixtures (não 10k). Cache fixtures cross-CI runs. Skip benchmark em PR < specific labels. Manual trigger para full benchmark. |
| R39 (NEW v3) | D34 Postgres schema migration falha mid-flight | MEDIUM | HIGH | Schema versioning via `sqlx-migrate`. Backup automático antes de migration. Rollback path testado. Migration é opt-in (não auto-run em update). |
| R40 (NEW v3) | D35 Cloudflare Workers Edge debugging painful | HIGH | MEDIUM | Postpone D35 até demand validation. Atrás de feature flag estável. Wrangler dev environment para local debugging. Logs via Workers Logs API. |
| R41 (NEW v3) | D36 bidir sync paradigm shift quebra mental model file-centric | HIGH | HIGH | **Recomendação explícita: NÃO IMPLEMENTAR** sem aprovação Gabriel L4+ explícita. Listed como D36 para completude mas WONTFIX por default. |
| R42 (NEW v3) | D39 MVKL multi-quarter — pivot midway por mudança de prioridades | HIGH | HIGH | **Postergado por design**. Não inicia sem demand validation + roadmap explícito. Pode ser dividido em 3 sub-investments independentes (L0/L1/L2 each shippable). |
| R43 (NEW v3) | D40 Unison-store WONTFIX mas implementado por engano | LOW | HIGH | Marcação explícita "DOCUMENTED, NOT RECOMMENDED" em §3.6 D40. Plan validation gate Wave 8 explicitamente confirma WONTFIX status. |
| R44 (NEW v3) | D41 CGM spike consome semanas sem ROI | MEDIUM | LOW | Time-boxed: 1 semana max. Output: feasibility report. Se ROI claro → submit Wave 9 separada. Se não → DOCUMENTED, postpone. |
| R45 (NEW v3) | Wave 7 amplificada (6 deliverables) excede paralelismo realístico | MEDIUM | MEDIUM | 6 engineers paralelos é máximo histórico (Wave Preditiva 2026-04-20 teve 3). Sub-dividir Wave 7 em W7.A (D27+D28+D29+D32) e W7.B (D30+D38) se necessário. |
| R46 (NEW v4) | Wave 7 v4 com **9 engineers paralelos** estoura limite operacional | HIGH | MEDIUM | Sub-dividir explicitamente em W7.A (D27+D28+D29+D32+D38 — core polish, 5 eng) e W7.B (D30+D42+D44+D45+D47+D49 — extensibility + CC integration, 4-5 eng). Cada sub-wave shippable independentemente. |
| R47 (NEW v4) | D42 `merge_settings_json` corrompe `~/.claude/settings.json` em conflitos | MEDIUM | HIGH | JSON-aware merge preserve fields desconhecidos. Backup automático antes de merge (`settings.json.bak-<timestamp>`). Idempotência garantida via test suite. Rollback path documentado. |
| R48 (NEW v4) | D43 Grep/Glob enrichment muito barulhento (false positives) | MEDIUM | LOW | Whitelist conservadora (PascalCase/snake_case strict, length 3-50 chars). Pass-through silencioso em <50ms timeout. Métrica `pre_grep_enrichment_count` e `pre_grep_zero_results_count` em gate-metrics — Gabriel pode disable via env var `TOURING_DISABLE_PREGREP=1`. |
| R49 (NEW v4) | D44 `handoffs:` frontmatter é proposta Thread, pode não ser feature CC nativa | MEDIUM | MEDIUM | **SPECULATION 0.6** — verificar Anthropic Claude Code docs antes de implementar. Se CC não suporta, fallback é commands sem handoffs (perda parcial de UX, ainda úteis). Sub-task FASE 3 (Context7 query) confirmar feature support. |
| R50 (NEW v4) | D47 multi-project registry conflita com daemon per-project actor design | LOW | MEDIUM | Daemon Touring é per-project actor por design (verified em memória). Multi-project é apenas registry alias-to-path; cada query continua per-project (alias resolve to path → daemon socket). Documentar em SKILL.md. |

</risks>

---

## 6. Validation strategy (per-wave gates)

### 6.1 TACO Phase 0 gate (always)

```bash
cargo check --workspace 2>&1 | grep "^error\[" | wc -l    # MUST be 0
touring doctor -j | jq '[.[] | select(.status != "ok")] | length'   # MUST be 0
touring status -j | jq '.composite_health_score'           # MUST be >= 0.6
```

### 6.2 Wave-end gates

**Wave 1 acceptance**:
```bash
# Test 1: DOT export valid
touring graph communities --format dot | dot -Tsvg > /tmp/w1.svg
[ -s /tmp/w1.svg ] || exit 1

# Test 2: Mermaid roundtrip
touring graph file <path> --format mermaid | grep -q "flowchart TD"

# Test 3: max-nodes cap
COUNT=$(touring graph file <huge_file> --max-nodes 50 --format json | jq '.nodes | length')
[ "$COUNT" -le 50 ] || exit 1

# Test 4: full test suite
cargo test --workspace --exclude touring-python 2>&1 | grep "test result: ok"

# Test 5: clippy zero warnings
cargo clippy --workspace -- -D warnings
```

**Wave 2 acceptance**:
```bash
# Test 1: theme override
echo '[node.fill]
quality_high = "#9c27b0"' > ~/.claude/touring/viz-theme.toml
touring viz workspace --format svg | grep -q "#9c27b0"

# Test 2: RRF unification
touring search unified "HookRuntime" --format json | jq '.results[0].badges' | grep -q "Unified"

# Test 3: tier label
touring wiring impact <hot-symbol> | jq '.tier' | grep -E "high|medium|low"
```

**Wave 3 acceptance** (amplificada com D17):
```bash
# Test 1-4: ver §4.2 Wave 3 validation gate

# Test 5: move detection (D17 NEW)
mv src/foo.rs src/bar.rs && touring vfs sync -j | jq '.moves | length'  # ≥ 1
```

**Wave 4 acceptance** (NEW):
```bash
# Test 1: family-aware fingerprint
touring config diff-fingerprint -j | jq '.compatibility'  # → "Compatible" | "BreakingMinor" | "BreakingMajor"

# Test 2: failover service status
touring failover status -j | jq '{primary_active, backup_ready, transitions_count}'

# Test 3: rignore parity
touring vfs filter-test secret.txt -j | jq '.ignored_by'  # → ".gitignore"

# Test 4: node types KB
touring ast node-types rust -j | jq '.node_types | length' | { read N; [ "$N" -ge 100 ] || exit 1; }
touring ast importance src/main.rs --threshold 0.7 -j | jq '.high_importance_nodes | length'
```

**Wave 5 acceptance** (NEW — CRITICAL):
```bash
# Test 1: embedding provider
touring embeddings test-provider candle bge-small-en-v1.5 "fn foo() {}" -j | jq '.dimensions'  # → 384

# Test 2: vector store backend
touring vector-store status -j | jq '{backend, collection_count, point_count}'

# Test 3: hybrid search end-to-end
touring index --rebuild-with-embeddings  # Long: 5-30 min depending workspace
touring search semantic "authentication validation flow" -j | jq '.results | length'  # ≥ 1
touring search unified "where do we handle retries" --hybrid -j | jq '.strategy_used'  # → "hybrid"

# Test 4: asymmetric embeddings
touring config set embeddings.dense.model bge-base-en-v1.5
touring session start asym-test type "test"  # should NOT trigger reembed (same family)
touring session events -j | jq '.embedding_state'  # → "compatible"

# Test 5: reranking cascade
touring search unified "auth" --rerank=cohere --rerank-fallback=local -j | jq '.reranked_by'

# Test 6: full test suite
cargo test --workspace --exclude touring-python --features hybrid-search 2>&1 | grep "test result: ok" | wc -l  # all packages green
```

**Wave 6 acceptance** (NEW — Agent UX):
```bash
# Test 1: super-tool MCP
echo '{"method":"tools/call","params":{"name":"touring_find_code","arguments":{"query":"where do we validate JWT tokens","token_limit":1000}}}' | touring mcp-test | jq '.result.matches | length'

# Test 2: CLI mirror
touring find-code "where do we handle retries" --token-limit 500 --format compact

# Test 3: graceful degradation
TOURING_DISABLE_HYBRID=1 touring find-code "auth flow" -j | jq '.strategy_used'  # → "rrf-only"

# Test 4: token budget respeitado
touring find-code "X" --token-limit 200 -j | jq '.total_tokens'  # ≤ 200
```

**Wave 7 acceptance** (NEW — Polish):
```bash
# Test 1: plugin DI
touring config providers list -j | jq '{embedding: (.embedding | length), vector_store: (.vector_store | length), reranker: (.reranker | length)}'
touring config providers set embedding=fastembed && touring config providers test fastembed -j | jq '.health'  # → "ok"

# Test 2: MCP overhead self-report
touring mcp-overhead --top 10 --format table
touring mcp-overhead -j | jq '{total_tokens, top_consumer: .by_tool[0]}'
```

### 6.3 Memory persistence (after each wave) — extended para 7 waves

```bash
touring memory store \
  "wave1-graph-viz-2026-05-XX" \
  "Wave 1 delivered D1 (graph --format dot|mermaid|json) + D2 (--max-nodes/--reduce). LOC: +600. Tests: +23. Hook Registry: 178." \
  --tier semantic --type lesson

touring memory store \
  "wave2-rich-encoding-search-2026-05-XX" \
  "Wave 2 (AMPLIFICADA) delivered D3+D4+D5+D13(intent)+D14(GracefulChunker)+D15(ResourceGovernor)+D16(profiles). LOC: +2780. Tests: +84. Hook Registry: 180." \
  --tier semantic --type lesson

touring memory store \
  "wave3-capability-parity-2026-05-XX" \
  "Wave 3 (AMPLIFICADA) delivered D6+D7+D8+D9+D17(move detection). LOC: +1950. Tests: +54. Hook Registry: 183." \
  --tier semantic --type lesson

touring memory store \
  "wave4-resilience-patterns-2026-05-XX" \
  "Wave 4 (NEW) delivered D18(checkpoint fingerprint)+D19(failover trait)+D20(rignore audit)+D21(node types KB). LOC: +1250. Tests: +64. Hook Registry: 187." \
  --tier semantic --type lesson

touring memory store \
  "wave5-hybrid-semantic-search-2026-06-XX" \
  "Wave 5 (CRITICAL) delivered D22(EmbeddingProvider)+D23(VectorStore)+D24(hybrid scoring+rerank)+D25(asymmetric embeddings). LOC: +3000. Tests: +200. Hook Registry: 192. Synergy 60. New crates: touring-embeddings, touring-vector-store, touring-search-fusion." \
  --tier semantic --type lesson

touring memory store \
  "wave6-find-code-supertool-2026-06-XX" \
  "Wave 6 delivered D26 (touring_find_code MCP super-tool). LOC: +600. Tests: +22. Hook Registry: 194." \
  --tier semantic --type lesson

touring memory store \
  "wave7-plugin-di-cost-report-2026-07-XX" \
  "Wave 7 delivered D27(plugin DI runtime) + D28(MCP overhead self-report). LOC: +880. Tests: +24. Hook Registry: 197." \
  --tier semantic --type lesson

# RL reward injection (per wave)
touring learning reward orchestrate 1.0 "wave1_graph_viz_completed"
touring learning reward orchestrate 1.0 "wave2_rich_encoding_search_amplified_completed"
touring learning reward orchestrate 1.0 "wave3_capability_parity_amplified_completed"
touring learning reward orchestrate 1.0 "wave4_resilience_patterns_completed"
touring learning reward orchestrate 1.0 "wave5_hybrid_semantic_search_completed"
touring learning reward orchestrate 1.0 "wave6_find_code_supertool_completed"
touring learning reward orchestrate 1.0 "wave7_plugin_di_cost_report_completed"
```

---

## 7. Quality gates (TACO Delivery Checklist)

Cada wave precisa passar TODOS:

```
□ GABRIEL APROVOU            — Objetivo do wave alcançado
□ FUNCTIONAL                 — Code executa, golden path + edge cases OK
□ TESTED                     — Happy + edge + integration
□ ROBUST                     — Error handling em todo path; no panic em prod
□ READABLE                   — Clear names, obvious flow, no god-files novos
□ DOCUMENTED                 — SKILL.md updated, refs novos, changelog entry
□ SKILL HYGIENE              — package_skill.py exit 0; SKILL.md < 500 linhas
□ NO REGRESS                 — Existing tests still green
□ NO HALLUC                  — VGP applied; symbols verified
□ DELIVERABLE                — `touring graph X --format dot \| dot` works
□ SCOPE POTENCIALIZADO       — Zero new orphans; if any, wired or deleted
□ TACO VALIDADO              — `touring e2e -j` OK; `touring doctor -j` 5/5
□ MEMORY PERSISTED           — Wave summary stored via `touring memory store`
□ RL REWARD INJECTED         — `touring learning reward orchestrate 1.0`
```

---

## 8. Self-validation of this plan (v2)

| Check | Status |
|---|---|
| Each deliverable atomically shippable? | ✅ — D1-D9 + D13-D21 + D26-D28 standalone. D22-D25 sequential mas cada sub-wave é independent. D10-D12 opcionais. |
| Dependencies explicit and acyclic? | ✅ — graph na §4.1 v2 é DAG; topological sort listada com 8-hop critical path |
| Estimates realistic (T-shirt)? | ✅ — XS (D5,D16,D20,D28) → S (D2,D4,D6,D14,D15,D17,D21) → M (D1,D7,D8,D9,D13,D18,D19,D22,D24,D25,D26) → L (D3,D11,D12,D23,D27) → XL (D10). Calibrado em waves históricos: Wave Preditiva 2026-04-20 (3 engineers parallel, 47 tests, ~1.500 LOC, 3-4 dias) |
| Risks have mitigations? | ✅ — 30 riscos listados (16 v1 + 14 v2 CodeWeaver-driven), todos com mitigation |
| Touring constitutional compliance? | ✅ — REGRA #0 (Potencializar) preserved (Wave 7 D27 wired all providers, sem orphans), REGRA #11 (zero git), REGRA #12 (disk hygiene — Wave 5 modelos lazy download, não build), REGRA #13 (skill hygiene — SKILL.md updates por wave, refs novos) |
| Code-First gate (FIX-S4)? | ✅ — Cadeia 5 em FASE 0 de cada wave; Cadeia 7 obrigatória em D7 (rename), D8 (snapshot), D17 (move detection — race conditions), D19 (failover) |
| VP-Scout cadeias 1-7 obrigatórias? | ✅ — FASE 1 cada wave invoca touring-scouter com VP-Scout v1.1 |
| FASE 4.5 GATE crítico? | ✅ — listado em cada wave; Wave 3.B (D7), Wave 4 (D19 cross-subsystem coordination), Wave 5 (D22 deps validation) requerem audit reforçado |
| Sequential-thinking carregado em FASE 0? | ✅ — `ToolSearch select:mcp__sequential-thinking__sequentialthinking` listado |
| Critical path identificado? | ✅ — D1 → D4 → D22 → D23 → D24 → D25 → D26 → D27 (8 hops, 6-8 weeks core delivery) |
| Greenfield crates listados? | ✅ — Wave 5 cria `touring-embeddings/` + `touring-vector-store/` + `touring-search-fusion/`. Workspace member adds explicitos. |
| Backward compat preservada? | ✅ — Todos novos fields em manifest/checkpoint usam `#[serde(default)]`; degraded modes graceful em D26 (sem hybrid) e D14 (sem semantic chunker) |
| Filosofia preservada? | ✅ — Touring continua dominante em RL/typestate/blast/wiring; CodeWeaver insights são **complemento**, não substituição |
| Multi-input synthesis coerente? | ✅ — D1-D12 (Graphviz/cargo-depgraph/code-graph-ai) + D13-D28 (CodeWeaver) integrados em DAG único, dependências cruzadas explícitas (D4→D24, D17→D25, D18→D25, D13→D26) |

---

## 9. Open questions (Gabriel decides) — atualizada v2

### 9.1 Visual & UX (do plan v1)

1. **Web UI demand** — proceder com Wave 8.C? Ou postergar indefinidamente?
2. **Theme defaults** — paleta proposta (verde/amarelo/vermelho) é OK ou prefere outras (e.g. purple/teal)?
3. **`--apply` in D7** — autorizar implementar ou ficar apenas `--plan`?
4. **Snapshot retention** — 50 snapshots LRU é OK ou prefere unbounded?
5. **DSL D12 vs shell+jq** — prefere DSL formal ou continua compose via shell?
6. **CI integration** — D8 `diff-impact <git-ref>` deveria ter um companion GitHub Action?

### 9.2 Hybrid Semantic Search — Wave 5 (NEW v2 — DECISÕES ESTRATÉGICAS)

7. **Wave 5 GO/NO-GO** — esta é o maior investimento (~3.000 LOC, 14-18 dias). É priority-0 (fechar gap competitivo crítico vs CodeWeaver/code-graph-ai/Cursor) ou priority-2 (defer pra v32.x)?
8. **Backend default Wave 5** — sqlite-vec local (zero deps externas, Touring-native) ou Qdrant (industry standard, requer Docker)?
9. **Embedding default Wave 5** — Candle BGE-small (puro Rust, ~130MB) ou FastEmbed (Python interop, mais modelos)?
10. **Voyage AI integration** — adicionar provider HTTP client ou ficar local-only (airgapped-friendly)?
11. **Rerank advanced** — Wave 5.3 ship com rerank no-op MVP ou já com Cohere/Voyage HTTP providers?
12. **Reembed cost tolerance** — Wave 5 introduz embedding inference (~50ms/chunk em CPU). É OK em dev workstation, ou exigimos GPU detection?

### 9.3 Agent UX — Wave 6 (NEW v2)

13. **find_code position** — D26 deve ser apenas mais 1 MCP tool (somar aos 96), ou ser **promovido a default** com os outros 96 retidos como advanced?
14. **Token budget default** — qual limite default em `token_limit`? CodeWeaver não documenta; sugestão 2000 (médio) vs 500 (agressivo) vs 5000 (generoso)
15. **Intent inference v2** — Wave 6 fica com keyword heuristics (D13 v1) ou já invoca LLM/inferlets WASM (mais caro mas mais preciso)?

### 9.4 Resilience & DI — Wave 4+7 (NEW v2)

16. **Asymmetric embeddings adoption** — D18+D25 são complexos. Adopt full ou shippamos symmetric-only e adicionamos asymmetric em uma sub-wave futura?
17. **Plugin DI tension** — D27 introduz runtime DI sobre typestate. Aceitável (typestate continua source-of-truth) ou prefere ficar apenas typestate?
18. **MCP overhead alarm threshold** — D28 emite warning se total > 16k tokens. Threshold OK ou prefere outro valor (12k mais agressivo)?

---

## 10. Master commit checklist (do day 1 do Wave 1)

- [ ] `update-touring` (rebuild + dual-target install + restart)
- [ ] `touring doctor -j` 5/5 ok
- [ ] `cargo check --workspace` exit 0
- [ ] `touring memory recall "graph-viz"` (verifica se há lessons prévias)
- [ ] `ToolSearch select:mcp__sequential-thinking__sequentialthinking` (load deferred tool)
- [ ] Criar branch local (Gabriel) — TACO **NÃO** toca git
- [ ] Open `~/.claude/rust/docs/2026-04-30-graph-viz-capability-parity-master-plan.md` (este doc) como single source of truth
- [ ] Confirmar resposta de open questions §9
- [ ] Spawn FASE 1 scout: touring-scouter agent
- [ ] Iterate FASE 0 → FASE 7 conforme TACO Phase Protocol v6.2

---

## 11. References

- **TACO Phase Protocol v6.2**: `~/.claude/rules/TACO-subagent.md`
- **VP-Scout v1.1 (cadeias 1-7)**: `~/.claude/rules/VP-Scout.md`
- **Touring CLI ranks (Tier 1-9)**: `~/.claude/rules/touring-cli-index.md`
- **Skill master**: `~/.claude/skills/Touring/SKILL.md`
- **Skill hygiene REGRA #13**: `~/.claude/CLAUDE.md` §REGRA #13
- **Disk hygiene**: `~/.claude/rules/disk-hygiene.md`
- **Touring rebuild**: `~/.claude/rules/touring-rebuild.md`

### External sources (v1 — graph viz)

- [Graphviz documentation](https://graphviz.org/documentation/)
- [Graphviz layouts](https://graphviz.org/docs/layouts/)
- [tred man page](https://graphviz.org/docs/cli/tred/)
- [gvpr docs](https://graphviz.org/docs/cli/gvpr/)
- [jplatte/cargo-depgraph](https://github.com/jplatte/cargo-depgraph)
- [MonsieurBarti/code-graph-ai](https://github.com/MonsieurBarti/code-graph-ai)
- [Holten Force-Directed Edge Bundling (2009)](https://classes.engineering.wustl.edu/cse557/readings/holten-edgebundling.pdf)
- [Cormack et al. Reciprocal Rank Fusion (2009)](https://plg.uwaterloo.ca/~gvcormac/cormacksigir09-rrf.pdf)
- [Awesome Graphviz](https://dbt4u.github.io/awesome-graphviz/)

### External sources (v2 — CodeWeaver merge)

- [knitli/codeweaver (GitHub)](https://github.com/knitli/codeweaver) — main repo
- [CodeWeaver README](https://github.com/knitli/codeweaver/blob/main/README.md) — features matrix
- [CodeWeaver WHY.md](https://github.com/knitli/codeweaver/blob/main/docs/WHY.md) — agent UX philosophy
- [CodeWeaver competitive comparison](https://github.com/knitli/codeweaver/blob/main/docs/archive/comparison.md) — vs Serena, Cursor, Continue, Cody, Bloop, Aider
- [SemanticChunker](https://github.com/knitli/codeweaver/blob/main/docs-site/src/content/docs/api/engine/chunker/semantic.mdx) — multi-tier degradation strategy
- [GracefulChunker (selector)](https://github.com/knitli/codeweaver/blob/main/docs-site/src/content/docs/api/engine/chunker/selector.mdx) — primary/fallback wrapper pattern
- [ResourceGovernor](https://github.com/knitli/codeweaver/blob/main/docs-site/src/content/docs/api/engine/chunker/governance.mdx) — context manager para timeout + chunk count
- [CheckpointManager + family-aware fingerprint](https://github.com/knitli/codeweaver/blob/main/docs-site/src/content/docs/api/engine/managers/checkpoint_manager.mdx)
- [FileManifestManager](https://github.com/knitli/codeweaver/blob/main/docs-site/src/content/docs/api/engine/managers/manifest_manager.mdx) — incremental indexing + move detection
- [VectorReconciliationService](https://github.com/knitli/codeweaver/blob/main/docs-site/src/content/docs/api/engine/services/reconciliation_service.mdx)
- [FailoverService](https://github.com/knitli/codeweaver/blob/main/docs-site/src/content/docs/api/engine/services/failover_service.mdx)
- [find_code agent_api](https://github.com/knitli/codeweaver/blob/main/docs-site/src/content/docs/api/server/agent_api/search/index.mdx) — single super-tool philosophy
- [Search pipeline](https://github.com/knitli/codeweaver/blob/main/docs-site/src/content/docs/api/server/agent_api/search/pipeline.mdx)
- [Intent classification](https://github.com/knitli/codeweaver/blob/main/docs-site/src/content/docs/api/server/agent_api/search/intent.mdx)
- [Hybrid scoring + semantic weighting](https://github.com/knitli/codeweaver/blob/main/docs-site/src/content/docs/api/server/agent_api/search/scoring.mdx)
- [IndexerSettings + rignore filtering](https://github.com/knitli/codeweaver/blob/main/docs-site/src/content/docs/api/engine/config/indexer.mdx)

### External sources (v3 — Thread merge — 2026-05-01)

- [knitli/thread (GitHub)](https://github.com/knitli/thread) — high-perf code analysis platform Rust
- [Thread README](https://github.com/knitli/thread/blob/main/README.md) — service-library dual arch + benchmarks
- [Thread AI Knowledge Layer Design (70k LOC)](https://github.com/knitli/thread/blob/main/docs/architecture/AI_KNOWLEDGE_LAYER_DESIGN.md) — Multi-Resolution MVKL (Option C recommended)
- [Thread Incremental Update System Design (54k LOC)](https://github.com/knitli/thread/blob/main/claudedocs/INCREMENTAL_UPDATE_SYSTEM_DESIGN.md) — AnalysisDefFingerprint + DependencyGraph + SymbolDependency
- [Thread Semantic Classification Spec (45k LOC)](https://github.com/knitli/thread/blob/main/docs/architecture/SEMANTIC_CLASSIFICATION_SPEC.md) — port CodeWeaver Python → Rust 99.7% accuracy
- [ThreadFlowBuilder source](https://github.com/knitli/thread/blob/main/crates/flow/src/flows/builder.rs) — fluent dataflow builder API
- [Thread Realtime Code Graph spec](https://github.com/knitli/thread/tree/main/specs/001-realtime-code-graph) — Overlay Graph Architecture (FR-017)

### Papers/standards relevantes

- [Cormack et al. — Reciprocal Rank Fusion (2009)](https://plg.uwaterloo.ca/~gvcormac/cormacksigir09-rrf.pdf) — k=60 default
- [Voyage AI Code-3 paper](https://blog.voyageai.com/2024/12/04/voyage-code-3/) — hybrid search +14.52% precision
- [BGE embeddings (BAAI)](https://huggingface.co/BAAI/bge-small-en-v1.5) — 33M params, 384 dims
- [Candle (HuggingFace)](https://github.com/huggingface/candle) — pure-Rust ML framework
- [sqlite-vec (Mozilla)](https://github.com/asg017/sqlite-vec) — zero-deps vector search SQLite extension
- [Qdrant](https://qdrant.tech) — production vector database
- [Code Graph Model NeurIPS 2025](https://arxiv.org/abs/2410.18936) — 512x context compression via graph attention masking, 44% SWE-bench Lite
- [Unison content-addressed code](https://www.unison-lang.org/) — paradigm reference for D40
- [Sourcegraph SCIP](https://sourcegraph.com/blog/announcing-scip) — 10x faster, 4-5x smaller than LSIF
- [JetBrains MPS](https://www.jetbrains.com/mps/) — projectional editing reference
- [Darklang AST-as-DB](https://darklang.com/) — paradigm reference
- [ast-grep](https://ast-grep.github.io/) — meta-variable pattern matching (`$VAR`, `$$$ITEMS`, `$_`) — base de D30 YAML rules
- [Cloudflare Workers Rust SDK](https://github.com/cloudflare/workers-rs) — base de D35
- [tokio-postgres](https://github.com/sfackler/rust-postgres) — base de D34

---

## Changelog do plano

### 2026-04-30 v1 (initial)

- 12 deliverables (D1-D12), 4 waves, 16 risks
- Foco: visual export (DOT/Mermaid) + capability parity com code-graph-ai
- 60 testes adicionados Waves 1-3 + 72 opcionais Wave 4
- ~5.800 LOC core (Waves 1-3) + ~3.700 LOC opcional (Wave 4)
- Total cumulativo Wave 3: +97 tests, +12 CLI cmds, +183 hook registry, +6 synergy pairs
- Status: PROPOSED, aguarda Gabriel approval e respostas de §9 (6 questões)

### 2026-04-30 v2 (CodeWeaver merge)

- **+16 deliverables** (D13-D28) derivados de análise profunda de knitli/codeweaver
- **+14 risks** (R17-R30) cobrindo Candle/sqlite-vec/Qdrant deps + asymmetric embeddings + cascade failures
- **Reorganização de 4 → 8 waves** com critical path explícito (D1 → D4 → D22 → D23 → D24 → D25 → D26 → D27, 8 hops)
- **Wave 2 amplificada** (era D3+D4+D5; agora +D13+D14+D15+D16 = 7 deliverables, ~2.780 LOC, 84 tests, 4 engineers parallel)
- **Wave 3 amplificada** (adiciona D17 move detection)
- **Wave 4 NEW** (resilience patterns: D18 family-aware fingerprint + D19 failover trait + D20 rignore audit + D21 node types KB)
- **Wave 5 NEW CRITICAL** (hybrid semantic search: D22 embedding provider + D23 vector store + D24 hybrid scoring + D25 asymmetric embeddings, ~3.000 LOC, 14-18 dias, 200 tests, 3 novos crates)
- **Wave 6 NEW** (D26 find_code super-tool MCP, agent UX)
- **Wave 7 NEW** (D27 plugin DI runtime + D28 MCP overhead self-report)
- **Wave 8** = original Wave 4 renomeada (D10 Web UI + D11 FDEB + D12 DSL — todas opcionais)
- **+12 open questions** (§9.2-9.4) sobre Wave 5 strategic decisions
- **Total cumulativo Wave 7 (core)**: ~10.460 LOC, ~471 tests, 6-8 semanas, +14 CLI cmds, hook registry 176→197 (+21), synergy 45→66 (+21)
- **Total cumulativo Wave 8 (com optional)**: ~14.160 LOC, ~543 tests, hook registry 203, synergy 70
- **3 novos crates**: `touring-embeddings/`, `touring-vector-store/`, `touring-search-fusion/`
- **Status**: PROPOSED v2, **aguarda Gabriel decision em §9.2 (Wave 5 GO/NO-GO)** primeiro — esse é o investimento estratégico maior. Wave 1-4 podem proceder independente da decisão Wave 5.

---

## Decision tree para Gabriel

```
PASSO 1: Aprovar Wave 1 (D1+D2)?
  ├─ SIM → Iniciar 2026-05-04
  └─ NÃO → Stop. Plano fica em backlog.

PASSO 2: Aprovar Wave 2 amplificada (D3+D4+D5+D13+D14+D15+D16)?
  ├─ SIM → Iniciar após Wave 1 (~2026-05-08)
  ├─ PARCIAL: Apenas D3+D4+D5 (sem CodeWeaver insights) → simpler scope, ~1.100 LOC
  └─ NÃO → Stop. Wave 1 isolada também é shippable.

PASSO 3: Aprovar Wave 3 (D6+D7+D8+D9+D17)?
  ├─ SIM → Iniciar após Wave 2 (~2026-05-14)
  ├─ PARCIAL: Sem D7 --apply (apenas --plan) → reduz risk
  └─ NÃO → Stop em Wave 2.

PASSO 4: Aprovar Wave 4 (D18+D19+D20+D21)?
  ├─ SIM → Iniciar após Wave 3 (~2026-05-23). Independente de Wave 5.
  └─ NÃO → Wave 5 ainda viável sem fingerprint family-aware (less optimal).

PASSO 5 (CRITICAL DECISION): Aprovar Wave 5 (D22+D23+D24+D25)?
  ├─ SIM → Iniciar após Wave 4 (~2026-06-02). Strategic 14-18 dias.
  ├─ PARCIAL: D22+D23 only (sem hybrid scoring/asymmetric) → still useful para Wave 6 fallback
  ├─ DEFER: pular para Wave 6 com BM25-only D26 (graceful degradation)
  └─ NÃO → Wave 6 ainda viável em mode degraded; Wave 7 segue normal.

PASSO 6: Aprovar Wave 6 (D26 find_code)?
  ├─ SIM → Iniciar após Wave 5 (~2026-06-23) ou após Wave 4 se W5 deferred (~2026-06-02)
  └─ NÃO → Stop. Touring continua com 96 MCP tools, sem super-tool agent UX.

PASSO 7: Aprovar Wave 7 (D27+D28)?
  ├─ SIM → Iniciar após Wave 6 (~2026-06-29). Polish + observability.
  └─ NÃO → Funcionalidade core já completa em W1-W6.

PASSO 8 (OPTIONAL): Aprovar Wave 8 (D10+D11+D12)?
  ├─ Web UI demand surge? → D10
  ├─ FDEB precisa para grafos > 5k nodes? → D11
  ├─ DSL formal preferida vs shell+jq? → D12
  └─ Default: NÃO. Postergar indefinidamente.
```

---

## Recomendação executiva (com base em v2)

**Cenário ideal (Gabriel quer fechar TODOS os gaps competitivos)**:
- Aprovar Waves 1-7 sequenciais. ~6-8 semanas de delivery. Hook Registry 176 → 197. Synergy 45 → 66. Touring v31.x.
- ROI: fecha 6 gaps competitivos (visual + capability parity + grafos densos + hybrid search + resilience patterns + agent UX) sem comprometer primazia analítica.
- Riscos principais: R17 (Candle deps), R20 (asymmetric logic), R22 (Wave 5 cascade) — todos com mitigation.

**Cenário conservador (Gabriel prioriza ROI imediato)**:
- Waves 1-3 + 7 (skip Waves 4-6). ~2-3 semanas de delivery.
- Entrega visual export + capability parity + DI polish.
- **Não fecha hybrid search gap** — Touring continua sem dense embeddings.
- Postpone Waves 4-6 para v32.x conforme demand validation.

**Cenário pragmático (recomendado)**:
- Waves 1-3 obrigatórias (visual + capability + intent + RRF + chunker resilience). ~2-3 semanas.
- Wave 4 (resilience patterns) — 1 semana.
- **Decision point**: Após Wave 4 completa, decide Wave 5 baseado em uso real do Touring (Gabriel observa que queries semânticas são limitação? Aprova W5).
- Wave 6 (find_code) shippa com graceful degradation (BM25-only se W5 not deployed).
- Wave 7 (polish) sempre vale a pena.
- Wave 8 (optional) — defer indefinidamente.

---

### 2026-05-01 v3 (Thread merge — esta revisão)

- **+13 deliverables** (D29-D41) derivados de análise profunda de knitli/thread (high-perf code analysis platform Rust)
- **+15 risks** (R31-R45) cobrindo flow builder API surface + YAML rule fix correctness + classification accuracy + tier subjectivity + SLA viability + overlay state + perf bench CI cost + Postgres migration + Edge debugging + bidir paradigm shift + MVKL pivot + Unison WONTFIX + CGM spike + Wave 7 paralelismo
- **Reorganização**: 3 waves amplificadas (W3.A+B+**C NEW**, W4 +**D31+D33**, W7 +**D29+D30+D32+D38**); Wave 8 expandida com 6 novos opcionais (D34-D41)
- **Wave 3 amplificada**: +D37 Overlay Graph (Wave 3.C NEW, ~1.200 LOC, 14 tests, evolução natural de D8 snapshot)
- **Wave 4 amplificada**: +D31 semantic classification (~500 LOC + ~200KB data, 50 tests, 99.5%+ accuracy target across 27 langs) + D33 multi-tier conflict SLAs (~250 LOC, 18 tests, P99 < 100ms/1s/5s targets)
- **Wave 7 amplificada (era D27+D28)**: agora 6 deliverables (D27+D28+D29+D30+D32+D38) com **6 engineers paralelos**:
  - D29 TouringFlowBuilder declarative API (~600 LOC, 40 tests, 12 steps + 5 targets, YAML pipeline)
  - D30 YAML rule engine + fix transformations (~800 LOC, 62 tests, 30 builtin rules, ast-grep operators)
  - D32 Tier-based language UX (~80 LOC, 5 tests, 4-tier honest matrix)
  - D38 Cross-language perf benchmarks (~400 LOC, 9 scenarios, regression CI gate)
- **Wave 8 amplificada**: +6 novos opcionais — D34 Postgres backend, D35 Cloudflare Workers Edge, D36 bidir file↔graph sync, D39 MVKL Knowledge Layer, D40 Unison-store WONTFIX, D41 CGM spike
- **Cumulative metrics core (Waves 1-7) v3**: ~14.290 LOC (+37% vs v2), ~679 tests (+44%), 60-72 dias (+33%), +32 CLI cmds (+128%), hook registry 176→**203** (+27, vs +21 v2), synergy 45→**75** (+30, vs +21 v2)
- **3 novos crates** (vs v2): `touring-flow/`, `touring-rule-engine/`, `touring-definitions/` (em adição aos 3 da v2)
- **Total novos crates v3**: 6 (`touring-embeddings`, `touring-vector-store`, `touring-search-fusion`, `touring-flow`, `touring-rule-engine`, `touring-definitions`)
- **Status**: PROPOSED v3, **gating decisions**:
  - §9.2 questão 7 — Wave 5 GO/NO-GO (mantém de v2)
  - **NEW**: D31 (semantic classification) é foundational para D13/D24/D29 — recomendado SHIP em Wave 4
  - **NEW**: D29+D30 (FlowBuilder + YAML rules) shippam juntos em Wave 7 — extensibility unlock
  - **NEW**: D37 (Overlay Graph) é evolução natural de D8 — recomendado SHIP em Wave 3.C
  - **NEW**: D32+D38 (tier UX + perf benchmarks) são polish baixo-risco — recomendado SHIP em Wave 7
  - **NEW**: D34-D41 são gated by demand validation — provavelmente postergar todos

---

### 2026-05-01 v4 (CC Integration foco — esta revisão)

- **+8 deliverables** (D42-D49) derivados de re-análise dos 3 repos (code-graph-cli + Thread + CodeWeaver) sob lens "Claude Code integration"
- **+5 risks** (R46-R50) cobrindo Wave 7 paralelismo (9 engineers), settings.json corruption, Grep/Glob enrichment noise, handoffs feature uncertainty, multi-project actor coordination
- **Wave 2 amplificada (era 7 em v3 → 8 em v4)**: + **D43 PreToolUse Grep/Glob enrichment** (S, ~250 LOC, 14 tests, massive token saving)
- **Wave 7 amplificada (era 6 em v3 → 11 em v4)**: + **D42 cc-setup installer** (M, ~500 LOC, 18 tests, mirror code-graph-cli `setup.rs` com `include_str!`) + **D44 Speckit slash commands** (M, ~500 LOC + 11 .md files, 12 tests, mirror Thread `.claude/commands/speckit.*`) + **D45 permission auto-add** (XS, ~30 LOC, sub-task D42) + **D47 multi-project registry** (M, ~400 LOC, 16 tests) + **D49 handoff frontmatter** (XS, ~50 LOC, sub-task D44)
- **Wave 8 amplificada (era 9 em v3 → 11 em v4)**: + **D46 .claude/plugins/touring/ system** (L, ~800 LOC, gated, mirror Thread `.claude/plugins/ctx/`) + **D48 multi-agent files** (XS, ~80 LOC, gated, mirror Thread `.specify/.serena/.jules`)
- **Cumulative metrics core (Waves 1-7) v4**: ~**16.020 LOC** (+12% vs v3), ~**746 tests** (+10%), 66-80 dias (+10%), **+40 CLI cmds** (+25%), hook registry 176→**211** (+35), synergy 45→**80** (+35)
- **Total comparação v1 → v4**: 12 → 49 deliverables (+308%); 4 → 8 waves (+100%); 16 → 50 risks (+213%); ~5.800 → ~16.020 LOC core (+176%)
- **Status**: PROPOSED v4. **Ações imediatas recomendadas**:
  1. ✅ **D43 (Grep/Glob enrichment) ship em Wave 2** — token saving massivo, baixo risco, pareamento com D4 RRF
  2. ✅ **D42+D45 (cc-setup installer + permission auto-add) ship em Wave 7** — UX zero-friction
  3. ✅ **D44+D49 (slash commands suite + handoffs) ship em Wave 7** — pareamento natural com D29 FlowBuilder
  4. ✅ **D47 (multi-project registry) ship em Wave 7** — útil mesmo single-user
  5. ⚠️ **D46+D48 ficam Wave 8 gated** — postergar até demand validation
  6. ⚠️ **R46 ATTENTION**: Wave 7 v4 tem 9 engineers paralelos — sub-dividir em W7.A (5 eng) + W7.B (4-5 eng)

---

_Plan generated under TACO v6.2 Phase Protocol via /Touring --ultrathink --sequential-thinking_
_v1 (2026-04-30): Graphviz/cargo-depgraph/code-graph-ai analysis — 12 deliverables, 4 waves, 16 risks, 6 open questions_
_v2 (2026-04-30): CodeWeaver insights merged — 28 deliverables (+16), 8 waves (+4), 30 risks (+14), 18 open questions (+12)_
_v3 (2026-05-01): Thread insights merged — 41 deliverables (+13), 8 waves (Wave 3.C NEW + 6 amplificadas), 45 risks (+15)_
_v4 (2026-05-01): CC Integration foco — 49 deliverables (+8 D42-D49), 8 waves (W2/W7/W8 amplificadas), 50 risks (+5 R46-R50)_
