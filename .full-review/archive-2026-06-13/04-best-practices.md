# Phase 4: Best Practices & Standards

> Touring workspace · 2026-06-13 · agents: rust-pro (4A) + deployment-engineer (4B)
> Full detail: `04a-rust-bestpractices.md` · `04b-cicd-devops.md`

## Rust & Language Best Practices (4A) — 0 Critical / 4 High / 7 Medium / 4 Low
**Strong on the floor, weak on the enforced ceiling.** Rare elite-grade hygiene confirmed: `[workspace.dependencies]` ~89% adopted (645 `workspace=true`, 0 genuinely hard-coded versions); `std::sync::Mutex` **never** held across `.await` (0 lock-across-await UB); `LazyLock`/`OnceLock` 244× (0 `lazy_static`); `let-else` 538×; `#[must_use]` 794×.

- **[High] RBP-01 — Lint ceiling stops at `clippy::all`** — no `unwrap_used`/`expect_used`/`missing_docs`/`pedantic`. CEG proves the ratchet works (`touring-ceg/gateway/mod.rs:43`).
- **[High] RBP-02 — 8 crates escape the lint floor entirely** (no `[lints]` section): `inferlets, touring-assists, touring-contracts, touring-generator, touring-identity, touring-license, touring-lsp, touring-rkyv` — incl. the public generator API + the IoC seam.
- **[High] RBP-03 — Stringly-typed public errors** — 373 `map_err(format!)` + 141 `pub fn -> Result<_, String>`. An SDK needs `thiserror` enums with `#[from]` (88 already exist; kernel `TouringError` is exemplary).
- **[High] RBP-04 — MSRV not pinned/tested** — workspace says 1.80 but 18 crates hard-code 1.75 (impossible — code uses `LazyLock` = 1.80); CI floats on `@stable`, no MSRV job, no `rust-toolchain.toml`.
- **[Med]** RBP-05 `unsafe impl Send` no SAFETY comment (SEC-06); RBP-06 duplicate-version sprawl (two wasmtime/cranelift trees, syn 1+2, thiserror 1+2, rand×3) inflating the 1,558-pkg tree; RBP-07 `deny.toml` exists, no CI gate (SEC-03 RED); RBP-08 `#[non_exhaustive]` on only 11/400 enums; RBP-09 45 glob re-exports; RBP-10 edition 2021 not 2024; RBP-11 `lints.rust` near-empty.

**rkyv refinement (corrects Phase 2 F4):** the daemon **request** path DOES use real zero-copy (`check_archived_root`, `daemon.rs:1003`); F4's full-deserialize cost holds only for the ipc **response** path.

**Deps-modernity verdict:** mostly current (tokio 1.40, thiserror 2.0, clap 4.5, wasmtime 44, tantivy 0.22). Two drags: duplicate-version sprawl (RBP-06) and pyo3 0.24 on the FFI/security boundary (SEC-03). Single-source dep discipline is elite.

**#1 lever:** RBP-01+RBP-02 — install the elite `[workspace.lints]` block, give the 8 escapees `[lints] workspace = true`, then ratchet `deny(unwrap_used/expect_used/panic)` outward from the already-clean CEG (7-step order in the file). Converts every robustness/idiom claim from "true today" to compiler-enforced invariant — prerequisite for the semver public-release waves.

## CI/CD & DevOps (4B) — 2 Critical / 7 High / 8 Medium / 4 Low
**Verdict: Potemkin CI — fully authored, never run, unfireable.**

- **🔴 CICD-01 — No git → the entire CI/release pipeline has never run and cannot.** `ls .git` → absent (confirmed up-tree). Actions trigger on `push`/`pull_request`/`tags ["v*"]` — all git-native. `release.yml:13` self-admits "authored ahead of first public release." This is the dependency root of masterplan **B-W1** and is **Gabriel-only to resolve** (TACO is git-prohibited, REGRA #11).
- **🔴 CICD-04 — Five versions across the release surface** — crate `0.1.0` + `publish=false` (`Cargo.toml:142`), tags `v*`, install.sh `v31.0.0`, Homebrew/Scoop `0.30.0`. `publish=false` structurally blocks `cargo install` (B-W4); binary printing `0.1.0` breaks Homebrew's version assert. (Same root as DOC-01.)
- **[High] CI gate gap — only 6/22 elite gates present-and-binding.** MISSING: `cargo fmt --check`, **`cargo deny check` (SEC-03 RED, live CVSS-8.7, ungated)**, `cargo doc -D warnings`, coverage floor (llvm-cov), fuzz smoke (8 targets, found 5 bugs, never run), perf P99 guard (T-03), `cargo nextest` (the `slow-timeout` harness is **already configured** in `.config/nextest.toml` but CI uses plain `cargo test --lib`).
- **[High] CICD-05/06 — repo identity contradiction** (`anthropics/touring` vs `gabrielgadea/touring` vs `touring.dev`) + target-triple mismatch (release builds `musl`, Homebrew expects `gnu`, Scoop expects Windows never built) → 3 of ~5 advertised platforms undeliverable.
- **[High] CICD-07/08 — no SBOM/cosign/sigstore/SLSA**; `curl|sh` installer with same-origin checksum (TOFU, no tamper protection) — worst on a tool that *executes agent code*.
- **[Med] Fail-open theater** — `wiring_integrity` ends `|| true` (`ci.yml:79`); `health-delta` "advisory only" (`:81`); template MSRV/semver are `continue-on-error`. "Add gate, then defang it" anti-pattern.

**Bright spots (don't regress):** graceful shutdown is real (`daemon_main.rs:6-9`, flushes WAL/LinUCB/CRDT); `deny.toml` is a high-quality 23KB policy (just unwired); a far more complete CI exists as `scripts/ci/per-project-deployment.yml.template` (fmt/MSRV/semver/shellcheck/multi-OS) — **all infrastructure is written, just not activated or binding.**

**#1 CI/CD lever:** after CICD-01 (Gabriel publishes repo+tag), one ~30-line `fmt-deny-doc` job closes the three most dangerous gaps at once — `fmt --check`, **`cargo deny check`**, `doc -D warnings` — plus remove `|| true` from the two fail-open gates and gate `cargo nextest` (config already exists). The authorship is done; the gap is **activation + binding**.

## Meta-pattern across Phases 1-4 (feeds Phase 5)
The recurring elite gap is **"invented but not binding."** Touring already contains the elite mechanism for nearly every gap — workspace.lints (just shallow), deny.toml (unwired), nextest slow-timeout (unused), cargo-fuzz (present, unrun), llvm-cov (historical, ungated), the CEG capability model (governs exec but not MCP), doc-as-code gates (first-match-only). **Reaching elite is overwhelmingly about activating + binding + ratcheting what already exists — not green-field building.** That is a cheap, high-confidence path.
