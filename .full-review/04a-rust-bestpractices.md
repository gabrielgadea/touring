# Phase 4a — Rust & Language Best Practices (F4.1–F4.6)

> Target: `/home/gabrielgadea/.claude/rust` · 45 workspace members · 544k LOC src · edition 2024 · clippy `--all-targets -D warnings` CLEAN.
> Methodology: counts-first, every claim cites `file:line` or command output. Read-only/advisory.
> Verdict headline: **F4.1–F4.4 are at or near elite; the only material best-practice gaps are F4.5 (1 supply-chain duplicate) and F4.6 (3 missing hygiene files).**

## Severity counts

| Severity | Count | Items |
|----------|-------|-------|
| **Critical** | 0 | — |
| **High** | 1 | RBP-F45-1 (schemars 0.8↔1.2 duplicate → cargo-deny `bans ❌`) |
| **Medium** | 2 | RBP-F46-1 (`rust-toolchain.toml` missing), RBP-F46-2 (`rustfmt.toml`/`clippy.toml` absent — relies on defaults) |
| **Low** | 3 | RBP-F46-3 (`CODEOWNERS` missing), RBP-F45-2 (`resolver = "2"` under edition 2024), RBP-F41-1 (10 numeric indexed loops) |

No P0/P1 correctness or framework-misuse defects in this axis. The lint ratchet does the heavy lifting.

---

## F4.1 — Language Idioms — **ELITE (credit)**

clippy `--all-targets -D warnings` is clean (00-scope) — already credited, not re-flagged. Idiom **density** is high:

| Idiom | Count | Source |
|-------|-------|--------|
| `let … else` | 484 | `grep -rn "let .* else {" crates/*/src \| wc -l` |
| `matches!(…)` | 657 | `grep -rn "matches!(" crates/*/src \| wc -l` |
| `for … in 0..` total | 497 | `grep -rn "for .* in 0\.\." crates/*/src \| wc -l` |
| → of which `for _ in 0..N` (fixed repetition, **not** anti-pattern) | 252 | `grep -rn "for _ in 0\.\." …` |
| → genuine C-style **indexed** `for i in 0..x.len()` | **10** | `grep -rn "for .* in 0\.\..*\.len()" … \| grep -v test` |

### RBP-F41-1 — **Low** — 10 numeric/ML indexed loops

`touring-intelligence/src/rl/rl/actor_critic.rs:167,182,380,391` · `touring-intelligence/src/reasoning/got.rs:1853` · `touring-hooks-core/src/error_predictor.rs:190` · `touring-intelligence/src/ann/cross_validator.rs:696` · `touring-storage/src/vfs/file_set.rs:167` (+2).
- **Current:** `for i in 0..input.len() { … input[i] … }` (numeric kernels) / `for start in 0..self.recent.len()` (sliding window).
- **Recommended:** most are legitimate (paired-index access into 2+ slices, or windowing where the index *is* the value — clippy's `needless_range_loop` correctly does not fire). `actor_critic.rs` loops that touch a single slice could use `.iter().enumerate()`/`.iter_mut()`.
- **Fix:** optional micro-refactor; **not** worth the LTO/iterator-bounds-check trade in hot ML paths. Leave as-is unless touched.

**Verdict: idioms are elite.** 484 let-else + 657 matches! against ~10 real indexed loops = M2+ idiomatic. The relaxed `clippy::indexing_slicing` (Cargo.toml:600) is a documented, justified priority-0 override, not drift.

---

## F4.2 — Framework Patterns (tokio/axum) — **STRONG (credit)**

| Probe | Result |
|-------|--------|
| `block_on` total in src | 59 (`grep -rn "block_on" crates/*/src \| wc -l`) |
| `Runtime::new` / `Builder::new_*_thread` | 104 |
| lock-across-await (Phase-2) | 0 — already credited |

**No nested-runtime-in-async defect found.** Every inspected `block_on` is at a **sync↔async boundary** that is the *correct* place for it:
- `touring-server/src/main.rs:163` `rt.block_on(async_main())` — top-level entrypoint.
- `touring-hooks/src/daemon_main.rs:42` `rt.block_on(run_daemon_async())` — daemon entry.
- `touring-orchestration/src/tasks/include.rs:63`, `touring-cli/src/cli/inferlets.rs:75`, `touring-cli/src/cli/saga.rs:109/149` — CLI handlers driving async services from a sync `fn`.
- `touring-server/src/server/mod.rs:368` — `Handle::current().block_on` correctly wrapped in `tokio::task::block_in_place(|| …)` (D24's documented-safe pattern; comment at :360 explains "so `block_on` can drive a nested future safely on the multi-thread runtime").
- `touring-ceg/src/gateway/sandbox_executor.rs:526-548` — explicitly isolates a `new_current_thread` runtime *because* it may be called from inside the CLI runtime; soundness documented inline ("Nested runtime: isolate so `block_on` is sound").
- `touring-bindings/src/desktop/components/*` — Dioxus desktop UI event handlers (synchronous render context) spawning a `new_current_thread` runtime to call `spawn_touring_command` — bounded, UI-feature-gated.

**Verdict: framework usage is idiomatic.** The `block_on` density is a function of Touring's CLI/daemon/sync-FFI surface, not misuse. No `block_on` is buried in an `async fn` body without `block_in_place` isolation.

---

## F4.3 — Deprecated APIs — **CLEAN (credit)**

| Probe | Result |
|-------|--------|
| Own `#[deprecated]` items | **2** (`touring-foundation/src/embedding/client.rs:966`, `touring-storage/src/embedding/client.rs:855`) — the 3rd grep hit is `touring-quality/.../f4_3_deprecated.rs:23` (the verifier *counting* the marker, not a deprecation) |
| Both have `since=`+`note=`? | yes (own API evolution path, D42-compliant) |
| Consumers of deprecated symbols | none flagged by clippy `-D warnings` → 0 internal callers of a deprecated item |

`cargo build 2>&1 | grep -ci deprecated` **skipped** (no warm build available; per task instructions — not run to avoid a cold compile).

**Verdict: clean.** 2 deliberate own-API deprecations with proper `since`/`note`, zero consumed deprecated APIs (a clippy `-D warnings` build would fail on `deprecated` lint otherwise). Edition 2024 means the toolchain surfaces deprecation warnings as errors via the ratchet.

---

## F4.4 — Modernization — **ELITE (credit)**

| Probe | Result | Source |
|-------|--------|--------|
| Workspace edition | **2024** (latest) | `Cargo.toml:146` `[workspace.package] edition = "2024"` |
| Per-crate editions | uniform — 24 `edition = "2024"` + 25 `edition.workspace = true`, **zero** 2021/2018 holdouts | `grep -rh '^edition' crates/*/Cargo.toml \| sort \| uniq -c` |
| `#[async_trait]` crates | 6 crates, 24 sites | `grep -rln async_trait crates/*/src` |

### `#[async_trait]` is NOT legacy debt here — it is the correct choice

D43 flags `#[async_trait]` "where native async-fn-in-trait would serve." It does **not** serve here: the storage uses are on **`dyn`-dispatched, object-safe traits** —
`touring-storage/src/embeddings/adapter.rs:56` holds `Arc<Box<dyn ProviderPlugin>>` (and `:13` an `ArcSwap<Box<dyn ProviderPlugin>>`). Native async-fn-in-trait (RPITIT, stable 1.75) is **not `dyn`-compatible** without `#[allow(async_fn_in_trait)]` + manual boxing. `#[async_trait]` remains the idiomatic, correct mechanism for `dyn`-dispatched async traits as of MSRV 1.85.

**Verdict: fully modern.** Edition 2024 across all 45 crates (the strongest possible modernization signal), MSRV 1.85, and the one "modernization smell" (`async_trait`) is architecturally justified by `dyn` dispatch. No `cargo fix --edition` migration owed.

---

## F4.5 — Package Management — **1 High finding**

| Probe | Result | Source |
|-------|--------|--------|
| `[workspace.dependencies]` count | 206 | `awk` over `[workspace.dependencies]` block |
| cargo-deny `bans` | ❌ (00-scope baseline) | `deny.toml:56` `multiple-versions = "deny"` |
| schemars in deny skip-list? | **NO** (`grep schemars deny.toml` → exit 1) | confirmed un-skipped |

### RBP-F45-1 — **High** — schemars 0.8 ↔ 1.2 duplicate (A1 / SEC-06 confirmed)

- **Evidence:** `touring-harness-mcp/Cargo.toml:21` → `schemars = "0.8"` (direct pin, **not** `workspace = true`), while workspace canonical is `Cargo.toml:257` `schemars = { version = "1.2", features = ["uuid1", "chrono04"] }`. Line 20 of the same manifest requests `rmcp = { workspace = true, features = […, "schemars"] }`.
- **Root cause (not careless drift):** `rmcp` 1.2 (`Cargo.toml:293`) with its `schemars` feature transitively locks to **schemars 0.8** (its `JsonSchema` derive integration predates schemars 1.x). The direct `schemars = "0.8"` pin on :21 exists to *match rmcp's transitive schemars* and avoid a `JsonSchema` trait-version mismatch. So the duplicate is imposed by an upstream constraint, not avoidable by simply bumping the pin.
- **Impact:** two `schemars` (+`schemars_derive`) versions in the graph → `cargo deny check bans` fails (the one open hygiene item; D08/D44). I-cache + binary bloat (two derive-macro codegens). It is **un-skipped** in `deny.toml`, so the failure is live, not waived.
- **Fix (pick one):**
  1. **Quarantine** — `cargo deny` skip-list entry `{ crate = "schemars@0.8.x", reason = "transitive lock from rmcp 1.2 schemars feature; revisit on rmcp ≥ next-minor with schemars 1.x" }` mirroring the existing W13 transitive-churn pattern (deny.toml:77+). Restores green `bans` with an auditable reason; no behavior change.
  2. **Eliminate at source** — drop `rmcp`'s `schemars` feature if `touring-harness-mcp` does not actually emit JSON-Schema tool descriptors via rmcp's macro path (verify: does harness-mcp use `#[derive(JsonSchema)]`/rmcp `schemars`-gated codegen? If only `touring-server`/`touring-generator` use schemars 1.2, harness-mcp can drop both lines 20-feature and 21).
  3. **Track upstream** — watch for an `rmcp` release that moves to schemars 1.x, then unify on `workspace = true`.
- **Recommendation:** (1) now (closes the gate today, auditable), (2) as the durable fix if the schemars feature is unused.

### RBP-F45-2 — **Low** — `resolver = "2"` under edition 2024

- **Evidence:** `Cargo.toml:94` `resolver = "2"`; `Cargo.toml:146` `edition = "2024"`. Edition 2024's **default** feature resolver is `"3"` (MSRV-aware unification). Explicit `"2"` overrides that default.
- **Impact:** harmless if intentional (resolver 3 changes how features unify for build-deps/targets; some teams pin `"2"` to avoid a rebuild surprise). But it's an *implicit* downgrade from the edition default that no comment explains.
- **Fix:** either add a one-line comment justifying the `"2"` pin, or test-bump to `resolver = "3"` (the edition-2024 native choice — better dev/host feature isolation) and re-run `cargo deny`/CI. Low priority.

**Other dep-mgmt:** `cargo machete` not run (no warm tree / not available without invocation). Worth adding to CI to catch unused declared deps. The 206-dep `[workspace.dependencies]` table with `workspace = true` propagation is the **correct** pattern (single source of version truth); the harness-mcp `schemars = "0.8"` is the **only** non-`workspace` version pin that breaks unification — confirmed via manifest scan (`grep schemars crates/*/Cargo.toml`).

**Verdict:** dep management is otherwise elite (workspace-version unification, `multiple-versions = "deny"`, curated auditable skip-list, `wildcards`/TLS-deny policy). One un-skipped transitive duplicate is the sole open item.

---

## F4.6 — Build Configuration — **STRONG profiles (credit) · 3 missing hygiene files**

### Profiles — **ELITE (credit)**

`Cargo.toml:545-594`:
- **`[profile.release]`** (`:572`): `lto = "fat"`, `opt-level = "s"`, `codegen-units = 1`, `strip = true`, `panic = "abort"` — textbook size-optimized release. ✅ all D46 release best-practices.
- **`[profile.dev]`** (`:545`): `opt-level = 0`, `debug = "line-tables-only"`, `incremental = false`, `split-debuginfo = "unpacked"` + `[profile.dev.package."*"]` `opt-level = 2`/`debug = false` — exactly the REGRA #12 defensive disk-hygiene profile (deps optimized, own code fast-build, minimal debug bloat). ✅
- Opt-in `[profile.fast-iter]` (incremental) + `[profile.debugging]` (full symbols) + `[profile.ci]` (mutation-testing parity) — sophisticated, purpose-built. ✅
- **`.cargo/config.toml`** present: gold linker (`-fuse-ld=gold`, mold-fallback documented), `--cfg tokio_unstable` (tokio-console), nextest aliases. ✅ build config is well-engineered.

### Missing hygiene files

| File | Status | Severity |
|------|--------|----------|
| `rust-toolchain.toml` | **MISSING** | **Medium** (RBP-F46-1) |
| `rustfmt.toml` | **MISSING** | **Medium** (RBP-F46-2) |
| `clippy.toml` | **MISSING** | (folded into RBP-F46-2) |
| `CODEOWNERS` | **MISSING** | **Low** (RBP-F46-3) |
| `LICENSE-APACHE` | ✅ **PRESENT** (scope-00 said "only MIT" — **stale**; both exist, matching `license = "MIT OR Apache-2.0"`) | n/a |

### RBP-F46-1 — **Medium** — `rust-toolchain.toml` missing (RBP-04, **scoped correction**)

- **Correction to scope-00/03 framing:** MSRV is **NOT** entirely absent. `Cargo.toml:147` `[workspace.package] rust-version = "1.85"` exists and propagates (`crates/touring-*/Cargo.toml:5` `rust-version = "1.85"` or `.workspace = true`). So the *MSRV declaration* is present — what's missing is the **toolchain pin file**.
- **Gap:** `rust-toolchain.toml` is absent → no pinned `channel`/`components`/`profile`. CI and every contributor float to whatever `rustc` is installed. Edition 2024 + MSRV 1.85 are *declared* but not *enforced at the toolchain level*; reproducible builds and "works on my machine" drift are unguarded.
- **Fix:** add a minimal `rust-toolchain.toml`:
  ```toml
  [toolchain]
  channel = "1.85.0"     # or a pinned stable ≥ MSRV, matching rust-version
  components = ["rustfmt", "clippy"]
  ```
  Pin to the MSRV (1.85) or to a known-good stable, with `components` ensuring fmt/clippy are present for the `-D warnings` gate. Closes RBP-04.

### RBP-F46-2 — **Medium** — `rustfmt.toml` + `clippy.toml` absent (relies on defaults)

- **Reality:** the repo relies on **rustfmt defaults** and **clippy defaults** (no `rustfmt.toml`/`clippy.toml`). This is *acceptable but not elite* for a 544k-LOC workspace: formatting/lint config is implicit, so a contributor's local rustfmt edition/style can diverge silently, and project-specific clippy thresholds (cognitive-complexity, too-many-args, MSRV-aware lints) are un-tuned.
- **Note:** the elite work is in `[workspace.lints.clippy]` (`Cargo.toml:596`, `all = deny` priority -1 + RBP-11 ratchets) — that's the *deny ratchet*, which is genuinely elite and **already credited**. But `[workspace.lints]` ≠ `clippy.toml`: the latter sets *thresholds* (e.g. `cognitive-complexity-threshold`, `msrv = "1.85"` so clippy suggests only MSRV-compatible modernizations), which are currently default.
- **Fix:**
  - `rustfmt.toml` with at minimum `edition = "2024"` (so `cargo fmt` doesn't apply 2015-default formatting) + any house style (e.g. `imports_granularity`, `group_imports`) — note several are nightly-only.
  - `clippy.toml` with `msrv = "1.85"` (makes clippy MSRV-aware — won't suggest APIs newer than MSRV; complements D43) + optional complexity thresholds matching the modularization debt (154 files >800 LOC, 00-scope).

### RBP-F46-3 — **Low** — `CODEOWNERS` missing

- Absent. For a single-author repo (Gabriel) this is low-impact, but it's a CI/governance hygiene gap if the repo opens to contributors (pairs with the F4.7 CI/CODEOWNERS gates). Add a minimal `.github/CODEOWNERS` (`* @gabrielgadea`) when publishing.

**Verdict:** profiles + `.cargo/config.toml` are elite; the gap is the *absence of declarative toolchain/format/lint-config files*, which lets implicit defaults govern a large workspace. None blocks the build today (the deny ratchet + `-D warnings` compensate), but they are the standard "premium-elite repo" hygiene files.

---

## Verified-strong (do not re-flag)

- **clippy `--all-targets -D warnings` CLEAN** + `[workspace.lints.clippy] all = deny` (priority -1) ratchet — elite (00-scope, Cargo.toml:596).
- **Edition 2024 uniform** across all 45 crates — strongest modernization signal possible.
- **Release profile** LTO=fat / opt-level=s / codegen-units=1 / strip / panic=abort — textbook (Cargo.toml:572).
- **Dev profile** defensive per REGRA #12 (deps opt-2/no-debug, own-code fast) — disk-hygiene elite (Cargo.toml:545-553).
- **`block_on` discipline** — all at sync↔async boundaries; the one runtime-internal case (`server/mod.rs:368`) uses `block_in_place` correctly.
- **`#[async_trait]`** — correct (dyn-dispatched object-safe traits), not legacy debt.
- **MSRV declared** (`rust-version = "1.85"`, Cargo.toml:147) and propagated.
- **Dual-license** MIT OR Apache-2.0 with **both** LICENSE files present.
- **cargo-deny** advisories/licenses/sources green; `multiple-versions=deny` + curated auditable skip-list — only `bans` open (the one schemars dup).

## Open-item cross-references
- RBP-F45-1 (schemars) = Phase-1 A1 / SEC-06 / D08/D44 — supply-chain hygiene, the single shared open item across phases.
- RBP-F46-1 (`rust-toolchain.toml`) = RBP-04.
- RBP-F46-2/3 + F4.7 CI under-gating (T2, 03-phase) = the "premium repo hygiene" cluster for the consolidated P2 action plan.
