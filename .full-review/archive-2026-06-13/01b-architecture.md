# Phase 1B — Architecture & Design Review

> **Target**: Touring Rust workspace (`~/.claude/rust`) — 46 crate members, 498,697 src LOC.
> **North star**: what blocks Touring from being a Premium, Elite-of-Market system/repository.
> **Method**: read-only, evidence-cited (`touring` CLI + `file:line` + Cargo.toml graph). Builds ON
> the 2026-06-04 diagnostic (modularization 3.5) and 2026-06-13 in-loco verification (composite 0.81).
> **Date**: 2026-06-13 | **Daemon**: doctor 7/8 healthy (wiring_diagnostic=warning), composite_health 0.69 this session.

---

## Verified ground truth (this session)

| Signal | Value | Evidence |
|---|---|---|
| Workspace cycles | **0** | `touring wiring cycles --min-depth 2` → `{"cycle_count":0,"cycles":[]}` |
| Crate members | 46 | `touring ast workspace-info` → `packages: 46` |
| Largest crate | touring-server 67,887 LOC (13.6%) | `find … -name '*.rs' \| wc -l` |
| 2nd largest | touring-intelligence 64,333 LOC (12.9%) | idem |
| `#[tool]` macros | **194** | `grep -rh '#\[tool' crates/touring-server/src/server/*.rs \| wc -l` |
| Shim crates (≤12 LOC) | 13 (incl. touring-learning, touring-cognitive, touring-ast, touring-antt, touring-wasm) | per-crate src LOC scan |
| IoC seam consumers | 2 (touring-dispatch, touring-ceg) | `grep -rl touring-contracts crates/*/Cargo.toml` |
| Highest fan-in | touring-foundation = 20 | per-crate reverse-dep scan |
| LlmProvider impls | **1 (NoopLlm only)** | `grep impl LlmProvider` → context.rs:2490 |

The decomposition is real and the SCC is genuinely clean — that is a strong, hard-won foundation.
The findings below are the **next layer of structural debt** that the monolith-split surfaced rather
than removed.

---

## Crate topology assessment

The 46 crates fall into coherent **domain bands**, which is good:

- **Foundation**: touring-foundation (21.7k, the shared kernel), touring-simd, touring-rkyv, touring-contracts (IoC leaf).
- **Code intelligence**: touring-ast(+polyglot), touring-code (26.6k), touring-analysis (14.7k), touring-lsp.
- **AI/learning**: touring-intelligence (64.3k), touring-cognitive(shim), touring-learning(shim).
- **Hooks plane (post-decomposition)**: touring-hooks(façade) → touring-dispatch (37.5k) → touring-hooks-core (31.8k) → touring-hook-handlers (26.2k) → touring-cli (25.6k) → touring-hook-runtime (18.5k) → touring-hooks-shared/-prediction/-rl/-saga.
- **Surfaces**: touring-server (67.9k), touring-server-{reasoning,visual,session}, touring-cli, touring-generator, touring-web(-server), touring-bindings (28.8k = python/wasm/capnp glue).
- **Security**: touring-ceg (17.6k, leaf), touring-offensive (10.2k).

The **band structure is sound**; the problems are (1) two crates that are now the de-facto monoliths,
(2) a permanent shim layer from an incomplete fusion, (3) a god-kernel, and (4) the documentation no
longer describes this reality at all.

---

## FINDINGS (ranked)

### A1 — `touring-server` is the next monolith (HIGH)

**Evidence**: 67,887 LOC = 13.6% of workspace, the single largest crate. Internally it is two
mega-modules glued in one crate:
- `src/cli/` = 2,520 KB across **89 files** (exec.rs 96K, generate.rs 76K, migrate.rs 56K, common.rs 52K, wiring/init/graph 40K each).
- `src/server/` = 1,536 KB (tools_infra.rs 100K, tools_analysis.rs 92K, tools_core.rs 80K, params.rs 76K, mod.rs 64K).

`src/lib.rs:1` self-describes as *"MCP Server (42 tools) + Cortex CLI (73 handlers) + Modular
Architecture"* — but it ships **194 `#[tool]` macros** and ~120 CLI subcommands in one compilation
unit. CLI dispatch and the MCP tool surface are two **independent public products** living in one
crate; they share almost nothing except `daemon_query`.

**Architectural impact**: slow incremental builds (any CLI edit recompiles all 194 MCP tools and
vice-versa); two release surfaces can't be versioned independently; it is the de-facto god-crate the
2026-06-04 diagnostic warned would replace touring-hooks.

**Recommendation**: split along the seam already visible in the directory tree —
`touring-cli-app` (the `cli/` tree + binary) and `touring-mcp` (the `server/` tool surface), leaving
`touring-server` as a thin orchestration shell or retiring it. This is the **single highest-leverage
structural move** toward elite (see Executive section). Rank #1 in "next decompositions".

---

### A2 — Permanent shim layer from an incomplete fusion (HIGH)

**Evidence**: 5 crates that ARCHITECTURE.md treats as live engines are now ≤12-LOC re-export shims,
yet are still on the critical path:

| Shim crate | src LOC | lib.rs | live consumers |
|---|---|---|---|
| touring-ast | 12 | `pub use` 1 | **15** |
| touring-learning | 9 | `pub use` 1 | **13** |
| touring-cognitive | 8 | `pub use` 1 | **13** |
| touring-antt | 9 | `pub use` 1 | 7 |
| touring-wasm | 6 | `pub use` 1 | 5 |

The real code moved into touring-intelligence / touring-code / touring-foundation (the "47→13
crates" residual plan, MEMORY.md). The fusion **stopped halfway**: the new crates exist AND the old
names persist as forwarding shims that 13–15 crates still depend on. So `touring-learning` (shim) and
`touring-intelligence` (real, 64.3k) coexist; `touring-ast` (shim) and the real AST code (in
touring-code) coexist.

**Architectural impact**: every reader/contributor must learn that "touring-learning" is a lie that
forwards to touring-intelligence. Fan-in numbers are doubled and misleading (the graph shows fan-in 13
on a 9-LOC shim). This is exactly the "accidental boundary" smell — the boundary is now a historical
artifact, not a domain line.

**Recommendation**: finish the migration — either (a) collapse the shims by rewriting the 13–15
consumers' Cargo.toml + imports to the real crate (mechanical, scriptable), then delete the shim
dirs; or (b) if the shim names are the intended public/stable names, make the REAL crate adopt the
name and delete the duplicate. Do not leave a permanent forwarding layer. Rank #2.

---

### A3 — Documentation describes an architecture that no longer exists (HIGH)

**Evidence**: `ARCHITECTURE.md` (the *stated* architecture, v30.3.6) lists **38 crates** at the
bottom (line 835: "Total | 38 crates | 476,728 LOC") and its crate map (lines 161–207) names
`touring-core`, `touring-index`, `touring-vfs`, `touring-semantics`, `touring-semantic-spike`,
`touring-definitions`, `touring-embeddings`, `touring-flow`, `touring-tasksfile`,
`touring-devrc-adapter`, `touring-search-fusion`, `touring-vector-store`, `touring-rule-engine`,
`touring-geopostgis`, `touring-desktop-ui` as live crates. **None of those directories exist** in the
real workspace. The real crates `touring-intelligence`, `touring-code`, `touring-foundation`,
`touring-bindings`, `touring-storage`, `touring-lsp`, `touring-license`, `touring-orchestration`,
`touring-contracts`, `touring-hooks-*` are **absent from the crate map**. It still says
`touring-hooks 127,575 LOC` (line 167/806) — the very monolith the masterplan claims to have
decomposed. The "Dependency Graph" (lines 333–353) and "3 Consolidated Databases" sections reflect
the pre-decomposition world.

This contradicts the scope's claim that the in-loco verification "closed 8/8 gaps incl. G-3
modules.md". The narrative `docs/explanation/architecture.md` may be current, but the **canonical
reference (ARCHITECTURE.md, which the file itself declares is the auto-synced source of truth via
`docs/sync_metrics.py`) is structurally stale** — only its top-line metrics (crates=46, LOC) are
synced; the crate map and dependency graph are not covered by the gate.

**Architectural impact**: for an aspiring open-source/elite repo, the headline architecture doc being
this wrong is a credibility-killer for external contributors (B-W1/B-W3 readiness). The 2026-06-04
diagnostic flagged exactly this (drift: ARCH 45/429k/5100 vs real). It was reported "fixed" but the
crate map is still pre-fusion.

**Recommendation**: extend `docs/sync_metrics.py --check` to also assert the crate-map table against
`touring ast workspace-info` (name set + LOC), failing CI on drift. Regenerate the crate map + the
dependency graph from the real graph. Rank: documentation-blocking for public release.

---

### A4 — `touring-foundation` is a god-kernel grab-bag (MEDIUM-HIGH)

**Evidence**: fan-in = **20** (highest in the workspace), 21,678 LOC, and its `src/` is a flat
collection of unrelated subsystems: `sentinel` (120K), `semantic` (72K), `embedding` (64K),
`telemetry` (52K), `schema` (52K), `failover` (52K), `conflict` (52K), `migration` (48K),
`activity` (48K), `shared` (44K), plus config/alloc/cgm/char_classes/checkpoint/chunker/diagnostic at
the root. It holds the DDL/SCHEMA_VERSION (`migration.rs:17 SCHEMA_VERSION=8`, `schema/{knowledge,
memory,graph}.rs`) AND four `*Provider` traits (PersistenceProvider, ProviderPlugin,
VectorStoreProvider in `failover/`).

**Architectural impact**: it is a "everything-depends-on-it" kernel by accretion, not by design. Any
change to embeddings, telemetry, or failover forces a recompile across 20 crates. It mixes pure
contracts (schema, config, error) with heavy logic (sentinel, embedding, conflict resolution). This
is the classic shared-kernel anti-pattern that erodes as the system grows.

**Recommendation**: peel the heavy, optional subsystems out (embedding → touring-storage which
already owns `embeddings/`; sentinel/failover/conflict → a `touring-resilience` crate) and keep
touring-foundation as a thin true-kernel (config, error, schema DDL, contracts only). The pure parts
are the legitimate kernel; the logic is not. Rank #3.

---

### A5 — Data layer is entangled into the hooks plane, not a storage crate (MEDIUM-HIGH)

**Evidence**: the architecture aspires to a clean data tier (knowledge.db, tantivy, rkyv, CRDT graph,
symbol store). Reality:
- `FileKnowledgeDB` (the knowledge.db owner, 4,456 LOC) is in **`touring-hooks-core/src/knowledge.rs:199`**.
- `tantivy_index.rs` is in **`touring-hooks-core`**.
- CRDT graph is in **`touring-intelligence/src/rl/memory/crdt_graph.rs`** (inside the *RL* tree).
- DDL/schema is in **`touring-foundation/src/schema/`**.
- A `touring-storage` crate exists (7.5k LOC) but only owns `embeddings`, `hybrid_search`, `salsa`,
  `vec`, `vfs` — **not** the primary knowledge/tantivy stores.
- `touring-hooks-core` pulls **14+ subsystem deps** (analysis, learning, cognitive, ast, simd, antt,
  ceg, orchestration) with many feature flags — so the "core" crate is a heavy aggregator, not a leaf.

So the three databases are owned by three different crates across three domains, and the canonical
data layer lives inside the hooks runtime crate. There is no single `touring-data`/`touring-kv`
boundary; persistence concerns are smeared across foundation, hooks-core, and intelligence.

**Architectural impact**: the data schema (SCHEMA_VERSION=8) and its runtime owner are in different
crates, so the migration story is split (DDL in foundation, `reindex_file` writers in hooks). A third
party cannot depend on "Touring's index" without pulling the entire hooks plane. CRDT graph being
under `rl/memory/` is a category error (it's a data structure, not an RL artifact).

**Recommendation**: define a `touring-storage` (or `touring-knowledge`) crate as the real home for
FileKnowledgeDB + tantivy_index + crdt_graph + the schema DDL, depending only on foundation. Hooks
should consume it, not own it. This also unlocks A1 (the MCP/CLI surfaces could depend on storage
directly without the hooks plane). Rank #4.

---

### A6 — `mcp-curated`/`mcp-legacy` are dead feature flags; the public surface is undisciplined (MEDIUM-HIGH)

**Evidence**: `touring-server/Cargo.toml:93-94` declares `mcp-legacy = []` and `mcp-curated = []` —
both gate **zero** blocks (empty feature lists, confirmed by the scope note "mcp-legacy gates 0
blocks"). The default build exposes **194 `#[tool]` macros** (measured), while docs claim ~164 and a
curated 22. The 102→22 reduction is *authored in docs* but **not wired**: flipping the flag changes
nothing because no `#[cfg(feature = "mcp-curated")]` guards exist on the tool definitions.

**Architectural impact**: the single most important "public API" of an AI code-intelligence engine —
its MCP tool surface — has **no versioning, no curation gate, and no stable contract**. 194 tools is
unmaintainable as a public API; an MCP client cannot know which tools are stable. This directly
blocks the "third-party could build on it" elite criterion.

**Recommendation**: make `mcp-curated` real (actually `#[cfg]`-gate the 22 curated tools), default it
ON for the published build, and treat the tool list + JSON param schemas as a semver-governed
contract (snapshot test on the tool manifest). Until then the feature flags are deceptive and should
be deleted rather than left as no-ops. Rank #5.

---

### A7 — IoC seam (touring-contracts) is correct but applied ad-hoc (MEDIUM)

**Evidence**: `touring-contracts/src/lib.rs` is a clean leaf (single dep serde_json) defining
`LearnRuntime` + `CegRuntime` to invert the gateway↔parent edge — genuinely good design, well
documented. But only **2 crates consume it** (touring-dispatch, touring-ceg). The same inversion
problem exists elsewhere: hooks-core depends *downward* on learning/cognitive/analysis directly
(14 concrete deps), and the LlmProvider/MemoryProvider traits live in `touring-generator`
(context.rs:2371/2434), not in a shared contracts crate.

**Architectural impact**: the dependency-inversion pattern was applied surgically to extract CEG, then
not generalized. The boundaries that would most benefit (hooks→intelligence, generator→llm) are still
hard-wired. Inconsistent application means the seam is a one-off, not an architectural principle.

**Recommendation**: promote the provider traits (LlmProvider, MemoryProvider, EmbeddingProvider) into
touring-contracts so they're a single, reusable IoC surface; apply the seam to the hooks→intelligence
boundary so the heavy AI crate can be swapped/tested in isolation. Rank #6.

---

### A8 — No real LLM provider; the system is structurally LLM-less (MEDIUM)

**Evidence**: `LlmProvider` trait exists (`touring-generator/src/core/context.rs:2371`) and is held
as `Arc<dyn LlmProvider>` (context.rs:2877), but the **only implementor is `NoopLlm`**
(context.rs:2490 `impl LlmProvider for NoopLlm`). The masterplan's B-W2 (LlmProvider) is in_progress
per MEMORY.md. So Touring today is a deterministic intelligence engine with an LLM-shaped hole and a
no-op plug.

**Architectural impact**: this is defensible as a design choice (Touring is the *substrate*, the LLM
is the harness/Claude Code) — but the trait sitting in `touring-generator` rather than contracts, with
only a noop, means the abstraction is unproven. The first real provider may force a re-shape. For
elite/market positioning, "bring your own LLM" is a likely requirement and the seam isn't ready.

**Recommendation**: move `LlmProvider` to touring-contracts (A7), add at least one real provider
behind a feature (even a thin reqwest-based one) to prove the abstraction, and document the
"Touring-is-substrate, LLM-is-pluggable" decision as an ADR so the noop is intentional, not a gap.
Rank #7.

---

### A9 — Design-pattern hygiene: typestate/actor are good; some over-engineering risk (LOW-MEDIUM)

**Evidence**: the CEG X0–X9 typestate pipeline (`touring-ceg`, now a clean leaf with deps
foundation/contracts/hooks-shared/offensive/ast-polyglot) and the daemon actor pattern
(one OS thread per project, mpsc+oneshot — ARCHITECTURE.md §Concurrency) are appropriate, well-bounded
patterns. The dispatch table (touring-dispatch 37.5k) is a reasonable hub. However: `touring-intelligence`
houses **10 RL sub-domains under `src/rl/`** (aco 260K, memory 256K, bandit 196K, rl/rl 192K, n1 120K,
n3 108K, semantic, evolution, ranking, clustering, meta, metacognitive_pipeline) — a 64.3k-LOC crate
that is itself a mini-monolith of competing learning paradigms. The neuro-metaphor layering
(Arcuate Fasciculus, Pheromone MCTS, IC-1↔IC-4 loops) adds conceptual surface that may exceed the
value delivered.

**Architectural impact**: touring-intelligence is the 2nd-largest crate and a candidate next-monolith
after touring-server. The breadth of RL machinery (ACO + LinUCB + QTable + HNSW + MCTS + GoT + TD(λ))
is a maintainability and testability liability if not all paths carry their weight.

**Recommendation**: audit touring-intelligence/src/rl/* for actually-wired vs experimental paradigms;
consider extracting touring-rl-memory (the 256K memory subtree incl. crdt_graph from A5) as a
boundary. Don't pre-emptively split — measure consumer fan-in per sub-domain first. Rank #8.

---

### A10 — Evolvability & contributor-readiness gaps (MEDIUM, cross-cutting)

**Evidence**: weak workspace lint policy (scope: only 4 `deny` + 4 `forbid` crate-roots of 46);
~3,686 non-test `.unwrap()`; the stale ARCHITECTURE.md (A3); no `touring-sdk` crate despite RFC-006
aspiration (`ls crates/touring-sdk` → absent); the public surfaces (CLI/MCP/hooks) have no single
documented, semver-stable entry point. `docs/sync_metrics.py` proves the team values metrics-as-code
but the gate covers only counts, not structure.

**Architectural impact**: the codebase is internally consistent enough to evolve *for the current
authors*, but not yet legible to external contributors. "Does it follow its own ARCHITECTURE.md?" —
**no**, the doc is pre-fusion. That is the gating issue for B-W1/B-W3 public release.

**Recommendation**: (1) workspace-level `[workspace.lints]` with deny-all clippy + a documented
`unwrap` budget; (2) make ARCHITECTURE.md structurally self-checking (A3); (3) introduce the
`touring-sdk` facade crate as the one stable public boundary (re-exporting curated MCP tools + a typed
client) so RFC-006 stops being aspirational.

---

## Next decompositions — ranked

| Rank | Move | Crate | LOC | Why | Effort |
|---|---|---|---|---|---|
| 1 | Split `cli/` ↔ `server/` (MCP) | touring-server | 67.9k | Two independent products in one unit; #1 monolith | M (dirs already split) |
| 2 | Finish the fusion: collapse shims | touring-{ast,learning,cognitive,antt,wasm} | ~44 LOC shims, 13–15 consumers each | Eliminate permanent forwarding layer / double naming | S–M (mechanical) |
| 3 | Peel god-kernel | touring-foundation | 21.7k, fan-in 20 | Embedding/sentinel/failover ≠ kernel | M |
| 4 | Real data tier | new touring-storage home | knowledge.rs 4.5k + tantivy + crdt | Persistence smeared across 3 crates | M–L |
| 5 | Wire mcp-curated for real | touring-server | 194 tools | Public API has no contract | S–M |
| 6 | Audit RL sub-monolith | touring-intelligence | 64.3k | 2nd next-monolith | L (measure first) |

---

## Severity counts

- **HIGH**: 3 (A1 touring-server monolith, A2 shim layer, A3 stale ARCHITECTURE.md)
- **MEDIUM-HIGH**: 3 (A4 god-kernel, A5 data entanglement, A6 dead mcp feature flags)
- **MEDIUM**: 3 (A7 ad-hoc IoC, A8 noop LLM, A10 evolvability)
- **LOW-MEDIUM**: 1 (A9 pattern hygiene)

## What is genuinely elite already (do not regress)

- SCC truly clean (cycles=0, Tarjan-verified) after a 169k→6-crate decomposition with 3,154 tests zero-loss.
- The IoC seam (touring-contracts) is textbook dependency inversion — the *pattern* is right, just under-applied.
- CEG X0–X9 typestate + daemon actor model are appropriate, well-bounded designs.
- Metrics-as-code discipline (`docs/sync_metrics.py --check`) exists — the muscle for A3 is already there.
- Domain band structure (intelligence/code/foundation/orchestration/surfaces/security) is coherent.
