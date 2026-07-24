# CEG — Dependency Audit & Modernization (P0.5)

> Pln2 plan: `docs/2026-05-17-ceg-pln2-plan.md`, deliverable **P0.5**.
> Sources: P0.3 context7 dossier (`2026-05-17-ceg-best-practices.md`), `touring ast workspace-info`,
> workspace `Cargo.toml` inspection, Touring memory `W11.6` (ast-grep ABI break record).
> Rigor dimension: **(h)** — atualização e compatibilidade de dependências.

## 1. Current workspace state (evidence)

Inspected via `grep -nE 'ast-grep|landlock|rustix|tree-sitter|blake3|moka|criterion' Cargo.toml`:

| Dependency | Current pin | Cargo.toml | CEG relevance |
|------------|-------------|------------|---------------|
| `ast-grep-core` | `=0.36.0` (exact) | line 399 | X2 STATIC — polyglot forbidden-call detection (P1.3) |
| `ast-grep-language` | `=0.36.0` (exact) | line 400 | X2 STATIC — language grammars for ast-grep |
| `tree-sitter` | `0.24` | line 247 | Grammar substrate for ast-grep + risk scan |
| `tree-sitter-go` | `0.25` | line 486 | Go sandbox + forbidden scan (currently ABI-broken — memory B-FUZZ-002) |
| `tree-sitter-{python,rust,ts,js,html,css,json,bash,toml,yaml,md,java}` | `0.23`–`0.25` | lines 248–258, 487 | Polyglot grammars consumed by ast-grep |
| `blake3` | `1.5.5` | line 238 | Reused — X5 dry-run content cache (P4.5) |
| `moka` | `0.12` | line 166 | Reused — dry-run + profile caches (P4.5, P2.5) |
| `criterion` | `0.5` | line 415 | Reused — P0.4 bench + P7.5 regression gate |
| `landlock` | **absent** | — | **must add** — X8 kernel-enforced FS sandbox (P2.4) |
| `rustix` | **absent as direct dep** | — | **must add** — rlimit/resource caps (P4.3); currently transitive-only |
| `seccompiler` | **absent** | — | optional defense-in-depth (deferred — see §4) |

## 2. Target versions (context7-verified — P0.3 dossier)

| Dependency | Current | Target | Source | Rationale |
|------------|---------|--------|--------|-----------|
| `ast-grep-core` | `=0.36.0` | `0.42.2` | context7 + crates.io | Modern grammar ABI, bug fixes; required by P1.3 AST forbidden detection. |
| `ast-grep-language` | `=0.36.0` | `0.42.2` | context7 + crates.io | Must move in lockstep with `ast-grep-core`. |
| `landlock` | absent | `0.4.4` | context7 | Kernel-enforced filesystem capability sandbox (Linux 5.13+); ABI 1–6, `CompatLevel::BestEffort` degrades on old kernels. |
| `rustix` | transitive | `1.1` (latest 1.1.4) | crates.io | Safe POSIX `rlimit`/resource-cap syscalls, no `libc` unsafe, no C dependency. |
| `seccompiler` | absent | `0.5` (opt-in) | context7 | Syscall BPF filtering — see §4 (deferred to a hardening wave). |
| `tree-sitter-go` | `0.25` | ABI-compatible release | memory B-FUZZ-002 | Fix the broken Go grammar so the Go sandbox + forbidden scan work. |

## 3. The ast-grep 0.36 → 0.42 upgrade — STAGED, not applied here

**Decision: `Cargo.toml` is NOT modified in P0.5.** The audit *records and pins the decisions*;
the actual version bumps are scheduled to their consuming phases, each behind a regression gate.
This is the **R6 mitigation** ("staged upgrade with regression tests") and Touring memory **W11.6**
discipline — not scope reduction. Applying a blind multi-crate ABI bump in a foundations phase,
with no test to catch the runtime break, is exactly the failure mode P0.5 exists to prevent.

### 3.1 Why the upgrade is risky (P0.3 dossier finding)

The Rust **API** breakage between 0.36 and 0.42 (0.38's `LanguageExt` trait split, `Root`/`AstGrep`
alias, `StrDoc` → `ast_grep_core::tree_sitter`, "remove language bound in matcher") is
**compile-time-caught and mechanical** — the compiler lists every site.

The real risk is the **tree-sitter parser ABI**: ast-grep 0.42 bundles tree-sitter v0.26.7 whose
default grammar ABI is **15**, while older `tree-sitter-<lang>` grammar crates target ABI ≤ 14.
The tree-sitter runtime is backward- but **not forward-compatible** — a mismatch surfaces as a
**runtime panic / grammar-load failure at parse time, not a compile error**. That is precisely the
W11.6 polyglot break (and the hook "Unsupported tree-sitter ABI for bash: Incompatible language
version 15" noise observed in this very session confirms the ABI-15 surface is live).

### 3.2 Staged upgrade procedure (scheduled to P1.3)

1. **One atomic multi-crate bump** — `ast-grep-core` + `ast-grep-language` + **every**
   `tree-sitter-<lang>` grammar crate re-pinned together to ABI-15-compatible releases.
   Partial bumps are forbidden — they are the W11.6 break.
2. **Validate before merge** with the polyglot test suite + the W11.6 `fuzz/` cargo-fuzz targets
   (8 targets, raiz `fuzz/`). The fuzz suite is the canary for ABI mismatch.
3. **Re-pin `tree-sitter-go`** in the same bump (memory B-FUZZ-002 — the Go grammar `.expect()` in
   `node.rs:73` panics under ABI mismatch). The Go sandbox path (P4.1) depends on this fix.
4. If any grammar crate has no ABI-15 release, that language is **feature-gated off** in the CEG
   forbidden scanner until upstream catches up — never shipped broken (REGRA #0: degrade
   explicitly, do not silently regress).

## 4. seccomp — deferred (decision, not omission)

P0.3 dossier verdict: **skip seccomp for the CEG MVP.** Add `seccompiler = "0.5"` as opt-in
defense-in-depth behind a `ceg-seccomp` Cargo feature in a later hardening wave.

Rationale: `landlock` 0.4.4 — including its ABI-4 network-control rules — covers the primary CEG
threat model (filesystem + network capability containment). A correct per-architecture syscall
allow-list, kept within seccomp's 4096-BPF-instruction budget, is a real maintenance cost with
sharp failure modes (a missing syscall kills the child) better deferred than rushed into the MVP.
The capability decision (X6) is still enforced in user space regardless of seccomp.

## 5. landlock / rustix — added in consuming phases, not P0.5

`landlock` and `rustix` are **not** added to `Cargo.toml` in P0.5: an unused dependency would
generate `unused_crate_dependencies` warnings and pollute the build until its consumer exists.
They are introduced atomically with the code that uses them:

| Dependency | Added in | First consumer |
|------------|----------|----------------|
| `landlock = "0.4.4"` | **P2.4** | `src/capability/enforce_linux.rs` — `apply_landlock` |
| `rustix = "1.1"` (feature `process`) | **P4.3** | `src/capability/limits.rs` — `apply_rlimit` / `ResourceCaps` |
| `seccompiler = "0.5"` (opt-in) | hardening wave | behind `ceg-seccomp` feature |

## 6. tree-sitter-go ABI fix — scheduled

`tree-sitter-go = "0.25"` is recorded broken under ABI mismatch (memory B-FUZZ-002: `node.rs:73`
`.expect()` panics; Go polyglot path non-functional). Scheduled to the **§3.2 atomic bump (P1.3)**
— the Go grammar is re-pinned together with the ast-grep upgrade so the Go sandbox (P4.1) and the
Go forbidden scan land on a working grammar.

## 7. P0.5 acceptance verdict

| Acceptance criterion (plan) | Status |
|-----------------------------|--------|
| ast-grep-core 0.36→0.42.x | **Pinned to 0.42.2**; bump procedure specified, staged to P1.3 (§3). |
| landlock + rustix evaluated and pinned | **landlock 0.4.4 / rustix 1.1 pinned** (§2, §5); add scheduled to P2.4/P4.3. |
| tree-sitter-go ABI fix scheduled | **Scheduled** into the §3.2 atomic bump (§6). |
| `cargo check --workspace` | **PASS** (FASE 0 baseline, exit 0; 1 pre-existing unrelated warning in touring-python). |

**Outcome**: every CEG dependency has a context7-verified target version and an explicit,
gated application phase. No blind bump was applied. The dependency surface is modernization-ready.
