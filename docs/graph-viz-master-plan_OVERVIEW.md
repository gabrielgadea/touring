---
name: graph-viz-master-plan
description: Graph Visualization, Capability-Parity, Hybrid Semantic Search, Knowledge Layer & CC Integration Master Plan — Overview
type: project
related_files:
  - graph-viz-master-plan_OVERVIEW.md
  - graph-viz-master-plan_STATUS.md
  - graph-viz-master-plan_WAVES_1_2.md
  - graph-viz-master-plan_WAVE_3.md
  - graph-viz-master-plan_WAVE_4.md
  - graph-viz-master-plan_WAVE_5.md
  - graph-viz-master-plan_WAVE_6.md
  - graph-viz-master-plan_WAVE_7.md
  - graph-viz-master-plan_WAVE_8.md
  - graph-viz-master-plan_DEPENDENCIES.md
---

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