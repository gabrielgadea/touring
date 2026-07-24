---
plan: "touring-premium-refactor-2026"
version: "1.0.0"
type: "architecture"
created: "2026-05-11"
supersedes: "nothing (greenfield architectural redesign)"
relates_to:
  - 02-DEPLOYMENT.md
  - 03-COMMERCIAL.md
  - MASTER-PLAN-2026
---
# 01-ARCHITECTURE — Touring Premium Topology

> **Status**: Proposed | **Date**: 2026-05-11
> **Approved by**: Gabriel Gadea (architect) via decision `decision:touring-premium-roadmap-2026-05-11`

## 1. Diagnostic context (from 2026-05-11 forensic audit)

| Symptom | Evidence |
|---|---|
| **Macrociclo arquitetural HIGH severity** | `touring wiring cycles` depth=618, 9 crates |
| **Fragmentação excessiva** | 46 crates, 6 anêmicos (<1k LOC), 3 mortos (0 LOC), 1 archived |
| **Mega-crates concentram 69% código** | hooks 152k, server 61k, learning 41k, cortex 32k, ast 23k |
| **Test-debt catastrófico** | cortex 0.56%, 8 crates com 0 tests |
| **No semver/MSRV foundation** | 0 `[workspace.dependencies]`, 0 `version.workspace` |
| **Duplicação intencional documentada** | touring-ast-polyglot DOC: "Extends touring-ast" |

## 2. Decision: 13 crates produtivos + 2 test-only

### Topologia em 6 layers

```
┌─────────────────────────────────────────────────────────────────────┐
│ LAYER 6 — PRODUCT  (touring-server, touring-hooks, touring-bindings)│
│   Binaries + CC interface + external API surface                    │
├─────────────────────────────────────────────────────────────────────┤
│ LAYER 5 — APPLICATION  (generator, assists, orchestration)          │
├─────────────────────────────────────────────────────────────────────┤
│ LAYER 4 — INTELLIGENCE  (touring-intelligence)                      │
│   Reasoning + RL + pipeline (mega-fusion — eliminates cycle 618)    │
├─────────────────────────────────────────────────────────────────────┤
│ LAYER 3 — DOMAIN CORE  (code, storage, analysis, offensive)         │
├─────────────────────────────────────────────────────────────────────┤
│ LAYER 2 — KERNEL  (simd, rkyv, identity)                            │
├─────────────────────────────────────────────────────────────────────┤
│ LAYER 1 — FOUNDATION  (touring-foundation)                          │
│   Zero deps in touring-*; configures everything                     │
└─────────────────────────────────────────────────────────────────────┘
```

## 3. Crate catalog (13 productive)

### Layer 1 — touring-foundation

- **Modules**: alloc, cgm, char_classes, checkpoint, chunker, config, conflict, diagnostic + 18 more
- **Public API**: Config, Diagnostic, Health, PluginRegistry, Profile, Schema, Sentinel, RuleEngine, ActivityLog, Telemetry
- **Features**: tracing-otel, tracing-jaeger, gpu-embeddings, mimalloc-allocator, sentinel-psi, rules-eval
- **Internal deps**: NONE (foundation)
- **LOC target**: 18,000 src / 4,000 test (22% ratio)
- **Pub target**: 250
- **MSRV**: 1.83
- **Absorves**: touring-core (rename+slim), touring-rule-engine, touring-definitions, touring-telemetry, touring-resource-monitor, touring-activity
- **Notes**: Zero deps in touring-*; configures everything. Layer 1.

### Layer 2 — touring-simd

- **Modules**: aco, cosine, gpu, u4_dot, quantize, bitvec, mask
- **Public API**: CosineComputer, BitVec, U4DotProduct, AcoPheromone
- **Features**: gpu-cuda, gpu-vulkan, gpu-metal, simd-avx2, simd-avx512, simd-neon
- **Internal deps**: touring-foundation
- **LOC target**: 9,000 src / 2,000 test (22% ratio)
- **Pub target**: 170
- **MSRV**: 1.83
- **Notes**: SIMD primitives + GPU u4_dot. Layer 2 Kernel.

### Layer 2 — touring-rkyv

- **Modules**: transport, wire, magic, dispatch
- **Public API**: WireFormat, MagicHeader, DispatchTable, IpcMessage
- **Features**: bincode-fallback, compression-zstd
- **Internal deps**: touring-foundation
- **LOC target**: 1,500 src / 600 test (40% ratio)
- **Pub target**: 40
- **MSRV**: 1.83
- **Notes**: Zero-copy IPC transport. Layer 2 Kernel.

### Layer 2 — touring-identity

- **Modules**: registry, schema, types, criterion, resolution
- **Public API**: EntityId, EntityKind, Criterion, Resolution, Registry, MatchKind
- **Features**: (none)
- **Internal deps**: touring-foundation
- **LOC target**: 2,000 src / 600 test (30% ratio)
- **Pub target**: 30
- **MSRV**: 1.83
- **Notes**: RFC-004 Entity Registry. Layer 2 Kernel.

### Layer 3 — touring-code

- **Modules**: parsers/tree_sitter, parsers/ast_grep, parsers/syn, languages, semantics, graph, format, complexity + 1 more
- **Public API**: Parser, Lang, Tier, Definition, Symbol, Visitor, Surgery, ComplexityMetrics, SemanticSearch
- **Features**: lang-rust, lang-typescript, lang-python, lang-go, lang-ruby, lang-java, lang-cpp, parser-tree-sitter, parser-ast-grep, parser-syn, semantic-search, incremental-salsa
- **Internal deps**: touring-foundation, touring-simd, touring-identity
- **LOC target**: 26,000 src / 6,000 test (23% ratio)
- **Pub target**: 280
- **MSRV**: 1.83
- **Absorves**: touring-ast, touring-ast-polyglot, touring-language, touring-semantics
- **Notes**: Code intelligence — tree-sitter + ast-grep + syn. Layer 3 Domain Core.

### Layer 3 — touring-storage

- **Modules**: fts, vec/sqlite, vec/qdrant, vec/in_memory, embeddings/candle, embeddings/fastembed, embeddings/voyage, vfs/mem + 4 more
- **Public API**: FtsIndex, VectorStore, EmbeddingProvider, Vfs, SalsaDb, HybridSearcher, Indexer
- **Features**: storage-fts, storage-vec-sqlite, storage-vec-qdrant, storage-vec-mem, storage-emb-candle, storage-emb-fastembed, storage-emb-voyage, storage-vfs-mem, storage-vfs-disk, storage-salsa
- **Internal deps**: touring-foundation
- **LOC target**: 10,000 src / 2,500 test (25% ratio)
- **Pub target**: 160
- **MSRV**: 1.83
- **Absorves**: touring-index, touring-vfs, touring-incremental-salsa, touring-vector-store, touring-embeddings, touring-search-fusion
- **Notes**: Unified storage layer. Layer 3 Domain Core.

### Layer 3 — touring-analysis

- **Modules**: blast_radius, quality, wiring, health, temporal, e2e, rules, knowledge + 4 more
- **Public API**: BlastRadius, QualityReport, WiringAuditor, HealthTracker, E2EHealth, KnowledgeBase
- **Features**: bench-iai, cache-moka, cache-dashmap, temporal-history
- **Internal deps**: touring-foundation, touring-code
- **LOC target**: 16,000 src / 4,000 test (25% ratio)
- **Pub target**: 300
- **MSRV**: 1.83
- **Notes**: Analysis — TDG, Halstead, MI, blast, wiring. Layer 3.

### Layer 3 — touring-offensive

- **Modules**: concolic, erickson, solver, vuln, bug_bounty
- **Public API**: ConcolicExecutor, EricksonAttack, SolverBackend, VulnReport
- **Features**: solver-z3, solver-cvc5, concolic-tracer, vuln-pattern-db
- **Internal deps**: touring-foundation, touring-code
- **LOC target**: 7,500 src / 2,000 test (26% ratio)
- **Pub target**: 110
- **MSRV**: 1.83
- **Notes**: Offensive security. Opt-in. Layer 3 Domain Core.

### Layer 4 — touring-intelligence

- **Modules**: reasoning, rl, pipeline, ann
- **Public API**: Reasoner, Mcts, BanditPolicy, RlAgent, Pipeline, FusionScorer, AnnIndex, Pensieve, Predictor
- **Features**: intel-reasoning, intel-rl, intel-pipeline, intel-mcts, intel-bandit, intel-aco, intel-ann, intel-clustering, intel-pensieve, intel-got, intel-dspy
- **Internal deps**: touring-foundation, touring-simd, touring-code, touring-storage, touring-analysis
- **LOC target**: 90,000 src / 18,000 test (20% ratio)
- **Pub target**: 1100
- **MSRV**: 1.83
- **Absorves**: touring-cognitive, touring-cortex, touring-learning, touring-antt
- **Notes**: Mega-fusion eliminating macrociclo of depth 618. Internal pub(crate) discipline; external façade is single. Layer 4.

### Layer 5 — touring-generator

- **Modules**: pipeline, kinds, vgp, render, speculate
- **Public API**: Generator, GeneratorKind, Plan, Verified, Rendered, Speculated, Committed
- **Features**: generator-rust, generator-python, generator-typescript, generator-tsx, vgp-strict
- **Internal deps**: touring-foundation, touring-code, touring-analysis, touring-intelligence, touring-identity, touring-rkyv
- **LOC target**: 13,000 src / 5,000 test (38% ratio)
- **Pub target**: 290
- **MSRV**: 1.83
- **Notes**: Typestate codegen pipeline, 36 kinds. Layer 5 Application.

### Layer 5 — touring-assists

- **Modules**: auto_wire, extract_function, inline_call, auto_import, generate_impl, merge_imports, change_visibility, add_missing_match_arms + 2 more
- **Public API**: AssistKind, AssistApplier, AssistResult
- **Features**: assist-rust, assist-typescript, assist-python
- **Internal deps**: touring-foundation, touring-code, touring-analysis
- **LOC target**: 2,500 src / 700 test (28% ratio)
- **Pub target**: 75
- **MSRV**: 1.83
- **Notes**: 10 assist handlers. Layer 5.

### Layer 5 — touring-orchestration

- **Modules**: flow, tasks, decompose, session, diary, devrc
- **Public API**: Flow, TaskGraph, Decompose, Session, Diary, DevRc
- **Features**: flow-dag, tasks-sqlite, decompose-mcts, session-persist
- **Internal deps**: touring-foundation, touring-identity
- **LOC target**: 3,500 src / 900 test (25% ratio)
- **Pub target**: 80
- **MSRV**: 1.83
- **Absorves**: touring-flow, touring-tasksfile, touring-devrc-adapter
- **Notes**: DAG + tasks + session/diary. Layer 5.

### Layer 6 — touring-server

- **Modules**: server-cli, server-tools, server-reasoning, server-session, server-telemetry, server-visual
- **Public API**: Server, Cli, ToolRegistry, Reasoner, SessionManager
- **Features**: tier-free, tier-standard, tier-premium, tier-enterprise
- **Internal deps**: ALL
- **LOC target**: 25,000 src / 6,000 test (24% ratio)
- **Pub target**: 600
- **MSRV**: 1.83
- **Notes**: Binary + CLI dispatch. 6 internal sub-crates, single external façade.

### Layer 6 — touring-hooks

- **Modules**: hooks-core, hooks-lifecycle, hooks-cli, hooks-tools, hooks-prediction, hooks-rl
- **Public API**: HookHandler, HookContext, HookRuntime, Hook
- **Features**: hooks-claude-code, hooks-mcp, hooks-prediction, hooks-rl, hooks-cortex
- **Internal deps**: ALL
- **LOC target**: 155,000 src / 32,000 test (20% ratio)
- **Pub target**: 1500
- **MSRV**: 1.83
- **Notes**: Claude Code interface. 6 internal sub-crates, single external façade.

### Layer 6 — touring-bindings

- **Modules**: bindings-python, bindings-wasm, bindings-capnp, bindings-web, bindings-desktop, bindings-postgis
- **Public API**: BindPython, BindWasm, BindCapnp, BindWeb, BindDesktop, BindPostgis
- **Features**: bind-python, bind-wasm, bind-capnp, bind-web, bind-desktop, bind-postgis
- **Internal deps**: ALL
- **LOC target**: 15,000 src / 3,500 test (23% ratio)
- **Pub target**: 280
- **MSRV**: 1.83
- **Absorves**: touring-python, touring-wasm, touring-capnp-server, touring-web, touring-web-server, touring-desktop-ui, touring-geopostgis
- **Notes**: Default features empty (opt-in). Layer 6.


## 4. Crates eliminated/merged

| Source crate | Disposition | Target |
|---|---|---|
| touring-semantic-spike (66L archived) | DELETE | — |
| touring-wasm-client (0L) | DELETE | — |
| touring-wasm-common (0L) | DELETE | — |
| touring-wasm-server (0L) | DELETE | — |
| touring-core (rename+slim) | RENAME | touring-foundation |
| touring-rule-engine (443L) | ABSORVE | touring-foundation/rules/ |
| touring-definitions (1.1k) | ABSORVE | touring-foundation/types/ |
| touring-telemetry (990L) | ABSORVE | touring-foundation/telemetry/ |
| touring-resource-monitor (2.4k) | ABSORVE | touring-foundation/sentinel/ |
| touring-activity (781L) | ABSORVE | touring-foundation/activity/ |
| touring-ast (23k) | FUSE | touring-code/parsers/tree_sitter/ |
| touring-ast-polyglot (769L) | FUSE | touring-code/parsers/ast_grep/ |
| touring-language (558L) | FUSE | touring-code/languages/ |
| touring-semantics (1072L) | FUSE | touring-code/semantics/ |
| touring-index (2.7k) | FUSE | touring-storage/fts/ |
| touring-vfs (1.6k) | FUSE | touring-storage/vfs/ |
| touring-incremental-salsa (387L) | FUSE | touring-storage/salsa/ |
| touring-vector-store (1.2k) | FUSE | touring-storage/vec/ |
| touring-embeddings (1.4k) | FUSE | touring-storage/embeddings/ |
| touring-search-fusion (1.5k) | FUSE | touring-storage/hybrid_search/ |
| touring-cognitive (15k) | FUSE | touring-intelligence/reasoning/ |
| touring-cortex (32k) | FUSE | touring-intelligence/pipeline/ |
| touring-learning (41k) | FUSE | touring-intelligence/rl/ |
| touring-antt (5.2k) | FUSE | touring-intelligence/ann/ |
| touring-flow (809L) | FUSE | touring-orchestration/flow/ |
| touring-tasksfile (1.2k) | FUSE | touring-orchestration/tasks/ |
| touring-devrc-adapter (591L) | FUSE | touring-orchestration/devrc/ |
| touring-python (3.5k) | FUSE | touring-bindings/bindings-python/ |
| touring-wasm (2.7k) | FUSE | touring-bindings/bindings-wasm/ |
| touring-capnp-server (1.5k) | FUSE | touring-bindings/bindings-capnp/ |
| touring-web (3.5k) | FUSE | touring-bindings/bindings-web/ |
| touring-web-server (1.7k) | FUSE | touring-bindings/bindings-web/ (merged) |
| touring-desktop-ui (1.2k) | FUSE | touring-bindings/bindings-desktop/ |
| touring-geopostgis (435L) | FUSE | touring-bindings/bindings-postgis/ |

**Net**: 46 → 13 productive + 2 test-only = **15 manifests (-67%)**.

## 5. Quality gates (non-negotiable per crate)

| Gate | Threshold | Verification |
|---|---|---|
| **Test ratio** | tests/src LOC ≥ 20% | cargo llvm-cov per crate |
| **Mutation kill rate** | ≥ 80% | cargo mutants per crate |
| **Documentation** | `#![warn(missing_docs)]` strict | cargo doc --warnings-as-errors |
| **API stability** | snapshot via cargo public-api | CI gate per PR |
| **SemVer** | cargo-semver-checks | CI gate before merge |
| **MSRV** | 1.83 LTS | cargo-msrv verify |
| **Lints** | `[workspace.lints]` strict | cargo clippy -- -D warnings |
| **Supply chain** | clean | cargo deny + audit + vet |
| **Performance** | Criterion baseline -5% budget | cargo bench regression CI |
| **No unsafe** | without `// SAFETY:` comment | grep gate |
| **No `unwrap()` em src/** | tests OK | clippy lint enforced |

## 6. References

- Forensic audit: memory `audit:touring-arch-premium-refactor-2026-05-11`
- Approved decisions: memory `decision:touring-premium-roadmap-2026-05-11`
- Baselines: `docs/baselines/`
- Sister docs: 02-DEPLOYMENT, 03-COMMERCIAL, 05-RISKS, 06-METRICS, 07-ROLLBACK
