# Phase 4A: Rust & Language Best Practices

> Touring workspace · 2026-06-13 · agent: systems-programming:rust-pro
> North star: idiomatic, modern, elite Rust. Read-only. All findings cite real `file:line` + CLI evidence.
> Builds on 01 (quality/arch), 02 (security/perf). Context7 consulted for rkyv, pyo3.

## TL;DR severity counts

| Severity | Count | IDs |
|---|---|---|
| Critical | 0 | — |
| High | 4 | RBP-01 (lint ceiling), RBP-02 (8 crates escape lint floor), RBP-03 (typed public errors), RBP-04 (MSRV not pinned/tested) |
| Medium | 7 | RBP-05 (unsafe Send no SAFETY), RBP-06 (dup dep versions), RBP-07 (cargo-deny not a CI gate), RBP-08 (#[non_exhaustive] gap), RBP-09 (glob re-exports), RBP-10 (edition 2021 not 2024), RBP-11 (lints.rust near-empty) |
| Low | 4 | RBP-12 (async-trait droppable), RBP-13 (Box<dyn> vs impl Trait), RBP-14 (pyo3 0.24 lag), RBP-15 (no rust-toolchain.toml) |

**The repo is already strong on the basics** — `workspace.dependencies` single-source is ~89% adopted (645 `workspace=true` lines vs effectively 0 hard-coded deps; the "77 hard-coded" are `version`/`edition`/`rust-version` *package metadata*, not deps), `std::sync::Mutex` is **never** used in an async-fn file (0 lock-across-await UB hazard), `LazyLock`/`OnceLock` is already used 244× (vs 68 legacy `once_cell::Lazy`, 0 `lazy_static`), `let-else` 538×, `#[must_use]` 794×, no empty `.expect("")`. The elite gap is **the enforced ceiling**, not the floor.

---

## RBP-01 [High] — Lint ceiling stops at `clippy::all`; no `unwrap_used`/`expect_used`/`missing_docs`/`pedantic` in `[workspace.lints]`

**Evidence**: `Cargo.toml:586-627`. The block denies `clippy::all` (priority -1) + `needless_collect`, relaxes `indexing_slicing = allow` and ~15 test-harness lints. It does **not** include any of the robustness/doc lints an SDK ships with. The CEG already proves the ratchet is viable: `touring-ceg/src/gateway/mod.rs:43` `#![cfg_attr(not(test), deny(clippy::unwrap_used))]` — module-scoped, test-exempt, clean.

**Why it matters**: `clippy -D warnings` = 0 today is a *claim about the present*; without `unwrap_used`/`expect_used` denied, nothing stops the next contributor from re-introducing the ~124 prod unwraps Phase 1 found (incl. `assist.rs:256` parsing untrusted line:col, a DoS vector per Phase 2). "Auditable" becomes a regression-proof invariant only when the compiler enforces it.

**Current**:
```toml
[workspace.lints.clippy]
all = { level = "deny", priority = -1 }
indexing_slicing = "allow"
needless_collect = "deny"
# ...test-harness allows...
```

**Recommended elite `[workspace.lints]` block** (see "Elite lints block" section below for the full copy-paste version with rationale).

---

## RBP-02 [High] — 8 crates escape the workspace lint floor entirely (no `[lints]` section)

**Evidence**: 37/45 crates declare `[lints]\nworkspace = true` (e.g. `touring-foundation/Cargo.toml:127-128`, `touring-server/Cargo.toml:346-347`). **8 crates have NO `[lints]` section at all**, so they do *not* inherit `workspace.lints.clippy.all = deny`:

```
inferlets, touring-assists, touring-contracts, touring-generator,
touring-identity, touring-license, touring-lsp, touring-rkyv
```

**Why it matters**: This is silent. The most damaging members of the list are **`touring-generator`** (a public generator API — the VGP pipeline) and **`touring-contracts`** (the IoC seam that A7/A8 want to grow). They build *without* the `clippy::all = deny` floor that the constitution claims is universal. `inferlets` and `touring-rkyv` sit on hot/unsafe paths. (`cargo clippy --workspace -D warnings` still scans them, but the *crate-local* `deny` that should travel with the crate is absent — so a `cargo clippy -p touring-generator` in isolation is permissive.)

**Recommended**: add to each of the 8 `Cargo.toml`:
```toml
[lints]
workspace = true
```
Then they inherit the elite block from RBP-01 automatically. Zero code change, closes the floor.

---

## RBP-03 [High] — Public/library API leaks stringly errors; not fully `thiserror`-typed for an SDK

**Evidence**:
- `373` `.map_err(|e| format!(...))` sites (top: touring-intelligence 12, touring-server 9, touring-hooks-core 8, touring-hook-runtime 7, touring-bindings 7). `grep map_err.*format!`.
- `141` public signatures `pub fn … -> Result<…, String>` — stringly errors crossing crate boundaries.
- Counterweight (good): `88` `#[derive(…Error…)]` enums, `86` `#[from]` impls already exist. The kernel `TouringError` is exemplary — `touring-foundation/src/error.rs:13-15` `#[derive(Error, Debug)] #[non_exhaustive]` with a doc rationale.

**Why it matters**: For a would-be `touring-sdk` (Phase 1 B-W1/B-W3 public-release waves), public errors must be *typed* so downstream can `match`/`#[from]`/programmatically branch. `Result<T, String>` forces consumers to string-parse. `.map_err(|e| format!("…: {e}"))` also *erases the source chain* (no `Error::source()`), defeating `anyhow` context and tracing's error capture.

**Current** (app-grade, in a library crate):
```rust
// touring-bindings, touring-intelligence, etc.
something().map_err(|e| format!("failed to X: {e}"))?;   // String, no source chain
pub fn parse(s: &str) -> Result<Foo, String> { … }       // stringly public API
```

**Recommended** (library-grade): one `thiserror` enum per crate boundary, `#[from]` for upstream errors:
```rust
#[derive(thiserror::Error, Debug)]
#[non_exhaustive]
pub enum BindingsError {
    #[error("postgis EWKB decode failed")]
    Ewkb(#[from] EwkbError),
    #[error("io while reading {path}")]
    Io { path: PathBuf, #[source] source: std::io::Error },
}
pub fn parse(s: &str) -> Result<Foo, BindingsError> { … }
```
Phase 1 estimates `#[from]` collapses ~60% of the 373 sites. App-level binaries (touring-cli `main`) keep `anyhow` — that split (`thiserror` in libs, `anyhow` at the app edge) is the correct idiom.

---

## RBP-04 [High] — MSRV declared but inconsistent across crates and NOT verified in CI

**Evidence**:
- `Cargo.toml:144-145` `edition = "2021"`, `rust-version = "1.80"` in `[workspace.package]`.
- But per-crate: `22` crates `rust-version.workspace = true` (correct), **`18` crates hard-code `rust-version = "1.75"`**, `4` hard-code `"1.80"`. So 18 crates *claim* a lower MSRV (1.75) than the workspace (1.80) — a contradiction. `grep '^rust-version' crates/*/Cargo.toml`.
- CI (`.github/workflows/ci.yml:31,59`) uses `dtolnay/rust-toolchain@stable` — **floating**, not pinned to 1.80, and there is **no MSRV job** that builds against 1.80 to prove the claim.
- No `rust-toolchain.toml` at root (`ls` → absent).

**Why it matters**: An MSRV that isn't tested is decoration. Today CI runs on whatever stable GitHub ships, so a 1.81+ feature could land and break the advertised 1.80 floor with zero signal. The 18 crates at "1.75" are aspirational — they depend (transitively) on `tokio 1.40`, `thiserror 2.0`, etc., and use `LazyLock` (stabilized 1.80), so they cannot actually build on 1.75.

**Recommended**:
1. Make all 18+4 crates use `rust-version.workspace = true` (single source = `Cargo.toml:145`). The hard-coded `1.75` are wrong (code uses `LazyLock` = 1.80).
2. Add an MSRV CI job:
```yaml
  msrv:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@1.80.0
      - uses: Swatinem/rust-cache@v2
      - run: cargo +1.80.0 check --workspace
```
3. Optionally pin developer toolchain via `rust-toolchain.toml` (`channel = "1.80"` or a recent stable) so local `cargo` matches CI.

---

## RBP-05 [Med] — `unsafe impl Send for HookRuntime` has NO SAFETY comment (overlaps SEC-06)

**Evidence**: `touring-hook-runtime/src/hook_runtime.rs:695` `unsafe impl Send for HookRuntime {}` — the lines immediately above are an unrelated `pub use … IsolationMode;`; there is **no `// SAFETY:` block** documenting the invariant. Contrast the *correct* idiom 30 lines of grep away: `touring-hooks-core/src/knowledge.rs:3059-3065` `ThreadSafeKnowledgeDB` carries a full SAFETY paragraph ("…Mutex serializes all access…").

All 6 `unsafe impl Send/Sync` sites (`incremental_pipeline.rs:817,818,969,971`, `knowledge.rs:3065`, `hook_runtime.rs:695`, `simd/learning.rs:93,94`) should each have a SAFETY comment. Most do; `hook_runtime.rs:695` and the simd ones are the bare ones.

**Why it matters**: `HookRuntime` holds 10+ `RefCell` fields (per SEC-06); the `Send` is sound *only* because the single-actor mpsc model never shares it across threads. That invariant is load-bearing and undocumented — a future refactor that spawns the runtime onto a thread pool would silently introduce data-race UB. An elite repo documents every `unsafe impl` with the exact invariant being upheld (and ideally a `debug_assert`/test that the actor model holds).

**Recommended**:
```rust
// SAFETY: HookRuntime contains RefCell fields that are not Sync. It is only ever
// owned and accessed by a single per-project actor task (see daemon.rs:220 serial
// actor); it is moved into that task once and never shared. The mpsc command
// channel is the only cross-thread boundary and it transfers ownership, not refs.
// Adding any code that &-shares HookRuntime across threads INVALIDATES this.
unsafe impl Send for HookRuntime {}
```
And consider whether `Send` is even needed, vs `Arc<Mutex<…>>` / making the `RefCell`s into the actor's owned state (removes the `unsafe` entirely — the elite move).

---

## RBP-06 [Med] — Heavy duplicate dependency versions inflate the 1,558-package tree

**Evidence** (`cargo tree -d`, distinct duplicated externals):

| Dep | Versions coexisting | Cost |
|---|---|---|
| **wasmtime / cranelift** | `36.0.10` + `44.0.2` (full duplicate codegen tree) | huge — two JIT backends compiled |
| **syn** | `1.0.109` + `2.0.117` | proc-macro compile time |
| **thiserror** | `1.0.69` + `2.0.18` | workspace pins 2.0 but a dep drags 1.0 |
| **rand** | `0.8.6` + `0.9.2` + `0.10.0` | 3 copies |
| **axum / axum-core** | `0.7.9` + `0.8.9` | two web stacks |
| **tonic** | `0.12.3` + `0.14.5` | two gRPC stacks |
| **tower** | `0.4.13` + `0.5.3` | |
| **nalgebra / ndarray / simba / gemm / pulp** | 2–3 versions each | linfa/ML stack drags old linalg |
| **base64** | `0.13` + `0.21` + `0.22` | |
| **bitflags / bit-set / bit-vec / hashbrown / itertools / nix / ordered-float / strum** | 2–4 each | |
| **indexmap** | `1.9.3` + `2.14.0` (via linfa→ndarray-stats→ahash 0.7) | |

**Why it matters**: 1,558 packages is a large supply-chain + CVE surface (Phase 2 SEC-03). The two wasmtime trees alone are a multi-minute build-time tax and ~tens of MB of binary. The `indexmap 1.x` / `ahash 0.7` / `hashbrown 0.12` chain is pulled solely by `linfa-clustering` → `ndarray-stats` (touring-intelligence).

**Recommended**:
1. Run `cargo tree -i wasmtime@36` / `-i syn@1` / `-i thiserror@1` to find the laggard dragging the old version; bump or replace it.
2. The wasmtime 36 vs 44 split is the #1 target — likely one crate pins `wasmtime = "36"` while workspace says `44`. Unify on 44.
3. The linfa ML stack drags old `ndarray 0.15`/`nalgebra 0.32`/`indexmap 1`; evaluate whether `linfa-clustering` (used in touring-intelligence) can be updated or the clustering re-implemented on the newer `ndarray 0.16/0.17` already in the tree.
4. Add a `cargo deny` `[bans]` `multiple-versions = "warn"` policy so new duplicates surface in CI (ties to RBP-07).

---

## RBP-07 [Med] — `cargo-deny` config exists but is NOT a CI gate (advisories RED per SEC-03)

**Evidence**: `deny.toml` exists at root (23.9 KB, mtime 2026-05-31). CI (`ci.yml`) has `check`, `test`, `gates` jobs — **none runs `cargo deny check`**. Phase 2 SEC-03 reports `cargo deny check advisories` is RED (6 vulns incl. postgres-protocol RUSTSEC-2026-0179 CVSS 8.7, pyo3 0.24, tokio-postgres).

**Why it matters**: A `deny.toml` that no job runs is documentation, not enforcement. The advisory DB moves daily; the only way RED→GREEN stays GREEN is a gate.

**Recommended** — add to `ci.yml`:
```yaml
  deny:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: EmbarkStudios/cargo-deny-action@v2
        with:
          command: check advisories bans licenses sources
```
Gate `bans` enforces RBP-06's `multiple-versions` policy; `advisories` enforces SEC-03; `licenses` protects the MIT/Apache dual-license claim. Stage it as `|| true` (warn) for one sprint to triage the existing RED, then flip to hard-fail.

---

## RBP-08 [Med] — `#[non_exhaustive]` on only 11 of ~400 public enums; most error enums omit it

**Evidence**: `11` total `#[non_exhaustive]` across `crates/*/src`; `400` `pub enum`. Of `88` `#[derive(…Error)]` enums, only **5** are `#[non_exhaustive]`. The kernel `TouringError` (`foundation/src/error.rs:14`) does it right; `GenerateError` (`generator/src/error.rs:17`), `ApplyError`, `EmbeddingError`, `PluginError`, `FailoverError`, `TrmError`, etc. do not.

**Why it matters**: For a semver-governed SDK, adding a variant to a public exhaustive enum is a **breaking change** (downstream `match` stops compiling). `#[non_exhaustive]` lets you add variants in a minor release. This is the single cheapest semver-discipline lever before the public release waves. It matters most on (a) error enums (errors grow) and (b) the MCP/config public enums.

**Recommended**: add `#[non_exhaustive]` to every `pub enum` that crosses a crate boundary, prioritizing the 83 error enums that lack it. (For enums consumers must exhaustively match by design, leave exhaustive and document why.)

---

## RBP-09 [Med] — 45 glob re-exports (`pub use …::*`) leak unintended public surface

**Evidence**: `45` `pub use …::*;` across `crates/*/src`. Combined with very high re-export counts on the public crates (touring-intelligence 168 `pub use`, touring-cli 155, touring-foundation 63, touring-generator 58).

**Why it matters**: `pub use module::*` re-exports *everything currently public* in `module`, including items you didn't mean to expose, and silently grows the public surface every time `module` gains a `pub` item — invisible semver breakage waiting to happen. For an SDK the public surface must be *intentional*.

**Recommended**: convert glob re-exports to explicit named re-exports at crate roots (`pub use module::{Foo, Bar};`). Then a public-API-diff tool (`cargo public-api` / `cargo semver-checks`) can guard the surface in CI — the natural pairing with the B-W3 semver wave.

---

## RBP-10 [Med] — Edition 2021, not 2024; missing the simplifications 2024 enables

**Evidence**: `Cargo.toml:144` `edition = "2021"`; `24` crates hard-code `edition = "2021"`, `21` use `edition.workspace = true`. `0` let-chains in code (`if let … && let …`) — an edition-2024-stable ergonomic the codebase can't use yet.

**Why it matters**: Edition 2024 (stable since Rust 1.85, Feb 2025) brings: stable `let`-chains (collapses nested `if let` pyramids — the codebase has 538 `let-else` already, signalling appetite for this), `gen` blocks, RPIT lifetime capture fixes (`impl Trait` returns become usable — see RBP-13), stricter `unsafe` in `extern`, and `Future`/`IntoFuture` prelude additions. For "elite, modern Rust" the edition itself is the headline lever.

**Recommended**: migrate to `edition = "2024"` workspace-wide (requires MSRV ≥ 1.85, so bump RBP-04's floor): `cargo fix --edition` per crate, then flip `[workspace.package] edition = "2024"` and `edition.workspace = true` everywhere. Do it *after* RBP-04 (MSRV pinned) lands. This is the one change that most directly earns the "modern Rust 2024" descriptor.

---

## RBP-11 [Med] — `[workspace.lints.rust]` is near-empty (only `unexpected_cfgs = allow`)

**Evidence**: `Cargo.toml` `[workspace.lints.rust]` contains a single line: `unexpected_cfgs = "allow"`. None of the standard `rust` lint group denials are present.

**Why it matters**: The `clippy` block is curated but the `rust` (rustc) block does no work. Elite repos deny `unsafe_op_in_unsafe_fn`, `rust_2018_idioms`, `unreachable_pub` (catches accidental over-`pub`, complements RBP-09), `unused_qualifications`, and warn `missing_debug_implementations` on public types.

**Recommended** (see Elite block below): add a curated `[workspace.lints.rust]` alongside the clippy block.

---

## RBP-12 [Low] — `async-trait` still in 5 crates; native async-fn-in-traits stable since 1.75

**Evidence**: `async-trait` declared in 5 `Cargo.toml`, `29` `#[async_trait]` sites. MSRV is 1.80; native `async fn` in traits is stable since 1.75.

**Why it matters**: `async-trait` boxes every call (`Pin<Box<dyn Future>>` allocation per invocation) and adds a proc-macro. For traits that are *not* object-safe / not used as `dyn`, native `async fn` is zero-cost and idiomatic.

**Recommended**: audit the 29 sites. Where the trait is used with generics (not `dyn`), drop `#[async_trait]` for native `async fn`. Keep `async-trait` only where `dyn AsyncTrait` object safety is genuinely required (or migrate those to `trait-variant` / explicit `-> impl Future`).

---

## RBP-13 [Low] — Zero `-> impl Trait` returns; 157 `Box<dyn>` where some could be `impl Trait`

**Evidence**: `0` `-> impl ` in returns; `157` `Box<dyn …>`. Some `Box<dyn Iterator>`/`Box<dyn Future>` returns allocate where `-> impl Iterator`/`-> impl Future` would be zero-cost.

**Why it matters**: RPIT (`-> impl Trait`) avoids the heap allocation + vtable indirection of `Box<dyn>`. Edition 2024 (RBP-10) fixes the RPIT lifetime-capture rules that historically made this awkward, so this finding pairs with the edition bump.

**Recommended**: post-edition-2024, sweep `Box<dyn Iterator/Future>` *return positions* (not storage/collections — those legitimately need boxing) for `impl Trait` conversion. Low priority; do it opportunistically.

---

## RBP-14 [Low] — pyo3 0.24 lags current (0.25+); soundness/security fixes in newer (overlaps SEC-03)

**Evidence**: `Cargo.toml` workspace dep `pyo3 = { version = "0.24", … }`. Context7: pyo3's `Bound`/`Python::attach` API is current; 0.24 already uses Bound (good — not the legacy GIL-Refs API), but SEC-03 flags 0.24→0.29 for advisory fixes. tree-sitter 0.26, wasmtime 44, tantivy 0.22, rkyv 0.7, clap 4.5, tokio 1.40 are all reasonably current.

**Why it matters**: pyo3 is on the FFI/unsafe boundary (touring-python); soundness fixes there are security-relevant. The migration is mechanical (0.24 already on Bound API).

**Recommended**: bump pyo3 to the latest 0.x in lockstep with SEC-03's advisory remediation; run touring-python tests. Low *idiom* priority but High *security* (tracked under SEC-03).

---

## RBP-15 [Low] — No `rust-toolchain.toml`; dev/CI toolchain unpinned (overlaps RBP-04)

**Evidence**: no `rust-toolchain.toml` at root. CI floats on `@stable`.

**Recommended**: add `rust-toolchain.toml`:
```toml
[toolchain]
channel = "1.85"   # or pinned recent stable; >= MSRV
components = ["clippy", "rustfmt"]
```
Gives reproducible local builds matching CI. (If you want CI to *also* test latest-stable for forward-compat, keep one `@stable` job + one pinned job.)

---

## rkyv nuance (refines Phase 2 F4)

Phase 2 F4 says rkyv "zero-copy" is negated by `serde_json::from_slice`. **Refinement**: the daemon *request* path **does** use real rkyv zero-copy — `daemon.rs:1003` `touring_rkyv::check_archived_root::<IpcRequest>(&body)` and `dependency_cache.rs:313` `rkyv::check_archived_root::<ArchivedIndexSnapshot>`. There are 26 `check_archived_root`/`access` sites. So rkyv *is* used correctly in places; F4's overhead is in the *response*/ipc.rs serde path, not universally. The elite move per Context7: rkyv 0.7's `check_archived_root` is the older API; rkyv 0.8 renames to `access` (safe, validated) / `access_unchecked` (trusted, max-perf). A 0.7→0.8 bump + standardizing on `access`/`access_unchecked` would modernize and unify the 26 sites. Pairs with RBP-06 (touring-rkyv also escapes the lint floor, RBP-02).

---

## THE ELITE `[workspace.lints]` BLOCK (copy-paste, with rationale)

```toml
[workspace.lints.rust]
unexpected_cfgs               = "allow"   # keep — feature cfgs intentionally pruned
unsafe_op_in_unsafe_fn        = "deny"    # explicit unsafe blocks inside unsafe fn
unreachable_pub               = "warn"    # catches accidental over-pub (pairs w/ RBP-09)
unused_qualifications         = "warn"
missing_debug_implementations = "warn"    # public types should be Debug
rust_2018_idioms              = { level = "warn", priority = -1 }
# After edition 2024 (RBP-10):
# edition_2024_expr_fragment_specifier = "warn"

[workspace.lints.clippy]
# ── Floor (existing) ──
all              = { level = "deny", priority = -1 }
needless_collect = "deny"
indexing_slicing = "allow"   # keep relaxed (runtime bounds-check still fires)

# ── Curated pedantic (priority -1 so individual allows win) ──
pedantic                 = { level = "warn", priority = -1 }
# Pedantic noise to silence workspace-wide (these fire on idiomatic code):
module_name_repetitions  = "allow"
must_use_candidate       = "allow"
missing_errors_doc       = "allow"   # flip to "warn" for the SDK crates only
missing_panics_doc       = "allow"
cast_precision_loss      = "allow"
cast_possible_truncation = "allow"
similar_names            = "allow"
too_many_lines           = "allow"

# ── Robustness ratchet (start "warn" workspace-wide, "deny" per-crate, see order) ──
unwrap_used   = "warn"   # ratchet to deny crate-by-crate
expect_used   = "warn"   # idem
panic         = "warn"   # idem (libs only; bins keep panic at app edge)
todo          = "warn"
unimplemented = "warn"
dbg_macro     = "deny"   # no stray dbg! ever
print_stdout  = "warn"   # nudge toward tracing (244 eprintln! — Phase 1)
print_stderr  = "warn"

# ── Existing test-harness allows (keep) ──
assertions_on_constants = "allow"
manual_range_contains   = "allow"
useless_vec             = "allow"
let_unit_value          = "allow"
approx_constant         = "allow"
absurd_extreme_comparisons = "allow"
field_reassign_with_default = "allow"
useless_conversion      = "allow"
redundant_closure       = "allow"
len_zero                = "allow"
manual_div_ceil         = "allow"
needless_update         = "allow"
unnecessary_map_or      = "allow"
int_plus_one            = "allow"
expect_fun_call         = "allow"
bool_assert_comparison  = "allow"
```

### Per-crate `deny`-ratchet order (start from the already-clean CEG)

Roll out `#![cfg_attr(not(test), deny(clippy::unwrap_used, clippy::expect_used, clippy::panic))]` at each crate's lib.rs / module root, in this order (cleanest → dirtiest, so each step is small and green):

1. **touring-ceg** — already `deny(unwrap_used)` at `gateway/mod.rs:43`. Promote to full `deny(unwrap_used, expect_used, panic)` workspace-visible. *The exemplar.*
2. **touring-contracts, touring-identity, touring-hooks-saga, touring-lsp, touring-code** — already `forbid(unsafe_code)`; smallest surfaces; add the deny here next. (Also fix RBP-02: these need `[lints] workspace = true` — contracts/identity/lsp currently have no `[lints]` section.)
3. **touring-foundation** — kernel; high fan-in (20) so paying it down here protects everything downstream. Already exemplary on errors.
4. **touring-rkyv, touring-generator, inferlets, touring-assists** — the RBP-02 escapees; add `[lints] workspace=true` AND the per-crate deny.
5. **touring-storage, touring-cognitive, touring-intelligence** — data/ML layer.
6. **touring-hooks-core, touring-hook-runtime, touring-dispatch** — hot path; the ~124 prod unwraps (Phase 1) + `assist.rs:256` live near here; do last with care, fixing each unwrap as the deny surfaces it.
7. **touring-server, touring-cli** — biggest; finish here. `touring-cli` is a binary, so keep `panic` at `allow` for `main`-edge but deny `unwrap_used`/`expect_used`.

Each step: add the attr → `cargo clippy -p <crate> -- -D warnings` → fix the handful it surfaces → commit. The workspace-wide `warn` from the block above means no step ever *breaks* CI; the per-crate `deny` is the regression-proof seal.

---

## Verdict on dependency modernity

**Mostly current, with two real drags.** tokio 1.40, thiserror 2.0, clap 4.5, tantivy 0.22, wasmtime 44, tree-sitter 0.26, criterion 0.5, rstest 0.26 are all recent. The single-source `[workspace.dependencies]` discipline (208 deps, ~89% `workspace=true` adoption, 0 genuinely hard-coded dep versions) is **elite-grade** and rare. The two drags: (1) **duplicate version sprawl** (two full wasmtime/cranelift trees, syn 1+2, thiserror 1+2, rand ×3, axum/tonic/tower ×2, the linfa→old-linalg chain) inflating the 1,558-package surface — RBP-06; (2) **pyo3 0.24** lagging on the FFI/security boundary — RBP-14/SEC-03. Neither is structural; both are version-bump + `cargo deny [bans]` work.

## The #1 best-practices lever toward elite

**RBP-01 + RBP-02 together: install the elite `[workspace.lints]` block AND give the 8 escapee crates `[lints] workspace = true`, then ratchet `deny(unwrap_used/expect_used/panic)` outward from the already-clean CEG.** This converts every robustness, doc, and idiom claim from "true today" into a **compiler-enforced, regression-proof invariant** — exactly what separates "auditable" (claim) from "elite" (guarantee), and it's the prerequisite that makes the public-release semver waves (B-W1/B-W3) defensible. Everything else (typed errors RBP-03, edition 2024 RBP-10, dedup RBP-06) is then enforced *by the ratchet as it advances*, not by hope.
