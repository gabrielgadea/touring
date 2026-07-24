# A4 — `touring-foundation` god-kernel split: verified extraction plan

> 2026-06-15 · operator TACO · L4 crate-extraction (class: daemon-lib-rearch / A1) · **plan only — execution = dedicated fresh session**
> Report ref: `05-final-report.md` P2 "`touring-foundation` god-kernel split (A4)" · `01b-architecture.md:136` · `02b-performance.md:226`
> All ground-truth below verified `file:line` against the code (FASE 1 scout + FASE 2 architect).

## Goal (prescriptive, from the review)

Peel the heavy, optional subsystems out of `touring-foundation` (fan-in **20**, 21.7k LOC) so the
crate becomes a **thin true-kernel** (config, error, schema DDL, contracts, types):

- **`embedding`** (64K, 2 files) → **`touring-storage`** (which already owns `embeddings/`).
- **`sentinel` (120K) + `failover` (52K) + `conflict` (52K)** → a new **`touring-resilience`** crate.

## Ground truth (FACT [1.0], verified)

| Claim | Evidence |
|---|---|
| `touring-storage` → `touring-foundation` (forward dep exists) | `touring-storage/Cargo.toml:16` `touring-foundation = { path = ... }` |
| `touring-foundation` ↛ `touring-storage` (no reverse) | grep of foundation/Cargo.toml — none |
| `embedding` is `#[cfg(feature = "gpu-embeddings")]`, a **default** feature | `foundation/lib.rs:62`; `foundation/Cargo.toml:99 default = ["gpu-embeddings"]`, `:100 gpu-embeddings = []` |
| `embedding` = 2 files (`client.rs` 46.9K, `mod.rs` 11.3K); minimal deps (async_trait, tokio, rkyv) | `ls foundation/src/embedding/` |
| `embedding` consumed by **3 files, all in `touring-server`** | grep `touring_foundation::embedding` |
| `sentinel` consumed by: foundation's own bin `touring_resource_monitor.rs`, `touring-hook-handlers`, `touring-hooks-shared` | grep `touring_foundation::sentinel` + `foundation/src/bin/touring_resource_monitor.rs:35,54,90,98,99,100` |
| `failover` is **re-exported at the foundation root** → consumed via root names, not `failover::` | `foundation/lib.rs:137 pub use failover::{ PersistenceProvider, ProviderPlugin, VectorStoreProvider, … }` |
| `conflict` is `pub mod conflict` with low external `conflict::` use | `foundation/lib.rs:34` |

## Wrinkles / blockers (the hard part — why this is NOT a naive move)

- **W1 — orphan rule (E0117) on `embedding`'s error bridge.** `embedding/client.rs:1028`
  `impl From<EmbeddingError> for crate::error::TouringError`. Today both types are local to
  `foundation` (legal). If `EmbeddingError` moves to `touring-storage`, this becomes
  `impl From<storage::EmbeddingError> for foundation::TouringError` = `impl From<Local> for Foreign`
  → **disallowed**. Keeping the impl in foundation would need `foundation → storage` (cycle).
  **Resolution:** drop the blanket `From`; convert explicitly at the 1 internal site
  (`client.rs:1086 let e: TouringError = err.into()`) + any consumer via
  `.map_err(|e| TouringError::Embedding(e.to_string()))` (the same shape `TouringError::Embedding`
  already provides). Verify no other crate relies on `EmbeddingError: Into<TouringError>` via `?`.
- **W2 — `failover` root re-export.** Consumers use `touring_foundation::PersistenceProvider` etc.
  Moving `failover` to `touring-resilience` requires EITHER (a) `foundation` re-exports
  `pub use touring_resilience::{PersistenceProvider, …}` (creates `foundation → resilience`; then
  `resilience` MUST NOT use any `foundation` type, else cycle), OR (b) repoint every root-name
  consumer to `touring_resilience::…`. **Decision: (b)** keeps the kernel dependency-free (no
  `foundation → resilience` back-edge), at the cost of consumer churn — must enumerate the
  `PersistenceProvider`/`ProviderPlugin`/`VectorStoreProvider` consumers first.
- **W3 — `sentinel`'s in-crate binary.** `foundation/src/bin/touring_resource_monitor.rs` uses
  `sentinel`. After the move, either the bin moves to `touring-resilience` (cleanest — it's a
  resilience tool) or `foundation` gains a `dev`/bin-only dep on `resilience`. **Decision: move the
  bin to `touring-resilience`** (no `foundation → resilience` edge).
- **W4 — feature gates travel with the code.** `gpu-embeddings` (embedding) must be recreated on the
  target crate (`touring-storage`), and any `resource-monitor-bin`/`ebpf-telemetry`/sentinel feature
  likewise on `touring-resilience`. Re-point `foundation`'s `default` feature set.
- **W5 — coupling/cycle precondition.** Before moving each module, verify it imports only
  foundation-**kernel** types (config/error/types), never the heavy peers — else the new leaf crate
  would need `foundation`'s heavy parts and risk a cycle. `embedding` is clean modulo W1; `failover`
  (the `*Provider` traits) and `sentinel`/`conflict` must be coupling-audited at execution time.

## Layering (cycle-free target DAG)

```
touring-foundation (thin kernel: config, error, schema DDL, contracts, types)  ← leaf-ward
        ▲                         ▲
        │                         │  (both depend ONLY on the kernel, never the reverse)
 touring-resilience        touring-storage (+ embedding)
 (sentinel/failover/conflict + the resource-monitor bin)
        ▲                         ▲
        └──────── higher crates (hooks-shared, hook-handlers, server, …) ───────┘
```
Invariant (gate every phase): `touring wiring cycles` = 0 ; `foundation` never gains an edge to
`resilience`/`storage`.

## Phased increments (reversible, A2-playbook style, safest-first)

Each phase: `taco-forge perfect-create-crate` (new crate) / `perfect-create` (moved files) +
`perfect-edit` (drop `pub mod` from foundation, repoint consumers, move features) →
`cargo check --workspace --exclude touring-quality` + `cargo clippy --all-targets -D` +
`touring wiring cycles`=0 (all real-exit). Old module dirs remain orphaned on disk (Gabriel
`git rm`, per the A2 precedent, REGRA #11).

1. **P1 — create `touring-resilience` + move `conflict`** (lowest external use). Establishes the crate
   + the layering. Repoint conflict consumers.
2. **P2 — move `failover`** into `touring-resilience`. Handle W2 (repoint the 3 `*Provider` root-name
   consumers to `touring_resilience::…`; drop the foundation root `pub use failover::…`).
3. **P3 — move `sentinel`** + its `touring_resource_monitor` bin (W3). Repoint hook-handlers +
   hooks-shared.
4. **P4 — move `embedding` → `touring-storage`** (W1 orphan-rule fix + W4 `gpu-embeddings` feature
   re-creation on storage; repoint the 3 touring-server consumers). This is the highest-coupling
   peel → last.
5. **P5 — thin-kernel validation**: confirm `foundation` `src/` now = config/error/schema/types/
   contracts (+ alloc/hash/char_classes kernel utils); `touring ast workspace-info` fan-in still 20
   but the **dirty-set per kernel edit** shrinks (the perf goal of A4 per `02b:226`); `wire-orphans`.

## Risk register

| Risk | Sev | Mitigation |
|---|---|---|
| Cycle (`foundation → resilience/storage`) | **HIGH** | Decisions W2(b)+W3 keep the kernel back-edge-free; `wiring cycles`=0 gate per phase; consumers repointed, foundation never re-exports the moved modules |
| Orphan-rule E0117 (embedding error bridge) | **HIGH** | W1: drop blanket `From`, explicit `.map_err` at the few sites |
| Missed root-re-export consumer (failover) | **MED** | enumerate `PersistenceProvider`/`ProviderPlugin`/`VectorStoreProvider` consumers before P2; `cargo check --workspace` catches the rest |
| Feature-gate drift (gpu-embeddings) | **MED** | W4: recreate the feature on storage; validate with `--features gpu-embeddings` |
| Move corrupts file | **LOW** | `perfect-edit` atomic snapshot + memory snapshot; old dirs orphaned (reversible until git rm) |

## Execution note

Per the **A1 (touring-server split, W12) and daemon-lib-rearch precedent**, crate-extraction of this
blast radius (fan-in 20, orphan-rule + re-export + cycle wrinkles) is executed in a **dedicated
fresh-context session**, one phase per `cargo check --workspace` gate. This plan makes that execution
safe and fast; improvising it inside a long context under no-git risks a broken workspace (the
opposite of the `/goal`'s "perfeito"). The plan IS the FASE 1–4 deliverable for A4.
