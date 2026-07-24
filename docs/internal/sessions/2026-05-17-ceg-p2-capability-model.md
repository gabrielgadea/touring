# CEG — P2 Capability Model (P2.1–P2.3 wave report)

> Pln2 plan: `docs/2026-05-17-ceg-pln2-plan.md`, phase **P2**.
> Executed 2026-05-17. FASE 0 gate PASS (`cargo check --workspace` exit 0;
> `touring doctor` 6/6 ok; index rebuilt).
> Wave scope: **P2.1 + P2.2 + P2.3** (the pure capability-model core).
> P2.4 (landlock) + P2.5 (per-project resolution) deferred — see §6.

## 1. What shipped

New module `crates/touring-hooks/src/capability/` — the Deno-inspired,
deny-by-default authority layer of the CEG. Four files, registered in
`lib.rs` as `pub mod capability;`.

| File | Deliverable | Symbols | LOC |
|------|-------------|---------|-----|
| `capability/scope.rs` | **P2.1** | `PathScope`, `HostScope`, `CmdScope`, `KeyScope` | ~370 |
| `capability/mod.rs` | **P2.1** | `Capability` enum (+ `covers`) | ~200 |
| `capability/profile.rs` | **P2.2** | `Decision`, `CapabilityProfile` (+ `resolve`) | ~290 |
| `capability/builtins.rs` | **P2.3** | `BuiltinProfile`, `ENV_ALLOWLIST`, `read_only` / `staged_write` / `trusted` / `sandboxed` | ~330 |

## 2. P2.1 — `Capability` enum + scope types

Five-variant total enum, one variant per resource class — no monolithic
"trusted" variant, mirroring Deno's `--allow-read/-write/-net/-run/-env` split:

- `FsRead(PathScope)` / `FsWrite(PathScope)` — `PathScope` covers a directory
  subtree; matching is **component-wise** (`Path::starts_with`), so `/var/log`
  does not spuriously cover `/var/logger`.
- `Net(HostScope)` — `host:port`; `*` host = any host, `None` port = any port.
- `Run(CmdScope)` — command name; `*` = any command.
- `Env(KeyScope)` — env-var key; trailing `*` = prefix pattern (`AWS_*`).

`Capability::covers(&self, requested)` — directional containment: a *granted*
capability covers a *requested* one only within the same class; the scope's
`matches` predicate then decides. All types `serde`-serializable + `Hash`.

## 3. P2.2 — `Decision` + `CapabilityProfile`

`Decision` = `Allow | Deny | Prompt`, mirroring Deno's
`PermissionStatus.state ∈ {granted, denied, prompt}`. `Decision::is_allowed()`
is `true` **only** for `Allow` — `Prompt` is intentionally *not* an allow: the
Touring daemon is non-interactive, so a would-prompt capability fails closed
(best-practices §"Prompt suppression").

`CapabilityProfile { name, default, allow, deny, prompt }` — `resolve(&cap)` is
**deny-by-default** and **deny-wins**, `O(grant_count)`:

1. any deny-set entry covers the request → `Deny`;
2. else any allow-set entry covers it → `Allow`;
3. else any prompt-set entry covers it → `Prompt`;
4. else the profile `default`.

Builder API: `.allowing()` / `.denying()` / `.prompting()`.

## 4. P2.3 — four built-in profiles

| Profile | Default | Grants | Use case |
|---------|---------|--------|----------|
| `ReadOnly` | `Deny` | FsRead(workspace) + env-allowlist | Static analysis, dry-run of pure code |
| `StagedWrite` | `Deny` | ReadOnly + FsWrite(staging dir only) | Generated scripts producing artifacts |
| `Trusted` | `Allow` | all minus `Run(rm)`, `Run(sudo)`, `Net(*)` | First-party tooling (taco-forge/touring/cargo) |
| `Sandboxed` | `Deny` | FsRead(workspace) + env-allowlist | Default for any unverified script |

`ENV_ALLOWLIST` = `PATH HOME USER LANG LC_ALL TERM TZ` — **deliberately
credential-free**: `AWS_*`, `GITHUB_TOKEN`, etc. are never granted, so a
sandboxed run cannot exfiltrate secrets (a test proves
`AWS_SECRET_ACCESS_KEY` resolves to `Deny`).

`BuiltinProfile` enum (`ReadOnly|StagedWrite|Trusted|Sandboxed`) — a
serializable identifier with `use_case()` (the documented-use-case the P2.3
acceptance criterion demands) and `build(workspace, staging)` dispatching to
the four constructors. **Deliberate addition** beyond the plan's literal
symbol list: it is the handle P2.5 (per-project resolution) will store in
project config — a project records `BuiltinProfile::Sandboxed`, not a closure.

## 5. Verification (all green)

| Gate | Result |
|------|--------|
| `cargo check -p touring-hooks` | exit 0 |
| `cargo check --workspace` | exit 0 — zero regression in consumers (touring-server et al.) |
| `cargo test -p touring-hooks --lib capability::` | **52 / 52 PASS** |
| `cargo clippy -p touring-hooks --lib` | 0 warnings / 0 errors |

**52 CEG capability tests** (plan budgeted 30: 12 P2.1 + 10 P2.2 + 8 P2.3):
scope 18 · mod 7 · profile 12 · builtins 15. Each built-in profile has a
test proving its exact grant set (P2.3 acceptance).

Note: `taco-forge perfect-create` STAGE 7 reported `format-rust failed
(non-blocking)` and `tdg unavailable` for all four files — the perfect-create
post-validation skipped cargo check (`no --rust-package`). The real gate is
the workspace `cargo check` + `cargo test` + `clippy` above, all run with the
module wired into `lib.rs`.

## 6. Deferred (next sub-waves)

| Deliverable | Reason deferred |
|-------------|-----------------|
| **P2.4** — landlock LSM binding + rlimit caps | Needs new workspace crates (`landlock`, `rustix`). Offline-dependency-addition risk (memory: ast-grep bump failed twice offline) — must verify `Cargo.lock` availability first, isolated sub-wave. |
| **P2.5** — per-project capability profile resolution | `deps: P2.3` (now satisfied). Will consume `BuiltinProfile` + `CapabilityProfile`. |
| **P1.6** — heredoc temporal-split `StagingRegistry` stub | P1 residue; touches `sandbox_executor.rs` (existing, high blast) — kept separate from this greenfield wave. |

## 7. REGRA #0 note — capability symbols are forward API by design

Every `capability::*` public symbol is currently orphan (consumed only by the
52 in-module tests). This is **the plan's DAG, not debt**: P2 is the
foundation phase whose explicit purpose is to mint the capability types that
**P3** (X6 CAPABILITY-GATE — `Capability` / `Decision` / `resolve`) and
**P2.5** (per-project resolution — `BuiltinProfile` / `CapabilityProfile`)
consume. Wiring them now would require implementing P3 — out of scope. The
consumers are named and sequenced in `docs/2026-05-17-ceg-pln2-plan.md`.

---
_P2.1+P2.2+P2.3 complete. Files: `capability/{mod,scope,profile,builtins}.rs`
(new), `touring-hooks/src/lib.rs` (1-line `pub mod` registration). 52 tests,
0 clippy, 0 regression._
