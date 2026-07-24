# ADR-001 — Touring Premium Architecture Vision

> **Status**: Proposed | **Date**: 2026-05-11 | **Authors**: Gabriel Gadea (architect) + TACO (orchestrator)
> **Supersedes**: nothing (greenfield architectural redesign)
> **Relates to**: ADR-002 (Per-Project Deployment), ADR-003 (Commercial Tiers + GTM), MASTER-PLAN-2026

## 1. Context

Touring grew organically from 0 → 46 crates / ~410k LOC over 18 months. The current
workspace shows multiple severe symptoms diagnosed in the 2026-05-11 forensic audit
(memory: `audit:touring-arch-premium-refactor-2026-05-11`):

| Symptom | Evidence |
|---|---|
| **Macrociclo arquitetural HIGH severity** | `touring wiring cycles` reports depth=618 cycle spanning 9 crates (server↔hooks↔analysis↔cognitive↔learning↔ast↔wasm↔inferlets↔resource-monitor) |
| **Fragmentação excessiva** | 46 crates, 6 anêmicos (<1k LOC), 3 mortos (0 LOC), 1 archived spike |
| **Mega-crates concentram 69% do código** | hooks 152k, server 61k, learning 41k, cortex 32k, ast 23k |
| **Test-debt catastrófico** | cortex 0.56% ratio, 8 crates com 0 tests |
| **No semver/MSRV foundation** | 0 `[workspace.dependencies]`, 0 `version.workspace = true` |
| **Duplicação intencional documentada** | touring-ast-polyglot DOC: "Extends touring-ast" |

The decision: **transform Touring into a premium-grade product** where the architecture
itself demonstrates the quality bar the product delivers. Reduce 46 → 13 productive
crates via deliberate fusion + internal split, with modular Cargo features.

## 2. Decision

**Target topology: 13 productive crates + 2 test-only manifests, organized in 6 strict layers.**

### Layer architecture

```
┌─────────────────────────────────────────────────────────────────────┐
│ LAYER 6 — PRODUCT  (touring-server, touring-hooks, touring-bindings)│
│   Binaries + CC interface + external API surface                    │
├─────────────────────────────────────────────────────────────────────┤
│ LAYER 5 — APPLICATION  (generator, assists, orchestration)          │
│   User-facing workflows                                             │
├─────────────────────────────────────────────────────────────────────┤
│ LAYER 4 — INTELLIGENCE  (touring-intelligence)                      │
│   Reasoning + RL + pipeline (mega-fusion to eliminate cycle)        │
├─────────────────────────────────────────────────────────────────────┤
│ LAYER 3 — DOMAIN CORE  (code, storage, analysis, offensive)         │
│   Code intelligence + storage + analysis + security                 │
├─────────────────────────────────────────────────────────────────────┤
│ LAYER 2 — KERNEL  (simd, rkyv, identity)                            │
│   Primitives without policy                                         │
├─────────────────────────────────────────────────────────────────────┤
│ LAYER 1 — FOUNDATION  (touring-foundation)                          │
│   Zero deps in touring-*; configures everything                     │
└─────────────────────────────────────────────────────────────────────┘
```

### Crate catalog (13 productive + 2 test-only)

#### Layer 1 — Foundation

| Crate | Modules | Features | LOC src / test | MSRV |
|---|---|---|---|---|
| **touring-foundation** | alloc, cgm, char_classes, checkpoint, chunker, config, conflict, diagnostic, drift, failover, feedback, governor, hash, health, migration, plugin, profile, schema, security, shared, shutdown, telemetry, sentinel, rules, definitions, activity | tracing-otel, tracing-jaeger, gpu-embeddings, mimalloc-allocator, sentinel-psi, rules-eval | 18k / 4k (22%) | 1.83 |

Absorves: `touring-core` (rename + slim), `touring-rule-engine`, `touring-definitions`,
`touring-telemetry`, `touring-resource-monitor`, `touring-activity`.

#### Layer 2 — Kernel

| Crate | Modules | Features | LOC src / test |
|---|---|---|---|
| **touring-simd** | aco, cosine, gpu, u4_dot, quantize, bitvec, mask | gpu-cuda, gpu-vulkan, gpu-metal, simd-avx2, simd-avx512, simd-neon | 9k / 2k (22%) |
| **touring-rkyv** | transport, wire, magic, dispatch | bincode-fallback, compression-zstd | 1.5k / 600 (40%) |
| **touring-identity** | registry, schema, types, criterion, resolution | (none) | 2k / 600 (30%) |

#### Layer 3 — Domain Core

| Crate | Modules | Features | LOC src / test |
|---|---|---|---|
| **touring-code** | parsers/{tree_sitter,ast_grep,syn}, languages, semantics, graph, format, complexity, incremental | lang-{rust,typescript,python,go,ruby,java,cpp}, parser-{tree-sitter,ast-grep,syn}, semantic-search, incremental-salsa | 26k / 6k (23%) |
| **touring-storage** | fts (Tantivy), vec/{sqlite,qdrant,in_memory}, embeddings/{candle,fastembed,voyage}, vfs/{mem,disk}, salsa, hybrid_search, indexer | storage-{fts,vec-sqlite,vec-qdrant,vec-mem,emb-candle,emb-fastembed,emb-voyage,vfs-mem,vfs-disk,salsa} | 10k / 2.5k (25%) |
| **touring-analysis** | blast_radius, quality (TDG, Halstead, MI), wiring, health, temporal, e2e, rules, knowledge, security, cache, report, pipeline | bench-iai, cache-moka, cache-dashmap, temporal-history | 16k / 4k (25%) |
| **touring-offensive** | concolic, erickson, solver, vuln, bug_bounty | solver-z3, solver-cvc5, concolic-tracer, vuln-pattern-db | 7.5k / 2k (26%) |

Code absorves: `touring-ast`, `touring-ast-polyglot`, `touring-language`, `touring-semantics`.
Storage absorves: `touring-index`, `touring-vfs`, `touring-incremental-salsa`, `touring-vector-store`, `touring-embeddings`, `touring-search-fusion`.

#### Layer 4 — Intelligence

| Crate | Modules | Features | LOC src / test |
|---|---|---|---|
| **touring-intelligence** | reasoning (ACO, ANN, BM25, MCTS, GoT, Pensieve), rl (bandit, ACO, clustering, online_rl, ranking), pipeline (handler, fusion, scoring, fascicles, cross_audit, DSPy), ann | intel-{reasoning,rl,pipeline,mcts,bandit,aco,ann,clustering,pensieve,got,dspy} | 90k / 18k (20%) |

Absorves: `touring-cognitive`, `touring-cortex`, `touring-learning`, `touring-antt`.
**This fusion eliminates the depth-618 macrociclo** by collapsing the cyclical
reasoning↔learning↔pipeline dependencies into a single crate with internal pub(crate)
discipline.

#### Layer 5 — Application

| Crate | Modules | Features | LOC src / test |
|---|---|---|---|
| **touring-generator** | pipeline (Draft→Verified→Rendered→Speculated→Committed), kinds (36), vgp, render, speculate | generator-{rust,python,typescript,tsx}, vgp-strict | 13k / 5k (38%) |
| **touring-assists** | 10 handlers (auto_wire, extract_function, inline_call, auto_import, generate_impl, merge_imports, change_visibility, add_missing_match_arms, move_module_to_file, convert_to_guarded_return) | assist-{rust,typescript,python} | 2.5k / 700 (28%) |
| **touring-orchestration** | flow, tasks, decompose, session, diary, devrc | flow-dag, tasks-sqlite, decompose-mcts, session-persist | 3.5k / 900 (25%) |

Orchestration absorves: `touring-flow`, `touring-tasksfile`, `touring-devrc-adapter`.

#### Layer 6 — Product (each with internal sub-crates for modularity)

| Crate | Internal sub-crates | Features | LOC src / test |
|---|---|---|---|
| **touring-server** | server-cli, server-tools, server-reasoning, server-session, server-telemetry, server-visual | tier-{free,standard,premium,enterprise} | 25k / 6k (24%) |
| **touring-hooks** | hooks-core, hooks-lifecycle, hooks-cli, hooks-tools, hooks-prediction, hooks-rl | hooks-{claude-code,mcp,prediction,rl,cortex} | 155k / 32k (20%) |
| **touring-bindings** | bindings-{python, wasm, capnp, web, desktop, postgis} | bind-{python,wasm,capnp,web,desktop,postgis} (default = empty) | 15k / 3.5k (23%) |

Bindings absorves: `touring-python`, `touring-wasm`, `touring-capnp-server`,
`touring-web`, `touring-web-server`, `touring-desktop-ui`, `touring-geopostgis`.
**Deletes 3 dead crates**: `touring-wasm-{client,common,server}` (0 LOC each).

#### Test-only (preserved)

- `touring-loom-proofs` (concurrency proofs, isolated workspace)
- `touring-integration-tests` (cross-crate E2E)

### Crates removed (5 immediate dead-code purge)

1. `touring-semantic-spike` (66 LOC, 0 pub, archived per ARCHITECTURE.md)
2. `touring-wasm-client` (0 LOC)
3. `touring-wasm-common` (0 LOC)
4. `touring-wasm-server` (0 LOC)

**Net manifest reduction: 46 → 15 = -67%.**

## 3. Quality Gates (non-negotiable per crate)

Every crate in the new topology MUST meet:

| Gate | Threshold | Verification |
|---|---|---|
| **Test ratio** | tests LOC / src LOC ≥ 20% | `cargo llvm-cov` per crate |
| **Mutation kill rate** | ≥ 80% | `cargo mutants` per crate |
| **Documentation** | `#![warn(missing_docs)]` strict | `cargo doc --warnings-as-errors` |
| **API stability** | snapshot via `cargo public-api` | CI gate per PR |
| **SemVer** | `cargo-semver-checks` | CI gate before merge |
| **MSRV** | 1.83 LTS | `cargo-msrv verify` |
| **Lints** | `[workspace.lints]` strict, deny warnings | `cargo clippy -- -D warnings` |
| **Supply chain** | clean | `cargo deny check` + `cargo audit` + `cargo vet` |
| **Performance** | Criterion baseline preserved (-5% budget) | `cargo bench` regression CI |
| **No unsafe without justification** | `// SAFETY:` comment + audit | grep gate |
| **No `unwrap()` in src/** | use `?` / `.expect()` / `.unwrap_or_default()` | clippy lint enforced |

## 4. Consequences

### Positive
- **Architecture lisible**: 6 strict layers; any new contributor understands the topology in 1 hour
- **Zero cycles**: `touring wiring cycles --min-depth 2` returns 0 after refactor
- **Builds faster cold**: fewer manifests, less Cargo work; estimated 30% faster on dev machine
- **Features composable**: users opt in to exactly what they need (`tier-free` is ~30% of binary size of `tier-enterprise`)
- **Onboarding faster**: 15 manifests vs 46; new hires productive in days not weeks
- **Marketing-ready**: clean topology becomes a sellable narrative

### Negative
- **Massive refactor effort**: ~138-182 engineer-days (see MASTER-PLAN-2026 for breakdown)
- **API churn for downstream**: every consumer of old crates must update imports (touring-ast → touring-code::ast, etc.)
- **Test debt repayment** required before fusion (W6.0 cortex 0.56% → 15% precondition)
- **W6 mega-fusion (90k LOC)** is single largest risk; mitigated by W6.0 pre-test gate

### Risks
- **Build time of 90k-LOC touring-intelligence** may degrade dev iteration. Mitigation: profile.dev `incremental=false` + sccache + split-debuginfo (already in REGRA #12)
- **Reexport shims** during W4 transition may persist beyond intended sunset. Mitigation: feature-flagged deprecations with clear sunset date in CHANGELOG
- **Hook split (W8)** may introduce new internal cycles between sub-crates. Mitigation: `cargo-depgraph` CI gate validates acyclic

## 5. References

- Forensic audit: memory `audit:touring-arch-premium-refactor-2026-05-11`
- Approved decisions: memory `decision:touring-premium-roadmap-2026-05-11`
- Baselines: `docs/baselines/` (wiring, cycles, status, workspace-info, snapshot)
- Companion ADRs: ADR-002 (deployment), ADR-003 (commercial)
- Execution plan: `docs/W0/MASTER-PLAN-2026.md`
