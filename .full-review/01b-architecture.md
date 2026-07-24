# 01b — Architecture Review (F1.7–F1.12)

> Premium-Elite Diagnostic · Touring Workspace `/home/gabrielgadea/.claude/rust`
> Run 2026-06-20 · Read-only · Every finding cites `Cargo.toml`/`file:line`/literal CLI output. Zero invented symbols.
> Baseline: `.full-review/00-scope.md` (44 members + `benches`; 50 crate dirs on disk; 544,590 LOC src; **0 cycles**).

---

## Executive Verdict

The dependency graph is **genuinely elite at the macro level**: 0 cycles (Tarjan SCC, verified), a true zero-dependency kernel (`touring-foundation`), correctly-directed layering with no inversions (verified via `cargo metadata`, not grep), a real typestate pipeline, and a type-driven storage data model. The A2/A5 fusion + "move-utils-down" playbook are legitimate strengths and the workspace lints are best-in-class.

The architecture debt is **micro-level and mechanical**, not structural: (1) a 2-error `cargo-deny bans` failure with a 1-line root cause, (2) ~5 dead-on-disk artifacts (shim dirs + an orphaned 3,468-LOC file) awaiting `git rm`, (3) a self-flagged ARCHITECTURE.md drift fixable by one command, (4) a `touring-server` mega-crate (70.9k LOC) that is two concerns (CLI + MCP) with a *partially extracted* seam, and (5) an inflated orphan count whose true dead-code fraction is **~7%, not 100%**.

Severity tally: **0 Critical · 3 High · 6 Medium · 4 Low.**

---

## Crate-Layering Diagram (verified via `cargo metadata --no-deps`, normal edges only)

```
LAYER 0 — KERNEL (zero touring deps)
  touring-foundation         deps: []          ← TRUE KERNEL (34 pub mods)
  touring-license, touring-simd, touring-rkyv, touring-identity, touring-contracts (leaves)

LAYER 1 — STORAGE / PRIMITIVES
  touring-storage            → foundation
  touring-offensive, touring-resilience, touring-ceg (gateway leaf)

LAYER 2 — CODE / ANALYSIS
  touring-code               → foundation, simd, storage
  touring-analysis           → code, foundation, offensive, simd
  touring-intelligence       → analysis, code, foundation, offensive, rkyv, simd

LAYER 3 — ORCHESTRATION / HOOKS
  touring-hooks-shared       → analysis, ast-polyglot, code, foundation, intelligence
  touring-generator, touring-orchestration, touring-cortex, touring-bindings
  touring-dispatch  (SCC anchor: cli + hooks family)
  touring-hooks  → dispatch (1.1k façade)

LAYER 4 — APPLICATION (top)
  touring-server  → analysis, assists, ast-polyglot, bindings, code, cortex,
                    foundation, generator, hooks, identity, intelligence,
                    orchestration, rkyv, server-{reasoning,session,visual},
                    simd, storage   ← also the `touring` BINARY
  touring-cli, touring-web, touring-web-server, touring-lsp, touring-python
```

**Rule check (foundation < storage/intelligence < code < server):** ✅ holds. `foundation` deps = `[]`; `storage → [foundation]`; `code → [foundation, simd, storage]`; `intelligence → [analysis, code, foundation, …]`; `server` sits on top. **No inversion.** (A naive grep of `Cargo.toml` *appears* to show `foundation → server/storage/telemetry`, but those are **comment lines** documenting W3.x absorptions; `cargo metadata` confirms the real edge set is `[]`.)

---

## F1.8 — Dependency Management

### ✅ Cycles: 0 (verified)
```
$ touring wiring cycles --min-depth 2 --format json
{"cycle_count":0,"cycles":[]}
```
Genuine Tarjan-SCC acyclicity. The "depth-683 cycle" noted in older diagnostics is gone. **Credit: elite.**

### 🔴 HIGH — `cargo-deny bans` fails: 2 `error[duplicate]` (precise 1-line root cause)
```
$ cargo deny check bans
error[duplicate]: found 2 duplicate entries for crate 'schemars'
error[duplicate]: found 2 duplicate entries for crate 'schemars_derive'
```
Reverse-dependency trace pinpoints the single odd-version-out:
```
$ cargo tree -i schemars@0.8.22
schemars v0.8.22
└── touring-harness-mcp                          ← direct `schemars = "0.8"` declaration

$ cargo tree -i schemars@1.2.1
schemars v1.2.1   ← workspace canonical
├── rmcp 1.2.0 → {touring-harness-mcp, touring-server}
├── touring-generator → {assists, integration-tests, server}
└── touring-server
```
- **Root cause:** `crates/touring-harness-mcp/Cargo.toml:21` declares `schemars = "0.8"` directly, while the workspace canonical (`Cargo.toml:257 → schemars = { version = "1.2", … }`) and `rmcp 1.2.0` both resolve to `1.2.1`. `schemars_derive` duplicates only because it follows its parent.
- **`harness-mcp` already depends on `rmcp` with the `schemars` feature** (`Cargo.toml:20`), so it gets `schemars 1.x` transitively regardless — the direct `0.8` pin is pure cruft.
- **Real fix (1 line):** change `crates/touring-harness-mcp/Cargo.toml:21` from `schemars = "0.8"` to `schemars = { workspace = true }`. Then re-run `cargo tree --workspace --duplicates --edges normal` to confirm `schemars`/`schemars_derive` collapse to single versions. This **removes the bans error entirely** — no `deny.toml` skip-entry needed (the skip list is the wrong tool here; this is a fixable convergence, not unavoidable churn).
- **Note:** the baseline's `image`/`tiff` item is a **warning, not an error**. The hard `bans` failures are *only* `schemars`/`schemars_derive` (2 errors). `tiff v0.11.3 → image v0.25.10 → fastembed v5.13.4 → touring-storage` is a transitive ML-toolchain dep; it surfaces under `external-default-features = "warn"`, not as `error[duplicate]`. It belongs in the curated `deny.toml` skip list if it ever duplicates, but today it does not fail bans.

**Architectural impact:** medium — bans is a CI gate; a red gate erodes the "Diamond" narrative. **Recommendation:** apply the 1-line harness-mcp fix; keep `multiple-versions = "deny"` (it is correctly strict). Severity High only because it is a **currently-failing gate** (REGRA #21).

### 🟡 MEDIUM — `deny.toml` skip-list is large (>60 entries) and one root is stale
```
warning[unmatched-skip-root]: deny.toml:206 windows-sys — no crate matched
```
The skip list (auto-refreshed W13 2026-05-31) is a legitimate pattern for an ML-heavy graph (gemm/cranelift/nalgebra churn), but it carries at least one dead entry (`windows-sys`, Win-only lane). **Recommendation:** re-run `cargo tree --workspace --duplicates`, prune unmatched roots, document the refresh cadence. Low effort, keeps the gate honest.

---

## F1.7 — Component Boundaries

### Kernel surface is genuine, not bloat
`touring-foundation` has **zero touring deps** and 34 `pub mod`s (`alloc, config, diagnostic, drift, error, gate_metrics, governor, health, knowledge_source, query_cache, schema, security, telemetry, activity, rules, semantic, …`). Spot-checks confirm cross-crate consumption: `EventId` (12 refs), `EntityActivity` (6 refs), `knowledge_source::KnowledgeSource` trait is the move-utils-down abstraction that dissolved the A5 cycle. **This is a real kernel, not a god-module dumping ground. Credit.**

### 🟡 MEDIUM — Orphan count (4,823) is **inflated**; true dead-code ≈7%
`touring wiring orphans` reports `orphan_count: 4823`, all `visibility: public`. A statistically-grounded triage (random sample n=60 of real-file orphans, word-boundary grep across `crates/*/src`):

| Class | Sample % | Extrapolated (of 4,766 real-file) | Meaning |
|---|---|---|---|
| **(a2) intra-crate-used `pub`** (over-broad visibility, ≥2 refs in home crate, 0 cross-crate) | **48.3%** | ~2,300 | NOT dead — candidate `pub` → `pub(crate)` |
| **(b) cross-crate API** (referenced by ≥1 *other* crate) | **45.0%** | ~2,150 | **False orphan** — the intra-crate detector can't see cross-crate consumers |
| **(a1) genuinely unreferenced** (def-only, 1 ref = the definition) | **6.7%** | **~320** | Real dead code / REGRA #0 targets |

Verified dead samples (def-only, zero call sites):
- `touring-ceg/src/gateway/txn.rs:184 apply_isolation_mode`
- `touring-simd/src/simd_utils/matrix.rs:346 batch_outer_product`
- `touring-bindings/src/web/cli.rs:112 fetch_viz_wiring_json`
- `touring-hooks-core/src/cortex_dispatcher.rs:484 drain_drift_reports`

**Do NOT report "4,823 orphans" as alarm.** ~45% are cross-crate public API (the detector is intra-crate by design), ~48% are intra-crate-used pub symbols (a *visibility-tightening* opportunity, not dead code), and only **~7% (~320 symbols) are genuinely dead.** Per-crate concentration (intelligence 791, server 584, bindings 426, foundation 398) tracks crate size, not rot.

**Recommendations (tiered):**
1. **Real fix:** triage the ~320 def-only symbols → wire (REGRA #0) or remove. Start with the 4 verified above.
2. **Hygiene:** down-grade the ~2,300 intra-crate-used `pub` to `pub(crate)` (D07 — shrinks the contract surface, lets the compiler catch real future orphans). This is the single highest-leverage F1.7 action.
3. **Tooling:** make the orphan detector cross-crate-aware (or tag cross-crate-consumed symbols) so the headline number stops crying wolf — a `cargo-deny`-style false-positive tax on every review.

### 🟡 MEDIUM — Stale wiring DB: 57 phantom-path orphans (confirmed)
57 of 4,823 orphans (1.2%) point at files/dirs that **no longer exist** — crates absorbed in W3.x but never re-indexed:
```
31  crates/touring-resource-monitor/...   (absorbed → touring-resilience, A4 P3)
17  crates/touring-telemetry/...          (absorbed → touring-foundation::telemetry, W3.6)
 9  crates/touring-rule-engine/...        (absorbed → touring-foundation::rules)
```
None of these dirs exist on disk. **Data-integrity verdict:** the inflation is **small (1.2%)** — the wiring DB is mostly accurate; the 4,766 real-file entries dominate. But the phantom entries prove the orphan/wiring DB is **not invalidated on crate-absorption**, so it accrues stale rows over fusion waves. **Recommendation:** `touring index rebuild $PWD` to flush phantoms; longer-term, hook absorption into a DB-invalidation step.

---

## F1.9 — API Design

### 🟡 MEDIUM — 231 `Result<_, String>` remain (RBP-03 incomplete)
Typed-error adoption is broad (**99 files use `thiserror::Error`/`derive(Error)`**; storage/server/cortex have proper enums). But 231 `Result<_, String>` survive in non-test code, concentrated in the binding/hook layers:
```
51  touring-bindings        21  touring-server         7  touring-hooks-core
42  touring-hook-runtime    14  touring-intelligence   7  touring-hooks
29  touring-hook-handlers   12  touring-generator      6  touring-{storage,foundation,cortex,server-reasoning}
```
Per RBP-03 doctrine, only `Result<_, String>` on **public APIs whose error is observed by a consumer** must convert. The bindings/hook-runtime concentration suggests these are next targets. **Recommendation:** continue RBP-03 crate-by-crate (bindings → hook-runtime → hook-handlers), gating each with `clippy --workspace --all-targets`.

### 🟡 MEDIUM — Semver governance gap: publish policy is inverted
Only **2 of 49** crates set `publish = false`. The other 47 are publishable-by-default yet wired with intra-workspace `path` deps and unpublished members (`touring-loom-proofs`, `inferlets`, etc.) → `cargo publish` would fail mid-graph. There is **no single semver-governable public API surface**; the workspace is a private mono-binary (`touring`) whose libraries are de-facto internal.
- `workspace.package` *is* configured for publishing (`version=0.1.0`, `license="MIT OR Apache-2.0"`, `repository`, `homepage`, `documentation="https://docs.rs/touring"`, `keywords`) — the *intent* to publish exists.
- **Recommendation:** decide the model explicitly. Either (a) mark internal crates `publish = false` and designate ONE façade crate (likely `touring-server` or a thin `touring` crate) as the published API, or (b) keep it private and drop the docs.rs/crates.io metadata to avoid implying a contract that isn't governed. Today it is ambiguous (`description`/`keywords`/`docs.rs` imply public; `path` deps + 2/49 publish-false imply private).

### ✅ Builder / From / Into ergonomics present
`ActivityAppendParams::new().with_action(…).with_actor(…).with_payload(…)` (server lib.rs:60+), `AgentDiaryBuilder`, generator `PlanExecutor` builder chain. Idiomatic. Credit.

---

## F1.10 — Data Model

### ✅ Type-driven invariants in storage are genuinely elite
`touring-storage` models domain in types, not primitives:
- **Newtype:** `vfs/projector.rs:13 pub struct SymbolId(u32)` — prevents mixing raw ids.
- **Domain enums (make-illegal-states-unrepresentable):** `ChainType` (functional_wiring.rs:98), `QueryIntent` (hybrid_search/intent.rs:11), `DistanceMetric` (vec/mod.rs:51), `EmbeddingModel` (embeddings/mod.rs:32), `Change` (projector.rs:53), `ConnectionState` (embedding/client.rs:74).
- **Per-module typed errors:** `VectorStoreError`, `PathError`, `ProjectorError`, `EmbeddingError`.
- **Kernel records:** `foundation/src/knowledge_source.rs` exposes `FileRelation`, `BashOutcomeRecord`, `CoEditPair`, `GotchaRecord`, `EditRecord`, `FileRisk` + the `KnowledgeSource: Send + Sync` trait — a clean port the storage layer implements. **D10: best-in-class.**

### 🟡 MEDIUM — `embedding/` vs `embeddings/` fragmentation (post-W5-fusion artifact)
`touring-storage/src/lib.rs:44-45` declares **both** `pub mod embedding;` AND `pub mod embeddings;`:
```
embedding/   → client.rs, mod.rs                       (2 files; EmbeddingError at client.rs:867)
embeddings/  → adapter.rs, error.rs, family.rs, mod.rs, providers/   (richer, canonical)
```
**Two `EmbeddingError` enums coexist** (`embedding/client.rs:867` and `embeddings/error.rs:7`). This is a naming-collision / duplicate-concept artifact from the W5 search-fusion (`touring-{search-fusion,vector-store,embeddings,vfs}` → `touring-storage`). **Recommendation:** consolidate `embedding/` into `embeddings/` (the latter is the fuller impl with adapter/family/providers), collapse the two error enums into one. Eliminates a "which one do I use?" hazard.

---

## F1.11 — Design Patterns

### ✅ Typestate pipeline is real and idiomatic — credit
The generator exports a genuine typestate lifecycle:
```rust
// touring-generator/src/lib.rs:59
pub use executor::typestate::{Committed, Draft, PlanExecutor, Rendered, Speculated, Verified};
```
plus a typestate **circuit-breaker** for replanning (`executor/replan.rs:1 ReplanRequest/RejectedPlan`) and shape-overflow typestate (`shape.rs:4`). `Draft → Verified → Rendered → Speculated → Committed` makes invalid transitions a *compile error*. This is textbook elite Rust (D11) and the system dogfoods it.

### ⚪ LOW — `system_info` synthetic-wiring module (REGRA #0 artifact)
`touring-server/src/lib.rs:29` defines a `system_info` module whose explicit purpose (per its own doc) is to "Wire the helper functions scattered across `server::tools_*` modules so all of them have a single production-side consumer" — i.e. it exists *only* to make ~10 otherwise-orphaned helpers reachable so cargo doesn't flag them. This is REGRA #0 satisfied **defensively** (potentialize-not-delete), which is policy-correct, but it is also a code smell: a module that exists to defeat the orphan detector rather than to serve a domain need. **Recommendation:** acceptable as-is, but if those helpers have no genuine call path, prefer deletion over synthetic wiring. Flag, don't churn.

---

## F1.12 — Architectural Consistency

### 🔴 HIGH — ARCHITECTURE.md drift confirmed by its OWN gate
```
$ python3 docs/sync_metrics.py --check
DRIFT: ARCHITECTURE.md crate inventory block is stale -> run docs/sync_metrics.py --sync
```
ARCHITECTURE.md claims `crates=45, loc_src=532180, loc_workspace=602584, test_fns=14292` (METRICS comment, measured 2026-06-15). Reality (00-scope, 2026-06-20): 44 members + benches, **544,590** LOC src. The doc is 5 days + several edits stale.
- **The elite part:** the workspace *has* a metrics-as-code gate (`sync_metrics.py --check`) that **detects this automatically** — that is genuinely best-in-class (D38 drift detection / Touring USP). The failure is purely that `--sync` wasn't run.
- **Severity High** because REGRA #21 (no failure dismissed): a doc that disagrees with its own gate is a live, detected inconsistency. **Fix (1 command):** `python3 docs/sync_metrics.py --sync`, then wire `--check` into CI (it may already be — verify in the CI/CD review). Trivial effort, removes the headline "doc drift" criticism permanently.

### ✅ Cross-cutting consistency is strong
`tracing` is the logging substrate (not ad-hoc `println!`); errors converge on `thiserror`/`anyhow`; the `clippy::all = deny` floor + 8 RBP-11 ratchets (`if_let_mutex`, `rc_mutex`, `lossy_float_literal`, `fn_to_numeric_cast_any`, `mut_mut`, `dbg_macro`, `wildcard_dependencies`, …) enforce a uniform idiom workspace-wide with documented per-lint rationale. **D12: USP-grade.**

---

## Must-Investigate Items — Resolutions

### `touring-server` monolith (70.9k LOC) — split verdict: **JUSTIFIED but NOT URGENT**
Module map (sub-agent verified): `cli/` 89 files / 34.3k LOC (48%) · `server/` 15 files / 14.1k LOC (20%, the 42 MCP tools) · `tools/` 22 files / 10.8k LOC (15%, shared business logic) · ~18 shared utility modules (`ingest, memory_store, output, plugins, agent_diary, context_compiler, …`). It is also the `touring` **binary** (`[[bin]] name = "touring", path = "src/main.rs"`) — a dual lib+bin.
- **Seam already partially extracted:** `daemon_client.rs` (197 LOC) is a *neutral seam* (lib.rs:226) — CLI handlers call the daemon via socket, not directly into `server/`. The seam *holds* in one direction (server→cli: 0 imports) but *leaks* once (`cli/find_code.rs` imports `server::params`).
- **Verdict:** cohesive enough to ship as one crate today (shared `tools/` + 18 utility modules genuinely serve both modes; features are monolithic). A split into `touring-cli-app` + `touring-mcp-server` + `touring-server-core` (the 18 shared modules) would make the seam *compile-enforced* and let modes release independently — but it is a **Medium**-priority refactor, not a Critical. The CLI's 48% LOC share means a CLI-internal audit should precede any split.

### 5 shim dirs (50 dirs vs 44+benches members) — **safe to remove, identified**
6 dirs on disk are absent from `[workspace] members`:
```
touring-antt       → pub use touring_intelligence::ann::*        (W6 shim, 14 LOC)
touring-ast        → DEPRECATED SHIM → touring_code::ast         (W4 shim, 17 LOC)
touring-cognitive  → pub use touring_intelligence::reasoning::*  (W6 shim, 13 LOC)
touring-learning   → pub use touring_intelligence::rl::*         (W6 shim, 14 LOC)
touring-wasm       → pub use touring_bindings::wasm::*           (transparent shim, 11 LOC)
hooks/             → 3 shell scripts (cc-*.sh) — NOT a crate
```
Each is a thin `pub use` re-export already de-listed from the workspace → **not compiled, zero blast radius, safe to `git rm`** (the baseline said "5"; precisely 5 crate shims + `hooks/` scripts). They cost nothing at build time but pollute `ls crates` and inflate the "50 vs 45" optics. **Recommendation:** delete (git, by Gabriel — REGRA #11 forbids me running it).

### `gate_metrics.rs` "duplication" (baseline flag) — **NOT duplication; it's a dead-on-disk file**
The baseline flagged `touring-foundation/src/gate_metrics.rs` (3,468 LOC) and `touring-hooks-shared/src/gate_metrics.rs` (3,468 LOC, byte-identical via `cmp`). **Resolved:** `touring-hooks-shared/src/lib.rs:51` does `pub use touring_foundation::gate_metrics;` (re-export), and crucially **there is NO `mod gate_metrics;` declaration in hooks-shared/lib.rs** → the file is **not compiled**. It is an orphaned-on-disk leftover from the A5 Path-A relocation (the lib.rs comment literally says "Old src/gate_metrics.rs orphaned (git-rm)"). **Same class as the shim dirs: dead file awaiting deletion**, not a live D03 duplication. **Recommendation:** `git rm crates/touring-hooks-shared/src/gate_metrics.rs`.

---

## Already-Elite (genuine strengths — credited)

1. **0 dependency cycles** (Tarjan SCC, verified live) — the hardest structural property, achieved and held.
2. **True kernel:** `touring-foundation` deps `= []`, consumed cross-crate, not bloat.
3. **Verified-clean layering** (cargo metadata ground truth): foundation < storage < code < intelligence < server, **no inversion**.
4. **Move-utils-down playbook** (A5): the `KnowledgeSource` trait + records hoisted to `foundation/knowledge_source.rs` dissolved a would-be `storage↔intelligence` cycle via a kernel abstraction below both ends — a reusable, documented anti-cycle pattern.
5. **A2/A5 relocation discipline:** fusions executed with re-export shims (identity-preserving) → consumers compiled with zero edits; the 5 leftover shim dirs are the *receipts* of a clean migration, not breakage.
6. **Typestate pipeline** (`Draft→Verified→Rendered→Speculated→Committed`) — compile-enforced lifecycle; the system dogfoods elite Rust.
7. **Type-driven data model** (newtypes + domain enums + per-module typed errors in storage; `KnowledgeSource` port).
8. **Workspace lints** (`clippy::all = deny` + 8 zero-violation RBP-11 ratchets, every `allow` justified) — D12 USP-grade consistency.
9. **Metrics-as-code drift gate** (`sync_metrics.py --check`) — the architecture *detects its own doc drift*; few repos at any tier have this.
10. **Dual-license satisfied:** both `LICENSE-MIT` and `LICENSE-APACHE` present on disk (baseline was wrong — Apache exists, 11,333 bytes).

---

## Priority Action Table

| # | Severity | Finding | Fix | Effort |
|---|---|---|---|---|
| A1 | 🔴 High | `cargo-deny bans` 2 errors (schemars/schemars_derive) | `harness-mcp/Cargo.toml:21`: `schemars = "0.8"` → `{ workspace = true }` | 1 line |
| A2 | 🔴 High | ARCHITECTURE.md drift (self-detected) | `python3 docs/sync_metrics.py --sync` + ensure `--check` in CI | 1 cmd |
| A3 | 🔴 High | 231 `Result<_,String>` (RBP-03 incomplete) | Continue typed-error conversion: bindings(51) → hook-runtime(42) → hook-handlers(29) | crate-by-crate |
| A4 | 🟡 Med | ~320 genuinely-dead pub symbols (of 4,823) | Wire (REGRA #0) or remove; start with 4 verified | targeted |
| A5 | 🟡 Med | ~2,300 intra-crate-used `pub` (over-broad visibility) | Down-grade `pub` → `pub(crate)` (D07) | mechanical |
| A6 | 🟡 Med | `embedding/` vs `embeddings/` fragmentation + 2× `EmbeddingError` | Consolidate into `embeddings/`; merge error enums | refactor |
| A7 | 🟡 Med | Semver governance ambiguous (2/49 `publish=false`, but docs.rs metadata set) | Decide: façade-crate-published OR fully-private + drop publish metadata | decision |
| A8 | 🟡 Med | `touring-server` 70.9k mega-crate (CLI+MCP, seam leaks once) | Optional split → cli-app + mcp-server + server-core; not urgent | L4 refactor |
| A9 | 🟡 Med | 57 phantom-path orphans (stale wiring DB) + `deny.toml` unmatched skip root | `touring index rebuild`; prune stale `deny.toml` roots | low |
| A10 | ⚪ Low | 5 shim dirs + dead `hooks-shared/gate_metrics.rs` on disk | `git rm` (Gabriel — REGRA #11) | trivial |
| A11 | ⚪ Low | Missing `rust-toolchain.toml`/`rustfmt.toml`/`clippy.toml`/`CODEOWNERS` | Add toolchain pin (MSRV 1.85 already declared) + fmt/clippy config | trivial |
| A12 | ⚪ Low | `system_info` synthetic-wiring module (REGRA #0 artifact) | Accept, or delete genuinely-unused helpers vs wiring them | flag only |

---

_F1.7–F1.12 complete. 0 Critical · 3 High · 6 Medium · 4 Low. Headline: the architecture is structurally elite (0 cycles, true kernel, clean layering, typestate, type-driven model); the debt is mechanical (1-line bans fix, 1-cmd doc sync, dead files to `git rm`, an inflated orphan number whose real dead fraction is ~7%)._
